//! `atd call` — invoke a tool with JSON args and print the result.

use atd_client::{AtdClient, CallOptions};
use atd_protocol::{AtdError, ToolResult};
use std::io::Write;

use crate::cli::CallArgs;

pub async fn run(
    client: &AtdClient,
    args: CallArgs,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let call_args: serde_json::Value =
        serde_json::from_str(&args.args).map_err(|e| AtdError::InvalidArguments {
            tool_id: args.tool_id.clone(),
            field: "--args".into(),
            reason: format!("not valid JSON: {e}"),
        })?;

    let result = client
        .call(
            &args.tool_id,
            call_args,
            CallOptions {
                dry_run: args.dry_run,
                preferred_binding: None,
            },
        )
        .await?;

    if args.json {
        let v = serde_json::to_string(&result)
            .map_err(|e| AtdError::ProtocolError {
                expected: "serializable ToolResult".into(),
                got: format!("serde error: {e}"),
            })?;
        writeln!(out, "{v}").ok();
        return Ok(());
    }

    match result {
        ToolResult::Success { data, .. } => {
            let pretty = serde_json::to_string_pretty(&data).unwrap_or_else(|_| "{}".into());
            writeln!(out, "ok:").ok();
            writeln!(out, "{pretty}").ok();
            Ok(())
        }
        ToolResult::Error { code, message, reason, retryable } => {
            Err(AtdError::ToolExecutionFailed {
                tool_id: args.tool_id.clone(),
                inner: Box::new(std::io::Error::other(format!(
                    "[{code}] {message}{}{}",
                    if retryable { " (retryable)" } else { "" },
                    reason.as_deref().map(|r| format!(" — raw: {r}")).unwrap_or_default()
                ))),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_client::Endpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    async fn spawn_fake_server(
        handler: fn(serde_json::Value) -> serde_json::Value,
    ) -> std::path::PathBuf {
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
                        if r.read_exact(&mut lb).await.is_err() { return; }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() { return; }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let reply = match req["type"].as_str() {
                            Some("ping") => serde_json::json!({"type":"pong"}),
                            _ => handler(req),
                        };
                        let body = serde_json::to_vec(&reply).unwrap();
                        if w.write_all(&(body.len() as u32).to_be_bytes()).await.is_err() { return; }
                        if w.write_all(&body).await.is_err() { return; }
                        let _ = w.flush().await;
                    }
                });
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ret
    }

    #[tokio::test]
    async fn call_prints_ok_and_data_on_success() {
        let sock = spawn_fake_server(|req| match req["type"].as_str() {
            Some("run_tool") => serde_json::json!({
                "type":"tool_result",
                "tool_id": req["tool_id"],
                "result": {"content":"hello"},
                "success": true,
                "dry_run": false
            }),
            _ => serde_json::json!({"type":"error","message":"no"}),
        })
        .await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            CallArgs {
                tool_id: "anos:fs.read".into(),
                args: r#"{"path":"/tmp/x"}"#.into(),
                dry_run: false,
                json: false,
            },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("ok:\n"));
        assert!(s.contains("\"content\": \"hello\""));
    }

    #[tokio::test]
    async fn call_errors_on_invalid_json_args() {
        let sock = spawn_fake_server(|_| serde_json::json!({"type":"error","message":"no"})).await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        let err = run(
            &client,
            CallArgs {
                tool_id: "anos:fs.read".into(),
                args: "not json".into(),
                dry_run: false,
                json: false,
            },
            &mut out,
        )
        .await
        .unwrap_err();
        match err {
            AtdError::InvalidArguments { field, .. } => assert_eq!(field, "--args"),
            _ => panic!("expected InvalidArguments variant"),
        }
    }

    #[tokio::test]
    async fn call_json_flag_emits_full_tool_result_envelope() {
        let sock = spawn_fake_server(|req| match req["type"].as_str() {
            Some("run_tool") => serde_json::json!({
                "type":"tool_result",
                "tool_id": req["tool_id"],
                "result": {"k":"v"},
                "success": true,
                "dry_run": false
            }),
            _ => serde_json::json!({"type":"error","message":"no"}),
        })
        .await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            CallArgs {
                tool_id: "anos:fs.read".into(),
                args: "{}".into(),
                dry_run: false,
                json: true,
            },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["status"], "success");
        assert_eq!(v["data"]["k"], "v");
    }

    #[tokio::test]
    async fn call_surfaces_server_reported_failure_as_error() {
        let sock = spawn_fake_server(|req| match req["type"].as_str() {
            Some("run_tool") => serde_json::json!({
                "type":"tool_result",
                "tool_id": req["tool_id"],
                "result": {"code":"EPERM","message":"denied","retryable":false},
                "success": false,
                "dry_run": false
            }),
            _ => serde_json::json!({"type":"error","message":"no"}),
        })
        .await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        let err = run(
            &client,
            CallArgs {
                tool_id: "anos:fs.read".into(),
                args: "{}".into(),
                dry_run: false,
                json: false,
            },
            &mut out,
        )
        .await
        .unwrap_err();
        let s = format!("{err:?}");
        assert!(s.contains("ToolExecutionFailed"), "got: {s}");
    }
}
