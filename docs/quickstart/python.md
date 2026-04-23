# Python Quickstart — ATD Client SDK

**Environment:** Linux, Python 3.10+, `uv` or `pip`. Tested on the `sp10-adapters` tag (252 workspace tests green).

---

## What this doc covers

By the end of this guide you will have:

- Installed `atd-client` from the local source tree (PyPI publication is future work)
- Connected to an ATD server using the async `AtdClient`
- Called `discover`, `describe`, and `call` against the `atd-ref-server` reference implementation
- Used the synchronous `AtdClientSync` wrapper for environments that cannot use `async`/`await`
- Handled `ToolSuccess` / `ToolFailure` results and the `AtdError` exception hierarchy
- Exported tool definitions to LangChain, OpenAI, and Anthropic function-calling formats

For the Rust SDK, see [`docs/quickstart/rust.md`](rust.md).  
For the raw wire protocol, see [`docs/protocol/wire-format.md`](../protocol/wire-format.md).

---

## Install

`atd-client` is not yet published to PyPI. Install it from the local source tree.

**Using `uv` (recommended):**

```bash
# From the root of the atd-mvp repo:
uv pip install -e python/

# With LangChain adapter support:
uv pip install -e 'python/[langchain]'
```

**Using `pip`:**

```bash
pip install -e /path/to/atd-mvp/python/

# With LangChain adapter:
pip install -e '/path/to/atd-mvp/python/[langchain]'
```

Replace `/path/to/atd-mvp` with the absolute path where you cloned the repository.

The package requires Python 3.10+ and `pydantic>=2`. The `langchain` extra additionally requires `langchain-core>=0.3`.

> **Future path:** Once published, you will use `pip install atd-client` or
> `pip install 'atd-client[langchain]'`. The public API will not change.

**Verify the install:**

```python
import atd_client
print(atd_client.__version__)  # 0.1.0
```

---

## Hello ATD (async)

The following script connects to the ref-server, discovers tools, and calls `ref:echo.say`.  
The ref-server must be running before you execute this script (see "Running against atd-ref-server" below).

```python
import asyncio
from atd_client import AtdClient, ToolSuccess, ToolFailure

async def main() -> None:
    # Connect to the ATD server over a Unix socket.
    # Replace <YOUR_SOCKET_PATH> with the actual socket path.
    async with await AtdClient.connect("<YOUR_SOCKET_PATH>") as client:
        # List all available tools.
        tools = await client.discover()
        print(f"connected — {len(tools)} tools available")

        # Call a tool.
        result = await client.call(
            "ref:echo.say",
            {"text": "hello from ATD"},
        )

        if isinstance(result, ToolSuccess):
            print(f"success: {result.data}")
        elif isinstance(result, ToolFailure):
            print(f"tool error [{result.code}]: {result.message}")

asyncio.run(main())
```

`AtdClient.connect` accepts a `str`, `pathlib.Path`, or `None`.  
Passing `None` uses the default socket path configured by the ATD server daemon.

`AtdClient` implements the async context manager protocol (`async with await AtdClient.connect(...)`). Use `await client.close()` directly if you manage the lifetime yourself.

**Run the in-repo example** (auto-spawns and tears down the ref-server):

```bash
cargo build --release -p atd-ref-server
cd /path/to/atd-mvp
uv run python python/examples/hello_atd.py
```

Expected output:

```
[atd] auto-spawning atd-ref-server → /tmp/.../demo.sock
[atd] connected
[atd] 3 tools registered

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"echo": "hello from ATD"}

[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → 5 paths: Cargo.toml, crates/atd-client/Cargo.toml, ... (+2 more)

[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout='Linux'

[atd] done.
```

---

## Hello ATD (sync wrapper)

`AtdClientSync` is a synchronous façade around `AtdClient`. It runs an event loop on a dedicated background thread, so you can use it in Jupyter notebooks, LangChain tool loaders, CLI scripts, or any other context that cannot use `async`/`await`.

```python
from atd_client import AtdClientSync, ToolSuccess, ToolFailure

# Replace <YOUR_SOCKET_PATH> with the actual socket path.
with AtdClientSync.connect("<YOUR_SOCKET_PATH>") as client:
    tools = client.discover()
    print(f"connected — {len(tools)} tools available")

    result = client.call("ref:echo.say", {"text": "hello sync"})

    if isinstance(result, ToolSuccess):
        print(f"success: {result.data}")
    elif isinstance(result, ToolFailure):
        print(f"tool error [{result.code}]: {result.message}")
```

