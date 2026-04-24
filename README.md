# atd-mvp

[![CI](https://github.com/downsea/atd-mvp/actions/workflows/ci.yml/badge.svg)](https://github.com/downsea/atd-mvp/actions/workflows/ci.yml)

**The reference implementation of the Agent Tool Dispatch (ATD) protocol.**

ATD is a neutral, cross-vendor wire protocol for letting any LLM agent
call any tool on any server. atd-mvp is the reference: a Rust client
SDK, a Rust reference server with 9 real tools, and an MCP bridge that
makes all of this work with Claude Desktop, Cursor, Hermes, and any
other MCP-speaking agent.

## Quick start

```bash
git clone https://github.com/downsea/atd-mvp
cd atd-mvp
cargo build --release -p atd-ref-server-bin
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
