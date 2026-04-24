//! Errors a tool may return.
//!
//! Axes chosen to map cleanly to the wire protocol:
//! - InvalidArgs / InternalError → wire `error` response
//! - ExecutionFailed → wire `tool_result { success: false }` response
//!
//! Named `ToolCallError` (not reusing `atd-types::AtdError`) because
//! client-side and server-side errors classify different concerns.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ToolCallError {
    /// Schema validation failed or args couldn't be coerced to the expected
    /// shape. The tool's own logic did not execute.
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    /// Tool ran to completion but reports a failure outcome. This is the
    /// domain-level "the operation didn't succeed" case, not a server error.
    #[error("execution failed ({code}): {message}")]
    ExecutionFailed {
        code: String,
        message: String,
        retryable: bool,
    },

    /// Server-side bug or unexpected condition during tool invocation.
    #[error("internal error: {0}")]
    InternalError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_args_display_format() {
        let e = ToolCallError::InvalidArgs("missing field `path`".into());
        assert_eq!(format!("{e}"), "invalid arguments: missing field `path`");
    }

    #[test]
    fn execution_failed_display_includes_code_and_message() {
        let e = ToolCallError::ExecutionFailed {
            code: "EPERM".into(),
            message: "denied".into(),
            retryable: false,
        };
        let s = format!("{e}");
        assert!(s.contains("EPERM"));
        assert!(s.contains("denied"));
    }

    #[test]
    fn internal_error_display_format() {
        let e = ToolCallError::InternalError("logic bug".into());
        assert_eq!(format!("{e}"), "internal error: logic bug");
    }

    #[test]
    fn enum_is_non_exhaustive_at_api_boundary() {
        let e = ToolCallError::InvalidArgs("x".into());
        match e {
            ToolCallError::InvalidArgs(_) => {}
            ToolCallError::ExecutionFailed { .. } => {}
            ToolCallError::InternalError(_) => {}
        }
    }
}
