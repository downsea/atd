# atd-server

Unix-socket listener and per-connection task layer for [Agent Tool Dispatch (ATD)](https://github.com/downsea/atd-mvp) servers.

Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime) (which holds
the `Tool` trait and `Registry`) to host an ATD-speaking server in ~30 lines.

## Minimal example

```rust,no_run
use std::sync::Arc;
use atd_runtime::registry::Registry;
use atd_server::{Server, ServerConfig};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut registry = Registry::new();
    // registry.register(Arc::new(MyTool::new()));

    let mut config = ServerConfig::default();
    config.socket_path = "/tmp/my-atd.sock".into();
    config.server_version = concat!("my-server ", env!("CARGO_PKG_VERSION")).to_string();

    Server::new(registry, config).run().await
}
```

## What's in the box

- `Server` — accept loop; spawns one tokio task per incoming connection
- `ServerConfig` — sock path, audit log, granted capabilities, `server_version`
  identity string returned in the `Hello` handshake
- Per-connection frame I/O wired to `atd-protocol`'s wire codec
- `ServerError` — alias of `std::io::Error` for v0.2.x

For a complete reference using this crate, see
[`atd-ref-server`](https://crates.io/crates/atd-ref-server) — it adds the 9
built-in tools (echo + fs + shell + web) on top of `atd-server`.

## Why this is its own crate

`atd-runtime` is transport-agnostic (just `Tool`, `Registry`, dispatch,
middleware, capability gate) so it can compose with future stdio-based MCP
binding or a REST transport. `atd-server` adds the Unix-socket transport
the protocol uses today. Vendors building their own ATD-speaking server
depend on `atd-runtime + atd-server` and skip `atd-ref-server`'s built-in
tool wiring.

## License

Apache-2.0.
