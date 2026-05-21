# LangChain Integration — ATD Client SDK

**Environment:** Linux, Python 3.10+, LangChain Core 0.2+. Tested on the `sp10-adapters` tag (252 workspace tests green).

---

## What you'll build

By the end of this guide you will have:

- A LangChain `AgentExecutor` that calls ATD tools to read files, run shell commands, and more
- A clear mental model of how `as_langchain_tools()` wraps ATD summaries into LangChain `StructuredTool` instances
- Strategies for filtering tools to keep the agent's context lean
- A troubleshooting reference for the most common LangChain + ATD integration failures

The integration works by wrapping the ATD discover-then-call pattern in LangChain's `StructuredTool` abstraction. You discover tools from the ATD server once at startup, convert the summaries into LangChain tool objects, then hand those objects to any LangChain agent or chain.

---

## Prerequisites

**ATD server running:**

```bash
# Build and start the reference server
cargo build --release -p atd-ref-server
./target/release/atd-ref-server --sock /tmp/my-atd.sock
```

**Python client with the LangChain extra:**

```bash
# From source (not yet on PyPI)
pip install -e '/path/to/atd/python[langchain]'

# Future: pip install 'atd-client[langchain]'
```

This installs `atd-client` plus `langchain-core` and `pydantic`. The `langchain-core` package is the minimum dependency; you'll need a separate provider package for the LLM (e.g., `langchain-openai`, `langchain-anthropic`).

**LLM API key in environment:**

```bash
export OPENAI_API_KEY="<YOUR_API_KEY>"
# or ANTHROPIC_API_KEY, depending on your provider
```

---

## End-to-end example

The following script connects to ATD, discovers all available tools, binds them to a LangChain agent, and runs a prompt that causes the agent to invoke a tool.

```python
"""LangChain agent wired to ATD tools.

Usage:
    cargo build --release -p atd-ref-server
    pip install -e '/path/to/atd/python[langchain]'
    pip install langchain-openai
    export OPENAI_API_KEY=<YOUR_API_KEY>
    export ATD_SOCK=/tmp/my-atd.sock   # optional; auto-spawns if unset
    python agent.py
"""

from __future__ import annotations

import asyncio
import os
import signal
import sys
from pathlib import Path

from langchain.agents import AgentExecutor, create_tool_calling_agent
from langchain_core.prompts import ChatPromptTemplate
from langchain_openai import ChatOpenAI

from atd_client import AtdClient
from atd_client.adapters import as_langchain_tools


async def _wait_for_socket(sock: Path, attempts: int = 30, interval: float = 0.1) -> bool:
    for _ in range(attempts):
        if sock.exists():
            return True
        await asyncio.sleep(interval)
    return False


async def main() -> None:
    # ── 1. Start or attach to the ATD reference server ──────────────────────
    proc = None
    tmpdir = None
    override = os.environ.get("ATD_SOCK")
    if override:
        sock = Path(override)
    else:
        import tempfile
        repo_root = Path(__file__).resolve().parent.parent
        binary = repo_root / "target" / "release" / "atd-ref-server"
        if not binary.exists():
            raise SystemExit(f"Build first: cargo build --release -p atd-ref-server")
        tmpdir = tempfile.TemporaryDirectory()
        sock = Path(tmpdir.name) / "demo.sock"
        proc = await asyncio.create_subprocess_exec(
            str(binary), "--sock", str(sock),
            stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.DEVNULL,
        )
        if not await _wait_for_socket(sock):
            proc.kill()
            raise SystemExit("atd-ref-server didn't bind within 3s")

    try:
        # ── 2. Discover tools and convert to LangChain ──────────────────────
        async with await AtdClient.connect(sock) as client:
            summaries = await client.discover(limit=None)
            tools = as_langchain_tools(summaries, client=client)
            print(f"[atd] {len(tools)} tools ready: {[t.name for t in tools]}")

            # ── 3. Build the LangChain agent ─────────────────────────────────
            llm = ChatOpenAI(model="gpt-4o-mini", temperature=0)
            prompt = ChatPromptTemplate.from_messages([
                ("system", "You are a helpful assistant with access to ATD tools."),
                ("human", "{input}"),
                ("placeholder", "{agent_scratchpad}"),
            ])
            agent = create_tool_calling_agent(llm, tools, prompt)
            executor = AgentExecutor(agent=agent, tools=tools, verbose=True)

            # ── 4. Run a prompt that exercises a tool ─────────────────────────
            result = await executor.ainvoke({
                "input": "Use the shell tool to run 'uname -r' and tell me the kernel version."
            })
            print("\n[agent] Final answer:", result["output"])

    finally:
        if proc is not None and proc.returncode is None:
            proc.send_signal(signal.SIGTERM)
            try:
                await asyncio.wait_for(proc.wait(), timeout=2.0)
            except asyncio.TimeoutError:
                proc.kill()
                await proc.wait()
        if tmpdir is not None:
            tmpdir.cleanup()


if __name__ == "__main__":
    asyncio.run(main())
```

