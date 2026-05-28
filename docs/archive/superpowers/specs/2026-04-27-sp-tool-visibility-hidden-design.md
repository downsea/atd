# SP-tool-visibility-hidden — `ToolVisibility::Hidden` variant + server-side discover filter

**Date:** 2026-04-27
**Status:** Approved — ready for implementation plan
**Parent:** Closes [atd-mvp#3](https://github.com/downsea/atd-mvp/issues/3). Replaces the `--expose-raw-tools` workaround introduced in healthkit_cli v1.2.0 (see [`docs/integrations/healthkit.md`](../../integrations/healthkit.md) §5).
**Anchor:** SP-8 §7.2 (gated-tool family pattern, mirrored here for the conformance fixture); atd-architecture.md §10 row "Extract socket listener … (healthkit_cli)" is the upstream signal that surfaced this need.

## 1. Context

`ToolVisibility` today has four variants (`Read`, `Write`, `Dangerous`, `System`) — all four are *visible* to discover. There is no protocol-level way for a server to say "this tool is callable but should not appear in the catalog."

healthkit_cli ran into this in v1.2.0: the 8 raw HMS schema tools confuse LLMs (see [`docs/integrations/healthkit.md`](../../integrations/healthkit.md) §2 — they brought success rate from 95% down to 24%). Their workaround was a server-side `--expose-raw-tools` flag controlling whether the raw tools get registered at all. This works for one adopter but is per-binary and per-operator: every future vendor needs the same flag.

A protocol-level `Hidden` variant solves this once. Server publishes raw tools as `Hidden`; agents browsing the catalog don't see them; humans/integration tests/advanced clients still reach them via `describe(id)` and `call(id)`.

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q1 | Can clients opt in to listing Hidden tools (e.g. `Request::ToolList { include_hidden: bool }`)? | **No (Option A).** Hidden is unconditional — invisible to discover, period. Keeps protocol additive-only (one new enum variant, no new request fields). Clients that need Hidden tools must know the id. If a real opt-in need surfaces later, it can be added without breaking changes. |
| Q2 | Where does the discover filter live — `atd-server` connection dispatch, `atd-runtime::Registry::summaries()`, or per-tool gate? | **`atd-server`'s `Request::ToolList` handler.** Registry stays uniform (it always knows about every tool — `describe`/`call` need that). The visibility filter is an export-format concern, not a registry concern. Keeps the registry pure. |
| Q3 | What about the SDK-side `DiscoverFilter::visibility = Hidden`? | Returns empty (the server already filtered). No special-casing in the SDK; the existing `out.retain(\|s\| s.visibility == v)` in `client.rs:190-192` does the right thing trivially because the server never sends Hidden summaries. |
| Q4 | atd-protocol semver bump? | **0.2.x → 0.3.0.** Adding an enum variant is technically additive on the wire (older clients see an unknown `"hidden"` string and `serde` rejects), but it breaks exhaustive `match` in downstream Rust consumers. Treat as minor-protocol bump. |
| Q5 | Conformance fixture shape? | Mirror the SP-8.1 / SP-8.2 pattern: add `ref:conformance.hidden_op` (Hidden visibility, no required capability, returns trivial success) registered behind the existing `--enable-conformance-tool` flag. One fixture proves: discover excludes it, schema-by-id returns it, run-by-id succeeds. |
| Q6 | healthkit_cli migration in this SP? | **Out of scope.** This SP lands in atd-mvp only. healthkit_cli v1.3.0 will drop `--expose-raw-tools` in a follow-up SP after this releases. |

## 3. Touch points

One commit. Nine code/doc locations.

| # | File | Change |
|---|---|---|
| 1 | `crates/atd-protocol/src/enums.rs` | Add `Hidden` variant to `ToolVisibility`. New unit test asserting snake_case `"hidden"` serde. |
| 2 | `crates/atd-server/src/connection.rs` | In `Request::ToolList` arm (line 70-75), filter out summaries where `visibility == ToolVisibility::Hidden` before serializing. |
| 3 | `crates/atd-ref-server-bin/src/conformance.rs` | Add `ConformanceHiddenTool` struct + `Tool` impl + 1 unit test (mirrors `ConformanceDeniedTool` shape). |
| 4 | `crates/atd-ref-server-bin/src/builtin.rs` | When `enable_conformance_tool == true`, register `ConformanceHiddenTool` alongside the existing two; rename `..._adds_two` test to `..._adds_three`. |
| 5 | `crates/atd-conformance/src/case.rs` | Add `expect_tools_exclude: Option<Vec<String>>` field (with `#[serde(default)]`) to `BehaviorCase`. |
| 6 | `crates/atd-conformance/src/runner.rs` (or `wire.rs` — wherever behavior cases assert) | After the existing `expect_response_matches` check in the behavior-case path, additionally assert that any tool ids listed in `expect_tools_exclude` are absent from the response's `tools` array. ~15 lines. |
| 7 | `crates/atd-conformance/fixtures/behavior/` | Three new fixtures (described in §6.3). |
| 8 | Workspace `Cargo.toml` files: `atd-protocol`, `atd-sdk`, `atd-runtime`, `atd-server`, `atd-conformance`, `atd-ref-server-bin` | Bump 0.2.x → 0.3.0 (path-deps cascade). |
| 9 | `docs/atd-architecture.md` | Add row to §10 status table (state: ✅ on land); add note to §3 (Layer 1 / Wire format) explaining Hidden semantics. Also update `crates/atd-conformance/README.md` documenting the new `expect_tools_exclude` primitive. |

**Not touched:**

- `atd-runtime` — `Registry::summaries()` keeps returning everything (tested behavior; the filter is at the protocol export boundary, not the registry)
- `atd-sdk` — existing `DiscoverFilter::visibility` semantics unchanged; no new client API
- `atd-cli` — `atd list` doesn't expose visibility filter today; no change needed
- `atd-mcp-bridge` — passes `Request::ToolList` through; nothing visibility-aware
- `atd-tools-*` — no built-in tool currently uses Hidden; opt-in only
- `crates/atd-conformance/tests/atd_mvp_self_conformance.rs` — `--enable-conformance-tool` already passed (SP-8.1)
- `crates/atd-protocol-schema` — schema regen runs in CI; the JsonSchema derive on `ToolVisibility` picks up the new variant automatically

## 4. `ToolVisibility::Hidden` variant

In `crates/atd-protocol/src/enums.rs`:

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

New unit test:

```rust
#[test]
fn visibility_hidden_serializes_snake_case() {
    assert_eq!(
        serde_json::to_string(&ToolVisibility::Hidden).unwrap(),
        "\"hidden\""
    );
}
```

## 5. Server-side filter (`atd-server`)

In `crates/atd-server/src/connection.rs::dispatch`, the `Request::ToolList` arm currently reads:

```rust
Request::ToolList => {
    let summaries = state.registry.summaries();
    Response::ToolListResponse {
        tools: serde_json::to_value(&summaries).unwrap_or_else(|_| serde_json::json!([])),
    }
}
```

Change to:

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

`Request::ToolSchema { tool_id }` (line 76-87) and `Request::RunTool { ... }` are unchanged — both look up by id, neither cares about visibility.

## 6. Conformance fixture (atd-ref-server-bin + atd-conformance)

### 6.1 The Hidden tool

Add to `crates/atd-ref-server-bin/src/conformance.rs` (sibling to `ConformanceDeniedTool` and `ConformanceSaturatedTool`):

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
            // ... (mirror denied_op / saturate_op for the rest)
            required_capabilities: vec![],
            // ...
        };
        Self { def }
    }
}

