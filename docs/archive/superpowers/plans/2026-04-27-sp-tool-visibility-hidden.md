# SP-tool-visibility-hidden Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `ToolVisibility::Hidden` variant to atd-protocol and filter Hidden tools out of `Request::ToolList` responses at the atd-server boundary, so vendors can publish tools (raw schema endpoints, debug helpers) that are still callable by id but stay out of the LLM-visible catalog.

**Architecture:** Single new enum variant in atd-protocol; one filter line in atd-server's `Request::ToolList` handler; a new `ref:conformance.hidden_op` tool in atd-ref-server-bin behind the existing `--enable-conformance-tool` flag; three conformance fixtures; one new declarative fixture-format primitive (`expect_tools_exclude`). One commit, workspace minor bump 0.2.x → 0.3.0.

**Tech Stack:** Rust 2021, Tokio, serde, atd-protocol / atd-sdk / atd-runtime / atd-server / atd-ref-server-bin / atd-conformance crates.

**Spec:** [`../specs/2026-04-27-sp-tool-visibility-hidden-design.md`](../specs/2026-04-27-sp-tool-visibility-hidden-design.md)

---

## Task 1: Add `Hidden` variant to `ToolVisibility`

**Files:**
- Modify: `crates/atd-protocol/src/enums.rs:1-101`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/atd-protocol/src/enums.rs`:

```rust
#[test]
fn visibility_hidden_serializes_snake_case() {
    assert_eq!(
        serde_json::to_string(&ToolVisibility::Hidden).unwrap(),
        "\"hidden\""
    );
}

#[test]
fn visibility_hidden_round_trips() {
    let json = "\"hidden\"";
    let parsed: ToolVisibility = serde_json::from_str(json).unwrap();
    assert_eq!(parsed, ToolVisibility::Hidden);
}
```

- [ ] **Step 2: Run tests — they should fail (variant doesn't exist)**

```bash
cargo test -p atd-protocol visibility_hidden 2>&1 | tail -20
```

Expected: compile error `no variant or associated item named 'Hidden' found for enum 'ToolVisibility'`.

- [ ] **Step 3: Add the `Hidden` variant**

In `crates/atd-protocol/src/enums.rs`, modify the `ToolVisibility` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ToolVisibility {
    #[default]
    #[serde(alias = "Read")]
    Read,
    #[serde(alias = "Write")]
    Write,
    #[serde(alias = "Dangerous")]
    Dangerous,
    #[serde(alias = "System")]
    System,
    /// Hidden from `Request::ToolList` discover responses, but still
    /// reachable by id via `Request::ToolSchema` (describe) and
    /// `Request::RunTool` (call). Use for tools that exist for
    /// integration tests, debugging, or advanced humans, but would add
    /// noise to an LLM's catalog.
    #[serde(alias = "Hidden")]
    Hidden,
}
```

- [ ] **Step 4: Run tests — should pass**

```bash
cargo test -p atd-protocol 2>&1 | tail -10
```

Expected: PASS for both new tests + all pre-existing tests in the crate.

- [ ] **Step 5: Verify workspace still compiles (downstream consumers may have non-exhaustive matches)**

```bash
cargo build --workspace --all-features 2>&1 | tail -20
```

Expected: build succeeds. If any downstream `match` becomes non-exhaustive, fix by adding `ToolVisibility::Hidden => …` arms (treat Hidden the same as the most permissive existing variant unless context says otherwise — for filtering / classification, it should usually be its own arm).

- [ ] **Step 6: Commit**

```bash
git add crates/atd-protocol/src/enums.rs
git commit -m "feat(atd-protocol): add ToolVisibility::Hidden variant"
```

---

## Task 2: Server-side filter in `Request::ToolList`

**Files:**
- Modify: `crates/atd-server/src/connection.rs:70-75`
- Test: `crates/atd-server/src/lib.rs` (or a new `tests/` integration file — pick the path that matches existing tests)

- [ ] **Step 1: Locate the existing test pattern for server dispatch**

Run:

```bash
grep -rn "Request::ToolList\|tool_list" crates/atd-server/ | head -10
```

Expected: shows `connection.rs::dispatch` and any existing integration tests. If integration tests exist for ToolList, add to the same file. If not, write the test inline as a `#[tokio::test]` in `connection.rs::tests` mod (mirror existing test mod placement).

