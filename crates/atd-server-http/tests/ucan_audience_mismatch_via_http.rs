//! SP-capability-v2 Phase F — HTTP UCAN audience-pin negative path.
//!
//! Spec §8.2 second entry:
//! > tests/ucan_audience_mismatch_via_http.rs — POST /mcp with a UCAN
//! > whose `aud` ≠ bearer subject; expect 401 ERR_AUDIENCE_MISMATCH.
//!
//! HTTP wire interpretation: the broker registers a set of DIDs it
//! recognises as legitimate audiences. A UCAN whose leaf `aud` is not
//! in that registry never validates — the broker returns
//! `BrokerError::Lookup("unregistered UCAN audience: ...")`, which the
//! HTTP listener translates to a 401 with JSON-RPC error -32002.
//!
//! This is the integration analog of the `unregistered UCAN audience`
//! unit test in `secrets::tests::ucan_jwt_branch`.

mod common;

use common::{
    EchoStub, build_jwt, did_key_for, future_exp, http_config_with_ucan_broker, payload_with,
    signing_key_for_seed, spawn_server,
};

use atd_runtime::registry::Registry;
use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::json;
use std::sync::Arc;

async fn post_with_bearer(
    addr: std::net::SocketAddr,
    bearer: &str,
    body: serde_json::Value,
) -> (hyper::StatusCode, serde_json::Value) {
    let client: Client<_, http_body_util::Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let bytes = Bytes::from(serde_json::to_vec(&body).unwrap());
    let req = hyper::Request::builder()
        .method("POST")
        .uri(format!("http://{addr}/mcp"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {bearer}"))
        .body(http_body_util::Full::new(bytes))
        .unwrap();
    let resp = client.request(req).await.expect("request");
    let status = resp.status();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or(json!(null));
    (status, parsed)
}

#[tokio::test]
async fn ucan_with_unregistered_audience_returns_401() {
    // Broker registers DID-A as caller "agent-A". UCAN issued to a
    // DIFFERENT (unregistered) DID-X — even though the JWT is otherwise
    // perfectly valid (signed, well-formed, not expired), the broker
    // should reject it as having no recognised audience.
    let sk_user = signing_key_for_seed(1);
    let sk_a = signing_key_for_seed(2);
    let sk_x = signing_key_for_seed(99); // unregistered

    let registered_did = did_key_for(&sk_a);
    let cfg = http_config_with_ucan_broker(
        /* require_bearer */ true,
        vec![(registered_did.clone(), "agent-A".to_string())],
        vec!["records:read".to_string()],
    );
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoStub::new()));
    let running = spawn_server(reg, cfg).await;

    // UCAN's aud is sk_x (unregistered with the broker).
    let p = payload_with(
        &did_key_for(&sk_user),
        &did_key_for(&sk_x),
        &["records:read"],
        &[],
        future_exp(),
    );
    let jwt = build_jwt(p, &sk_user);

    let (status, _body) = post_with_bearer(
        running.addr,
        &jwt,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;

    assert_eq!(
        status,
        hyper::StatusCode::UNAUTHORIZED,
        "unregistered UCAN audience must produce HTTP 401"
    );

    running.handle.abort();
}

#[tokio::test]
async fn ucan_with_registered_audience_admits_request() {
    // Same setup as above but the UCAN's aud IS the registered DID —
    // request should reach the server and succeed (or at least not 401).
    let sk_user = signing_key_for_seed(1);
    let sk_a = signing_key_for_seed(2);

    let registered_did = did_key_for(&sk_a);
    let cfg = http_config_with_ucan_broker(
        true,
        vec![(registered_did.clone(), "agent-A".to_string())],
        vec!["records:read".to_string()],
    );
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoStub::new()));
    let running = spawn_server(reg, cfg).await;

    let p = payload_with(
        &did_key_for(&sk_user),
        &registered_did,
        &["records:read"],
        &[],
        future_exp(),
    );
    let jwt = build_jwt(p, &sk_user);

    let (status, _body) = post_with_bearer(
        running.addr,
        &jwt,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;

    assert_eq!(
        status,
        hyper::StatusCode::OK,
        "registered UCAN audience must be admitted by the broker"
    );

    running.handle.abort();
}
