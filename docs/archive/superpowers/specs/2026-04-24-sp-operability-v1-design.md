# SP-operability-v1 — Audit Logging + Rate Limiting + Dry-run Consistency

**Date:** 2026-04-24
**Status:** Approved — ready for implementation plan
**Scope:** 3 architecture-§10 ❌ items merged into one operational-readiness SP.
**Parent:** Follows `sp-8.1-capability-denied-gated-tool`.
**Anchor:** `docs/architecture.md` §5.2 ("audit is the most valuable next security-adjacent SP") + §10 roadmap.

## 1. Context

Architecture §10 lists four "no adopter gate" items Q2 2026 expected:
audit logging, rate limiting + `max_concurrent` enforcement, dry-run
consistency across tools, and per-call agent identity tracking (marked
"bundled with audit"). This SP delivers all four in one bisect-clean
package — they share touch points (dispatch lifecycle, CallContext
shape) and land more coherently together than as three separate SPs.

Post-SP, atd-mvp transitions from "protocol + reference implementation"
to "production-operable system": operators can see what happened
(audit), protect downstream resources (rate limit), and reason about
safety drills (dry-run contract).

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q1 | Dry-run scope? | A — Lightweight: documentation of current server-side short-circuit + tool flag audit. No dispatch change. |
| Q2 | Audit hook mechanism? | A — New `AuditSink` trait. Middleware stays result-rewriter-only per its existing semantics. |
| Q3 | Audit sink impls? | A — Only `JsonLinesAuditSink`. CLI flag `--audit-log <stdout\|stderr\|path>`. No tracing crate dep. |
| Q4 | `CallEvent` schema? | Full schema below with top-level `dry_run: bool` flag (Q4a (i)). |
| Q5 | Rate limit scope? | A — Only `max_concurrent` via per-tool `Arc<Semaphore>`. `rate_limit_per_min` stays declarative. |
| Q5a | Rate limit wire shape? | (iii) — New `ToolCallError::RateLimited` variant + `ERR_RATE_LIMITED = 1002`. |
| Q5b | Rate limit conformance fixture? | (i) — Not in this SP. Reference server internal test only; cross-impl conformance deferred. |
| Q6 | SP structure? | A — 3 commits: C1 audit logging (~1 day), C2 rate limit (~0.5 day), C3 dry-run docs + fixups (~0.5 day). Total ~2 days. |

## 3. Touch points

| Crate / file | Change | Commit |
|---|---|---|
| `atd-protocol/src/messages.rs` | Add `pub const ERR_RATE_LIMITED: u16 = 1002;` | C2 |
| `atd-runtime/src/audit.rs` (new) | `AuditSink` trait + `CallEvent` + `Outcome` enum + `JsonLinesAuditSink` + unit tests | C1 |
| `atd-runtime/src/context.rs` | `CallContext` gains `pub caller_id: Option<String>`; annotate `#[non_exhaustive]` | C1 |
| `atd-runtime/src/error.rs` | Add `ToolCallError::RateLimited { tool_id, limit, retry_after_ms }` variant (already `#[non_exhaustive]`) | C2 |
| `atd-runtime/src/registry.rs` | `RegisteredTool` gains `semaphore: Arc<Semaphore>`; annotate `#[non_exhaustive]`; `Registry::register*` sizes the semaphore from `def.resources.max_concurrent` | C2 |
| `atd-runtime/src/lib.rs` | `pub mod audit;` + re-exports (`AuditSink`, `CallEvent`, `JsonLinesAuditSink`) | C1 |
| `atd-ref-server-bin/src/server.rs` | Dispatch loop: per-connection `caller_id` cache from Hello (C1); audit emit at every return (C1); `try_acquire_owned` post-cap / pre-binding (C2) | C1 + C2 |
| `atd-ref-server-bin/src/main.rs` | clap `--audit-log <Option<String>>` parsing + sink installation | C1 |
| `atd-tools-shell/src/exec.rs` | `ToolSafety.dry_run: false → true` (has side effects) | C3 |
| `atd-tools-shell/src/pwsh.rs` | `ToolSafety.dry_run: false → true` | C3 |
| `docs/protocol/dry-run-contract.md` (new) | Informational-field semantics + agent-side contract | C3 |
| `docs/architecture.md` §10 | 4 rows (audit / rate limit / dry-run / per-call identity) flip ❌ → ✅ | C3 |
| `crates/atd-ref-server-bin/tests/audit_emits_events.rs` (new) | Integration: spawn server with `--audit-log <tmpfile>`, run 3 calls, parse events | C1 |
| `crates/atd-ref-server-bin/tests/rate_limit.rs` (new) | Integration: in-process test harness with a gated `max_concurrent = 1` tool, fire 2 concurrent requests, assert 1002 | C2 |

