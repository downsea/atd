# atd-ref-server — SP-3 Shell Execution Design Spec

**Date:** 2026-04-22
**Status:** Design approved; plan pending.
**Scope:** Sub-project 3 of atd-ref-server. Adds `ref:shell.exec` (Bash) + `ref:shell.pwsh` (PowerShell) + a shared subprocess handler. Expands the tool catalog from "file ops only" to "file ops + arbitrary shell."
**Builds on:** SP-2 (`sp2-ref-server-file-io`) — 188 Rust workspace tests, 4 tools registered.

---

## 1. Motivation

Shell execution is the highest-frequency tool across real agent workflows. Every non-trivial task chain has a `run command X` step somewhere. Without shell.exec, atd-ref-server is a file-browser. With it, atd-ref-server is a general-purpose workspace.

Two separate tools (not one polymorphic shell tool) because:
- POSIX shell and PowerShell have fundamentally different argument parsing, pipe semantics, and error conventions
- Windows-first users shouldn't be forced to install Bash; POSIX-first users shouldn't be forced to know PowerShell quoting rules
- Explicit tool IDs (`shell.exec` vs `shell.pwsh`) make agent intent unambiguous

Like SP-1 and SP-2, this is clean-room: designed from universal shell-execution concepts (subprocess + timeout + output capture) using Rust's standard library + `tokio::process` + `libc` for signals. No proprietary source consulted.

---

## 2. Scope

### 2.1 In scope

- **`ref:shell.exec`** — POSIX shell (`bash -c "..."`)
- **`ref:shell.pwsh`** — PowerShell (`pwsh -NoProfile -Command "..."` preferred; `powershell -NoProfile -Command "..."` Windows-only fallback; `NOT_AVAILABLE` error if neither)
- **`tools/shell/shared.rs`** — shared subprocess handler: spawn + concurrent output capture + SIGTERM/grace/SIGKILL timeout + UTF-8-lossy decoding + size-budget truncation
- Registration in `builtin.rs` (2 new tools)
- **5 integration tests** covering happy path, exit-code propagation, timeout, stderr capture, PowerShell unavailable
- README update — mark SP-3 shipped; add brief "Shell tools" section

### 2.2 Explicitly deferred

- **Streaming output** — Phase 2 (requires ATD wire-protocol extension for progressive responses)
- **stdin piping** — Phase 2 (subprocess gets empty stdin in SP-3)
- **Per-call environment variable overrides** — agent writes `VAR=x command` inline for now
- **Per-call cwd override** — uses `ctx.cwd` only
- **Interactive TTY / PTY** — not an agent-protocol concern
- **Background / detached processes** — out of scope forever at this layer
- **Output interleaving** — separate stdout/stderr fields chosen in brainstorm Q2
- **Shell availability pre-flight at startup** — lazy check: first call fails if shell not installed

### 2.3 Prerequisites

- atd-ref-server at tag `sp2-ref-server-file-io`, 188 Rust workspace tests green
- `tokio` workspace dep already has `"process"` feature (added in SP-1 Task 1)
- `libc` crate — already transitively available via tokio; add as direct dep for signal access

---

## 3. Locked decisions (from the brainstorm)

