//! Bridge dispatch: route MCP methods to atd-client calls.

use atd_client::{AtdClient, CallOptions, DiscoverFilter};
use serde_json::json;

use crate::jsonrpc::{Request, Response};
use crate::mcp::{
    ContentBlock, InitializeParams, InitializeResult, ServerCapabilities, ServerInfo, Tool,
    ToolsCallParams, ToolsCallResult, ToolsCapability, ToolsListResult,
};
use atd_client::sanitize::{desanitize_tool_name, sanitize_tool_name};

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
            _ => Some(Response::err(id, -32601, format!("method not found: {}", req.method))),
        }
    }

    fn handle_initialize(&self, id: serde_json::Value, params: Option<serde_json::Value>) -> Response {
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
        let summaries = match self
            .client
            .discover(None, DiscoverFilter::default())
            .await
        {
            Ok(s) => s,
            Err(e) => return Response::err(id, -32000, format!("discover failed: {e}")),
        };

        let tools: Vec<Tool> = summaries
            .iter()
            .map(|s| Tool {
                name: sanitize_tool_name(&s.id),
                description: if !s.description.is_empty() {
                    s.description.clone()
                } else {
                    s.name.clone()
                },
                // We ship a minimal stub schema here. A richer version would
                // call describe() for each tool and map input_schema — but
                // tools/list runs on every Hermes session start and should
                // stay cheap. Per-tool schemas are lazily loaded only when
                // the LLM needs to call the tool (see handle_tools_call).
                input_schema: json!({"type": "object"}),
            })
            .collect();

        Response::ok(id, serde_json::to_value(ToolsListResult { tools }).unwrap())
    }

    async fn handle_tools_call(&self, id: serde_json::Value, params: Option<serde_json::Value>) -> Response {
        let params: ToolsCallParams = match params.and_then(|p| serde_json::from_value(p).ok()) {
            Some(p) => p,
            None => return Response::err(id, -32602, "invalid params for tools/call"),
        };

        // Resolve the MCP-sanitized name back to an ATD tool id by consulting
        // the live tool list. This avoids hardcoded namespace prefixes and
        // stays correct as new namespaces are registered.
        let summaries = match self
            .client
            .discover(None, DiscoverFilter::default())
            .await
        {
            Ok(s) => s,
            Err(e) => return Response::err(id, -32000, format!("discover failed: {e}")),
        };
        let known_ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
        let atd_id = match desanitize_tool_name(&params.name, known_ids.iter().copied()) {
            Some(id_str) => id_str.to_string(),
            None => {
                return Response::err(
                    id,
                    -32602,
                    format!("unknown tool name: {}", params.name),
                )
            }
        };
        let result = self
            .client
            .call(&atd_id, params.arguments, CallOptions::default())
            .await;

        let mcp_result = match result {
            Ok(atd_types::ToolResult::Success { data, .. }) => ToolsCallResult {
                content: vec![ContentBlock::Text {
                    text: serde_json::to_string(&data).unwrap_or_else(|_| "{}".into()),
                }],
                is_error: false,
            },
            Ok(atd_types::ToolResult::Error { code, message, .. }) => ToolsCallResult {
                content: vec![ContentBlock::Text {
                    text: format!("[{code}] {message}"),
                }],
                is_error: true,
            },
            Err(e) => {
                use std::error::Error as StdError;
                let text = match e.source() {
                    Some(src) => format!("atd-client error: {e}: {src}"),
                    None => format!("atd-client error: {e}"),
                };
                ToolsCallResult {
                    content: vec![ContentBlock::Text { text }],
                    is_error: true,
                }
            }
        };

        Response::ok(id, serde_json::to_value(mcp_result).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_client::Endpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    async fn spawn_fake_atd_server(reply: fn(serde_json::Value) -> serde_json::Value) -> std::path::PathBuf {
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
                        if r.read_exact(&mut lb).await.is_err() { return; }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() { return; }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let resp_json = reply(req);
                        let body = serde_json::to_vec(&resp_json).unwrap();
                        if w.write_all(&(body.len() as u32).to_be_bytes()).await.is_err() { return; }
                        if w.write_all(&body).await.is_err() { return; }
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
        let sock = spawn_fake_atd_server(|req| {
            match req["type"].as_str() {
                Some("ping") => json!({"type":"pong"}),
                _ => json!({"type":"error","message":"unexpected"}),
            }
        })
        .await;

        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let bridge = Bridge::new(client);
        let req = Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "initialize".into(),
            params: Some(json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"t","version":"0"}})),
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
            assert!(!name.contains(':'), "MCP tool name must not contain `:`: {name}");
        }
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
        assert!(j.contains("hi"), "content should include the tool result payload, got: {j}");
        assert!(!j.contains("\"isError\":true"));
    }

    // Covers the real-ANOS scenario: the daemon answers run_tool with
    // `{"type":"error","message":"direct tool execution via IPC not yet supported — use RunTurn"}`
    // (see docs/issues/2026-04-21-atd-run-tool-stub.md). atd-client maps this
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
            j.contains("atd-client error") && j.contains("direct tool execution"),
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
}
