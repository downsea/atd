//! Per-connection task: read frames, dispatch, write responses.
//!
//! SP-streamable-http §6.3: the dispatch body that used to live here moved
//! to `atd_runtime::dispatch::dispatch_request` so the HTTP listener
//! (`atd-server-http`) can drive the same state machine over the same
//! `Arc<Registry>` / `SharedServerConfig`. This module is now a thin
//! transport layer: read length-prefixed JSON frames from the Unix
//! stream, forward to runtime dispatch, write the response frame.

use std::sync::Arc;

use tokio::net::UnixStream;

use atd_protocol::wire::{read_frame, write_frame};
use atd_protocol::{Request, Response};

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

/// UDS dispatch entry point — kept as a `pub(crate)` wrapper so the
/// existing test suite (the 16+ unit tests below) continues to address it
/// by name. The body is the canonical
/// `atd_runtime::dispatch::dispatch_request`; the wrapper exists only to
/// document the UDS contract (per-connection `caps` + `caller_id` are
/// mutated in place when `req == Hello`) and to keep the import paths
/// for the test suite stable across the SP-streamable-http refactor.
pub(crate) async fn dispatch(
    state: &Arc<ServerState>,
    tracker: &Arc<atd_runtime::ReadTracker>,
    caps: &mut Arc<atd_runtime::CapabilitySet>,
    caller_id: &mut Option<String>,
    req: Request,
) -> Response {
    atd_runtime::dispatch::dispatch_request(state, tracker, caps, caller_id, req).await
}

#[cfg(test)]
mod tests {
    //! Dispatch unit tests using stub tools (no atd-tools-* deps).
    //!
    //! These cover the dispatch state-machine for each Request variant. The
    //! 9-built-in-tool integration tests live in `atd-ref-server/tests/`.
    //!
    //! SP-streamable-http §6.3: tests address the local `dispatch` wrapper
    //! by name, which forwards to `atd_runtime::dispatch::dispatch_request`.
    //! Behaviour is unchanged from the pre-refactor inline implementation —
    //! these tests are the regression guard for that promise.

    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    use atd_runtime::context::CallContext;
    use atd_runtime::dispatch::SharedServerConfig;
    use atd_runtime::error::ToolCallError;
    use atd_runtime::registry::{CallFuture, Registry, Tool};

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

    fn shared_test_config() -> SharedServerConfig {
        SharedServerConfig {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            default_call_timeout_ms: 60_000,
            granted_capabilities: vec![],
            audit_sink: None,
            server_version: "atd-server-test 0.0.0".into(),
            token_broker: None,
        }
    }

    fn test_state_with(tools: Vec<Arc<dyn Tool>>) -> Arc<ServerState> {
        let mut reg = Registry::new();
        for t in tools {
            reg.register(t);
        }
        Arc::new(ServerState {
            registry: reg,
            config: shared_test_config(),
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

        let state = test_state_with(vec![Arc::new(EchoStub::new()), Arc::new(HiddenStub::new())]);

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

    // ---- SP-token-broker-phase1: dispatch wiring tests ----

    /// A stub tool that asserts on whether `ctx.secrets()` was populated.
    /// Returns the name of the secret it found (or "none") in the response
    /// so the test can verify both the propagation and the value.
    struct SecretInspectorTool {
        def: atd_protocol::ToolDefinition,
    }
    impl SecretInspectorTool {
        fn new() -> Self {
            Self {
                def: stub_def("test:secret_inspector", "test"),
            }
        }
    }
    impl Tool for SecretInspectorTool {
        fn definition(&self) -> &atd_protocol::ToolDefinition {
            &self.def
        }
        fn call<'a>(&'a self, _args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
            let observed = ctx
                .secrets()
                .and_then(|b| b.get("oauth_token"))
                .map(|v| v.expose().to_string());
            Box::pin(async move {
                Ok(serde_json::json!({
                    "saw_secret": observed.is_some(),
                    "value": observed,
                }))
            })
        }
    }

    fn test_state_with_broker(broker: Arc<dyn atd_runtime::TokenBroker>) -> Arc<ServerState> {
        let mut reg = Registry::new();
        reg.register(Arc::new(SecretInspectorTool::new()));
        let mut config = shared_test_config();
        config.token_broker = Some(broker);
        Arc::new(ServerState {
            registry: reg,
            config,
            tier_policy: atd_runtime::TierPolicy::defaults(),
            middleware: vec![],
        })
    }

    #[tokio::test]
    async fn dispatch_with_broker_propagates_secrets_to_tool() {
        use atd_runtime::secrets::{InMemoryTokenBroker, RedactedString, SecretBundle};
        let mut bundle = SecretBundle::new();
        bundle.insert("oauth_token".into(), RedactedString::new("tok-for-agent-A"));
        let mut broker = InMemoryTokenBroker::new();
        broker.insert("agent-A", bundle);

        let state = test_state_with_broker(Arc::new(broker));
        // Hello first to set caller_id = "agent-A".
        let _ = dispatch(
            &state,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut Some("agent-A".into()),
            Request::Hello {
                client_id: Some("agent-A".into()),
                requested_capabilities: vec![],
                ucan_tokens: vec![],
            },
        )
        .await;

        let mut caller = Some("agent-A".to_string());
        let r = dispatch(
            &state,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut caller,
            Request::RunTool {
                tool_id: "test:secret_inspector".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::ToolResultResponse {
                result, success, ..
            } => {
                assert!(success);
                assert_eq!(result["saw_secret"], serde_json::Value::Bool(true));
                assert_eq!(result["value"], "tok-for-agent-A");
            }
            other => panic!("expected ToolResultResponse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_without_broker_leaves_secrets_none() {
        // No broker on ServerConfig — falls back to the default test_state.
        let mut reg = Registry::new();
        reg.register(Arc::new(SecretInspectorTool::new()));
        let state = Arc::new(ServerState {
            registry: reg,
            config: shared_test_config(),
            tier_policy: atd_runtime::TierPolicy::defaults(),
            middleware: vec![],
        });

        let mut caller = Some("agent-A".to_string());
        let r = dispatch(
            &state,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut caller,
            Request::RunTool {
                tool_id: "test:secret_inspector".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::ToolResultResponse {
                result, success, ..
            } => {
                assert!(success);
                assert_eq!(result["saw_secret"], serde_json::Value::Bool(false));
            }
            other => panic!("expected ToolResultResponse, got {other:?}"),
        }
    }

    /// A broker that always errors. Used to exercise ERR_BROKER_FAILED.
    struct AlwaysErrorBroker;
    impl atd_runtime::TokenBroker for AlwaysErrorBroker {
        fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> atd_runtime::ResolveFuture<'a> {
            Box::pin(async {
                Err(atd_runtime::secrets::BrokerError::Lookup(
                    "synthetic test failure".into(),
                ))
            })
        }
    }

    #[tokio::test]
    async fn dispatch_with_broker_lookup_failure_returns_broker_error() {
        let state = test_state_with_broker(Arc::new(AlwaysErrorBroker));
        let mut caller = Some("agent-A".to_string());
        let r = dispatch(
            &state,
            &fresh_tracker(),
            &mut fresh_caps(),
            &mut caller,
            Request::RunTool {
                tool_id: "test:secret_inspector".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::Error {
                code, retryable, ..
            } => {
                assert_eq!(code, Some(atd_protocol::ERR_BROKER_FAILED));
                assert_eq!(retryable, Some(true));
            }
            other => panic!("expected Response::Error with broker code, got {other:?}"),
        }
    }
}
