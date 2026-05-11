//! SP-capability-v2 Phase F — UDS end-to-end UCAN handshake.
//!
//! Spec §8.2 first entry:
//! > tests/ucan_hello_grants_union.rs — UDS server with
//! > `--grant-capability records:read`; client sends `ucan_tokens`
//! > proving `summary:read`; granted = `{records:read, summary:read}`.
//!
//! End-to-end means: real Unix-socket Server, real AtdClient, real wire
//! framing. The inner verifier path is covered by Phase B + Phase C unit
//! tests; this test guarantees the full transport stack agrees.

use std::sync::Arc;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_sdk::{AtdClient, Endpoint};
use atd_server::{Server, ServerConfig};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

// ---- minimal tool fixture (mirrors e2e_minimal.rs) ----

struct NoopTool {
    def: ToolDefinition,
}
impl NoopTool {
    fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "demo:noop".into(),
                name: "noop".into(),
                description: "always ok".into(),
                version: "0.0.0".into(),
                capability: ToolCapability {
                    domain: "demo".into(),
                    actions: vec![],
                    tags: vec![],
                    intent_examples: vec![],
                },
                input_schema: json!({"type": "object"}),
                output_schema: json!({"type": "object"}),
                bindings: vec![ToolBinding {
                    protocol: BindingProtocol::Cli,
                    config: json!({}),
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
impl Tool for NoopTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(
        &'a self,
        _args: serde_json::Value,
        _ctx: &'a atd_runtime::context::CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move { Ok(json!({"ok": true})) })
    }
}

// ---- UCAN helpers (inline; shared in spirit with the unit-test helpers) ----

fn signing_key_for_seed(seed: u8) -> SigningKey {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    SigningKey::from_bytes(&bytes)
}

fn did_key_for(sk: &SigningKey) -> String {
    let raw = sk.verifying_key().to_bytes();
    let mut prefixed = Vec::with_capacity(34);
    prefixed.extend_from_slice(&[0xed, 0x01]);
    prefixed.extend_from_slice(&raw);
    let mb = multibase::encode(multibase::Base::Base58Btc, &prefixed);
    format!("did:key:{mb}")
}

fn build_jwt(payload: serde_json::Value, sk: &SigningKey) -> String {
    let header = json!({"alg": "EdDSA", "typ": "ucan/1.0+jwt", "ucv": "1.0"});
    let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
    let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
    let signed = format!("{h}.{p}");
    let sig = sk.sign(signed.as_bytes());
    let s = URL_SAFE_NO_PAD.encode(sig.to_bytes());
    format!("{h}.{p}.{s}")
}

fn future_exp() -> i64 {
    (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600) as i64
}

fn root_ucan_for(aud_did: &str, caps: &[&str]) -> String {
    let sk_root = signing_key_for_seed(1);
    let payload = json!({
        "iss":  did_key_for(&sk_root),
        "aud":  aud_did,
        "sub":  did_key_for(&sk_root),
        "cmd":  "atd-cap",
        "args": { "caps": caps, "with": [] },
        "nonce": "phase-f-test-nonce",
        "exp":  future_exp(),
        "prf":  Vec::<String>::new()
    });
    build_jwt(payload, &sk_root)
}

// ---- the test ----

#[tokio::test]
async fn uds_hello_with_ucan_grants_union() {
    // Server: --grant-capability records:read
    let socket_path =
        std::env::temp_dir().join(format!("atd-ucan-uds-test-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket_path);

    let mut reg = Registry::new();
    reg.register(Arc::new(NoopTool::new()));

    let cfg = ServerConfig {
        socket_path: socket_path.clone(),
        granted_capabilities: vec!["records:read".into(), "fs.write".into()],
        server_version: "ucan-uds-test 0.0.0".into(),
        ..ServerConfig::default()
    };
    let server = Server::new(reg, cfg);
    let serve_handle = tokio::spawn(server.run());
    // Wait for the socket to appear.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !socket_path.exists() {
        if std::time::Instant::now() > deadline {
            panic!("server did not bind socket within 3s");
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    // Client: present a UCAN proving `summary:read`. Audience is the
    // client's own did:key; Hello.client_id must match for audience pin
    // (Phase C requires this).
    let sk_client = signing_key_for_seed(7);
    let agent_did = did_key_for(&sk_client);
    let ucan = root_ucan_for(&agent_did, &["summary:read"]);

    let client = AtdClient::connect(Endpoint::unix(socket_path.clone()))
        .await
        .expect("client connect");

    let granted = client
        .hello_with_ucan_tokens(
            Some(&agent_did),
            vec!["records:read".into(), "fs.write".into()],
            vec![ucan],
        )
        .await
        .expect("hello_with_ucan should succeed");

    // Union semantics:
    // - server allow-list = [records:read, fs.write]
    // - requested = [records:read, fs.write]
    // - string ∩ requested = [records:read, fs.write]
    // - UCAN proves [summary:read]
    // - granted = {records:read, fs.write, summary:read} (deterministic alpha sort)
    assert!(
        granted.contains(&"records:read".to_string()),
        "expected records:read in {granted:?}"
    );
    assert!(
        granted.contains(&"summary:read".to_string()),
        "expected summary:read (from UCAN) in {granted:?}"
    );
    assert!(
        granted.contains(&"fs.write".to_string()),
        "expected fs.write in {granted:?}"
    );

    // Cleanup.
    serve_handle.abort();
    let _ = std::fs::remove_file(&socket_path);
}
