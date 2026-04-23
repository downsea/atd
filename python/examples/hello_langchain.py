"""Demo of atd-client's LangChain adapter.

Auto-spawns atd-ref-server (same pattern as hello_atd.py) and builds
LangChain tools bound to the live client. Prints tool metadata and
invokes one of them.

Run:
    cargo build --release -p atd-ref-server
    uv pip install -e '.[langchain]'
    uv run python examples/hello_langchain.py

If `atd-client[langchain]` is not installed, exits 0 with a note.
"""

from __future__ import annotations

import asyncio
import os
import signal
import sys
from pathlib import Path

# Gracefully handle missing langchain-core before doing anything else.
try:
    from langchain_core.tools import BaseTool  # noqa: F401
except ImportError:
    print("[skip] langchain_core not installed.")
    print("       Install with: uv pip install -e '.[langchain]'")
    sys.exit(0)

from atd_client import AtdClient  # noqa: E402
from atd_client.adapters import as_langchain_tools  # noqa: E402

SOCKET_WAIT_ATTEMPTS = 30
SOCKET_WAIT_INTERVAL_S = 0.1


def _repo_root() -> Path:
    # This file lives at <root>/python/examples/hello_langchain.py
    return Path(__file__).resolve().parent.parent.parent


async def _wait_for_socket(sock: Path) -> bool:
    for _ in range(SOCKET_WAIT_ATTEMPTS):
        if sock.exists():
            return True
        await asyncio.sleep(SOCKET_WAIT_INTERVAL_S)
    return False


async def _spawn_ref_server():
    """Spawn the ref-server or use ATD_SOCK override.

    Returns (proc, tmpdir_obj) and the socket path.
    When ATD_SOCK is set, returns (None, None) and that path.
    """
    import tempfile

    override = os.environ.get("ATD_SOCK")
    if override:
        print(f"[atd] using ATD_SOCK override → {override}")
        return None, None, Path(override)

    binary = _repo_root() / "target" / "release" / "atd-ref-server"
    if not binary.exists():
        raise SystemExit(
            f"atd-ref-server release binary missing at {binary}.\n"
            "Build with: cargo build --release -p atd-ref-server"
        )

    tmpdir = tempfile.TemporaryDirectory()
    sock = Path(tmpdir.name) / "demo.sock"
    print(f"[atd] auto-spawning atd-ref-server → {sock}")

    proc = await asyncio.create_subprocess_exec(
        str(binary),
        "--sock",
        str(sock),
        stdout=asyncio.subprocess.DEVNULL,
        stderr=asyncio.subprocess.DEVNULL,
    )

    if not await _wait_for_socket(sock):
        proc.kill()
        tmpdir.cleanup()
        raise SystemExit("atd-ref-server didn't bind socket within 3s")

    return proc, tmpdir, sock


async def _teardown(proc, tmpdir) -> None:
    if proc is None:
        return
    if proc.returncode is None:
        proc.send_signal(signal.SIGTERM)
        try:
            await asyncio.wait_for(proc.wait(), timeout=2.0)
        except asyncio.TimeoutError:
            proc.kill()
            await proc.wait()
    if tmpdir is not None:
        tmpdir.cleanup()


async def main() -> int:
    proc, tmpdir, sock = await _spawn_ref_server()
    try:
        async with await AtdClient.connect(sock) as client:
            summaries = await client.discover(limit=None)
            print(f"[demo] {len(summaries)} ATD tools discovered")

            tools = as_langchain_tools(summaries, client=client)
            print(f"[demo] built {len(tools)} LangChain tools\n")

            # Print the first tool's metadata.
            first = tools[0]
            print(f"[demo] first tool  : {first.name}")
            print(f"       description : {first.description}")
            print(f"       args_schema : {first.args_schema.model_json_schema()}\n")

            # Invoke ref_echo_say via its coroutine.
            echo_tool = next(t for t in tools if t.name == "ref_echo_say")
            result = await echo_tool.coroutine(text="hello from langchain adapter")
            print(f"[demo] echo_tool result: {result}")
    finally:
        await _teardown(proc, tmpdir)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(asyncio.run(main()))
    except KeyboardInterrupt:
        sys.exit(130)
