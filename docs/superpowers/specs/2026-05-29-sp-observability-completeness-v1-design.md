# SP-observability-completeness-v1 — closing the gaps between ATD's audit/redaction promises and its dispatch reality

| Status | Draft |
| Created | 2026-05-29 |
| Author | atd maintainers (post design-audit) |
| Phase | ATD post-1.1; observability + egress-safety completeness |
| Trigger | 2026-05-29 design audit (12 issues surfaced); celia HIPAA accounting hard requirement |
| Anchor | design-philosophy 原则 7 (dispatch bounded + observable) · 三消费者 #2 (human operator) · positioning §4.3 (audit) |
| Related | SP-medical-middleware (`2026-05-11-sp-medical-middleware-design.md` §4.2 — assumption revised here) · SP-concurrency-baseline (`2026-05-12-sp-concurrency-baseline-design.md` §5.4 — audit mpsc base) · SP-operability-v1 (audit C1) · SP-capability-v2 (`2026-05-11-sp-capability-v2-design.md` — provenance source) · ADR 0004 (per-crate SemVer) · ADR 0005 (UCAN-lite sunset — sibling, not in this SP) |

---

## 1. Motivation

A 2026-05-29 design audit walked ATD against its own two constitutional documents (`docs/atd-positioning.md`, `docs/atd-design-philosophy.md`) and surfaced **four places where ATD's protocol-level promise to its second consumer — the human operator — is quietly discounted at the dispatch layer**.

The design-philosophy frames every ATD server as serving three consumers simultaneously (§1 "The Three Consumers"): the LLM agent (wire frames), the human operator (audit sink), the bridge (handshake). Principle 7 ("Dispatch is bounded and observable") is the operator's guarantee. The audit found four leaks in that guarantee — none of them violates the frozen 1.x wire format, but each lets the operator (and, for the first one, the *agent itself*) see less, or be misled by, what ATD claims to deliver.

### 1.1 The four gaps

**Gap A — Error paths bypass the egress middleware pipeline → PHI leak.** `crates/atd-runtime/src/middleware.rs:9-10` states the design as built: *"Error paths bypass middleware in SP-12 — spec §8 Q4."* SP-medical-middleware §4.2 ratified this with the argument "no PHI exit point through audit exists" — but that argument was about the *audit sink*, not the *wire reply to the agent*. Reading dispatch as it stands today (`crates/atd-runtime/src/dispatch.rs`):

- A tool that fails with `ToolCallError::ExecutionFailed { code, message, retryable }` returns `Response::ToolResultResponse { success: false, result: { code, message, retryable } }` (`dispatch.rs:792-815`). **The `message` reaches the LLM verbatim and never touches a middleware.** A FHIR tool that does `Err(ExecutionFailed { message: format!("failed to parse Patient/{mrn} birthDate {dob}"), .. })` leaks the MRN and DOB straight to a third-party LLM (Anthropic / OpenAI / DeepSeek) — exactly the HIPAA §164.502 disclosure the PHI middleware was installed to prevent.
- A tool that fails with `ToolCallError::InvalidArgs(msg)` or `InternalError(msg)` returns `Response::Error { message: format!("... {msg}") }` (`dispatch.rs:777-791` / `817-830`). Same leak via a different wire variant.

The PHI middleware (`atd-middleware-pii-redact-medical`) is wired to `Middleware::on_result`, which only runs on the success branch (`dispatch.rs:764-767` and the paginated twin at `:479-481`). **Failure is the one path where PHI most commonly appears in free text, and it is exactly the path with no redaction.** This is a bug, not a trade-off.

**Gap B — Audit drops lose caller/tool identity → HIPAA accounting black hole.** SP-concurrency-baseline §5.4 correctly made `JsonLinesAuditSink::on_call` non-blocking via a bounded `tokio::sync::mpsc` (`audit.rs:106-179`): when the channel is full, the event is dropped and `drops` increments. The rationale — "log loss >> dispatch stall" — is right for the *default* server. It is **wrong for a medical adopter**: HIPAA §164.528 (accounting of disclosures) requires *every* PHI access be traceable. "We dropped 23 audit events but cannot say which 23" is not an acceptable answer to an auditor — any one of the dropped events might be the disclosure under review. ATD ships exactly one backpressure policy (silent Drop) and no way for the operator to choose differently.

