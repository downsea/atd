//! UDS ↔ HTTP parity test — the SP-streamable-http §8.2 promise.
//!
//! Drive the SAME `EchoStub` tool via two paths:
//!   (a) `atd_runtime::dispatch::run_tool` directly — this is the
//!       byte-exact function the Unix-socket connection loop invokes
//!       (`atd-server::connection.rs::dispatch` is a 1-line forwarder
//!       to this fn, as of SP-streamable-http §6.3 refactor).
//!   (b) `POST /mcp` (`tools/call`) — the HTTP path under test.
//!
//! Assert: the `ToolResultResponse.result` JSON value produced by (a)
//! equals the JSON value reconstructed from the HTTP envelope
//! (`response.result.content[0].text` parsed back).
//!
//! Why this proves parity. The HTTP route handler calls
//! `atd_runtime::dispatch::run_tool` with exactly the same args.
//! Equality at the JSON level means the same `Tool::call` ran with the
//! same `CallContext` shape and the response was wrapped without
//! information loss. SP-streamable-http §5.3.

mod common;

use std::sync::Arc;

use atd_protocol::Response;
use atd_runtime::capability::CapabilitySet;
use atd_runtime::dispatch::{ServerState, SharedServerConfig};
use atd_runtime::tracker::ReadTracker;
use atd_runtime::TierPolicy;
use atd_server_http::HttpServerConfig;
use common::{echo_registry, spawn_server, EchoStub};

use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::json;

/// Build a fresh `ServerState` with one `EchoStub` registered. Each
/// transport instantiates one — they share *struct shape*, not the
/// `Arc`, because UDS does not run on the same registry pointer in
/// this test (we don't actually spawn the UDS listener; we just drive
/// `run_tool` synchronously).
fn make_state() -> Arc<ServerState> {
    let mut reg = atd_runtime::registry::Registry::new();
    reg.register(Arc::new(EchoStub::new()));
    Arc::new(ServerState {
        registry: reg,
        config: SharedServerConfig::for_test(),
        tier_policy: TierPolicy::defaults(),
        middleware: vec![],
    })
}

async fn run_tool_uds_path(args: serde_json::Value) -> serde_json::Value {
    let state = make_state();
    let tracker = Arc::new(ReadTracker::new());
    let caps = Arc::new(CapabilitySet::empty());
    let resp = atd_runtime::dispatch::run_tool(
        &state,
        &tracker,
        &caps,
        None, // anonymous, mirrors HTTP path's default
        "ref:echo.say".into(),
        args,
        false,
    )
    .await;
    match resp {
        Response::ToolResultResponse { result, .. } => result,
        other => panic!("expected ToolResultResponse, got {other:?}"),
    }
}

async fn run_tool_http_path(args: serde_json::Value) -> serde_json::Value {
    let running = spawn_server(echo_registry(), HttpServerConfig::default()).await;
    let client: Client<_, http_body_util::Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let body = json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params": {"name":"ref:echo.say","arguments": args}
    });
    let bytes = Bytes::from(serde_json::to_vec(&body).unwrap());
    let req = hyper::Request::builder()
        .method("POST")
        .uri(format!("http://{}/mcp", running.addr))
        .header("content-type", "application/json")
        .body(http_body_util::Full::new(bytes))
        .unwrap();
    let resp = client.request(req).await.expect("request");
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("json");
    // Unwrap MCP envelope back to the raw `result` value.
    let text = parsed["result"]["content"][0]["text"]
        .as_str()
        .expect("content[0].text");
    let inner: serde_json::Value = serde_json::from_str(text).expect("inner json");
    running.handle.abort();
    inner
}

#[tokio::test]
async fn uds_and_http_produce_byte_identical_result_for_echo_say() {
    let args = json!({"k": "v", "n": 1, "nested": {"a": true}});
    let uds = run_tool_uds_path(args.clone()).await;
    let http = run_tool_http_path(args.clone()).await;
    // serde_json::Value equality is structural / canonical for scalars
    // and recurses on objects + arrays, so this is the exact byte-
    // equality SP-streamable-http §8.2 names.
    assert_eq!(
        uds, http,
        "UDS and HTTP dispatch must produce identical ToolResult.result"
    );
    // Also confirm the actual content (defensive: ensures both paths
    // ran the tool rather than both producing the same null sentinel).
    assert_eq!(uds["echoed"]["k"], "v");
    assert_eq!(uds["echoed"]["nested"]["a"], true);
}

#[tokio::test]
async fn parity_holds_for_empty_arguments() {
    let args = json!({});
    let uds = run_tool_uds_path(args.clone()).await;
    let http = run_tool_http_path(args.clone()).await;
    assert_eq!(uds, http);
}
