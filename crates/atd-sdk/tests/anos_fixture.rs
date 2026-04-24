//! Contract test: atd-sdk must correctly parse the wire shapes that the
//! ANOS daemon actually produces. Fixtures in tests/fixtures/ are captured
//! from a live daemon via scripts/capture_anos_fixtures.sh — refresh them
//! whenever ANOS bumps its protocol.

use atd_sdk::{AtdClient, DiscoverFilter, Endpoint};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing fixture {}: {}", path.display(), e))
}

async fn spawn_replay_server(tool_list: String, tool_schema: String) -> PathBuf {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("replay.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::mem::forget(dir); // keep tempdir alive for test process lifetime

    let path_ret = path.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let list = tool_list.clone();
            let schema = tool_schema.clone();
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
                    let reply: &str = match req.get("type").and_then(|v| v.as_str()) {
                        Some("ping") => r#"{"type":"pong"}"#,
                        Some("tool_list") => list.as_str(),
                        Some("tool_schema") => schema.as_str(),
                        other => panic!("replay server got unexpected request type: {:?}", other),
                    };
                    let body = reply.as_bytes();
                    if w.write_all(&(body.len() as u32).to_be_bytes())
                        .await
                        .is_err()
                    {
                        return;
                    }
                    if w.write_all(body).await.is_err() {
                        return;
                    }
                    if w.flush().await.is_err() {
                        return;
                    }
                }
            });
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    path_ret
}

#[tokio::test]
async fn discover_against_real_anos_tool_list_fixture() {
    let list = fixture("anos_tool_list.json");
    let schema = fixture("anos_tool_schema_fs_read.json");
    let sock = spawn_replay_server(list, schema).await;

    let client = AtdClient::connect(Endpoint::unix(&sock)).await.unwrap();
    let summaries = client
        .discover(None, DiscoverFilter::default())
        .await
        .unwrap();

    // ANOS returns >= 100 hot tools locally. This test is version-tolerant:
    // we assert a strict lower bound and that known tools parse correctly.
    assert!(
        summaries.len() >= 50,
        "expected >=50 tools in fixture, got {}",
        summaries.len()
    );

    let fs_read = summaries
        .iter()
        .find(|s| s.id == "anos:fs.read")
        .expect("fixture must contain anos:fs.read");
    assert_eq!(fs_read.domain, "fs");
    assert!(
        !fs_read.name.is_empty(),
        "name must be filled from description or id"
    );
}

#[tokio::test]
async fn describe_against_real_anos_tool_schema_fixture() {
    let list = fixture("anos_tool_list.json");
    let schema = fixture("anos_tool_schema_fs_read.json");
    let sock = spawn_replay_server(list, schema).await;

    let client = AtdClient::connect(Endpoint::unix(&sock)).await.unwrap();
    let def = client.describe("anos:fs.read").await.unwrap();

    assert_eq!(def.id, "anos:fs.read");
    assert_eq!(def.capability.domain, "fs");
    assert!(
        def.bindings.iter().any(|b| matches!(
            b.protocol,
            atd_protocol::BindingProtocol::AppFunction | atd_protocol::BindingProtocol::Cli
        )),
        "expected at least one known binding protocol"
    );
}
