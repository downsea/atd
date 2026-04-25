//! SP-12 Task 2 — capability-denied integration test.
//!
//! Spawns an in-process `atd-ref-server` whose registry contains a single
//! test tool declaring `required_capabilities: ["exec"]`. The server's
//! `--grant-capability` allow-list is empty. A client connects, skips the
//! `Hello` handshake (so its capability set is empty by default), and calls
//! the gated tool. Expect `type = error`, `code = ERR_CAPABILITY_DENIED`
//! (1001), and `details` carrying both the required and granted sets so the
//! client can render a useful error.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_server::{Server, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// A test-only tool that, if actually invoked, returns a sentinel payload.
/// The point of the test is that dispatch short-circuits **before** we get
/// here — so `call` should never run.
struct GatedTool {
    def: ToolDefinition,
}

impl GatedTool {
    fn new() -> Self {
        let def = ToolDefinition {
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
            errors: vec![],
        };
        Self { def }
    }
}

impl Tool for GatedTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(
        &'a self,
        _args: serde_json::Value,
        _ctx: &'a atd_runtime::context::CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async {
            Err(ToolCallError::InternalError(
                "GatedTool::call must not run when capability gate denies".into(),
            ))
        })
    }
}

struct ServerHandle {
    sock: PathBuf,
    _tempdir: tempfile::TempDir,
    _task: tokio::task::JoinHandle<std::io::Result<()>>,
}

/// Spawn a server in-process with the given granted_capabilities allow-list
/// and the single GatedTool in its registry.
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
        audit_sink: None,
        server_version: concat!("atd-ref-server ", env!("CARGO_PKG_VERSION")).to_string(),
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
async fn run_tool_denied_when_no_hello_and_required_cap_missing() {
    let srv = spawn(vec![]).await;

    let r = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:gated.op",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;

    assert_eq!(r["type"], "error");
    assert_eq!(r["code"], 1001);
    assert_eq!(r["retryable"], serde_json::json!(false));
    assert!(
        r["message"].as_str().unwrap().contains("capability denied"),
        "message should mention capability denial: {}",
        r["message"]
    );
    let details = &r["details"];
    let required: Vec<String> = serde_json::from_value(details["required"].clone()).unwrap();
    let granted: Vec<String> = serde_json::from_value(details["granted"].clone()).unwrap();
    let missing: Vec<String> = serde_json::from_value(details["missing"].clone()).unwrap();
    assert_eq!(required, vec!["exec"]);
    assert!(granted.is_empty());
    assert_eq!(missing, vec!["exec"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_tool_denied_when_hello_asks_for_cap_not_granted_by_server() {
    // Server allow-list does NOT include "exec". Even if the client requests
    // it in Hello, the Hello intersect rejects it and the subsequent run_tool
    // still fails with CAPABILITY_DENIED.
    let srv = spawn(vec!["read".into()]).await;

    // Send Hello + run_tool on the SAME connection so the server's
    // per-connection capability state survives between requests.
    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    async fn one(stream: &mut UnixStream, req: serde_json::Value) -> serde_json::Value {
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

    let hello = one(
        &mut stream,
        serde_json::json!({
            "type": "hello",
            "requested_capabilities": ["exec"],
        }),
    )
    .await;
    assert_eq!(hello["type"], "hello_ack");
    let granted_by_hello: Vec<String> =
        serde_json::from_value(hello["granted_capabilities"].clone()).unwrap();
    assert!(
        granted_by_hello.is_empty(),
        "server allow-list excludes 'exec', so Hello must grant nothing"
    );

    let call = one(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:gated.op",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(call["type"], "error");
    assert_eq!(call["code"], 1001);
}
