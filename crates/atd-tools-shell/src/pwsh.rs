//! `ref:shell.pwsh` — PowerShell execution.
//!
//! Tries `pwsh` (PowerShell 7+, cross-platform) first. On NotFound, Windows
//! falls back to `powershell` (built-in since XP). Other platforms without
//! `pwsh` return NOT_AVAILABLE.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Tool};
use crate::shared::{run, RunError, RunRequest};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:shell.pwsh".into(),
        name: "PowerShell Execute".into(),
        description: "Run a command via PowerShell. Prefers `pwsh` (PS 7+ cross-platform); on Windows falls back to `powershell`. Returns exit code + separated stdout/stderr. -NoProfile is applied to skip $PROFILE scripts.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "shell".into(),
            actions: vec!["pwsh".into()],
            tags: vec!["shell".into(), "powershell".into(), "subprocess".into()],
            intent_examples: vec![
                "list directories via PowerShell".into(),
                "run a PS cmdlet".into(),
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

pub struct ShellPwshTool;

impl ShellPwshTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ShellPwshTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct PwshArgs {
    command: String,
    #[serde(default)]
    grace_ms: Option<u64>,
}

/// List of program names to try in order, per-platform.
fn pwsh_programs() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["pwsh", "powershell"]
    }
    #[cfg(not(windows))]
    {
        &["pwsh"]
    }
}

impl Tool for ShellPwshTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move {
            let args: PwshArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if args.command.trim().is_empty() {
                return Err(ToolCallError::InvalidArgs(
                    "command is empty or whitespace-only".into(),
                ));
            }

            let deadline = ctx.deadline.or_else(|| {
                Some(Instant::now() + Duration::from_secs(60))
            });
            let half = ctx.max_output_bytes / 2;
            let grace_ms = args.grace_ms.unwrap_or(1000);

            // Try each candidate program; on NotFound, try the next.
            for &program in pwsh_programs() {
                let req = RunRequest {
                    program,
                    args: &["-NoProfile", "-Command", &args.command],
                    cwd: &ctx.cwd,
                    deadline,
                    grace_ms,
                    max_stdout_bytes: half,
                    max_stderr_bytes: half,
                };
                match run(req).await {
                    Ok(out) => {
                        return Ok(serde_json::json!({
                            "exit_code": out.exit_code,
                            "stdout": out.stdout,
                            "stdout_truncated": out.stdout_truncated,
                            "stderr": out.stderr,
                            "stderr_truncated": out.stderr_truncated,
                            "duration_ms": out.duration_ms,
                        }));
                    }
                    Err(RunError::NotFound { .. }) => continue, // try next candidate
                    Err(RunError::TimedOut { after_ms }) => {
                        return Err(ToolCallError::ExecutionFailed {
                            code: "TIMEOUT".into(),
                            message: format!("command timed out after {after_ms}ms"),
                            retryable: true,
                        });
                    }
                    Err(RunError::SpawnFailed(e)) | Err(RunError::Io(e)) => {
                        return Err(ToolCallError::ExecutionFailed {
                            code: "IO".into(),
                            message: format!("io: {e}"),
                            retryable: true,
                        });
                    }
                }
            }

            // All candidates were NotFound.
            Err(ToolCallError::ExecutionFailed {
                code: "NOT_AVAILABLE".into(),
                message: "neither `pwsh` nor `powershell` is on PATH".into(),
                retryable: false,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Detect PowerShell availability at runtime; use it to decide the
    /// expected test branch.
    fn pwsh_available() -> bool {
        let candidates = pwsh_programs();
        for &program in candidates {
            if std::process::Command::new(program)
                .arg("-Version")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok()
            {
                return true;
            }
        }
        false
    }

    #[tokio::test]
    async fn happy_path_when_pwsh_available() {
        if !pwsh_available() {
            // Skip on systems without PowerShell.
            return;
        }
        let t = ShellPwshTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"command": "Write-Output 'hi'"}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["exit_code"], 0);
        assert!(r["stdout"].as_str().unwrap().contains("hi"));
    }

    #[tokio::test]
    async fn exit_code_passes_through() {
        if !pwsh_available() {
            return;
        }
        let t = ShellPwshTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"command": "exit 5"}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["exit_code"], 5);
    }

    #[tokio::test]
    async fn not_available_when_no_pwsh() {
        if pwsh_available() {
            // Skip on systems with PowerShell — this test only makes sense
            // when the shell is absent.
            return;
        }
        let t = ShellPwshTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"command": "Write-Output 'hi'"}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, retryable, .. } => {
                assert_eq!(code, "NOT_AVAILABLE");
                assert!(!retryable);
            }
            _ => panic!("expected NOT_AVAILABLE"),
        }
    }

    #[tokio::test]
    async fn empty_command_is_invalid_args() {
        let t = ShellPwshTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(serde_json::json!({"command": ""}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }

    #[tokio::test]
    async fn grace_ms_override_is_accepted() {
        // Schema accepts the optional grace_ms; behaviorally we can't easily
        // distinguish grace values with PS, but the call should at least not
        // reject the argument and should complete promptly on deadline.
        if !pwsh_available() {
            return;
        }
        let t = ShellPwshTool::new();
        let mut ctx = CallContext::for_test();
        ctx.deadline = Some(Instant::now() + Duration::from_millis(150));
        let start = Instant::now();
        let _ = t
            .call(
                serde_json::json!({
                    "command": "Start-Sleep -Seconds 10",
                    "grace_ms": 100
                }),
                &ctx,
            )
            .await;
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(3), "too slow: {elapsed:?}");
    }
}
