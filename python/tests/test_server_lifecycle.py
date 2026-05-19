"""Phase D lifecycle tests: drain semantics + the log-counter bug fix.

These tests catch the specific `_drain_and_close` counter regression where
`len(self._connection_tasks) - len(pending)` went negative because
`done_callback` discarded done tasks synchronously during `asyncio.wait`.
"""

from __future__ import annotations

import asyncio
import contextlib
import time
from pathlib import Path

import pytest

from atd_client.wire import read_frame, write_frame
from atd_server import AtdServer

from ._helpers import spawn, stop_and_wait


async def test_drain_with_idle_connection_reports_correct_counts(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    """A connection that ping-ponged then idled gets force-cancelled on stop.

    Pre-fix bug: the "stopped (drained N connections, M forced)" log line
    computed `len(self._connection_tasks) - len(pending)` AFTER asyncio.wait
    had already discarded done tasks via add_done_callback → underflow. This
    test fails (negative count in the log message) if the fix regresses.
    """
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await spawn(server)
    caplog.set_level("INFO", logger="atd_server")

    # Connect + one ping/pong, then leave the connection open (idle, blocked
    # in read_frame on the server side).
    reader, writer = await asyncio.open_unix_connection(sock)
    try:
        await write_frame(writer, {"type": "ping"})
        assert await asyncio.wait_for(read_frame(reader), timeout=2.0) == {"type": "pong"}

        t0 = time.perf_counter()
        await server.stop(drain_timeout_s=0.1)
        await asyncio.wait_for(task, timeout=2.0)
        elapsed = time.perf_counter() - t0
    finally:
        writer.close()
        with contextlib.suppress(Exception):
            await writer.wait_closed()

    # Drain timeout is 0.1s + cancel overhead.
    assert 0.05 < elapsed < 1.5

    stopped_lines = [
        r.message for r in caplog.records if "stopped" in r.message and "drained" in r.message
    ]
    assert stopped_lines, "expected a 'stopped (drained ...)' log entry"
    msg = stopped_lines[-1]
    # The bug produced messages like 'drained -1 connections, 1 forced'.
    assert "drained 0 connections, 1 forced" in msg, msg


async def test_drain_with_no_connections_logs_clean_path(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await spawn(server)
    caplog.set_level("INFO", logger="atd_server")
    await stop_and_wait(server, task)
    quiet_lines = [r.message for r in caplog.records if "no in-flight" in r.message]
    assert quiet_lines, "expected 'no in-flight connections' log"


async def test_drain_completes_when_client_disconnects_cleanly(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
) -> None:
    """A client that disconnects before stop counts as 'drained', not 'forced'."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await spawn(server)
    caplog.set_level("INFO", logger="atd_server")

    reader, writer = await asyncio.open_unix_connection(sock)
    await write_frame(writer, {"type": "ping"})
    await read_frame(reader)
    writer.close()
    await writer.wait_closed()
    # Give the server's connection task a tick to observe the disconnect.
    await asyncio.sleep(0.05)

    await stop_and_wait(server, task)
    # Either "no in-flight" (the cleanup already removed the task) or
    # "drained 1 connections, 0 forced" — both indicate a clean shutdown.
    quiet_or_drained = [
        r.message
        for r in caplog.records
        if "no in-flight" in r.message
        or "drained 1 connections, 0 forced" in r.message
    ]
    assert quiet_or_drained
