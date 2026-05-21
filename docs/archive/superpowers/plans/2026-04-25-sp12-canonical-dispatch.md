# SP-12 — Canonical Dispatch Demo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship four dispatch primitives in `atd-ref-server` — binding abstraction, capability allow-list gate, result-middleware pipeline, tier-aware deadlines — plus the wire additions (`Hello`/`HelloAck`, `CAPABILITY_DENIED`) and one demonstration tool (`ref:external.uname`). Structural match to the v3 brief's Slide 1 "Dispatch Layer"; no v3 distributed/cryptographic/multi-device features.

**Architecture:** Six sequential tasks. Task 1 lays the protocol + empty-shell modules so each subsequent primitive can be implemented and tested in isolation without re-editing `server.rs::dispatch` every time. Tasks 2–5 fill each primitive end-to-end (test-first). Task 6 is the cross-primitive E2E, docs updates, Python parity, and release tag.

**Tech Stack:** Rust (workspace), tokio async, serde_json on the wire, `tokio::process::Command` for `CliBinding`, `regex` for `RedactPathsMiddleware`. Python `atd_client` gets a single new method.

**Spec:** `docs/superpowers/specs/2026-04-25-sp12-canonical-dispatch.md` — all type sketches, module layout, wire shapes, test-strategy mapping live there.

**Locked decisions** (from spec §8 review):
- Q1 error code → **ad-hoc `u16`** (enum-ification is a separate refactor)
- Q2 handshake → **new `Hello`** (keep `Ping` heartbeat-only)
- Q3 uname → **`#[cfg(unix)]` gated registration** + skip on Windows CI
- Q4 middleware-on-error → **success-only** in SP-12; `on_error` when a real consumer appears
- Q5 missing tier → **default `Warm`** (back-compat)

**Scope boundary:**
- **In:** `binding.rs` / `capability.rs` / `middleware.rs` / `tier.rs`; edits to `registry.rs`, `context.rs`, `protocol.rs`, `server.rs`, `main.rs`; one new tool `ref:external.uname`; Rust `AtdClient::hello()` + Python parity; README/design.md edits; integration tests.
- **Out:** UCAN crypto, device affinity, distributed sessions, additional middlewares, AppFunction/REST bindings, porting existing 9 tools to declare `required_capabilities`/`tier`, `AtdErrorCode` enum refactor.

**Prerequisites:**
- `sp11-docs` tag; 252 workspace tests green.
- `/usr/bin/uname` present on Linux dev + CI runners (verified on `ubuntu-latest`).
- No external crates beyond `regex` (already in workspace via `atd-ref-server/tools/fs`).

**Exit criteria:**
1. Six integration tests in `crates/atd-ref-server/tests/` all green (per spec §5.2).
2. All four primitives observable in `server.rs::dispatch` call flow (spec §3.6 diagram matches code).
3. `CAPABILITY_DENIED` surfaces as `AtdError::CapabilityDenied` with both sets populated.
4. `Hello` is additive: a client that skips it still reaches every tool with empty `required_capabilities`.
5. `cargo test --workspace --all-targets` passes; total = 252 (existing) + ≥ 18 (new unit) + 6 (new integration) = **≥ 276**.
6. `clippy --all-targets --all-features -- -D warnings` clean.
7. README + design.md §2.1 reflect new dispatch layer.
8. Tag `sp12-canonical-dispatch` applied.

---

## File Structure

```
crates/atd-ref-server/
├── src/
│   ├── binding.rs           (NEW — Task 4)
│   ├── capability.rs        (NEW — Task 2)
│   ├── middleware.rs        (NEW — Task 5)
│   ├── tier.rs              (NEW — Task 3)
│   ├── context.rs           (MOD — Task 1 adds fields)
│   ├── protocol.rs          (MOD — Task 1 adds Hello/HelloAck)
│   ├── registry.rs          (MOD — Task 4 adds binding assoc)
│   ├── server.rs            (MOD — Tasks 1..5 each add one insertion point)
│   ├── main.rs              (MOD — Task 1 + Task 3 + Task 5 add CLI flags)
│   ├── lib.rs               (MOD — re-export new modules as they land)
│   └── tools/
│       └── external/
│           └── uname.rs     (NEW — Task 4)
└── tests/
    ├── dispatch_capability_denied_path.rs   (NEW — Task 2)
    ├── dispatch_capability_granted_path.rs  (NEW — Task 2)
    ├── dispatch_tier_hot_deadline.rs        (NEW — Task 3)
    ├── dispatch_cli_binding_uname.rs        (NEW — Task 4)
    ├── dispatch_middleware_redacts_home.rs  (NEW — Task 5)
    └── dispatch_end_to_end.rs               (NEW — Task 6)

crates/atd-client/
└── src/
    └── client.rs            (MOD — Task 6 adds hello())

python/src/atd_client/
├── client.py                (MOD — Task 6 adds hello())
└── protocol.py              (MOD — Task 6 adds Hello message)

README.md                    (MOD — Task 6 architecture-at-a-glance update)
docs/design.md               (MOD — Task 6 §2.1 dispatch paragraph)
crates/atd-mcp-bridge/README.md  (MOD — Task 6 one-paragraph capability note)
```