`AtdClientSync` mirrors `AtdClient` exactly: same `discover`, `describe`, `call` signatures, same return types, same exception types. The only differences are:

- Methods are synchronous (no `await`)
- The constructor is `AtdClientSync.connect(...)` (no `await`)
- `close()` is synchronous and stops the background loop

`AtdClientSync` is not thread-safe for concurrent calls. If multiple threads need to call tools simultaneously, create one `AtdClientSync` per thread.

---

## Discover, describe, call

### discover

```python
async def discover(
    self,
    query: str | None = None,
    *,
    domain: str | None = None,
    tier: ToolTier | None = None,
    visibility: ToolVisibility | None = None,
    limit: int | None = None,
) -> list[ToolSummary]
```

Returns a list of `ToolSummary` objects. Filtering is applied client-side after fetching the full list from the server.

**Examples:**

```python
from atd_client import AtdClient, ToolVisibility

async with await AtdClient.connect("<YOUR_SOCKET_PATH>") as client:
    # No filter — get everything.
    all_tools = await client.discover()

    # Text search across id, name, description.
    fs_tools = await client.discover(query="fs")

    # Domain filter.
    web_tools = await client.discover(domain="web")

    # Visibility filter.
    safe_tools = await client.discover(visibility=ToolVisibility.read)

    # Limit results.
    first_five = await client.discover(limit=5)

    # Combine filters.
    top_fs = await client.discover(query="fs", domain="fs", limit=3)
```

**`ToolSummary` key fields:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | `str` | Canonical tool id: `<namespace>:<domain>.<action>` |
| `name` | `str` | Human-readable display name |
| `description` | `str` | One-line purpose |
| `domain` | `str` | Derived from `id` when the server omits it |
| `tier` | `ToolTier` | `hot` / `warm` / `cold` |
| `visibility` | `ToolVisibility` | `read` / `write` / `dangerous` / `internal` |
| `tags` | `list[str]` | Freeform labels |
| `input_schema` | `dict | None` | JSON Schema when populated by the server |

The `id` field uses the format `<namespace>:<domain>.<action>`, e.g. `ref:echo.say`. LLM adapters sanitize this to `ref_echo_say` for provider APIs that require alphanumeric-plus-underscore names.

### describe

```python
async def describe(self, tool_id: str) -> ToolDefinition
```

Fetches the full `ToolDefinition` for one tool. This includes input/output JSON Schemas, safety metadata, binding configuration, and trust information — detail that `ToolSummary` omits.

```python
from atd_client import AtdClient, ToolNotFound

async with await AtdClient.connect("<YOUR_SOCKET_PATH>") as client:
    try:
        defn = await client.describe("ref:echo.say")
        print(f"version:      {defn.version}")
        print(f"domain:       {defn.capability.domain}")
        print(f"safety level: {defn.safety.level}")
        print(f"input schema: {defn.input_schema}")
    except ToolNotFound as exc:
        print(f"tool not found: {exc.tool_id}")
        if exc.suggestions:
            print(f"did you mean: {exc.suggestions[0]}")
```

`ToolDefinition` is a Pydantic v2 model. All fields are validated on construction from the server response.

### call

```python
async def call(
    self,
    tool_id: str,
    args: Any = None,
    *,
    dry_run: bool = False,
) -> ToolSuccess | ToolFailure
```

Executes a tool. `args` is a dict (or any JSON-serializable value) matching the tool's `input_schema`. The result is always `ToolSuccess` or `ToolFailure` — exceptions from `call` indicate transport or protocol failures, not tool-reported errors.

**`ToolSuccess` fields:**

| Field | Type | Description |
|-------|------|-------------|
| `data` | `Any` | Tool output, JSON-decoded |
| `metadata` | `ToolResultMetadata` | Contains `tool_id`, other fields when populated by server |

**`ToolFailure` fields:**

| Field | Type | Description |
|-------|------|-------------|
| `code` | `str` | Short error code, e.g. `"EPERM"`, `"TIMEOUT"` |
| `message` | `str` | Human-readable description |
| `reason` | `str | None` | Raw server payload as JSON string (for opaque payloads) |
| `retryable` | `bool` | Whether the call is safe to retry |

**Pattern-matching the result:**

