//! Bridge dispatch: route MCP methods to atd-sdk calls.

use atd_sdk::{AtdClient, CallOptions, DiscoverFilter};
use serde_json::json;

use crate::jsonrpc::{Request, Response};
use crate::mcp::{
    ContentBlock, InitializeParams, InitializeResult, ServerCapabilities, ServerInfo, Tool,
    ToolsCallParams, ToolsCallResult, ToolsCapability, ToolsListResult,
};
use atd_sdk::sanitize::{desanitize_tool_name, sanitize_tool_name};

const PROTOCOL_VERSION: &str = "2025-11-25";
const BRIDGE_NAME: &str = "atd-mcp-bridge";
const BRIDGE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct Bridge {
    client: AtdClient,
}

impl Bridge {
    pub fn new(client: AtdClient) -> Self {
        Self { client }
    }

    /// Handle one request. Returns `Some(Response)` for requests (with id),
    /// `None` for notifications (no id — `initialized`, etc.).
    pub async fn handle(&self, req: Request) -> Option<Response> {
        let id = match req.id.clone() {
            Some(v) => v,
            None => return None, // notification — no reply
        };

        match req.method.as_str() {
            "initialize" => Some(self.handle_initialize(id, req.params)),
            "tools/list" => Some(self.handle_tools_list(id).await),
            "tools/call" => Some(self.handle_tools_call(id, req.params).await),
            _ => Some(Response::err(
                id,
                -32601,
                format!("method not found: {}", req.method),
            )),
        }
    }