- [ ] **Step 2: Write the failing test**

The test must spin up a `Server` with two tools registered — one `Visible` (e.g., `Read`), one `Hidden` — connect via `atd-sdk`, and assert `discover()` returns only the Visible tool. If `atd-runtime::Registry` is testable directly (no need for full Server bring-up), keep it simple. Otherwise mirror the integration-test shape in `crates/atd-runtime/tests/` or `crates/atd-server/tests/`.

A minimal test (adapt path/imports to actual location):

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_list_excludes_hidden_visibility() {
    use atd_protocol::{ToolDefinition, ToolVisibility, Capability};
    use atd_runtime::registry::Registry;
    // ... build a Registry with one Read tool and one Hidden tool
    // ... bring up a Server bound to a temp socket
    // ... connect via atd_sdk::AtdClient
    // ... assert discover().len() == 1 and the one returned has visibility = Read
}
```

If wiring a full Server in-test is heavy, prefer a unit test on the `dispatch` function directly in `connection.rs` (extract the filter into a tiny pure function or test the dispatch result by constructing a `Registry` + calling `dispatch` synchronously).

- [ ] **Step 3: Run the failing test**

```bash
cargo test -p atd-server tool_list_excludes_hidden 2>&1 | tail -20
```

Expected: FAIL — server returns both tools.

- [ ] **Step 4: Add the filter**

In `crates/atd-server/src/connection.rs`, change the `Request::ToolList` arm from:

```rust
Request::ToolList => {
    let summaries = state.registry.summaries();
    Response::ToolListResponse {
        tools: serde_json::to_value(&summaries).unwrap_or_else(|_| serde_json::json!([])),
    }
}
```

to:

```rust
Request::ToolList => {
    let summaries: Vec<_> = state
        .registry
        .summaries()
        .into_iter()
        .filter(|s| !matches!(s.visibility, atd_protocol::ToolVisibility::Hidden))
        .collect();
    Response::ToolListResponse {
        tools: serde_json::to_value(&summaries).unwrap_or_else(|_| serde_json::json!([])),
    }
}
```

(Add `use atd_protocol::ToolVisibility;` near the top of the file if not already imported, and reference it without the `atd_protocol::` prefix.)

- [ ] **Step 5: Run the test — should pass**

```bash
cargo test -p atd-server 2>&1 | tail -10
```

Expected: PASS for the new test + all pre-existing atd-server tests.

- [ ] **Step 6: Commit**

```bash
git add crates/atd-server/
git commit -m "feat(atd-server): filter Hidden tools out of Request::ToolList"
```

---

## Task 3: `ConformanceHiddenTool` in atd-ref-server-bin

**Files:**
- Modify: `crates/atd-ref-server-bin/src/conformance.rs` (append)
- Modify: `crates/atd-ref-server-bin/src/builtin.rs`
- Read for reference: existing `ConformanceDeniedTool` and `ConformanceSaturatedTool` in same `conformance.rs`

- [ ] **Step 1: Read the existing two ConformanceTools to copy the shape**

```bash
sed -n '1,200p' crates/atd-ref-server-bin/src/conformance.rs
```

Note: `ConformanceDeniedTool` (capability-denied, code 1001) and `ConformanceSaturatedTool` (rate-limited, code 1002) both follow the same shape: a struct holding a `ToolDefinition`, a `new()` constructor, and an `#[async_trait] impl Tool`.

- [ ] **Step 2: Write the failing unit test**

In `crates/atd-ref-server-bin/src/conformance.rs::tests` (matching the existing test mod pattern):

```rust
#[test]
fn hidden_op_definition_has_hidden_visibility() {
    let tool = ConformanceHiddenTool::new();
    assert_eq!(tool.definition().id, "ref:conformance.hidden_op");
    assert_eq!(tool.definition().visibility, atd_protocol::ToolVisibility::Hidden);
    assert!(tool.definition().required_capabilities.is_empty());
}
```

