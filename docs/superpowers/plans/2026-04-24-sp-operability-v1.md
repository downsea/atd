# SP-operability-v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring atd-mvp to "production-operable" by landing three §10 ❌ items (audit logging, rate limiting, dry-run consistency) + per-call agent identity tracking in one bisect-clean SP.

**Architecture:** Three commits. C1 ships audit logging end-to-end: new `AuditSink` trait + `JsonLinesAuditSink` in `atd-runtime::audit`; `CallContext.caller_id` populated from Hello; `atd-ref-server-bin` gains `--audit-log <target>` CLI flag. C2 adds per-tool `Arc<Semaphore>` to `Registry` for `max_concurrent` enforcement + new `ERR_RATE_LIMITED = 1002` wire constant + `ToolCallError::RateLimited` variant + audit outcome integration. C3 audits each built-in tool's `ToolSafety.dry_run` declaration, fixes `shell.exec`/`shell.pwsh`, writes `docs/protocol/dry-run-contract.md`, flips four arch §10 rows to ✅.

**Tech Stack:** Rust 2024. New deps: `chrono` (RFC 3339 timestamps — already a workspace dep via `workspace.dependencies`). No crate additions beyond that.

**Spec:** `docs/superpowers/specs/2026-04-24-sp-operability-v1-design.md`

**Preconditions:** Working tree clean on master; 4-gate green; HEAD at or past `sp-8.1-capability-denied-gated-tool` (9d9e96a). Workspace test count 322. Conformance fixture count 32.

---

## Task 0: Pre-flight baseline

**Files:** No code changes; only a tag.

- [ ] **Step 1: Verify working tree clean**

Run: `git status --short | grep -vE "^\?\?"`
Expected: empty output (no tracked changes). Untracked pre-existing files (`CLAUDE.md`, `claude-code-source`, `docs/whitepaper/*`, `docs/superpowers/plans/2026-04-2{1,2}-*.md`) are out-of-scope.

- [ ] **Step 2: Verify 4-gate green**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count 322.

- [ ] **Step 3: Tag baseline**

```bash
git tag pre-sp-operability-v1
git log -1 --oneline
```

Expected: tag created on current HEAD. Rollback: `git reset --hard pre-sp-operability-v1`.

- [ ] **Step 4: Verify `chrono` is already a workspace dep**

```bash
grep -E "^chrono" Cargo.toml
```

Expected: `chrono = { version = "0.4", features = ["serde"] }` present at the workspace level. This is needed by C1 for RFC 3339 timestamps in audit events. If missing, STOP and escalate — the plan assumes this dep exists.

- [ ] **Step 5: No commit for this task** — tag only.

---

## Task 1 (C1): Audit Logging + Per-Call Identity

**Files:**
- Create: `crates/atd-runtime/src/audit.rs` — `AuditSink` trait, `CallEvent`, `Outcome`, `JsonLinesAuditSink` + unit tests
- Modify: `crates/atd-runtime/src/context.rs` — `CallContext` gains `caller_id: Option<String>`; `#[non_exhaustive]`
- Modify: `crates/atd-runtime/src/lib.rs` — `pub mod audit;` + re-exports
- Modify: `crates/atd-runtime/Cargo.toml` — add `chrono = { workspace = true }`
- Modify: `crates/atd-ref-server-bin/src/server.rs` — per-connection `caller_id` from Hello; dispatch loop emits audit events; `audit_sink` field in server state
- Modify: `crates/atd-ref-server-bin/src/main.rs` — clap `--audit-log <target>` + sink installation
- Create: `crates/atd-ref-server-bin/tests/audit_emits_events.rs` — integration test

### 1.1 Add chrono dep to atd-runtime

- [ ] **Step 1: Edit `crates/atd-runtime/Cargo.toml`**

In the `[dependencies]` section, add (keep alphabetical order among existing deps):

```toml
chrono = { workspace = true }
```

- [ ] **Step 2: Verify compile**

```bash
cargo check -p atd-runtime
```

Expected: clean. chrono resolves from workspace.

### 1.2 Create `atd-runtime/src/audit.rs`

- [ ] **Step 3: Write the full module**

Full contents of `crates/atd-runtime/src/audit.rs`:

