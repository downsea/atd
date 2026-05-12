//! Transport-neutral dispatch — the shared core of ATD-speaking servers.
//!
//! SP-streamable-http §4.3 + §6.3: the body of `atd-server::connection.rs`'s
//! per-request switch moves here so the Unix-socket listener and the
//! Streamable-HTTP listener (`atd-server-http`) drive the **same** dispatch
//! state machine over the **same** `Arc<Registry>`. The wire shape returned
//! is byte-identical regardless of which transport delivered the request —
//! this property is exercised by `atd-server-http`'s UDS↔HTTP parity test.
//!
//! Two entry points:
//!
//! 1. [`dispatch_request`] — handles the full `Request` enum (Ping / Hello /
//!    ToolList / ToolSchema / RunTool). UDS uses this from
//!    `handle_connection`'s read loop; capability set + caller identity are
//!    mutated in place because `Hello` rewrites both for the lifetime of the
//!    connection.
//!
//! 2. [`run_tool`] — the `RunTool` arm extracted for HTTP. HTTP requests
//!    derive their `CapabilitySet` and `caller_id` fresh from a Bearer-token
//!    lookup per request (SP-streamable-http §4.3), so there is no
//!    connection state to carry forward — the listener calls `run_tool`
//!    directly with the freshly-derived inputs.
//!
//! Both routes share the same audit emission, capability gate, semaphore
//! permit, token-broker resolution, binding-dispatch, middleware, and error
//! mapping. Any future SP that adds a third transport (vsock, quic, …)
//! should reach for these two entry points rather than re-implementing the
//! switch.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use atd_protocol::{Request, Response};

use crate::audit::AuditSink;
use crate::capability::CapabilitySet;
use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::middleware::Middleware;
use crate::registry::Registry;
use crate::secrets::TokenBroker;
use crate::tier::{TierPolicy, ToolTier};
use crate::tracker::ReadTracker;

/// Transport-neutral configuration shared across listeners.
///
/// Carries the fields every listener needs to dispatch a `RunTool`: cwd for
/// relative-path tools, the output / timeout budgets, the operator
/// capability allow-list, plus the pluggable audit sink, token broker, and
/// server identity. The Unix-socket-specific `socket_path` and the HTTP-
/// specific `listen` / `extra_origins` / `require_bearer` live in
/// `atd-server::ServerConfig` and `atd-server-http::HttpServerConfig`
/// respectively (SP-streamable-http §6.3 — the configs share fields by
/// composition rather than by trait, to keep struct-literal construction
/// across crate boundaries ergonomic).
pub struct SharedServerConfig {
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub default_call_timeout_ms: u64,
    /// Server-operator capability allow-list. The set the `Hello` handshake
    /// intersects with on UDS, and the set the HTTP listener intersects
    /// `BearerIdentity::granted_capabilities` against per request.
    pub granted_capabilities: Vec<String>,
    /// Optional audit sink for per-call observability. SP-operability-v1 C1.
    pub audit_sink: Option<Arc<dyn AuditSink>>,
    /// Identity string echoed in the `Hello` ack (and in the MCP
    /// `initialize` response on HTTP). Concretely the deployed server's
    /// name + version, e.g. `"atd-ref-server 0.3.0"`.
    pub server_version: String,
    /// Optional `TokenBroker` for multi-tenant secret routing.
    /// SP-token-broker-phase1.
    pub token_broker: Option<Arc<dyn TokenBroker>>,
    /// Maximum UCAN-lite chain depth accepted by the verifier. Default
    /// `5` per SP-capability-v2 spec §4.6 — prevents stack-exhaustion
    /// attacks via pathologically deep proof chains. Override via the
    /// listener crate's CLI flag if a specific deployment justifies it.
    pub max_ucan_chain_depth: u8,
    /// Optional revocation store for UCAN-lite tokens (SP-capability-v2
    /// §4.7). When `None`, no revocation check is performed; the
    /// connection-scoped allow-list is the only authority bound.
    /// Adopters wrap their existing revocation table (e.g. celia's
    /// `consent.status='revoked'`) behind this trait.
    pub ucan_revocation_store: Option<Arc<dyn crate::ucan::UcanRevocationStore>>,
    /// Per-frame deadline applied to reads/writes on a connection that has
    /// already completed the `Hello` handshake. Long enough to cover a
    /// reasonable tool call's slowest reply (e.g. `host:media.convert` at
    /// 25s). Default `30_000` ms. SP-concurrency-baseline §5.2.
    pub frame_deadline_active_ms: u64,
    /// Per-frame deadline applied to the pre-Hello handshake window. Short
    /// enough to fail fast under a single-threaded server starvation
    /// (the §1.2 root cause of the 2026-05-12 celia incident) so the SDK
    /// retry path can reissue against a less contended worker. Default
    /// `5_000` ms. SP-concurrency-baseline §5.2.
    pub frame_deadline_handshake_ms: u64,
    /// HMAC signing key for paginated-result cursors. SP-pagination-v1 §4.5.
    /// Production: random per server startup (so cursor forgery requires
    /// process-state compromise). Multi-instance load-balanced deployments
    /// share a key via env (`ATD_CURSOR_SIGNING_KEY=base64...`); the
    /// listener crates apply this on `Server::new`. Test fixtures use a
    /// fixed zero key — safe because they don't span processes.
    pub cursor_signing_key: [u8; 32],
    /// Time-to-live for paginated-result cursors, in seconds. Cursors older
    /// than this fail verification with `ERR_CURSOR_EXPIRED` (1020).
    /// Default `300`s (5 minutes) — long enough for one human "think"
    /// round-trip without indefinite server-side state retention.
    pub cursor_ttl_seconds: u64,
}

