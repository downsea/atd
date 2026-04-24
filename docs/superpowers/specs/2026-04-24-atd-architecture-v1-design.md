# ATD Architecture v1 — Design Spec (for `docs/architecture.md`)

**Date:** 2026-04-24
**Status:** Design approved through brainstorming; plan pending.
**Scope:** Produce a single canonical architecture document at
`docs/architecture.md` that (a) aligns with the ATD v3 whitepaper's
layer model, (b) honestly reconciles v3 aspirations with the current
atd-mvp implementation (including SP-12 canonical-dispatch work), and
(c) serves as the north-star reference for a subsequent refactor
series. This spec describes **how the architecture doc will be
written** — it is not the architecture doc itself. The document that
this spec plans is the deliverable; subagent-driven-development will
produce it from the plan writing-plans derives from this spec.
**Builds on:** `sp11-docs` + SP-12 canonical-dispatch work in
progress at time of writing.

---

## 1. Motivation

### 1.1 Why this document needs to exist

atd-mvp has accumulated three layers of truth that sometimes disagree:

1. **ATD v3 whitepaper** (`docs/whitepaper/v3-multi-device.md`) — the
   protocol's long-term aspiration. Multi-device dispatch, distributed
   sessions, middleware, aliases, Skills layer.
2. **Current code** (`crates/*/src/`) — SP-1 through SP-12 shipped.
   Schema layer nearly complete; dispatch layer recently got 4 canonical
   primitives (capability gate, tier, binding, middleware); security
   layer is classifications + per-tool runtime controls.
3. **Existing doc set** — `docs/design.md` (original Phase 0 spec,
   now partly stale), `docs/protocol/wire-format.md` (wire reference,
   current), `docs/integrations/*` (per-framework), `docs/issues/`
   (10 tracked gaps), individual SP specs.

There is no single document that says:

- Here is the full layer model.
- Here is exactly what's implemented today and what isn't.
- Here is what we deferred on purpose vs what's tracked as a gap.
- Here is the crate/module map that implements each layer.
- Here is the roadmap mapping layers → SP numbers → status.

Every reader currently has to synthesise that picture themselves from
eight disjoint files. The proposed architecture document produces the
synthesis once, authoritatively, so future readers (and the eventual
refactor) have one place to point at.

### 1.2 Why the brainstorming picked "reconciled over aspirational"

Three options were weighed. A pure aspirational doc (v3-aligned only)
would duplicate the whitepaper without reconciling reality. A pure
current-state doc would not explain the long-term shape. A reconciled
doc — per-layer **definition · current state · target state · gap
mapping** — serves all three reader classes identified in §3 below with
one artifact.

### 1.3 Why this is brainstorm-separate from the refactor itself

The refactor (crate reshape + remaining gap work) is a multi-SP effort.
Without a north-star doc, each refactor SP re-litigates the
architecture. Producing the architecture doc first, then brainstorming
the refactor plan with the doc as input, eliminates the per-SP
re-litigation. The two phases must run serially: writing the
architecture doc is its own project.

---

## 2. Scope

### 2.1 In scope for this spec

- Structure, sectioning, length budget, and section-level content brief
  for `docs/architecture.md`
- Convention for status tables (vocabulary, column layout, per-layer
  placement)
- Resolved architectural decisions that the document asserts
  authoritatively (vs leaving deliberately open)
- Positioning of Skills layer (adjacent, not ATD-native)
- Evolution/roadmap section format
- Delivery conventions: filename, header metadata, license note

### 2.2 Out of scope for this spec