Not touched: `atd-sdk`, `atd-cli`, `atd-mcp-bridge`, `atd-conformance`, `atd-tools-echo`, `atd-tools-fs`, `atd-tools-web`, `atd-runtime::middleware.rs`.

## 4. C1 — Audit logging

### 4.1 `atd-runtime::audit` module (new)

```rust
use serde::Serialize;
use std::io::Write;
use std::sync::Mutex;

/// Structured per-call audit event. Schema version 1.
#[derive(Debug, Clone, Serialize)]
pub struct CallEvent {
    pub ts: String,                        // RFC 3339 UTC
    pub call_id: String,                   // ULID, same as CallContext.call_id
    pub tool_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    pub granted_capabilities: Vec<String>, // sorted
    pub duration_ms: u64,
    pub outcome: Outcome,
    pub tier: String,                      // "hot" | "warm" | "cold"
    pub dry_run: bool,
    pub schema_version: u32,               // 1
}

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

/// Observer-only hook. Writes happen synchronously; the sink
/// owns its own backpressure.
pub trait AuditSink: Send + Sync {
    fn on_call(&self, event: &CallEvent);
}

/// JSON-lines sink. Thread-safe via interior mutex; write errors
/// are swallowed (log loss >> dispatch stall).
pub struct JsonLinesAuditSink {
    writer: Mutex<Box<dyn Write + Send>>,
}

impl JsonLinesAuditSink {
    pub fn new(writer: Box<dyn Write + Send>) -> Self { /* ... */ }
    pub fn stdout() -> Self { /* ... */ }
    pub fn stderr() -> Self { /* ... */ }
    pub fn file(path: &std::path::Path) -> std::io::Result<Self> { /* ... */ }
}

impl AuditSink for JsonLinesAuditSink {
    fn on_call(&self, event: &CallEvent) {
        if let Ok(mut line) = serde_json::to_vec(event) {
            line.push(b'\n');
            if let Ok(mut w) = self.writer.lock() {
                let _ = w.write_all(&line);
                let _ = w.flush();
            }
        }
    }
}
```

### 4.2 CLI flag

```rust
// atd-ref-server-bin main.rs
/// Path or sink keyword for audit log. "stdout", "stderr", or a file path.
/// Omitted → no audit sink (default; zero overhead).
#[arg(long)]
audit_log: Option<String>,
```

Parse at startup:
- `None` → no sink installed
- `Some("stdout")` → `JsonLinesAuditSink::stdout()`
- `Some("stderr")` → `JsonLinesAuditSink::stderr()`
- `Some(path)` → `JsonLinesAuditSink::file(Path::new(path))`; on I/O error, print to stderr and exit code 2

### 4.3 Dispatch integration

Server state gains `audit_sink: Option<Arc<dyn AuditSink>>` and a
per-connection cache of `caller_id: Option<String>` (populated on
Hello). In `handle_message`, each `return Response::...` for a
`Request::RunTool` path is preceded by:

```rust
let event = CallEvent {
    ts: <now RFC3339>,
    call_id: <ULID — same as CallContext::call_id when available>,
    tool_id: tool_id.clone(),
    caller_id: state.caller_id_of(conn).clone(),
    granted_capabilities: caps.granted(), // sorted
    duration_ms: start.elapsed().as_millis() as u64,
    outcome: <derived from the Response being returned>,
    tier: tier.as_str().to_string(),
    dry_run,
    schema_version: 1,
};
if let Some(sink) = &state.audit_sink {
    sink.on_call(&event);
}
// existing return Response::...
```

