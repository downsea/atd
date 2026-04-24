//! Binding abstraction.
//!
//! A `Binding` is *how* a tool's semantics are realized — native in-process
//! (wrapping a `Tool` impl), CLI subprocess, and later MCP/REST/AppFunction.
//! In SP-12 the dispatch path resolves `tool_id` to a `(Tool, Binding)`
//! pair and invokes `Binding::call`; `NativeBinding` simply delegates back
//! to the `Tool`, so all 9 existing tools keep working with zero behavior
//! change.
//!
//! Concrete implementations (`NativeBinding`, `CliBinding`) land in Task 4.
//! This file ships only the trait + future type so Task 1 compiles.

use std::future::Future;
use std::pin::Pin;

use atd_types::ToolDefinition;

use crate::context::CallContext;
use crate::error::ToolCallError;

/// Boxed future returned by `Binding::call`. Shape mirrors `registry::CallFuture`
/// so the two can be freely composed.
pub type BindingFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolCallError>> + Send + 'a>>;

/// A tool's execution binding. `name()` returns a short discriminator
/// (`"native"`, `"cli"`, `"mcp"`, ...) used by observability hooks and tests.
pub trait Binding: Send + Sync {
    fn name(&self) -> &'static str;

    fn call<'a>(
        &'a self,
        tool_def: &'a ToolDefinition,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> BindingFuture<'a>;
}
