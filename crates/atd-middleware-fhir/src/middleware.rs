//! [`FhirMiddleware`] — `atd_runtime::Middleware` implementation.
//!
//! Spec: SP-medical-middleware §4.3, §4.4, §5.1.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use atd_protocol::ToolDefinition;
use atd_runtime::Middleware;
use serde_json::Value;

use crate::config::{FhirMiddlewareConfig, MismatchPolicy};
use crate::required_fields::REQUIRED_FIELDS_TABLE;
use crate::types::FhirValidationError;

/// Egress FHIR R4 validation middleware.
///
/// `Arc`-wraps the runtime-needed lookups (effective systems set,
/// known-types set, required-fields map) so cloning the middleware is
/// cheap — every connection / dispatch task gets its own clone.
#[derive(Debug, Clone)]
pub struct FhirMiddleware {
    config: FhirMiddlewareConfig,
    effective_systems: Arc<HashSet<String>>,
    known_types: Arc<HashSet<String>>,
    required_fields: Arc<HashMap<String, String>>,
}

impl FhirMiddleware {
    pub fn new(config: FhirMiddlewareConfig) -> Self {
        let effective_systems: Arc<HashSet<String>> =
            Arc::new(config.effective_systems().into_iter().collect());
        let known_types: Arc<HashSet<String>> =
            Arc::new(config.known_resource_types.iter().cloned().collect());
        let required_fields: Arc<HashMap<String, String>> = Arc::new(
            REQUIRED_FIELDS_TABLE
                .iter()
                .map(|(t, f)| ((*t).to_string(), (*f).to_string()))
                .collect(),
        );
        Self {
            config,
            effective_systems,
            known_types,
            required_fields,
        }
    }

    /// Validate `value` against the configured policy. Pure function:
    /// returns the errors found, does not mutate `value`. The caller
    /// ([`Self::on_result`]) decides whether to mutate based on
    /// [`MismatchPolicy`].
    fn collect_errors(&self, value: &Value) -> Vec<FhirValidationError> {
        let mut errors = Vec::new();

        // Skip non-FHIR results entirely. Spec §4.3 trigger: presence
        // of `resourceType`. An array / Bundle of resources is left to
        // a future SP.
        let Some(rt) = value.get("resourceType").and_then(|v| v.as_str()) else {
            return errors;
        };

        // (1) Known resource type?
        if !self.known_types.contains(rt) {
            errors.push(FhirValidationError::UnknownResourceType(rt.to_string()));
            // Even if unknown, continue with coding-system check so the
            // result gets all findings at once (operator UX).
        }

        // (2) Required field present?
        if let Some(field) = self.required_fields.get(rt) {
            if value.get(field).is_none() || value.get(field).map(|v| v.is_null()) == Some(true) {
                errors.push(FhirValidationError::MissingRequiredField {
                    resource_type: rt.to_string(),
                    field: field.clone(),
                });
            }
        }

        // (3) Recursively walk and check every `coding[].system`.
        walk_codings(value, &mut |system: &str| {
            if !self.effective_systems.contains(system) {
                errors.push(FhirValidationError::DisallowedCodingSystem(
                    system.to_string(),
                ));
            }
        });

        errors
    }

    /// Apply [`MismatchPolicy::StripOffending`]: walk the result tree
    /// and null out `coding[]` entries whose `system` isn't allowed.
    fn strip_offending_codings(&self, value: &mut Value) {
        walk_codings_mut(value, &|system: &str| {
            self.effective_systems.contains(system)
        });
    }
}

impl Default for FhirMiddleware {
    fn default() -> Self {
        Self::new(FhirMiddlewareConfig::default())
    }
}

