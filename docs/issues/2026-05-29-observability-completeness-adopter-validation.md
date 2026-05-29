# SP-observability-completeness-v1 — adopter validation (celia + healthkit + cbrain)

**Layer:** cross-project (atd ↔ celia_phr / healthkit_cli / cbrain)
**Status:** ready-for-agent (atd-side implementation in flight)
**Filed:** 2026-05-29
**Related SP:** [`sp-observability-completeness-v1`](../superpowers/specs/2026-05-29-sp-observability-completeness-v1-design.md)
**Triggering audit:** 2026-05-29 design audit — 4 of 12 issues (error-path PII leak, audit-drop accounting hole, capability-provenance gap, advisory-field schema silence)

## Summary

ATD is shipping **SP-observability-completeness-v1** to close four places where the dispatch layer discounts ATD's promise to the human operator (and, for Axis A, to the agent). All four changes are additive (frozen 1.x wire untouched; `CallEvent.schema_version` 2→3 is an additive optional field). This issue asks each adopter to validate the change against their stack.

## What shipped on the ATD side (per SP axes)

| Axis | Change | Adopter-visible knob |
|---|---|---|
| A — error-path PII | `Middleware::on_error` (default no-op) + dispatch runs egress middleware on failure replies too | PHI middleware now redacts failure `message`/`details`; no config needed (PII crate overrides `on_error`) |
| B — audit backpressure | `AuditSink::backpressure_strategy()` (default `Drop`) + `BackpressureStrategy::{Drop, Block, FallbackSink}` + `JsonLinesAuditSink::with_strategy` | medical adopters opt into `Block` for HIPAA §164.528 no-loss audit |
| C — capability provenance | `CallEvent.capability_provenance: Option<Vec<CapProvenance>>` (schema v3) + `ProvSource::{StringAllowList, UcanChain{issuer_did, chain_depth}}` | audit consumers can trace each cap to its source |
| D — schema advisory docs | `rate_limit_per_min` + `trust_level` carry "advisory/self-declared" caveat in `/atd-protocol-schema.json` | SDK auto-doc / IDE hover surfaces the caveat |

## What we need from celia_phr (primary validator — has the compliance teeth)

### Step 1 — rebuild + smoke
```bash
cd /home/nan/code/pha/celia_phr
cargo build --release && cargo nextest run --workspace
```
Expected: drop-in. No source edit. If `cargo build` errors on a missing `on_error` (because a celia custom middleware fully replaces the trait), add the default no-op — but celia uses ATD's middleware, so none expected.

### Step 2 — Axis A: PHI must not leak on failure
Add (or confirm) a test where a celia tool fails with PHI in the message:
```rust
// pseudo: tool returns Err(ExecutionFailed { message: "decrypt failed for Patient/<mrn>" })
// assert the wire reply the LLM sees has the MRN redacted by the PHI middleware
```
**Pre-SP signature:** failure `message` reaches the LLM verbatim — MRN/DOB leak.
**Post-SP signature:** `[REDACTED:ID]` / `[REDACTED:NAME]` in the failure reply.

This is the load-bearing fix — it closes a HIPAA §164.502 disclosure path. If celia has any tool that embeds patient identifiers in error text, this is where it stops leaking.

### Step 3 — Axis B: Block-mode audit under load
```rust
// construct the audit sink with Block strategy
JsonLinesAuditSink::with_strategy(writer, capacity, BackpressureStrategy::Block)
```
Rerun the 120-query SHARP baseline (`scripts/agent-eval-hermes-family.ts`) with a deliberately slow audit disk (e.g. tmpfs throttle or an fsync-on-every-write wrapper).
**Assert:** `drops() == 0` across the run; dispatch p99 stays bounded (if it blows past the 200ms SLO, bump mpsc capacity and document the celia-side capacity choice).

### Step 4 — Axis C: capability provenance in the audit
With a `Hello` carrying both `requested_capabilities` (string allow-list) and `ucan_tokens` (if celia exercises the UCAN path in any test), confirm:
```bash
jq -c 'select(.capability_provenance) | {caller_id, capability_provenance}' celia-audit.jsonl
# expect each cap mapped to {kind:"string_allow_list"} or {kind:"ucan_chain", issuer_did, chain_depth}
```
Even without UCAN traffic (the common case), assert `schema_version: 3` parses and string-granted caps show `StringAllowList`.

### Step 5 — report back
File `celia_phr/docs/sp-observability-completeness-adopter.md` (mirroring the SP-concurrency-baseline adopter pattern) or comment here: PHI-on-failure result, Block-mode p99 + drops, provenance sample.

## What we need from healthkit_cli

- Rebuild + smoke (drop-in expected).
- Confirm any audit-log consumer parses `schema_version: 3` without breaking (the new field is optional; a v2 parser should ignore it).

## What we need from cbrain (Python server)

The Python `atd_server` runtime mirrors the Rust extension traits. To stay byte-compatible:
- Mirror `Middleware.on_error` (default no-op) in `python/src/atd_server/`.
- Mirror `backpressure_strategy` on the Python `AuditSink`.
- Re-run the conformance fixture corpus; the 3 new behavioural scenarios (`error_pii_redaction`, `audit_backpressure_block`, `capability_provenance`) that apply cross-impl should pass — bumps cbrain from 22/24 toward 25-27.

## Acceptance criteria

This issue closes when:
- [ ] celia rebuilds drop-in, existing suite green.
- [ ] celia confirms PHI absent from failure-path wire replies (Axis A).
- [ ] celia runs Block-mode audit with `drops == 0` + bounded p99 (Axis B).
- [ ] celia confirms `schema_version: 3` + provenance parse (Axis C).
- [ ] healthkit_cli confirms drop-in + v3 audit parse.
- [ ] cbrain mirrors `on_error` + `backpressure_strategy`; cross-impl conformance scenarios pass.
- [ ] Results documented (celia adopter doc + comment here).

## Out of scope for this issue

- UCAN-lite activation/sunset (ADR 0005 — separate track).
- Per-tool rate-limiter enforcement (Axis D only *documents* `rate_limit_per_min` is advisory; does not enforce it).
- Cursor HMAC key rotation (Phase 0 doc note only; awaits a federation adopter feeling the re-fetch cost).

## References

- ATD spec: `docs/superpowers/specs/2026-05-29-sp-observability-completeness-v1-design.md`
- ATD plan: `docs/superpowers/plans/2026-05-29-sp-observability-completeness-v1.md`
- Amends: `docs/archive/superpowers/specs/2026-05-11-sp-medical-middleware-design.md` §4.2 (error paths now redacted)
- Sibling ADR: `docs/adr/0005-ucan-lite-sunset-timeline.md`