```python
from atd_client import AtdClient, ToolSuccess, ToolFailure

async with await AtdClient.connect("<YOUR_SOCKET_PATH>") as client:
    result = await client.call(
        "ref:fs.glob",
        {"pattern": "**/*.toml", "path": "."},
    )

    if isinstance(result, ToolSuccess):
        paths = result.data.get("paths", [])
        print(f"found {len(paths)} files")
    elif isinstance(result, ToolFailure):
        print(f"[{result.code}] {result.message}")
        if result.retryable:
            print("this call can be retried")
```

**Dry-run mode:**

```python
# Validate args without executing the tool.
result = await client.call(
    "ref:shell.exec",
    {"command": "rm -rf /"},
    dry_run=True,
)
```

---

## Error handling

The Python SDK distinguishes between two error layers:

1. **`ToolSuccess` / `ToolFailure`** — returned from `call()` as normal values. These represent the tool's own success/failure, not a client error. Always check `isinstance(result, ToolFailure)`.

2. **`AtdError` exception hierarchy** — raised from any SDK method when the transport or protocol fails. These are exceptions, not return values.

### Exception hierarchy

All exceptions live in `atd_client.errors` and are re-exported from `atd_client`:

```python
from atd_client import (
    AtdError,          # base class
    ToolNotFound,      # client.describe() or client.call() for unknown id
    InvalidArguments,  # args fail local validation before the call
    CapabilityDenied,  # server denied the capability
    BindingUnavailable,# no usable binding exists for the tool
    ToolExecutionFailed, # server attempted execution, failed at OS/network level
    Timeout,           # server did not respond within deadline
    ServerUnreachable, # socket connect failed or connection dropped
    NotImplementedFeature, # server does not support the requested capability
    ProtocolError,     # response shape does not match expected message type
)
```

### Retryable errors and robust callers

```python
import asyncio
from atd_client import AtdClient, AtdError, ToolSuccess, ToolFailure

async def robust_call(
    client: AtdClient,
    tool_id: str,
    args: dict,
    max_attempts: int = 3,
) -> ToolSuccess | ToolFailure:
    for attempt in range(max_attempts):
        try:
            return await client.call(tool_id, args)
        except AtdError as exc:
            if exc.retryable and attempt < max_attempts - 1:
                delay = 0.2 * (2 ** attempt)
                await asyncio.sleep(delay)
            else:
                raise
    raise RuntimeError("unreachable")
```

The exceptions that carry a `.retryable` attribute set to `True` are `ServerUnreachable`, `Timeout`, and `BindingUnavailable`.

### ToolFailure.retryable

`ToolFailure` also has a `retryable` field populated from the server's response. A `ToolFailure` with `retryable=True` means the server executed the tool, got a transient error, and believes retrying is worthwhile.

```python
result = await client.call("ref:shell.exec", {"command": "some-flaky-command"})
if isinstance(result, ToolFailure) and result.retryable:
    # retry after a delay
    pass
```

---

## LangChain adapter

The LangChain adapter converts `ToolSummary` objects into `langchain_core.tools.StructuredTool` instances. Each tool is backed by the live ATD client — when LangChain calls a tool, the adapter calls `client.call(original_id, args)` on your behalf.

**Install:**

```bash
uv pip install -e 'python/[langchain]'
```

**Wire up an agent:**

```python
import asyncio
from atd_client import AtdClient
from atd_client.adapters import as_langchain_tools

async def main() -> None:
    async with await AtdClient.connect("<YOUR_SOCKET_PATH>") as client:
        summaries = await client.discover()
        lc_tools = as_langchain_tools(summaries, client=client)

        # Each element is a langchain_core.tools.StructuredTool.
        for tool in lc_tools:
            print(f"{tool.name}: {tool.description}")
            print(f"  args schema: {tool.args_schema.model_json_schema()}")

        # Use with any LangChain agent that accepts a tools= list.
        # The agent invokes tool.coroutine(**kwargs) when it chooses a tool.
        echo_tool = next(t for t in lc_tools if t.name == "ref_echo_say")
        result = await echo_tool.coroutine(text="hello from langchain")
        print(f"echo result: {result}")

asyncio.run(main())
```

**Key details:**

