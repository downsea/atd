# ADR 0001 — Celia ↔ ATD roadmap alignment

- **Status:** Accepted (amended same-day after author re-read SP-capability-v2 design; SP-capability-v2 shipped 2026-05-11 — see §2.4 amendment)
- **Date:** 2026-05-11 · amendments: §2.4 UCAN re-categorized; SP-capability-v2 shipped (tag `sp-capability-v2`)
- **Deciders:** `atd` maintainers
- **Related:** [`docs/architecture.md`](../architecture.md) §9 + §10 · [`docs/roadmap.md`](../roadmap.md) · [`docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md`](../archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md) · upstream tracker: `~/code/pha/celia_phr/docs/ATD_FUTURE_ISSUES.md`

## 1. Context

The `celia_phr` downstream adopter (a Tauri-packaged PHR application) maintains an `ATD_FUTURE_ISSUES.md` file in its own repo. Family 1 of that file ("ATD protocol gaps") enumerates four items the adopter expects `atd` to address:

| celia label | Topic |
|---|---|
| 1.A | UCAN-style capability tokens |
| 1.B | Streamable HTTP transport |
| 1.C | Multi-device dispatch + distributed sessions |
| 4.A | `atd` crates.io publication |

The adopter framing ("on the roadmap") risks an expectations gap: `atd`'s authoritative roadmap surfaces are the v3 whitepaper (aspirational scope) and `docs/architecture.md` §9 (non-goals) / §10 (evolution path). Neither has previously stated, item-by-item, where each of these four sits. A conformance review (`docs/atd-vs-mcp.md` was its predecessor; this ADR is its sequel) found no spec conflicts in celia's integration, but flagged the four items as needing explicit alignment.

This ADR records the categorization, so adopters can read one document instead of cross-referencing two.

## 2. Decision

Each of celia's four items is categorized against two sources of authority:

- **Whitepaper v3** (`atd-v3-multi-device.md`) — what the protocol *aspires* to be
- **architecture.md** (this repo) — what the reference implementation *commits* to, under §9 (non-goals + their re-open gates) and §10 (evolution path with status glyphs)

The categorization:

