//! SP-medical-middleware §8.2 — end-to-end via `atd-server` UDS.
//!
//! Register a synthetic FHIR-returning tool, mount `FhirMiddleware`,
//! drive via `atd-sdk::AtdClient`, assert the validation annotation
//! reaches the client over real wire framing.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_middleware_fhir::FhirMiddleware;
use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::context::CallContext;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_sdk::{AtdClient, CallOptions, Endpoint};
use atd_server::{Server, ServerConfig};

/// Tool that returns whatever payload is supplied via env-var-style
/// keys in its args. Used to vary the FHIR shape per test case.
struct FhirReturner {
    def: ToolDefinition,
}
impl FhirReturner {
    fn new(id: &str) -> Self {
        Self {
            def: ToolDefinition {
                id: id.into(),
                name: "fhir-returner".into(),
                description: "returns the JSON in args.payload verbatim".into(),
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
impl Tool for FhirReturner {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            // Echo whatever was passed as `payload`.
            Ok(args
                .get("payload")
                .cloned()
                .unwrap_or(serde_json::Value::Null))
        })
    }
}

async fn spin_server(socket_path: PathBuf) -> tokio::task::JoinHandle<std::io::Result<()>> {
    let mut reg = Registry::new();
    reg.register(Arc::new(FhirReturner::new("hms:observation.get")));

    let cfg = ServerConfig {
        socket_path: socket_path.clone(),
        granted_capabilities: vec![],
        server_version: "fhir-mw-e2e 0.0.0".into(),
        ..ServerConfig::default()
    };
    let mut server = Server::new(reg, cfg);
    server.set_middleware(vec![Arc::new(FhirMiddleware::default())]);
    let handle = tokio::spawn(server.run());

    // Wait for socket bind.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !socket_path.exists() {
        if std::time::Instant::now() > deadline {
            panic!("server did not bind socket within 3s");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    handle
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loinc_payload_passes_without_annotation() {
    let sock = std::env::temp_dir().join(format!("atd-fhir-mw-good-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock);
    let handle = spin_server(sock.clone()).await;

    let client = AtdClient::connect(Endpoint::unix(sock.clone()))
        .await
        .expect("connect");
    let result = client
        .call(
            "hms:observation.get",
            serde_json::json!({"payload": {
                "resourceType": "Observation",
                "status": "final",
                "code": {"coding": [{
                    "system": "http://loinc.org",
                    "code": "15074-8"
                }]}
            }}),
            CallOptions::default(),
        )
        .await
        .expect("call");
    let data = result.data().expect("success has data");
    assert_eq!(data["resourceType"], "Observation");
    assert!(
        data.get("_fhir_validation_errors").is_none(),
        "LOINC URI must not trigger annotation; got {data}"
    );

    drop(client);
    handle.abort();
    let _ = std::fs::remove_file(&sock);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_system_triggers_annotation_on_wire() {
    let sock = std::env::temp_dir().join(format!("atd-fhir-mw-bad-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock);
    let handle = spin_server(sock.clone()).await;

    let client = AtdClient::connect(Endpoint::unix(sock.clone()))
        .await
        .expect("connect");
    let result = client
        .call(
            "hms:observation.get",
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
        .expect("call");
    let data = result.data().expect("success has data");
    let errs = data["_fhir_validation_errors"]
        .as_array()
        .expect("expected `_fhir_validation_errors` annotation on the wire");
    assert!(
        errs.iter()
            .any(|e| e.as_str().unwrap_or("").contains("celia.health")),
        "expected disallowed-coding-system finding mentioning celia.health, got {errs:?}"
    );

    drop(client);
    handle.abort();
    let _ = std::fs::remove_file(&sock);
}