Outcome derivation rules (by Response shape):
- `Response::ToolResultResponse { success: true, .. }` → `Outcome::Success`
- `Response::ToolResultResponse { success: false, result, .. }` → `Outcome::ExecutionFailed { code: result["code"], retryable: result["retryable"] }`
- `Response::Error { code: Some(1001), details, .. }` → `Outcome::CapabilityDenied { missing: details["missing"] }`
- `Response::Error { code: Some(1002), .. }` → `Outcome::RateLimited { retry_after_ms: None }` (set by C2)
- `Response::Error { message starts with "tool not found", .. }` → `Outcome::ToolNotFound`
- `Response::Error { message starts with "invalid args", .. }` → `Outcome::InvalidArgs { message }`

Only `Request::RunTool` emits events. Ping / Hello / ToolList / ToolSchema are silent.

### 4.4 `CallContext` update

```rust
#[non_exhaustive]   // NEW — future-proof against new fields
pub struct CallContext {
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub call_id: ulid::Ulid,
    pub deadline: Option<Instant>,
    pub read_tracker: Option<Arc<ReadTracker>>,
    pub capabilities: Arc<CapabilitySet>,
    pub tier: ToolTier,
    pub caller_id: Option<String>,  // NEW
}
```

`CallContext::for_test()` defaults `caller_id: None`. All internal
construction sites in server.rs populate from the per-connection
cache.

### 4.5 Tests

- Unit (audit.rs):
  - `JsonLinesAuditSink::new(Vec<u8>)` round-trips a CallEvent through serde
  - All 6 `Outcome` variants serialize to their expected `kind` tags
  - Concurrent `on_call` calls don't interleave JSON lines (spawn 10 threads, verify 10 distinct lines)
- Integration (`tests/audit_emits_events.rs`):
  - Spawn ref-server with `--audit-log <tmpfile>`
  - Issue 3 run_tool requests: (a) success (echo), (b) invalid_args (fs.read missing path), (c) capability_denied (conformance tool)
  - Parse tmpfile as 3 JSON lines; assert each has correct outcome kind, tool_id, duration > 0, schema_version == 1

## 5. C2 — Rate limiting (max_concurrent)

### 5.1 New wire constant + error variant

```rust
// atd-protocol/src/messages.rs
pub const ERR_CAPABILITY_DENIED: u16 = 1001;
pub const ERR_RATE_LIMITED: u16 = 1002;   // NEW
```

```rust
// atd-runtime/src/error.rs — already #[non_exhaustive]
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ToolCallError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("execution failed ({code}): {message}")]
    ExecutionFailed { code: String, message: String, retryable: bool },
    #[error("internal error: {0}")]
    InternalError(String),
    #[error("rate limited ({tool_id}): max_concurrent={limit} in-flight")]
    RateLimited {
        tool_id: String,
        limit: u32,
        retry_after_ms: Option<u64>,
    },
}
```

### 5.2 Registry: per-tool Semaphore

```rust
// atd-runtime/src/registry.rs
use tokio::sync::Semaphore;

#[non_exhaustive]  // NEW — future-proof
pub struct RegisteredTool {
    pub tool: Arc<dyn Tool>,
    pub binding: Arc<dyn Binding>,
    pub semaphore: Arc<Semaphore>,  // NEW
}
```

Constructed in `Registry::register` + `register_with_binding`:

```rust
let max = tool.definition().resources.max_concurrent;
let permits = if max == 0 {
    // Declaratively unlimited (defensive; current builtins all ≥ 1).
    tokio::sync::Semaphore::MAX_PERMITS
} else {
    max as usize
};
let semaphore = Arc::new(Semaphore::new(permits));
```

### 5.3 Dispatch integration

In `server.rs` after the capability check (line ~235) and before tier
derivation / `CallContext` construction:

```rust
let permit = match entry.semaphore.clone().try_acquire_owned() {
    Ok(p) => p,
    Err(_) => {
        // audit (C1) emits with Outcome::RateLimited { retry_after_ms: None }
        return Response::Error {
            message: format!(
                "rate limited for {tool_id}: max_concurrent={} in-flight",
                entry.tool.definition().resources.max_concurrent,
            ),
            code: Some(atd_protocol::ERR_RATE_LIMITED),
            retryable: Some(true),
            details: Some(serde_json::json!({
                "tool_id": tool_id,
                "limit": entry.tool.definition().resources.max_concurrent,
            })),
        };
    }
};
// ... existing CallContext construction + binding.call ...
// permit drops at scope end → semaphore releases automatically
```

