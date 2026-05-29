# ADR 0006 — Observability completeness; SP-medical-middleware §4.2 amended

- **Status:** Accepted
- **Date:** 2026-05-29
- **Deciders:** `atd` maintainers
- **Implements:** [`docs/superpowers/specs/2026-05-29-sp-observability-completeness-v1-design.md`](../superpowers/specs/2026-05-29-sp-observability-completeness-v1-design.md)
- **Amends:** [`docs/archive/superpowers/specs/2026-05-11-sp-medical-middleware-design.md`](../archive/superpowers/specs/2026-05-11-sp-medical-middleware-design.md) §4.2
- **Related:** ADR 0005 (UCAN-lite sunset — sibling) · `docs/atd-design-philosophy.md` 原则 7 · `docs/atd-architecture.md` §6.4 / §7

## 1. Context

A 2026-05-29 design audit against the two constitutional docs found four
places where dispatch discounted ATD's promise to the human operator (and,
for the first, to the agent). SP-observability-completeness-v1 closes them.
This ADR records the decisions that change a previously-ratified contract.

## 2. Decisions

### 2.1 Error paths now run egress middleware (amends SP-medical-middleware §4.2)

SP-medical-middleware §4.2 stated error paths bypass middleware, justified by
"no PHI exit point through audit exists." That justification was scoped to the
**audit sink** (true, and still true — Axes B/C add only metadata, never a
body). But it was used to skip middleware on the **wire reply to the agent**,
which *does* carry tool-authored text:

- `ExecutionFailed { message }` → `ToolResultResponse { success:false, result }`
- `InvalidArgs(msg)` / `InternalError(msg)` → `Response::Error { message }`

Both reach the LLM verbatim. A FHIR tool failing with `"...Patient/{mrn}..."`
leaked the MRN — a HIPAA §164.502 disclosure the PHI middleware was installed
to prevent. **Amendment:** the `Middleware` trait gains `on_error` (default
no-op); dispatch runs `on_result` on the `ExecutionFailed` result Value and
`on_error` on every `Response::Error` exit. SP-medical-middleware §4.2's
audit-side claim stands; its "skip middleware entirely on error" consequence
is reversed. `PiiRedactMiddleware` overrides `on_error`.

### 2.2 Audit backpressure is selectable; default stays `Drop`

`AuditSink` gains `backpressure_strategy()` (default `Drop` — byte-compatible
with SP-concurrency-baseline). `BackpressureStrategy::{Drop, Block,
FallbackSink}`; `JsonLinesAuditSink::with_strategy`. The global default is
**not** flipped — flipping to `Block` would regress throughput for every
non-compliance adopter to serve a guarantee only compliance adopters need.
Medical adopters opt into `Block` (HIPAA §164.528 no-loss audit). The sink's
internal queue moved from tokio mpsc to a std `sync_channel` + std-thread
drain (removes the "construction requires tokio runtime" constraint).

### 2.3 `CallEvent` gains `capability_provenance`; `schema_version` 2 → 3

Additive optional field attributing each granted capability to its source
(`StringAllowList` / `UcanChain { issuer_did, chain_depth }`). The bump is the
v1→v2→v3 additive-field pattern, not a breaking shape change. Implemented
without changing any function signature: `CapabilitySet` carries provenance,
`verify_jwt` populates it inline (UCAN tests unchanged).

### 2.4 Declarative-only fields self-describe in the schema

`ToolResources::rate_limit_per_min` and `ToolTrust::trust_level` carry
schemars doc-comments stating their advisory / publisher-self-declared status,
now present in `/atd-protocol-schema.json`. No structural change; a conformance
test locks the caveat text.

## 3. Consequences

- **Positive:** error replies are redacted (closes a real PHI leak); operators
  can choose no-loss audit; capability grants are traceable to their source;
  advisory fields can't be silently mis-used. All additive — frozen 1.x wire
  untouched; old clients/sinks/middleware behave identically by default.
- **Negative / risks:** `schema_version` 2→3 trips any audit consumer pinning
  `== 2` (caught + fixed in `atd-ref-server` tests; flagged in the celia
  adopter-validation issue). `Block` requires a multi-thread runtime (ref
  binaries use one).
- **Versioning (per ADR 0004):** `atd-protocol` → 1.2.0 (schema doc-comments),
  `atd-runtime` → 1.2.0 (trait additions + provenance), middleware /
  conformance minor, listeners / sdk / cli patch. Cut at release time.

## 4. Revisit conditions

- A second non-celia medical adopter appears → promote the A/B/C unit coverage
  to standalone `atd-conformance` end-to-end scenarios (currently carried by
  celia adopter validation per plan F.1).
- A future SP enriches `CallEvent` with a body field → revisit whether an
  audit-side redaction hook is then needed (today it isn't — no body flows).
