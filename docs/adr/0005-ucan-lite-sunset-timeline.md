# ADR 0005 — UCAN-lite shipped-dormant: sunset timeline + reactivation contract

- **Status:** Accepted
- **Date:** 2026-05-29
- **Deciders:** `atd` maintainers
- **Context source:** 2026-05-29 design audit (issue #4 of 12). Builds on [`docs/issues/2026-05-28-ucan-dormant-awaiting-product-trigger.md`](../issues/2026-05-28-ucan-dormant-awaiting-product-trigger.md).
- **Related:** [`docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md`](../archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md) (the SP that shipped UCAN-lite) · [`docs/adr/0001-celia-atd-roadmap-alignment.md`](0001-celia-atd-roadmap-alignment.md) §2.1 (1.A categorization) · `docs/atd-design-philosophy.md` 原则 7 + `docs/atd-positioning.md` §2 (实证文化)

## 1. Context

SP-capability-v2 (tag `sp-capability-v2`, 2026-05-11) shipped UCAN-lite end-to-end into the **frozen 1.x wire surface**:

- `Request::Hello.ucan_tokens: Vec<String>` (`crates/atd-protocol/src/messages.rs:87-88`)
- wire error codes 1010–1013 (`ERR_UCAN_INVALID` / `EXPIRED` / `DELEGATION_TOO_DEEP` / `AUDIENCE_MISMATCH`)
- `atd_runtime::ucan::{parse, verify, revocation}` + `UcanRevocationStore` trait + `max_ucan_chain_depth` config
- 27 unit + 12 integration tests, all green and kept green by the workspace gate

**The problem the audit surfaced.** Six months on, production traffic across every adopter still rides `ce_<hex>` bearer tokens. No adopter *mints* a UCAN chain in production. UCAN-lite is "shipped but unused" — and because it shipped into the 1.x wire, the `Hello.ucan_tokens` field + the four error codes are now **frozen**: removing them is a 2.0 breaking change.

This sits in tension with two constitutional commitments:

- **Positioning §2 (实证文化):** ATD's case for every feature is "a real session ran this." UCAN-lite has no such session. Its sole justification is the keystone *scenario* ("share my last 3 months heart-rate with Dr. Wang, expires in 7 days") — a vision, not a shipped product flow.
- **Design-philosophy 原则 7 / SP-token-broker-phase1 precedent** ("add the hook *when* the data exists, not before"). UCAN-lite is exactly the pre-built-infrastructure-for-absent-data this precedent warns against — except it's worse, because it's pre-built into the *frozen wire*, not a swappable trait.

The asymmetry with the healthy trigger pattern is the crux: cbrain→Python-runtime shipped *because cbrain was using it*; atd-ts is *correctly waiting* for a named ArkTS adopter before shipping. UCAN-lite inverted that discipline — it shipped on celia's "we will need this for sub-agent delegation," and that need has not materialized in production.

## 2. Decision

**Set a dated sunset clock with an explicit reactivation contract. Pursue activation and prepare deprecation in parallel.**

### 2.1 The clock

- **Deadline: 2026-12-01** (≈6 months from this ADR). The question evaluated on that date: *has any adopter minted and verified a UCAN chain in production traffic — even one real use case?*
- This is a **review trigger, not an auto-delete.** On 2026-12-01 the maintainers make an explicit go/sunset call recorded as an amendment to this ADR.

### 2.2 If activated (the good outcome)

A single adopter putting UCAN-lite on a real production path (celia's "share with Dr. Wang", or any sub-agent delegation flow) **cancels the sunset**. This ADR is amended to "Activated", the dormant-issue is closed-verified, and UCAN-lite graduates to a normal load-bearing feature. The maintainers actively pursue this (§2.4).

### 2.3 If not activated by 2026-12-01 (the deprecation path)

A staged, wire-respecting deprecation — the frozen field is NOT removed inside 1.x:

| Stage | When | Action |
|---|---|---|
| Deprecate | 2026-12 (Q4) | `#[deprecated]` on the public UCAN surface (`atd_runtime::ucan` re-exports, `UcanRevocationStore`); CHANGELOG note; positioning/architecture mark UCAN-lite "deprecated, unused — slated for 2.0 removal" |
| Warn | 2027-Q1 | Optional sunset-warning log line when a server is configured to accept UCAN tokens but none arrive; docs steer new adopters away |
| Remove | 2.0 (wire-breaking, whenever 2.0 happens) | Drop `Hello.ucan_tokens` + codes 1010–1013 + the `ucan` module. This is the ONLY point removal is allowed — it's a major bump by definition |

The tests stay green throughout 1.x (no bit-rot via deletion); the code is marked, not ripped.

### 2.4 Parallel activation pursuit

Independent of the clock, the maintainers actively look for a real trigger:
- Raise with celia whether the "share with Dr. Wang, 7-day expiry" flow is on their near-term product roadmap (it's the keystone scenario; celia is the natural first minter).
- Watch the multi-agent orchestration (Hermes "orchestrator + N children") path — the §1.2 sub-agent-delegation motivation in SP-capability-v2 — for the first production workflow that needs scoped child delegation.

## 3. What this ADR does NOT do

- **Does not touch UCAN code or tests in this cycle.** SP-observability-completeness-v1 (the sibling iteration) *reads* the UCAN chain to populate `CallEvent.capability_provenance` (Gap C) — that's a consumer of UCAN state, not a change to it, and is unaffected by this sunset clock. If anything, provenance makes a future UCAN activation *more* observable.
- **Does not change the 1.x stability commitment.** `Hello.ucan_tokens` keeps deserializing for every 1.x client regardless of this ADR; removal is gated on 2.0.
- **Does not pre-judge the 2026-12-01 outcome.** It forces a *decision* on that date instead of letting "shipped but unused" drift indefinitely — which is the actual debt.

## 4. Consequences

### 4.1 Positive
- Converts an open-ended "shipped-dormant" status into a dated, decidable question — no indefinite wire-level technical debt by neglect.
- Honors 实证文化: a feature with no production session gets an explicit "use it or sunset it" rather than a permanent pass.
- The staged deprecation respects the frozen wire (no surprise breakage inside 1.x).
- Makes the cbrain/atd-ts trigger discipline retroactively consistent — UCAN-lite is the one that jumped the gun, and now has a path back to discipline.

### 4.2 Negative / risks
- A near-deadline adopter commitment that then slips would leave the clock awkwardly mid-deprecation; mitigated by §2.2 (activation cancels sunset at any stage before 2.0 removal).
- Marking a feature `#[deprecated]` that an adopter *later* wants re-creates churn; accepted — the signal value of the deadline outweighs the small re-activation cost.

## 5. Revisit conditions

This ADR is amended (not superseded) when:
1. **2026-12-01 arrives** — record the go/sunset decision as a dated amendment.
2. **An adopter mints a UCAN chain in production before then** — amend to "Activated", cancel sunset, close the dormant issue.
3. **2.0 planning begins** — if still un-activated, the removal stage moves from "whenever" to a concrete 2.0 task.
