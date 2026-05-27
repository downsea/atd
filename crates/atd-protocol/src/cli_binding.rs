//! Canonical typed config shape for `BindingProtocol::Cli` (SP-cli-binding-v2).
//!
//! `ToolBinding.config` is wire-level `serde_json::Value` — kept untyped so
//! third-party binding implementations can carry their own config without
//! every change requiring a protocol bump. The CLI binding has a recurring
//! shape that's worth codifying as a typed sibling, so that:
//!
//! 1. ATD servers wrapping arbitrary CLI tools (kubectl / gh / mycli /
//!    healthkit_cli) can describe their binding declaratively rather than
//!    each writing custom dispatch logic.
//! 2. Adopter CLI authors get a stable target schema to populate manifests
//!    against.
//! 3. A future `SP-cli-dispatcher-v1` can read this typed shape from
//!    `ToolBinding.config` and spawn subprocesses without each server
//!    re-deriving the field meanings.
//!
//! ## Wire compatibility
//!
//! The v1-era minimal config `{"cmd": "cat"}` parses unchanged through
//! [`CliBindingConfig::from_value`]; every new field uses
//! `skip_serializing_if` so default values stay off the wire. Pre-v2
//! parsers (`serde_json::Value`-typed consumers) see unchanged bytes for
//! configs that haven't started using the v2 fields.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Canonical CLI binding config (SP-cli-binding-v2).
///
/// All fields except `cmd` are optional and use `skip_serializing_if` so
/// existing `{"cmd": "..."}` configs from before SP-cli-binding-v2 still
/// round-trip byte-identically.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CliBindingConfig {
    /// Executable name or absolute path. Required.
    pub cmd: String,

    /// Fixed prefix arguments prepended to `args_template` expansion.
    /// Useful for subcommand routing (e.g. `["api", "v1"]` so a tool
    /// always invokes `cmd api v1 <rendered template>`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Templated argv tail. Placeholders are replaced before spawn:
    ///
    /// | Placeholder | Substitution |
    /// |---|---|
    /// | `{tool_id}` | ATD tool id, e.g. `mycli:gmail.users.messages.list` |
    /// | `{params_json}` | `RunTool.args` serialized as one JSON argv slot (quoting handled by the dispatcher) |
    /// | `{dry_run}` | Empty string when `dry_run=false`; the value of [`Self::dry_run_flag`] when `dry_run=true` |
    /// | `{page_all}` | Empty string normally; [`Self::page_all_flag`] when the dispatcher is fanning out for `call_all` |
    ///
    /// The split between `cmd` + `args` (fixed) and `args_template`
    /// (per-call) lets servers reuse one binding for many tool ids by
    /// changing only the template. Servers are free to extend the
    /// placeholder vocabulary; consumers that don't recognize a
    /// placeholder should pass it through literally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_template: Option<String>,

    /// Environment variables injected into the spawned process.
    ///
    /// The literal value `"$ATD_BEARER"` opts into bearer-token
    /// passthrough — the dispatcher replaces it with the connection's
    /// bearer (from the SP-12 `hello` handshake) at spawn time. Other
    /// `$NAME` syntax is not interpolated; values are passed verbatim.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,

    /// How to parse the subprocess stdout into the `ToolResult.data`
    /// field. Defaults to [`CliOutputFormat::Json`] when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<CliOutputFormat>,

    /// CLI-side flag the dispatcher appends when fanning out for
    /// `call_all` pagination. Absent = tool doesn't support pagination;
    /// the dispatcher MUST surface `call_page` / `call_all` requests as
    /// unsupported instead of silently calling the tool without paging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_all_flag: Option<String>,

    /// CLI-side flag passed when the ATD client sets `dry_run: true` on
    /// `RunTool`. Absent = tool doesn't expose dry-run; the dispatcher
    /// MUST reject the call rather than silently execute the side
    /// effect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run_flag: Option<String>,

    /// Process exit-code → ATD `ToolResult.code` mapping for the error
    /// envelope. Codes not present in the map default to
    /// `"TOOL_FAILED"`; exit code 0 always succeeds and never consults
    /// the map. String values are domain-specific (e.g.
    /// `"auth_required"` for an OAuth refresh, `"invalid_args"` for
    /// CLI-side validation errors).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub exit_code_map: BTreeMap<i32, String>,
}

