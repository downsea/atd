//! atd capstone demo. Auto-spawns `atd-ref-server` (the in-repo neutral
//! reference ATD server), connects via `atd-sdk`, exercises three
//! representative tools end-to-end.
//!
//! This demo has ZERO dependency on an external agent orchestration server.
//! It proves the ATD protocol is vendor-neutral: the client speaks the wire
//! format, the ref-server answers.
//!
//! Run:
//!   cargo build --release -p atd-ref-server
//!   cargo run --example hello_atd
//!
//! Override the server (e.g., to use a third-party ATD server):
//!   ATD_SOCK=/path/to/server.sock cargo run --example hello_atd

use std::path::PathBuf;
use std::time::Duration;

use atd_sdk::{AtdClient, CallOptions, DiscoverFilter, Endpoint};
use tokio::process::{Child, Command};

const SOCKET_WAIT_ATTEMPTS: u32 = 30;
const SOCKET_WAIT_INTERVAL_MS: u64 = 100;

/// Walk up from this example's manifest directory to find the workspace root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples/ should have a parent")
        .to_path_buf()
}

async fn wait_for_socket(sock: &std::path::Path) -> bool {
    for _ in 0..SOCKET_WAIT_ATTEMPTS {
        if sock.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(SOCKET_WAIT_INTERVAL_MS)).await;
    }
    false
}

/// Either return the externally-specified socket, or spawn ref-server with a
/// temp socket. Returns (child_process, tempdir_guard, socket_path). The
/// tempdir_guard keeps the temp directory alive — drop it and the socket
/// file is cleaned up.
async fn acquire_server()
-> Result<(Option<Child>, Option<tempfile::TempDir>, PathBuf), Box<dyn std::error::Error>> {
    if let Ok(override_sock) = std::env::var("ATD_SOCK") {
        let sock = PathBuf::from(override_sock);
        println!("[atd] using ATD_SOCK override → {}", sock.display());
        return Ok((None, None, sock));
    }

    let binary = repo_root().join("target/release/atd-ref-server");
    if !binary.exists() {
        return Err(format!(
            "atd-ref-server release binary not found at {}.\n\
             build it first: cargo build --release -p atd-ref-server",
            binary.display()
        )
        .into());
    }

    let tmp = tempfile::tempdir()?;
    let sock = tmp.path().join("demo.sock");
    println!("[atd] auto-spawning atd-ref-server → {}", sock.display());
    let child = Command::new(&binary)
        .arg("--sock")
        .arg(&sock)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    if !wait_for_socket(&sock).await {
        return Err("ref-server didn't bind its socket within 3s".into());
    }

    Ok((Some(child), Some(tmp), sock))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut child, _tmpdir, sock) = acquire_server().await?;

    // Ensure we clean up on any early exit path.
    let run = async {
        let client = AtdClient::connect(Endpoint::unix(sock.clone())).await?;
        println!("[atd] connected");

        let all = client.discover(None, DiscoverFilter::default()).await?;
        println!("[atd] {} tools registered", all.len());

        // 1/3 — echo.say
        println!();
        println!("[1/3] ref:echo.say {{\"text\":\"hello from ATD\"}}");
        let r = client
            .call(
                "ref:echo.say",
                serde_json::json!({"text": "hello from ATD"}),
                CallOptions {
                    dry_run: false,
                    preferred_binding: None,
                },
            )
            .await?;
        print_result(r)?;

        // 2/3 — fs.glob (find Cargo manifests)
        println!();
        println!("[2/3] ref:fs.glob {{\"pattern\":\"**/*.toml\",\"path\":\".\"}}");
        let r = client
            .call(
                "ref:fs.glob",
                serde_json::json!({"pattern": "**/*.toml", "path": "."}),
                CallOptions {
                    dry_run: false,
                    preferred_binding: None,
                },
            )
            .await?;
        print_glob_result(r)?;

        // 3/3 — shell.exec (platform identity)
        println!();
        println!("[3/3] ref:shell.exec {{\"command\":\"uname -s\"}}");
        let r = client
            .call(
                "ref:shell.exec",
                serde_json::json!({"command": "uname -s"}),
                CallOptions {
                    dry_run: false,
                    preferred_binding: None,
                },
            )
            .await?;
        print_shell_result(r)?;

        println!();
        println!("[atd] done.");
        Ok::<_, Box<dyn std::error::Error>>(())
    };

    let outcome = run.await;

    // Teardown
    if let Some(c) = child.as_mut() {
        let _ = c.kill().await;
        let _ = c.wait().await;
    }

    outcome
}

fn print_result(r: atd_protocol::ToolResult) -> Result<(), Box<dyn std::error::Error>> {
    match r {
        atd_protocol::ToolResult::Success { data, .. } => {
            println!("      → {}", serde_json::to_string(&data)?);
        }
        atd_protocol::ToolResult::Error { code, message, .. } => {
            println!("      ✗ {code}: {message}");
        }
    }
    Ok(())
}

fn print_glob_result(r: atd_protocol::ToolResult) -> Result<(), Box<dyn std::error::Error>> {
    match r {
        atd_protocol::ToolResult::Success { data, .. } => {
            let paths = data["paths"].as_array().cloned().unwrap_or_default();
            let preview: Vec<String> = paths
                .iter()
                .take(3)
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            let suffix = if paths.len() > 3 {
                format!(" (+{} more)", paths.len() - 3)
            } else {
                String::new()
            };
            println!(
                "      → {} paths: {}{}",
                paths.len(),
                preview.join(", "),
                suffix
            );
        }
        atd_protocol::ToolResult::Error { code, message, .. } => {
            println!("      ✗ {code}: {message}");
        }
    }
    Ok(())
}

fn print_shell_result(r: atd_protocol::ToolResult) -> Result<(), Box<dyn std::error::Error>> {
    match r {
        atd_protocol::ToolResult::Success { data, .. } => {
            let exit = data["exit_code"].as_i64().unwrap_or(-1);
            let stdout = data["stdout"].as_str().unwrap_or("").trim();
            println!("      → exit {exit}, stdout={stdout:?}");
        }
        atd_protocol::ToolResult::Error { code, message, .. } => {
            println!("      ✗ {code}: {message}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_for_socket_returns_true_when_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("ready.sock");
        tokio::fs::File::create(&sock).await.unwrap();
        assert!(wait_for_socket(&sock).await);
    }

    #[tokio::test]
    async fn wait_for_socket_returns_false_on_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("never.sock");
        // Using the real constants would take 3s. Verify quickly by checking
        // the function honors nonexistence across at least a few cycles.
        // We test the real helper end-to-end in integration anyway.
        let start = std::time::Instant::now();
        let got = wait_for_socket(&sock).await;
        let elapsed = start.elapsed();
        assert!(!got);
        assert!(
            elapsed >= Duration::from_millis(SOCKET_WAIT_INTERVAL_MS * 2),
            "should have polled at least twice: {elapsed:?}"
        );
    }
}
