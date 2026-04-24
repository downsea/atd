//! LLM-provider adapters.
//!
//! Feature-gated — none compile by default. Opt-in via:
//!   cargo add atd-sdk --features adapters   (all three)
//!   cargo add atd-sdk --features openai     (just OpenAI)
//!   cargo add atd-sdk --features anthropic  (just Anthropic)
//!   cargo add atd-sdk --features langchain  (just LangChain)
//!
//! Adapters return `serde_json::Value` shaped for each provider's API.
//! Callers feed the output directly into their provider SDK (typically
//! one line of `.tools(output.into_iter().map(…))`).

#[cfg(feature = "openai")]
pub mod openai;

#[cfg(feature = "anthropic")]
pub mod anthropic;

#[cfg(feature = "langchain")]
pub mod langchain;

use atd_protocol::ToolSummary;

/// Given a sanitized name returned by an LLM, find the original ATD tool id.
/// Convenience wrapper over `sanitize::desanitize_tool_name` that takes a
/// slice of `ToolSummary` (the common return type from `client.discover()`).
pub fn resolve_sanitized_id<'a>(sanitized: &str, known: &'a [ToolSummary]) -> Option<&'a str> {
    let ids = known.iter().map(|t| t.id.as_str());
    atd_protocol::sanitize::desanitize_tool_name(sanitized, ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_protocol::{ToolSummary, ToolTier, ToolVisibility};

    fn make_summary(id: &str) -> ToolSummary {
        ToolSummary {
            id: id.into(),
            name: id.into(),
            description: "test tool".into(),
            domain: "test".into(),
            tier: ToolTier::Warm,
            visibility: ToolVisibility::Read,
            tags: vec![],
            input_schema: None,
        }
    }

    #[test]
    fn resolve_known_id_returns_original() {
        let summaries = vec![make_summary("ref:fs.read"), make_summary("ref:shell.exec")];
        assert_eq!(
            resolve_sanitized_id("ref_fs_read", &summaries),
            Some("ref:fs.read")
        );
    }

    #[test]
    fn resolve_unknown_returns_none() {
        let summaries = vec![make_summary("ref:fs.read")];
        assert_eq!(resolve_sanitized_id("nonexistent", &summaries), None);
    }

    #[test]
    fn resolve_empty_slice_returns_none() {
        assert_eq!(resolve_sanitized_id("ref_fs_read", &[]), None);
    }
}