**Gap C — `CallEvent.granted_capabilities` is a flat string list → capability provenance is unrecoverable.** Since SP-capability-v2, a call's granted set is a *union* of two independent sources (`docs/atd-architecture.md` §5.2): the operator string allow-list (`requested ∩ offered`) and the UCAN-lite attenuation chain (`granted_ucan`). The audit event records the *result* (`["records:write"]`) but not the *source*. When an operator asks "why did caller `hermes-orch` have `records:write` on 2026-05-29?", the audit cannot answer: was it the server's `--grant-capability`, or a UCAN chain link, and if so, which issuer? For a delegation model whose whole point is "the user trusted Agent A, who lent a subset to B", the audit being unable to name *which delegation* granted an authority defeats the observability principle.

**Gap D — Declarative-only fields don't say so in the schema → adopters mis-use them.** `ToolResources::rate_limit_per_min` (`tool.rs`) and `ToolTrust::trust_level` are documented as advisory/self-declared in `docs/atd-architecture.md` §10.7 / §6.1, but the *schema* (`/atd-protocol-schema.json`, generated from Rust doc-comments via schemars) doesn't carry that caveat. An adopter reading `trust_level: L4Certified` off a `tool_schema` response, or an SDK consumer auto-documenting `rate_limit_per_min`, has no in-band signal that neither is enforced. An LLM that bases a security decision on `trust_level` is reasoning on an unverified, publisher-self-declared field. The fix is one schemars doc-comment per field; the cost of *not* fixing it is a class of silent mis-use.

### 1.2 Why bundle these four

All four are the same shape: **"ATD ships a one-consistent-mechanism promise (audit / cap gate / egress redaction), and the dispatch reality discounts it for the human operator (or, in Gap A, the agent)."** They share one test surface (`atd-conformance`), one schema bump (Gap C bumps `CallEvent.schema_version` 2→3; Gap D edits schemars output), and one adopter validator (celia, which has the compliance teeth to exercise all four). Shipping them as four separate PRs would re-pay the conformance + schema-regen + adopter-validation overhead four times. One SP, one tag, one adopter validation cycle.

A fifth axis (E) is the **Phase 0 documentation alignment** the audit also surfaced (ADR 0004 drift, MCP-lossy boundary, cross-vendor capability boundary, cursor-key-rotation gap). It is doc-only, ships *first* (before any code), and is folded into this SP so the SP isn't written against stale constitutional sources.

---

## 2. Goals

- **Gap A** — error/failure paths traverse a middleware pipeline before reaching the wire, symmetric with the success path. PHI in failure `message`/`details` is redacted by the same middleware that redacts success results.
- **Gap B** — `AuditSink` exposes a `backpressure_strategy()` the operator can set; ship `Drop` (current default, byte-compatible), `Block` (dispatch waits — medical/compliance), and `FallbackSink` (mpsc-full → secondary sink).
- **Gap C** — `CallEvent` carries optional `capability_provenance`; the operator can trace each granted capability to `StringAllowList` or `UcanChain { issuer_did, chain_depth }`.
- **Gap D** — `ToolResources::rate_limit_per_min` and `ToolTrust::trust_level` carry schemars doc-comments stating their advisory/self-declared status; the caveat appears in `/atd-protocol-schema.json`.
- **Gap E (Phase 0)** — four constitutional docs aligned to current reality before any code lands.
- **Back-compat** — every code change is additive. Old clients, old sinks, old middleware, and old SDKs keep working with byte-identical default behaviour. `schema_version` bump is additive (new optional field).

## 3. Non-goals

