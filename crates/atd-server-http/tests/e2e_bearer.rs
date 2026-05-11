//! Bearer-token + broker integration.
//!
//! SP-streamable-http §8.2:
//! - `require_bearer = true` + no header → 401 / -32002
//! - good bearer → 200 with broker-derived caller_id reaching dispatch
//! - bad bearer (broker rejects) → 401 / -32002

mod common;

use std::sync::Arc;

use atd_runtime::secrets::BearerIdentity;
use atd_server_http::HttpServerConfig;
use common::{echo_registry, spawn_server, FixedBroker};

use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::json;

async fn post(
    addr: std::net::SocketAddr,
    bearer: Option<&str>,
    body: serde_json::Value,
) -> (hyper::StatusCode, serde_json::Value) {
    let client: Client<_, http_body_util::Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let bytes = Bytes::from(serde_json::to_vec(&body).unwrap());
    let mut req = hyper::Request::builder()
        .method("POST")
        .uri(format!("http://{addr}/mcp"))
        .header("content-type", "application/json");
    if let Some(b) = bearer {
        req = req.header("authorization", format!("Bearer {b}"));
    }
    let req = req.body(http_body_util::Full::new(bytes)).unwrap();
    let resp = client.request(req).await.expect("request");
    let status = resp.status();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json");
    (status, parsed)
}

fn config_with_broker(require_bearer: bool, token: &str, caller: &str) -> HttpServerConfig {
    let identity = BearerIdentity {
        caller_id: caller.to_string(),
        granted_capabilities: vec!["echo".to_string()],
        secrets: None,
        expires_at: None,
        cache_until: None,
    };
    let broker: Arc<dyn atd_runtime::TokenBroker> =
        Arc::new(FixedBroker::new(token, identity));

    let mut shared = atd_runtime::dispatch::SharedServerConfig::for_test();
    shared.token_broker = Some(broker);
    shared.granted_capabilities = vec!["echo".to_string()];

    let mut cfg = HttpServerConfig::default();
    cfg.require_bearer = require_bearer;
    cfg.shared = Arc::new(shared);
    cfg
}

#[tokio::test]
async fn require_bearer_without_header_returns_401() {
    let cfg = config_with_broker(true, "ce_valid", "agent-X");
    let running = spawn_server(echo_registry(), cfg).await;
    let (status, body) = post(
        running.addr,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(status, hyper::StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], -32002);
    running.handle.abort();
}

#[tokio::test]
async fn good_bearer_admits_tools_call() {
    let cfg = config_with_broker(true, "ce_valid", "agent-X");
    let running = spawn_server(echo_registry(), cfg).await;
    let (status, body) = post(
        running.addr,
        Some("ce_valid"),
        json!({
            "jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"ref:echo.say","arguments":{"hi":"there"}}
        }),
    )
    .await;
    assert_eq!(status, hyper::StatusCode::OK);
    assert_eq!(body["result"]["isError"], false);
    running.handle.abort();
}

#[tokio::test]
async fn bad_bearer_returns_401() {
    let cfg = config_with_broker(true, "ce_valid", "agent-X");
    let running = spawn_server(echo_registry(), cfg).await;
    let (status, body) = post(
        running.addr,
        Some("ce_unknown"),
        json!({"jsonrpc":"2.0","id":3,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(status, hyper::StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], -32002);
    running.handle.abort();
}

#[tokio::test]
async fn anonymous_mode_admits_request_without_header() {
    // require_bearer = false: no Authorization header should not 401.
    let cfg = config_with_broker(false, "ce_valid", "agent-X");
    let running = spawn_server(echo_registry(), cfg).await;
    let (status, body) = post(
        running.addr,
        None,
        json!({"jsonrpc":"2.0","id":4,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(status, hyper::StatusCode::OK);
    assert!(body["result"].is_object());
    running.handle.abort();
}
