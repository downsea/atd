"""Phase G — Python server passes a representative subset of `atd-conformance`.

The Rust fixture corpus at `crates/atd-conformance/fixtures/` is the
protocol-level conformance suite. Phase G exercises ~22 of the ~24
fixtures against `AtdServer` (skipping rate-limit + raw frame codec
fixtures that are out of v1 scope). Full Python conformance runner CLI
is `SP-conformance-py-v1` (depends on this SP).

Setup: each fixture runs against a fresh `AtdServer` configured with a
small reference-tool registry that satisfies the fixture's referenced
tool ids (ref:echo.say, ref:fs.read, ref:conformance.denied_op,
ref:conformance.hidden_op).
"""

from __future__ import annotations

import asyncio
import json
from collections.abc import Iterable
from pathlib import Path
from typing import Any

import pytest

from atd_client.types import ToolVisibility
from atd_client.wire import read_frame, write_frame
from atd_server import AtdServer, CallContext, GrantedCapabilities

from ._helpers import make_definition, spawn, stop_and_wait

_FIXTURES_ROOT = (
    Path(__file__).resolve().parents[2] / "crates" / "atd-conformance" / "fixtures"
)

# v1 scope: skip rate-limit and pure wire-codec fixtures.
_SKIP_FIXTURES = {
    "rate_limited_returns_code_1002",       # rate limiting not implemented in v1
    "frame_length_big_endian_u32",          # covered by atd_client.wire unit tests
}

# Reference policy: grants {"read"} only. Mirrors the Rust ref-server's
# `SharedServerConfig.granted_capabilities` allow-list semantics.
_REF_ALLOW = frozenset({"read"})


async def _ref_policy(
    hello: dict[str, Any], ucan_tokens: tuple[str, ...]
) -> GrantedCapabilities:
    requested = hello.get("requested_capabilities") or []
    if not isinstance(requested, list):
        return GrantedCapabilities(capabilities=frozenset())
    granted = {str(c) for c in requested if c in _REF_ALLOW}
    return GrantedCapabilities(capabilities=frozenset(granted))


def _build_reference_server(sock: str) -> AtdServer:
    """Register the reference tools the conformance fixtures expect."""
    server = AtdServer(socket_path=sock, policy=_ref_policy)

    @server.register(definition=make_definition("ref:echo.say", name="Echo"))
    async def echo(args: dict, ctx: CallContext) -> dict:
        return {"echoed": args}

    @server.register(
        definition=make_definition(
            "ref:fs.read",
            name="Read file",
            required_capabilities=["read"],
            input_schema={
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
            },
        )
    )
    async def fs_read(args: dict, ctx: CallContext) -> dict:
        return {"path": args["path"], "contents": "<stub>"}

    @server.register(
        definition=make_definition(
            "ref:conformance.denied_op",
            name="Denied op",
            required_capabilities=["conformance.denied"],
        )
    )
    async def denied(args: dict, ctx: CallContext) -> dict:
        return {}

    @server.register(
        definition=make_definition(
            "ref:conformance.hidden_op",
            name="Hidden op",
            visibility=ToolVisibility.HIDDEN,
        )
    )
    async def hidden(args: dict, ctx: CallContext) -> dict:
        return {}

    return server


# --------------------------------------------------------------------------
# fixture discovery + collection
# --------------------------------------------------------------------------


def _load_fixtures() -> list[tuple[str, Path, dict[str, Any]]]:
    out: list[tuple[str, Path, dict[str, Any]]] = []
    for category in ("wire", "behavior"):
        for path in sorted((_FIXTURES_ROOT / category).glob("*.json")):
            raw = json.loads(path.read_text(encoding="utf-8"))
            name = str(raw.get("name", path.stem))
            if name in _SKIP_FIXTURES:
                continue
            out.append((name, path, raw))
    return out


_FIXTURES = _load_fixtures()


# --------------------------------------------------------------------------
# partial-match helper
# --------------------------------------------------------------------------


def _matches_subset(actual: Any, expected: Any) -> bool:
    """Return True iff `actual` satisfies the `expected` subset spec.

    - `expected == "*"` matches anything.
    - dict: every key in expected must be present in actual and recurse.
    - list: same length, pairwise recurse.
    - scalar: equality.
    """
    if expected == "*":
        return True
    if isinstance(expected, dict):
        if not isinstance(actual, dict):
            return False
        return all(k in actual and _matches_subset(actual[k], v) for k, v in expected.items())
    if isinstance(expected, list):
        if not isinstance(actual, list):
            return False
        if len(actual) != len(expected):
            return False
        return all(_matches_subset(a, e) for a, e in zip(actual, expected, strict=False))
    return actual == expected


# --------------------------------------------------------------------------
# the test
# --------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("fixture_name", "fixture"),
    [(n, raw) for n, _, raw in _FIXTURES],
    ids=[n for n, _, _ in _FIXTURES],
)
async def test_conformance_fixture(
    fixture_name: str, fixture: dict[str, Any], tmp_path: Path
) -> None:
    sock = str(tmp_path / "atd.sock")
    server = _build_reference_server(sock)
    task = await spawn(server)
    try:
        reader, writer = await asyncio.open_unix_connection(sock)
        try:
            await _apply_setup(reader, writer, fixture.get("setup"))
            await write_frame(writer, fixture["send"])
            actual = await asyncio.wait_for(read_frame(reader), timeout=3.0)
        finally:
            writer.close()
            await writer.wait_closed()
    finally:
        await stop_and_wait(server, task)

    assert isinstance(actual, dict), f"{fixture_name}: response is not an object: {actual!r}"
    expected = fixture["expect_response_matches"]
    assert _matches_subset(actual, expected), (
        f"{fixture_name}: subset match failed\n"
        f"  expected: {expected!r}\n"
        f"  actual:   {actual!r}"
    )

    extra_exclude = fixture.get("expect_tools_exclude")
    if extra_exclude:
        tools = actual.get("tools", [])
        ids = {t.get("id") for t in tools if isinstance(t, dict)}
        for excluded in extra_exclude:
            assert excluded not in ids, (
                f"{fixture_name}: tool {excluded!r} should be excluded from tool_list; ids={ids}"
            )


async def _apply_setup(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    setup: Any,
) -> None:
    if setup is None:
        return
    if not isinstance(setup, dict):
        return
    kind = setup.get("kind")
    if kind == "hello":
        await write_frame(
            writer,
            {
                "type": "hello",
                "client_id": setup.get("client_id", "conformance"),
                "requested_capabilities": setup.get("requested_capabilities", []),
            },
        )
        await asyncio.wait_for(read_frame(reader), timeout=2.0)  # consume the ack


# --------------------------------------------------------------------------
# meta-test: surface the count of v1-relevant fixtures so a future shrinkage
# is visible in test output.
# --------------------------------------------------------------------------


def test_phase_g_runs_at_least_18_conformance_fixtures() -> None:
    """v1 scope target: ~18+ of the ~24 fixtures. Drops to <18 should be
    investigated (likely a regression in scope or a fixture removed upstream)."""
    runnable: Iterable[str] = (n for n, _, _ in _FIXTURES)
    assert sum(1 for _ in runnable) >= 18
