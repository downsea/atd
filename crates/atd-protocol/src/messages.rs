use serde::{Deserialize, Serialize};

/// Wire value of `code` on `Response::Error` when dispatch refuses a call
/// whose `required_capabilities` are not a subset of the connection's
/// granted capability set. SP-12 Task 2.
pub const ERR_CAPABILITY_DENIED: u16 = 1001;

/// Wire value of `code` on `Response::Error` when dispatch refuses
/// a call because the tool's `max_concurrent` semaphore is saturated.
/// SP-operability-v1 C2.
pub const ERR_RATE_LIMITED: u16 = 1002;

/// Request frames sent from client → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping,

    /// SP-12 Hello handshake. Optional: pre-SP-12 servers do not recognize
    /// it; `AtdClient::hello` tolerates that and returns an empty granted
    /// set so callers can treat "no capabilities" and "server too old"
    /// identically.
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

/// Response frames sent from server → client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "pong")]
    Pong,

    #[serde(rename = "hello_ack")]
    HelloAck {
        #[serde(default)]
        granted_capabilities: Vec<String>,
        #[serde(default)]
        server_version: String,
        #[serde(default)]
        supported_tiers: Vec<String>,
    },

    #[serde(rename = "tool_list")]
    ToolListResponse { tools: serde_json::Value },

    #[serde(rename = "tool_schema")]
    ToolSchemaResponse { schema: serde_json::Value },

    #[serde(rename = "tool_result")]
    ToolResultResponse {
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
        let j = serde_json::to_string(&Request::Ping).unwrap();
        assert_eq!(j, r#"{"type":"ping"}"#);
    }

    #[test]
    fn run_tool_roundtrip() {
        let r = Request::RunTool {
            tool_id: "anos:fs.read".into(),
            args: serde_json::json!({"path": "/tmp/x"}),
            dry_run: false,
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&j).unwrap();
        match back {
            Request::RunTool {
                tool_id, dry_run, ..
            } => {
                assert_eq!(tool_id, "anos:fs.read");
                assert!(!dry_run);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tool_list_response_carries_array() {
        let r = Response::ToolListResponse {
            tools: serde_json::json!([{"id": "a"}, {"id": "b"}]),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"type\":\"tool_list\""));
        let back: Response = serde_json::from_str(&j).unwrap();
        match back {
            Response::ToolListResponse { tools } => {
                assert_eq!(tools.as_array().unwrap().len(), 2);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn error_deserializes_with_optional_fields_missing() {
        let j = r#"{"type":"error","message":"boom"}"#;
        let back: Response = serde_json::from_str(j).unwrap();
        match back {
            Response::Error {
                message,
                code,
                retryable,
                details,
            } => {
                assert_eq!(message, "boom");
                assert!(code.is_none());
                assert!(retryable.is_none());
                assert!(details.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }
}
