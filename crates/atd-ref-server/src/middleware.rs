//! Result-middleware pipeline.
//!
//! A `Middleware` is invoked **on success** after a tool returns, with a
//! mutable reference to the result value. SP-12 ships one built-in
//! (`RedactPathsMiddleware`, Task 5) to demonstrate the shape; the v3
//! brief's full suite (pii_redact, source_device_tag, compress, audit_log,
//! rate_shape) is deferred.
//!
//! Error paths bypass middleware in SP-12 — spec §8 Q4. A future SP can add
//! an `on_error` hook once a real consumer exists.

use atd_types::ToolDefinition;

/// A result-rewriting hook. Must be deterministic and side-effect-free
/// beyond the `result` mutation + any internal audit sinks the impl owns.
pub trait Middleware: Send + Sync {
    fn name(&self) -> &'static str;

    fn on_result(
        &self,
        tool_id: &str,
        tool_def: &ToolDefinition,
        result: &mut serde_json::Value,
    );
}