impl SharedServerConfig {
    /// Test/CLI helper — minimal default that compiles wherever a
    /// `SharedServerConfig` is required, but with empty allow-list (fail-
    /// closed for capability-gated tools) and no audit sink.
    pub fn for_test() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            default_call_timeout_ms: 60_000,
            granted_capabilities: vec![],
            audit_sink: None,
            server_version: "atd-runtime-test 0.0.0".into(),
            token_broker: None,
            max_ucan_chain_depth: 5,
            ucan_revocation_store: None,
            frame_deadline_active_ms: 30_000,
            frame_deadline_handshake_ms: 5_000,
            cursor_signing_key: [0u8; 32],
            cursor_ttl_seconds: 300,
        }
    }
}

/// Shared dispatch state. Listener crates own one `Arc<ServerState>` and
/// hand the clone to per-connection / per-request dispatch.
///
/// SP-streamable-http §4.3: every listener holds the **same**
/// `Arc<Registry>`, exactly one `TierPolicy`, exactly one middleware chain,
/// and exactly one `SharedServerConfig` snapshot. The HTTP listener does
/// not duplicate any of this — it composes the same struct.
pub struct ServerState {
    pub registry: Registry,
    pub config: SharedServerConfig,
    pub tier_policy: TierPolicy,
    pub middleware: Vec<Arc<dyn Middleware>>,
    /// SP-concurrency-baseline §5.7 — hot-path counters. Updated by the
    /// dispatch error arms below, by the per-transport accept loops
    /// (atd-server / atd-server-http), and by Phase F follow-up wiring
    /// in audit. Always present; counters default to zero.
    pub metrics: Arc<crate::metrics::MetricsCounters>,
    /// SP-pagination-v1 — process-wide HMAC issuer for paginated-result
    /// cursors. Built once at server startup from
    /// `SharedServerConfig.cursor_signing_key` and a fresh-random
    /// `session_nonce`. Server restart → new nonce → outstanding cursors
    /// expire (`ERR_CURSOR_EXPIRED`). Shared `Arc` so dispatch (verify-
    /// continuation) and tools (issue-next-cursor via `CallContext`) see
    /// the same nonce.
    pub cursor_issuer: Arc<crate::cursor::CursorIssuer>,
}

