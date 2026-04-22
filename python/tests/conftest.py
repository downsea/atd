"""Shared pytest fixtures for the ATD client test suite."""

from __future__ import annotations

import asyncio
import contextlib
import json
import struct
import tempfile
from collections.abc import AsyncIterator, Callable
from pathlib import Path
from typing import Any

import pytest_asyncio


async def _serve_one_client(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    handler: Callable[[dict[str, Any]], dict[str, Any]],
) -> None:
    try:
        while True:
            try:
                header = await reader.readexactly(4)
            except asyncio.IncompleteReadError:
                return
            (length,) = struct.unpack(">I", header)
            body = await reader.readexactly(length)
            req = json.loads(body.decode("utf-8"))
            resp = handler(req)
            out = json.dumps(resp, separators=(",", ":")).encode("utf-8")
            writer.write(struct.pack(">I", len(out)))
            writer.write(out)
            await writer.drain()
    finally:
        writer.close()
        with contextlib.suppress(Exception):
            await writer.wait_closed()


@pytest_asyncio.fixture
async def mock_server() -> AsyncIterator[Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Path]]:
    """Factory that spawns a mock ANOS-like server with the caller's handler.

    Yields a callable ``make(handler) -> socket_path``. Multiple mock servers
    can coexist — each gets its own tempdir. Servers are torn down when the
    outer test ends.
    """
    tempdirs: list[tempfile.TemporaryDirectory[str]] = []
    servers: list[asyncio.Server] = []

    async def make(handler: Callable[[dict[str, Any]], dict[str, Any]]) -> Path:
        d = tempfile.TemporaryDirectory()
        tempdirs.append(d)
        sock_path = Path(d.name) / "mock.sock"

        async def cb(r: asyncio.StreamReader, w: asyncio.StreamWriter) -> None:
            await _serve_one_client(r, w, handler)

        srv = await asyncio.start_unix_server(cb, path=str(sock_path))
        servers.append(srv)
        # Give the event loop a tick so the server's accept task is scheduled.
        await asyncio.sleep(0)
        return sock_path

    try:
        yield make
    finally:
        for s in servers:
            s.close()
            with contextlib.suppress(Exception):
                await s.wait_closed()
        for d in tempdirs:
            d.cleanup()
