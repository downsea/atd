//! Core walker + regex layer. Public function [`redact_value`] is
//! reusable as a free function — spec §4.7 keeps the door open for a
//! future audit-side hook that wraps the same logic without going
//! through the `Middleware` trait.
//!
//! Spec: SP-medical-middleware §4.5 + §4.6.

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::config::PiiRedactConfig;
use crate::paths::DEFAULT_PHI_PATHS;
use crate::strategy::RedactionStrategy;

/// Pre-compiled catch-all regex set per spec §4.5. Compiled once at
/// first use, shared across all `redact_value` calls.
struct RegexSet {
    ssn: Regex,
    plate: Regex,
    ip: Regex,
    url: Regex,
    email: Regex,
}

fn regexes() -> &'static RegexSet {
    static SET: OnceLock<RegexSet> = OnceLock::new();
    SET.get_or_init(|| RegexSet {
        // US SSN: 3-2-4 digits with optional hyphens.
        ssn: Regex::new(r"\b\d{3}-?\d{2}-?\d{4}\b").expect("ssn regex"),
        // US license plate: 2 letters + 6-10 digits (heuristic per spec §4.5).
        plate: Regex::new(r"\b[A-Z]{2}\d{6,10}\b").expect("plate regex"),
        // IPv4: 4 octets ≤255.
        ip: Regex::new(
            r"\b(?:(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.){3}(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b",
        )
        .expect("ip regex"),
        // URL: simple http(s):// match. (Stops at whitespace or common
        // JSON-end delimiters.)
        url: Regex::new("https?://[^\\s\\}\\]\\)\"']+").expect("url regex"),
        // Email: any@any.any (intentionally loose; false positives are OK).
        email: Regex::new(r"\b[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}\b").expect("email regex"),
    })
}

/// Apply the configured PHI redactions to `value` in place. Returns
/// the JSON Pointer paths that were touched (for `annotate_findings`).
pub fn redact_value(value: &mut Value, cfg: &PiiRedactConfig) -> Vec<String> {
    let mut findings: Vec<String> = Vec::new();

    if cfg.fhir_aware {
        // (1) Walk default paths first, then operator extras.
        let strategies = effective_strategies(cfg);
        for (pointer, strat) in &strategies {
            apply_at_pointer(value, pointer, strat, &mut findings);
        }
    }

    // (2) Catch-all regex layer over every string in the tree.
    if !cfg.disable_regex_phi {
        walk_strings_mut(value, &mut |s, path| apply_regexes(s, path, &mut findings));
    }

    // (3) Annotation.
    if cfg.annotate_findings && !findings.is_empty() {
        if let Some(obj) = value.as_object_mut() {
            // De-duplicate findings while preserving order.
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            let unique: Vec<Value> = findings
                .iter()
                .filter(|p| seen.insert(p.as_str()))
                .map(|p| Value::String(p.clone()))
                .collect();
            obj.insert("_phi_findings".into(), Value::Array(unique));
        }
    }

    findings
}

fn effective_strategies(cfg: &PiiRedactConfig) -> Vec<(String, RedactionStrategy)> {
    let mut out: Vec<(String, RedactionStrategy)> = Vec::new();
    for (path, default_strat) in DEFAULT_PHI_PATHS {
        let strat = cfg
            .override_strategies
            .get(*path)
            .cloned()
            .unwrap_or_else(|| default_strat.clone());
        out.push(((*path).to_string(), strat));
    }
    for (path, strat) in &cfg.extra_paths {
        out.push((path.clone(), strat.clone()));
    }
    out
}

/// Walk a possibly-wildcarded JSON Pointer (`*` matches any array
/// index) and apply `strat` at every leaf the pointer resolves to.
///
/// For wildcard expansion: split the pointer on `/`, descend
/// recursively; on a `*` segment, iterate array indices.
fn apply_at_pointer(
    value: &mut Value,
    pointer: &str,
    strat: &RedactionStrategy,
    findings: &mut Vec<String>,
) {
    let segments: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    descend(value, &segments, "", strat, findings);
}

