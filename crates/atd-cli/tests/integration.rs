//! Integration test: spawn the `atd` binary against a mock Unix server and
//! assert on stdout / exit status.

use std::path::PathBuf;
use std::process::Command;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

/// Locate the compiled `atd` binary. Cargo sets `CARGO_BIN_EXE_atd` when
/// building integration tests for a crate with `[[bin]]`.
fn atd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atd"))
}

async fn spawn_3_tool_mock() -> PathBuf {
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
                                {"id":"anos:fs.write","description":"Write a file","tier":"hot","visibility":"write"},
                                {"id":"anos:web.search","description":"Search the web","tier":"hot","visibility":"read"}
                            ]
                        }),
                        _ => serde_json::json!({"type":"error","message":"unexpected"}),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atd_list_against_mock_prints_table() {
    let sock = spawn_3_tool_mock().await;
    let bin = atd_bin();
    let sock_str = sock.to_str().unwrap().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args(["--sock", &sock_str, "list"])
            .output()
            .expect("atd binary should run")
    })
    .await
    .unwrap();
    assert!(output.status.success(), "non-zero exit, stderr={}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("anos:fs.read"));
    assert!(stdout.contains("anos:web.search"));
    assert!(stdout.contains("3 tool(s) total"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atd_list_json_flag_produces_parseable_array() {
    let sock = spawn_3_tool_mock().await;
    let bin = atd_bin();
    let sock_str = sock.to_str().unwrap().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args(["--sock", &sock_str, "list", "--json"])
            .output()
            .expect("atd binary should run")
    })
    .await
    .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atd_doctor_prints_3_tools_for_reachable_mock() {
    let sock = spawn_3_tool_mock().await;
    let bin = atd_bin();
    let sock_str = sock.to_str().unwrap().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args(["--sock", &sock_str, "doctor"])
            .output()
            .expect("atd binary should run")
    })
    .await
    .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("tool count:    3"));
    assert!(stdout.contains("ping:          ok"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atd_exits_nonzero_when_sock_missing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.sock");
    let bin = atd_bin();
    let sock_str = missing.to_str().unwrap().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args(["--sock", &sock_str, "list"])
            .output()
            .expect("atd binary should run")
    })
    .await
    .unwrap();
    assert!(!output.status.success(), "expected non-zero exit when socket missing");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("atd:"), "stderr should start with 'atd:' prefix, got: {stderr}");
}