```rust
//! Structured per-call audit events + pluggable sinks.
//!
//! `AuditSink` is the observation hook called at dispatch return points.
//! It sits OUTSIDE `Middleware` (which is a result-rewriter, success-only)
//! because audit needs to observe every outcome including failures.
//!
//! `JsonLinesAuditSink` is the default sink shipped in v1: one JSON
//! object per line, thread-safe, writes to any `Write + Send`.

use chrono::Utc;
use serde::Serialize;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

/// Audit schema version. Consumers should branch on this if future
/// breaking changes land. v1 is the initial stable schema.
pub const SCHEMA_VERSION: u32 = 1;

/// One per-call audit event. Emitted at every `Request::RunTool`
/// return point (success, invalid_args, execution_failed, cap_denied,
/// rate_limited, tool_not_found). Ping / Hello / ToolList / ToolSchema
/// do NOT emit events in v1.
#[derive(Debug, Clone, Serialize)]
pub struct CallEvent {
    pub ts: String,
    pub call_id: String,
    pub tool_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    pub granted_capabilities: Vec<String>,
    pub duration_ms: u64,
    pub outcome: Outcome,
    pub tier: String,
    pub dry_run: bool,
    pub schema_version: u32,
}

/// Outcome variants cover the full dispatch-return space for RunTool.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Success,
    ExecutionFailed { code: String, retryable: bool },
    InvalidArgs { message: String },
    CapabilityDenied { missing: Vec<String> },
    RateLimited { retry_after_ms: Option<u64> },
    ToolNotFound,
}

/// Observer hook. Non-blocking: writes happen synchronously to the
/// sink's own backpressure (no queuing here). Must not panic.
pub trait AuditSink: Send + Sync {
    fn on_call(&self, event: &CallEvent);
}

/// Writes one JSON object per line to the wrapped writer. Thread-safe
/// via a mutex around the writer. Write errors are silently dropped
/// (log loss >> dispatch stall).
pub struct JsonLinesAuditSink {
    writer: Mutex<Box<dyn Write + Send>>,
}

impl JsonLinesAuditSink {
    pub fn new(writer: Box<dyn Write + Send>) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }

    pub fn stdout() -> Self {
        Self::new(Box::new(std::io::stdout()))
    }

    pub fn stderr() -> Self {
        Self::new(Box::new(std::io::stderr()))
    }

    /// Open `path` for append; creates the file if missing.
    pub fn file(path: &Path) -> std::io::Result<Self> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self::new(Box::new(f)))
    }
}

impl AuditSink for JsonLinesAuditSink {
    fn on_call(&self, event: &CallEvent) {
        let Ok(mut line) = serde_json::to_vec(event) else { return; };
        line.push(b'\n');
        let Ok(mut w) = self.writer.lock() else { return; };
        let _ = w.write_all(&line);
        let _ = w.flush();
    }
}

/// Produce an RFC 3339 UTC timestamp string suitable for `CallEvent::ts`.
/// Dispatch sites use this rather than calling chrono directly so the
/// format stays consistent.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn mk_event(outcome: Outcome) -> CallEvent {
        CallEvent {
            ts: now_rfc3339(),
            call_id: "01J000000000000000000000TEST".into(),
            tool_id: "ref:echo.say".into(),
            caller_id: Some("test-client".into()),
            granted_capabilities: vec!["read".into(), "write".into()],
            duration_ms: 17,
            outcome,
            tier: "warm".into(),
            dry_run: false,
            schema_version: SCHEMA_VERSION,
        }
    }

    #[test]
    fn success_event_serializes() {
        let e = mk_event(Outcome::Success);
        let j: serde_json::Value = serde_json::from_slice(
            &serde_json::to_vec(&e).expect("serialize"),
        ).expect("parse");
        assert_eq!(j["tool_id"], "ref:echo.say");
        assert_eq!(j["outcome"]["kind"], "success");
        assert_eq!(j["schema_version"], 1);
        assert_eq!(j["dry_run"], false);
    }

    #[test]
    fn capability_denied_outcome_tagged_correctly() {
        let e = mk_event(Outcome::CapabilityDenied {
            missing: vec!["conformance.denied".into()],
        });
        let j: serde_json::Value = serde_json::from_slice(
            &serde_json::to_vec(&e).unwrap(),
        ).unwrap();
        assert_eq!(j["outcome"]["kind"], "capability_denied");
        assert_eq!(j["outcome"]["missing"][0], "conformance.denied");
    }

    #[test]
    fn execution_failed_carries_code_and_retryable() {
        let e = mk_event(Outcome::ExecutionFailed {
            code: "FS_NOT_FOUND".into(),
            retryable: false,
        });
        let j: serde_json::Value = serde_json::from_slice(
            &serde_json::to_vec(&e).unwrap(),
        ).unwrap();
        assert_eq!(j["outcome"]["kind"], "execution_failed");
        assert_eq!(j["outcome"]["code"], "FS_NOT_FOUND");
        assert_eq!(j["outcome"]["retryable"], false);
    }

    #[test]
    fn rate_limited_outcome_with_null_retry_after() {
        let e = mk_event(Outcome::RateLimited { retry_after_ms: None });
        let j: serde_json::Value = serde_json::from_slice(
            &serde_json::to_vec(&e).unwrap(),
        ).unwrap();
        assert_eq!(j["outcome"]["kind"], "rate_limited");
        assert!(j["outcome"]["retry_after_ms"].is_null());
    }

    #[test]
    fn caller_id_skipped_when_none() {
        let mut e = mk_event(Outcome::Success);
        e.caller_id = None;
        let s = serde_json::to_string(&e).unwrap();
        assert!(!s.contains("caller_id"),
            "caller_id None should be skipped, got: {}", s);
    }

    #[test]
    fn json_lines_sink_writes_one_line_per_event() {
        let buf: Vec<u8> = Vec::new();
        let buf_arc = Arc::new(Mutex::new(buf));
        let cloned = buf_arc.clone();

        struct SharedBuf(Arc<Mutex<Vec<u8>>>);
        impl Write for SharedBuf {
            fn write(&mut self, bs: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bs);
                Ok(bs.len())
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }

        let sink = JsonLinesAuditSink::new(Box::new(SharedBuf(buf_arc)));
        sink.on_call(&mk_event(Outcome::Success));
        sink.on_call(&mk_event(Outcome::ToolNotFound));

        let out = cloned.lock().unwrap().clone();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines.len(), 2, "expected 2 lines, got: {:?}", lines);
        for line in &lines {
            let _: CallEvent = serde_json::from_str(line)
                .expect("each line parses as CallEvent");
        }
    }

    #[test]
    fn now_rfc3339_format_is_parseable() {
        let s = now_rfc3339();
        chrono::DateTime::parse_from_rfc3339(&s).expect("RFC 3339 parseable");
    }
}
```

- [ ] **Step 4: Run the audit module tests**

```bash
cargo test -p atd-runtime --lib audit
```

Expected: error "could not find `audit`" — the module isn't declared yet. Continue to Step 5.

### 1.3 Declare `audit` module + add `non_exhaustive` + `caller_id`

- [ ] **Step 5: Declare `pub mod audit;` in lib.rs**

Read `crates/atd-runtime/src/lib.rs` first:

```bash
head -30 crates/atd-runtime/src/lib.rs
```

Add `pub mod audit;` in the module list (alphabetical among existing mod declarations). Also add a re-export at crate root for ergonomics:

```rust
pub use audit::{AuditSink, CallEvent, JsonLinesAuditSink, Outcome, SCHEMA_VERSION};
```

Placement: in the `pub use` block where other re-exports live (e.g., near `pub use binding::*;` and `pub use registry::*;`). Keep alphabetical ordering among `pub use` groups.

- [ ] **Step 6: Add `caller_id` field + `#[non_exhaustive]` to `CallContext`**

Read `crates/atd-runtime/src/context.rs`. Find `pub struct CallContext {` and apply:

```rust
// BEFORE (line numbers approximate)
#[derive(Debug, Clone)]
pub struct CallContext {
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub call_id: ulid::Ulid,
    pub deadline: Option<Instant>,
    pub read_tracker: Option<Arc<ReadTracker>>,
    pub capabilities: Arc<CapabilitySet>,
    pub tier: ToolTier,
}

// AFTER
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CallContext {
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub call_id: ulid::Ulid,
    pub deadline: Option<Instant>,
    pub read_tracker: Option<Arc<ReadTracker>>,
    pub capabilities: Arc<CapabilitySet>,
    pub tier: ToolTier,
    pub caller_id: Option<String>,
}
```

Then update `CallContext::for_test()` and `CallContext::for_test_with_tracker()` to initialize `caller_id: None`. Read those functions and add the field to their struct literals.

- [ ] **Step 7: Verify audit unit tests pass and CallContext compiles**

```bash
cargo test -p atd-runtime --lib audit
cargo check -p atd-runtime
```

Expected:
- `cargo test audit` → 7 tests pass
- `cargo check` → clean; but ALL workspace callers of `CallContext { ... }` struct literal will fail compile because of the new required field. That's the reason we add `#[non_exhaustive]` — but `#[non_exhaustive]` only protects from **downstream** crates. Within-workspace consumers still see the new required field.

Run `cargo check --workspace --all-features` — there will be compile errors in ref-server-bin / atd-tools-* tests that construct `CallContext`. Step 8 fixes those.

- [ ] **Step 8: Fix in-workspace `CallContext` struct-literal callers**

```bash
grep -rn "CallContext {" crates/ --include="*.rs"
```

Expected hits (approximate): `atd-runtime/src/middleware.rs` (test), `atd-runtime/src/binding.rs` (test), `atd-runtime/src/context.rs` (for_test funcs — already fixed in Step 6), `atd-ref-server-bin/src/server.rs` dispatch loop + tests.

For each site, add `caller_id: None` to the struct literal (keep alphabetical among fields if the callsite is alphabetical).

The `atd-ref-server-bin/src/server.rs` dispatch-loop construction (around line 247 in pre-SP HEAD) needs:

```rust
// Before: construction at dispatch time
let ctx = CallContext {
    cwd: state.config.cwd.clone(),
    max_output_bytes: tier_max_output,
    call_id: ulid::Ulid::new(),
    deadline: Some(Instant::now() + tier_timeout),
    read_tracker: Some(tracker.clone()),
    capabilities: caps.clone(),
    tier,
};

// After: populate caller_id from per-connection cache (wired in Step 11)
let ctx = CallContext {
    cwd: state.config.cwd.clone(),
    max_output_bytes: tier_max_output,
    call_id: ulid::Ulid::new(),
    deadline: Some(Instant::now() + tier_timeout),
    read_tracker: Some(tracker.clone()),
    capabilities: caps.clone(),
    tier,
    caller_id: current_caller_id.clone(),  // populated from Hello; Step 11
};
```

