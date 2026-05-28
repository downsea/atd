# UCAN-lite shipped but dormant — awaiting product-side trigger for activation

**Layer:** security / capability (cross-project: atd ↔ celia_phr ↔ future adopters)
**Status:** shipped-dormant (deferred-decision)
**Carrying cost:** ~0 (no hot-path overhead; broker JWT-shape branch is cheap dispatch)
**Activation cost:** depends on trigger — see §5
**Filed:** 2026-05-28
**Related SP:** [`sp-capability-v2`](../archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md) (shipped 2026-05-11)
**Related ADR:** [`docs/adr/0001-celia-atd-roadmap-alignment.md`](../adr/0001-celia-atd-roadmap-alignment.md) §2.1
**Related adopter doc:** `celia_phr/docs/sp-capability-v2-adopter.md` (5/5 phases done, 27 tests green)
**Supersedes (semantically):** `docs/issues/2026-04-24-security-capability-tokens-deferred.md` (UCAN was "deferred"; now shipped-dormant)

## Summary

SP-capability-v2 (UCAN-lite) shipped end-to-end on 2026-05-11. The infrastructure works:

- **Issuer** (celia): `celia-core/src/ucan_issuer.rs` mints root + sub-delegation JWTs from DEK-derived signing keys; §13.1 invariant preserved.
- **Verifier** (atd): `atd_runtime::ucan::{parse_jwt, verify_jwt}` runs full chain validation (signature / exp / aud / depth / attenuation widening).
- **Revocation** (celia): `SqliteUcanRevocationStore` implements `atd_runtime::UcanRevocationStore` trait + wired into both UDS and HTTP brokers via `Server::set_ucan_revocation_store` + `CeliaConsentTokenBroker::with_ucan_revocation_store`.
- **Pair-time root UCAN**: every `celia_agent_authorize` returns both `ce_<hex>` opaque bearer (legacy) and root UCAN JWT (future-ready).
- **Broker dispatch**: `CeliaConsentTokenBroker::resolve_bearer` JWT-shape detection — 3-segment dot form → `resolve_ucan` → full verifier; otherwise → legacy bearer path.

**But: in production traffic, no UCAN JWT is presented.** All current adopters (Hermes, Claude Desktop, Cursor, Kimi via atd-mcp-bridge) call celia with the legacy `ce_<hex>` opaque bearer. The chain walker / depth check / revocation store are **effectively no-ops** until an adopter emits real tokens.

This issue records that gap explicitly + defers activation to a product-side trigger.

## Current state — what's shipped vs running

| Component | Code state | Runtime state |
|---|---|---|
| Pair-time root UCAN signing | ✅ shipped | ✅ runs on every `celia_agent_authorize` |
| External agent presents UCAN as bearer | ✅ broker JWT path live | ⚠️ no adopter does this; opaque `ce_<hex>` still dominates |
| Sub-delegation chains (A→B with `prf=[root]`) | ✅ `issue_delegation_ucan` available | ❌ nobody signs them (no orchestrator UI exists) |
| `SqliteUcanRevocationStore` | ✅ wired both transports | ⚠️ table empty; revocations route via `consent.status='revoked'` legacy path |
| `max_ucan_chain_depth = 5` enforcement | ✅ configured | ⚠️ never triggers; no chain longer than 1 ever arrives |
| `atd-runtime::ucan::verify_jwt` | ✅ 27 unit + 12 integration tests green | ⚠️ only test traffic, no production samples |

**Failure mode this prevents (today): none — no user-visible scenario depends on UCAN.**

**Failure mode the existence of this infrastructure prevents (future): catastrophic retrofit cost** if/when a product scenario surfaces that needs cryptographic delegation. See §3.

## The product-trigger scenario(s) — what would activate UCAN

These are the *concrete* business scenarios that would move UCAN from dormant → active. Listed in order of likelihood × business value:

### 3.1 Time-bounded scoped sharing ("share my last 3 months heart-rate with Dr. Wang, expires in 7 days")

