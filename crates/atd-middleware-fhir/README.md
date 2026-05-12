# atd-middleware-fhir

Egress FHIR R4 validation middleware for `atd-runtime`.

Sibling of [`atd-middleware-pii-redact-medical`](../atd-middleware-pii-redact-medical); both implement
the existing `atd_runtime::Middleware` trait and compose via
`atd-server::Server::set_middleware`.

## What it does

Validates **outgoing** tool results that claim to be FHIR R4
(`result.resourceType` is set):

1. `resourceType` is in the known set (default = celia's 12 supported
   resource types).
2. Required fields per resource type are present (table-driven from
   celia's `crates/celia-core/src/fhir/validate.rs:117-166`).
3. Every `coding[].system` URI is in the whitelist (default = celia's
   70-URI baseline, drift-guarded by a unit test).

Non-FHIR results pass through untouched.

On mismatch, applies a configured [`MismatchPolicy`]:

- `AnnotateAndPass` (default) — append `_fhir_validation_errors: [...]`
  to the result; dispatch continues.
- `ReplaceWithError` — replace the result with `{"error":
  "fhir_validation_failed", "details": [...]}`. Tool's wire success
  flag is preserved (dispatch logs `Outcome::Success`; the *middleware*
  objected, not the tool).
- `StripOffending` — null out the offending coding entries, keep
  everything else.

## Usage

```rust
use atd_middleware_fhir::FhirMiddleware;
use std::sync::Arc;

let mut server = atd_server::Server::new(registry, config);
server.set_middleware(vec![Arc::new(FhirMiddleware::default())]);
server.run().await?;
```

For the combined FHIR + PHI-redaction chain, see
[`atd-middleware-pii-redact-medical`](../atd-middleware-pii-redact-medical/README.md).

## Out of scope

See [SP-medical-middleware spec §3](../../docs/superpowers/specs/2026-05-11-sp-medical-middleware-design.md#3-non-goals) — full FHIR R4 schema validation, NLP PHI detection,
DICOM stripping, region-specific code systems, compliance certifications.

## License

Apache-2.0.
