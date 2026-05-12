//! [`PiiRedactMiddleware`] — thin `Middleware` wrapper around
//! [`crate::redact::redact_value`].
//!
//! Spec: SP-medical-middleware §4.5 + §4.6 + §5.2.

use atd_protocol::ToolDefinition;
use atd_runtime::Middleware;
use serde_json::Value;

use crate::config::PiiRedactConfig;

#[derive(Debug, Clone)]
pub struct PiiRedactMiddleware {
    config: PiiRedactConfig,
}

impl PiiRedactMiddleware {
    pub fn new(config: PiiRedactConfig) -> Self {
        Self { config }
    }

    /// Convenience: spec §7 migration step 2 — annotate `_phi_findings`
    /// but don't mutate values. Adopters observe coverage before
    /// flipping to real redaction.
    pub fn log_only() -> Self {
        Self::new(PiiRedactConfig::log_only())
    }
}

impl Default for PiiRedactMiddleware {
    fn default() -> Self {
        Self::new(PiiRedactConfig::default())
    }
}

impl Middleware for PiiRedactMiddleware {
    fn name(&self) -> &'static str {
        "pii_redact_medical"
    }

    fn on_result(&self, _tool_id: &str, _tool_def: &ToolDefinition, result: &mut Value) {
        let _findings = crate::redact::redact_value(result, &self.config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_protocol::{
        BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
        ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
    };
    use serde_json::json;

    fn stub_def() -> ToolDefinition {
        ToolDefinition {
            id: "test:tool".into(),
            name: "t".into(),
            description: "stub".into(),
            version: "0.0.0".into(),
            capability: ToolCapability {
                domain: "test".into(),
                actions: vec![],
                tags: vec![],
                intent_examples: vec![],
            },
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            bindings: vec![ToolBinding {
                protocol: BindingProtocol::Cli,
                config: json!({}),
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
            errors: vec![],
        }
    }

    #[test]
    fn middleware_name_is_stable() {
        assert_eq!(PiiRedactMiddleware::default().name(), "pii_redact_medical");
    }

    #[test]
    fn middleware_default_redacts_patient_payload() {
        let mw = PiiRedactMiddleware::default();
        let mut v = json!({
            "resourceType": "Patient",
            "name": [{"family": "Smith"}],
            "birthDate": "1955-03-15"
        });
        mw.on_result("hms:patient.get", &stub_def(), &mut v);
        assert_eq!(v["name"], "[REDACTED:NAME]");
        assert_eq!(v["birthDate"], "1955");
    }

    #[test]
    fn middleware_log_only_does_not_mutate() {
        let mw = PiiRedactMiddleware::log_only();
        let mut v = json!({
            "resourceType": "Patient",
            "name": [{"family": "Smith"}]
        });
        mw.on_result("t", &stub_def(), &mut v);
        assert_eq!(v["name"], json!([{"family": "Smith"}]));
        assert!(v["_phi_findings"].is_array());
    }
}
