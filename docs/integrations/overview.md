# Integrating ATD with Mainstream Agent Systems

This is the entry point for the `docs/integrations/` directory. It maps
the landscape of agent frameworks and LLM clients to the five paths by
which they can consume ATD tools, tells you which path fits your
situation, then sends you to the framework-specific deep-dive.

If you already know which framework you're using, jump straight to its
dedicated guide:

- [LangChain (Python)](langchain.md)
- [Hermes Agent](hermes.md)
- [Claude Desktop / Claude Code / Cursor](claude-code.md)
- [OpenClaw](openclaw.md)

If you're publishing tools (writing your own ATD server) rather than consuming them, the [Huawei HMS HealthKit case study](healthkit.md) walks through one adopter's failure → fix arc and the architectural lessons that came out of it.

If you're evaluating ATD's cross-vendor composition story (one agent session, multiple ATD servers), the [cross-vendor pattern doc](cross-vendor-pattern.md) ships a runnable demo: healthkit + a mock weather server bridged into one agent platform.

If you're choosing between options or evaluating coverage, read on.

For readers who want the full architectural picture underneath these integration paths — the layer model, mechanisms, crate map, and non-goals — see [`../architecture.md`](../architecture.md).

---

## The five integration paths

Every agent system fits into one of five paths depending on the
protocol surface it speaks.

### Path 1 — Direct SDK (Rust or Python)

You import `atd-sdk` into your own agent code, call
`discover()` + `describe()` + `call()` directly, and feed the results
into whatever LLM SDK you're using.

**When it fits:**

- You're writing the agent loop yourself
- Your agent is Python or Rust
- You want first-class control over tool lifecycle, caching, and error
  handling

**Minimum viable example (Python + OpenAI):**

```python
from atd_client import AtdClient
from atd_client.adapters import as_openai_tools
import openai

async with await AtdClient.connect("/tmp/atd.sock") as atd:
    summaries = await atd.discover()
    tools = as_openai_tools(summaries)

response = openai.OpenAI().chat.completions.create(
    model="gpt-4o",
    messages=[{"role": "user", "content": "Read README.md"}],
    tools=tools,
)
# Parse response.choices[0].message.tool_calls → call atd.call(...) per tool
```

**Covered frameworks:** LangChain, LangGraph, crewAI, AutoGen,
LlamaIndex, custom Rust/Python agents. See
[`langchain.md`](langchain.md) for the LangChain-specific walkthrough.

**Status:** Shipped in SP-10. Rust and Python both have OpenAI,
Anthropic, and LangChain adapters.

---

### Path 2 — MCP bridge (generic)

You run `atd-mcp-bridge` as a subprocess of your MCP-speaking client.
The bridge translates MCP `tools/list` and `tools/call` to ATD wire
format and back. The client never knows ATD exists.

**When it fits:**

- Your client speaks Model Context Protocol (MCP) natively
- You don't control the client's source code (desktop app, commercial
  editor)
- You want tools to appear in your client's regular tool picker UI

**Minimum viable configuration (any MCP client):**

```json
{
  "mcpServers": {
    "atd": {
      "command": "/usr/local/bin/atd-mcp-bridge",
      "env": { "ATD_SOCK": "/tmp/atd.sock" }
    }
  }
}
```

**Covered clients:**

| Client | Status | Guide |
|---|---|---|
| **Hermes Agent** | End-to-end verified with live LLM (SP-7) | [`hermes.md`](hermes.md) |
| **Claude Desktop** | Configuration documented, manual testing | [`claude-code.md`](claude-code.md) |
| **Claude Code** | Same config shape as Claude Desktop | [`claude-code.md`](claude-code.md) |
| **Cursor** | Same config shape, different file path | [`claude-code.md`](claude-code.md) |
| **Continue.dev** | MCP-compatible, not explicitly tested | Protocol-compatible |
| **Cline** (VSCode) | MCP-compatible, not explicitly tested | Protocol-compatible |
| **Zed** | MCP-compatible as of recent versions | Protocol-compatible |
| **OpenAI Codex** (MCP-enabled) | MCP-compatible, not explicitly tested | Protocol-compatible |

