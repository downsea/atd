"""Transport layer — Unix socket for Phase 1. Future: stdio, HTTP."""

from __future__ import annotations

import asyncio
from pathlib import Path


async def connect_unix(path: Path | str) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
    """Open a Unix domain socket connection.

    Raises :class:`OSError` on connect failure; the caller wraps into
    :class:`atd_client.errors.ServerUnreachable`.
    """
    return await asyncio.open_unix_connection(path=str(path))


def default_sock_path() -> Path:
    """Default ANOS daemon socket: ``$HOME/.anos/anos.sock``."""
    home = Path.home()
    return home / ".anos" / "anos.sock"
