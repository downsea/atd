"""LLM-provider tool-format adapters.

Convert :class:`ToolSummary` lists into the JSON shapes required by the
OpenAI, Anthropic, and LangChain function-calling APIs. Per-provider
schema-fetch is not performed — we use the summary's ``input_schema`` when
present, or fall back to a minimal ``{"type": "object"}`` stub. Callers who
need richer schemas per tool can call :meth:`AtdClient.describe` per-tool and
build their own payload.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from atd_client.sanitize import sanitize_tool_name  # noqa: F401 — re-exported

if TYPE_CHECKING:
    from atd_client.types import ToolSummary

# Known namespaces shared with the Rust atd-mcp-bridge.
_KNOWN_NAMESPACES = ("anos", "host", "mock")


def desanitize_tool_name(mcp_name: str) -> str:
    """Reverse sanitize when the namespace is one we know.

    .. deprecated::
        This function uses hardcoded namespace heuristics. Prefer
        :func:`atd_client.sanitize.desanitize_tool_name` with an explicit
        ``known`` id list for accurate round-tripping.
    """
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
                "parameters": s.input_schema or {"type": "object"},
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
            "input_schema": s.input_schema or {"type": "object"},
        }
        for s in summaries
    ]


def as_langchain_tools(
    summaries: list[ToolSummary],
    client: Any | None = None,
) -> list[Any]:
    """Convert ATD tool summaries to LangChain ``StructuredTool`` instances.

    Each returned tool sanitizes its name (``ref:fs.read`` -> ``ref_fs_read``)
    and, when a client is provided, invokes the ATD tool via
    ``await client.call(original_id, args)`` on execution.

    If ``client`` is None, tools are returned but calling them raises
    ``RuntimeError("client not bound")``. Use this form when you only need
    the tool definitions for downstream introspection.

    Requires the ``langchain`` extras::

        pip install 'atd-client[langchain]'

    Raises:
        ImportError: if langchain-core is not installed, with an install hint.
    """
    try:
        from langchain_core.tools import StructuredTool
        from pydantic import Field, create_model
    except ImportError as exc:  # pragma: no cover - tested via patched import
        raise ImportError(
            "as_langchain_tools() requires the 'langchain' extra. "
            "Install with: pip install 'atd-client[langchain]'"
        ) from exc

    return [
        _make_langchain_tool(summary, client, StructuredTool, create_model, Field)
        for summary in summaries
    ]


def _make_langchain_tool(
    summary: Any,
    client: Any | None,
    StructuredTool: Any,
    create_model: Any,
    Field: Any,
) -> Any:
    """Build one LangChain StructuredTool from an ATD ToolSummary."""
    original_id: str = summary.id
    sanitized = sanitize_tool_name(original_id)
    schema_dict: dict[str, Any] = summary.input_schema or {
        "type": "object",
        "properties": {},
    }
    args_model = _build_pydantic_model(f"{sanitized}_args", schema_dict, create_model, Field)

    async def _arun(**kwargs: Any) -> Any:
        if client is None:
            raise RuntimeError(
                f"ATD tool '{original_id}' has no client bound; "
                "pass client=<AtdClient> to as_langchain_tools()"
            )
        result = await client.call(original_id, kwargs)
        # ToolFailure has .code / .message; ToolSuccess has .data.
        if hasattr(result, "code") and hasattr(result, "message"):
            raise RuntimeError(f"[{result.code}] {result.message}")
        return result.data

    return StructuredTool.from_function(
        coroutine=_arun,
        name=sanitized,
        description=summary.description,
        args_schema=args_model,
    )


def _build_pydantic_model(
    name: str,
    schema: dict[str, Any],
    create_model: Any,
    Field: Any,
) -> Any:
    """Convert a JSON Schema 'object' into a Pydantic v2 model.

    Minimum viable subset: type:object with properties of type string /
    integer / number / boolean / array / object. Anything more complex
    maps to ``Any``.
    """
    from typing import Any as AnyType

    props: dict[str, Any] = schema.get("properties", {})
    required: set[str] = set(schema.get("required", []))

    type_map: dict[str, type] = {
        "string": str,
        "integer": int,
        "number": float,
        "boolean": bool,
        "array": list,
        "object": dict,
    }

    fields: dict[str, Any] = {}
    for field_name, spec in props.items():
        py_type = type_map.get(spec.get("type", ""), AnyType)
        default = ... if field_name in required else None
        fields[field_name] = (
            py_type,
            Field(default, description=spec.get("description", "")),
        )

    if not fields:
        # Pydantic requires at least one field; use a permissive model.
        fields["_extra"] = (AnyType, Field(None))

    return create_model(name, **fields)