| # | celia item | Whitepaper v3 stance | architecture.md status | celia-side re-open gate | Categorization |
|---|---|---|---|---|---|
| 1.A | UCAN capability tokens | ✅ Core vision (Security Layer is the three primitives at L341; concept 4 at L1212-1229; multi-device delegation chain at L1923-1925) | ✅ **SP-capability-v2 SHIPPED 2026-05-11** (tag `sp-capability-v2`); §9.3 amended; §10 row ✅ | Real gate was **sub-agent delegation** (not "multi-tenant per-socket" as originally framed in §9.3): "Agent A delegates read-only Patient X access to sub-agent B." **Triggered** by celia_phr (Hermes orchestrator + N specialised children; celia's flat RBAC forces user to re-pair every child). See SP-capability-v2 §1.2. | **ATD owned; near-term unlock = ✅ SHIPPED — adopter validation work tracked at `celia_phr/docs/sp-capability-v2-adopter.md` + `healthkit_cli/docs/sp-capability-v2-no-regression.md`.** |
| 1.B | Streamable HTTP transport | ❌ Not mentioned (grep: 0 hits) | ✅ Landed 2026-05-11 (architecture §10 row "HTTP transport"; §9.7 marked transitioned) | Gate at §9.7: "cloud-hosted ATD deployment surfaces a real need." **Triggered** by `celia_phr` 2026-05; gate cleared via SP-streamable-http + SP-token-broker-phase2 + SP-1.B. | **ATD owned and DONE.** |
| 1.C | Multi-device dispatch + distributed sessions | ✅ Core vision (the whole v3 whitepaper is titled "Multi-Device Extension"; §2.5 routing primitives, §2.6 session migration) | 🚫 v1 / Phase 2 (§9.1, §9.2, §10) | Gate at §9.1: "a device-vendor adopter (HarmonyOS, Apple, Google) commits to implementing an ATD server exposing device-scoped tools." **Not triggered** — celia is a desktop/web PHR application, not a device-class vendor. | **ATD long-term owned; gate untriggered.** |
| 4.A | crates.io publication | ❌ Not a protocol concern | ❌ Not in §9 / §10 — scheduling-only. `SP-publish-v2` design exists at `docs/archive/superpowers/specs/2026-04-25-sp-publish-v2-design.md` but pre-dates the 11→14 crate refactor and is stale. | None (scheduling). | **ATD owned; pure scheduling — refresh of SP-publish-v2 needed.** |

### 2.1 What ATD commits to (this ADR)

- **1.A:** **✅ SHIPPED as SP-capability-v2 (tag `sp-capability-v2`, 2026-05-11).** End-to-end: design (`docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md`) + 7-task TDD plan (`docs/archive/superpowers/plans/2026-05-11-sp-capability-v2.md`) + implementation across `crates/atd-protocol` (Hello.ucan_tokens + 4 wire codes 1010-1013) + `crates/atd-runtime/src/ucan/{parse,verify,revocation}.rs` + dispatch Hello arm union + InMemoryTokenBroker UCAN-JWT branch + UDS+HTTP integration tests (27 unit + 12 integration green). UCAN-lite (JWT-shape + Ed25519 + did:key) is additive to the SP-12 string allow-list — SP-12 adopters keep working untouched; clients that supply `Hello.ucan_tokens` get the union. celia_phr validation: 8 acceptance criteria + 5-phase implementation tracked at `celia_phr/docs/sp-capability-v2-adopter.md`. healthkit_cli: no-regression validation tracked at `healthkit_cli/docs/sp-capability-v2-no-regression.md`.
- **1.B:** Closed. Adopter can consume `atd-server-http` directly. Phase-2 follow-ups (TLS termination, OAuth/OIDC, request signing) remain explicitly **adopter-side**; ATD owns transport + bearer plumbing only.
- **1.C:** No near-term unlock from `celia_phr` alone. The gate is a device-class vendor adopter. If `celia_phr` *itself* wants device-class routing (e.g., to dispatch to a paired Apple Watch), it could co-author SP-multi-device-v1 — but the gate condition for `atd`'s `🚫 v1` deferral was specifically a device-vendor adopter, and `celia_phr` does not change that gate.
- **4.A:** Will refresh and execute SP-publish-v2 once the 14-crate layout stabilizes (no near-term blocker; celia's `path =` deps work today). No commitment to a specific quarter.

### 2.2 What ATD does not commit to

- A quarterly schedule for any 🚫-status item.
- Co-evolving with celia-specific feature requests that fall outside the four-`any` interoperability claim (architecture §1.1).
- Maintaining backward compatibility of *deferred* primitives (e.g., if multi-device routing eventually lands, its wire shape is not bound by celia's current single-tenant assumptions).

### 2.3 What `celia_phr` should do

- **§1.A** — track as "in flight as SP-capability-v2 — adopter validation work pending (see atd issue filed against this repo)". Prepare for the consent-schema migration sketched in SP-capability-v2 §4.8 + §6 (new `consent.parent_consent_id` + `consent.ucan_jwt` columns).
- **§1.C** — keep as "awaiting ATD gate trigger — re-evaluate quarterly". `celia_phr` is not a device-class vendor.
- Keep §1.B closed (already done).
- Keep §4.A open as adopter-side awareness; no action until SP-publish-v2 ships.
- Surface any new gate-triggering signals (e.g., device-class adopter pairing) to the `atd` maintainers explicitly.

### 2.4 Amendment note (2026-05-11 same-day)

The original 1.A categorization read "near-term unlock = NO" based on the architecture.md §9.3 gate text "multi-tenant deployment needs per-agent authorization finer than per-socket." A same-day re-read of `docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md` §1.2 surfaced that the **real** gate is *sub-agent delegation* (workflow-level), not *multi-tenant per-socket* (process-level), and celia_phr has been articulating this pain via Hermes's "orchestrator + N specialised children" workflow. The §9.3 gate text was rewritten on the same day to match the spec. This ADR is amended (not superseded) to record the corrected verdict; the categorization table row, §2.1 first bullet, §2.3 first bullet, and §5 first revisit condition all reflect the amendment.

### 2.5 What `healthkit_cli` should do

- **Confirm no regression** when SP-capability-v2 lands. UCAN-lite is additive (SP-capability-v2 §2 + §4.2 union semantics); healthkit_cli's existing `--grant-capability` startup mode keeps working with `Hello.ucan_tokens = None`. End-to-end smoke: existing Hermes integration test must remain green.
- **Optional adapter extension** (low priority): if a future Hermes pattern spawns sub-agents needing scoped healthkit access (e.g., one child reads sleep, another writes workouts), opt into UCAN ingress by adding `"ucan-jwt"` to its `TokenBroker::accepted_token_formats()`. This is not required for v1 of SP-capability-v2 to ship.

## 3. Consequences

### 3.1 Positive

- One reference document (this ADR) replaces ad-hoc Q&A about "is X on the ATD roadmap."
- `architecture.md` §9.7 + §10 are now factually correct (HTTP transport ✅ instead of 🚫).
- Future adopters get a clear template for asking the same question: cite the whitepaper section + architecture.md status + gate trigger condition.
- Avoids implicit promises around UCAN / multi-device that ATD has not made.

### 3.2 Negative / risks

- This ADR codifies that ATD's roadmap is **adopter-gate-driven** rather than calendar-driven. Adopters who prefer calendar commitments may find this unsatisfying; the explicit answer is that the four-`any` interoperability claim (§1.1) prioritizes protocol neutrality over vendor schedules.
- If `celia_phr` later does become multi-tenant or device-class, this ADR will need a sequel (or amendment) to record the gate-trigger event and reschedule.
- ADR predates a stable `docs/adr/` infrastructure (this is the first ADR in the repo); the ADR-numbering convention is established here.

### 3.3 Out of scope

- Detailed design of UCAN integration (SP-capability-v2 design exists at `docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md` but is design-only; no implementation commitment).
- Detailed design of multi-device dispatch (no SP exists yet).
- celia's own feature roadmap (Family 2 + Family 3 of `ATD_FUTURE_ISSUES.md` — those are celia-internal and out of ATD's purview).

## 4. References

- `docs/architecture.md` §9.7 (HTTP transitioned), §10 (evolution path)
- `docs/roadmap.md` — evolution scope; superseded the v3 whitepaper as the multi-device / UCAN vision surface
- `docs/archive/superpowers/specs/2026-05-11-sp-streamable-http-design.md` (1.B design)
- `docs/archive/superpowers/specs/2026-05-11-sp-token-broker-phase2-design.md` (1.B bearer integration)
- `docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md` (UCAN design, no commitment)
- `docs/archive/superpowers/specs/2026-04-25-sp-publish-v2-design.md` (4.A — stale, pre-14-crate refactor)
- `~/code/pha/celia_phr/docs/ATD_FUTURE_ISSUES.md` (upstream tracker, 2026-05-11)
- Commits: `758ce40` (1.B spec) · `db3287c` (broker phase 2 spec) · `dcdfd92` (BearerIdentity runtime) · `0448aad` (HTTP body) · `c269ce8` (medical middleware spec)

## 5. Revisit conditions

This ADR is amended (not superseded) if any of the following happens:

1. SP-capability-v2 lands (tag `sp-capability-v2`) — mark 1.A as fully closed; ADR sequel records end-to-end validation results from celia + healthkit.
2. A device-class vendor commits to an ATD server with device-scoped tools — re-open 1.C.
3. The 14-crate layout stabilizes for one quarter without churn — execute 4.A.
4. `celia_phr`'s `ATD_FUTURE_ISSUES.md` adds or removes items from Family 1 — re-categorize.