Run it:

```bash
python agent.py
```

Expected output (abbreviated):

```
[atd] 9 tools ready: ['ref_echo_say', 'ref_fs_read', 'ref_fs_glob', ...]

> Entering new AgentExecutor chain...
Invoking: `ref_shell_exec` with `{'command': 'uname -r'}`

[agent] Final answer: The kernel version is 6.19.10-200.fc43.x86_64.
```

---

## How `as_langchain_tools` works

`as_langchain_tools(summaries, client=client)` converts each `ToolSummary` into a LangChain `StructuredTool`. Understanding this conversion helps you debug schema mismatches and customize tool behaviour.

### Name sanitization

ATD tool IDs use the form `namespace:domain.action` (e.g., `ref:fs.read`). MCP and LangChain both reject colons and dots in tool names. The adapter applies the same sanitization rule as the MCP bridge:

```
ref:fs.read  →  ref_fs_read
ref:shell.exec  →  ref_shell_exec
```

The mapping is performed by `atd_client.sanitize.sanitize_tool_name()`. When a tool call appears in agent traces, you will see the sanitized name. Use `atd_client.adapters.desanitize_tool_name()` to map it back if you need the original ID for logging or the ATD CLI.

### Pydantic v2 argument model

Each tool's JSON Schema `input_schema` is converted to a Pydantic v2 model at adapter construction time via `_build_pydantic_model()`. The conversion supports:

| JSON Schema type | Python type |
|---|---|
| `string` | `str` |
| `integer` | `int` |
| `number` | `float` |
| `boolean` | `bool` |
| `array` | `list` |
| `object` | `dict` |
| anything else | `Any` |

Required properties become `Field(...)` (mandatory). Optional properties become `Field(None)`. If the schema has no properties, the adapter generates a permissive `extra: Any = None` field rather than an empty model, because Pydantic v2 rejects zero-field models.

### Async invocation path

The tool's `coroutine` (the async callable LangChain invokes) does:

```python
async def _arun(**kwargs: Any) -> Any:
    if client is None:
        raise RuntimeError(
            f"ATD tool '{original_id}' has no client bound; "
            "pass client=<AtdClient> to as_langchain_tools()"
        )
    result = await client.call(original_id, kwargs)
    if hasattr(result, "code") and hasattr(result, "message"):
        raise RuntimeError(f"[{result.code}] {result.message}")
    return result.data
```

On `ToolFailure`, the adapter raises a `RuntimeError` with the ATD error code and message. LangChain's `AgentExecutor` catches this and passes the error text back to the LLM so it can self-correct (as demonstrated in the SP-7 Hermes transcript).

On `ToolSuccess`, the adapter returns `result.data` — the raw JSON-decoded payload from the server.

### StructuredTool construction

```python
StructuredTool.from_function(
    coroutine=_arun,        # async path used by AgentExecutor.ainvoke()
    name=sanitized,         # "ref_fs_read"
    description=summary.description,
    args_schema=args_model, # Pydantic v2 model derived from input_schema
)
```

No synchronous `_run` is registered. If you call a tool synchronously (e.g., in a `Tool.run()` call), LangChain will raise an error. Always use `AgentExecutor.ainvoke()` or `await tool.coroutine(...)` directly.

---

## Handling ATD errors in LangChain

ATD surfaces two error layers when called through LangChain:

**1. Transport errors (before the tool call reaches the server)**

These appear as `AtdError` exceptions raised from `client.call()`. They escape the `_arun` coroutine as unhandled exceptions and terminate the agent run. Wrap the outer `executor.ainvoke()` call to handle them:

```python
from atd_client.errors import AtdError, ServerUnreachableError

try:
    result = await executor.ainvoke({"input": user_prompt})
except ServerUnreachableError:
    print("ATD server is not running at the configured socket path.")
except AtdError as exc:
    print(f"ATD protocol error: {exc}")
```

**2. Tool execution failures (server returned ToolFailure)**

