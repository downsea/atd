//! Error variants surfaced as JSON strings inside
//! `_fhir_validation_errors` (or `error.details[]` under `ReplaceWithError`).

use thiserror::Error;

/// One validation finding. Renders via `Display` into a JSON-friendly
/// string that the middleware emits into the result's
/// `_fhir_validation_errors` array.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum FhirValidationError {
    #[error("FHIR result missing resourceType discriminator")]
    MissingResourceType,
    #[error("unknown resourceType: {0}")]
    UnknownResourceType(String),
    #[error("required field missing on {resource_type}: {field}")]
    MissingRequiredField {
        resource_type: String,
        field: String,
    },
    #[error("disallowed coding system: {0}")]
    DisallowedCodingSystem(String),
}
