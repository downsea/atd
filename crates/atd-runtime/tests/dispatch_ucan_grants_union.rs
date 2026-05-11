//! SP-capability-v2 Phase C — dispatch `Hello` arm consumes `ucan_tokens`.
//!
//! Integration test for the union semantics (spec §4.2):
//! `granted = (server_allow_list ∩ requested_capabilities) ∪ ucan_derived_caps`
//!
//! Plus negative tests: invalid UCAN → `Response::Error` with correct
//! wire code from `ucan::wire_code()`.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use atd_protocol::{ERR_AUDIENCE_MISMATCH, ERR_UCAN_EXPIRED, ERR_UCAN_INVALID, Request, Response};
use atd_runtime::capability::CapabilitySet;
use atd_runtime::dispatch::{ServerState, SharedServerConfig, dispatch_request};
use atd_runtime::middleware::Middleware;
use atd_runtime::registry::Registry;
use atd_runtime::tier::TierPolicy;
use atd_runtime::tracker::ReadTracker;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;

// ---- helpers (mirror ucan::verify::tests, kept inline for integration scope) ----

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
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    now + 3600
}

fn past_exp() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    now - 3600
}

fn payload_with(
    iss: &str,
    aud: &str,
    caps: &[&str],
    prf: &[String],
    exp: i64,
) -> serde_json::Value {
    json!({
        "iss":  iss,
        "aud":  aud,
        "sub":  iss,
        "cmd":  "atd-cap",
        "args": { "caps": caps, "with": [] },
        "nonce": "test-nonce-fixed",
        "exp":  exp,
        "prf":  prf
    })
}

fn server_state_with_allow_list(allow: Vec<String>) -> Arc<ServerState> {
    let mut cfg = SharedServerConfig::for_test();
    cfg.granted_capabilities = allow;
    Arc::new(ServerState {
        registry: Registry::new(),
        config: cfg,
        tier_policy: TierPolicy::default(),
        middleware: Vec::<Arc<dyn Middleware>>::new(),
    })
}

async fn dispatch_hello(
    state: &Arc<ServerState>,
    client_id: Option<&str>,
    requested: Vec<&str>,
    ucan_tokens: Vec<String>,
) -> Response {
    let mut caps: Arc<CapabilitySet> = Arc::new(CapabilitySet::empty());
    let mut caller: Option<String> = None;
    let tracker = Arc::new(ReadTracker::default());
    let req = Request::Hello {
        client_id: client_id.map(|s| s.to_string()),
        requested_capabilities: requested.into_iter().map(String::from).collect(),
        ucan_tokens,
    };
    dispatch_request(state, &tracker, &mut caps, &mut caller, req).await
}

// ---- tests --------------------------------------------------------------

