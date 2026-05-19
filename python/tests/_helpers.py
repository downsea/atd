"""Shared test helpers for the atd_server test suite."""

from __future__ import annotations

import asyncio
from typing import Any

from atd_client.types import (
    BindingProtocol,
    SafetyLevel,
    ToolBinding,
    ToolCapability,
    ToolDefinition,
    ToolResources,
    ToolSafety,
    ToolTrust,
    ToolVisibility,
    TrustLevel,
)
from atd_client.wire import read_frame, write_frame
from atd_server import AtdServer


async def spawn(server: AtdServer) -> asyncio.Task[None]:
    task = asyncio.create_task(server.serve())
    await server.wait_until_serving()
    return task


async def stop_and_wait(server: AtdServer, task: asyncio.Task[None]) -> None:
    await server.stop()
    await asyncio.wait_for(task, timeout=2.0)


async def round_trip(sock: str, payload: dict[str, Any]) -> dict[str, Any]:
    reader, writer = await asyncio.open_unix_connection(sock)
    try:
        await write_frame(writer, payload)
        reply = await asyncio.wait_for(read_frame(reader), timeout=2.0)
    finally:
        writer.close()
        await writer.wait_closed()
    assert isinstance(reply, dict)
    return reply


def make_definition(
    tool_id: str = "demo:echo",
    *,
    name: str = "Echo",
    description: str = "Echo tool",
    visibility: ToolVisibility = ToolVisibility.READ,
    capability_domain: str = "demo",
    capability_actions: list[str] | None = None,
    capability_tags: list[str] | None = None,
    required_capabilities: list[str] | None = None,
    timeout_ms: int = 5000,
    input_schema: dict[str, Any] | None = None,
) -> ToolDefinition:
    """Build a ToolDefinition with sensible test defaults.

    `capability_domain` / `capability_actions` populate the structured
    `ToolCapability` (metadata for discovery / summaries). `required_capabilities`
    is the FLAT opaque-string list the dispatcher uses for the gate
    (matches `crates/atd-protocol/src/tool.rs:31`).
    """
    return ToolDefinition(
        id=tool_id,
        name=name,
        description=description,
        version="0.1.0",
        capability=ToolCapability(
            domain=capability_domain,
            actions=capability_actions or ["read"],
            tags=capability_tags or [],
            intent_examples=[],
        ),
        input_schema=input_schema or {},
        output_schema={},
        bindings=[ToolBinding(protocol=BindingProtocol.APP_FUNCTION, config={})],
        safety=ToolSafety(level=SafetyLevel.READ, dry_run=True, side_effects=[]),
        resources=ToolResources(timeout_ms=timeout_ms, max_concurrent=1),
        trust=ToolTrust(publisher="atd-server-py.tests", trust_level=TrustLevel.L0_UNVERIFIED),
        visibility=visibility,
        required_capabilities=required_capabilities or [],
    )
