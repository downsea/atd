//! SP-medical-middleware §8.2 — combined FHIR + PII chain via real UDS.
//!
//! Mounts both middleware on `atd-server::Server`, calls a synthetic
//! Celia-shaped Patient tool, asserts:
//! 1. FHIR validation passes (LOINC coding, no `_fhir_validation_errors`)
//! 2. PHI redacted (name tokenised, birthDate year-only, SSN in note
//!    caught by regex)

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_middleware_fhir::FhirMiddleware;
use atd_middleware_pii_redact_medical::PiiRedactMiddleware;
use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::context::CallContext;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_sdk::{AtdClient, CallOptions, Endpoint};
use atd_server::{Server, ServerConfig};

struct PatientReturner {
    def: ToolDefinition,
}

impl PatientReturner {
    fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "test:patient.get".into(),
                name: "patient.get".into(),
                description: "returns a synthetic Patient payload".into(),
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

impl Tool for PatientReturner {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            // Celia-shaped Patient with valid LOINC coding (for FHIR
            // validator to pass) and PHI in every HIPAA category we
            // claim to cover.
            Ok(serde_json::json!({
                "resourceType": "Patient",
                "id": "abc-123",
                "name": [{"family": "Smith", "given": ["John"]}],
                "birthDate": "1955-03-15",
                "address": [{
                    "line": ["1 Main St"],
                    "city": "Palo Alto",
                    "postalCode": "94303"
                }],
                "telecom": [{"system": "phone", "value": "555-1212"}],
                "photo": [{"contentType": "image/png", "data": "AAAA"}],
                "identifier": [{
                    "system": "http://hl7.org/fhir/sid/us-mbi",
                    "value": "MBI-XYZ"
                }],
                "note": [{"text": "Contact 555-12-3456 for follow-up"}]
            }))
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn combined_chain_validates_fhir_and_redacts_phi() {
    let sock: PathBuf =
        std::env::temp_dir().join(format!("atd-med-combined-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock);

    let mut reg = Registry::new();
    reg.register(Arc::new(PatientReturner::new()));
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        server_version: "med-mw-combined 0.0.0".into(),
        ..ServerConfig::default()
    };
    let mut server = Server::new(reg, cfg);
    // Order matters: FHIR validates first, PII redacts second
    // (spec §5.2). Reversing would let PII run on rejected payloads.
    server.set_middleware(vec![
        Arc::new(FhirMiddleware::default()),
        Arc::new(PiiRedactMiddleware::default()),
    ]);
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
    let result = client
        .call(
            "test:patient.get",
            serde_json::json!({}),
            CallOptions::default(),
        )
        .await
        .expect("call");
    let data = result.data().expect("success has data");

    // --- FHIR side: no validation errors (us-mbi is whitelisted) ---
    assert!(
        data.get("_fhir_validation_errors").is_none(),
        "expected no FHIR errors, got: {data}"
    );

    // --- PII side: each HIPAA category we cover is redacted ---
    assert_eq!(data["name"], "[REDACTED:NAME]");
    assert_eq!(data["birthDate"], "1955");
    assert_eq!(data["telecom"], "[REDACTED:PHONE]");
    assert_eq!(data["photo"], serde_json::Value::Null);
    assert_eq!(data["identifier"], "[REDACTED:ID]");
    assert_eq!(data["address"][0]["postalCode"], "943");
    assert_eq!(data["address"][0]["line"], serde_json::Value::Null);
    // City preserved (HIPAA Safe Harbor §164.514(b)(2)(i)(B) permits >state).
    assert_eq!(data["address"][0]["city"], "Palo Alto");
    // SSN in note caught by regex.
    let note_text = data["note"][0]["text"].as_str().unwrap();
    assert!(
        note_text.contains("[REDACTED:SSN]"),
        "expected SSN regex match in note, got: {note_text}"
    );
    assert!(!note_text.contains("555-12-3456"));

    drop(client);
    handle.abort();
    let _ = std::fs::remove_file(&sock);
}
