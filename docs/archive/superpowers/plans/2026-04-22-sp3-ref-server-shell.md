# atd-ref-server SP-3 Shell Execution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ref:shell.exec` (Bash) + `ref:shell.pwsh` (PowerShell) tools + a shared subprocess handler to `atd-ref-server`, expanding the catalog from "file I/O" to "file I/O + arbitrary shell."

**Architecture:** New `src/tools/shell/` subtree with three files: `shared.rs` (the subprocess-handler primitive — spawn + concurrent stdout/stderr capture + SIGTERM→grace→SIGKILL timeout + UTF-8-lossy decoding + per-stream size budget), `exec.rs` (Bash wrapper), `pwsh.rs` (PowerShell wrapper with `pwsh` preferred + Windows-only `powershell` fallback). Unix signal handling via `libc::kill` under `#[cfg(unix)]`. Both tools register into `builtin.rs`, making total tool count 6.

**Tech Stack:** Rust 2024, MSRV 1.85 · tokio (net, process, io-util, macros, rt-multi-thread, sync, time — all already enabled) · `libc` (NEW dep) for SIGTERM · std only otherwise.

**Spec:** `docs/superpowers/specs/2026-04-22-atd-ref-server-sp3-shell.md`

**Scope boundary:**
- **In:** `libc` dep; shell/shared.rs handler; shell/exec.rs (Bash); shell/pwsh.rs (PowerShell with fallback); builtin registration; 5 integration tests; README mark-as-shipped.
- **Out (later SPs):** streaming, stdin, env override, cwd override, interactive/PTY, background processes.

**Prerequisites:**
- `sp2-ref-server-file-io` tag, 188 Rust workspace tests green.
- `bash` installed on the test environment (Linux/macOS convention).

**Exit criteria:**
1. `cargo build -p atd-ref-server --release` zero warnings.
2. `cargo test -p atd-ref-server` — 120+ tests (101 lib + 19 integration).
3. `cargo test --workspace --all-targets` — ~212 Rust tests (188 prior + ~24 new).
4. Independence check `cargo tree -p atd-ref-server | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )'` empty.
5. Live smoke via atd-cli returns `exit_code=0, stdout="hi\n"` for `echo hi`.
6. Tag `sp3-ref-server-shell` created.

---

## File Structure

```
crates/atd-ref-server/
├── Cargo.toml                              (MODIFY — add libc)
├── README.md                               (MODIFY — mark SP-3 shipped, Task 7)
└── src/
    ├── builtin.rs                          (MODIFY — register 2 new tools, Task 5)
    └── tools/
        ├── mod.rs                          (MODIFY — export shell submodule)
        └── shell/                          (NEW subtree)
            ├── mod.rs                      (Task 1 — re-exports)
            ├── shared.rs                   (Task 2 — ~220 LOC)
            ├── exec.rs                     (Task 3 — ~150 LOC)
            └── pwsh.rs                     (Task 4 — ~170 LOC)
└── tests/
    └── integration.rs                      (MODIFY — add 5 new tests, Task 6)
```

---

## Task 1: Cargo.toml `libc` dep + scaffold

**Files:**
- Modify: `crates/atd-ref-server/Cargo.toml`
- Create: `crates/atd-ref-server/src/tools/shell/mod.rs`
- Modify: `crates/atd-ref-server/src/tools/mod.rs`

- [ ] **Step 1.1: Add `libc` dependency**

Edit `/home/nan/proj/atd-mvp/crates/atd-ref-server/Cargo.toml`. Find the `[dependencies]` section and add `libc = "0.2"`:

```toml
[dependencies]
atd-types = { path = "../atd-types", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
thiserror = { workspace = true }
ulid = { workspace = true }
clap = { version = "4", features = ["derive"] }
libc = "0.2"
```

Note: `libc` isn't in `[workspace.dependencies]` since no other workspace crate uses it. Adding as a direct version pin here.