`try_acquire_owned` is non-blocking: denial is immediate, returned as
`retryable: true` to signal the client that the request was rejected
for capacity reasons, not correctness reasons.

### 5.4 Tests

- Unit (registry.rs): `RegisteredTool::semaphore.available_permits()` matches `max_concurrent`
- Integration (`tests/rate_limit.rs`):
  - In-process harness (like `dispatch_capability_denied_path.rs`) registering a test tool with `max_concurrent = 1` whose `call` blocks on a `Notify` signal
  - Fire two concurrent requests; first acquires permit and parks; second's `try_acquire_owned` fails and gets 1002
  - Release the first (notify); assert third concurrent request succeeds

### 5.5 Conformance coverage

Deferred per Q5b (i). `atd-conformance` fixtures + self-conformance
test unchanged. Rate-limit cross-impl coverage is a potential SP-8.2
or SP-operability-v2 item.

## 6. C3 — Dry-run consistency

### 6.1 Tool flag audit

Current `ToolSafety.dry_run` declarations vs. side-effect reality:

| Tool | Current | Side effects? | Target |
|---|---|---|---|
| `ref:echo.say` | false | no | false ✓ |
| `ref:fs.read` | false | no | false ✓ |
| `ref:fs.glob` | false | no | false ✓ |
| `ref:fs.grep` | false | no | false ✓ |
| `ref:fs.write` | true | yes | true ✓ |
| `ref:fs.edit` | true | yes | true ✓ |
| `ref:shell.exec` | false | **yes** | **true — fix** |
| `ref:shell.pwsh` | false | **yes** | **true — fix** |
| `ref:web.fetch` | false | no (GET only) | false ✓ |
| `ref:external.uname` | false | no | false ✓ |

Two files change: `atd-tools-shell/src/exec.rs` and `pwsh.rs`, one
line each.

### 6.2 `docs/protocol/dry-run-contract.md` (new, ~40 lines)

Contract document explaining:
1. v1 server-side short-circuit: `dry_run: true` never invokes the tool; synthetic `ToolResultResponse` returned.
2. `ToolSafety.dry_run` is informational: "this tool could in principle support preview".
3. When to declare `true` (side-effect-bearing tools).
4. Agent-side guidance: don't assume `result` reflects tool-specific preview semantics in v1.
5. Audit event correlation: top-level `dry_run: bool` on `CallEvent` distinguishes real calls from drills.
6. Forward-compatibility: SP-operability-v2 may delegate `dry_run: true` to tools declaring it; clients that need preview fidelity should version-gate.

### 6.3 Architecture §10 updates

Flip 4 rows from ❌ to ✅:
- Audit logging → landed; `--audit-log` flag; JsonLinesAuditSink
- Rate limiting + max_concurrent → landed; per-tool Semaphore; 1002 wire code
- Dry-run consistency → landed; short-circuit documented; shell flag corrected
- Per-call agent identity tracking → landed; `CallContext.caller_id` from Hello

### 6.4 Tests

Unit test in `builtin.rs` asserts `shell.exec` and `shell.pwsh` declare
`dry_run: true` (regression guard against silent revert):

```rust
#[test]
fn shell_tools_declare_dry_run_true() {
    let reg = builtin_registry(false);
    assert!(reg.get("ref:shell.exec").unwrap().tool.definition().safety.dry_run);
    assert!(reg.get("ref:shell.pwsh").unwrap().tool.definition().safety.dry_run);
}
```

No new conformance fixtures. Existing `run_tool_echo_dry_run_shape`
covers wire shape stability.

## 7. Non-goals

