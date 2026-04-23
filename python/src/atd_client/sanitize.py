"""Tool-id <-> sanitized-name mapping. Mirrors atd_client/src/sanitize.rs.

Kept in sync via a shared test fixture (see python/tests/test_sanitize.py).
If the Rust implementation is updated, this file and the fixture must be
updated together.
"""

from __future__ import annotations

from collections.abc import Iterable


def sanitize_tool_name(tool_id: str) -> str:
    """Map an ATD tool id to an LLM-/MCP-safe name.

    Any character outside [a-zA-Z0-9_-] becomes '_'. See the Rust
    counterpart for the authoritative rule set.

    Examples:
        >>> sanitize_tool_name("ref:fs.read")
        'ref_fs_read'
        >>> sanitize_tool_name("xiaomi:light.toggle")
        'xiaomi_light_toggle'
    """
    return "".join(
        c if (c.isascii() and (c.isalnum() or c in "_-")) else "_" for c in tool_id
    )


def desanitize_tool_name(sanitized: str, known: Iterable[str]) -> str | None:
    """Reverse-map by searching the given set of known original ids.

    Returns the first original id whose sanitization matches, or None.
    If multiple original ids sanitize to the same form, returns the first
    match from the iteration order of ``known``.
    """
    for original in known:
        if sanitize_tool_name(original) == sanitized:
            return original
    return None
