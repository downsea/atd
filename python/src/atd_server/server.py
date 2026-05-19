"""AtdServer — phase-B skeleton.

What this phase delivers:
- Construct with a UDS path or an explicit Transport.
- `serve()` binds, awaits a stop signal, drains in-flight connections.
- `stop()` triggers graceful shutdown (with a drain timeout).
- Per-connection handler echoes one frame back and closes (placeholder —
  Phase C replaces with the Hello / state-machine logic).
- `register` / `middleware` raise NotImplementedError; Phase D / F wire them.

Wire format reuses `atd_client.wire.{read_frame, write_frame}` to guarantee
byte-compat with the Rust ref-server and the cbrain shim from day one.
"""

from __future__ import annotations

import asyncio
import contextlib
import logging
from typing import Any

from atd_client.wire import read_frame, write_frame
from atd_server._runtime import install_signal_handlers
from atd_server.adapters import Transport
from atd_server.adapters.unix import UnixSocketTransport

_log = logging.getLogger("atd_server")


class AtdServer:
    """Async ATD server runtime.

    Construct with either `socket_path` (a UDS path; we build a
    `UnixSocketTransport` for you) or `transport` (explicit; e.g. a custom
    adapter). Mutually exclusive.

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
    ) -> None:
        if (socket_path is None) == (transport is None):
            raise ValueError("exactly one of `socket_path` or `transport` must be set")
        self._transport: Transport = (
            transport if transport is not None else UnixSocketTransport(socket_path or "")
        )
        self.server_id = server_id

        self._stop_event = asyncio.Event()
        self._serving_event = asyncio.Event()
        self._connection_tasks: set[asyncio.Task[None]] = set()
        self._started = False

    # ------------------------------------------------------------------ Phase D / F stubs

    def register(self, *args: Any, **kwargs: Any) -> Any:
        raise NotImplementedError("register lands in SP-server-py-v1 Phase D")

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
        _log.info("atd-server listening (server_id=%s)", self.server_id)

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
        # Called from signal handler; sync entry into async stop.
        _log.info("atd-server received stop signal")
        self._stop_event.set()

    async def _drain_and_close(self) -> None:
        drain_timeout = getattr(self, "_drain_timeout_s", 5.0)
        await self._transport.close()
        if not self._connection_tasks:
            _log.info("atd-server stopped (no in-flight connections)")
            return

        _, pending = await asyncio.wait(self._connection_tasks, timeout=drain_timeout)
        for task in pending:
            task.cancel()
        if pending:
            await asyncio.gather(*pending, return_exceptions=True)
        _log.info(
            "atd-server stopped (drained %d connections, %d forced)",
            len(self._connection_tasks) - len(pending),
            len(pending),
        )

    # ------------------------------------------------------------------ Per-connection handler (Phase B placeholder)

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
            await self._echo_one_frame(reader, writer)
        except asyncio.IncompleteReadError:
            return
        except Exception:
            _log.exception("unexpected error in connection handler (remote=%s)", remote)
        finally:
            writer.close()
            with contextlib.suppress(Exception):
                await writer.wait_closed()

    async def _echo_one_frame(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        """Phase B placeholder: read one frame, write it back, return.

        Phase C replaces this with the Hello state machine.
        """
        msg = await read_frame(reader)
        await write_frame(writer, msg)
