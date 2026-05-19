"""Transport adapters for atd-server.

Phase B ships only `UnixSocketTransport`. HTTP / stdio adapters land in
follow-up SPs (`SP-server-py-http-v1`).
"""

from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable
from typing import Protocol

ConnectionHandler = Callable[
    [asyncio.StreamReader, asyncio.StreamWriter, str],
    Awaitable[None],
]


class Transport(Protocol):
    """Pluggable accept-loop transport.

    `start` binds + begins accepting; the supplied `on_connection` callback is
    invoked for every accepted client. `start` returns once the listener is
    ready (so callers can rely on "after `start`, clients may connect").

    `close` stops accepting new connections and releases the underlying
    resource (UDS unlink / HTTP port close / etc.). In-flight per-connection
    tasks are NOT awaited here — `AtdServer._drain_and_close` does that.
    """

    async def start(self, on_connection: ConnectionHandler) -> None: ...

    async def close(self) -> None: ...


__all__ = ["ConnectionHandler", "Transport"]
