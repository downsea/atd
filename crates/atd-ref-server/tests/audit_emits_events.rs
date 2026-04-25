//! Integration: spawn ref-server with --audit-log <tmpfile>, drive
//! three RunTool requests covering three outcome kinds, then assert
//! the resulting JSON-lines file has the expected shape.
//!
//! Covers SP-operability-v1 C1: the dispatch loop must emit exactly
//! one `CallEvent` per `Request::RunTool` return branch (success,
//! tool_not_found, invalid_args|execution_failed), and each line must
//! be a well-formed `CallEvent` with `schema_version == 1`.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_log_emits_expected_event_kinds() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sock = tmp.path().join("audit.sock");
    let log_path = tmp.path().join("audit.jsonl");

    let bin = ref_server_bin();
    let mut child: Child = Command::new(&bin)
        .arg("--sock")
        .arg(&sock)
        .arg("--grant-capability")
        .arg("read")
        .arg("--grant-capability")
        .arg("write")
        .arg("--grant-capability")
        .arg("exec")
        .arg("--audit-log")
        .arg(&log_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn atd-ref-server");

    wait_for_socket(&sock, Duration::from_secs(5))
        .await
        .expect("sock up");

    // Drive 3 RunTool calls via the SDK. `connect` internally pings; we
    // also need an explicit hello to set caller_id on the server side
    // (the server's Hello arm writes client_id → conn_state.caller_id).
    let client = atd_sdk::AtdClient::connect(atd_sdk::Endpoint::unix(&sock))
        .await
        .expect("connect");
    let _ = client
        .hello(Some("audit-integration-test"), vec![])
        .await
        .expect("hello");

    // (a) success — echo.say with valid args.
    let _ = client
        .call(
            "ref:echo.say",
            serde_json::json!({ "text": "hi" }),
            atd_sdk::CallOptions::default(),
        )
        .await
        .expect("echo call");

    // (b) tool_not_found — a tool id that isn't registered.
    let _ = client
        .call(
            "ref:definitely.does.not.exist",
            serde_json::json!({}),
            atd_sdk::CallOptions::default(),
        )
        .await;

    // (c) invalid_args — fs.read schema requires `path`; empty object
    // fails deserialization with ToolCallError::InvalidArgs. If the
    // future ever changes this to ExecutionFailed, the assertion below
    // accepts both.
    let _ = client
        .call(
            "ref:fs.read",
            serde_json::json!({}),
            atd_sdk::CallOptions::default(),
        )
        .await;

    // Tear down: drop client, kill server, wait. On unix we can't rely
    // on SIGTERM cleanup order; kill is acceptable because audit writes
    // flush synchronously per on_call.
    drop(client);
    let _ = child.kill();
    let _ = child.wait();

    let content = std::fs::read_to_string(&log_path).expect("read log");
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "expected 3 audit lines, got {}: {}",
        lines.len(),
        content
    );

    let kinds: Vec<String> = lines
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("parse jsonl");
            v["outcome"]["kind"]
                .as_str()
                .expect("outcome.kind string")
                .to_string()
        })
        .collect();

    assert!(
        kinds.contains(&"success".to_string()),
        "missing success outcome: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"tool_not_found".to_string()),
        "missing tool_not_found outcome: {:?}",
        kinds
    );
    // fs.read with no path → InvalidArgs in current impl; accept either
    // variant so a future impl change doesn't break the audit contract.
    assert!(
        kinds.contains(&"invalid_args".to_string())
            || kinds.contains(&"execution_failed".to_string()),
        "expected invalid_args or execution_failed for fs.read {{}}, got: {:?}",
        kinds
    );

    // Field-level checks: schema_version pin, tool_id prefix, sane duration,
    // caller_id echoed from the Hello handshake.
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("parse jsonl");
        assert_eq!(v["schema_version"], 1);
        assert!(
            v["tool_id"].as_str().unwrap().starts_with("ref:"),
            "tool_id should be ref:-prefixed, got: {}",
            v["tool_id"]
        );
        assert!(
            v["duration_ms"].as_u64().unwrap() < 5_000,
            "duration unreasonable: {}",
            v["duration_ms"]
        );
        assert_eq!(
            v["caller_id"], "audit-integration-test",
            "caller_id should mirror Hello.client_id"
        );
    }
}

fn ref_server_bin() -> PathBuf {
    // Same-package CARGO_BIN_EXE_ env var: the bin is defined in this
    // crate (`[[bin]] name = "atd-ref-server"`), so Cargo exposes the
    // compiled path as `CARGO_BIN_EXE_atd-ref-server` for integration
    // tests in the same crate.
    PathBuf::from(env!("CARGO_BIN_EXE_atd-ref-server"))
}

async fn wait_for_socket(path: &std::path::Path, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "socket {:?} did not appear within {:?}",
        path, timeout
    ))
}
