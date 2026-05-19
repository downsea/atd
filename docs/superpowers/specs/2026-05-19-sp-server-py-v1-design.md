# SP-server-py-v1: Python server runtime for the ATD reference implementation

| Status | Draft |
| Created | 2026-05-19 |
| Author | ATD team (response to cbrain adopter requirements 2026-05-19) |
| Phase | post `sp-pagination-v1`; no in-flight SP dependency |
| Related | cbrain issue `docs/issues/2026-05-19-cbrain-adopter-requirements.md` (P0-1, bundles P2-8 middleware) · sibling `SP-cancel-streaming-v1` (future; this SP leaves the per-request-id seam) · `SP-conformance-py-v1` (follow-up; depends on this SP) · existing `crates/atd-server` Rust runtime (parity target) |

---

## 1. Motivation

**1.1 cbrain is the third confirmed adopter and the first one that cannot use the Rust server.** cbrain (embodied-agent S2 layer, `/home/nan/code/cbrain`) ships its tool host inside the same Python process that owns the MuJoCo simulator state. MuJoCo's `MjData` is a stateful singleton — cross-process sharing means snapshot/serialize/restore on every tool call, which destroys the 30 Hz perception loop. So cbrain's tool host **must be Python**, and the existing Rust `atd-server` is not an option.

The cbrain requirements doc (`docs/issues/2026-05-19-cbrain-adopter-requirements.md` §3 P0-1) frames this as the only blocking gap for their W1 milestone (cognitive plane bring-up with Hermes Agent ↔ ATD ↔ cbrain-sim).

**1.2 The Python package today is client-only.** `python/src/atd_client/transport.py` is 21 LOC — just `connect_unix()` and a default-path helper. There is no `serve_unix()`, no accept loop, no handshake responder, no registry, no dispatch. Adopters who want a Python tool host today have two choices: (a) write an `atd-server` Rust binary and call out via FFI/subprocess (defeats the in-process simulator point), or (b) vendor their own ~300 LOC shim per project. cbrain has committed to (b) for W1-W7 to avoid blocking, but explicitly asks the ATD team to upstream the runtime so cbrain can delete the shim.

**1.3 A Python server is also a protocol-conformance forcing function.** Today the only production-quality server is `atd-ref-server` (Rust). The reference implementation passing all 36 `atd-conformance` fixtures only proves that the *Rust* implementation is consistent with itself. A second-language server materially raises confidence that the protocol spec (`docs/protocol/wire-format.md`, the JSON Schema in `crates/atd-protocol/schema/`) is implementable from the spec alone, not just from reading Rust source. cbrain's shim already demonstrated this informally (~300 LOC + byte-compat); an officially maintained Python server productionizes that demonstration.

**1.4 The right shape is symmetric to `atd-server` (Rust), not a slim subset.** A reduced "minimal" Python server that omits tier deadlines / capability gating / dry-run / middleware would force every Python adopter to reinvent the same plumbing cbrain just wrote. The Python runtime ships with the same protocol-level invariants the Rust runtime enforces (`atd-runtime`), exposed via idiomatic asyncio handlers. Adopters opt out of features by not using them, not by re-implementing them.

After this SP, a Python adopter can register tools as `async def handler(args, ctx)` and `await server.serve()` and have a byte-compatible ATD server that the Python `AtdClient`, the Rust `atd-sdk`, `atd-mcp-bridge`, and any third-party MCP client can connect to, with the same wire behavior as `atd-ref-server`.

## 2. Goals

