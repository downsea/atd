use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ToolVisibility {
    #[default]
    #[serde(alias = "Read")]
    Read,
    #[serde(alias = "Write")]
    Write,
    #[serde(alias = "Dangerous")]
    Dangerous,
    #[serde(alias = "System")]
    System,
    /// Hidden from `Request::ToolList` discover responses, but still
    /// reachable by id via `Request::ToolSchema` (describe) and
    /// `Request::RunTool` (call). Use for tools that exist for
    /// integration tests, debugging, or advanced humans, but would add
    /// noise to an LLM's catalog.
    #[serde(alias = "Hidden")]
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ToolTier {
    #[serde(alias = "Hot")]
    Hot,
    #[serde(alias = "Warm")]
    Warm,
    #[serde(alias = "Cold")]
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "PascalCase")]
pub enum BindingProtocol {
    Cli,
    Mcp,
    AppFunction,
    Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SafetyLevel {
    Read = 0,
    Write = 1,
    Financial = 2,
    Privacy = 3,
    Physical = 4,
    Destructive = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum TrustLevel {
    L0Unverified = 0,
    L1SchemaValid = 1,
    L2Tested = 2,
    L3Verified = 3,
    L4Certified = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ToolVisibility::Dangerous).unwrap(),
            "\"dangerous\""
        );
    }

    #[test]
    fn visibility_default_is_read() {
        assert_eq!(ToolVisibility::default(), ToolVisibility::Read);
    }

    #[test]
    fn visibility_hidden_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ToolVisibility::Hidden).unwrap(),
            "\"hidden\""
        );
    }

    #[test]
    fn visibility_hidden_round_trips() {
        let parsed: ToolVisibility = serde_json::from_str("\"hidden\"").unwrap();
        assert_eq!(parsed, ToolVisibility::Hidden);
    }

    #[test]
    fn tier_ordering_is_hot_warm_cold() {
        assert!(ToolTier::Hot < ToolTier::Warm);
        assert!(ToolTier::Warm < ToolTier::Cold);
    }

    #[test]
    fn binding_protocol_pascal_case() {
        assert_eq!(
            serde_json::to_string(&BindingProtocol::AppFunction).unwrap(),
            "\"AppFunction\""
        );
    }

    #[test]
    fn safety_level_ord() {
        assert!(SafetyLevel::Read < SafetyLevel::Destructive);
    }

    #[test]
    fn trust_level_ord() {
        assert!(TrustLevel::L0Unverified < TrustLevel::L4Certified);
    }
}
