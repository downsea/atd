"""AtdServer — phase-D registry + tool_list / tool_schema dispatch.

What this phase delivers on top of Phase C:
- `@server.register(definition=...)` decorator wires a handler into the
  registry. Phase E will actually invoke the handler from `run_tool`.
- `tool_list` returns visible summaries; `tool_schema` returns the full
  definition or `1000 not_found`.
- Hidden tools (`visibility == ToolVisibility.HIDDEN`) are excluded from
  `tool_list` but reachable by id via `tool_schema` / `run_tool`.
- `_drain_and_close` log math: snapshot `total` BEFORE `asyncio.wait`
  removes done tasks from the set (via `done_callback`). Previous
  formulation `len(self._connection_tasks) - len(pending)` could go
  negative when all tasks discarded themselves before the log line.

`middleware` is still a stub (Phase F). `run_tool` returns a Phase-E
placeholder error until Phase E lands.
"""

from __future__ import annotations

import asyncio
import contextlib
import logging
from typing import Any

from atd_client.types import ToolDefinition
from atd_client.wire import read_frame, write_frame
from atd_server._runtime import install_signal_handlers
from atd_server.adapters import Transport
from atd_server.adapters.unix import UnixSocketTransport
from atd_server.context import ConnectionContext
from atd_server.handshake import negotiate_hello
from atd_server.policy import ServerPolicy, default_policy
from atd_server.registry import HandlerFn, ToolRegistry

_log = logging.getLogger("atd_server")

_DEFAULT_SUPPORTED_TIERS: tuple[str, ...] = ("hot", "warm", "cold")
_ERR_TOOL_NOT_FOUND = 1000
_ERR_NOT_IMPLEMENTED = 1099


