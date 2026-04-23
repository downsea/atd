//! OpenAI function-calling adapter.
//!
//! Emits the shape expected by the `tools` parameter of OpenAI's
//! chat-completions API:
//!
//! ```json
//! [
//!   {
//!     "type": "function",
//!     "function": {
//!       "name": "ref_shell_exec",
//!       "description": "...",
//!       "parameters": { /* JSON Schema */ }
//!     }
//!   }
//! ]
//! ```
//!
//! Note: `ToolSummary` does not carry `input_schema` (that field lives on
//! `ToolDefinition`). Tools emitted here will have an empty parameters
//! schema (`{"type":"object","properties":{}}`). For richer schemas, call
//! `client.describe(id)` to get the full `ToolDefinition` and use its
//! `input_schema` field directly.

use atd_types::ToolSummary;
use serde_json::{json, Value};

use crate::sanitize::sanitize_tool_name;

/// Convert a list of ATD tool summaries to OpenAI function-calling tools.
///
/// Each tool's `parameters` field uses an empty JSON Schema object because
/// `ToolSummary` does not include the full input schema. Use
/// `client.describe(id)` and `ToolDefinition.input_schema` for full schema
/// details.
pub fn as_openai_tools(summaries: &[ToolSummary]) -> Vec<Value> {
    summaries
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": sanitize_tool_name(&t.id),
                    "description": t.description,
                    "parameters": json!({"type": "object", "properties": {}}),
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_types::{ToolSummary, ToolTier, ToolVisibility};

    fn fake_summary(id: &str, desc: &str) -> ToolSummary {
        ToolSummary {
            id: id.into(),
            name: id.into(),
            description: desc.into(),
            domain: "test".into(),
            tier: ToolTier::Warm,
            visibility: ToolVisibility::Read,
            tags: vec![],
        }
    }

    #[test]
    fn empty_input_empty_output() {
        assert!(as_openai_tools(&[]).is_empty());
    }

    #[test]
    fn single_tool_has_openai_shape() {
        let out = as_openai_tools(&[fake_summary("ref:echo.say", "echo test")]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["type"], "function");
        assert_eq!(out[0]["function"]["name"], "ref_echo_say");
        assert_eq!(out[0]["function"]["description"], "echo test");
        assert!(out[0]["function"]["parameters"].is_object());
    }

    #[test]
    fn name_sanitization_applied() {
        let out = as_openai_tools(&[fake_summary("ref:fs.read", "")]);
        assert_eq!(out[0]["function"]["name"], "ref_fs_read");
    }
}
