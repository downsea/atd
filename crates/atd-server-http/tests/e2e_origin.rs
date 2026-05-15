//! Origin allow-list — fail-closed default + opt-in extras.
//!
//! SP-streamable-http §8.2: an off-list `Origin` is rejected with 403 +
//! JSON-RPC -32001. `extra_origins` admits a verbatim match.

mod common;

use common::{echo_registry, spawn_server};

use atd_server_http::HttpServerConfig;
use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::json;

async fn post_with_origin(
    addr: std::net::SocketAddr,
    origin: &str,
) -> (hyper::StatusCode, serde_json::Value) {
    let client: Client<_, http_body_util::Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let body = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    let bytes = Bytes::from(serde_json::to_vec(&body).unwrap());
    let req = hyper::Request::builder()
        .method("POST")
        .uri(format!("http://{addr}/mcp"))
        .header("content-type", "application/json")
        .header("origin", origin)
        .body(http_body_util::Full::new(bytes))
        .unwrap();
    let resp = client.request(req).await.expect("request");
    let status = resp.status();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json");
    (status, parsed)
}

#[tokio::test]
async fn off_list_origin_rejected_403() {
    let running = spawn_server(echo_registry(), HttpServerConfig::default()).await;
    let (status, body) = post_with_origin(running.addr, "https://evil.example").await;
    assert_eq!(status, hyper::StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], -32001);
    running.handle.abort();
}

#[tokio::test]
async fn loopback_origin_accepted() {
    let running = spawn_server(echo_registry(), HttpServerConfig::default()).await;
    let (status, body) = post_with_origin(running.addr, "http://127.0.0.1:5173").await;
    assert_eq!(status, hyper::StatusCode::OK);
    assert!(body["result"].is_object());
    running.handle.abort();
}

#[tokio::test]
async fn extra_origin_admits_verbatim_match() {
    let cfg = HttpServerConfig {
        extra_origins: vec!["https://celia.health".to_string()],
        ..HttpServerConfig::default()
    };
    let running = spawn_server(echo_registry(), cfg).await;
    let (status, body) = post_with_origin(running.addr, "https://celia.health").await;
    assert_eq!(status, hyper::StatusCode::OK);
    assert!(body["result"].is_object());
    running.handle.abort();
}