1. **Timeout strategy:** SIGTERM → default 1 s grace → SIGKILL. Grace window configurable per-call via optional `grace_ms` (CallContext deadline drives the timeout itself). Windows uses `Child::kill()` (SIGKILL equivalent) since graceful termination semantics differ.
2. **Output model:** separate `stdout` and `stderr` string fields. Each has its own budget = `ctx.max_output_bytes / 2`. Truncation at line boundaries when possible, byte boundaries otherwise. Each returns a `stdout_truncated` / `stderr_truncated` flag.
3. **`exit_code` is a business outcome, not a tool error.** Only spawn failures / timeouts / IO errors → `ExecutionFailed`. A ran-to-completion process returns `Ok`, with `exit_code` in the result JSON regardless of zero-vs-nonzero.
4. **PowerShell fallback:** try `pwsh` first (cross-platform PS 7+). If spawn fails with `NotFound`, on Windows fall back to `powershell`; elsewhere return `NOT_AVAILABLE` error.
5. **stdin:** empty (subprocess reads EOF immediately).
6. **Environment:** inherited from server process, no per-call override.
7. **cwd:** `ctx.cwd`, no per-call override.
8. **Output decoding:** UTF-8 lossy (`String::from_utf8_lossy`). Non-UTF-8 bytes become replacement chars rather than failing the call.
9. **Shared handler API returns a structured `RunOutput`**, tools wrap it into the per-tool JSON response.
10. **Timeout-killed process:** `timed_out: true` + `exit_code: null` in the result; `success: false` at the ToolCallError level is not used (process did run, just didn't finish). Rationale: agent needs to know "command timed out and produced 200 bytes of partial output" vs "command failed to spawn entirely" — different recovery paths.

Wait — rereading decision 10: "timed_out=true means ExecutionFailed or Ok with flag?" This needs clarification.

**Clarification for decision 10:** Timeout IS an `ExecutionFailed` at the tool level (the agent asked for a command to complete; it didn't). Code `TIMEOUT`, retryable `true`. Partial output is lost — we don't try to preserve it. An agent that cares about partial output can set a longer timeout, or the user can raise `default_call_timeout_ms` on the server. This keeps the success/failure axis clean: if the command finished on its own terms (any exit code), it's `Ok`; if something blocked it from finishing, it's `Err`.

---

## 4. Architecture

### 4.1 Module layout additions

```
crates/atd-ref-server/src/
├── tools/
│   ├── mod.rs                         (MODIFY — export shell submodule)
│   ├── echo.rs                        (unchanged)
│   ├── fs/                            (unchanged, from SP-2)
│   └── shell/                         (NEW subtree)
│       ├── mod.rs                     (re-exports)
│       ├── shared.rs                  (subprocess handler, ~180 LOC)
│       ├── exec.rs                    (ref:shell.exec, ~120 LOC)
│       └── pwsh.rs                    (ref:shell.pwsh, ~120 LOC)
└── builtin.rs                         (MODIFY — register 2 new tools)
```

No changes to server.rs, context.rs, tracker.rs, registry.rs — shell tools fit the existing framework.

### 4.2 `Cargo.toml` change

Add `libc = "0.2"` to `[dependencies]` for `SIGTERM` constant + `kill()` signal sending on Unix. Keep `#[cfg(unix)]` gates around the signal code so Windows builds stay clean (Windows uses `Child::kill()` = SIGKILL equivalent).

### 4.3 Shared subprocess handler — `tools/shell/shared.rs`

Core primitive for both Bash and PowerShell:

```rust
pub struct RunRequest<'a> {
    /// Program to exec (e.g., "bash", "pwsh", "powershell")
    pub program: &'a str,
    /// Arguments passed to the program (e.g., ["-c", "echo hi"])
    pub args: &'a [&'a str],
    /// Working directory for the subprocess
    pub cwd: &'a Path,
    /// Hard deadline for process completion
    pub deadline: Option<Instant>,
    /// Grace period between SIGTERM and SIGKILL on timeout (default 1000ms)
    pub grace_ms: u64,
    /// Max bytes captured from stdout before truncation
    pub max_stdout_bytes: usize,
    /// Max bytes captured from stderr before truncation
    pub max_stderr_bytes: usize,
}

pub struct RunOutput {
    /// Process exit code. `None` only if process was killed by signal (SIGKILL).
    pub exit_code: Option<i32>,
    /// UTF-8-lossy decoded stdout, truncated to max_stdout_bytes if needed
    pub stdout: String,
    pub stdout_truncated: bool,
    /// UTF-8-lossy decoded stderr, truncated to max_stderr_bytes if needed
    pub stderr: String,
    pub stderr_truncated: bool,
    /// Total wall-clock time the process ran (includes SIGTERM grace)
    pub duration_ms: u64,
}

#[derive(Debug, thiserror::Error)]
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

pub async fn run(req: RunRequest<'_>) -> Result<RunOutput, RunError>;
```

### 4.4 Runtime flow of `shared::run`

1. Build `tokio::process::Command` with program + args + cwd + stdin=null + stdout=piped + stderr=piped.
2. `spawn()`. If it fails with `NotFound`, map to `RunError::NotFound`; other io errors → `SpawnFailed`.
3. Split child's stdout + stderr handles.
4. Spawn two background tasks: one drains stdout into `Vec<u8>` up to `max_stdout_bytes` (then keeps reading to EOF but discards), one does the same for stderr. Track truncation flags.
5. Await `child.wait()` with `tokio::select!` against `tokio::time::sleep_until(deadline)`.
6. **On deadline hit:**
   - Unix: `libc::kill(pid, SIGTERM)`; sleep `grace_ms`; if still running, `child.kill().await` (SIGKILL).
   - Windows: `child.kill().await` directly.
   - Join reader tasks (pipes close when process dies, bounded).
   - Return `RunError::TimedOut { after_ms: elapsed }`.
7. **On normal exit:** join reader tasks; UTF-8-lossy decode; return `RunOutput`.

### 4.5 `ref:shell.exec`

Input schema:
```json
{
  "type": "object",
  "properties": {
    "command": { "type": "string", "minLength": 1 },
    "grace_ms": { "type": "integer", "minimum": 0 }
  },
  "required": ["command"]
}
```

Output JSON:
```json
{
  "exit_code": 0,
  "stdout": "hello\n",
  "stdout_truncated": false,
  "stderr": "",
  "stderr_truncated": false,
  "duration_ms": 5
}
```

Behavior:
- Resolve shell: `bash` (POSIX). No fallback — if bash isn't installed, return `NOT_AVAILABLE`.
- Build args: `["-c", &command]`.
- Delegate to `shared::run`.
- Map `RunError::NotFound` → `ExecutionFailed{code:"NOT_AVAILABLE"}`, `TimedOut` → `ExecutionFailed{code:"TIMEOUT", retryable:true}`, `SpawnFailed`/`Io` → `ExecutionFailed{code:"IO", retryable:true}`.

### 4.6 `ref:shell.pwsh`

Input schema: same shape as `shell.exec` but tool-id-specific:
```json
{
  "type": "object",
  "properties": {
    "command": { "type": "string", "minLength": 1 },
    "grace_ms": { "type": "integer", "minimum": 0 }
  },
  "required": ["command"]
}
```

Output: same shape as `shell.exec`.

Behavior:
1. Try `pwsh -NoProfile -Command "..."` first (PS 7+, cross-platform).
2. If `RunError::NotFound`:
   - On Windows (`cfg!(windows)`): try `powershell -NoProfile -Command "..."`.
   - Other platforms: return `NOT_AVAILABLE`.
3. If the Windows fallback also fails with `NotFound`: `NOT_AVAILABLE`.
4. Other errors: same mapping as `shell.exec`.

The `-NoProfile` flag is important — it skips `$PROFILE` script execution, which would otherwise add latency + side effects per call.

### 4.7 Error-code table (SP-3 tool-level codes)

| Code | Situation | retryable |
|---|---|---|
| `NOT_AVAILABLE` | shell binary not installed | `false` |
| `TIMEOUT` | command didn't finish by deadline | `true` |
| `IO` | I/O error during spawn or wait | `true` |
| `INVALID_ARGS` (via `ToolCallError::InvalidArgs`) | schema validation failure | — |

`InvalidArgs` and `InternalError` map to wire `error` response (consistent with SP-1/SP-2). `ExecutionFailed` with the codes above maps to wire `tool_result { success: false, result: { code, message, retryable } }`.

---

## 5. Tests

### 5.1 Unit tests

| Module | Tests | What each covers |
|---|---|---|
| `tools/shell/shared.rs` | 8 | happy run (exit=0); nonzero exit_code returned (exit=1 `test -e /nope`); stderr capture; stdout truncation at budget; stderr truncation at budget; timeout triggers SIGTERM→SIGKILL; NotFound for bogus program; cwd honored |
| `tools/shell/exec.rs` | 6 | happy path; stderr propagation; exit_code passthrough; timeout propagation; InvalidArgs on empty command; grace_ms override respected |
| `tools/shell/pwsh.rs` | 5 | happy path (skip if pwsh unavailable); exit_code passthrough; NOT_AVAILABLE when no pwsh/powershell; Windows fallback honored (cfg-gated); grace_ms override |
| `builtin.rs` | +1 (update) | count now 6; all 6 IDs registered |

**New unit tests: ~19-20.**

### 5.2 Integration tests (`tests/integration.rs`)

5 new e2e scenarios:

| # | Scenario |
|---|---|
| I-1 | `shell.exec` with `echo hello` returns stdout="hello\n", exit_code=0 |
| I-2 | `shell.exec` with `test -e /does/not/exist; echo $?` returns stdout="1\n", exit_code=0 (the grep/test case — exit code of test flows to stdout) |
| I-3 | `shell.exec` with an intentional timeout (`sleep 5` with short deadline) returns TIMEOUT |
| I-4 | `shell.exec` stderr capture: command `>&2 echo boom; exit 3` returns stderr="boom\n", exit_code=3 |
| I-5 | `shell.pwsh` availability — on a system with pwsh, runs `Write-Output 'hi'`; without pwsh, returns NOT_AVAILABLE. Test uses `std::process::Command::new("pwsh").arg("-V").status()` at test start to decide expected branch. |

### 5.3 Total test target

| | After SP-2 | SP-3 additions | Total |
|---|---|---|---|
| atd-ref-server lib tests | 82 | +19 | ~101 |
| atd-ref-server integration | 14 | +5 | 19 |
| workspace Rust tests | 188 | +24 | ~212 |

---

## 6. Platform-specific concerns

### 6.1 Unix (Linux, macOS, BSD)

- `bash` typically at `/bin/bash` or `/usr/bin/bash`. Tokio uses `$PATH`; lazy check.
- SIGTERM via `libc::kill(pid, SIGTERM)`. PID is `tokio::process::Child::id()` (available pre-exit).
- Grace sleep via `tokio::time::sleep(Duration::from_millis(grace_ms))`.
- Subsequent `child.kill().await` (SIGKILL) only if still running.

### 6.2 Windows

- `pwsh.exe` if PS7 installed (`C:\Program Files\PowerShell\7\pwsh.exe` typical).
- `powershell.exe` present on all Windows versions since XP.
- No `SIGTERM`; `Child::kill()` is SIGKILL-equivalent. The `grace_ms` parameter is effectively ignored on Windows (documented in the tool description). Future enhancement could use `GenerateConsoleCtrlEvent` for Ctrl-C; out of scope for SP-3.

### 6.3 Test portability

- All 5 integration tests assume `bash` is present (a requirement on our Linux CI and on dev Macs).
- The pwsh test has a runtime skip: if `pwsh` isn't on PATH, the test asserts `NOT_AVAILABLE` (and on non-Windows `powershell` wasn't tried). If `pwsh` IS on PATH, it asserts happy path. Either branch passes on a correctly-configured system.

---

## 7. Exit criteria

1. `cargo build -p atd-ref-server --release` zero warnings (Unix; Windows untested since no CI on Windows yet).
2. `cargo test -p atd-ref-server` — ~120 tests (101 lib + 19 integration).
3. `cargo test --workspace --all-targets` — ~212 Rust tests.
4. Independence check passes: `cargo tree -p atd-ref-server | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )'` still empty.
5. Live manual smoke:
   - `atd --sock $SOCK call ref:shell.exec --args '{"command":"echo hi"}'` → `{"exit_code":0,"stdout":"hi\n",...}`
   - `atd --sock $SOCK call ref:shell.exec --args '{"command":"false"}'` → `{"exit_code":1,"stdout":"","stderr":"",...}` (no tool error)
6. Crate README updated: "What's shipped and what's next" shows SP-3 as shipped. New brief "Shell tools" note under the Quick start with a `ref:shell.exec` example.
7. Git tag `sp3-ref-server-shell` created.

---

## 8. Design decisions locked (reference)

1. Two separate tools: `ref:shell.exec` (Bash) + `ref:shell.pwsh` (PowerShell). Not one polymorphic tool.
2. SIGTERM → grace (default 1s, overridable per-call) → SIGKILL. Windows: SIGKILL only.
3. Separate `stdout` / `stderr` fields in result; each capped at `max_output_bytes / 2`.
4. `exit_code` is a business outcome; `ExecutionFailed` is reserved for process-did-not-run-to-completion.
5. Timeout → `ExecutionFailed{code:"TIMEOUT"}`, partial output discarded.
6. PowerShell: `pwsh` → (Windows only) `powershell` → `NOT_AVAILABLE`.
7. `-NoProfile` flag on both PS invocations to skip $PROFILE.
8. stdin = empty (no streaming/interactive support in Phase 0).
9. Env inherited; cwd = `ctx.cwd`. No per-call overrides.
10. UTF-8-lossy decoding; non-UTF-8 bytes become replacement characters.
11. `libc` crate added as direct dep (~0 compile cost, transitively present already).
12. Lazy shell detection (no startup pre-flight).

---

## 9. Open questions (none blocking)

All surfaced in the brainstorm are resolved.

Non-blocking forward-looking notes:

- **PATH resolution:** tokio uses the server's `$PATH`. If a user runs `atd-ref-server` under a different PATH than their interactive shell, "command not found" errors can be surprising. Document in the README that the server inherits its own env at startup. Mitigation for users: pass absolute paths or `env -i PATH=... command`.
- **Zombie processes on abrupt server shutdown:** currently per-connection tasks spawn children; if the server process dies without cleanup, orphaned children are reparented to init (Linux) and eventually reaped. Not a leak per se, but documented behavior.
- **Windows `grace_ms` is a no-op:** maybe should return it as a warning in the result. YAGNI for SP-3.
