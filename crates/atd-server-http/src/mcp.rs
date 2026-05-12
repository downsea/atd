//! MCP JSON-RPC 2.0 ↔ ATD translation.
//!
//! SP-streamable-http §4.2: ATD-native `Request::Hello` / `Request::ToolList`
//! / `Request::RunTool` (`atd-protocol::messages.rs:34-52`) does **not**
//! appear on the HTTP wire. Instead the listener translates the four MCP
//! methods (`initialize`, `notifications/initialized`, `tools/list`,
//! `tools/call`) into ATD operations, then dispatches via
//! `atd_runtime::dispatch::run_tool` for tool calls. Bytes returned from
//! `Tool::call` are byte-identical to the UDS path — the parity test in
//! `tests/e2e_parity.rs` is the regression guard.

use std::sync::Arc;

use atd_protocol::{Response, ToolVisibility};
use atd_runtime::capability::CapabilitySet;
use atd_runtime::dispatch::ServerState;
use atd_runtime::secrets::BearerIdentity;
use atd_runtime::tracker::ReadTracker;
use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Incoming JSON-RPC envelope (MCP `POST /mcp` body). `jsonrpc` is
/// retained for downstream introspection but the listener does not
/// branch on it — MCP rev `2025-06-18` pins it to `"2.0"`.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    #[serde(default)]
    pub jsonrpc: String,
    /// `null` for notifications, integer/string for requests. Echoed
    /// back on the response.
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Outgoing JSON-RPC envelope. `result` and `error` are mutually
/// exclusive — exactly one is `Some` per MCP `2025-06-18`.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

/// Convenience builder for a JSON-RPC error response paired with an HTTP
/// status. SP-streamable-http §5.6 codifies the (status, code) pairs:
/// origin → 403/-32001; missing bearer → 401/-32002; unknown method →
/// 200/-32601; capability denied / broker error / internal → 200/-32603.
pub fn error_response(
    status: StatusCode,
    id: Option<Value>,
    code: i32,
    message: impl Into<String>,
) -> axum::response::Response {
    error_response_with_headers(status, id, code, message, &[])
}

/// `error_response` with optional response headers (e.g.
/// `WWW-Authenticate`, `Retry-After`). SP-token-broker-phase2 §4.4
/// requires distinct headers for each bearer-auth error class so
/// adopter-side UIs can distinguish "expired" / "revoked" / "unknown"
/// without re-parsing the message body.
pub fn error_response_with_headers(
    status: StatusCode,
    id: Option<Value>,
    code: i32,
    message: impl Into<String>,
    extra_headers: &[(&str, String)],
) -> axum::response::Response {
    let mut resp: axum::response::Response = (
        status,
        Json(JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }),
    )
        .into_response();

    if !extra_headers.is_empty() {
        let headers = resp.headers_mut();
        for (name, value) in extra_headers {
            if let (Ok(hn), Ok(hv)) = (
                axum::http::HeaderName::from_bytes(name.as_bytes()),
                axum::http::HeaderValue::from_str(value),
            ) {
                headers.insert(hn, hv);
            }
        }
    }

    resp
}

/// JSON-RPC success envelope.
pub fn ok_response(id: Option<Value>, result: Value) -> axum::response::Response {
    Json(JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    })
    .into_response()
}

/// Build the `initialize` reply. Echoes `serverInfo` (name + version),
/// an empty `capabilities.tools` map, and — when a `TokenBroker` is
/// wired — the broker's `accepted_token_formats()` as an `experimental`
/// hint (SP-token-broker-phase2 §8.2 last entry).
///
/// SP-streamable-http §4.2: `initialize` does NOT enter the dispatch
/// state machine — the listener synthesises this reply directly.
/// Mirrors `celia-cli/src/http_server.rs:417-435`.
pub fn handle_initialize(
    id: Option<Value>,
    server_version: &str,
    token_broker: Option<&Arc<dyn atd_runtime::TokenBroker>>,
) -> axum::response::Response {
    let (name, version) = split_server_version(server_version);
    let mut capabilities = json!({ "tools": {} });

    // SP-token-broker-phase2 §4.2 / §8.2 — the listener does NOT route
    // on this field; it is a diagnostic / discovery hint so clients +
    // operators can see at a glance which bearer formats the deployed
    // broker accepts. MCP's `capabilities.experimental` map is the
    // designated extension slot for non-MCP-spec capabilities.
    if let Some(broker) = token_broker {
        let formats = broker.accepted_token_formats();
        if !formats.is_empty() {
            let formats_value = serde_json::Value::Array(
                formats
                    .iter()
                    .map(|f| serde_json::Value::String((*f).to_string()))
                    .collect(),
            );
            capabilities["experimental"] = json!({
                "atd": {
                    "acceptedTokenFormats": formats_value
                }
            });
        }
    }

    let info = json!({
        "protocolVersion": "2025-06-18",
        "serverInfo": {
            "name": name,
            "version": version,
        },
        "capabilities": capabilities,
    });
    ok_response(id, info)
}

