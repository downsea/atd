//! SP-observability-completeness-v1 Axis D conformance.
//!
//! Locks the schemars output: the two declarative-only fields
//! (`ToolResources::rate_limit_per_min`, `ToolTrust::trust_level`) MUST
//! carry their advisory / self-declared caveat in the published
//! `/atd-protocol-schema.json`, so SDK auto-doc and IDE hover surface that
//! neither field is enforced/verified. A future doc-comment edit that drops
//! the caveat (and a stale regen) fails here.

use std::path::PathBuf;

fn schema() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../atd-protocol-schema.json");
    let text = std::fs::read_to_string(&path).expect("read atd-protocol-schema.json");
    serde_json::from_str(&text).expect("parse atd-protocol-schema.json")
}

fn field_description(schema: &serde_json::Value, type_name: &str, field: &str) -> String {
    schema["definitions"][type_name]["properties"][field]["description"]
        .as_str()
        .unwrap_or_else(|| panic!("{type_name}.{field} has no `description` in the schema"))
        .to_string()
}

#[test]
fn rate_limit_per_min_states_advisory_only() {
    let desc = field_description(&schema(), "ToolResources", "rate_limit_per_min");
    assert!(
        desc.contains("Advisory only"),
        "ToolResources.rate_limit_per_min description must state it is advisory-only \
         (not enforced by dispatch); got: {desc}"
    );
}

#[test]
fn trust_level_states_self_declared() {
    let desc = field_description(&schema(), "ToolTrust", "trust_level");
    assert!(
        desc.contains("self-declared"),
        "ToolTrust.trust_level description must state it is publisher self-declared \
         (not ATD-verified); got: {desc}"
    );
}