- [ ] **Step 3: Run — should fail (struct doesn't exist)**

```bash
cargo test -p atd-ref-server-bin hidden_op 2>&1 | tail -10
```

Expected: compile error.

- [ ] **Step 4: Implement `ConformanceHiddenTool`**

Append to `crates/atd-ref-server-bin/src/conformance.rs` (use the `ConformanceDeniedTool` and `ConformanceSaturatedTool` already present as templates — match their fields exactly, only diverging where noted below):

```rust
/// A trivial tool registered with `ToolVisibility::Hidden`. Behaves
/// identically to ref:echo.say but proves the visibility filter at the
/// `Request::ToolList` boundary: discover should not surface it; describe
/// and call by id must work.
pub struct ConformanceHiddenTool {
    def: ToolDefinition,
}

impl ConformanceHiddenTool {
    pub fn new() -> Self {
        let def = ToolDefinition {
            id: "ref:conformance.hidden_op".to_string(),
            name: "hidden_op".to_string(),
            description: "Conformance test tool registered as Hidden — invisible to discover, callable by id.".to_string(),
            domain: "conformance".to_string(),
            visibility: ToolVisibility::Hidden,
            tier: Some(ToolTier::Hot),
            required_capabilities: vec![],
            // (Match the field set on ConformanceDeniedTool / ConformanceSaturatedTool
            // verbatim for any remaining fields — capability, input_schema, bindings,
            // publisher, trust, max_concurrent, etc.)
        };
        Self { def }
    }
}

#[async_trait]
impl Tool for ConformanceHiddenTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    async fn call(&self, _ctx: &CallContext, _args: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        Ok(serde_json::json!({"ok": true}))
    }
}
```

(Imports near the top of `conformance.rs` already cover `ToolDefinition`, `ToolVisibility`, `ToolTier`, `Tool`, `CallContext`, `ToolError`, `async_trait` — add only what's missing after the existing two structs.)

- [ ] **Step 5: Wire it into `builtin.rs`**

Read the current registration block:

```bash
grep -B2 -A10 "ConformanceDeniedTool\|ConformanceSaturatedTool" crates/atd-ref-server-bin/src/builtin.rs
```

In `crates/atd-ref-server-bin/src/builtin.rs`, find the `if enable_conformance_tool { ... }` block (registers the two existing conformance tools). Add the third:

```rust
if enable_conformance_tool {
    registry.register(Arc::new(ConformanceDeniedTool::new()));
    registry.register(Arc::new(ConformanceSaturatedTool::new()));
    registry.register(Arc::new(ConformanceHiddenTool::new()));  // NEW
}
```

Find the existing `..._adds_two` test (or whatever it's called now — last bumped in SP-8.2). Update the count assertion from 2 to 3 and add `ref:conformance.hidden_op` to the asserted-id set. Rename the test from `..._adds_two` to `..._adds_three`.

- [ ] **Step 6: Run all atd-ref-server-bin tests**

```bash
cargo test -p atd-ref-server-bin 2>&1 | tail -10
```

Expected: PASS for the new unit test + the renamed registration test + all pre-existing tests.

- [ ] **Step 7: Commit**

```bash
git add crates/atd-ref-server-bin/
git commit -m "feat(atd-ref-server-bin): add ref:conformance.hidden_op tool"
```

---

## Task 4: Extend `BehaviorCase` with `expect_tools_exclude`

**Files:**
- Modify: `crates/atd-conformance/src/case.rs` (BehaviorCase struct)
- Modify: `crates/atd-conformance/src/runner.rs` or `crates/atd-conformance/src/wire.rs` (wherever behavior cases are asserted — locate first)

- [ ] **Step 1: Find the behavior-case execution path**

```bash
grep -rn "BehaviorCase\|expect_response_matches" crates/atd-conformance/src/ | head -20
```

Note which file contains the assertion on `expect_response_matches`. That's where the new `expect_tools_exclude` check will land.

- [ ] **Step 2: Write the failing test**

In `crates/atd-conformance/src/case.rs::tests` (or wherever fixture parsing is unit-tested), add:

```rust
#[test]
fn behavior_case_parses_expect_tools_exclude() {
    let json = r#"{
        "category": "behavior",
        "name": "test_exclude",
        "description": "test",
        "send": { "type": "tool_list" },
        "expect_response_matches": { "type": "tool_list", "tools": "*" },
        "expect_tools_exclude": ["ref:conformance.hidden_op"]
    }"#;
    let case: BehaviorCase = serde_json::from_str(json).unwrap();
    assert_eq!(
        case.expect_tools_exclude.as_deref(),
        Some(&["ref:conformance.hidden_op".to_string()][..])
    );
}

#[test]
fn behavior_case_expect_tools_exclude_defaults_to_none() {
    let json = r#"{
        "category": "behavior",
        "name": "test_no_exclude",
        "description": "test",
        "send": { "type": "tool_list" },
        "expect_response_matches": { "type": "tool_list", "tools": "*" }
    }"#;
    let case: BehaviorCase = serde_json::from_str(json).unwrap();
    assert!(case.expect_tools_exclude.is_none());
}
```

- [ ] **Step 3: Run — should fail (field doesn't exist)**

```bash
cargo test -p atd-conformance behavior_case_parses_expect_tools_exclude 2>&1 | tail -10
```

- [ ] **Step 4: Add the field**

In `crates/atd-conformance/src/case.rs`, modify `BehaviorCase`:

```rust
pub struct BehaviorCase {
    pub name: String,
    pub description: String,
    #[serde(default = "default_must_pass")]
    pub must: Must,
    #[serde(default)]
    pub setup: Option<SetupStep>,
    pub send: serde_json::Value,
    pub expect_response_matches: serde_json::Value,
    /// Tool ids that MUST NOT appear in the response's `tools` field
    /// (only meaningful for `tool_list` responses). Empty-by-default
    /// — fixtures without this field skip the assertion.
    #[serde(default)]
    pub expect_tools_exclude: Option<Vec<String>>,
}
```

- [ ] **Step 5: Run the parse tests — should pass**

```bash
cargo test -p atd-conformance behavior_case 2>&1 | tail -10
```

- [ ] **Step 6: Add the runtime assertion**

In the file located in Step 1 (the one running behavior cases), after the existing `expect_response_matches` check succeeds, add:

```rust
if let Some(excluded_ids) = &case.expect_tools_exclude {
    let tools_array = response_value
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or_else(|| "expect_tools_exclude requires a `tools` array in the response".to_string())?;
    let present_ids: Vec<&str> = tools_array
        .iter()
        .filter_map(|t| t.get("id").and_then(|i| i.as_str()))
        .collect();
    for excluded in excluded_ids {
        if present_ids.iter().any(|id| id == excluded) {
            return Err(format!(
                "fixture violation: tool id '{excluded}' was expected to be EXCLUDED from tool_list, but appeared"
            ));
        }
    }
}
```

(Adapt error-handling to whatever `Result` shape the surrounding function uses — check the existing `expect_response_matches` assertion for the pattern.)

- [ ] **Step 7: Run all atd-conformance tests**

```bash
cargo test -p atd-conformance 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/atd-conformance/src/
git commit -m "feat(atd-conformance): add expect_tools_exclude fixture primitive"
```

---

## Task 5: Three new conformance fixtures

**Files:**
- Create: `crates/atd-conformance/fixtures/behavior/hidden_visibility_excludes_from_tool_list.json`
- Create: `crates/atd-conformance/fixtures/behavior/hidden_tool_describable_by_id.json`
- Create: `crates/atd-conformance/fixtures/behavior/hidden_tool_callable_by_id.json`

- [ ] **Step 1: Look at an existing setup-using fixture for shape reference**

```bash
cat crates/atd-conformance/fixtures/behavior/capability_granted_allows_call.json
cat crates/atd-conformance/fixtures/behavior/rate_limited_returns_code_1002.json
```

Most behavior fixtures have a Hello-handshake `setup` step. The new fixtures don't need authentication since `ref:conformance.hidden_op` has `required_capabilities: []`, but should still do a Hello handshake to be a well-formed session — match the pattern from `capability_granted_allows_call.json`.

- [ ] **Step 2: Create the three fixtures**

`hidden_visibility_excludes_from_tool_list.json`:

```json
{
  "category": "behavior",
  "name": "hidden_visibility_excludes_from_tool_list",
  "description": "ref:conformance.hidden_op is registered with ToolVisibility::Hidden. The server must exclude it from Request::ToolList responses. describe and run-by-id are exercised by separate fixtures.",
  "setup": {
    "kind": "hello",
    "requested_capabilities": []
  },
  "send": { "type": "tool_list" },
  "expect_response_matches": {
    "type": "tool_list",
    "tools": "*"
  },
  "expect_tools_exclude": ["ref:conformance.hidden_op"]
}
```

`hidden_tool_describable_by_id.json`:

```json
{
  "category": "behavior",
  "name": "hidden_tool_describable_by_id",
  "description": "Hidden tools are still individually describe-able by id (only discover/list is filtered).",
  "setup": {
    "kind": "hello",
    "requested_capabilities": []
  },
  "send": { "type": "tool_schema", "tool_id": "ref:conformance.hidden_op" },
  "expect_response_matches": {
    "type": "tool_schema",
    "schema": "*"
  }
}
```

`hidden_tool_callable_by_id.json`:

```json
{
  "category": "behavior",
  "name": "hidden_tool_callable_by_id",
  "description": "Hidden tools are still individually call-able by id.",
  "setup": {
    "kind": "hello",
    "requested_capabilities": []
  },
  "send": {
    "type": "run_tool",
    "tool_id": "ref:conformance.hidden_op",
    "args": {},
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "run_tool_result",
    "result": "*"
  }
}
```

(Verify the exact `setup.kind` value and `requested_capabilities` field name against `case.rs::SetupStep::Hello { ... }` — adjust if different.)

- [ ] **Step 3: Run the self-conformance integration test**

```bash
cargo test -p atd-conformance --test atd_mvp_self_conformance 2>&1 | tail -20
```

Expected: PASS. The test should now run 35 fixtures (was 32). If it reports a different count, double-check that the fixture loader picks up newly added files automatically (it should — the loader globs the directory).

- [ ] **Step 4: Commit**

```bash
git add crates/atd-conformance/fixtures/
git commit -m "test(atd-conformance): 3 fixtures for ToolVisibility::Hidden semantics"
```

---

## Task 6: Workspace version bumps + docs

**Files:**
- Modify: `crates/atd-protocol/Cargo.toml`
- Modify: `crates/atd-sdk/Cargo.toml`
- Modify: `crates/atd-runtime/Cargo.toml`
- Modify: `crates/atd-server/Cargo.toml`
- Modify: `crates/atd-conformance/Cargo.toml`
- Modify: `crates/atd-ref-server-bin/Cargo.toml`
- Modify: `docs/atd-architecture.md` (§3 Wire format paragraph + §10 status row)
- Modify: `crates/atd-conformance/README.md` (document `expect_tools_exclude`)

- [ ] **Step 1: Inspect current versions**

```bash
grep -A1 'name = "atd-protocol"\|name = "atd-sdk"\|name = "atd-runtime"\|name = "atd-server"\|name = "atd-conformance"\|name = "atd-ref-server-bin"' crates/*/Cargo.toml | grep version
```

Expected: each shows `version = "0.2.x"`. (Workspace may use a `[workspace.package]` section — if so, bump there once instead of per-crate.)

- [ ] **Step 2: Check for workspace-level version**

```bash
grep -A2 '\[workspace\.package\]\|version' Cargo.toml | head -10
```

If `[workspace.package]` defines `version = "0.2.x"` and the per-crate `Cargo.toml`s use `version.workspace = true`, the bump is one line at the workspace root. Otherwise, bump each crate.

- [ ] **Step 3: Bump 0.2.x → 0.3.0**

Edit the workspace root `Cargo.toml` (or each per-crate `Cargo.toml`) to set version to `"0.3.0"`. Crates listed in §3 of the spec only — `atd-mcp-bridge`, `atd-cli`, `atd-tools-*` stay on their current versions (no code touched).

- [ ] **Step 4: Update `docs/atd-architecture.md`**

In §10 status table, add a row right after the SP-listener-extract row:

```
| `ToolVisibility::Hidden` variant + server-side discover filter | Wire / Dispatch | ✅ | SP-tool-visibility-hidden | 2026-04-27 | Landed; protocol bump 0.2.x → 0.3.0; one new variant in atd-protocol::enums; filter at atd-server's Request::ToolList boundary; conformance covered via ref:conformance.hidden_op + 3 new fixtures. Replaces the per-binary `--expose-raw-tools` workaround that healthkit_cli v1.2.0 used; v1.3.0 of healthkit_cli will drop the flag and register raw tools as Hidden unconditionally. |
```

In §3 (Layer 1 / Wire format), add one paragraph:

> **Hidden tools.** A tool with `ToolVisibility::Hidden` is excluded from `Request::ToolList` responses but remains reachable via `Request::ToolSchema { tool_id }` and `Request::RunTool { tool_id, ... }`. Use this for vendor-side raw schema endpoints, integration-test tools, or debug helpers that would clutter an LLM's catalog. Hiding is server-enforced; the SDK's `DiscoverFilter::visibility = Hidden` returns empty because the server never emits Hidden summaries. See SP-tool-visibility-hidden.

- [ ] **Step 5: Update `crates/atd-conformance/README.md`**

Add a section (or extend an existing fixture-format reference) documenting the `expect_tools_exclude` primitive:

```markdown
### `expect_tools_exclude` (behavior cases only)

Optional array of tool ids that MUST NOT appear in the response's `tools` field.
Useful for asserting visibility filters (e.g., that Hidden tools are excluded
from `tool_list` responses).

```json
"expect_tools_exclude": ["ref:conformance.hidden_op"]
```

If the response is not a `tool_list` (no `tools` array), the assertion fails.
```

- [ ] **Step 6: Run the full validation suite**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```

All four must pass. Expected workspace test count: ~354+ (was 352; +2 new units in protocol, +2-3 in conformance/case, +1 in ref-server, plus the integration test now picks up 3 more fixtures).

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/*/Cargo.toml docs/atd-architecture.md crates/atd-conformance/README.md Cargo.lock
git commit -m "$(cat <<'EOF'
chore(workspace): bump 0.2.x → 0.3.0 + docs for SP-tool-visibility-hidden

- Workspace minor bump for new ToolVisibility::Hidden variant (breaks
  exhaustive match in downstream consumers; nothing published yet).
- atd-architecture.md §10 status row + §3 wire-format paragraph.
- atd-conformance README documents the new expect_tools_exclude primitive.
EOF
)"
```

- [ ] **Step 8: Tag the SP**

```bash
git tag sp-tool-visibility-hidden
```

---

## Task 7: Final cross-check

- [ ] **Step 1: Confirm spec exit gates**

Re-read `docs/superpowers/specs/2026-04-27-sp-tool-visibility-hidden-design.md` §8 and tick each box. If anything is amber/red, fix before pushing.

- [ ] **Step 2: Confirm issue body acceptance criteria**

Re-read [atd-mvp#3](https://github.com/downsea/atd-mvp/issues/3). Each acceptance bullet must map to landed code:

- New variant + serde behavior in atd-protocol → Task 1
- Filter applied in atd-server's ToolList handler → Task 2
- 1 conformance fixture proving the contract → Task 5 (delivered 3, exceeds requirement)
- atd-protocol-schema regenerated → CI gate; verify schema regen on next push runs cleanly
- All workspace tests pass → Task 6 Step 6
- One SP committed at `docs/superpowers/specs/...` → Already committed in pre-flight

- [ ] **Step 3: Push + close issue**

```bash
git push origin master --tags
gh issue close 3 --comment "Fixed in $(git rev-parse --short HEAD) (tag sp-tool-visibility-hidden). See spec at docs/superpowers/specs/2026-04-27-sp-tool-visibility-hidden-design.md."
```

---

## Self-Review

Spec coverage:
- §3 (Touch points 1) → Task 1 ✓
- §3 (Touch points 2) → Task 2 ✓
- §3 (Touch points 3-4) → Task 3 ✓
- §3 (Touch points 5-6) → Task 4 ✓
- §3 (Touch points 7) → Task 5 ✓
- §3 (Touch points 8) → Task 6 Step 3 ✓
- §3 (Touch points 9) → Task 6 Steps 4-5 ✓
- §8 (Validation) → Task 6 Step 6 + Task 7 Step 1 ✓
- §9 (Out of scope) — explicitly NOT touched: healthkit_cli migration ✓

Type consistency:
- `ToolVisibility::Hidden` referenced consistently (snake_case `"hidden"` on the wire, PascalCase variant in Rust)
- `ConformanceHiddenTool` mirrors existing `ConformanceDeniedTool` / `ConformanceSaturatedTool`
- `expect_tools_exclude` field name consistent across struct, fixture, runner, README

No placeholders.