For this step, just add `caller_id: None` as a placeholder — the real value is wired in Step 11.

- [ ] **Step 9: Verify workspace compiles after caller_id fixups**

```bash
cargo check --workspace --all-features
cargo test -p atd-runtime --lib
```

Expected: all clean; all atd-runtime tests (including 7 new audit tests) pass.

### 1.4 Wire audit into server.rs

- [ ] **Step 10: Read the full dispatch loop**

```bash
grep -n "Request::" crates/atd-ref-server-bin/src/server.rs
```

This maps the dispatch handler's Request match arms. The `RunTool` arm starts around line 182 and has multiple return points (dry_run short-circuit, tool_not_found, cap_denied, binding result branches).

Read the full handler (approximately lines 130-310) to understand state flow. Plan-time note: server state is per-connection in `ServerState`, which lives inside `handle_connection`. Audit sink is process-global and goes in the outer `Server` struct, shared via `Arc<dyn AuditSink>`.

- [ ] **Step 11: Add `audit_sink` + `caller_id` to Server/ServerState**

In `crates/atd-ref-server-bin/src/server.rs`, find the `Server` struct and add:

```rust
pub struct Server {
    // existing fields...
    audit_sink: Option<Arc<dyn atd_runtime::AuditSink>>,
}
```

And `ServerConfig`:

```rust
pub struct ServerConfig {
    // existing fields...
    pub audit_sink: Option<Arc<dyn atd_runtime::AuditSink>>,
}
```

Wire through `Server::new(config)` constructor.

For per-connection state (the `ConnState` or similar inside `handle_connection`), add:

```rust
struct ConnState {
    // existing per-connection fields...
    caller_id: Option<String>,
}
```

Initialized to `None` on new connection; populated when `Request::Hello { client_id, ... }` arrives — find the Hello arm and add:

```rust
Request::Hello { client_id, requested_capabilities } => {
    conn_state.caller_id = client_id.clone();
    // existing Hello handling (capability grant etc.)
}
```

(The exact shape depends on current Hello handling. Preserve all existing logic; only add the `caller_id` assignment.)

- [ ] **Step 12: Add helper `emit_audit` in server.rs**

Before the dispatch match, introduce a helper that captures start time and the common event fields, and emits on the way out. Simplest form — in the RunTool arm specifically:

```rust
Request::RunTool { tool_id, args, dry_run } => {
    let start = std::time::Instant::now();
    let call_id_str = {
        // match CallContext::call_id; use a pre-generated one or the
        // one constructed later. For outcomes that return before ctx
        // is built (tool_not_found, cap_denied, rate_limited), we
        // generate a ULID here; for success/execution_failed/invalid_args,
        // it matches the CallContext::call_id.
        ulid::Ulid::new().to_string()
    };

    // Helper closure that emits the audit event with shared fields
    let audit_sink = state.audit_sink.clone();
    let caller_id = conn_state.caller_id.clone();
    let granted = caps.granted();
    let emit = |outcome: atd_runtime::Outcome, tier_str: &str| {
        if let Some(sink) = &audit_sink {
            sink.on_call(&atd_runtime::CallEvent {
                ts: atd_runtime::audit::now_rfc3339(),
                call_id: call_id_str.clone(),
                tool_id: tool_id.clone(),
                caller_id: caller_id.clone(),
                granted_capabilities: granted.clone(),
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                tier: tier_str.to_string(),
                dry_run,
                schema_version: atd_runtime::SCHEMA_VERSION,
            });
        }
    };
    // ... dispatch continues ...
}
```

Note: closures capturing by move over `Response` is finicky; in practice use a function-shape `fn emit(...)` taking explicit params, or inline the event construction at each return point. The closure approach is sketched here for brevity — if lifetime issues arise, inline 6 event-constructions (one per outcome branch) at the 6 return sites.

- [ ] **Step 13: Emit audit events at every RunTool return point**

In `server.rs`, at each `return Response::...` inside the RunTool arm, call `emit(outcome, tier_str)` BEFORE the return. Map each branch:

| Return branch | `outcome` value | `tier_str` |
|---|---|---|
| dry_run short-circuit → `ToolResultResponse { success: true, dry_run: true }` | `Outcome::Success` | `"warm"` (dry_run short-circuits before tier derivation; use a default, or derive from tool def if reachable) |
| tool not found → `Response::Error { message: "tool not found", ... }` | `Outcome::ToolNotFound` | `"warm"` (no tool def available) |
| capability denied → `Response::Error { code: Some(1001), details, ... }` | `Outcome::CapabilityDenied { missing: <sorted missing> }` | Tier from tool definition |
| (C2 will add: rate_limited → `Response::Error { code: Some(1002), ... }`) | `Outcome::RateLimited { retry_after_ms: None }` | Tier from tool definition |
| success → `Response::ToolResultResponse { success: true, dry_run: false }` | `Outcome::Success` | Tier from tool definition |
| ExecutionFailed → `Response::ToolResultResponse { success: false, result: { code, message, retryable }, ... }` | `Outcome::ExecutionFailed { code, retryable }` | Tier from tool def |
| InvalidArgs → `Response::Error { message: "invalid args for ...", ... }` | `Outcome::InvalidArgs { message: <cleaned> }` | Tier from tool def |
| InternalError → `Response::Error { message: "internal error for ...", ... }` | `Outcome::ExecutionFailed { code: "INTERNAL", retryable: false }` | Tier from tool def |

