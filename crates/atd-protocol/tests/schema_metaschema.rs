//! Runs the same meta-schema validation that `gen-schema -- --check` does,
//! but as a `cargo test` so it reports inside the workspace test count.

#![cfg(feature = "schema")]

use schemars::r#gen::SchemaSettings;

#[test]
fn generated_schema_validates_against_draft_2020_12_metaschema() {
    let settings = SchemaSettings::draft2019_09();
    let mut generator = settings.into_generator();
    generator.subschema_for::<atd_protocol::Request>();
    generator.subschema_for::<atd_protocol::Response>();
    generator.subschema_for::<atd_protocol::ToolDefinition>();
    generator.subschema_for::<atd_protocol::ToolResult>();
    generator.subschema_for::<atd_protocol::AtdError>();
    let root = generator.into_root_schema_for::<()>();
    let value = serde_json::to_value(&root).unwrap();

    let metaschema = serde_json::json!({
        "$ref": "https://json-schema.org/draft/2020-12/schema"
    });
    let validator = jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&metaschema)
        .expect("compile draft-2020-12 metaschema");
    if let Err(errs) = validator.validate(&value) {
        let msgs: Vec<String> = errs.map(|e| e.to_string()).collect();
        panic!("schema does not validate against draft-2020-12: {msgs:?}");
    }
}
