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

### Capability-gated tools

The reference server enforces a connection-scoped capability gate: tools can
declare `required_capabilities` which the server checks before dispatch. The
MCP bridge does **not** issue an ATD `Hello` handshake, so every call it
proxies runs with an empty capability set. This is fine for the default ATD
reference tools (all declare `required_capabilities: []`), but any tool you
install that requires capabilities is refused with code `1001`
(`CAPABILITY_DENIED`) when called through the bridge.

To call capability-gated tools, use the Rust or Python ATD client directly
(both expose `hello()` on the client surface). Propagating capabilities through
the MCP bridge is tracked as a future enhancement.

## See also

- [`atd-protocol`](https://crates.io/crates/atd-protocol) — protocol types
- [`atd-sdk`](https://crates.io/crates/atd-sdk) — Rust client SDK
- [`docs/integrations/`](https://github.com/downsea/atd/tree/master/docs/integrations)
  — per-framework wiring guides

## License

Apache-2.0. See [LICENSE](https://github.com/downsea/atd/blob/master/LICENSE).
</content>
