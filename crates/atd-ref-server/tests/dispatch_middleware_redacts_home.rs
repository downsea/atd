//! SP-12 Task 5 — result-middleware integration test.
//!
//! Registers a tool whose output contains an absolute `$HOME/...` path,
//! installs `RedactPathsMiddleware::with_home_default`, and verifies the
//! path is rewritten to `<redacted:home>/...` by the time the result
//! reaches the wire. Complements the unit tests in `middleware.rs` by
//! proving the server actually invokes the chain on the success path.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::middleware::RedactPathsMiddleware;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_server::{Server, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

struct EmitHomePathTool {
    def: ToolDefinition,
}

impl EmitHomePathTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "test:emit.home_path".into(),
                name: "emit home path".into(),
                description: "returns an absolute $HOME path for middleware tests".into(),
                version: "0.0.0".into(),
                capability: ToolCapability {
                    domain: "test".into(),
                    actions: vec![],
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
                required_capabilities: vec![],
                tier: None,
                errors: vec![],
            },
        }
    }
}

impl Tool for EmitHomePathTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(
        &'a self,
        _args: serde_json::Value,
        _ctx: &'a atd_runtime::context::CallContext,
    ) -> CallFuture<'a> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        Box::pin(async move {
            Ok(serde_json::json!({
                "leaked_path": format!("{}/secret/config.toml", home),
                "unrelated": "this string has no home in it",
                "nested": {
                    "also_leaked": format!("{}/another", home),
                },
            }))
        })
    }
}

struct ServerHandle {
    sock: PathBuf,
    _tempdir: tempfile::TempDir,
    _task: tokio::task::JoinHandle<std::io::Result<()>>,
}

async fn spawn_with_middleware(
    middleware: Vec<Arc<dyn atd_runtime::middleware::Middleware>>,
) -> ServerHandle {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("server.sock");

    let mut registry = Registry::new();
    registry.register(Arc::new(EmitHomePathTool::new()));

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap(),
        max_output_bytes: 1_048_576,
        default_call_timeout_ms: 5_000,
        granted_capabilities: vec![],
        audit_sink: None,
        server_version: concat!("atd-ref-server ", env!("CARGO_PKG_VERSION")).to_string(),
    };
    let mut server = Server::new(registry, cfg);
    server.set_middleware(middleware);
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
async fn middleware_redacts_home_path_on_wire() {
    // Pin HOME so the middleware's pattern matches a known value
    // deterministically across CI/dev environments.
    unsafe {
        std::env::set_var("HOME", "/tmp/sp12-test-home");
    }

    let mw: Arc<dyn atd_runtime::middleware::Middleware> =
        Arc::new(RedactPathsMiddleware::with_home_default());
    let srv = spawn_with_middleware(vec![mw]).await;

    let r = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:emit.home_path",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(
        r["result"]["leaked_path"],
        "<redacted:home>/secret/config.toml"
    );
    assert_eq!(r["result"]["unrelated"], "this string has no home in it");
    // Nested fields also get redacted.
    assert_eq!(
        r["result"]["nested"]["also_leaked"],
        "<redacted:home>/another"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_middleware_leaves_result_untouched() {
    // Same setup, empty middleware chain → raw path comes through.
    unsafe {
        std::env::set_var("HOME", "/tmp/sp12-test-home");
    }
    let srv = spawn_with_middleware(vec![]).await;

    let r = send_one(
        &srv.sock,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "test:emit.home_path",
            "args": {},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(
        r["result"]["leaked_path"],
        "/tmp/sp12-test-home/secret/config.toml"
    );
}
