# ATD Architecture v1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce `docs/architecture.md` — a single reconciled-but-authoritative architecture reference for the ATD reference implementation, 1300-1800 lines, per the approved spec at `docs/superpowers/specs/2026-04-24-atd-architecture-v1-design.md`.

**Architecture:** Five sequential tasks writing one file in four bite-sized content passes (narrative + schema/dispatch + security/extensibility + skills/crate map/non-goals/evolution) plus a fifth wrap-up task for ancillary cross-link updates and the release tag. Each task independently re-verifies the current code state before writing ✅/⚠️/❌ rows — the subagents do not trust the spec's snapshot.

**Tech Stack:** Markdown only. No code, no tests. Inspection tools: `git log`, `grep`, `ls`, `cargo tree`.

**Spec:** `docs/superpowers/specs/2026-04-24-atd-architecture-v1-design.md`

**Scope boundary:**
- **In:** one new file (`docs/architecture.md`); four ancillary doc updates (README, design.md header note, wire-format.md cross-link, integrations/overview.md cross-link); one git tag.
- **Out:** code changes; new tests; refactor planning; whitepaper edits.

**Prerequisites:**
- `sp12-canonical-dispatch` landed on master (or equivalent commit with the 5 dispatch primitives — Hello/capability gate, tier, binding, middleware, CliBinding demo). If no tag exists, the plan references the latest commit on each SP-12 primitive.
- 10 issues in `docs/issues/` committed (verify via `ls docs/issues/`).

