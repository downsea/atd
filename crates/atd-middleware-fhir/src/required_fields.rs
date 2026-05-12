//! Per-resource required-field table — port of celia's
//! `crates/celia-core/src/fhir/validate.rs:117-166`.
//!
//! Each row is `(resourceType, required_field)`. A resource of the
//! given type missing the given field triggers a
//! `FhirValidationError::MissingRequiredField` annotation.
//!
//! We deliberately stop at presence checking (per spec §4.3): we do
//! NOT enforce cardinality, slicing, FHIRPath invariants, or numeric
//! ranges. Deeper checks require a full FHIR R4 schema bundle (~5000
//! fields, ~8-12 MB) deferred to a future SP.

/// 12 resource types × 1 required field each = 12 entries. Order is
/// not load-bearing (the verifier builds a `HashMap<&str, &[&str]>`
/// keyed by `resourceType`).
///
/// Spec: SP-medical-middleware §4.3 + §8.1
/// `missing_required_field_per_type`.
pub const REQUIRED_FIELDS_TABLE: &[(&str, &str)] = &[
    ("Patient", "id"),
    ("Observation", "status"),
    ("Condition", "subject"),
    ("MedicationStatement", "status"),
    ("Goal", "lifecycleStatus"),
    ("CarePlan", "status"),
    ("DocumentReference", "status"),
    ("AllergyIntolerance", "code"),
    ("Procedure", "status"),
    ("ServiceRequest", "intent"),
    ("DiagnosticReport", "status"),
    ("Encounter", "status"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_covers_celia_12_types() {
        // The 12 types celia's `validate.rs:17-19` enumerates.
        let expected: std::collections::HashSet<&str> = [
            "Patient",
            "Observation",
            "Condition",
            "MedicationStatement",
            "Goal",
            "CarePlan",
            "DocumentReference",
            "AllergyIntolerance",
            "Procedure",
            "ServiceRequest",
            "DiagnosticReport",
            "Encounter",
        ]
        .into_iter()
        .collect();
        let actual: std::collections::HashSet<&str> =
            REQUIRED_FIELDS_TABLE.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            actual, expected,
            "required-fields table drifted from celia 12"
        );
    }

    #[test]
    fn each_type_appears_exactly_once() {
        let mut types: Vec<&str> = REQUIRED_FIELDS_TABLE.iter().map(|(t, _)| *t).collect();
        types.sort_unstable();
        let mut deduped = types.clone();
        deduped.dedup();
        assert_eq!(
            types.len(),
            deduped.len(),
            "duplicate resourceType in REQUIRED_FIELDS_TABLE"
        );
    }
}
