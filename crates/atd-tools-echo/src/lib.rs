//! Echo tool — test-anchor reference tool.
//!
//! Ships with atd-ref-server; the smallest real `Tool` implementation,
//! useful for wire round-trip tests and documentation examples.

use std::sync::OnceLock;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Tool};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:echo.say".into(),
        name: "Echo".into(),
        description: "Echoes input args back verbatim. Framework test anchor.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "echo".into(),
            actions: vec!["say".into()],
            tags: vec!["test".into(), "framework".into()],
            intent_examples: vec!["echo this".into()],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": true,
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "echoed": {},
                "truncated": { "type": "boolean" },
                "original_bytes": { "type": "integer" }
            }
        }),
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
            timeout_ms: 5_000,
            max_concurrent: 100,
            rate_limit_per_min: None,
            estimated_tokens: Some(10),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Read,
        required_capabilities: vec![],
        tier: None,
    })
}

pub struct EchoTool;

impl EchoTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EchoTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for EchoTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            // Estimate output size: serialized length of `{"echoed": <args>}`.
            let serialized = serde_json::to_vec(&args)
                .map_err(|e| ToolCallError::InternalError(format!("serialize args: {e}")))?;
            let estimated = serialized.len() + 16; // envelope overhead
            if estimated > ctx.max_output_bytes {
                // Return a truncation marker instead of the full echo.
                return Ok(serde_json::json!({
                    "truncated": true,
                    "original_bytes": serialized.len(),
                    "max_output_bytes": ctx.max_output_bytes,
                }));
            }
            Ok(serde_json::json!({ "echoed": args }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn happy_path_echoes_args_verbatim() {
        let t = EchoTool::new();
        let ctx = CallContext::for_test();
        let args = serde_json::json!({"hello": "world", "n": 42});
        let r = t.call(args.clone(), &ctx).await.unwrap();
        assert_eq!(r, serde_json::json!({"echoed": args}));
    }

    #[tokio::test]
    async fn empty_args_echoed_as_empty_object() {
        let t = EchoTool::new();
        let ctx = CallContext::for_test();
        let r = t.call(serde_json::json!({}), &ctx).await.unwrap();
        assert_eq!(r, serde_json::json!({"echoed": {}}));
    }

    #[tokio::test]
    async fn oversized_args_return_truncation_marker() {
        let t = EchoTool::new();
        // Tiny budget so even a small payload overflows.
        let mut ctx = CallContext::for_test();
        ctx.max_output_bytes = 32;
        let big = "x".repeat(1_000);
        let args = serde_json::json!({"big": big});
        let r = t.call(args, &ctx).await.unwrap();
        assert_eq!(r["truncated"], serde_json::json!(true));
        assert!(r["original_bytes"].as_u64().unwrap() > 32);
        assert!(r.get("echoed").is_none());
    }

    #[test]
    fn definition_has_expected_id_and_domain() {
        let t = EchoTool::new();
        let d = t.definition();
        assert_eq!(d.id, "ref:echo.say");
        assert_eq!(d.capability.domain, "echo");
    }
}