| Not doing | Why | When it opens |
|---|---|---|
| Token bucket / `rate_limit_per_min` enforcement | `max_concurrent` covers P0 fork-bomb class; token bucket needs multi-caller deployment to matter | SP-operability-v2 when first multi-caller deployment appears |
| Audit `args` / `result` in events | Privacy risk (API keys, file contents); opt-in needs its own design | `--audit-log-include-args` in a follow-up SP when operators request |
| Hello / ping / tool_list events | Noise; audit focuses on tool invocation semantics | Adopter demanding full connection-lifecycle audit |
| Per-tool dry-run preview (dispatch delegation) | Q1=A explicitly chose short-circuit + informational field | SP-operability-v2 candidate when agent code requires preview fidelity |
| Audit schema migration tooling | v1 is first; no data to migrate | Schema bump to v2 |
| External sinks (Datadog, OTel, Slack, …) | `AuditSink` trait is an extension point; third parties write their impl | Specific vendor adopter appears |
| Rate-limit conformance fixture + saturate_op tool | Cross-impl non-normative for v1 | SP-8.2 or SP-operability-v2 |
| Identity sources beyond Hello.client_id | Hello is the protocol's existing identity handshake; transport-level identity needs transport redesign | HTTP transport SP (arch §9.7) |
| CLI-side 1002 human-readable error string | SDK fallback covers; UX work can follow adopter feedback | UX complaint surfaces |

## 8. Backwards-compatibility analysis

### 8.1 Wire format
- New `ERR_RATE_LIMITED = 1002` code — safe; existing clients handle unknown codes via generic `Response::Error` fallback.
- `Response::Error` shape unchanged.
- `ToolResultResponse.dry_run: bool` unchanged.
- No field added to any existing enum variant.

### 8.2 Rust API (atd-runtime)
- `CallContext` gains field — potentially breaking for external Rust
  consumers using struct-literal construction. Mitigated by adding
  `#[non_exhaustive]` to `CallContext` in this SP (breaking now in a
  controlled way, then never breaking again for new fields).
- `RegisteredTool` gains field — same story; add `#[non_exhaustive]`.
- `ToolCallError` already `#[non_exhaustive]` (SP-refactor-v1); new
  variant non-breaking for match arms with wildcards.
- `CallEvent`, `Outcome`, `AuditSink`, `JsonLinesAuditSink` — all net
  new; no compatibility concerns.

### 8.3 Runtime behavior
- Default (no `--audit-log` flag) → no audit sink → **zero overhead**
  vs. pre-SP-operability-v1 dispatch.
- Rate limit always active once C2 lands, but existing built-in tools
  declare generous `max_concurrent` values (echo=100, fs.read=50,
  fs.edit/write=20, fs.glob/grep=10, shell.exec/pwsh=10, web.fetch=10,
  external.uname=8) that dwarf any realistic per-connection workload —
  no accidental denials expected.

## 9. Success criteria

SP is complete when all of the following hold:

1. 4-gate green on each commit (fmt + clippy + test + release build).
2. `./target/release/atd-ref-server --sock ... --audit-log <tmpfile>` produces one JSON line per `run_tool`; schema parses back cleanly.
3. `--audit-log` omitted → no audit overhead (no sink allocation, no emission).
4. Setting a tool's `max_concurrent = 1` and firing 2 concurrent calls yields: first `tool_result`, second `Response::Error { code: 1002, retryable: true }`.
5. Audit event for rate-limited call has `outcome: { kind: "rate_limited" }`.
6. `docs/protocol/dry-run-contract.md` exists; `docs/architecture.md` §10 has 4 flipped rows.
7. `shell.exec` / `shell.pwsh` declare `ToolSafety.dry_run: true`.
8. Workspace test count: 322 → ~328-330 (+6-8 from new unit + integration tests).
9. Conformance: 32 cases still pass; self-conformance test unchanged.
10. Tag `sp-operability-v1` on C3 (the final commit).

## 10. Rollback

Before starting: `git tag pre-sp-operability-v1`. Each of C1/C2/C3 is
independently revertable. Worst case: `git reset --hard
pre-sp-operability-v1` removes all SP changes.

## 11. Next steps unlocked

- **Rate-limit conformance coverage** (SP-8.2 candidate): add
  `ref:conformance.saturate_op` and fixture; fold into the
  conformance suite.
- **Audit args opt-in**: `--audit-log-include-args` flag with a privacy
  warning banner; needs a separate brainstorm on redaction rules.
- **Token bucket `rate_limit_per_min`**: SP-operability-v2 when
  multi-caller deployments arrive; `retry_after_ms` field in
  `Outcome::RateLimited` already reserved.
- **UCAN capability tokens** (arch §9.3, Phase 2): per-call identity
  tracking is now in place, which is a prerequisite.
- **External sinks** (Datadog, OTel, …): trait is ready; each is a
  ~100-line crate once an adopter wants it.
