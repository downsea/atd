use atd_protocol::{ToolDefinition, ToolErrorDef};

#[test]
fn tool_error_def_roundtrips() {
    let e = ToolErrorDef {
        code: "FILE_NOT_FOUND".into(),
        description: "the path does not exist".into(),
        retryable: false,
    };
    let j = serde_json::to_string(&e).unwrap();
    let back: ToolErrorDef = serde_json::from_str(&j).unwrap();
    assert_eq!(back.code, "FILE_NOT_FOUND");
    assert_eq!(back.description, "the path does not exist");
    assert!(!back.retryable);
}

#[test]
fn tool_definition_without_errors_key_defaults_to_empty() {
    let j = r#"{
        "id": "ref:fs.read",
        "name": "Read",
        "description": "d",
        "version": "0.1.0",
        "capability": {"domain":"fs","actions":[],"tags":[],"intent_examples":[]},
        "input_schema": {},
        "output_schema": {},
        "bindings": [],
        "safety": {"level":"Read","dry_run":false,"side_effects":[],"data_sensitivity":null},
        "resources": {"timeout_ms":1000,"max_concurrent":1,"rate_limit_per_min":null,"estimated_tokens":null},
        "trust": {"publisher":"x","trust_level":"L2Tested","signature":null}
    }"#;
    let def: ToolDefinition = serde_json::from_str(j).unwrap();
    assert!(def.errors.is_empty());
}

#[test]
fn tool_definition_errors_roundtrip() {
    let j = r#"{
        "id": "ref:fs.read",
        "name": "Read",
        "description": "d",
        "version": "0.1.0",
        "capability": {"domain":"fs","actions":[],"tags":[],"intent_examples":[]},
        "input_schema": {},
        "output_schema": {},
        "bindings": [],
        "safety": {"level":"Read","dry_run":false,"side_effects":[],"data_sensitivity":null},
        "resources": {"timeout_ms":1000,"max_concurrent":1,"rate_limit_per_min":null,"estimated_tokens":null},
        "trust": {"publisher":"x","trust_level":"L2Tested","signature":null},
        "errors": [{"code":"E1","description":"x","retryable":true}]
    }"#;
    let def: ToolDefinition = serde_json::from_str(j).unwrap();
    assert_eq!(def.errors.len(), 1);
    assert_eq!(def.errors[0].code, "E1");
    assert!(def.errors[0].retryable);
}