- Tool names are sanitized: `ref:echo.say` → `ref_echo_say`. Use the sanitized name when filtering the tool list by name.
- Each tool's `args_schema` is a Pydantic v2 model derived from the tool's `input_schema`. If the server did not populate `input_schema`, the model falls back to a permissive schema with a single `extra: Any` field.
- If `client=None` is passed to `as_langchain_tools`, the tools are constructed without a live backend — calling them raises `RuntimeError("client not bound")`. This form is useful for introspection without a running server.
- `ToolFailure` from `client.call` is surfaced as `RuntimeError("[CODE] message")` within the tool coroutine, so LangChain sees it as a tool error and can decide whether to retry or report to the user.

**Run the in-repo LangChain example:**

```bash
cargo build --release -p atd-ref-server
uv pip install -e 'python/[langchain]'
uv run python python/examples/hello_langchain.py
```

For a full AgentExecutor walk-through, see [`docs/integrations/langchain.md`](../integrations/langchain.md).

---

## OpenAI and Anthropic adapters

These adapters return plain Python dicts in each provider's function-calling shape. They do not require the `langchain` extra.

### OpenAI

```python
from atd_client import AtdClient
from atd_client.adapters import as_openai_tools

async with await AtdClient.connect("<YOUR_SOCKET_PATH>") as client:
    summaries = await client.discover()
    tools = as_openai_tools(summaries)

# tools is a list[dict] shaped for OpenAI's `tools` parameter.
# Pass it directly to the openai SDK (not included in atd-client):
#
#   response = openai_client.chat.completions.create(
#       model="gpt-4o",
#       tools=tools,
#       messages=[...],
#   )
```

Each element has the shape:

```json
{
  "type": "function",
  "function": {
    "name": "ref_echo_say",
    "description": "Echo text back to the caller",
    "parameters": {
      "type": "object",
      "properties": { "text": { "type": "string" } },
      "required": ["text"]
    }
  }
}
```

### Anthropic

```python
from atd_client import AtdClient
from atd_client.adapters import as_anthropic_tools

async with await AtdClient.connect("<YOUR_SOCKET_PATH>") as client:
    summaries = await client.discover()
    tools = as_anthropic_tools(summaries)

# tools is a list[dict] shaped for Anthropic's `tools` parameter.
# Pass it directly to the anthropic SDK (not included in atd-client):
#
#   response = anthropic_client.messages.create(
#       model="claude-opus-4-5",
#       tools=tools,
#       messages=[...],
#   )
```

Anthropic's shape differs from OpenAI's: no `"type": "function"` wrapper, and the schema field is `"input_schema"`:

```json
{
  "name": "ref_echo_say",
  "description": "Echo text back to the caller",
  "input_schema": {
    "type": "object",
    "properties": { "text": { "type": "string" } }
  }
}
```

### Resolving sanitized names back to ATD ids

When an LLM returns a tool_call with the sanitized name (e.g. `ref_echo_say`), resolve it back to the canonical ATD id before calling the tool:

```python
from atd_client.adapters import desanitize_tool_name

# desanitize_tool_name uses a hardcoded namespace heuristic.
# It works for known namespaces (ref, host, mock).
atd_id = desanitize_tool_name("ref_echo_say")  # → "ref:echo.say"

result = await client.call(atd_id, llm_tool_args)
```

For namespaces not in the known list, keep your own mapping:

```python
summaries = await client.discover()
name_to_id = {
    tool_name: s.id
    for s in summaries
    for tool_name in [s.id.replace(":", "_").replace(".", "_")]
}

atd_id = name_to_id.get(llm_tool_name)
if atd_id is None:
    raise ValueError(f"unknown tool name: {llm_tool_name}")

result = await client.call(atd_id, llm_tool_args)
```

---

## Next steps

- **LangChain integration:** [`docs/integrations/langchain.md`](../integrations/langchain.md) — full `AgentExecutor` walk-through, `StructuredTool` internals, error surfaces, async considerations.
- **Wire protocol:** [`docs/protocol/wire-format.md`](../protocol/wire-format.md) — length-prefixed JSON framing, all message types, extension points.
- **Error reference:** [`docs/protocol/error-codes.md`](../protocol/error-codes.md) — full exception table with trigger conditions and recovery strategies.
- **Rust SDK:** [`docs/quickstart/rust.md`](rust.md) — same three APIs in Rust, adapter usage, retry wrapper.
- **In-repo examples:**
  - `python/examples/hello_atd.py` — self-contained async demo that auto-spawns `atd-ref-server`
  - `python/examples/hello_langchain.py` — LangChain adapter demo with live tool invocation
