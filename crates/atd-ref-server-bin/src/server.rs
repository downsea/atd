//! Server loop + request dispatcher.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::{UnixListener, UnixStream};

use atd_protocol::wire::{read_frame, write_frame};
use atd_protocol::{Request, Response};
use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::Registry;

pub struct ServerConfig {
    pub socket_path: PathBuf,
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub default_call_timeout_ms: u64,
    /// Server-operator allow-list. Capabilities a client may ask for in
    /// `Hello`; anything not in this list is refused. Empty (default) means
    /// no client can hold any capability, so tools with
    /// `required_capabilities != []` cannot be called — matching the fail-
    /// closed policy for SP-12.
    pub granted_capabilities: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            socket_path: PathBuf::from(home).join(".atd-ref").join("server.sock"),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            default_call_timeout_ms: 60_000,
            granted_capabilities: vec![],
        }
    }
}

pub struct Server {
    state: Arc<ServerState>,
}

pub(crate) struct ServerState {
    pub(crate) registry: Registry,
    pub(crate) config: ServerConfig,
    pub(crate) tier_policy: atd_runtime::TierPolicy,
    pub(crate) middleware: Vec<Arc<dyn atd_runtime::Middleware>>,
}

impl Server {
    pub fn new(registry: Registry, config: ServerConfig) -> Self {
        Self {
            state: Arc::new(ServerState {
                registry,
                config,
                tier_policy: atd_runtime::TierPolicy::defaults(),
                middleware: Vec::new(),
            }),
        }
    }

    /// Replace the tier policy. Valid only before `run()` — after the server
    /// starts, `state` has already been handed to connection tasks and is
    /// effectively immutable. Tests and CLI startup call this once.
    pub fn set_tier_policy(&mut self, policy: atd_runtime::TierPolicy) {
        let state = Arc::get_mut(&mut self.state)
            .expect("set_tier_policy must be called before run() hands out Arcs");
        state.tier_policy = policy;
    }

    /// Install the result-middleware chain. Order matters: first registered
    /// runs first. Must be called before `run()` for the same reason as
    /// `set_tier_policy` — `state` becomes shared when connections spawn.
    pub fn set_middleware(&mut self, middleware: Vec<Arc<dyn atd_runtime::Middleware>>) {
        let state = Arc::get_mut(&mut self.state)
            .expect("set_middleware must be called before run() hands out Arcs");
        state.middleware = middleware;
    }

    pub async fn run(self) -> std::io::Result<()> {
        let sock = &self.state.config.socket_path;

        // Ensure parent dir exists.
        if let Some(parent) = sock.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Remove stale socket.
        if sock.exists() {
            std::fs::remove_file(sock)?;
        }

        let listener = UnixListener::bind(sock)?;
        // Unix 0600: owner-only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(sock, perms);
        }

        eprintln!(
            "atd-ref-server: listening on {:?} ({} tool(s) registered)",
            sock,
            self.state.registry.count()
        );

        loop {
            let (stream, _) = listener.accept().await?;
            let state = self.state.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(state, stream).await {
                    eprintln!("atd-ref-server: connection error: {e}");
                }
            });
        }
    }
}

async fn handle_connection(state: Arc<ServerState>, stream: UnixStream) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let tracker = Arc::new(atd_runtime::ReadTracker::new()); // per-connection
    // Per-connection capability set, replaced on `Hello`. Default: empty.
    let mut caps: Arc<atd_runtime::CapabilitySet> = Arc::new(atd_runtime::CapabilitySet::empty());
    loop {
        let req: Request = match read_frame(&mut reader).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let resp = dispatch(&state, &tracker, &mut caps, req).await;
        write_frame(&mut writer, &resp).await?;
    }
}

