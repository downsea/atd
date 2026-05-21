# atd-middleware-pii-redact-medical

Healthcare PHI redaction middleware for
[`atd-runtime`](https://crates.io/crates/atd-runtime). Strips the 18 HIPAA Safe
Harbor identifier categories from tool-result JSON before it crosses to the
LLM / client.

Sibling of [`atd-middleware-fhir`](../atd-middleware-fhir); both implement the
`atd_runtime::Middleware` trait and compose via
`atd_server::Server::set_middleware`.

## What it does

Walks the outgoing tool result (FHIR-aware by default, generic-JSON optional)
and applies a per-field `RedactionStrategy`:

- 13 default JSON Pointer paths covering the canonical FHIR R4 PHI loci
  (`Patient.name`, `.identifier`, `.birthDate`, `.telecom`, `.address.line`,
  `.photo`, etc.)
- 5 catch-all regex rules for PHI that drifts into free text (SSN / US license
  plate / IP / URL / email)

Strategies:

- `Strip` — replace with JSON null
- `Token("CATEGORY")` — replace with `"[REDACTED:CATEGORY]"`
- `YearOnly` — truncate `1955-03-15` → `"1955"`
- `ZipPrefix3` — truncate `"94303"` → `"943"`
- `HashSha256Truncated` — cross-call correlation without identity leak
- `FirstCharPrefix` — diagnostic preview
- `LogOnly` — annotate without mutating

Generic-JSON mode (`fhir_aware: false`) skips the FHIR-shape paths and runs the
regex layer only — useful for non-medical tools that may still leak PHI in
free-text fields.

## Usage (composed with the FHIR validator)

```rust
use atd_middleware_fhir::FhirMiddleware;
use atd_middleware_pii_redact_medical::PiiRedactMiddleware;
use std::sync::Arc;

let mut server = atd_server::Server::new(registry, config);
// FHIR validates structure first; PII redacts afterwards.
server.set_middleware(vec![
    Arc::new(FhirMiddleware::default()),
    Arc::new(PiiRedactMiddleware::default()),
]);
server.run().await?;
```

## Out of scope

NLP PHI detection, DICOM stripping, region-specific code systems, compliance
certifications, and schema-deep FHIR validation. See
[`docs/architecture.md`](../../docs/architecture.md) §7 (Middleware) for the
middleware model.

## License

Apache-2.0.
</content>
