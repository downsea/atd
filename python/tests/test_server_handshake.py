"""Phase C tests for AtdServer: ping/pong, Hello negotiation, capability grants."""

from __future__ import annotations

import asyncio
from pathlib import Path
from typing import Any

from atd_client.wire import read_frame, write_frame
from atd_server import AtdServer, GrantedCapabilities


async def _spawn(server: AtdServer) -> asyncio.Task[None]:
    task = asyncio.create_task(server.serve())
    await server.wait_until_serving()
    return task


async def _round_trip(sock: str, payload: dict[str, Any]) -> dict[str, Any]:
    reader, writer = await asyncio.open_unix_connection(sock)
    try:
        await write_frame(writer, payload)
        reply = await asyncio.wait_for(read_frame(reader), timeout=2.0)
    finally:
        writer.close()
        await writer.wait_closed()
    assert isinstance(reply, dict)
    return reply


async def test_ping_returns_pong(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await _spawn(server)
    try:
        reply = await _round_trip(sock, {"type": "ping"})
        assert reply == {"type": "pong"}
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_hello_default_policy_grants_all_requested(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock, server_version="atd-server-py/test")
    task = await _spawn(server)
    try:
        reply = await _round_trip(
            sock,
            {
                "type": "hello",
                "client_id": "demo",
                "requested_capabilities": ["fs.read", "fs.write"],
            },
        )
        assert reply["type"] == "hello_ack"
        assert sorted(reply["granted_capabilities"]) == ["fs.read", "fs.write"]
        assert reply["server_version"] == "atd-server-py/test"
        assert reply["supported_tiers"] == ["hot", "warm", "cold"]
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_hello_custom_policy_can_deny_a_capability(tmp_path: Path) -> None:
    async def deny_write(
        hello: dict[str, Any], ucan_tokens: tuple[str, ...]
    ) -> GrantedCapabilities:
        requested = hello.get("requested_capabilities") or []
        granted = {c for c in requested if "write" not in c}
        return GrantedCapabilities(capabilities=frozenset(granted))

    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock, policy=deny_write)
    task = await _spawn(server)
    try:
        reply = await _round_trip(
            sock,
            {
                "type": "hello",
                "requested_capabilities": ["fs.read", "fs.write", "net.write"],
            },
        )
        assert reply["type"] == "hello_ack"
        assert sorted(reply["granted_capabilities"]) == ["fs.read"]
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_hello_passes_ucan_tokens_to_policy(tmp_path: Path) -> None:
    received_tokens: list[tuple[str, ...]] = []

    async def observe(
        hello: dict[str, Any], ucan_tokens: tuple[str, ...]
    ) -> GrantedCapabilities:
        received_tokens.append(ucan_tokens)
        return GrantedCapabilities(capabilities=frozenset())

    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock, policy=observe)
    task = await _spawn(server)
    try:
        await _round_trip(
            sock,
            {
                "type": "hello",
                "requested_capabilities": [],
                "ucan_tokens": ["jwt-a", "jwt-b"],
            },
        )
        assert received_tokens == [("jwt-a", "jwt-b")]
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_hello_can_be_resent_and_replaces_prior_grants(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await _spawn(server)
    try:
        reader, writer = await asyncio.open_unix_connection(sock)
        try:
            # first hello
            await write_frame(writer, {"type": "hello", "requested_capabilities": ["fs.read"]})
            first = await asyncio.wait_for(read_frame(reader), timeout=2.0)
            assert first["granted_capabilities"] == ["fs.read"]
            # second hello on same connection
            await write_frame(
                writer, {"type": "hello", "requested_capabilities": ["net.read", "net.write"]}
            )
            second = await asyncio.wait_for(read_frame(reader), timeout=2.0)
            assert sorted(second["granted_capabilities"]) == ["net.read", "net.write"]
        finally:
            writer.close()
            await writer.wait_closed()
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_ping_works_before_hello(tmp_path: Path) -> None:
    """Hello is optional — Rust byte-compat. Ping must work pre-Hello."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await _spawn(server)
    try:
        reader, writer = await asyncio.open_unix_connection(sock)
        try:
            await write_frame(writer, {"type": "ping"})
            assert await read_frame(reader) == {"type": "pong"}
            await write_frame(writer, {"type": "hello", "requested_capabilities": []})
            ack = await read_frame(reader)
            assert ack["type"] == "hello_ack"
            await write_frame(writer, {"type": "ping"})
            assert await read_frame(reader) == {"type": "pong"}
        finally:
            writer.close()
            await writer.wait_closed()
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_unknown_message_type_returns_stub_error(tmp_path: Path) -> None:
    """Phase D handles ping/hello/tool_list/tool_schema/run_tool. Anything else is 1099."""
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await _spawn(server)
    try:
        reply = await _round_trip(sock, {"type": "future_message_type"})
        assert reply["type"] == "error"
        assert reply["code"] == 1099
        assert "future_message_type" in reply["message"]
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)


async def test_non_object_frame_returns_error(tmp_path: Path) -> None:
    sock = str(tmp_path / "atd.sock")
    server = AtdServer(socket_path=sock)
    task = await _spawn(server)
    try:
        reader, writer = await asyncio.open_unix_connection(sock)
        try:
            await write_frame(writer, ["not", "an", "object"])
            reply = await asyncio.wait_for(read_frame(reader), timeout=2.0)
            assert reply["type"] == "error"
            assert reply["code"] == 1099
            assert "JSON object" in reply["message"]
        finally:
            writer.close()
            await writer.wait_closed()
    finally:
        await server.stop()
        await asyncio.wait_for(task, timeout=2.0)