---

## Task 1: Protocol foundation + empty-shell modules

**Purpose:** Land the additive wire messages, the `CAPABILITY_DENIED` error code constant, and the four empty primitive modules with their trait signatures. Everything compiles; no behavior changes; dispatch is unmodified. This lets Tasks 2–5 implement each primitive in isolation with a minimal diff against `server.rs`.

**Files:**
- Modify: `crates/atd-ref-server/src/protocol.rs`
- Modify: `crates/atd-ref-server/src/context.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`
- Create: `crates/atd-ref-server/src/capability.rs` (signatures only)
- Create: `crates/atd-ref-server/src/tier.rs` (signatures only)
- Create: `crates/atd-ref-server/src/binding.rs` (signatures only)
- Create: `crates/atd-ref-server/src/middleware.rs` (signatures only)

### Step 1.1: Add wire messages

- [ ]

Edit `crates/atd-ref-server/src/protocol.rs`:

1. Add to `enum Request`:
   ```rust
   #[serde(rename = "hello")]
   Hello {
       #[serde(default, skip_serializing_if = "Option::is_none")]
       client_id: Option<String>,
       #[serde(default)]
       requested_capabilities: Vec<String>,
   },
   ```
2. Add to `enum Response`:
   ```rust
   #[serde(rename = "hello_ack")]
   HelloAck {
       granted_capabilities: Vec<String>,
       server_version: String,
       supported_tiers: Vec<String>,
   },
   ```
3. Keep **all existing variants** and **all existing tag strings** byte-identical.
4. Add an exported constant `pub const ERR_CAPABILITY_DENIED: u16 = 1001;` at module top. (Existing `Error.code` is `Option<u16>`; 1000+ range is free.)
5. Unit tests (add to the existing `#[cfg(test)] mod tests` block):
   - `hello_serializes_with_default_empty_caps`
   - `hello_roundtrip_with_client_id`
   - `hello_ack_roundtrip_with_granted_caps_and_tiers`
   - `existing_ping_response_is_unchanged` (regression)

### Step 1.2: Extend `CallContext`

- [ ]

Edit `crates/atd-ref-server/src/context.rs`:

1. Add fields (both non-optional; `CapabilitySet` and `Tier` will carry empty-default values):
   ```rust
   pub capabilities: std::sync::Arc<crate::capability::CapabilitySet>,
   pub tier: crate::tier::Tier,
   ```
2. Update `for_test()` and `for_test_with_tracker()` to inject empty-default values (`CapabilitySet::empty()` and `Tier::Warm`).
3. Existing tests must still pass unchanged.

### Step 1.3: Create empty `capability.rs`

- [ ]

```rust
// crates/atd-ref-server/src/capability.rs

use std::collections::BTreeSet;

#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    granted: BTreeSet<String>,
}

impl CapabilitySet {
    pub fn empty() -> Self { Self::default() }
    pub fn from_iter(iter: impl IntoIterator<Item = String>) -> Self { /* ... */ }
    pub fn contains(&self, cap: &str) -> bool { /* ... */ }
    pub fn granted(&self) -> Vec<String> { /* sorted Vec<String> */ }
    /// Intersect requested with self. Returns (granted, denied).
    pub fn intersect(&self, requested: &[String]) -> (Vec<String>, Vec<String>) { /* ... */ }
}

pub struct DenialError {
    pub tool_id: String,
    pub required: Vec<String>,
    pub granted: Vec<String>,
}
```

Unit tests (same file, ≥ 5):
- `empty_contains_nothing`
- `intersect_with_empty_granted_denies_all`
- `intersect_partial`
- `intersect_full`
- `granted_returns_sorted_deterministic_order`

### Step 1.4: Create empty `tier.rs`

- [ ]

```rust
// crates/atd-ref-server/src/tier.rs
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier { Hot, Warm, Cold }

impl Default for Tier { fn default() -> Self { Tier::Warm } }

impl Tier {
    /// Parse from the string used on `ToolSummary.tier`. Returns `Warm` on
    /// `None`/unknown (per spec §8 Q5 locked decision).
    pub fn from_opt_str(s: Option<&str>) -> Self { /* ... */ }
}

#[derive(Debug, Clone)]
pub struct TierPolicy {
    pub hot_timeout: Duration,
    pub warm_timeout: Duration,
    pub cold_timeout: Duration,
    pub hot_max_output: usize,
    pub warm_max_output: usize,
    pub cold_max_output: usize,
}

impl TierPolicy {
    /// Hot=500ms/64KiB, Warm=5s/1MiB (current server default), Cold=60s/16MiB.
    pub fn defaults() -> Self { /* ... */ }

    pub fn timeout(&self, tier: Tier) -> Duration { /* ... */ }
    pub fn max_output(&self, tier: Tier) -> usize { /* ... */ }

    /// Parse `"hot=timeout_ms=300"` → mutate self.
    pub fn apply_override(&mut self, spec: &str) -> Result<(), String> { /* ... */ }
}
```

