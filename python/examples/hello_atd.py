"""Minimum working example — Python SDK.

Mirrors the Rust `examples/hello_atd.rs`: connect, discover up to 3 tools,
describe the first, call with dry_run=True, print at each step.

Run:
    ANOS_SOCK=~/.anos/anos.sock uv run python examples/hello_atd.py
"""

from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path

from atd_client import AtdClient, ToolFailure, ToolSuccess


async def main() -> None:
    sock_env = os.environ.get("ANOS_SOCK")
    sock = Path(sock_env) if sock_env else None

    print(f"[atd] connecting to {sock or '<default>'}")
    async with await AtdClient.connect(sock) as client:
        print("[atd] connected")

        tools = await client.discover(limit=3)
        print(f"[atd] {len(tools)} tool(s) discovered")
        for t in tools:
            print(f"        - {t.id} ({t.name})")

        if not tools:
            print("[atd] no tools to describe/call — done.")
            return

        first = tools[0]
        d = await client.describe(first.id)
        print(
            f"[atd] describe({d.id}) → domain={d.capability.domain}, "
            f"bindings={len(d.bindings)}"
        )

        r = await client.call(first.id, {}, dry_run=True)
        if isinstance(r, ToolSuccess):
            print(f"[atd] call ok: {json.dumps(r.data)}")
        elif isinstance(r, ToolFailure):
            print(f"[atd] call error: [{r.code}] {r.message}")


if __name__ == "__main__":
    asyncio.run(main())
