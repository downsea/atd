use atd_protocol::AtdError;
#[cfg(test)]
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::endpoint::Endpoint;
use atd_protocol::wire::{read_frame, write_frame};
use atd_protocol::{Request, Response};

/// Async ATD client.
///
/// Each request/response pair is serialized under an internal mutex so the
/// client is safe to clone across tasks by wrapping in `Arc<AtdClient>`.
pub struct AtdClient {
    inner: Mutex<Pipe>,
}

enum Pipe {
    Unix {
        read: tokio::net::unix::OwnedReadHalf,
        write: tokio::net::unix::OwnedWriteHalf,
    },
    /// Used only by in-crate tests.
    #[cfg(test)]
    Duplex {
        read: Box<dyn AsyncRead + Send + Unpin>,
        write: Box<dyn AsyncWrite + Send + Unpin>,
    },
}

impl AtdClient {
    pub async fn connect(endpoint: Endpoint) -> Result<Self, AtdError> {
        match endpoint {
            Endpoint::UnixSocket(path) => {
                let stream = UnixStream::connect(&path).await?;
                let (read, write) = stream.into_split();
                let client = AtdClient {
                    inner: Mutex::new(Pipe::Unix { read, write }),
                };
                client.ping().await?;
                Ok(client)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn from_duplex<R, W>(read: R, write: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        AtdClient {
            inner: Mutex::new(Pipe::Duplex {
                read: Box::new(read),
                write: Box::new(write),
            }),
        }
    }

    pub async fn ping(&self) -> Result<(), AtdError> {
        match self.request(&Request::Ping).await? {
            Response::Pong => Ok(()),
            other => Err(AtdError::ProtocolError {
                expected: "pong".into(),
                got: format!("{other:?}"),
            }),
        }
    }

    /// SP-12 Hello handshake. Declare the capabilities the client would
    /// like to hold on this connection; returns the subset the server
    /// actually granted.
    ///
    /// Back-compat: pre-SP-12 servers do not recognize `hello` and will
    /// typically respond with a wire error. This method demotes that to
    /// "no capabilities granted" (`Ok(vec![])`) so callers can treat the
    /// pre-SP-12 case identically to the fail-closed SP-12 case — a single
    /// `hello()` call works against any server version.
    pub async fn hello(
        &self,
        client_id: Option<&str>,
        requested: Vec<String>,
    ) -> Result<Vec<String>, AtdError> {
        let req = Request::Hello {
            client_id: client_id.map(|s| s.to_string()),
            requested_capabilities: requested,
            ucan_tokens: Vec::new(),
        };
        match self.request(&req).await {
            Ok(Response::HelloAck {
                granted_capabilities,
                ..
            }) => Ok(granted_capabilities),
            // Pre-SP-12 server: it doesn't know `hello`; it may reply with
            // a generic error. Demote to "no caps granted" rather than
            // failing — the caller can still call tools that declare no
            // required_capabilities.
            Ok(Response::Error { .. }) => Ok(vec![]),
            // Protocol-level failure (e.g. wire decode): same back-compat
            // treatment.
            Err(AtdError::ProtocolError { .. }) => Ok(vec![]),
            Ok(other) => Err(AtdError::ProtocolError {
                expected: "hello_ack".into(),
                got: format!("{other:?}"),
            }),
            Err(e) => Err(e),
        }
    }

    pub(crate) async fn request(&self, req: &Request) -> Result<Response, AtdError> {
        let mut guard = self.inner.lock().await;
        match &mut *guard {
            Pipe::Unix { read, write } => {
                write_frame(write, req).await?;
                let resp: Response = read_frame(read).await?;
                Ok(resp)
            }
            #[cfg(test)]
            Pipe::Duplex { read, write } => {
                write_frame(write, req).await?;
                let resp: Response = read_frame(read).await?;
                Ok(resp)
            }
        }
    }

    pub async fn discover(
        &self,
        query: Option<&str>,
        filter: crate::options::DiscoverFilter,
    ) -> Result<Vec<atd_protocol::ToolSummary>, AtdError> {
        let resp = self.request(&Request::ToolList).await?;
        let raw = match resp {
            Response::ToolListResponse { tools } => tools,
            Response::Error { message, .. } => {
                return Err(AtdError::ProtocolError {
                    expected: "tool_list".into(),
                    got: format!("error: {message}"),
                });
            }
            other => {
                return Err(AtdError::ProtocolError {
                    expected: "tool_list".into(),
                    got: format!("{other:?}"),
                });
            }
        };

        let arr = raw.as_array().ok_or_else(|| AtdError::ProtocolError {
            expected: "array of tool summaries".into(),
            got: format!("{raw}"),
        })?;

        let mut out: Vec<atd_protocol::ToolSummary> = Vec::with_capacity(arr.len());
        for v in arr {
            match serde_json::from_value::<atd_protocol::ToolSummary>(v.clone()) {
                Ok(s) => out.push(s),
                Err(_) => {
                    // Tolerate entries that are full ToolDefinitions by projecting down.
                    if let Ok(def) =
                        serde_json::from_value::<atd_protocol::ToolDefinition>(v.clone())
                    {
                        out.push(atd_protocol::ToolSummary::from(&def));
                    }
                }
            }
        }

        // Fill derived defaults for fields the server may omit (notably ANOS).
        for s in &mut out {
            if s.name.is_empty() {
                s.name = derive_name(s);
            }
            if s.domain.is_empty() {
                s.domain = derive_domain(&s.id);
            }
        }

        if let Some(q) = query {
            let q_lower = q.to_lowercase();
            out.retain(|s| {
                s.name.to_lowercase().contains(&q_lower)
                    || s.description.to_lowercase().contains(&q_lower)
                    || s.id.to_lowercase().contains(&q_lower)
            });
        }
        if let Some(d) = filter.domain.as_deref() {
            out.retain(|s| s.domain == d);
        }
        if let Some(v) = filter.visibility {
            out.retain(|s| s.visibility == v);
        }
        if let Some(t) = filter.tier {
            out.retain(|s| s.tier == t);
        }
        if let Some(n) = filter.limit {
            out.truncate(n);
        }

        Ok(out)
    }

    pub async fn describe(&self, tool_id: &str) -> Result<atd_protocol::ToolDefinition, AtdError> {
        let resp = self
            .request(&Request::ToolSchema {
                tool_id: tool_id.to_string(),
            })
            .await?;

        match resp {
            Response::ToolSchemaResponse { schema } => {
                serde_json::from_value(schema).map_err(|e| AtdError::ProtocolError {
                    expected: "ToolDefinition".into(),
                    got: format!("deserialize error: {e}"),
                })
            }
            Response::Error { message, .. } if message.to_lowercase().contains("not found") => {
                Err(AtdError::ToolNotFound {
                    tool_id: tool_id.to_string(),
                    suggestions: vec![],
                })
            }
            Response::Error { message, .. } => Err(AtdError::ProtocolError {
                expected: "tool_schema".into(),
                got: format!("error: {message}"),
            }),
            other => Err(AtdError::ProtocolError {
                expected: "tool_schema".into(),
                got: format!("{other:?}"),
            }),
        }
    }

    pub async fn call(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        opts: crate::options::CallOptions,
    ) -> Result<atd_protocol::ToolResult, AtdError> {
        let resp = self
            .request(&Request::RunTool {
                tool_id: tool_id.to_string(),
                args,
                dry_run: opts.dry_run,
            })
            .await?;

        match resp {
            Response::ToolResultResponse {
                tool_id: resp_tool_id,
                result,
                success,
                dry_run: _,
            } => {
                if success {
                    // Server returned raw data JSON. Metadata carries only the
                    // tool_id echoed by the server; timestamp/request_id/etc.
                    // remain None until the server populates them (tracked in
                    // the ANOS-side issue for run_tool metadata parity). The
                    // client must not synthesize values it doesn't have.
                    Ok(atd_protocol::ToolResult::Success {
                        data: result,
                        metadata: atd_protocol::ToolResultMetadata::for_tool(resp_tool_id),
                    })
                } else {
                    let (code, message, retryable) = extract_error(&result);
                    // Preserve the raw server payload so callers can inspect
                    // fields not covered by the canonical (code, message,
                    // retryable) extraction. Compact form keeps `reason`
                    // small when the payload already matches the canonical
                    // shape.
                    let reason = serde_json::to_string(&result).ok();
                    Ok(atd_protocol::ToolResult::Error {
                        code,
                        message,
                        reason,
                        retryable,
                    })
                }
            }
            // SP-12: server returns code=1001 for capability denial with
            // a `details` payload carrying `required` + `granted`. Surface
            // as the typed AtdError::CapabilityDenied so callers can catch
            // it without string-matching.
            Response::Error {
                message: _,
                code: Some(code),
                details,
                ..
            } if code == atd_protocol::ERR_CAPABILITY_DENIED => {
                let (required, granted) = extract_cap_denied_sets(details.as_ref());
                Err(AtdError::CapabilityDenied {
                    tool_id: tool_id.to_string(),
                    required,
                    granted,
                })
            }
            Response::Error {
                message, retryable, ..
            } => Err(AtdError::ToolExecutionFailed {
                tool_id: tool_id.to_string(),
                inner: Box::new(std::io::Error::other(format!(
                    "{message} (retryable={})",
                    retryable.unwrap_or(false)
                ))),
            }),
            other => Err(AtdError::ProtocolError {
                expected: "tool_result".into(),
                got: format!("{other:?}"),
            }),
        }
    }
}

/// Derive a display name if the server didn't send one.
/// Preference order: explicit name > description > id.
fn derive_name(s: &atd_protocol::ToolSummary) -> String {
    if !s.name.is_empty() {
        s.name.clone()
    } else if !s.description.is_empty() {
        s.description.clone()
    } else {
        s.id.clone()
    }
}

/// Derive domain from a tool id of form `<namespace>:<domain>.<action>[.<variant>]`.
/// Returns the empty string if parsing fails; callers can substitute a default.
fn derive_domain(id: &str) -> String {
    match id.split_once(':') {
        Some((_ns, rest)) => rest.split('.').next().unwrap_or("").to_string(),
        None => String::new(),
    }
}

/// Pull `required` + `granted` out of a `details` payload for
/// CAPABILITY_DENIED. Tolerant: missing / malformed fields become empty
/// vectors so the client surfaces whatever the server sent without
/// failing on its own.
fn extract_cap_denied_sets(details: Option<&serde_json::Value>) -> (Vec<String>, Vec<String>) {
    let Some(d) = details else {
        return (vec![], vec![]);
    };
    let to_vec = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let required = d.get("required").map(to_vec).unwrap_or_default();
    let granted = d.get("granted").map(to_vec).unwrap_or_default();
    (required, granted)
}

fn extract_error(value: &serde_json::Value) -> (String, String, bool) {
    let code = value
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN")
        .to_string();
    let message = value
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("tool call failed")
        .to_string();
    let retryable = value
        .get("retryable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    (code, message, retryable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    /// Spawn a task that acts as a one-shot server: reads exactly one request
    /// from the server-side of a duplex pipe, maps it to a scripted response.
    async fn spin_server<F>(server_end: tokio::io::DuplexStream, mut handler: F)
    where
        F: FnMut(Request) -> Response + Send + 'static,
    {
        let (mut read, mut write) = tokio::io::split(server_end);
        tokio::spawn(async move {
            while let Ok(req) = read_frame::<_, Request>(&mut read).await {
                let resp = handler(req);
                if write_frame(&mut write, &resp).await.is_err() {
                    break;
                }
            }
        });
    }

    #[tokio::test]
    async fn ping_returns_ok_when_server_sends_pong() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::Ping => Response::Pong,
            _ => Response::Error {
                message: "unexpected".into(),
                code: None,
                retryable: None,
                details: None,
            },
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        client.ping().await.unwrap();
    }

    #[tokio::test]
    async fn ping_errors_when_server_sends_wrong_response() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::ToolListResponse {
            tools: serde_json::json!([]),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let err = client.ping().await.unwrap_err();
        assert!(matches!(err, AtdError::ProtocolError { .. }));
    }

    #[tokio::test]
    async fn discover_projects_tool_definitions_to_summaries() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |req| match req {
            Request::ToolList => Response::ToolListResponse {
                tools: serde_json::json!([
                    {
                        "id": "anos:fs.read",
                        "name": "Read",
                        "description": "read a file",
                        "version": "0.1.0",
                        "capability": {
                            "domain": "fs",
                            "actions": ["read"],
                            "tags": ["filesystem"],
                            "intent_examples": []
                        },
                        "input_schema": {},
                        "output_schema": {},
                        "bindings": [{"protocol": "Cli", "config": {}}],
                        "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
                        "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
                        "trust": {"publisher": "anos", "trust_level": "L2Tested", "signature": null},
                        "visibility": "read"
                    }
                ]),
            },
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let summaries = client
            .discover(None, crate::options::DiscoverFilter::default())
            .await
            .unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "anos:fs.read");
        assert_eq!(summaries[0].domain, "fs");
    }

    #[tokio::test]
    async fn discover_applies_query_and_limit_client_side() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |_| Response::ToolListResponse {
            tools: serde_json::json!([
                {"id": "anos:fs.read", "name": "Read", "description": "read a file", "domain": "fs", "tags": []},
                {"id": "anos:fs.write", "name": "Write", "description": "write a file", "domain": "fs", "tags": []},
                {"id": "anos:web.fetch", "name": "Fetch", "description": "download a url", "domain": "web", "tags": []}
            ]),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);

        let only_fs = client
            .discover(
                Some("fs"),
                crate::options::DiscoverFilter {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(only_fs.len(), 1);
        assert!(only_fs[0].id.starts_with("anos:fs"));
    }

    fn tool_def_json() -> serde_json::Value {
        serde_json::json!({
            "id": "anos:fs.read",
            "name": "Read",
            "description": "read a file",
            "version": "0.1.0",
            "capability": {
                "domain": "fs", "actions": ["read"], "tags": [], "intent_examples": []
            },
            "input_schema": {"type": "object"},
            "output_schema": {"type": "string"},
            "bindings": [{"protocol": "Cli", "config": {}}],
            "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
            "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
            "trust": {"publisher": "anos", "trust_level": "L2Tested", "signature": null},
            "visibility": "read"
        })
    }

    #[tokio::test]
    async fn describe_returns_full_tool_definition() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |req| match req {
            Request::ToolSchema { tool_id } => {
                assert_eq!(tool_id, "anos:fs.read");
                Response::ToolSchemaResponse {
                    schema: tool_def_json(),
                }
            }
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let def = client.describe("anos:fs.read").await.unwrap();
        assert_eq!(def.id, "anos:fs.read");
        assert_eq!(def.capability.domain, "fs");
    }

    #[tokio::test]
    async fn describe_maps_not_found_error_to_tool_not_found() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::Error {
            message: "tool not found: anos:nope".into(),
            code: None,
            retryable: None,
            details: None,
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let err = client.describe("anos:nope").await.unwrap_err();
        assert!(matches!(err, AtdError::ToolNotFound { .. }));
    }

    #[tokio::test]
    async fn call_success_returns_tool_result_success() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |req| match req {
            Request::RunTool {
                tool_id,
                args,
                dry_run,
            } => {
                assert_eq!(tool_id, "anos:fs.read");
                assert_eq!(args["path"], "/tmp/x");
                assert!(!dry_run);
                Response::ToolResultResponse {
                    tool_id,
                    result: serde_json::json!({"content": "ok"}),
                    success: true,
                    dry_run: false,
                }
            }
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let r = client
            .call(
                "anos:fs.read",
                serde_json::json!({"path": "/tmp/x"}),
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap();
        assert!(r.is_success());
        assert_eq!(r.data().unwrap()["content"], "ok");
    }

    #[tokio::test]
    async fn call_failure_returns_tool_result_error() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::ToolResultResponse {
            tool_id: "anos:fs.read".into(),
            result: serde_json::json!({"code": "EPERM", "message": "no", "retryable": false}),
            success: false,
            dry_run: false,
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let r = client
            .call(
                "anos:fs.read",
                serde_json::json!({}),
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap();
        match r {
            atd_protocol::ToolResult::Error { code, .. } => assert_eq!(code, "EPERM"),
            _ => panic!("expected error variant"),
        }
    }

    #[tokio::test]
    async fn call_failure_preserves_raw_payload_in_reason() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::ToolResultResponse {
            tool_id: "anos:fs.read".into(),
            // Payload has NO `code`/`message`/`retryable`; it's an opaque server
            // shape. Without `reason`, the info would be lost.
            result: serde_json::json!({"unexpected": {"nested": [1, 2, 3]}, "hint": "quota exceeded"}),
            success: false,
            dry_run: false,
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let r = client
            .call(
                "anos:fs.read",
                serde_json::json!({}),
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap();
        match r {
            atd_protocol::ToolResult::Error {
                code,
                message,
                reason,
                retryable,
            } => {
                assert_eq!(code, "UNKNOWN"); // defaults used for structured extraction
                assert_eq!(message, "tool call failed");
                assert!(!retryable);
                let reason = reason.expect("reason must carry the raw payload");
                assert!(
                    reason.contains("\"quota exceeded\""),
                    "reason should preserve hint, got: {reason}"
                );
                assert!(
                    reason.contains("\"unexpected\""),
                    "reason should preserve unknown keys, got: {reason}"
                );
            }
            _ => panic!("expected error variant"),
        }
    }

    #[tokio::test]
    async fn call_forwards_dry_run_flag() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::RunTool { dry_run, .. } => {
                assert!(dry_run);
                Response::ToolResultResponse {
                    tool_id: "anos:fs.read".into(),
                    result: serde_json::json!({}),
                    success: true,
                    dry_run: true,
                }
            }
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        client
            .call(
                "anos:fs.read",
                serde_json::json!({}),
                crate::options::CallOptions {
                    dry_run: true,
                    preferred_binding: None,
                },
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn discover_fills_name_and_domain_from_id_when_missing() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |_| Response::ToolListResponse {
            tools: serde_json::json!([
                {"id":"anos:fs.read","description":"File Read","tier":"hot","visibility":"read","lifecycle":"Active"},
                {"id":"anos:web.search","description":"Web Search","tier":"hot","visibility":"read"},
                {"id":"host:media.convert","description":"","tier":"warm","visibility":"dangerous"}
            ]),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let summaries = client
            .discover(None, crate::options::DiscoverFilter::default())
            .await
            .unwrap();
        assert_eq!(summaries.len(), 3);

        // name ← description when provided
        assert_eq!(summaries[0].id, "anos:fs.read");
        assert_eq!(summaries[0].name, "File Read");
        assert_eq!(summaries[0].domain, "fs");

        // web.search → domain "web"
        assert_eq!(summaries[1].domain, "web");

        // host:media.convert → domain "media", and name falls back to id when both name and description empty
        assert_eq!(summaries[2].domain, "media");
        assert_eq!(summaries[2].name, "host:media.convert");
    }

    // ---- SP-12 additions ----

    #[tokio::test]
    async fn hello_returns_granted_subset_from_server() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::Hello {
                client_id,
                requested_capabilities,
                ..
            } => {
                assert_eq!(client_id.as_deref(), Some("test"));
                assert_eq!(requested_capabilities, vec!["exec", "admin"]);
                Response::HelloAck {
                    granted_capabilities: vec!["exec".into()],
                    server_version: "atd-ref-server 0.2.0".into(),
                    supported_tiers: vec!["hot".into(), "warm".into(), "cold".into()],
                }
            }
            _ => unreachable!(),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let granted = client
            .hello(Some("test"), vec!["exec".into(), "admin".into()])
            .await
            .unwrap();
        assert_eq!(granted, vec!["exec"]);
    }

    #[tokio::test]
    async fn hello_degrades_to_empty_caps_on_pre_sp12_server_error() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::Hello { .. } => Response::Error {
                message: "unknown request".into(),
                code: None,
                retryable: None,
                details: None,
            },
            _ => unreachable!(),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let granted = client.hello(None, vec!["exec".into()]).await.unwrap();
        assert!(granted.is_empty(), "pre-SP-12 server → empty grant");
    }

    #[tokio::test]
    async fn call_surfaces_capability_denied_with_both_sets() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::RunTool { .. } => Response::Error {
                message: "capability denied for ref:x: missing [\"exec\"]".into(),
                code: Some(atd_protocol::ERR_CAPABILITY_DENIED),
                retryable: Some(false),
                details: Some(serde_json::json!({
                    "required": ["exec"],
                    "granted": [],
                    "missing": ["exec"],
                })),
            },
            _ => unreachable!(),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let err = client
            .call(
                "ref:x",
                serde_json::json!({}),
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap_err();
        match err {
            AtdError::CapabilityDenied {
                tool_id,
                required,
                granted,
            } => {
                assert_eq!(tool_id, "ref:x");
                assert_eq!(required, vec!["exec"]);
                assert!(granted.is_empty());
            }
            other => panic!("expected CapabilityDenied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn call_non_capability_error_still_maps_to_tool_execution_failed() {
        // Regression: pre-existing error shape (no code, or non-1001 code)
        // must continue to map to ToolExecutionFailed.
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::Error {
            message: "something else".into(),
            code: Some(500),
            retryable: Some(true),
            details: None,
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let err = client
            .call(
                "ref:x",
                serde_json::json!({}),
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, AtdError::ToolExecutionFailed { .. }),
            "non-1001 errors must still be ToolExecutionFailed, got {err:?}"
        );
    }
}