Unit tests (≥ 5):
- `from_opt_str_defaults_to_warm_on_none`
- `from_opt_str_defaults_to_warm_on_unknown`
- `defaults_match_current_server_warm_budget` (pins `Warm` to existing 1 MiB / 60 s behavior for migration safety)
- `apply_override_timeout_ms`
- `apply_override_rejects_malformed_spec`

### Step 1.5: Create empty `binding.rs` and `middleware.rs`

- [ ]

`binding.rs` — just the trait and a placeholder enum:

```rust
use crate::context::CallContext;
use crate::error::ToolCallError;
use atd_types::ToolDefinition;

pub type BindingFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, ToolCallError>> + Send + 'a>>;

pub trait Binding: Send + Sync {
    fn name(&self) -> &'static str;
    fn call<'a>(
        &'a self,
        tool_def: &'a ToolDefinition,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> BindingFuture<'a>;
}
```

`middleware.rs`:

```rust
use atd_types::ToolDefinition;

pub trait Middleware: Send + Sync {
    fn name(&self) -> &'static str;
    fn on_result(
        &self,
        tool_id: &str,
        tool_def: &ToolDefinition,
        result: &mut serde_json::Value,
    );
}
```

No impls yet. No tests beyond "the module compiles" (a smoke `#[test]` that constructs nothing; trait-only modules are covered when their first impl lands in Tasks 4 and 5).

### Step 1.6: Re-export from `lib.rs`

- [ ]

Add:
```rust
pub mod binding;
pub mod capability;
pub mod middleware;
pub mod tier;
```

### Step 1.7: Verify + commit

- [ ]

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server
cargo test -p atd-ref-server --lib -- capability:: tier:: protocol::tests::hello
cargo test --workspace --all-targets 2>&1 | grep 'test result:' | awk '{s+=$4} END{print "total:", s}'
# Expect 252 existing + ~10 new unit tests = ~262. No regressions.

cargo clippy -p atd-ref-server --all-targets -- -D warnings

git add crates/atd-ref-server/src/{protocol,context,capability,tier,binding,middleware,lib}.rs
git commit -m "feat(ref-server): SP-12 foundation — Hello wire, CapabilitySet, Tier, Binding/Middleware traits"
```

---

## Task 2: Capability gate — wire + dispatch + tests

**Purpose:** Wire up connection-scoped capability handshake + dispatch-time enforcement. First primitive to modify `server.rs`.

**Files:**
- Modify: `crates/atd-ref-server/src/server.rs`
- Modify: `crates/atd-ref-server/src/main.rs`
- Modify: `crates/atd-types/src/tool.rs` (add `required_capabilities` field)
- Modify: `crates/atd-ref-server/src/context.rs` (remove placeholder wiring from Task 1 if any)
- Create: `crates/atd-ref-server/tests/dispatch_capability_denied_path.rs`
- Create: `crates/atd-ref-server/tests/dispatch_capability_granted_path.rs`

### Step 2.1: Add `required_capabilities` to `ToolDefinition`

- [ ]

Edit `crates/atd-types/src/tool.rs`:

```rust
#[serde(default)]
pub required_capabilities: Vec<String>,
```

Add `#[serde(default)]` so older-serialized definitions still parse. Update any `ToolDefinition { ... }` constructors in the workspace (grep for brace-init sites; builtin tools in `atd-ref-server/src/tools/` will need `required_capabilities: vec![]` added — ~9 sites).

Unit test: `tool_definition_default_deserializes_empty_required_capabilities` in `atd-types`.

### Step 2.2: Write the denial integration test FIRST (TDD — red)

- [ ]

Create `crates/atd-ref-server/tests/dispatch_capability_denied_path.rs`:

Skeleton:
```rust
// Spin up atd-ref-server binary, pre-register a test tool with
// required_capabilities: ["exec"], connect a raw Unix-socket client,
// send Hello{requested: []}, call run_tool, assert Error with
// code = ERR_CAPABILITY_DENIED and details.required == ["exec"],
// details.granted == [].
```

Reuse the integration-test harness pattern from existing `tests/` (see `tests/end_to_end.rs` or equivalent in SP-1). The test must register a **test-only** tool that declares `required_capabilities: vec!["exec".into()]` — don't mutate builtins.

