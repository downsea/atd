"""Phase F tests for AtdServer: pre_call / post_call / on_error middleware."""

from __future__ import annotations

from pathlib import Path

import pytest

from atd_client.types import ToolFailure
from atd_server import AtdServer, CallContext, ToolError

from ._helpers import make_definition, spawn, stop_and_wait
from .test_server_dispatch import _handshake_and_call


async def test_pre_call_can_short_circuit_handler(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    invocations: list[str] = []

    @server.register(definition=make_definition("demo:echo"))
    async def echo(args: dict, ctx: CallContext) -> dict:
        invocations.append("handler")
        return {"echoed": args}

    @server.middleware(stage="pre_call")
    async def reject(request: dict, ctx: CallContext, call_next):
        invocations.append("pre_call")
        # Don't await call_next — short-circuit with a typed failure.
        return ToolFailure(code="9999", message="rejected by pre_call", retryable=False)

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["demo:read"], tool_id="demo:echo"
        )
        assert reply["type"] == "tool_result"
        assert reply["success"] is False
        assert reply["result"]["code"] == 9999
        assert reply["result"]["message"] == "rejected by pre_call"
        # Handler MUST NOT have been called.
        assert invocations == ["pre_call"]
    finally:
        await stop_and_wait(server, task)


async def test_post_call_can_observe_and_mutate_response(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    observed: list[dict] = []

    @server.register(definition=make_definition("demo:echo"))
    async def echo(args: dict, ctx: CallContext) -> dict:
        return {"echoed": args}

    @server.middleware(stage="post_call")
    async def audit(request: dict, ctx: CallContext, call_next):
        response = await call_next()
        observed.append({"req_tool_id": request["tool_id"], "resp": response})
        # Return mutated response (here: prepend a marker)
        if isinstance(response, dict):
            response = {**response, "_audited": True}
        return response

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["demo:read"], tool_id="demo:echo", args={"x": 1}
        )
        assert reply["success"] is True
        assert reply["result"]["_audited"] is True
        assert reply["result"]["echoed"] == {"x": 1}
        assert len(observed) == 1
        assert observed[0]["req_tool_id"] == "demo:echo"
    finally:
        await stop_and_wait(server, task)


async def test_middleware_order_pre_post_lifo_around_handler(tmp_path: Path) -> None:
    """Spec §5.6: pre1 → pre2 → handler → post2 unwinds → post1 unwinds."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    events: list[str] = []

    @server.register(definition=make_definition("demo:echo"))
    async def echo(args: dict, ctx: CallContext) -> dict:
        events.append("handler")
        return {}

    @server.middleware(stage="pre_call")
    async def pre1(request: dict, ctx: CallContext, call_next):
        events.append("pre1:enter")
        r = await call_next()
        events.append("pre1:exit")
        return r

    @server.middleware(stage="pre_call")
    async def pre2(request: dict, ctx: CallContext, call_next):
        events.append("pre2:enter")
        r = await call_next()
        events.append("pre2:exit")
        return r

    @server.middleware(stage="post_call")
    async def post1(request: dict, ctx: CallContext, call_next):
        events.append("post1:enter")
        r = await call_next()
        events.append("post1:exit")
        return r

    @server.middleware(stage="post_call")
    async def post2(request: dict, ctx: CallContext, call_next):
        events.append("post2:enter")
        r = await call_next()
        events.append("post2:exit")
        return r

    task = await spawn(server)
    try:
        await _handshake_and_call(sock, capabilities=["demo:read"], tool_id="demo:echo")
        assert events == [
            "pre1:enter",
            "pre2:enter",
            "post1:enter",
            "post2:enter",
            "handler",
            "post2:exit",
            "post1:exit",
            "pre2:exit",
            "pre1:exit",
        ]
    finally:
        await stop_and_wait(server, task)


async def test_on_error_can_suppress_exception_into_typed_failure(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:boom"))
    async def boom(args: dict, ctx: CallContext) -> dict:
        raise ValueError("kaboom-secret")

    @server.middleware(stage="on_error")
    async def trap(request: dict, ctx: CallContext, exc: BaseException):
        # Suppress the default 1099 envelope; emit a curated failure instead.
        return ToolFailure(
            code="7042", message="trapped by on_error", retryable=True
        )

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["demo:read"], tool_id="demo:boom"
        )
        assert reply["success"] is False
        assert reply["result"]["code"] == 7042
        assert reply["result"]["message"] == "trapped by on_error"
        # The original exception text MUST NOT leak.
        assert "kaboom-secret" not in str(reply)
    finally:
        await stop_and_wait(server, task)


async def test_on_error_returning_none_falls_through_to_default(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:boom"))
    async def boom(args: dict, ctx: CallContext) -> dict:
        raise ToolError(code=2001, message="cbrain explicit failure")

    @server.middleware(stage="on_error")
    async def passthrough(request: dict, ctx: CallContext, exc: BaseException):
        return None  # do not suppress

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["demo:read"], tool_id="demo:boom"
        )
        # Default ToolError envelope wins.
        assert reply["success"] is False
        assert reply["result"]["code"] == 2001
        assert reply["result"]["message"] == "cbrain explicit failure"
    finally:
        await stop_and_wait(server, task)


async def test_on_error_first_non_none_suppresses_rest(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    invocations: list[str] = []

    @server.register(definition=make_definition("demo:boom"))
    async def boom(args: dict, ctx: CallContext) -> dict:
        raise ValueError("x")

    @server.middleware(stage="on_error")
    async def first(request: dict, ctx: CallContext, exc: BaseException):
        invocations.append("first")
        return ToolFailure(code="9001", message="trapped by first", retryable=False)

    @server.middleware(stage="on_error")
    async def second(request: dict, ctx: CallContext, exc: BaseException):
        invocations.append("second")  # should not be called
        return ToolFailure(code="9002", message="should not appear", retryable=False)

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["demo:read"], tool_id="demo:boom"
        )
        assert reply["result"]["code"] == 9001
        assert invocations == ["first"]
    finally:
        await stop_and_wait(server, task)


async def test_on_error_middleware_that_raises_is_logged_and_skipped(
    tmp_path: Path,
) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)

    @server.register(definition=make_definition("demo:boom"))
    async def boom(args: dict, ctx: CallContext) -> dict:
        raise ValueError("orig")

    @server.middleware(stage="on_error")
    async def buggy(request: dict, ctx: CallContext, exc: BaseException):
        raise RuntimeError("middleware bug")

    @server.middleware(stage="on_error")
    async def fallback(request: dict, ctx: CallContext, exc: BaseException):
        return ToolFailure(code="8000", message="fallback fired", retryable=False)

    task = await spawn(server)
    try:
        reply = await _handshake_and_call(
            sock, capabilities=["demo:read"], tool_id="demo:boom"
        )
        # Buggy middleware was skipped; fallback fired.
        assert reply["result"]["code"] == 8000
        assert reply["result"]["message"] == "fallback fired"
    finally:
        await stop_and_wait(server, task)


async def test_middleware_decorator_rejects_unknown_stage(tmp_path: Path) -> None:
    server = AtdServer(socket_path=str(tmp_path / "atd.sock"))
    with pytest.raises(ValueError, match="unknown middleware stage"):
        server.middleware(stage="nope")  # type: ignore[arg-type]
