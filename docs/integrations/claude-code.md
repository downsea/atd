# Claude Desktop / Claude Code / Cursor Integration — ATD via MCP Bridge

**Environment:** Linux or macOS, `atd-mcp-bridge` binary from source. Works with any MCP-speaking desktop client.

---

## What this covers

This guide shows you how to expose ATD tools inside Claude Desktop, Claude Code, and Cursor using the `atd-mcp-bridge` binary. All three clients speak the Model Context Protocol (MCP) and accept the same JSON configuration structure — they differ only in where the config file lives.

By the end you will have:

- ATD tools appearing in your desktop AI client's tool panel
- The reference server running persistently (via launchd on macOS or systemd on Linux)
- A multi-server setup for separating dev and prod tool sets

The integration path is: `Client UI → atd-mcp-bridge (stdio) → Unix socket → atd-ref-server`. The client spawns the bridge as a child process, the bridge connects to the ATD server over the Unix socket, and tool calls flow through transparently.

---

## Prerequisites

**ATD binaries built from source:**

```bash
git clone https://github.com/atd-protocol/atd-mvp
cd atd-mvp
cargo build --release -p atd-ref-server-bin -p atd-mcp-bridge
```

The binaries end up at `target/release/atd-ref-server` and `target/release/atd-mcp-bridge`. Note the **absolute paths** — you will paste these into the config files below.

**A running ATD server:**

The client-side config spawns the bridge on demand, but the bridge requires a running ATD server at the configured socket path. Start the server before opening the client (see "Starting the ATD server" below).

**One of the supported clients installed:**

- Claude Desktop (macOS or Linux)
- Claude Code CLI
- Cursor

---

## Per-client configs

All three clients use the same `mcpServers` JSON structure. Only the config file path differs.

| Client | Platform | Config file path |
|--------|----------|-----------------|
| Claude Desktop | macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Claude Desktop | Linux | `~/.config/Claude/claude_desktop_config.json` |
| Claude Code | Linux / macOS | `~/.config/claude-code/mcp.json` (consult Claude Code docs for the current path — it may vary by version) |
| Cursor | Linux / macOS | `~/.cursor/mcp.json` (consult Cursor docs for the current path — it may vary by OS and installation method) |

> **Path accuracy note:** Claude Code and Cursor update their config paths across major versions. If the path above doesn't match your installation, check the client's official documentation or look for an MCP settings panel in the client's UI. The JSON structure shown below is the same regardless of path.

**JSON config snippet (paste into whichever config file applies to your client):**

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

Replace `/abs/path/to/atd-mvp` with the absolute path to your clone. Replace `/tmp/my-atd.sock` with the socket path where your ATD server is listening.

**Alternative: use `args` instead of `env`:**

```json
{
  "mcpServers": {
    "atd": {
      "command": "/abs/path/to/atd-mvp/target/release/atd-mcp-bridge",
      "args": ["--sock", "/tmp/my-atd.sock"]
    }
  }
}
```

Either form works. Use `env` if the client's UI exposes environment variables but not command-line arguments (some hosted clients have this restriction).

**If the config file already exists,** merge the `"atd"` key into the existing `"mcpServers"` object — do not replace the whole file.

