"""Async ATD client.

Mirrors the Rust `atd-client::AtdClient`. One client owns one Unix socket
connection; concurrent callers serialize through an ``asyncio.Lock``.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
from pathlib import Path
from typing import Any

from atd_client import protocol
from atd_client.errors import (
    InvalidArguments,
    ProtocolError,
    ServerUnreachable,
    ToolExecutionFailed,
    ToolNotFound,
)
from atd_client.transport import connect_unix, default_sock_path
from atd_client.types import (
    ToolDefinition,
    ToolFailure,
    ToolResultMetadata,
    ToolSuccess,
    ToolSummary,
    ToolTier,
    ToolVisibility,
)
from atd_client.wire import read_frame, write_frame


def _derive_domain(tool_id: str) -> str:
    """Parse ``anos:fs.read`` → ``"fs"``."""
    if ":" not in tool_id:
        return ""
    _, rest = tool_id.split(":", 1)
    return rest.split(".", 1)[0] if "." in rest else rest


def _derive_name(s: ToolSummary) -> str:
    if s.name:
        return s.name
    if s.description:
        return s.description
    return s.id


class AtdClient:
    """Async client. Use :meth:`connect` to construct.

    Example::

        client = await AtdClient.connect()      # default ~/.anos/anos.sock
        tools = await client.discover(query="fs")
        result = await client.call("anos:fs.read", {"path": "/tmp/x"})
        await client.close()
    """

    _reader: asyncio.StreamReader
    _writer: asyncio.StreamWriter
    _lock: asyncio.Lock
    _closed: bool

    def __init__(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        self._reader = reader
        self._writer = writer
        self._lock = asyncio.Lock()
        self._closed = False

    @classmethod
    async def connect(cls, sock: Path | str | None = None) -> AtdClient:
        path = Path(sock) if sock is not None else default_sock_path()
        try:
            reader, writer = await connect_unix(path)
        except OSError as e:
            raise ServerUnreachable(str(e)) from e

        client = cls(reader, writer)
        try:
            await client._ping()
        except BaseException:
            await client.close()
            raise
        return client

    def is_connected(self) -> bool:
        return not self._closed

    async def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        self._writer.close()
        with contextlib.suppress(Exception):
            await self._writer.wait_closed()

    async def _request(self, req: dict[str, Any]) -> dict[str, Any]:
        if self._closed:
            raise ServerUnreachable("client is closed")
        async with self._lock:
            try:
                await write_frame(self._writer, req)
                resp = await read_frame(self._reader)
            except (OSError, asyncio.IncompleteReadError) as e:
                raise ServerUnreachable(str(e)) from e
        if not isinstance(resp, dict):
            raise ProtocolError(expected="json object", got=repr(resp))
        return resp

    async def _ping(self) -> None:
        resp = await self._request(protocol.ping_request())
        if resp.get("type") != protocol.RESP_PONG:
            raise ProtocolError(expected="pong", got=str(resp.get("type")))

    # ---------- public API ----------

    async def discover(
        self,
        query: str | None = None,
        *,
        domain: str | None = None,
        tier: ToolTier | None = None,
        visibility: ToolVisibility | None = None,
        limit: int | None = None,
    ) -> list[ToolSummary]:
        resp = await self._request(protocol.tool_list_request())
        if resp.get("type") == protocol.RESP_ERROR:
            raise ProtocolError(
                expected="tool_list", got=f"error: {resp.get('message')}"
            )
        if resp.get("type") != protocol.RESP_TOOL_LIST:
            raise ProtocolError(expected="tool_list", got=str(resp.get("type")))

        raw = resp.get("tools")
        if not isinstance(raw, list):
            raise ProtocolError(expected="array of tool summaries", got=repr(raw))

        out: list[ToolSummary] = []
        for entry in raw:
            if not isinstance(entry, dict):
                continue
            try:
                s = ToolSummary.model_validate(entry)
            except Exception:
                # Tolerate full ToolDefinition entries by projecting down.
                try:
                    d = ToolDefinition.model_validate(entry)
                except Exception:
                    continue
                s = ToolSummary(
                    id=d.id,
                    name=d.name,
                    description=d.description,
                    domain=d.capability.domain,
                    tags=list(d.capability.tags),
                    visibility=d.visibility,
                )
            out.append(s)

        # Fill derived defaults (ANOS omits name/domain).
        for i, s in enumerate(out):
            if not s.name or not s.domain:
                out[i] = s.model_copy(
                    update={
                        "name": _derive_name(s),
                        "domain": s.domain or _derive_domain(s.id),
                    }
                )

        if query is not None:
            q = query.lower()
            out = [
                s
                for s in out
                if q in s.name.lower() or q in s.description.lower() or q in s.id.lower()
            ]
        if domain is not None:
            out = [s for s in out if s.domain == domain]
        if tier is not None:
            out = [s for s in out if s.tier == tier]
        if visibility is not None:
            out = [s for s in out if s.visibility == visibility]
        if limit is not None:
            out = out[:limit]
        return out

    async def describe(self, tool_id: str) -> ToolDefinition:
        resp = await self._request(protocol.tool_schema_request(tool_id))
        t = resp.get("type")
        if t == protocol.RESP_TOOL_SCHEMA:
            schema = resp.get("schema")
            try:
                return ToolDefinition.model_validate(schema)
            except Exception as e:
                raise ProtocolError(
                    expected="ToolDefinition", got=f"deserialize error: {e}"
                ) from e
        if t == protocol.RESP_ERROR:
            msg = str(resp.get("message", ""))
            if "not found" in msg.lower():
                raise ToolNotFound(tool_id=tool_id, suggestions=[])
            raise ProtocolError(expected="tool_schema", got=f"error: {msg}")
        raise ProtocolError(expected="tool_schema", got=str(t))

    async def call(
        self,
        tool_id: str,
        args: Any = None,
        *,
        dry_run: bool = False,
    ) -> ToolSuccess | ToolFailure:
        if args is None:
            args = {}
        if not isinstance(args, (dict, list, str, int, float, bool, type(None))):
            raise InvalidArguments(
                tool_id=tool_id,
                field="args",
                reason="must be a JSON-serializable value",
            )

        resp = await self._request(protocol.run_tool_request(tool_id, args, dry_run))
        t = resp.get("type")
        if t == protocol.RESP_TOOL_RESULT:
            success = bool(resp.get("success"))
            result = resp.get("result")
            resp_tool_id = str(resp.get("tool_id", tool_id))
            if success:
                return ToolSuccess(
                    data=result,
                    metadata=ToolResultMetadata(tool_id=resp_tool_id),
                )
            code = (
                str(result.get("code"))
                if isinstance(result, dict) and "code" in result
                else "UNKNOWN"
            )
            message = (
                str(result.get("message"))
                if isinstance(result, dict) and "message" in result
                else "tool call failed"
            )
            retryable = bool(result.get("retryable")) if isinstance(result, dict) else False
            return ToolFailure(
                code=code,
                message=message,
                reason=json.dumps(result) if result is not None else None,
                retryable=retryable,
            )
        if t == protocol.RESP_ERROR:
            raise ToolExecutionFailed(
                tool_id=tool_id,
                inner=RuntimeError(
                    f"{resp.get('message')} (retryable={resp.get('retryable', False)})"
                ),
            )
        raise ProtocolError(expected="tool_result", got=str(t))

    async def __aenter__(self) -> AtdClient:
        return self

    async def __aexit__(self, *_: Any) -> None:
        await self.close()


__all__ = ["AtdClient"]
