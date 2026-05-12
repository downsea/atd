//! SP-medical-middleware §8.4 — non-medical tools unaffected by the
//! medical chain.
//!
//! Mounts both `FhirMiddleware` + `PiiRedactMiddleware` on a server
//! that hosts a mix of: 1 echo tool (non-FHIR) and 1 FHIR-returning
//! tool. Asserts:
//! - Echo tool's response is bit-identical with vs without the chain
//!   (no `_fhir_validation_errors`, no `_phi_findings`, no Token
//!   replacements).
//! - FHIR tool's response gets PHI redacted.
//!
//! Regression guard: a future change to the FHIR or PII walker that
//! accidentally over-triggers on non-medical results breaks this test.

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

fn def(id: &str) -> ToolDefinition {
    ToolDefinition {
        id: id.into(),
        name: id.into(),
        description: "stub".into(),
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
    }
}

struct EchoTool {
    def: ToolDefinition,
}
impl EchoTool {
    fn new() -> Self {
        Self {
            def: def("ref:echo.say"),
        }
    }
}
impl Tool for EchoTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move { Ok(serde_json::json!({"echoed": args, "count": 7})) })
    }
}

struct PatientTool {
    def: ToolDefinition,
}
impl PatientTool {
    fn new() -> Self {
        Self {
            def: def("hms:patient.get"),
        }
    }
}
impl Tool for PatientTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            Ok(serde_json::json!({
                "resourceType": "Patient",
                "id": "abc",
                "name": [{"family": "Smith"}],
                "birthDate": "1955-03-15"
            }))
        })
    }
}

async fn spin_server(
    sock: PathBuf,
    install_middleware: bool,
) -> tokio::task::JoinHandle<std::io::Result<()>> {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoTool::new()));
    reg.register(Arc::new(PatientTool::new()));
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        server_version: "med-mw-cross 0.0.0".into(),
        ..ServerConfig::default()
    };
    let mut server = Server::new(reg, cfg);
    if install_middleware {
        server.set_middleware(vec![
            Arc::new(FhirMiddleware::default()),
            Arc::new(PiiRedactMiddleware::default()),
        ]);
    }
    let h = tokio::spawn(server.run());
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !sock.exists() {
        if std::time::Instant::now() > deadline {
            panic!("server did not bind");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    h
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_medical_tool_unaffected_by_chain() {
    // First call: no middleware, capture baseline.
    let sock_a = std::env::temp_dir().join(format!("atd-med-cross-a-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock_a);
    let h_a = spin_server(sock_a.clone(), /*install_middleware=*/ false).await;
    let c_a = AtdClient::connect(Endpoint::unix(sock_a.clone()))
        .await
        .expect("connect-a");
    let baseline = c_a
        .call(
            "ref:echo.say",
            serde_json::json!({"hi": "x"}),
            CallOptions::default(),
        )
        .await
        .expect("call-a");
    let baseline_data = baseline.data().expect("baseline data").clone();
    drop(c_a);
    h_a.abort();
    let _ = std::fs::remove_file(&sock_a);

    // Second call: middleware installed, same input.
    let sock_b = std::env::temp_dir().join(format!("atd-med-cross-b-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock_b);
    let h_b = spin_server(sock_b.clone(), /*install_middleware=*/ true).await;
    let c_b = AtdClient::connect(Endpoint::unix(sock_b.clone()))
        .await
        .expect("connect-b");
    let with_mw = c_b
        .call(
            "ref:echo.say",
            serde_json::json!({"hi": "x"}),
            CallOptions::default(),
        )
        .await
        .expect("call-b");
    let with_mw_data = with_mw.data().expect("with-mw data").clone();
    drop(c_b);
    h_b.abort();
    let _ = std::fs::remove_file(&sock_b);

    // Non-FHIR result must be bit-identical between the two runs.
    assert_eq!(
        baseline_data, with_mw_data,
        "non-medical tool result diverged between baseline and middleware-installed runs:\
         \nbaseline: {baseline_data}\nwith-mw : {with_mw_data}"
    );
    // No annotations of any kind.
    assert!(with_mw_data.get("_fhir_validation_errors").is_none());
    assert!(with_mw_data.get("_phi_findings").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn medical_tool_in_same_registry_still_redacts_phi() {
    // Same server has BOTH tools registered. The medical chain runs on
    // every successful tool — verify it still catches PHI on the
    // Patient tool even though the registry also contains a non-medical
    // tool.
    let sock = std::env::temp_dir().join(format!("atd-med-cross-mix-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock);
    let h = spin_server(sock.clone(), /*install_middleware=*/ true).await;
    let c = AtdClient::connect(Endpoint::unix(sock.clone()))
        .await
        .expect("connect");

    let result = c
        .call(
            "hms:patient.get",
            serde_json::json!({}),
            CallOptions::default(),
        )
        .await
        .expect("call");
    let data = result.data().expect("data");
    assert_eq!(data["name"], "[REDACTED:NAME]");
    assert_eq!(data["birthDate"], "1955");

    drop(c);
    h.abort();
    let _ = std::fs::remove_file(&sock);
}
