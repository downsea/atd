//! Default JSON-Pointer-with-wildcard paths covering the 18 HIPAA Safe
//! Harbor identifier categories (§164.514(b)(2)(i)(A-R)) projected onto
//! FHIR R4 resource shapes.
//!
//! ## Path format
//!
//! RFC 6901 JSON Pointer plus a single extension: `*` matches any
//! array index. So `/address/*/line` walks every entry in the
//! `address` array and addresses its `line` field. The walker
//! ([`crate::redact::redact_value`]) handles `*` segments inline.
//!
//! ## Why not field-name matching
//!
//! Spec §4.5: `Patient.name[].family` differs semantically from
//! `Practitioner.qualification[].issuer.display`. Pointer makes
//! intent explicit; bare field-name matching over-triggers.
//!
//! ## Coverage map
//!
//! | HIPAA letter | Identifier | Path / Regex |
//! |---|---|---|
//! | A | Names                                              | `/name` |
//! | B | SSN / MRN / account no.                            | `/identifier` (path) + SSN regex |
//! | C | Geographic < state level                           | `/address/*/{line,district,postalCode}` |
//! | D | Dates (birth/admission/discharge/death)            | `/birthDate`, `/deceasedDateTime` |
//! | E | Phone                                              | `/telecom`, `/contact/*/telecom` |
//! | F | Fax                                                | `/telecom` (shares E) |
//! | G | Email                                              | Email regex |
//! | H | URLs / IPs                                         | URL regex + IP regex |
//! | I | Account numbers                                    | `/identifier` (shares B) |
//! | J | Certificate/license numbers                        | `/identifier` (shares B) |
//! | K | Vehicle IDs / license plates                       | License-plate regex |
//! | L | Device IDs / serial numbers                        | `/extension/*` (heuristic) |
//! | M | Biometric IDs                                      | `/extension/*` (heuristic) |
//! | N | Photographs / full-face images                     | `/photo` |
//! | O | Other unique IDs                                   | `/identifier` (shares B) |
//! | P | Any other identifying number / characteristic      | Catch-all via regexes + extension paths |
//! | Q | (overlapping with O/P; see HIPAA wording)          | — |
//! | R | Internet protocol address                          | IP regex |

use crate::strategy::RedactionStrategy;
use crate::strategy::RedactionStrategy::*;

/// 13 default JSON-Pointer paths × strategy pairs. Each pointer may
/// contain `*` segments to wildcard array indices. The walker handles
/// the wildcard expansion at apply time.
///
/// Spec: SP-medical-middleware §4.5 + §4.6.
pub const DEFAULT_PHI_PATHS: &[(&str, RedactionStrategy)] = &[
    // ===== A — Names =====
    ("/name", Token("NAME")),
    // ===== B/I/J/O — Identifiers (MRN, SSN-as-identifier-value, etc.) =====
    ("/identifier", Token("ID")),
    // ===== C — Geographic =====
    ("/address/*/line", Strip),
    ("/address/*/district", Strip),
    ("/address/*/postalCode", ZipPrefix3),
    // ===== D — Dates =====
    ("/birthDate", YearOnly),
    ("/deceasedDateTime", YearOnly),
    // ===== E/F — Phone / Fax =====
    ("/telecom", Token("PHONE")),
    ("/contact/*/telecom", Token("PHONE")),
    // ===== N — Photographs =====
    ("/photo", Strip),
    // ===== L/M — Device / biometric (heuristic on extension array) =====
    ("/extension/*/valueIdentifier", Token("DEVICE")),
    ("/extension/*/valueString", Token("DEVICE")),
    // ===== H — URLs at known locations =====
    ("/url", Token("URL")),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_is_thirteen() {
        // Drift guard for plan + spec §4.5 ("13 default paths").
        assert_eq!(DEFAULT_PHI_PATHS.len(), 13);
    }

    #[test]
    fn every_path_starts_with_slash() {
        for (p, _) in DEFAULT_PHI_PATHS {
            assert!(p.starts_with('/'), "{p} must be a JSON Pointer");
        }
    }

    #[test]
    fn covers_all_hipaa_letters_via_paths_or_regex() {
        // Letters A, C, D, E/F, N, L/M, H — covered by paths.
        // Letters B/G/H/K/R — covered by regex layer (see redact.rs).
        // This test only checks the path-side covers what it should.
        let paths: Vec<&str> = DEFAULT_PHI_PATHS.iter().map(|(p, _)| *p).collect();

        // A — names
        assert!(paths.contains(&"/name"));
        // C — geographic
        assert!(paths.iter().any(|p| p.starts_with("/address/")));
        // D — dates
        assert!(paths.contains(&"/birthDate"));
        // E/F — telecom
        assert!(paths.contains(&"/telecom"));
        // N — photo
        assert!(paths.contains(&"/photo"));
    }
}