**This is the highest-leverage path.** One `atd-mcp-bridge` binary
reaches every mature MCP-speaking client without any per-client
engineering.

**Status:** Shipped SP-7. Verified with Hermes + DeepSeek in a live
chat. Documentation covers the three Anthropic-adjacent clients
explicitly; any other MCP client follows the same config pattern.

---

### Path 3 — Raw OpenAI / Anthropic API users

You're calling the OpenAI or Anthropic SDK directly without a wrapper
framework, but you still want ATD tools.

**When it fits:**

- You have custom agent logic too specific for LangChain/LlamaIndex
- You want minimum dependencies
- You're targeting an OpenAI-compatible gateway (OpenRouter, Groq,
  Together, DeepSeek, self-hosted vLLM) — the tool format is identical

**Minimum viable example:**

```python
from atd_client import AtdClient
from atd_client.adapters import as_openai_tools, as_anthropic_tools

async with await AtdClient.connect("/tmp/atd.sock") as atd:
    summaries = await atd.discover()

openai_tools = as_openai_tools(summaries)        # dict shape for OpenAI
anthropic_tools = as_anthropic_tools(summaries)  # dict shape for Anthropic
```

Feed either list directly into the provider's SDK's `tools=` parameter.

**Covered:** OpenAI API, Anthropic Messages API, and every
OpenAI-compatible gateway.

**Status:** Shipped SP-10.

---

### Path 4 — Custom client in an unsupported language

You want to integrate from Go / Java / .NET / C# / other languages that
don't have an ATD SDK yet.

**When it fits:**

- Your agent is in a non-Python, non-Rust language
- You don't want to spawn `atd-mcp-bridge` as a subprocess

**What you do:** Implement a minimal ATD client against the wire
protocol in your language of choice. The protocol is simple:

- 4-byte big-endian `u32` length prefix
- UTF-8 JSON body
- Unix socket (or Windows named pipe)
- Three messages: `discover`, `describe`, `call`
- Reference: [`../protocol/wire-format.md`](../protocol/wire-format.md)

Both the Rust client (`crates/atd-sdk/`) and the Python client
(`python/src/atd_client/`) are small enough to be read end-to-end as
porting references.

**Alternative:** If your language has an MCP SDK (Node.js has
`@modelcontextprotocol/sdk`, Python has `mcp`), take Path 2 instead —
spawn `atd-mcp-bridge` from your code and speak MCP over stdio. Less
protocol work, same outcome.

**Status:** No SDK shipped. Path 4 is the current solution for
non-Rust, non-Python languages. TypeScript SDK is planned per
[`../quickstart/typescript.md`](../quickstart/typescript.md); no
timeline.

---

### Path 5 — SKILL.md platforms (planned)

Anthropic's Agent Skills spec is used by 26+ platforms (Claude Code
skills, OpenClaw ClawHub, VS Code Copilot, GitHub Copilot Chat,
Atlassian, Figma, etc.). A single `atd-dispatch` skill published once
would reach all of them without per-platform engineering.

**Status:** **NOT SHIPPED.** The `atd-dispatch` SKILL.md is designed
in `docs/design.md` §5.1 but hasn't been written or published.
[`openclaw.md`](openclaw.md) describes the current workaround (MCP
bridge, Path 2) and the future plan.

---

## Decision matrix

Pick based on what you're doing:

