use serde::{Deserialize, Serialize};

use crate::enums::{BindingProtocol, SafetyLevel, ToolVisibility, TrustLevel};

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapability {
    pub domain: String,
    pub actions: Vec<String>,
    pub tags: Vec<String>,
    pub intent_examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolBinding {
    pub protocol: BindingProtocol,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSafety {
    pub level: SafetyLevel,
    pub dry_run: bool,
    pub side_effects: Vec<String>,
    pub data_sensitivity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResources {
    pub timeout_ms: u64,
    pub max_concurrent: u32,
    pub rate_limit_per_min: Option<u32>,
    pub estimated_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTrust {
    pub publisher: String,
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
}
