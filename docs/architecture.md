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
| Schema | [§3](#3-schema-layer) | `crates/atd-types/` | mostly ✅; machine-readable schema file ❌ |
| Dispatch core (discover/describe/call) | [§4.1](#41-core-dispatch) | `crates/atd-ref-server/` + `crates/atd-client/` | ✅ |
| Dispatch · binding abstraction | [§4.2.1](#421-binding-abstraction) | `crates/atd-ref-server/src/binding.rs` | ✅ (SP-12) |
| Dispatch · tier-aware deadlines | [§4.2.2](#422-tier-aware-deadlines) | `crates/atd-ref-server/src/tier.rs` | ✅ (SP-12) |
| Dispatch · capability gate | [§4.2.3](#423-capability-gate) | `crates/atd-ref-server/src/capability.rs` | ✅ (SP-12) |
| Dispatch · result-middleware pipeline | [§4.2.4](#424-result-middleware-pipeline) | `crates/atd-ref-server/src/middleware.rs` | ✅ (SP-12, one built-in) |
| Dispatch · sessions & cancellation | [§4.2.5](#425-sessions-and-cancellation) | — | ❌ deferred by design |
| Dispatch · ergonomic aliases | [§4.2.6](#426-ergonomic-aliases) | — | ❌ SDK-only; planned |
| Security · classifications | [§5.1](#51-classification-taxonomy) | `crates/atd-types/` | ✅ |
| Security · per-tool runtime controls | [§5.2](#52-per-tool-runtime-controls) | per-tool files in `crates/atd-ref-server/src/tools/` | ✅ where applicable |
| Security · capability tokens | [§5.3](#53-capability-tokens) | `crates/atd-ref-server/src/capability.rs` | ✅ v1 (allow-list); full HMAC/UCAN 🚫 |
| Security · audit logging | [§5.4](#54-audit-logging) | — | ❌ |
| Security · rate limiting & concurrency | [§5.5](#55-rate-limiting-and-concurrency) | — | ❌ |
| Security · dry-run consistency | [§5.6](#56-dry-run-consistency) | per-tool files | ⚠️ |
| Skills (adjacent) | [§7](#7-skills-layer-adjacent) | — | out of scope for ATD core |

### 2.4 Two-pager call graph examples

**Example A: direct agent → ATD (one-shot):**

```
agent.llm
    ↓ decides tool_id = "ref:shell.exec" and args = {"command": "uname -s"}
atd_client::AtdClient::call(tool_id, args, CallOptions { .. })
    ↓ writes length-prefixed JSON over Unix socket
atd-ref-server accepts connection
    ↓ dispatcher: capability gate → registry lookup → tier deadline → binding → tool
NativeBinding::invoke(&args)
    ↓ executes
ToolResult { success: true, data: { stdout: "Linux\n", exit_code: 0, .. } }
    ↓ result-middleware pipeline (RedactPathsMiddleware rewrites any $HOME paths)
    ↓ serialized, length-prefixed JSON back
atd_client delivers ToolResult to agent
```

**Example B: Skills runtime → ATD (multi-step):**

```
skills_runtime loads skill @acme/morning-briefing per user intent
    ↓ install-time: runtime verified atd-tools: required are discoverable on the socket
skill body executed in agent context
    ↓ body step 1 says: call hms:health.sleep.get for yesterday
atd_client::AtdClient::call("hms:health.sleep.get", { "date": "2026-04-23" }, ..)
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
| `ToolSummary` (incl. `input_schema`) | `crates/atd-types/src/summary.rs` | ✅ | types roundtrip tests | `input_schema` added in SP-10 Task 2.5 so LLM adapters emit real schemas |
| `ToolDefinition` + sub-structs | `crates/atd-types/src/tool.rs` | ✅ | roundtrip tests | — |
| `ToolResult` (Success/Error variants) | `crates/atd-types/src/result.rs` | ✅ | — | |
| `AtdError` (9 variants + `is_retryable` + `suggest_fix`) | `crates/atd-types/src/error.rs` | ✅ | — | See [`docs/protocol/error-codes.md`](protocol/error-codes.md) for the reference table |
| `SafetyLevel` / `ToolVisibility` / `TrustLevel` / `BindingProtocol` | `crates/atd-types/src/enums.rs` | ✅ | — | |
| `ToolTier` enum (`Hot` / `Warm` / `Cold`) | `crates/atd-types/src/enums.rs` | ✅ | — | Runtime semantics in §4.2.2 |
| `ToolResources.rate_limit_per_min` | `crates/atd-types/src/tool.rs` | 📜 | — | Field exists; runtime ignores. Issue [`resource-limits-not-enforced`](issues/2026-04-24-resource-limits-not-enforced.md) |
| `ToolResources.max_concurrent` | same | 📜 | — | Same — declared, not enforced |
| `ToolTrust.signature` | `crates/atd-types/src/tool.rs` | 📜 | — | Always `None`; issue [`security-trust-signature-unverified`](issues/2026-04-24-security-trust-signature-unverified.md) |
| `CapabilityToken` / UCAN types | — | 🚫 | — | See [§9.3](#9-non-goals-explicit) |
| Sanitize (`sanitize_tool_name` + `desanitize_tool_name`) | `crates/atd-client/src/sanitize.rs` | ✅ | 6 tests | Moved from bridge in SP-10 Task 1 |
| Python schema mirror | `python/src/atd_client/types.py` | ✅ | — | Hand-ported; drift-prone |
| **Machine-readable protocol schema** (`atd-protocol-schema.json`) | — | ❌ | — | Issue [`schema-protocol-machine-readable-missing`](issues/2026-04-24-schema-protocol-machine-readable-missing.md) |

### 3.3 Target state

The schema layer reaches full v1 when:

1. A machine-readable `atd-protocol-schema.json` is generated from the Rust type definitions (via `schemars`), published in-repo, and validated against the [JSON Schema 2020-12 meta-schema](https://json-schema.org/draft/2020-12/schema). Tracked in issue [`schema-protocol-machine-readable-missing`](issues/2026-04-24-schema-protocol-machine-readable-missing.md).
2. CI verifies schema ↔ Rust type drift on every push.
3. External implementers (TypeScript, Go, Swift, ArkTS) consume the JSON schema directly instead of reading Rust source.

Beyond v1: the schema layer accumulates optional additions as new capabilities land — session types, capability-token types — always additive, always backward-compatible per the 0.x semver contract.

### 3.4 Gap → SP mapping

| Gap | Next SP | Severity |
|---|---|---|
| No machine-readable schema | Proposed SP: schema generation via `schemars` | Medium — blocks non-Rust/Python SDK authoring |
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
| Wire framing (length-prefixed JSON, UTF-8) | `crates/atd-client/src/wire.rs` | ✅ | unit tests | See [`docs/protocol/wire-format.md`](protocol/wire-format.md) |
| `ClientMessage::ToolList` / `ToolSchema` / `RunTool` | `crates/atd-client/src/protocol.rs` | ✅ | roundtrip tests | |
| `Registry::dispatch()` (server-side routing) | `crates/atd-ref-server/src/registry.rs` | ✅ | integration tests | Tool id → `Arc<dyn Tool>` |
| `Tool` trait + `CallContext` | `crates/atd-ref-server/src/registry.rs` + `context.rs` | ✅ | — | — |
| `AtdClient::connect` / `discover` / `describe` / `call` / `ping` | `crates/atd-client/src/client.rs` | ✅ | 8 integration tests across workspace | — |
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
| `Binding` trait | `crates/atd-ref-server/src/binding.rs` | ✅ (SP-12) | SP-12 unit tests | |
| `NativeBinding` (delegates to `Tool` impl) | same | ✅ | — | Default for every registered built-in tool |
| `CliBinding` (spawn subprocess, map JSON args to argv, honor deadlines) | same | ✅ | SP-12 tests | `ref:external.uname` is the demo tool |
| `MCP` / `REST` / `AppFunction` bindings | — | 🚫 | — | See [§9.5](#9-non-goals-explicit). Trait designed to extend without breaking existing bindings. |

The binding trait's contract: given `args: serde_json::Value` and a `&CallContext`, return `Result<serde_json::Value, ToolCallError>`. The trait is the extension point for future invocation back-ends.

#### 4.2.2 Tier-aware deadlines

| Component | Source | Status | Notes |
|---|---|---|---|
| `Tier` type (`Hot` / `Warm` / `Cold`) | `crates/atd-ref-server/src/tier.rs` | ✅ (SP-12) | Resolution of per-call deadline + max_output_bytes based on the tool's declared tier, overridable via `--tier-override` CLI flag |
| Default deadlines per tier | same | ✅ | `Hot` = 300ms, `Warm` = 5s, `Cold` = 60s at time of writing; verify against `crates/atd-ref-server/src/tier.rs` before quoting |
| Tool-declared tier → dispatch honor | `crates/atd-ref-server/src/registry.rs` | ✅ | Existing built-in tools: most ship as `Warm`; ref:external.uname (CliBinding demo) uses `Warm`. Re-classification PRs welcome. |
| Hot-tier warmup / Cold-tier lazy-load | — | 🚫 | See [§9.5](#9-non-goals-explicit). `Hot` / `Cold` today mean latency/cost class, not lifecycle policy. |

#### 4.2.3 Capability gate

| Component | Source | Status | Notes |
|---|---|---|---|
| `Hello` wire message (client → server on connect) | `crates/atd-client/src/protocol.rs` | ✅ (SP-12) | Client requests a subset of capabilities it plans to use |
| Server-side allow-list (`--grant-capability`) | `crates/atd-ref-server/src/main.rs` | ✅ | CLI-declared at startup: which capabilities the socket allows in total |
| `CapabilitySet` type + intersection logic | `crates/atd-ref-server/src/capability.rs` | ✅ | — |
| Enforcement: refuse tools whose `required_capabilities` ⊄ granted | `crates/atd-ref-server/src/registry.rs` | ✅ | Returns `AtdError::CapabilityDenied` with error code `1001` |
| Full UCAN-style tokens (delegation, revocation, signatures) | — | 🚫 | See [§9.3](#9-non-goals-explicit) |

The v1 capability gate is connection-scoped and allow-list-based. Token-based per-call authorization is deferred; the allow-list closes the 80% case of "limit what an adopter's socket exposes" without the cryptographic complexity of full UCAN.

#### 4.2.4 Result-middleware pipeline

| Component | Source | Status | Notes |
|---|---|---|---|
| `Middleware` trait | `crates/atd-ref-server/src/middleware.rs` | ✅ (SP-12) | Runs on success before wire reply |
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
