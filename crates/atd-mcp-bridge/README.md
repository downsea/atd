# atd-mcp-bridge

MCP-over-stdio bridge that lets any MCP-speaking client (Claude Desktop,
Cursor, Hermes, OpenAI Codex, …) drive tools served by an
[ATD (Agent Tool Dispatch) server](https://github.com/<YOUR_USERNAME>/atd-mvp).

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
from the `atd-mvp` repo) and restart Claude Desktop. The ATD tools will
appear in Claude's tool list.

## What you need elsewhere

- A running ATD server. Build one from source:
  ```bash
  git clone https://github.com/<YOUR_USERNAME>/atd-mvp
  cargo build --release -p atd-ref-server
  atd-ref-server --sock /tmp/my-atd.sock
  ```

## See also

- [`atd-types`](https://crates.io/crates/atd-types) — protocol types
- [`atd-client`](https://crates.io/crates/atd-client) — Rust client SDK

## License

Apache-2.0. See [LICENSE](https://github.com/<YOUR_USERNAME>/atd-mvp/blob/master/LICENSE).
