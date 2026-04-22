from __future__ import annotations

import asyncio
import io
import struct

import pytest

from atd_client.wire import MAX_FRAME_BYTES, read_frame, write_frame


class _BytesReader:
    """Minimal asyncio-compatible reader wrapping a bytes buffer."""

    def __init__(self, data: bytes) -> None:
        self._buf = io.BytesIO(data)

    async def readexactly(self, n: int) -> bytes:
        b = self._buf.read(n)
        if len(b) != n:
            raise asyncio.IncompleteReadError(b, n)
        return b


class _BytesWriter:
    def __init__(self) -> None:
        self.buf = bytearray()

    def write(self, data: bytes) -> None:
        self.buf.extend(data)

    async def drain(self) -> None:
        pass


@pytest.mark.asyncio
async def test_write_then_read_roundtrip() -> None:
    writer = _BytesWriter()
    msg = {"kind": "ping", "n": 7}
    await write_frame(writer, msg)

    reader = _BytesReader(bytes(writer.buf))
    back = await read_frame(reader)
    assert back == msg


@pytest.mark.asyncio
async def test_frame_uses_big_endian_u32_prefix() -> None:
    writer = _BytesWriter()
    await write_frame(writer, {"x": 1})
    prefix = bytes(writer.buf[:4])
    body_len = struct.unpack(">I", prefix)[0]
    assert body_len == len(writer.buf) - 4


@pytest.mark.asyncio
async def test_oversized_frame_raises() -> None:
    bogus = struct.pack(">I", 20 * 1024 * 1024)
    reader = _BytesReader(bogus)
    with pytest.raises(Exception) as excinfo:
        await read_frame(reader)
    assert "too large" in str(excinfo.value)


@pytest.mark.asyncio
async def test_max_frame_bytes_matches_rust_constant() -> None:
    assert MAX_FRAME_BYTES == 10 * 1024 * 1024
