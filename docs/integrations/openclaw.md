# OpenClaw Integration — ATD Client SDK

**Status as of v0.1.0:** No native ATD integration path exists for OpenClaw. This document explains the current workaround, the planned `atd-dispatch` skill, and how to contribute.

---

## Status

atd-mvp v0.1.0 does not ship a native OpenClaw integration. Specifically:

- No `atd-dispatch` skill is published to ClawHub.
- OpenClaw cannot discover or call ATD tools via a first-class mechanism.
- The planned `skills/atd-dispatch/SKILL.md` file exists as a design artifact in `docs/design.md` §5.1 but has not been implemented or published.

This is an honest stub. If you need OpenClaw + ATD today, use the MCP bridge workaround below.

---

## Current workaround — MCP bridge

If your version of OpenClaw supports MCP servers (check the OpenClaw documentation for your version), you can point OpenClaw's MCP config at `atd-mcp-bridge`. This is the same bridge used by Claude Desktop, Claude Code, and Cursor.

**Step 1 — Build the bridge:**

```bash
cargo build --release -p atd-ref-server-bin -p atd-mcp-bridge
```

**Step 2 — Start the ATD server:**

```bash
/abs/path/to/atd-mvp/target/release/atd-ref-server --sock /tmp/my-atd.sock
```

**Step 3 — Configure OpenClaw's MCP settings:**

The JSON structure is the standard MCP client config:

```json
{
  "mcpServers": {
    "atd": {
      "command": "/abs/path/to/atd-mvp/target/release/atd-mcp-bridge",
      "env": {
        "ATD_SOCK": "/tmp/my-atd.sock"
      }
    }
  }
}
```

Consult the OpenClaw documentation for the exact config file path on your platform. The JSON structure above is the same as Claude Desktop and Cursor (both are documented in [`docs/integrations/claude-code.md`](claude-code.md)).

**If OpenClaw does not support MCP**, the bridge path is not available. You would need to write a thin OpenClaw plugin that calls ATD directly over the Unix socket using the length-prefixed JSON wire protocol documented in [`docs/protocol/wire-format.md`](../protocol/wire-format.md).

---

## Planned: `atd-dispatch` skill on ClawHub

The long-term integration path is a SKILL.md-compatible skill published to ClawHub. Once published, OpenClaw users could install it with a single command and get access to all ATD tools without any manual configuration.

The design for this skill is specified in `docs/design.md` §5.1. The planned `SKILL.md` content is:

```yaml
---
name: atd-dispatch
description: |
  Dispatch tool calls to ATD-compatible servers. Unlocks cross-platform,
  cross-vendor tools (Xiaomi, HealthKit, HMS, Jira, etc.) in any
  SKILL.md-compatible agent.
version: 0.1.0
license: MIT
atd-tools:
  required: []
---

When the user needs a tool that isn't in the native toolset, check ATD:

1. `atd list --query "<domain>"` — discover candidates
2. `atd schema <tool_id>` — read input/output contract
3. `atd call <tool_id> --args '<json>'` — invoke

Every call returns JSON with `{ok, data, error, metadata}`. Pass `data` forward.
```

This skill wraps the `atd` CLI binary. The user or administrator installs `atd` (from `cargo install atd-cli`), configures the socket path, and the skill handles the rest. The LLM receives the dispatch instructions in its context and can discover and invoke ATD tools using `atd list`, `atd schema`, and `atd call`.

**What you can do today (without ClawHub publication):**

1. Clone the repo: `git clone https://github.com/atd-protocol/atd-mvp`
2. Build the `atd` CLI: `cargo build --release -p atd-cli`
3. Start the server: `./target/release/atd-ref-server --sock /tmp/my-atd.sock`
4. Create the SKILL.md file locally at `skills/atd-dispatch/SKILL.md` using the content above
5. Install it in your local OpenClaw instance per OpenClaw's local skill installation instructions
6. Test with: `atd list`, `atd schema ref:echo.say`, `atd call ref:echo.say --args '{"text":"hello"}'`

The CLI workflow proves the dispatch loop works. The ClawHub publication step is what makes it available to other OpenClaw users without a manual install.

---

## Contributing

If you want to accelerate OpenClaw support, the most valuable contributions are:

**1. Implement and publish the `atd-dispatch` skill**

- Write `skills/atd-dispatch/SKILL.md` based on the design above
- Test it locally with OpenClaw (or any SKILL.md-compatible platform)
- Submit a PR to this repository with the skill file
- Follow ClawHub's publication process to publish it to the skill registry

**2. Write an OpenClaw MCP compatibility report**

OpenClaw's MCP support may differ from Claude Desktop and Cursor in config file location or supported features. Documenting the exact steps for your OpenClaw version helps other users. Submit findings as a PR that updates this document's "Current workaround" section.

**3. Upstream OpenClaw PR**

If OpenClaw does not support MCP natively, a PR to the OpenClaw project that adds `atd-mcp-bridge` as a first-class integration path would benefit the whole community. The bridge binary is a standalone Rust executable with no runtime dependencies beyond the OS.

To get started, open an issue at [github.com/atd-protocol/atd-mvp](https://github.com/atd-protocol/atd-mvp) describing what you're working on. The maintainers can point you at the relevant design decisions and review your approach before you write code.

---

## See also

- [`docs/integrations/claude-code.md`](claude-code.md) — MCP bridge config for clients that already support MCP
- [`docs/integrations/hermes.md`](hermes.md) — Hermes Agent integration (same bridge, different client)
- [`docs/protocol/wire-format.md`](../protocol/wire-format.md) — ATD wire protocol (for building a direct integration without the MCP bridge)
- [`docs/design.md`](../design.md) §5.1 — full design for the `atd-dispatch` skill
