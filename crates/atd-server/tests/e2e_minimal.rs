//! End-to-end smoke test: spin up a `Server` with a one-tool `Registry`,
//! drive it via `atd-sdk`, assert discover + call round-trip.
//!
//! This is the minimal proof that the listener layer (atd-server) can be
//! used standalone — without atd-ref-server's built-in tools — by any
//! third-party server. It's the test a vendor wrapping their own service
//! into ATD would write first.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_sdk::{AtdClient, CallOptions, DiscoverFilter, Endpoint};
use atd_server::{Server, ServerConfig};

struct GreetTool {
    def: ToolDefinition,
}
impl GreetTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "demo:greet.say".into(),
                name: "greet".into(),
                description: "returns {greeted: name}".into(),
                version: "0.0.0".into(),
                capability: ToolCapability {
                    domain: "demo".into(),
                    actions: vec!["say".into()],
                    tags: vec![],
                    intent_examples: vec![],
                },
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {"name": {"type": "string"}},
                    "required": ["name"]
                }),
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
                    publisher: "demo".into(),
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
impl Tool for GreetTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        _ctx: &'a atd_runtime::CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move {
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("world");
            Ok(serde_json::json!({"greeted": name}))
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_with_one_tool_round_trips_through_sdk() {
    // Bind the server on a temp socket.
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("e2e.sock");

    let mut reg = Registry::new();
    reg.register(Arc::new(GreetTool::new()));

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        ..ServerConfig::default()
    };
    let server = Server::new(reg, cfg);
    let task = tokio::spawn(server.run());

    // Wait for the socket to appear.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !sock.exists() {
        if std::time::Instant::now() > deadline {
            panic!("server did not bind socket within 3s");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Drive via atd-sdk.
    let client = AtdClient::connect(Endpoint::unix(sock.clone()))
        .await
        .expect("connect");
    client.ping().await.expect("ping");

    let tools = client
        .discover(None, DiscoverFilter::default())
        .await
        .expect("discover");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].id, "demo:greet.say");

    let result = client
        .call(
            "demo:greet.say",
            serde_json::json!({"name": "atd"}),
            CallOptions::default(),
        )
        .await
        .expect("call");
    assert!(result.is_success(), "expected success, got {result:?}");
    assert_eq!(result.data().expect("success has data")["greeted"], "atd");

    // Drop the client (closes the connection); abort the server task.
    drop(client);
    task.abort();
}
