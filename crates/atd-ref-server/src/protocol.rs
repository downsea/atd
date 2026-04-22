//! Wire message types.
//!
//! Tag names match the Rust atd-client (`ping`, `tool_list`, `tool_schema`,
//! `run_tool`, `pong`, `error`) so both sides speak the same JSON. Type
//! definitions are independent — this server has no dep on atd-client.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "tool_list")]
    ToolList,

    #[serde(rename = "tool_schema")]
    ToolSchema { tool_id: String },

    #[serde(rename = "run_tool")]
    RunTool {
        tool_id: String,
        args: serde_json::Value,
        dry_run: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "pong")]
    Pong,

    #[serde(rename = "tool_list")]
    ToolList { tools: serde_json::Value },

    #[serde(rename = "tool_schema")]
    ToolSchema { schema: serde_json::Value },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_id: String,
        result: serde_json::Value,
        success: bool,
        dry_run: bool,
    },

    #[serde(rename = "error")]
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retryable: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_serializes_with_type_tag() {
        assert_eq!(
            serde_json::to_string(&Request::Ping).unwrap(),
            r#"{"type":"ping"}"#
        );
    }

    #[test]
    fn tool_list_request_is_unit_variant_on_wire() {
        let j = serde_json::to_string(&Request::ToolList).unwrap();
        assert_eq!(j, r#"{"type":"tool_list"}"#);
    }

    #[test]
    fn tool_schema_carries_tool_id() {
        let r = Request::ToolSchema { tool_id: "ref:echo.say".into() };
        let j = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&j).unwrap();
        match back {
            Request::ToolSchema { tool_id } => assert_eq!(tool_id, "ref:echo.say"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn run_tool_roundtrip_with_all_fields() {
        let r = Request::RunTool {
            tool_id: "ref:echo.say".into(),
            args: serde_json::json!({"a": 1, "b": [2]}),
            dry_run: true,
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&j).unwrap();
        match back {
            Request::RunTool { tool_id, args, dry_run } => {
                assert_eq!(tool_id, "ref:echo.say");
                assert_eq!(args["a"], 1);
                assert!(dry_run);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tool_result_serializes_with_success_flag() {
        let r = Response::ToolResult {
            tool_id: "ref:echo.say".into(),
            result: serde_json::json!({"echoed": {}}),
            success: true,
            dry_run: false,
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains(r#""type":"tool_result""#));
        assert!(j.contains(r#""success":true"#));
    }

    #[test]
    fn error_response_omits_null_optionals_when_missing() {
        let r = Response::Error {
            message: "boom".into(),
            code: None,
            retryable: None,
            details: None,
        };
        let j = serde_json::to_string(&r).unwrap();
        assert_eq!(j, r#"{"type":"error","message":"boom"}"#);
    }
}