After saving the config, restart the client (or reload MCP settings from the client's UI). The ATD tools will appear in the tool panel on next connection.

---

## Starting the ATD server

The client config spawns the bridge automatically, but the bridge requires a running ATD server. The server must be started separately and kept alive.

### Manual launch (for development and testing)

```bash
/abs/path/to/atd-mvp/target/release/atd-ref-server --sock /tmp/my-atd.sock
```

Leave this terminal open. The server exits when you close it.

### Persistent launch with systemd (Linux)

Create `/etc/systemd/user/atd-ref-server.service`:

```ini
[Unit]
Description=ATD Reference Server
After=default.target

[Service]
Type=simple
ExecStart=/abs/path/to/atd-mvp/target/release/atd-ref-server --sock /tmp/my-atd.sock
Restart=on-failure
RestartSec=3s
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
```

Enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable atd-ref-server
systemctl --user start atd-ref-server
systemctl --user status atd-ref-server
```

Logs:

```bash
journalctl --user -u atd-ref-server -f
```

### Persistent launch with launchd (macOS)

Create `~/Library/LaunchAgents/com.atd-protocol.ref-server.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.atd-protocol.ref-server</string>
  <key>ProgramArguments</key>
  <array>
    <string>/abs/path/to/atd-mvp/target/release/atd-ref-server</string>
    <string>--sock</string>
    <string>/tmp/my-atd.sock</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/tmp/atd-ref-server.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/atd-ref-server.err</string>
</dict>
</plist>
```

Load and start:

```bash
launchctl load ~/Library/LaunchAgents/com.atd-protocol.ref-server.plist
launchctl start com.atd-protocol.ref-server
```

Check status:

```bash
launchctl list | grep atd-protocol
```

Logs:

```bash
tail -f /tmp/atd-ref-server.log
tail -f /tmp/atd-ref-server.err
```

---

## Example session

The following is an illustrative text transcript of what you see in Claude Desktop after configuration. This is representative of expected behavior, not a screenshot.

**In Claude Desktop's chat UI, after ATD tools are registered:**

> **User:** List all the Rust source files in /home/nan/proj/atd-mvp/crates/atd-sdk/src using the ATD file glob tool.
>
> **Claude:** I'll use the ATD file glob tool to find the Rust source files.
>
> _(Claude invokes `ref_fs_glob` with `{"pattern": "**/*.rs", "root": "/home/nan/proj/atd-mvp/crates/atd-sdk/src"}`)_
>
> _(Tool returns a list of `.rs` files)_
>
> **Claude:** I found the following Rust source files in that directory:
> - `lib.rs`
> - `client.rs`
> - `wire.rs`
> - `sanitize.rs`
> - `errors.rs`
> - `types.rs`

The tool invocation appears in the "Tool calls" panel in Claude Desktop's UI (the exact UI element name may vary by client version). You will see the sanitized tool name (`ref_fs_glob`) and the arguments the model passed.

**To verify ATD tools are visible to the client**, open the tool panel or MCP settings in the client's UI. The `atd` server should appear with 9 tools listed (when using `atd-ref-server` with all default tools enabled).

---

## Multi-server setup

You can register multiple ATD servers under different MCP server names. This is useful for separating development and production tool sets, or for pointing different clients at different ATD server instances.

**Example: dev + prod separation:**

```json
{
  "mcpServers": {
    "atd-dev": {
      "command": "/abs/path/to/atd-mcp-bridge",
      "env": { "ATD_SOCK": "/tmp/atd-dev.sock" }
    },
    "atd-prod": {
      "command": "/abs/path/to/atd-mcp-bridge",
      "env": { "ATD_SOCK": "/tmp/atd-prod.sock" }
    }
  }
}
```

Each entry spawns a separate bridge process. The client sees tools from both servers in a single combined tool list. If both servers expose the same tool IDs, the MCP-level tool names will collide. To prevent this, use different namespaces on different ATD servers (e.g., `dev:fs.read` vs `prod:fs.read`), which sanitize to different names (`dev_fs_read` vs `prod_fs_read`).

**Switching the socket path via environment variable:**

If you want a single config entry that can be redirected at runtime:

```json
{
  "mcpServers": {
    "atd": {
      "command": "/abs/path/to/atd-mcp-bridge",
      "env": { "ATD_SOCK": "${ATD_SOCK}" }
    }
  }
}
```

Then set `ATD_SOCK` in the shell where you launch the client. Some clients inherit the shell environment; others do not (see your client's docs). If environment variable expansion doesn't work, use explicit socket paths instead.

---

## Troubleshooting

**The `atd` server doesn't appear in the tool panel**

1. Verify the config file path is correct for your client (see the table above).
2. Verify the JSON is valid — a single missing comma or brace will cause the client to silently ignore the config. Use a JSON linter:
   ```bash
   python3 -m json.tool ~/.config/Claude/claude_desktop_config.json
   ```
3. Restart the client after editing the config.

**Tools appear but every call fails**

The bridge process started but cannot connect to the ATD server. Verify the server is running at the configured socket path:

```bash
ls -la /tmp/my-atd.sock
# Should show: srwxrwxrwx ...
```

If the socket is missing, start the server. If it exists but calls still fail, check the bridge logs (see "Viewing bridge logs" below).

**Socket permission errors on macOS**

macOS may restrict socket access for apps in the app sandbox. If Claude Desktop fails to connect, check that the socket path is accessible from the user's home directory or `/tmp`:

```bash
# Check permissions
ls -la /tmp/my-atd.sock
stat /tmp/my-atd.sock

# Workaround: use a socket path under your home directory
/abs/path/to/atd-ref-server --sock ~/atd.sock
```

Update the config to match the new socket path.

**Sanitized tool names confuse debugging**

Claude's UI shows `ref_shell_exec` (the MCP-sanitized name). The ATD server logs show `ref:shell.exec` (the original ATD ID). These are the same tool. The bridge handles the translation. When cross-referencing logs, apply the rule manually: underscores in the MCP name map to either `:` or `.` in the ATD ID.

**Viewing bridge logs**

The bridge writes diagnostic output to stderr. Most clients capture stderr from child processes (the bridge is a child process). Check your client's log output:

- **Claude Desktop (macOS):** `~/Library/Logs/Claude/mcp-server-atd.log` (path may vary by version)
- **Claude Desktop (Linux):** `~/.config/Claude/logs/mcp-server-atd.log` (check with `find ~/.config/Claude -name "*.log"`)
- **Cursor:** check the developer tools panel (Help → Toggle Developer Tools) for MCP process output
- **Claude Code:** run `claude-code --debug` to see MCP subprocess output in the terminal

If no log file is created, run the bridge manually to see its stderr directly:

```bash
ATD_SOCK=/tmp/my-atd.sock \
  /abs/path/to/atd-mcp-bridge 2>&1 | head -20
# Should print startup messages and then wait for stdin
```

**stdio vs HTTP confusion**

The bridge speaks MCP over stdio (stdin/stdout), not HTTP. Clients that support only HTTP-based MCP servers cannot use `atd-mcp-bridge`. All three clients covered in this guide (Claude Desktop, Claude Code, Cursor) support stdio-based MCP servers. HTTP transport is planned for ATD Phase 2 but is not yet available.

---

## See also

- [`docs/integrations/hermes.md`](hermes.md) — command-line agent with ATD tools (same bridge, CLI workflow)
- [`docs/integrations/langchain.md`](langchain.md) — Python SDK + LangChain (no MCP layer)
- [`crates/atd-mcp-bridge/README.md`](../../crates/atd-mcp-bridge/README.md) — bridge binary reference
- [`docs/protocol/wire-format.md`](../protocol/wire-format.md) — ATD wire protocol reference
- [Anthropic MCP documentation](https://docs.anthropic.com/en/docs/agents-and-tools/mcp) — canonical MCP client config reference
