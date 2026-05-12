# atd-server-http

Streamable-HTTP (MCP JSON-RPC) transport for ATD-speaking servers.
Sibling of [`atd-server`](https://crates.io/crates/atd-server) (the
Unix-socket transport); both consume the same
[`atd-runtime`](https://crates.io/crates/atd-runtime) `Registry`, so a
single `Tool` implementation reaches both UDS and HTTP clients without
code duplication.

## What this is

A `POST /mcp` axum router that translates MCP JSON-RPC 2.0
(`initialize` / `tools/list` / `tools/call` / `notifications/initialized`)
into ATD operations and dispatches via `atd_runtime::dispatch::run_tool`.
Bytes returned from `Tool::call` are byte-identical to the UDS path
(parity test in this crate's `tests/e2e_parity.rs` is the regression
guard).

Designed for adopters who need **cloud-hosted ATD servers** —
i.e., agents (Claude Desktop, Cursor, browser-based assistants) that
speak MCP Streamable HTTP and can't open a Unix socket directly.

## Adopters in production

- **[celia_phr](https://github.com/downsea/celia_phr)** — Tauri PHR
  app exposes `/mcp` for its Hermes / Claude assistants;
  `CeliaConsentTokenBroker` (SQLite-backed) drives bearer auth.

## Features

- Origin allow-list (mitigates DNS-rebinding attacks)
- Bearer-token auth via the [`TokenBroker`](https://docs.rs/atd-runtime/latest/atd_runtime/trait.TokenBroker.html) trait
  - Typed `BearerOutcome` (SP-token-broker-phase2) with per-variant
    HTTP status + `WWW-Authenticate` / `Retry-After` headers (spec §4.4)
  - SSE long-connection bearer-refresh helper
    (`spawn_bearer_refresh`, spec §4.7)
- `/initialize` advertises broker's `accepted_token_formats()` under
  `capabilities.experimental.atd.acceptedTokenFormats`
- UCAN-lite capability tokens via additive `Hello.ucan_tokens` field
  (SP-capability-v2)

## Composition (SP-streamable-http §4.5)

```rust
use atd_runtime::Registry;
use atd_server_http::{HttpServerConfig, Server};

let registry: Registry = my_tools();
let cfg = HttpServerConfig::default();
let (router, server) = Server::builder(registry)
    .config(cfg)
    .build();
// Adopters may extend the router with their own routes here
// (e.g. /chat/stream for SSE).
server.serve(router).await?;
```

## What is NOT in this crate

- TLS termination — operators front with nginx / Caddy / Tauri
- OAuth 2.1 token issuance — bearer is validated, not minted; adopters
  bring their own broker
- `Mcp-Session-Id` sticky sessions / resumability / `Last-Event-ID` —
  reserved (SP-streamable-http §9)
- SSE inside `tools/call` — single response per request; adopters
  layer SSE on adopter-owned routes

## Part of the ATD reference implementation

This crate is part of [ATD — Agent Tool Dispatch](https://github.com/downsea/atd-mvp).
See [`docs/architecture.md`](https://github.com/downsea/atd-mvp/blob/master/docs/architecture.md)
for the full layer model and [`docs/integrations/`](https://github.com/downsea/atd-mvp/tree/master/docs/integrations)
for adopter-side tutorials.

## License

Apache-2.0.
