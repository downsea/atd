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
