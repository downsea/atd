# atd-server

Unix-socket listener and per-connection task layer for
[Agent Tool Dispatch (ATD)](https://github.com/downsea/atd) servers.

This is the Unix-socket transport. It wraps the transport-agnostic
[`atd-runtime`](https://crates.io/crates/atd-runtime) `Registry` (which holds
the `Tool` trait and dispatch pipeline) with an accept loop, so you can host an
ATD-speaking server in ~30 lines. Its sibling
[`atd-server-http`](https://crates.io/crates/atd-server-http) does the same for
HTTP — both consume the same `Registry`.

## Minimal example

```rust,no_run
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

- `Server` — accept loop; spawns one tokio task per incoming connection, with
  handshake + frame deadlines
- `ServerConfig` — socket path, audit sink, granted capabilities, token broker,
  and the `server_version` identity string returned in the `Hello` handshake
- One-shot pre-`run()` mutators: `set_middleware`, `set_tier_policy`,
  `set_ucan_revocation_store`
- Per-connection frame I/O wired to `atd-protocol`'s wire codec
- `ServerError` — a type alias of `std::io::Error`

## Why this is its own crate

`atd-runtime` is transport-agnostic (just `Tool`, `Registry`, dispatch,
middleware, capability gate). `atd-server` adds the Unix-socket transport.
Keeping them separate lets the same `Registry` also be served over HTTP via
`atd-server-http`, or a future transport, without touching tool code. Vendors
building their own ATD server depend on `atd-runtime` + `atd-server` and skip
[`atd-ref-server`](https://crates.io/crates/atd-ref-server)'s built-in tool
wiring.

For a complete reference using this crate, see `atd-ref-server` — it registers
the nine built-in reference tools (echo + fs + shell + web) on top of
`atd-server`.

## License

Apache-2.0.
</content>
