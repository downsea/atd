# ATD Architecture (v1)

**Version:** 1.0 — 2026-04-24
**Implementation baseline:** `sp12-canonical-dispatch` (or the most recent commit on master containing the four dispatch primitives described in §4.2).
**Scope:** Normative architecture for the **reference implementation** (`atd-mvp` crates). Complements but does not replace the ATD whitepaper (`docs/whitepaper/v3-multi-device.md`) or the wire reference (`docs/protocol/wire-format.md`).
**Authority:** Where this document disagrees with `docs/design.md` (which predates SP-1), this document is authoritative. Where it disagrees with the v3 whitepaper on aspirational scope, the whitepaper remains authoritative for the protocol's long-term direction; this document is authoritative for what the reference implementation commits to.
**License:** Apache-2.0.

---

## Table of contents

1. [The protocol identity](#1-the-protocol-identity)
2. [The layer model](#2-the-layer-model)
3. [Schema Layer](#3-schema-layer)
4. [Dispatch Layer](#4-dispatch-layer)
5. [Security Layer](#5-security-layer)
6. [Extensibility](#6-extensibility)
7. [Skills Layer (adjacent)](#7-skills-layer-adjacent)
8. [Component / crate map](#8-component--crate-map)
9. [Non-goals (explicit)](#9-non-goals-explicit)
10. [Evolution path](#10-evolution-path)

**Reading guidance:** External protocol implementers — start with §2 and §3. Internal contributors — status tables in §3-§6 each row cites an `issues/` file or SP tag; pick work by status. Decision-makers — §1, §9, §10 give the identity, boundaries, and direction.

**Legend for status-table glyphs** (used throughout §3-§6):

| Glyph | Meaning |
|---|---|
| ✅ | **implemented** — code + tests + docs present |
| ⚠️ | **partial** — code exists; runtime skeletal, tests thin, or documented aspect missing |
| 🔨 | **in-progress** — actively being landed at this document's write time |
| ❌ | **missing** — not started; row cites `docs/issues/<file>.md` |
| 🚫 | **non-goal** — deferred by design; row cites §9 |
| 📜 | **informational** — type/field exists but documented as not load-bearing |

---

## 1. The protocol identity

### 1.1 One-sentence definition

ATD (Agent Tool Dispatch) is a protocol that lets **any tool, on any platform, be callable by any agent, through any framework**.

The four "any"s frame the interoperability claim. Today:

| Dimension | Fragmentation | ATD's answer |
|---|---|---|
| Any tool | CLI, REST, MCP, native SDK — mutually incompatible shapes | One tool definition maps to multiple bindings |
| Any platform | Linux / macOS / Windows / iOS / Android / HarmonyOS each have distinct call surfaces | Platform-available binding is auto-selected at dispatch time |
| Any agent | Claude Code cannot consume OpenAI function-calling dicts without a shim | All agents call through a common client SDK; adapters produce per-provider dicts when needed |
| Any framework | LangChain tool ≠ MCP tool ≠ Apple App Intent | One definition, many framework consumers |

### 1.2 What this document is

A normative architecture reference for the **reference implementation** published in this repository. It documents the layer model, the mechanisms each layer implements, the mapping from layers to crates, and the evolution path.

Three reader classes:

- **External protocol implementers** — authors of Go / Java / Swift / TS / ArkTS SDKs, or tool-server implementers in languages this repository does not ship. This document gives them the layer model, wire contract, capability semantics, and binding extension points.
- **Internal contributors** — working against the reference implementation. This document gives them per-layer status, crate maps, and the SP roadmap for deciding what to pick up.
- **Decision-makers** — evaluating adoption. This document gives them identity, non-goals, and direction.

### 1.3 What this document is not

- Not the wire-level reference — see [`docs/protocol/wire-format.md`](protocol/wire-format.md).
- Not a roadmap commitment calendar — see §10 for the directional path; specific dates depend on adopter timing.
- Not a rewrite of the whitepaper — [`docs/whitepaper/v3-multi-device.md`](whitepaper/v3-multi-device.md) remains authoritative for the protocol's long-term aspirational scope. This document reconciles whitepaper direction with implementation reality.
- Not a successor to `docs/design.md` in a way that deletes history — `design.md` is retained as the original Phase 0 spec for archival context; this document supersedes it as the current reference.

### 1.4 Relationship to existing documents

| Document | Relationship |
|---|---|
| `docs/whitepaper/v3-multi-device.md` | Aspirational protocol scope. Whitepaper authoritative on long-term direction; this doc authoritative on reference-implementation commitments. |
| `docs/whitepaper/atd-v3-skills-architecture-brief.md` | Source for the five-layer stack diagram replicated in §2. |
| `docs/design.md` | Original Phase 0 spec. Superseded by this document for architecture questions; retained for history. |
| `docs/protocol/wire-format.md` | Wire-level reference — byte framing, message types, full type tables. Refer out to it; this document does not repeat wire details. |
| `docs/protocol/error-codes.md` | Error taxonomy. Refer out. |
| `docs/integrations/*.md` | Consumer-side guides per framework. This document gives them the layer model they assume. |
| `docs/issues/*.md` | Per-issue gap tracking. Every `❌` row in this document cites an `issues/` file. |

## 2. The layer model

### 2.1 Stack

Adapted from [`docs/whitepaper/atd-v3-skills-architecture-brief.md`](whitepaper/atd-v3-skills-architecture-brief.md) Slide 1:

```
┌────────────────────────────────────────────────────────────────┐
│  User intent (voice · text · trigger)                           │
└───────────────────────────┬─────────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────────┐
│  Agent Framework                                                │
│  (Claude Code · Cursor · Hermes · LangChain · custom)           │
└────────────┬──────────────────────────────┬─────────────────────┘
             │                              │
   via Skill │                              │ direct tool call
             ▼                              ▼
┌──────────────────────────────┐  ┌───────────────────────────┐
│  Skills Layer (§7 — adjacent) │  │  (no Skill intermediary)  │
│  SKILL.md · atd-tools · body │  │  simple / one-shot tasks  │
└──────────────┬───────────────┘  └──────────────┬────────────┘
               │                                 │
               └──────────────┬──────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│  Client SDK (§4.1 + §6.4 ergonomic aliases)                     │
│  discover · describe · call                                     │
└────────────────────────────┬───────────────────────────────────┘
                             │
                             ▼
┌────────────────────────────────────────────────────────────────┐
│  Dispatch Layer (§4)                                            │
│  capability gate · tier · binding · result middleware          │
└────────────────────────────┬───────────────────────────────────┘
                             │
     ┌──────┬──────┬──────┬──┴──────┬────────────────┐
     ▼      ▼      ▼      ▼         ▼                ▼
   ┌────┐┌─────┐┌──────┐┌────────────┐┌──────────────┐
   │CLI ││ MCP ││ REST ││AppFunction ││ Distributed  │
   │ (✅)││(✅)* ││(🚫)  ││(🚫)        ││ (🚫)         │
   └────┘└─────┘└──────┘└────────────┘└──────────────┘
                             │
                             ▼
┌────────────────────────────────────────────────────────────────┐
│  Tool Universe  (§4.2 NativeBinding + §6.1 extension points)    │
│  ref:echo, ref:fs.*, ref:shell.*, ref:web.fetch, ...            │
└────────────────────────────────────────────────────────────────┘
```

\* MCP: as an external-client protocol reached through `atd-mcp-bridge` (a separate binary speaking MCP over stdio, forwarding to ATD). Not a server-side dispatch target.

### 2.2 Three core mechanisms, two extensibility mechanisms

**Core mechanisms** (each a top-level section in this document):

1. **Schema Layer (§3)** — one machine-readable description for a tool's intent semantics AND its concrete invocation contract, usable across all bindings
2. **Dispatch Layer (§4)** — a deterministic pipeline through capability gate, tier-aware deadline resolution, binding selection, tool invocation, and result middleware
3. **Security Layer (§5)** — classifications + per-tool runtime controls + connection-scoped capability allow-listing

**Extensibility mechanisms:**

4. **Tier system (§4.2.3)** — three tiers (`Hot` / `Warm` / `Cold`) map to per-call deadline + output-budget overrides; gives tool authors a way to signal cost/latency class without naming specific values
5. **Binding extensibility (§6.1)** — pluggable invocation back-ends; v1 ships `NativeBinding` and `CliBinding`; more (MCP, REST, AppFunction, distributed) via the same trait

### 2.3 Layer ↔ section cross-reference

| Layer | Section | Primary crate | Status glance |
|---|---|---|---|
| Schema | [§3](#3-schema-layer) | `crates/atd-protocol/` | mostly ✅; machine-readable schema file ❌ |
| Dispatch core (discover/describe/call) | [§4.1](#41-core-dispatch) | `crates/atd-runtime/` + `crates/atd-sdk/` | ✅ |
| Dispatch · binding abstraction | [§4.2.1](#421-binding-abstraction) | `crates/atd-runtime/src/binding.rs` | ✅ (SP-12) |
| Dispatch · tier-aware deadlines | [§4.2.2](#422-tier-aware-deadlines) | `crates/atd-runtime/src/tier.rs` | ✅ (SP-12) |
| Dispatch · capability gate | [§4.2.3](#423-capability-gate) | `crates/atd-runtime/src/capability.rs` | ✅ (SP-12) |
| Dispatch · result-middleware pipeline | [§4.2.4](#424-result-middleware-pipeline) | `crates/atd-runtime/src/middleware.rs` | ✅ (SP-12, one built-in) |
| Dispatch · sessions & cancellation | [§4.2.5](#425-sessions-and-cancellation) | — | ❌ deferred by design |
| Dispatch · ergonomic aliases | [§4.2.6](#426-ergonomic-aliases) | — | ❌ SDK-only; planned |
| Security · classifications | [§5.1](#51-classification-taxonomy) | `crates/atd-protocol/` | ✅ |
| Security · per-tool runtime controls | [§5.2](#52-per-tool-runtime-controls) | per-tool files in `crates/atd-tools-*/src/` | ✅ where applicable |
| Security · capability tokens | [§5.3](#53-capability-tokens) | `crates/atd-runtime/src/capability.rs` | ✅ v1 (allow-list); full HMAC/UCAN 🚫 |
| Security · audit logging | [§5.4](#54-audit-logging) | — | ❌ |
| Security · rate limiting & concurrency | [§5.5](#55-rate-limiting-and-concurrency) | — | ❌ |
| Security · dry-run consistency | [§5.6](#56-dry-run-consistency) | per-tool files | ⚠️ |
| Skills (adjacent) | [§7](#7-skills-layer-adjacent) | — | out of scope for ATD core |

### 2.4 Two-pager call graph examples

**Example A: direct agent → ATD (one-shot):**

```
agent.llm
    ↓ decides tool_id = "ref:shell.exec" and args = {"command": "uname -s"}
atd_sdk::AtdClient::call(tool_id, args, CallOptions { .. })
    ↓ writes length-prefixed JSON over Unix socket
atd-ref-server accepts connection
    ↓ dispatcher: capability gate → registry lookup → tier deadline → binding → tool
NativeBinding::invoke(&args)
    ↓ executes
ToolResult { success: true, data: { stdout: "Linux\n", exit_code: 0, .. } }
    ↓ result-middleware pipeline (RedactPathsMiddleware rewrites any $HOME paths)
    ↓ serialized, length-prefixed JSON back
atd_sdk delivers ToolResult to agent
```

**Example B: Skills runtime → ATD (multi-step):**

```
skills_runtime loads skill @acme/morning-briefing per user intent
    ↓ install-time: runtime verified atd-tools: required are discoverable on the socket
skill body executed in agent context
    ↓ body step 1 says: call hms:health.sleep.get for yesterday
atd_sdk::AtdClient::call("hms:health.sleep.get", { "date": "2026-04-23" }, ..)
    ↓ same dispatch path as Example A
    ... (body continues with step 2, step 3)
skill returns to agent context with synthesised output
```

Both examples traverse exactly the same ATD dispatch. The Skills runtime is an agent-side orchestrator; it does not modify ATD's dispatch.

## 3. Schema Layer

### 3.1 Definition

The schema layer is the set of types and invariants that describe what a tool is — independent of any specific binding, transport, or agent framework. A valid ATD message — request or response — serializes to JSON shapes defined at this layer.

The schema layer owns:

- **Envelope types** — `ClientMessage` / `ServerMessage` (the wire messages)
- **Tool types** — `ToolSummary` (discover response), `ToolDefinition` (describe response), `ToolResult` (call response)
- **Structural types** — `ToolCapability`, `ToolBinding`, `ToolSafety`, `ToolResources`, `ToolTrust`
- **Enums** — `SafetyLevel`, `ToolVisibility`, `TrustLevel`, `ToolTier`, `BindingProtocol`
- **Error taxonomy** — `AtdError` (9 variants) with `is_retryable()` + `suggest_fix()` contracts
- **Name sanitization** — `ref:fs.read` → `ref_fs_read` for LLM/MCP name slots

The schema layer does NOT own: dispatch behavior, security enforcement, binding execution. Those are §4 and §5.

### 3.2 Current state

| Component | Source | Status | Tests | Notes |
|---|---|---|---|---|
| `ToolSummary` (incl. `input_schema`) | `crates/atd-protocol/src/summary.rs` | ✅ | types roundtrip tests | `input_schema` added in SP-10 Task 2.5 so LLM adapters emit real schemas |
| `ToolDefinition` + sub-structs | `crates/atd-protocol/src/tool.rs` | ✅ | roundtrip tests | — |
| `ToolResult` (Success/Error variants) | `crates/atd-protocol/src/result.rs` | ✅ | — | |
| `AtdError` (9 variants + `is_retryable` + `suggest_fix`) | `crates/atd-protocol/src/error.rs` | ✅ | — | See [`docs/protocol/error-codes.md`](protocol/error-codes.md) for the reference table |
| `SafetyLevel` / `ToolVisibility` / `TrustLevel` / `BindingProtocol` | `crates/atd-protocol/src/enums.rs` | ✅ | — | |
| `ToolTier` enum (`Hot` / `Warm` / `Cold`) | `crates/atd-protocol/src/enums.rs` | ✅ | — | Runtime semantics in §4.2.2 |
| `ToolResources.rate_limit_per_min` | `crates/atd-protocol/src/tool.rs` | 📜 | — | Field exists; runtime ignores. Issue [`resource-limits-not-enforced`](issues/2026-04-24-resource-limits-not-enforced.md) |
| `ToolResources.max_concurrent` | same | 📜 | — | Same — declared, not enforced |
| `ToolTrust.signature` | `crates/atd-protocol/src/tool.rs` | 📜 | — | Always `None`; issue [`security-trust-signature-unverified`](issues/2026-04-24-security-trust-signature-unverified.md) |
| `CapabilityToken` / UCAN types | — | 🚫 | — | See [§9.3](#9-non-goals-explicit) |
| Sanitize (`sanitize_tool_name` + `desanitize_tool_name`) | `crates/atd-sdk/src/sanitize.rs` | ✅ | 6 tests | Moved from bridge in SP-10 Task 1 |
| `ToolDefinition.output_schema` | `crates/atd-protocol/src/tool.rs` | ✅ | tool roundtrip tests | Was previously unlisted; surfaced in describe responses. |
| `ToolErrorDef` / `ToolDefinition.errors[]` | `crates/atd-protocol/src/tool.rs` | ✅ | `tests/error_def_roundtrip.rs` | Added in SP-protocol-schema. Built-ins ship `errors: vec![]`; per-tool catalogs are a future SP. |
| Python schema mirror | `python/src/atd_client/types.py` | ✅ | — | Hand-ported; drift-prone |
| **Machine-readable protocol schema** (`atd-protocol-schema.json`) | `/atd-protocol-schema.json` | ✅ | — | Generated by gen-schema bin (SP-protocol-schema). CI gates drift + metaschema validity. |

### 3.3 Target state

The schema layer reaches full v1 when:

1. A machine-readable `atd-protocol-schema.json` is generated from the Rust type definitions (via `schemars`), published in-repo, and validated against the [JSON Schema 2020-12 meta-schema](https://json-schema.org/draft/2020-12/schema). Tracked in issue [`schema-protocol-machine-readable-missing`](issues/2026-04-24-schema-protocol-machine-readable-missing.md).
2. CI verifies schema ↔ Rust type drift on every push.
3. External implementers (TypeScript, Go, Swift, ArkTS) consume the JSON schema directly instead of reading Rust source.

Beyond v1: the schema layer accumulates optional additions as new capabilities land — session types, capability-token types — always additive, always backward-compatible per the 0.x semver contract.

### 3.4 Gap → SP mapping

| Gap | Next SP | Severity |
|---|---|---|
| `ToolTrust.signature` unverified | Deferred to Phase 2 per [§9.4](#9-non-goals-explicit) | Low for v1 |
| `ToolResources.rate_limit_per_min` + `.max_concurrent` ignored | Covered in §5.5 (dispatch-enforcement problem, not schema) | — |

### 3.5 See also

- [`docs/protocol/wire-format.md`](protocol/wire-format.md) — the authoritative wire-level reference, including byte framing + full type tables
- [`docs/protocol/error-codes.md`](protocol/error-codes.md) — `AtdError` taxonomy + server-emitted error codes
- [`docs/issues/2026-04-24-schema-protocol-machine-readable-missing.md`](issues/2026-04-24-schema-protocol-machine-readable-missing.md)
- [`docs/issues/2026-04-24-security-trust-signature-unverified.md`](issues/2026-04-24-security-trust-signature-unverified.md)

## 4. Dispatch Layer

### 4.1 Core dispatch

`discover` / `describe` / `call` — the three APIs that the Client SDK exposes and the server responds to. Length-prefixed JSON over a Unix socket.

| Component | Source | Status | Tests | Notes |
|---|---|---|---|---|
| Wire framing (length-prefixed JSON, UTF-8) | `crates/atd-sdk/src/wire.rs` | ✅ | unit tests | See [`docs/protocol/wire-format.md`](protocol/wire-format.md) |
| `ClientMessage::ToolList` / `ToolSchema` / `RunTool` | `crates/atd-sdk/src/protocol.rs` | ✅ | roundtrip tests | |
| `Registry::dispatch()` (server-side routing) | `crates/atd-runtime/src/registry.rs` | ✅ | integration tests | Tool id → `Arc<dyn Tool>` |
| `Tool` trait + `CallContext` | `crates/atd-runtime/src/registry.rs` + `context.rs` | ✅ | — | — |
| `AtdClient::connect` / `discover` / `describe` / `call` / `ping` | `crates/atd-sdk/src/client.rs` | ✅ | 8 integration tests across workspace | — |
| Python mirror (`AtdClient` + `AtdClientSync`) | `python/src/atd_client/` | ✅ | 45 pytest tests | — |

### 4.2 Dispatch primitives (v1 — per SP-12 and follow-ups)

Beyond core dispatch, the server layers four additional primitives that make the "ATD = agent-era POSIX" claim concrete. The primitives compose; each call flows:

```
accept connection → Hello handshake (capability gate) → receive RunTool
  → registry.get(tool_id)
  → capability check (refuse if required_capabilities ⊄ granted)
  → tier-aware deadline resolution (timeout + max_output_bytes)
  → binding.invoke(args, &ctx)
  → result middleware pipeline
  → serialize response
```

#### 4.2.1 Binding abstraction

| Component | Source | Status | Tests | Notes |
|---|---|---|---|---|
| `Binding` trait | `crates/atd-runtime/src/binding.rs` | ✅ (SP-12) | SP-12 unit tests | |
| `NativeBinding` (delegates to `Tool` impl) | same | ✅ | — | Default for every registered built-in tool |
| `CliBinding` (spawn subprocess, map JSON args to argv, honor deadlines) | same | ✅ | SP-12 tests | `ref:external.uname` is the demo tool |
| `MCP` / `REST` / `AppFunction` bindings | — | 🚫 | — | See [§9.5](#9-non-goals-explicit). Trait designed to extend without breaking existing bindings. |

The binding trait's contract: given `args: serde_json::Value` and a `&CallContext`, return `Result<serde_json::Value, ToolCallError>`. The trait is the extension point for future invocation back-ends.

#### 4.2.2 Tier-aware deadlines

| Component | Source | Status | Notes |
|---|---|---|---|
| `Tier` type (`Hot` / `Warm` / `Cold`) | `crates/atd-runtime/src/tier.rs` | ✅ (SP-12) | Resolution of per-call deadline + max_output_bytes based on the tool's declared tier, overridable via `--tier-override` CLI flag |
| Default deadlines per tier | same | ✅ | `Hot` = 300ms, `Warm` = 5s, `Cold` = 60s at time of writing; verify against `crates/atd-runtime/src/tier.rs` before quoting |
| Tool-declared tier → dispatch honor | `crates/atd-runtime/src/registry.rs` | ✅ | Existing built-in tools: most ship as `Warm`; ref:external.uname (CliBinding demo) uses `Warm`. Re-classification PRs welcome. |
| Hot-tier warmup / Cold-tier lazy-load | — | 🚫 | See [§9.5](#9-non-goals-explicit). `Hot` / `Cold` today mean latency/cost class, not lifecycle policy. |

#### 4.2.3 Capability gate

| Component | Source | Status | Notes |
|---|---|---|---|
| `Hello` wire message (client → server on connect) | `crates/atd-sdk/src/protocol.rs` | ✅ (SP-12) | Client requests a subset of capabilities it plans to use |
| Server-side allow-list (`--grant-capability`) | `crates/atd-ref-server/src/main.rs` | ✅ | CLI-declared at startup: which capabilities the socket allows in total |
| `CapabilitySet` type + intersection logic | `crates/atd-runtime/src/capability.rs` | ✅ | — |
| Enforcement: refuse tools whose `required_capabilities` ⊄ granted | `crates/atd-runtime/src/registry.rs` | ✅ | Returns `AtdError::CapabilityDenied` with error code `1001` |
| Full UCAN-style tokens (delegation, revocation, signatures) | — | 🚫 | See [§9.3](#9-non-goals-explicit) |

The v1 capability gate is connection-scoped and allow-list-based. Token-based per-call authorization is deferred; the allow-list closes the 80% case of "limit what an adopter's socket exposes" without the cryptographic complexity of full UCAN.

#### 4.2.4 Result-middleware pipeline

| Component | Source | Status | Notes |
|---|---|---|---|
| `Middleware` trait | `crates/atd-runtime/src/middleware.rs` | ✅ (SP-12) | Runs on success before wire reply |
| `Pipeline` composition | same | ✅ | Composed at startup via repeated `--middleware` CLI flags |
| `RedactPathsMiddleware` (redact `$HOME` paths) | same | ✅ | Enabled by default; disable with `--middleware none` |
| Additional builtins: `pii_redact`, `injection_detect`, `image_meta_strip`, `trim`, `format` | — | ❌ | Tracked for future SPs — see §10 |
| Third-party middleware registration | — | ⚠️ | Trait is public; no crate boundary prevents a third party writing one. Discoverability is informal. |

The pipeline's contract: each middleware receives the prior result + metadata; can rewrite or reject; chain short-circuits on rejection. Built-ins ship as examples; operators compose per deployment.

#### 4.2.5 Sessions and cancellation

| Component | Status | Notes |
|---|---|---|
| `session.start` / `session.end` wire messages | ❌ | Not shipped. Issue [`dispatch-session-cancel-not-implemented`](issues/2026-04-24-dispatch-session-cancel-not-implemented.md) |
| `cancel(call_id)` | ❌ | Same issue |
| Call-id correlation (client-visible mid-flight) | ❌ | Requires design |

The session/cancel design surface is wide (state scope, wire mechanism, idempotency, concurrency). Deferring preserves the option to design against a concrete adopter's requirements rather than guessing.

#### 4.2.6 Ergonomic aliases

| Component | Status | Notes |
|---|---|---|
| SDK-side alias → canonical-id transform | ❌ | Planned SDK-only (client rewrites before sending). Server unaware. Rationale: v3 whitepaper Appendix J. |
| Alias DSL grammar | ❌ | Not yet specified |
| Built-in alias pack | ❌ | Not yet assembled |

### 4.3 Target state

v1 dispatch layer is complete when:
- Core dispatch ✅ (done)
- Binding abstraction ✅ (done — SP-12)
- Tier-aware deadlines ✅ (done — SP-12)
- Capability gate ✅ (done — SP-12)
- Result middleware pipeline ✅ with at least one built-in (done — SP-12)
- Ergonomic aliases (SDK-only) — planned
- Sessions / cancellation — **intentionally not in v1**; see [§9.2](#9-non-goals-explicit)

v2 dispatch extends with additional bindings, more built-in middleware, and potentially sessions if adopter demand materializes.

### 4.4 Gap → SP mapping

| Gap | Next SP | Status |
|---|---|---|
| Ergonomic aliases (SDK-side) | Proposed SP after SP-13 | ❌ |
| Additional built-in middleware (pii_redact, etc.) | Proposed SP | ❌ |
| Sessions / cancellation | — | 🚫 per [§9.2](#9-non-goals-explicit) |

### 4.5 See also

- [`docs/protocol/wire-format.md`](protocol/wire-format.md) — wire-level protocol, message definitions
- [`docs/protocol/error-codes.md`](protocol/error-codes.md) — error taxonomy
- [`docs/superpowers/specs/2026-04-25-sp12-canonical-dispatch.md`](superpowers/specs/2026-04-25-sp12-canonical-dispatch.md) — SP-12 design spec for the four primitives
- [`docs/issues/2026-04-24-dispatch-session-cancel-not-implemented.md`](issues/2026-04-24-dispatch-session-cancel-not-implemented.md)
- [`docs/issues/2026-04-24-dispatch-tier-hardcoded-warm.md`](issues/2026-04-24-dispatch-tier-hardcoded-warm.md) — **note:** this issue described state *before* SP-12; verify with `git log` whether it's still open
- [`docs/issues/2026-04-24-dispatch-binding-single-impl.md`](issues/2026-04-24-dispatch-binding-single-impl.md) — same note, pre-SP-12
- [`docs/issues/2026-04-24-dispatch-preferred-binding-ignored.md`](issues/2026-04-24-dispatch-preferred-binding-ignored.md)

## 5. Security Layer

### 5.1 Classification taxonomy

Every tool declares three classifications as part of its `ToolDefinition`. They are **descriptive metadata** — callers and human operators use them to reason about risk. They are NOT (in v1) enforcement mechanisms on their own; §5.2-§5.5 describe the actual runtime controls.

| Classification | Values | Declaring field |
|---|---|---|
| Safety level | `Read` / `Write` / `Financial` / `Privacy` / `Physical` / `Destructive` | `ToolSafety::level` |
| Visibility | `Read` / `Write` / `Dangerous` / `System` | `ToolVisibility` (top-level) |
| Trust level | `L1` / `L2Tested` / `L3Audited` | `ToolTrust::trust_level` |

Status: ✅ implemented in `crates/atd-protocol/`. Every built-in tool declares all three. LLM adapters surface `Visibility` and `SafetyLevel` to agent-framework tool pickers where supported.

Trust signatures (`ToolTrust::signature`) are declarative-only in v1 (`📜 informational`). Full signature verification is 🚫 non-goal — see [§9.4](#9-non-goals-explicit).

### 5.2 Per-tool runtime controls

Four specific runtime defenses run inside individual tools, not at the dispatch layer. Each defends a specific attack surface exposed by that tool's category.

| Control | Applies to | Source | Status |
|---|---|---|---|
| **SSRF guard** (loopback + RFC1918 + link-local + CGN + TEST-NET + 0.0.0.0/8 + IPv4-mapped-private; re-checked on every redirect hop) | `ref:web.fetch` | `crates/atd-tools-web/src/fetch.rs::check_ssrf` | ✅ (SP-5) |
| **Header allowlist** (Accept, Accept-Language, Referer, User-Agent only; Authorization + Cookie rejected with `InvalidArgs`) | `ref:web.fetch` | same file, `build_headers` | ✅ (SP-5) |
| **Must-read-before-edit** (mtime + size proof required in session before `fs.edit` will apply) | `ref:fs.edit` | `crates/atd-runtime/src/tracker.rs` (ReadTracker), used from `crates/atd-tools-fs/src/edit.rs` | ✅ (SP-2) |
| **SIGTERM → grace → SIGKILL subprocess timeout** | `ref:shell.exec` / `ref:shell.pwsh` | `crates/atd-tools-shell/src/shared.rs` | ✅ (SP-3) |
| **Request-arg schema validation** (serde + per-tool checks) | all tools | per-tool `call` impls | ✅ |

### 5.3 Capability tokens

v1's capability mechanism is the connection-scoped allow-list described in [§4.2.3](#423-capability-gate). Clients request capabilities via the `Hello` message; the server intersects with its `--grant-capability` allow-list; tools declaring `required_capabilities` outside the intersection are refused with `AtdError::CapabilityDenied` (code `1001`).

Cryptographically signed, delegatable UCAN-style tokens are 🚫 non-goal for v1; see [§9.3](#9-non-goals-explicit) for the deferral rationale and for the interim multi-tenant workaround (separate sockets per access tier).

| Component | Status | Notes |
|---|---|---|
| Connection-scoped allow-list | ✅ (SP-12) | See §4.2.3 |
| UCAN delegation tree | 🚫 | See [§9.3](#9-non-goals-explicit) |
| Token revocation store | 🚫 | Same |
| Per-call agent identity tracking | ✅ (SP-operability-v1) | `CallContext.caller_id` populated from `Hello.client_id`; see `crates/atd-runtime/src/context.rs`. Prerequisite for UCAN tokens (§9.3). |

### 5.4 Audit logging

| Component | Status | Notes |
|---|---|---|
| Structured per-call audit (tool_id, args_hash, outcome, duration, caller, tier, binding) | ✅ (SP-operability-v1) | `CallEvent` schema v1 emitted per call; see `crates/atd-runtime/src/audit.rs`. |
| `--audit-log <path>` CLI flag | ✅ (SP-operability-v1) | Enables `JsonLinesAuditSink` on the ref server. |
| `tracing` subscriber integration | ✅ (SP-operability-v1) | `JsonLinesAuditSink` writes JSONL events alongside the existing `tracing` subscriber. |

Audit is the observability spine for the other security layers. SP-operability-v1 landed this as the first security-adjacent SP post-SP-13; it unblocks meaningful multi-tenant authz work (§9.3 defers UCAN tokens, but audit no longer blocks that path).

### 5.5 Rate limiting and concurrency

| Component | Source | Status | Notes |
|---|---|---|---|
| `ToolResources.rate_limit_per_min` | `crates/atd-protocol/src/tool.rs` | 📜 | Declared on every tool; runtime ignores. Issue [`resource-limits-not-enforced`](issues/2026-04-24-resource-limits-not-enforced.md) |
| `ToolResources.max_concurrent` | same | ✅ (SP-operability-v1) | Enforced by per-tool `tokio::sync::Semaphore` in `Registry`. |
| Server-side semaphore wrapping per-tool invocation | `crates/atd-runtime/src/registry.rs` | ✅ (SP-operability-v1) | Refuses with `ERR_RATE_LIMITED` (1002, retryable) when permits are exhausted. |
| Server-side rate-limiter (token bucket via `governor`) | — | ❌ | Still planned; `rate_limit_per_min` remains declarative. |
| `AtdError::RateLimited` variant | `crates/atd-protocol/src/error.rs` | ✅ (SP-operability-v1) | Wire code 1002; see [`docs/protocol/error-codes.md`](protocol/error-codes.md). |

### 5.6 Dry-run consistency

| Component | Status | Notes |
|---|---|---|
| `CallOptions.dry_run` wire field | ✅ | Part of `RunTool` message. |
| Server-side short-circuit on `dry_run: true` | ✅ (SP-operability-v1) | Uniform across tools; see [`docs/protocol/dry-run-contract.md`](protocol/dry-run-contract.md). |
| `ToolSafety.dry_run` metadata correctness | ✅ (SP-operability-v1) | `shell.exec` / `shell.pwsh` corrected to `true` (they have side effects); field remains informational in v1. |
| Per-tool dry-run semantics delegation | 🚫 v1 | Deferred to a possible SP-operability-v2: route `dry_run: true` to tools declaring `ToolSafety.dry_run: true` for tool-specific previews. |

The v1 contract is a server-side short-circuit: the server returns a synthetic `tool_result` without invoking the tool. This closes the silent-execute footgun — `ref:shell.exec("rm -rf /", dry_run=true)` no longer runs the command.

### 5.7 Target state (v1)

v1 security posture closes when:

- Classifications ✅ (done)
- Per-tool runtime controls ✅ (done for current tool set)
- Connection-scoped capability gate ✅ (done — SP-12)
- Audit logging ✅ (landed — SP-operability-v1)
- Rate limiting + max_concurrent enforcement ✅ (landed — SP-operability-v1)
- Dry-run consistency ✅ (landed — SP-operability-v1)
- Per-call agent identity tracking ✅ (landed — SP-operability-v1)
- Full UCAN tokens 🚫 (Phase 2)
- Tool signature verification 🚫 (Phase 2)

### 5.8 Gap → SP mapping

| Gap | Next SP | Status |
|---|---|---|
| Audit logging | SP-operability-v1 | ✅ |
| Rate limiting + max_concurrent | SP-operability-v1 | ✅ |
| Dry-run consistency | SP-operability-v1 | ✅ |
| Per-call agent identity | SP-operability-v1 | ✅ |
| UCAN tokens | Phase 2 — see [§9.3](#9-non-goals-explicit) | 🚫 |
| Tool signature verification | Phase 2 — see [§9.4](#9-non-goals-explicit) | 🚫 |

### 5.9 See also

- [`docs/protocol/error-codes.md`](protocol/error-codes.md) — error taxonomy including `CapabilityDenied`
- [`docs/issues/2026-04-24-security-audit-logging-missing.md`](issues/2026-04-24-security-audit-logging-missing.md)
- [`docs/issues/2026-04-24-resource-limits-not-enforced.md`](issues/2026-04-24-resource-limits-not-enforced.md)
- [`docs/issues/2026-04-24-security-dry-run-inconsistent.md`](issues/2026-04-24-security-dry-run-inconsistent.md)
- [`docs/issues/2026-04-24-security-capability-tokens-deferred.md`](issues/2026-04-24-security-capability-tokens-deferred.md)
- [`docs/issues/2026-04-24-security-trust-signature-unverified.md`](issues/2026-04-24-security-trust-signature-unverified.md)

## 6. Extensibility

Four extension surfaces where ATD accepts code outside the reference implementation: new bindings, new tools, new middleware, and (v1+ planned) new aliases.

### 6.1 Binding extensibility

Adding a new binding back-end (for example: a gRPC binding, a WebAssembly binding, a REST binding):

| Step | Contract |
|---|---|
| 1. Implement `Binding` trait | Defined in `crates/atd-runtime/src/binding.rs`. Given `args: serde_json::Value` + `&CallContext`, return `Result<serde_json::Value, ToolCallError>`. Respect `ctx.deadline`. |
| 2. Register an instance | `Registry::register_binding("grpc", Arc::new(GrpcBinding::new(...)))` at startup |
| 3. Tools declare `bindings: [ToolBinding { protocol: BindingProtocol::..., config: ... }, ...]` | One tool may have multiple bindings; dispatch picks one (currently: first) |

Current bindings:

| Binding | Protocol enum | Status |
|---|---|---|
| `NativeBinding` | `BindingProtocol::Cli` (historical name retained) | ✅ |
| `CliBinding` (subprocess) | `BindingProtocol::Cli` | ✅ |
| MCP binding | `BindingProtocol::Mcp` | 🚫 ([§9.5](#9-non-goals-explicit)) |
| REST binding | `BindingProtocol::Rest` | 🚫 ([§9.5](#9-non-goals-explicit)) |
| AppFunction binding | `BindingProtocol::AppFunction` | 🚫 ([§9.5](#9-non-goals-explicit)) |
| Distributed binding | — | 🚫 ([§9.1](#9-non-goals-explicit)) |

**Runtime-routing note:** v1 always routes to the first (and usually only) binding a tool declares. `CallOptions::preferred_binding` is currently dropped; issue [`dispatch-preferred-binding-ignored`](issues/2026-04-24-dispatch-preferred-binding-ignored.md). If real multi-binding tools land, the dispatcher's selection logic needs a small upgrade (pick preferred if available; else first).

### 6.2 Tool extensibility

Adding a new tool to the reference server (or to a third-party ATD server):

| Step | Contract |
|---|---|
| 1. Implement `Tool` trait | Defined in `crates/atd-runtime/src/registry.rs`. Return `ToolDefinition` in `definition()`; implement `call(args, ctx)` returning `Result<serde_json::Value, ToolCallError>`. |
| 2. Register | `registry.register(Arc::new(MyTool::new()))` in `builtin.rs` or equivalent |
| 3. Declare required capabilities, safety, tier, bindings | Via the returned `ToolDefinition` |

Tools outside this repo can implement the same trait and register in their own binary that links `atd-runtime`. The reference server is not required to host all tools; any crate can host a `Registry` and serve an ATD socket.

Canonical examples: `crates/atd-tools-{echo,fs,shell,web}/`.

### 6.3 Middleware extensibility

Adding a new result-middleware:

| Step | Contract |
|---|---|
| 1. Implement `Middleware` trait | Defined in `crates/atd-runtime/src/middleware.rs`. Given the prior result + metadata, return a (possibly rewritten) result or an error to short-circuit the chain. |
| 2. Register | `Pipeline::from_flags(["my_middleware", ...])` at startup, or programmatically via `Pipeline::add(Arc::new(MyMiddleware))` |
| 3. Enable per deployment | CLI: repeated `--middleware <name>` flags compose a chain in declaration order |

Middleware receives the `ToolResult::Success` case only; errors flow past untouched. Middleware can transform `success.data`, strip metadata fields, or reject with an error (rare; usually used for policy enforcement like "never return paths starting with `/etc/`").

Built-ins so far: `RedactPathsMiddleware`. Proposed additions — see §10 roadmap.

### 6.4 Ergonomic aliases (SDK-only)

Planned, not yet shipped.

| Component | Status | Target |
|---|---|---|
| SDK-side alias table (e.g., `current_time` → `ref:system.time`) | ❌ | SDK exposes a registration API |
| Alias → canonical id resolution before `call()` | ❌ | Transform happens in the client before serialization; server sees canonical id only |
| Built-in alias pack | ❌ | One pack per high-traffic domain (fs, shell, web) |

**Scope discipline:** the alias mechanism is SDK-side; the server does not participate. This mirrors v3 Appendix J's recommended approach and avoids protocol-level ambiguity.

### 6.5 Extension-point checklist

A third-party implementer asking "what can I extend without forking the reference server?" — the answer for v1:

| You want to... | Extension surface | Requires fork of ref-server? |
|---|---|---|
| Add a new tool (any domain) | `Tool` trait implementation | No |
| Add a new binding back-end | `Binding` trait implementation | No |
| Add a new result middleware | `Middleware` trait implementation | No |
| Add an SDK-side alias | SDK's alias-registration API (when landed) | No |
| Change the wire format | — | Yes (not an extension point) |
| Change the error taxonomy | — | Yes |
| Add a new `ToolTier` variant | — | Yes |

### 6.6 See also

- [`crates/atd-runtime/src/binding.rs`](../crates/atd-runtime/src/binding.rs) — `Binding` trait definition
- [`crates/atd-runtime/src/middleware.rs`](../crates/atd-runtime/src/middleware.rs) — `Middleware` trait definition
- [`crates/atd-runtime/src/registry.rs`](../crates/atd-runtime/src/registry.rs) — `Tool` trait and registration
- [`docs/superpowers/specs/2026-04-25-sp12-canonical-dispatch.md`](superpowers/specs/2026-04-25-sp12-canonical-dispatch.md) — origin of the `Binding` / `Middleware` traits

## 7. Skills Layer (adjacent)

The Skills layer (SKILL.md files + `atd-tools:` dependency declarations + progressive-disclosure skill bodies) is drawn as a stack layer in the ATD v3 brief. From a protocol standpoint, Skills is an **upstream consumer** of ATD — not part of ATD itself.

### 7.1 Division of concern

| Concern | Owner |
|---|---|
| SKILL.md authoring, validation, install | Skills runtime (Anthropic Skills, OpenClaw ClawHub, third parties) |
| Progressive disclosure into agent context | Skills runtime |
| `atd-tools:` dependency declarations | SKILL.md format (owned by Skills spec); ATD's contribution is stable tool IDs |
| Invoking ATD tools from a skill body | Skills runtime calls ATD client (`atd_client.call(...)` in Python, `atd_sdk::call(...)` in Rust) like any other agent |
| The `discover` / `describe` / `call` API the skill body relies on | ATD (this project) |

### 7.2 ATD's commitments toward Skills

- Stable `discover` / `describe` / `call` semantics
- Stable `AtdError` taxonomy
- Stable tool-id conventions (namespace + dot segments; sanitization rules documented in §3)

### 7.3 ATD's non-commitments

- ATD does not parse SKILL.md
- ATD does not manage skill installation
- ATD does not store skill state across calls

### 7.4 Two consumption patterns

1. **Direct agent → ATD** (one-shot) — agent LLM decides `tool_id` + `args`; see [§2.4 Example A](#24-two-pager-call-graph-examples)
2. **Skill body → ATD** (multi-step, orchestrated) — skill runtime loads a skill body into agent context; body calls ATD tools in sequence; see [§2.4 Example B](#24-two-pager-call-graph-examples)

Both patterns traverse identical ATD dispatch. Skills adds orchestration on top; it does not modify dispatch.

### 7.5 Future: SKILL.md generation from ATD tools

A future SP (proposed) adds `atd skills --target skillmd` to generate SKILL.md stubs from registered tools — enabling the 26+ SKILL.md-compatible platforms (Claude Code, Cursor, OpenClaw, VS Code Copilot, …) to consume ATD-hosted tool catalogs. This is an ATD-side generator; the Skills runtime side is unchanged.

### 7.6 See also

- [`docs/whitepaper/atd-v3-skills-architecture-brief.md`](whitepaper/atd-v3-skills-architecture-brief.md) — the v3 brief defining the Skills layer positioning
- [`docs/integrations/openclaw.md`](integrations/openclaw.md) — interim MCP-bridge workaround until SKILL.md generation lands

## 8. Component / crate map

### 8.1 Principle

A clean logical decomposition of the reference implementation has three core components + satellites:

- **Protocol** (the spec): types, wire format, sanitization rules. Shared between SDK and runtime; depends on neither.
- **SDK** (the client side): how agents and framework integrations call ATD. Depends on Protocol.
- **Runtime** (the server side): how tools get invoked. Depends on Protocol, not on SDK.
- **Tools**: concrete tool implementations. Logically separate from Runtime; in v1 they share a crate for convenience.
- **Bridges**: protocol translators (MCP ⇄ ATD is the only one shipped). Consume SDK; speak an external protocol outward.
- **Binaries**: end-user artifacts (`atd` CLI, `atd-ref-server` binary). Thin wrappers.

### 8.2 Current → target mapping

The current crate layout (post-`SP-refactor-v1`) cleanly separates each logical component into its own crate. The table below names each logical component and its current home.

| Logical component | Current crate | Status | Notes |
|---|---|---|---|
| **Protocol** (types, wire, sanitize) | `atd-protocol` | ✅ | Consolidated in SP-refactor-v1. |
| **Rust SDK** | `atd-sdk` | ✅ | Renamed from `atd-client` in SP-refactor-v1. Adapters feature-gated. |
| **Python SDK** | `python/src/atd_client/` | ⚠️ pending Python-mirror SP | Still named `atd_client`; rename deferred. |
| **Runtime** (`Tool` trait, `Registry`, dispatch, binding, middleware, tier, capability) | `atd-runtime` | ✅ | Extracted from `atd-ref-server` in SP-refactor-v1. Transport-agnostic. |
| **Server transport** (Unix-socket listener, accept loop, per-connection task) | `atd-server` | ✅ | Extracted from `atd-ref-server` in SP-listener-extract (triggered by `healthkit_cli` first-vendor-server signal). Pair with `atd-runtime` to host any ATD-speaking server. |
| **Built-in tools** (echo, fs, shell, web) | `atd-tools-echo`, `atd-tools-fs`, `atd-tools-shell`, `atd-tools-web` | ✅ | Split per-domain in SP-refactor-v1. |
| **MCP bridge** | `atd-mcp-bridge` | ✅ | Binary |
| **CLI** | `atd-cli` | ✅ | Binary — `atd` command |
| **Ref-server binary** | `atd-ref-server` (binary name `atd-ref-server`) | ✅ | Slim wiring of `atd-server` + `atd-runtime` + `atd-tools-*` into the reference / demo binary. |
| **Examples** | `examples/` (not published) | ✅ | |
| **Conformance suite** (future) | not yet | ❌ | Future SP (SP-8) |

### 8.3 Dependency graph (current)

```
atd-protocol
   ▲
   ├── atd-sdk (client + adapters)
   │       ▲
   │       ├── atd-mcp-bridge
   │       └── atd-cli
   │
   └── atd-runtime (Tool/Binding/Middleware/Registry/dispatch — transport-agnostic)
           ▲
           ├── atd-tools-echo
           ├── atd-tools-fs
           ├── atd-tools-shell
           ├── atd-tools-web
           └── atd-server (Unix-socket listener + connection task)
                   ▲
                   └── atd-ref-server (slim binary: wires runtime + tools + server)
                       │
                       └── + future vendor servers (e.g. healthkit-server)
                           depend directly on atd-runtime + atd-server,
                           skip atd-ref-server entirely
```

Python SDK (`python/src/atd_client/`) mirrors `atd-protocol` + `atd-sdk` as a standalone Python package with its own sanitize + adapters. Python rename to `atd_sdk` is a deferred SP.

### 8.4 Refactor history

Target layout landed in `SP-refactor-v1` (tag `sp-refactor-v1`). Pre-refactor
state is available at tag `pre-refactor-v1` if someone needs the historical
crate-lumping for comparison. The refactor was mechanical: zero behavior
change, zero wire-format change, binary names (`atd`, `atd-ref-server`,
`atd-mcp-bridge`) unchanged.

### 8.5 Refactor triggers (resolved)

The refactor triggers discussed in prior doc versions have been resolved; see §8.4 above for the landing record.

### 8.6 See also

- [`docs/superpowers/specs/2026-04-24-crate-refactor-design.md`](superpowers/specs/2026-04-24-crate-refactor-design.md) — design spec for SP-refactor-v1
- [`docs/design.md`](design.md) — the original Phase 0 spec that established the pre-refactor crate names

## 9. Non-goals (explicit)

These are intentional exclusions for v1.x. Each entry states: what the non-goal is, why it's out of scope, and what event would re-open it.

### 9.1 Multi-device routing

**What:** The v3 whitepaper's device-class routing (phone / watch / earbuds / tablet / pc / car / tv) with per-device-class binding selection.

**Why deferred:** Requires a device registry, device-availability probing, binding fallback logic, and hardware to validate against. No adopter yet depends on this in the reference implementation.

**Re-opens when:** A device-vendor adopter (HarmonyOS, Apple, Google) commits to implementing an ATD server exposing device-scoped tools.

### 9.2 Distributed sessions (migrate / fork / handoff)

**What:** Cross-device session migration, forking, handoff as described in v3 §2.6.

**Why deferred:** Strictly depends on multi-device routing (§9.1). Without multiple devices to route between, the distributed-session primitives have no use case.

**Re-opens when:** Multi-device routing lands AND an adopter has a cross-device agent use case (e.g., start on watch, finish on phone).

### 9.3 Full UCAN capability tokens

**What:** Cryptographically signed, delegation-tree-based, revocable capability tokens per the UCAN spec.

**Why deferred:** The v1 connection-scoped allow-list (§4.2.3) closes the single-tenant use case. Full UCAN's complexity is justified only when multi-tenant deployments with agent-to-agent delegation actually exist. Implementing UCAN before the use case risks wrong primitives.

**Interim workaround:** Run multiple ATD sockets per access tier (dev / prod / read-only). Each socket grants a different `--grant-capability` allow-list. Documented in [`docs/integrations/overview.md`](integrations/overview.md).

**Re-opens when:** A multi-tenant deployment needs per-agent authorization finer than per-socket.

### 9.4 Tool signature verification

**What:** Cryptographic signatures on `ToolDefinition.publisher` + `trust_level` with verification at discovery time.

**Why deferred:** Requires a signing ceremony, a key distribution story, and at least one non-reference publisher. None exist. See [§5.1](#51-classification-taxonomy) + issue [`security-trust-signature-unverified`](issues/2026-04-24-security-trust-signature-unverified.md).

**Re-opens when:** A tool marketplace with multiple publishers exists AND an adopter demands verification. Likely sigstore-based.

### 9.5 REST, AppFunction, and distributed bindings

**What:** Additional binding back-ends named by `BindingProtocol` but not yet implemented.

**Why deferred:** `NativeBinding` + `CliBinding` cover all current tools. REST would enable cloud-hosted tools; AppFunction would enable mobile-native tools; distributed would enable cross-machine tools. Each requires a real adopter to inform the contract.

**Re-opens when:** A concrete tool (or tool author) surfaces a binding need and is willing to co-design the contract.

### 9.6 Native Skills-layer support

**What:** ATD becoming aware of SKILL.md / progressive disclosure / skill state management.

**Why deferred:** Wrong layer. Skills is an orchestrator above ATD ([§7](#7-skills-layer-adjacent)). Merging them would couple two projects with different adopters and different evolution cadences.

**Re-opens when:** Never, likely. ATD and Skills are designed to coexist, not merge.

### 9.7 HTTP transport for the wire protocol

**What:** Running the ATD wire protocol over HTTP/JSON (as opposed to Unix socket + stdio).

**Why deferred:** HTTP is a Phase 2 goal per `docs/design.md`. Requires: routing / auth / TLS / path structure / streaming decisions. No current adopter needs it; MCP bridge covers most remote-reach cases.

**Re-opens when:** A cloud-hosted ATD deployment surfaces a real need. Meanwhile, wrap ATD in an HTTP service if needed — the Unix socket is still beneath.

## 10. Evolution path

A directional roadmap — **not a commitment calendar**. Each row states the item, the layer it touches, its status (from the status vocabulary), the proposed or expected SP number, a rough quarter, and the gating condition.

| Item | Layer | Status | Target SP | Rough window | Gate |
|---|---|---|---|---|---|
| Audit logging (structured per-call events) | Security | ✅ | SP-operability-v1 | 2026-04-24 | Landed; JsonLinesAuditSink via --audit-log flag; CallEvent schema v1. |
| Rate limiting + `max_concurrent` enforcement | Security | ✅ | SP-operability-v1 | 2026-04-24 | Landed; per-tool tokio Semaphore in Registry; ERR_RATE_LIMITED (1002) wire code. |
| Dry-run consistency across tools | Security | ✅ | SP-operability-v1 | 2026-04-24 | Landed; server-side short-circuit documented in docs/protocol/dry-run-contract.md; shell.exec/pwsh ToolSafety.dry_run corrected to true. |
| Per-call agent identity tracking | Security | ✅ | SP-operability-v1 | 2026-04-24 | Landed; CallContext.caller_id populated from Hello.client_id; prerequisite for UCAN tokens (arch §9.3). |
| Machine-readable `atd-protocol-schema.json` | Schema | ✅ | SP-protocol-schema | 2026-04-25 | Landed; gen-schema bin + CI drift check; see SP-protocol-schema. |
| Conformance suite (sanitize / wire / behavior categories) | Cross-cutting | ✅ | SP-8 | 2026-04-24 | Landed; `atd-conformance` crate with 32 fixtures + `run_conformance` API + CLI binary; self-conformance integration test green. |
| Conformance: capability-denied gated tool | Cross-cutting | ✅ | SP-8.1 | 2026-04-24 | Landed; `ref:conformance.denied_op` returns ERR_CAPABILITY_DENIED (1001); fixture restored to behavior category. |
| Conformance: rate-limit fixture (`saturate_op`) | Cross-cutting | ✅ | SP-8.2 | 2026-04-25 | Landed; `ref:conformance.saturate_op` exercises ERR_RATE_LIMITED (1002) wire path. |
| Ergonomic aliases DSL (SDK-only) | Dispatch | ❌ | proposed SP | Q3 2026 | No strict gate; low priority |
| Additional built-in middleware (pii_redact, injection_detect, image_meta_strip) | Dispatch | ❌ | proposed SP | Q3 2026 | No strict gate |
| Sessions + cancellation | Dispatch | 🚫 v1 | — | undecided | Need a concrete adopter use case |
| TypeScript SDK | SDK | ❌ | TBD | undecided | Waiting for a concrete TS adopter |
| Crate refactor (atd-protocol / atd-sdk / atd-runtime / atd-tools-*) | Cross-cutting | ✅ | SP-refactor-v1 | 2026-04-24 | Landed; see §8.4 |
| Extract socket listener from atd-ref-server into reusable `atd-server` crate | Dispatch (transport) | ✅ | SP-listener-extract | 2026-04-25 | Landed; Server/ServerConfig/connection moved to crates/atd-server. atd-ref-server reduced to binary + built-in tool wiring. Triggered by `healthkit_cli` first-vendor-server signal — vendors can now depend on atd-runtime + atd-server without pulling atd-ref-server's built-in tools. |
| MCP server-side binding (`BindingProtocol::Mcp`) | Dispatch (binding) | 🚫 v1 | — | undecided | Adopter with an MCP-native tool set |
| REST binding | Dispatch (binding) | 🚫 v1 | — | undecided | Cloud-hosted tool with REST API |
| AppFunction binding | Dispatch (binding) | 🚫 v1 | — | undecided | Mobile-vendor adopter |
| Full UCAN capability tokens | Security | 🚫 v1 | — | Phase 2 | Multi-tenant adopter |
| Tool signature verification | Security | 🚫 v1 | — | Phase 2 | Multi-publisher marketplace |
| Multi-device routing | Dispatch | 🚫 v1 | — | Phase 2 | Device-vendor adopter |
| Distributed sessions | Dispatch | 🚫 v1 | — | Phase 2 | Multi-device lands first |
| Native Skills-layer integration | Cross-cutting | 🚫 forever | — | — | Intentionally separate project |
| HTTP transport | Dispatch | 🚫 v1 | — | Phase 2 | Cloud-hosted ATD adopter |

### 10.1 Update cadence

This document is maintained by the atd-mvp maintainers (see `CODEOWNERS`). Expected cadence:

- **Per major SP:** The SP's plan includes a step to update this document's relevant status tables.
- **Per minor SP:** Update only if status glyphs change or new issues are filed.
- **Quarterly:** Re-read §9 (non-goals) and §10 (roadmap) for stale entries; re-open or close as needed.

### 10.2 When to amend this document vs file an issue

- **File an issue** in `docs/issues/` for a specific gap that needs tracking and fixing.
- **Amend this document** when: (a) a gap is closed (update status glyph, remove the issue link), (b) a new non-goal is added or removed, (c) the layer model itself changes (rare — would signal a semver-breaking moment), or (d) a new layer / component / extension point is added.

### 10.3 Versioning this document

This document is `v1.0`. A `v2.0` version would be warranted when:

- A non-goal category moves out of 🚫 (e.g., multi-device routing lands)
- The layer count changes (e.g., a new layer is inserted)
- The extension-point contracts change incompatibly

Minor edits (status updates, new entries in §10) do NOT require a version bump. They're tracked by `git log`.
