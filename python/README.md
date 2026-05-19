# atd-client / atd-server (Python)

Reference Python SDK and **server runtime** for the [ATD protocol](../docs/design.md).

- `atd_client` — async-first client (with sync wrapper), LLM adapters, type
  models. The original Phase-1 package.
- `atd_server` — Python server runtime (SP-server-py-v1, 2026-05-19).
  Use this when your tool host needs to live inside a Python process
  (e.g. a MuJoCo-backed simulator, a notebook server). Byte-compat with
  the Rust `atd-server`; the Python `AtdClient`, Rust `atd-sdk`, and
  `atd-mcp-bridge` can all connect to either reference server
  indistinguishably. See [`docs/integrations/python-server.md`](../docs/integrations/python-server.md).

## Install

From source (during Phase 1, before PyPI publish):

```bash
cd atd-mvp/python
uv sync
uv run python examples/hello_atd.py
```

Once published:

```bash
pip install atd-client
# or: uv add atd-client
```

## Quickstart — async

```python
import asyncio
from atd_client import AtdClient

async def main():
    async with await AtdClient.connect() as client:     # uses ~/.anos/anos.sock
        tools = await client.discover(query="fs", limit=5)
        for t in tools:
            print(t.id, "—", t.name)
        result = await client.call("anos:fs.read", {"path": "/etc/hostname"})
        print(result)

asyncio.run(main())
```

## Quickstart — server (cbrain-style)

```python
import asyncio
from atd_server import AtdServer, CallContext
# ToolDefinition / ToolSuccess / etc. live in atd_client.types
from atd_client.types import ToolDefinition, ToolSuccess, ToolResultMetadata, ...

server = AtdServer(socket_path="/tmp/my-tools.sock", server_id="demo")

@server.register(definition=ToolDefinition(id="demo:echo", ...))
async def echo(args: dict, ctx: CallContext) -> ToolSuccess:
    return ToolSuccess(data={"echoed": args},
                       metadata=ToolResultMetadata(tool_id="demo:echo"))

asyncio.run(server.serve())
```

Full guide: [`docs/integrations/python-server.md`](../docs/integrations/python-server.md).

## Quickstart — sync (LangChain / notebooks)

```python
from atd_client import AtdClientSync

client = AtdClientSync.connect()
tools = client.discover(query="fs", limit=5)
result = client.call("anos:fs.read", {"path": "/etc/hostname"})
client.close()
```

## LLM adapters

```python
from atd_client import AtdClient, as_openai_tools, as_anthropic_tools

async with await AtdClient.connect() as client:
    summaries = await client.discover()
    openai_tools = as_openai_tools(summaries)          # OpenAI function-calling shape
    anthropic_tools = as_anthropic_tools(summaries)    # Anthropic tool-use shape
```

## Development

```bash
uv sync                    # install dev deps
uv run pytest              # full test suite
uv run mypy src            # type check
uv run ruff check src tests
```

Contract tests under `tests/test_anos_fixture.py` replay live-ANOS responses captured at `../crates/atd-client/tests/fixtures/`. Refresh with `../scripts/capture_anos_fixtures.sh`.

## Phase 0 known limitation

`call()` currently returns an error against the ANOS reference server because its `run_tool` IPC is stubbed. See `../docs/issues/2026-04-21-atd-run-tool-stub.md`.

## License

Apache-2.0.
