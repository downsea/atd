//! JSON-RPC 2.0 over newline-delimited UTF-8 (MCP stdio transport,
//! spec 2025-11-25 §transports — "messages are delimited by newlines and
//! must not contain embedded newlines").
//!
//! We do not validate the full JSON-RPC 2.0 grammar — our peer (MCP client
//! like Hermes) produces well-formed requests. We do produce spec-conformant
//! responses.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Success {
        jsonrpc: String,
        id: serde_json::Value,
        result: serde_json::Value,
    },
    Error {
        jsonrpc: String,
        id: serde_json::Value,
        error: RpcError,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Response {
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Response::Success {
            jsonrpc: "2.0".into(),
            id,
            result,
        }
    }

    pub fn err(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Response::Error {
            jsonrpc: "2.0".into(),
            id,
            error: RpcError {
                code,
                message: message.into(),
                data: None,
            },
        }
    }
}

/// Read one JSON-RPC message (one line). Returns Ok(None) on clean EOF.
pub fn read_request<R: BufRead>(reader: &mut R) -> std::io::Result<Option<Request>> {
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        // MCP spec forbids embedded newlines but doesn't forbid blank lines.
        // Skip them politely.
        return read_request(reader);
    }
    let req: Request = serde_json::from_str(trimmed)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(req))
}

/// Write one JSON-RPC response terminated by `\n` and flush.
pub fn write_response<W: Write>(writer: &mut W, resp: &Response) -> std::io::Result<()> {
    let json = serde_json::to_string(resp)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    writer.write_all(json.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn read_request_parses_single_line() {
        let mut cursor =
            Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n".to_vec());
        let req = read_request(&mut cursor).unwrap().unwrap();
        assert_eq!(req.method, "ping");
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.id.unwrap(), serde_json::json!(1));
    }

    #[test]
    fn read_request_returns_none_on_eof() {
        let mut cursor = Cursor::new(b"".to_vec());
        assert!(read_request(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn read_request_skips_blank_lines() {
        let mut cursor =
            Cursor::new(b"\n\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"x\"}\n".to_vec());
        let req = read_request(&mut cursor).unwrap().unwrap();
        assert_eq!(req.method, "x");
    }

    #[test]
    fn write_response_emits_one_line() {
        let mut buf: Vec<u8> = Vec::new();
        write_response(
            &mut buf,
            &Response::ok(serde_json::json!(1), serde_json::json!({"ok": true})),
        )
        .unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.ends_with('\n'));
        assert_eq!(s.matches('\n').count(), 1);
        assert!(s.contains("\"jsonrpc\":\"2.0\""));
        assert!(s.contains("\"id\":1"));
        assert!(s.contains("\"result\""));
    }

    #[test]
    fn error_response_has_error_field() {
        let mut buf: Vec<u8> = Vec::new();
        write_response(
            &mut buf,
            &Response::err(serde_json::json!(7), -32601, "method not found"),
        )
        .unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\"error\""));
        assert!(s.contains("\"code\":-32601"));
        assert!(s.contains("method not found"));
    }
}