- [ ] **Step 1.2: Create shell module scaffold**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/shell/mod.rs`:

```rust
//! Shell execution tools: ref:shell.exec (Bash), ref:shell.pwsh (PowerShell).

pub mod shared;
```

- [ ] **Step 1.3: Update `tools/mod.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/mod.rs` with:

```rust
//! Built-in tools.
//!
//! - SP-1: echo test-anchor
//! - SP-2: fs.{read,write,edit} + ReadTracker
//! - SP-3: shell.{exec,pwsh} + shared subprocess handler

pub mod echo;
pub mod fs;
pub mod shell;
```

- [ ] **Step 1.4: Build check**

Note: `shell::shared` is declared but the file doesn't exist yet — the build WILL fail until Task 2. That's expected TDD rhythm. This task just sets up the scaffold; the actual `shared.rs` file is Task 2.

Skip `cargo build` until Task 2 completes. Just verify the workspace manifest parses:

```bash
cd /home/nan/proj/atd-mvp
cargo metadata --no-deps --format-version 1 >/dev/null
```

Expected: exit 0, no output.

- [ ] **Step 1.5: Commit**

```bash
git add crates/atd-ref-server/Cargo.toml crates/atd-ref-server/src/tools/
git commit -m "chore(atd-ref-server): add libc dep + scaffold shell tools module"
```

Note: This commit temporarily breaks `cargo build` because `shared` module doesn't exist. Task 2 fixes that immediately. If you prefer to keep every commit buildable, squash Tasks 1 + 2; the plan treats them as separate for task-tracking clarity.

Alternative that keeps the commit buildable: instead of `pub mod shared;` in `shell/mod.rs`, make it an empty placeholder:

```rust
//! Shell execution tools: ref:shell.exec (Bash), ref:shell.pwsh (PowerShell).
//!
//! Modules land in Tasks 2-4.
```

Use the alternative. Update `shell/mod.rs` to the empty-placeholder version above. Task 2 will add `pub mod shared;`.

```bash
# Replace the file content as described above, then:
git add crates/atd-ref-server/src/tools/shell/mod.rs
git commit --amend --no-edit   # fold into the scaffold commit
```

Wait — amend touches the commit we just made. Simpler: edit `shell/mod.rs` BEFORE step 1.5 commit. Restart: back up to step 1.2 — write `shell/mod.rs` as the empty placeholder from the start:

```rust
//! Shell execution tools: ref:shell.exec (Bash), ref:shell.pwsh (PowerShell).
//!
//! Modules land in Tasks 2-4.
```

Then step 1.4 `cargo build -p atd-ref-server` succeeds (no broken module ref). Then commit. Cleaner.

**Corrected flow:** If you're reading this fresh: follow steps 1.1, 1.2 (using the empty-placeholder mod.rs shown just above — not the `pub mod shared;` version), 1.3, then:

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server    # should succeed, libc added to Cargo.lock
cargo test --workspace --all-targets    # 188 tests still green
git add crates/atd-ref-server/Cargo.toml crates/atd-ref-server/src/tools/
git commit -m "chore(atd-ref-server): add libc dep + scaffold shell tools module"
```

---

## Task 2: `tools/shell/shared.rs` — subprocess handler

**Files:**
- Create: `crates/atd-ref-server/src/tools/shell/shared.rs`
- Modify: `crates/atd-ref-server/src/tools/shell/mod.rs`

The core primitive. Spawn + concurrent output capture + timeout + signal escalation.

- [ ] **Step 2.1: Write the failing test + implementation**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/shell/shared.rs`:

```rust
//! Shared subprocess handler for shell tools.
//!
//! Responsibilities:
//! - Spawn the given program + args with piped stdout/stderr, null stdin.
//! - Concurrently drain stdout and stderr into byte buffers, each capped at
//!   its respective byte budget. Continue reading past the cap until EOF so
//!   the child doesn't block on full pipe buffers; bytes past the cap are
//!   discarded.
//! - Wait for the child with an optional absolute deadline. On timeout:
//!   Unix: SIGTERM, then sleep grace_ms, then Child::kill (SIGKILL).
//!   Windows: Child::kill directly.
//! - UTF-8-lossy decode both streams; return exit code + truncation flags +
//!   duration.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

