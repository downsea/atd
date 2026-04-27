//! Integration test for `atd skills sync`. Spawns a mock ATD server that
//! implements the skills meta-tool convention (a `stub:test.skills.list`
//! tool returning two summaries + a `stub:test.skills.get` tool returning
//! `{name, content_md}`), then shells out to the built `atd` binary and
//! asserts on the stdout / filesystem effects of the sync.

use std::path::PathBuf;
use std::process::Command;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

fn atd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atd"))
}

async fn spawn_skills_mock() -> PathBuf {
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
                        Some("tool_list") => serde_json::json!({
                            "type":"tool_list",
                            "tools":[
                                {
                                    "id":"stub:test.skills.list",
                                    "name":"skills.list",
                                    "description":"List skills",
                                    "domain":"test",
                                    "tier":"hot",
                                    "visibility":"read"
                                },
                                {
                                    "id":"stub:test.skills.get",
                                    "name":"skills.get",
                                    "description":"Get one skill",
                                    "domain":"test",
                                    "tier":"hot",
                                    "visibility":"read"
                                }
                            ]
                        }),
                        Some("run_tool") => {
                            let tool_id = req["tool_id"].as_str().unwrap_or("");
                            let args = &req["args"];
                            match tool_id {
                                "stub:test.skills.list" => serde_json::json!({
                                    "type":"tool_result",
                                    "tool_id": tool_id,
                                    "result": [
                                        {"name":"alpha","description":"alpha skill"},
                                        {"name":"beta","description":"beta skill"}
                                    ],
                                    "success": true,
                                    "dry_run": false
                                }),
                                "stub:test.skills.get" => {
                                    let name = args["name"].as_str().unwrap_or("");
                                    if name == "alpha" || name == "beta" {
                                        serde_json::json!({
                                            "type":"tool_result",
                                            "tool_id": tool_id,
                                            "result": {
                                                "name": name,
                                                "content_md": format!("# {name}\n\ncontent for {name}\n")
                                            },
                                            "success": true,
                                            "dry_run": false
                                        })
                                    } else {
                                        serde_json::json!({
                                            "type":"tool_result",
                                            "tool_id": tool_id,
                                            "result": {
                                                "code":"skill_not_found",
                                                "message":format!("unknown skill: {name}"),
                                                "retryable":false
                                            },
                                            "success": false,
                                            "dry_run": false
                                        })
                                    }
                                }
                                _ => serde_json::json!({"type":"error","message":"unknown tool"}),
                            }
                        }
                        _ => serde_json::json!({"type":"error","message":"unexpected"}),
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skills_sync_stdout_round_trips_two_stubs() {
    let sock = spawn_skills_mock().await;
    let bin = atd_bin();
    let sock_str = sock.to_str().unwrap().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args([
                "--sock", &sock_str, "skills", "sync", "--target", "stdout",
            ])
            .output()
            .expect("atd binary should run")
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "exit failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("--- stub-test-alpha ---"));
    assert!(stdout.contains("content for alpha"));
    assert!(stdout.contains("--- stub-test-beta ---"));
    assert!(stdout.contains("content for beta"));
    assert!(stdout.contains("2 skill(s) synced from 1 publisher(s)"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skills_sync_hermes_writes_two_files() {
    let sock = spawn_skills_mock().await;
    let bin = atd_bin();
    let sock_str = sock.to_str().unwrap().to_owned();
    let out_dir = tempfile::tempdir().unwrap();
    let out_dir_str = out_dir.path().to_str().unwrap().to_owned();

    let output = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args([
                "--sock",
                &sock_str,
                "skills",
                "sync",
                "--target",
                "hermes",
                "--out-dir",
                &out_dir_str,
            ])
            .output()
            .expect("atd binary should run")
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "exit failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let alpha = out_dir.path().join("stub-test-alpha/SKILL.md");
    let beta = out_dir.path().join("stub-test-beta/SKILL.md");
    assert!(alpha.exists(), "alpha SKILL.md must exist: {alpha:?}");
    assert!(beta.exists(), "beta SKILL.md must exist: {beta:?}");
    let alpha_content = std::fs::read_to_string(&alpha).expect("read alpha");
    assert!(
        alpha_content.contains("content for alpha"),
        "alpha content mismatch: {alpha_content}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skills_sync_no_skills_tool_warns_and_exits_clean() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::mem::forget(dir);

    let sock = path.clone();
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
                        Some("tool_list") => serde_json::json!({
                            "type":"tool_list",
                            "tools":[
                                {"id":"anos:fs.read","name":"read","description":"x","domain":"fs","tier":"hot","visibility":"read"}
                            ]
                        }),
                        _ => serde_json::json!({"type":"error","message":"x"}),
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

    let bin = atd_bin();
    let sock_str = sock.to_str().unwrap().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(bin)
            .args([
                "--sock", &sock_str, "skills", "sync", "--target", "stdout",
            ])
            .output()
            .expect("atd binary should run")
    })
    .await
    .unwrap();
    assert!(output.status.success(), "exit must be clean even when no skills tool");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("no *.skills.list tool found"),
        "stdout: {stdout}"
    );
}
