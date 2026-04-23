//! Anthropic Messages API tool-use adapter.
//!
//! Emits the shape expected by Anthropic's `tools` parameter:
//!
//! ```json
//! [
//!   {
//!     "name": "ref_shell_exec",
//!     "description": "...",
//!     "input_schema": { /* JSON Schema */ }
//!   }
//! ]
//! ```
//!
//! Note: `ToolSummary` does not carry `input_schema` (that field lives on
//! `ToolDefinition`). Tools emitted here will have an empty `input_schema`
//! (`{"type":"object","properties":{}}`). For richer schemas, call
//! `client.describe(id)` to get the full `ToolDefinition` and use its
//! `input_schema` field directly.

use atd_types::ToolSummary;
use serde_json::{json, Value};

use crate::sanitize::sanitize_tool_name;

/// Convert a list of ATD tool summaries to Anthropic Messages API tools.
///
/// Each tool's `input_schema` field uses an empty JSON Schema object because
/// `ToolSummary` does not include the full input schema. Use
/// `client.describe(id)` and `ToolDefinition.input_schema` for full schema
/// details.
pub fn as_anthropic_tools(summaries: &[ToolSummary]) -> Vec<Value> {
    summaries
        .iter()
        .map(|t| {
            json!({
                "name": sanitize_tool_name(&t.id),
                "description": t.description,
                "input_schema": json!({"type": "object", "properties": {}}),
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
        assert!(as_anthropic_tools(&[]).is_empty());
    }

    #[test]
    fn single_tool_has_anthropic_shape() {
        let out = as_anthropic_tools(&[fake_summary("ref:fs.read", "read a file")]);
        assert_eq!(out.len(), 1);
        // Anthropic shape: name/description/input_schema at top level,
        // no "function" wrapper, no "type: function".
        assert_eq!(out[0]["name"], "ref_fs_read");
        assert_eq!(out[0]["description"], "read a file");
        assert!(out[0]["input_schema"].is_object());
        assert!(out[0].get("function").is_none());
        assert!(out[0].get("type").is_none());
    }

    #[test]
    fn name_sanitization_applied() {
        let out = as_anthropic_tools(&[fake_summary("xiaomi:light.toggle", "")]);
        assert_eq!(out[0]["name"], "xiaomi_light_toggle");
    }
}