- The content of the architecture document itself (that is the
  writing-plans output's job, executed by subagents)
- Refactor planning, code changes, crate renames
- New protocol features (middleware pipeline specifics, HMAC token
  fields, etc.) — those belong in refactor SPs
- Updating `docs/design.md` or `docs/whitepaper/*` — those remain as
  historical records; the architecture doc supersedes design.md for the
  reference implementation but does not rewrite the whitepaper

### 2.3 Prerequisites

- SP-12 canonical-dispatch work (`sp12-canonical-dispatch` or equivalent
  commit landing point) — because the architecture doc's "current state"
  tables for dispatch layer need accurate input. At time of writing, 5
  SP-12 commits are on master; final tag may or may not be cut. The
  implementation plan (post-brainstorm) must verify state of each
  primitive at the time the doc is written.

---

## 3. Readers and usage

Three reader classes, all first-class:

1. **External protocol implementers.** Authors of Go / Java / Swift / TS
   / ArkTS / other-language SDKs, or tool-server implementers in
   languages atd-mvp does not ship. They need the protocol layer model,
   wire-level links, capability-token semantics, and binding contract.
2. **Internal contributors.** The refactor series will run over months;
   each SP needs to cite the architecture doc for the piece it's
   landing. Contributors need the per-layer status tables, crate
   maps, and the SP roadmap.
3. **Decision-makers / evaluators.** Partners, upstream vendors,
   prospective adopters evaluating whether to bet on ATD. They need the
   identity claim, non-goals, evolution path, and honest status.

The document serves all three in one artifact — not by being
everything-to-everyone, but by having clear section-level affordances
each class navigates to.

---

## 4. Structure

### 4.1 File location and format

```
/home/nan/proj/atd-mvp/docs/architecture.md
```

Single Markdown file. H1 title; 10 top-level sections (H2); H3 inside
sections as needed. Estimated length: **1300-1800 lines**, skewing
longer than shorter to accommodate the per-layer tables. Longer than
1800 is a signal that non-architecture content snuck in.

Header metadata on the doc:

```markdown
# ATD Architecture (v1)

**Version:** 1.0 — 2026-04-24
**Implementation baseline:** `sp12-canonical-dispatch` (or the most
recent relevant tag at doc-write time)
**Scope:** normative architecture for the reference implementation
  (atd-mvp crates). Complements but does not replace the ATD
  whitepaper (`docs/whitepaper/v3-multi-device.md`) or the wire
  reference (`docs/protocol/wire-format.md`).
**Authority:** Where this document disagrees with `docs/design.md`
  (which predates SP-1), this document is authoritative. Where it
  disagrees with the v3 whitepaper on aspirational scope, the
  whitepaper remains authoritative for the protocol's long-term
  direction; this document is authoritative for what the reference
  implementation commits to.
**License:** Apache-2.0.
```

### 4.2 Section map (10 top-level)

```
1.  The protocol identity                    ~80 lines
2.  The layer model                          ~200 lines
3.  Schema Layer                             ~180 lines
4.  Dispatch Layer                           ~280 lines  (largest)
5.  Security Layer                           ~220 lines
6.  Extensibility                            ~180 lines
7.  Skills Layer (adjacent)                  ~80 lines   (deliberately short)
8.  Component / Crate map                    ~180 lines
9.  Non-goals (explicit)                     ~80 lines
10. Evolution path                           ~120 lines
                                             ─────────
                                             ~1600 lines
(plus intro frontmatter + ToC + trailing index ≈ 80-150 lines)
```

### 4.3 Per-section content briefs

Each core layer section (§3, §4, §5, §6) follows the same internal
pattern:

```
§N.1 Definition          — what this layer is, what it promises
§N.2 Current state       — status table (see §5 for format)
§N.3 Target state        — what the v1 reference commits to
§N.4 Gap → SP mapping    — each row in the status table points to
                           either (a) an `issues/*.md` file, or
                           (b) an SP number, or (c) a "resolved
                           decision" in §6 of this spec
§N.5 See also            — authoritative references (wire-format.md,
                           individual SP specs)
```

Narrative sections (§1, §2, §7, §9, §10) use freer structure.
Component-map section (§8) uses tables exclusively.

**§1 identity:** one-paragraph what ATD is (the "any tool, any
platform, any agent, any framework" claim verbatim from v3), who this
document serves, what it supersedes, what it leaves to other
documents.

**§2 layer model:** the stack diagram (Agent × Skill × SDK × Dispatch ×
Bindings × Tools × Devices) adapted from the v3 brief. Then: the three
core mechanisms (schema / dispatch / security) + the two extensibility
mechanisms (tier / binding). End with a cross-reference table mapping
each layer to the section that documents it.

**§3 Schema Layer:** types, JSON Schema for tool I/O, machine-readable
protocol schema status (gap → issue
`schema-protocol-machine-readable-missing`), sanitize rules.

**§4 Dispatch Layer:** discover/describe/call core. Then the six
sub-mechanisms:

- §4.2.1 Core dispatch path (SP-1 through SP-11 ref-server)
- §4.2.2 Binding abstraction (SP-12 NativeBinding + CliBinding; state:
  ✅ if SP-12 landed, ⚠️ if mid-flight at write-time)
- §4.2.3 Tier-aware deadlines (SP-12; same note)
- §4.2.4 Capability gate (SP-12 Hello handshake + grant/request)
- §4.2.5 Result-middleware pipeline (SP-12 RedactPathsMiddleware)
- §4.2.6 Sessions & cancellation (deferred — issue
  `dispatch-session-cancel-not-implemented`)
- §4.2.7 Ergonomic aliases (v3-borrowed; SDK-only, planned)

**§5 Security Layer:** classification taxonomy (✅), per-tool runtime
controls (SSRF / header allowlist / read-tracker / timeout — ✅ per
tool, not system-level), capability tokens (SP-12 allow-list is the
v1; full HMAC/UCAN deferred), audit logging (gap
`security-audit-logging-missing`), rate limiting (gap
`resource-limits-not-enforced`), dry_run consistency (gap
`security-dry-run-inconsistent`).

**§6 Extensibility:** binding-protocol contract (how to add an RPC
/ AppFunction binding), tool-registration contract, middleware trait
contract, aliases DSL.

**§7 Skills Layer (adjacent):** ~80 lines, intentionally short. See
§6 of this spec for exact content.

**§8 Component / crate map:** two sub-sections. (a) Principle (protocol
/ sdk / runtime / tools / bridges). (b) Current → target mapping table
(current crate name in atd-mvp → proposed target name per the refactor
framing from the prior architectural discussion, with `status: rename /
extract / split / unchanged` column).

**§9 Non-goals:** per-bullet, each non-goal says:

- What the non-goal is
- Why it's out of scope for v1.x
- What event (adopter / use case / hardware availability) would
  re-open it

Non-goals list:

- Multi-device routing (Phase 2+)
- Distributed sessions (Phase 2+)
- Full UCAN delegation tree (simplified token suffices for v1)
- Native Skills layer support
- REST transport over HTTP
- AppFunction binding reference (Phase 2+)

**§10 Evolution path:** a roadmap table mapping layer × SP × status ×
month-of-expected-landing for open items. Does not speculate beyond
24 months. Ends with "this document's update cadence" — who updates
it, when, how.

---

## 5. Status table convention

Single table shape used across every current-state sub-section:

```markdown
| Component / mechanism       | Source                                      | Status        | Tests        | Notes |
|----------------------------|---------------------------------------------|---------------|--------------|-------|
| `ToolSummary.input_schema` | `crates/atd-types/src/summary.rs`          | ✅ implemented | 2 (SP-10)    | Added in SP-10 Task 2.5 |
| Binding abstraction        | `crates/atd-ref-server/src/binding.rs`     | ✅ implemented | SP-12 tests  | `NativeBinding` + `CliBinding` |
| Rate-limiting enforcement  | `crates/atd-ref-server/src/registry.rs`    | ❌ missing     | —            | Issue `resource-limits-not-enforced` |
| Multi-device routing       | —                                           | 🚫 non-goal    | —            | See §9.1 |
```

**Status vocabulary (exactly these 6):**

- ✅ **implemented** — code + tests + docs present
- ⚠️ **partial** — code exists but either runtime is skeletal, tests are
  thin, or a documented aspect is missing
- 🔨 **in-progress** — actively being landed at doc-write time (only
  valid at the exact moment of writing; plan should refresh before
  commit)
- ❌ **missing** — not started; the row must cite an `issues/` file
- 🚫 **non-goal** — intentionally deferred; row must cite §9 of the
  doc itself
- 📜 **informational** — type / field exists but documented as not
  load-bearing (e.g., `ToolTier::{Hot,Cold}` on any given day)

Any other status is a lint error. The doc ends with a one-line
summary count of statuses across the whole doc to give readers a
quick gauge.

---

## 6. Resolved architectural decisions

The following decisions are **asserted authoritatively** in the
architecture doc. They are not left open; if a contributor wants to
change one, they change the architecture doc first and then the code.

| Decision item | Architecture doc asserts | Rationale (compressed) |
|---------------|--------------------------|------------------------|
| Binding dispatch runtime | Real multi-binding routing is v1 (per SP-12 NativeBinding + CliBinding). Further bindings (MCP, REST, AppFunction) added via the same trait; no runtime distinction for callers. | SP-12 shipped it. |
| `ToolTier` runtime semantics | v1 enforces per-tier **timeout + output-budget overrides** via `--tier-override`; `Hot`/`Cold` have specific meaning (Hot = lower timeout cap; Cold = higher latency budget). No warmup/eviction. | Concrete enough to be useful; avoids overcommitting to scheduler design. |
| Session / cancel | Not in v1 wire. Reserved words in `ClientMessage` to prevent collision with future additions. | Design surface too wide without a real use case. |
| Capability tokens | v1 ships **allow-list-based capability gate** via Hello handshake (SP-12). Full UCAN / cryptographic tokens deferred to Phase 2. | Solves 80% of single-deployment use cases; Phase 2 reopens for multi-tenant. |
| Result middleware | v1 ships the pipeline (SP-12 `middleware::Pipeline`) and one built-in (`RedactPathsMiddleware`). Third-party middleware can be composed via CLI flags. More built-ins (`pii_redact`, `injection_detect`, `image_meta_strip`) tracked as future SPs. | v3-borrowed; SP-12 landed the minimum. |
| Ergonomic aliases | SDK-only. Client-side transform before sending. Server unaware. | v3 Appendix J's recommended approach. |
| Multi-device routing | Non-goal for v1.x. | No hardware adopter yet. |
| Distributed sessions | Non-goal for v1.x; strictly depends on multi-device. | Same. |
| Full UCAN tokens | Non-goal for v1.x; simplified allow-list suffices. | Lower complexity; full UCAN adopter-driven. |
| Skills-layer native support | Non-goal for ATD core. Skills is a separate project that consumes ATD via the `discover/describe/call` API. | Cleaner separation; matches v3 brief's layering. |

---

## 7. Skills Layer section (special)

§7 of the architecture doc contains ~80 lines of content following this
outline:

1. **Claim:** Skills (SKILL.md + atd-tools dependency) is **adjacent
   but independent** of ATD. The document does not require ATD to
   understand SKILL.md semantics.
2. **Boundary:** ATD promises stable `discover/describe/call` + stable
   `AtdError` taxonomy. Everything else (skill body execution, state,
   progressive disclosure) is the skills runtime's concern.
3. **Use cases two:**
   - Direct agent → ATD call (one-shot; v3 brief Slide 2)
   - Skill body → ATD call (multi-step; v3 brief Slide 3)
4. **Where a Skills-compatible tool catalog would live:** future
   `atd skills --target skillmd` generator (tracked; see §10 roadmap).
5. **Why not in ATD core:** separate evolution cadence, different
   adopter, version-independence.

The section explicitly does NOT attempt to specify the Skills runtime
or the atd-tools YAML schema — those belong to the Skills project
when it exists.

---

## 8. Evolution path (§10 content brief)

Format: roadmap table with columns `Layer · Item · Status · Target SP
· Expected quarter · Blocking issues`.

Rows include (partial list):

- Schema / Machine-readable protocol schema / ❌ / SP-13 (proposed) /
  Q2 2026 / issue `schema-protocol-machine-readable-missing`
- Dispatch / Sessions + cancel / ❌ / TBD / Q3 2026 earliest / issue
  `dispatch-session-cancel-not-implemented` + needs adopter
- Security / Rate limiting enforcement / ❌ / quick-win / Q2 2026 /
  issue `resource-limits-not-enforced`
- Security / Audit logging / ❌ / quick-win / Q2 2026 / issue
  `security-audit-logging-missing`
- Security / Capability tokens (HMAC) / 🚫 non-goal v1 / v2+ / — / —
- Dispatch / Multi-device routing / 🚫 non-goal v1 / v2+ / — / needs
  hardware adopter
- Extensibility / Middleware library expansion / ⚠️ partial / SP-14
  (proposed) / Q3 2026 / —

The table is not a commitment calendar. Each row is accompanied by a
single-sentence gating condition. The section header explicitly says:

> "This is a directional roadmap, not a schedule. 'Expected quarter'
> is a rough aim; individual items land when the preceding gates
> are met and an SP is written."

A trailing sub-section on **update cadence** names the doc's owner
(CODEOWNERS — atd-mvp maintainers as a whole, not a named person) and
the expected review cycle (each major SP should touch the doc's
status tables).

---

## 9. Deliverables

### 9.1 Primary artifact

- `docs/architecture.md` — the architecture document, per §4-§8 of
  this spec
- Length: 1300-1800 lines
- Status vocabulary: exactly the 6 values in §5 of this spec

### 9.2 Ancillary changes

- **Update** `README.md`'s Documentation section: add a "Architecture"
  link pointing to `docs/architecture.md`, placed ABOVE Quick start
  guides since it's the entry point for readers seeking the full model.
- **Cross-references from existing docs:**
  - `docs/design.md` — add a top-of-file note saying "this is the
    original Phase 0 spec; `docs/architecture.md` supersedes it as the
    normative architecture reference; this file is retained for
    historical context"
  - `docs/protocol/wire-format.md` — add a link in §1 Overview to
    `docs/architecture.md` as the higher-level reference
  - `docs/integrations/overview.md` — add in the introduction "see
    `docs/architecture.md` for the layer model underlying these
    integration paths"

### 9.3 No code changes

This spec produces documentation only. The refactor (crate renames,
feature work) is a separate project in a separate brainstorm.

---

## 10. Acceptance criteria

- Architecture doc exists at `docs/architecture.md`
- Length in the 1300-1800 line band
- All 10 sections present, ordered per §4.2, with the per-section
  length guidance honored ±20%
- Every status table row uses exactly one of the 6 status values from
  §5; no other values
- Every ❌ / ⚠️ / 🔨 row cites a source (`issues/*.md`, SP spec, or §9
  non-goal)
- §6 of the doc (resolved decisions) contains the 10 decisions from
  §6 of this spec
- `README.md` and the three ancillary docs (§9.2) are updated
- No ANOS references in new content
- `cargo test --workspace --all-targets` still passes (sanity — docs
  can't break tests, but confirm)

---

## 11. Risks

- **Risk A — SP-12 state drift.** SP-12 commits are on master but the
  tag may or may not exist at write time. The plan must verify each
  SP-12 primitive's state before writing the status row. If SP-12 is
  tagged by plan-execution time, reference the tag directly.
- **Risk B — v3 whitepaper scope creep.** A contributor writing the doc
  may be tempted to rewrite v3 content. §2 of this spec and the
  authority clause in §4.1 guard against this. Enforcement: if the plan
  task asks to "expand on v3 §2.6 distributed sessions," the plan is
  wrong; escalate to controller.
- **Risk C — Status table drift from code.** Status tables are
  snapshot-in-time. The §10 update cadence policy handles future drift.
  For initial write, the plan MUST verify each row against the code at
  write time (not against this spec, which may lag).
- **Risk D — `docs/design.md` readers getting stale direction.** The
  §9.2 top-of-file note on design.md is mandatory, not optional. If
  the plan skips the design.md update, readers will continue to trust
  the stale doc.

---

## 12. Non-risks

- **Length:** 1300-1800 lines in one file is well-precedented (Linux
  kernel architecture docs are 2000+).
- **Code accuracy:** the plan's status-verification step closes this.
- **Review load:** the doc is navigable by §2's cross-reference table
  and a ToC in the intro.
- **Breaking external readers:** the architecture doc is additive; it
  doesn't rewrite any existing external-facing doc.

---

## 13. After this spec

- Self-review (pending — this spec's §14 placeholder-free check)
- User review
- Hand off to `superpowers:writing-plans` for implementation plan
- Subagent execution (writes the architecture doc per plan)
- Refactor brainstorm (separate, follows this one once the architecture
  doc is landed and the team has a canonical reference)

## 14. Self-review checklist (for author before asking user to review)

- [ ] All sections cite at least one concrete file path or existing
      artifact
- [ ] No "TBD" / "TODO" / placeholder text
- [ ] Section 6's decision table matches the brainstorm decisions
- [ ] Length budgets add up and leave room for frontmatter + ToC
- [ ] Every `issues/*.md` reference resolves (the 10 existing issues
      are all referenced here; verify `ls docs/issues/`)
- [ ] Risk section names concrete mitigations, not just the risks
- [ ] No ANOS leaks into assertions (architecture doc is reference-impl
      focused, not protocol-origin-focused)