#[tokio::test]
async fn hello_without_ucan_tokens_matches_sp12_behavior() {
    // Empty ucan_tokens → granted = string-allow-list ∩ requested.
    let state = server_state_with_allow_list(vec!["records:read".into(), "fs.write".into()]);
    let resp = dispatch_hello(
        &state,
        Some("agent-A"),
        vec!["records:read", "summary:read"],
        vec![],
    )
    .await;
    match resp {
        Response::HelloAck {
            granted_capabilities,
            ..
        } => {
            assert_eq!(granted_capabilities, vec!["records:read"]);
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
}

#[tokio::test]
async fn hello_with_ucan_unions_caps_into_granted() {
    // server_allow_list = [records:read]
    // requested = [records:read]
    // ucan_tokens proves [summary:read] → union = {records:read, summary:read}
    let sk_user = signing_key_for_seed(1);
    let sk_agent = signing_key_for_seed(2);
    let aud_did = did_key_for(&sk_agent);

    let payload = payload_with(
        &did_key_for(&sk_user),
        &aud_did,
        &["summary:read"],
        &[],
        future_exp(),
    );
    let jwt = build_jwt(payload, &sk_user);

    let state = server_state_with_allow_list(vec!["records:read".into(), "fs.write".into()]);
    let resp = dispatch_hello(&state, Some(&aud_did), vec!["records:read"], vec![jwt]).await;

    match resp {
        Response::HelloAck {
            granted_capabilities,
            ..
        } => {
            assert!(
                granted_capabilities.iter().any(|c| c == "records:read"),
                "expected records:read in granted (from string allow-list), got {granted_capabilities:?}"
            );
            assert!(
                granted_capabilities.iter().any(|c| c == "summary:read"),
                "expected summary:read in granted (from UCAN), got {granted_capabilities:?}"
            );
            assert!(
                !granted_capabilities.iter().any(|c| c == "fs.write"),
                "fs.write was on allow-list but NOT requested → must not appear"
            );
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
}

#[tokio::test]
async fn hello_with_ucan_only_grants_via_ucan_path() {
    // server_allow_list empty; client requests nothing; UCAN proves
    // [records:read]. Union semantics: granted = {records:read} —
    // proves UCAN path is sufficient on its own.
    let sk_user = signing_key_for_seed(1);
    let sk_agent = signing_key_for_seed(2);
    let aud_did = did_key_for(&sk_agent);

    let payload = payload_with(
        &did_key_for(&sk_user),
        &aud_did,
        &["records:read"],
        &[],
        future_exp(),
    );
    let jwt = build_jwt(payload, &sk_user);

    let state = server_state_with_allow_list(vec![]); // empty allow-list
    let resp = dispatch_hello(&state, Some(&aud_did), vec![], vec![jwt]).await;

    match resp {
        Response::HelloAck {
            granted_capabilities,
            ..
        } => {
            assert_eq!(granted_capabilities, vec!["records:read".to_string()]);
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
}

#[tokio::test]
async fn hello_with_expired_ucan_returns_err_ucan_expired() {
    let sk_user = signing_key_for_seed(1);
    let sk_agent = signing_key_for_seed(2);
    let aud_did = did_key_for(&sk_agent);

    let payload = payload_with(
        &did_key_for(&sk_user),
        &aud_did,
        &["records:read"],
        &[],
        past_exp(),
    );
    let jwt = build_jwt(payload, &sk_user);

    let state = server_state_with_allow_list(vec!["records:read".into()]);
    let resp = dispatch_hello(&state, Some(&aud_did), vec!["records:read"], vec![jwt]).await;

    match resp {
        Response::Error {
            code: Some(c),
            retryable: Some(false),
            ..
        } if c == ERR_UCAN_EXPIRED => {}
        other => panic!("expected Error code 1011 ERR_UCAN_EXPIRED, got {other:?}"),
    }
}

#[tokio::test]
async fn hello_with_audience_mismatch_returns_err_1013() {
    let sk_user = signing_key_for_seed(1);
    let sk_a = signing_key_for_seed(2);
    let sk_b = signing_key_for_seed(3);

    // UCAN's aud = sk_a; but Hello.client_id = sk_b → audience mismatch
    let payload = payload_with(
        &did_key_for(&sk_user),
        &did_key_for(&sk_a),
        &["records:read"],
        &[],
        future_exp(),
    );
    let jwt = build_jwt(payload, &sk_user);

    let state = server_state_with_allow_list(vec!["records:read".into()]);
    let resp = dispatch_hello(
        &state,
        Some(&did_key_for(&sk_b)),
        vec!["records:read"],
        vec![jwt],
    )
    .await;

    match resp {
        Response::Error {
            code: Some(c),
            retryable: Some(false),
            ..
        } if c == ERR_AUDIENCE_MISMATCH => {}
        other => panic!("expected Error code 1013 ERR_AUDIENCE_MISMATCH, got {other:?}"),
    }
}

#[tokio::test]
async fn hello_with_ucan_but_no_client_id_returns_err_1013() {
    // Cannot bind audience without a client_id → reject early.
    let sk_user = signing_key_for_seed(1);
    let sk_a = signing_key_for_seed(2);
    let payload = payload_with(
        &did_key_for(&sk_user),
        &did_key_for(&sk_a),
        &["records:read"],
        &[],
        future_exp(),
    );
    let jwt = build_jwt(payload, &sk_user);

    let state = server_state_with_allow_list(vec!["records:read".into()]);
    let resp = dispatch_hello(&state, None, vec!["records:read"], vec![jwt]).await;

    match resp {
        Response::Error {
            code: Some(c),
            message,
            retryable: Some(false),
            ..
        } if c == ERR_AUDIENCE_MISMATCH => {
            assert!(
                message.contains("client_id"),
                "message should explain missing client_id: {message}"
            );
        }
        other => panic!("expected Error code 1013 (no client_id), got {other:?}"),
    }
}

#[tokio::test]
async fn hello_with_malformed_ucan_returns_err_1010_ucan_invalid() {
    // Garbage token — not even a valid JWT shape.
    let state = server_state_with_allow_list(vec!["records:read".into()]);
    let resp = dispatch_hello(
        &state,
        Some("did:key:zSomething"),
        vec!["records:read"],
        vec!["not.a.jwt.token".into()], // 4 segments — malformed
    )
    .await;

    match resp {
        Response::Error {
            code: Some(c),
            retryable: Some(false),
            ..
        } if c == ERR_UCAN_INVALID => {}
        other => panic!("expected Error code 1010 ERR_UCAN_INVALID, got {other:?}"),
    }
}