| Your situation | Path | Primary doc |
|---|---|---|
| Writing Python agent with LangChain | 1 | [`langchain.md`](langchain.md) |
| Writing Python agent with raw OpenAI SDK | 3 | [`../quickstart/python.md`](../quickstart/python.md) |
| Writing Python agent with raw Anthropic SDK | 3 | [`../quickstart/python.md`](../quickstart/python.md) |
| Writing Rust agent | 1 | [`../quickstart/rust.md`](../quickstart/rust.md) |
| Using Claude Desktop | 2 | [`claude-code.md`](claude-code.md) |
| Using Cursor | 2 | [`claude-code.md`](claude-code.md) |
| Using Claude Code | 2 | [`claude-code.md`](claude-code.md) |
| Using Hermes Agent | 2 | [`hermes.md`](hermes.md) |
| Using another MCP-speaking client | 2 | [`claude-code.md`](claude-code.md) (pattern transfers) |
| Writing TS / Go / Java agent | 4 (or 2) | [`../protocol/wire-format.md`](../protocol/wire-format.md) |
| Publishing a tool catalog to SKILL.md platforms | 5 | [`openclaw.md`](openclaw.md) (blocked; workaround via Path 2) |

---

## Framework compatibility table

Consolidated view of where each framework lands:

| Framework | Path | Shipped? | Verified? |
|---|---|---|---|
| LangChain (Python) | 1 | ✅ | Unit tests + doc example |
| LangGraph | 1 (via LangChain tools) | ✅ | Inherits from LangChain |
| crewAI | 1 (via LangChain tools) | ✅ | Inherits |
| AutoGen | 1 (`as_openai_tools`) | ✅ | API-compatible; not explicitly tested |
| LlamaIndex | 1 (`as_openai_tools`) | ✅ | API-compatible; not explicitly tested |
| OpenAI API direct | 3 | ✅ | Unit tests |
| Anthropic API direct | 3 | ✅ | Unit tests |
| OpenAI-compatible gateways (OpenRouter, Groq, DeepSeek, Together) | 3 | ✅ | DeepSeek verified live via Hermes (SP-7) |
| Hermes Agent | 2 | ✅ | End-to-end with live LLM |
| Claude Desktop | 2 | ✅ | Config doc; manual testing |
| Claude Code | 2 | ✅ | Config doc; inherits Claude Desktop shape |
| Cursor | 2 | ✅ | Config doc; inherits MCP shape |
| Continue.dev | 2 | ⚠️ | Protocol-compatible; not tested |
| Cline (VSCode) | 2 | ⚠️ | Protocol-compatible; not tested |
| Zed | 2 | ⚠️ | Protocol-compatible; not tested |
| OpenAI Codex (MCP variant) | 2 | ⚠️ | Protocol-compatible; not tested |
| Go / Java / .NET agents | 4 | ❌ | No SDK yet |
| TypeScript agents | 4 | ❌ | No SDK yet (planned) |
| OpenClaw | 5 → 2 interim | ❌ native | MCP bridge works as interim |
| Claude Code skills ecosystem | 5 | ❌ | No `atd-dispatch` skill published |

Legend: ✅ shipped and verified — ⚠️ protocol-compatible, not explicitly tested — ❌ not shipped

---

## When to use which path: additional guidance

Some judgment calls you'll hit repeatedly.

### "I'm using LangChain but don't want async"

Use Path 1 with `AtdClientSync` — a synchronous wrapper over the async
client, designed for exactly this case. See
[`../quickstart/python.md`](../quickstart/python.md) for the sync/async
split.

### "I need both MCP clients and my own code to share tools"

Run `atd-ref-server` (or your own ATD server) once. Point
`atd-mcp-bridge` at it for MCP clients. Point `AtdClient.connect()`
at the same socket for your own code. One server, multiple consumers.

### "I want to filter which tools an agent sees"

Both Path 1 and Path 2 support this.

- **Path 1 (SDK):** call `discover()` with a `DiscoverFilter` or
  post-filter the result before passing to adapters.
- **Path 2 (MCP):** in Hermes, use
  `hermes mcp configure atd` to toggle per-tool enablement. Claude
  Desktop / Cursor / Continue support similar per-server filtering in
  their UIs.

### "I'm building a commercial product and need tool-level access control"

