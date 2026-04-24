//! SP-operability-v1 C2 — `max_concurrent` rate-limit integration test.
//!
//! Spawns an in-process `atd-ref-server` whose registry contains a single
//! `BlockingTool` with `max_concurrent: 1` whose `call` awaits a shared
//! `Notify`. Two clients connect concurrently:
//!   * A arrives first → `try_acquire_owned` succeeds → blocks on Notify.
//!   * B arrives ~100 ms later → `try_acquire_owned` fails → returns
//!     `Response::Error { code: 1002, retryable: true, .. }` immediately.
//!
//! After asserting B's shape, the test notifies Notify so A completes; a
//! third client C then calls the same tool and succeeds, proving the
//! permit was released on A's return path (not leaked).
//!
//! This is the end-to-end counterpart to
//! `registry::tests::semaphore_permits_match_max_concurrent`, which only
//! covers the permit-sizing half.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_ref_server_bin::server::{Server, ServerConfig};
use atd_runtime::context::CallContext;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::Notify;

struct BlockingTool {
    def: ToolDefinition,
    gate: Arc<Notify>,
}

impl BlockingTool {
    fn new(gate: Arc<Notify>) -> Self {
        let def = ToolDefinition {
            id: "test:blocker".into(),
            name: "blocker".into(),
            description: "awaits notify, used to pin a permit".into(),
            version: "0.0.0".into(),
            capability: ToolCapability {
                domain: "test".into(),
                actions: vec!["block".into()],
                tags: vec![],
                intent_examples: vec![],
            },
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({"type": "object"}),
            bindings: vec![ToolBinding {
                protocol: BindingProtocol::Cli,
                config: serde_json::json!({}),
            }],
            safety: ToolSafety {
                level: SafetyLevel::Read,
                dry_run: false,
                side_effects: vec![],
                data_sensitivity: None,
            },
            resources: ToolResources {
                timeout_ms: 5_000,
                max_concurrent: 1,
                rate_limit_per_min: None,
                estimated_tokens: None,
            },
            trust: ToolTrust {
                publisher: "test".into(),
                trust_level: TrustLevel::L0Unverified,
                signature: None,
            },
            visibility: ToolVisibility::Read,
            required_capabilities: vec![],
            tier: None,
        };
        Self { def, gate }
    }
}

impl Tool for BlockingTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        let gate = self.gate.clone();
        Box::pin(async move {
            gate.notified().await;
            Ok(serde_json::json!({ "done": true }))
        })
    }
}

struct ServerHandle {
    sock: PathBuf,
    _tempdir: tempfile::TempDir,
    _task: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn spawn(gate: Arc<Notify>) -> ServerHandle {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("server.sock");

    let mut registry = Registry::new();
    registry.register(Arc::new(BlockingTool::new(gate)));

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap(),
        max_output_bytes: 1_048_576,
        default_call_timeout_ms: 10_000,
        granted_capabilities: vec![],
        audit_sink: None,
    };

    let server = Server::new(registry, cfg);
    let task = tokio::spawn(server.run());

    // Wait for socket to appear.
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

async fn write_frame(stream: &mut UnixStream, req: &serde_json::Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(req).unwrap();
    stream.write_all(&(body.len() as u32).to_be_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await
}

async fn read_frame(stream: &mut UnixStream) -> std::io::Result<serde_json::Value> {
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let n = u32::from_be_bytes(header) as usize;
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await?;
    Ok(serde_json::from_slice(&buf).unwrap())
}

async fn send_one(sock: &std::path::Path, req: serde_json::Value) -> serde_json::Value {
    let mut stream = UnixStream::connect(sock).await.unwrap();
    write_frame(&mut stream, &req).await.unwrap();
    read_frame(&mut stream).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn max_concurrent_saturation_yields_1002() {
    let gate = Arc::new(Notify::new());
    let srv = spawn(gate.clone()).await;

    // Client A: fires first, acquires the single permit, then blocks in
    // `BlockingTool::call` awaiting the Notify. Run in a detached task so
    // we can drive Client B in parallel from the main test task.
    let sock_a = srv.sock.clone();
    let handle_a = tokio::spawn(async move {
        let mut stream = UnixStream::connect(&sock_a).await.unwrap();
        write_frame(
            &mut stream,
            &serde_json::json!({
                "type": "run_tool",
                "tool_id": "test:blocker",
                "args": {},
                "dry_run": false,
            }),
        )
        .await
        .unwrap();
        read_frame(&mut stream).await.unwrap()
    });

    // Give A enough wall-clock time to reach `try_acquire_owned` and
    // begin awaiting the Notify. 100 ms is roomy vs Unix-socket latency
    // and avoids flakiness on loaded CI boxes.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client B: should land on a saturated semaphore and get 1002 back
    // without ever waking BlockingTool::call.
    let b = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:blocker",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;

    assert_eq!(b["type"], "error");
    assert_eq!(b["code"], 1002);
    assert_eq!(b["retryable"], serde_json::json!(true));
    assert!(
        b["message"].as_str().unwrap().contains("rate limited"),
        "message should mention rate limiting: {}",
        b["message"]
    );
    assert_eq!(b["details"]["tool_id"], "test:blocker");
    assert_eq!(b["details"]["limit"], 1);

    // Release A: the Notify wakes `call`, which completes with `done`.
    gate.notify_one();
    let a = handle_a.await.expect("client A task joined");
    assert_eq!(a["type"], "tool_result");
    assert_eq!(a["success"], serde_json::json!(true));
    assert_eq!(a["result"]["done"], serde_json::json!(true));

    // Client C: proves the permit was released on A's return — if it had
    // leaked, C would also see 1002. Pre-arm the Notify so the tool's
    // inner await resolves immediately.
    gate.notify_one();
    let c = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:blocker",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(c["type"], "tool_result", "post-release client must succeed");
    assert_eq!(c["success"], serde_json::json!(true));
    assert_eq!(c["result"]["done"], serde_json::json!(true));
}
