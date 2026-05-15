# ATD — Agent Tool Dispatch

[![CI](https://github.com/downsea/atd-mvp/actions/workflows/ci.yml/badge.svg)](https://github.com/downsea/atd-mvp/actions/workflows/ci.yml)

**The reference implementation of the Agent Tool Dispatch (ATD) protocol.**

ATD is a neutral, cross-vendor wire protocol for letting any LLM agent
call any tool on any server. This repository hosts the reference
implementation: a Rust client SDK, a Rust reference server with 9 real
tools, and an MCP bridge that makes all of this work with Claude
Desktop, Cursor, Hermes, and any other MCP-speaking agent.

> **Naming.** The protocol is **Agent Tool Dispatch (ATD)** — that's the
> public-facing brand and the name of every published crate (`atd-protocol`,
> `atd-sdk`, `atd-runtime`, ...). This repository's directory name
> (`atd-mvp`) is an internal handle that pre-dates the brand and is
> retained to avoid breaking existing path-based dependencies; future
> work will rename it to `atd` once adopters have migrated to crates.io
> versions.

## What's new in 0.3.0 (federation + multi-tenant + performance + medical)

Highlights since `v0.2.1`. Full inventory in [`CHANGELOG.md`](CHANGELOG.md);
release strategy and per-crate publish matrix in
[`docs/release-plan-v0.3.0.md`](docs/release-plan-v0.3.0.md).

- **Perf-v1** — multi-thread tokio runtime + per-state wire deadlines + SDK
  connect-retry + bounded mpsc audit sink + metrics counters
  (`sp-concurrency-baseline`); HMAC-signed cursor pagination across the
  wire + `AtdClient::call_page` / `call_all` SDK ergonomics
  (`sp-pagination-v1`). 50-client storm: p99=125ms, 0 errors, 0 audit drops.
- **Security** — UCAN-lite capability tokens with attenuation chains +
  revocation store (`sp-capability-v2`); HTTP bearer auth on the wire
  with typed `BearerOutcome` + SSE refresh helper
  (`sp-token-broker-phase2`); disk-backed `FileTokenBroker` for
  multi-tenant production deployments (`phase-l-0`).
- **Medical** — `atd-middleware-fhir` (FHIR R4 egress validation + 75-URI
  coding whitelist set-equal to celia's source-of-truth) and
  `atd-middleware-pii-redact-medical` (HIPAA Safe Harbor PHI redaction).
  Both mount via the existing `Middleware` trait.
- **Transports** — `atd-server-http` for HTTP/MCP-JSON-RPC adopters
  alongside the original UDS listener (`sp-streamable-http`).
- **Federation** — Phase L.0 cross-repo invariant `I1` keeps atd-mvp's
  coding-system whitelist set-equal to celia's via vendored toml +
  `include_str!`-loaded drift-guard test (`phase-l-0`).

If you're a path-dep adopter (`celia_phr`, `healthkit_cli`), `cargo
update` picks up everything above. crates.io publish for v0.3.0 is
gated by the checklist in the release-plan doc.

## Quick start

```bash
git clone https://github.com/downsea/atd-mvp
cd atd-mvp
cargo build --release -p atd-ref-server
cargo run --example hello_atd -p atd-examples
```

Expected output:
```
[atd] auto-spawning atd-ref-server → /tmp/.../demo.sock
[atd] connected
[atd] 10 tools registered   # 9 native + ref:external.uname on unix (SP-12)

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"echoed":{"text":"hello from ATD"}}
[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → 9 paths: Cargo.toml, crates/atd-cli/Cargo.toml, ...
[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout="Linux"

[atd] done.
```

No ANOS, no external daemon — everything runs from this repo.

## Install as a library

For Rust agents that want to speak ATD:

```bash
cargo add atd-sdk
```

For MCP clients (Claude Desktop, Cursor, Hermes, …) that want to reach
ATD tool servers:

```bash
cargo install atd-mcp-bridge
```

Then configure your MCP client to run the bridge — see
[`crates/atd-mcp-bridge/README.md`](crates/atd-mcp-bridge/README.md) for
examples.

## Architecture at a glance

```
┌──────────────┐  length-prefixed JSON  ┌────────────────────────────────────────┐
│   atd-sdk    │ ←───────────────────→  │ atd-ref-server                          │
│              │    (Unix socket)       │  Hello → capability gate                │
│              │                        │  registry → tier → binding → middleware │
└──────────────┘                        └────────────────────────────────────────┘

┌──────────────┐   MCP JSON-RPC    ┌────────────────┐      ┌──────────────┐
│  MCP client  │ ← stdio ────────→ │ atd-mcp-bridge │ ←──→ │  ATD server  │
│ (Claude      │                   │                │      │              │
│  Desktop,    │                   └────────────────┘      └──────────────┘
│  Cursor,     │
│  Hermes)     │
└──────────────┘
```

- The ATD wire protocol is length-prefixed JSON over a Unix socket —
  trivial to implement in any language.
- The reference server `atd-ref-server` ships with 9 native tools
  (`ref:echo.say`, `ref:fs.{read,write,edit,glob,grep}`,
  `ref:shell.{exec,pwsh}`, `ref:web.fetch`) plus `ref:external.uname`
  on unix (SP-12 `CliBinding` demo).
- The MCP bridge is a thin forwarder — ~200 lines — letting any MCP
  client reach an ATD server.

### Dispatch layer (SP-12)

`atd-ref-server` demonstrates four canonical dispatch primitives that
make the "ATD = agent-era POSIX" framing concrete in code:

- **Capability gate** — connection-scoped allow-list declared via
  `--grant-capability`; clients request a subset through `Hello`; tools
  whose `required_capabilities` are not granted are refused with error
  code `1001` (`AtdError::CapabilityDenied`).
- **Tier-aware deadlines** — `Hot`, `Warm`, `Cold` tiers on the tool
  definition drive per-call timeout + max-output budgets, overridable
  via `--tier-override hot=timeout_ms=300`.
- **Binding abstraction** — `NativeBinding` (delegates to the `Tool`
  impl, default) and `CliBinding` (spawns a subprocess, maps JSON args
  to argv, honors deadlines). Future bindings (MCP, REST,
  AppFunction) slot in through the same trait.
- **Result-middleware chain** — run on success before the wire reply;
  ships with `RedactPathsMiddleware` (redacts `$HOME` paths), enabled
  by default. Disable with `--middleware none`; compose chains with
  repeated `--middleware`.

The v3 distributed-dispatch features — device affinity, UCAN tokens,
session migrate/fork/handoff — remain Phase 2+. See
`docs/whitepaper/atd-v3-skills-architecture-brief.md` for the target.

## Validation

Two evidence docs prove the independence and cross-vendor claims:

- [`docs/validation/2026-04-23-sp6-capstone.md`](docs/validation/2026-04-23-sp6-capstone.md)
  — `hello_atd` runs with zero ANOS dependency; dep tree + license audit.
- [`docs/validation/2026-04-24-sp7-mcp-bridge.md`](docs/validation/2026-04-24-sp7-mcp-bridge.md)
  — MCP bridge end-to-end tests prove a non-ANOS MCP client can drive
  atd-ref-server through the bridge.

## Documentation

### Architecture

- [**Architecture (v1)**](docs/architecture.md) — canonical layer model (Schema · Dispatch · Security · Extensibility · adjacent Skills layer), per-layer status tables, component/crate map, non-goals, and evolution path. Start here for the full picture.

### Quick start guides

- [Rust](docs/quickstart/rust.md) — `cargo add atd-sdk`, first tool call, adapter usage
- [Python](docs/quickstart/python.md) — sync + async API, LangChain adapter, OpenAI/Anthropic helpers
- [TypeScript](docs/quickstart/typescript.md) — planned; stub with design preview

### Integration guides

- [**Overview**](docs/integrations/overview.md) — how mainstream agent systems (LangChain, Hermes, Claude Desktop, Cursor, OpenAI/Anthropic SDK users, MCP clients, etc.) integrate with ATD; decision matrix + compatibility table
- [LangChain](docs/integrations/langchain.md) — wire ATD tools into a LangChain agent (AgentExecutor + StructuredTool)
- [Hermes Agent](docs/integrations/hermes.md) — `hermes mcp add atd` + LLM-driven chat (verbatim SP-7 transcripts)
- [Claude Desktop / Claude Code / Cursor](docs/integrations/claude-code.md) — MCP bridge configuration for all three clients
- [OpenClaw](docs/integrations/openclaw.md) — current MCP-bridge workaround + future skill plan

### Protocol reference

- [Wire format](docs/protocol/wire-format.md) — length-prefixed JSON framing, message types, server bindings, full type definitions
- [Error codes](docs/protocol/error-codes.md) — `AtdError` taxonomy, server error codes, retry strategy

### Protocol schema

The wire types are mirrored as a JSON Schema artifact at the repo
root: [`atd-protocol-schema.json`](./atd-protocol-schema.json). Regenerate
after editing types in `crates/atd-protocol/`:

```bash
cargo run -p atd-protocol --features schema --bin gen-schema
```

CI verifies the committed file is fresh and meta-schema-valid via:

```bash
cargo run -p atd-protocol --features schema --bin gen-schema -- --check
```

### Known gaps + issues

- [docs/issues/](docs/issues/) — honest gap tracking: 10 open items across schema, dispatch, and security layers. Each issue explains current state, impact, and proposed fix or deferral rationale.

## Project status

This is v0.1.0. Under the SemVer 0.x contract, breaking changes are
allowed until 1.0 — API stability is a Phase 2 concern. The scope is
MVP. The design trail lives in
[`docs/superpowers/specs/`](docs/superpowers/specs/) and
[`docs/superpowers/plans/`](docs/superpowers/plans/); readers curious
about trade-offs will find them there.

## License

Apache-2.0. See [LICENSE](LICENSE).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Issues, PRs, and design feedback
welcome.
