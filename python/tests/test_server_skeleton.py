"""Phase B skeleton tests for AtdServer.

Covers: bind / accept / graceful stop / drain timing / lifecycle invariants.
Protocol-level round-trip semantics (ping/pong, Hello negotiation) live in
`test_server_handshake.py`. Phase B's throwaway frame-echo handler was
replaced by Phase C's dispatch loop; the round-trip test moved with it.
"""

from __future__ import annotations

import asyncio
import time
from pathlib import Path

import pytest

from atd_server import AtdServer, UnixSocketTransport


async def _spawn(server: AtdServer) -> asyncio.Task[None]:
    task = asyncio.create_task(server.serve())
    await server.wait_until_serving()
    return task


async def test_stop_drains_quickly_with_no_clients(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await _spawn(server)
    t0 = time.perf_counter()
    await server.stop()
    await asyncio.wait_for(task, timeout=2.0)
    assert time.perf_counter() - t0 < 1.0


async def test_serve_twice_raises(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await _spawn(server)
    try:
        with pytest.raises(RuntimeError, match="more than once"):
            await server.serve()
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_constructor_requires_exactly_one_of_path_or_transport(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    with pytest.raises(ValueError, match="exactly one"):
        AtdServer()
    with pytest.raises(ValueError, match="exactly one"):
        AtdServer(socket_path=sock, transport=UnixSocketTransport(sock))


async def test_unlink_existing_clears_stale_socket(tmp_path: Path) -> None:
    sock_path = tmp_path / "atd.sock"
    sock_path.touch()  # simulate stale socket file
    server = AtdServer(socket_path=str(sock_path))
    task = await _spawn(server)
    try:
        # If start() had failed, wait_until_serving would have timed out.
        assert sock_path.exists()
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_register_and_middleware_are_phase_d_f_stubs(tmp_path: Path) -> None:
    server = AtdServer(socket_path=str(tmp_path / "atd.sock"))
    with pytest.raises(NotImplementedError, match="Phase D"):
        server.register()
    with pytest.raises(NotImplementedError, match="Phase F"):
        server.middleware()


async def test_partial_frame_closes_cleanly(tmp_path: Path) -> None:
    """Client connects then disconnects mid-header → no crash, drains."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await _spawn(server)
    try:
        _, writer = await asyncio.open_unix_connection(sock)
        writer.write(b"\x00\x00")  # 2 bytes of a 4-byte header, then close
        await writer.drain()
        writer.close()
        await writer.wait_closed()
        # Give the server a tick to observe the disconnect.
        await asyncio.sleep(0.05)
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)