/// How to parse subprocess stdout into the ATD `ToolResult.data` payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CliOutputFormat {
    /// stdout is a single JSON document. Default.
    #[default]
    Json,
    /// stdout is newline-delimited JSON. Each line becomes one page on
    /// the wire (when paginated) or is concatenated into a JSON array
    /// (when called as a single-shot tool).
    Ndjson,
    /// stdout is plain text lines. The dispatcher wraps the output as
    /// `{"lines": [...], "stderr": "..."}` for the `ToolResult.data`
    /// payload.
    Lines,
}

/// Errors from [`CliBindingConfig::from_value`].
#[derive(Debug, thiserror::Error)]
pub enum CliBindingConfigError {
    #[error("CLI binding config is not a JSON object: {0}")]
    NotAnObject(String),
    #[error("CLI binding config missing required field `cmd`")]
    MissingCmd,
    #[error("CLI binding config invalid: {0}")]
    Invalid(#[from] serde_json::Error),
}

impl CliBindingConfig {
    /// Parse from a [`crate::tool::ToolBinding::config`] value. Returns
    /// `Err` when `cmd` is missing or other fields fail to deserialize.
    /// Unknown fields are tolerated (serde's default) so future
    /// `SP-cli-binding-v3` additions don't break v2 parsers.
    pub fn from_value(v: &serde_json::Value) -> Result<Self, CliBindingConfigError> {
        if !v.is_object() {
            return Err(CliBindingConfigError::NotAnObject(v.to_string()));
        }
        if v.get("cmd").and_then(|c| c.as_str()).is_none() {
            return Err(CliBindingConfigError::MissingCmd);
        }
        let parsed: Self = serde_json::from_value(v.clone())?;
        Ok(parsed)
    }

    /// Serialize to a `serde_json::Value` suitable for
    /// [`crate::tool::ToolBinding::config`].
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("CliBindingConfig is always serializable")
    }
}

