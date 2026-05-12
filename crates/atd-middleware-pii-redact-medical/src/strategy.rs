//! Per-field redaction strategies.
//!
//! Spec: SP-medical-middleware §4.6. Each path entry in
//! [`crate::DEFAULT_PHI_PATHS`] pairs a JSON Pointer with one of
//! these strategies; operators override via
//! [`crate::PiiRedactConfig::override_strategies`].

use serde_json::Value;
use sha2::{Digest, Sha256};

/// How to transform a PHI value when found.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RedactionStrategy {
    /// Replace value with JSON null. Used for `photo`, `signature`,
    /// `address.line`, `address.district`.
    Strip,
    /// Replace with literal `"[REDACTED:<CATEGORY>]"`. Preserves
    /// cardinality (LLM still sees "field exists") without leaking
    /// content. Default for `name`, `identifier`, `telecom`, etc.
    Token(&'static str),
    /// Replace string with `"<first-char>..."` for diagnostic preview.
    /// Length capped at 8 to avoid accidental info leak.
    FirstCharPrefix,
    /// SHA-256 hex of the original (truncated to 16 hex chars).
    /// Cross-call correlation without identity leak. Not in default
    /// table — operator opts in.
    HashSha256Truncated,
    /// Truncate ISO-8601 date string to year (`1955-03-15` →
    /// `"1955"`). Default for `birthDate`, `deceasedDateTime` —
    /// HIPAA Safe Harbor §164.514(b)(2)(i)(C) explicitly permits
    /// year-of-birth retention.
    YearOnly,
    /// Keep first 3 chars (US ZIP prefix), drop the rest.
    /// HIPAA Safe Harbor §164.514(b)(2)(i)(B) permits 3-digit ZIP
    /// prefix when population in that prefix > 20k (this strategy
    /// applies the truncation; further population check is operator-
    /// side per spec §9).
    ZipPrefix3,
    /// Annotate (when `annotate_findings = true`) but do NOT mutate
    /// the value. Used for spec §7 migration step 2 — adopters
    /// observe what would be redacted before flipping to real redaction.
    LogOnly,
}

impl RedactionStrategy {
    /// Apply this strategy to `value` in place. Returns `true` if the
    /// value was modified (`LogOnly` returns `false`). The caller
    /// decides whether to record the path in `_phi_findings`.
    pub fn apply(&self, value: &mut Value) -> bool {
        match self {
            Self::Strip => {
                *value = Value::Null;
                true
            }
            Self::Token(category) => {
                *value = Value::String(format!("[REDACTED:{category}]"));
                true
            }
            Self::FirstCharPrefix => match value.as_str() {
                Some(s) if !s.is_empty() => {
                    let first: String = s.chars().next().into_iter().collect();
                    *value = Value::String(format!("{first}..."));
                    true
                }
                _ => false,
            },
            Self::HashSha256Truncated => match value.as_str() {
                Some(s) => {
                    let mut h = Sha256::new();
                    h.update(s.as_bytes());
                    let digest = h.finalize();
                    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
                    *value = Value::String(format!("sha256:{hex}"));
                    true
                }
                _ => false,
            },
            Self::YearOnly => match value.as_str() {
                Some(s) if s.len() >= 4 => {
                    *value = Value::String(s[..4].to_string());
                    true
                }
                _ => false,
            },
            Self::ZipPrefix3 => match value.as_str() {
                Some(s) if s.len() >= 3 => {
                    *value = Value::String(s[..3].to_string());
                    true
                }
                _ => false,
            },
            Self::LogOnly => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strip_nulls_the_value() {
        let mut v = json!("anything");
        RedactionStrategy::Strip.apply(&mut v);
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn token_replaces_with_marker() {
        let mut v = json!({"family": "Smith"});
        RedactionStrategy::Token("NAME").apply(&mut v);
        assert_eq!(v, "[REDACTED:NAME]");
    }

    #[test]
    fn year_only_truncates_iso_date() {
        let mut v = json!("1955-03-15");
        RedactionStrategy::YearOnly.apply(&mut v);
        assert_eq!(v, "1955");
    }

    #[test]
    fn year_only_passes_short_strings() {
        let mut v = json!("19");
        let mutated = RedactionStrategy::YearOnly.apply(&mut v);
        assert!(!mutated);
        assert_eq!(v, "19");
    }

    #[test]
    fn zip_prefix_3_truncates() {
        let mut v = json!("94303");
        RedactionStrategy::ZipPrefix3.apply(&mut v);
        assert_eq!(v, "943");
    }

    #[test]
    fn hash_sha256_truncated_emits_prefix_format() {
        let mut v = json!("John Smith");
        RedactionStrategy::HashSha256Truncated.apply(&mut v);
        let s = v.as_str().unwrap();
        assert!(s.starts_with("sha256:"));
        assert_eq!(s.len(), "sha256:".len() + 16); // 8 bytes × 2 hex
    }

    #[test]
    fn log_only_does_not_mutate() {
        let mut v = json!("secret");
        let mutated = RedactionStrategy::LogOnly.apply(&mut v);
        assert!(!mutated);
        assert_eq!(v, "secret");
    }

    #[test]
    fn first_char_prefix() {
        let mut v = json!("Smith");
        RedactionStrategy::FirstCharPrefix.apply(&mut v);
        assert_eq!(v, "S...");
    }
}
