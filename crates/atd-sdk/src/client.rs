use std::time::Duration;

use atd_protocol::AtdError;
#[cfg(test)]
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::ConnectOptions;
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
    /// Connect with default retry behaviour (read from env, see
    /// [`ConnectOptions`]). For explicit control use
    /// [`AtdClient::connect_with_options`].
    pub async fn connect(endpoint: Endpoint) -> Result<Self, AtdError> {
        Self::connect_with_options(endpoint, ConnectOptions::default()).await
    }

    /// SP-concurrency-baseline §5.3 — connect with explicit retry policy.
    ///
    /// Retries on transient errors (refused / reset / would-block / timeout)
    /// with exponential backoff capped at `opts.backoff_cap_ms` and ±20%
    /// jitter to break lockstep retries during a spawn-storm. Short-circuits
    /// on truly fatal errors (`NotFound`, `PermissionDenied`) where retry
    /// cannot help. Each attempt is wrapped in
    /// `tokio::time::timeout(opts.connect_timeout_ms)` so a stalled
    /// `connect()` + `ping()` round-trip surfaces as a retryable error
    /// instead of hanging.
    pub async fn connect_with_options(
        endpoint: Endpoint,
        opts: ConnectOptions,
    ) -> Result<Self, AtdError> {
        let mut delay_ms = opts.backoff_base_ms;
        let mut last_err: Option<AtdError> = None;
        for attempt in 0..opts.max_attempts {
            let attempt_fut = Self::connect_once(&endpoint);
            let result =
                tokio::time::timeout(Duration::from_millis(opts.connect_timeout_ms), attempt_fut)
                    .await
                    .unwrap_or_else(|_| {
                        Err(AtdError::ServerUnreachable(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            format!(
                                "connect attempt timed out after {}ms",
                                opts.connect_timeout_ms
                            ),
                        )))
                    });
            match result {
                Ok(client) => return Ok(client),
                Err(e) if is_fatal_connect_error(&e) => return Err(e),
                Err(e) => {
                    last_err = Some(e);
                    if attempt + 1 < opts.max_attempts {
                        let jitter_pct = jitter_factor(); // ±0.2
                        let wait_ms = (delay_ms as f64 * (1.0 + jitter_pct)).max(1.0) as u64;
                        tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                        delay_ms = (delay_ms.saturating_mul(2)).min(opts.backoff_cap_ms);
                    }
                }
            }
        }
        Err(last_err.expect("loop runs at least once"))
    }

    async fn connect_once(endpoint: &Endpoint) -> Result<Self, AtdError> {
        match endpoint {
            Endpoint::UnixSocket(path) => {
                let stream = UnixStream::connect(path).await?;
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
        self.hello_with_ucan_tokens(client_id, requested, Vec::new())
            .await
    }

    /// SP-capability-v2 extension of [`Self::hello`].
    ///
    /// Same handshake but also presents `ucan_tokens` — each entry a
    /// UCAN-lite JWT compact form. The server verifies each chain
    /// independently and replies with `granted_capabilities` =
    /// `(server_allow_list ∩ requested) ∪ ucan_derived_caps`.
    ///
    /// On verifier failures the server returns a `Response::Error`
    /// with one of codes `1010` (ERR_UCAN_INVALID) / `1011`
    /// (ERR_UCAN_EXPIRED) / `1012` (ERR_DELEGATION_TOO_DEEP) / `1013`
    /// (ERR_AUDIENCE_MISMATCH); this method surfaces that as
    /// [`AtdError`] for the caller to inspect, **without** the
    /// "pre-SP-12 demote to empty" fallback `hello()` uses — UCAN
    /// failures are deterministic and the caller wants to know.
    pub async fn hello_with_ucan_tokens(
        &self,
        client_id: Option<&str>,
        requested: Vec<String>,
        ucan_tokens: Vec<String>,
    ) -> Result<Vec<String>, AtdError> {
        let presenting_ucan = !ucan_tokens.is_empty();
        let req = Request::Hello {
            client_id: client_id.map(|s| s.to_string()),
            requested_capabilities: requested,
            ucan_tokens,
        };
        match self.request(&req).await {
            Ok(Response::HelloAck {
                granted_capabilities,
                ..
            }) => Ok(granted_capabilities),
            Ok(Response::Error { message, code, .. }) if presenting_ucan => {
                // SP-capability-v2: surface UCAN verifier failures
                // verbatim — they are deterministic; the back-compat
                // demote to empty would hide the actual problem from
                // the caller. ProtocolError is the closest fit in the
                // current AtdError taxonomy; the wire code lands in
                // `got` so callers can match on 1010-1013.
                Err(AtdError::ProtocolError {
                    expected: "hello_ack with verified UCAN".into(),
                    got: format!("server error code={code:?} message={message}"),
                })
            }
            // Pre-SP-12 server: it doesn't know `hello`; it may reply with
            // a generic error. Demote to "no caps granted" rather than
            // failing — the caller can still call tools that declare no
            // required_capabilities. Only applied when not presenting UCAN.
            Ok(Response::Error { .. }) => Ok(vec![]),
            Err(AtdError::ProtocolError { .. }) if !presenting_ucan => Ok(vec![]),
            Err(AtdError::ProtocolError { .. }) => Err(AtdError::ProtocolError {
                expected: "hello_ack with verified UCAN".into(),
                got: "protocol error".into(),
            }),
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

    /// SP-pagination-v1 §4.8 — fetch one page of a paginated tool result.
    ///
    /// On the initial call, pass `cursor = None`. The server's response
    /// carries `next_cursor`; pass it verbatim to the next `call_page` to
    /// fetch the subsequent page. Cursors are opaque — clients MUST NOT
    /// parse them.
    ///
    /// For tools that don't paginate, the first page returns `next_cursor: None`
    /// and the response shape is identical to `call`.
    pub async fn call_page(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        cursor: Option<&str>,
        opts: crate::options::CallOptions,
    ) -> Result<crate::options::PaginatedSdkResult, AtdError> {
        let req = match cursor {
            None => Request::RunTool {
                tool_id: tool_id.to_string(),
                args,
                dry_run: opts.dry_run,
            },
            Some(c) => Request::RunToolContinue {
                tool_id: tool_id.to_string(),
                cursor: c.to_string(),
            },
        };
        let resp = self.request(&req).await?;
        match resp {
            Response::ToolResultResponse {
                result,
                success,
                next_cursor,
                ..
            } => {
                if success {
                    Ok(crate::options::PaginatedSdkResult {
                        value: result,
                        next_cursor,
                    })
                } else {
                    let (code, message, retryable) = extract_error(&result);
                    Err(AtdError::ToolExecutionFailed {
                        tool_id: tool_id.to_string(),
                        inner: Box::new(std::io::Error::other(format!(
                            "{code} {message} (retryable={retryable})"
                        ))),
                    })
                }
            }
            Response::Error {
                message,
                code,
                retryable,
                ..
            } => Err(AtdError::ToolExecutionFailed {
                tool_id: tool_id.to_string(),
                inner: Box::new(std::io::Error::other(format!(
                    "server error code={code:?} retryable={retryable:?}: {message}"
                ))),
            }),
            other => Err(AtdError::ProtocolError {
                expected: "tool_result".into(),
                got: format!("{other:?}"),
            }),
        }
    }

    /// SP-pagination-v1 §4.8 — fetch all pages via auto-loop, merging per
    /// the configured `MergePolicy`. Aborts with `PaginationLimitExceeded`
    /// if `max_pages` or `max_total_bytes` is hit before exhaustion.
    pub async fn call_all(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        opts: crate::options::CallAllOptions,
    ) -> Result<serde_json::Value, AtdError> {
        let mut accumulated: Option<serde_json::Value> = None;
        let mut bytes_total: usize = 0;
        let mut cursor: Option<String> = None;
        for page_idx in 0..opts.max_pages {
            let page_args = if page_idx == 0 {
                args.clone()
            } else {
                serde_json::Value::Null
            };
            let page = self
                .call_page(
                    tool_id,
                    page_args,
                    cursor.as_deref(),
                    crate::options::CallOptions::default(),
                )
                .await?;
            let page_bytes = serde_json::to_vec(&page.value)
                .map(|v| v.len())
                .unwrap_or(0);
            bytes_total += page_bytes;
            if bytes_total > opts.max_total_bytes {
                return Err(AtdError::PaginationLimitExceeded {
                    pages_fetched: page_idx + 1,
                    bytes_fetched: bytes_total,
                });
            }
            accumulated = Some(merge_pages(accumulated, page.value, &opts.merge_policy)?);
            match page.next_cursor {
                Some(c) => cursor = Some(c),
                None => return Ok(accumulated.unwrap_or(serde_json::Value::Null)),
            }
        }
        Err(AtdError::PaginationLimitExceeded {
            pages_fetched: opts.max_pages,
            bytes_fetched: bytes_total,
        })
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
                next_cursor: _,
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

/// SP-pagination-v1 §4.8 — combine a new page with the accumulator per
/// the chosen `MergePolicy`. Returns the new accumulator.
fn merge_pages(
    accumulated: Option<serde_json::Value>,
    page: serde_json::Value,
    policy: &crate::options::MergePolicy,
) -> Result<serde_json::Value, AtdError> {
    use crate::options::MergePolicy;
    match (accumulated, policy) {
        // First page: just take the page verbatim.
        (None, _) => Ok(page),
        (Some(acc), MergePolicy::FirstPageOnly) => {
            // First page wins; the new page is dropped silently. The
            // call_all loop still bumps page_idx for byte / page caps so
            // a misbehaving server emitting forever-cursors still triggers
            // PaginationLimitExceeded.
            let _ = page;
            Ok(acc)
        }
        (Some(acc), MergePolicy::ConcatArray) => match (acc, page) {
            (serde_json::Value::Array(mut a), serde_json::Value::Array(b)) => {
                a.extend(b);
                Ok(serde_json::Value::Array(a))
            }
            _ => Err(AtdError::MergeFailed {
                reason: "ConcatArray requires every page to be a JSON array".into(),
            }),
        },
        (Some(acc), MergePolicy::ConcatField(field)) => {
            let acc_obj = match acc {
                serde_json::Value::Object(m) => m,
                _ => {
                    return Err(AtdError::MergeFailed {
                        reason: format!(
                            "ConcatField({field}) requires every page to be a JSON object"
                        ),
                    });
                }
            };
            let mut page_obj = match page {
                serde_json::Value::Object(m) => m,
                _ => {
                    return Err(AtdError::MergeFailed {
                        reason: format!("ConcatField({field}) page is not a JSON object"),
                    });
                }
            };
            let acc_arr =
                acc_obj
                    .get(field.as_str())
                    .cloned()
                    .ok_or_else(|| AtdError::MergeFailed {
                        reason: format!("ConcatField({field}): field missing in accumulator"),
                    })?;
            let page_arr =
                page_obj
                    .get(field.as_str())
                    .cloned()
                    .ok_or_else(|| AtdError::MergeFailed {
                        reason: format!("ConcatField({field}): field missing in page"),
                    })?;
            let combined = match (acc_arr, page_arr) {
                (serde_json::Value::Array(mut a), serde_json::Value::Array(b)) => {
                    a.extend(b);
                    serde_json::Value::Array(a)
                }
                _ => {
                    return Err(AtdError::MergeFailed {
                        reason: format!("ConcatField({field}) is not an array"),
                    });
                }
            };
            // Last page's other fields win — metadata totals etc. that
            // don't change across pages stay consistent.
            page_obj.insert(field.clone(), combined);
            Ok(serde_json::Value::Object(page_obj))
        }
    }
}

/// SP-concurrency-baseline §5.3 — distinguish "this attempt is hopeless"
/// from "transient; back off and retry." Hopeless errors short-circuit
/// the retry loop so a typo'd socket path doesn't waste five attempts.
fn is_fatal_connect_error(err: &AtdError) -> bool {
    matches!(
        err,
        AtdError::ServerUnreachable(io) if matches!(
            io.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
        )
    )
}

/// Returns a jitter factor in the range `[-0.2, 0.2]` derived from the
/// system clock's subsecond nanos. Used to spread otherwise-synchronized
/// retries during a spawn-storm; the noise source doesn't need to be
/// cryptographically random — it just needs to break lockstep.
fn jitter_factor() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    ((nanos % 1000) as f64 / 1000.0 - 0.5) * 0.4
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
                    next_cursor: None,
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
            next_cursor: None,
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
            next_cursor: None,
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
                    next_cursor: None,
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

    // ---- SP-concurrency-baseline §5.3 connect retry tests ----

    /// Spawn a UnixListener that accepts and immediately drops each stream,
    /// counting accepts. AtdClient::connect_with_options should see EOF on
    /// its ping, treat it as retryable, and try again.
    async fn spawn_immediate_close_listener() -> (
        std::path::PathBuf,
        std::sync::Arc<std::sync::atomic::AtomicU32>,
    ) {
        use std::sync::atomic::{AtomicU32, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("close.sock");
        let counter = std::sync::Arc::new(AtomicU32::new(0));
        let counter_for_task = counter.clone();
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        std::mem::forget(dir); // keep the path alive
        let path_ret = path.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                counter_for_task.fetch_add(1, Ordering::Relaxed);
                drop(stream); // close immediately → ping read sees EOF
            }
        });
        // Give the listener a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        (path_ret, counter)
    }

    #[tokio::test]
    async fn connect_retries_on_transient_failure() {
        let (path, accepts) = spawn_immediate_close_listener().await;
        let opts = ConnectOptions {
            max_attempts: 3,
            backoff_base_ms: 5,
            backoff_cap_ms: 20,
            connect_timeout_ms: 500,
        };
        let result = AtdClient::connect_with_options(Endpoint::unix(path), opts).await;
        assert!(
            result.is_err(),
            "connect should fail when listener closes streams"
        );
        // 3 attempts → listener accepted 3 times.
        let n = accepts.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(n, 3, "expected 3 connect attempts, listener saw {n}");
    }

    #[tokio::test]
    async fn connect_respects_max_attempts() {
        let (path, accepts) = spawn_immediate_close_listener().await;
        let opts = ConnectOptions {
            max_attempts: 5,
            backoff_base_ms: 5,
            backoff_cap_ms: 20,
            connect_timeout_ms: 500,
        };
        let _ = AtdClient::connect_with_options(Endpoint::unix(path), opts).await;
        let n = accepts.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            n, 5,
            "max_attempts=5 should yield exactly 5 attempts, got {n}"
        );
    }

    #[tokio::test]
    async fn connect_short_circuits_on_not_found() {
        // No socket at this path → UnixStream::connect returns NotFound,
        // which is fatal — the retry loop must not even sleep.
        let opts = ConnectOptions {
            max_attempts: 5,
            backoff_base_ms: 100, // long enough that retries would be obvious
            backoff_cap_ms: 100,
            connect_timeout_ms: 500,
        };
        let started = std::time::Instant::now();
        let result = AtdClient::connect_with_options(
            Endpoint::unix("/tmp/atd-sdk-test-no-such-socket-xy7q"),
            opts,
        )
        .await;
        let elapsed = started.elapsed();
        match result {
            Err(AtdError::ServerUnreachable(_)) => {}
            Err(other) => panic!("expected ServerUnreachable, got {other:?}"),
            Ok(_) => panic!("connect to nonexistent path should not succeed"),
        }
        assert!(
            elapsed < std::time::Duration::from_millis(80),
            "short-circuit should be near-instant, took {elapsed:?}"
        );
    }

    /// `ConnectOptions::default()` reads env vars at call time. We use the
    /// `serial_test`-style technique of saving + restoring env to avoid
    /// cross-test races (other tests may run concurrently in the same
    /// process).
    #[test]
    fn connect_options_default_reads_env() {
        // Capture original values for restore.
        let orig = (
            std::env::var("ATD_CONNECT_RETRIES").ok(),
            std::env::var("ATD_CONNECT_BACKOFF_BASE_MS").ok(),
        );
        // Safety: these env mutations may race with parallel tests reading
        // the same vars. To minimise the window we set + read + restore in
        // one synchronous block; the test asserts the read reflected our
        // write at the moment we read it.
        unsafe {
            std::env::set_var("ATD_CONNECT_RETRIES", "2");
            std::env::set_var("ATD_CONNECT_BACKOFF_BASE_MS", "123");
        }
        let opts = ConnectOptions::default();
        // Restore before assertions so a panic doesn't leak.
        unsafe {
            match &orig.0 {
                Some(v) => std::env::set_var("ATD_CONNECT_RETRIES", v),
                None => std::env::remove_var("ATD_CONNECT_RETRIES"),
            }
            match &orig.1 {
                Some(v) => std::env::set_var("ATD_CONNECT_BACKOFF_BASE_MS", v),
                None => std::env::remove_var("ATD_CONNECT_BACKOFF_BASE_MS"),
            }
        }
        assert_eq!(opts.max_attempts, 2);
        assert_eq!(opts.backoff_base_ms, 123);
    }

    #[test]
    fn is_fatal_classifies_not_found_and_permission_denied() {
        let nf =
            AtdError::ServerUnreachable(std::io::Error::new(std::io::ErrorKind::NotFound, "x"));
        let pd = AtdError::ServerUnreachable(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "x",
        ));
        let cr = AtdError::ServerUnreachable(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "x",
        ));
        assert!(is_fatal_connect_error(&nf));
        assert!(is_fatal_connect_error(&pd));
        assert!(!is_fatal_connect_error(&cr));
    }

    #[test]
    fn jitter_factor_stays_within_bounds() {
        for _ in 0..1000 {
            let j = jitter_factor();
            assert!(j >= -0.2 && j <= 0.2, "jitter {j} out of ±0.2 bound");
        }
    }

    // ---- SP-pagination-v1 §4.8 call_page + call_all + merge_pages tests ----

    #[tokio::test]
    async fn call_page_initial_sends_run_tool() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::RunTool { tool_id, .. } => Response::ToolResultResponse {
                tool_id,
                result: serde_json::json!([1, 2, 3]),
                success: true,
                dry_run: false,
                next_cursor: Some("CURSOR_AFTER_PAGE_1".into()),
            },
            other => panic!("expected RunTool, got {other:?}"),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let page = client
            .call_page(
                "celia:list_obs",
                serde_json::json!({"p": "x"}),
                None,
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(page.value, serde_json::json!([1, 2, 3]));
        assert_eq!(page.next_cursor.as_deref(), Some("CURSOR_AFTER_PAGE_1"));
    }

    #[tokio::test]
    async fn call_page_with_cursor_sends_run_tool_continue() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::RunToolContinue { tool_id, cursor } => {
                assert_eq!(cursor, "CURSOR_X");
                Response::ToolResultResponse {
                    tool_id,
                    result: serde_json::json!([4, 5]),
                    success: true,
                    dry_run: false,
                    next_cursor: None,
                }
            }
            other => panic!("expected RunToolContinue, got {other:?}"),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let page = client
            .call_page(
                "celia:list_obs",
                serde_json::Value::Null,
                Some("CURSOR_X"),
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(page.value, serde_json::json!([4, 5]));
        assert!(page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn call_all_concats_arrays_until_no_cursor() {
        let (client_end, server_end) = duplex(4096);
        // 3-page sequence: [1,2] cursor=a → [3,4] cursor=b → [5,6] cursor=None.
        spin_server(server_end, move |req| match req {
            Request::RunTool { tool_id, .. } => Response::ToolResultResponse {
                tool_id,
                result: serde_json::json!([1, 2]),
                success: true,
                dry_run: false,
                next_cursor: Some("cursor-a".into()),
            },
            Request::RunToolContinue { tool_id, cursor } => match cursor.as_str() {
                "cursor-a" => Response::ToolResultResponse {
                    tool_id,
                    result: serde_json::json!([3, 4]),
                    success: true,
                    dry_run: false,
                    next_cursor: Some("cursor-b".into()),
                },
                "cursor-b" => Response::ToolResultResponse {
                    tool_id,
                    result: serde_json::json!([5, 6]),
                    success: true,
                    dry_run: false,
                    next_cursor: None,
                },
                other => panic!("unexpected cursor: {other}"),
            },
            other => panic!("unexpected req: {other:?}"),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let all = client
            .call_all(
                "t",
                serde_json::json!({}),
                crate::options::CallAllOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(all, serde_json::json!([1, 2, 3, 4, 5, 6]));
    }

    #[tokio::test]
    async fn call_all_concat_field_merges_named_array() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::RunTool { tool_id, .. } => Response::ToolResultResponse {
                tool_id,
                result: serde_json::json!({"patient": "p1", "obs": [{"id": 1}], "total": 4}),
                success: true,
                dry_run: false,
                next_cursor: Some("c1".into()),
            },
            Request::RunToolContinue { tool_id, .. } => Response::ToolResultResponse {
                tool_id,
                result: serde_json::json!({"patient": "p1", "obs": [{"id": 2}, {"id": 3}, {"id": 4}], "total": 4}),
                success: true,
                dry_run: false,
                next_cursor: None,
            },
            other => panic!("unexpected: {other:?}"),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let opts = crate::options::CallAllOptions {
            merge_policy: crate::options::MergePolicy::ConcatField("obs".into()),
            ..Default::default()
        };
        let all = client
            .call_all("t", serde_json::json!({}), opts)
            .await
            .unwrap();
        // Last page's metadata wins; obs array is concatenated.
        assert_eq!(all["patient"], "p1");
        assert_eq!(all["total"], 4);
        assert_eq!(
            all["obs"],
            serde_json::json!([{"id":1},{"id":2},{"id":3},{"id":4}])
        );
    }

    #[tokio::test]
    async fn call_all_respects_max_pages() {
        let (client_end, server_end) = duplex(4096);
        // Server keeps issuing cursors forever — exercise the max_pages cap.
        spin_server(server_end, |req| match req {
            Request::RunTool { tool_id, .. } => Response::ToolResultResponse {
                tool_id,
                result: serde_json::json!([0]),
                success: true,
                dry_run: false,
                next_cursor: Some("c".into()),
            },
            Request::RunToolContinue { tool_id, .. } => Response::ToolResultResponse {
                tool_id,
                result: serde_json::json!([0]),
                success: true,
                dry_run: false,
                next_cursor: Some("c".into()),
            },
            other => panic!("unexpected: {other:?}"),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let opts = crate::options::CallAllOptions {
            max_pages: 3,
            ..Default::default()
        };
        let err = client.call_all("t", serde_json::json!({}), opts).await;
        match err {
            Err(AtdError::PaginationLimitExceeded { pages_fetched, .. }) => {
                assert_eq!(pages_fetched, 3);
            }
            other => panic!("expected PaginationLimitExceeded, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn call_all_respects_max_total_bytes() {
        let (client_end, server_end) = duplex(8192);
        // Each page returns a big array. With max_total_bytes set low,
        // we abort early.
        spin_server(server_end, |req| {
            let big = serde_json::Value::Array((0..100).map(serde_json::Value::from).collect());
            match req {
                Request::RunTool { tool_id, .. } => Response::ToolResultResponse {
                    tool_id,
                    result: big,
                    success: true,
                    dry_run: false,
                    next_cursor: Some("c".into()),
                },
                Request::RunToolContinue { tool_id, .. } => Response::ToolResultResponse {
                    tool_id,
                    result: big,
                    success: true,
                    dry_run: false,
                    next_cursor: Some("c".into()),
                },
                other => panic!("unexpected: {other:?}"),
            }
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let opts = crate::options::CallAllOptions {
            max_total_bytes: 400, // ~2 pages worth
            ..Default::default()
        };
        let err = client.call_all("t", serde_json::json!({}), opts).await;
        match err {
            Err(AtdError::PaginationLimitExceeded {
                bytes_fetched,
                pages_fetched: _,
            }) => {
                assert!(
                    bytes_fetched > 400,
                    "expected byte overflow, got {bytes_fetched}"
                );
            }
            other => panic!("expected PaginationLimitExceeded, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn call_all_single_page_returns_value_unchanged() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::RunTool { tool_id, .. } => Response::ToolResultResponse {
                tool_id,
                result: serde_json::json!({"data": [1, 2, 3]}),
                success: true,
                dry_run: false,
                next_cursor: None,
            },
            other => panic!("unexpected: {other:?}"),
        })
        .await;
        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let all = client
            .call_all(
                "t",
                serde_json::json!({}),
                crate::options::CallAllOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(all, serde_json::json!({"data": [1, 2, 3]}));
    }

    #[test]
    fn merge_pages_concat_array_basic() {
        use crate::options::MergePolicy;
        let r = merge_pages(
            Some(serde_json::json!([1, 2])),
            serde_json::json!([3, 4]),
            &MergePolicy::ConcatArray,
        )
        .unwrap();
        assert_eq!(r, serde_json::json!([1, 2, 3, 4]));
    }

    #[test]
    fn merge_pages_concat_array_rejects_non_array() {
        use crate::options::MergePolicy;
        let err = merge_pages(
            Some(serde_json::json!([1, 2])),
            serde_json::json!({"x": 1}),
            &MergePolicy::ConcatArray,
        )
        .unwrap_err();
        assert!(matches!(err, AtdError::MergeFailed { .. }));
    }

    #[test]
    fn merge_pages_first_page_only_drops_subsequent() {
        use crate::options::MergePolicy;
        let r = merge_pages(
            Some(serde_json::json!({"first": true})),
            serde_json::json!({"second": true}),
            &MergePolicy::FirstPageOnly,
        )
        .unwrap();
        assert_eq!(
            r,
            serde_json::json!({"first": true}),
            "FirstPageOnly: accumulator wins; subsequent pages dropped"
        );
    }
}