fn split_server_version(s: &str) -> (String, String) {
    // Server-version strings look like `"atd-ref-server 0.3.0"` —
    // last whitespace-separated token is the version. Defensive: if no
    // space found, return the whole string as the name and "0.0.0" as
    // version.
    if let Some(idx) = s.rfind(' ') {
        let (name, ver) = s.split_at(idx);
        (name.to_string(), ver.trim().to_string())
    } else {
        (s.to_string(), "0.0.0".to_string())
    }
}

/// `notifications/initialized` is one-way per MCP; we return an empty
/// JSON-RPC result so the client's request future resolves cleanly.
pub fn handle_initialized_notification(id: Option<Value>) -> axum::response::Response {
    ok_response(id, json!({}))
}

/// `tools/list` — wraps `Registry::summaries` filtered by visibility
/// into the MCP `{ tools: [...] }` shape. Each entry exposes
/// `{name, description, inputSchema, ...atd_fields...}` so MCP clients
/// see the standard fields and ATD-aware clients still see the full
/// definition. Mirrors `celia-cli/src/http_server.rs:437-455` with the
/// ATD `ToolSummary` payload preserved.
pub fn handle_tools_list(id: Option<Value>, state: &ServerState) -> axum::response::Response {
    let tools: Vec<Value> = state
        .registry
        .summaries()
        .into_iter()
        .filter(|s| !matches!(s.visibility, ToolVisibility::Hidden))
        .map(|summary| {
            // Compose the MCP-required shape (`name` / `description` /
            // `inputSchema`) by reading the underlying `ToolDefinition`
            // — the summary alone does not carry the output schema /
            // tier-aware version. For any tool whose definition is
            // missing (deleted between summaries() and get(), should not
            // happen), fall back to the summary alone.
            let def_extras = state
                .registry
                .get(&summary.id)
                .map(|t| {
                    let def = t.definition();
                    json!({
                        "version": def.version,
                        "outputSchema": def.output_schema,
                    })
                })
                .unwrap_or_else(|| json!({}));
            let mut entry = json!({
                "name": summary.id,
                "description": summary.description,
                "inputSchema": summary.input_schema.clone().unwrap_or_else(|| json!({})),
            });
            if let Some(obj) = entry.as_object_mut() {
                if let Some(extra_obj) = def_extras.as_object() {
                    for (k, v) in extra_obj {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
            entry
        })
        .collect();
    ok_response(id, json!({ "tools": tools }))
}

/// `tools/call` — the load-bearing path.
///
/// 1. Extract `name` + `arguments` from `params`.
/// 2. Build a per-request `CapabilitySet` from the bearer identity
///    (intersected with the server allow-list — SP-streamable-http §4.3).
/// 3. Call `atd_runtime::dispatch::run_tool` — the very same function
///    `atd-server`'s connection loop uses.
/// 4. Wrap the returned `Response` in MCP's
///    `{ content: [{type:"text", text}], isError }` envelope.
///
/// The `text` field carries the JSON-serialised
/// `Response::ToolResultResponse.result`. SP-streamable-http §5.3
/// pins this contract: bytes are equal to UDS for the same `RunTool`.
pub async fn handle_tools_call(
    id: Option<Value>,
    state: &Arc<ServerState>,
    identity: Option<&BearerIdentity>,
    params: Value,
) -> axum::response::Response {
    let tool_id = match params.get("name").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return error_response(StatusCode::OK, id, -32602, "missing `name` parameter");
        }
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));

    // SP-streamable-http §4.3: capability set is per-request, intersected
    // with the server allow-list. Anonymous mode (no identity) yields
    // empty caps — same as the UDS pre-Hello default.
    let caps = Arc::new(build_caps(state, identity));
    let caller_id = identity.map(|i| i.caller_id.clone());
    // SP-streamable-http §4.3: fresh tracker per request. Read-budget
    // enforcement still works but is scoped to one call rather than the
    // connection lifetime.
    let tracker = Arc::new(ReadTracker::new());

    let resp = atd_runtime::dispatch::run_tool(
        state,
        &tracker,
        &caps,
        caller_id.as_deref(),
        tool_id.clone(),
        args,
        false, // HTTP does not expose dry_run on tools/call
    )
    .await;

    wrap_tool_response(id, resp)
}

