"""Tool registry: register/lookup/list/describe.

Hidden tools (`visibility == ToolVisibility.HIDDEN`) are excluded from
`tool_list` responses but remain reachable via `tool_schema` and `run_tool`
by their explicit id. Mirrors `sp-tool-visibility-hidden`.
"""

from __future__ import annotations

import asyncio
from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import Any

from atd_client.types import (
    ToolDefinition,
    ToolSummary,
    ToolTier,
    ToolVisibility,
)

HandlerFn = Callable[[dict[str, Any], Any], Awaitable[Any]]
# Loose `Any` on ctx + return to keep Phase D free of CallContext / ToolSuccess
# imports — Phase E tightens these into `(args, CallContext) -> ToolSuccess | ToolFailure`.


@dataclass(frozen=True)
class _RegisteredTool:
    definition: ToolDefinition
    handler: HandlerFn


class ToolRegistry:
    """In-memory tool registry. Not thread-safe — owned by one AtdServer."""

    def __init__(self) -> None:
        self._tools: dict[str, _RegisteredTool] = {}

    def register(self, definition: ToolDefinition, handler: HandlerFn) -> None:
        if not definition.id:
            raise ValueError("tool definition must have a non-empty id")
        if definition.id in self._tools:
            raise ValueError(f"duplicate tool id: {definition.id}")
        if not asyncio.iscoroutinefunction(handler):
            raise TypeError(
                f"handler for {definition.id} must be async "
                f"(got {type(handler).__name__})"
            )
        self._tools[definition.id] = _RegisteredTool(definition=definition, handler=handler)

    def summaries(self, *, include_hidden: bool = False) -> list[ToolSummary]:
        out: list[ToolSummary] = []
        for t in self._tools.values():
            if not include_hidden and t.definition.visibility == ToolVisibility.HIDDEN:
                continue
            out.append(self._summary_from_definition(t.definition))
        return out

    def describe(self, tool_id: str) -> ToolDefinition | None:
        rt = self._tools.get(tool_id)
        return rt.definition if rt else None

    def get(self, tool_id: str) -> _RegisteredTool | None:
        return self._tools.get(tool_id)

    def __len__(self) -> int:
        return len(self._tools)

    @staticmethod
    def _summary_from_definition(d: ToolDefinition) -> ToolSummary:
        return ToolSummary(
            id=d.id,
            name=d.name,
            description=d.description,
            domain=d.capability.domain,
            tags=list(d.capability.tags),
            visibility=d.visibility,
            tier=ToolTier.WARM,  # v0.1.0 has no per-definition tier; Phase E uses resources.timeout_ms
            input_schema=d.input_schema or None,
        )
