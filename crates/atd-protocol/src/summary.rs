use serde::{Deserialize, Serialize};

use crate::enums::{ToolTier, ToolVisibility};
use crate::tool::ToolDefinition;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolSummary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub visibility: ToolVisibility,
    #[serde(default = "default_tier")]
    pub tier: ToolTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

fn default_tier() -> ToolTier {
    ToolTier::Warm
}

impl From<&ToolDefinition> for ToolSummary {
    fn from(def: &ToolDefinition) -> Self {
        Self {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            domain: def.capability.domain.clone(),
            tags: def.capability.tags.clone(),
            visibility: def.visibility,
            // SP-12: propagate the definition's tier when set; fall back to
            // Warm (the summary default) when absent.
            tier: def.tier.unwrap_or_else(default_tier),
            input_schema: Some(def.input_schema.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::{BindingProtocol, SafetyLevel, TrustLevel};
    use crate::tool::{ToolCapability, ToolResources, ToolSafety, ToolTrust};

    fn def() -> ToolDefinition {
        ToolDefinition {
            id: "anos:fs.read".into(),
            name: "Read File".into(),
            description: "desc".into(),
            version: "0.1.0".into(),
            capability: ToolCapability {
                domain: "fs".into(),
                actions: vec!["read".into()],
                tags: vec!["filesystem".into()],
                intent_examples: vec![],
            },
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            bindings: vec![crate::tool::ToolBinding {
                protocol: BindingProtocol::Cli,
                config: serde_json::json!({}),
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
                publisher: "anos".into(),
                trust_level: TrustLevel::L2Tested,
                signature: None,
            },
            visibility: ToolVisibility::Read,
            required_capabilities: vec![],
            tier: None,
            errors: vec![],
        }
    }

    #[test]
    fn summary_is_derivable_from_definition() {
        let s = ToolSummary::from(&def());
        assert_eq!(s.id, "anos:fs.read");
        assert_eq!(s.domain, "fs");
        assert_eq!(s.tags, vec!["filesystem"]);
        assert_eq!(s.tier, ToolTier::Warm);
        // input_schema is populated from the definition
        assert!(s.input_schema.is_some());
    }

    #[test]
    fn input_schema_absent_in_wire_when_none() {
        // A ToolSummary with input_schema: None must not emit the field in JSON
        let s = ToolSummary {
            id: "x".into(),
            name: "x".into(),
            description: "d".into(),
            domain: "d".into(),
            tags: vec![],
            visibility: ToolVisibility::Read,
            tier: ToolTier::Warm,
            input_schema: None,
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(
            !j.contains("input_schema"),
            "field should be absent when None: {j}"
        );
    }

    #[test]
    fn summary_roundtrip_json() {
        let s = ToolSummary::from(&def());
        let j = serde_json::to_string(&s).unwrap();
        let back: ToolSummary = serde_json::from_str(&j).unwrap();
        assert_eq!(back.id, s.id);
    }

    #[test]
    fn missing_tier_defaults_to_warm() {
        let j = r#"{"id":"a","name":"A","description":"d","domain":"x","tags":[]}"#;
        let s: ToolSummary = serde_json::from_str(j).unwrap();
        assert_eq!(s.tier, ToolTier::Warm);
        assert_eq!(s.visibility, ToolVisibility::Read);
    }

    #[test]
    fn summary_parses_anos_shape_with_missing_name_domain_tags() {
        let j = r#"{"id":"anos:fs.read","description":"File Read","tier":"hot","visibility":"read","lifecycle":"Active"}"#;
        let s: ToolSummary = serde_json::from_str(j).unwrap();
        assert_eq!(s.id, "anos:fs.read");
        assert_eq!(s.description, "File Read");
        assert_eq!(s.name, ""); // defaults to empty; discover() fills in
        assert_eq!(s.domain, ""); // defaults to empty; discover() fills in
        assert!(s.tags.is_empty());
        // lifecycle is not a ToolSummary field — serde ignores unknown keys by default
    }
}