pub(crate) async fn dispatch(
    state: &Arc<ServerState>,
    tracker: &Arc<atd_runtime::ReadTracker>,
    caps: &mut Arc<atd_runtime::CapabilitySet>,
    req: Request,
) -> Response {
    match req {
        Request::Ping => Response::Pong,
        // SP-12 Task 2: intersect requested capabilities with the server's
        // allow-list; store the granted subset on the connection. Replying with
        // the subset (not the full server set) lets clients discover what they
        // actually hold.
        Request::Hello {
            client_id: _,
            requested_capabilities,
        } => {
            let allow = atd_runtime::CapabilitySet::from_iter(
                state.config.granted_capabilities.iter().cloned(),
            );
            let (granted, _denied) = allow.intersect(&requested_capabilities);
            *caps = Arc::new(atd_runtime::CapabilitySet::from_iter(granted.clone()));
            Response::HelloAck {
                granted_capabilities: granted,
                server_version: concat!("atd-ref-server ", env!("CARGO_PKG_VERSION")).to_string(),
                supported_tiers: vec!["hot".into(), "warm".into(), "cold".into()],
            }
        }
        Request::ToolList => {
            let summaries = state.registry.summaries();
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
            if dry_run {
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
                    return Response::Error {
                        message: format!("tool not found: {tool_id}"),
                        code: None,
                        retryable: Some(false),
                        details: None,
                    };
                }
            };
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
            // SP-12 Task 3: derive tier from the tool definition; TierPolicy
            // maps each tier to deadline + max_output budgets. Tools without
            // a tier field default to Warm (spec §8 Q5), preserving pre-SP-12
            // behavior for the 9 built-in tools that never set tier.
            let tier = entry
                .definition()
                .tier
                .unwrap_or(atd_runtime::tier::ToolTier::Warm);
            let tier_timeout = state.tier_policy.timeout(tier);
            let tier_max_output = state.tier_policy.max_output(tier);

            let ctx = CallContext {
                cwd: state.config.cwd.clone(),
                max_output_bytes: tier_max_output,
                call_id: ulid::Ulid::new(),
                deadline: Some(Instant::now() + tier_timeout),
                read_tracker: Some(tracker.clone()),
                capabilities: caps.clone(),
                tier,
            };
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
                    Response::ToolResultResponse {
                        tool_id,
                        result: data,
                        success: true,
                        dry_run: false,
                    }
                }
                Err(ToolCallError::InvalidArgs(msg)) => Response::Error {
                    message: format!("invalid args for {tool_id}: {msg}"),
                    code: None,
                    retryable: Some(false),
                    details: None,
                },
                Err(ToolCallError::ExecutionFailed {
                    code,
                    message,
                    retryable,
                }) => Response::ToolResultResponse {
                    tool_id,
                    result: serde_json::json!({
                        "code": code,
                        "message": message,
                        "retryable": retryable,
                    }),
                    success: false,
                    dry_run: false,
                },
                Err(ToolCallError::InternalError(msg)) => Response::Error {
                    message: format!("internal error in {tool_id}: {msg}"),
                    code: None,
                    retryable: Some(false),
                    details: None,
                },
                Err(other) => Response::Error {
                    message: format!("unhandled tool error in {tool_id}: {other}"),
                    code: Some(1999),
                    retryable: Some(false),
                    details: None,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::builtin_registry;
    use atd_runtime::registry::{CallFuture, Tool};

    fn fresh_tracker() -> Arc<atd_runtime::ReadTracker> {
        Arc::new(atd_runtime::ReadTracker::new())
    }

    /// Empty capability set, wrapped in `Arc`. Used by dispatch tests that
    /// don't exercise the capability gate; callers that do should build a
    /// populated one directly.
    fn fresh_caps() -> Arc<atd_runtime::CapabilitySet> {
        Arc::new(atd_runtime::CapabilitySet::empty())
    }

    fn test_state() -> Arc<ServerState> {
        Arc::new(ServerState {
            registry: builtin_registry(false),
            config: ServerConfig {
                socket_path: PathBuf::from("/tmp/unused-in-dispatch-tests.sock"),
                cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                max_output_bytes: 1_048_576,
                default_call_timeout_ms: 60_000,
                granted_capabilities: vec![],
            },
            tier_policy: atd_runtime::TierPolicy::defaults(),
            middleware: vec![],
        })
    }

    #[tokio::test]
    async fn ping_returns_pong() {
        let s = test_state();
        let r = dispatch(&s, &fresh_tracker(), &mut fresh_caps(), Request::Ping).await;
        assert!(matches!(r, Response::Pong));
    }

    #[tokio::test]
    async fn tool_list_returns_registered_summaries() {
        let s = test_state();
        let r = dispatch(&s, &fresh_tracker(), &mut fresh_caps(), Request::ToolList).await;
        match r {
            Response::ToolListResponse { tools } => {
                let arr = tools.as_array().unwrap();
                // SP-12: +1 for ref:external.uname on unix.
                #[cfg(unix)]
                assert_eq!(arr.len(), 10);
                #[cfg(not(unix))]
                assert_eq!(arr.len(), 9);
                let ids: Vec<&str> = arr.iter().map(|t| t["id"].as_str().unwrap()).collect();
                assert!(ids.contains(&"ref:echo.say"));
                assert!(ids.contains(&"ref:fs.read"));
                assert!(ids.contains(&"ref:fs.write"));
                assert!(ids.contains(&"ref:fs.edit"));
                assert!(ids.contains(&"ref:fs.glob"));
                assert!(ids.contains(&"ref:fs.grep"));
                assert!(ids.contains(&"ref:shell.exec"));
                assert!(ids.contains(&"ref:shell.pwsh"));
                assert!(ids.contains(&"ref:web.fetch"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn tool_schema_found_returns_definition() {
        let s = test_state();
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
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

    // --- ToolCallError → Response mapping tests (spec §5.3) ---

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
            use atd_protocol::{
                BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources,
                ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
            };
            Self {
                def: atd_protocol::ToolDefinition {
                    id: id.into(),
                    name: id.into(),
                    description: "test failure tool".into(),
                    version: "0.0.0".into(),
                    capability: ToolCapability {
                        domain: "test".into(),
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
                },
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

    fn state_with_failing_tool(id: &str, mode: FailureMode) -> Arc<ServerState> {
        let mut reg = Registry::new();
        reg.register(Arc::new(FailingTool::new(id, mode)));
        Arc::new(ServerState {
            registry: reg,
            config: ServerConfig {
                socket_path: PathBuf::from("/tmp/unused.sock"),
                cwd: PathBuf::from("."),
                max_output_bytes: 1024,
                default_call_timeout_ms: 1000,
                granted_capabilities: vec![],
            },
            tier_policy: atd_runtime::TierPolicy::defaults(),
            middleware: vec![],
        })
    }

    #[tokio::test]
    async fn run_tool_invalid_args_error_maps_to_error_response() {
        let s = state_with_failing_tool("test:invalid", FailureMode::InvalidArgs);
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
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
        let s = state_with_failing_tool("test:exec", FailureMode::ExecutionFailed);
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
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
        let s = state_with_failing_tool("test:internal", FailureMode::InternalError);
        let r = dispatch(
            &s,
            &fresh_tracker(),
            &mut fresh_caps(),
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
