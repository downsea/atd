//! End-to-end integration: spawn the `atd-ref-server` binary and drive it
//! over a real Unix socket with a self-contained client. Deliberately no
//! dependency on `atd-client` — this verifies the server is reachable by
//! any correct ATD client, not a specific SDK.

use std::path::PathBuf;
use std::time::Duration;
use std::time::Duration as StdDuration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atd-ref-server"))
}

/// Self-contained tiny client. Same pattern as atd-client's mock_server.rs but
/// inverted: here the client is in the test file and the server is the
/// production binary we just built.
async fn send_one_request(
    sock: &std::path::Path,
    req: &serde_json::Value,
) -> std::io::Result<serde_json::Value> {
    let mut stream = UnixStream::connect(sock).await?;
    let body = serde_json::to_vec(req).unwrap();
    let len = (body.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;

    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let n = u32::from_be_bytes(header) as usize;
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await?;
    Ok(serde_json::from_slice(&buf).unwrap())
}

async fn send_on_stream(
    stream: &mut UnixStream,
    req: serde_json::Value,
) -> serde_json::Value {
    let body = serde_json::to_vec(&req).unwrap();
    stream.write_all(&(body.len() as u32).to_be_bytes()).await.unwrap();
    stream.write_all(&body).await.unwrap();
    stream.flush().await.unwrap();
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await.unwrap();
    let n = u32::from_be_bytes(header) as usize;
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await.unwrap();
    serde_json::from_slice(&buf).unwrap()
}

#[allow(dead_code)]
struct ServerHandle {
    // The Child MUST be held to keep the process alive (kill_on_drop=true).
    child: Child,
    pub sock: PathBuf,
    // The tempdir MUST be held so the socket file survives.
    tempdir: tempfile::TempDir,
}

async fn spawn_server() -> ServerHandle {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("server.sock");

    let mut child = Command::new(bin_path())
        .arg("--sock")
        .arg(&sock)
        .kill_on_drop(true)
        .spawn()
        .expect("spawn atd-ref-server");

    // Poll for socket file to appear (max ~5s).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if sock.exists() {
            // Give the listener a tick to be accept()-ready.
            tokio::time::sleep(Duration::from_millis(20)).await;
            return ServerHandle { child, sock, tempdir: dir };
        }
        if let Ok(Some(status)) = child.try_wait() {
            panic!("server exited before creating socket: status {status:?}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not create socket within 5s at {sock:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_ping_returns_pong() {
    let srv = spawn_server().await;
    let r = send_one_request(&srv.sock, &serde_json::json!({"type": "ping"}))
        .await
        .unwrap();
    assert_eq!(r["type"], "pong");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_tool_list_returns_echo() {
    let srv = spawn_server().await;
    let r = send_one_request(&srv.sock, &serde_json::json!({"type": "tool_list"}))
        .await
        .unwrap();
    assert_eq!(r["type"], "tool_list");
    let tools = r["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 4);
    let ids: Vec<&str> = tools.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"ref:echo.say"));
    assert!(ids.contains(&"ref:fs.read"));
    assert!(ids.contains(&"ref:fs.write"));
    assert!(ids.contains(&"ref:fs.edit"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_tool_schema_returns_full_definition() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({"type": "tool_schema", "tool_id": "ref:echo.say"}),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_schema");
    assert_eq!(r["schema"]["id"], "ref:echo.say");
    assert_eq!(r["schema"]["capability"]["domain"], "echo");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_tool_schema_not_found_returns_error() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({"type": "tool_schema", "tool_id": "ref:missing"}),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "error");
    assert!(r["message"].as_str().unwrap().contains("tool not found"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_run_tool_success_echoes_args() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:echo.say",
            "args": {"hello": "world"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["echoed"]["hello"], "world");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_run_tool_dry_run_returns_preview() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:echo.say",
            "args": {"x": 1},
            "dry_run": true,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["dry_run"], serde_json::json!(true));
    assert_eq!(r["result"]["dry_run"], serde_json::json!(true));
    assert_eq!(r["result"]["args_preview"]["x"], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_multiple_requests_on_one_connection() {
    let srv = spawn_server().await;
    // Open ONE stream, send two requests in sequence, read two responses.
    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    async fn one(
        stream: &mut UnixStream,
        req: serde_json::Value,
    ) -> serde_json::Value {
        let body = serde_json::to_vec(&req).unwrap();
        stream.write_all(&(body.len() as u32).to_be_bytes()).await.unwrap();
        stream.write_all(&body).await.unwrap();
        stream.flush().await.unwrap();

        let mut header = [0u8; 4];
        stream.read_exact(&mut header).await.unwrap();
        let n = u32::from_be_bytes(header) as usize;
        let mut buf = vec![0u8; n];
        stream.read_exact(&mut buf).await.unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    let r1 = one(&mut stream, serde_json::json!({"type": "ping"})).await;
    assert_eq!(r1["type"], "pong");
    let r2 = one(&mut stream, serde_json::json!({"type": "tool_list"})).await;
    assert_eq!(r2["type"], "tool_list");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_fs_write_then_read_roundtrip() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("roundtrip.txt");

    // Write
    let w = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.write",
            "args": {"path": path.to_string_lossy(), "content": "hello\nworld\n"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(w["success"], serde_json::json!(true));
    assert_eq!(w["result"]["bytes_written"], 12);

    // Read (new connection OK; Read doesn't need tracker history)
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["line_count"], 2);
    assert!(r["result"]["content"].as_str().unwrap().contains("   1\thello"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_read_then_edit_same_connection_succeeds() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("rw.txt");
    std::fs::write(&path, "hello world\n").unwrap();

    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    // Read first (records in tracker)
    let r = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["success"], serde_json::json!(true));

    // Then Edit on the SAME connection
    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "hello",
                "new_string": "HI"
            },
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(e["success"], serde_json::json!(true));
    assert_eq!(e["result"]["replacements"], 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "HI world\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_edit_without_prior_read_returns_not_read() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("no-read-edit.txt");
    std::fs::write(&path, "hello\n").unwrap();

    // Fresh connection — tracker is empty. Edit must reject.
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "hello",
                "new_string": "hi"
            },
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(false));
    assert_eq!(r["result"]["code"], "NOT_READ");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_edit_after_external_modification_returns_file_modified() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("ext-mod.txt");
    std::fs::write(&path, "original\n").unwrap();

    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    // Read to populate tracker.
    let _ = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await;

    // External modification + wait for mtime to move forward.
    tokio::time::sleep(StdDuration::from_millis(1100)).await;
    std::fs::write(&path, "externally changed\n").unwrap();

    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "externally",
                "new_string": "xxx"
            },
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(e["success"], serde_json::json!(false));
    assert_eq!(e["result"]["code"], "FILE_MODIFIED");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_edit_multi_match_without_replace_all_is_invalid_args() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("multi.txt");
    std::fs::write(&path, "foo foo foo\n").unwrap();

    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();
    // Populate tracker via Read.
    let _ = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await;

    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "foo",
                "new_string": "bar"
            },
            "dry_run": false,
        }),
    )
    .await;
    // InvalidArgs maps to wire `error` response (not a tool_result).
    assert_eq!(e["type"], "error");
    assert!(e["message"].as_str().unwrap().contains("replace_all"));
    assert!(e["message"].as_str().unwrap().contains("3"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_edit_multi_match_with_replace_all_succeeds() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("multi-ok.txt");
    std::fs::write(&path, "foo foo foo\n").unwrap();

    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();
    let _ = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await;
    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "foo",
                "new_string": "bar",
                "replace_all": true
            },
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(e["success"], serde_json::json!(true));
    assert_eq!(e["result"]["replacements"], 3);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "bar bar bar\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_read_with_offset_beyond_file_returns_empty() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("short.txt");
    std::fs::write(&path, "only two\nlines\n").unwrap();

    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy(), "offset": 100},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["line_count"], 0);
    assert_eq!(r["result"]["total_lines"], 2);
    assert_eq!(r["result"]["content"], "");
}
