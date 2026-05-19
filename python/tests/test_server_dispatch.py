"""Phase E tests for AtdServer: run_tool dispatch.

Coverage:
- capability gate (allow / deny / details payload)
- dry-run short-circuit
- tier-derived deadline → 1004
- handler return shapes: ToolSuccess / ToolFailure / plain dict
- ToolError exception → typed envelope (preserves adopter-namespace codes)
- generic Exception → 1099 internal_error (no traceback on wire)
- tool_not_found → 1000
- malformed request_id / args
- JSONSchema validation (if jsonschema is installed)
"""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import Any

import pytest

from atd_client.types import ToolFailure, ToolResultMetadata, ToolSuccess
from atd_server import AtdServer, CallContext, ToolError

from ._helpers import make_definition, round_trip, spawn, stop_and_wait

# ---------- handshake + call helper ----------------------------------------


async def _handshake_and_call(
    sock: str,
    *,
    capabilities: list[str],
    tool_id: str,
    args: Any = None,
    dry_run: bool = False,
) -> dict[str, Any]:
    """Open one connection, send Hello to negotiate caps, then run_tool, then close."""
    from atd_client.wire import read_frame, write_frame

    reader, writer = await asyncio.open_unix_connection(sock)
    try:
        await write_frame(
            writer, {"type": "hello", "requested_capabilities": list(capabilities)}
        )
        await asyncio.wait_for(read_frame(reader), timeout=2.0)
        await write_frame(
            writer,
            {
                "type": "run_tool",
                "tool_id": tool_id,
                "args": args if args is not None else {},
                "dry_run": dry_run,
            },
        )
        reply = await asyncio.wait_for(read_frame(reader), timeout=3.0)
    finally:
        writer.close()
        await writer.wait_closed()
    assert isinstance(reply, dict)
    return reply


# ---------- happy path -----------------------------------------------------


async def test_run_tool_success_returns_handler_data(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(
        definition=make_definition("demo:echo", required_capabilities=["read"])
    )
    async def echo(args: dict, ctx: CallContext) -> dict:
        return {"echoed": args, "request_id_shape": ctx.request_id[:4]}

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock,
            capabilities=["read"],
            tool_id="demo:echo",
            args={"x": 1},
        )
        assert reply["type"] == "tool_result"
        assert reply["success"] is True
        assert reply["dry_run"] is False
        assert reply["tool_id"] == "demo:echo"
        assert reply["result"]["echoed"] == {"x": 1}
        assert reply["result"]["request_id_shape"] == "req-"
    finally:
        await stop_and_wait(server, task)


async def test_handler_returning_tool_success_unwraps_to_data(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:s"))
    async def h(args: dict, ctx: CallContext) -> ToolSuccess:
        return ToolSuccess(
            data={"ok": True},
            metadata=ToolResultMetadata(tool_id="demo:s"),
        )

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["read"], tool_id="demo:s"
        )
        assert reply["success"] is True
        assert reply["result"] == {"ok": True}
    finally:
        await stop_and_wait(server, task)


async def test_handler_returning_tool_failure_carries_code_and_message(
    tmp_path: Path,
) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:f"))
    async def h(args: dict, ctx: CallContext) -> ToolFailure:
        return ToolFailure(code="2001", message="cbrain perception failed", retryable=True)

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["read"], tool_id="demo:f"
        )
        assert reply["type"] == "tool_result"
        assert reply["success"] is False
        # ToolFailure.code is str("2001"); we int()-coerce when numeric so the
        # adopter-namespace allocation (cbrain 2000+) lands as an int on the wire.
        assert reply["result"]["code"] == 2001
        assert reply["result"]["message"] == "cbrain perception failed"
        assert reply["result"]["retryable"] is True
    finally:
        await stop_and_wait(server, task)


# ---------- capability gate -------------------------------------------------


async def test_capability_denied_returns_1001_with_details(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(
        definition=make_definition(
            "demo:dangerous",
            required_capabilities=["write"],
        )
    )
    async def h(args: dict, ctx: CallContext) -> dict:
        return {}

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock,
            capabilities=["read"],  # write is missing
            tool_id="demo:dangerous",
        )
        assert reply["type"] == "error"
        assert reply["code"] == 1001
        assert reply["details"]["missing"] == ["write"]
        assert reply["details"]["granted"] == ["read"]
    finally:
        await stop_and_wait(server, task)


