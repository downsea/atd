# SP-observability-completeness-v1 — implementation plan

Spec: [`docs/superpowers/specs/2026-05-29-sp-observability-completeness-v1-design.md`](../specs/2026-05-29-sp-observability-completeness-v1-design.md)

**Discipline:** TDD red→green→refactor per task. Workspace test discipline (CLAUDE.md): iterate with `cargo test -p <crate> --lib <module>`, fire `cargo nextest run --workspace` exactly once as the pre-commit gate. One in-flight cargo at a time.

**Ordering rationale:** Phase 0 (doc) first — don't write code against stale constitutional sources. Then Axis D (smallest, pure docstring, warms the schema-regen path). Then Axis A (the security bug — highest value). Then C (provenance), B (backpressure). Conformance + workspace gate last.

---

## Phase 0 — Documentation alignment (no code) · Task 0.1–0.4

Lands first. No tests (doc-only); verification is "grep the old claim is gone".

### Task 0.1 — architecture §9.4 → per-crate SemVer
- File `docs/atd-architecture.md` §9.4 (`Workspace versioning`, ~line 879).
- Replace "All publishable crates share `workspace.package.version` ... workspace-lockstep through the 1.x line" with the ADR-0004 reality: per-crate independent SemVer, `atd-protocol` version = ATD release identity, sibling pins record minimum required version. Link `docs/adr/0004-per-crate-versioning.md`.
- Verify: `rg "workspace-lockstep" docs/atd-architecture.md` → 0 hits (or only in a historical-note context).

### Task 0.2 — architecture §10.8 new non-goal (cross-vendor capability federation)
- File `docs/atd-architecture.md` §10, add §10.8 after §10.7.
- Content: a single UCAN audience pin / caller_id / TokenBroker / audit log is per-server. An agent connecting to two ATD servers gets two independent capability contexts; one UCAN does not validate across both audiences. Cross-vendor *composition* (merged catalog) ships; cross-vendor *multi-agent协作 with shared delegation* is adopter-built (federation registry is out of scope). Cites the keystone scenario boundary.
- Verify: `rg "Cross-vendor capability" docs/atd-architecture.md` → 1 hit.

### Task 0.3 — architecture §5.6 cursor key-rotation note
- File `docs/atd-architecture.md` §5.6 (after the existing CursorIssuer bullets, ~line 556).
- Add: "Operational note — the HMAC signing key is per-process (random or `ATD_CURSOR_SIGNING_KEY`). Server restart invalidates all outstanding cursors (→ 1020 `ERR_CURSOR_EXPIRED`); clients re-issue the original `RunTool`. Cross-restart cursor continuity (key persistence / rotation with `kid`) is adopter-side in v1; a federation adopter feeling the re-fetch cost would trigger an SP."
- Verify: `rg "key.*rotation|key persistence" docs/atd-architecture.md` → ≥1 hit.

### Task 0.4 — MCP-lossy boundary (positioning §5.2 + bridge README)
- File `docs/atd-positioning.md` §5.2 (`vs raw MCP`, ~line 212): add a "What the MCP bridge drops" note + table after the existing list.
- File `crates/atd-mcp-bridge/README.md` §Limitations: expand the existing "Capability-gated tools" note into a full lossy-mapping table — tier / safety.level / required_capabilities / output_schema / dry_run / cursor (truncate unless `ATD_MCP_PASSTHROUGH_CURSOR=1`) / caller_id (multi-tenant). Recommend native SDK (Rust/Python) for full feature set.
- Housekeeping (same task): `docs/intro/atd-tech-deck.zh.md` §1.6 + `CLAUDE.md` "Current project state" — replace "workspace-lockstep" phrasing with per-crate SemVer (ADR 0004).
- Verify: `rg "lossy|MCP clients lose|drops" crates/atd-mcp-bridge/README.md` → ≥1 hit.

**Phase 0 commit:** `docs(align): SP-observability-completeness-v1 Phase 0 — ADR-0004 / MCP-lossy / cross-vendor / cursor-key gaps`

---

## Axis D — Schema advisory docstrings · Task D.1

Smallest, warms the schema-regen path before the bigger axes.

