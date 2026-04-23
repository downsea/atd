"""Tests for atd_client.sanitize. Uses the shared fixture to ensure
the Python and Rust implementations stay in sync."""

from __future__ import annotations

import pytest

from atd_client.sanitize import desanitize_tool_name, sanitize_tool_name

# Shared fixture — same list must produce same outputs in Rust and Python.
# If you add a case here, add the matching Rust test too.
_SANITIZE_CASES = [
    ("echo_say", "echo_say"),
    ("tool-name", "tool-name"),
    ("ref:fs.read", "ref_fs_read"),
    ("xiaomi:light.toggle", "xiaomi_light_toggle"),
    ("a/b c+d", "a_b_c_d"),
]


@pytest.mark.parametrize("input_id,expected", _SANITIZE_CASES)
def test_sanitize(input_id: str, expected: str) -> None:
    assert sanitize_tool_name(input_id) == expected


def test_desanitize_round_trips() -> None:
    known = ["ref:fs.read", "ref:shell.exec", "ref:echo.say"]
    assert desanitize_tool_name("ref_shell_exec", known) == "ref:shell.exec"


def test_desanitize_returns_none_for_unknown() -> None:
    assert desanitize_tool_name("unknown_tool", ["ref:fs.read"]) is None
