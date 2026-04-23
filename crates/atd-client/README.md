# atd-client

Rust client SDK for the [Agent Tool Dispatch (ATD) protocol](https://github.com/<YOUR_USERNAME>/atd-mvp).

Connect to any ATD-speaking server over a Unix socket, discover tools, describe them, and call them.

## Install

```bash
cargo add atd-client
```

## Quick example

```rust
use atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = AtdClient::connect(
        Endpoint::unix("/tmp/my-atd.sock")
    ).await?;

    let tools = client.discover(None, DiscoverFilter::default()).await?;
    println!("{} tools available", tools.len());

    let result = client.call(
        "ref:echo.say",
        serde_json::json!({"text": "hello"}),
        CallOptions { dry_run: false, preferred_binding: None },
    ).await?;

    println!("{result:?}");
    Ok(())
}
```

## Features

- `discover` + `describe` + `call` — the full ATD v0.1 surface
- Async (tokio)
- Length-prefixed JSON wire protocol over Unix sockets
- No server dependency — works against any ATD-speaking server (including
  the reference server, `atd-ref-server`)

## See also

- [`atd-types`](https://crates.io/crates/atd-types) — shared protocol types
- [`atd-mcp-bridge`](https://crates.io/crates/atd-mcp-bridge) — MCP bridge
  for third-party MCP clients like Claude Desktop, Cursor, Hermes

## License

Apache-2.0. See [LICENSE](https://github.com/<YOUR_USERNAME>/atd-mvp/blob/master/LICENSE).