- **G1: byte-compatible Python server runtime.** New top-level package `atd_server` (sibling of `atd_client`) under `python/src/atd_server/`. Wire frames read/written via the existing `atd_client.wire.{read_frame, write_frame}` and message tags via `atd_client.protocol.*` — no duplication of constants.
- **G2: Unix-socket transport.** `AtdServer.serve()` listens on a configurable UDS path, accepts N concurrent client connections, spawns one `asyncio.Task` per connection. Graceful shutdown on SIGTERM / SIGINT / `await server.stop()`.
- **G3: handshake with capability negotiation.** On `Hello`, server applies a configurable `ServerPolicy` (default: grant all `requested_capabilities`) and returns `HelloAck` with `granted_capabilities`, server id, and protocol version. The granted set is stored per-connection and used by the capability gate at dispatch.
- **G4: tool registry with `tool_list` + `tool_schema`.** `@server.register(definition=ToolDefinition(...))` registers a Python `async` handler against a `ToolDefinition`. `tool_list` returns the registered `ToolSummary`s honoring `visibility` (hidden tools excluded by default; explicit query reveals them — mirrors `sp-tool-visibility-hidden`). `tool_schema(tool_id)` returns the full `ToolDefinition`.
- **G5: `run_tool` dispatch with tier-aware deadlines, dry-run, and capability gate.** Dispatcher checks the granted capability set against `definition.capabilities`; on deny, returns `ERR_CAPABILITY_DENIED` (1001). If `dry_run=true`, returns `args_preview` without invoking the handler. Otherwise wraps handler invocation in `asyncio.wait_for(handler(args, ctx), timeout=tier_to_deadline(definition.tier))`. Tier defaults: `HOT=1s`, `WARM=30s`, `COLD=300s` (matches Rust runtime's frame_deadline_active).
- **G6: middleware hooks (P2-8 bundled).** `@server.middleware(stage="pre_call" | "post_call" | "on_error")` registers async wrappers that fire in registration order. `pre_call` can short-circuit (return a `ToolFailure` instead of `None`); `post_call` can mutate the response; `on_error` sees exceptions and may suppress (return a `ToolFailure`) or re-raise.
- **G7: explicit error envelope.** Handlers return `ToolSuccess | ToolFailure` (already in `atd_client.types`) or raise `atd_server.ToolError(code, message, ...)`. Unhandled exceptions become generic `1099 internal_error` with the exception class name (no traceback leaked to the wire).
- **G8: optional UCAN-lite passthrough seam.** `Hello.ucan_tokens` (already in `atd_client.protocol`) is parsed and stashed on the connection context as raw strings; an optional `UcanVerifier` Protocol lets adopters plug in a verifier. Full parse + revocation-store parity with `atd-runtime::ucan` is **out of scope for v1** — see Non-goals.
- **G9: tests + a representative conformance subset.** `python/tests/test_server_*` covers handshake, list/schema, dispatch, capability denial, tier deadline, dry-run, middleware ordering, error envelope, graceful shutdown. A new `python/tests/test_server_conformance.py` exercises ~10 of the 36 `atd-conformance` JSON fixtures against `AtdServer` (chosen by protocol surface, not by tool semantics; full Python conformance runner is `SP-conformance-py-v1`).
- **G10: documentation.** New `docs/integrations/python-server.md` (cbrain-style hello-world + tier + capability + middleware example). `docs/architecture.md` §8 crate table grows a Python-package row. `python/README.md` gains a "Server runtime" section linking to the integrations page.

## 3. Non-goals

- **Cancel / abort.** `SP-cancel-streaming-v1` (P1-3 + P1-4 combined). v1 server reads one request, dispatches it, writes the response, reads the next — strictly serial per connection. Multiple concurrent in-flight requests on the same connection are not supported. The connection handler is architected so the dispatch path is replaceable: v2 will swap the "read → dispatch → reply" body for a "spawn `RequestTask` per request, route response by `request_id`" body without changing the public registry API.
- **Chunked / streaming results.** Same SP as cancel. Handlers in v1 return a single `ToolSuccess | ToolFailure`; no `AsyncGenerator[chunk]` support.
- **Binary frame extension.** `SP-binary-frames-v1` (P1-5). v1 stays pure JSON UTF-8; same 10 MiB cap as the Rust ref-server.
- **HTTP transport (cloud / multi-tenant shape).** `SP-server-py-http-v1` (future). The architecture leaves room — the transport layer is `atd_server.adapters.unix.UnixSocketTransport`; adding `adapters/http.py` later does not affect the registry or dispatch — but no HTTP code lands here.
- **Full UCAN-lite verification parity with `atd-runtime::ucan`.** v1 ships a passthrough seam (G8); full parse / signature verify / revocation store mirrors land in `SP-server-py-ucan-v1` once a Python adopter actually needs to verify tokens server-side. cbrain does not (their shim ignores `ucan_tokens`).
- **Rate-limit / max-concurrent enforcement.** `ToolResources.rate_limit_per_min` and `.max_concurrent` are still ignored by the Rust runtime too (`docs/issues/2026-04-24-resource-limits-not-enforced.md`). Stay symmetric — fix in a future SP that touches both runtimes at once.
- **Audit sink mpsc parity.** Python adopters that want audit write to a file should use the middleware hook (G6) + `asyncio.Queue` themselves. Building a JsonLinesAuditSink-equivalent into the Python server is over-scope; cbrain wants a Merkle chain anyway (different sink shape), which middleware handles.
- **Conformance runner CLI.** v1 ships a *test suite* using fixtures, not a `atd-conformance-py` CLI binary that points at arbitrary servers. CLI is `SP-conformance-py-v1`.
- **`atd_server.AtdServer` published to pypi.** Distribution is `SP-publish-v2`'s problem (currently itself stale per CLAUDE.md). cbrain consumes via `path = "../atd-mvp/python"` for now, same as celia consumes the Rust crates via `path = ...`.

## 4. Public API surface

```python
# python/src/atd_server/__init__.py
from atd_server.server import AtdServer
from atd_server.context import CallContext, ConnectionContext
from atd_server.errors import ToolError, server_error_code
from atd_server.policy import ServerPolicy, default_policy, GrantedCapabilities
from atd_server.middleware import middleware_stage  # "pre_call" | "post_call" | "on_error" literal

__all__ = [
    "AtdServer",
    "CallContext",
    "ConnectionContext",
    "ToolError",
    "ServerPolicy",
    "default_policy",
    "GrantedCapabilities",
]
```

```python
# Public usage (the cbrain example):
import asyncio
from atd_server import AtdServer, CallContext, ToolError
from atd_client.types import (
    ToolDefinition, ToolSuccess, ToolFailure,
    ToolTier, ToolVisibility, ToolCapability, ToolResources,
)

server = AtdServer(
    socket_path="/tmp/cbrain-sim.sock",
    server_id="cbrain-sim",
)

@server.register(definition=ToolDefinition(
    id="cbrain:perception.snapshot",
    name="Snapshot",
    tier=ToolTier.WARM,
    visibility=ToolVisibility.READ,
    capabilities=[ToolCapability(domain="perception", action="read")],
    resources=ToolResources(rate_limit_per_min=None, max_concurrent=None),
    # ... description, args_schema, errors, ...
))
async def snapshot(args: dict, ctx: CallContext) -> ToolSuccess | ToolFailure:
    if ctx.dry_run:
        return ToolSuccess(data={"args_preview": args})
    frame = sim.render()  # closure-captures the singleton simulator
    return ToolSuccess(data={
        "rgb_b64": frame.rgb_b64,
        "depth_b64": frame.depth_b64,
    })

@server.middleware(stage="post_call")
async def merkle_audit(request, response, ctx, call_next):
    response = await call_next()
    merkle_chain.append(build_entry(request, response, ctx))
    return response

asyncio.run(server.serve())  # blocks until SIGTERM / server.stop()
```

The `ctx` parameter (`CallContext`) carries:

```python
@dataclass(frozen=True)
class CallContext:
    request_id: str            # echo of Request.request_id (server-generated if absent)
    tool_id: str
    dry_run: bool
    granted_capabilities: frozenset[ToolCapability]
    connection: ConnectionContext

@dataclass(frozen=True)
class ConnectionContext:
    client_id: str | None
    ucan_tokens: tuple[str, ...]   # raw passthrough; verifier optional
    remote_socket_addr: str        # for logging only
```

## 5. Design

This is the core of the SP. Eight subsections, each one of the architecture decisions.

### 5.1 Package layout: `atd_server` as a sibling of `atd_client`

**Decision.**

```
python/src/atd_server/
├── __init__.py            # public re-exports (§4)
├── server.py              # AtdServer: register / middleware / serve / stop
├── context.py             # CallContext / ConnectionContext frozen dataclasses
├── policy.py              # ServerPolicy Protocol + default_policy() + GrantedCapabilities
├── registry.py            # ToolRegistry: register / get / list (visibility-aware) / describe
├── dispatch.py            # run_tool dispatch + tier deadline wrap + dry_run short-circuit
├── handshake.py           # Hello → HelloAck negotiation
├── middleware.py          # middleware chain build + stage-ordered execution
├── errors.py              # ToolError exception class + ERR_* code constants + envelope helpers
├── adapters/
│   ├── __init__.py
│   └── unix.py            # UnixSocketTransport: bind / accept / close
└── _runtime.py            # connection task lifecycle + signal handlers + shutdown coordination
```

**Why a sibling package, not `atd_client.server`.**
- **Dependency hygiene.** A client-only adopter (e.g., a downstream notebook calling cbrain-sim) doesn't need the server-side asyncio listener, signal handlers, or accept loop. `import atd_client` should not transitively pull server code.
- **Mirrors Rust crate split.** `atd-sdk` (client) vs `atd-server` + `atd-runtime` (server) is the established crate boundary; Python should mirror it for cross-language familiarity.
- **Cleaner test isolation.** `python/tests/` already splits `test_client_*` from future `test_server_*` — separate packages keep imports unambiguous.

**Why not vendor cbrain's shim as `atd_client.server`.** Tempting (faster), but cbrain's shim is consciously minimal (no middleware, no UCAN seam, abbreviated error envelope) — productionizing it means adding the missing pieces and re-namespacing anyway. Building right from spec is faster than retro-fitting.

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| `atd_client.server` submodule | One package | Server deps pollute client import | rejected |
| New `atd_server` sibling | Clean split, mirrors Rust | One more dir | **chosen** |
| Merge into `atd_runtime_py` umbrella | Future-proof for Phase 2 | Speculative naming; nothing requires umbrella today | deferred |

### 5.2 Transport abstraction: `UnixSocketTransport` behind a `Transport` Protocol

**Decision.** `adapters/unix.py` defines a concrete `UnixSocketTransport` whose interface is captured by a `Transport` Protocol so HTTP / stdio adapters can land later without restructuring:

```python
class Transport(Protocol):
    async def bind(self) -> None: ...
    async def accept(self) -> tuple[asyncio.StreamReader, asyncio.StreamWriter, str]: ...
        # Returns (reader, writer, remote_addr_str). remote_addr_str is opaque
        # (UDS = path; HTTP = peer ip:port). Used for logging only.
    async def close(self) -> None: ...

class UnixSocketTransport:
    def __init__(self, socket_path: str, *, unlink_existing: bool = True) -> None: ...
    # implements Transport
```

`AtdServer.__init__` accepts either `socket_path: str` (constructs `UnixSocketTransport` itself) or `transport: Transport` (full control). cbrain's example uses the first form.

**Why a Protocol, not an ABC.** `typing.Protocol` is structural; adapters don't need to inherit. Matches the Pythonic patterns rule in user CLAUDE.md.

**`unlink_existing=True` by default.** UDS files persist on filesystem if the previous process crashed without cleanup. Default-safe behavior: unlink stale socket file before bind. If a real second instance is already listening, the bind fails fast with `OSError`. Adopters that don't want this can pass `unlink_existing=False`.

### 5.3 Concurrency model: one `asyncio.Task` per connection, serial dispatch within a connection

**Decision.** `AtdServer.serve()`:

```python
async def serve(self) -> None:
    await self._transport.bind()
    self._stop_event = asyncio.Event()
    self._install_signal_handlers()
    try:
        while not self._stop_event.is_set():
            try:
                reader, writer, remote = await self._accept_or_stop()
            except _StopRequested:
                break
            task = asyncio.create_task(self._handle_connection(reader, writer, remote))
            self._connection_tasks.add(task)
            task.add_done_callback(self._connection_tasks.discard)
    finally:
        await self._drain_and_close()

async def _handle_connection(self, reader, writer, remote) -> None:
    conn_ctx: ConnectionContext | None = None    # filled after Hello
    granted: frozenset[ToolCapability] = frozenset()
    try:
        while not self._stop_event.is_set():
            raw = await read_frame(reader)       # from atd_client.wire
            msg_type = raw.get("type")
            response = await self._dispatch_msg(raw, msg_type, conn_ctx_setter, granted_setter)
            await write_frame(writer, response)
    except asyncio.IncompleteReadError:
        return  # client closed
    except Exception as e:
        self._log_unexpected(e)
    finally:
        writer.close()
        with contextlib.suppress(Exception):
            await writer.wait_closed()
```

**Strictly serial within a connection.** v1 reads → dispatches → writes → next read. Concurrent `run_tool`s on the same connection are not supported. This matches the Rust `atd-server::connection` behavior today (verified at `crates/atd-server/src/connection.rs:23-39`). Adopters who want concurrency open multiple connections (one per logical caller).

**Why serial.** Two reasons:
1. The v0.1.0 wire format has no `request_id` field on `Request` (only on `Response.tool_result.request_id`, generated by server). Routing concurrent replies back to the right caller requires `SP-cancel-streaming-v1` to add `Request.request_id`. Pre-empting that decision now is premature.
2. cbrain's stateful simulator wants serial-per-connection anyway — concurrent `manipulation.pick` and `world.reset` on the same connection would interleave physically incoherent actions. Multi-client = explicit "this is a different agent talking to me."

**The Phase-2 seam.** `_handle_connection`'s body is structured so the inner `await self._dispatch_msg(...)` can be replaced with `asyncio.create_task(self._dispatch_msg(...))` + a `request_id` → `writer queue` router. Registry / handler API doesn't change.

### 5.4 Handler signature: `async def (args, ctx) -> ToolSuccess | ToolFailure`

**Decision.** Every registered handler has the shape:

```python
HandlerFn = Callable[[dict[str, Any], CallContext], Awaitable[ToolSuccess | ToolFailure]]
```

The `args` dict is the JSON-decoded `Request::RunTool.args` (already type-validated against `definition.args_schema` if a schema is present — see §5.5). The `ctx` is the §4 `CallContext`. Sync handlers (rare in cbrain — simulator I/O is async anyway) are rejected at registration time with `TypeError("handler must be async (use `async def`)")`. We don't auto-wrap sync handlers — silent `loop.run_in_executor` wrapping hides blocking calls that stall the reactor.

**Tier-aware deadline.** `dispatch.run_tool` does:

```python
deadline_s = _tier_to_deadline(definition.tier)
try:
    result = await asyncio.wait_for(handler(args, ctx), timeout=deadline_s)
except asyncio.TimeoutError:
    return _build_error_response(ctx.request_id, code=1003, message=f"tool exceeded {tier} tier deadline ({deadline_s}s)")
```

`_tier_to_deadline` (in `dispatch.py`):

```python
_TIER_DEADLINES_S: dict[ToolTier, float] = {
    ToolTier.HOT: 1.0,
    ToolTier.WARM: 30.0,
    ToolTier.COLD: 300.0,
}
```

Adopters can override per-server via `AtdServer(..., tier_deadlines={ToolTier.WARM: 60.0})`. Per-call override (Phase 2 wire field) is `SP-cancel-streaming-v1`'s concern.

**Why the table, not "frame deadline."** The Rust runtime today bundles "wire frame timeout" with "tool execution timeout." Splitting them in Python is correct: a tool that legitimately takes 20s under `WARM` shouldn't trip a 5s wire-frame timeout. Wire frames in Python have their own (longer, 30s) read timeout; tool execution has the tier deadline.

### 5.5 Tool registry: visibility-aware list, schema-aware describe

**Decision.** `ToolRegistry`:

```python
class ToolRegistry:
    def __init__(self) -> None:
        self._tools: dict[str, _RegisteredTool] = {}    # tool_id → (definition, handler)

    def register(self, definition: ToolDefinition, handler: HandlerFn) -> None:
        _validate_tool_id(definition.id)
        if definition.id in self._tools:
            raise ValueError(f"duplicate tool id: {definition.id}")
        if not asyncio.iscoroutinefunction(handler):
            raise TypeError(f"handler for {definition.id} must be async (got {type(handler).__name__})")
        self._tools[definition.id] = _RegisteredTool(definition=definition, handler=handler)

    def summaries(self, *, include_hidden: bool) -> list[ToolSummary]:
        out: list[ToolSummary] = []
        for t in self._tools.values():
            if t.definition.visibility == ToolVisibility.HIDDEN and not include_hidden:
                continue
            out.append(_definition_to_summary(t.definition))
        return out

    def describe(self, tool_id: str) -> ToolDefinition | None:
        return self._tools.get(tool_id, _RegisteredTool(None, None)).definition

    def get(self, tool_id: str) -> _RegisteredTool | None:
        return self._tools.get(tool_id)
```

**Visibility rule** (mirrors `sp-tool-visibility-hidden`): `tool_list` default omits `HIDDEN` tools; clients pass `{"include_hidden": true}` (Phase 2 wire field — not in v0.1.0 spec) to reveal them. For v1, the registry honors `include_hidden=False` always; `include_hidden=True` is only used by internal calls (e.g., dispatch checks `get(tool_id)` directly so hidden tools are still callable via direct id).

**Args validation.** If `definition.args_schema` is a JSON Schema (typed as `dict[str, Any]` in `atd_client.types`), the dispatcher validates `args` against it via `jsonschema` (a new dep — Python-only, doesn't affect Rust crates). On schema violation, return `1002 invalid_arguments`. If schema is None, args are passed through. Adopters who want stricter validation use `attrs` / `pydantic` themselves inside the handler.

### 5.6 Middleware: stage-ordered async wrappers with a `call_next` continuation

**Decision.** Three stages: `pre_call`, `post_call`, `on_error`. Each registered handler is async; they fire in registration order; the call_next-continuation pattern lets a middleware skip the handler entirely.

```python
MiddlewareStage = Literal["pre_call", "post_call", "on_error"]
MiddlewareFn = Callable[..., Awaitable[Any]]  # signature varies by stage; see below

class AtdServer:
    def middleware(self, *, stage: MiddlewareStage) -> Callable[[MiddlewareFn], MiddlewareFn]:
        def decorator(fn: MiddlewareFn) -> MiddlewareFn:
            self._middleware[stage].append(fn)
            return fn
        return decorator
```

**Stage signatures:**

```python
# pre_call: can short-circuit by returning a ToolFailure / ToolSuccess
async def pre_call_mw(
    request: dict,        # raw run_tool message
    ctx: CallContext,
    call_next: Callable[[], Awaitable[ToolSuccess | ToolFailure]],
) -> ToolSuccess | ToolFailure: ...

# post_call: receives the response, can mutate or replace
async def post_call_mw(
    request: dict,
    ctx: CallContext,
    call_next: Callable[[], Awaitable[ToolSuccess | ToolFailure]],
) -> ToolSuccess | ToolFailure: ...

# on_error: receives the exception; return ToolFailure to suppress, or re-raise
async def on_error_mw(
    request: dict,
    ctx: CallContext,
    exc: Exception,
) -> ToolFailure | None: ...   # None = re-raise
```

**Execution.** The dispatcher builds a chain:

```python
async def run() -> ToolSuccess | ToolFailure:
    return await handler(args, ctx)

# Wrap with post_call middlewares (innermost first → outermost last)
for mw in reversed(self._middleware["post_call"]):
    run = _wrap_post(mw, request, ctx, run)
# Wrap with pre_call middlewares
for mw in reversed(self._middleware["pre_call"]):
    run = _wrap_pre(mw, request, ctx, run)

try:
    return await run()
except Exception as e:
    for mw in self._middleware["on_error"]:
        suppressed = await mw(request, ctx, e)
        if suppressed is not None:
            return suppressed
    raise
```

**Why `call_next` not `next(req)`-style.** The continuation pattern (à la ASP.NET Core / Express.js middleware) makes "skip the handler" a single early `return` instead of a special `request.short_circuit = True` flag. Adopters who've used FastAPI / Starlette will recognize it.

**Adopter use cases:**
- cbrain's Merkle audit (P2-8 from the requirements doc): `post_call` middleware that appends `(request_hash, response_hash)` to a chain.
- Rate limiting: `pre_call` that returns `1004 rate_limited` if a token bucket is empty.
- Tracing: `pre_call` that opens an OpenTelemetry span and `post_call` that closes it (via a contextvar bridge).

### 5.7 Error envelope: `ToolError` exception + explicit `ToolFailure` return + generic fallback

**Decision.** Three paths from handler back to wire:

| Handler does | Server response |
|---|---|
| `return ToolSuccess(data=..., metadata=...)` | `Response::ToolResult { request_id, success: <data> }` |
| `return ToolFailure(error_code=N, error_message=msg, ...)` | `Response::ToolResult { request_id, failure: <envelope> }` |
| `raise ToolError(code=N, message=msg, partial_data=...)` | `Response::ToolResult { request_id, failure: <envelope from exc> }` |
| `raise <any other Exception>` | `Response::ToolResult { request_id, failure: { code: 1099, message: "internal_error: <ExcClass>" } }` |
| `asyncio.TimeoutError` (from tier deadline wrap) | `Response::ToolResult { request_id, failure: { code: 1003, message: "tool exceeded <tier> deadline (<s>s)" } }` |
| Capability denied (pre-dispatch) | `Response::Error { code: 1001, message: "capability denied: <missing>" }` |
| Tool not found (pre-dispatch) | `Response::Error { code: 1000, message: "tool not found: <id>" }` |
| Args invalid (schema check) | `Response::Error { code: 1002, message: "invalid arguments: <jsonschema-msg>" }` |

`ToolError`:

```python
class ToolError(Exception):
    def __init__(
        self,
        code: int,
        message: str,
        *,
        partial_data: dict[str, Any] | None = None,
        details: dict[str, Any] | None = None,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.partial_data = partial_data
        self.details = details
```

Adopters who want adopter-namespace error codes (P1-6) use `ToolError(code=2001, message="cbrain:perception failed")` — server passes the code through without validation in v1. `SP-error-namespace-v1` adds validation against the publisher namespace.

**Why no tracebacks on the wire.** Tracebacks can leak filesystem paths, dependency versions, and sometimes secrets in repr'd state. Server logs the full traceback (Python `logging` at ERROR level); the wire carries only `code` + `message` + the exception class name.

### 5.8 Lifecycle: bind → serve → drain → close, with graceful SIGTERM

**Decision.** `AtdServer.serve()` installs `SIGTERM` / `SIGINT` → `self.stop()` handlers. `stop()`:

```python
async def stop(self, *, drain_timeout_s: float = 5.0) -> None:
    self._stop_event.set()
    await self._transport.close()    # stop accepting new connections
    # let in-flight handlers finish
    if self._connection_tasks:
        _, pending = await asyncio.wait(self._connection_tasks, timeout=drain_timeout_s)
        for task in pending:
            task.cancel()
        await asyncio.gather(*self._connection_tasks, return_exceptions=True)
```

Drain timeout of 5s matches the Rust ref-server's `SIGTERM` behavior. Adopters running long tools (cbrain's `COLD` tier can be 5min) should set `drain_timeout_s` accordingly.

**Why `asyncio.wait` not `asyncio.gather`.** `wait` gives us a `(done, pending)` split so we know which connections to forcibly cancel. `gather(*, return_exceptions=True)` then collects the cancellation results without raising.

**Signal handler caveat.** `loop.add_signal_handler` is Unix-only. On Windows (no Unix sockets anyway), `AtdServer.serve()` skips signal installation and relies on `await server.stop()` from the embedding application.

## 6. Wire / API impact

**Wire format: zero change.** Python server speaks the same v0.1.0 wire as the Rust server. All message types, field names, and error codes match `crates/atd-protocol` and `docs/protocol/wire-format.md`. cbrain shim → upstream switch is import-line-only.

**Python public API additions:**

- `atd_server.AtdServer` (constructor, `register`, `middleware`, `serve`, `stop`).
- `atd_server.CallContext` / `ConnectionContext` (frozen dataclasses).
- `atd_server.ToolError`.
- `atd_server.ServerPolicy` Protocol + `default_policy()`.
- `atd_server.GrantedCapabilities` (frozen dataclass: `capabilities: frozenset[ToolCapability]`, `server_id: str`).

**No changes to `atd_client`.** Reusing `atd_client.wire.{read_frame, write_frame}` and `atd_client.protocol.*` constants is import-only; both packages stay independently usable.

**New Python deps:** `jsonschema` (for §5.5 args validation). MIT-licensed, widely used, transitive deps minimal. Optional install via extras: `pip install atd-server[validation]` — if not installed, schema validation is skipped with a single warn-once log. Hard dep would be friendlier; we'll likely make it hard once we see adoption.

## 7. Migration / adopter notes

**cbrain.** Three swap steps when v1 alpha lands:

1. Add `atd-server = { path = "../atd-mvp/python" }` to `cbrain/sim/cbrain_sim/pyproject.toml` (or equivalent uv add).
2. Replace `from cbrain.sim.atd_shim import AtdServer` with `from atd_server import AtdServer`.
3. Delete `cbrain/sim/cbrain_sim/atd_shim/`.

Handlers and middleware require no changes if cbrain followed the §9.3 shim guidance in the cbrain requirements doc (handler signature, middleware stage names, byte-compat wire). Expected swap effort: ~2 hours.

**healthkit_cli / celia_phr.** No impact. Both use Rust `atd-server` / `atd-server-http`; the Python package is a new optional thing.

**Other Python adopters not yet known.** Documented in `docs/integrations/python-server.md` with the cbrain-style example so future adopters discover the runtime via the integrations index.

## 8. Open questions

**Q1: should `tier_deadlines` be definition-level or server-level?** Today the SP puts overrides at server-level (`AtdServer(..., tier_deadlines={...})`). Definition-level (`ToolDefinition(deadline_ms=15000)` — already a protocol field considered for `SP-pagination-v1` but not landed) is more granular. **Decision:** server-level only in v1; promote to definition-level if/when the protocol-level field lands. Don't paint adopters into a corner with a Python-only definition-level field.

**Q2: should middleware see `Response::Error` (capability denied / tool not found) the same way it sees `ToolFailure`?** Currently no — capability denial returns a top-level `Response::Error` before any handler-pathway middleware fires. cbrain's Merkle audit will miss capability-denied calls. **Decision:** add `pre_dispatch` middleware stage in `SP-server-py-v2` if needed; v1 ships the three stages above. The rationale: capability-denied is a *protocol-level* event, not a *tool execution* event; middleware for the latter shouldn't accidentally observe the former.

**Q3: should `ServerPolicy` see the UCAN tokens?** Yes. The policy callback signature is `async def policy(hello: HelloMessage, ucan_tokens: tuple[str, ...]) -> GrantedCapabilities`. Adopters that verify UCAN can run their verifier inside the policy and let it shape the granted set. Adopters that don't verify ignore the param.

**Q4: should the server log to `stderr` by default, or stay silent?** Stay silent (Python `logging`-only). Adopters call `logging.basicConfig(level=logging.INFO)` to opt in. CLAUDE.md python rules ban `print()` for logging anyway. The one exception: a single `stderr` line on graceful shutdown (`atd-server: stopped (drained N connections)`) so operators don't think the process hung.

**Q5: how do we test against the Rust `atd-conformance` fixtures without re-implementing the runner in Python?** v1 includes a `python/tests/test_server_conformance.py` that reads fixture JSON files directly from `crates/atd-conformance/fixtures/` and exercises the protocol surface. Approximately 10 of 36 fixtures cover the surface this SP delivers (handshake, list, schema, dispatch, capability denial, dry-run). The remaining 26 (rate limit, pagination, UCAN signatures, audit invariants) wait for the matching Python implementations to land.

**Q6: should the `default_policy()` be `grant all requested` or `grant none, require explicit policy`?** `grant all requested` matches `atd-ref-server`'s today behavior and is the friendlier default for adopters writing internal tools. Production deployments must pass a real policy — we document this loudly in `docs/integrations/python-server.md`.

**Q7: do we need an `AtdServerSync` (threading-based) sibling for adopters not using asyncio?** cbrain is asyncio-native; healthkit_cli is Rust. No identified adopter needs sync today. Defer until requested. The existing `atd_client.sync.AtdClientSync` is mostly a wrapper around the async client; a similar `atd_server.sync.AtdServerSync` is straightforward when needed.

## 9. Phasing

Detailed task list lives in `docs/superpowers/plans/2026-05-19-sp-server-py-v1.md`. High-level phases:

- **Phase A** (this spec): land. Tag: `sp-server-py-v1-spec`.
- **Phase B**: skeleton. `atd_server/` package, `UnixSocketTransport`, accept loop, frame echo. Health-check: `nc -U /tmp/foo.sock` round-trips a ping. Tag: `sp-server-py-v1-phase-b`.
- **Phase C**: handshake + `ServerPolicy` + UCAN passthrough seam. `HelloAck` returns server_id + granted_capabilities. Tag: `sp-server-py-v1-phase-c`.
- **Phase D**: registry + `tool_list` + `tool_schema` + visibility. Tag: `sp-server-py-v1-phase-d`.
- **Phase E**: dispatch + tier deadline + `dry_run` + capability gate + error envelope. Tag: `sp-server-py-v1-phase-e`.
- **Phase F**: middleware (`pre_call` / `post_call` / `on_error`). Tag: `sp-server-py-v1-phase-f`.
- **Phase G**: tests + Python conformance subset. `pytest python/tests/test_server_*` green; ≥80% coverage. Tag: `sp-server-py-v1-phase-g`.
- **Phase H**: docs (`docs/integrations/python-server.md` + `docs/architecture.md` §8 row + `python/README.md` server section) + umbrella tag `sp-server-py-v1`.

Expected effort: 5-7 person-days for one Python-comfortable developer (~1 day per non-trivial phase + 0.5 for the spec land + tests). cbrain unblocks at Phase E (B-E ship a usable alpha; F-H polish).
