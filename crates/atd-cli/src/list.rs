//! `atd list` — discover tools and print them, filtered by flags.

use atd_client::{AtdClient, DiscoverFilter};
use atd_protocol::{AtdError, ToolTier, ToolVisibility};
use std::io::Write;

use crate::cli::ListArgs;

pub async fn run(
    client: &AtdClient,
    args: ListArgs,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let filter = DiscoverFilter {
        tier: args.tier.as_deref().and_then(parse_tier),
        visibility: args.visibility.as_deref().and_then(parse_visibility),
        domain: args.domain,
        limit: args.limit,
    };

    let summaries = client.discover(args.query.as_deref(), filter).await?;

    if args.json {
        serde_json::to_writer(&mut *out, &summaries)
            .map_err(|e| AtdError::ProtocolError {
                expected: "serializable ToolSummary list".into(),
                got: format!("serde error: {e}"),
            })?;
        writeln!(out).ok();
        return Ok(());
    }

    if summaries.is_empty() {
        writeln!(out, "no tools matched").ok();
        return Ok(());
    }

    writeln!(
        out,
        "{:<40} {:<24} {:<10} {:<6} {:<10}",
        "ID", "NAME", "DOMAIN", "TIER", "VIS"
    )
    .ok();
    for s in &summaries {
        writeln!(
            out,
            "{:<40} {:<24} {:<10} {:<6} {:<10}",
            truncate(&s.id, 40),
            truncate(&s.name, 24),
            truncate(&s.domain, 10),
            tier_str(s.tier),
            visibility_str(s.visibility)
        )
        .ok();
    }
    writeln!(out, "{} tool(s) total", summaries.len()).ok();
    Ok(())
}

fn parse_tier(s: &str) -> Option<ToolTier> {
    match s {
        "hot" => Some(ToolTier::Hot),
        "warm" => Some(ToolTier::Warm),
        "cold" => Some(ToolTier::Cold),
        _ => None,
    }
}

fn parse_visibility(s: &str) -> Option<ToolVisibility> {
    match s {
        "read" => Some(ToolVisibility::Read),
        "write" => Some(ToolVisibility::Write),
        "dangerous" => Some(ToolVisibility::Dangerous),
        "system" => Some(ToolVisibility::System),
        _ => None,
    }
}

fn tier_str(t: ToolTier) -> &'static str {
    match t {
        ToolTier::Hot => "hot",
        ToolTier::Warm => "warm",
        ToolTier::Cold => "cold",
    }
}

fn visibility_str(v: ToolVisibility) -> &'static str {
    match v {
        ToolVisibility::Read => "read",
        ToolVisibility::Write => "write",
        ToolVisibility::Dangerous => "dangerous",
        ToolVisibility::System => "system",
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_client::Endpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    async fn spawn_fake_server() -> std::path::PathBuf {
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
                        let reply: serde_json::Value = match req["type"].as_str() {
                            Some("ping") => serde_json::json!({"type":"pong"}),
                            Some("tool_list") => serde_json::json!({
                                "type":"tool_list",
                                "tools":[
                                    {"id":"anos:fs.read","description":"Read a file","tier":"hot","visibility":"read"},
                                    {"id":"anos:fs.write","description":"Write a file","tier":"hot","visibility":"write"}
                                ]
                            }),
                            _ => serde_json::json!({"type":"error","message":"no"}),
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
    async fn list_prints_table_with_totals() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            ListArgs { query: None, domain: None, tier: None, visibility: None, limit: None, json: false },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("ID") && s.contains("NAME") && s.contains("DOMAIN"));
        assert!(s.contains("anos:fs.read"));
        assert!(s.contains("anos:fs.write"));
        assert!(s.contains("2 tool(s) total"));
    }

    #[tokio::test]
    async fn list_json_flag_emits_array() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            ListArgs { query: None, domain: None, tier: None, visibility: None, limit: None, json: true },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert!(v.is_array());
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn list_limit_truncates_output() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            ListArgs { query: None, domain: None, tier: None, visibility: None, limit: Some(1), json: false },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("1 tool(s) total"));
        assert!(s.contains("anos:fs.read"));
        assert!(!s.contains("anos:fs.write"));
    }
}