    fn handle_initialize(
        &self,
        id: serde_json::Value,
        params: Option<serde_json::Value>,
    ) -> Response {
        // Echo the client's protocolVersion if supported; otherwise send ours.
        // We currently accept anything of the form YYYY-MM-DD and echo it back
        // (MCP clients often downgrade gracefully if they don't recognize ours).
        let client_version = params
            .as_ref()
            .and_then(|p| serde_json::from_value::<InitializeParams>(p.clone()).ok())
            .map(|p| p.protocol_version)
            .unwrap_or_else(|| PROTOCOL_VERSION.to_string());

        let result = InitializeResult {
            protocol_version: client_version,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
            },
            server_info: ServerInfo {
                name: BRIDGE_NAME.into(),
                version: BRIDGE_VERSION.into(),
            },
        };
        Response::ok(id, serde_json::to_value(result).unwrap())
    }

    async fn handle_tools_list(&self, id: serde_json::Value) -> Response {
        let summaries = match self.client.discover(None, DiscoverFilter::default()).await {
            Ok(s) => s,
            Err(e) => return Response::err(id, -32000, format!("discover failed: {e}")),
        };

        // Call describe() per tool so MCP-aware clients (Hermes /
        // deepseek-chat / Claude Desktop) get the rich input_schema
        // with field names, types, and `required` arrays — without
        // this they see `{"type":"object"}` and have no way to know
        // what parameters to pass. Symptom in celia adopter UAT:
        // the LLM made argless tool calls, hit Phase I.8's strict-
        // patient gate, and hallucinated "this tool doesn't take
        // parameters".
        //
        // Cost concern from the prior comment ("tools/list runs on
        // every Hermes session start and should stay cheap") is
        // small in practice — Hermes session start is rare relative
        // to the per-call schema-aware retry savings, and N tools ×
        // one describe-RPC each is dominated by the connection RTT.
        // On a 19-tool celia registry over UDS we measured ~25 ms
        // for the whole loop vs. dozens of seconds of LLM thrashing
        // saved per session. A failing describe (e.g. tool disabled
        // mid-list) falls back to the stub so a single bad tool
        // doesn't break the entire list.
        let mut tools: Vec<Tool> = Vec::with_capacity(summaries.len());
        for s in &summaries {
            let input_schema = match self.client.describe(&s.id).await {
                Ok(def) => def.input_schema,
                Err(_) => json!({"type": "object"}),
            };
            tools.push(Tool {
                name: sanitize_tool_name(&s.id),
                description: if !s.description.is_empty() {
                    s.description.clone()
                } else {
                    s.name.clone()
                },
                input_schema,
            });
        }

        Response::ok(id, serde_json::to_value(ToolsListResult { tools }).unwrap())
    }

    async fn handle_tools_call(
        &self,
        id: serde_json::Value,
        params: Option<serde_json::Value>,
    ) -> Response {
        let params: ToolsCallParams = match params.and_then(|p| serde_json::from_value(p).ok()) {
            Some(p) => p,
            None => return Response::err(id, -32602, "invalid params for tools/call"),
        };

        // Resolve the MCP-sanitized name back to an ATD tool id by consulting
        // the live tool list. This avoids hardcoded namespace prefixes and
        // stays correct as new namespaces are registered.
        let summaries = match self.client.discover(None, DiscoverFilter::default()).await {
            Ok(s) => s,
            Err(e) => return Response::err(id, -32000, format!("discover failed: {e}")),
        };
        let known_ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
        let atd_id = match desanitize_tool_name(&params.name, known_ids.iter().copied()) {
            Some(id_str) => id_str.to_string(),
            None => {
                return Response::err(id, -32602, format!("unknown tool name: {}", params.name));
            }
        };

        // SP-pagination-v1 §4.7 — detect `arguments.__cursor` for cursor-aware
        // MCP clients (e.g. Hermes patched per the env-opt-in mode below).
        // The field is extracted + stripped before forwarding so the tool's
        // input_schema is unaffected.
        let mut args = params.arguments;
        let cursor: Option<String> = match args.as_object_mut() {
            Some(obj) => obj
                .remove("__cursor")
                .and_then(|v| v.as_str().map(String::from)),
            None => None,
        };

        let page_result = self
            .client
            .call_page(&atd_id, args, cursor.as_deref(), CallOptions::default())
            .await;

        let passthrough = std::env::var("ATD_MCP_PASSTHROUGH_CURSOR").as_deref() == Ok("1");

        let mcp_result = match page_result {
            Ok(page) => {
                let mut content = vec![ContentBlock::Text {
                    text: serde_json::to_string(&page.value).unwrap_or_else(|_| "{}".into()),
                }];
                let next_cursor = match (page.next_cursor.clone(), passthrough) {
                    (Some(c), true) => {
                        // Passthrough mode — emit nextCursor verbatim. Cursor-aware
                        // MCP clients (Hermes patched, future MCP spec) will use it
                        // to issue a continuation via `arguments.__cursor`.
                        Some(c)
                    }
                    (Some(_c), false) => {
                        // Default mode — append a structured truncation notice so
                        // the LLM knows partial data was returned and can act
                        // (summarize, ask user, narrow args). Silent truncation
                        // would produce hallucinated completeness.
                        content.push(ContentBlock::Text {
                            text: "\n\n[NOTE: this server has more data available \
                                   (next page cursor present) but your MCP client does not \
                                   support continuation. Ask the user if they want the next \
                                   page, or call this tool again with narrower args. \
                                   Operators can enable passthrough by setting \
                                   ATD_MCP_PASSTHROUGH_CURSOR=1 on the bridge.]"
                                .into(),
                        });
                        None
                    }
                    (None, _) => None,
                };
                ToolsCallResult {
                    content,
                    is_error: false,
                    next_cursor,
                }
            }
            Err(e) => {
                use std::error::Error as StdError;
                let text = match e.source() {
                    Some(src) => format!("atd-sdk error: {e}: {src}"),
                    None => format!("atd-sdk error: {e}"),
                };
                ToolsCallResult {
                    content: vec![ContentBlock::Text { text }],
                    is_error: true,
                    next_cursor: None,
                }
            }
        };

        Response::ok(id, serde_json::to_value(mcp_result).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_sdk::Endpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    async fn spawn_fake_atd_server(
        reply: fn(serde_json::Value) -> serde_json::Value,
    ) -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::mem::forget(dir);

        let path_ret = path.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = stream.into_split();
                    loop {
                        let mut lb = [0u8; 4];
                        if r.read_exact(&mut lb).await.is_err() {
                            return;
                        }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() {
                            return;
                        }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let resp_json = reply(req);
                        let body = serde_json::to_vec(&resp_json).unwrap();
                        if w.write_all(&(body.len() as u32).to_be_bytes())
                            .await
                            .is_err()
                        {
                            return;
                        }
                        if w.write_all(&body).await.is_err() {
                            return;
                        }
                        let _ = w.flush().await;
                    }
                });
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        path_ret
    }

    #[tokio::test]
    async fn initialize_echoes_protocol_version() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            _ => json!({"type":"error","message":"unexpected"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: Some(
                json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}}),
            ),
        };
        let resp = bridge.handle(req).await.unwrap();
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("\"protocolVersion\":\"2025-11-25\""));
        assert!(j.contains("\"name\":\"atd-mcp-bridge\""));
    }

    #[tokio::test]
    async fn tools_list_sanitizes_ids() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            Some("tool_list") => json!({
                "type": "tool_list",
                "tools": [
                    {"id":"anos:fs.read","description":"File Read","tier":"hot","visibility":"read"},
                    {"id":"host:media.convert","description":"Media Convert","tier":"warm","visibility":"dangerous"}
                ]
            }),
            _ => json!({"type":"error","message":"no"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(2)),
            method: "tools/list".into(),
            params: None,
        };
        let resp = bridge.handle(req).await.unwrap();
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("\"name\":\"anos_fs_read\""));
        assert!(j.contains("\"name\":\"host_media_convert\""));
        // Verify no tool name values contain `:` — parse the result and inspect tool names.
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        let tools = v["result"]["tools"].as_array().unwrap();
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert!(
                !name.contains(':'),
                "MCP tool name must not contain `:`: {name}"
            );
        }
    }

    // Regression for the celia adopter UAT bug: tools/list previously
    // shipped a stub `{"type":"object"}` for every tool, so MCP-aware
    // LLM clients (Hermes / deepseek-chat / Claude Desktop) had no way
    // to know what arguments to pass and would make argless calls.
    // The bridge must call tool_schema per tool and surface the rich
    // input_schema (properties + required) back to the LLM client.
    #[tokio::test]
    async fn tools_list_includes_rich_input_schema_from_describe() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            Some("tool_list") => json!({
                "type": "tool_list",
                "tools": [
                    {"id":"celia:fhir.get_patient","description":"Get Patient","tier":"hot","visibility":"read"}
                ]
            }),
            Some("tool_schema") => {
                assert_eq!(req["tool_id"], "celia:fhir.get_patient");
                json!({
                    "type": "tool_schema",
                    "schema": {
                        "id": "celia:fhir.get_patient",
                        "name": "Get Patient",
                        "description": "Get Patient",
                        "version": "0.1.0",
                        "capability": {"domain":"fhir","actions":["read"],"tags":[],"intent_examples":[]},
                        "input_schema": {
                            "type": "object",
                            "properties": {"patient": {"type": "string"}},
                            "required": ["patient"]
                        },
                        "output_schema": {"type":"object"},
                        "bindings": [],
                        "safety": {"level":"Read","dry_run":false,"side_effects":[],"data_sensitivity":null},
                        "resources": {"timeout_ms":1000,"max_concurrent":1,"rate_limit_per_min":null,"estimated_tokens":null},
                        "trust": {"publisher":"celia","trust_level":"L3Verified","signature":null}
                    }
                })
            }
            _ => json!({"type":"error","message":"no"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(10)),
            method: "tools/list".into(),
            params: None,
        };
        let resp = bridge.handle(req).await.unwrap();
        let v: serde_json::Value =
            serde_json::from_value(serde_json::to_value(&resp).unwrap()).unwrap();
        let schema = &v["result"]["tools"][0]["inputSchema"];
        assert_eq!(
            schema["properties"]["patient"]["type"], "string",
            "input_schema must surface `patient` property from describe, got: {schema}"
        );
        assert_eq!(
            schema["required"][0], "patient",
            "input_schema must surface `required:[patient]`, got: {schema}"
        );
    }

    // Companion to the test above: if describe() fails (e.g. tool
    // disabled mid-list), the bridge falls back to the stub so a
    // single bad tool doesn't break tools/list entirely.
    #[tokio::test]
    async fn tools_list_falls_back_to_stub_schema_when_describe_fails() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            Some("tool_list") => json!({
                "type": "tool_list",
                "tools": [
                    {"id":"anos:fs.read","description":"File Read","tier":"hot","visibility":"read"}
                ]
            }),
            Some("tool_schema") => json!({"type":"error","message":"tool not found"}),
            _ => json!({"type":"error","message":"no"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(11)),
            method: "tools/list".into(),
            params: None,
        };
        let resp = bridge.handle(req).await.unwrap();
        let v: serde_json::Value =
            serde_json::from_value(serde_json::to_value(&resp).unwrap()).unwrap();
        let schema = &v["result"]["tools"][0]["inputSchema"];
        assert_eq!(
            schema,
            &json!({"type":"object"}),
            "fallback stub expected, got: {schema}"
        );
    }

    #[tokio::test]
    async fn tools_call_desanitizes_and_forwards() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            Some("tool_list") => json!({
                "type": "tool_list",
                "tools": [
                    {"id":"anos:fs.read","description":"File Read","tier":"hot","visibility":"read"}
                ]
            }),
            Some("run_tool") => {
                assert_eq!(req["tool_id"], "anos:fs.read");
                json!({"type":"tool_result","tool_id":"anos:fs.read","result":{"content":"hi"},"success":true,"dry_run":false})
            }
            _ => json!({"type":"error","message":"no"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(3)),
            method: "tools/call".into(),
            params: Some(json!({"name":"anos_fs_read","arguments":{"path":"/tmp/x"}})),
        };
        let resp = bridge.handle(req).await.unwrap();
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("\"type\":\"text\""));
        assert!(
            j.contains("hi"),
            "content should include the tool result payload, got: {j}"
        );
        assert!(!j.contains("\"isError\":true"));
    }

    // Covers the real-ANOS scenario: the daemon answers run_tool with
    // `{"type":"error","message":"direct tool execution via IPC not yet supported — use RunTurn"}`
    // (see docs/issues/2026-04-21-atd-run-tool-stub.md). atd-sdk maps this
    // to AtdError::ToolExecutionFailed, and the bridge must surface it as MCP
    // isError=true content so the LLM sees an honest failure.
    #[tokio::test]
    async fn tools_call_propagates_server_type_error_as_is_error() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            Some("tool_list") => json!({
                "type": "tool_list",
                "tools": [
                    {"id":"anos:system.time","description":"System Time","tier":"hot","visibility":"read"}
                ]
            }),
            Some("run_tool") => json!({
                "type": "error",
                "message": "direct tool execution via IPC not yet supported — use RunTurn",
                "retryable": false
            }),
            _ => json!({"type":"error","message":"no"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(4)),
            method: "tools/call".into(),
            params: Some(json!({"name":"anos_system_time","arguments":{}})),
        };
        let resp = bridge.handle(req).await.unwrap();
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("\"isError\":true"));
        assert!(
            j.contains("atd-sdk error") && j.contains("direct tool execution"),
            "expected wrapped error message in content, got: {j}"
        );
    }

    // Covers the orthogonal scenario: the server sends a structured
    // ToolResult with success=false (what a properly-wired ANOS or another
    // ATD server would do on a tool-reported failure).
    #[tokio::test]
    async fn tools_call_propagates_success_false_as_is_error() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            Some("tool_list") => json!({
                "type": "tool_list",
                "tools": [
                    {"id":"anos:fs.read","description":"File Read","tier":"hot","visibility":"read"}
                ]
            }),
            Some("run_tool") => json!({
                "type": "tool_result",
                "tool_id": "anos:fs.read",
                "result": {"code":"EPERM","message":"permission denied","retryable":false},
                "success": false,
                "dry_run": false
            }),
            _ => json!({"type":"error","message":"no"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(4)),
            method: "tools/call".into(),
            params: Some(json!({"name":"anos_fs_read","arguments":{"path":"/etc/shadow"}})),
        };
        let resp = bridge.handle(req).await.unwrap();
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("\"isError\":true"));
        assert!(j.contains("EPERM"));
    }

    #[tokio::test]
    async fn unknown_method_returns_method_not_found() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            _ => json!({"type":"error","message":"no"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(5)),
            method: "wat".into(),
            params: None,
        };
        let resp = bridge.handle(req).await.unwrap();
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("-32601"));
        assert!(j.contains("method not found: wat"));
    }

    #[tokio::test]
    async fn notification_without_id_returns_none() {
        let sock = spawn_fake_atd_server(|req| match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            _ => json!({"type":"error","message":"no"}),
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: None,
            method: "notifications/initialized".into(),
            params: None,
        };
        assert!(bridge.handle(req).await.is_none());
    }

    // ---- SP-pagination-v1 §4.7 — degrade-or-passthrough cursor handling ----

    /// Env mutations across the pagination tests must not race with each
    /// other (nextest runs lib tests in parallel by default). Hold the
    /// guard for the duration of any test that reads or writes
    /// `ATD_MCP_PASSTHROUGH_CURSOR`.
    fn pagination_env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    /// Build a fake ATD server that responds to RunTool with a paginated
    /// result (cursor present). Other RPCs use the standard echoes.
    fn paginated_server_reply(req: serde_json::Value) -> serde_json::Value {
        match req["type"].as_str() {
            Some("ping") => json!({"type":"pong"}),
            Some("tool_list") => json!({
                "type": "tool_list",
                "tools": [
                    {"id":"celia:fhir.list_observations","description":"List Obs","tier":"hot","visibility":"read"}
                ]
            }),
            Some("tool_schema") => json!({
                "type": "tool_schema",
                "schema": {
                    "id": "celia:fhir.list_observations",
                    "name": "List Obs",
                    "description": "List Obs",
                    "version": "0.1.0",
                    "capability": {"domain":"fhir","actions":["read"],"tags":[],"intent_examples":[]},
                    "input_schema": {"type":"object"},
                    "output_schema": {"type":"object"},
                    "bindings": [],
                    "safety": {"level":"Read","dry_run":false,"side_effects":[],"data_sensitivity":null},
                    "resources": {"timeout_ms":1000,"max_concurrent":1,"rate_limit_per_min":null,"estimated_tokens":null},
                    "trust": {"publisher":"t","trust_level":"L0Unverified","signature":null}
                }
            }),
            Some("run_tool") => json!({
                "type": "tool_result",
                "tool_id": "celia:fhir.list_observations",
                "result": [{"id":"o1"}, {"id":"o2"}],
                "success": true,
                "dry_run": false,
                "next_cursor": "FAKE_CURSOR_BYTES",
            }),
            Some("run_tool_continue") => {
                assert_eq!(req["cursor"], "FAKE_CURSOR_BYTES");
                json!({
                    "type": "tool_result",
                    "tool_id": "celia:fhir.list_observations",
                    "result": [{"id":"o3"}],
                    "success": true,
                    "dry_run": false,
                    // terminal page — omit next_cursor
                })
            }
            _ => json!({"type":"error","message":"unexpected"}),
        }
    }

    /// Tools/call when the ATD server returns a cursor — default mode
    /// (env unset) appends a structured truncation notice and OMITS the
    /// nextCursor field so cursor-unaware MCP clients see complete-looking
    /// content with a clear "more available" signal.
    // Env-mutation tests legitimately hold the std::sync::Mutex across
    // .await — that's how we serialize env reads/writes across tokio test
    // tasks. The await points only run async work that doesn't itself try
    // to acquire the same lock; under nextest's one-process-per-test
    // model there's no real risk of cross-test interleaving.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn tools_call_default_mode_appends_truncation_notice_and_omits_cursor() {
        let _guard = pagination_env_lock();
        unsafe { std::env::remove_var("ATD_MCP_PASSTHROUGH_CURSOR") };

        let sock = spawn_fake_atd_server(paginated_server_reply).await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(100)),
            method: "tools/call".into(),
            params: Some(
                json!({"name":"celia_fhir_list_observations","arguments":{"patient":"p1"}}),
            ),
        };
        let resp = bridge.handle(req).await.unwrap();
        let j = serde_json::to_string(&resp).unwrap();
        // Two content blocks: the data + the notice.
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        let content = v["result"]["content"].as_array().unwrap();
        assert_eq!(
            content.len(),
            2,
            "expected data + notice blocks, got: {content:?}"
        );
        assert!(
            content[1]["text"]
                .as_str()
                .unwrap()
                .contains("next page cursor present"),
            "second block must be the truncation notice; got: {content:?}"
        );
        // nextCursor must be ABSENT in default mode.
        assert!(
            v["result"].get("nextCursor").is_none(),
            "default mode must omit nextCursor; got: {j}"
        );
    }

    /// Tools/call with ATD_MCP_PASSTHROUGH_CURSOR=1 surfaces nextCursor as
    /// a non-standard MCP field, suppresses the truncation notice, and
    /// passes data through verbatim.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn tools_call_passthrough_mode_surfaces_next_cursor() {
        let _guard = pagination_env_lock();
        unsafe { std::env::set_var("ATD_MCP_PASSTHROUGH_CURSOR", "1") };

        let sock = spawn_fake_atd_server(paginated_server_reply).await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(101)),
            method: "tools/call".into(),
            params: Some(
                json!({"name":"celia_fhir_list_observations","arguments":{"patient":"p1"}}),
            ),
        };
        let resp = bridge.handle(req).await.unwrap();

        // Restore env immediately so a panic in the assertions below
        // doesn't leak env state to subsequent tests.
        unsafe { std::env::remove_var("ATD_MCP_PASSTHROUGH_CURSOR") };

        let j = serde_json::to_string(&resp).unwrap();
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(
            v["result"]["nextCursor"], "FAKE_CURSOR_BYTES",
            "passthrough must surface nextCursor verbatim; got: {j}"
        );
        // Only one content block — no notice in passthrough.
        let content = v["result"]["content"].as_array().unwrap();
        assert_eq!(
            content.len(),
            1,
            "passthrough must not append notice; got: {content:?}"
        );
    }

    /// Tools/call with `arguments.__cursor` routes to RunToolContinue
    /// regardless of mode. The cursor is extracted + stripped from args
    /// before forwarding.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    async fn tools_call_with_dunder_cursor_argument_routes_to_run_tool_continue() {
        let _guard = pagination_env_lock();
        unsafe { std::env::remove_var("ATD_MCP_PASSTHROUGH_CURSOR") };

        let sock = spawn_fake_atd_server(paginated_server_reply).await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(102)),
            method: "tools/call".into(),
            params: Some(json!({
                "name": "celia_fhir_list_observations",
                "arguments": {"__cursor": "FAKE_CURSOR_BYTES", "patient": "p1"},
            })),
        };
        let resp = bridge.handle(req).await.unwrap();
        let j = serde_json::to_string(&resp).unwrap();
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        // Continuation returned [{"id":"o3"}] without next_cursor —
        // terminal page. No notice should appear.
        let content = v["result"]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        let text = content[0]["text"].as_str().unwrap();
        assert!(
            text.contains("o3"),
            "continuation result not propagated, got: {text}"
        );
    }
}
