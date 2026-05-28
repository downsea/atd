# Python server runtime (`atd_server`)

Reference Python server runtime for the [ATD protocol](../atd-architecture.md). Sibling
package of `atd_client`; lives at `python/src/atd_server/`.

Use this when your tool host needs to run inside a Python process — e.g.
cbrain-sim co-located with a MuJoCo simulator singleton, a notebook server
exposing internal helpers, a Hermes Agent bridge that owns stateful Python
objects.

The Rust [`atd-server`](../../crates/atd-server/) and `atd-server-http` are the
right choice when the tool host is naturally Rust (healthkit_cli, celia_phr,
performance-critical reference servers). Both ship the same wire protocol —
clients and bridges don't care which language served them.

## Status

`SP-server-py-v1` (2026-05-19). Phase B–H. Byte-compat with the Rust
ref-server (verified via 22 of the 24 `atd-conformance` fixtures in
`python/tests/test_server_conformance.py`).

What works:

- Unix socket transport (`UnixSocketTransport`); pluggable via `Transport` Protocol.
- Hello handshake with adopter-supplied `ServerPolicy`; `granted_capabilities`
  is the policy's choice (default: grant verbatim).
- `tool_list` + `tool_schema` with `ToolVisibility.HIDDEN` filter (hidden tools
  are reachable by id, excluded from list — matches `sp-tool-visibility-hidden`).
- `run_tool` dispatch: capability gate, optional JSONSchema args validation
  (via `pip install atd-client[validation]`), tier-derived deadline from
  `definition.resources.timeout_ms`, dry-run short-circuit.
- Typed error envelope: `ToolError(code, message, partial_data, retryable)`
  for explicit failure; unhandled `Exception` → `1099` with `ExcClass` only
  (no traceback on the wire).
- Middleware: `@server.middleware(stage="pre_call" | "post_call" | "on_error")`
  with the standard call_next continuation pattern.
- Graceful shutdown: SIGTERM / SIGINT → `stop()` → drain → cancel after timeout.

Not in v1 (tracked separately):

- Cancel / streaming responses → `SP-cancel-streaming-v1`.
- Binary frame payloads → `SP-binary-frames-v1` (design only).
- HTTP transport → `SP-server-py-http-v1`.
- Conformance runner CLI (`atd-conformance-py`) → `SP-conformance-py-v1`
  (depends on this SP).
- Full UCAN-lite verification parity → `SP-server-py-ucan-v1`.

## Hello world (cbrain-style)

```python
import asyncio

from atd_client.types import (
    ToolDefinition, ToolCapability, ToolSafety, ToolResources, ToolTrust,
    ToolBinding, BindingProtocol, SafetyLevel, TrustLevel, ToolVisibility,
    ToolSuccess, ToolResultMetadata,
)
from atd_server import AtdServer, CallContext, ToolError


server = AtdServer(
    socket_path="/tmp/cbrain-sim.sock",
    server_id="cbrain-sim",
)


@server.register(
    definition=ToolDefinition(
        id="cbrain:perception.snapshot",
        name="Snapshot",
        description="Render one RGB+depth frame from the simulator.",
        version="0.1.0",
        capability=ToolCapability(
            domain="perception",
            actions=["read"],
            tags=[],
            intent_examples=[],
        ),
        input_schema={"type": "object", "properties": {"camera": {"type": "string"}}},
        output_schema={},
        bindings=[ToolBinding(protocol=BindingProtocol.APP_FUNCTION, config={})],
        safety=ToolSafety(level=SafetyLevel.READ, dry_run=True, side_effects=[]),
        resources=ToolResources(timeout_ms=2000, max_concurrent=1),
        trust=ToolTrust(publisher="cbrain", trust_level=TrustLevel.L0_UNVERIFIED),
        visibility=ToolVisibility.READ,
        required_capabilities=["perception"],
    )
)
async def snapshot(args: dict, ctx: CallContext) -> ToolSuccess:
    # closure-captures the singleton simulator; we're in the same Python process
    frame = await sim.render(camera=args.get("camera", "default"))
    return ToolSuccess(
        data={"rgb_b64": frame.rgb_b64, "depth_b64": frame.depth_b64},
        metadata=ToolResultMetadata(tool_id="cbrain:perception.snapshot"),
    )


asyncio.run(server.serve())  # blocks until SIGTERM / server.stop()
```

The handler receives:

- `args: dict` — JSON-decoded `Request::RunTool.args`. If the definition's
  `input_schema` is non-empty AND `jsonschema` is installed, it has already
  been validated when the handler runs.