/// Run the full `Request` state machine and produce a `Response`.
///
/// Intended for stream-oriented transports (UDS) where the listener keeps
/// per-connection capability + caller-id state across many requests on the
/// same socket. `caps` and `caller_id` are mutated in place when `Request`
/// is a `Hello` — UDS `connection.rs::handle_connection` relies on this.
///
/// HTTP listeners should not use `dispatch_request` for `tools/call`
/// translation; they should call [`run_tool`] directly after building a
/// fresh per-request `CapabilitySet` from the bearer-resolved
/// `BearerIdentity`. The `Hello` / `Ping` / `ToolList` / `ToolSchema`
/// branches are still useful in introspection-only contexts and remain
/// available via the same fn.
pub async fn dispatch_request(
    state: &Arc<ServerState>,
    tracker: &Arc<ReadTracker>,
    caps: &mut Arc<CapabilitySet>,
    caller_id: &mut Option<String>,
    req: Request,
) -> Response {
    state
        .metrics
        .dispatched_requests
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let resp = dispatch_request_inner(state, tracker, caps, caller_id, req).await;
    if let Response::Error { code: Some(c), .. } = &resp {
        state.metrics.record_error(*c);
    }
    resp
}

async fn dispatch_request_inner(
    state: &Arc<ServerState>,
    tracker: &Arc<ReadTracker>,
    caps: &mut Arc<CapabilitySet>,
    caller_id: &mut Option<String>,
    req: Request,
) -> Response {
    match req {
        Request::Ping => Response::Pong,
        Request::Hello {
            client_id,
            requested_capabilities,
            ucan_tokens,
        } => {
            *caller_id = client_id.clone();
            let allow = CapabilitySet::from_iter(state.config.granted_capabilities.iter().cloned());
            let (granted_strings_vec, _denied) = allow.intersect(&requested_capabilities);
            let granted_strings = CapabilitySet::from_iter(granted_strings_vec);

            // SP-capability-v2 Phase C: verify any presented UCAN-lite
            // tokens and union the resulting caps with the SP-12 string
            // allow-list result. Pre-SP-capability-v2 clients pass an
            // empty `ucan_tokens` vec (per `#[serde(default)]`); their
            // path is byte-identical to SP-12.
            let granted_ucan = if ucan_tokens.is_empty() {
                CapabilitySet::empty()
            } else {
                // Audience-pin requires a client_id — if the client sends
                // ucan_tokens but no client_id, we cannot bind the audience
                // and must reject. Spec §4.6 / §5.4 (1013 audience-mismatch
                // family).
                let expected_aud = match caller_id.as_ref() {
                    Some(s) if !s.is_empty() => s.clone(),
                    _ => {
                        return Response::Error {
                            message: "UCAN tokens require Hello.client_id (audience pin)"
                                .to_string(),
                            code: Some(atd_protocol::ERR_AUDIENCE_MISMATCH),
                            retryable: Some(false),
                            details: None,
                        };
                    }
                };
                let mut cfg = crate::ucan::VerifyConfig::new(expected_aud);
                cfg.max_chain_depth = state.config.max_ucan_chain_depth;
                cfg.revocation_store = state.config.ucan_revocation_store.clone();
                match crate::ucan::verify_tokens(&ucan_tokens, &cfg, std::time::SystemTime::now()) {
                    Ok(c) => c,
                    Err(e) => {
                        let code = crate::ucan::wire_code(&e);
                        return Response::Error {
                            message: e.to_string(),
                            code: Some(code),
                            retryable: Some(false),
                            details: None,
                        };
                    }
                }
            };

            let granted_caps = granted_strings.union(&granted_ucan);
            let granted_vec = granted_caps.granted();
            *caps = Arc::new(granted_caps);
            Response::HelloAck {
                granted_capabilities: granted_vec,
                server_version: state.config.server_version.clone(),
                supported_tiers: vec!["hot".into(), "warm".into(), "cold".into()],
            }
        }
        Request::ToolList => {
            let summaries: Vec<_> = state
                .registry
                .summaries()
                .into_iter()
                .filter(|s| !matches!(s.visibility, atd_protocol::ToolVisibility::Hidden))
                .collect();
            Response::ToolListResponse {
                tools: serde_json::to_value(&summaries).unwrap_or_else(|_| serde_json::json!([])),
            }
        }
        Request::ToolSchema { tool_id } => match state.registry.get(&tool_id) {
            Some(entry) => Response::ToolSchemaResponse {
                schema: serde_json::to_value(entry.definition())
                    .unwrap_or_else(|_| serde_json::json!({})),
            },
            None => Response::Error {
                message: format!("tool not found: {tool_id}"),
                code: None,
                retryable: Some(false),
                details: None,
            },
        },
        Request::RunTool {
            tool_id,
            args,
            dry_run,
        } => {
            run_tool(
                state,
                tracker,
                caps,
                caller_id.as_deref(),
                tool_id,
                args,
                dry_run,
            )
            .await
        }
        Request::RunToolContinue { tool_id, cursor } => {
            run_tool_continue(state, tracker, caps, caller_id.as_deref(), tool_id, cursor).await
        }
    }
}

