//! Binding abstraction.
//!
//! A `Binding` is *how* a tool's semantics are realized — native in-process
//! (wrapping a `Tool` impl), CLI subprocess, and later MCP/REST/AppFunction.
//! Dispatch resolves `tool_id` to a `(Tool, Binding)` pair and invokes
//! `Binding::call`; `NativeBinding` simply delegates back to the `Tool`, so
//! all 9 existing tools keep working with zero behavior change.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use atd_protocol::ToolDefinition;

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::Tool;

/// Boxed future returned by `Binding::call`. Shape mirrors `registry::CallFuture`
/// so the two can be freely composed.
pub type BindingFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolCallError>> + Send + 'a>>;

/// A tool's execution binding. `name()` returns a short discriminator
/// (`"native"`, `"cli"`, `"mcp"`, ...) used by observability hooks and tests.
pub trait Binding: Send + Sync {
    fn name(&self) -> &'static str;

    fn call<'a>(
        &'a self,
        tool_def: &'a ToolDefinition,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> BindingFuture<'a>;
}

/// Default binding: delegate directly to the `Tool::call` implementation.
/// Assigned to every tool registered via `Registry::register`; the 9 built-in
/// tools continue to run through it.
pub struct NativeBinding {
    tool: Arc<dyn Tool>,
}

impl NativeBinding {
    pub fn new(tool: Arc<dyn Tool>) -> Self {
        Self { tool }
    }
}

impl Binding for NativeBinding {
    fn name(&self) -> &'static str {
        "native"
    }

    fn call<'a>(
        &'a self,
        _tool_def: &'a ToolDefinition,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> BindingFuture<'a> {
        self.tool.call(args, ctx)
    }
}

/// Spawn a subprocess to realize the tool. Demonstrates that dispatch can
/// route a single `tool_id` to an external program without the `Tool` impl
/// carrying the execution logic.
///
/// `args_mapper` is a function pointer (not a closure) so `CliBinding` stays
/// `Send + Sync` without interior mutability. SP-12 ships one mapper
/// (`ref:external.uname`); a future refactor can swap this for a trait
/// object if more than one CLI-backed tool needs configuration.
pub struct CliBinding {
    pub program: PathBuf,
    pub base_args: Vec<String>,
    pub args_mapper: fn(&serde_json::Value) -> Vec<String>,
}

impl Binding for CliBinding {
    fn name(&self) -> &'static str {
        "cli"
    }

    fn call<'a>(
        &'a self,
        _tool_def: &'a ToolDefinition,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> BindingFuture<'a> {
        let program = self.program.clone();
        let base = self.base_args.clone();
        let mapper = self.args_mapper;
        // Respect the dispatch-provided deadline. Fall back to a 5 s cap if
        // the CallContext carries none (unusual — the server always attaches
        // one).
        let budget = ctx
            .remaining_time()
            .unwrap_or(std::time::Duration::from_secs(5));
        Box::pin(async move {
            let mut argv = base;
            argv.extend(mapper(&args));
            let fut = tokio::process::Command::new(&program).args(&argv).output();
            let output = match tokio::time::timeout(budget, fut).await {
                Ok(Ok(o)) => o,
                Ok(Err(e)) => {
                    return Err(ToolCallError::InternalError(format!(
                        "cli binding failed to spawn {:?}: {e}",
                        program
                    )));
                }
                Err(_) => {
                    return Err(ToolCallError::ExecutionFailed {
                        code: "TIMEOUT".into(),
                        message: "cli binding deadline exceeded".into(),
                        retryable: false,
                    });
                }
            };
            if !output.status.success() {
                return Err(ToolCallError::ExecutionFailed {
                    code: format!("EXIT_{}", output.status.code().unwrap_or(-1)),
                    message: String::from_utf8_lossy(&output.stderr).into_owned(),
                    retryable: false,
                });
            }
            Ok(serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).into_owned(),
                "exit_code": output.status.code().unwrap_or(0),
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::CallFuture;

    struct PassthroughTool {
        def: ToolDefinition,
    }
    impl PassthroughTool {
        fn new() -> Self {
            use atd_protocol::{
                BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources,
                ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
            };
            Self {
                def: ToolDefinition {
                    id: "test:passthrough".into(),
                    name: "passthrough".into(),
                    description: "echoes native-binding marker".into(),
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
                    errors: vec![],
                },
            }
        }
    }
    impl Tool for PassthroughTool {
        fn definition(&self) -> &ToolDefinition {
            &self.def
        }
        fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
            Box::pin(async { Ok(serde_json::json!({"native": true})) })
        }
    }

    #[tokio::test]
    async fn native_binding_delegates_to_tool_call() {
        let tool = Arc::new(PassthroughTool::new());
        let binding = NativeBinding::new(tool.clone());
        assert_eq!(binding.name(), "native");
        let ctx = CallContext::for_test();
        let r = binding
            .call(tool.definition(), serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(r["native"], true);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cli_binding_runs_true_program_succeeds() {
        let tool_def = PassthroughTool::new().def;
        let binding = CliBinding {
            program: PathBuf::from("/bin/true"),
            base_args: vec![],
            args_mapper: |_| vec![],
        };
        assert_eq!(binding.name(), "cli");
        let ctx = CallContext::for_test();
        let r = binding
            .call(&tool_def, serde_json::json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(r["exit_code"], 0);
        assert_eq!(r["stdout"], "");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cli_binding_surfaces_nonzero_exit_as_execution_failed() {
        let tool_def = PassthroughTool::new().def;
        let binding = CliBinding {
            program: PathBuf::from("/bin/false"),
            base_args: vec![],
            args_mapper: |_| vec![],
        };
        let ctx = CallContext::for_test();
        let err = binding
            .call(&tool_def, serde_json::json!({}), &ctx)
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed {
                code, retryable, ..
            } => {
                assert!(code.starts_with("EXIT_"));
                assert!(!retryable);
            }
            other => panic!("expected ExecutionFailed, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cli_binding_times_out_when_sleep_exceeds_deadline() {
        let tool_def = PassthroughTool::new().def;
        let binding = CliBinding {
            program: PathBuf::from("/bin/sleep"),
            base_args: vec!["5".into()],
            args_mapper: |_| vec![],
        };
        let mut ctx = CallContext::for_test();
        ctx.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
        let err = binding
            .call(&tool_def, serde_json::json!({}), &ctx)
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "TIMEOUT"),
            other => panic!("expected TIMEOUT, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cli_binding_args_mapper_propagates_flags() {
        let tool_def = PassthroughTool::new().def;
        let binding = CliBinding {
            program: PathBuf::from("/bin/echo"),
            base_args: vec![],
            args_mapper: |args| {
                let mut out = vec!["-n".to_string()];
                if let Some(s) = args.get("msg").and_then(|v| v.as_str()) {
                    out.push(s.to_string());
                }
                out
            },
        };
        let ctx = CallContext::for_test();
        let r = binding
            .call(&tool_def, serde_json::json!({"msg": "hi"}), &ctx)
            .await
            .unwrap();
        assert_eq!(r["stdout"], "hi");
    }
}
