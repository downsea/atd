//! Minimum working example: connect to the ANOS daemon (or any ATD server)
//! over a Unix socket, discover up to 3 tools, describe the first one, and
//! call it with `dry_run=true`. Prints structured output at each step.
//!
//! Run:
//!   ANOS_SOCK=~/.anos/anos.sock cargo run -p atd-examples --bin hello_atd

use atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sock = std::env::var("ANOS_SOCK").ok().map(std::path::PathBuf::from);
    let endpoint = match sock {
        Some(p) => Endpoint::unix(p),
        None => Endpoint::default_anos(),
    };

    println!("[atd] connecting to {endpoint:?}");
    let client = AtdClient::connect(endpoint).await?;
    println!("[atd] connected");

    let tools = client
        .discover(
            None,
            DiscoverFilter {
                limit: Some(3),
                ..Default::default()
            },
        )
        .await?;
    println!("[atd] {} tools discovered", tools.len());
    for t in &tools {
        println!("        - {} ({})", t.id, t.name);
    }

    let Some(first) = tools.first() else {
        println!("[atd] no tools to describe/call — done.");
        return Ok(());
    };

    let def = client.describe(&first.id).await?;
    println!(
        "[atd] describe({}) → domain={}, bindings={}",
        def.id,
        def.capability.domain,
        def.bindings.len()
    );

    let result = client
        .call(
            &first.id,
            serde_json::json!({}),
            CallOptions {
                dry_run: true,
                preferred_binding: None,
            },
        )
        .await?;

    match result {
        atd_types::ToolResult::Success { data, .. } => {
            println!("[atd] call ok: {}", serde_json::to_string(&data)?);
        }
        atd_types::ToolResult::Error { code, message, .. } => {
            println!("[atd] call error: {code} — {message}");
        }
    }

    Ok(())
}
