//! `atd schema` — describe a tool and pretty-print its ToolDefinition.

use atd_protocol::AtdError;
use atd_sdk::AtdClient;
use std::io::Write;

use crate::cli::SchemaArgs;

pub async fn run(
    client: &AtdClient,
    args: SchemaArgs,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let def = client.describe(&args.tool_id).await?;

    let json = if args.json {
        serde_json::to_string(&def)
    } else {
        serde_json::to_string_pretty(&def)
    }
    .map_err(|e| AtdError::ProtocolError {
        expected: "serializable ToolDefinition".into(),
        got: format!("serde error: {e}"),
    })?;
    writeln!(out, "{json}").ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_sdk::Endpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    fn sample_tool_def() -> serde_json::Value {
        serde_json::json!({
            "id": "anos:fs.read",
            "name": "Read File",
            "description": "Read a file from disk.",
            "version": "0.1.0",
            "capability": {"domain": "fs", "actions": ["read"], "tags": [], "intent_examples": []},
            "input_schema": {"type": "object"},
            "output_schema": {"type": "string"},
            "bindings": [{"protocol": "Cli", "config": {}}],
            "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
            "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
            "trust": {"publisher": "anos", "trust_level": "L2Tested", "signature": null},
            "visibility": "read"
        })
    }

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
                        if r.read_exact(&mut lb).await.is_err() {
                            return;
                        }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() {
                            return;
                        }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let reply: serde_json::Value = match req["type"].as_str() {
                            Some("ping") => serde_json::json!({"type":"pong"}),
                            Some("tool_schema") => serde_json::json!({
                                "type":"tool_schema","schema": sample_tool_def(),
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
    async fn schema_pretty_by_default_has_newlines_and_indent() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            SchemaArgs {
                tool_id: "anos:fs.read".into(),
                json: false,
            },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\n"));
        assert!(
            s.contains("  \"id\""),
            "pretty output should have indented keys"
        );
        assert!(s.contains("anos:fs.read"));
    }

    #[tokio::test]
    async fn schema_json_flag_emits_compact_single_line() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            SchemaArgs {
                tool_id: "anos:fs.read".into(),
                json: true,
            },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        let trimmed = s.trim_end_matches('\n');
        assert!(
            !trimmed.contains('\n'),
            "json output should be one line, got: {s}"
        );
        let v: serde_json::Value = serde_json::from_str(trimmed).unwrap();
        assert_eq!(v["id"], "anos:fs.read");
    }
}
