# Hermes Agent Integration — ATD via MCP Bridge

**Environment:** Linux, Hermes Agent v0.9.0+, atd-ref-server + atd-mcp-bridge built from source. Validated 2026-04-23 against `sp7-mcp-bridge-validated` tag.

---

## What you'll set up

By the end of this guide you will have:

- Hermes Agent discovering and calling ATD tools through the MCP bridge
- An LLM of your choice (DeepSeek, Kimi, OpenAI, Anthropic, …) autonomously selecting and invoking ATD tools based on natural-language prompts
- Confidence from a real validation transcript: the exact Hermes + DeepSeek session that proved SP-7

The integration path is: `Hermes → atd-mcp-bridge (stdio) → Unix socket → atd-ref-server`. Hermes speaks MCP; the bridge translates to the ATD wire protocol; the ATD server executes the tool. No code changes to any of these components.

---

## Prerequisites

**Hermes Agent installed:**

```bash
hermes --version
# Expected: hermes 0.9.0 or newer
```

If Hermes is not installed, follow the [Hermes installation docs](https://github.com/hermesagent/hermes).

**ATD binaries built:**

```bash
cargo build --release -p atd-ref-server -p atd-mcp-bridge
```

Both binaries end up in `target/release/`. Verify:

```bash
./target/release/atd-ref-server --help
./target/release/atd-mcp-bridge --help
```

**An LLM provider configured in Hermes:**

```bash
hermes model list
# Shows available providers. At least one must be configured.
```

If no provider is configured, see the "Provider selection" section below.

---

## The 90-second setup

**Step 1 — Start the ATD reference server (keep it alive in a tmux pane or background job):**

```bash
# Option A: tmux (recommended — survives terminal close)
tmux new-session -d -s atd \
  "/home/nan/code/atd/target/release/atd-ref-server --sock /tmp/my-atd.sock"

# Option B: nohup background
nohup /home/nan/code/atd/target/release/atd-ref-server \
  --sock /tmp/my-atd.sock &>/tmp/atd-ref-server.log &
echo $!  > /tmp/atd-ref-server.pid

# Option C: foreground (leave this terminal open)
/home/nan/code/atd/target/release/atd-ref-server --sock /tmp/my-atd.sock
```

**Step 2 — Register the bridge with Hermes:**

```bash
hermes mcp add atd \
  --command /home/nan/code/atd/target/release/atd-mcp-bridge \
  --env ATD_SOCK=/tmp/my-atd.sock
```

`--env ATD_SOCK=...` passes the socket path into the bridge's environment. The bridge reads `ATD_SOCK` at startup and connects to the ATD server. This is the preferred form because some Hermes UIs expose `--env` but not `--args`.

Alternative using `--args` if your Hermes version supports it:

```bash
hermes mcp add atd \
  --command /home/nan/code/atd/target/release/atd-mcp-bridge \
  --args "--sock" --args "/tmp/my-atd.sock"
```

**Step 3 — Verify the registration:**

```bash
hermes mcp test atd
```

---

## Verification

`hermes mcp test atd` connects to the bridge, performs the MCP `initialize` handshake, and calls `tools/list`. Expected output:

```
Testing 'atd'...
Transport: stdio → /home/nan/code/atd/target/release/atd-mcp-bridge
Auth: none
✓ Connected (470ms)
✓ Tools discovered: 9

  ref_shell_exec    Run a command via `bash -c`. …
  ref_echo_say      Echoes input args back verbatim. …
  ref_fs_read       Read a UTF-8 text file with 1-indexed line numbers. …
  ref_shell_pwsh    Run a command via PowerShell. …
  ref_fs_glob       Find files matching a glob pattern. …
  ref_fs_grep       Regex search across files under a root. …
  ref_fs_edit       Exact-string find-and-replace in a UTF-8 file. …
  ref_web_fetch     HTTP GET a URL and return the body. …
  ref_fs_write      Atomically write text content to a file …
```

Note the tool name format: `ref_shell_exec` instead of `ref:shell.exec`. The bridge applies ATD-to-MCP name sanitization automatically (colons and dots → underscores). This is required by MCP's identifier grammar. Agents see the sanitized names; the bridge handles the reverse mapping transparently.

If you see 0 tools or a connection error, see the Troubleshooting section.

---

## First chat session

The following transcript is copied verbatim from the SP-7 validation run captured 2026-04-23. It shows Hermes Agent + DeepSeek `deepseek-chat` autonomously discovering and calling `ref_shell_exec` based on a natural-language prompt. No part of the tool selection or argument filling was pre-scripted.

### Transcript 1 — `ref_shell_exec` via LLM (verbatim from SP-7 §3.2)

**Prompt:**
> "Use the ATD shell tool (ref_shell_exec) to run 'uname -s' and report the exact stdout returned."

**Hermes output:**

```
╭─ ⚕ Hermes ──────────────────────────────────────────────────────────────╮
    I'll use the ATD shell tool to run the command for you.
╰─────────────────────────────────────────────────────────────────────────╯
  📞 Tool 1: mcp_atd_ref_shell_exec([])
     Args: {}

╭─ ⚕ Hermes ──────────────────────────────────────────────────────────────╮
  ✅ Tool 1 completed in 0.00s
     Result: {"error": "atd-client error: tool execution failed:
     ref:shell.exec: invalid args for ref:shell.exec: missing field
     `command` (retryable=false)"}
    I need to provide the command parameter. Let me run the command properly:
╰─────────────────────────────────────────────────────────────────────────╯
  📞 Tool 1: mcp_atd_ref_shell_exec(['command'])
     Args: {
       "command": "uname -s"
     }

╭─ ⚕ Hermes ──────────────────────────────────────────────────────────────╮
  ✅ Tool 1 completed in 0.00s
     Result: {"result": "{\"duration_ms\":2,\"exit_code\":0,\"stderr\":\"\",
     \"stderr_truncated\":false,\"stdout\":\"Linux\\n\",
     \"stdout_truncated\":false}"}
    The exact stdout returned is: `Linux`

    The command `uname -s` executed successfully with exit code 0, and the
    output shows the system is running Linux.
╰─────────────────────────────────────────────────────────────────────────╯

Session:   20260423_113932_8325a0
Duration:  38s
Messages:  6 (1 user, 4 tool calls)
Model:     deepseek-chat
```

**What this demonstrates:**

- The LLM initially called `ref_shell_exec` with no arguments (`{}`). The ATD error (`missing field 'command'`, `retryable=false`) came back through the bridge faithfully — the bridge is a pure forwarder, not a result rewriter.
- The LLM self-corrected and called again with `{"command": "uname -s"}`. No human intervention.
- Total wall-clock time: 38s, dominated by LLM latency. The ATD tool calls themselves were under 2ms each.

### Transcript 2 — `ref_fs_glob` via LLM (verbatim from SP-7 §3.3)

**Prompt:**
> "Use the ATD file glob tool (ref_fs_glob) to find all TOML files under /home/nan/code/atd. Report how many files were found and the first three paths."

**Hermes output:**

```
╭─ ⚕ Hermes ──────────────────────────────────────────────────────────────╮
    I'll use the ATD file glob tool to find all TOML files in the atd
    project directory.
╰─────────────────────────────────────────────────────────────────────────╯
  📞 Tool 1: mcp_atd_ref_fs_glob(['pattern', 'root'])
     Args: {
       "pattern": "**/*.toml",
       "root": "/home/nan/code/atd"
     }

╭─ ⚕ Hermes ──────────────────────────────────────────────────────────────╮
  ✅ Tool 1 completed in 0.02s
     Result: {"result": "{\"duration_ms\":16,\"paths\":[\"Cargo.toml\",
     \"crates/atd-cli/Cargo.toml\",\"crates/atd-client/Cargo.toml\",
     \"crates/atd-mcp-bridge/Cargo.toml\",\"crates/atd-ref-server/Cargo.toml\",
     \"crates/atd-types/Cargo.toml\",\"examples/Cargo.toml\",
     \"python/pyproject.toml\",\"rust-toolchain.toml\"],
     \"root\":\"/home/nan/code/atd\",\"truncated\":false}"}
    Found 9 TOML files under /home/nan/code/atd:

    1. Cargo.toml
    2. crates/atd-cli/Cargo.toml
    3. crates/atd-client/Cargo.toml
    4. crates/atd-mcp-bridge/Cargo.toml
    5. crates/atd-ref-server/Cargo.toml
    6. crates/atd-types/Cargo.toml
    7. examples/Cargo.toml
    8. python/pyproject.toml
    9. rust-toolchain.toml

    The first three paths are:
    - Cargo.toml
    - crates/atd-cli/Cargo.toml
    - crates/atd-client/Cargo.toml
╰─────────────────────────────────────────────────────────────────────────╯

Session:   20260423_115300_ee7623
Duration:  22s
Messages:  4 (1 user, 2 tool calls)
Model:     deepseek-chat
```

---

## Provider selection

Hermes can drive any configured LLM provider. The SP-7 sessions used `deepseek-chat`. You can override the provider and model per invocation without changing the MCP registration.

**View configured providers and models:**

```bash
hermes model list
```

**Set a default model:**

```bash
hermes model set deepseek-chat
# or
hermes model set gpt-4o-mini
```

**Override per session:**

```bash
hermes chat --provider openai --model gpt-4o-mini \
  "Use ref_fs_glob to find all .rs files under /home/nan/code/atd"
```

**Override via config file (`~/.hermes/config.yaml`):**

```yaml
default_provider: deepseek
default_model: deepseek-chat
providers:
  deepseek:
    api_key: "<YOUR_API_KEY>"
  openai:
    api_key: "<YOUR_API_KEY>"
```

The ATD tools are available in any Hermes session regardless of provider — they are registered as an MCP server, not tied to a specific LLM.

**Note on rate limits:** During SP-7 validation, a session with `kimi-k2.5` hit a provider quota error. Hermes surfaced this as a generic LLM error, not an ATD error. If your sessions fail silently after the first few tool calls, check your provider's rate limit dashboard, not the ATD logs.

---

## Managing tool visibility in chat

By default, all tools discovered from the `atd` MCP server are available in every Hermes session. You can disable specific tools if you want a leaner context or if certain tools are not appropriate for a given use case.

**List registered MCP servers and their tool status:**

```bash
hermes mcp list
```

**Configure tool enablement:**

```bash
hermes mcp configure atd
```

This opens an interactive tool list where you can toggle individual tools on or off. Disabled tools are excluded from `tools/list` responses for that Hermes session.

**Temporary override via session flag (if supported by your Hermes version):**

```bash
hermes chat --mcp-tools ref_shell_exec,ref_fs_read \
  "Read the first 5 lines of /etc/os-release"
```

If you are building a pipeline that should only use file tools (not shell tools), filter at the Hermes level rather than the ATD level. The ATD server always serves all registered tools; Hermes controls which subset the LLM sees.

---

## Troubleshooting

**`hermes mcp test atd` returns "0 tools discovered"**

The bridge connected but `tools/list` returned empty. Most likely cause: the ATD server is not running or the socket path is wrong. Check:

```bash
ls -la /tmp/my-atd.sock
# Should show: srwxrwxrwx ... /tmp/my-atd.sock
```

If the socket file is missing, the server exited. Restart it and retry.

**`hermes mcp test atd` returns "failed to connect"**

The bridge process itself failed to start. Check the bridge binary path:

```bash
/home/nan/code/atd/target/release/atd-mcp-bridge --help
# Should print usage without error
```

If the binary is missing, build it:

```bash
cargo build --release -p atd-mcp-bridge
```

**ATD_SOCK not found — tools connect but fail on every call**

You registered the bridge without `--env ATD_SOCK=...` and the socket path is not in the environment at bridge launch time. Re-register:

```bash
hermes mcp remove atd
hermes mcp add atd \
  --command /home/nan/code/atd/target/release/atd-mcp-bridge \
  --env ATD_SOCK=/tmp/my-atd.sock
```

**Sanitized tool names confuse debugging**

Agent traces show `ref_shell_exec`; the ATD server logs show `ref:shell.exec`; the ATD CLI uses `ref:shell.exec`. These are the same tool. The sanitization rule is deterministic: replace `:` and `.` with `_`. You can reverse it manually or use `atd_client.adapters.desanitize_tool_name("ref_shell_exec")` in Python.

**LLM hits provider rate limits mid-session**

Rate-limit errors surface as LLM-layer errors in Hermes, not ATD errors. The ATD tool calls themselves are unaffected. Switch to a provider with a higher quota, or add delays between requests. The ATD tools are not the bottleneck.

**`ref_web_fetch` fails with "private address blocked"**

This is expected in sandboxed or restricted environments. The SSRF guard in `atd-ref-server` blocks requests to private IP ranges (RFC 1918, loopback, link-local). This is by design (SP-5 security validation). If you need to fetch internal URLs, you will need a custom ATD server build with the SSRF guard relaxed, which is outside the scope of this guide.

**The LLM keeps calling the wrong tool**

The MCP schema the bridge advertises comes from ATD `ToolSummary.description`. If the description is ambiguous, the LLM may mis-select. Use a more explicit prompt that names the tool, or filter the tool list to only tools relevant to your task (see "Managing tool visibility in chat" above).

---

## Scaling up

**Multiple ATD servers behind different socket paths:**

You can register multiple ATD servers as separate MCP servers in Hermes:

```bash
# Dev server
hermes mcp add atd-dev \
  --command /path/to/atd-mcp-bridge \
  --env ATD_SOCK=/tmp/atd-dev.sock

# Production server (different tool set)
hermes mcp add atd-prod \
  --command /path/to/atd-mcp-bridge \
  --env ATD_SOCK=/tmp/atd-prod.sock
```

Each registration spawns a separate bridge process. The LLM sees the tools from both under their respective sanitized names (e.g., `ref_shell_exec` from `atd-dev` and `prod_shell_exec` from `atd-prod`, if the servers use different namespaces).

**Mixing ATD tools with Hermes built-ins:**

ATD tools appear alongside any tools Hermes provides natively (web search, memory, code execution, etc.). The LLM selects from the combined pool. You can disable Hermes built-ins for specific sessions to force ATD-only tool use:

```bash
hermes chat --no-builtin-tools \
  "Use ATD to find all Python files under /home/nan/code/atd"
```

**Running multiple bridge processes for the same server:**

Each `hermes mcp add` registration spawns a fresh bridge process per session. Multiple simultaneous Hermes sessions each get their own bridge process, all connecting to the same ATD server socket. The ATD server handles concurrent connections; no special configuration is required.

---

## Pagination (SP-pagination-v1)

ATD supports tool-result pagination via `Request::RunToolContinue` + `Response.next_cursor`. The MCP spec (`2025-11-25`) does not standardize cursors on `tools/call`, so the bridge offers two modes for cursor-aware tools:

### Default mode — degrade with truncation notice

When the ATD server returns a paginated result (`next_cursor` present), the bridge:

1. Returns the first page's data as a `text` content block (verbatim, no truncation).
2. Appends a second `text` content block: a structured notice like *"this server has more data available but your MCP client does not support continuation. Ask the user if they want the next page, or call this tool again with narrower args."*
3. Omits the `nextCursor` field from the MCP envelope.

This is the safe default for any MCP client (Hermes default, Claude Desktop, Cursor) — the LLM sees partial data with a clear signal to ask the user or narrow the query.

### Passthrough mode — for cursor-aware MCP clients

Set `ATD_MCP_PASSTHROUGH_CURSOR=1` on the bridge process to surface `nextCursor` as a non-standard field in the MCP result envelope:

```jsonc
{
  "content": [{"type":"text","text":"[...page 1 rows...]"}],
  "isError": false,
  "nextCursor": "eyJhcmdzX2ZpbmdlcnByaW50IjogIi4uLiJ9...HMAC..."
}
```

The MCP client must understand the field and re-issue `tools/call` with `arguments.__cursor` set to fetch the next page:

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "celia_fhir_list_observations",
    "arguments": {"patient": "p1", "__cursor": "eyJhcmdz..."}
  }
}
```

Use this only with MCP clients patched to consume `nextCursor` (Hermes >= the version that lands cursor-pass-through; check `hermes mcp version`). If the MCP client doesn't understand the field it will ignore it and the user will see only the first page without a continuation prompt.

### Bridge config example

```bash
# Default mode — degrade with notice (most clients)
hermes mcp add --command atd-mcp-bridge --args "--sock /tmp/celia.sock" --name celia

# Passthrough mode — Hermes >= 1.x with cursor-aware MCP support
hermes mcp add --command atd-mcp-bridge \
  --args "--sock /tmp/celia.sock" \
  --env ATD_MCP_PASSTHROUGH_CURSOR=1 \
  --name celia
```

For native ATD SDK callers (not via MCP), use `AtdClient::call_all` for auto-loop, or `AtdClient::call_page` for per-page control. See [`docs/architecture.md`](../architecture.md) §11.5 and [`docs/protocol/wire-format.md`](../protocol/wire-format.md) §4.4.1.

---

## See also

- [`docs/integrations/claude-code.md`](claude-code.md) — Claude Desktop / Claude Code / Cursor MCP config (same bridge, different client)
- [`docs/integrations/langchain.md`](langchain.md) — Python SDK + LangChain agent (no MCP layer)
- [`docs/quickstart/rust.md`](../quickstart/rust.md) — Rust SDK for direct ATD access
- [`crates/atd-mcp-bridge/README.md`](../../crates/atd-mcp-bridge/README.md) — bridge binary reference
- [`docs/archive/validation/2026-04-24-sp7-mcp-bridge.md`](../archive/validation/2026-04-24-sp7-mcp-bridge.md) — full SP-7 validation transcript
