//! MCP (Model Context Protocol) message shapes for initialize/tools/list/tools/call.
//! Spec: https://modelcontextprotocol.io/specification/2025-11-25

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    #[serde(default)]
    pub capabilities: serde_json::Value,
    #[serde(rename = "clientInfo", default)]
    pub client_info: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged", default)]
    pub list_changed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolsListResult {
    pub tools: Vec<Tool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolsCallParams {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolsCallResult {
    pub content: Vec<ContentBlock>,
    #[serde(rename = "isError", default, skip_serializing_if = "is_false")]
    pub is_error: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_result_serializes_camel_case() {
        let r = InitializeResult {
            protocol_version: "2025-11-25".into(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability::default()),
            },
            server_info: ServerInfo {
                name: "atd-mcp-bridge".into(),
                version: "0.1.0".into(),
            },
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"protocolVersion\":\"2025-11-25\""));
        assert!(j.contains("\"serverInfo\""));
        assert!(j.contains("\"tools\""));
    }

    #[test]
    fn tool_serializes_with_camel_case_inputSchema() {
        let t = Tool {
            name: "x".into(),
            description: "d".into(),
            input_schema: serde_json::json!({"type":"object"}),
        };
        let j = serde_json::to_string(&t).unwrap();
        assert!(j.contains("\"inputSchema\""));
    }

    #[test]
    fn tools_call_result_content_is_text_tagged() {
        let r = ToolsCallResult {
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
            is_error: false,
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"type\":\"text\""));
        assert!(j.contains("\"text\":\"hello\""));
        assert!(
            !j.contains("isError"),
            "isError should be suppressed when false, got: {j}"
        );
    }

    #[test]
    fn tools_call_result_error_flag_emitted_when_true() {
        let r = ToolsCallResult {
            content: vec![ContentBlock::Text {
                text: "fail".into(),
            }],
            is_error: true,
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"isError\":true"));
    }

    #[test]
    fn initialize_params_parses_from_hermes_style_request() {
        let j = r#"{"protocolVersion":"2025-11-25","capabilities":{"roots":{}},"clientInfo":{"name":"Hermes","version":"0.9.0"}}"#;
        let p: InitializeParams = serde_json::from_str(j).unwrap();
        assert_eq!(p.protocol_version, "2025-11-25");
    }
}