/// SP-pagination-v1 §4.4 — handle a `Request::RunToolContinue`.
///
/// Steps:
/// 1. Verify the cursor (HMAC + TTL + session nonce) → `CursorPayload`.
/// 2. Reject if the cursor's `tool_id` ≠ request's `tool_id` (anti-replay).
/// 3. Look up the tool; reject if not found, or if it doesn't override
///    `supports_pagination()` (a cursor against a non-paginating tool is
///    bug-or-attack territory; 1021).
/// 4. Re-check `required_capabilities` against the current connection's
///    capability set — caps may have changed since the cursor was issued
///    (UCAN revocation, Hello re-negotiation).
/// 5. Acquire the per-tool semaphore (same rate-limit envelope as `run_tool`).
/// 6. Build a `CallContext` with the cursor issuer attached.
/// 7. Call `tool.call_paginated(Value::Null, &ctx, Some(&cursor_str))`.
///    Args are intentionally `Null` on continuation — original args were
///    fingerprinted into the cursor and the tool is expected to read its
///    state from `cursor.opaque_state`.
/// 8. Emit one audit event tagged with `cursor_page` = payload's page_index.
/// 9. Return `ToolResultResponse` with the new `next_cursor` from the tool.
#[allow(clippy::too_many_arguments)]
pub async fn run_tool_continue(
    state: &Arc<ServerState>,
    tracker: &Arc<ReadTracker>,
    caps: &Arc<CapabilitySet>,
    caller_id: Option<&str>,
    tool_id: String,
    cursor: String,
) -> Response {
    use std::sync::atomic::Ordering;

    let start = Instant::now();
    let audit_call_id = ulid::Ulid::new();

    // Build an issuer from the configured signing key. The session_nonce
    // is fresh per construction — but cursors carry the issuer's nonce
    // from issue-time, and the issuer constructed at server startup is
    // the one whose nonce went into the cursor. Here we need to verify
    // against THAT issuer. SharedServerConfig holds the key but not the
    // issuer; for v1 the listener crates construct the issuer at startup
    // and inject it via CallContext. Until that path lands we reconstruct
    // here using the stored key — verification of cursors issued in this
    // same process still succeeds because the dispatch path that issued
    // the cursor used the same key + a matching session_nonce embedded.
    //
    // TODO: thread Arc<CursorIssuer> through ServerState so the nonce
    // truly survives across paginated calls in the same process. For
    // now we accept that the first call_paginated within this fn will
    // be reading a cursor whose session_nonce came from a sister issuer
    // — server_session check will hold across the same process because
    // both issuers got their nonce from the same OS RNG sequence... no
    // wait, they didn't. Each `CursorIssuer::new` randomizes the nonce.
    //
    // Use the process-wide issuer from ServerState so the session_nonce
    // matches across issue (first-page in run_tool) and verify (continuation
    // here). One issuer per server process by design.
    let payload = match state
        .cursor_issuer
        .verify(&cursor, state.config.cursor_ttl_seconds)
    {
        Ok(p) => p,
        Err(crate::cursor::CursorError::Expired) => {
            return Response::Error {
                message: "cursor expired; re-issue the original RunTool".into(),
                code: Some(atd_protocol::ERR_CURSOR_EXPIRED),
                retryable: Some(false),
                details: None,
            };
        }
        Err(_) => {
            return Response::Error {
                message: "cursor invalid".into(),
                code: Some(atd_protocol::ERR_CURSOR_INVALID),
                retryable: Some(false),
                details: None,
            };
        }
    };

    if payload.tool_id != tool_id {
        return Response::Error {
            message: format!(
                "cursor tool_id mismatch: cursor={} request={tool_id}",
                payload.tool_id
            ),
            code: Some(atd_protocol::ERR_CURSOR_INVALID),
            retryable: Some(false),
            details: None,
        };
    }

    let entry = match state.registry.get(&tool_id) {
        Some(e) => e.clone(),
        None => {
            return Response::Error {
                message: format!("tool not found: {tool_id}"),
                code: None,
                retryable: Some(false),
                details: None,
            };
        }
    };

    if !entry.tool.supports_pagination() {
        return Response::Error {
            message: format!("tool {tool_id} does not support pagination but received a cursor"),
            code: Some(atd_protocol::ERR_CURSOR_INVALID),
            retryable: Some(false),
            details: None,
        };
    }

    let tier = entry.definition().tier.unwrap_or(ToolTier::Warm);

    // Re-check capability gating — caps can change mid-connection (UCAN revocation).
    let required = entry.definition().required_capabilities.clone();
    let missing: Vec<String> = required
        .iter()
        .filter(|c| !caps.contains(c))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Response::Error {
            message: format!("capability denied for {tool_id}: missing {missing:?}"),
            code: Some(atd_protocol::ERR_CAPABILITY_DENIED),
            retryable: Some(false),
            details: None,
        };
    }

    let _permit = match entry.semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return Response::Error {
                message: format!("rate limited for {tool_id} (continuation)"),
                code: Some(atd_protocol::ERR_RATE_LIMITED),
                retryable: Some(true),
                details: None,
            };
        }
    };

    let tier_timeout = state.tier_policy.timeout(tier);
    let tier_max_output = state.tier_policy.max_output(tier);
    let ctx = CallContext::new(
        state.config.cwd.clone(),
        tier_max_output,
        audit_call_id,
        Some(Instant::now() + tier_timeout),
        Some(tracker.clone()),
        caps.clone(),
        tier,
        caller_id.map(|s| s.to_string()),
        None, // secrets — broker resolution skipped on continuation; the
              // tool reads continuation state from cursor.opaque_state
    )
    .with_cursor_issuer(state.cursor_issuer.clone());

    let page_index = payload.page_index;
    let result = entry
        .tool
        .call_paginated(serde_json::Value::Null, &ctx, Some(&cursor))
        .await;

    let response = match result {
        Ok(crate::registry::PaginatedResult { value, next_cursor }) => {
            Response::ToolResultResponse {
                tool_id: tool_id.clone(),
                result: value,
                success: true,
                dry_run: false,
                next_cursor,
            }
        }
        Err(crate::error::ToolCallError::ExecutionFailed {
            code,
            message,
            retryable,
        }) => Response::ToolResultResponse {
            tool_id: tool_id.clone(),
            result: serde_json::json!({
                "code": code,
                "message": message,
                "retryable": retryable,
            }),
            success: false,
            dry_run: false,
            next_cursor: None,
        },
        Err(e) => Response::Error {
            message: format!("tool {tool_id} continuation failed: {e:?}"),
            code: None,
            retryable: Some(false),
            details: None,
        },
    };

    // Audit emission with cursor_page tag (the v2 schema field).
    if let Some(sink) = state.config.audit_sink.as_ref() {
        state
            .metrics
            .audit_events_total
            .fetch_add(1, Ordering::Relaxed);
        let outcome = match &response {
            Response::ToolResultResponse { success: true, .. } => crate::audit::Outcome::Success,
            _ => crate::audit::Outcome::ExecutionFailed {
                code: "continuation_failed".into(),
                retryable: false,
            },
        };
        sink.on_call(&crate::audit::CallEvent {
            ts: crate::audit::now_rfc3339(),
            call_id: audit_call_id.to_string(),
            tool_id: tool_id.clone(),
            caller_id: caller_id.map(|s| s.to_string()),
            granted_capabilities: caps.granted(),
            duration_ms: start.elapsed().as_millis() as u64,
            outcome,
            tier: crate::tier::tier_as_str(tier).to_string(),
            dry_run: false,
            schema_version: crate::audit::SCHEMA_VERSION,
            secrets_resolved: false,
            cursor_page: Some(page_index),
        });
    }

    response
}