impl Middleware for FhirMiddleware {
    fn name(&self) -> &'static str {
        "fhir_egress_validate"
    }

    fn on_result(&self, _tool_id: &str, _tool_def: &ToolDefinition, result: &mut Value) {
        let errors = self.collect_errors(result);
        if errors.is_empty() {
            return;
        }
        let detail_strings: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        match self.config.on_mismatch {
            MismatchPolicy::AnnotateAndPass => {
                if let Some(obj) = result.as_object_mut() {
                    obj.insert(
                        "_fhir_validation_errors".into(),
                        Value::Array(detail_strings.into_iter().map(Value::String).collect()),
                    );
                }
                // Non-object results (rare for FHIR but possible for an
                // array Bundle) pass through silently — annotation in
                // arrays would require synthesizing a wrapper, which is
                // out of scope here.
            }
            MismatchPolicy::ReplaceWithError => {
                *result = serde_json::json!({
                    "error":   "fhir_validation_failed",
                    "details": detail_strings,
                });
            }
            MismatchPolicy::StripOffending => {
                self.strip_offending_codings(result);
                // After strip, the remaining errors are non-coding
                // (UnknownResourceType, MissingRequiredField); annotate
                // those so operators still see them.
                let remaining: Vec<String> = errors
                    .iter()
                    .filter(|e| !matches!(e, FhirValidationError::DisallowedCodingSystem(_)))
                    .map(|e| e.to_string())
                    .collect();
                if !remaining.is_empty() {
                    if let Some(obj) = result.as_object_mut() {
                        obj.insert(
                            "_fhir_validation_errors".into(),
                            Value::Array(remaining.into_iter().map(Value::String).collect()),
                        );
                    }
                }
            }
        }
    }
}

/// Recursive walk: invoke `visit` on every `system` string inside any
/// `coding` array, anywhere in the tree.
fn walk_codings(value: &Value, visit: &mut dyn FnMut(&str)) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(coding_arr)) = map.get("coding") {
                for coding in coding_arr {
                    if let Some(system) = coding.get("system").and_then(|s| s.as_str()) {
                        visit(system);
                    }
                }
            }
            // Recurse into all values regardless of key — codings can
            // appear nested under any field (e.g., `valueCodeableConcept.coding[]`).
            for v in map.values() {
                walk_codings(v, visit);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                walk_codings(v, visit);
            }
        }
        _ => {}
    }
}

