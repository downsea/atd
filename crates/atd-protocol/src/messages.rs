use serde::{Deserialize, Serialize};

/// Wire value of `code` on `Response::Error` when dispatch refuses a call
/// whose `required_capabilities` are not a subset of the connection's
/// granted capability set. SP-12 Task 2.
pub const ERR_CAPABILITY_DENIED: u16 = 1001;

/// Wire value of `code` on `Response::Error` when dispatch refuses
/// a call because the tool's `max_concurrent` semaphore is saturated.
/// SP-operability-v1 C2.
pub const ERR_RATE_LIMITED: u16 = 1002;

/// Wire value of `code` on `Response::Error` when a configured
/// `TokenBroker` returns `Err(_)` while resolving secrets for the
/// caller. Server-side only; SDKs may surface this code but won't
/// generate it. `retryable: true` because broker failures may be
/// transient (network blip, secret manager hiccup).
/// SP-token-broker-phase1.
pub const ERR_BROKER_FAILED: u16 = 1003;

/// Wire value of `code` on `Response::Error` when a `Hello.ucan_tokens`
/// entry fails structural / signature validation: malformed JWT,
/// unsupported `alg`, unsupported DID method, bad signature, missing
/// required field, or chain-attenuation widening.
/// `retryable: false` — deterministic; retry without changing the token
/// is pointless. SP-capability-v2.
pub const ERR_UCAN_INVALID: u16 = 1010;

/// Wire value of `code` on `Response::Error` when a UCAN chain link has
/// `exp <= now()`. `retryable: false` (re-issue required).
/// SP-capability-v2.
pub const ERR_UCAN_EXPIRED: u16 = 1011;

/// Wire value of `code` on `Response::Error` when a UCAN chain exceeds
/// `ServerConfig.max_ucan_chain_depth` (default 5).
/// `retryable: false`. SP-capability-v2.
pub const ERR_DELEGATION_TOO_DEEP: u16 = 1012;

/// Wire value of `code` on `Response::Error` when the deepest UCAN's
/// `aud` does not match the connection's `client_id` (or the bearer's
/// caller). Prevents intercepted-token replay by a third party.
/// `retryable: false`. SP-capability-v2.
pub const ERR_AUDIENCE_MISMATCH: u16 = 1013;

/// Wire value of `code` on `Response::Error` when a `Request::RunToolContinue`
/// presents a cursor whose `issued_at_unix` is older than the server's
/// `cursor_ttl_seconds` (default 300s) or whose `server_session` does not
/// match the current server-process random nonce (server-restart invalidation).
/// `retryable: false` — the cursor is permanently dead; the client must
/// re-issue the original `RunTool` to get a fresh cursor. SP-pagination-v1.
pub const ERR_CURSOR_EXPIRED: u16 = 1020;

/// Wire value of `code` on `Response::Error` when a cursor fails HMAC
/// verification, has malformed framing, references a non-matching
/// `tool_id`, or carries an `args_fingerprint` that doesn't match the
/// continuation's intended args. Distinct from `ERR_CURSOR_EXPIRED`
/// because an invalid cursor suggests a bug or attack (forge attempt)
/// while expiry is a normal lifecycle event — ops alert differently.
/// `retryable: false`. SP-pagination-v1.
pub const ERR_CURSOR_INVALID: u16 = 1021;

/// Request frames sent from client → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping,

    /// SP-12 Hello handshake. Optional: pre-SP-12 servers do not recognize
    /// it; `AtdClient::hello` tolerates that and returns an empty granted
    /// set so callers can treat "no capabilities" and "server too old"
    /// identically.
    ///
    /// SP-capability-v2 (2026-05-11) adds the optional `ucan_tokens` field.
    /// When non-empty, each element is a UCAN-lite JWT compact form
    /// (`alg=EdDSA`, `typ=ucan/1.0+jwt`); the server's verifier produces
    /// `granted = granted_strings ∪ granted_ucan`. Pre-SP-capability-v2
    /// servers omit the field via serde default; their behaviour is byte-
    /// identical to SP-12. Spec §4.2 + §5.2.
    #[serde(rename = "hello")]
    Hello {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        #[serde(default)]
        requested_capabilities: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ucan_tokens: Vec<String>,
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

    /// Fetch the next page of a paginated tool result. The cursor is the
    /// server-issued opaque string from a prior `Response::ToolResultResponse.next_cursor`.
    /// `tool_id` must match the cursor's embedded tool_id (server validates).
    /// SP-pagination-v1 §4.1.
    #[serde(rename = "run_tool_continue")]
    RunToolContinue { tool_id: String, cursor: String },
}

/// Response frames sent from server → client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
        /// SP-pagination-v1 §4.1 — when present, the tool has more pages.
        /// The client passes this verbatim to `Request::RunToolContinue.cursor`
        /// to fetch the next page. Server-opaque (HMAC-signed in the
        /// reference impl); clients MUST NOT parse it. Absent on terminal
        /// pages and on responses from non-paginating tools.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
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

    // ---- SP-pagination-v1 Phase B: wire format round-trips ----

    #[test]
    fn run_tool_continue_round_trips() {
        let r = Request::RunToolContinue {
            tool_id: "celia:fhir.list_observations".into(),
            cursor: "abc123".into(),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains(r#""type":"run_tool_continue""#));
        let back: Request = serde_json::from_str(&j).unwrap();
        match back {
            Request::RunToolContinue { tool_id, cursor } => {
                assert_eq!(tool_id, "celia:fhir.list_observations");
                assert_eq!(cursor, "abc123");
            }
            _ => panic!("wrong variant: {j}"),
        }
    }

    #[test]
    fn tool_result_response_without_next_cursor_omits_field_on_wire() {
        let r = Response::ToolResultResponse {
            tool_id: "x".into(),
            result: serde_json::json!({}),
            success: true,
            dry_run: false,
            next_cursor: None,
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(
            !j.contains("next_cursor"),
            "next_cursor: None must be omitted on the wire (back-compat), got: {j}"
        );
    }

    #[test]
    fn tool_result_response_with_next_cursor_includes_field_on_wire() {
        let r = Response::ToolResultResponse {
            tool_id: "x".into(),
            result: serde_json::json!({}),
            success: true,
            dry_run: false,
            next_cursor: Some("abc".into()),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(
            j.contains(r#""next_cursor":"abc""#),
            "next_cursor: Some(_) must serialize, got: {j}"
        );
    }

    #[test]
    fn tool_result_response_back_compat_default_when_field_missing() {
        // Pre-pagination wire shape (no next_cursor field) must deserialize
        // to next_cursor: None — adopters on old atd-mvp builds keep working.
        let j =
            r#"{"type":"tool_result","tool_id":"x","result":{},"success":true,"dry_run":false}"#;
        let back: Response = serde_json::from_str(j).unwrap();
        match back {
            Response::ToolResultResponse { next_cursor, .. } => {
                assert!(next_cursor.is_none(), "missing field must default to None");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn err_cursor_codes_distinct_from_existing_families() {
        // Sanity: don't collide with the existing 100x or 101x families.
        assert_eq!(ERR_CURSOR_EXPIRED, 1020);
        assert_eq!(ERR_CURSOR_INVALID, 1021);
        assert_ne!(ERR_CURSOR_EXPIRED, ERR_CURSOR_INVALID);
        assert_ne!(ERR_CURSOR_EXPIRED, ERR_AUDIENCE_MISMATCH);
        assert_ne!(ERR_CURSOR_INVALID, ERR_RATE_LIMITED);
    }
}
