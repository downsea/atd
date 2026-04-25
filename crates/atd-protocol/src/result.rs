use serde::{Deserialize, Serialize};

use crate::enums::BindingProtocol;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolResult {
    Success {
        data: serde_json::Value,
        metadata: ToolResultMetadata,
    },
    Error {
        code: String,
        message: String,
        reason: Option<String>,
        retryable: bool,
    },
}

/// Metadata attached to a successful tool result.
///
/// Only `tool_id` is required — servers always echo it. All other fields are
/// server-populated and optional: clients must not synthesize them, because a
/// fabricated `timestamp`/`request_id` would silently masquerade as server
/// truth. `timestamp` and `request_id` are kept as opaque strings so this crate
/// has no dependency on a specific datetime or ULID implementation; callers
/// that need typed access can parse with their preferred library.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolResultMetadata {
    pub tool_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<BindingProtocol>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// ISO-8601 / RFC-3339 timestamp, server-populated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Opaque request identifier (ULID, UUID, or whatever the server emits).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ToolResultMetadata {
    /// Construct minimal metadata with only `tool_id` set.
    /// The canonical way for clients to create metadata — everything else
    /// is for the server to fill in.
    pub fn for_tool(tool_id: impl Into<String>) -> Self {
        Self {
            tool_id: tool_id.into(),
            version: None,
            binding: None,
            latency_ms: None,
            timestamp: None,
            request_id: None,
        }
    }
}

impl ToolResult {
    pub fn is_success(&self) -> bool {
        matches!(self, ToolResult::Success { .. })
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ToolResult::Error {
                retryable: true,
                ..
            }
        )
    }

    pub fn data(&self) -> Option<&serde_json::Value> {
        match self {
            ToolResult::Success { data, .. } => Some(data),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn success() -> ToolResult {
        ToolResult::Success {
            data: serde_json::json!({"content": "hello"}),
            metadata: ToolResultMetadata::for_tool("anos:fs.read"),
        }
    }

    #[test]
    fn success_roundtrip() {
        let r = success();
        let j = serde_json::to_string(&r).unwrap();
        let back: ToolResult = serde_json::from_str(&j).unwrap();
        assert!(back.is_success());
        assert_eq!(back.data().unwrap()["content"], "hello");
    }

    #[test]
    fn error_retryable() {
        let r = ToolResult::Error {
            code: "TIMEOUT".into(),
            message: "timed out".into(),
            reason: None,
            retryable: true,
        };
        assert!(!r.is_success());
        assert!(r.is_retryable());
    }

    #[test]
    fn status_tag_uses_snake_case() {
        let j = serde_json::to_string(&success()).unwrap();
        assert!(j.contains("\"status\":\"success\""), "got: {j}");
    }

    #[test]
    fn metadata_with_only_tool_id_serializes_without_null_fields() {
        let m = ToolResultMetadata::for_tool("anos:fs.read");
        let j = serde_json::to_string(&m).unwrap();
        assert_eq!(j, r#"{"tool_id":"anos:fs.read"}"#);
    }

    #[test]
    fn metadata_roundtrips_with_missing_optional_fields() {
        let j = r#"{"tool_id":"anos:fs.read"}"#;
        let m: ToolResultMetadata = serde_json::from_str(j).unwrap();
        assert_eq!(m.tool_id, "anos:fs.read");
        assert!(m.version.is_none());
        assert!(m.binding.is_none());
        assert!(m.latency_ms.is_none());
        assert!(m.timestamp.is_none());
        assert!(m.request_id.is_none());
    }

    #[test]
    fn metadata_roundtrips_with_server_populated_fields() {
        let j = r#"{"tool_id":"anos:fs.read","version":"0.1.2","binding":"Cli","latency_ms":7,"timestamp":"2026-04-21T10:00:00Z","request_id":"01HY..."}"#;
        let m: ToolResultMetadata = serde_json::from_str(j).unwrap();
        assert_eq!(m.tool_id, "anos:fs.read");
        assert_eq!(m.version.as_deref(), Some("0.1.2"));
        assert_eq!(m.binding, Some(BindingProtocol::Cli));
        assert_eq!(m.latency_ms, Some(7));
        assert!(m.timestamp.is_some());
        assert!(m.request_id.is_some());
    }
}