fn descend(
    value: &mut Value,
    segments: &[&str],
    path_so_far: &str,
    strat: &RedactionStrategy,
    findings: &mut Vec<String>,
) {
    let Some((head, rest)) = segments.split_first() else {
        // No more segments — apply the strategy here.
        if strat.apply(value) || matches!(strat, RedactionStrategy::LogOnly) {
            findings.push(path_so_far.to_string());
        }
        return;
    };

    match value {
        Value::Object(map) => {
            if let Some(child) = map.get_mut(*head) {
                let new_path = format!("{path_so_far}/{head}");
                descend(child, rest, &new_path, strat, findings);
            }
        }
        Value::Array(arr) if *head == "*" => {
            for (i, child) in arr.iter_mut().enumerate() {
                let new_path = format!("{path_so_far}/{i}");
                descend(child, rest, &new_path, strat, findings);
            }
        }
        Value::Array(arr) => {
            if let Ok(idx) = head.parse::<usize>() {
                if let Some(child) = arr.get_mut(idx) {
                    let new_path = format!("{path_so_far}/{idx}");
                    descend(child, rest, &new_path, strat, findings);
                }
            }
        }
        _ => {}
    }
}

/// Walk every string value in the tree, calling `visit` to optionally
/// mutate it. `visit` returns the new string (or `None` to leave as-is)
/// and pushes any finding path itself.
fn walk_strings_mut(value: &mut Value, visit: &mut dyn FnMut(&str, &str) -> Option<String>) {
    walk_strings_mut_inner(value, "", visit);
}

fn walk_strings_mut_inner(
    value: &mut Value,
    path: &str,
    visit: &mut dyn FnMut(&str, &str) -> Option<String>,
) {
    match value {
        Value::String(s) => {
            if let Some(new_s) = visit(s, path) {
                *value = Value::String(new_s);
            }
        }
        Value::Object(map) => {
            // Clone keys to avoid borrow conflict during iteration.
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                if let Some(child) = map.get_mut(&k) {
                    let new_path = format!("{path}/{k}");
                    walk_strings_mut_inner(child, &new_path, visit);
                }
            }
        }
        Value::Array(arr) => {
            for (i, child) in arr.iter_mut().enumerate() {
                let new_path = format!("{path}/{i}");
                walk_strings_mut_inner(child, &new_path, visit);
            }
        }
        _ => {}
    }
}

