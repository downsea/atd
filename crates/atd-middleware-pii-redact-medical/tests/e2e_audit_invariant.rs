//! SP-medical-middleware §4.7 + §8.2 — `AuditSink` PHI-leak guard.
//!
//! The spec promises (§4.7): `CallEvent` carries no result body today,
//! so PHI never reaches the audit sink even before the redaction
//! middleware runs. This test is the regression guard against future
//! drift — if someone enriches `CallEvent` with a `result_preview`
//! field, this test must fail BEFORE the change ships so the SP-medical
//! authors can decide whether to add an audit-side hook.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use atd_middleware_pii_redact_medical::PiiRedactMiddleware;
use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::audit::{AuditSink, CallEvent};
use atd_runtime::context::CallContext;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_sdk::{AtdClient, CallOptions, Endpoint};
use atd_server::{Server, ServerConfig};

const FORBIDDEN_STRINGS: &[&str] = &["John Smith", "555-12-3456", "94303-1234"];

/// AuditSink that captures every event's JSON-serialised form into a
/// `Vec<String>` so the test can assert on the body afterwards.
#[derive(Debug, Default)]
struct CapturingSink {
    captured: Arc<Mutex<Vec<String>>>,
}

impl AuditSink for CapturingSink {
    fn on_call(&self, event: &CallEvent) {
        let s = serde_json::to_string(event).expect("CallEvent serializes");
        self.captured.lock().unwrap().push(s);
    }
}

struct LeakyPatientTool {
    def: ToolDefinition,
}

impl LeakyPatientTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "test:patient.get".into(),
                name: "patient.get".into(),
                description: "returns PHI-rich Patient".into(),
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

impl Tool for LeakyPatientTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            // PHI in every conceivable place. If `CallEvent` ever
            // carries a result snippet, the strings below WILL appear
            // in captured audit events — and this test catches it.
            Ok(serde_json::json!({
                "resourceType": "Patient",
                "id": "abc",
                "name": [{"family": "Smith", "given": ["John"]}],
                "note": [{"text": "Contact John Smith at 555-12-3456, zip 94303-1234"}]
            }))
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_events_never_contain_phi_strings() {
    let sock: PathBuf = std::env::temp_dir().join(format!(
        "atd-med-audit-invariant-{}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&sock);

    let sink = CapturingSink::default();
    let captured_handle = sink.captured.clone();

    let mut reg = Registry::new();
    reg.register(Arc::new(LeakyPatientTool::new()));
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        audit_sink: Some(Arc::new(sink)),
        server_version: "med-mw-audit 0.0.0".into(),
        ..ServerConfig::default()
    };
    let mut server = Server::new(reg, cfg);
    server.set_middleware(vec![Arc::new(PiiRedactMiddleware::default())]);
    let handle = tokio::spawn(server.run());

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !sock.exists() {
        if std::time::Instant::now() > deadline {
            panic!("server did not bind");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let client = AtdClient::connect(Endpoint::unix(sock.clone()))
        .await
        .expect("connect");
    let _ = client
        .call(
            "test:patient.get",
            serde_json::json!({}),
            CallOptions::default(),
        )
        .await
        .expect("call");
    // Give the audit task a moment to flush.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let captured = captured_handle.lock().unwrap().clone();
    assert!(
        !captured.is_empty(),
        "expected at least one CallEvent recorded"
    );
    for event_json in &captured {
        for forbidden in FORBIDDEN_STRINGS {
            assert!(
                !event_json.contains(forbidden),
                "PHI string `{forbidden}` leaked into audit event: {event_json}\n\
                 This means the CallEvent schema has changed to include result \
                 body data. SP-medical-middleware §4.7 explicitly designed for the \
                 \"audit carries metadata only\" invariant. Either:\n\
                 (a) revert the CallEvent schema change, or\n\
                 (b) file a follow-up SP adding an audit-side middleware hook \
                     that runs redact_value on the audit payload."
            );
        }
    }

    drop(client);
    handle.abort();
    let _ = std::fs::remove_file(&sock);
}
