from __future__ import annotations

from collections.abc import Callable
from typing import Any

import pytest

from atd_client import AtdClient, ProtocolError, ToolFailure, ToolSuccess


def _handler_all_ok(req: dict[str, Any]) -> dict[str, Any]:
    t = req.get("type")
    if t == "ping":
        return {"type": "pong"}
    if t == "tool_list":
        return {
            "type": "tool_list",
            "tools": [
                {
                    "id": "anos:fs.read",
                    "description": "Read a file",
                    "tier": "hot",
                    "visibility": "read",
                },
                {
                    "id": "anos:fs.write",
                    "description": "Write a file",
                    "tier": "hot",
                    "visibility": "write",
                },
            ],
        }
    if t == "tool_schema":
        return {
            "type": "tool_schema",
            "schema": {
                "id": req["tool_id"],
                "name": "Read",
                "description": "Read a file.",
                "version": "0.1.0",
                "capability": {
                    "domain": "fs",
                    "actions": ["read"],
                    "tags": [],
                    "intent_examples": [],
                },
                "input_schema": {"type": "object"},
                "output_schema": {"type": "string"},
                "bindings": [{"protocol": "Cli", "config": {}}],
                "safety": {
                    "level": "Read",
                    "dry_run": False,
                    "side_effects": [],
                    "data_sensitivity": None,
                },
                "resources": {
                    "timeout_ms": 1000,
                    "max_concurrent": 1,
                    "rate_limit_per_min": None,
                    "estimated_tokens": None,
                },
                "trust": {
                    "publisher": "anos",
                    "trust_level": "L2Tested",
                    "signature": None,
                },
                "visibility": "read",
            },
        }
    if t == "run_tool":
        return {
            "type": "tool_result",
            "tool_id": req["tool_id"],
            "result": {"echo": req.get("args")},
            "success": True,
            "dry_run": bool(req.get("dry_run")),
        }
    return {"type": "error", "message": f"unexpected: {t}"}


async def test_connect_succeeds_and_pings(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        assert client.is_connected()
    finally:
        await client.close()


async def test_discover_returns_summaries(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        summaries = await client.discover()
        assert len(summaries) == 2
        ids = {s.id for s in summaries}
        assert ids == {"anos:fs.read", "anos:fs.write"}
        for s in summaries:
            assert s.name, f"name should be filled, got empty for {s.id}"
            assert s.domain, f"domain should be filled, got empty for {s.id}"
    finally:
        await client.close()


async def test_discover_filters_client_side(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        summaries = await client.discover(query="read", limit=1)
        assert len(summaries) == 1
        assert summaries[0].id == "anos:fs.read"
    finally:
        await client.close()


async def test_describe_returns_full_definition(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        d = await client.describe("anos:fs.read")
        assert d.id == "anos:fs.read"
        assert d.capability.domain == "fs"
    finally:
        await client.close()


async def test_call_success(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        r = await client.call("anos:fs.read", {"path": "/tmp/x"})
        assert isinstance(r, ToolSuccess)
        assert r.data == {"echo": {"path": "/tmp/x"}}
    finally:
        await client.close()


async def test_call_failure_becomes_tool_failure(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    def h(req: dict[str, Any]) -> dict[str, Any]:
        if req.get("type") == "ping":
            return {"type": "pong"}
        if req.get("type") == "run_tool":
            return {
                "type": "tool_result",
                "tool_id": req["tool_id"],
                "result": {"code": "EPERM", "message": "denied", "retryable": False},
                "success": False,
                "dry_run": False,
            }
        return {"type": "error", "message": "no"}

    sock = await mock_server(h)
    client = await AtdClient.connect(sock)
    try:
        r = await client.call("anos:fs.read", {})
        assert isinstance(r, ToolFailure)
        assert r.code == "EPERM"
        assert r.reason is not None and "EPERM" in r.reason
    finally:
        await client.close()


async def test_ping_error_when_server_sends_wrong_response(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    def h(req: dict[str, Any]) -> dict[str, Any]:
        return {"type": "tool_list", "tools": []}

    sock = await mock_server(h)
    with pytest.raises(ProtocolError):
        await AtdClient.connect(sock)
