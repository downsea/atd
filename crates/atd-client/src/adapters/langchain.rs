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
//! Emitting plain JSON keeps `atd-client` stable.
//!
//! Note: `ToolSummary` does not carry `input_schema` (that field lives on
//! `ToolDefinition`). Tools emitted here will have an empty parameters
//! schema (`{"type":"object","properties":{}}`). For richer schemas, call
//! `client.describe(id)` to get the full `ToolDefinition` and use its
//! `input_schema` field directly.

use atd_types::ToolSummary;
use serde_json::{json, Value};

use crate::sanitize::sanitize_tool_name;

/// Convert ATD tool summaries to LangChain-compatible JSON (OpenAI shape).
///
/// Each tool's `parameters` field uses an empty JSON Schema object because
/// `ToolSummary` does not include the full input schema. Use
/// `client.describe(id)` and `ToolDefinition.input_schema` for full schema
/// details.
pub fn as_langchain_tools(summaries: &[ToolSummary]) -> Vec<Value> {
    // Mirrors the OpenAI adapter shape. Kept independent so that callers
    // can enable only the `langchain` feature without also enabling `openai`.
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
    }

    #[test]
    fn name_sanitization_applied() {
        let out = as_langchain_tools(&[fake_summary("a:b.c", "")]);
        assert_eq!(out[0]["function"]["name"], "a_b_c");
    }
}