- `ctx: CallContext` — `request_id` (server-generated), `tool_id`,
  `granted_capabilities` (frozenset of strings), `connection` (the
  `ConnectionContext` with `client_id` / `ucan_tokens` / `remote_addr`).

`ctx.dry_run` does NOT exist: dispatch auto-short-circuits dry-run before
the handler is called. If you need handler-controlled dry-run behavior, file
an issue.

## Capability gate

```python
# tool requires "perception":
required_capabilities=["perception"]

# Hello:
# {"type": "hello", "requested_capabilities": ["perception"]}
# ServerPolicy decides what to grant. The default grants verbatim.
# A production policy intersects with an allow-list:

from atd_server import GrantedCapabilities

_OFFER = frozenset({"perception", "world.read"})

async def my_policy(hello: dict, ucan_tokens: tuple[str, ...]) -> GrantedCapabilities:
    requested = hello.get("requested_capabilities") or []
    granted = {str(c) for c in requested if c in _OFFER}
    return GrantedCapabilities(capabilities=frozenset(granted))

server = AtdServer(socket_path=..., policy=my_policy)
```

If the client never sends Hello, `granted_capabilities` is empty —
cap-requiring tools fail with `1001` at call time.

## Error envelope

Three ways a handler can fail:

```python
@server.register(definition=...)
async def handler(args: dict, ctx: CallContext):
    # 1. Explicit ToolFailure return (use cbrain's 2000-2099 namespace)
    if missing_required(args):
        return ToolFailure(code="2001", message="missing 'target' field", retryable=False)

    # 2. Raise ToolError — same effect, with partial_data:
    raise ToolError(code=2042, message="skill aborted", partial_data={"step": 3})

    # 3. Let an unexpected exception propagate — server converts to 1099:
    # raise ValueError(...)  # → {"code": 1099, "message": "internal_error: ValueError"}
    #                            (the exception text is NOT on the wire)
```

Numeric `code` values from `ToolFailure(code="2001", ...)` are int-coerced
on the wire (so adopters can stay in their numeric namespace). Non-numeric
codes pass through as strings.

## Middleware

```python
import time

audit_log: list[dict] = []

@server.middleware(stage="pre_call")
async def trace_in(request: dict, ctx: CallContext, call_next):
    start = time.perf_counter()
    response = await call_next()
    audit_log.append({
        "tool_id": ctx.tool_id,
        "request_id": ctx.request_id,
        "wall_ms": (time.perf_counter() - start) * 1000,
        "success": isinstance(response, dict) and response.get("success", False),
    })
    return response


@server.middleware(stage="on_error")
async def trap(request: dict, ctx: CallContext, exc: BaseException):
    # Return None to fall through to the default envelope; return a typed
    # failure to suppress.
    if isinstance(exc, ConnectionResetError):
        return ToolFailure(code="2999", message="downstream disconnected",
                           retryable=True)
    return None
```

Ordering with `pre1`, `pre2`, `post1`, `post2` registered in that order:

```
pre1:enter → pre2:enter → post1:enter → post2:enter → handler
           → post2:exit  → post1:exit  → pre2:exit   → pre1:exit
```

A `pre_call` that returns without `await call_next()` short-circuits the
handler (useful for rate limiting / quota / contract checks).

## Graceful shutdown

```python
# In your application:
async def main():
    server = AtdServer(socket_path="/tmp/x.sock")
    # ...register tools...
    serve_task = asyncio.create_task(server.serve())
    try:
        await asyncio.shield(serve_task)
    except asyncio.CancelledError:
        await server.stop(drain_timeout_s=10.0)
        await serve_task
```

`SIGTERM` / `SIGINT` (Unix, main thread) install handlers automatically and
trigger `stop()`. Drain timeout default is 5s; long-running tools (cbrain's
COLD tier or external API waits) should override.

## Related

- Spec: [`docs/archive/superpowers/specs/2026-05-19-sp-server-py-v1-design.md`](../archive/superpowers/specs/2026-05-19-sp-server-py-v1-design.md)
- Plan: [`docs/archive/superpowers/plans/2026-05-19-sp-server-py-v1.md`](../archive/superpowers/plans/2026-05-19-sp-server-py-v1.md)
- Driving issue: [`docs/issues/2026-05-19-cbrain-adopter-requirements.md`](../issues/2026-05-19-cbrain-adopter-requirements.md) (cbrain P0-1)
- Sibling Rust runtime: [`crates/atd-server/`](../../crates/atd-server/) (UDS) and `crates/atd-server-http/` (HTTP)
- Wire format (shared with Rust): [`docs/protocol/wire-format.md`](../protocol/wire-format.md)
