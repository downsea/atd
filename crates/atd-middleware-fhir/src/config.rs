//! Configuration for [`crate::FhirMiddleware`].
//!
//! Spec: SP-medical-middleware §4.4 + §5.1. All public types are
//! `#[non_exhaustive]` so additive fields/variants don't break
//! adopters in subsequent 0.x releases.

use crate::required_fields::REQUIRED_FIELDS_TABLE;
use crate::systems::ALLOWED_SYSTEMS_DEFAULT;

/// What to do when a FHIR result fails validation.
///
/// Default (via [`FhirMiddlewareConfig::default`]) is
/// [`Self::AnnotateAndPass`] — fail-open: the result reaches the
/// caller with an `_fhir_validation_errors: [...]` field. Operators
/// who want fail-closed semantics pick [`Self::ReplaceWithError`].
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum MismatchPolicy {
    /// Append `_fhir_validation_errors: ["..."]` to the result; pass
    /// through. Dispatch records `Outcome::Success` (the *tool*
    /// succeeded; the *middleware* objected).
    #[default]
    AnnotateAndPass,
    /// Replace the result with `{"error": "fhir_validation_failed",
    /// "details": [...]}`. Caller sees a clean error envelope.
    ReplaceWithError,
    /// Null out the offending `coding[]` entry (when the system URI
    /// is rejected). Keeps the surrounding structure intact.
    StripOffending,
}

/// Operator-facing tuning.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FhirMiddlewareConfig {
    /// Coding systems to add to the default whitelist (additive).
    /// Useful for region-specific code systems (e.g., Chinese
    /// ICD-10-CM国家版) without losing the international defaults.
    pub extra_systems: Vec<String>,
    /// If `Some`, fully replaces the default whitelist. Use for
    /// highly-curated environments that want a strict subset.
    pub replace_systems: Option<Vec<String>>,
    /// Acceptable `resourceType` values. Default = celia's 12.
    pub known_resource_types: Vec<String>,
    /// Behaviour on validation failure (see [`MismatchPolicy`]).
    pub on_mismatch: MismatchPolicy,
}

impl Default for FhirMiddlewareConfig {
    fn default() -> Self {
        Self {
            extra_systems: Vec::new(),
            replace_systems: None,
            known_resource_types: REQUIRED_FIELDS_TABLE
                .iter()
                .map(|(t, _)| (*t).to_string())
                .collect(),
            on_mismatch: MismatchPolicy::default(),
        }
    }
}

impl FhirMiddlewareConfig {
    /// Resolve the active allow-list: `replace_systems` wins if set,
    /// else defaults ∪ `extra_systems`.
    pub fn effective_systems(&self) -> Vec<String> {
        if let Some(replacement) = &self.replace_systems {
            return replacement.clone();
        }
        let mut out: Vec<String> = ALLOWED_SYSTEMS_DEFAULT
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        out.extend(self.extra_systems.iter().cloned());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_celia_12_types() {
        let cfg = FhirMiddlewareConfig::default();
        assert_eq!(cfg.known_resource_types.len(), 12);
        assert!(
            cfg.known_resource_types
                .iter()
                .any(|t| t == "Patient" || t == "Observation")
        );
    }

    #[test]
    fn effective_systems_defaults_to_70() {
        let cfg = FhirMiddlewareConfig::default();
        assert_eq!(cfg.effective_systems().len(), 70);
    }

    #[test]
    fn effective_systems_appends_extra() {
        let mut cfg = FhirMiddlewareConfig::default();
        cfg.extra_systems
            .push("http://example.org/regional-codes".into());
        assert_eq!(cfg.effective_systems().len(), 71);
    }

    #[test]
    fn replace_systems_takes_precedence_over_extra() {
        let mut cfg = FhirMiddlewareConfig::default();
        cfg.replace_systems = Some(vec!["http://only-this-one.example".into()]);
        cfg.extra_systems.push("http://ignored.example".into());
        let eff = cfg.effective_systems();
        assert_eq!(eff, vec!["http://only-this-one.example"]);
    }
}
