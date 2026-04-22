"""Length-prefixed JSON codec over asyncio streams.

Wire format is byte-compatible with the Rust `atd-client::wire`:
- 4-byte big-endian ``u32`` length prefix (max 10 MiB)
- UTF-8 JSON body

Used for both Unix socket and future stdio transports.
"""

from __future__ import annotations

import json
import struct
from typing import Any, Protocol

MAX_FRAME_BYTES = 10 * 1024 * 1024


class _AsyncReader(Protocol):
    async def readexactly(self, n: int) -> bytes: ...


class _AsyncWriter(Protocol):
    def write(self, data: bytes) -> None: ...
    async def drain(self) -> None: ...


async def write_frame(writer: _AsyncWriter, msg: Any) -> None:
    body = json.dumps(msg, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    if len(body) > MAX_FRAME_BYTES:
        raise ValueError(f"frame too large: {len(body)} bytes")
    writer.write(struct.pack(">I", len(body)))
    writer.write(body)
    await writer.drain()


async def read_frame(reader: _AsyncReader) -> Any:
    header = await reader.readexactly(4)
    (length,) = struct.unpack(">I", header)
    if length > MAX_FRAME_BYTES:
        raise ValueError(f"frame too large: {length} bytes")
    body = await reader.readexactly(length)
    return json.loads(body.decode("utf-8"))
