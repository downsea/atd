//! Phase L.0 — baseline-verification integration test.
//!
//! Closes the verification section of atd-mvp#6 (the cross-repo
//! L.0 ask from celia PHASE_L_PLAN.md §6 L.0). Five protocol
//! primitives must compose end-to-end through `atd-server` UDS +
//! `atd-sdk`:
//!
//! - **AC1** — `AtdClient::call_all` follows cursors transparently.
//!   Covered by `paginated_dispatch::call_all_walks_all_pages_via_concat_array`
//!   in this same crate's `tests/`. Not re-tested here.
//! - **AC2** — `CursorIssuer` HMAC-fingerprints the args, so a
//!   continuation with tampered args is rejected. Covered by
//!   `paginated_dispatch::cross_tool_cursor_returns_invalid` plus
//!   the lib-level fingerprint stability tests in
//!   `crates/atd-runtime/src/cursor.rs`. Not re-tested here.
//! - **AC3** — `TokenBroker` routes per `BearerIdentity` (here, per
//!   `Hello.client_id`-derived `caller_id`). Two clients connecting
//!   to one server with different client ids see their own secrets
//!   only. **Exercises the new `FileTokenBroker` over the wire.**
//! - **AC4** — `FhirMiddleware` with `MismatchPolicy::ReplaceWithError`
//!   surfaces a structured error envelope as the result, not an
//!   annotation. Adopter-side fail-closed for celia's I8 invariant.
//! - **AC5** — `CapabilitySet` negotiation: client's
//!   `Hello.requested_capabilities` is intersected with the
//!   operator-allowlist, and only the intersection is granted —
//!   strict subset of what the server offers.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use atd_middleware_fhir::{FhirMiddleware, FhirMiddlewareConfig, MismatchPolicy};
use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::context::CallContext;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_runtime::{FileTokenBroker, FileTokenRecord};
use atd_sdk::{AtdClient, CallOptions, ConnectOptions, Endpoint};
use atd_server::{Server, ServerConfig};

// -------- shared scaffolding --------