The adapter converts `ToolFailure` into `RuntimeError("[CODE] message")`. LangChain's `AgentExecutor` does **not** propagate `RuntimeError` to the caller; instead, it adds the error text to the agent scratchpad and lets the LLM respond. This is the self-correction behaviour you see in the SP-7 transcript.

To observe tool failures in logs, set `verbose=True` on `AgentExecutor`. You will see:

```
Error: [INVALID_ARGS] missing field `command`
```

**3. Retry strategy**

ATD error codes carry an `is_retryable` flag. The adapter does not implement retry automatically. For production use, wrap the `executor.ainvoke()` call with exponential back-off for retryable failures:

```python
import asyncio
from atd_client.errors import AtdError

async def run_with_retry(executor, input_dict, max_attempts: int = 3) -> dict:
    for attempt in range(max_attempts):
        try:
            return await executor.ainvoke(input_dict)
        except AtdError as exc:
            if not exc.is_retryable or attempt == max_attempts - 1:
                raise
            delay = 2 ** attempt
            await asyncio.sleep(delay)
    raise RuntimeError("unreachable")
```

---

## Working with tool subsets

Passing all 9 reference server tools to the LLM wastes context tokens and increases the chance of hallucinated tool calls. Filter `discover()` results before calling `as_langchain_tools()`.

**Filter by query string:**

```python
summaries = await client.discover(query="file", limit=5)
tools = as_langchain_tools(summaries, client=client)
```

**Filter by tool ID prefix:**

```python
all_summaries = await client.discover(limit=None)
fs_summaries = [s for s in all_summaries if s.id.startswith("ref:fs.")]
tools = as_langchain_tools(fs_summaries, client=client)
```

**Use `DiscoverFilter` for structured filtering:**

```python
from atd_client import DiscoverFilter

summaries = await client.discover(
    filter=DiscoverFilter(namespace="ref", tier="hot"),
    limit=10,
)
tools = as_langchain_tools(summaries, client=client)
```

Pass only the tools your agent needs. A file-reading agent doesn't need `ref:shell.exec`; a system-info agent doesn't need `ref:fs.write`. Lean tool sets produce faster, more reliable agents.

---

## Common pitfalls

**1. Sanitized name in traces vs original ID in `atd call`**

The agent trace shows `ref_fs_read` (sanitized), but `atd call ref:fs.read` uses the original ATD ID. These are the same tool. Use `desanitize_tool_name("ref_fs_read")` if you need to cross-reference trace output with `atd` CLI output.

**2. `langchain-core` 0.2 vs 0.3 API differences**

`create_tool_calling_agent` is available in `langchain` ≥0.1.14. In older versions, you may need `create_openai_tools_agent` or `create_react_agent` depending on the LLM backend. Check your `langchain` version:

```bash
python -c "import langchain; print(langchain.__version__)"
```

If you see `AttributeError: module 'langchain.agents' has no attribute 'create_tool_calling_agent'`, upgrade:

```bash
pip install --upgrade langchain langchain-core
```

**3. Empty `args_schema` for tools with no declared input schema**

Some ATD tools may have no `input_schema` in their summary (they rely on the full `ToolDefinition`). The adapter falls back to a permissive `extra: Any = None` model. The LLM will see an empty schema and may not know what arguments to supply. For such tools, call `client.describe(tool_id)` and post-process the `ToolDefinition.input_schema` before passing to the adapter:

```python
summaries = await client.discover(limit=None)
for s in summaries:
    if s.input_schema is None:
        defn = await client.describe(s.id)
        # Rebuild summary with the richer schema — or pass summaries list
        # to a custom StructuredTool builder that uses defn.input_schema.
```

**4. Calling tools synchronously**

`as_langchain_tools()` only registers the `coroutine` path. Calling `tool.run(...)` (synchronous) raises:

```
NotImplementedError: Tool ref_fs_read does not support sync invocation.
```

Always use `AgentExecutor.ainvoke()` or `asyncio.run(tool.coroutine(...))`.

**5. Client connection closed before tool invocation**

If you build tools inside an `async with AtdClient.connect(sock) as client:` block and then use those tools outside it, the connection will be closed and every tool call will raise `ServerUnreachableError`. Keep the client alive for the lifetime of the agent run.

**6. Pydantic v1 installed alongside v2**

If your environment has both `pydantic` v1 and v2 (common with older LangChain installs), `create_model` may resolve to the v1 API, which uses different field definition syntax. Ensure you are on Pydantic v2:

```bash
python -c "import pydantic; print(pydantic.__version__)"
```