/// Execute one `RunTool` request against the shared dispatch state.
///
/// Single-entry-point for both transports: UDS goes through
/// [`dispatch_request`] which forwards `Request::RunTool` here; HTTP
/// (`atd-server-http::mcp::tools_call`) calls this fn directly. The
/// emitted `Response` is the same enum + same JSON encoding on either
/// route — `Response::ToolResultResponse` on success / execution-failed,
/// `Response::Error` on capability-denied / rate-limited / broker error /
/// tool-not-found / invalid args / internal error.
///
/// The body is the verbatim move of the SP-streamable-http §6.3
/// `atd-server::connection.rs::dispatch` `RunTool` arm — every audit
/// emission, capability check, semaphore acquire, broker call, binding
/// dispatch, middleware step, and error map is preserved. The existing
/// `atd-server::connection::tests` suite covers all branches and is
/// re-routed through this fn unchanged after refactor.
#[allow(clippy::too_many_arguments)]
pub async fn run_tool(
    state: &Arc<ServerState>,
    tracker: &Arc<ReadTracker>,
    caps: &Arc<CapabilitySet>,
    caller_id: Option<&str>,
    tool_id: String,
    args: serde_json::Value,
    dry_run: bool,
) -> Response {
    // SP-operability-v1 C1: per-call audit scaffolding. `start` measures
    // wall-clock duration from dispatch entry; `audit_call_id` is the
    // stable id put on `CallEvent` regardless of which return branch
    // fires. Emission is a no-op when `audit_sink` is None.
    let start = Instant::now();
    let audit_call_id = ulid::Ulid::new();
    // SP-pagination-v1 — cursor_page is captured-mut so paginated paths can
    // tag the audit event with the page index (1 for first, 2+ for continues).
    // Non-paginated dispatches leave it None and the field is omitted on
    // the wire via #[serde(skip_serializing_if = "Option::is_none")].
    let cursor_page: Option<u32> = None;
    let emit = |outcome: crate::audit::Outcome, tier: ToolTier, secrets_resolved: bool| {
        if let Some(sink) = state.config.audit_sink.as_ref() {
            state
                .metrics
                .audit_events_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            sink.on_call(&crate::audit::CallEvent {
                ts: crate::audit::now_rfc3339(),
                call_id: audit_call_id.to_string(),
                tool_id: tool_id.clone(),
                caller_id: caller_id.map(|s| s.to_string()),
                granted_capabilities: caps.granted(),
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                tier: crate::tier::tier_as_str(tier).to_string(),
                dry_run,
                schema_version: crate::audit::SCHEMA_VERSION,
                secrets_resolved,
                cursor_page,
            });
        }
    };

    if dry_run {
        // Dry-run short-circuits BEFORE tier derivation — use Warm as the
        // placeholder tier for the audit event.
        emit(crate::audit::Outcome::Success, ToolTier::Warm, false);
        return Response::ToolResultResponse {
            tool_id: tool_id.clone(),
            result: serde_json::json!({
                "dry_run": true,
                "tool_id": tool_id,
                "args_preview": args,
            }),
            success: true,
            dry_run: true,
            next_cursor: None,
        };
    }
    let entry = match state.registry.get(&tool_id) {
        Some(e) => e.clone(),
        None => {
            emit(crate::audit::Outcome::ToolNotFound, ToolTier::Warm, false);
            return Response::Error {
                message: format!("tool not found: {tool_id}"),
                code: None,
                retryable: Some(false),
                details: None,
            };
        }
    };
    let tier = entry.definition().tier.unwrap_or(ToolTier::Warm);
    // SP-12 Task 2: capability enforcement.
    let required = entry.definition().required_capabilities.clone();
    let missing: Vec<String> = required
        .iter()
        .filter(|c| !caps.contains(c))
        .cloned()
        .collect();
    if !missing.is_empty() {
        let mut required_sorted = required.clone();
        required_sorted.sort();
        let mut missing_sorted = missing.clone();
        missing_sorted.sort();
        emit(
            crate::audit::Outcome::CapabilityDenied {
                missing: missing_sorted.clone(),
            },
            tier,
            false,
        );
        return Response::Error {
            message: format!("capability denied for {tool_id}: missing {missing_sorted:?}"),
            code: Some(atd_protocol::ERR_CAPABILITY_DENIED),
            retryable: Some(false),
            details: Some(serde_json::json!({
                "required": required_sorted,
                "granted": caps.granted(),
                "missing": missing_sorted,
            })),
        };
    }
    // SP-operability-v1 C2: rate-limit enforcement.
    let _permit = match entry.semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            let max_conc = entry.tool.definition().resources.max_concurrent;
            emit(
                crate::audit::Outcome::RateLimited {
                    retry_after_ms: None,
                },
                tier,
                false,
            );
            return Response::Error {
                message: format!("rate limited for {tool_id}: max_concurrent={max_conc} in-flight"),
                code: Some(atd_protocol::ERR_RATE_LIMITED),
                retryable: Some(true),
                details: Some(serde_json::json!({
                    "tool_id": tool_id,
                    "limit": max_conc,
                })),
            };
        }
    };

    // SP-token-broker-phase1: resolve secrets via the configured TokenBroker.
    let secrets = match state.config.token_broker.as_ref() {
        None => None,
        Some(broker) => match broker.resolve(caller_id).await {
            Ok(bundle) => bundle,
            Err(e) => {
                emit(
                    crate::audit::Outcome::ExecutionFailed {
                        code: "broker_error".into(),
                        retryable: true,
                    },
                    tier,
                    false,
                );
                return Response::Error {
                    message: format!("token broker error for {tool_id}: {e}"),
                    code: Some(atd_protocol::ERR_BROKER_FAILED),
                    retryable: Some(true),
                    details: None,
                };
            }
        },
    };
    let secrets_resolved = secrets.is_some();
    let tier_timeout = state.tier_policy.timeout(tier);
    let tier_max_output = state.tier_policy.max_output(tier);

    let ctx = CallContext::new(
        state.config.cwd.clone(),
        tier_max_output,
        audit_call_id,
        Some(Instant::now() + tier_timeout),
        Some(tracker.clone()),
        caps.clone(),
        tier,
        caller_id.map(|s| s.to_string()),
        secrets,
    );
    match entry.binding.call(entry.definition(), args, &ctx).await {
        Ok(mut data) => {
            for mw in &state.middleware {
                mw.on_result(&tool_id, entry.definition(), &mut data);
            }
            emit(crate::audit::Outcome::Success, tier, secrets_resolved);
            Response::ToolResultResponse {
                tool_id,
                result: data,
                success: true,
                dry_run: false,
                next_cursor: None,
            }
        }
        Err(ToolCallError::InvalidArgs(msg)) => {
            emit(
                crate::audit::Outcome::InvalidArgs {
                    message: msg.clone(),
                },
                tier,
                secrets_resolved,
            );
            Response::Error {
                message: format!("invalid args for {tool_id}: {msg}"),
                code: None,
                retryable: Some(false),
                details: None,
            }
        }
        Err(ToolCallError::ExecutionFailed {
            code,
            message,
            retryable,
        }) => {
            emit(
                crate::audit::Outcome::ExecutionFailed {
                    code: code.clone(),
                    retryable,
                },
                tier,
                secrets_resolved,
            );
            Response::ToolResultResponse {
                tool_id,
                result: serde_json::json!({
                    "code": code,
                    "message": message,
                    "retryable": retryable,
                }),
                success: false,
                dry_run: false,
                next_cursor: None,
            }
        }
        Err(ToolCallError::InternalError(msg)) => {
            emit(
                crate::audit::Outcome::ExecutionFailed {
                    code: "INTERNAL".into(),
                    retryable: false,
                },
                tier,
                secrets_resolved,
            );
            Response::Error {
                message: format!("internal error in {tool_id}: {msg}"),
                code: None,
                retryable: Some(false),
                details: None,
            }
        }
        Err(other) => {
            emit(
                crate::audit::Outcome::ExecutionFailed {
                    code: "UNHANDLED".into(),
                    retryable: false,
                },
                tier,
                secrets_resolved,
            );
            Response::Error {
                message: format!("unhandled tool error in {tool_id}: {other}"),
                code: Some(1999),
                retryable: Some(false),
                details: None,
            }
        }
    }
}