/// Mutating walk: drop `coding[]` entries whose `system` predicate
/// returns false.
fn walk_codings_mut(value: &mut Value, system_is_allowed: &dyn Fn(&str) -> bool) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(coding_arr)) = map.get_mut("coding") {
                coding_arr.retain(|coding| {
                    coding
                        .get("system")
                        .and_then(|s| s.as_str())
                        .map(system_is_allowed)
                        .unwrap_or(true)
                });
            }
            for v in map.values_mut() {
                walk_codings_mut(v, system_is_allowed);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                walk_codings_mut(v, system_is_allowed);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_protocol::{
        BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
        ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
    };
    use serde_json::json;

    fn stub_def() -> ToolDefinition {
        ToolDefinition {
            id: "test:tool".into(),
            name: "t".into(),
            description: "stub".into(),
            version: "0.0.0".into(),
            capability: ToolCapability {
                domain: "test".into(),
                actions: vec![],
                tags: vec![],
                intent_examples: vec![],
            },
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            bindings: vec![ToolBinding {
                protocol: BindingProtocol::Cli,
                config: json!({}),
            }],
            safety: ToolSafety {
                level: SafetyLevel::Read,
                dry_run: false,
                side_effects: vec![],
                data_sensitivity: None,
            },
            resources: ToolResources {
                timeout_ms: 1000,
                max_concurrent: 1,
                rate_limit_per_min: None,
                estimated_tokens: None,
            },
            trust: ToolTrust {
                publisher: "test".into(),
                trust_level: TrustLevel::L0Unverified,
                signature: None,
            },
            visibility: ToolVisibility::Read,
            required_capabilities: vec![],
            tier: None,
            errors: vec![],
        }
    }

    // ---- spec §8.1 unit cases ----

    #[test]
    fn passes_non_fhir_result_untouched() {
        let mw = FhirMiddleware::default();
        let mut v = json!({"echoed": "hi", "count": 7});
        let snapshot = v.clone();
        mw.on_result("ref:echo.say", &stub_def(), &mut v);
        assert_eq!(v, snapshot);
    }

    #[test]
    fn rejects_celia_legacy_uri() {
        let mw = FhirMiddleware::default();
        let mut v = json!({
            "resourceType": "Observation",
            "status": "final",
            "code": {"coding": [{
                "system": "https://celia.health/fhir/codes",
                "code": "x"
            }]}
        });
        mw.on_result("hms:observation.get", &stub_def(), &mut v);
        let errs = v["_fhir_validation_errors"]
            .as_array()
            .expect("annotation expected");
        assert!(
            errs.iter()
                .any(|e| e.as_str().unwrap_or("").contains("celia.health")),
            "expected celia URI in errors, got {errs:?}"
        );
    }

    #[test]
    fn accepts_loinc_snomed_rxnorm() {
        let mw = FhirMiddleware::default();
        for sys in [
            "http://loinc.org",
            "http://snomed.info/sct",
            "http://www.nlm.nih.gov/research/umls/rxnorm",
        ] {
            let mut v = json!({
                "resourceType": "Observation",
                "status": "final",
                "code": {"coding": [{"system": sys, "code": "x"}]}
            });
            mw.on_result("t", &stub_def(), &mut v);
            assert!(
                v.get("_fhir_validation_errors").is_none(),
                "{sys} should pass without annotation, got {v}"
            );
        }
    }

    #[test]
    fn missing_required_field_per_type() {
        let mw = FhirMiddleware::default();
        for (rt, missing) in REQUIRED_FIELDS_TABLE {
            let mut v = json!({"resourceType": rt});
            mw.on_result("t", &stub_def(), &mut v);
            let errs = v["_fhir_validation_errors"]
                .as_array()
                .unwrap_or_else(|| panic!("{rt}: expected annotation"));
            assert!(
                errs.iter()
                    .any(|e| e.as_str().unwrap_or("").contains(missing)),
                "{rt} should report missing {missing}, got {errs:?}"
            );
        }
    }

    #[test]
    fn policy_replace_with_error() {
        let mut cfg = FhirMiddlewareConfig::default();
        cfg.on_mismatch = MismatchPolicy::ReplaceWithError;
        let mw = FhirMiddleware::new(cfg);
        let mut v = json!({
            "resourceType": "Observation",
            "status": "final",
            "code": {"coding": [{"system": "https://celia.health/fhir/codes", "code": "x"}]}
        });
        mw.on_result("t", &stub_def(), &mut v);
        assert_eq!(v["error"], "fhir_validation_failed");
        assert!(v["details"].is_array());
    }

    #[test]
    fn unknown_resource_type_rejected() {
        let mw = FhirMiddleware::default();
        let mut v = json!({"resourceType": "DeviceMetric"}); // not in celia 12
        mw.on_result("t", &stub_def(), &mut v);
        let errs = v["_fhir_validation_errors"].as_array().expect("annotation");
        assert!(
            errs.iter()
                .any(|e| e.as_str().unwrap_or("").contains("DeviceMetric")),
            "expected DeviceMetric in errors, got {errs:?}"
        );
    }

    // ---- supplementary: strip-offending strategy ----

    #[test]
    fn policy_strip_offending_drops_bad_coding_keeps_good() {
        let mut cfg = FhirMiddlewareConfig::default();
        cfg.on_mismatch = MismatchPolicy::StripOffending;
        let mw = FhirMiddleware::new(cfg);
        let mut v = json!({
            "resourceType": "Observation",
            "status": "final",
            "code": {"coding": [
                {"system": "http://loinc.org", "code": "good"},
                {"system": "https://celia.health/fhir/codes", "code": "bad"}
            ]}
        });
        mw.on_result("t", &stub_def(), &mut v);
        let codings = v["code"]["coding"].as_array().unwrap();
        assert_eq!(codings.len(), 1);
        assert_eq!(codings[0]["code"], "good");
    }

    #[test]
    fn nested_coding_validated_recursively() {
        let mw = FhirMiddleware::default();
        let mut v = json!({
            "resourceType": "Observation",
            "status": "final",
            "valueCodeableConcept": {
                "coding": [{"system": "https://celia.health/fhir/codes", "code": "x"}]
            }
        });
        mw.on_result("t", &stub_def(), &mut v);
        assert!(
            v["_fhir_validation_errors"].is_array(),
            "nested coding system should also be checked"
        );
    }
}