For "tier_str":
- Before tier derivation (tool_not_found, cap_denied on tools you can't look up): use `"warm"` as default
- After tier derivation: use `tier.as_str()` — need to add `pub fn as_str(&self) -> &'static str` to `ToolTier` if not present. Check `atd-runtime/src/tier.rs`:

```bash
grep "as_str" crates/atd-runtime/src/tier.rs
```

If absent, add:

```rust
// atd-runtime/src/tier.rs
impl ToolTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolTier::Hot => "hot",
            ToolTier::Warm => "warm",
            ToolTier::Cold => "cold",
        }
    }
}
```

- [ ] **Step 14: Verify server.rs compiles + existing tests pass**

```bash
cargo check -p atd-ref-server-bin
cargo test -p atd-ref-server-bin --lib
```

Expected: clean; all existing ref-server tests pass (no behavior change for no-audit-sink case; audit emit is a no-op when sink is `None`).

### 1.5 CLI flag + integration test

- [ ] **Step 15: Add `--audit-log` flag to main.rs**

In `crates/atd-ref-server-bin/src/main.rs`, add to the `Cli`/`Args` clap struct (placement: after other `--` flags, alphabetical position):

```rust
    /// Path or keyword for audit log sink. Values: "stdout", "stderr",
    /// or a file path. If omitted, audit logging is disabled (zero
    /// overhead — no events are constructed).
    #[arg(long)]
    audit_log: Option<String>,
```

Then construct the sink at startup, before `Server::new`. Place after the existing clap parse:

```rust
let audit_sink: Option<std::sync::Arc<dyn atd_runtime::AuditSink>> = match args.audit_log.as_deref() {
    None => None,
    Some("stdout") => Some(std::sync::Arc::new(
        atd_runtime::JsonLinesAuditSink::stdout(),
    )),
    Some("stderr") => Some(std::sync::Arc::new(
        atd_runtime::JsonLinesAuditSink::stderr(),
    )),
    Some(path) => match atd_runtime::JsonLinesAuditSink::file(std::path::Path::new(path)) {
        Ok(s) => Some(std::sync::Arc::new(s)),
        Err(e) => {
            eprintln!("atd-ref-server: cannot open audit log {path}: {e}");
            std::process::exit(2);
        }
    },
};
```

Then pass `audit_sink` into `ServerConfig`.

- [ ] **Step 16: 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count: 322 + ~7 (audit module unit tests) = ~329.

- [ ] **Step 17: Smoke-test the audit flag against ref-server**

```bash
rm -f /tmp/op-audit.sock /tmp/op-audit.jsonl
./target/release/atd-ref-server \
    --sock /tmp/op-audit.sock \
    --grant-capability read \
    --grant-capability write \
    --grant-capability exec \
    --audit-log /tmp/op-audit.jsonl &
sleep 1

./target/release/atd --sock /tmp/op-audit.sock call ref:echo.say --args '{"text":"hi"}'
./target/release/atd --sock /tmp/op-audit.sock call ref:fs.read --args '{}'  # invalid: no path
./target/release/atd --sock /tmp/op-audit.sock call ref:nonexistent.tool --args '{}'

pkill -f 'atd-ref-server --sock /tmp/op-audit' 2>/dev/null || true
rm -f /tmp/op-audit.sock

echo "=== audit log ==="
cat /tmp/op-audit.jsonl
rm -f /tmp/op-audit.jsonl
```

Expected: 3 JSON lines in `/tmp/op-audit.jsonl`:
- One `outcome.kind == "success"` for echo
- One `outcome.kind == "invalid_args"` OR `"execution_failed"` for fs.read (depends on how fs.read reports missing path; both are valid audit outcomes)
- One `outcome.kind == "tool_not_found"` for the nonexistent tool

If the lines look correct, audit wiring works.

### 1.6 Integration test

- [ ] **Step 18: Create `crates/atd-ref-server-bin/tests/audit_emits_events.rs`**

```rust
//! Integration: spawn ref-server with --audit-log <tmpfile>, drive
//! three RunTool requests covering 3 outcome kinds, assert log content.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_log_emits_expected_event_kinds() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let sock = tmp.path().join("audit.sock");
    let log_path = tmp.path().join("audit.jsonl");

    let bin = ref_server_bin();
    let mut child: Child = Command::new(&bin)
        .arg("--sock").arg(&sock)
        .arg("--grant-capability").arg("read")
        .arg("--grant-capability").arg("write")
        .arg("--grant-capability").arg("exec")
        .arg("--audit-log").arg(&log_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn atd-ref-server");

    wait_for_socket(&sock, Duration::from_secs(5)).await.expect("sock up");

    // Drive 3 calls via direct Unix socket writes using atd-sdk.
    let client = atd_sdk::AtdClient::connect(atd_sdk::Endpoint::unix(&sock))
        .await
        .expect("connect");

    // (a) success
    let _ = client
        .call(
            "ref:echo.say",
            serde_json::json!({ "text": "hi" }),
            atd_sdk::CallOptions::default(),
        )
        .await
        .expect("echo call");

    // (b) tool_not_found
    let _ = client
        .call(
            "ref:definitely.does.not.exist",
            serde_json::json!({}),
            atd_sdk::CallOptions::default(),
        )
        .await;

    // (c) invalid_args OR execution_failed — fs.read with no path
    let _ = client
        .call(
            "ref:fs.read",
            serde_json::json!({}),
            atd_sdk::CallOptions::default(),
        )
        .await;

    // Shutdown
    drop(client);
    let _ = child.kill();
    let _ = child.wait();

    // Read log
    let content = std::fs::read_to_string(&log_path).expect("read log");
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 3, "expected 3 audit lines, got {}: {}", lines.len(), content);

    // Parse each and check outcome kind
    let kinds: Vec<String> = lines
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["outcome"]["kind"].as_str().unwrap().to_string()
        })
        .collect();

    assert!(kinds.contains(&"success".to_string()), "missing success: {:?}", kinds);
    assert!(kinds.contains(&"tool_not_found".to_string()), "missing tool_not_found: {:?}", kinds);
    // Either invalid_args OR execution_failed accepted for fs.read {}
    assert!(
        kinds.contains(&"invalid_args".to_string())
            || kinds.contains(&"execution_failed".to_string()),
        "expected invalid_args or execution_failed for fs.read {{}}, got: {:?}", kinds
    );

    // Verify schema_version + tool_id fields present
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert!(v["tool_id"].as_str().unwrap().starts_with("ref:"));
        assert!(v["duration_ms"].as_u64().unwrap() < 5000, "duration unreasonable: {}", v["duration_ms"]);
    }
}

fn ref_server_bin() -> PathBuf {
    // Same pattern as atd-conformance tests — CARGO_BIN_EXE_<name>
    // works here because atd-ref-server IS in this crate.
    PathBuf::from(env!("CARGO_BIN_EXE_atd-ref-server"))
}

async fn wait_for_socket(path: &std::path::Path, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("socket {:?} did not appear within {:?}", path, timeout))
}
```

Note: this test uses `CARGO_BIN_EXE_atd-ref-server` — unlike atd-conformance's self-test, this file lives INSIDE atd-ref-server-bin, so the same-package env var works directly. Also depends on `atd-sdk` being in `[dev-dependencies]` of ref-server-bin — verify:

```bash
grep -A2 "^\[dev-dependencies\]" crates/atd-ref-server-bin/Cargo.toml
```

If `atd-sdk` isn't present, add it. Also add `tempfile` if missing.

- [ ] **Step 19: Run the integration test**

```bash
cargo test -p atd-ref-server-bin --test audit_emits_events
```

Expected: PASS. If invalid_args vs execution_failed case doesn't match, adjust the assertion to match whichever atd-tools-fs::read produces for missing path.

### 1.7 Final 4-gate + commit

- [ ] **Step 20: Full 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Workspace test count: ~329-330.

- [ ] **Step 21: Commit C1**

```bash
git add -A  # scope check first
git status --short
```

Sanity: files touched should be under `crates/atd-runtime/` + `crates/atd-ref-server-bin/`. If any unexpected files surface, use explicit paths instead of `-A`.

```bash
git commit -m "feat(atd-runtime,atd-ref-server-bin): audit logging + per-call identity (C1)

New atd-runtime::audit module: AuditSink trait + CallEvent schema
(v1) + JsonLinesAuditSink. Observer-only hook emitted at every
Request::RunTool dispatch return point. Thread-safe via Mutex around
the writer; write errors are silently dropped.

CallContext gains caller_id: Option<String> populated from Hello.
client_id; annotated #[non_exhaustive] for forward-compat.

atd-ref-server-bin main.rs adds --audit-log <stdout|stderr|path>
flag. Omitted (default) = no sink installed = zero overhead.

Coverage:
- 7 unit tests in audit.rs (per-outcome serialize, skip_if_none on
  caller_id, multi-line writer, RFC 3339 timestamp)
- 1 integration test tests/audit_emits_events.rs: spawns ref-server,
  drives 3 RunTool calls (success, tool_not_found, invalid_args or
  execution_failed), asserts 3 audit lines with correct outcome kinds.

Unblocks C2 (rate limit will extend Outcome::RateLimited with real
use) and future UCAN capability tokens (caller_id is prerequisite).

Refs: docs/superpowers/specs/2026-04-24-sp-operability-v1-design.md §4"
```

---

## Task 2 (C2): Rate Limiting (max_concurrent)

**Files:**
- Modify: `crates/atd-protocol/src/messages.rs` — add `pub const ERR_RATE_LIMITED: u16 = 1002;`
- Modify: `crates/atd-runtime/src/error.rs` — add `ToolCallError::RateLimited` variant
- Modify: `crates/atd-runtime/src/registry.rs` — `RegisteredTool` gains `semaphore: Arc<Semaphore>`; `#[non_exhaustive]`; construction reads `max_concurrent`
- Modify: `crates/atd-ref-server-bin/src/server.rs` — `try_acquire_owned` post-cap / pre-binding; emit rate_limited audit outcome
- Create: `crates/atd-ref-server-bin/tests/rate_limit.rs` — integration: in-process harness with blocking tool, verify 1002 on overflow

### 2.1 Wire constant + error variant

- [ ] **Step 1: Add `ERR_RATE_LIMITED` to atd-protocol**

Read `crates/atd-protocol/src/messages.rs`. Find the line `pub const ERR_CAPABILITY_DENIED: u16 = 1001;` and add directly below:

```rust
/// Wire value of `code` on `Response::Error` when dispatch refuses
/// a call because the tool's `max_concurrent` semaphore is saturated.
/// SP-operability-v1 C2.
pub const ERR_RATE_LIMITED: u16 = 1002;
```

- [ ] **Step 2: Add `RateLimited` variant to `ToolCallError`**

Read `crates/atd-runtime/src/error.rs`. Extend the enum (already `#[non_exhaustive]`):

```rust
// Place after the InternalError variant
    #[error("rate limited ({tool_id}): max_concurrent={limit} in-flight")]
    RateLimited {
        tool_id: String,
        limit: u32,
        retry_after_ms: Option<u64>,
    },
```

- [ ] **Step 3: Verify compile**

```bash
cargo check --workspace --all-features
```

Expected: clean. `ToolCallError` match sites in server.rs have wildcard arms (verified in SP-refactor-v1), so new variant doesn't break compile. `ERR_RATE_LIMITED` is a net-new constant; nothing references it yet.

### 2.2 Registry: per-tool Semaphore

- [ ] **Step 4: Add `semaphore` field to `RegisteredTool` + `#[non_exhaustive]`**

Read `crates/atd-runtime/src/registry.rs`. Modify:

```rust
// BEFORE
#[derive(Clone)]
pub struct RegisteredTool {
    pub tool: Arc<dyn Tool>,
    pub binding: Arc<dyn crate::binding::Binding>,
}

// AFTER
#[derive(Clone)]
#[non_exhaustive]
pub struct RegisteredTool {
    pub tool: Arc<dyn Tool>,
    pub binding: Arc<dyn crate::binding::Binding>,
    pub semaphore: Arc<tokio::sync::Semaphore>,
}
```

- [ ] **Step 5: Construct semaphore in `Registry::register_with_binding`**

Replace the existing construction logic in `Registry::register_with_binding`:

```rust
// BEFORE
pub fn register_with_binding(
    &mut self,
    tool: Arc<dyn Tool>,
    binding: Arc<dyn crate::binding::Binding>,
) {
    let id = tool.definition().id.clone();
    if self.tools.contains_key(&id) {
        panic!("duplicate tool registration: {id}");
    }
    self.tools.insert(id, RegisteredTool { tool, binding });
}

// AFTER
pub fn register_with_binding(
    &mut self,
    tool: Arc<dyn Tool>,
    binding: Arc<dyn crate::binding::Binding>,
) {
    let id = tool.definition().id.clone();
    if self.tools.contains_key(&id) {
        panic!("duplicate tool registration: {id}");
    }
    // Size the semaphore from the tool's declared max_concurrent.
    // 0 is treated as "unlimited" (defensive; built-in tools all
    // declare ≥ 1). Semaphore::MAX_PERMITS is the tokio-documented
    // ceiling — large enough for any real u32.
    let max = tool.definition().resources.max_concurrent;
    let permits = if max == 0 {
        tokio::sync::Semaphore::MAX_PERMITS
    } else {
        max as usize
    };
    let semaphore = Arc::new(tokio::sync::Semaphore::new(permits));
    self.tools.insert(id, RegisteredTool { tool, binding, semaphore });
}
```

- [ ] **Step 6: Verify all `RegisteredTool` struct-literal construction sites are updated**

```bash
grep -rn "RegisteredTool {" crates/ --include="*.rs"
```

Expected hits: only `registry.rs::register_with_binding` (just updated). Since `RegisteredTool` is now `#[non_exhaustive]`, external struct-literal construction is forbidden — external callers must go through `Registry::register*` methods. If any test code hand-constructs `RegisteredTool`, it'll fail to compile; fix by routing through `Registry`.

- [ ] **Step 7: Add a unit test for Semaphore sizing**

Append to `#[cfg(test)] mod tests` in `registry.rs`:

```rust
    #[test]
    fn semaphore_permits_match_max_concurrent() {
        use std::sync::Arc;
        use crate::binding::NativeBinding;
        use atd_protocol::{
            BindingProtocol, SafetyLevel, ToolBinding, ToolCapability,
            ToolDefinition, ToolResources, ToolSafety, ToolTrust, ToolVisibility,
            TrustLevel,
        };

        struct StubTool(ToolDefinition);
        impl super::Tool for StubTool {
            fn definition(&self) -> &ToolDefinition { &self.0 }
            fn call<'a>(&'a self, _a: serde_json::Value, _c: &'a crate::context::CallContext) -> super::CallFuture<'a> {
                Box::pin(async { Ok(serde_json::json!({})) })
            }
        }

        fn mk_tool(id: &str, max_concurrent: u32) -> Arc<dyn super::Tool> {
            Arc::new(StubTool(ToolDefinition {
                id: id.into(),
                name: id.into(),
                description: "t".into(),
                version: "0".into(),
                capability: ToolCapability {
                    domain: "d".into(),
                    actions: vec![],
                    tags: vec![],
                    intent_examples: vec![],
                },
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                bindings: vec![ToolBinding { protocol: BindingProtocol::Cli, config: serde_json::json!({}) }],
                safety: ToolSafety {
                    level: SafetyLevel::Read,
                    dry_run: false,
                    side_effects: vec![],
                    data_sensitivity: None,
                },
                resources: ToolResources {
                    timeout_ms: 100,
                    max_concurrent,
                    rate_limit_per_min: None,
                    estimated_tokens: None,
                },
                trust: ToolTrust {
                    publisher: "p".into(),
                    trust_level: TrustLevel::L0Unverified,
                    signature: None,
                },
                visibility: ToolVisibility::Read,
                required_capabilities: vec![],
                tier: None,
            }))
        }

        let mut reg = super::Registry::new();
        let tool_a = mk_tool("stub:a", 5);
        let tool_b = mk_tool("stub:b", 0);  // unlimited
        reg.register(tool_a);
        reg.register(tool_b);

        let a = reg.get("stub:a").unwrap();
        assert_eq!(a.semaphore.available_permits(), 5);

        let b = reg.get("stub:b").unwrap();
        assert_eq!(
            b.semaphore.available_permits(),
            tokio::sync::Semaphore::MAX_PERMITS,
            "max_concurrent=0 should map to MAX_PERMITS"
        );
    }
```

- [ ] **Step 8: Run registry tests**

```bash
cargo test -p atd-runtime --lib registry
```

Expected: PASS including the new test.

### 2.3 Dispatch: try_acquire + emit rate_limited

- [ ] **Step 9: Add rate-limit check in server.rs**

In `crates/atd-ref-server-bin/src/server.rs`, locate the RunTool arm's dispatch path — after the capability check's `return` branches (around line 235, post cap-denied) and before the tier derivation / CallContext construction (around line 240).

Insert:

```rust
// SP-operability-v1 C2: rate limit enforcement via per-tool
// Semaphore. Fail-fast (try_acquire_owned) — saturated tools
// return 1002 immediately with retryable: true, rather than
// blocking the dispatch thread.
let permit = match entry.semaphore.clone().try_acquire_owned() {
    Ok(p) => p,
    Err(_) => {
        let max_conc = entry.tool.definition().resources.max_concurrent;
        // audit emit (rate_limited) BEFORE return
        emit(atd_runtime::Outcome::RateLimited { retry_after_ms: None }, "warm");
        return Response::Error {
            message: format!(
                "rate limited for {tool_id}: max_concurrent={} in-flight",
                max_conc,
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
```

Note: `"warm"` is used for the tier string here because tier derivation happens below this point; for the rate-limited path we don't yet know the tier. If the architecture later moves tier derivation before the rate-limit check, update this to use `tier.as_str()`.

The variable `permit` is bound in the outer scope. It'll be kept alive through `binding.call(...).await`; when the future completes or is dropped (including on error), the permit is released automatically. Ensure no early `return` elides the permit — Rust's ownership model makes this hard to get wrong, but audit the dispatch path after this insertion to confirm `permit` stays in scope.

- [ ] **Step 10: Verify no behavior change for default tools**

All built-in tools declare `max_concurrent` ≥ 8. Existing integration tests and smoke tests should still pass.

```bash
cargo test --workspace --all-targets
```

Expected: all existing tests still pass.

### 2.4 Rate-limit integration test

- [ ] **Step 11: Create `crates/atd-ref-server-bin/tests/rate_limit.rs`**

This test uses an in-process server harness (no subprocess) because we need a blocking tool whose behavior the test controls — similar to the `dispatch_capability_denied_path.rs` pattern.

```rust
//! Integration: a tool with max_concurrent=1 whose call() blocks on
//! a Notify. Fire two concurrent requests; assert second gets 1002.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition,
    ToolResources, ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_ref_server_bin::server::{Server, ServerConfig};
use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::Notify;

struct BlockingTool {
    def: ToolDefinition,
    gate: Arc<Notify>,
}

impl BlockingTool {
    fn new(gate: Arc<Notify>) -> Self {
        Self {
            def: ToolDefinition {
                id: "test:blocker".into(),
                name: "blocker".into(),
                description: "blocks until notified".into(),
                version: "0".into(),
                capability: ToolCapability {
                    domain: "test".into(),
                    actions: vec!["block".into()],
                    tags: vec![],
                    intent_examples: vec![],
                },
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "object"}),
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
                    timeout_ms: 5000,
                    max_concurrent: 1,     // key for this test
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
            gate,
        }
    }
}

impl Tool for BlockingTool {
    fn definition(&self) -> &ToolDefinition { &self.def }
    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        let gate = self.gate.clone();
        Box::pin(async move {
            gate.notified().await;
            Ok(serde_json::json!({ "done": true }))
        })
    }
}

async fn write_frame(w: &mut (impl AsyncWriteExt + Unpin), msg: &serde_json::Value) {
    let body = serde_json::to_vec(msg).unwrap();
    let len = (body.len() as u32).to_be_bytes();
    w.write_all(&len).await.unwrap();
    w.write_all(&body).await.unwrap();
    w.flush().await.unwrap();
}

async fn read_frame(r: &mut (impl AsyncReadExt + Unpin)) -> serde_json::Value {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await.unwrap();
    let n = u32::from_be_bytes(len) as usize;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await.unwrap();
    serde_json::from_slice(&buf).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn max_concurrent_saturation_yields_1002() {
    let tmp = tempfile::tempdir().unwrap();
    let sock: PathBuf = tmp.path().join("rl.sock");

    let gate = Arc::new(Notify::new());
    let mut registry = Registry::new();
    registry.register(Arc::new(BlockingTool::new(gate.clone())));

    let server = Server::new(ServerConfig {
        sock: sock.clone(),
        cwd: std::env::temp_dir(),
        max_output_bytes: 1 << 16,
        timeout_ms: 5000,
        grant_capabilities: vec![],
        tier_overrides: vec![],
        middleware: vec![],
        audit_sink: None,
    });
    let registry_arc = Arc::new(registry);
    let server_handle = tokio::spawn({
        let server = server.clone();
        let reg = registry_arc.clone();
        async move { server.run(reg).await.expect("server.run") }
    });

    // Wait for socket
    for _ in 0..50 {
        if sock.exists() { break; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(sock.exists(), "socket did not appear");

    // Spawn two concurrent clients
    let sock_a = sock.clone();
    let sock_b = sock.clone();
    let a = tokio::spawn(async move {
        let mut s = UnixStream::connect(&sock_a).await.unwrap();
        write_frame(&mut s, &serde_json::json!({
            "type": "run_tool", "tool_id": "test:blocker",
            "args": {}, "dry_run": false,
        })).await;
        read_frame(&mut s).await
    });

    // Give A time to acquire the permit
    tokio::time::sleep(Duration::from_millis(100)).await;

    let b = tokio::spawn(async move {
        let mut s = UnixStream::connect(&sock_b).await.unwrap();
        write_frame(&mut s, &serde_json::json!({
            "type": "run_tool", "tool_id": "test:blocker",
            "args": {}, "dry_run": false,
        })).await;
        read_frame(&mut s).await
    });

    // B should fail fast with 1002
    let b_resp = tokio::time::timeout(Duration::from_secs(2), b)
        .await
        .expect("B should return quickly")
        .unwrap();
    assert_eq!(b_resp["type"], "error");
    assert_eq!(b_resp["code"], atd_protocol::ERR_RATE_LIMITED);
    assert_eq!(b_resp["retryable"], true);

    // Now release A
    gate.notify_one();
    let a_resp = tokio::time::timeout(Duration::from_secs(2), a)
        .await
        .expect("A should complete after notify")
        .unwrap();
    assert_eq!(a_resp["type"], "tool_result");
    assert_eq!(a_resp["success"], true);

    // Shut down
    server_handle.abort();
}
```

NOTE: the exact `Server::new` / `ServerConfig` fields + `Server::run` signature may not match this sketch. Read `crates/atd-ref-server-bin/src/server.rs` for the actual API. If the pattern differs from `dispatch_capability_denied_path.rs`, follow that test's pattern closely (same in-process harness, same Server::run usage).

- [ ] **Step 12: Run the rate-limit test**

```bash
cargo test -p atd-ref-server-bin --test rate_limit
```

Expected: PASS. If Server::new API differs from sketch, adjust the test call site.

- [ ] **Step 13: 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count: ~330-331.

- [ ] **Step 14: Commit C2**

```bash
git status --short
# Verify scope: atd-protocol, atd-runtime (error.rs + registry.rs), atd-ref-server-bin
git add crates/atd-protocol crates/atd-runtime crates/atd-ref-server-bin
git commit -m "feat(atd-runtime,atd-ref-server-bin): rate limiting via max_concurrent (C2)

- atd-protocol/messages.rs: ERR_RATE_LIMITED = 1002 (SP-operability-v1)
- atd-runtime/error.rs: new ToolCallError::RateLimited variant with
  tool_id + limit + retry_after_ms (already #[non_exhaustive])
- atd-runtime/registry.rs: RegisteredTool gains Arc<Semaphore> sized
  from tool.definition().resources.max_concurrent (0 → unlimited);
  #[non_exhaustive] added
- atd-ref-server-bin/server.rs: try_acquire_owned() post-cap /
  pre-tier; saturation returns code=1002 retryable=true; audit event
  emits Outcome::RateLimited

Coverage:
- Unit test registry::tests::semaphore_permits_match_max_concurrent
- Integration test tests/rate_limit.rs: in-process harness with a
  blocking tool at max_concurrent=1, asserts 2nd concurrent request
  gets 1002, 3rd succeeds after first releases permit

No behavior change for built-in tools (all declare max_concurrent ≥ 8,
workloads can't realistically saturate).

Refs: docs/superpowers/specs/2026-04-24-sp-operability-v1-design.md §5"
```

---

## Task 3 (C3): Dry-run Consistency Docs + Shell Tool Fixup

**Files:**
- Modify: `crates/atd-tools-shell/src/exec.rs` — `ToolSafety.dry_run: false → true`
- Modify: `crates/atd-tools-shell/src/pwsh.rs` — `ToolSafety.dry_run: false → true`
- Create: `docs/protocol/dry-run-contract.md`
- Modify: `docs/architecture.md` §10 — flip 4 rows ❌ → ✅
- Modify: `crates/atd-ref-server-bin/src/builtin.rs` — add unit test asserting shell tools declare dry_run: true

### 3.1 Fix shell tool declarations

- [ ] **Step 1: Update `crates/atd-tools-shell/src/exec.rs`**

Find `dry_run: false,` (line ~58) and change to `dry_run: true,`. Use Edit with surrounding context:

```rust
            safety: ToolSafety {
                level: SafetyLevel::Execute,
                dry_run: true,   // CHANGED from false; shell.exec has side effects
                ...
            },
```

Preserve the exact surrounding fields. The `// CHANGED from false;` comment is optional — prefer keeping the file clean without the drive-by comment; the commit message explains the rationale.

Actual edit:
- Find: `dry_run: false,` in exec.rs (the one in ToolSafety, not anywhere else)
- Replace: `dry_run: true,`

- [ ] **Step 2: Update `crates/atd-tools-shell/src/pwsh.rs`**

Same change: find the `dry_run: false,` inside the `ToolSafety` block, change to `dry_run: true,`.

### 3.2 Unit-test regression guard

- [ ] **Step 3: Append regression test to `crates/atd-ref-server-bin/src/builtin.rs`**

In the existing `#[cfg(test)] mod tests` block at the bottom of `builtin.rs`, append:

```rust
    #[test]
    fn shell_tools_declare_dry_run_true() {
        let reg = builtin_registry(false);
        let exec = reg
            .get("ref:shell.exec")
            .expect("shell.exec registered by default");
        assert!(
            exec.tool.definition().safety.dry_run,
            "shell.exec has side effects → should declare dry_run: true"
        );
        let pwsh = reg
            .get("ref:shell.pwsh")
            .expect("shell.pwsh registered by default");
        assert!(
            pwsh.tool.definition().safety.dry_run,
            "shell.pwsh has side effects → should declare dry_run: true"
        );
    }
```

- [ ] **Step 4: Verify test passes**

```bash
cargo test -p atd-ref-server-bin --lib builtin
```

Expected: PASS including the new test (+1 test; total lib test count for ref-server-bin goes up by 1).

### 3.3 Dry-run contract document

- [ ] **Step 5: Create `docs/protocol/dry-run-contract.md`**

```markdown
# Dry-run semantics (v1)

`Request::RunTool { dry_run: true }` is a **server-side short-circuit**
in v1. When a client sends `dry_run: true`, the server returns a
synthetic `tool_result` without invoking the tool:

```json
{
  "type": "tool_result",
  "tool_id": "<requested>",
  "success": true,
  "dry_run": true,
  "result": {
    "dry_run": true,
    "tool_id": "<requested>",
    "args_preview": <args echoed back>
  }
}
```

This is **uniform across all tools**. The tool is never invoked; no
binding (`NativeBinding` or `CliBinding`) runs.

## Interpretation of `ToolSafety.dry_run`

The `ToolSafety.dry_run: bool` field on each tool's `ToolDefinition`
is **informational**: it signals whether the tool *could in principle*
support a meaningful preview. It is metadata for clients, schema
generators, and future dispatch versions — the v1 server does not
read it.

### When to declare `dry_run: true`

Declare `true` if invoking the tool has side effects:
- Filesystem writes (fs.write, fs.edit)
- Subprocess execution (shell.exec, shell.pwsh)
- HTTP POST/PUT/DELETE (none in v1 — web.fetch is GET-only)

Declare `false` for read-only tools (echo, fs.read, fs.glob, fs.grep,
web.fetch, external.uname).

## Agent-side contract

Agents that rely on preview fidelity MUST NOT assume the `result`
field of a v1 dry-run response reflects tool-specific semantics. A
future SP (SP-operability-v2 candidate) may route `dry_run: true`
to tools declaring `ToolSafety.dry_run: true` and allow them to
return meaningful previews. At that point, version-gated clients
will need to branch on `schema_version` in the audit event (see
`docs/protocol/audit-events.md` when it lands) or on a new
`Response` field.

## Audit event correlation

A `dry_run: true` call emits a `CallEvent` with:
- Top-level `dry_run: true` field
- `outcome: { "kind": "success" }`

Operators wanting to distinguish real calls from dry-run drills in log
queries should match on the top-level `dry_run` flag rather than on
outcome. Example `jq`:

```bash
jq 'select(.dry_run == false) | .tool_id' audit.jsonl
```

## Forward-compatibility notes

- `ToolSafety.dry_run` becoming actionable in a future SP is a
  **non-breaking** wire change — clients that ignore it today keep
  working.
- The synthetic `result.args_preview` field in v1 short-circuit
  responses is **not** part of the stable contract; future v2
  dispatch that delegates to tools will replace it with
  tool-specific preview content.
```

- [ ] **Step 6: Verify the doc file exists and has no placeholder warts**

```bash
grep -nE "TBD|TODO|XXX" docs/protocol/dry-run-contract.md
wc -l docs/protocol/dry-run-contract.md
```

Expected: no hits, ~60-80 lines.

### 3.4 Architecture §10 status updates

- [ ] **Step 7: Update `docs/architecture.md` §10 — flip 4 rows**

Find the §10 evolution-path table. Apply 4 row rewrites:

Before:
```markdown
| Audit logging (structured per-call events) | Security | ❌ | post-SP-13 small SP | Q2 2026 | No adopter gate |
| Rate limiting + `max_concurrent` enforcement | Security | ❌ | post-SP-13 small SP | Q2 2026 | No adopter gate |
| Dry-run consistency across tools | Security | ❌ | post-SP-13 small SP | Q2 2026 | No adopter gate |
| Per-call agent identity tracking | Security | ❌ | bundled with audit | Q2 2026 | Prerequisite for audit and UCAN tokens |
```

After:
```markdown
| Audit logging (structured per-call events) | Security | ✅ | SP-operability-v1 | 2026-04-24 | Landed; JsonLinesAuditSink via --audit-log flag; CallEvent schema v1. |
| Rate limiting + `max_concurrent` enforcement | Security | ✅ | SP-operability-v1 | 2026-04-24 | Landed; per-tool tokio Semaphore in Registry; ERR_RATE_LIMITED (1002) wire code. |
| Dry-run consistency across tools | Security | ✅ | SP-operability-v1 | 2026-04-24 | Landed; server-side short-circuit documented in docs/protocol/dry-run-contract.md; shell.exec/pwsh ToolSafety.dry_run corrected to true. |
| Per-call agent identity tracking | Security | ✅ | SP-operability-v1 | 2026-04-24 | Landed; CallContext.caller_id populated from Hello.client_id; prerequisite for UCAN tokens (arch §9.3). |
```

Also scan §5 (security chapter) for text that says "audit not implemented" or similar — if present, update references to point at the shipped module. Specifically §5.2 may have a sentence "Shipping audit is the most valuable next security-adjacent SP"; rephrase to past tense or replace with "Audit landed in SP-operability-v1; see `crates/atd-runtime/src/audit.rs`."

```bash
grep -n "audit" docs/architecture.md | head -15
```

Review each match; update any that claim audit is missing or pending. Preserve everything else.

- [ ] **Step 8: Scan for other docs that may need updating**

```bash
grep -rn "audit\|rate.limit\|dry.run" docs/ --include="*.md" | grep -v "superpowers/" | grep -v "whitepaper/"
```

Review hits. Likely candidates for minor updates:
- `docs/protocol/error-codes.md` — add `1002 ERR_RATE_LIMITED` to the error-code catalogue
- `CLAUDE.md` — if it mentions arch §10 items as ❌, flip the relevant lines

For `docs/protocol/error-codes.md`, find the table of wire error codes and add:

```markdown
| `1002` | `ERR_RATE_LIMITED` | Dispatch | A tool's `max_concurrent` permits are exhausted; the call was refused without invocation. `retryable: true` — the client may retry after a backoff. | SP-operability-v1 |
```

Match the format of the existing `1001 ERR_CAPABILITY_DENIED` row exactly.

### 3.5 Final 4-gate + commit

- [ ] **Step 9: Full 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count: ~331-332 (+1 from Step 3).

- [ ] **Step 10: Commit C3**

```bash
git status --short
# Verify scope: atd-tools-shell, atd-ref-server-bin/src/builtin.rs, docs/
git add crates/atd-tools-shell crates/atd-ref-server-bin/src/builtin.rs docs/
git commit -m "feat(docs,atd-tools-shell): dry-run consistency + arch §10 status flip (C3)

- atd-tools-shell/exec.rs + pwsh.rs: ToolSafety.dry_run false → true.
  Both tools have side effects (arbitrary command execution) — the
  field is informational 'could preview in principle'.
- atd-ref-server-bin/builtin.rs: regression test asserts both shell
  tools declare dry_run: true.
- docs/protocol/dry-run-contract.md: new contract document explaining
  v1 server-side short-circuit, when to declare true, agent-side
  contract, audit event correlation.
- docs/protocol/error-codes.md: document ERR_RATE_LIMITED (1002).
- docs/architecture.md §10: 4 rows flip ❌ → ✅ (audit / rate limit /
  dry-run / per-call identity).

Zero wire changes. Zero behavior changes in the tools themselves
(the declarative dry_run field is not read by v1 dispatch).

Refs: docs/superpowers/specs/2026-04-24-sp-operability-v1-design.md §6"
```

---

## Task 4: Post-flight + milestone tag

**Files:** None modified.

- [ ] **Step 1: Full 4-gate on HEAD**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count ~331-332.

- [ ] **Step 2: Verify commit history + scope**

```bash
git log --oneline pre-sp-operability-v1..HEAD
```

Expected: exactly 3 commits (C1, C2, C3).

```bash
git diff --stat pre-sp-operability-v1..HEAD | tail -5
```

Expected: files under `crates/atd-protocol/`, `crates/atd-runtime/`, `crates/atd-ref-server-bin/`, `crates/atd-tools-shell/`, `docs/`. Nothing else.

- [ ] **Step 3: End-to-end smoke with all three features**

```bash
rm -f /tmp/op-final.sock /tmp/op-final.jsonl

./target/release/atd-ref-server \
    --sock /tmp/op-final.sock \
    --grant-capability read \
    --grant-capability write \
    --grant-capability exec \
    --audit-log /tmp/op-final.jsonl &
sleep 1

# Simple success call
./target/release/atd --sock /tmp/op-final.sock call ref:echo.say --args '{"text":"ops"}'

# Dry-run short-circuit
./target/release/atd --sock /tmp/op-final.sock call ref:fs.write --args '{"path":"/tmp/x","content":"y"}' --dry-run

# Verify shell.exec now advertises dry_run=true in its schema
./target/release/atd --sock /tmp/op-final.sock schema ref:shell.exec | grep -i "dry_run"

pkill -f 'atd-ref-server --sock /tmp/op-final' 2>/dev/null || true
rm -f /tmp/op-final.sock

echo "=== audit log ==="
cat /tmp/op-final.jsonl
rm -f /tmp/op-final.jsonl
```

Expected:
- 3 lines in audit log: echo success, fs.write dry-run success (with `dry_run: true` at top level), shell.exec schema call (schema fetch doesn't emit audit; only RunTool does — so actually 2 audit lines: echo + fs.write)
- `shell.exec schema` output contains `"dry_run": true` somewhere in the serialized `ToolSafety`

Adjust expectation to what the CLI actually prints.

- [ ] **Step 4: Tag milestone**

```bash
git tag sp-operability-v1
git log --oneline pre-sp-operability-v1..sp-operability-v1
```

Expected: 3 commits listed.

- [ ] **Step 5: No commit for this task** — tag only.

---

## Self-review checklist (fill in after executing)

- [ ] All 3 implementation commits (C1, C2, C3) independently pass 4-gate at HEAD.
- [ ] `cargo test --workspace --all-targets` passes at expected count ~331-332.
- [ ] `--audit-log` omitted → no sink overhead (no audit events, no writer open).
- [ ] `--audit-log stdout` / `--audit-log stderr` / `--audit-log <path>` all work.
- [ ] Tool with `max_concurrent=1` returns 1002 on 2nd concurrent call (verified by `tests/rate_limit.rs`).
- [ ] `shell.exec` and `shell.pwsh` declare `ToolSafety.dry_run: true`.
- [ ] `docs/protocol/dry-run-contract.md` exists.
- [ ] `docs/architecture.md` §10 has 4 flipped rows.
- [ ] `docs/protocol/error-codes.md` lists 1002.
- [ ] Conformance suite still at 32/32 passing (no change).
- [ ] Tags: `pre-sp-operability-v1` at baseline, `sp-operability-v1` at completion.