If you see `1.x`, upgrade: `pip install --upgrade pydantic`.

**7. Async event loop conflicts in Jupyter**

Jupyter kernels run their own event loop. `asyncio.run(main())` raises `RuntimeError: This event loop is already running`. Use `nest_asyncio` or restructure the code:

```python
import nest_asyncio
nest_asyncio.apply()
asyncio.run(main())
```

**8. Tool not found after name collision**

If two ATD tools sanitize to the same LangChain name (e.g., `ref:fs_read` and `ref:fs.read` both become `ref_fs_read`), only the last one in the list survives. This shouldn't happen in a well-named ATD registry, but if you see unexpected "tool not found" errors, inspect:

```python
for t in tools:
    print(t.name)
```

Duplicates indicate a naming collision. Filter or rename before passing to the agent.

---

## Advanced: custom tool descriptions

The LLM selects tools based on their description. ATD's `ToolSummary.description` is set by the server. You can override or augment it before building LangChain tools:

```python
from dataclasses import replace

summaries = await client.discover(limit=None)

# Override description for a specific tool
augmented = [
    replace(s, description="Read a text file with line numbers. Use this for inspecting source code.")
    if s.id == "ref:fs.read"
    else s
    for s in summaries
]

tools = as_langchain_tools(augmented, client=client)
```

`ToolSummary` is a dataclass; `dataclasses.replace()` creates a new instance with the modified field (immutable pattern — no mutation of the original).

You can also add tool-specific routing hints:

```python
routing_hints = {
    "ref:shell.exec": "Use ONLY when the user explicitly requests a shell command. Never use for file operations.",
    "ref:fs.write": "Use ONLY when the user asks to write, save, or create a file.",
}

augmented = [
    replace(s, description=routing_hints.get(s.id, s.description))
    for s in summaries
]
```

This is useful when your agent tends to reach for a broad tool (like `shell.exec`) for tasks better handled by a specialized one.

---

## Troubleshooting

**`ImportError: as_langchain_tools() requires the 'langchain' extra. Install with: pip install 'atd-client[langchain]'`**

You imported `as_langchain_tools` in an environment without `langchain-core`. Install the extras:

```bash
pip install 'atd-client[langchain]'
# or from source:
pip install -e '/path/to/atd/python[langchain]'
```

**`RuntimeError: ATD tool 'ref:fs.read' has no client bound; pass client=<AtdClient> to as_langchain_tools()`**

You called `as_langchain_tools(summaries)` without the `client=` argument. Pass the live client:

```python
tools = as_langchain_tools(summaries, client=client)
```

**`pydantic.error_wrappers.ValidationError` on tool invocation**

The LLM provided an argument that doesn't match the Pydantic model's type. For example, it passed a string where an integer was expected. Check the tool's `args_schema`:

```python
tool = next(t for t in tools if t.name == "ref_shell_exec")
print(tool.args_schema.model_json_schema())
```

This prints the JSON Schema the LLM sees. Verify the LLM is using the correct type. If the schema is wrong, call `client.describe()` and inspect `ToolDefinition.input_schema`.

**Agent runs silently without invoking any tools**

This usually means the LLM doesn't understand when to use the tools. Improve the system prompt:

```python
prompt = ChatPromptTemplate.from_messages([
    ("system", (
        "You have access to ATD tools. "
        "When the user asks for file operations or shell commands, "
        "ALWAYS use the provided tools — do not attempt to simulate them."
    )),
    ("human", "{input}"),
    ("placeholder", "{agent_scratchpad}"),
])
```

**`ConnectionRefusedError` or `FileNotFoundError` on socket**

The ATD server is not running at the configured socket path. Verify:

```bash
ls -la /tmp/my-atd.sock
# Should show: srwxrwxrwx ... /tmp/my-atd.sock
```

If missing, start the server:

```bash
./target/release/atd-ref-server --sock /tmp/my-atd.sock
```

---

## See also

- [`docs/quickstart/python.md`](../quickstart/python.md) — Python SDK basics, `discover`/`describe`/`call` API
- [`docs/integrations/hermes.md`](hermes.md) — command-line LLM agent with ATD tools via MCP
- [`docs/protocol/wire-format.md`](../protocol/wire-format.md) — ATD wire protocol reference
- [`python/examples/hello_langchain.py`](../../python/examples/hello_langchain.py) — minimal runnable demo
- [`python/src/atd_client/adapters.py`](../../python/src/atd_client/adapters.py) — adapter source
