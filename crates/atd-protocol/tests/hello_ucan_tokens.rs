//! SP-capability-v2 Phase A — Hello.ucan_tokens additive field roundtrip
//! and back-compat tests.
//!
//! Spec: docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md §4.2 + §5.2
//! Plan: docs/superpowers/plans/2026-05-11-sp-capability-v2.md Task 1 Step 2

use atd_protocol::Request;

#[test]
fn hello_with_ucan_tokens_roundtrips() {
    let h = Request::Hello {
        client_id: Some("agent-B".into()),
        requested_capabilities: vec!["records:read".into()],
        ucan_tokens: vec!["dummy.jwt.compact".into()],
    };
    let j = serde_json::to_string(&h).unwrap();
    assert!(
        j.contains("ucan_tokens"),
        "serialized form must include ucan_tokens key: {j}"
    );
    let back: Request = serde_json::from_str(&j).unwrap();
    match back {
        Request::Hello { ucan_tokens, .. } => {
            assert_eq!(ucan_tokens, vec!["dummy.jwt.compact".to_string()]);
        }
        _ => panic!("expected Hello variant after roundtrip"),
    }
}

#[test]
fn hello_without_ucan_tokens_back_compat_parses() {
    // Pre-SP-capability-v2 wire payload — no ucan_tokens field at all.
    let json = r#"{"type":"hello","client_id":"X","requested_capabilities":[]}"#;
    let back: Request = serde_json::from_str(json).unwrap();
    match back {
        Request::Hello { ucan_tokens, .. } => {
            assert!(
                ucan_tokens.is_empty(),
                "absent ucan_tokens must default to empty Vec, got {ucan_tokens:?}"
            );
        }
        _ => panic!("expected Hello variant"),
    }
}

#[test]
fn hello_with_empty_ucan_tokens_serializes_without_field() {
    // skip_serializing_if = "Vec::is_empty" — empty vec must not appear on the wire,
    // preserving byte-identical pre-SP shape.
    let h = Request::Hello {
        client_id: Some("X".into()),
        requested_capabilities: vec!["records:read".into()],
        ucan_tokens: vec![],
    };
    let j = serde_json::to_string(&h).unwrap();
    assert!(
        !j.contains("ucan_tokens"),
        "empty ucan_tokens must be omitted from wire form: {j}"
    );
}

#[test]
fn err_ucan_invalid_code_is_1010() {
    assert_eq!(atd_protocol::ERR_UCAN_INVALID, 1010);
}

#[test]
fn err_ucan_expired_code_is_1011() {
    assert_eq!(atd_protocol::ERR_UCAN_EXPIRED, 1011);
}

#[test]
fn err_delegation_too_deep_code_is_1012() {
    assert_eq!(atd_protocol::ERR_DELEGATION_TOO_DEEP, 1012);
}

#[test]
fn err_audience_mismatch_code_is_1013() {
    assert_eq!(atd_protocol::ERR_AUDIENCE_MISMATCH, 1013);
}
