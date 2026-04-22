"""Contract test: the Python SDK must parse the same live-ANOS responses that
the Rust SDK parses. Fixtures live in the Rust crate tree (single source of
truth); refresh with `scripts/capture_anos_fixtures.sh`."""

from __future__ import annotations

import json
from collections.abc import Callable
from pathlib import Path
from typing import Any

from atd_client import AtdClient

_FIXTURE_DIR = Path(__file__).resolve().parents[2] / "crates" / "atd-client" / "tests" / "fixtures"


def _load(name: str) -> dict[str, Any]:
    with (_FIXTURE_DIR / name).open("r", encoding="utf-8") as f:
        return json.load(f)


async def test_discover_against_real_anos_tool_list_fixture(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    tool_list = _load("anos_tool_list.json")
    assert tool_list["type"] == "tool_list"
    tool_count = len(tool_list["tools"])
    assert tool_count >= 50, f"fixture should have many tools, got {tool_count}"

    def handler(req: dict[str, Any]) -> dict[str, Any]:
        if req.get("type") == "ping":
            return {"type": "pong"}
        if req.get("type") == "tool_list":
            return tool_list
        return {"type": "error", "message": "unexpected"}

    sock: Path = await mock_server(handler)
    client = await AtdClient.connect(sock)
    try:
        summaries = await client.discover()
        assert len(summaries) >= 50
        fs_read = next((s for s in summaries if s.id == "anos:fs.read"), None)
        assert fs_read is not None, "fixture must contain anos:fs.read"
        assert fs_read.domain == "fs"
        assert fs_read.name, "name should be derived from description or id"
    finally:
        await client.close()


async def test_describe_against_real_anos_tool_schema_fixture(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    tool_schema = _load("anos_tool_schema_fs_read.json")
    assert tool_schema["type"] == "tool_schema"

    def handler(req: dict[str, Any]) -> dict[str, Any]:
        if req.get("type") == "ping":
            return {"type": "pong"}
        if req.get("type") == "tool_schema":
            return tool_schema
        return {"type": "error", "message": "unexpected"}

    sock: Path = await mock_server(handler)
    client = await AtdClient.connect(sock)
    try:
        d = await client.describe("anos:fs.read")
        assert d.id == "anos:fs.read"
        assert d.capability.domain == "fs"
        assert d.bindings, "expected at least one binding"
    finally:
        await client.close()
