# atd-sdk

Rust client SDK for the
[Agent Tool Dispatch (ATD) protocol](https://github.com/downsea/atd).

Connect to any ATD-speaking server, discover tools, describe them, call them,
and page through large results. This is the client half of the workspace — pair
it with a server built on [`atd-runtime`](https://crates.io/crates/atd-runtime)
(any reachable ATD endpoint will do).

## Install

```bash
cargo add atd-sdk
```

## Quick example

```rust
use atd_sdk::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

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

## Client surface

- `connect` / `connect_with_options` — open a connection to an `Endpoint`
- `hello` / `hello_with_ucan_tokens` — capability handshake (UCAN-lite tokens)
- `discover` — list tool summaries, with an optional query + `DiscoverFilter`
- `describe` — fetch a tool's full `ToolDefinition`
- `call` — invoke a tool
- `call_page` / `call_all` — paginated invocation over HMAC-signed cursors;
  `call_all` follows cursors and merges pages per `CallAllOptions`

The client is async (tokio) and speaks the length-prefixed JSON wire format. It
has no server dependency — it works against any ATD-speaking server, including
the reference server [`atd-ref-server`](https://crates.io/crates/atd-ref-server).

## See also

- [`atd-protocol`](https://crates.io/crates/atd-protocol) — shared protocol types
- [`atd-mcp-bridge`](https://crates.io/crates/atd-mcp-bridge) — MCP bridge for
  third-party MCP clients like Claude Desktop, Cursor, Hermes
- [`docs/quickstart/rust.md`](https://github.com/downsea/atd/blob/master/docs/quickstart/rust.md)
  — first-call walkthrough

## License

Apache-2.0. See [LICENSE](https://github.com/downsea/atd/blob/master/LICENSE).
</content>
