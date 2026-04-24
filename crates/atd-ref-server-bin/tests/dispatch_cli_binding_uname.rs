//! SP-12 Task 4 — CliBinding integration via `ref:external.uname`.
//!
//! Proves that dispatch routes a registered tool through `CliBinding` when
//! the registry entry's binding says so. Complements the in-crate unit
//! tests in `binding.rs` with an end-to-end wire round-trip.
//!
//! Unix-only: `/usr/bin/uname` is not universally available on Windows, so
//! the tool is `#[cfg(unix)]`-gated in the registry.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use atd_ref_server_bin::builtin::builtin_registry;
use atd_ref_server_bin::server::{Server, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

struct ServerHandle {
    sock: PathBuf,
    _tempdir: tempfile::TempDir,
    _task: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn spawn() -> ServerHandle {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("server.sock");
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap(),
        max_output_bytes: 1_048_576,
        default_call_timeout_ms: 5_000,
        granted_capabilities: vec![],
        audit_sink: None,
    };
    let server = Server::new(builtin_registry(false), cfg);
    let task = tokio::spawn(server.run());

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if sock.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
            return ServerHandle {
                sock,
                _tempdir: dir,
                _task: task,
            };
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not create socket within 5s at {sock:?}");
}

async fn send_one(sock: &std::path::Path, req: serde_json::Value) -> serde_json::Value {
    let mut stream = UnixStream::connect(sock).await.unwrap();
    let body = serde_json::to_vec(&req).unwrap();
    stream
        .write_all(&(body.len() as u32).to_be_bytes())
        .await
        .unwrap();
    stream.write_all(&body).await.unwrap();
    stream.flush().await.unwrap();
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await.unwrap();
    let n = u32::from_be_bytes(header) as usize;
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await.unwrap();
    serde_json::from_slice(&buf).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uname_dispatches_through_cli_binding_and_returns_kernel_name() {
    // Require the host program. If someone runs this on an exotic box
    // without /usr/bin/uname, skip gracefully rather than fail.
    if !std::path::Path::new("/usr/bin/uname").exists() {
        eprintln!("skipping: /usr/bin/uname not present");
        return;
    }

    let srv = spawn().await;
    let r = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:external.uname",
            "args": {"flag": "-s"},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["exit_code"], 0);
    let stdout = r["result"]["stdout"].as_str().unwrap();
    // Linux CI → "Linux\n"; macOS dev box → "Darwin\n". Either is valid.
    assert!(
        stdout == "Linux\n" || stdout == "Darwin\n",
        "unexpected uname -s output: {stdout:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uname_default_flag_is_dash_s() {
    if !std::path::Path::new("/usr/bin/uname").exists() {
        eprintln!("skipping: /usr/bin/uname not present");
        return;
    }
    let srv = spawn().await;
    // Omit the "flag" arg; CliBinding's args_mapper defaults to -s.
    let r = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:external.uname",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["success"], serde_json::json!(true));
    let stdout = r["result"]["stdout"].as_str().unwrap();
    assert!(stdout.ends_with('\n'));
    assert!(!stdout.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uname_discoverable_in_tool_list() {
    let srv = spawn().await;
    let r = send_one(&srv.sock, serde_json::json!({"type": "tool_list"})).await;
    let tools = r["tools"].as_array().unwrap();
    let entry = tools
        .iter()
        .find(|t| t["id"] == "ref:external.uname")
        .expect("uname must be listed on unix");
    // Confirms the Hot tier declaration flows through ToolSummary.
    assert_eq!(entry["tier"], "hot");
}
