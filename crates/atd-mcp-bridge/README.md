# atd-mcp-bridge

MCP-over-stdio bridge that lets any MCP-speaking client (Claude Desktop,
Cursor, Hermes, OpenAI Codex, …) drive tools served by an
[ATD (Agent Tool Dispatch) server](https://github.com/downsea/atd).

It reads MCP JSON-RPC 2.0 requests on stdin, forwards them to an ATD server
over a Unix socket, and writes responses on stdout — the standard MCP stdio
contract. This is how MCP clients consume ATD tools without speaking ATD
natively.

## Install

```bash
cargo install atd-mcp-bridge
```

## Usage

The bridge needs to point at a running ATD server (Unix socket). Two ways to
configure it:

```bash
# 1. --sock flag
atd-mcp-bridge --sock /path/to/atd-server.sock

# 2. ATD_SOCK env var
ATD_SOCK=/path/to/atd-server.sock atd-mcp-bridge
```

## Example: Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "atd": {
      "command": "atd-mcp-bridge",
      "env": { "ATD_SOCK": "/tmp/my-atd.sock" }
    }
  }
}
```

Then run any ATD server at `/tmp/my-atd.sock` (e.g.
[`atd-ref-server`](https://crates.io/crates/atd-ref-server)) and restart Claude
Desktop. The ATD tools appear in Claude's tool list.

## What you need elsewhere

A running ATD server. Build one from source:

```bash
git clone https://github.com/downsea/atd
cargo build --release -p atd-ref-server
atd-ref-server --sock /tmp/my-atd.sock
```

## Limitations

The MCP bridge is a **lossy down-mapping**. MCP's wire format has no fields
for most of ATD's tool metadata, so the bridge drops or degrades it. Use the
bridge when your consumer doesn't need ATD's full surface (e.g. a phone-side
MCP-only client); use a **native ATD client** (Rust [`atd-sdk`] / Python
[`atd_client`], both expose `hello()` + the full `ToolDefinition`) when you do.

### What MCP clients lose

| ATD provides | Over the MCP bridge |
|---|---|
| `tier` (Hot/Warm/Cold) | dropped — client can't size its timeout |
| `safety.level` (Read/Write/Financial/Privacy/Physical/Destructive) | dropped — the LLM can't tell a read from a destructive op, so no risk-gated confirmation |
| `output_schema` | dropped — no pre-validation, no "you'll get shape X" hint |
| `dry_run` flag | dropped — MCP has no such field, so no preview for dangerous ops |
| `required_capabilities` | dropped — the bridge issues no ATD `Hello`, so every proxied call runs with an empty capability set |
| cursor pagination | truncated + annotated by default, unless `ATD_MCP_PASSTHROUGH_CURSOR=1` |
| `caller_id` (multi-tenant) | dropped — one stdio session, so the `TokenBroker` can't route per-caller secrets |

### Capability-gated tools (consequence of the above)

Because the bridge issues no `Hello`, every call runs with an empty capability
set. This is fine for the default ATD reference tools (all declare
`required_capabilities: []`), but any tool that requires capabilities is
refused with code `1001` (`CAPABILITY_DENIED`) through the bridge. Call such
tools via the native Rust/Python client (`hello()` negotiates capabilities).
Propagating capabilities / safety / tier through the MCP bridge is tracked as a
future enhancement.

[`atd-sdk`]: https://crates.io/crates/atd-sdk
[`atd_client`]: https://github.com/downsea/atd/tree/master/python

## See also

- [`atd-protocol`](https://crates.io/crates/atd-protocol) — protocol types
- [`atd-sdk`](https://crates.io/crates/atd-sdk) — Rust client SDK
- [`docs/integrations/`](https://github.com/downsea/atd/tree/master/docs/integrations)
  — per-framework wiring guides

## License

Apache-2.0. See [LICENSE](https://github.com/downsea/atd/blob/master/LICENSE).
</content>
