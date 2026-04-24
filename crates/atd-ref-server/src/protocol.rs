//! Wire message types.
//!
//! Tag names match the Rust atd-client (`ping`, `tool_list`, `tool_schema`,
//! `run_tool`, `pong`, `error`) so both sides speak the same JSON. Type
//! definitions are independent — this server has no dep on atd-client.

use serde::{Deserialize, Serialize};

/// Error code emitted when a client calls a tool whose required capabilities
/// are not a subset of the connection's granted set. Surfaced on the wire via
/// `Response::Error { code: Some(ERR_CAPABILITY_DENIED), ... }`.
pub const ERR_CAPABILITY_DENIED: u16 = 1001;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "hello")]
    Hello {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        #[serde(default)]
        requested_capabilities: Vec<String>,
    },

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

    #[serde(rename = "hello_ack")]
    HelloAck {
        granted_capabilities: Vec<String>,
        server_version: String,
        supported_tiers: Vec<String>,
    },

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

    #[test]
    fn hello_serializes_with_default_empty_caps() {
        let r = Request::Hello {
            client_id: None,
            requested_capabilities: vec![],
        };
        let j = serde_json::to_string(&r).unwrap();
        // client_id is skipped when None; requested_capabilities serialized empty.
        assert_eq!(j, r#"{"type":"hello","requested_capabilities":[]}"#);
    }

    #[test]
    fn hello_roundtrip_with_client_id_and_caps() {
        let r = Request::Hello {
            client_id: Some("agent-7".into()),
            requested_capabilities: vec!["read".into(), "exec".into()],
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&j).unwrap();
        match back {
            Request::Hello {
                client_id,
                requested_capabilities,
            } => {
                assert_eq!(client_id.as_deref(), Some("agent-7"));
                assert_eq!(requested_capabilities, vec!["read", "exec"]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn hello_deserializes_with_missing_fields() {
        // requested_capabilities default = [], client_id default = None.
        let j = r#"{"type":"hello"}"#;
        let back: Request = serde_json::from_str(j).unwrap();
        match back {
            Request::Hello {
                client_id,
                requested_capabilities,
            } => {
                assert!(client_id.is_none());
                assert!(requested_capabilities.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn hello_ack_roundtrip() {
        let r = Response::HelloAck {
            granted_capabilities: vec!["read".into()],
            server_version: "atd-ref-server 0.2.0".into(),
            supported_tiers: vec!["hot".into(), "warm".into(), "cold".into()],
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains(r#""type":"hello_ack""#));
        let back: Response = serde_json::from_str(&j).unwrap();
        match back {
            Response::HelloAck {
                granted_capabilities,
                server_version,
                supported_tiers,
            } => {
                assert_eq!(granted_capabilities, vec!["read"]);
                assert_eq!(server_version, "atd-ref-server 0.2.0");
                assert_eq!(supported_tiers, vec!["hot", "warm", "cold"]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn err_capability_denied_constant_is_1001() {
        // Regression pin — the wire value is part of the protocol.
        assert_eq!(ERR_CAPABILITY_DENIED, 1001);
    }

    #[test]
    fn existing_ping_pong_unchanged() {
        // Regression: SP-12 additions must not change Ping/Pong wire form.
        assert_eq!(
            serde_json::to_string(&Request::Ping).unwrap(),
            r#"{"type":"ping"}"#
        );
        assert_eq!(
            serde_json::to_string(&Response::Pong).unwrap(),
            r#"{"type":"pong"}"#
        );
    }
}
