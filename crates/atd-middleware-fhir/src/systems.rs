//! Default coding-system whitelist (70 URIs) — verbatim port of
//! celia's `crates/celia-core/src/fhir/systems.rs:15-91` as of
//! 2026-05-12.
//!
//! Drift between this list and celia's is caught by
//! [`tests::default_systems_match_celia_count`]. Removing or changing
//! entries is a minor-version bump on `atd-middleware-fhir`; adding
//! entries is additive and triggers a release note only.
//!
//! Operators can override at runtime via
//! [`crate::FhirMiddlewareConfig::extra_systems`] (append) or
//! [`crate::FhirMiddlewareConfig::replace_systems`] (full replacement
//! for highly-curated environments).
//!
//! Spec: SP-medical-middleware §4.4.

pub const ALLOWED_SYSTEMS_DEFAULT: &[&str] = &[
    // ===== International standards =====
    "http://loinc.org",
    "http://snomed.info/sct",
    "http://snomed.info/sct/2061000004102",
    "http://www.nlm.nih.gov/research/umls/rxnorm",
    "http://hl7.org/fhir/sid/icd-10",
    "http://hl7.org/fhir/sid/icd-10-cm",
    "urn:oid:2.16.156.10011.2.3.3.10",
    "http://www.ama-assn.org/go/cpt",
    "http://hl7.org/fhir/sid/ndc",
    "http://unitsofmeasure.org",
    // ===== HL7 categories =====
    "http://terminology.hl7.org/CodeSystem/observation-category",
    "http://terminology.hl7.org/CodeSystem/condition-category",
    "http://hl7.org/fhir/us/core/CodeSystem/us-core-documentreference-category",
    // ===== MIMIC-IV-on-FHIR research codes =====
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-d-labitems",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-chartevents-d-items",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-d-items",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-d-icd-diagnoses",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-d-icd-procedures",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-diagnosis-icd9",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-diagnosis-icd10",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-procedure-icd9",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-procedure-icd10",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-procedure-category",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-observation-category",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-bodysite",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-hcpcs-events",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-datetimeevents-d-items",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-outputevents-d-items",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-microbiology-test",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-microbiology-organism",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-microbiology-antibiotic",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-formulary-drug-cd",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-prod-code",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-ndc",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-gsn",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-etc",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-name",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-frequency",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-poe-iv",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-route",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-vital-signs-ed",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-observation-ed",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-triage",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-units",
    // ===== US Core / HL7 v3 / OMB =====
    "urn:oid:2.16.840.1.113883.6.238",
    "http://hl7.org/fhir/us/core/CodeSystem/us-core-race",
    "http://hl7.org/fhir/us/core/CodeSystem/us-core-ethnicity",
    "http://hl7.org/fhir/us/core/CodeSystem/us-core-birthsex",
    "http://hl7.org/fhir/v3/AdministrativeGender",
    "http://terminology.hl7.org/CodeSystem/v3-MaritalStatus",
    "http://terminology.hl7.org/CodeSystem/v3-NullFlavor",
    "http://terminology.hl7.org/CodeSystem/data-absent-reason",
    "http://terminology.hl7.org/CodeSystem/v3-ObservationInterpretation",
    "http://terminology.hl7.org/CodeSystem/v3-ActCode",
    "http://terminology.hl7.org/CodeSystem/condition-clinical",
    "http://terminology.hl7.org/CodeSystem/diagnosis-role",
    "http://terminology.hl7.org/CodeSystem/v3-ActPriority",
    "http://terminology.hl7.org/CodeSystem/location-physical-type",
    "http://terminology.hl7.org/CodeSystem/organization-type",
    "urn:ietf:bcp:47",
    // ===== MIMIC additions seen in real ICU data =====
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-services",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-admit-source",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-discharge-disposition",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-hcpcs-cd",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-lab-fluid",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-spec-type-desc",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-icu",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-method",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medication-method-icu",
    "http://mimic.mit.edu/fhir/mimic/CodeSystem/mimic-medadmin-category-icu",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_systems_match_celia_count() {
        // Drift guard mirroring celia's `systems.rs:104-110`. If this
        // fails, re-sync from
        // ~/code/pha/celia_phr/crates/celia-core/src/fhir/systems.rs:15-91.
        assert_eq!(
            ALLOWED_SYSTEMS_DEFAULT.len(),
            70,
            "ALLOWED_SYSTEMS_DEFAULT drifted from celia 70-entry baseline"
        );
    }

    #[test]
    fn no_duplicates() {
        let mut sorted: Vec<&&str> = ALLOWED_SYSTEMS_DEFAULT.iter().collect();
        sorted.sort_unstable();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(
            sorted.len(),
            deduped.len(),
            "duplicate entry in ALLOWED_SYSTEMS_DEFAULT"
        );
    }

    #[test]
    fn loinc_snomed_rxnorm_present() {
        for sys in [
            "http://loinc.org",
            "http://snomed.info/sct",
            "http://www.nlm.nih.gov/research/umls/rxnorm",
        ] {
            assert!(
                ALLOWED_SYSTEMS_DEFAULT.contains(&sys),
                "{sys} missing from default whitelist"
            );
        }
    }

    #[test]
    fn celia_legacy_uri_absent() {
        // SP-medical-middleware §8.1 — celia's own legacy URI must NOT
        // appear; if it does, an upstream PR has accidentally re-introduced
        // a non-standard system.
        assert!(
            !ALLOWED_SYSTEMS_DEFAULT.contains(&"https://celia.health/fhir/codes"),
            "celia legacy URI must not be in the default whitelist"
        );
    }
}
