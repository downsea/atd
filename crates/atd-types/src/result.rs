use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::BindingProtocol;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMetadata {
    pub tool_id: String,
    pub version: String,
    pub binding: BindingProtocol,
    pub latency_ms: u64,
    pub timestamp: DateTime<Utc>,
    pub request_id: ulid::Ulid,
}

impl ToolResult {
    pub fn is_success(&self) -> bool {
        matches!(self, ToolResult::Success { .. })
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, ToolResult::Error { retryable: true, .. })
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
            metadata: ToolResultMetadata {
                tool_id: "anos:fs.read".into(),
                version: "0.1.0".into(),
                binding: BindingProtocol::Cli,
                latency_ms: 3,
                timestamp: Utc::now(),
                request_id: ulid::Ulid::new(),
            },
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
}
