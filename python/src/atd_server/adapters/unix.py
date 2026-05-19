"""UnixSocketTransport — wraps `asyncio.start_unix_server` for AtdServer."""

from __future__ import annotations

import asyncio
import contextlib
import os

from atd_server.adapters import ConnectionHandler


class UnixSocketTransport:
    """Listen on a Unix domain socket path.

    `unlink_existing` (default True) removes a stale socket file before
    binding. A second live process listening on the same path will still cause
    `start()` to raise `OSError(EADDRINUSE)`.
    """

    def __init__(self, socket_path: str, *, unlink_existing: bool = True) -> None:
        self._socket_path = socket_path
        self._unlink_existing = unlink_existing
        self._server: asyncio.AbstractServer | None = None

    @property
    def socket_path(self) -> str:
        return self._socket_path

    async def start(self, on_connection: ConnectionHandler) -> None:
        # ASYNC240 noqa: bind-time, one-shot syscall on a known path; not hot-path I/O.
        if self._unlink_existing and os.path.exists(self._socket_path):  # noqa: ASYNC240
            os.unlink(self._socket_path)

        async def _cb(reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
            await on_connection(reader, writer, self._socket_path)

        self._server = await asyncio.start_unix_server(_cb, path=self._socket_path)

    async def close(self) -> None:
        if self._server is None:
            return
        self._server.close()
        with contextlib.suppress(Exception):
            await self._server.wait_closed()
        self._server = None
        if self._unlink_existing:
            with contextlib.suppress(FileNotFoundError):
                os.unlink(self._socket_path)
