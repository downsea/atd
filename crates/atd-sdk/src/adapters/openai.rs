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
//! `ToolSummary.input_schema` carries the JSON Schema when the server
//! populates it. If absent, adapters fall back to an empty schema stub
//! (`{"type":"object","properties":{}}`). For full schema details, call
//! `client.describe(id)` to get the `ToolDefinition`.

use atd_protocol::ToolSummary;
use serde_json::{json, Value};

use atd_protocol::sanitize::sanitize_tool_name;

/// Convert a list of ATD tool summaries to OpenAI function-calling tools.
///
/// Each tool's `parameters` field uses `ToolSummary.input_schema` when
/// present, falling back to an empty JSON Schema stub when the server did
/// not populate it.
pub fn as_openai_tools(summaries: &[ToolSummary]) -> Vec<Value> {
    summaries
        .iter()
        .map(|t| {
            let parameters = t.input_schema.clone().unwrap_or_else(
                || serde_json::json!({"type": "object", "properties": {}}),
            );
            json!({
                "type": "function",
                "function": {
                    "name": sanitize_tool_name(&t.id),
                    "description": t.description,
                    "parameters": parameters,
                },
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
        assert_eq!(out[0]["function"]["parameters"]["properties"]["text"]["type"], "string");
    }

    #[test]
    fn name_sanitization_applied() {
        let out = as_openai_tools(&[fake_summary("ref:fs.read", "")]);
        assert_eq!(out[0]["function"]["name"], "ref_fs_read");
    }

    #[test]
    fn falls_back_to_empty_schema_when_input_schema_is_none() {
        let mut s = fake_summary("ref:no.schema", "no schema");
        s.input_schema = None;
        let out = as_openai_tools(&[s]);
        assert_eq!(out[0]["function"]["parameters"], serde_json::json!({"type": "object", "properties": {}}));
    }
}
