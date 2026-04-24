//! `ref:shell.exec` — POSIX shell command execution (bash -c).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::shared::{RunError, RunRequest, run};
use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Tool};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:shell.exec".into(),
        name: "Shell Execute".into(),
        description: "Run a command via `bash -c`. Captures stdout/stderr separately (each capped at ctx.max_output_bytes/2), returns the exit code. Nonzero exit is not a tool error — the agent interprets exit codes itself.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "shell".into(),
            actions: vec!["exec".into()],
            tags: vec!["shell".into(), "bash".into(), "subprocess".into()],
            intent_examples: vec![
                "run `ls -la`".into(),
                "list files matching '*.rs' via shell".into(),
            ],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command":  { "type": "string", "minLength": 1 },
                "grace_ms": { "type": "integer", "minimum": 0 }
            },
            "required": ["command"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "exit_code":        { "type": ["integer", "null"] },
                "stdout":           { "type": "string" },
                "stdout_truncated": { "type": "boolean" },
                "stderr":           { "type": "string" },
                "stderr_truncated": { "type": "boolean" },
                "duration_ms":      { "type": "integer" }
            }
        }),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Destructive,
            dry_run: false,
            side_effects: vec!["subprocess".into(), "filesystem".into(), "network".into()],
            data_sensitivity: Some("depends on command".into()),
        },
        resources: ToolResources {
            timeout_ms: 60_000,
            max_concurrent: 10,
            rate_limit_per_min: None,
            estimated_tokens: Some(500),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Dangerous,
        required_capabilities: vec![],
        tier: None,
    })
}

pub struct ShellExecTool;

impl ShellExecTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShellExecTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct ExecArgs {
    command: String,
    #[serde(default)]
    grace_ms: Option<u64>,
}

impl Tool for ShellExecTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            let args: ExecArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if args.command.trim().is_empty() {
                return Err(ToolCallError::InvalidArgs(
                    "command is empty or whitespace-only".into(),
                ));
            }

            let deadline = ctx.deadline.or_else(|| {
                // Fallback: small default if no server-side deadline was set
                Some(Instant::now() + Duration::from_secs(60))
            });

            let half = ctx.max_output_bytes / 2;
            let req = RunRequest {
                program: "bash",
                args: &["-c", &args.command],
                cwd: &ctx.cwd,
                deadline,
                grace_ms: args.grace_ms.unwrap_or(1000),
                max_stdout_bytes: half,
                max_stderr_bytes: half,
            };

            match run(req).await {
                Ok(out) => Ok(serde_json::json!({
                    "exit_code": out.exit_code,
                    "stdout": out.stdout,
                    "stdout_truncated": out.stdout_truncated,
                    "stderr": out.stderr,
                    "stderr_truncated": out.stderr_truncated,
                    "duration_ms": out.duration_ms,
                })),
                Err(RunError::NotFound { program }) => Err(ToolCallError::ExecutionFailed {
                    code: "NOT_AVAILABLE".into(),
                    message: format!("{program} not on PATH"),
                    retryable: false,
                }),
                Err(RunError::TimedOut { after_ms }) => Err(ToolCallError::ExecutionFailed {
                    code: "TIMEOUT".into(),
                    message: format!("command timed out after {after_ms}ms"),
                    retryable: true,
                }),
                Err(RunError::SpawnFailed(e)) | Err(RunError::Io(e)) => {
                    Err(ToolCallError::ExecutionFailed {
                        code: "IO".into(),
                        message: format!("io: {e}"),
                        retryable: true,
                    })
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn happy_path_echo() {
        let t = ShellExecTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(serde_json::json!({"command": "echo hi"}), &ctx)
            .await
            .unwrap();
        assert_eq!(r["exit_code"], 0);
        assert_eq!(r["stdout"], "hi\n");
        assert_eq!(r["stderr"], "");
    }

    #[tokio::test]
    async fn stderr_propagates() {
        let t = ShellExecTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"command": ">&2 echo boom; exit 2"}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["exit_code"], 2);
        assert_eq!(r["stderr"], "boom\n");
    }

    #[tokio::test]
    async fn nonzero_exit_code_is_not_a_tool_error() {
        let t = ShellExecTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(serde_json::json!({"command": "false"}), &ctx)
            .await
            .unwrap();
        assert_eq!(r["exit_code"], 1);
    }

    #[tokio::test]
    async fn timeout_returns_execution_failed() {
        let t = ShellExecTool::new();
        let mut ctx = CallContext::for_test();
        ctx.deadline = Some(Instant::now() + Duration::from_millis(200));
        let err = t
            .call(
                serde_json::json!({"command": "sleep 10", "grace_ms": 50}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed {
                code, retryable, ..
            } => {
                assert_eq!(code, "TIMEOUT");
                assert!(retryable);
            }
            _ => panic!("expected TIMEOUT"),
        }
    }

    #[tokio::test]
    async fn empty_command_is_invalid_args() {
        let t = ShellExecTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(serde_json::json!({"command": "   "}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }

    #[tokio::test]
    async fn grace_ms_override_respected() {
        // Can't directly observe SIGTERM vs SIGKILL, but we can verify the
        // call doesn't take longer than deadline + grace for a sleep that
        // ignores SIGTERM (sleep handles SIGTERM and exits cleanly, so this
        // tests the happy-exit path after SIGTERM). Acceptable proxy.
        let t = ShellExecTool::new();
        let mut ctx = CallContext::for_test();
        ctx.deadline = Some(Instant::now() + Duration::from_millis(150));
        let start = Instant::now();
        let _ = t
            .call(
                serde_json::json!({"command": "sleep 10", "grace_ms": 200}),
                &ctx,
            )
            .await;
        let elapsed = start.elapsed();
        // Deadline + grace + small overhead — well under the 10s sleep.
        assert!(elapsed < Duration::from_secs(2), "too slow: {elapsed:?}");
    }
}
