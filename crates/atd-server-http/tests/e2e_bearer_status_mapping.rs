//! SP-token-broker-phase2 §4.4 + §8.2 — HTTP status + headers per
//! bearer-auth outcome class.
//!
//! Five end-to-end cases proving the wire-observable distinction:
//! - `Ok(None)` → 401 + `WWW-Authenticate: Bearer error="invalid_token"`
//! - `Err(Expired)` → 401 + `... error_description="expired"`
//! - `Err(Revoked)` → 401 + `... error_description="revoked"`
//! - `Err(Lookup)` → 503 + `Retry-After: 5`
//! - `Err(Internal)` → 500 + no auth headers
//!
//! Complements the existing `e2e_bearer.rs` happy/missing/bad-token
//! suite; together they cover the full spec §4.4 status table.

mod common;

use std::sync::Arc;

use atd_runtime::secrets::{
    BearerIdentity, BrokerError, ResolveBearerFuture, ResolveFuture, TokenBroker,
};
use atd_server_http::HttpServerConfig;
use common::{echo_registry, spawn_server};

use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::json;

// ---- broker fixtures: each one always returns a specific BrokerError ----

#[derive(Debug)]
struct UnknownBroker;
impl TokenBroker for UnknownBroker {
    fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async { Ok(None) })
    }
    fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
        Box::pin(async { Ok(None) })
    }
}

#[derive(Debug)]
struct ExpiredBroker;
impl TokenBroker for ExpiredBroker {
    fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async { Ok(None) })
    }
    fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
        Box::pin(async { Err(BrokerError::Expired) })
    }
}

#[derive(Debug)]
struct RevokedBroker;
impl TokenBroker for RevokedBroker {
    fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async { Ok(None) })
    }
    fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
        Box::pin(async { Err(BrokerError::Revoked("by user".into())) })
    }
}

#[derive(Debug)]
struct LookupBroker;
impl TokenBroker for LookupBroker {
    fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async { Ok(None) })
    }
    fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
        Box::pin(async { Err(BrokerError::Lookup("sqlite locked".into())) })
    }
}

#[derive(Debug)]
struct InternalBroker;
impl TokenBroker for InternalBroker {
    fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async { Ok(None) })
    }
    fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
        Box::pin(async { Err(BrokerError::Internal("oh no".into())) })
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct ValidBroker(BearerIdentity);
impl TokenBroker for ValidBroker {
    fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async { Ok(None) })
    }
    fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
        let id = self.0.clone();
        Box::pin(async move { Ok(Some(id)) })
    }
}

fn cfg_with_broker(broker: Arc<dyn TokenBroker>) -> HttpServerConfig {
    let mut shared = atd_runtime::dispatch::SharedServerConfig::for_test();
    shared.token_broker = Some(broker);
    shared.granted_capabilities = vec!["echo".to_string()];

    let mut cfg = HttpServerConfig::default();
    cfg.require_bearer = true;
    cfg.shared = Arc::new(shared);
    cfg
}

/// POST `/mcp` with the given bearer and parse status + body + a
/// caller-named subset of response headers.
async fn post_collect(
    addr: std::net::SocketAddr,
    bearer: &str,
    body: serde_json::Value,
    pick_headers: &[&str],
) -> (hyper::StatusCode, serde_json::Value, Vec<(String, String)>) {
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
    let headers: Vec<(String, String)> = pick_headers
        .iter()
        .filter_map(|name| {
            resp.headers().get(*name).and_then(|v| {
                v.to_str()
                    .ok()
                    .map(|s| ((*name).to_string(), s.to_string()))
            })
        })
        .collect();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap_or(json!(null));
    (status, body, headers)
}

// ---- tests ----

#[tokio::test]
async fn e2e_bearer_unknown_returns_401_invalid_token() {
    let running = spawn_server(echo_registry(), cfg_with_broker(Arc::new(UnknownBroker))).await;
    let (status, body, headers) = post_collect(
        running.addr,
        "anything",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        &["www-authenticate"],
    )
    .await;
    assert_eq!(status, hyper::StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], -32002);
    let www = headers
        .iter()
        .find(|(k, _)| k == "www-authenticate")
        .map(|(_, v)| v.as_str());
    assert_eq!(www, Some(r#"Bearer error="invalid_token""#));
    running.handle.abort();
}

#[tokio::test]
async fn e2e_bearer_expired_returns_401_expired() {
    let running = spawn_server(echo_registry(), cfg_with_broker(Arc::new(ExpiredBroker))).await;
    let (status, body, headers) = post_collect(
        running.addr,
        "anything",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        &["www-authenticate"],
    )
    .await;
    assert_eq!(status, hyper::StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], -32002);
    let www = headers
        .iter()
        .find(|(k, _)| k == "www-authenticate")
        .map(|(_, v)| v.as_str());
    assert_eq!(
        www,
        Some(r#"Bearer error="invalid_token", error_description="expired""#)
    );
    running.handle.abort();
}

#[tokio::test]
async fn e2e_bearer_revoked_returns_401_revoked() {
    let running = spawn_server(echo_registry(), cfg_with_broker(Arc::new(RevokedBroker))).await;
    let (status, body, headers) = post_collect(
        running.addr,
        "anything",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        &["www-authenticate"],
    )
    .await;
    assert_eq!(status, hyper::StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], -32002);
    let www = headers
        .iter()
        .find(|(k, _)| k == "www-authenticate")
        .map(|(_, v)| v.as_str());
    assert_eq!(
        www,
        Some(r#"Bearer error="invalid_token", error_description="revoked""#)
    );
    // The message body should contain the broker-supplied reason so
    // adopter UIs can show it verbatim.
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("by user")
    );
    running.handle.abort();
}

#[tokio::test]
async fn e2e_bearer_broker_lookup_returns_503_retry_after_5() {
    let running = spawn_server(echo_registry(), cfg_with_broker(Arc::new(LookupBroker))).await;
    let (status, body, headers) = post_collect(
        running.addr,
        "anything",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        &["retry-after", "www-authenticate"],
    )
    .await;
    assert_eq!(status, hyper::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], -32002);
    let retry = headers
        .iter()
        .find(|(k, _)| k == "retry-after")
        .map(|(_, v)| v.as_str());
    assert_eq!(retry, Some("5"));
    // No WWW-Authenticate — this is a server-side hiccup, not an auth challenge.
    assert!(headers.iter().all(|(k, _)| k != "www-authenticate"));
    running.handle.abort();
}

#[tokio::test]
async fn e2e_bearer_broker_internal_returns_500_no_auth_headers() {
    let running = spawn_server(echo_registry(), cfg_with_broker(Arc::new(InternalBroker))).await;
    let (status, body, headers) = post_collect(
        running.addr,
        "anything",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        &["www-authenticate", "retry-after"],
    )
    .await;
    assert_eq!(status, hyper::StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"]["code"], -32002);
    assert!(headers.is_empty(), "no auth headers expected on 500");
    running.handle.abort();
}
