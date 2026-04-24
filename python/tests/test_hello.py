"""SP-12 tests for ``AtdClient.hello()`` and capability-denied mapping.

These exercise the client surface against a mock server; end-to-end
coverage against the real ``atd-ref-server`` lives in the Rust
integration tests.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

import pytest

from atd_client import AtdClient, protocol
from atd_client.errors import CapabilityDenied


def _handler_hello_ack_grants(granted: list[str]) -> Callable[[dict[str, Any]], dict[str, Any]]:
    def h(req: dict[str, Any]) -> dict[str, Any]:
        t = req.get("type")
        if t == "ping":
            return {"type": "pong"}
        if t == "hello":
            return {
                "type": "hello_ack",
                "granted_capabilities": granted,
                "server_version": "atd-ref-server 0.2.0",
                "supported_tiers": ["hot", "warm", "cold"],
            }
        return {"type": "error", "message": "unexpected"}

    return h


def _handler_pre_sp12(req: dict[str, Any]) -> dict[str, Any]:
    # Simulates a server that doesn't recognize `hello` — replies with a
    # generic error, which AtdClient.hello should demote to [].
    t = req.get("type")
    if t == "ping":
        return {"type": "pong"}
    if t == "hello":
        return {"type": "error", "message": "unknown request type: hello"}
    return {"type": "error", "message": "unexpected"}


def _handler_capability_denied(req: dict[str, Any]) -> dict[str, Any]:
    t = req.get("type")
    if t == "ping":
        return {"type": "pong"}
    if t == "run_tool":
        return {
            "type": "error",
            "message": "capability denied for ref:x: missing ['exec']",
            "code": protocol.ERR_CAPABILITY_DENIED,
            "retryable": False,
            "details": {
                "required": ["exec"],
                "granted": [],
                "missing": ["exec"],
            },
        }
    return {"type": "error", "message": "unexpected"}


@pytest.mark.asyncio
async def test_hello_returns_granted_subset(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_hello_ack_grants(["exec"]))
    client = await AtdClient.connect(sock)
    try:
        granted = await client.hello(["exec", "admin"], client_id="pytest")
        assert granted == ["exec"]
    finally:
        await client.close()


@pytest.mark.asyncio
async def test_hello_empty_grants_when_server_grants_nothing(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_hello_ack_grants([]))
    client = await AtdClient.connect(sock)
    try:
        granted = await client.hello(["exec"])
        assert granted == []
    finally:
        await client.close()


@pytest.mark.asyncio
async def test_hello_on_pre_sp12_server_returns_empty_list(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_pre_sp12)
    client = await AtdClient.connect(sock)
    try:
        granted = await client.hello(["exec"])
        assert granted == []
    finally:
        await client.close()


@pytest.mark.asyncio
async def test_call_surfaces_capability_denied_as_typed_exception(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_capability_denied)
    client = await AtdClient.connect(sock)
    try:
        with pytest.raises(CapabilityDenied) as excinfo:
            await client.call("ref:x", args={})
        assert excinfo.value.tool_id == "ref:x"
        assert excinfo.value.required == ["exec"]
        assert excinfo.value.granted == []
    finally:
        await client.close()