- **UCAN-lite strategic decision** — its shipped-dormant status, sunset timeline, and deprecation path are ADR 0005 (sibling), not this SP. This SP only *reads* the UCAN chain to populate provenance (Gap C); it does not change UCAN semantics.
- **Cursor HMAC key rotation implementation** — Gap E documents the gap (cursor continuity across server restart is adopter-side); the actual key-rotation mechanism waits for an adopter that feels the re-fetch cost. Doc note only here.
- **Audit body carrying `result_preview`** — SP-medical-middleware §4.7 (no PHI through audit) stands. We add `on_error` middleware for the *wire reply*, NOT a result body on `CallEvent`. The audit sink still never sees PHI.
- **Wire format change** — `schema_version` 2→3 is an additive optional field on `CallEvent` (an audit-log shape, not a wire envelope). No `Request`/`Response` variant changes. The frozen 1.x wire contract is untouched.
- **Skills `ToolingOnly` visibility** (audit issue #6), **AGENTS/CLAUDE doc merge** (#12) — perceived issues / repo housekeeping, not promoted to this SP.
- **Per-tool rate-limiter enforcement** (architecture §10.7) — Gap D *documents* that `rate_limit_per_min` is advisory; it does not *enforce* it. Enforcement waits for an adopter need.
- **Making `Block` the default backpressure strategy** — `Drop` stays the default (byte-compatible, protects the throughput SLO for the 90% case). Medical adopters opt into `Block`.

---

## 4. Design

Five axes. Each: chosen answer, evidence from current source, rejected alternatives.

### 4.1 Axis A — Error-path egress middleware

**The bug, precisely.** Three failure exits, two wire shapes, zero middleware today:

| `ToolCallError` variant | Wire response | Carries tool text? | Middleware today |
|---|---|---|---|
| `ExecutionFailed { code, message, retryable }` | `ToolResultResponse { success: false, result: {code, message, retryable} }` (`dispatch.rs:792-815`, paginated twin `:490-504`) | **yes — `message` is tool-authored** | **none** |
| `InvalidArgs(msg)` | `Response::Error { message: "invalid args for {id}: {msg}" }` (`dispatch.rs:777-791`) | yes — `msg` is tool-authored | none |
| `InternalError(msg)` | `Response::Error { message: "internal error in {id}: {msg}" }` (`dispatch.rs:817-830`) | yes — panic/error text | none |

(Capability-denied / rate-limited / tool-not-found / broker-failed also produce `Response::Error`, but their `message` is framework-generated and PHI-free; they still flow through the new hook harmlessly.)

**Decision — two complementary fixes, matched to the two wire shapes:**

**A1 — `ExecutionFailed` result runs the existing `on_result` pipeline.** The `ExecutionFailed` exit produces a `ToolResultResponse` whose `result` is already a `serde_json::Value` — structurally identical to a success result, only `success: false`. The fix is to run the **same** `for mw in &state.middleware { mw.on_result(...) }` loop on it that the success branch (`:764-767`) and paginated success branch (`:479-481`) already run. This is the minimal, semantically-unambiguous fix: a `tool_result` envelope — success or failure — is exactly what egress middleware exists to rewrite. The PHI middleware's `walk_strings` already scrubs every string leaf, so `result.message` containing an MRN is caught with zero new redaction logic.

**A2 — `Response::Error` runs a new `on_error` hook.** The `InvalidArgs` / `InternalError` exits produce `Response::Error { message: String, code, retryable, details: Option<Value> }` — a different shape (bare `String` message + optional `details` value), not a `result` Value. For these we add one trait method:

```rust
pub trait Middleware: Send + Sync {
    fn name(&self) -> &'static str;
    fn on_result(&self, tool_id: &str, tool_def: &ToolDefinition, result: &mut serde_json::Value);

    /// SP-observability-completeness-v1 Axis A. Egress redaction for the
    /// FAILURE wire shape `Response::Error { message, details }`. Default
    /// is a no-op, preserving pre-SP behaviour for middleware that only
    /// rewrite success results. **Security-sensitive middleware (PHI / PII
    /// redaction) MUST override this** — a tool's `InvalidArgs` /
    /// `InternalError` text reaches the LLM verbatim, and may carry PHI
    /// (an arg echo, a panic message naming a patient). `details` is the
    /// optional structured error payload; redact both.
    fn on_error(
        &self,
        tool_id: &str,
        tool_def: &ToolDefinition,
        message: &mut String,
        details: &mut Option<serde_json::Value>,
    ) {
        let _ = (tool_id, tool_def, message, details);
    }
}
```

`PiiRedactMiddleware` and `FhirMiddleware` override `on_error`; `RedactPathsMiddleware` overrides it too (its `$HOME` scrub is as relevant to error text as to results). The PII crate's redaction core (`redact_value` per SP-medical-middleware §4.7) already operates on `&mut Value` for `details`; for the bare `message: String` it wraps it as a transient `Value::String`, runs the same regex/path scrub, unwraps.

**Why default no-op (not abstract/required).** Forcing every existing `Middleware` impl to add `on_error` would break adopter middleware on recompile — violating the additive goal. A no-op default means: pre-SP middleware behave exactly as today (error text un-rewritten — same as current), and security middleware opt into the stronger behaviour. The conformance suite asserts the PHI middleware *does* override (a no-op PHI middleware would be the real bug).

**Why `on_error` for `Response::Error` but `on_result` for the `ExecutionFailed` envelope.** They are genuinely different shapes. `ExecutionFailed` → `ToolResultResponse` (a `result: Value`); reusing `on_result` is correct and free. `Response::Error` → a bare `message: String` + `details`; it has no `result` Value, so it needs its own hook. Conflating them (e.g. synthesising a fake `result` Value for `Response::Error`) would force every `on_result` impl to defend against a shape it never sees in success — worse than one honest extra method.

**Dispatch touch points (4 sites):**
- `dispatch.rs:792-815` (`run_tool` ExecutionFailed) — add `on_result` loop before constructing the `ToolResultResponse`.
- `dispatch.rs:490-504` (paginated continuation ExecutionFailed) — same.
- `dispatch.rs:777-791` (`run_tool` InvalidArgs) — add `on_error` loop before `Response::Error`.
- `dispatch.rs:817-830` (`run_tool` InternalError) + `:505-510` (paginated `Err(e)`) — same.

For `Response::Error` sites we need `entry.definition()` for the `tool_def` arg; both InvalidArgs/InternalError sites have `entry` in scope. Capability-denied / tool-not-found early-returns (where no `entry` exists) do NOT run `on_error` — their messages are framework-generated and PHI-free; skipping them avoids an `Option<&ToolDefinition>` complication for zero benefit.

**Revises SP-medical-middleware §4.2.** That section's "error path no PHI surface" claim was scoped to the audit sink (true — still true) but was cited as justification for error paths skipping middleware *entirely*. This SP narrows it: audit sink still sees no PHI (Gap B/C add only metadata), but the *wire reply* now runs egress redaction on failure too. SP-medical-middleware §4.2 gets an amendment note pointing here.

### 4.2 Axis B — Audit backpressure strategy

**Decision.** Add a strategy selector to `AuditSink`, defaulting to today's behaviour:

```rust
/// SP-observability-completeness-v1 Axis B. How a sink behaves when its
/// internal queue is full at `on_call` time.
#[derive(Clone)]
pub enum BackpressureStrategy {
    /// Drop the event, increment `drops()`. The SP-concurrency-baseline
    /// default — protects the dispatch throughput SLO; correct for the
    /// 90% non-compliance case. "log loss >> dispatch stall."
    Drop,
    /// Block the dispatch task until the queue drains enough to enqueue.
    /// For compliance adopters (HIPAA §164.528) where a dropped audit
    /// event is unacceptable: dispatch slows under audit backpressure
    /// rather than losing the disclosure record. Throughput becomes
    /// bounded by sink drain rate under sustained load.
    Block,
    /// On queue-full, write the event synchronously to a fallback sink
    /// (e.g. stderr, a second file) instead of dropping. Bounds the hot
    /// path (no indefinite block) while guaranteeing no silent loss.
    FallbackSink(Arc<dyn AuditSink>),
}

pub trait AuditSink: Send + Sync {
    fn on_call(&self, event: &CallEvent);
    fn drops(&self) -> u64 { 0 }
    /// Default `Drop` — byte-compatible with pre-SP sinks. Adopters that
    /// need no-loss audit override this (or construct `JsonLinesAuditSink`
    /// via `with_strategy`).
    fn backpressure_strategy(&self) -> BackpressureStrategy {
        BackpressureStrategy::Drop
    }
}
```

`JsonLinesAuditSink` gains `with_strategy(writer, capacity, strategy)` and honours it in `on_call`:
- `Drop` — current `try_send` + counter (unchanged).
- `Block` — `blocking_send` semantics: since `on_call` is sync but the channel is async, use `tx.try_send` in a bounded spin-with-yield, or switch the internal channel to a `std::sync::mpsc` with a blocking `send` when strategy is `Block`. (Plan picks the concrete mechanism after a spike; the trait surface is fixed here.)
- `FallbackSink(fb)` — on `try_send` Err, call `fb.on_call(event)` synchronously.

**Why keep `Drop` default.** SP-concurrency-baseline's 50-client storm SLO (p99 < 200ms, 0 audit_drops measured) was achieved with `Drop`. Changing the default to `Block` would regress throughput for every non-compliance adopter to protect a guarantee only compliance adopters need. The audit's own framing ("Drop is the wrong *default for medical*") is satisfied by making it *selectable*, not by flipping the global default.

**Why a trait method + not a `ServerConfig` flag.** The strategy is a property of the *sink* (a Kafka sink blocks differently than a file sink), not of the server. Putting it on `AuditSink` lets each sink declare its own; `JsonLinesAuditSink::with_strategy` is the ergonomic constructor. A `ServerConfig` flag would only work for the one shipped sink.

**Adopter validation (celia).** celia flips to `Block`, reruns the 120-query SHARP baseline with a simulated slow disk, and confirms dispatch p99 stays bounded (if not, bump mpsc capacity). 0 drops is the assertion.

### 4.3 Axis C — Capability provenance

**Decision.** Additive optional field on `CallEvent`, `schema_version` 2→3:

```rust
pub struct CallEvent {
    // ... existing fields ...
    pub schema_version: u32,   // 2 → 3
    /// SP-observability-completeness-v1 Axis C. Per-capability source
    /// attribution. `None` for servers/calls where provenance wasn't
    /// tracked (back-compat); `Some(vec)` when dispatch recorded which
    /// mechanism granted each capability. Lets an operator answer
    /// "why did caller X have capability Y?" without re-deriving the
    /// UCAN chain by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_provenance: Option<Vec<CapProvenance>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapProvenance {
    pub cap: String,
    pub source: ProvSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProvSource {
    /// Granted by the operator string allow-list (`--grant-capability`
    /// ∩ `Hello.requested_capabilities`).
    StringAllowList,
    /// Granted by a UCAN-lite chain link. `issuer_did` is the link's
    /// `iss`; `chain_depth` is its position (0 = root).
    UcanChain { issuer_did: String, chain_depth: u8 },
}
```

Dispatch already computes the union at Hello time, branching on the two sources (architecture §5.2). The capability gate has both the string-intersection set and the UCAN-derived set in hand; recording each cap's origin is a few lines where the union is formed. The per-call `CallEvent` reads it from the connection's negotiated state.

**Why `Option<Vec<_>>` not `Vec<_>`.** Old `CallEvent` JSON (no field) must deserialize into a v3 reader as `None`, and a v3 event must deserialize into a v2 reader by ignoring the unknown field. `#[serde(default, skip_serializing_if = "Option::is_none")]` gives both. A non-optional `Vec` would break v2-reader-of-v3 (unknown field is fine) but more importantly would force every dispatch path (dry-run, tool-not-found — which have no capability context) to synthesise an empty vec; `None` is the honest "not tracked here" signal.

**Why `schema_version` bump to 3.** Consistent with the v1→v2 precedent (`audit.rs:21-30`): the bump records *when* the field landed, for consumers that branch on version. It is not a breaking shape change — additive optional field, same as v2's `cursor_page`.

**Why not put provenance on the wire (`HelloAck`).** The agent doesn't need to know *why* it has a capability — it has it or it doesn't. Provenance is an *operator/audit* concern (consumer #2), so it belongs on `CallEvent`, not the wire envelope. This keeps the frozen wire untouched.

### 4.4 Axis D — Schema advisory doc-comments

**Decision.** Doc-comments (which schemars compiles into `/atd-protocol-schema.json` field descriptions) on the two declarative-only fields:

```rust
pub struct ToolResources {
    pub timeout_ms: u64,
    pub max_concurrent: u32,
    /// **Advisory only in v1 — NOT enforced by dispatch.** The only
    /// enforced concurrency control is `max_concurrent` (per-tool
    /// semaphore, `atd-runtime` registry). Adopters needing real
    /// per-minute rate limiting compose their own limiter (e.g. the
    /// `governor` crate) outside dispatch. A future SP may make this
    /// enforced; adopters relying on advisory-only behaviour should
    /// re-audit when that lands. See architecture §10.7.
    pub rate_limit_per_min: Option<u32>,
    pub estimated_tokens: Option<u32>,
}

pub struct ToolTrust {
    pub publisher: String,
    /// **Publisher self-declared in v1 — ATD does NOT verify trust
    /// level.** `L4Certified` means "the publisher claims certification",
    /// not "ATD verified it". Use only as a hint to higher layers; do
    /// NOT base a security decision on this field alone. A future SP may
    /// add publisher-key PKI verification. See architecture §6.1 / §10.3.
    pub trust_level: TrustLevel,
    pub signature: Option<String>,
}
```

The `gen-schema` bin (CI-gated, `--all-features`) regenerates `/atd-protocol-schema.json`; the committed JSON now carries both caveats in the relevant `description` fields. Any SDK auto-doc or IDE hover surfaces them.

**Why doc-comments, not a structural marker** (e.g. `enforced: bool` field). A structural marker would be a wire-format change (new field) for zero runtime behaviour — the caveat is documentation, and schemars is exactly the mechanism that turns Rust docs into machine-readable schema. Cheapest honest fix.

### 4.5 Axis E (Phase 0) — Documentation alignment

Doc-only, lands **first** (before code), so the SP and its implementation aren't written against stale sources. Four edits:

| # | File / section | Change | Audit issue |
|---|---|---|---|
| E1 | `docs/atd-architecture.md` §9.4 | "workspace-lockstep" → per-crate independent SemVer per ADR 0004 | #8 |
| E2 | `docs/atd-architecture.md` §10 (new §10.8) | New non-goal: "Cross-vendor capability federation" — a single UCAN/caller_id/audit does not span two ATD servers; multi-vendor multi-agent协作 is adopter-built | #5 |
| E3 | `docs/atd-architecture.md` §5.6 | Operational note: cursor HMAC key rotation is adopter-side; no cross-restart cursor continuity spec in v1 (server restart → 1020, client re-fetches) | #9 |
| E4 | `docs/atd-positioning.md` §5.2 + `crates/atd-mcp-bridge/README.md` | "MCP path is lossy" table: tier / safety / required_capabilities / output_schema / dry_run / cursor / caller_id all dropped or degraded over the bridge; native SDK for full feature set | #3 |

Plus housekeeping: `docs/intro/atd-tech-deck.zh.md` §1.6 + `CLAUDE.md` "Current project state" — the workspace-lockstep phrasing → per-crate SemVer (ADR 0004).

These are the audit's 🟡 "honest-the-gap" items; none changes code, all remove a positioning/architecture claim that's now false or a boundary that's undocumented.

---

## 5. Conformance scenarios

New `atd-conformance` scenarios (each its own failure-mode test):

| Scenario | Axis | Asserts |
|---|---|---|
| `error_pii_redaction` | A | A tool returning `ExecutionFailed { message: "...Patient/12345..." }` and one raising `InvalidArgs("...MRN 999..")`, behind a redacting middleware, both reach the client with the PHI replaced by `[REDACTED:*]`. A no-op middleware leaves them unchanged (control). |
| `audit_backpressure_block` | B | A `JsonLinesAuditSink::with_strategy(.., Block)` fronting a deliberately slow writer: under a 200-event burst, `drops() == 0` and every event eventually lands. Contrast a `Drop` sink on the same burst showing `drops() > 0`. |
| `capability_provenance` | C | A `Hello` carrying both `requested_capabilities` (string allow-list) and a `ucan_tokens` chain: the resulting `CallEvent.capability_provenance` contains both a `StringAllowList` entry and a `UcanChain { issuer_did, chain_depth }` entry, mapping each granted cap to its source. |
| `schema_advisory_docs` | D | Parse `/atd-protocol-schema.json`; assert the `rate_limit_per_min` description contains "Advisory only" and `trust_level` contains "self-declared". Locks the schemars output so a future doc-comment edit can't silently drop the caveat. |

The existing `concurrent_handshake_storm` and `paginated_dispatch` scenarios must stay green (regression guard for the dispatch + audit changes).

---

## 6. Wire / schema impact

| Surface | Change | Compat |
|---|---|---|
| `Request` / `Response` enums | **none** | frozen 1.x wire untouched |
| `CallEvent` (audit-log shape, not wire) | `+capability_provenance: Option<Vec<CapProvenance>>`; `schema_version` 2→3 | additive; v2 readers ignore field, v3 readers see `None` on v2 events |
| `Middleware` trait | `+on_error(..)` default no-op | additive; pre-SP middleware unchanged |
| `AuditSink` trait | `+backpressure_strategy()` default `Drop` | additive; pre-SP sinks unchanged |
| `/atd-protocol-schema.json` | `rate_limit_per_min` + `trust_level` descriptions gain caveat text | additive description-only; CI gen-schema regen |
| `ToolResources` / `ToolTrust` Rust | doc-comments only | no ABI change |

**1.x stability check** — confirmed against `docs/release-plan-v1.0.md`: additive optional fields are minor bumps; no field removed, no shape changed, no wire envelope touched. Old↔new in both directions verified by the conformance regression set + serde default tests (mirroring the existing `tool_result_response_back_compat_default_when_field_missing` test pattern at `messages.rs:282-294`).

---

## 7. Sizing

| Axis | code LoC | test LoC |
|---|---|---|
| A error-path PII | ~40 middleware trait + ~50 dispatch (4 sites) + ~30 PII/FHIR override | ~140 |
| B backpressure | ~50 trait + enum + ~70 `JsonLinesAuditSink::with_strategy` | ~90 |
| C provenance | ~30 audit struct + ~40 dispatch union-record | ~80 |
| D docstring | ~12 doc-comments + schema regen | ~30 |
| E doc (Phase 0) | ~250 doc lines | 0 |
| **Total** | **~290 LoC code + ~250 doc** | **~340 LoC test** |

Estimate **~5-7 working days** (1 engineer).

---

## 8. Release

- **Tag:** `sp-observability-completeness-v1`
- **Versioning (per ADR 0004 — per-crate independent SemVer):**
  - `atd-protocol` 1.1.x → **1.2.0** (`CallEvent` provenance field + schema doc-comments → schema_version 3; this is the ATD release identity bump).
  - `atd-runtime` 1.1.x → **1.2.0** (`Middleware::on_error` + `AuditSink::backpressure_strategy` — additive trait methods with defaults).
  - `atd-middleware-pii-redact-medical` / `atd-middleware-fhir` → **minor** (override `on_error`).
  - `atd-conformance` → **minor** (4 new scenarios).
  - `atd-server` / `atd-server-http` / `atd-sdk` / `atd-cli` → **patch** (rebuild against new runtime; no own-source behaviour change).
- **Companion ADR:** ADR 0006 — "Observability completeness; SP-medical-middleware §4.2 amended (error paths now redacted)."

---

## 9. Adopter validation plan

| Adopter | Runs | Expected |
|---|---|---|
| **celia_phr** | (a) SP-medical-middleware test suite + a new "tool fails with PHI in message" case; (b) flip `AuditSink` to `Block`, rerun 120-query SHARP baseline with simulated slow disk; (c) enable `capability_provenance`, query audit for cap source | (a) PHI absent from failure replies; (b) p99 bounded, `drops == 0`; (c) one-line `jq` shows StringAllowList vs UcanChain per cap |
| **healthkit_cli** | (a) smoke test; (b) audit consumer parses `schema_version: 3` without breaking | drop-in; no regression |
| **cbrain (Python server)** | (a) mirror `on_error` + `backpressure_strategy` in `python/src/atd_server/`; (b) conformance fixtures 22/24 → 25-27 (the 3 new behavioural scenarios that apply cross-impl) | Python impl reaches parity |

---

## 10. Open questions for the plan

1. **Block mechanism** — `try_send` spin-with-yield vs a strategy-selected `std::sync::mpsc` blocking channel. Spike both for latency under the conformance burst; pick in the plan.
2. **`on_error` for paginated `Err(e)` catch-all** (`dispatch.rs:505-510`) — that arm formats `{e:?}` which can embed PHI from a `Debug` impl. Decide: run `on_error` there too (safest) vs document that `Debug`-formatted errors are a tool-author responsibility. Lean: run `on_error` (defence in depth).
3. **`FallbackSink` recursion guard** — a `FallbackSink` whose fallback is itself another mpsc sink could chain-block. Document: fallback sinks should be synchronous (stderr/file), and assert non-recursion in the constructor or by convention. Decide in plan.