**Exit criteria:**
1. `docs/architecture.md` exists, 1300-1800 lines, 10 H2 sections in spec order.
2. Every `✅`/`⚠️`/`❌`/`🔨`/`🚫`/`📜` status row cites a concrete source (file path, issue file, or §9 non-goal anchor).
3. Spec §6's 10 resolved decisions all appear authoritatively in the doc's §6 section.
4. `grep -E 'TBD|TODO|placeholder' docs/architecture.md` returns empty EXCEPT the legitimate TBD in §10's Target-SP column for session/cancel (one row).
5. `grep -E '^#{0,6}.*ANOS' docs/architecture.md` returns no assertion-form references (historical "previously depended on ANOS" context is acceptable; current "ATD depends on ANOS" claims are not).
6. README, design.md, wire-format.md, integrations/overview.md each updated per §9.2 of the spec.
7. `cargo test --workspace --all-targets` still passes (sanity — docs shouldn't affect tests).
8. Annotated tag `sp13-architecture-doc` created.
9. At least one internal link from the doc to each of: `wire-format.md`, `error-codes.md`, a `docs/issues/` file, a `docs/superpowers/specs/` file.

---

## File Structure

```
/home/nan/proj/atd-mvp/
├── docs/
│   ├── architecture.md                (NEW — Tasks 1-4)
│   ├── design.md                       (MODIFY — Task 5; add supersede note)
│   ├── protocol/
│   │   └── wire-format.md              (MODIFY — Task 5; cross-link to arch)
│   └── integrations/
│       └── overview.md                 (MODIFY — Task 5; cross-link to arch)
└── README.md                           (MODIFY — Task 5; add Architecture row)
```

---

## Shared conventions (applies to every task)

Each content task writes **additional content appended** to `docs/architecture.md` — the file grows monotonically across Tasks 1-4. Do not rewrite earlier sections; only append. This means each task's first action after verification is reading the file's current end to confirm the append position.

**Status vocabulary — use exactly these six glyphs and words:**

```
✅ implemented      — code + tests + docs present
⚠️ partial          — code exists; runtime skeletal OR tests thin OR aspect missing
🔨 in-progress      — actively being landed at write time (rare; use sparingly)
❌ missing          — not started; MUST cite an issues/* file or SP number
🚫 non-goal         — intentionally deferred; MUST cite §9 of the doc itself
📜 informational    — type/field exists but documented as not load-bearing
```

**Status-row citation rules (subagents MUST follow):**
- Every `❌` row cites `[`docs/issues/<file>.md`](...)` inline
- Every `🚫` row cites `[§9.N](...)` inline (an anchor into the doc's own non-goals section)
- Every `⚠️` or `🔨` row cites either an `issues/` file OR a `sp<NN>-*` tag name or SP spec

**Evidence-first rule:** before writing a status row for a component, the subagent runs at minimum:
```bash
ls crates/atd-ref-server/src/                                   # module presence
grep -rn <symbol> crates/atd-ref-server/src/ crates/atd-client/  # usage
git log --oneline -20                                            # recency
```
The subagent records what it observed in the task report, then picks the status value from that observation — not from the spec's tentative labeling.

**Length guardrails per section** (spec §4.2):
```
§1  ~80     §6  ~180
§2  ~200    §7  ~80   (deliberately short!)
§3  ~180    §8  ~180
§4  ~280    §9  ~80
§5  ~220    §10 ~120
```
Plus ~80-150 for frontmatter/ToC/trailer. Total: ~1600 lines midpoint.

---

## Task 1: Frontmatter, ToC, §1 identity, §2 layer model

**Files:**
- Create: `/home/nan/proj/atd-mvp/docs/architecture.md`

**Target content:** H1 + frontmatter + ToC + §1 (~80 lines) + §2 (~200 lines) ≈ **380-450 lines of content**.

### Step 1.1: Verify baseline

- [ ] **Verify the spec exists and read the constraints.**

Run:
```bash
cd /home/nan/proj/atd-mvp
ls docs/superpowers/specs/2026-04-24-atd-architecture-v1-design.md
ls docs/architecture.md  # should NOT exist yet
ls docs/issues/ | wc -l  # should be 11 (10 issues + README)
git log --oneline | head -15
```

Expected: spec file exists; `docs/architecture.md` is absent; 11 files in `docs/issues/`; git log shows SP-12 commits landed (search for `SP-12`).

### Step 1.2: Verify SP-12 landing state (informs §2 cross-ref tables later, and Task 2)

- [ ] **Confirm SP-12 primitives are on master.**

Run:
```bash
cd /home/nan/proj/atd-mvp
git log --oneline | grep -i sp-12 | head -10
ls crates/atd-ref-server/src/binding.rs crates/atd-ref-server/src/capability.rs \
   crates/atd-ref-server/src/middleware.rs crates/atd-ref-server/src/tier.rs
grep -l 'CapabilityGate\|NativeBinding\|CliBinding\|RedactPathsMiddleware' crates/atd-ref-server/src/ -r
```

Expected: SP-12 commits present; `binding.rs`, `capability.rs`, `middleware.rs`, `tier.rs` all exist; grep finds the four symbols. Record findings in task report — they drive status-row values in Task 2 and Task 3.

If SP-12 primitives are NOT all landed, mark the relevant sub-section as `⚠️ partial` and cite the specific missing commit in the status row's Notes column.

### Step 1.3: Write the file's H1 + frontmatter + ToC

- [ ] **Create `docs/architecture.md` with this EXACT content.**

```markdown
# ATD Architecture (v1)

**Version:** 1.0 — 2026-04-24
**Implementation baseline:** `sp12-canonical-dispatch` (or the most recent commit on master containing the four dispatch primitives described in §4.2).
**Scope:** Normative architecture for the **reference implementation** (`atd-mvp` crates). Complements but does not replace the ATD whitepaper (`docs/whitepaper/atd-v3-multi-device.md`) or the wire reference (`docs/protocol/wire-format.md`).
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

```

### Step 1.4: Write §1 — The protocol identity (~80 lines)

- [ ] **Append §1 to the file.**

```markdown
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
- Not a rewrite of the whitepaper — [`docs/whitepaper/atd-v3-multi-device.md`](whitepaper/atd-v3-multi-device.md) remains authoritative for the protocol's long-term aspirational scope. This document reconciles whitepaper direction with implementation reality.
- Not a successor to `docs/design.md` in a way that deletes history — `design.md` is retained as the original Phase 0 spec for archival context; this document supersedes it as the current reference.

### 1.4 Relationship to existing documents

| Document | Relationship |
|---|---|
| `docs/whitepaper/atd-v3-multi-device.md` | Aspirational protocol scope. Whitepaper authoritative on long-term direction; this doc authoritative on reference-implementation commitments. |
| `docs/whitepaper/atd-v3-skills-architecture-brief.md` | Source for the five-layer stack diagram replicated in §2. |
| `docs/design.md` | Original Phase 0 spec. Superseded by this document for architecture questions; retained for history. |
| `docs/protocol/wire-format.md` | Wire-level reference — byte framing, message types, full type tables. Refer out to it; this document does not repeat wire details. |
| `docs/protocol/error-codes.md` | Error taxonomy. Refer out. |
| `docs/integrations/*.md` | Consumer-side guides per framework. This document gives them the layer model they assume. |
| `docs/issues/*.md` | Per-issue gap tracking. Every `❌` row in this document cites an `issues/` file. |
```

### Step 1.5: Write §2 — The layer model (~200 lines)

- [ ] **Append §2.**

```markdown
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
```

### Step 1.6: Verify line count + commit

- [ ] **Verify the file is within the target length.**

Run:
```bash
cd /home/nan/proj/atd-mvp
wc -l docs/architecture.md
```

Expected: 380-450 lines. If >500 or <300, trim or expand to fit budget. Shorter is OK if §1 and §2 still cover everything listed; longer means either §1 or §2 is exceeding budget and should be tightened.

- [ ] **Commit.**

```bash
cd /home/nan/proj/atd-mvp
git add docs/architecture.md
git commit -m "docs(architecture): scaffold architecture doc — §1 identity + §2 layer model"
```

---

## Task 2: §3 Schema Layer + §4 Dispatch Layer

**Files:**
- Modify: `/home/nan/proj/atd-mvp/docs/architecture.md` (append §3 + §4)

**Target content:** §3 (~180 lines) + §4 (~280 lines) = **450-550 additional lines**.

### Step 2.1: Verify current code state before writing status rows

- [ ] **Run the full evidence grep for schema + dispatch layer.**

```bash
cd /home/nan/proj/atd-mvp

# Schema layer evidence
echo "=== Schema: atd-types modules ==="
ls crates/atd-types/src/

echo "=== Schema: public structs/enums ==="
grep -hE '^pub (struct|enum) [A-Z]' crates/atd-types/src/*.rs | sort -u

echo "=== Schema: input_schema on ToolSummary (SP-10 Task 2.5) ==="
grep -n 'input_schema' crates/atd-types/src/summary.rs

echo "=== Schema: sanitize module (SP-10 Task 1) ==="
ls crates/atd-client/src/sanitize.rs

echo "=== Schema: machine-readable JSON schema ==="
ls atd-protocol-schema.json 2>&1 || echo "  (absent — issue schema-protocol-machine-readable-missing)"

# Dispatch layer evidence
echo "=== Dispatch: ref-server modules ==="
ls crates/atd-ref-server/src/

echo "=== Dispatch: SP-12 primitives presence ==="
for sym in CapabilityGate CapabilitySet NativeBinding CliBinding RedactPathsMiddleware Tier Hello; do
  echo -n "  $sym: "
  grep -rl "$sym" crates/atd-ref-server/src/ 2>/dev/null | head -1 || echo "NOT FOUND"
done

echo "=== Dispatch: SP-12 commits on master ==="
git log --oneline | grep -i sp-12 | head -10

echo "=== Dispatch: session/cancel wire messages ==="
grep -hE '"session|"cancel' crates/atd-client/src/protocol.rs 2>&1 | head -5 || echo "  (not present — issue dispatch-session-cancel-not-implemented)"

echo "=== Dispatch: ergonomic aliases ==="
grep -rn 'alias' crates/atd-client/src/ 2>&1 | grep -v test | head -5
```

Record every finding verbatim in the task report. These determine the status glyph for each row.

### Step 2.2: Write §3 Schema Layer (~180 lines)

- [ ] **Append §3.**

```markdown
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
```

### Step 2.3: Write §4 Dispatch Layer (~280 lines)

- [ ] **Append §4.**

```markdown
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
```

### Step 2.4: Verify line count + commit

- [ ] **Check length.**

```bash
cd /home/nan/proj/atd-mvp
wc -l docs/architecture.md
```

Expected: 830-980 lines (Task 1's ~400 + Task 2's ~450-550).

- [ ] **Commit.**

```bash
cd /home/nan/proj/atd-mvp
git add docs/architecture.md
git commit -m "docs(architecture): §3 schema layer + §4 dispatch layer with SP-12 primitives"
```

---

## Task 3: §5 Security Layer + §6 Extensibility

**Files:**
- Modify: `/home/nan/proj/atd-mvp/docs/architecture.md` (append §5 + §6)

**Target content:** §5 (~220 lines) + §6 (~180 lines) = **380-440 additional lines**.

### Step 3.1: Verify security + extensibility code state

- [ ] **Grep for security primitives.**

```bash
cd /home/nan/proj/atd-mvp

echo "=== Security: SSRF guard (web.fetch) ==="
grep -n 'check_ssrf\|ip_is_private' crates/atd-ref-server/src/tools/web/fetch.rs | head

echo "=== Security: header allowlist (web.fetch) ==="
grep -n 'allowed_headers\|build_headers' crates/atd-ref-server/src/tools/web/fetch.rs | head

echo "=== Security: ReadTracker (fs tools) ==="
grep -rn 'ReadTracker' crates/atd-ref-server/src/ | head -5

echo "=== Security: shell timeout (SIGTERM→grace→SIGKILL) ==="
grep -n 'SIGTERM\|start_kill' crates/atd-ref-server/src/tools/shell/ 2>&1 | head -5

echo "=== Security: audit logging (tracing) ==="
grep -rn 'tracing::info_span\|tracing::event' crates/atd-ref-server/src/ 2>&1 | head -5 || echo "  (absent)"

echo "=== Security: rate limit enforcement ==="
grep -rn 'Semaphore\|governor' crates/atd-ref-server/src/ 2>&1 | head -5 || echo "  (absent)"

echo "=== Security: dry_run handling per tool ==="
grep -rn 'dry_run' crates/atd-ref-server/src/tools/ 2>&1 | head -10

echo "=== Extensibility: middleware trait ==="
grep -n 'pub trait Middleware' crates/atd-ref-server/src/middleware.rs

echo "=== Extensibility: Binding trait ==="
grep -n 'pub trait Binding\|pub trait Tool' crates/atd-ref-server/src/binding.rs \
  crates/atd-ref-server/src/registry.rs 2>&1 | head -5
```

Record every finding verbatim.

### Step 3.2: Write §5 Security Layer (~220 lines)

- [ ] **Append §5.**

```markdown
## 5. Security Layer

### 5.1 Classification taxonomy

Every tool declares three classifications as part of its `ToolDefinition`. They are **descriptive metadata** — callers and human operators use them to reason about risk. They are NOT (in v1) enforcement mechanisms on their own; §5.2-§5.5 describe the actual runtime controls.

| Classification | Values | Declaring field |
|---|---|---|
| Safety level | `Read` / `Write` / `Financial` / `Privacy` / `Physical` / `Destructive` | `ToolSafety::level` |
| Visibility | `Read` / `Write` / `Dangerous` / `System` | `ToolVisibility` (top-level) |
| Trust level | `L1` / `L2Tested` / `L3Audited` | `ToolTrust::trust_level` |

Status: ✅ implemented in `crates/atd-types/`. Every built-in tool declares all three. LLM adapters surface `Visibility` and `SafetyLevel` to agent-framework tool pickers where supported.

Trust signatures (`ToolTrust::signature`) are declarative-only in v1 (`📜 informational`). Full signature verification is 🚫 non-goal — see [§9.4](#9-non-goals-explicit).

### 5.2 Per-tool runtime controls

Four specific runtime defenses run inside individual tools, not at the dispatch layer. Each defends a specific attack surface exposed by that tool's category.

| Control | Applies to | Source | Status |
|---|---|---|---|
| **SSRF guard** (loopback + RFC1918 + link-local + CGN + TEST-NET + 0.0.0.0/8 + IPv4-mapped-private; re-checked on every redirect hop) | `ref:web.fetch` | `crates/atd-ref-server/src/tools/web/fetch.rs::check_ssrf` | ✅ (SP-5) |
| **Header allowlist** (Accept, Accept-Language, Referer, User-Agent only; Authorization + Cookie rejected with `InvalidArgs`) | `ref:web.fetch` | same file, `build_headers` | ✅ (SP-5) |
| **Must-read-before-edit** (mtime + size proof required in session before `fs.edit` will apply) | `ref:fs.edit` | `crates/atd-ref-server/src/tracker.rs` (ReadTracker), used from `crates/atd-ref-server/src/tools/fs/edit.rs` | ✅ (SP-2) |
| **SIGTERM → grace → SIGKILL subprocess timeout** | `ref:shell.exec` / `ref:shell.pwsh` | `crates/atd-ref-server/src/tools/shell/shared.rs` | ✅ (SP-3) |
| **Request-arg schema validation** (serde + per-tool checks) | all tools | per-tool `call` impls | ✅ |

### 5.3 Capability tokens

v1's capability mechanism is the connection-scoped allow-list described in [§4.2.3](#423-capability-gate). Clients request capabilities via the `Hello` message; the server intersects with its `--grant-capability` allow-list; tools declaring `required_capabilities` outside the intersection are refused with `AtdError::CapabilityDenied` (code `1001`).

Cryptographically signed, delegatable UCAN-style tokens are 🚫 non-goal for v1; see [§9.3](#9-non-goals-explicit) for the deferral rationale and for the interim multi-tenant workaround (separate sockets per access tier).

| Component | Status | Notes |
|---|---|---|
| Connection-scoped allow-list | ✅ (SP-12) | See §4.2.3 |
| UCAN delegation tree | 🚫 | See §9.3 |
| Token revocation store | 🚫 | Same |
| Per-call agent identity tracking | ❌ | All calls currently execute as `did:anos:system`. Blocks fine-grained audit + tokens. |

### 5.4 Audit logging

| Component | Status | Notes |
|---|---|---|
| Structured per-call audit (tool_id, args_hash, outcome, duration, caller, tier, binding) | ❌ | Issue [`security-audit-logging-missing`](issues/2026-04-24-security-audit-logging-missing.md) |
| `--log-format json` CLI flag | ❌ | Planned |
| `tracing` subscriber integration | ❌ | Prerequisite |

Without audit, the other security layers are unobservable in retrospect. Shipping audit is the most valuable next security-adjacent SP; it is a prerequisite for meaningful multi-tenant authz (§9.3 defers that, but keeps this on the critical path).

### 5.5 Rate limiting and concurrency

| Component | Source | Status | Notes |
|---|---|---|---|
| `ToolResources.rate_limit_per_min` | `crates/atd-types/src/tool.rs` | 📜 | Declared on every tool; runtime ignores. Issue [`resource-limits-not-enforced`](issues/2026-04-24-resource-limits-not-enforced.md) |
| `ToolResources.max_concurrent` | same | 📜 | Same |
| Server-side semaphore wrapping per-tool invocation | — | ❌ | Planned: `tokio::sync::Semaphore` in `Registry` |
| Server-side rate-limiter (token bucket via `governor`) | — | ❌ | Planned |
| `AtdError::TooManyCalls` variant | — | ❌ | Would need to be added |

### 5.6 Dry-run consistency

| Component | Status | Notes |
|---|---|---|
| `CallOptions.dry_run` wire field | ✅ | Part of `RunTool` message |
| `Tool::honor_dry_run()` trait method | ❌ | Proposed in issue [`security-dry-run-inconsistent`](issues/2026-04-24-security-dry-run-inconsistent.md) |
| Dispatch-level rejection when `dry_run: true` but tool doesn't honor | ❌ | Planned — `AtdError::NotImplemented { feature: "dry_run" }` |
| Per-tool dry-run semantics (read-only tools vs destructive tools) | ⚠️ | Some tools silently ignore; others implicitly honor. Inconsistent. |

Closing this gap (a small SP) removes a silent-execute footgun: today, an agent asking `ref:shell.exec("rm -rf /", dry_run=true)` will run the command. v1 target: explicit rejection unless the tool opts in.

### 5.7 Target state (v1)

v1 security posture closes when:

- Classifications ✅ (done)
- Per-tool runtime controls ✅ (done for current tool set)
- Connection-scoped capability gate ✅ (done — SP-12)
- Audit logging ✅ (proposed SP after SP-13)
- Rate limiting + max_concurrent enforcement ✅ (proposed SP)
- Dry-run consistency ✅ (proposed small SP)
- Full UCAN tokens 🚫 (Phase 2)
- Tool signature verification 🚫 (Phase 2)

### 5.8 Gap → SP mapping

| Gap | Next SP | Status |
|---|---|---|
| Audit logging | Proposed SP post-SP-13 | ❌ |
| Rate limiting + max_concurrent | Proposed SP post-SP-13 | ❌ |
| Dry-run consistency | Proposed small SP | ❌ |
| Per-call agent identity | Enabler for audit + tokens — part of audit SP | ❌ |
| UCAN tokens | Phase 2 — see §9.3 | 🚫 |
| Tool signature verification | Phase 2 — see §9.4 | 🚫 |

### 5.9 See also

- [`docs/protocol/error-codes.md`](protocol/error-codes.md) — error taxonomy including `CapabilityDenied`
- [`docs/issues/2026-04-24-security-audit-logging-missing.md`](issues/2026-04-24-security-audit-logging-missing.md)
- [`docs/issues/2026-04-24-resource-limits-not-enforced.md`](issues/2026-04-24-resource-limits-not-enforced.md)
- [`docs/issues/2026-04-24-security-dry-run-inconsistent.md`](issues/2026-04-24-security-dry-run-inconsistent.md)
- [`docs/issues/2026-04-24-security-capability-tokens-deferred.md`](issues/2026-04-24-security-capability-tokens-deferred.md)
- [`docs/issues/2026-04-24-security-trust-signature-unverified.md`](issues/2026-04-24-security-trust-signature-unverified.md)
```

### Step 3.3: Write §6 Extensibility (~180 lines)

- [ ] **Append §6.**

```markdown
## 6. Extensibility

Four extension surfaces where ATD accepts code outside the reference implementation: new bindings, new tools, new middleware, and (v1+ planned) new aliases.

### 6.1 Binding extensibility

Adding a new binding back-end (for example: a gRPC binding, a WebAssembly binding, a REST binding):

| Step | Contract |
|---|---|
| 1. Implement `Binding` trait | Defined in `crates/atd-ref-server/src/binding.rs`. Given `args: serde_json::Value` + `&CallContext`, return `Result<serde_json::Value, ToolCallError>`. Respect `ctx.deadline`. |
| 2. Register an instance | `Registry::register_binding("grpc", Arc::new(GrpcBinding::new(...)))` at startup |
| 3. Tools declare `bindings: [ToolBinding { protocol: BindingProtocol::..., config: ... }, ...]` | One tool may have multiple bindings; dispatch picks one (currently: first) |

Current bindings:

| Binding | Protocol enum | Status |
|---|---|---|
| `NativeBinding` | `BindingProtocol::Cli` (historical name retained) | ✅ |
| `CliBinding` (subprocess) | `BindingProtocol::Cli` | ✅ |
| MCP binding | `BindingProtocol::Mcp` | 🚫 (§9.5) |
| REST binding | `BindingProtocol::Rest` | 🚫 (§9.5) |
| AppFunction binding | `BindingProtocol::AppFunction` | 🚫 (§9.5) |
| Distributed binding | — | 🚫 (§9.1) |

**Runtime-routing note:** v1 always routes to the first (and usually only) binding a tool declares. `CallOptions::preferred_binding` is currently dropped; issue [`dispatch-preferred-binding-ignored`](issues/2026-04-24-dispatch-preferred-binding-ignored.md). If real multi-binding tools land, the dispatcher's selection logic needs a small upgrade (pick preferred if available; else first).

### 6.2 Tool extensibility

Adding a new tool to the reference server (or to a third-party ATD server):

| Step | Contract |
|---|---|
| 1. Implement `Tool` trait | Defined in `crates/atd-ref-server/src/registry.rs`. Return `ToolDefinition` in `definition()`; implement `call(args, ctx)` returning `Result<serde_json::Value, ToolCallError>`. |
| 2. Register | `registry.register(Arc::new(MyTool::new()))` in `builtin.rs` or equivalent |
| 3. Declare required capabilities, safety, tier, bindings | Via the returned `ToolDefinition` |

Tools outside this repo can implement the same trait and register in their own `atd-ref-server`-analogue binary. The reference server is not required to host all tools; any crate can host a `Registry` and serve an ATD socket.

Canonical examples: `crates/atd-ref-server/src/tools/{echo,fs,shell,web}/`.

### 6.3 Middleware extensibility

Adding a new result-middleware:

| Step | Contract |
|---|---|
| 1. Implement `Middleware` trait | Defined in `crates/atd-ref-server/src/middleware.rs`. Given the prior result + metadata, return a (possibly rewritten) result or an error to short-circuit the chain. |
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

- [`crates/atd-ref-server/src/binding.rs`](../crates/atd-ref-server/src/binding.rs) — `Binding` trait definition
- [`crates/atd-ref-server/src/middleware.rs`](../crates/atd-ref-server/src/middleware.rs) — `Middleware` trait definition
- [`crates/atd-ref-server/src/registry.rs`](../crates/atd-ref-server/src/registry.rs) — `Tool` trait and registration
- [`docs/superpowers/specs/2026-04-25-sp12-canonical-dispatch.md`](superpowers/specs/2026-04-25-sp12-canonical-dispatch.md) — origin of the `Binding` / `Middleware` traits
```

### Step 3.4: Verify length + commit

- [ ] **Check.**

```bash
cd /home/nan/proj/atd-mvp
wc -l docs/architecture.md
```

Expected: 1210-1420 lines total.

- [ ] **Commit.**

```bash
cd /home/nan/proj/atd-mvp
git add docs/architecture.md
git commit -m "docs(architecture): §5 security layer + §6 extensibility"
```

---

## Task 4: §7 Skills + §8 Crate map + §9 Non-goals + §10 Evolution path

**Files:**
- Modify: `/home/nan/proj/atd-mvp/docs/architecture.md` (append §7, §8, §9, §10)

**Target content:** §7 (~80) + §8 (~180) + §9 (~80) + §10 (~120) = **360-460 additional lines**.

### Step 4.1: Verify crate layout

- [ ] **Confirm current crate names.**

```bash
cd /home/nan/proj/atd-mvp
ls crates/
echo "---"
for c in atd-types atd-client atd-ref-server atd-mcp-bridge atd-cli; do
  grep -A 1 '^\[package\]' crates/$c/Cargo.toml 2>/dev/null | head -3
  echo ""
done
echo "---"
ls python/src/ 2>/dev/null
```

Record the current crate names exactly — §8 uses them in the mapping table.

### Step 4.2: Write §7 Skills Layer (adjacent) — ~80 lines

- [ ] **Append §7. Keep it deliberately short.**

```markdown
## 7. Skills Layer (adjacent)

The Skills layer (SKILL.md files + `atd-tools:` dependency declarations + progressive-disclosure skill bodies) is drawn as a stack layer in the ATD v3 brief. From a protocol standpoint, Skills is an **upstream consumer** of ATD — not part of ATD itself.

### 7.1 Division of concern

| Concern | Owner |
|---|---|
| SKILL.md authoring, validation, install | Skills runtime (Anthropic Skills, OpenClaw ClawHub, third parties) |
| Progressive disclosure into agent context | Skills runtime |
| `atd-tools:` dependency declarations | SKILL.md format (owned by Skills spec); ATD's contribution is stable tool IDs |
| Invoking ATD tools from a skill body | Skills runtime calls ATD client (`atd_client.call(...)`) like any other agent |
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
```

### Step 4.3: Write §8 Component / crate map — ~180 lines

- [ ] **Append §8.**

```markdown
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

The current crate layout lumps some of these together. The table below names each logical component and its current home, with a suggested target for a future structural refactor.

| Logical component | Current crate | Status | Notes for future refactor |
|---|---|---|---|
| **Protocol** (types, wire, sanitize) | `atd-types` + `atd-client::wire` + `atd-client::protocol` + `atd-client::sanitize` | ⚠️ split across crates | Sanitize was moved to client in SP-10 to fix a reverse-dep; wire + protocol are in client because the client did both sides initially. Future refactor: consolidate into a single `atd-protocol` crate. |
| **Rust SDK** | `atd-client` | ✅ | Includes sanitize + adapters; feature-gated. |
| **Python SDK** | `python/src/atd_client/` | ✅ | Hand-ported mirror |
| **Runtime** (`Tool` trait, `Registry`, dispatch, context, tracker, binding, middleware, tier, capability) | `atd-ref-server/src/` (outside `tools/`) | ⚠️ lumped with tools + binary | Future refactor: `atd-runtime` crate as a library; ref-server binary becomes a thin wrapper. |
| **Built-in tools** (echo, fs, shell, web) | `atd-ref-server/src/tools/` | ⚠️ lumped | Future refactor: per-domain `atd-tools-*` crates (fs, shell, web, echo); each registers against runtime |
| **MCP bridge** | `atd-mcp-bridge` | ✅ | Binary |
| **CLI** | `atd-cli` | ✅ | Binary — `atd` command |
| **Examples** | `examples/` (not a published crate) | ✅ | |
| **Conformance suite** (future) | not yet | ❌ | Future SP |

### 8.3 Dependency graph (current)

```
atd-types
   ▲
   ├── atd-client (+ sanitize, adapters, wire, protocol)
   │       ▲
   │       ├── atd-mcp-bridge (depends on client)
   │       └── atd-cli (depends on client)
   │
   └── atd-ref-server (+ runtime + tools + binary)
           ▲
           └── atd-examples (hello_atd, hello_langchain)
```

Python SDK (`python/src/atd_client/`) mirrors `atd-types` + `atd-client` as a standalone Python package, with its own sanitize + adapters.

### 8.4 Target-state graph (if/when refactor lands)

```
atd-protocol (types + wire + sanitize + ready-to-generate JSON schema)
   ▲
   ├── atd-sdk (Rust client, adapters)
   │       ▲
   │       ├── atd-mcp-bridge
   │       └── atd-cli
   │
   ├── atd-runtime (Tool/Binding/Middleware traits, Registry, dispatch)
   │       ▲
   │       ├── atd-tools-fs
   │       ├── atd-tools-shell
   │       ├── atd-tools-web
   │       ├── atd-tools-echo
   │       └── atd-ref-server-bin (wires runtime + tools into a binary)
   │
   ├── atd-conformance (cross-impl tests — future)
   └── atd-sdk-py (Python mirror)
```

### 8.5 When to refactor

The refactor itself is a separate, yet-to-be-brainstormed project. This architecture doc names the target structure so refactor SPs have a concrete destination; it does not commit to a timeline. Triggering condition: either (a) a third-party server implementer asks for `atd-runtime` as a reusable library, or (b) multiple independent tool crates want to coexist.

Until then, the current lumping is acceptable — functionally correct, just not architecturally clean.

### 8.6 See also

- A future refactor brainstorm, yet to be written, will live at `docs/superpowers/specs/YYYY-MM-DD-atd-refactor-design.md`
- [`docs/design.md`](design.md) — the original Phase 0 spec that established the current crate names
```

### Step 4.4: Write §9 Non-goals — ~80 lines

- [ ] **Append §9.**

```markdown
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
```

### Step 4.5: Write §10 Evolution path — ~120 lines

- [ ] **Append §10.**

```markdown
## 10. Evolution path

A directional roadmap — **not a commitment calendar**. Each row states the item, the layer it touches, its status (from the status vocabulary), the proposed or expected SP number, a rough quarter, and the gating condition.

| Item | Layer | Status | Target SP | Rough window | Gate |
|---|---|---|---|---|---|
| Audit logging (structured per-call events) | Security | ❌ | post-SP-13 small SP | Q2 2026 | No adopter gate |
| Rate limiting + `max_concurrent` enforcement | Security | ❌ | post-SP-13 small SP | Q2 2026 | No adopter gate |
| Dry-run consistency across tools | Security | ❌ | post-SP-13 small SP | Q2 2026 | No adopter gate |
| Per-call agent identity tracking | Security | ❌ | bundled with audit | Q2 2026 | Prerequisite for audit and UCAN tokens |
| Machine-readable `atd-protocol-schema.json` | Schema | ❌ | proposed SP | Q2 2026 | No adopter gate |
| Conformance suite (SP-8 original) | Cross-cutting | ❌ | SP to be planned | Q2-Q3 2026 | Benefits from protocol schema being shipped first |
| Ergonomic aliases DSL (SDK-only) | Dispatch | ❌ | proposed SP | Q3 2026 | No strict gate; low priority |
| Additional built-in middleware (pii_redact, injection_detect, image_meta_strip) | Dispatch | ❌ | proposed SP | Q3 2026 | No strict gate |
| Sessions + cancellation | Dispatch | 🚫 v1 | — | undecided | Need a concrete adopter use case |
| TypeScript SDK | SDK | ❌ | TBD | undecided | Waiting for a concrete TS adopter |
| Crate refactor (atd-protocol / atd-sdk / atd-runtime / atd-tools-*) | Cross-cutting | ❌ | separate brainstorm | undecided | Triggered by a third-party server implementer request or multi-tool-crate need |
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
```

### Step 4.6: Verify total length + commit

- [ ] **Final length check.**

```bash
cd /home/nan/proj/atd-mvp
wc -l docs/architecture.md
```

Expected: 1500-1800 lines total. If outside 1300-1800, figure out which sections overran/undershot and fix. Typical over-run comes from §4 — if so, check whether §4.2.x subsections exceed their briefs.

- [ ] **Commit.**

```bash
cd /home/nan/proj/atd-mvp
git add docs/architecture.md
git commit -m "docs(architecture): §7 skills · §8 crate map · §9 non-goals · §10 evolution path"
```

---

## Task 5: Ancillary cross-links + grep-gate + tag

**Files:**
- Modify: `/home/nan/proj/atd-mvp/README.md`
- Modify: `/home/nan/proj/atd-mvp/docs/design.md`
- Modify: `/home/nan/proj/atd-mvp/docs/protocol/wire-format.md`
- Modify: `/home/nan/proj/atd-mvp/docs/integrations/overview.md`

### Step 5.1: Add Architecture entry to README

- [ ] **Read the current README Documentation section.**

```bash
cd /home/nan/proj/atd-mvp
grep -n '## Documentation' README.md
```

Open `README.md` at that section. Locate the "### Quick start guides" sub-heading. Insert a new sub-heading ABOVE it titled "### Architecture":

```markdown
### Architecture

- [**Architecture (v1)**](docs/architecture.md) — canonical layer model (Schema · Dispatch · Security · Extensibility · adjacent Skills layer), per-layer status tables, component/crate map, non-goals, and evolution path. Start here for the full picture.
```

The Quick start, Integration, Protocol, and Issues sub-headings that follow remain unchanged.

### Step 5.2: Add supersede note to design.md

- [ ] **Prepend a note to `docs/design.md`.**

Read the first 3 lines of `docs/design.md` (which is `# ATD Client SDK MVP — Design Spec` + frontmatter). After the frontmatter block ending (before the `---` or before `## 0. Context and Independence`), insert:

```markdown
> **Note (2026-04-24):** This document is the original Phase 0 design spec from 2026-04-21. It has been **superseded by** [`docs/architecture.md`](architecture.md) as the normative architecture reference for the reference implementation. This file is retained for historical context — to understand the Phase 0 scoping decisions and the then-open questions (§10), read this doc. To understand the current architecture, crate layout, and evolution path, read `docs/architecture.md`.
```

Use `Edit` to insert after the frontmatter (the part with `**Date:** 2026-04-21` etc.) and before the `---` that separates frontmatter from §0. Do not modify the rest of the file.

### Step 5.3: Add cross-link to wire-format.md

- [ ] **Open `docs/protocol/wire-format.md` and find §1 Overview.**

```bash
cd /home/nan/proj/atd-mvp
grep -n '^## 1' docs/protocol/wire-format.md
```

At the end of §1 (just before `## 2` or the next top-level heading), append a paragraph:

```markdown

See [`../architecture.md`](../architecture.md) for the higher-level layer model this wire protocol implements. The architecture doc describes the three core mechanisms (schema, dispatch, security) and points back to this document for byte-level detail.
```

### Step 5.4: Add cross-link to integrations/overview.md

- [ ] **Open `docs/integrations/overview.md`.**

Find the top-level introductory paragraph (before "## The five integration paths"). Append at the end of the introduction, just before the first `##` heading:

```markdown

For readers who want the full architectural picture underneath these integration paths — the layer model, mechanisms, crate map, and non-goals — see [`../architecture.md`](../architecture.md).
```

### Step 5.5: Run the grep gate

- [ ] **Check for placeholder text and ANOS assertions.**

```bash
cd /home/nan/proj/atd-mvp

echo "=== Placeholder check (expected: zero results or only the one legitimate TBD in §10) ==="
grep -nE 'TBD|TODO|placeholder|fill.?in|XXX' docs/architecture.md

echo "=== ANOS body-text check (expected: zero matches) ==="
grep -nE '^#{0,6}.*(ATD.*depends.*ANOS|current.*ANOS|require.*ANOS)' docs/architecture.md

echo "=== Section count check (expected: 10 H2 headings) ==="
grep -c '^## ' docs/architecture.md
```

Expected:
- Placeholder: at most 1 "TBD" occurrence in the §10 evolution table's "TypeScript SDK" row
- ANOS: 0 assertions
- H2 count: 10

If placeholder grep returns more than the 1 legitimate TBD, open the doc and fix each instance.

### Step 5.6: Verify cross-link correctness

- [ ] **Check that every internal link resolves.**

```bash
cd /home/nan/proj/atd-mvp

echo "=== Arch doc links to issues/ ==="
grep -oE '\(issues/[^)]+\)' docs/architecture.md | sort -u | while read -r link; do
  p=$(echo "$link" | sed 's/[()]//g')
  if [ -f "docs/$p" ]; then echo "  OK  $p"; else echo "  BROKEN  $p"; fi
done

echo "=== Arch doc links to protocol/ ==="
grep -oE '\(protocol/[^)]+\)' docs/architecture.md | sort -u | while read -r link; do
  p=$(echo "$link" | sed 's/[()]//g')
  if [ -f "docs/$p" ]; then echo "  OK  $p"; else echo "  BROKEN  $p"; fi
done

echo "=== Arch doc links to superpowers/ ==="
grep -oE '\(superpowers/[^)]+\)' docs/architecture.md | sort -u | while read -r link; do
  p=$(echo "$link" | sed 's/[()]//g')
  if [ -f "docs/$p" ]; then echo "  OK  $p"; else echo "  BROKEN  $p"; fi
done

echo "=== Arch doc links to whitepaper/ ==="
grep -oE '\(whitepaper/[^)]+\)' docs/architecture.md | sort -u | while read -r link; do
  p=$(echo "$link" | sed 's/[()]//g')
  if [ -f "docs/$p" ]; then echo "  OK  $p"; else echo "  BROKEN  $p"; fi
done
```

Fix any BROKEN entries before tagging.

### Step 5.7: Final regression

- [ ] **Sanity: workspace tests still pass.**

```bash
cd /home/nan/proj/atd-mvp
cargo test --workspace --all-targets 2>&1 | grep 'test result:' | awk '{s+=$4} END{print "total:", s}'
```

Expected: same total as before docs-only changes (around 252 or whatever it was after SP-12; doc changes do not affect tests).

### Step 5.8: Commit the ancillary updates + tag

- [ ] **Commit.**

```bash
cd /home/nan/proj/atd-mvp
git add README.md docs/design.md docs/protocol/wire-format.md docs/integrations/overview.md
git commit -m "docs: cross-link docs/architecture.md from README, design.md, wire-format, integrations/overview"
```

- [ ] **Create the annotated tag.**

```bash
cd /home/nan/proj/atd-mvp
git tag -a sp13-architecture-doc -m "SP-13: atd-mvp architecture v1 — single-file north-star (~1500 lines, 10 sections, reconciled status tables, 10 resolved decisions, 7 non-goals, evolution roadmap)"
git log --oneline --decorate=short | head -10
git tag | grep sp13
```

---

## Post-Plan Verification Checklist

- [ ] `docs/architecture.md` exists, between 1300 and 1800 lines
- [ ] All 10 H2 sections present in the spec's order
- [ ] Every ✅/⚠️/🔨/❌/🚫/📜 row cites a concrete source (file, issue file, or §9 anchor)
- [ ] Exactly 1 legitimate "TBD" remains (in §10 TypeScript SDK row); no other placeholders
- [ ] No ANOS assertions in the body (historical context lines are fine)
- [ ] README has an Architecture sub-section linking `docs/architecture.md`
- [ ] `docs/design.md` has a top-of-file supersede note
- [ ] `docs/protocol/wire-format.md` §1 links to `docs/architecture.md`
- [ ] `docs/integrations/overview.md` introduction links to `docs/architecture.md`
- [ ] `cargo test --workspace --all-targets` unaffected
- [ ] Internal links all resolve (no "BROKEN" from Step 5.6)
- [ ] Tag `sp13-architecture-doc` created

## What this plan does NOT do

- No code changes
- No refactor of crates (the crate refactor is a separate brainstorm, triggered by the conditions in §8.5 of the architecture doc)
- No new tests
- No new features / new middleware / new bindings
- No whitepaper edits
- No `docs/design.md` content changes beyond the top-of-file supersede note

## What comes after this plan

1. The architecture doc is landed.
2. A fresh brainstorm can start for **the refactor itself** — the architecture doc becomes the input document. Target: split `atd-types` / `atd-client`'s wire + sanitize / `atd-ref-server`'s runtime vs tools along the lines drawn in §8.4.
3. Individual remaining gap SPs (audit logging, rate limiting, dry-run, schema generation) become easier to scope because the architecture doc's status tables give each gap an authoritative anchor.
