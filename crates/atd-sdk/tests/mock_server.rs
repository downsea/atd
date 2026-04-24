//! Integration test: prove atd-sdk can drive the full protocol against a
//! server that has zero `anos-*` crate dependencies. This is the load-bearing
//! check for the "independent reference implementation" claim in CLAUDE.md.

use atd_sdk::{AtdClient, CallOptions, DiscoverFilter, Endpoint};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

// Re-declare wire + protocol shapes here so the mock server has literally no
// path dependency into atd-sdk or atd-types crate internals.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerReq {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "tool_list")]
    ToolList,
    #[serde(rename = "tool_schema")]
    ToolSchema { tool_id: String },
    #[serde(rename = "run_tool")]
    RunTool {
        tool_id: String,
        args: serde_json::Value,
        dry_run: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerResp {
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "tool_list")]
    ToolList { tools: serde_json::Value },
    #[serde(rename = "tool_schema")]
    ToolSchema { schema: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_id: String,
        result: serde_json::Value,
        success: bool,
        dry_run: bool,
    },
}

async fn read_frame<R: AsyncReadExt + Unpin>(r: &mut R) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let n = u32::from_be_bytes(len) as usize;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_frame<W: AsyncWriteExt + Unpin, T: Serialize>(
    w: &mut W,
    msg: &T,
) -> std::io::Result<()> {
    let b = serde_json::to_vec(msg).unwrap();
    let len = (b.len() as u32).to_be_bytes();
    w.write_all(&len).await?;
    w.write_all(&b).await?;
    w.flush().await
}

fn sample_tool() -> serde_json::Value {
    serde_json::json!({
        "id": "mock:echo.say",
        "name": "Echo",
        "description": "echo back the input",
        "version": "0.1.0",
        "capability": {
            "domain": "echo", "actions": ["say"], "tags": ["test"], "intent_examples": []
        },
        "input_schema": {"type": "object"},
        "output_schema": {"type": "object"},
        "bindings": [{"protocol": "Cli", "config": {}}],
        "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
        "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
        "trust": {"publisher": "mock", "trust_level": "L2Tested", "signature": null},
        "visibility": "read"
    })
}

async fn spawn_mock_server() -> PathBuf {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mock.sock");
    let listener = UnixListener::bind(&path).unwrap();
    // Keep the tempdir alive for the duration of the test process.
    std::mem::forget(dir);

    let path_clone = path.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut read, mut write) = stream.into_split();
                loop {
                    let buf = match read_frame(&mut read).await {
                        Ok(b) => b,
                        Err(_) => return,
                    };
                    let req: ServerReq = serde_json::from_slice(&buf).unwrap();
                    let resp = match req {
                        ServerReq::Ping => ServerResp::Pong,
                        ServerReq::ToolList => ServerResp::ToolList {
                            tools: serde_json::json!([sample_tool()]),
                        },
                        ServerReq::ToolSchema { tool_id } => {
                            assert_eq!(tool_id, "mock:echo.say");
                            ServerResp::ToolSchema { schema: sample_tool() }
                        }
                        ServerReq::RunTool { tool_id, args, dry_run } => ServerResp::ToolResult {
                            tool_id,
                            result: serde_json::json!({"echo": args}),
                            success: true,
                            dry_run,
                        },
                    };
                    if write_frame(&mut write, &resp).await.is_err() {
                        return;
                    }
                }
            });
        }
    });

    // Give the listener a beat to be ready.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let _ = path_clone;
    path
}

#[tokio::test]
async fn end_to_end_against_anos_free_mock() {
    let sock = spawn_mock_server().await;
    let client = AtdClient::connect(Endpoint::unix(&sock)).await.unwrap();

    let summaries = client
        .discover(None, DiscoverFilter::default())
        .await
        .unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "mock:echo.say");

    let def = client.describe("mock:echo.say").await.unwrap();
    assert_eq!(def.capability.domain, "echo");

    let result = client
        .call(
            "mock:echo.say",
            serde_json::json!({"hello": "world"}),
            CallOptions::default(),
        )
        .await
        .unwrap();
    assert!(result.is_success());
    assert_eq!(
        result.data().unwrap()["echo"]["hello"],
        serde_json::json!("world")
    );
}