Run `cargo test -p atd-ref-server --test dispatch_capability_denied_path` → expect **red** (dispatch doesn't check yet).

### Step 2.3: Implement the dispatch gate (green)

- [ ]

Edit `crates/atd-ref-server/src/server.rs`:

1. Thread a per-connection `Arc<CapabilitySet>` through `handle_connection`, initialized to `CapabilitySet::empty()`.
2. Handle `Request::Hello` before the main loop (or within it — see `handle_connection` existing flow). On `Hello`:
   - Intersect `requested_capabilities` with the server's `granted_set` (injected via `ServerConfig`).
   - Replace connection's `CapabilitySet` with `CapabilitySet::from_iter(granted_subset)`.
   - Respond `HelloAck { granted_capabilities, server_version: env!("CARGO_PKG_VERSION").into(), supported_tiers: vec!["hot","warm","cold"].into() }`.
3. In the `RunTool` arm of `dispatch`, **immediately after** the `registry.get(tool_id)` lookup, compute `required = tool_def.required_capabilities.clone()`. If any element is not in the connection's `CapabilitySet`:
   ```rust
   return Response::Error {
       message: format!("capability denied for {tool_id}"),
       code: Some(crate::protocol::ERR_CAPABILITY_DENIED),
       retryable: Some(false),
       details: Some(json!({"required": required, "granted": caps.granted()})),
   };
   ```
4. Put `caps.clone()` onto `CallContext.capabilities` before invoking the tool.

Re-run denial test → expect **green**.

### Step 2.4: Write the granted-path test + verify

- [ ]

Create `crates/atd-ref-server/tests/dispatch_capability_granted_path.rs`:
- Server config: `granted_set = {"exec"}`.
- Client: `Hello{requested: ["exec"]}` → `HelloAck{granted: ["exec"]}`.
- Call the `required_capabilities: ["exec"]` test tool → expect `ToolResult{success: true}`.

### Step 2.5: Add `--grant-capability` CLI flag

- [ ]

Edit `crates/atd-ref-server/src/main.rs`:

```rust
/// Grant a named capability to clients that request it during Hello.
/// Repeatable: --grant-capability read --grant-capability exec
#[arg(long = "grant-capability", action = clap::ArgAction::Append)]
grant_capabilities: Vec<String>,
```

Feed into `ServerConfig`. Integration tests set this via the spawned-binary harness (or, in-process tests, via `ServerConfig` directly).

### Step 2.6: Verify + commit

- [ ]

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --test dispatch_capability_denied_path
cargo test -p atd-ref-server --test dispatch_capability_granted_path
cargo test --workspace --all-targets
cargo clippy -p atd-ref-server --all-targets -- -D warnings

git add crates/atd-types crates/atd-ref-server
git commit -m "feat(ref-server): SP-12 capability gate — Hello handshake, dispatch enforcement"
```

---

## Task 3: Tier-aware dispatch

**Purpose:** Make `ToolDefinition.tier` load-bearing. Deadline and max-output come from tier policy instead of the global config default.

**Files:**
- Modify: `crates/atd-ref-server/src/server.rs`
- Modify: `crates/atd-ref-server/src/main.rs`
- Create: `crates/atd-ref-server/tests/dispatch_tier_hot_deadline.rs`

### Step 3.1: Write hot-tier timeout integration test (red)

- [ ]

`tests/dispatch_tier_hot_deadline.rs`:
- Register a test tool declaring `tier: "hot"` that sleeps 500 ms then returns `{"ok":true}`.
- Server config: `TierPolicy::defaults()` with override `hot=timeout_ms=100`.
- Expect `ToolResult{success: false}` with a timeout-flavored error, **not** `{"ok":true}`.

Expect red: dispatch currently uses `default_call_timeout_ms`, ignores tier.

### Step 3.2: Implement tier-derived deadline (green)

- [ ]

Edit `server.rs`:

1. `ServerState` gains `tier_policy: TierPolicy`.
2. In the `RunTool` arm, after capability check, before `CallContext` construction:
   ```rust
   let tier = Tier::from_opt_str(tool.definition().tier.as_deref());
   let timeout = state.tier_policy.timeout(tier);
   let max_output = state.tier_policy.max_output(tier);
   ```
3. `CallContext` uses `timeout` for `deadline` and `max_output` for `max_output_bytes`, instead of the global config values.
4. Set `ctx.tier = tier`.

Re-run timeout test → green.

### Step 3.3: Add matching warm-tier test

- [ ]

A second integration test (or an added `#[tokio::test]` in the same file) using `tier: "warm"`, same 500 ms sleep, `warm=timeout_ms=5000` (default) → expect success. This pins tier-differentiation: same tool, different tier, different outcome.

### Step 3.4: Add `--tier-override` CLI flag

- [ ]

```rust
/// Override tier budgets. Repeatable. Format: "<tier>=<key>=<value>".
/// Keys: timeout_ms, max_output_bytes. Tiers: hot, warm, cold.
/// Example: --tier-override hot=timeout_ms=300
#[arg(long = "tier-override", action = clap::ArgAction::Append)]
tier_overrides: Vec<String>,
```

On startup: construct `TierPolicy::defaults()`, then apply each `--tier-override` via `TierPolicy::apply_override`. Malformed specs → exit 2 with a clear message.

### Step 3.5: Regression check — existing tools unchanged

- [ ]

Existing 9 tools do **not** declare `tier`. They must parse as `Warm` (per locked Q5) and keep current behavior. Verify by re-running the existing `end_to_end.rs` suite — no edits needed.

### Step 3.6: Verify + commit

- [ ]

```bash
cargo test -p atd-ref-server --test dispatch_tier_hot_deadline
cargo test --workspace --all-targets
cargo clippy -p atd-ref-server --all-targets -- -D warnings

git add crates/atd-ref-server
git commit -m "feat(ref-server): SP-12 tier-aware dispatch — Hot/Warm/Cold deadline + max_output"
```

---

## Task 4: Binding abstraction + `ref:external.uname`

**Purpose:** Introduce `Binding` between `Tool` and execution. Refactor registry so each tool is paired with a binding; wrap existing tools in `NativeBinding`; add `CliBinding` + one tool that uses it.

**Files:**
- Modify: `crates/atd-ref-server/src/binding.rs` (fill in impls)
- Modify: `crates/atd-ref-server/src/registry.rs`
- Modify: `crates/atd-ref-server/src/server.rs`
- Modify: `crates/atd-ref-server/src/builtin.rs`
- Modify: `crates/atd-ref-server/src/tools/mod.rs`
- Create: `crates/atd-ref-server/src/tools/external/mod.rs`
- Create: `crates/atd-ref-server/src/tools/external/uname.rs`
- Create: `crates/atd-ref-server/tests/dispatch_cli_binding_uname.rs`

### Step 4.1: Implement `NativeBinding` + `CliBinding`

- [ ]

`binding.rs`:

```rust
pub struct NativeBinding {
    tool: std::sync::Arc<dyn crate::registry::Tool>,
}

impl NativeBinding {
    pub fn new(tool: std::sync::Arc<dyn crate::registry::Tool>) -> Self { Self { tool } }
}

impl Binding for NativeBinding {
    fn name(&self) -> &'static str { "native" }
    fn call<'a>(&'a self, _tool_def: &'a ToolDefinition, args: Value, ctx: &'a CallContext)
        -> BindingFuture<'a>
    { self.tool.call(args, ctx) }
}

pub struct CliBinding {
    pub program: std::path::PathBuf,
    pub base_args: Vec<String>,
    /// How to map args JSON into argv. SP-12 uses a very simple strategy:
    /// take the `flag` field if present, append verbatim. Sufficient for uname.
    pub args_mapper: fn(&serde_json::Value) -> Vec<String>,
}

impl Binding for CliBinding {
    fn name(&self) -> &'static str { "cli" }
    fn call<'a>(&'a self, _tool_def: &'a ToolDefinition, args: Value, ctx: &'a CallContext)
        -> BindingFuture<'a>
    {
        Box::pin(async move {
            let mut argv = self.base_args.clone();
            argv.extend((self.args_mapper)(&args));
            let deadline = ctx.remaining_time().unwrap_or(std::time::Duration::from_secs(5));
            let output = tokio::time::timeout(
                deadline,
                tokio::process::Command::new(&self.program).args(&argv).output(),
            ).await
              .map_err(|_| ToolCallError::ExecutionFailed {
                  code: "TIMEOUT".into(), message: "cli binding timed out".into(), retryable: false,
              })?
              .map_err(|e| ToolCallError::InternalError(e.to_string()))?;
            if !output.status.success() {
                return Err(ToolCallError::ExecutionFailed {
                    code: format!("EXIT_{}", output.status.code().unwrap_or(-1)),
                    message: String::from_utf8_lossy(&output.stderr).into(),
                    retryable: false,
                });
            }
            Ok(serde_json::json!({
                "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                "exit_code": output.status.code().unwrap_or(0),
            }))
        })
    }
}
```

Unit tests in `binding.rs`:
- `cli_binding_runs_true_program_succeeds` (uses `/bin/true`)
- `cli_binding_runs_false_program_surfaces_nonzero_exit`
- `cli_binding_times_out_on_sleep_past_deadline`
- `native_binding_delegates_to_tool`

All gated `#[cfg(unix)]`.

### Step 4.2: Add binding to `Registry`

- [ ]

Edit `registry.rs`:

```rust
pub struct RegisteredTool {
    pub tool: std::sync::Arc<dyn Tool>,
    pub binding: std::sync::Arc<dyn crate::binding::Binding>,
}

impl Registry {
    pub fn register(&mut self, tool: std::sync::Arc<dyn Tool>) {
        // Default: NativeBinding — preserves current behavior for all 9 tools
        let binding = std::sync::Arc::new(crate::binding::NativeBinding::new(tool.clone()));
        self.register_with_binding(tool, binding);
    }

    pub fn register_with_binding(
        &mut self,
        tool: std::sync::Arc<dyn Tool>,
        binding: std::sync::Arc<dyn crate::binding::Binding>,
    ) { /* ... */ }
}
```

`Registry::get` returns `Option<&RegisteredTool>`. Existing callers adjust: `.tool` for the definition, `.binding` for the call.

### Step 4.3: Update dispatch to route through binding

- [ ]

`server.rs`: change the call site from `tool.call(args, &ctx)` to `reg_entry.binding.call(reg_entry.tool.definition(), args, &ctx)`.

Re-run full suite: all 9 existing tools still pass (they run through `NativeBinding` now).

### Step 4.4: Add `ref:external.uname`

- [ ]

`tools/external/uname.rs`:

```rust
use atd_types::{ToolDefinition, /* ... */};

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        id: "ref:external.uname".into(),
        name: "uname".into(),
        description: "Operating system identifier via CLI uname.".into(),
        // tier: Some("hot".into()), required_capabilities: vec![], ...
        // input_schema: { flag: "-s"|"-a"|"-r"|"-m" }
        // output_schema: { stdout: string, exit_code: int }
        /* fill in per ToolDefinition shape */
    }
}

pub fn args_mapper(args: &serde_json::Value) -> Vec<String> {
    args.get("flag").and_then(|v| v.as_str()).map(|s| vec![s.to_string()]).unwrap_or_default()
}
```

`tools/external/mod.rs` re-exports; `tools/mod.rs` adds `#[cfg(unix)] pub mod external;`.

### Step 4.5: Register uname via `CliBinding`

- [ ]

In `builtin.rs::builtin_registry()` (or equivalent), after registering the 9 Native tools, gate on unix:

```rust
#[cfg(unix)]
{
    use crate::binding::CliBinding;
    let uname_def = crate::tools::external::uname::definition();
    let uname_tool = std::sync::Arc::new(/* a trivial Tool wrapper that holds only the definition */);
    let binding = std::sync::Arc::new(CliBinding {
        program: "/usr/bin/uname".into(),
        base_args: vec![],
        args_mapper: crate::tools::external::uname::args_mapper,
    });
    reg.register_with_binding(uname_tool, binding);
}
```

The `Tool` wrapper exists only because `RegisteredTool` pairs `(tool, binding)`; for pure-CLI tools, `Tool::call` can be `unreachable!()` — the dispatch path never reaches it because `binding.call` shadows it. Document this with a one-liner comment.

### Step 4.6: Integration test

- [ ]

`tests/dispatch_cli_binding_uname.rs` (`#[cfg(unix)]`):
- Start server, connect, skip `Hello` (no capabilities needed; uname declares none).
- `run_tool ref:external.uname { "flag": "-s" }`
- Expect `result.stdout` == `"Linux\n"` on CI linux runners; accept `"Darwin\n"` if running on macOS dev box. (Use `cfg!(target_os = "linux")` to pick the expected value, or relax the assert to `result.stdout.trim().len() > 0`.)

### Step 4.7: Verify + commit

- [ ]

```bash
cargo test -p atd-ref-server --test dispatch_cli_binding_uname
cargo test --workspace --all-targets
cargo clippy -p atd-ref-server --all-targets -- -D warnings

git add crates/atd-ref-server
git commit -m "feat(ref-server): SP-12 binding abstraction — NativeBinding, CliBinding, ref:external.uname"
```

---

## Task 5: Result-middleware pipeline + `RedactPathsMiddleware`

**Purpose:** Ship the `Middleware` trait, a one-middleware chain, and the `redact_paths` built-in. Dispatch runs the chain post-success.

**Files:**
- Modify: `crates/atd-ref-server/src/middleware.rs`
- Modify: `crates/atd-ref-server/src/server.rs`
- Modify: `crates/atd-ref-server/src/main.rs`
- Create: `crates/atd-ref-server/tests/dispatch_middleware_redacts_home.rs`

### Step 5.1: Implement `RedactPathsMiddleware`

- [ ]

```rust
pub struct RedactPathsMiddleware {
    patterns: Vec<(regex::Regex, String)>,  // (pattern, replacement)
}

impl RedactPathsMiddleware {
    /// Default: redact absolute paths under $HOME.
    pub fn with_home_default() -> Self { /* ... */ }

    pub fn with_patterns(patterns: Vec<(regex::Regex, String)>) -> Self { /* ... */ }
}

impl Middleware for RedactPathsMiddleware {
    fn name(&self) -> &'static str { "redact_paths" }

    fn on_result(&self, _: &str, _: &ToolDefinition, result: &mut Value) {
        // Recursive walk: for every string leaf, apply each pattern in order.
        walk_mut(result, &mut |s| {
            for (re, rep) in &self.patterns { *s = re.replace_all(s, rep.as_str()).into_owned(); }
        });
    }
}
```

Unit tests (same file, ≥ 6):
- `redacts_home_in_top_level_string`
- `redacts_home_in_nested_object`
- `redacts_home_in_array`
- `leaves_non_matching_paths_untouched`
- `applies_multiple_patterns_in_order`
- `with_home_default_handles_missing_HOME_env_gracefully`

### Step 5.2: Wire the chain into dispatch

- [ ]

`ServerState` gains `middleware: Vec<Arc<dyn Middleware>>`. In the `RunTool` arm, **only on success**:

```rust
match binding.call(...).await {
    Ok(mut data) => {
        for mw in &state.middleware { mw.on_result(&tool_id, tool.definition(), &mut data); }
        Response::ToolResult { tool_id, result: data, success: true, dry_run: false }
    }
    // error paths unchanged
}
```

### Step 5.3: Integration test

- [ ]

`tests/dispatch_middleware_redacts_home.rs`:
- Register a test tool whose output is `{ "path": format!("{}/secret/file.txt", std::env::var("HOME").unwrap()) }`.
- Register server with default `RedactPathsMiddleware::with_home_default()`.
- Call tool, expect `result.path == "<redacted:home>/secret/file.txt"`.

### Step 5.4: Add `--middleware` CLI flag

- [ ]

```rust
/// Enable result-middleware by name. Repeatable.
/// Known: redact_paths (default). Unknown names exit 2.
#[arg(long = "middleware", action = clap::ArgAction::Append, default_values_t = vec!["redact_paths".to_string()])]
middleware: Vec<String>,
```

Resolve to `Vec<Arc<dyn Middleware>>` at startup. Unknown name → clear error, exit 2.

### Step 5.5: Verify + commit

- [ ]

```bash
cargo test -p atd-ref-server --test dispatch_middleware_redacts_home
cargo test --workspace --all-targets
cargo clippy -p atd-ref-server --all-targets -- -D warnings

git add crates/atd-ref-server
git commit -m "feat(ref-server): SP-12 result-middleware pipeline — RedactPathsMiddleware"
```

---

## Task 6: Cross-primitive E2E, client parity, docs, tag

**Purpose:** Prove all four primitives interact correctly, add client surfaces for `Hello`, update the canonical docs, and cut the release tag.

**Files:**
- Modify: `crates/atd-client/src/client.rs`
- Modify: `crates/atd-client/src/protocol.rs`
- Modify: `crates/atd-client/src/options.rs` (maybe — for `HelloOptions`)
- Modify: `python/src/atd_client/client.py`
- Modify: `python/src/atd_client/protocol.py`
- Modify: `python/src/atd_client/errors.py` (add `CapabilityDenied`)
- Modify: `README.md`
- Modify: `docs/design.md`
- Modify: `crates/atd-mcp-bridge/README.md`
- Create: `crates/atd-ref-server/tests/dispatch_end_to_end.rs`

### Step 6.1: Cross-primitive integration test

- [ ]

`tests/dispatch_end_to_end.rs`:
- Server: `grant_capabilities = {"exec", "read"}`, `middleware = [redact_paths]`, `tier_override = hot=timeout_ms=2000`.
- Register a test tool: `id="ref:demo.fullstack"`, `required_capabilities=["exec"]`, `tier="hot"`, routed via `NativeBinding`, returns `{"path": "$HOME/x", "touched_binding": "native"}`.
- Client: `Hello{requested=["exec","read","admin"]}` → `HelloAck{granted=["exec","read"]}` (admin denied).
- Call the tool → expect `ToolResult{success=true, result.path == "<redacted:home>/x"}`.
- Second client omits `Hello` entirely, calls the same tool → expect `Error{code=ERR_CAPABILITY_DENIED}` with `required=["exec"], granted=[]`.
- Log-based assertion (or custom hook) confirms: binding chosen = "native", tier resolved = "hot", middleware ran (`redact_paths`).

This single test exercises **all four primitives** in one call flow (spec §5.2 test #6).

### Step 6.2: Rust `AtdClient::hello()`

- [ ]

`crates/atd-client/src/protocol.rs` — add `Hello` / `HelloAck` variants (mirror the server types, independent definitions).

`crates/atd-client/src/client.rs`:

```rust
pub async fn hello(&self, requested: Vec<String>) -> Result<Vec<String>, AtdError> {
    match self.request(&Request::Hello { client_id: None, requested_capabilities: requested }).await {
        Ok(Response::HelloAck { granted_capabilities, .. }) => Ok(granted_capabilities),
        // Server pre-dates SP-12 and doesn't know Hello: demote to "empty capabilities".
        Ok(Response::Error { .. }) | Err(AtdError::ProtocolError { .. }) => Ok(vec![]),
        other => /* ProtocolError */,
    }
}
```

Map `Response::Error{code: Some(ERR_CAPABILITY_DENIED), details}` in `call()` to `AtdError::CapabilityDenied{ required, granted }` by parsing `details`.

Unit tests (existing `mod tests` pattern in `client.rs`):
- `hello_returns_granted_subset`
- `hello_on_pre_sp12_server_returns_empty_caps`
- `call_surfaces_capability_denied_with_both_sets`

### Step 6.3: Python `atd_client` parity

- [ ]

`python/src/atd_client/protocol.py`: add `Hello`, `HelloAck`.

`python/src/atd_client/client.py`:

```python
async def hello(self, requested: list[str]) -> list[str]:
    """Declare requested capabilities; returns the subset the server granted.
    Back-compat: pre-SP-12 servers return an empty list."""
    ...
```

`python/src/atd_client/errors.py`: add `class CapabilityDenied(AtdError): ...` with `required: list[str]`, `granted: list[str]`.

`python/src/atd_client/client.py::call()` maps error code `ERR_CAPABILITY_DENIED` → `CapabilityDenied`.

Pytest: one test each, in `python/tests/test_hello.py`, using the existing spawned-server fixture.

### Step 6.4: README update

- [ ]

Edit `README.md` "Architecture at a glance" ASCII diagram:

Before (condensed):
```
┌──────────────┐         ┌──────────────────┐
│  atd-client  │ ←─────→ │ ATD server       │
└──────────────┘         │ (atd-ref-server) │
                         └──────────────────┘
```

After:
```
┌──────────────┐         ┌───────────────────────────────────────────────┐
│  atd-client  │ ←─────→ │ ATD server (atd-ref-server)                    │
│              │         │   Hello → Capability gate                       │
│              │         │   Registry → Tier policy → Binding → Middleware │
└──────────────┘         └───────────────────────────────────────────────┘
```

Add under "Architecture at a glance":

> **Dispatch layer (SP-12).** The reference server demonstrates four
> canonical dispatch primitives: connection-scoped capability allow-list
> (`--grant-capability`), tier-aware deadlines (`--tier-override`),
> pluggable bindings (`NativeBinding` + `CliBinding`, with
> `ref:external.uname` as the CLI example), and a result-middleware chain
> (default: `redact_paths`). The v3 distributed-dispatch features
> (device affinity, UCAN tokens, session handoff) remain Phase 2+.

### Step 6.5: `docs/design.md` update

- [ ]

Edit §2.1 layering diagram + table — the "ATD Server / Dispatch Core: reference = ANOS daemon" line is out of date. Replace with:

> **ATD Server / Dispatch Core** — `atd-ref-server` ships canonical dispatch in atd-mvp itself (SP-12): capability allow-list, tier-aware deadlines, binding selection, result-middleware chain. The v3 distributed dispatch (device affinity, UCAN, session migrate/fork/handoff) is Phase 2+.

Edit §1.2 Non-Goals — `CapabilityDenied`: was "Phase 2 enforced"; now "SP-12 enforced via allow-list; Phase 2 enforced via UCAN".

Edit §3.6 Key Design Decisions table — `Capability token`: "SP-12 allow-list (trusted server-declared grants); Phase 2 cryptographic tokens."

### Step 6.6: MCP bridge note

- [ ]

Edit `crates/atd-mcp-bridge/README.md` — add one paragraph near the "Limitations" section (create if absent):

> **Capability-gated tools.** The bridge does not issue an ATD `Hello`
> handshake, so tools declaring `required_capabilities` will be refused
> when called through the bridge. All nine built-in reference tools
> declare no required capabilities and work unchanged. If you need to
> call a capability-gated tool, use the Rust or Python ATD client
> directly; bridge propagation is tracked as a future-SP item.

### Step 6.7: Protocol doc update

- [ ]

Edit `docs/protocol/wire-format.md` (shipped in SP-11) — add two sections:

- "§N Hello handshake" — message shape, when to send, back-compat with pre-SP-12 servers.
- "§N+1 Capability denial" — the `ERR_CAPABILITY_DENIED` error code, `details.required` and `details.granted` shape, how clients should surface.

Append to `docs/protocol/error-codes.md` — one entry for `CAPABILITY_DENIED` (value 1001, trigger, `is_retryable = false`, suggested fix).

### Step 6.8: Final verification + tag

- [ ]

```bash
cd /home/nan/proj/atd-mvp

# 1. Full test sweep — Rust + Python
cargo test --workspace --all-targets
cargo test --workspace --all-targets 2>&1 | grep 'test result:' | awk '{s+=$4} END{print "total:", s}'
# Expect ≥ 276 (252 existing + ~18 unit + 6 integration).

cd python && pytest
cd ..

# 2. Clippy sweep
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 3. hello_atd regression — the headline example still works
cargo run --example hello_atd -p atd-examples

# 4. Spawn ref-server with all SP-12 flags, sanity-check CLI parsing
./target/release/atd-ref-server \
    --grant-capability exec \
    --tier-override hot=timeout_ms=400 \
    --middleware redact_paths &
REFPID=$!
sleep 1
kill $REFPID

# 5. Tag
git add crates/atd-client crates/atd-mcp-bridge python README.md docs/
git commit -m "feat(sp12): client Hello parity, docs updates, cross-primitive E2E"
git tag sp12-canonical-dispatch
```

- [ ] Tag `sp12-canonical-dispatch` applied.

---

## Out-of-scope reminders (per spec §2.2 / §7)

Do **not**, within SP-12:
- Add UCAN / token signing / attenuation.
- Add device routing or any `device_*` field to the wire.
- Add `session.start` / `session.migrate` / `session.fork` / `session.handoff`.
- Add middlewares other than `redact_paths` (even if trivial).
- Refactor existing 9 tools to declare realistic `required_capabilities` or `tier` values.
- Propagate capabilities through `atd-mcp-bridge`.
- Introduce `AtdErrorCode` enum.
- Add HTTP or stdio transport.

Each of those is a future SP; keeping SP-12's diff narrow preserves review quality and lets the plan land in one sprint.

---

**Summary.** Six tasks. ~580 LoC of new Rust (four primitive modules, one tool). ~40 LoC of Python. Three doc files edited. Six integration tests + ~18 unit tests added. Existing 252 tests unaffected. Exit tag: `sp12-canonical-dispatch`.
