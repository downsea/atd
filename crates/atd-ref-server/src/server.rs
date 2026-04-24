//! Server loop + request dispatcher.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::{UnixListener, UnixStream};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::protocol::{Request, Response};
use crate::registry::Registry;
use crate::wire::{read_frame, write_frame};

pub struct ServerConfig {
    pub socket_path: PathBuf,
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub default_call_timeout_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            socket_path: PathBuf::from(home).join(".atd-ref").join("server.sock"),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            default_call_timeout_ms: 60_000,
        }
    }
}

pub struct Server {
    state: Arc<ServerState>,
}

pub(crate) struct ServerState {
    pub(crate) registry: Registry,
    pub(crate) config: ServerConfig,
}

impl Server {
    pub fn new(registry: Registry, config: ServerConfig) -> Self {
        Self {
            state: Arc::new(ServerState { registry, config }),
        }
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
    let tracker = Arc::new(crate::tracker::ReadTracker::new());  // per-connection
    loop {
        let req: Request = match read_frame(&mut reader).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let resp = dispatch(&state, &tracker, req).await;
        write_frame(&mut writer, &resp).await?;
    }
}

pub(crate) async fn dispatch(
    state: &Arc<ServerState>,
    tracker: &Arc<crate::tracker::ReadTracker>,
    req: Request,
) -> Response {
    match req {
        Request::Ping => Response::Pong,
        // SP-12 Task 1: stub Hello handler — always grants nothing.
        // Task 2 intersects `requested_capabilities` with the server's
        // `--grant-capability` set and stores the result on the connection.
        Request::Hello { client_id: _, requested_capabilities: _ } => Response::HelloAck {
            granted_capabilities: vec![],
            server_version: concat!("atd-ref-server ", env!("CARGO_PKG_VERSION")).to_string(),
            supported_tiers: vec!["hot".into(), "warm".into(), "cold".into()],
        },
        Request::ToolList => {
            let summaries = state.registry.summaries();
            Response::ToolList {
                tools: serde_json::to_value(&summaries).unwrap_or_else(|_| serde_json::json!([])),
            }
        }
        Request::ToolSchema { tool_id } => match state.registry.get(&tool_id) {
            Some(tool) => Response::ToolSchema {
                schema: serde_json::to_value(tool.definition())
                    .unwrap_or_else(|_| serde_json::json!({})),
            },
            None => Response::Error {
                message: format!("tool not found: {tool_id}"),
                code: None,
                retryable: Some(false),
                details: None,
            },
        },
        Request::RunTool { tool_id, args, dry_run } => {
            if dry_run {
                return Response::ToolResult {
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
            let tool = match state.registry.get(&tool_id) {
                Some(t) => t.clone(),
                None => {
                    return Response::Error {
                        message: format!("tool not found: {tool_id}"),
                        code: None,
                        retryable: Some(false),
                        details: None,
                    };
                }
            };
            let ctx = CallContext {
                cwd: state.config.cwd.clone(),
                max_output_bytes: state.config.max_output_bytes,
                call_id: ulid::Ulid::new(),
                deadline: Some(
                    Instant::now() + Duration::from_millis(state.config.default_call_timeout_ms),
                ),
                read_tracker: Some(tracker.clone()),
                // SP-12 Task 1: empty capability set + Warm tier preserves
                // current behavior. Task 2 wires `Hello`-derived caps; Task 3
                // derives tier from the tool definition.
                capabilities: std::sync::Arc::new(crate::capability::CapabilitySet::empty()),
                tier: crate::tier::ToolTier::Warm,
            };
            match tool.call(args, &ctx).await {
                Ok(data) => Response::ToolResult {
                    tool_id,
                    result: data,
                    success: true,
                    dry_run: false,
                },
                Err(ToolCallError::InvalidArgs(msg)) => Response::Error {
                    message: format!("invalid args for {tool_id}: {msg}"),
                    code: None,
                    retryable: Some(false),
                    details: None,
                },
                Err(ToolCallError::ExecutionFailed { code, message, retryable }) => {
                    Response::ToolResult {
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
                Err(ToolCallError::InternalError(msg)) => Response::Error {
                    message: format!("internal error in {tool_id}: {msg}"),
                    code: None,
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
    use crate::registry::{CallFuture, Tool};

    fn fresh_tracker() -> Arc<crate::tracker::ReadTracker> {
        Arc::new(crate::tracker::ReadTracker::new())
    }

    fn test_state() -> Arc<ServerState> {
        Arc::new(ServerState {
            registry: builtin_registry(),
            config: ServerConfig {
                socket_path: PathBuf::from("/tmp/unused-in-dispatch-tests.sock"),
                cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                max_output_bytes: 1_048_576,
                default_call_timeout_ms: 60_000,
            },
        })
    }

    #[tokio::test]
    async fn ping_returns_pong() {
        let s = test_state();
        let r = dispatch(&s, &fresh_tracker(), Request::Ping).await;
        assert!(matches!(r, Response::Pong));
    }

    #[tokio::test]
    async fn tool_list_returns_registered_summaries() {
        let s = test_state();
        let r = dispatch(&s, &fresh_tracker(), Request::ToolList).await;
        match r {
            Response::ToolList { tools } => {
                let arr = tools.as_array().unwrap();
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
            Request::ToolSchema { tool_id: "ref:echo.say".into() },
        )
        .await;
        match r {
            Response::ToolSchema { schema } => {
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
            Request::ToolSchema { tool_id: "ref:missing".into() },
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
            Request::RunTool {
                tool_id: "ref:echo.say".into(),
                args: serde_json::json!({"k": "v"}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::ToolResult { result, success, dry_run, .. } => {
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
            Request::RunTool {
                tool_id: "ref:echo.say".into(),
                args: serde_json::json!({"x": 1}),
                dry_run: true,
            },
        )
        .await;
        match r {
            Response::ToolResult { result, success, dry_run, .. } => {
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
        def: atd_types::ToolDefinition,
        mode: FailureMode,
    }

    impl FailingTool {
        fn new(id: &str, mode: FailureMode) -> Self {
            use atd_types::{
                BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources,
                ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
            };
            Self {
                def: atd_types::ToolDefinition {
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
                },
                mode,
            }
        }
    }

    impl Tool for FailingTool {
        fn definition(&self) -> &atd_types::ToolDefinition {
            &self.def
        }
        fn call<'a>(
            &'a self,
            _args: serde_json::Value,
            _ctx: &'a CallContext,
        ) -> CallFuture<'a> {
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
            },
        })
    }

    #[tokio::test]
    async fn run_tool_invalid_args_error_maps_to_error_response() {
        let s = state_with_failing_tool("test:invalid", FailureMode::InvalidArgs);
        let r = dispatch(
            &s,
            &fresh_tracker(),
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
            Request::RunTool {
                tool_id: "test:exec".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::ToolResult { result, success, dry_run, tool_id } => {
                assert!(!success);
                assert!(!dry_run);
                assert_eq!(tool_id, "test:exec");
                assert_eq!(result["code"], "EPERM");
                assert_eq!(result["message"], "denied");
                assert_eq!(result["retryable"], serde_json::json!(false));
            }
            _ => panic!("wrong variant, expected Response::ToolResult"),
        }
    }

    #[tokio::test]
    async fn run_tool_internal_error_maps_to_error_response() {
        let s = state_with_failing_tool("test:internal", FailureMode::InternalError);
        let r = dispatch(
            &s,
            &fresh_tracker(),
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