async fn wait_for_sock(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !path.exists() {
        if Instant::now() > deadline {
            panic!("server did not bind socket within 3s");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn fast_connect() -> ConnectOptions {
    ConnectOptions {
        max_attempts: 3,
        backoff_base_ms: 5,
        backoff_cap_ms: 20,
        connect_timeout_ms: 2000,
    }
}

fn baseline_def(id: &str, caps: Vec<String>) -> ToolDefinition {
    ToolDefinition {
        id: id.into(),
        name: id.into(),
        description: format!("phase-l-baseline test tool {id}"),
        version: "0.0.0".into(),
        capability: ToolCapability {
            domain: "phase-l".into(),
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
            max_concurrent: 4,
            rate_limit_per_min: None,
            estimated_tokens: None,
        },
        trust: ToolTrust {
            publisher: "atd-phase-l-baseline".into(),
            trust_level: TrustLevel::L0Unverified,
            signature: None,
        },
        visibility: ToolVisibility::Read,
        required_capabilities: caps,
        tier: None,
        errors: vec![],
    }
}

// -------- AC3: multi-tenant TokenBroker routing via FileTokenBroker --------

/// Echoes the caller's access_token secret value verbatim in the
/// result so the test can assert per-caller routing.
struct SecretEchoer {
    def: ToolDefinition,
}
impl Tool for SecretEchoer {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, _args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
        let echo = match ctx.secrets() {
            Some(bundle) => bundle
                .get("access_token")
                .map(|r| r.expose().to_string())
                .unwrap_or_else(|| "<no-access-token>".into()),
            None => "<no-bundle>".into(),
        };
        let caller = ctx.caller_id.clone().unwrap_or_else(|| "<anon>".into());
        Box::pin(async move {
            Ok(serde_json::json!({
                "caller_id": caller,
                "access_token_echo": echo,
            }))
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac3_file_broker_routes_per_caller_e2e() {
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("ac3.sock");

    let tokens = tempfile::tempdir().unwrap();
    let broker = FileTokenBroker::new(tokens.path());
    let later = SystemTime::now() + Duration::from_secs(3600);
    broker
        .put(
            "agent-A",
            FileTokenRecord::from_expires_at("tok-A", "ref-A", later),
        )
        .await
        .unwrap();
    broker
        .put(
            "agent-B",
            FileTokenRecord::from_expires_at("tok-B", "ref-B", later),
        )
        .await
        .unwrap();
    let broker: Arc<dyn atd_runtime::TokenBroker> = Arc::new(broker);

    let mut reg = Registry::new();
    reg.register(Arc::new(SecretEchoer {
        def: baseline_def("phase-l:echo-secret", vec![]),
    }));

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        token_broker: Some(broker),
        ..ServerConfig::default()
    };
    let server = Server::new(reg, cfg);
    let task = tokio::spawn(server.run());
    wait_for_sock(&sock).await;

    let client_a = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();
    client_a.hello(Some("agent-A"), vec![]).await.unwrap();
    let result_a = client_a
        .call(
            "phase-l:echo-secret",
            serde_json::json!({}),
            CallOptions::default(),
        )
        .await
        .unwrap();
    let data_a = result_a.data().expect("success");
    assert_eq!(data_a["caller_id"], "agent-A");
    assert_eq!(data_a["access_token_echo"], "tok-A");

    let client_b = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();
    client_b.hello(Some("agent-B"), vec![]).await.unwrap();
    let result_b = client_b
        .call(
            "phase-l:echo-secret",
            serde_json::json!({}),
            CallOptions::default(),
        )
        .await
        .unwrap();
    let data_b = result_b.data().expect("success");
    assert_eq!(data_b["caller_id"], "agent-B");
    assert_eq!(data_b["access_token_echo"], "tok-B");

    drop(client_a);
    drop(client_b);
    task.abort();
}

// -------- AC4: FhirMiddleware ReplaceWithError over the wire --------

/// Returns whatever JSON is passed in `args.payload`. Used to stage
/// FHIR-shaped results with disallowed coding systems.
struct PayloadEcho {
    def: ToolDefinition,
}
impl Tool for PayloadEcho {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            Ok(args
                .get("payload")
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac4_fhir_replace_with_error_e2e() {
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("ac4.sock");

    let mut reg = Registry::new();
    reg.register(Arc::new(PayloadEcho {
        def: baseline_def("phase-l:payload-echo", vec![]),
    }));

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        ..ServerConfig::default()
    };
    // FhirMiddlewareConfig is #[non_exhaustive] — literal syntax is
    // forbidden outside its crate, so post-construction assignment is
    // the only path. clippy's field_reassign_with_default doesn't fire
    // on non_exhaustive types for exactly this reason.
    let mut mw_cfg = FhirMiddlewareConfig::default();
    mw_cfg.on_mismatch = MismatchPolicy::ReplaceWithError;
    let mut server = Server::new(reg, cfg);
    server.set_middleware(vec![Arc::new(FhirMiddleware::new(mw_cfg))]);
    let task = tokio::spawn(server.run());
    wait_for_sock(&sock).await;

    let client = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();
    let result = client
        .call(
            "phase-l:payload-echo",
            serde_json::json!({"payload": {
                "resourceType": "Observation",
                "status": "final",
                "code": {"coding": [{
                    "system": "https://celia.health/fhir/codes",
                    "code": "x"
                }]}
            }}),
            CallOptions::default(),
        )
        .await
        .unwrap();
    let data = result
        .data()
        .expect("middleware-rewritten result still carries data");
    assert_eq!(
        data["error"], "fhir_validation_failed",
        "ReplaceWithError must rewrite the payload, not annotate it; got {data}"
    );
    assert!(
        data["details"].is_array(),
        "ReplaceWithError must include the offending findings; got {data}"
    );
    assert!(
        data.get("resourceType").is_none(),
        "ReplaceWithError must NOT retain the original FHIR shape; got {data}"
    );

    drop(client);
    task.abort();
}

// -------- AC5: CapabilitySet subset negotiation at Hello time --------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac5_capability_set_negotiates_strict_subset() {
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("ac5.sock");

    let reg = Registry::new();
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        // Server offers a superset.
        granted_capabilities: vec![
            "healthkit:read".into(),
            "healthkit:write".into(),
            "fhir:export".into(),
        ],
        ..ServerConfig::default()
    };
    let server = Server::new(reg, cfg);
    let task = tokio::spawn(server.run());
    wait_for_sock(&sock).await;

    let client = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();
    // Client asks for read-only + one capability the server does NOT offer.
    // The server-side intersection must drop `not-offered` and keep only
    // the read-only subset.
    let granted = client
        .hello(
            Some("client-1"),
            vec!["healthkit:read".into(), "not-offered:capability".into()],
        )
        .await
        .unwrap();

    let granted_set: std::collections::BTreeSet<_> = granted.iter().cloned().collect();
    assert!(
        granted_set.contains("healthkit:read"),
        "requested+offered capability must be granted; got {granted:?}"
    );
    assert!(
        !granted_set.contains("not-offered:capability"),
        "capability not offered by server must NOT be granted; got {granted:?}"
    );
    assert!(
        !granted_set.contains("healthkit:write"),
        "capability offered but NOT requested must NOT be granted; got {granted:?}"
    );
    assert!(
        !granted_set.contains("fhir:export"),
        "capability offered but NOT requested must NOT be granted; got {granted:?}"
    );

    drop(client);
    task.abort();
}
