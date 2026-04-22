"""Synchronous wrapper around :class:`AtdClient`.

Runs a dedicated event loop on a background daemon thread, so sync call sites
(LangChain tool loaders, Jupyter cells, CLI scripts) can drive the async core
without writing ``async def`` or managing loops themselves.
"""

from __future__ import annotations

import asyncio
import threading
from collections.abc import Coroutine
from pathlib import Path
from typing import Any, TypeVar

from atd_client.client import AtdClient
from atd_client.types import (
    ToolDefinition,
    ToolFailure,
    ToolSuccess,
    ToolSummary,
    ToolTier,
    ToolVisibility,
)

_T = TypeVar("_T")


class _LoopThread:
    """A dedicated asyncio event loop running on a daemon thread.

    Use :meth:`submit` to schedule a coroutine and block until it completes.
    """

    def __init__(self) -> None:
        self._loop: asyncio.AbstractEventLoop | None = None
        self._ready = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()
        self._ready.wait()

    def _run(self) -> None:
        loop = asyncio.new_event_loop()
        self._loop = loop
        asyncio.set_event_loop(loop)
        self._ready.set()
        loop.run_forever()

    def submit(self, coro: Coroutine[Any, Any, _T]) -> _T:
        assert self._loop is not None
        fut = asyncio.run_coroutine_threadsafe(coro, self._loop)
        return fut.result()

    def stop(self) -> None:
        if self._loop is not None and not self._loop.is_closed():
            self._loop.call_soon_threadsafe(self._loop.stop)
            self._thread.join(timeout=1.0)


class AtdClientSync:
    """Synchronous façade. Internally drives an :class:`AtdClient` on a
    dedicated background-thread event loop. Not thread-safe for concurrent
    calls from multiple threads — use separate instances if you need that.
    """

    _loop: _LoopThread
    _inner: AtdClient

    def __init__(self, loop: _LoopThread, inner: AtdClient) -> None:
        self._loop = loop
        self._inner = inner

    @classmethod
    def connect(cls, sock: Path | str | None = None) -> AtdClientSync:
        loop = _LoopThread()
        inner = loop.submit(AtdClient.connect(sock))
        return cls(loop, inner)

    def close(self) -> None:
        try:
            self._loop.submit(self._inner.close())
        finally:
            self._loop.stop()

    def discover(
        self,
        query: str | None = None,
        *,
        domain: str | None = None,
        tier: ToolTier | None = None,
        visibility: ToolVisibility | None = None,
        limit: int | None = None,
    ) -> list[ToolSummary]:
        return self._loop.submit(
            self._inner.discover(
                query,
                domain=domain,
                tier=tier,
                visibility=visibility,
                limit=limit,
            )
        )

    def describe(self, tool_id: str) -> ToolDefinition:
        return self._loop.submit(self._inner.describe(tool_id))

    def call(
        self,
        tool_id: str,
        args: Any = None,
        *,
        dry_run: bool = False,
    ) -> ToolSuccess | ToolFailure:
        return self._loop.submit(self._inner.call(tool_id, args, dry_run=dry_run))

    def __enter__(self) -> AtdClientSync:
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()