/// Run all 5 regexes against `s`. Returns `Some(new_string)` if any
/// match was replaced, else `None`. Records each matched pointer in
/// `findings` so the caller can dedup-annotate.
fn apply_regexes(s: &str, path: &str, findings: &mut Vec<String>) -> Option<String> {
    let r = regexes();
    let mut current = s.to_string();
    let mut touched = false;

    let replace_with = |re: &Regex, token: &str, current: &mut String, touched: &mut bool| {
        if re.is_match(current) {
            *current = re.replace_all(current, token).into_owned();
            *touched = true;
        }
    };

    replace_with(&r.ssn, "[REDACTED:SSN]", &mut current, &mut touched);
    replace_with(&r.plate, "[REDACTED:PLATE]", &mut current, &mut touched);
    replace_with(&r.ip, "[REDACTED:IP]", &mut current, &mut touched);
    replace_with(&r.url, "[REDACTED:URL]", &mut current, &mut touched);
    replace_with(&r.email, "[REDACTED:EMAIL]", &mut current, &mut touched);

    if touched {
        findings.push(format!("{path}@regex"));
        Some(current)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- Spec §8.1 PII unit cases ----

    #[test]
    fn default_paths_cover_18_hipaa_categories() {
        // Spec §4.5: 13 paths + 5 regex rules cover A-R of HIPAA Safe
        // Harbor. Confirm the path table has ≥ 13 entries (drift guard).
        assert!(DEFAULT_PHI_PATHS.len() >= 13);
        // Confirm the 5-rule regex set compiles and has all 5 patterns.
        let r = regexes();
        for re in [&r.ssn, &r.plate, &r.ip, &r.url, &r.email] {
            assert!(!re.as_str().is_empty());
        }
    }

    #[test]
    fn patient_name_tokenized() {
        let mut v = json!({
            "resourceType": "Patient",
            "name": [{"family": "Smith", "given": ["John"]}]
        });
        redact_value(&mut v, &PiiRedactConfig::default());
        assert_eq!(v["name"], json!("[REDACTED:NAME]"));
    }

    #[test]
    fn birthdate_truncated_to_year() {
        let mut v = json!({"resourceType": "Patient", "birthDate": "1955-03-15"});
        redact_value(&mut v, &PiiRedactConfig::default());
        assert_eq!(v["birthDate"], "1955");
    }

    #[test]
    fn ssn_regex_anywhere() {
        let mut v = json!({
            "resourceType": "Patient",
            "note": [{"text": "Contact 555-12-3456 for follow-up"}]
        });
        redact_value(&mut v, &PiiRedactConfig::default());
        let text = v["note"][0]["text"].as_str().unwrap();
        assert!(text.contains("[REDACTED:SSN]"), "text: {text}");
        assert!(!text.contains("555-12-3456"));
    }

    #[test]
    fn log_only_does_not_mutate_but_annotates() {
        let mut v = json!({"resourceType": "Patient", "name": [{"family": "Smith"}]});
        let cfg = PiiRedactConfig::log_only();
        redact_value(&mut v, &cfg);
        // Name unchanged.
        assert_eq!(v["name"], json!([{"family": "Smith"}]));
        // _phi_findings recorded.
        let findings = v["_phi_findings"].as_array().expect("findings");
        assert!(
            findings
                .iter()
                .any(|p| p.as_str().unwrap_or("").contains("/name"))
        );
    }

    #[test]
    fn generic_json_mode_skips_fhir_paths() {
        let mut v = json!({"user": "alice", "email": "a@b.com", "name": "Alice"});
        let mut cfg = PiiRedactConfig::default();
        cfg.fhir_aware = false;
        redact_value(&mut v, &cfg);
        // /name FHIR path skipped — "Alice" preserved as plain string.
        assert_eq!(v["name"], "Alice");
        assert_eq!(v["user"], "alice");
        // Email caught by regex layer.
        assert_eq!(v["email"], "[REDACTED:EMAIL]");
    }

    #[test]
    fn zip_prefix_3_truncates() {
        let mut v = json!({
            "resourceType": "Patient",
            "address": [{"postalCode": "94303", "line": ["1 Main St"]}]
        });
        redact_value(&mut v, &PiiRedactConfig::default());
        assert_eq!(v["address"][0]["postalCode"], "943");
        // /address/*/line should be stripped (Strip strategy → null).
        assert_eq!(v["address"][0]["line"], Value::Null);
    }

    #[test]
    fn disable_regex_phi_skips_regex_layer() {
        let mut v = json!({
            "resourceType": "Patient",
            "note": [{"text": "SSN 555-12-3456"}]
        });
        let mut cfg = PiiRedactConfig::default();
        cfg.disable_regex_phi = true;
        redact_value(&mut v, &cfg);
        assert!(
            v["note"][0]["text"]
                .as_str()
                .unwrap()
                .contains("555-12-3456")
        );
    }

    // ---- supplementary defensive tests ----

    #[test]
    fn telecom_array_replaced_by_token() {
        let mut v = json!({
            "resourceType": "Patient",
            "telecom": [{"system": "phone", "value": "555-1212"}]
        });
        redact_value(&mut v, &PiiRedactConfig::default());
        assert_eq!(v["telecom"], "[REDACTED:PHONE]");
    }

    #[test]
    fn photo_stripped_to_null() {
        let mut v = json!({
            "resourceType": "Patient",
            "photo": [{"contentType": "image/png", "data": "AAAA"}]
        });
        redact_value(&mut v, &PiiRedactConfig::default());
        assert_eq!(v["photo"], Value::Null);
    }

    #[test]
    fn url_regex_catches_http_links() {
        let mut v = json!({
            "resourceType": "DocumentReference",
            "content": [{"attachment": {"url": "https://example.com/patient/123"}}]
        });
        redact_value(&mut v, &PiiRedactConfig::default());
        // /url path: only fires on top-level /url, not nested. So this
        // must be caught by the regex layer.
        let url = v["content"][0]["attachment"]["url"].as_str().unwrap();
        assert!(url.contains("[REDACTED:URL]"), "got: {url}");
    }

    #[test]
    fn ip_regex_catches_v4_address() {
        let mut v = json!({"trace": "client 192.168.1.100 connected"});
        let mut cfg = PiiRedactConfig::default();
        cfg.fhir_aware = false;
        redact_value(&mut v, &cfg);
        let s = v["trace"].as_str().unwrap();
        assert!(s.contains("[REDACTED:IP]"));
        assert!(!s.contains("192.168.1.100"));
    }
}
