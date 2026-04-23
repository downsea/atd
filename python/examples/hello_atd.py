"""atd-mvp capstone demo (Python SDK).

Auto-spawns `atd-ref-server` (the in-repo neutral reference ATD server),
connects via the Python `atd_client` SDK, exercises three representative
tools end-to-end.

This demo has ZERO dependency on ANOS. It proves the ATD protocol is
vendor-neutral: the SDK speaks the wire format, the ref-server answers.

Run:
    cargo build --release -p atd-ref-server
    uv run python examples/hello_atd.py

Override the server (e.g., to demo against ANOS):
    ATD_SOCK=~/.anos/anos.sock uv run python examples/hello_atd.py
"""

from __future__ import annotations

import asyncio
import json
import os
import signal
import sys
import tempfile
from contextlib import asynccontextmanager
from pathlib import Path
from typing import AsyncIterator

from atd_client import AtdClient, ToolFailure, ToolSuccess

SOCKET_WAIT_ATTEMPTS = 30
SOCKET_WAIT_INTERVAL_S = 0.1


def repo_root() -> Path:
    # This file lives at <root>/python/examples/hello_atd.py
    return Path(__file__).resolve().parent.parent.parent


async def _wait_for_socket(sock: Path) -> bool:
    for _ in range(SOCKET_WAIT_ATTEMPTS):
        if sock.exists():
            return True
        await asyncio.sleep(SOCKET_WAIT_INTERVAL_S)
    return False


@asynccontextmanager
async def acquire_server() -> AsyncIterator[Path]:
    """Yield a Unix socket path pointing at a usable atd-ref-server.

    If ATD_SOCK is set, assume a server is already running there.
    Otherwise, spawn one (from target/release/atd-ref-server) into a
    tempdir and tear it down at exit.
    """
    override = os.environ.get("ATD_SOCK")
    if override:
        print(f"[atd] using ATD_SOCK override → {override}")
        yield Path(override)
        return

    binary = repo_root() / "target" / "release" / "atd-ref-server"
    if not binary.exists():
        raise RuntimeError(
            f"atd-ref-server release binary not found at {binary}.\n"
            "build it first: cargo build --release -p atd-ref-server"
        )

    with tempfile.TemporaryDirectory() as td:
        sock = Path(td) / "demo.sock"
        print(f"[atd] auto-spawning atd-ref-server → {sock}")
        proc = await asyncio.create_subprocess_exec(
            str(binary),
            "--sock",
            str(sock),
            stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.DEVNULL,
        )
        try:
            if not await _wait_for_socket(sock):
                raise RuntimeError("ref-server didn't bind its socket within 3s")
            yield sock
        finally:
            if proc.returncode is None:
                proc.send_signal(signal.SIGTERM)
                try:
                    await asyncio.wait_for(proc.wait(), timeout=2.0)
                except asyncio.TimeoutError:
                    proc.kill()
                    await proc.wait()


def _print_echo(result: ToolSuccess | ToolFailure) -> None:
    if isinstance(result, ToolSuccess):
        print(f"      → {json.dumps(result.data)}")
    else:
        print(f"      ✗ {result.code}: {result.message}")


def _print_glob(result: ToolSuccess | ToolFailure) -> None:
    if isinstance(result, ToolFailure):
        print(f"      ✗ {result.code}: {result.message}")
        return
    paths = result.data.get("paths", [])
    preview = paths[:3]
    suffix = f" (+{len(paths) - 3} more)" if len(paths) > 3 else ""
    print(f"      → {len(paths)} paths: {', '.join(preview)}{suffix}")


def _print_shell(result: ToolSuccess | ToolFailure) -> None:
    if isinstance(result, ToolFailure):
        print(f"      ✗ {result.code}: {result.message}")
        return
    exit_code = result.data.get("exit_code")
    stdout = result.data.get("stdout", "").rstrip()
    print(f"      → exit {exit_code}, stdout={stdout!r}")


async def main() -> int:
    async with acquire_server() as sock:
        async with await AtdClient.connect(sock) as client:
            print("[atd] connected")

            tools = await client.discover(limit=None)
            print(f"[atd] {len(tools)} tools registered")

            print()
            print('[1/3] ref:echo.say {"text":"hello from ATD"}')
            r = await client.call(
                "ref:echo.say",
                {"text": "hello from ATD"},
                dry_run=False,
            )
            _print_echo(r)

            print()
            print('[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}')
            r = await client.call(
                "ref:fs.glob",
                {"pattern": "**/*.toml", "path": "."},
                dry_run=False,
            )
            _print_glob(r)

            print()
            print('[3/3] ref:shell.exec {"command":"uname -s"}')
            r = await client.call(
                "ref:shell.exec",
                {"command": "uname -s"},
                dry_run=False,
            )
            _print_shell(r)

            print()
            print("[atd] done.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(asyncio.run(main()))
    except KeyboardInterrupt:
        sys.exit(130)
