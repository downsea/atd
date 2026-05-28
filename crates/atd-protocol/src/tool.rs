use serde::{Deserialize, Serialize};

use crate::enums::{BindingProtocol, SafetyLevel, ToolVisibility, TrustLevel};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,

    pub capability: ToolCapability,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,

    pub bindings: Vec<ToolBinding>,
    pub safety: ToolSafety,
    pub resources: ToolResources,
    pub trust: ToolTrust,

    #[serde(default)]
    pub visibility: ToolVisibility,

    /// Capabilities a caller must hold for this tool to be invoked.
    /// Enforced by the server's dispatch layer (SP-12). Empty = unrestricted.
    /// The existing `capability` field above is a *descriptor* (domain, actions,
    /// intent examples); this is the *enforcement* list — intentionally
    /// separate to avoid overloading the schema.
    #[serde(default)]
    pub required_capabilities: Vec<String>,

    /// Execution tier hint used by the dispatch layer to derive deadline and
    /// max-output budgets. Absent / unknown values default to `Warm` on the
    /// server side (SP-12 back-compat). The client-facing `ToolSummary` carries
    /// an equivalent field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<crate::enums::ToolTier>,

    /// Domain errors this tool may emit. Optional; missing on the wire =
    /// empty. Surfaces only via `describe`, never via `discover` (kept off
    /// `ToolSummary`).
    #[serde(default)]
    pub errors: Vec<ToolErrorDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolErrorDef {
    /// SCREAMING_SNAKE error code, e.g. "FILE_NOT_FOUND".
    pub code: String,
    pub description: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolCapability {
    pub domain: String,
    pub actions: Vec<String>,
    pub tags: Vec<String>,
    pub intent_examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolBinding {
    pub protocol: BindingProtocol,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolSafety {
    pub level: SafetyLevel,
    pub dry_run: bool,
    pub side_effects: Vec<String>,
    pub data_sensitivity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolResources {
    pub timeout_ms: u64,
    pub max_concurrent: u32,
    /// **Advisory only in v1 — NOT enforced by dispatch.** The only
    /// enforced concurrency control is `max_concurrent` (a per-tool
    /// semaphore in the `atd-runtime` registry). Adopters needing real
    /// per-minute rate limiting compose their own limiter (e.g. the
    /// `governor` crate) outside dispatch. A future SP may make this
    /// field enforced; adopters relying on advisory-only behaviour should
    /// re-audit when that lands. See architecture §10.7.
    pub rate_limit_per_min: Option<u32>,
    pub estimated_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolTrust {
    pub publisher: String,
    /// **Publisher self-declared in v1 — ATD does NOT verify trust level.**
    /// `L4Certified` means "the publisher claims certification", not "ATD
    /// verified it". Use only as a hint to higher layers; do NOT base a
    /// security decision on this field alone. A future SP may add
    /// publisher-key PKI verification. See architecture §6.1 / §10.3.
    pub trust_level: TrustLevel,
    pub signature: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ToolDefinition {
        ToolDefinition {
            id: "anos:fs.read".into(),
            name: "Read File".into(),
            description: "Read a file from disk.".into(),
            version: "0.1.0".into(),
            capability: ToolCapability {
                domain: "fs".into(),
                actions: vec!["read".into()],
                tags: vec!["filesystem".into()],
                intent_examples: vec!["read config.toml".into()],
            },
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
            output_schema: serde_json::json!({"type": "string"}),
            bindings: vec![ToolBinding {
                protocol: BindingProtocol::Cli,
                config: serde_json::json!({"cmd": "cat"}),
            }],
            safety: ToolSafety {
                level: SafetyLevel::Read,
                dry_run: false,
                side_effects: vec![],
                data_sensitivity: None,
            },
            resources: ToolResources {
                timeout_ms: 5_000,
                max_concurrent: 8,
                rate_limit_per_min: None,
                estimated_tokens: Some(100),
            },
            trust: ToolTrust {
                publisher: "anos".into(),
                trust_level: TrustLevel::L3Verified,
                signature: None,
            },
            visibility: ToolVisibility::Read,
            required_capabilities: vec![],
            tier: None,
            errors: vec![],
        }
    }

    #[test]
    fn tool_definition_roundtrip() {
        let t = sample();
        let json = serde_json::to_string(&t).unwrap();
        let back: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, t.id);
        assert_eq!(back.capability.domain, "fs");
        assert_eq!(back.safety.level, SafetyLevel::Read);
    }

    #[test]
    fn visibility_defaults_when_missing_in_json() {
        let mut v = serde_json::to_value(sample()).unwrap();
        v.as_object_mut().unwrap().remove("visibility");
        let back: ToolDefinition = serde_json::from_value(v).unwrap();
        assert_eq!(back.visibility, ToolVisibility::Read);
    }

    #[test]
    fn required_capabilities_defaults_to_empty_when_missing() {
        // Pre-SP-12 serialized definitions omit the field; must round-trip.
        let mut v = serde_json::to_value(sample()).unwrap();
        v.as_object_mut().unwrap().remove("required_capabilities");
        let back: ToolDefinition = serde_json::from_value(v).unwrap();
        assert!(back.required_capabilities.is_empty());
    }

    #[test]
    fn required_capabilities_roundtrip_when_set() {
        let mut t = sample();
        t.required_capabilities = vec!["exec".into(), "read".into()];
        let j = serde_json::to_string(&t).unwrap();
        let back: ToolDefinition = serde_json::from_str(&j).unwrap();
        assert_eq!(back.required_capabilities, vec!["exec", "read"]);
    }

    #[test]
    fn tier_omitted_when_none() {
        let j = serde_json::to_string(&sample()).unwrap();
        assert!(
            !j.contains("\"tier\""),
            "tier should be skipped when None: {j}"
        );
    }
}