async def test_capability_denied_when_no_hello_at_all(tmp_path: Path) -> None:
    """Without a Hello, granted_capabilities=∅ — any cap-requiring tool denies."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:read_only", required_capabilities=["read"]))
    async def h(args: dict, ctx: CallContext) -> dict:
        return {}

    task = await spawn(server)
    try:
        reply = await round_trip(
            sock,
            {
                "type": "run_tool",
                "tool_id": "demo:read_only",
                "args": {},
                "dry_run": False,
            },
        )
        assert reply["type"] == "error"
        assert reply["code"] == 1001
    finally:
        await stop_and_wait(server, task)


# ---------- dry-run ---------------------------------------------------------


async def test_dry_run_short_circuits_without_invoking_handler(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    invocations: list[dict] = []

    @server.register(definition=make_definition("demo:never_called"))
    async def h(args: dict, ctx: CallContext) -> dict:
        invocations.append(args)
        return {}

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock,
            capabilities=["demo:read"],
            tool_id="demo:never_called",
            args={"preview_me": 42},
            dry_run=True,
        )
        assert reply["success"] is True
        assert reply["dry_run"] is True
        assert reply["result"] == {"args_preview": {"preview_me": 42}}
        # Handler MUST NOT have been called.
        assert invocations == []
    finally:
        await stop_and_wait(server, task)


# ---------- deadline --------------------------------------------------------


async def test_deadline_exceeded_returns_1004(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(
        definition=make_definition(
            "demo:slow",
            timeout_ms=100,  # 0.1s deadline
        )
    )
    async def h(args: dict, ctx: CallContext) -> dict:
        await asyncio.sleep(0.5)
        return {}

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock,
            capabilities=["demo:read"],
            tool_id="demo:slow",
        )
        assert reply["type"] == "tool_result"
        assert reply["success"] is False
        assert reply["result"]["code"] == 1004
        assert "deadline" in reply["result"]["message"]
    finally:
        await stop_and_wait(server, task)


# ---------- error envelope --------------------------------------------------


async def test_tool_error_exception_becomes_typed_envelope(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:err"))
    async def h(args: dict, ctx: CallContext) -> dict:
        raise ToolError(code=2042, message="cbrain skill aborted", partial_data={"step": 3})

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["demo:read"], tool_id="demo:err"
        )
        assert reply["type"] == "tool_result"
        assert reply["success"] is False
        assert reply["result"]["code"] == 2042
        assert reply["result"]["message"] == "cbrain skill aborted"
        assert reply["result"]["partial_data"] == {"step": 3}
    finally:
        await stop_and_wait(server, task)


async def test_unhandled_exception_becomes_1099_without_traceback_on_wire(
    tmp_path: Path,
) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:boom"))
    async def h(args: dict, ctx: CallContext) -> dict:
        raise ValueError("boom — should not leak filepath")

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["demo:read"], tool_id="demo:boom"
        )
        assert reply["success"] is False
        assert reply["result"]["code"] == 1099
        assert "ValueError" in reply["result"]["message"]
        # Wire MUST NOT carry the actual exception text or traceback.
        assert "boom" not in reply["result"]["message"]
    finally:
        await stop_and_wait(server, task)


# ---------- not-found / malformed ------------------------------------------


async def test_run_tool_unknown_returns_1000(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await spawn(server)
    try:
        reply = await round_trip(
            sock,
            {
                "type": "run_tool",
                "tool_id": "demo:nope",
                "args": {},
                "dry_run": False,
            },
        )
        assert reply["type"] == "error"
        assert reply["code"] == 1000
        assert "not found" in reply["message"]
    finally:
        await stop_and_wait(server, task)


async def test_run_tool_missing_tool_id_returns_1005(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await spawn(server)
    try:
        reply = await round_trip(
            sock,
            {"type": "run_tool", "args": {}, "dry_run": False},
        )
        assert reply["type"] == "error"
        assert reply["code"] == 1005
    finally:
        await stop_and_wait(server, task)


# ---------- JSONSchema validation (if installed) ---------------------------


jsonschema_installed: bool
try:
    import jsonschema  # noqa: F401

    jsonschema_installed = True
except ImportError:  # pragma: no cover
    jsonschema_installed = False


@pytest.mark.skipif(
    not jsonschema_installed, reason="jsonschema extra not installed"
)
async def test_invalid_args_returns_1005_when_jsonschema_installed(
    tmp_path: Path,
) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(
        definition=make_definition(
            "demo:typed",
            input_schema={
                "type": "object",
                "properties": {"n": {"type": "integer"}},
                "required": ["n"],
            },
        )
    )
    async def h(args: dict, ctx: CallContext) -> dict:
        return {"got": args["n"]}

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock,
            capabilities=["demo:read"],
            tool_id="demo:typed",
            args={"n": "not-an-int"},
        )
        assert reply["type"] == "error"
        assert reply["code"] == 1005
        assert "invalid arguments" in reply["message"]
    finally:
        await stop_and_wait(server, task)
