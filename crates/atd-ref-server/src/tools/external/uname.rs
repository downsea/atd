//! `ref:external.uname` — dispatch demo for `CliBinding`.
//!
//! Deliberately trivial: the tool logic is "run `/usr/bin/uname <flag>` and
//! return its stdout". What's being demonstrated is the **binding** layer —
//! dispatch chooses `CliBinding` over `NativeBinding`, argv is derived from
//! the request's JSON args, deadlines and exit-code handling all flow
//! through the binding. This isn't meant to be production uname; it's the
//! smallest tool that exercises a non-native execution path.
//!
//! Linux CI runners (`ubuntu-latest`) ship `/usr/bin/uname`; macOS dev
//! boxes have it at the same path. Windows is excluded at the `tools/mod.rs`
//! `#[cfg(unix)]` gate.

use std::path::PathBuf;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTier, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::binding::CliBinding;
use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};

/// The ToolDefinition served over the wire. Notably:
/// - `tier: Some(Hot)` so the tool runs under the hot-tier budget (500 ms
///   default); uname is cheap.
/// - `required_capabilities: []` so no Hello-scoped capability is needed;
///   any client can call it.
pub fn definition() -> ToolDefinition {
    ToolDefinition {
        id: "ref:external.uname".into(),
        name: "uname".into(),
        description: "Operating system identifier (via /usr/bin/uname).".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "external".into(),
            actions: vec!["uname".into()],
            tags: vec!["host".into(), "sys".into()],
            intent_examples: vec!["what kernel is this".into()],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "flag": {
                    "type": "string",
                    "enum": ["-s", "-r", "-m", "-a"],
                    "default": "-s",
                    "description": "uname flag: -s kernel name (default), -r release, -m machine, -a all."
                }
            }
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "stdout": { "type": "string" },
                "exit_code": { "type": "integer" }
            }
        }),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({"program": "/usr/bin/uname"}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Read,
            dry_run: false,
            side_effects: vec![],
            data_sensitivity: None,
        },
        resources: ToolResources {
            timeout_ms: 1_000,
            max_concurrent: 8,
            rate_limit_per_min: None,
            estimated_tokens: Some(20),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Read,
        required_capabilities: vec![],
        tier: Some(ToolTier::Hot),
    }
}

/// Map request args → argv. Only `flag` is read; anything else is ignored.
/// Defaults to `-s` (kernel name) to match POSIX uname's default.
pub fn args_mapper(args: &serde_json::Value) -> Vec<String> {
    let flag = args
        .get("flag")
        .and_then(|v| v.as_str())
        .unwrap_or("-s")
        .to_string();
    vec![flag]
}

/// Build the `CliBinding` for uname.
pub fn cli_binding() -> CliBinding {
    CliBinding {
        program: PathBuf::from("/usr/bin/uname"),
        base_args: vec![],
        args_mapper,
    }
}

/// A minimal `Tool` carrying only the definition. `Registry` pairs every
/// entry with a binding, and `RegisteredTool` also needs a `Tool` slot. For
/// pure-CLI tools, dispatch goes through `CliBinding::call`, so `call` here
/// is never invoked in normal operation — we leave it as an internal error
/// so a future wiring mistake (accidentally swapping the binding for
/// `NativeBinding`) fails loud rather than silently doing nothing.
pub struct UnameStub {
    def: ToolDefinition,
}

impl UnameStub {
    pub fn new() -> Self {
        Self {
            def: definition(),
        }
    }
}

impl Default for UnameStub {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for UnameStub {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(
        &'a self,
        _args: serde_json::Value,
        _ctx: &'a CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async {
            Err(ToolCallError::InternalError(
                "ref:external.uname must be dispatched through CliBinding, not NativeBinding"
                    .into(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_mapper_defaults_to_minus_s() {
        assert_eq!(args_mapper(&serde_json::json!({})), vec!["-s"]);
    }

    #[test]
    fn args_mapper_passes_flag_through() {
        assert_eq!(
            args_mapper(&serde_json::json!({"flag": "-m"})),
            vec!["-m"]
        );
    }

    #[test]
    fn args_mapper_ignores_extra_fields() {
        assert_eq!(
            args_mapper(&serde_json::json!({"flag": "-r", "other": 42})),
            vec!["-r"]
        );
    }

    #[test]
    fn definition_declares_hot_tier_and_no_required_caps() {
        let d = definition();
        assert_eq!(d.id, "ref:external.uname");
        assert_eq!(d.tier, Some(ToolTier::Hot));
        assert!(d.required_capabilities.is_empty());
    }
}
