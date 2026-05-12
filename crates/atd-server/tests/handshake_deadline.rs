//! SP-concurrency-baseline §5.2 — verify the per-state frame deadline
//! actually closes a stalled connection.
//!
//! Failure mode this catches: a client that completes `UnixStream::connect()`
//! but never writes a frame would (pre-SP) hold the server's per-connection
//! task forever, since the `read_frame` loop was unbounded. With deadlines
//! applied, the handshake-window deadline (default 5s, overridden here to
//! 200ms for test speed) fires and the connection task returns Ok cleanly.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use atd_runtime::registry::Registry;
use atd_server::{Server, ServerConfig};
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handshake_deadline_closes_stalled_connection() {
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("stall.sock");

    let reg = Registry::new();
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        ..ServerConfig::default()
    };
    let mut server = Server::new(reg, cfg);
    // Tight 200ms handshake window; default would be 5s.
    server.set_frame_deadlines(30_000, 200);
    let task = tokio::spawn(server.run());

    // Wait for the socket to appear.
    let deadline = Instant::now() + Duration::from_secs(3);
    while !sock.exists() {
        if Instant::now() > deadline {
            panic!("server did not bind socket within 3s");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Open a raw UnixStream and write nothing — never send a frame.
    let mut stream = UnixStream::connect(&sock).await.expect("connect");

    // The server should close the connection within ~200ms + small slack.
    // We detect closure by reading: an unwritten peer will give us 0 bytes
    // (EOF) once the server's handle_connection task returns.
    let mut buf = [0u8; 1];
    let started = Instant::now();
    let read_result = tokio::time::timeout(Duration::from_millis(800), stream.read(&mut buf)).await;

    match read_result {
        Ok(Ok(0)) => {
            // EOF — server closed. Verify it happened within the deadline.
            let elapsed = started.elapsed();
            assert!(
                elapsed < Duration::from_millis(700),
                "server took {elapsed:?} to close stalled connection (deadline was 200ms)"
            );
            assert!(
                elapsed >= Duration::from_millis(150),
                "server closed too eagerly at {elapsed:?} (deadline was 200ms, expected ≥150ms)"
            );
        }
        Ok(Ok(n)) => panic!("read returned {n} bytes; expected EOF"),
        Ok(Err(e)) => panic!("read errored: {e}"),
        Err(_) => panic!("server did not close stalled connection within 800ms"),
    }

    task.abort();
}
