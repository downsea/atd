//! Conformance-suite-specific tools — registered ONLY when
//! `--enable-conformance-tool` is passed. These tools exist solely so the
//! `atd-conformance` suite can validate protocol paths (like
//! `ERR_CAPABILITY_DENIED` / code 1001) that no production built-in tool
//! naturally exercises.

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTier, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Tool};

/// A trivial tool that requires the `conformance.denied` capability.
/// The test harness never grants this capability, so every call is
/// short-circuited by the dispatch layer with `ERR_CAPABILITY_DENIED`
/// before `call` ever runs. If `call` *does* run, that's a bug in the
/// capability gate — we return an `InternalError` sentinel so the
/// violation is loud.
pub struct ConformanceDeniedTool {
    def: ToolDefinition,
}

impl ConformanceDeniedTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "ref:conformance.denied_op".into(),
                name: "conformance denied op".into(),
                description: "Test tool: requires 'conformance.denied' capability. \
                     Exists only to let atd-conformance validate the \
                     ERR_CAPABILITY_DENIED (code 1001) wire path. Enabled \
                     via --enable-conformance-tool."
                    .into(),
                version: "0.0.0".into(),
                capability: ToolCapability {
                    domain: "conformance".into(),
                    actions: vec!["denied_op".into()],
                    tags: vec!["test".into()],
                    intent_examples: vec![],
                },
                input_schema: serde_json::json!({ "type": "object" }),
                output_schema: serde_json::json!({ "type": "object" }),
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
                    publisher: "atd-ref-server".into(),
                    trust_level: TrustLevel::L0Unverified,
                    signature: None,
                },
                visibility: ToolVisibility::Read,
                required_capabilities: vec!["conformance.denied".into()],
                tier: None,
            },
        }
    }
}

impl Default for ConformanceDeniedTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ConformanceDeniedTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async {
            Err(ToolCallError::InternalError(
                "ref:conformance.denied_op was reached by dispatch — \
                 capability gate should have rejected with code 1001"
                    .into(),
            ))
        })
    }
}

/// A trivial tool whose semaphore is permanently saturated by the
/// startup-time `Box::leak` in `main.rs`. Every call returns
/// `ERR_RATE_LIMITED` (code 1002) at the dispatch layer's
/// `try_acquire_owned` check, before `call` ever runs.
///
/// If `call` *does* run, that's a bug in the rate-limit gate or in the
/// startup leak — the `InternalError` sentinel makes the violation
/// loud rather than silently succeeding.
pub struct ConformanceSaturatedTool {
    def: ToolDefinition,
}

impl ConformanceSaturatedTool {
    pub fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "ref:conformance.saturate_op".into(),
                name: "conformance saturate op".into(),
                description: "Test tool: registered with max_concurrent=1, then has \
                     its sole permit leaked at server startup so the \
                     semaphore is permanently empty. Exists only to let \
                     atd-conformance validate the ERR_RATE_LIMITED \
                     (code 1002) wire path. Enabled via \
                     --enable-conformance-tool."
                    .into(),
                version: "0.0.0".into(),
                capability: ToolCapability {
                    domain: "conformance".into(),
                    actions: vec!["saturate_op".into()],
                    tags: vec!["test".into()],
                    intent_examples: vec![],
                },
                input_schema: serde_json::json!({ "type": "object" }),
                output_schema: serde_json::json!({ "type": "object" }),
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
                    publisher: "atd-ref-server".into(),
                    trust_level: TrustLevel::L0Unverified,
                    signature: None,
                },
                visibility: ToolVisibility::Read,
                required_capabilities: vec![],
                tier: Some(ToolTier::Hot),
            },
        }
    }
}

impl Default for ConformanceSaturatedTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for ConformanceSaturatedTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async {
            Err(ToolCallError::InternalError(
                "ref:conformance.saturate_op was reached by dispatch — \
                 startup permit leak should keep the semaphore empty so \
                 try_acquire_owned() always fails with code 1002"
                    .into(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_declares_conformance_denied_cap() {
        let t = ConformanceDeniedTool::new();
        let d = t.definition();
        assert_eq!(d.id, "ref:conformance.denied_op");
        assert_eq!(
            d.required_capabilities,
            vec!["conformance.denied".to_string()]
        );
        assert_eq!(d.trust.publisher, "atd-ref-server");
        assert_eq!(d.trust.trust_level, TrustLevel::L0Unverified);
    }

    #[test]
    fn saturated_tool_definition_declares_max_concurrent_one() {
        let t = ConformanceSaturatedTool::new();
        let d = t.definition();
        assert_eq!(d.id, "ref:conformance.saturate_op");
        assert_eq!(d.resources.max_concurrent, 1);
        assert!(d.required_capabilities.is_empty());
        assert_eq!(d.trust.publisher, "atd-ref-server");
        assert_eq!(d.trust.trust_level, TrustLevel::L0Unverified);
        assert_eq!(d.tier, Some(ToolTier::Hot));
    }
}