impl crate::tool::ToolBinding {
    /// Convenience: if this binding is `Cli`, parse its config as the
    /// canonical [`CliBindingConfig`] shape. Returns `Ok(None)` for
    /// non-CLI bindings; returns `Err` if the protocol is `Cli` but the
    /// config doesn't match the canonical shape.
    pub fn cli_config(&self) -> Result<Option<CliBindingConfig>, CliBindingConfigError> {
        if !matches!(self.protocol, crate::enums::BindingProtocol::Cli) {
            return Ok(None);
        }
        CliBindingConfig::from_value(&self.config).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::BindingProtocol;
    use crate::tool::ToolBinding;
    use serde_json::json;

    fn minimal_v1() -> serde_json::Value {
        // The pre-SP-cli-binding-v2 wire shape — `{"cmd": "cat"}` alone.
        json!({"cmd": "cat"})
    }

    fn full_v2() -> CliBindingConfig {
        CliBindingConfig {
            cmd: "mycli".into(),
            args: vec!["api".into(), "v1".into()],
            args_template: Some("{tool_id} --params {params_json} {dry_run}".into()),
            env: BTreeMap::from([("MYCLI_TOKEN".into(), "$ATD_BEARER".into())]),
            output_format: Some(CliOutputFormat::Json),
            page_all_flag: Some("--page-all".into()),
            dry_run_flag: Some("--dry-run".into()),
            exit_code_map: BTreeMap::from([
                (2, "auth_required".into()),
                (3, "invalid_args".into()),
            ]),
        }
    }

    #[test]
    fn pre_v2_minimal_config_round_trips_byte_identical() {
        let parsed = CliBindingConfig::from_value(&minimal_v1()).unwrap();
        assert_eq!(parsed.cmd, "cat");
        assert!(parsed.args.is_empty());
        assert!(parsed.args_template.is_none());
        // Round-trip back to JSON must produce the same minimal shape —
        // this is the load-bearing back-compat guarantee.
        let back = parsed.to_value();
        assert_eq!(back, minimal_v1());
    }

    #[test]
    fn v2_full_config_round_trips() {
        let cfg = full_v2();
        let v = cfg.to_value();
        let back = CliBindingConfig::from_value(&v).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn parse_rejects_non_object() {
        let err = CliBindingConfig::from_value(&json!(42)).unwrap_err();
        assert!(matches!(err, CliBindingConfigError::NotAnObject(_)));
    }

    #[test]
    fn parse_rejects_missing_cmd() {
        let err = CliBindingConfig::from_value(&json!({"args": ["x"]})).unwrap_err();
        assert!(matches!(err, CliBindingConfigError::MissingCmd));
    }

    #[test]
    fn output_format_snake_case() {
        assert_eq!(
            serde_json::to_string(&CliOutputFormat::Ndjson).unwrap(),
            "\"ndjson\""
        );
        assert_eq!(
            serde_json::to_string(&CliOutputFormat::Lines).unwrap(),
            "\"lines\""
        );
        assert_eq!(
            serde_json::to_string(&CliOutputFormat::Json).unwrap(),
            "\"json\""
        );
    }

    #[test]
    fn output_format_default_is_json() {
        assert_eq!(CliOutputFormat::default(), CliOutputFormat::Json);
    }

    #[test]
    fn empty_collections_skip_serialization() {
        let cfg = CliBindingConfig {
            cmd: "x".into(),
            ..Default::default()
        };
        let s = serde_json::to_string(&cfg).unwrap();
        assert_eq!(s, "{\"cmd\":\"x\"}");
    }

    #[test]
    fn parse_tolerates_unknown_fields_for_forward_compat() {
        // Future fields added by SP-cli-binding-v3 should not break v2
        // parsers. (serde ignores unknown fields by default — verify it.)
        let v = json!({
            "cmd": "x",
            "future_field": {"foo": "bar"},
        });
        let parsed = CliBindingConfig::from_value(&v).unwrap();
        assert_eq!(parsed.cmd, "x");
    }

    #[test]
    fn tool_binding_helper_cli_returns_some() {
        let b = ToolBinding {
            protocol: BindingProtocol::Cli,
            config: full_v2().to_value(),
        };
        let parsed = b.cli_config().unwrap().expect("expected Some");
        assert_eq!(parsed, full_v2());
    }

    #[test]
    fn tool_binding_helper_non_cli_returns_none() {
        let b = ToolBinding {
            protocol: BindingProtocol::Mcp,
            config: json!({"endpoint": "..."}),
        };
        assert!(b.cli_config().unwrap().is_none());
    }

    #[test]
    fn tool_binding_helper_cli_with_bad_config_errors() {
        let b = ToolBinding {
            protocol: BindingProtocol::Cli,
            config: json!({"no_cmd": "oops"}),
        };
        let err = b.cli_config().unwrap_err();
        assert!(matches!(err, CliBindingConfigError::MissingCmd));
    }

    #[test]
    fn exit_code_map_serialization_key_is_string() {
        // BTreeMap<i32, String> serializes to an object with string
        // keys per JSON convention. Verify the wire shape is what the
        // dispatcher will see.
        let cfg = CliBindingConfig {
            cmd: "x".into(),
            exit_code_map: BTreeMap::from([(2, "auth".into()), (3, "args".into())]),
            ..Default::default()
        };
        let v = cfg.to_value();
        let map = v["exit_code_map"].as_object().unwrap();
        assert_eq!(map.get("2").unwrap(), "auth");
        assert_eq!(map.get("3").unwrap(), "args");
    }
}
