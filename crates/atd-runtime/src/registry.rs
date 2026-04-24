//! `Tool` trait + `Registry` — the contract third-party implementers see.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use atd_protocol::{ToolDefinition, ToolSummary};

use crate::context::CallContext;
use crate::error::ToolCallError;

/// Boxed future returned by [`Tool::call`].
pub type CallFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolCallError>> + Send + 'a>>;

/// A tool. One `impl Tool for MyTool` per tool; registered once at startup.
/// Tools MUST NOT panic; they return `Err(ToolCallError)` instead.
///
/// `call` returns a boxed future so the trait is dyn-compatible without
/// requiring the `async_trait` macro.
pub trait Tool: Send + Sync {
    /// Stable borrow of the tool's definition. Registry calls this once at
    /// registration time (for summaries/schema lookup) — implementers
    /// typically store a single `ToolDefinition` in the struct.
    fn definition(&self) -> &ToolDefinition;

    /// Invoke the tool. Args are the deserialized JSON from the wire.
    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a>;
}

/// One registered tool plus the binding dispatch uses to execute it.
/// SP-12 Task 4: `Binding` sits between dispatch and the `Tool` impl so
/// the same tool can be served via different execution strategies
/// (in-process, CLI subprocess, future MCP / REST / AppFunction).
#[derive(Clone)]
pub struct RegisteredTool {
    pub tool: Arc<dyn Tool>,
    pub binding: Arc<dyn crate::binding::Binding>,
}

impl RegisteredTool {
    pub fn definition(&self) -> &ToolDefinition {
        self.tool.definition()
    }
}

pub struct Registry {
    tools: HashMap<String, RegisteredTool>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool with the default `NativeBinding` — dispatch will call
    /// the tool's `Tool::call` directly. Panics on duplicate tool_id:
    /// startup misconfiguration should fail loud, not at request time.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let binding: Arc<dyn crate::binding::Binding> =
            Arc::new(crate::binding::NativeBinding::new(tool.clone()));
        self.register_with_binding(tool, binding);
    }

    /// Register a tool paired with an explicit binding. Use this for tools
    /// whose execution strategy differs from "run the `Tool::call` future"
    /// (e.g. `CliBinding` for subprocess-backed tools).
    pub fn register_with_binding(
        &mut self,
        tool: Arc<dyn Tool>,
        binding: Arc<dyn crate::binding::Binding>,
    ) {
        let id = tool.definition().id.clone();
        if self.tools.contains_key(&id) {
            panic!("duplicate tool registration: {id}");
        }
        self.tools.insert(id, RegisteredTool { tool, binding });
    }

    pub fn get(&self, tool_id: &str) -> Option<&RegisteredTool> {
        self.tools.get(tool_id)
    }

    pub fn summaries(&self) -> Vec<ToolSummary> {
        self.tools
            .values()
            .map(|r| ToolSummary::from(r.tool.definition()))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_protocol::{
        BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources, ToolSafety,
        ToolTrust, ToolVisibility, TrustLevel,
    };

    struct StubTool {
        def: ToolDefinition,
    }

    impl StubTool {
        fn new(id: &str) -> Self {
            Self {
                def: ToolDefinition {
                    id: id.into(),
                    name: id.into(),
                    description: "stub".into(),
                    version: "0.0.0".into(),
                    capability: ToolCapability {
                        domain: "stub".into(),
                        actions: vec![],
                        tags: vec![],
                        intent_examples: vec![],
                    },
                    input_schema: serde_json::json!({}),
                    output_schema: serde_json::json!({}),
                    bindings: vec![ToolBinding {
                        protocol: BindingProtocol::Cli,
                        config: serde_json::json!({}),
                    }],
                    safety: ToolSafety {
                        level: SafetyLevel::Read,
                        dry_run: false,
                        side_effects: vec![],
                        data_sensitivity: None,
                    },
                    resources: ToolResources {
                        timeout_ms: 1000,
                        max_concurrent: 1,
                        rate_limit_per_min: None,
                        estimated_tokens: None,
                    },
                    trust: ToolTrust {
                        publisher: "test".into(),
                        trust_level: TrustLevel::L0Unverified,
                        signature: None,
                    },
                    visibility: ToolVisibility::Read,
                    required_capabilities: vec![],
                    tier: None,
                },
            }
        }
    }

    impl Tool for StubTool {
        fn definition(&self) -> &ToolDefinition {
            &self.def
        }
        fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
            Box::pin(async move { Ok(serde_json::json!({})) })
        }
    }

    #[test]
    fn register_and_get_returns_the_tool() {
        let mut r = Registry::new();
        r.register(Arc::new(StubTool::new("test:a")));
        assert!(r.get("test:a").is_some());
        assert!(r.get("test:missing").is_none());
    }

    #[test]
    fn summaries_projects_registered_tools() {
        let mut r = Registry::new();
        r.register(Arc::new(StubTool::new("test:a")));
        r.register(Arc::new(StubTool::new("test:b")));
        let sums = r.summaries();
        assert_eq!(sums.len(), 2);
        let ids: std::collections::HashSet<_> = sums.iter().map(|s| s.id.clone()).collect();
        assert!(ids.contains("test:a"));
        assert!(ids.contains("test:b"));
    }

    #[test]
    #[should_panic(expected = "duplicate tool registration: test:a")]
    fn duplicate_registration_panics() {
        let mut r = Registry::new();
        r.register(Arc::new(StubTool::new("test:a")));
        r.register(Arc::new(StubTool::new("test:a")));
    }

    #[test]
    fn empty_registry_reports_zero() {
        let r = Registry::new();
        assert_eq!(r.count(), 0);
        assert!(r.summaries().is_empty());
    }
}
