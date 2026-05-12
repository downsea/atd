//! [`PiiRedactConfig`] — operator-facing tuning.
//!
//! Spec: SP-medical-middleware §4.6.

use std::collections::HashMap;

use crate::strategy::RedactionStrategy;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PiiRedactConfig {
    /// Additional `(pointer, strategy)` pairs appended to the
    /// default 13-path table. For region-specific PHI loci.
    pub extra_paths: Vec<(String, RedactionStrategy)>,
    /// Override a default path's strategy. Key is the JSON Pointer
    /// string (must match the table entry verbatim).
    pub override_strategies: HashMap<String, RedactionStrategy>,
    /// Disable the catch-all regex layer (SSN, US license plate, IP,
    /// URL, email). Operators with high false-positive concern (e.g.
    /// legitimate serial numbers matching SSN regex) opt out.
    pub disable_regex_phi: bool,
    /// `true` (default): walk FHIR-shaped paths in `DEFAULT_PHI_PATHS`.
    /// `false`: skip those paths and run only the regex layer — for
    /// non-medical tools where PHI may still leak in free text.
    pub fhir_aware: bool,
    /// `true`: emit `_phi_findings: [pointer, ...]` on the result
    /// listing the paths that were touched (or *would* have been
    /// touched under `LogOnly`). Default `false` — adopters enable
    /// for migration step 2 observability.
    pub annotate_findings: bool,
}

impl Default for PiiRedactConfig {
    fn default() -> Self {
        Self {
            extra_paths: Vec::new(),
            override_strategies: HashMap::new(),
            disable_regex_phi: false,
            fhir_aware: true,
            annotate_findings: false,
        }
    }
}

impl PiiRedactConfig {
    /// Convenience: spec §7 migration step 2 — observe what would be
    /// redacted, don't actually redact. Sets all default paths to
    /// `LogOnly` via `override_strategies` and enables
    /// `annotate_findings`.
    pub fn log_only() -> Self {
        let mut cfg = Self::default();
        for (path, _) in crate::paths::DEFAULT_PHI_PATHS {
            cfg.override_strategies
                .insert((*path).to_string(), RedactionStrategy::LogOnly);
        }
        cfg.annotate_findings = true;
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe_for_fhir() {
        let cfg = PiiRedactConfig::default();
        assert!(cfg.fhir_aware);
        assert!(!cfg.disable_regex_phi);
        assert!(!cfg.annotate_findings);
        assert!(cfg.extra_paths.is_empty());
        assert!(cfg.override_strategies.is_empty());
    }

    #[test]
    fn log_only_overrides_every_default_path() {
        let cfg = PiiRedactConfig::log_only();
        assert!(cfg.annotate_findings);
        assert_eq!(
            cfg.override_strategies.len(),
            crate::paths::DEFAULT_PHI_PATHS.len()
        );
        for (path, _) in crate::paths::DEFAULT_PHI_PATHS {
            assert!(matches!(
                cfg.override_strategies.get(*path),
                Some(RedactionStrategy::LogOnly)
            ));
        }
    }
}
