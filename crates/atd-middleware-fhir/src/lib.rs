//! Egress FHIR R4 validation middleware for `atd-runtime`.
//!
//! SP-medical-middleware §4.3 + §5.1. Mount via
//! `Server::set_middleware(vec![Arc::new(FhirMiddleware::default())])`.
//!
//! What this middleware does:
//! - When a tool's result is a FHIR-shaped JSON (carries `resourceType`),
//!   validates that `resourceType` is in a known set (default = celia's
//!   12 supported types), all required fields per resource type are
//!   present, and every `coding[].system` URI is in the operator-
//!   configurable whitelist (default = celia's 70-URI baseline).
//! - On mismatch, applies the configured [`MismatchPolicy`]
//!   (default `AnnotateAndPass`: appends `_fhir_validation_errors:
//!   [...]` to the result; alternatives `ReplaceWithError` and
//!   `StripOffending`).
//! - Non-FHIR results pass through untouched (no `resourceType` →
//!   no-op).
//!
//! What this middleware does **not** do:
//! - Full FHIR R4 schema validation (cardinality, slicing, FHIRPath
//!   invariants — spec §3, deferred to a future SP gated on a hospital
//!   HIS gateway adopter).
//! - Inbound (tool-arg) validation — middleware is egress only per
//!   `atd-runtime::Middleware` contract.
//! - PHI redaction — that lives in the sibling
//!   `atd-middleware-pii-redact-medical` crate.

pub mod config;
pub mod middleware;
pub mod required_fields;
pub mod systems;
pub mod types;

pub use config::{FhirMiddlewareConfig, MismatchPolicy};
pub use middleware::FhirMiddleware;
pub use required_fields::REQUIRED_FIELDS_TABLE;
pub use systems::ALLOWED_SYSTEMS_DEFAULT;
pub use types::FhirValidationError;
