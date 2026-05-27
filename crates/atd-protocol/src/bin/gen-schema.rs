//! Generates `atd-protocol-schema.json` at the repo root.
//!
//! Two modes:
//!   bare         → write the file (developer flow after type changes)
//!   --check      → re-generate in memory; byte-diff against on-disk;
//!                  validate result against JSON Schema Draft 2020-12
//!                  meta-schema; exit non-zero on any failure.
//!
//! Note: schemars 0.8 does not expose a `draft2020_12()` preset, so the
//! generator runs in draft 2019-09 mode (its closest available preset).
//! The emitted wrapper still advertises `$schema: draft/2020-12/schema`
//! because draft 2020-12 accepts unknown keywords like `definitions` as
//! annotations, and the root wrapper itself stays within vocabulary.

use schemars::r#gen::SchemaSettings;
use std::path::PathBuf;
use std::process::ExitCode;

fn build_schema_text() -> String {
    let settings = SchemaSettings::draft2019_09().with(|s| {
        s.inline_subschemas = false;
    });
    let mut generator = settings.into_generator();

    // Walk every public root type so transitive references land in `definitions`.
    generator.subschema_for::<atd_protocol::Request>();
    generator.subschema_for::<atd_protocol::Response>();
    generator.subschema_for::<atd_protocol::ToolSummary>();
    generator.subschema_for::<atd_protocol::ToolDefinition>();
    generator.subschema_for::<atd_protocol::ToolResult>();
    generator.subschema_for::<atd_protocol::ToolResultMetadata>();
    generator.subschema_for::<atd_protocol::AtdError>();
    // SP-cli-binding-v2: typed canonical shape for `ToolBinding.config`
    // when `protocol = "Cli"`. Walked explicitly because `ToolBinding.config`
    // is still untyped `serde_json::Value` on the wire (back-compat).
    generator.subschema_for::<atd_protocol::CliBindingConfig>();

    let root_schema = generator.into_root_schema_for::<()>();
    let definitions = serde_json::to_value(&root_schema.definitions).unwrap();

    let root = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://atd.dev/schema/v1.0.0/atd-protocol-schema.json",
        "title": "ATD Protocol Schema",
        "description": "Wire types for the ATD reference implementation. Generated from atd-protocol Rust types via schemars; do not hand-edit.",
        "definitions": definitions,
    });
    let mut text = serde_json::to_string_pretty(&root).unwrap();
    text.push('\n');
    text
}

fn out_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../atd-protocol-schema.json")
}

fn run_check(text: &str) -> bool {
    let mut ok = true;
    let on_disk = match std::fs::read_to_string(out_path()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read atd-protocol-schema.json: {e}");
            return false;
        }
    };
    if on_disk != text {
        eprintln!(
            "error: atd-protocol-schema.json is stale.\n\
             run: cargo run -p atd-protocol --features schema --bin gen-schema"
        );
        ok = false;
    }

    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    // jsonschema-0.18 with the `draft202012` feature bundles the
    // draft-2020-12 metaschema locally (no network fetch).
    let metaschema = serde_json::json!({
        "$ref": "https://json-schema.org/draft/2020-12/schema"
    });
    match jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&metaschema)
    {
        Ok(validator) => {
            if let Err(errors) = validator.validate(&value) {
                for err in errors {
                    eprintln!("metaschema: {err}");
                }
                ok = false;
            }
        }
        Err(e) => {
            eprintln!("error: cannot compile draft-2020-12 metaschema: {e}");
            ok = false;
        }
    }
    ok
}

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let text = build_schema_text();
    if check {
        if run_check(&text) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    } else {
        if let Err(e) = std::fs::write(out_path(), &text) {
            eprintln!("error: write failed: {e}");
            return ExitCode::FAILURE;
        }
        eprintln!("wrote {}", out_path().display());
        ExitCode::SUCCESS
    }
}
