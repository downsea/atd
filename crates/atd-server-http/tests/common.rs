//! Shared test fixtures for the atd-server-http integration suite.
//!
//! `EchoStub` is a minimal `Tool` impl that returns `{"echoed": args}` —
//! the same shape `atd-server::connection::tests::EchoStub` uses, so the
//! parity test can drive UDS and HTTP with identical results. Helpers
//! spin up a `Server` on `127.0.0.1:0` and return the bound address +
//! the task handle so tests can shut down cleanly.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::context::CallContext;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_runtime::secrets::{
    BearerIdentity, BrokerError, ResolveBearerFuture, ResolveFuture, TokenBroker,
};
use atd_server_http::{HttpServerConfig, Server};
use tokio::task::JoinHandle;

pub fn stub_def(id: &str) -> ToolDefinition {
    ToolDefinition {
        id: id.into(),
        name: id.into(),
        description: "stub".into(),
        version: "0.0.0".into(),
        capability: ToolCapability {
            domain: "echo".into(),
            actions: vec![],
            tags: vec![],
            intent_examples: vec![],
        },
        input_schema: serde_json::json!({"type":"object"}),
        output_schema: serde_json::json!({"type":"object"}),
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
    }
}

pub struct EchoStub {
    def: ToolDefinition,
}

impl EchoStub {
    pub fn new() -> Self {
        Self {
            def: stub_def("ref:echo.say"),
        }
    }
}

impl Tool for EchoStub {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move { Ok(serde_json::json!({"echoed": args})) })
    }
}

pub fn echo_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoStub::new()));
    reg
}

pub struct RunningServer {
    pub addr: SocketAddr,
    pub handle: JoinHandle<()>,
}

/// Spin up an HTTP server with the supplied registry on a kernel-chosen
/// port and return the bound address + the join handle of the serve
/// task. Caller is responsible for `.abort()`ing the handle at teardown.
pub async fn spawn_server(registry: Registry, cfg: HttpServerConfig) -> RunningServer {
    let (router, mut server) = Server::builder(registry).config(cfg).build();
    let listener = server.bind().await.expect("bind");
    let addr = server.local_addr().expect("local_addr after bind");
    let handle = tokio::spawn(async move {
        let _ = server.serve_with_listener(listener, router).await;
    });
    // Allow the serve future to register before tests dial in.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    RunningServer { addr, handle }
}

/// Broker that returns a known identity for an exact token, plus an
/// option to mark the token expired / revoked / lookup-failed for
/// negative tests. Mirrors the `FixedBroker` pattern in
/// `src/bearer.rs::tests` so the suite has one fixture for both.
pub struct FixedBroker {
    pub good_token: String,
    pub identity: BearerIdentity,
}

impl FixedBroker {
    pub fn new(token: &str, identity: BearerIdentity) -> Self {
        Self {
            good_token: token.to_string(),
            identity,
        }
    }
}

impl TokenBroker for FixedBroker {
    fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async { Ok(None) })
    }
    fn resolve_bearer<'a>(&'a self, bearer: &'a str) -> ResolveBearerFuture<'a> {
        let good = self.good_token.clone();
        let identity = self.identity.clone();
        let bearer = bearer.to_string();
        Box::pin(async move {
            if bearer == good {
                Ok(Some(identity))
            } else {
                Err(BrokerError::Lookup("unknown token".into()))
            }
        })
    }
}