#[async_trait]
impl Tool for ConformanceHiddenTool {
    fn definition(&self) -> &ToolDefinition { &self.def }

    async fn call(&self, _ctx: &CallContext, _args: Value) -> Result<Value, ToolError> {
        Ok(serde_json::json!({"ok": true}))
    }
}
```

Existing unit test pattern in `conformance.rs` already covers per-tool def shape; add one for `hidden_op` matching the existing two.

### 6.2 builtin.rs registration

In `crates/atd-ref-server-bin/src/builtin.rs`, when `enable_conformance_tool == true`, register all three:

```rust
if enable_conformance_tool {
    registry.register(Arc::new(ConformanceDeniedTool::new()));
    registry.register(Arc::new(ConformanceSaturatedTool::new()));
    registry.register(Arc::new(ConformanceHiddenTool::new()));  // NEW
}
```

Test rename: `..._adds_two` → `..._adds_three`; assert all three tool ids present.

### 6.3 Conformance fixture

`crates/atd-conformance/fixtures/behavior/hidden_visibility_excludes_from_tool_list.json`:

```json
{
  "category": "behavior",
  "name": "hidden_visibility_excludes_from_tool_list",
  "description": "ref:conformance.hidden_op is registered with ToolVisibility::Hidden. The server must exclude it from Request::ToolList responses (and the SDK round-trips that filter). describe and run-by-id are exercised by separate fixtures (...by_id_describes, ...by_id_runs).",
  "setup": null,
  "send": { "type": "tool_list" },
  "expect_response_matches": {
    "type": "tool_list",
    "tools": "*"
  },
  "expect_tools_exclude": ["ref:conformance.hidden_op"]
}
```

**Note:** `expect_tools_exclude` is a new fixture-format primitive. The runner currently has `expect_response_matches` for shape matching. We need either:
- (a) Extend the JSON fixture format with `expect_tools_exclude: [...]` (an array of tool ids that must NOT appear in the `tools` field), OR
- (b) Add a code-level check in `runner.rs` for this specific fixture name (lazy escape hatch).

**Decision:** (a) — clean fixture extension. Keeps fixtures declarative; small runner addition (~15 lines including parsing + assertion).

A second fixture covers describe-by-id:

```json
{
  "category": "behavior",
  "name": "hidden_tool_describable_by_id",
  "description": "Hidden tools are still individually describe-able by id (only discover/list is filtered).",
  "setup": null,
  "send": { "type": "tool_schema", "tool_id": "ref:conformance.hidden_op" },
  "expect_response_matches": {
    "type": "tool_schema",
    "schema": "*"
  }
}
```

A third fixture for call-by-id:

```json
{
  "category": "behavior",
  "name": "hidden_tool_callable_by_id",
  "description": "Hidden tools are still individually call-able by id.",
  "setup": null,
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

Three fixtures total (count goes 32 → 35 in the conformance suite).

## 7. Versioning

| Crate | Before | After | Reason |
|---|---|---|---|
| `atd-protocol` | 0.2.1 | 0.3.0 | New enum variant breaks exhaustive `match` in downstream Rust consumers (semver minor for protocol crates per [SP-9 / SP-publish-v2 design](2026-04-25-sp-publish-v2-design.md)) |
| `atd-sdk` | 0.2.1 | 0.3.0 | Cascade — re-exports `ToolVisibility` |
| `atd-runtime` | 0.2.1 | 0.3.0 | Cascade — uses `ToolVisibility` in summaries |
| `atd-server` | 0.2.1 | 0.3.0 | Cascade + the new filter in dispatch |
| `atd-conformance` | 0.2.1 | 0.3.0 | Cascade + new fixture format primitive `expect_tools_exclude` |
| `atd-ref-server-bin` | 0.2.1 | 0.3.0 | Cascade + the new conformance tool |
| `atd-mcp-bridge`, `atd-cli`, `atd-tools-*` | unchanged | unchanged | No code touched |

This is a workspace-coordinated bump; the bins move with the protocol because their Cargo.toml path-deps require it. Nothing has been published to crates.io yet, so external semver impact is zero.

## 8. Validation

Exit gate (must all be ✅ before commit):

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features` — workspace tests pass (currently 352; +1 protocol unit, +1 ref-server unit → 354+ after this SP)
- [ ] `cargo build --release --workspace`
- [ ] Self-conformance test (`atd-conformance/tests/atd_mvp_self_conformance.rs`) green with 35 fixtures (was 32 — the 3 new fixtures from §6.3)
- [ ] `atd-protocol-schema` JSON regenerated; CI gate passes
- [ ] New fixture format primitive `expect_tools_exclude` documented in `crates/atd-conformance/README.md`

## 9. Out of scope (deferred)

- **healthkit_cli migration to drop `--expose-raw-tools`** — separate SP in healthkit_cli repo, after this lands.
- **Permission gate for listing Hidden tools** — the Q1 alternative (Option B). Deferred until a real adopter need surfaces; can be added additively if needed (`Request::ToolList { include_hidden: bool }` with default `false`).
- **CLI flag on `atd list`** to opt in to seeing Hidden tools — same reasoning as above; the underlying SDK doesn't support it, so the CLI doesn't need to.
- **Migration tool / linter** flagging tools that should be Hidden (e.g., raw schema endpoints in vendor servers) — adopter education, not a protocol concern.

## 10. Architecture.md §10 row

Add to the status table (right after the SP-listener-extract row):

```
| `ToolVisibility::Hidden` variant + server-side discover filter | Wire / Dispatch | ✅ | SP-tool-visibility-hidden | 2026-04-27 | Landed; protocol bump 0.2.x → 0.3.0; one new variant in atd-protocol::enums; filter at atd-server's Request::ToolList boundary; conformance covered via ref:conformance.hidden_op + 3 new fixtures. Replaces the per-binary `--expose-raw-tools` workaround that healthkit_cli v1.2.0 used; v1.3.0 of healthkit_cli will drop the flag and register raw tools as Hidden unconditionally. |
```

Also update §3 (Layer 1 / Wire format) with one paragraph noting that `Hidden` tools are excluded from `Request::ToolList` but reachable by id; cross-reference the conformance fixtures.