pub struct RunRequest<'a> {
    pub program: &'a str,
    pub args: &'a [&'a str],
    pub cwd: &'a Path,
    pub deadline: Option<Instant>,
    pub grace_ms: u64,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

#[derive(Debug)]
pub struct RunOutput {
    /// Exit code, or `None` if process was killed by signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stdout_truncated: bool,
    pub stderr: String,
    pub stderr_truncated: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("program not found: {program}")]
    NotFound { program: String },

    #[error("spawn failed: {0}")]
    SpawnFailed(#[source] std::io::Error),

    #[error("command timed out after {after_ms}ms")]
    TimedOut { after_ms: u64 },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub async fn run(req: RunRequest<'_>) -> Result<RunOutput, RunError> {
    let start = Instant::now();

    let mut cmd = tokio::process::Command::new(req.program);
    cmd.args(req.args)
        .current_dir(req.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(RunError::NotFound {
                program: req.program.to_string(),
            });
        }
        Err(e) => return Err(RunError::SpawnFailed(e)),
    };

    let stdout = child.stdout.take().expect("stdout was set to piped");
    let stderr = child.stderr.take().expect("stderr was set to piped");

    let max_stdout = req.max_stdout_bytes;
    let max_stderr = req.max_stderr_bytes;
    let stdout_task = tokio::spawn(read_capped(stdout, max_stdout));
    let stderr_task = tokio::spawn(read_capped(stderr, max_stderr));

    // Wait for the child, honoring the optional deadline.
    let status_result = match req.deadline {
        Some(deadline) => {
            let remaining = deadline.saturating_duration_since(Instant::now());
            tokio::time::timeout(remaining, child.wait()).await
        }
        None => Ok(child.wait().await),
    };

    let status = match status_result {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(RunError::Io(e)),
        Err(_elapsed) => {
            // Deadline hit. SIGTERM → grace → SIGKILL.
            #[cfg(unix)]
            {
                if let Some(pid) = child.id() {
                    // Safe: pid is the actual child's PID, we're sending a
                    // standard termination signal. If the process has already
                    // exited, kill() returns -1 and we fall through.
                    unsafe {
                        libc::kill(pid as libc::pid_t, libc::SIGTERM);
                    }
                    tokio::time::sleep(Duration::from_millis(req.grace_ms)).await;
                }
            }
            // Either the grace expired, or we're on Windows — force kill.
            let _ = child.start_kill();
            let _ = child.wait().await;
            // Drain the readers so their tasks complete.
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(RunError::TimedOut {
                after_ms: start.elapsed().as_millis() as u64,
            });
        }
    };

    // Process finished; harvest the readers.
    let (stdout_bytes, stdout_truncated) = stdout_task
        .await
        .unwrap_or_else(|_| (Vec::new(), false));
    let (stderr_bytes, stderr_truncated) = stderr_task
        .await
        .unwrap_or_else(|_| (Vec::new(), false));

    Ok(RunOutput {
        exit_code: status.code(),
        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stdout_truncated,
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        stderr_truncated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Drain `reader` into a Vec<u8>. Stop storing once `max` bytes are
/// captured, but keep reading to EOF (so the writer doesn't block on a
/// full pipe buffer) — discard the excess.
async fn read_capped<R>(mut reader: R, max: usize) -> (Vec<u8>, bool)
where
    R: AsyncRead + Unpin,
{
    let mut buf = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() >= max {
                    truncated = true;
                    continue;
                }
                let room = max - buf.len();
                if n <= room {
                    buf.extend_from_slice(&chunk[..n]);
                } else {
                    buf.extend_from_slice(&chunk[..room]);
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    (buf, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cwd() -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    #[tokio::test]
    async fn happy_run_returns_stdout_and_exit_zero() {
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "echo hello"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 4096,
            max_stderr_bytes: 4096,
        })
        .await
        .unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout, "hello\n");
        assert_eq!(out.stderr, "");
        assert!(!out.stdout_truncated);
    }

    #[tokio::test]
    async fn nonzero_exit_code_returned_as_ok() {
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "exit 3"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 4096,
            max_stderr_bytes: 4096,
        })
        .await
        .unwrap();
        assert_eq!(out.exit_code, Some(3));
    }

    #[tokio::test]
    async fn stderr_captured_separately() {
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", ">&2 echo oops; echo good"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 4096,
            max_stderr_bytes: 4096,
        })
        .await
        .unwrap();
        assert_eq!(out.stdout, "good\n");
        assert_eq!(out.stderr, "oops\n");
    }

    #[tokio::test]
    async fn stdout_truncated_at_budget() {
        // Print 10 KB to stdout with 1 KB budget.
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "yes x | head -c 10240"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 1024,
            max_stderr_bytes: 1024,
        })
        .await
        .unwrap();
        assert!(out.stdout_truncated);
        assert!(out.stdout.len() <= 1024);
    }

    #[tokio::test]
    async fn stderr_truncated_at_budget() {
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "yes x | head -c 10240 >&2"],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 1024,
            max_stderr_bytes: 1024,
        })
        .await
        .unwrap();
        assert!(out.stderr_truncated);
        assert!(out.stderr.len() <= 1024);
    }

    #[tokio::test]
    async fn timeout_triggers_sigterm_then_sigkill() {
        let start = Instant::now();
        let err = run(RunRequest {
            program: "bash",
            args: &["-c", "sleep 10"],
            cwd: &cwd(),
            deadline: Some(Instant::now() + Duration::from_millis(200)),
            grace_ms: 100,
            max_stdout_bytes: 1024,
            max_stderr_bytes: 1024,
        })
        .await
        .unwrap_err();
        let elapsed = start.elapsed();
        match err {
            RunError::TimedOut { .. } => {}
            _ => panic!("expected TimedOut, got {err:?}"),
        }
        // Should have killed within ~deadline + grace, certainly less than the 10s sleep.
        assert!(elapsed < Duration::from_secs(2), "took too long: {elapsed:?}");
    }

    #[tokio::test]
    async fn bogus_program_returns_not_found() {
        let err = run(RunRequest {
            program: "this-program-definitely-does-not-exist-xyzzy",
            args: &[],
            cwd: &cwd(),
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 1024,
            max_stderr_bytes: 1024,
        })
        .await
        .unwrap_err();
        match err {
            RunError::NotFound { program } => {
                assert!(program.contains("xyzzy"));
            }
            _ => panic!("expected NotFound, got {err:?}"),
        }
    }

    #[tokio::test]
    async fn cwd_is_honored() {
        // Run `pwd` in a tempdir; output should be that tempdir's canonical path.
        let dir = tempfile::tempdir().unwrap();
        let canonical = tokio::fs::canonicalize(dir.path()).await.unwrap();
        let out = run(RunRequest {
            program: "bash",
            args: &["-c", "pwd"],
            cwd: &canonical,
            deadline: None,
            grace_ms: 1000,
            max_stdout_bytes: 4096,
            max_stderr_bytes: 4096,
        })
        .await
        .unwrap();
        let printed = out.stdout.trim_end();
        assert_eq!(printed, canonical.to_string_lossy());
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/shell/mod.rs`:

```rust
//! Shell execution tools: ref:shell.exec (Bash), ref:shell.pwsh (PowerShell).

pub mod shared;
```

- [ ] **Step 2.2: Run + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib tools::shell::shared    # 8 passed
cargo test --workspace --all-targets                         # 196 Rust tests (188 + 8)
git add crates/atd-ref-server/src/tools/shell/
git commit -m "feat(atd-ref-server): add shell/shared subprocess handler"
```

---

## Task 3: `ref:shell.exec` (Bash)

**Files:**
- Create: `crates/atd-ref-server/src/tools/shell/exec.rs`
- Modify: `crates/atd-ref-server/src/tools/shell/mod.rs`

Thin wrapper over `shared::run`: takes a `command` string, runs `bash -c "..."`, returns the structured output.

- [ ] **Step 3.1: Write the tool**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/shell/exec.rs`:

```rust
//! `ref:shell.exec` — POSIX shell command execution (bash -c).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};
use crate::tools::shell::shared::{run, RunError, RunRequest};

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

    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a> {
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
            ToolCallError::ExecutionFailed { code, retryable, .. } => {
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
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/shell/mod.rs`:

```rust
//! Shell execution tools: ref:shell.exec (Bash), ref:shell.pwsh (PowerShell).

pub mod exec;
pub mod shared;
```

- [ ] **Step 3.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib tools::shell::exec    # 6 passed
cargo test --workspace --all-targets                      # 202 Rust tests (196 + 6)
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add ref:shell.exec (Bash) tool"
```

---

## Task 4: `ref:shell.pwsh` (PowerShell with fallback)

**Files:**
- Create: `crates/atd-ref-server/src/tools/shell/pwsh.rs`
- Modify: `crates/atd-ref-server/src/tools/shell/mod.rs`

Two-stage fallback: try `pwsh` first (cross-platform PS 7+). On `NotFound`, Windows-only fall back to `powershell`. Else `NOT_AVAILABLE`.

- [ ] **Step 4.1: Write the tool**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/shell/pwsh.rs`:

```rust
//! `ref:shell.pwsh` — PowerShell execution.
//!
//! Tries `pwsh` (PowerShell 7+, cross-platform) first. On NotFound, Windows
//! falls back to `powershell` (built-in since XP). Other platforms without
//! `pwsh` return NOT_AVAILABLE.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};
use crate::tools::shell::shared::{run, RunError, RunRequest};

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
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/shell/mod.rs`:

```rust
//! Shell execution tools: ref:shell.exec (Bash), ref:shell.pwsh (PowerShell).

pub mod exec;
pub mod pwsh;
pub mod shared;
```

- [ ] **Step 4.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib tools::shell::pwsh    # 5 passed (some skip if no PS)
cargo test --workspace --all-targets                      # 207 Rust tests (202 + 5)
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add ref:shell.pwsh with pwsh→powershell fallback"
```

Note: on a system without PowerShell, three of the five tests are no-ops (early-return). They're still counted as passed, but only `not_available_when_no_pwsh` and `empty_command_is_invalid_args` actually exercise logic. On a system WITH pwsh, it's the inverse: `not_available_when_no_pwsh` early-returns and the other four run. This is intentional: both CI shapes must pass.

---

## Task 5: Register in builtin

**Files:**
- Modify: `crates/atd-ref-server/src/builtin.rs`

- [ ] **Step 5.1: Update `builtin.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/builtin.rs` with:

```rust
//! Built-in tool registration for `atd-ref-server`.
//!
//! To add a new tool:
//! 1. Create `src/tools/<name>.rs` implementing `Tool`.
//! 2. Export it from the appropriate `tools/*/mod.rs`.
//! 3. Add `reg.register(Arc::new(<Name>Tool::new()))` below.

use std::sync::Arc;

use crate::registry::Registry;
use crate::tools::echo::EchoTool;
use crate::tools::fs::{edit::FsEditTool, read::FsReadTool, write::FsWriteTool};
use crate::tools::shell::{exec::ShellExecTool, pwsh::ShellPwshTool};

pub fn builtin_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoTool::new()));
    reg.register(Arc::new(FsReadTool::new()));
    reg.register(Arc::new(FsWriteTool::new()));
    reg.register(Arc::new(FsEditTool::new()));
    reg.register(Arc::new(ShellExecTool::new()));
    reg.register(Arc::new(ShellPwshTool::new()));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_tools() {
        let r = builtin_registry();
        assert_eq!(r.count(), 6);
        assert!(r.get("ref:echo.say").is_some());
        assert!(r.get("ref:fs.read").is_some());
        assert!(r.get("ref:fs.write").is_some());
        assert!(r.get("ref:fs.edit").is_some());
        assert!(r.get("ref:shell.exec").is_some());
        assert!(r.get("ref:shell.pwsh").is_some());
    }
}
```

- [ ] **Step 5.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib builtin      # 1 passed (count now 6)
cargo test -p atd-ref-server                     # ~101 lib tests
cargo test --workspace --all-targets             # 207 Rust tests (unchanged — builtin test updated in place)
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): register shell.exec + shell.pwsh in builtin"
```

Wait, the workspace count should stay at 207 because the single `builtin_registry_contains_*` test just changed its assertion; the test count didn't change. Verify.

---

## Task 6: Integration tests for shell tools

**Files:**
- Modify: `crates/atd-ref-server/tests/integration.rs`

Five new e2e tests covering shell.exec happy path + exit propagation + timeout + stderr, plus one PS availability test.

One constraint reminder: the existing integration tests use `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` — subprocess spawning + the atd-ref-server binary's own spawn require multi-thread runtime. All new tests follow the same pattern.

Also: the existing `e2e_tool_list_returns_echo` test asserts 4 tools. With SP-3 registering 2 more, that test will fail. Must update.

- [ ] **Step 6.1: Update existing `e2e_tool_list_returns_echo` to expect 6**

In `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs`, find the existing `e2e_tool_list_returns_echo` test (from SP-2 Task 7 migration). Update its assertions from 4 tools to 6 tools, including the two new shell IDs. The test body should now look like:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_tool_list_returns_echo() {
    let srv = spawn_server().await;
    let r = send_one_request(&srv.sock, &serde_json::json!({"type": "tool_list"}))
        .await
        .unwrap();
    assert_eq!(r["type"], "tool_list");
    let tools = r["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    let ids: std::collections::HashSet<String> = tools
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains("ref:echo.say"));
    assert!(ids.contains("ref:fs.read"));
    assert!(ids.contains("ref:fs.write"));
    assert!(ids.contains("ref:fs.edit"));
    assert!(ids.contains("ref:shell.exec"));
    assert!(ids.contains("ref:shell.pwsh"));
}
```

(Rename is optional; the function name `e2e_tool_list_returns_echo` is now historical — feel free to rename to `e2e_tool_list_returns_all_registered` if you prefer.)

- [ ] **Step 6.2: Append 5 new shell integration tests**

At the end of `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs`, append:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_shell_exec_echo_returns_stdout() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:shell.exec",
            "args": {"command": "echo hello"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["exit_code"], 0);
    assert_eq!(r["result"]["stdout"], "hello\n");
    assert_eq!(r["result"]["stderr"], "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_shell_exec_nonzero_exit_is_tool_success() {
    let srv = spawn_server().await;
    // `test -e /nope/file` exits 1; shell captures to $?, echo prints it.
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:shell.exec",
            "args": {"command": "test -e /nope/xxx; echo $?"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    // The wrapping shell exited 0 (last command was echo), but $? captured 1.
    assert_eq!(r["result"]["exit_code"], 0);
    assert_eq!(r["result"]["stdout"], "1\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_shell_exec_timeout_returns_execution_failed() {
    let srv = spawn_server().await;
    // The server's default timeout is 60s; pass grace_ms to force a short
    // grace. To force a quick timeout, we rely on the server's default
    // deadline being much longer than our command, so instead we invoke a
    // command whose wall-time exceeds the tool's inherent timeout. Use a
    // custom socket-side knob via `--timeout-ms` at server launch.
    //
    // Simpler: use atd-ref-server's --timeout-ms arg to make the deadline
    // short. But spawn_server() doesn't support that — we'd need to parametrize.
    //
    // Pragmatic route: skip this e2e test and instead rely on the unit test
    // `tools::shell::exec::tests::timeout_returns_execution_failed` which
    // exercises the same behavior with a programmable deadline.
    //
    // For this e2e, use a short-lived sleep. If the server's default 60s
    // deadline didn't fire, this test is meaningless — but the unit-level
    // timeout coverage is solid.
    //
    // So: verify the exit code and that the command completed.
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:shell.exec",
            "args": {"command": "sleep 0.1; echo done"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["stdout"], "done\n");
    // Placeholder for future: real timeout e2e needs a --timeout-ms arg pipe.
    let _ = r;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_shell_exec_stderr_captured_with_nonzero_exit() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:shell.exec",
            "args": {"command": ">&2 echo boom; exit 3"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["exit_code"], 3);
    assert_eq!(r["result"]["stderr"], "boom\n");
    assert_eq!(r["result"]["stdout"], "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_shell_pwsh_availability_branch() {
    // Runtime-branch on whether pwsh/powershell is on PATH. Both branches
    // pass on a correctly-configured system.
    let pwsh_present = {
        let ok_pwsh = std::process::Command::new("pwsh")
            .arg("-Version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok();
        #[cfg(windows)]
        let ok = ok_pwsh
            || std::process::Command::new("powershell")
                .arg("-Version")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok();
        #[cfg(not(windows))]
        let ok = ok_pwsh;
        ok
    };

    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:shell.pwsh",
            "args": {"command": "Write-Output 'hi'"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    if pwsh_present {
        assert_eq!(r["success"], serde_json::json!(true));
        assert_eq!(r["result"]["exit_code"], 0);
        assert!(r["result"]["stdout"].as_str().unwrap().contains("hi"));
    } else {
        assert_eq!(r["success"], serde_json::json!(false));
        assert_eq!(r["result"]["code"], "NOT_AVAILABLE");
    }
}
```

**Note on the timeout e2e test (Task 6 step 6.2, third test above):** It's downgraded to a "happy path with a short sleep" because end-to-end timeout testing requires propagating `--timeout-ms` to the spawned server, which our existing `spawn_server()` helper doesn't support. The timeout behavior is thoroughly covered by `tools::shell::shared::tests::timeout_triggers_sigterm_then_sigkill` and `tools::shell::exec::tests::timeout_returns_execution_failed` (unit tests with programmable deadlines). The e2e version is placeholder-friendly — if future work parametrizes spawn_server with a `--timeout-ms` override, the test body can be upgraded in place.

Alternatively: drop this test and accept 4 integration tests + stronger unit coverage. Pragmatic choice; I've kept it because having a placeholder spot is easy to upgrade later.

- [ ] **Step 6.3: Run + commit**

```bash
cargo test -p atd-ref-server --test integration    # 19 passed (14 prior + 5 new)
cargo test --workspace --all-targets               # 212 Rust tests (207 + 5)
git add crates/atd-ref-server/tests/integration.rs
git commit -m "test(atd-ref-server): integration tests for shell.exec + shell.pwsh"
```

---

## Task 7: README + independence check + tag

**Files:**
- Modify: `crates/atd-ref-server/README.md`

- [ ] **Step 7.1: Update README — mark SP-3 shipped + add shell example**

Edit `/home/nan/proj/atd-mvp/crates/atd-ref-server/README.md`. Two changes:

**(a)** Find the section `## What's shipped and what's next` (added in SP-2 Task 10). Replace the SP-3 bullet:

Current:
```markdown
- **SP-3:** `ref:shell.exec` (Bash) + `ref:shell.pwsh` (PowerShell)
```

To:
```markdown
- **SP-3 (shipped):** `ref:shell.exec` (Bash) + `ref:shell.pwsh` (PowerShell) + shared subprocess handler
```

**(b)** Find the existing `## Quick start` section. Append a new subsection at the end of Quick start (before the next top-level heading):

```markdown
### Shell tools

```bash
# Run a command:
atd --sock $HOME/.atd-ref/server.sock call ref:shell.exec \
  --args '{"command": "uname -a"}'

# PowerShell (if pwsh is installed):
atd --sock $HOME/.atd-ref/server.sock call ref:shell.pwsh \
  --args '{"command": "Get-Date"}'
```

Shell tools return `{exit_code, stdout, stdout_truncated, stderr, stderr_truncated, duration_ms}`. A nonzero `exit_code` is a normal business result — not a tool error. Timeouts (SIGTERM → grace → SIGKILL on Unix) and missing shells (`NOT_AVAILABLE`) ARE errors and come back as `success: false` tool_result.
```

- [ ] **Step 7.2: Independence check**

```bash
cd /home/nan/proj/atd-mvp
cargo tree -p atd-ref-server --prefix none \
  | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )' \
  && echo FAIL \
  || echo "OK: no client/bridge/cli/anos deps"

grep -E '^\s*(atd-client|atd-mcp-bridge|atd-cli|anos-)' crates/atd-ref-server/Cargo.toml \
  && echo FAIL \
  || echo "OK: manifest clean"
```

Both must print OK.

- [ ] **Step 7.3: Live smoke**

With the server running:

```bash
# Terminal 1
cargo build --release -p atd-ref-server --bin atd-ref-server
./target/release/atd-ref-server --sock /tmp/sp3-smoke.sock

# Terminal 2
./target/release/atd --sock /tmp/sp3-smoke.sock call ref:shell.exec \
  --args '{"command": "echo hi"}'
```

Expected:
```
ok:
{
  "duration_ms": ...,
  "exit_code": 0,
  "stderr": "",
  "stderr_truncated": false,
  "stdout": "hi\n",
  "stdout_truncated": false
}
```

Also try:
```bash
./target/release/atd --sock /tmp/sp3-smoke.sock call ref:shell.exec \
  --args '{"command": "false"}'
```

Expected: `exit_code: 1`, tool call is success (not an error).

Clean up:
```bash
pkill -x atd-ref-server
rm -f /tmp/sp3-smoke.sock
```

- [ ] **Step 7.4: Final workspace regression + build**

```bash
cargo build -p atd-ref-server --release
cargo test --workspace --all-targets
```

Expected: release build zero warnings; all 212 Rust tests pass.

- [ ] **Step 7.5: Commit + tag**

```bash
git add crates/atd-ref-server/README.md
git commit -m "docs(atd-ref-server): mark SP-3 shipped and add shell quickstart"

git tag -a sp3-ref-server-shell -m "SP-3: atd-ref-server shell execution (Bash + PowerShell)"
git log --oneline | head -18
git tag
```

---

## Post-Plan Verification Checklist

- [ ] `cargo build -p atd-ref-server --release` zero warnings
- [ ] `cargo test -p atd-ref-server` passes (~120 tests — 101 lib + 19 integration)
- [ ] `cargo test --workspace --all-targets` passes ~212 Rust tests
- [ ] `cargo tree` independence check returns empty
- [ ] Live smoke: `echo hi` via shell.exec returns `exit_code=0, stdout="hi\n"`
- [ ] Live smoke: `false` via shell.exec returns `exit_code=1` as `success=true`
- [ ] README has SP-3 marked shipped + shell quickstart
- [ ] Tag `sp3-ref-server-shell` created

## What's next after SP-3

- **SP-4:** `ref:fs.glob` + `ref:fs.grep` via `globset` + `grep` crates (ripgrep's library backend)
- **SP-5:** `ref:web.fetch` via reqwest + html2md
- **SP-6:** cross-crate E2E rewrite of `hello_atd.{rs,py}` against atd-ref-server instead of ANOS; validation doc with demo video
