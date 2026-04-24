//! `atd doctor` — connectivity sanity check: socket exists, ping succeeds,
//! how many tools does `discover` return.

use atd_protocol::AtdError;
use atd_sdk::{AtdClient, DiscoverFilter};
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;

use crate::cli::DoctorArgs;

#[derive(Serialize)]
pub struct DoctorReport {
    pub socket_path: String,
    pub socket_exists: bool,
    pub ping_ok: bool,
    pub tool_count: Option<usize>,
    pub error: Option<String>,
}

/// `sock` is the resolved endpoint path — we need it separately from the
/// connected client to report socket existence when connect fails.
pub async fn run(sock: PathBuf, args: DoctorArgs, out: &mut impl Write) -> Result<(), AtdError> {
    let socket_exists = sock.exists();
    let socket_path = sock.to_string_lossy().into_owned();

    let (ping_ok, tool_count, error) =
        match AtdClient::connect(atd_sdk::Endpoint::unix(&sock)).await {
            Ok(client) => match client.discover(None, DiscoverFilter::default()).await {
                Ok(v) => (true, Some(v.len()), None),
                Err(e) => (true, None, Some(format!("discover failed: {e}"))),
            },
            Err(e) => (false, None, Some(format!("connect failed: {e}"))),
        };

    let report = DoctorReport {
        socket_path,
        socket_exists,
        ping_ok,
        tool_count,
        error,
    };

    if args.json {
        serde_json::to_writer(&mut *out, &report).map_err(|e| AtdError::ProtocolError {
            expected: "serializable DoctorReport".into(),
            got: format!("serde error: {e}"),
        })?;
        writeln!(out).ok();
    } else {
        writeln!(out, "socket path:   {}", report.socket_path).ok();
        writeln!(out, "socket exists: {}", report.socket_exists).ok();
        writeln!(
            out,
            "ping:          {}",
            if report.ping_ok { "ok" } else { "FAIL" }
        )
        .ok();
        match report.tool_count {
            Some(n) => writeln!(out, "tool count:    {n}").ok(),
            None => writeln!(out, "tool count:    unavailable").ok(),
        };
        if let Some(e) = &report.error {
            writeln!(out, "error:         {e}").ok();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    async fn spawn_server_with_3_tools() -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::mem::forget(dir);

        let ret = path.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = stream.into_split();
                    loop {
                        let mut lb = [0u8; 4];
                        if r.read_exact(&mut lb).await.is_err() {
                            return;
                        }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() {
                            return;
                        }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let reply = match req["type"].as_str() {
                            Some("ping") => serde_json::json!({"type":"pong"}),
                            Some("tool_list") => serde_json::json!({
                                "type":"tool_list",
                                "tools":[
                                    {"id":"anos:fs.read","description":"r","tier":"hot","visibility":"read"},
                                    {"id":"anos:fs.write","description":"w","tier":"hot","visibility":"write"},
                                    {"id":"anos:web.search","description":"s","tier":"hot","visibility":"read"}
                                ]
                            }),
                            _ => serde_json::json!({"type":"error","message":"no"}),
                        };
                        let body = serde_json::to_vec(&reply).unwrap();
                        if w.write_all(&(body.len() as u32).to_be_bytes())
                            .await
                            .is_err()
                        {
                            return;
                        }
                        if w.write_all(&body).await.is_err() {
                            return;
                        }
                        let _ = w.flush().await;
                    }
                });
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ret
    }

    #[tokio::test]
    async fn doctor_reports_ok_against_reachable_server() {
        let sock = spawn_server_with_3_tools().await;
        let mut out: Vec<u8> = Vec::new();
        run(sock.clone(), DoctorArgs { json: false }, &mut out)
            .await
            .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("socket exists: true"));
        assert!(s.contains("ping:          ok"));
        assert!(s.contains("tool count:    3"));
    }

    #[tokio::test]
    async fn doctor_json_flag_emits_structured_report() {
        let sock = spawn_server_with_3_tools().await;
        let mut out: Vec<u8> = Vec::new();
        run(sock.clone(), DoctorArgs { json: true }, &mut out)
            .await
            .unwrap();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["socket_exists"], true);
        assert_eq!(v["ping_ok"], true);
        assert_eq!(v["tool_count"], 3);
        assert!(v["error"].is_null());
    }

    #[tokio::test]
    async fn doctor_reports_unreachable_when_socket_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.sock");
        let mut out: Vec<u8> = Vec::new();
        run(missing, DoctorArgs { json: false }, &mut out)
            .await
            .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("socket exists: false"));
        assert!(s.contains("ping:          FAIL"));
        assert!(s.contains("error:"));
    }
}
