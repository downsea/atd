# ATD — Agent Tool Dispatch

[![CI](https://github.com/downsea/atd/actions/workflows/ci.yml/badge.svg)](https://github.com/downsea/atd/actions/workflows/ci.yml)

**The reference implementation of the Agent Tool Dispatch (ATD) protocol.**

ATD is a neutral, cross-vendor wire protocol that lets **any LLM agent, on any
framework, call any tool, on any platform** — through a single typed RPC
surface. This repository is the Rust source of truth: a wire-type crate, a
server runtime, a client SDK, two transports, middleware, built-in tools, an
MCP bridge, a CLI, and a conformance suite — plus a Python package mirror.

> **New here?** Read [`docs/index.md`](docs/index.md) for the documentation map,
> or [`docs/architecture.md`](docs/architecture.md) for the full picture.
> AI coding agents: start at [`AGENTS.md`](AGENTS.md).

## Why ATD

| Dimension | Fragmentation today | ATD's answer |
|---|---|---|
| Any tool | CLI, REST, MCP, native SDK — incompatible shapes | One `ToolDefinition`, many bindings |
| Any platform | Linux / macOS / Windows / mobile each differ | Binding selection is server-side at dispatch |
| Any agent | Claude Code can't consume OpenAI shapes without a shim | All agents call one SDK; adapters render per-provider |
| Any framework | LangChain tool ≠ MCP tool ≠ App Intent | One definition, many framework consumers |

Every message, in every direction, over every transport, serialises to one
machine-readable schema: [`atd-protocol-schema.json`](atd-protocol-schema.json).

## Quick start

```bash
git clone https://github.com/downsea/atd
cd atd
cargo run --example hello_atd -p atd-examples
```

The example auto-spawns the reference server and exercises three tools:

```
[atd] auto-spawning atd-ref-server → /tmp/.../demo.sock
[atd] connected — 10 tools registered

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"echoed":{"text":"hello from ATD"}}
[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → 9 paths: Cargo.toml, crates/atd-cli/Cargo.toml, ...
[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout="Linux"
```

No external daemon — everything runs from this repo, with zero ANOS dependency.

## Using the atd crates

Every crate publishes to crates.io under the `atd-*` prefix at one shared
version — pin `atd-sdk = "1"` and the whole stack stays mutually consistent.
Pick by what you're doing:

### Call ATD tools from an agent — `atd-sdk`

```bash
cargo add atd-sdk          # Rust   ·   Python:  pip install atd-client
```

```rust
use atd_sdk::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

let atd = AtdClient::connect(Endpoint::unix("/tmp/atd.sock")).await?;
let tools = atd.discover(None, DiscoverFilter::default()).await?;        // discovery
let out = atd.call("ref:echo.say",
    serde_json::json!({ "text": "hello" }), CallOptions::default()).await?;
```

Full walkthrough — discover / describe / call, the OpenAI · Anthropic ·
LangChain adapters, error handling: [Rust quickstart](docs/quickstart/rust.md) ·
[Python quickstart](docs/quickstart/python.md).

### Build your own ATD server (publish tools) — `atd-runtime` + a listener

```bash
cargo add atd-runtime atd-server        # Unix-socket transport
cargo add atd-runtime atd-server-http   # …or HTTP + MCP JSON-RPC
```

Implement the `Tool` trait, register it, hand the registry to a listener:

```rust
let mut registry = Registry::new();
registry.register(Arc::new(MyTool::new()));
atd_server::Server::new(registry, ServerConfig::default()).run().await?;
```

How-to: [docs/extending/tool.md](docs/extending/tool.md). Working skeleton
(~80 lines): [`crates/atd-mock-weather-server`](crates/atd-mock-weather-server).
Opt-in egress middleware: `atd-middleware-fhir`,
`atd-middleware-pii-redact-medical`.

### Reach an ATD server from an MCP client — `atd-mcp-bridge`

```bash
cargo install atd-mcp-bridge
```

Point Claude Desktop / Cursor / Hermes / any MCP client at the bridge binary —
config examples in [docs/integrations/](docs/integrations/).

### Drive ATD from the command line — `atd-cli`

```bash
cargo install atd-cli
atd --sock /tmp/atd.sock list           # subcommands: list · schema · call · doctor · skills
```

See [docs/cli.md](docs/cli.md).

### Verify a third-party implementation — `atd-conformance`

Dev-dep on `atd-conformance` and run the cross-implementation fixture corpus
against your server — pass it and you interoperate. See
[`crates/atd-conformance`](crates/atd-conformance).

> Not sure which path fits? [docs/integrations/overview.md](docs/integrations/overview.md)
> maps every framework to one of five integration paths; the full 16-crate
> map is [docs/architecture.md](docs/architecture.md) §9.

## What ships

- **Protocol** — length-prefixed JSON over a duplex byte stream; one unified
  schema; a full `AtdError` taxonomy.
- **Two transports** — `atd-server` (Unix socket) and `atd-server-http` (HTTP +
  MCP JSON-RPC), both routing into one transport-agnostic dispatcher.
- **Dispatch** — capability gate, tier-aware deadlines, pluggable bindings,
  HMAC-signed cursor pagination, an egress middleware pipeline.
- **Security** — capability allow-listing, UCAN-lite bearer tokens, multi-tenant
  secret routing (`TokenBroker`), structured audit.
- **Reference server** — `atd-ref-server` with 10 built-in tools
  (`ref:echo.say`, `ref:fs.{read,write,edit,glob,grep}`,
  `ref:shell.{exec,pwsh}`, `ref:web.fetch`, `ref:external.uname`).
- **Medical middleware** — FHIR R4 egress validation and HIPAA PHI redaction as
  opt-in crates.
- **Conformance suite** — `atd-conformance`; pass it and you interoperate.
- **MCP bridge** — `atd-mcp-bridge` connects Claude Desktop, Cursor, Hermes, and
  any MCP client to any ATD server.

Full inventory: [`CHANGELOG.md`](CHANGELOG.md).

## Architecture at a glance

```
┌──────────────┐  length-prefixed JSON   ┌─────────────────────────────────────┐
│   atd-sdk    │ ←────────────────────→  │ atd-ref-server                       │
│  (client)    │   Unix socket / HTTP    │  Hello → capability gate             │
└──────────────┘                         │  registry → tier → binding → mw      │
                                          └─────────────────────────────────────┘
┌──────────────┐   MCP JSON-RPC    ┌────────────────┐      ┌──────────────┐
│  MCP client  │ ← stdio ────────→ │ atd-mcp-bridge │ ←──→ │  ATD server  │
└──────────────┘                   └────────────────┘      └──────────────┘
```

The full layer model, dispatch pipeline, security model, and crate map are in
[`docs/architecture.md`](docs/architecture.md).

## Extending ATD

ATD attaches third-party code without forking — every extension point is a
`pub` trait with a how-to guide in [`docs/extending/`](docs/extending/): add a
[tool](docs/extending/tool.md), [binding](docs/extending/binding.md),
[middleware](docs/extending/middleware.md),
[transport](docs/extending/transport.md),
[auth scheme](docs/extending/token-broker.md), or
[audit sink](docs/extending/audit-sink.md).

## Documentation

- [`docs/index.md`](docs/index.md) — the documentation map (start here)
- [`docs/architecture.md`](docs/architecture.md) — normative architecture
- [`docs/protocol/`](docs/protocol/) — wire format + error taxonomy
- [`docs/extending/`](docs/extending/) — how to extend each layer
- [`docs/integrations/`](docs/integrations/) — per-framework wiring
- [`docs/roadmap.md`](docs/roadmap.md) — evolution scope and deferred work
- [`docs/issues/`](docs/issues/) — tracked gaps and adopter validation

The wire schema is regenerated from the Rust types and gated in CI:

```bash
cargo run -p atd-protocol --features schema --bin gen-schema -- --check
```

## Project status

**1.0** — the wire format and the public extension traits are frozen for the
1.x line. See [`docs/release-plan-v1.0.md`](docs/release-plan-v1.0.md) for the
stability contract and [`CHANGELOG.md`](CHANGELOG.md) for release history.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Issues, PRs, and design feedback
welcome.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