/// Intersect `identity.granted_capabilities` (when present) with the
/// operator allow-list `state.config.granted_capabilities`. Anonymous
/// callers (no identity) get an empty set — matching the UDS pre-Hello
/// default (`atd-server::connection.rs:22`). This is the SP-12 Hello
/// semantics specialised per-request rather than per-connection.
fn build_caps(state: &ServerState, identity: Option<&BearerIdentity>) -> CapabilitySet {
    let Some(id) = identity else {
        return CapabilitySet::empty();
    };
    let allow = CapabilitySet::from_iter(state.config.granted_capabilities.iter().cloned());
    let (granted, _denied) = allow.intersect(&id.granted_capabilities);
    CapabilitySet::from_iter(granted)
}

/// Translate an ATD `Response` (from `run_tool`) into the MCP wire shape
/// the HTTP listener returns. Mirrors `celia-cli/src/http_server.rs:497-525`
/// for the success / execution_failed branches; non-result `Response`
/// variants (capability denied, broker error, internal) map to the
/// JSON-RPC `error` envelope per SP-streamable-http §5.6.
pub fn wrap_tool_response(id: Option<Value>, resp: Response) -> axum::response::Response {
    match resp {
        Response::ToolResultResponse {
            result,
            success,
            dry_run: _,
            tool_id: _,
        } => {
            let text = serde_json::to_string(&result).unwrap_or_else(|_| "{}".into());
            let body = json!({
                "content": [{ "type": "text", "text": text }],
                "isError": !success,
            });
            ok_response(id, body)
        }
        Response::Error {
            message,
            code,
            retryable,
            details,
        } => {
            // SP-streamable-http §5.6: ATD numeric codes (1001/1002/1003)
            // ride inside the JSON-RPC error.data so MCP clients see
            // `-32603` (generic server error) and ATD-aware clients can
            // introspect the numeric. Capability-denied is the one
            // exception — surfaced as 1001 in `data` but still `-32603`
            // at JSON-RPC level (no MCP code for capability semantics).
            let jsonrpc_code = -32603i32;
            let mut data = serde_json::Map::new();
            if let Some(c) = code {
                data.insert("atd_code".into(), Value::from(c));
            }
            if let Some(r) = retryable {
                data.insert("retryable".into(), Value::Bool(r));
            }
            if let Some(d) = details {
                data.insert("details".into(), d);
            }
            let err = JsonRpcError {
                code: jsonrpc_code,
                message,
            };
            // Inline `data` into the JSON-RPC error object as MCP spec
            // permits (extension field). axum::Json will serialise the
            // whole envelope.
            let envelope = JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(err),
            };
            let mut value = serde_json::to_value(&envelope).unwrap_or_else(|_| {
                json!({"jsonrpc":"2.0", "error": {"code": jsonrpc_code, "message": "serialise"}})
            });
            if !data.is_empty() {
                if let Some(err_obj) = value.get_mut("error").and_then(|e| e.as_object_mut()) {
                    err_obj.insert("data".into(), Value::Object(data));
                }
            }
            (StatusCode::OK, Json(value)).into_response()
        }
        // Hello / Pong / ToolList / ToolSchema cannot be produced by
        // `run_tool` — covered for completeness.
        other => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            id,
            -32603,
            format!("unexpected dispatch variant: {other:?}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_runtime::dispatch::SharedServerConfig;
    use atd_runtime::registry::{CallFuture, Registry, Tool};
    use atd_runtime::{CallContext, TierPolicy};
    use http_body_util::BodyExt;
    use std::sync::Arc;

    fn stub_def(id: &str) -> atd_protocol::ToolDefinition {
        use atd_protocol::{
            BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources, ToolSafety,
            ToolTier, ToolTrust, ToolVisibility, TrustLevel,
        };
        let _ = ToolTier::Warm; // ensure import path is correct
        atd_protocol::ToolDefinition {
            id: id.into(),
            name: id.into(),
            description: "stub".into(),
            version: "0.0.0".into(),
            capability: ToolCapability {
                domain: "echo".into(),
                actions: vec![],
                tags: vec![],
                intent_examples: vec![],
            },
            input_schema: json!({"type":"object"}),
            output_schema: json!({"type":"object"}),
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
        }
    }

    struct EchoStub {
        def: atd_protocol::ToolDefinition,
    }
    impl EchoStub {
        fn new() -> Self {
            Self {
                def: stub_def("ref:echo.say"),
            }
        }
    }
    impl Tool for EchoStub {
        fn definition(&self) -> &atd_protocol::ToolDefinition {
            &self.def
        }
        fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
            Box::pin(async move { Ok(json!({"echoed": args})) })
        }
    }

    fn state_with_echo() -> Arc<ServerState> {
        let mut reg = Registry::new();
        reg.register(Arc::new(EchoStub::new()));
        Arc::new(ServerState {
            registry: reg,
            config: SharedServerConfig::for_test(),
            tier_policy: TierPolicy::defaults(),
            middleware: vec![],
            metrics: Arc::new(atd_runtime::MetricsCounters::default()),
        })
    }

    async fn body_to_json(resp: axum::response::Response) -> Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn initialize_returns_server_info_with_tools_capability() {
        let resp = handle_initialize(Some(json!(1)), "atd-test 0.3.0", None);
        let body = body_to_json(resp).await;
        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["id"], 1);
        assert_eq!(body["result"]["serverInfo"]["name"], "atd-test");
        assert_eq!(body["result"]["serverInfo"]["version"], "0.3.0");
        assert!(body["result"]["capabilities"]["tools"].is_object());
        assert_eq!(body["result"]["protocolVersion"], "2025-06-18");
        // No broker → no `experimental.atd.acceptedTokenFormats`.
        assert!(body["result"]["capabilities"]["experimental"].is_null());
    }

    #[tokio::test]
    async fn initialize_echoes_broker_accepted_token_formats_when_wired() {
        use atd_runtime::secrets::{ResolveBearerFuture, ResolveFuture, TokenBroker};
        #[derive(Debug)]
        struct DeclaringBroker;
        impl TokenBroker for DeclaringBroker {
            fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
                Box::pin(async { Ok(None) })
            }
            fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
                Box::pin(async { Ok(None) })
            }
            fn accepted_token_formats(&self) -> &'static [&'static str] {
                &["ucan-jwt", "opaque", "ce-pairing-code"]
            }
        }
        let broker: Arc<dyn TokenBroker> = Arc::new(DeclaringBroker);
        let resp = handle_initialize(Some(json!(1)), "atd-test 0.3.0", Some(&broker));
        let body = body_to_json(resp).await;
        let formats =
            &body["result"]["capabilities"]["experimental"]["atd"]["acceptedTokenFormats"];
        assert_eq!(
            formats,
            &json!(["ucan-jwt", "opaque", "ce-pairing-code"]),
            "broker formats should appear in initialize.capabilities.experimental.atd"
        );
    }

    #[tokio::test]
    async fn initialize_omits_experimental_when_broker_declares_empty_formats() {
        use atd_runtime::secrets::{ResolveBearerFuture, ResolveFuture, TokenBroker};
        #[derive(Debug)]
        struct SilentBroker;
        impl TokenBroker for SilentBroker {
            fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
                Box::pin(async { Ok(None) })
            }
            fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
                Box::pin(async { Ok(None) })
            }
            // returns &[] (default impl) — listener should NOT emit
            // an empty acceptedTokenFormats array, just omit the field.
        }
        let broker: Arc<dyn TokenBroker> = Arc::new(SilentBroker);
        let resp = handle_initialize(Some(json!(1)), "atd-test 0.3.0", Some(&broker));
        let body = body_to_json(resp).await;
        assert!(body["result"]["capabilities"]["experimental"].is_null());
    }

    #[tokio::test]
    async fn tools_list_advertises_registered_tools() {
        let state = state_with_echo();
        let resp = handle_tools_list(Some(json!(2)), &state);
        let body = body_to_json(resp).await;
        let tools = body["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "ref:echo.say");
        assert!(tools[0]["inputSchema"].is_object());
    }

    #[tokio::test]
    async fn tools_call_echo_returns_isError_false_with_serialised_result() {
        let state = state_with_echo();
        let params = json!({"name": "ref:echo.say", "arguments": {"hi": "x"}});
        let resp = handle_tools_call(Some(json!(3)), &state, None, params).await;
        let body = body_to_json(resp).await;
        assert_eq!(body["result"]["isError"], false);
        let text = body["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["echoed"]["hi"], "x");
    }

    #[tokio::test]
    async fn tools_call_missing_name_returns_invalid_params_error() {
        let state = state_with_echo();
        let params = json!({"arguments": {}});
        let resp = handle_tools_call(Some(json!(4)), &state, None, params).await;
        let body = body_to_json(resp).await;
        assert_eq!(body["error"]["code"], -32602);
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("missing")
        );
    }

    #[tokio::test]
    async fn tools_call_unknown_tool_returns_error_envelope() {
        let state = state_with_echo();
        let params = json!({"name": "ref:does-not-exist", "arguments": {}});
        let resp = handle_tools_call(Some(json!(5)), &state, None, params).await;
        let body = body_to_json(resp).await;
        // Wrapped in JSON-RPC error (Response::Error → -32603 / tool not found message).
        assert_eq!(body["error"]["code"], -32603);
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("tool not found")
        );
    }
}
