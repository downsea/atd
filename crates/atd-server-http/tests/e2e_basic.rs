//! End-to-end smoke test: start the server, run the standard MCP
//! handshake (`initialize` → `tools/list` → `tools/call`) over real
//! HTTP, assert all three succeed.
//!
//! SP-streamable-http §8.2.

mod common;

use common::{echo_registry, spawn_server};

use atd_server_http::HttpServerConfig;
use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::json;

fn post_url(addr: std::net::SocketAddr) -> String {
    format!("http://{addr}/mcp")
}

async fn post_json(
    client: &Client<hyper_util::client::legacy::connect::HttpConnector, http_body_util::Full<Bytes>>,
    url: &str,
    body: serde_json::Value,
) -> (hyper::StatusCode, serde_json::Value) {
    let bytes = Bytes::from(serde_json::to_vec(&body).unwrap());
    let req = hyper::Request::builder()
        .method("POST")
        .uri(url)
        .header("content-type", "application/json")
        .body(http_body_util::Full::new(bytes))
        .unwrap();
    let resp = client.request(req).await.expect("request");
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("json");
    (status, parsed)
}

#[tokio::test]
async fn initialize_tools_list_tools_call_round_trip() {
    let running = spawn_server(echo_registry(), HttpServerConfig::default()).await;
    let url = post_url(running.addr);
    let client: Client<_, http_body_util::Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    // 1. initialize
    let (status, body) = post_json(
        &client,
        &url,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(status, hyper::StatusCode::OK);
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    assert_eq!(body["result"]["protocolVersion"], "2025-06-18");

    // 2. tools/list
    let (status, body) = post_json(
        &client,
        &url,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await;
    assert_eq!(status, hyper::StatusCode::OK);
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "ref:echo.say");
    assert!(tools[0]["inputSchema"].is_object());

    // 3. tools/call
    let (status, body) = post_json(
        &client,
        &url,
        json!({
            "jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"ref:echo.say","arguments":{"hello":"world"}}
        }),
    )
    .await;
    assert_eq!(status, hyper::StatusCode::OK);
    assert_eq!(body["result"]["isError"], false);
    let text = body["result"]["content"][0]["text"].as_str().unwrap();
    let inner: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(inner["echoed"]["hello"], "world");

    running.handle.abort();
}

#[tokio::test]
async fn unknown_method_returns_method_not_found() {
    let running = spawn_server(echo_registry(), HttpServerConfig::default()).await;
    let url = post_url(running.addr);
    let client: Client<_, http_body_util::Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let (status, body) = post_json(
        &client,
        &url,
        json!({"jsonrpc":"2.0","id":99,"method":"resources/list","params":{}}),
    )
    .await;
    // SP-streamable-http §5.6: unknown method → 200 + -32601.
    assert_eq!(status, hyper::StatusCode::OK);
    assert_eq!(body["error"]["code"], -32601);

    running.handle.abort();
}
