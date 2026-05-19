"""Phase D tests for AtdServer: registry + tool_list + tool_schema + visibility."""

from __future__ import annotations

from pathlib import Path

import pytest

from atd_client.types import ToolSummary, ToolVisibility
from atd_server import AtdServer

from ._helpers import make_definition, round_trip, spawn, stop_and_wait


async def test_register_decorator_returns_handler_unchanged(tmp_path: Path) -> None:
    server = AtdServer(socket_path=str(tmp_path / "atd.sock"))

    @server.register(definition=make_definition("demo:echo"))
    async def handler(args: dict, ctx: object) -> dict:
        return {"data": args}

    # decorator transparency
    assert handler.__name__ == "handler"
    assert callable(handler)


async def test_register_rejects_duplicate_id(tmp_path: Path) -> None:
    server = AtdServer(socket_path=str(tmp_path / "atd.sock"))

    @server.register(definition=make_definition("demo:dup"))
    async def first(args: dict, ctx: object) -> dict:
        return {}

    with pytest.raises(ValueError, match="duplicate"):

        @server.register(definition=make_definition("demo:dup"))
        async def second(args: dict, ctx: object) -> dict:
            return {}


async def test_register_rejects_sync_handler(tmp_path: Path) -> None:
    server = AtdServer(socket_path=str(tmp_path / "atd.sock"))

    with pytest.raises(TypeError, match="async"):

        @server.register(definition=make_definition("demo:sync"))
        def sync_handler(args: dict, ctx: object) -> dict:  # type: ignore[misc]
            return {}


async def test_register_rejects_empty_id(tmp_path: Path) -> None:
    server = AtdServer(socket_path=str(tmp_path / "atd.sock"))

    with pytest.raises(ValueError, match="non-empty id"):

        @server.register(definition=make_definition(""))
        async def handler(args: dict, ctx: object) -> dict:
            return {}


async def test_tool_list_returns_registered_summaries(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:read", name="Read"))
    async def read_h(args: dict, ctx: object) -> dict:
        return {}

    @server.register(definition=make_definition("demo:write", name="Write"))
    async def write_h(args: dict, ctx: object) -> dict:
        return {}

    task = await spawn(server)
    try:
        reply = await round_trip(sock, {"type": "tool_list"})
        assert reply["type"] == "tool_list"
        ids = sorted(t["id"] for t in reply["tools"])
        assert ids == ["demo:read", "demo:write"]
        # Each entry parses cleanly as a ToolSummary on the client side.
        for raw in reply["tools"]:
            ToolSummary.model_validate(raw)
    finally:
        await stop_and_wait(server, task)


async def test_tool_list_excludes_hidden_visibility(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:visible"))
    async def vis(args: dict, ctx: object) -> dict:
        return {}

    @server.register(
        definition=make_definition("demo:hidden", visibility=ToolVisibility.HIDDEN)
    )
    async def hid(args: dict, ctx: object) -> dict:
        return {}

    task = await spawn(server)
    try:
        reply = await round_trip(sock, {"type": "tool_list"})
        ids = [t["id"] for t in reply["tools"]]
        assert ids == ["demo:visible"]
    finally:
        await stop_and_wait(server, task)


async def test_tool_schema_returns_full_definition(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    defn = make_definition("demo:read", name="Read", description="Reads things")

    @server.register(definition=defn)
    async def handler(args: dict, ctx: object) -> dict:
        return {}

    task = await spawn(server)
    try:
        reply = await round_trip(sock, {"type": "tool_schema", "tool_id": "demo:read"})
        assert reply["type"] == "tool_schema"
        schema = reply["schema"]
        assert schema["id"] == "demo:read"
        assert schema["name"] == "Read"
        assert schema["description"] == "Reads things"
        assert schema["capability"]["domain"] == "demo"
        assert schema["visibility"] == "read"
    finally:
        await stop_and_wait(server, task)


async def test_tool_schema_returns_hidden_tools_by_id(tmp_path: Path) -> None:
    """Hidden tools are excluded from tool_list but reachable by id."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(
        definition=make_definition("demo:hidden", visibility=ToolVisibility.HIDDEN)
    )
    async def handler(args: dict, ctx: object) -> dict:
        return {}

    task = await spawn(server)
    try:
        reply = await round_trip(sock, {"type": "tool_schema", "tool_id": "demo:hidden"})
        assert reply["type"] == "tool_schema"
        assert reply["schema"]["id"] == "demo:hidden"
        assert reply["schema"]["visibility"] == "hidden"
    finally:
        await stop_and_wait(server, task)


async def test_tool_schema_unknown_returns_1000_not_found(tmp_path: Path) -> None:
    """Python client's `describe()` matches `"not found"` substring; we comply."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await spawn(server)
    try:
        reply = await round_trip(
            sock, {"type": "tool_schema", "tool_id": "demo:never_registered"}
        )
        assert reply["type"] == "error"
        assert reply["code"] == 1000
        assert "not found" in reply["message"]
    finally:
        await stop_and_wait(server, task)


async def test_tool_schema_missing_id_returns_error(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await spawn(server)
    try:
        reply = await round_trip(sock, {"type": "tool_schema"})
        assert reply["type"] == "error"
        assert reply["code"] == 1099
        assert "tool_id" in reply["message"]
    finally:
        await stop_and_wait(server, task)


async def test_run_tool_for_unknown_tool_returns_1000_after_phase_e(
    tmp_path: Path,
) -> None:
    """Phase E wired run_tool through `dispatch_run_tool`. The old Phase E stub
    (1099 placeholder) is gone; the unknown-tool path now returns 1000 instead."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await spawn(server)
    try:
        reply = await round_trip(
            sock,
            {"type": "run_tool", "tool_id": "demo:any", "args": {}, "dry_run": False},
        )
        assert reply["type"] == "error"
        assert reply["code"] == 1000
        assert "not found" in reply["message"]
    finally:
        await stop_and_wait(server, task)


async def test_middleware_stub_still_raises(tmp_path: Path) -> None:
    server = AtdServer(socket_path=str(tmp_path / "atd.sock"))
    with pytest.raises(NotImplementedError, match="Phase F"):
        server.middleware()
