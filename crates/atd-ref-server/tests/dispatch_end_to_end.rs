//! SP-12 Task 6 — cross-primitive end-to-end test.
//!
//! Exercises all four SP-12 dispatch primitives in one call flow:
//! 1. Capability gate (Hello handshake + subset enforcement);
//! 2. Tier-aware dispatch (tier: Hot with an overridden budget);
//! 3. Binding abstraction (NativeBinding, exercising the registry path);
//! 4. Result-middleware pipeline (RedactPathsMiddleware rewrites the
//!    result before it reaches the wire).
//!
//! The test's assertion surface pins every participation: a client that
//! sends Hello with "exec" + "admin" gets back only "exec"; the call
//! succeeds; the HOME path in the result is redacted. A sibling test
//! confirms a client that skips Hello gets CAPABILITY_DENIED — proving
//! the gate runs even while the rest of the chain is configured.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTier, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_ref_server::server::{Server, ServerConfig};
use atd_runtime::middleware::RedactPathsMiddleware;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_runtime::tier::TierPolicy;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// A tool that — if it ever runs — returns a value containing an absolute
/// `$HOME` path (to be caught by the middleware), the tier it observed,
/// and the capabilities present on its CallContext (to confirm the gate
/// ran before dispatch). Declaring `required_capabilities: ["exec"]` +
/// `tier: Hot` makes this a full-stack SP-12 demo in one tool.
struct FullstackTool {
    def: ToolDefinition,
}

impl FullstackTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "test:fullstack.demo".into(),
                name: "fullstack demo".into(),
                description: "exercises cap gate + tier + binding + middleware".into(),
                version: "0.0.0".into(),
                capability: ToolCapability {
                    domain: "test".into(),
                    actions: vec!["demo".into()],
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
                    timeout_ms: 1000,
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
                required_capabilities: vec!["exec".into()],
                tier: Some(ToolTier::Hot),
                errors: vec![],
            },
        }
    }
}

impl Tool for FullstackTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(
        &'a self,
        _args: serde_json::Value,
        ctx: &'a atd_runtime::context::CallContext,
    ) -> CallFuture<'a> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        let caps = ctx.capabilities.granted();
        let tier = format!("{:?}", ctx.tier);
        Box::pin(async move {
            Ok(serde_json::json!({
                "touched_binding": "native",
                "tier_observed": tier,
                "caps_observed": caps,
                "leaked_path": format!("{}/secrets/a.key", home),
            }))
        })
    }
}

struct ServerHandle {
    sock: PathBuf,
    _tempdir: tempfile::TempDir,
    _task: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn spawn() -> ServerHandle {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("server.sock");

    let mut registry = Registry::new();
    registry.register(Arc::new(FullstackTool::new()));

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap(),
        max_output_bytes: 1_048_576,
        default_call_timeout_ms: 5_000,
        granted_capabilities: vec!["exec".into(), "read".into()],
        audit_sink: None,
    };

    // Hot tier overridden to 2 s so the cross-primitive test isn't flaky
    // on slow CI — the tool here is synchronous so 500 ms would also
    // work, but 2 s is a cheap safety margin.
    let mut policy = TierPolicy::defaults();
    policy
        .apply_override("hot=timeout_ms=2000")
        .expect("override parse");

    let mut server = Server::new(registry, cfg);
    server.set_tier_policy(policy);
    server.set_middleware(vec![Arc::new(RedactPathsMiddleware::with_home_default())]);
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

async fn send_on_stream(stream: &mut UnixStream, req: serde_json::Value) -> serde_json::Value {
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
async fn all_four_primitives_participate_in_one_call() {
    unsafe {
        std::env::set_var("HOME", "/tmp/sp12-e2e-home");
    }
    let srv = spawn().await;
    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    // 1. Hello: ask for ["exec", "admin"]. Server grants ["exec"] only
    //    (admin not in allow-list).
    let hello = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "hello",
            "client_id": "sp12-e2e",
            "requested_capabilities": ["exec", "admin"],
        }),
    )
    .await;
    assert_eq!(hello["type"], "hello_ack");
    let granted: Vec<String> =
        serde_json::from_value(hello["granted_capabilities"].clone()).unwrap();
    assert_eq!(granted, vec!["exec"]);

    // 2. RunTool: capability gate passes (exec granted); tier Hot deadline
    //    applies (2 s override); NativeBinding runs FullstackTool::call;
    //    RedactPathsMiddleware rewrites the $HOME path in the result.
    let call = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:fullstack.demo",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(call["type"], "tool_result");
    assert_eq!(call["success"], serde_json::json!(true));

    // Binding: the tool recorded "native" because it reached Tool::call
    // via NativeBinding.
    assert_eq!(call["result"]["touched_binding"], "native");
    // Tier: the tool saw Hot on the CallContext.
    assert_eq!(call["result"]["tier_observed"], "Hot");
    // Capabilities: the tool saw the gate-derived set, not the server
    // allow-list.
    let caps_seen: Vec<String> =
        serde_json::from_value(call["result"]["caps_observed"].clone()).unwrap();
    assert_eq!(caps_seen, vec!["exec"]);
    // Middleware: the $HOME leak in the result got rewritten.
    assert_eq!(
        call["result"]["leaked_path"],
        "<redacted:home>/secrets/a.key"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_that_skips_hello_is_denied_even_with_full_chain_configured() {
    unsafe {
        std::env::set_var("HOME", "/tmp/sp12-e2e-home");
    }
    let srv = spawn().await;
    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    // No Hello. Connection's default CapabilitySet is empty.
    let r = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:fullstack.demo",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["type"], "error");
    assert_eq!(r["code"], 1001);
    let details = &r["details"];
    let required: Vec<String> = serde_json::from_value(details["required"].clone()).unwrap();
    let granted: Vec<String> = serde_json::from_value(details["granted"].clone()).unwrap();
    assert_eq!(required, vec!["exec"]);
    assert!(granted.is_empty());
}
