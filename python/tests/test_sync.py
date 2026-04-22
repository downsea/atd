from __future__ import annotations

from collections.abc import Callable
from pathlib import Path
from typing import Any

from atd_client import AtdClientSync, ToolSuccess


def _handler(req: dict[str, Any]) -> dict[str, Any]:
    t = req.get("type")
    if t == "ping":
        return {"type": "pong"}
    if t == "tool_list":
        return {
            "type": "tool_list",
            "tools": [
                {"id": "anos:fs.read", "description": "r", "tier": "hot", "visibility": "read"}
            ],
        }
    if t == "run_tool":
        return {
            "type": "tool_result",
            "tool_id": req["tool_id"],
            "result": {"ok": True},
            "success": True,
            "dry_run": False,
        }
    return {"type": "error", "message": "no"}


async def test_sync_wrapper_discover_and_call(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock: Path = await mock_server(_handler)

    # Although this test is async (so it can use the async fixture to spin the
    # server), we use the sync client inside a thread to verify it works from
    # a fully synchronous caller.
    import asyncio

    def run_sync_work() -> tuple[int, Any]:
        client = AtdClientSync.connect(sock)
        try:
            tools = client.discover()
            result = client.call("anos:fs.read", {})
        finally:
            client.close()
        return len(tools), result

    count, result = await asyncio.to_thread(run_sync_work)
    assert count == 1
    assert isinstance(result, ToolSuccess)
    assert result.data == {"ok": True}
