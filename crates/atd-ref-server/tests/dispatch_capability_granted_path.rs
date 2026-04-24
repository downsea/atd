//! SP-12 Task 2 — capability-granted integration test.
//!
//! Complement to `dispatch_capability_denied_path.rs`. Here the server grants
//! `exec`; the client requests it in `Hello`; the subsequent `run_tool` is
//! allowed through the gate and the tool's result is returned. Together, the
//! two tests pin both branches of the `CapabilityDenied` error path.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_ref_server::server::{Server, ServerConfig};
use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

struct GatedTool {
    def: ToolDefinition,
}

impl GatedTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "test:gated.op".into(),
                name: "gated op".into(),
                description: "requires exec capability".into(),
                version: "0.0.0".into(),
                capability: ToolCapability {
                    domain: "test".into(),
                    actions: vec!["op".into()],
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
                tier: None,
            },
        }
    }
}

impl Tool for GatedTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(
        &'a self,
        _args: serde_json::Value,
        ctx: &'a atd_runtime::context::CallContext,
    ) -> CallFuture<'a> {
        // Echo a marker + the capabilities the tool saw, so the test can
        // confirm caps flow through the CallContext.
        let caps_view = ctx.capabilities.granted();
        Box::pin(async move {
            Ok(serde_json::json!({
                "ran": true,
                "caps_in_context": caps_view,
            }))
        })
    }
}

struct ServerHandle {
    sock: PathBuf,
    _tempdir: tempfile::TempDir,
    _task: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn spawn(granted: Vec<String>) -> ServerHandle {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("server.sock");

    let mut registry = Registry::new();
    registry.register(Arc::new(GatedTool::new()));

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap(),
        max_output_bytes: 1_048_576,
        default_call_timeout_ms: 5_000,
        granted_capabilities: granted,
    };

    let server = Server::new(registry, cfg);
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

async fn send_on_stream(
    stream: &mut UnixStream,
    req: serde_json::Value,
) -> serde_json::Value {
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
async fn hello_grants_requested_subset_when_server_allows() {
    let srv = spawn(vec!["exec".into(), "read".into()]).await;
    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    let hello = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "hello",
            "client_id": "integration-test",
            "requested_capabilities": ["exec", "write"],
        }),
    )
    .await;
    assert_eq!(hello["type"], "hello_ack");
    let granted: Vec<String> =
        serde_json::from_value(hello["granted_capabilities"].clone()).unwrap();
    assert_eq!(granted, vec!["exec"]); // "write" not in server's allow-list
    assert!(
        hello["server_version"]
            .as_str()
            .unwrap()
            .starts_with("atd-ref-server ")
    );
    let tiers: Vec<String> = serde_json::from_value(hello["supported_tiers"].clone()).unwrap();
    assert_eq!(tiers, vec!["hot", "warm", "cold"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_tool_allowed_after_hello_grants_required_cap() {
    let srv = spawn(vec!["exec".into()]).await;
    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    let _hello = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "hello",
            "requested_capabilities": ["exec"],
        }),
    )
    .await;

    let call = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:gated.op",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(call["type"], "tool_result");
    assert_eq!(call["success"], serde_json::json!(true));
    assert_eq!(call["result"]["ran"], serde_json::json!(true));
    // CallContext.capabilities must mirror the connection's Hello-granted set.
    let caps: Vec<String> =
        serde_json::from_value(call["result"]["caps_in_context"].clone()).unwrap();
    assert_eq!(caps, vec!["exec"]);
}

// Silence unused warning from shared test helpers.
#[allow(dead_code)]
fn _unused(_: ToolCallError) {}
