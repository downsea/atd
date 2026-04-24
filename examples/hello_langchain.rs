//! Demo of atd-client's LangChain (and OpenAI-shape) adapter.
//!
//! Uses fake ToolSummaries — self-contained, no server needed.
//! Prints JSON-formatted output of 3 tool descriptions via `as_langchain_tools`.
//!
//! Run:
//!   cargo run --example hello_langchain -p atd-examples --features langchain

use atd_client::adapters::langchain::as_langchain_tools;
use atd_protocol::ToolSummary;
use serde_json::json;

fn main() {
    // Normally you'd get these via client.discover(). Using fakes here to
    // keep the example self-contained and adapter-focused.
    let summaries = vec![
        fake_summary("ref:echo.say", "Echo test anchor"),
        fake_summary("ref:shell.exec", "Run a bash command"),
        fake_summary("ref:fs.read", "Read a UTF-8 text file"),
    ];

    let tools = as_langchain_tools(&summaries);
    println!(
        "{}",
        serde_json::to_string_pretty(&tools).unwrap()
    );
}

fn fake_summary(id: &str, desc: &str) -> ToolSummary {
    ToolSummary {
        id: id.into(),
        name: id.into(),
        description: desc.into(),
        domain: "demo".into(),
        tier: atd_protocol::ToolTier::Warm,
        visibility: atd_protocol::ToolVisibility::Read,
        tags: vec![],
        input_schema: Some(json!({
            "type": "object",
            "properties": {"arg": {"type": "string"}},
        })),
    }
}
