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
//! `ToolSummary.input_schema` carries the JSON Schema when the server
//! populates it. If absent, adapters fall back to an empty schema stub
//! (`{"type":"object","properties":{}}`). For full schema details, call
//! `client.describe(id)` to get the `ToolDefinition`.

use atd_protocol::ToolSummary;
use serde_json::{json, Value};

use atd_protocol::sanitize::sanitize_tool_name;

/// Convert a list of ATD tool summaries to Anthropic Messages API tools.
///
/// Each tool's `input_schema` field uses `ToolSummary.input_schema` when
/// present, falling back to an empty JSON Schema stub when the server did
/// not populate it.
pub fn as_anthropic_tools(summaries: &[ToolSummary]) -> Vec<Value> {
    summaries
        .iter()
        .map(|t| {
            let input_schema = t.input_schema.clone().unwrap_or_else(
                || serde_json::json!({"type": "object", "properties": {}}),
            );
            json!({
                "name": sanitize_tool_name(&t.id),
                "description": t.description,
                "input_schema": input_schema,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_protocol::{ToolSummary, ToolTier, ToolVisibility};

    fn fake_summary(id: &str, desc: &str) -> ToolSummary {
        ToolSummary {
            id: id.into(),
            name: id.into(),
            description: desc.into(),
            domain: "test".into(),
            tier: ToolTier::Warm,
            visibility: ToolVisibility::Read,
            tags: vec![],
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"],
            })),
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
        assert_eq!(out[0]["input_schema"]["properties"]["text"]["type"], "string");
        assert!(out[0].get("function").is_none());
        assert!(out[0].get("type").is_none());
    }

    #[test]
    fn name_sanitization_applied() {
        let out = as_anthropic_tools(&[fake_summary("xiaomi:light.toggle", "")]);
        assert_eq!(out[0]["name"], "xiaomi_light_toggle");
    }

    #[test]
    fn falls_back_to_empty_schema_when_input_schema_is_none() {
        let mut s = fake_summary("ref:no.schema", "no schema");
        s.input_schema = None;
        let out = as_anthropic_tools(&[s]);
        assert_eq!(out[0]["input_schema"], serde_json::json!({"type": "object", "properties": {}}));
    }
}