### Task D.1 (red→green)
- **Red:** add conformance `crates/atd-conformance/tests/schema_advisory_docs.rs` — load `/atd-protocol-schema.json`, assert `rate_limit_per_min` description contains `"Advisory only"` and `trust_level` description contains `"self-declared"`. Run → **fails** (descriptions absent).
- **Green:**
  - `crates/atd-protocol/src/tool.rs` — add the two doc-comments per spec §4.4 (`ToolResources::rate_limit_per_min`, `ToolTrust::trust_level`).
  - Regen schema: `cargo run -p atd-protocol --features schema --bin gen-schema` (or the repo's documented regen path) → updates `/atd-protocol-schema.json`.
  - Run conformance → **passes**.
- **Verify:** `cargo test -p atd-conformance --test schema_advisory_docs` green; `git diff atd-protocol-schema.json` shows only the two description additions.

---

## Axis A — Error-path egress middleware · Task A.1–A.3 (the security fix)

### Task A.1 — `Middleware::on_error` trait method (red→green)
- **Red:** `crates/atd-runtime/src/middleware.rs` tests — add a test middleware that uppercases the error `message`; assert a (not-yet-existing) `on_error` mutates it. Won't compile → red.
- **Green:** add `on_error(&self, tool_id, tool_def, message: &mut String, details: &mut Option<Value>)` default no-op to the `Middleware` trait (spec §4.1). Update `middleware.rs:9-10` module doc ("Error paths bypass middleware" → "error paths run `on_error` since SP-observability-completeness-v1"). `RedactPathsMiddleware` overrides `on_error` to run its `walk_strings` scrub on the message (wrap as transient `Value::String`) + details.
- **Verify:** `cargo test -p atd-runtime --lib middleware` green.

### Task A.2 — dispatch wires error paths (red→green)
- **Red:** `crates/atd-runtime/src/dispatch.rs` tests (or a new dispatch test module) — register a tool that returns `ExecutionFailed { message: "leak SECRET" }` and one returning `InvalidArgs("leak SECRET")`, behind a `RedactPathsMiddleware` matching `SECRET`. Assert the wire reply has `SECRET` redacted. Run → fails (no middleware on error paths).
- **Green:** 4 sites:
  - `dispatch.rs:792-815` (`run_tool` ExecutionFailed) — before building `ToolResultResponse`, run `for mw in &state.middleware { mw.on_result(&tool_id, entry.definition(), &mut <the result Value>) }` (A1: it's a result envelope).
  - `dispatch.rs:490-504` (paginated ExecutionFailed) — same A1 loop.
  - `dispatch.rs:777-791` (`run_tool` InvalidArgs) — build `message` (+ `details: None`) as `mut`, run `for mw { mw.on_error(&tool_id, entry.definition(), &mut message, &mut details) }`, then `Response::Error`.
  - `dispatch.rs:817-830` (`run_tool` InternalError) + `:505-510` (paginated `Err(e)` catch-all, per spec §10 Q2 — run `on_error` for defence in depth) — same A2 loop.
- **Verify:** `cargo test -p atd-runtime --lib dispatch` green. Confirm capability-denied / tool-not-found early-returns are NOT touched (no `entry` in scope; framework messages PHI-free).

### Task A.3 — PHI/FHIR middleware override `on_error` (red→green)
- **Red:** `crates/atd-middleware-pii-redact-medical/` test — a `ToolFailure`-shaped message `"failed for Patient John Doe SSN 123-45-6789"`; assert `on_error` redacts name + SSN. Run → fails (no override; default no-op).
- **Green:** `PiiRedactMiddleware::on_error` — wrap `message` as `Value::String`, run the existing `redact_value` core (SP-medical-middleware §4.7), unwrap; run `redact_value` on `details` if `Some`. `FhirMiddleware::on_error` — error replies aren't FHIR resources, so its override is a no-op *or* a light annotation; default no-op is acceptable (document why: FHIR validation is about resource shape, not error text). Lean: PII overrides (load-bearing), FHIR doesn't.
- **Verify:** `cargo test -p atd-middleware-pii-redact-medical` green.

**Axis A commit:** `feat(atd-runtime): error-path egress middleware (on_error hook) — SP-observability-completeness-v1 Axis A`

---

## Axis C — Capability provenance · Task C.1–C.2

### Task C.1 — `CallEvent.capability_provenance` + `ProvSource` (red→green)
- **Red:** `crates/atd-runtime/src/audit.rs` tests — construct a `CallEvent` with `capability_provenance: Some(vec![StringAllowList cap, UcanChain cap])`, serialize, assert both `kind: "string_allow_list"` and `kind: "ucan_chain"` appear + `schema_version == 3`. Also a back-compat test: a v2 JSON (no field) deserializes with `capability_provenance: None`. Run → fails (field/types absent).
- **Green:** `audit.rs` — add `CapProvenance` + `ProvSource` types (spec §4.3), the optional field on `CallEvent`, bump `SCHEMA_VERSION` 2→3, extend the version doc-comment (v3 note). Fix the two in-tree `CallEvent` literals (dispatch.rs:526 + :588) to set `capability_provenance` (compute from caps, or `None` if not yet wired — Task C.2 fills it).
- **Verify:** `cargo test -p atd-runtime --lib audit` green; back-compat test green.

### Task C.2 — dispatch records provenance (red→green)
- **Red:** dispatch test — a `Hello` with `requested_capabilities: ["records:read"]` (string) + a `ucan_tokens` chain granting `["records:write"]`; assert the emitted `CallEvent.capability_provenance` maps `records:read → StringAllowList` and `records:write → UcanChain { issuer_did, chain_depth }`. Run → fails (`None` today).
- **Green:** at the Hello capability-union site (architecture §5.2 — find via `rg "granted_ucan|requested_capabilities|intersect" crates/atd-runtime/src`), record each cap's source into the connection's negotiated state; thread it to the `CallEvent` construction in `run_tool`'s `emit` closure (dispatch.rs:582-603) + the continuation emit (`:526`).
- **Verify:** `cargo test -p atd-runtime --lib dispatch` green; the UCAN integration tests (`ucan_*`) stay green.

**Axis C commit:** `feat(atd-protocol,atd-runtime): capability provenance in CallEvent (schema v3) — Axis C`

---

## Axis B — Audit backpressure strategy · Task B.1–B.2

### Task B.1 — `BackpressureStrategy` + `AuditSink::backpressure_strategy` (red→green)
- **Red:** `audit.rs` tests — assert `AuditSink::backpressure_strategy()` defaults to `Drop` for a bare sink; a `JsonLinesAuditSink::with_strategy(.., Block)` reports `Block`. Run → fails (type/method absent).
- **Green:** `audit.rs` — add `BackpressureStrategy` enum (spec §4.2), `backpressure_strategy()` default `Drop` on the trait, `JsonLinesAuditSink::with_strategy(writer, capacity, strategy)` storing the strategy.
- **Verify:** `cargo test -p atd-runtime --lib audit` green.

### Task B.2 — `on_call` honours strategy (red→green; resolves spec §10 Q1)
- **Spike first:** measure `try_send` spin-with-yield vs strategy-selected `std::sync::mpsc` blocking under a 200-event burst with a slow writer. Pick the lower-latency mechanism for `Block`.
- **Red:** `audit.rs` test — `with_strategy(slow_writer, capacity=4, Block)`, 200-event burst, assert `drops() == 0` and all 200 eventually drain. Contrast existing `drops_counter_increments_when_channel_full` (Drop) stays as-is. Run → fails (Block not honoured).
- **Green:** `JsonLinesAuditSink::on_call` branches on `self.strategy`: `Drop` = current `try_send`+counter; `Block` = chosen blocking mechanism; `FallbackSink(fb)` = on Err, `fb.on_call(event)`. Add the §10 Q3 non-recursion convention doc to `FallbackSink`.
- **Verify:** `cargo test -p atd-runtime --lib audit` green; `on_call_is_non_blocking_under_burst` (Drop default) stays green.

**Axis B commit:** `feat(atd-runtime): selectable audit backpressure (Drop/Block/Fallback) — Axis B`

---

## Conformance + architecture doc + workspace gate · Task F.1–F.3

### Task F.1 — conformance scenarios

**Landed:** `crates/atd-conformance/tests/schema_advisory_docs.rs` (Axis D) —
end-to-end schema lock; green.

**Coverage decision (2026-05-29):** the behaviour of Axes A/B/C is covered by
dedicated **unit tests** at the layer where the logic lives, exercised green:

- Axis A — `atd-runtime` middleware (`on_error` redacts message + details;
  default no-op) + `atd-middleware-pii-redact-medical` (`on_error` redacts
  structured PHI); the 6 dispatch error exits verified compiling + 152
  runtime lib tests green (no regression).
- Axis B — `atd-runtime` audit (`block_strategy_loses_nothing_under_burst`
  with a throttled writer + 0 drops; `fallback_strategy_routes_overflow`;
  `bare_sink_defaults_to_drop`).
- Axis C — `atd-runtime` audit (`capability_provenance_roundtrips_both_sources`;
  `v2_event_without_provenance_deserializes_to_none` back-compat).

**End-to-end cross-impl conformance for A/B/C** (real server + wire +
custom tools/middleware/UCAN-minting) is carried by the **celia adopter
validation** (`docs/issues/2026-05-29-observability-completeness-adopter-validation.md`,
steps 2-4: PHI-on-failure / Block-mode / provenance) rather than duplicated
as standalone `atd-conformance` scenarios this cycle. Adding them to
`atd-conformance` (each needs a `Tool` impl + `CallFuture` + middleware
wiring + ed25519 UCAN minting) is a clean follow-up if a second non-celia
medical adopter appears (same wait-for-second-adopter discipline as cbrain
→ Python runtime). Logged here so the coverage boundary isn't silent.

### Task F.2 — architecture doc updates (the §6.4 / §7 inventory the SP touches)
- `docs/atd-architecture.md` §6.4 (Audit) — document `capability_provenance` field + `BackpressureStrategy` (Drop default, Block/Fallback opt-in). Bump the `CallEvent` listing's `schema_version` comment to 3.
- §7 (Middleware) — document `Middleware::on_error` (error-path egress redaction); update the "errors flow past untouched" line (§7 intro) which is now false.
- §4.3 / error-codes — no new codes, but note error replies are now redacted.
- New-work-checklist compliance (CLAUDE.md): architecture updated in the same change set as the wire/extension-point change.

### Task F.3 — workspace gate (fire once)
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-features -- -D warnings`
- `cargo nextest run --workspace` (the single pre-commit gate; narrow `-p` iteration already done per-axis)
- `cargo build --release --workspace`
- Schema CI: confirm `gen-schema` output matches committed `/atd-protocol-schema.json`.

**Final commits (per ADR 0004 per-crate bumps):**
- `feat(atd-protocol): schema v3 + advisory docstrings → 1.2.0`
- `feat(atd-runtime): observability completeness (on_error / backpressure / provenance) → 1.2.0`
- `feat(atd-middleware-pii-redact-medical): on_error PHI redaction → minor`
- `test(atd-conformance): observability-completeness scenarios → minor`
- `docs(architecture): §6.4/§7 observability completeness`
- ADR 0006 (amends SP-medical-middleware §4.2).

---

## Risk register

| Risk | Mitigation |
|---|---|
| Touching `atd-runtime` recompiles ~10 downstream crates + test bins (burn-in risk per CLAUDE.md) | `CARGO_BUILD_JOBS=4 cargo test -p atd-runtime -- --test-threads=4` for tight loops; workspace gate once at end |
| `Block` strategy deadlocks if drain task and dispatch share a single-thread runtime | Spike (B.2) on the `multi_thread` ref runtime; document `Block` requires multi-thread runtime (ref binaries already use it per SP-concurrency-baseline) |
| `on_error` on the paginated `{e:?}` catch-all changes an error string a test pins | grep tests for the `continuation failed: {e:?}` format before editing; update the one paginated error test if it pins the literal |
| schema_version 2→3 breaks a downstream audit consumer pinning `== 2` | celia/healthkit adopter-validation step (b) explicitly checks v3 parses; back-compat serde test in C.1 |
| `FhirMiddleware::on_error` no-op leaves a gap if a future FHIR error carries codes | documented decision (FHIR validates resource shape, not error text); PII override is the load-bearing one |
