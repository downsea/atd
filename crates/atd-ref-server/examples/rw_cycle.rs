//! Single-connection Write → Read → Edit cycle against atd-ref-server.
//!
//! This example starts a Server instance in the current process, opens ONE
//! connection to it, and performs a full round-trip to illustrate the
//! must-read-before-edit invariant working over the wire.

use std::sync::Arc;
use std::time::Duration;

use atd_ref_server::builtin::builtin_registry;
use atd_server::{Server, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

async fn send_on_stream(
    stream: &mut UnixStream,
    req: serde_json::Value,
) -> std::io::Result<serde_json::Value> {
    let body = serde_json::to_vec(&req).unwrap();
    stream.write_all(&(body.len() as u32).to_be_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let n = u32::from_be_bytes(header) as usize;
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await?;
    Ok(serde_json::from_slice(&buf).unwrap())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Workdir for the demo
    let workdir = tempfile::tempdir()?;
    let sock = workdir.path().join("rw_cycle.sock");
    let file_path = workdir.path().join("demo.txt");

    // Start server in a background task
    let config = ServerConfig {
        socket_path: sock.clone(),
        cwd: workdir.path().to_path_buf(),
        ..ServerConfig::default()
    };
    let server = Server::new(builtin_registry(false), config);
    let _server_handle = Arc::new(tokio::spawn(async move {
        let _ = server.run().await;
    }));

    // Wait for socket to appear
    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !sock.exists() {
        return Err("server did not create socket".into());
    }

    let mut stream = UnixStream::connect(&sock).await?;

    println!("[rw_cycle] 1. Write");
    let w = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.write",
            "args": {
                "path": file_path.to_string_lossy(),
                "content": "hello world\nline two\n"
            },
            "dry_run": false,
        }),
    )
    .await?;
    println!("    result: {}", serde_json::to_string(&w["result"])?);

    println!("[rw_cycle] 2. Read");
    let r = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": file_path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await?;
    println!(
        "    {} lines total, content:\n{}",
        r["result"]["line_count"],
        r["result"]["content"].as_str().unwrap()
    );

    println!("[rw_cycle] 3. Edit (replace 'hello' → 'HI')");
    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": file_path.to_string_lossy(),
                "old_string": "hello",
                "new_string": "HI"
            },
            "dry_run": false,
        }),
    )
    .await?;
    println!("    result: {}", serde_json::to_string(&e["result"])?);

    println!("[rw_cycle] 4. Verify (Read again)");
    let r2 = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": file_path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await?;
    println!(
        "    final content:\n{}",
        r2["result"]["content"].as_str().unwrap()
    );

    println!("[rw_cycle] done.");
    Ok(())
}
