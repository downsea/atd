use thiserror::Error;

#[derive(Debug, Error)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum AtdError {
    #[error("tool not found: {tool_id}")]
    ToolNotFound {
        tool_id: String,
        suggestions: Vec<String>,
    },

    #[error("invalid arguments for {tool_id}: field `{field}` — {reason}")]
    InvalidArguments {
        tool_id: String,
        field: String,
        reason: String,
    },

    #[error("capability denied for {tool_id}: required={required:?} granted={granted:?}")]
    CapabilityDenied {
        tool_id: String,
        required: Vec<String>,
        granted: Vec<String>,
    },

    #[error("no binding available for {tool_id}: tried={tried:?} ({reason})")]
    BindingUnavailable {
        tool_id: String,
        tried: Vec<String>,
        reason: String,
    },

    #[error("tool execution failed: {tool_id}")]
    // `inner` is a boxed trait object that JsonSchema cannot describe;
    // skip this variant entirely in the generated schema.
    #[cfg_attr(feature = "schema", schemars(skip))]
    ToolExecutionFailed {
        tool_id: String,
        #[source]
        inner: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("timed out calling {tool_id} after {after_ms}ms")]
    Timeout { tool_id: String, after_ms: u64 },

    #[error("server unreachable: {0}")]
    #[cfg_attr(feature = "schema", schemars(skip))]
    ServerUnreachable(#[from] std::io::Error),

    #[error("not implemented: {feature}")]
    NotImplemented { feature: String },

    #[error("protocol error: expected {expected}, got {got}")]
    ProtocolError { expected: String, got: String },

    /// SP-pagination-v1 §4.8 — `AtdClient::call_all` hit either `max_pages`
    /// or `max_total_bytes` before exhausting cursors. Callers can decide
    /// whether to treat partial as success.
    #[error("pagination limit exceeded: fetched {pages_fetched} pages / {bytes_fetched} bytes")]
    #[cfg_attr(feature = "schema", schemars(skip))]
    PaginationLimitExceeded {
        pages_fetched: u32,
        bytes_fetched: usize,
    },

    /// SP-pagination-v1 §4.8 — `MergePolicy` couldn't combine pages
    /// (e.g., `ConcatArray` but a page wasn't an array; `ConcatField`
    /// but the named field was missing).
    #[error("page merge failed: {reason}")]
    MergeFailed { reason: String },
}

impl AtdError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AtdError::Timeout { .. }
                | AtdError::ServerUnreachable(_)
                | AtdError::BindingUnavailable { .. }
        )
    }

    pub fn suggest_fix(&self) -> Option<String> {
        match self {
            AtdError::ToolNotFound { suggestions, .. } if !suggestions.is_empty() => {
                Some(format!("did you mean '{}'?", suggestions[0]))
            }
            AtdError::ToolNotFound { .. } => {
                Some("try `atd list --query <keyword>` to find available tools".into())
            }
            AtdError::CapabilityDenied { tool_id, .. } => Some(format!(
                "run `atd allow {tool_id}` to grant for this session"
            )),
            AtdError::ServerUnreachable(_) => {
                Some("is the ANOS daemon running? try `anos daemon status`".into())
            }
            AtdError::Timeout { tool_id, .. } => {
                Some(format!("increase timeout or retry; tool_id={tool_id}"))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_not_found_suggests_candidate() {
        let e = AtdError::ToolNotFound {
            tool_id: "fs.red".into(),
            suggestions: vec!["fs.read".into()],
        };
        assert_eq!(e.suggest_fix().unwrap(), "did you mean 'fs.read'?");
        assert!(!e.is_retryable());
    }

    #[test]
    fn tool_not_found_without_suggestions_hints_discovery() {
        let e = AtdError::ToolNotFound {
            tool_id: "xx".into(),
            suggestions: vec![],
        };
        assert!(e.suggest_fix().unwrap().contains("atd list"));
    }

    #[test]
    fn timeout_is_retryable() {
        let e = AtdError::Timeout {
            tool_id: "fs.read".into(),
            after_ms: 5000,
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn io_error_converts_to_server_unreachable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "no");
        let e: AtdError = io_err.into();
        assert!(matches!(e, AtdError::ServerUnreachable(_)));
        assert!(e.is_retryable());
    }

    #[test]
    fn display_includes_tool_id() {
        let e = AtdError::InvalidArguments {
            tool_id: "fs.read".into(),
            field: "path".into(),
            reason: "must be string".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("fs.read"));
        assert!(s.contains("path"));
    }
}
