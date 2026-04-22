"""LLM-provider tool-format adapters.

Convert :class:`ToolSummary` lists into the JSON shapes required by the
OpenAI and Anthropic function-calling APIs. Per-provider schema-fetch is not
performed — we ship a minimal ``{"type": "object"}`` stub, matching the
atd-mcp-bridge's policy. Callers who need richer schemas per tool can call
:meth:`AtdClient.describe` per-tool and build their own payload.
"""

from __future__ import annotations

from typing import Any

from atd_client.types import ToolSummary

# Known namespaces shared with the Rust atd-mcp-bridge.
_KNOWN_NAMESPACES = ("anos", "host", "mock")


def sanitize_tool_name(tool_id: str) -> str:
    """``anos:fs.read`` → ``anos_fs_read``."""
    return tool_id.replace(":", "_").replace(".", "_")


def desanitize_tool_name(mcp_name: str) -> str:
    """Reverse sanitize when the namespace is one we know."""
    for ns in _KNOWN_NAMESPACES:
        prefix = f"{ns}_"
        if mcp_name.startswith(prefix):
            rest = mcp_name[len(prefix) :]
            if "_" in rest:
                domain, _, action = rest.partition("_")
                return f"{ns}:{domain}.{action.replace('_', '.')}"
            return f"{ns}:{rest}"
    return mcp_name


def as_openai_tools(summaries: list[ToolSummary]) -> list[dict[str, Any]]:
    """Emit the OpenAI function-calling tool array."""
    return [
        {
            "type": "function",
            "function": {
                "name": sanitize_tool_name(s.id),
                "description": s.description or s.name or s.id,
                "parameters": {"type": "object"},
            },
        }
        for s in summaries
    ]


def as_anthropic_tools(summaries: list[ToolSummary]) -> list[dict[str, Any]]:
    """Emit the Anthropic native-tool-use array."""
    return [
        {
            "name": sanitize_tool_name(s.id),
            "description": s.description or s.name or s.id,
            "input_schema": {"type": "object"},
        }
        for s in summaries
    ]