class AtdServer:
    """Async ATD server runtime.

    Construct with either `socket_path` (a UDS path; we build a
    `UnixSocketTransport` for you) or `transport` (explicit; e.g. a custom
    adapter). Mutually exclusive.

    Register tools with `@server.register(definition=ToolDefinition(...))`.
    Phase E adds the dispatch loop that actually calls the handlers.

    Lifecycle:
        server = AtdServer(socket_path="/tmp/foo.sock", server_id="demo")
        await server.serve()         # blocks until SIGTERM / server.stop()

    Tests usually do:
        task = asyncio.create_task(server.serve())
        await server.wait_until_serving()
        # ... drive the server ...
        await server.stop()
        await task
    """

    def __init__(
        self,
        *,
        socket_path: str | None = None,
        transport: Transport | None = None,
        server_id: str = "atd-server-py",
        server_version: str = "atd-server-py/0.0.1",
        supported_tiers: tuple[str, ...] = _DEFAULT_SUPPORTED_TIERS,
        policy: ServerPolicy | None = None,
    ) -> None:
        if (socket_path is None) == (transport is None):
            raise ValueError("exactly one of `socket_path` or `transport` must be set")
        self._transport: Transport = (
            transport if transport is not None else UnixSocketTransport(socket_path or "")
        )
        self.server_id = server_id
        self.server_version = server_version
        self.supported_tiers = supported_tiers
        self._policy: ServerPolicy = policy if policy is not None else default_policy

        self._registry = ToolRegistry()
        self._stop_event = asyncio.Event()
        self._serving_event = asyncio.Event()
        self._connection_tasks: set[asyncio.Task[None]] = set()
        self._started = False

    # ------------------------------------------------------------------ Registration

    def register(
        self, *, definition: ToolDefinition
    ) -> Any:
        """Decorator: register an `async def handler(args, ctx)` against a definition.

        Returns the handler unchanged so the decorator is transparent.

        Raises:
            ValueError: duplicate `definition.id` or empty id.
            TypeError:  handler is not an async function.
        """

        def decorator(handler: HandlerFn) -> HandlerFn:
            self._registry.register(definition, handler)
            return handler

        return decorator

    def middleware(self, *args: Any, **kwargs: Any) -> Any:
        raise NotImplementedError("middleware lands in SP-server-py-v1 Phase F")

    # ------------------------------------------------------------------ Lifecycle

    async def serve(self) -> None:
        if self._started:
            raise RuntimeError("AtdServer.serve() called more than once")
        self._started = True

        await self._transport.start(self._handle_connection)
        loop = asyncio.get_running_loop()
        install_signal_handlers(loop, self._signal_stop)
        self._serving_event.set()
        _log.info(
            "atd-server listening (server_id=%s, tools=%d)",
            self.server_id,
            len(self._registry),
        )

        try:
            await self._stop_event.wait()
        finally:
            await self._drain_and_close()

    async def wait_until_serving(self, *, timeout: float = 2.0) -> None:
        """Block until `serve()` has bound the transport. Useful in tests."""
        await asyncio.wait_for(self._serving_event.wait(), timeout=timeout)

    async def stop(self, *, drain_timeout_s: float = 5.0) -> None:
        """Trigger graceful shutdown. Safe to call multiple times."""
        self._stop_event.set()
        self._drain_timeout_s = drain_timeout_s

    def _signal_stop(self) -> None:
        _log.info("atd-server received stop signal")
        self._stop_event.set()

    async def _drain_and_close(self) -> None:
        drain_timeout = getattr(self, "_drain_timeout_s", 5.0)
        await self._transport.close()
        # Snapshot BEFORE asyncio.wait: done_callback.discard runs synchronously
        # when each task completes, mutating self._connection_tasks. Without this
        # snapshot, the post-wait `len()` computation would underflow.
        in_flight = list(self._connection_tasks)
        if not in_flight:
            _log.info("atd-server stopped (no in-flight connections)")
            return

        total = len(in_flight)
        _, pending = await asyncio.wait(in_flight, timeout=drain_timeout)
        for task in pending:
            task.cancel()
        if pending:
            await asyncio.gather(*pending, return_exceptions=True)
        _log.info(
            "atd-server stopped (drained %d connections, %d forced)",
            total - len(pending),
            len(pending),
        )

    # ------------------------------------------------------------------ Per-connection dispatch

    async def _handle_connection(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
        remote: str,
    ) -> None:
        task = asyncio.current_task()
        if task is not None:
            self._connection_tasks.add(task)
            task.add_done_callback(self._connection_tasks.discard)
        try:
            await self._serve_one_connection(reader, writer, remote)
        except asyncio.IncompleteReadError:
            return
        except asyncio.CancelledError:
            raise
        except Exception:
            _log.exception("unexpected error in connection handler (remote=%s)", remote)
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()

    async def _serve_one_connection(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
        remote: str,
    ) -> None:
        """Read → dispatch → write loop. Strictly serial within one connection."""
        ctx = ConnectionContext(remote_addr=remote)
        while not self._stop_event.is_set():
            msg = await read_frame(reader)
            if not isinstance(msg, dict):
                await write_frame(writer, _error("invalid frame: expected JSON object"))
                continue
            response, ctx = await self._dispatch(msg, ctx)
            await write_frame(writer, response)

    async def _dispatch(
        self,
        msg: dict[str, Any],
        ctx: ConnectionContext,
    ) -> tuple[dict[str, Any], ConnectionContext]:
        msg_type = msg.get("type")
        if msg_type == "ping":
            return {"type": "pong"}, ctx
        if msg_type == "hello":
            ack, ctx = await negotiate_hello(
                msg,
                current_ctx=ctx,
                policy=self._policy,
                server_version=self.server_version,
                supported_tiers=self.supported_tiers,
            )
            return ack, ctx
        if msg_type == "tool_list":
            summaries = self._registry.summaries(include_hidden=False)
            return {
                "type": "tool_list",
                "tools": [s.model_dump(mode="json") for s in summaries],
            }, ctx
        if msg_type == "tool_schema":
            tool_id = msg.get("tool_id")
            if not isinstance(tool_id, str) or not tool_id:
                return _error("tool_schema requires a non-empty `tool_id`"), ctx
            defn = self._registry.describe(tool_id)
            if defn is None:
                return _error(
                    f"tool not found: {tool_id}",
                    code=_ERR_TOOL_NOT_FOUND,
                ), ctx
            return {"type": "tool_schema", "schema": defn.model_dump(mode="json")}, ctx
        if msg_type == "run_tool":
            return _error("run_tool lands in SP-server-py-v1 Phase E"), ctx
        # Phase D/E/F replace these placeholders with real dispatch.
        return _error(f"{msg_type!r} not implemented in SP-server-py-v1 phase D"), ctx


def _error(
    message: str,
    *,
    code: int = _ERR_NOT_IMPLEMENTED,
    retryable: bool = False,
) -> dict[str, Any]:
    return {"type": "error", "code": code, "message": message, "retryable": retryable}
