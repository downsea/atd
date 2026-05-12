# atd-mcp-bridge

MCP-over-stdio bridge that lets any MCP-speaking client (Claude Desktop,
Cursor, Hermes, OpenAI Codex, …) drive tools served by an
[ATD (Agent Tool Dispatch) server](https://github.com/downsea/atd-mvp).

## Install

```bash
cargo install atd-mcp-bridge
```

## Usage

The bridge needs to point at a running ATD server (Unix socket). Two
ways to configure:

```bash
# 1. --sock flag
atd-mcp-bridge --sock /path/to/atd-server.sock

# 2. ATD_SOCK env var
ATD_SOCK=/path/to/atd-server.sock atd-mcp-bridge
```

The bridge reads MCP JSON-RPC 2.0 requests on stdin and writes responses
on stdout, as MCP spec requires.

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

Then run any ATD server at `/tmp/my-atd.sock` (e.g., `atd-ref-server`
from the [ATD repository](https://github.com/downsea/atd-mvp)) and
restart Claude Desktop. The ATD tools will appear in Claude's tool
list.

## What you need elsewhere

- A running ATD server. Build one from source:
  ```bash
  git clone https://github.com/downsea/atd-mvp
  cargo build --release -p atd-ref-server
  atd-ref-server --sock /tmp/my-atd.sock
  ```

## Limitations

### Capability-gated tools

SP-12 introduced a connection-scoped capability gate on
`atd-ref-server`: tools can declare `required_capabilities` which the
server enforces before dispatch. The MCP bridge does **not** issue an
ATD `Hello` handshake, so every call it proxies runs with an empty
capability set. This is fine for the default ATD reference tools
(all declare `required_capabilities: []`) but any tool you install
that requires capabilities will be refused with code `1001`
(`CAPABILITY_DENIED`) when called through the bridge.

If you need to call capability-gated tools, use the Rust or Python
ATD client directly (both expose `hello()` on the client surface).
Propagating capabilities through the MCP bridge is a future-SP item.

## See also

- [`atd-protocol`](https://crates.io/crates/atd-protocol) — protocol types
- [`atd-sdk`](https://crates.io/crates/atd-sdk) — Rust client SDK

## License

Apache-2.0. See [LICENSE](https://github.com/downsea/atd-mvp/blob/master/LICENSE).