ATD's capability system is Phase 2 (per `docs/design.md` §3.6). v0.1.0
has no authentication or token-scoped access — all tools on a socket
are exposed equally.

Interim: run separate ATD servers with different tool sets on
different sockets (dev-socket, prod-socket, readonly-socket). Each
consumer connects only to the sockets it's authorized to see.

### "I want to run ATD tools from a Jupyter notebook"

Path 1 with Python, using `AtdClientSync`. Example:

```python
from atd_client.sync import AtdClientSync

with AtdClientSync.connect("/tmp/atd.sock") as atd:
    tools = atd.discover()
    result = atd.call("ref:fs.read", {"path": "notebook.ipynb"})
```

### "My agent framework isn't in the compatibility table"

If it speaks OpenAI function-calling format (most do), use Path 3:
`as_openai_tools()` produces the dict shape OpenAI expects.

If it speaks MCP, use Path 2: point it at `atd-mcp-bridge`.

If it's an exotic protocol, use Path 4: read
[`../protocol/wire-format.md`](../protocol/wire-format.md) and write a
minimal client.

---

## What ATD does not integrate with

Honest gaps, for expectations management:

- **Cloud-only agent platforms (e.g., closed SaaS without local agent
  access)** — ATD's HTTP transport (`atd-server-http`, landed 2026-05-11
  via SP-streamable-http + SP-1.B) opens this surface. First cloud-hosted
  adopter is `celia_phr`. TLS termination + OAuth/OIDC remain adopter-side;
  ATD owns transport + bearer plumbing. See architecture §10 and
  `crates/atd-server-http/`.
- **Agent platforms that require Apache-2.0-incompatible licensing** —
  ATD is Apache-2.0. Dual-licensed integrations are possible but not
  shipped.

---

## Next steps by role

- **Agent framework author (Rust/Python):** read
  [`../quickstart/rust.md`](../quickstart/rust.md) or
  [`../quickstart/python.md`](../quickstart/python.md), then the
  relevant integration guide.

- **Tool-registry operator (running `atd-ref-server` or writing your
  own):** read [`../protocol/wire-format.md`](../protocol/wire-format.md)
  and [`../protocol/error-codes.md`](../protocol/error-codes.md).
  Conformance suite to validate your server against the protocol is
  planned (SP-8, not yet shipped).

- **Agent UI user (Claude Desktop / Cursor / Hermes):** read
  [`hermes.md`](hermes.md) or [`claude-code.md`](claude-code.md). The
  setup is 5 lines of JSON.

- **Porting to a new language:** read
  [`../protocol/wire-format.md`](../protocol/wire-format.md) and the
  Rust or Python client source.

- **Publishing your own tools:** run an ATD server exposing them.
  Start from `atd-ref-server` as a template
  (`crates/atd-ref-server/`) and link `atd-runtime` +
  one of the `crates/atd-tools-*` crates. Each tool is roughly one file.

---

## Related documents

- Per-framework deep-dives: [`langchain.md`](langchain.md),
  [`hermes.md`](hermes.md), [`claude-code.md`](claude-code.md),
  [`openclaw.md`](openclaw.md)
- Language quickstarts: [`../quickstart/rust.md`](../quickstart/rust.md),
  [`../quickstart/python.md`](../quickstart/python.md),
  [`../quickstart/typescript.md`](../quickstart/typescript.md)
- Protocol reference:
  [`../protocol/wire-format.md`](../protocol/wire-format.md),
  [`../protocol/error-codes.md`](../protocol/error-codes.md)
- Design rationale: [`../design.md`](../design.md)
- End-to-end validation:
  [`../validation/2026-04-23-sp6-capstone.md`](../validation/2026-04-23-sp6-capstone.md)
  (standalone),
  [`../validation/2026-04-24-sp7-mcp-bridge.md`](../validation/2026-04-24-sp7-mcp-bridge.md)
  (MCP + live LLM)
