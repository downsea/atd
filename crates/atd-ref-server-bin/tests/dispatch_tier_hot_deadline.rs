//! SP-12 Task 3 — tier-aware dispatch integration test.
//!
//! Proves that the `tier` field on `ToolDefinition` drives the per-call
//! deadline via `TierPolicy`. A hot-tier tool that sleeps past the hot
//! timeout is killed by the deadline; the same sleep under the warm default
//! budget completes. This pins "`tier` is load-bearing" — no longer a
//! decorative field — while preserving back-compat: tools that declare no
//! tier default to Warm and keep their pre-SP-12 deadlines.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_ref_server_bin::server::{Server, ServerConfig};
use atd_runtime::tier::TierPolicy;
use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTier, ToolTrust, ToolVisibility, TrustLevel,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// A tool that sleeps then returns, honoring `ctx.remaining_time()` as the
/// budget (the pattern every well-behaved tool should follow).
struct SleepTool {
    def: ToolDefinition,
    sleep: Duration,
}

impl SleepTool {
    fn new(id: &str, sleep: Duration, tier: Option<ToolTier>) -> Self {
        Self {
            def: ToolDefinition {
                id: id.into(),
                name: id.into(),
                description: "sleeps then returns".into(),
                version: "0.0.0".into(),
                capability: ToolCapability {
                    domain: "test".into(),
                    actions: vec!["sleep".into()],
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
                    timeout_ms: 60_000,
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
                tier,
            },
            sleep,
        }
    }
}

impl Tool for SleepTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(
        &'a self,
        _args: serde_json::Value,
        ctx: &'a atd_runtime::context::CallContext,
    ) -> CallFuture<'a> {
        let sleep = self.sleep;
        let budget = ctx.remaining_time();
        let tier_observed = ctx.tier;
        Box::pin(async move {
            // Honor the dispatch-provided deadline: if the sleep would exceed
            // the budget, timeout fires and we surface ExecutionFailed. Using
            // tokio::time::timeout here mirrors the recommended tool pattern.
            let work = async {
                tokio::time::sleep(sleep).await;
                Ok::<_, ToolCallError>(serde_json::json!({
                    "ran": true,
                    "tier_observed": format!("{tier_observed:?}"),
                }))
            };
            match budget {
                Some(b) => match tokio::time::timeout(b, work).await {
                    Ok(r) => r,
                    Err(_) => Err(ToolCallError::ExecutionFailed {
                        code: "TIMEOUT".into(),
                        message: "deadline exceeded".into(),
                        retryable: false,
                    }),
                },
                None => work.await,
            }
        })
    }
}

struct ServerHandle {
    sock: PathBuf,
    _tempdir: tempfile::TempDir,
    _task: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn spawn_with(registry: Registry, tier_policy: TierPolicy) -> ServerHandle {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("server.sock");

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap(),
        max_output_bytes: 1_048_576,
        // Outer fallback; tier-derived deadline should win.
        default_call_timeout_ms: 60_000,
        granted_capabilities: vec![],
    };

    let mut server = Server::new(registry, cfg);
    server.set_tier_policy(tier_policy);
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
async fn hot_tier_tool_times_out_when_sleep_exceeds_hot_budget() {
    // Override: hot=100ms. Tool sleeps 500ms → must be killed by deadline.
    let mut policy = TierPolicy::defaults();
    policy
        .apply_override("hot=timeout_ms=100")
        .expect("override parse");

    let mut reg = Registry::new();
    reg.register(Arc::new(SleepTool::new(
        "test:sleep.hot",
        Duration::from_millis(500),
        Some(ToolTier::Hot),
    )));
    let srv = spawn_with(reg, policy).await;

    let r = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:sleep.hot",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(false));
    assert_eq!(r["result"]["code"], "TIMEOUT");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn warm_tier_tool_with_same_sleep_succeeds_under_default_budget() {
    // Same 500ms sleep as above, but Warm tier (5s budget) → succeeds.
    let policy = TierPolicy::defaults();
    let mut reg = Registry::new();
    reg.register(Arc::new(SleepTool::new(
        "test:sleep.warm",
        Duration::from_millis(500),
        Some(ToolTier::Warm),
    )));
    let srv = spawn_with(reg, policy).await;

    let r = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:sleep.warm",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["ran"], serde_json::json!(true));
    assert_eq!(r["result"]["tier_observed"], "Warm");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_without_tier_defaults_to_warm() {
    // tier: None. Must use warm budget (5s) → the 200ms sleep succeeds.
    let policy = TierPolicy::defaults();
    let mut reg = Registry::new();
    reg.register(Arc::new(SleepTool::new(
        "test:sleep.untired",
        Duration::from_millis(200),
        None,
    )));
    let srv = spawn_with(reg, policy).await;

    let r = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:sleep.untired",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["tier_observed"], "Warm");
}
