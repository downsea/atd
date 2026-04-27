//! Per-connection task: read frames, dispatch, write responses.

use std::sync::Arc;
use std::time::Instant;

use tokio::net::UnixStream;

use atd_protocol::wire::{read_frame, write_frame};
use atd_protocol::{Request, Response};
use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;

use crate::server::ServerState;

pub(crate) async fn handle_connection(
    state: Arc<ServerState>,
    stream: UnixStream,
) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let tracker = Arc::new(atd_runtime::ReadTracker::new()); // per-connection
    // Per-connection capability set, replaced on `Hello`. Default: empty.
    let mut caps: Arc<atd_runtime::CapabilitySet> = Arc::new(atd_runtime::CapabilitySet::empty());
    // Per-connection caller identity, populated from the Hello handshake's
    // `client_id`. `None` until the first Hello; shared with every RunTool
    // CallContext and stamped on audit events.
    let mut caller_id: Option<String> = None;
    loop {
        let req: Request = match read_frame(&mut reader).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let resp = dispatch(&state, &tracker, &mut caps, &mut caller_id, req).await;
        write_frame(&mut writer, &resp).await?;
    }
}

pub(crate) async fn dispatch(
    state: &Arc<ServerState>,
    tracker: &Arc<atd_runtime::ReadTracker>,
    caps: &mut Arc<atd_runtime::CapabilitySet>,
    caller_id: &mut Option<String>,
    req: Request,
) -> Response {
    match req {
        Request::Ping => Response::Pong,
        // SP-12 Task 2: intersect requested capabilities with the server's
        // allow-list; store the granted subset on the connection. Replying with
        // the subset (not the full server set) lets clients discover what they
        // actually hold.
        Request::Hello {
            client_id,
            requested_capabilities,
        } => {
            // Cache the caller identity for the lifetime of this connection.
            // `None` client_id is preserved (SDK may omit it). Each subsequent
            // Hello on the same connection overwrites the identity.
            *caller_id = client_id;
            let allow = atd_runtime::CapabilitySet::from_iter(
                state.config.granted_capabilities.iter().cloned(),
            );
            let (granted, _denied) = allow.intersect(&requested_capabilities);
            *caps = Arc::new(atd_runtime::CapabilitySet::from_iter(granted.clone()));
            Response::HelloAck {
                granted_capabilities: granted,
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
            // SP-operability-v1 C1: per-call audit scaffolding. `start` measures
            // wall-clock duration from dispatch entry; `audit_call_id` is the
            // stable id put on `CallEvent` regardless of which return branch
            // fires (on success/exec_failed/invalid_args branches it matches
            // `ctx.call_id` — see the Ulid construction below). Emission is a
            // no-op when `audit_sink` is None (the default).
            let start = Instant::now();
            let audit_call_id = ulid::Ulid::new();
            let emit = |outcome: atd_runtime::Outcome, tier: atd_runtime::tier::ToolTier| {
                if let Some(sink) = state.config.audit_sink.as_ref() {
                    sink.on_call(&atd_runtime::CallEvent {
                        ts: atd_runtime::audit::now_rfc3339(),
                        call_id: audit_call_id.to_string(),
                        tool_id: tool_id.clone(),
                        caller_id: (*caller_id).clone(),
                        granted_capabilities: caps.granted(),
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome,
                        tier: atd_runtime::tier_as_str(tier).to_string(),
                        dry_run,
                        schema_version: atd_runtime::SCHEMA_VERSION,
                    });
                }
            };

            if dry_run {
                // Dry-run short-circuits BEFORE tier derivation — use Warm as
                // the placeholder tier for the audit event.
                emit(
                    atd_runtime::Outcome::Success,
                    atd_runtime::tier::ToolTier::Warm,
                );
                return Response::ToolResultResponse {
                    tool_id: tool_id.clone(),
                    result: serde_json::json!({
                        "dry_run": true,
                        "tool_id": tool_id,
                        "args_preview": args,
                    }),
                    success: true,
                    dry_run: true,
                };
            }
            let entry = match state.registry.get(&tool_id) {
                Some(e) => e.clone(),
                None => {
                    emit(
                        atd_runtime::Outcome::ToolNotFound,
                        atd_runtime::tier::ToolTier::Warm,
                    );
                    return Response::Error {
                        message: format!("tool not found: {tool_id}"),
                        code: None,
                        retryable: Some(false),
                        details: None,
                    };
                }
            };
            // SP-12 Task 3: derive tier from the tool definition; TierPolicy
            // maps each tier to deadline + max_output budgets. Tools without
            // a tier field default to Warm (spec §8 Q5), preserving pre-SP-12
            // behavior for the 9 built-in tools that never set tier.
            let tier = entry
                .definition()
                .tier
                .unwrap_or(atd_runtime::tier::ToolTier::Warm);
            // SP-12 Task 2: capability enforcement. Refuse calls whose
            // required_capabilities are not a subset of the connection's
            // granted set. Sorted `missing` + `granted` keep the error shape
            // deterministic for tests and UI.
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
                    atd_runtime::Outcome::CapabilityDenied {
                        missing: missing_sorted.clone(),
                    },
                    tier,
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
            // SP-operability-v1 C2: rate limit enforcement via per-tool
            // Semaphore. Fail-fast (`try_acquire_owned`): a saturated tool
            // returns 1002 immediately with `retryable: true` rather than
            // queueing, keeping dispatch latency predictable.
            //
            // The returned `_permit` must remain in scope through the
            // `binding.call(...).await` below — dropping it releases the
            // slot regardless of which result arm runs (success, error,
            // panic/future-drop).
            let _permit = match entry.semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    let max_conc = entry.tool.definition().resources.max_concurrent;
                    emit(
                        atd_runtime::Outcome::RateLimited {
                            retry_after_ms: None,
                        },
                        tier,
                    );
                    return Response::Error {
                        message: format!(
                            "rate limited for {tool_id}: max_concurrent={max_conc} in-flight"
                        ),
                        code: Some(atd_protocol::ERR_RATE_LIMITED),
                        retryable: Some(true),
                        details: Some(serde_json::json!({
                            "tool_id": tool_id,
                            "limit": max_conc,
                        })),
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
                (*caller_id).clone(),
            );
            // SP-12 Task 4: dispatch through the binding. NativeBinding (the
            // default for `Registry::register`) simply calls back into
            // `Tool::call`; CliBinding spawns a subprocess. Same surface
            // either way.
            match entry.binding.call(entry.definition(), args, &ctx).await {
                Ok(mut data) => {
                    // SP-12 Task 5: result-middleware chain runs on success
                    // only (spec §8 Q4). Order is the order set via
                    // Server::set_middleware.
                    for mw in &state.middleware {
                        mw.on_result(&tool_id, entry.definition(), &mut data);
                    }
                    emit(atd_runtime::Outcome::Success, tier);
                    Response::ToolResultResponse {
                        tool_id,
                        result: data,
                        success: true,
                        dry_run: false,
                    }
                }
                Err(ToolCallError::InvalidArgs(msg)) => {
                    emit(
                        atd_runtime::Outcome::InvalidArgs {
                            message: msg.clone(),
                        },
                        tier,
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
                        atd_runtime::Outcome::ExecutionFailed {
                            code: code.clone(),
                            retryable,
                        },
                        tier,
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
                    }
                }
                Err(ToolCallError::InternalError(msg)) => {
                    emit(
                        atd_runtime::Outcome::ExecutionFailed {
                            code: "INTERNAL".into(),
                            retryable: false,
                        },
                        tier,
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
                        atd_runtime::Outcome::ExecutionFailed {
                            code: "UNHANDLED".into(),
                            retryable: false,
                        },
                        tier,
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
    }
}

#[cfg(test)]
mod tests {
    //! Dispatch unit tests using stub tools (no atd-tools-* deps).
    //!
    //! These cover the dispatch state-machine for each Request variant. The
    //! 9-built-in-tool integration tests live in `atd-ref-server/tests/`.

    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    use atd_runtime::registry::{CallFuture, Registry, Tool};

    use crate::config::ServerConfig;
    use crate::server::ServerState;

    fn fresh_tracker() -> Arc<atd_runtime::ReadTracker> {
        Arc::new(atd_runtime::ReadTracker::new())
    }

    fn fresh_caps() -> Arc<atd_runtime::CapabilitySet> {
        Arc::new(atd_runtime::CapabilitySet::empty())
    }

    /// Build a minimal `ToolDefinition` for stubs with sensible defaults.
    fn stub_def(id: &str, domain: &str) -> atd_protocol::ToolDefinition {
        use atd_protocol::{
            BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources, ToolSafety,
            ToolTrust, ToolVisibility, TrustLevel,
        };
        atd_protocol::ToolDefinition {
            id: id.into(),
            name: id.into(),
            description: "stub tool for dispatch tests".into(),
            version: "0.0.0".into(),
            capability: ToolCapability {
                domain: domain.into(),
                actions: vec![],
                tags: vec![],
                intent_examples: vec![],
            },
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            bindings: vec![ToolBinding {
                protocol: BindingProtocol::Cli,
                config: serde_json::json!({}),
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

    /// Stub that mimics `ref:echo.say`: returns `{"echoed": args}`.
    struct EchoStub {
        def: atd_protocol::ToolDefinition,
    }
    impl EchoStub {
        fn new() -> Self {
            Self {
                def: stub_def("ref:echo.say", "echo"),
            }
        }
    }
    impl Tool for EchoStub {
        fn definition(&self) -> &atd_protocol::ToolDefinition {
            &self.def
        }
        fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
            Box::pin(async move { Ok(serde_json::json!({ "echoed": args })) })
        }
    }

    #[derive(Clone, Copy)]
    enum FailureMode {
        InvalidArgs,
        ExecutionFailed,
        InternalError,
    }

    struct FailingTool {
        def: atd_protocol::ToolDefinition,
        mode: FailureMode,
    }
    impl FailingTool {
        fn new(id: &str, mode: FailureMode) -> Self {
            Self {
                def: stub_def(id, "test"),
                mode,
            }
        }
    }
    impl Tool for FailingTool {
        fn definition(&self) -> &atd_protocol::ToolDefinition {
            &self.def
        }
        fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
            let mode = self.mode;
            Box::pin(async move {
                match mode {
                    FailureMode::InvalidArgs => Err(ToolCallError::InvalidArgs("bad field".into())),
                    FailureMode::ExecutionFailed => Err(ToolCallError::ExecutionFailed {
                        code: "EPERM".into(),
                        message: "denied".into(),
                        retryable: false,
                    }),
                    FailureMode::InternalError => Err(ToolCallError::InternalError("bug".into())),
                }
            })
        }
    }

    fn test_state_with(tools: Vec<Arc<dyn Tool>>) -> Arc<ServerState> {
        let mut reg = Registry::new();
        for t in tools {
            reg.register(t);
        }
        Arc::new(ServerState {
            registry: reg,
            config: ServerConfig {
                socket_path: PathBuf::from("/tmp/unused-in-dispatch-tests.sock"),
                cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                max_output_bytes: 1_048_576,
                default_call_timeout_ms: 60_000,
                granted_capabilities: vec![],
                audit_sink: None,
                server_version: "atd-server-test 0.0.0".into(),
            },
            tier_policy: atd_runtime::TierPolicy::defaults(),
            middleware: vec![],
        })
    }

    fn test_state() -> Arc<ServerState> {
        test_state_with(vec![Arc::new(EchoStub::new())])
    }

    #[tokio::test]
    async fn ping_returns_pong() {
        let s = test_state();
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::Ping,
        )
        .await;
        assert!(matches!(r, Response::Pong));
    }

    #[tokio::test]
    async fn tool_list_returns_registered_summaries() {
        let s = test_state();
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::ToolList,
        )
        .await;
        match r {
            Response::ToolListResponse { tools } => {
                let arr = tools.as_array().unwrap();
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0]["id"], "ref:echo.say");
            }
            _ => panic!("wrong variant"),
        }
    }

    /// Hidden visibility tools are filtered out of `Request::ToolList`,
    /// but `Request::ToolSchema` and `Request::RunTool` still work by id.
    #[tokio::test]
    async fn tool_list_excludes_hidden_visibility() {
        struct HiddenStub {
            def: atd_protocol::ToolDefinition,
        }
        impl HiddenStub {
            fn new() -> Self {
                let mut def = stub_def("ref:test.hidden_op", "test");
                def.visibility = atd_protocol::ToolVisibility::Hidden;
                Self { def }
            }
        }
        impl Tool for HiddenStub {
            fn definition(&self) -> &atd_protocol::ToolDefinition {
                &self.def
            }
            fn call<'a>(
                &'a self,
                _args: serde_json::Value,
                _ctx: &'a CallContext,
            ) -> CallFuture<'a> {
                Box::pin(async { Ok(serde_json::json!({"ok": true})) })
            }
        }

        let state = test_state_with(vec![
            Arc::new(EchoStub::new()),
            Arc::new(HiddenStub::new()),
        ]);

        // (1) tool_list MUST exclude the Hidden tool.
        let list = dispatch(
            &state,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::ToolList,
        )
        .await;
        match list {
            Response::ToolListResponse { tools } => {
                let arr = tools.as_array().unwrap();
                assert_eq!(arr.len(), 1, "Hidden tool leaked into tool_list: {arr:?}");
                assert_eq!(arr[0]["id"], "ref:echo.say");
            }
            _ => panic!("wrong variant"),
        }

        // (2) tool_schema by id MUST still describe the Hidden tool.
        let schema = dispatch(
            &state,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::ToolSchema {
                tool_id: "ref:test.hidden_op".into(),
            },
        )
        .await;
        match schema {
            Response::ToolSchemaResponse { schema } => {
                assert_eq!(schema["id"], "ref:test.hidden_op");
                assert_eq!(schema["visibility"], "hidden");
            }
            other => panic!("expected ToolSchemaResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tool_schema_found_returns_definition() {
        let s = test_state();
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::ToolSchema {
                tool_id: "ref:echo.say".into(),
            },
        )
        .await;
        match r {
            Response::ToolSchemaResponse { schema } => {
                assert_eq!(schema["id"], "ref:echo.say");
                assert_eq!(schema["capability"]["domain"], "echo");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn tool_schema_not_found_returns_error() {
        let s = test_state();
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::ToolSchema {
                tool_id: "ref:missing".into(),
            },
        )
        .await;
        match r {
            Response::Error { message, .. } => {
                assert!(message.contains("tool not found"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn run_tool_success_wraps_data() {
        let s = test_state();
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::RunTool {
                tool_id: "ref:echo.say".into(),
                args: serde_json::json!({"k": "v"}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::ToolResultResponse {
                result,
                success,
                dry_run,
                ..
            } => {
                assert!(success);
                assert!(!dry_run);
                assert_eq!(result["echoed"]["k"], "v");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn run_tool_dry_run_returns_preview_without_calling_tool() {
        let s = test_state();
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::RunTool {
                tool_id: "ref:echo.say".into(),
                args: serde_json::json!({"x": 1}),
                dry_run: true,
            },
        )
        .await;
        match r {
            Response::ToolResultResponse {
                result,
                success,
                dry_run,
                ..
            } => {
                assert!(success);
                assert!(dry_run);
                assert_eq!(result["dry_run"], serde_json::json!(true));
                assert_eq!(result["args_preview"]["x"], 1);
                assert!(result.get("echoed").is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn run_tool_unknown_id_returns_error() {
        let s = test_state();
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::RunTool {
                tool_id: "ref:missing".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::Error { message, .. } => {
                assert!(message.contains("tool not found"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn run_tool_invalid_args_error_maps_to_error_response() {
        let s = test_state_with(vec![Arc::new(FailingTool::new(
            "test:invalid",
            FailureMode::InvalidArgs,
        ))]);
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::RunTool {
                tool_id: "test:invalid".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::Error { message, .. } => {
                assert!(message.contains("invalid args for test:invalid"));
                assert!(message.contains("bad field"));
            }
            _ => panic!("wrong variant, expected Response::Error"),
        }
    }

    #[tokio::test]
    async fn run_tool_execution_failed_maps_to_tool_result_success_false() {
        let s = test_state_with(vec![Arc::new(FailingTool::new(
            "test:exec",
            FailureMode::ExecutionFailed,
        ))]);
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::RunTool {
                tool_id: "test:exec".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::ToolResultResponse {
                result,
                success,
                dry_run,
                tool_id,
            } => {
                assert!(!success);
                assert!(!dry_run);
                assert_eq!(tool_id, "test:exec");
                assert_eq!(result["code"], "EPERM");
                assert_eq!(result["message"], "denied");
                assert_eq!(result["retryable"], serde_json::json!(false));
            }
            _ => panic!("wrong variant, expected Response::ToolResultResponse"),
        }
    }

    #[tokio::test]
    async fn run_tool_internal_error_maps_to_error_response() {
        let s = test_state_with(vec![Arc::new(FailingTool::new(
            "test:internal",
            FailureMode::InternalError,
        ))]);
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut None,
            Request::RunTool {
                tool_id: "test:internal".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::Error { message, .. } => {
                assert!(message.contains("internal error in test:internal"));
                assert!(message.contains("bug"));
            }
            _ => panic!("wrong variant, expected Response::Error"),
        }
    }
}