**The keystone scenario.** This is the use case where UCAN is *uniquely necessary* (bearer + server-side ACL cannot cleanly express it without dragging Dr. Wang's identity into celia's `consent` table, which violates celia's local-first / device-scoped model).

Shape:
- User signs a UCAN with `aud=did:key:<doctor>`, `cap=["records:read:heart-rate:lastN=90d"]`, `exp=now+7d`
- Doctor's app (e.g., a separate ATD client, or even a non-ATD HTTPS endpoint that knows UCAN) presents this JWT as bearer
- celia verifies signature locally (doctor's identity never persisted) → grants scoped, time-limited access
- After 7 days the JWT expires automatically; revocation is *built into the token*, not into celia's state

**Why bearer can't do this**: bearer + server-side ACL requires celia to maintain a row for "Dr. Wang's bearer X has read:heart-rate:lastN=90d until T+7d". That row belongs to celia's DB → ties Dr. Wang to celia's account model → defeats the portable / device-scoped invariant. UCAN keeps the authority statement **in the token itself**, signed by the user, verifiable without celia knowing who Dr. Wang is.

### 3.2 Hermes / orchestrator sub-agent delegation

A→B handoff: orchestrator agent receives root UCAN at pair time → forks sub-agents → signs A→B sub-UCANs with attenuated scope → sub-agents call celia with [root, A→B] chain.

Needs:
- Hermes (or successor) to grow multi-agent topology
- celia UI surface to manage delegation tree + revoke individual sub-agents

Lower urgency than 3.1 because today's Hermes is single-layer; multi-layer orchestrator is speculative.

### 3.3 Cross-system data interop (Bluesky atproto-style health-data federation)

User wants to share data across vendors without each vendor importing the others' ACL schema. UCAN as portable signed capability is the cleanest interop format — competing with OAuth-style flows but better for the local-first model.

Most speculative; depends on whether such interop ecosystem materializes.

## ATD-side adaptation work the trigger would require

When 3.1 or 3.2 matures product-side, ATD itself needs follow-up work — **not just "flip a switch":**

1. **`Hello.ucan_tokens` adopter ergonomics.** Today only celia's `CeliaConsentTokenBroker` knows how to consume UCAN bearer. Other adopters (healthkit_cli, future cbrain, future oh-cli) would need:
   - SDK helper: `AtdClient::hello_with_ucan(jwt)` (today `hello_with_ucan_tokens` exists but no high-level convenience)
   - Documentation: "how to be a UCAN-aware ATD client" recipe page
   - Reference impl: maybe `atd-cli` learns `--ucan <jwt>` flag for testing

2. **Caveat field for 3.1 (time-bounded resource sharing).** Spec §4 says cap shape is `cmd: atd-cap + ...`. For "lastN=90d" / "expires:7d-after-issue" semantics, we'd need either:
   - Extend `Caveat` field on `UcanPayload` (currently minimal/absent) per UCAN v1.0 spec §5
   - Or push semantic constraints into `cap` string itself (e.g., `records:read:heart-rate?since=90d`) — simpler but less spec-canonical

   Need a sub-SP if a real adopter wants 3.1.

3. **Conformance test for chain emission.** `atd-conformance` currently tests UCAN *verification* via `ucan_hello_grants_union.rs` etc. But there's no test that exercises **"adopter emits a chain → atd verifies it correctly"** with a non-test issuer. Once celia's issuer is in production traffic, harden this:
   - Add `crates/atd-conformance/tests/external_issuer_chain.rs` that spawns a real signer (could be celia's `ucan_issuer` as a dev-dependency, or a synthetic signer that does the same operations)
   - Walks 3-link chain, verifies byte-alignment with `atd_runtime::ucan::types::UcanPayload`

4. **Multi-instance signing key deployment story.** Phase K shipped `ATD_CURSOR_SIGNING_KEY` env (perf-v1 axis 2, SP-pagination-v1) for cursor signing across LB-fronted instances, but there's no equivalent for the UCAN audience pin / verifier key sharing. Multi-instance celia (rare today, plausible if SaaS-shape adopter shows up) would need this. Document or implement.

5. **JSON Schema drift guard.** `atd-protocol-schema.json` (SP-protocol-schema) covers Request/Response. UCAN `UcanPayload` shape is currently inside `atd-runtime` (not `atd-protocol`) and not in the drift-guard. If atd-runtime evolves the payload (caveat field, new did methods, alternative CID hash), celia's `ucan_issuer` would silently diverge — both crates compile, but issuer-signed JWTs fail at celia's own verifier (which uses atd-runtime). **Mitigation suggestion** (low effort, do now even before trigger):
   - Add a `UcanPayload` schema to `atd-protocol-schema.json` (or a sibling doc) and gate atd-runtime changes against it
   - Or add a doc comment on `atd-runtime::ucan::types::UcanPayload` warning maintainers: "Format change here breaks adopter byte-alignment — notify celia_phr team before merging"

## Maintenance hooks (do now, low effort)

Even while UCAN is dormant, two cheap hooks prevent rot:

- [ ] **Schema drift guard** (§4 item 5 above): add `UcanPayload` to drift-guard or add maintainer-warning doc comment. Estimated ~30 min.
- [ ] **Passive review checkpoint**: this issue gets re-read whenever **any** of these natural events occur (no calendar-based review needed):
  - Next security-touching SP starts (e.g., SP-tool-signing / SP-attestation)
  - Patent §13.5 comes up in USPTO / 国知局 office action and counsel needs deployment status
  - Any adopter files an issue containing "multi-agent" / "delegation" / "limited-time sharing" / "share with external party"
  - A second adopter beyond celia ships a UCAN issuer

## Acceptance criteria for "trigger has fired"

Close this issue when **any one** of the following becomes true:

- [ ] A celia-side product feature (Tauri UI, Capacitor screen, or PWA page) lets the user issue a scoped time-bounded UCAN to a specified `aud` for explicit sharing (3.1 scenario shipped).
- [ ] An external orchestrator (Hermes, oh-cli, cbrain, or other) ships UCAN-aware client code that presents a real chain to celia in production traffic (3.2 scenario shipped).
- [ ] A second ATD adopter beyond celia implements a UCAN issuer (signals protocol-neutrality maturity).
- [ ] Patent §13.5 office action requires demonstrating the cryptographic-token branch in production deployment (legal trigger; would force activation regardless of business demand).

Closing the issue means: do the ATD-side adaptation work in §4, write activation SPs as needed, update `atd-architecture.md` §11.6 (or wherever) to record actual production usage.

## Until trigger: do nothing

Explicitly **not on the roadmap**:

- ❌ Enable criterion 6 (Playwright `sub_agent_delegation.spec.ts`) preemptively — no real workflow to demo
- ❌ Push Hermes / atd-mcp-bridge to start emitting UCAN bearer when none is needed
- ❌ Remove UCAN code to "simplify" — patent §13.5 load-bearing + bolt-on cost > carrying cost + sunk infrastructure
- ❌ Calendar-based "review every quarter" — passive touch-point review per §6 is sufficient and zero-overhead

## Why this is filed as an issue rather than just left implicit

Two reasons:

1. **Future-Claude / future-maintainer protection**: without this issue, a future reviewer might either (a) think UCAN is in production traffic and design around that false assumption, or (b) propose deleting UCAN as dead code. This issue is the canonical "shipped-dormant; by design; here's the trigger" reference.
2. **Patent priority-date paper trail**: explicit issue showing the infrastructure was shipped + tested + documented before the trigger arrived is good evidence for §13.5 "implemented and operative" arguments. Counsel can point at this if needed.

## References

- atd SP-capability-v2 design: `docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md`
- atd SP-capability-v2 plan: `docs/archive/superpowers/plans/2026-05-11-sp-capability-v2.md`
- atd ADR-0001 §2.1 (celia roadmap alignment): `docs/adr/0001-celia-atd-roadmap-alignment.md`
- celia adopter status: `~/code/pha/celia_phr/docs/sp-capability-v2-adopter.md`
- celia issuer impl: `~/code/pha/celia_phr/crates/celia-core/src/ucan_issuer.rs`
- celia broker impl: `~/code/pha/celia_phr/crates/celia-cli/src/atd_broker.rs`
- atd verifier impl: `crates/atd-runtime/src/ucan/{parse,verify,revocation,types,error}.rs`
- Closed sibling (now superseded by this dormancy framing): `docs/issues/2026-04-24-security-capability-tokens-deferred.md`
