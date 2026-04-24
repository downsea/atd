//! LangChain (Rust) tool adapter.
//!
//! Emits OpenAI-shape JSON. langchain-rust's `AgentExecutor` accepts
//! OpenAI-compatible tool definitions when the underlying LLM is an
//! OpenAI-compatible model (which covers most current usage). If your
//! LangChain Rust setup needs a provider-specific shape, use
//! `as_anthropic_tools` instead.
//!
//! Why no direct langchain-rust type dependency? langchain-rust is
//! pre-1.0 and its public API surface changes across minor versions.
//! Emitting plain JSON keeps `atd-sdk` stable.
//!
//! `ToolSummary.input_schema` carries the JSON Schema when the server
//! populates it. If absent, adapters fall back to an empty schema stub
//! (`{"type":"object","properties":{}}`). For full schema details, call
//! `client.describe(id)` to get the `ToolDefinition`.

use atd_protocol::ToolSummary;
use serde_json::{Value, json};

use atd_protocol::sanitize::sanitize_tool_name;

/// Convert ATD tool summaries to LangChain-compatible JSON (OpenAI shape).
///
/// Each tool's `parameters` field uses `ToolSummary.input_schema` when
/// present, falling back to an empty JSON Schema stub when the server did
/// not populate it.
pub fn as_langchain_tools(summaries: &[ToolSummary]) -> Vec<Value> {
    // Mirrors the OpenAI adapter shape. Kept independent so that callers
    // can enable only the `langchain` feature without also enabling `openai`.
    summaries
        .iter()
        .map(|t| {
            let parameters = t
                .input_schema
                .clone()
                .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
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
        assert!(as_langchain_tools(&[]).is_empty());
    }

    #[test]
    fn emits_openai_compatible_shape() {
        let out = as_langchain_tools(&[fake_summary("ref:echo.say", "echo")]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["type"], "function");
        assert_eq!(out[0]["function"]["name"], "ref_echo_say");
        assert_eq!(out[0]["function"]["description"], "echo");
        assert!(out[0]["function"]["parameters"].is_object());
        assert_eq!(
            out[0]["function"]["parameters"]["properties"]["text"]["type"],
            "string"
        );
    }

    #[test]
    fn name_sanitization_applied() {
        let out = as_langchain_tools(&[fake_summary("a:b.c", "")]);
        assert_eq!(out[0]["function"]["name"], "a_b_c");
    }

    #[test]
    fn falls_back_to_empty_schema_when_input_schema_is_none() {
        let mut s = fake_summary("ref:no.schema", "no schema");
        s.input_schema = None;
        let out = as_langchain_tools(&[s]);
        assert_eq!(
            out[0]["function"]["parameters"],
            serde_json::json!({"type": "object", "properties": {}})
        );
    }
}
