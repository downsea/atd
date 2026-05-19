"""run_tool dispatch — capability gate, dry-run, tier deadline, error envelope.

Flow:

    1. Validate `tool_id` is a non-empty string; lookup in registry.
        miss -> Response::Error { code: 1000, "tool not found: ..." }
    2. Capability gate: required(domain:action for each action) subset granted?
        deny -> Response::Error { code: 1001, details: {required, granted} }
    3. (Optional) JSONSchema validation of `args` against definition.input_schema.
        fail -> Response::Error { code: 1005, message: <validator-msg> }
    4. dry_run=True -> short-circuit with {args_preview: args} (handler not called).
    5. Build CallContext; wrap handler in asyncio.wait_for(deadline).
        TimeoutError -> Response::ToolResult { success: false, code: 1004 }
        ToolError(code,msg) -> Response::ToolResult { success: false, code, message }
        Exception -> Response::ToolResult { success: false, code: 1099 }
    6. Normalize handler return:
        ToolSuccess -> Response::ToolResult { success: true, result: data }
        ToolFailure -> Response::ToolResult { success: false, code, message, retryable }
        anything-else -> wrapped as ToolSuccess(data=<value>)

Deadline source (v0.1.0): `ToolDefinition.resources.timeout_ms`. ToolDefinition
has no `tier` field today, so we derive the deadline from `resources` rather
than the spec's tier-table. The tier-table (HOT=1s/WARM=30s/COLD=300s) is
applied only as a fallback when `timeout_ms` is 0 (treated as "unset").
"""

from __future__ import annotations

import asyncio
import logging
import secrets
from typing import Any

from atd_client.types import ToolFailure, ToolSuccess
from atd_server.context import CallContext, ConnectionContext
from atd_server.errors import (
    ERR_CAPABILITY_DENIED,
    ERR_DEADLINE_EXCEEDED,
    ERR_INTERNAL,
    ERR_INVALID_ARGS,
    ERR_TOOL_NOT_FOUND,
    ToolError,
    build_error_response,
    build_tool_result_failure,
    build_tool_result_success,
)
from atd_server.registry import ToolRegistry

_log = logging.getLogger("atd_server")

_DEFAULT_DEADLINE_S = 30.0  # WARM fallback when resources.timeout_ms is 0

try:
    import jsonschema  # type: ignore[import-untyped]

    _HAS_JSONSCHEMA = True
except ImportError:  # pragma: no cover — optional dep
    _HAS_JSONSCHEMA = False
    _log.debug("jsonschema not installed; arg validation disabled")


async def dispatch_run_tool(
    request: dict[str, Any],
    *,
    registry: ToolRegistry,
    conn_ctx: ConnectionContext,
    default_deadline_s: float = _DEFAULT_DEADLINE_S,
) -> dict[str, Any]:
    """Dispatch a `Request::RunTool` frame. Returns the wire response dict."""
    tool_id = request.get("tool_id")
    if not isinstance(tool_id, str) or not tool_id:
        return build_error_response(
            code=ERR_INVALID_ARGS,
            message="run_tool requires a non-empty `tool_id`",
        )

    dry_run = bool(request.get("dry_run", False))
    args = request.get("args")
    if args is None:
        args = {}

    registered = registry.get(tool_id)
    if registered is None:
        return build_error_response(
            code=ERR_TOOL_NOT_FOUND,
            message=f"tool not found: {tool_id}",
        )

    definition = registered.definition
    handler = registered.handler

    required = _required_capability_strings(definition.capability.domain, definition.capability.actions)
    missing = sorted(c for c in required if c not in conn_ctx.granted_capabilities)
    if missing:
        return build_error_response(
            code=ERR_CAPABILITY_DENIED,
            message=f"capability denied: missing {missing}",
            details={
                "required": sorted(required),
                "granted": sorted(conn_ctx.granted_capabilities),
                "missing": missing,
            },
        )

    if _HAS_JSONSCHEMA and definition.input_schema:
        try:
            jsonschema.validate(args, definition.input_schema)
        except jsonschema.ValidationError as e:
            return build_error_response(
                code=ERR_INVALID_ARGS,
                message=f"invalid arguments: {e.message}",
            )

    if dry_run:
        return build_tool_result_success(
            tool_id=tool_id,
            data={"args_preview": args},
            dry_run=True,
        )

    deadline_s = _resolve_deadline(definition.resources.timeout_ms, default_deadline_s)
    ctx = CallContext(
        request_id=_generate_request_id(),
        tool_id=tool_id,
        granted_capabilities=conn_ctx.granted_capabilities,
        connection=conn_ctx,
    )

    try:
        result = await asyncio.wait_for(handler(args, ctx), timeout=deadline_s)
    except asyncio.TimeoutError:
        return build_tool_result_failure(
            tool_id=tool_id,
            code=ERR_DEADLINE_EXCEEDED,
            message=f"tool exceeded deadline ({deadline_s:.1f}s)",
            dry_run=False,
        )
    except ToolError as e:
        return build_tool_result_failure(
            tool_id=tool_id,
            code=e.code,
            message=e.message,
            retryable=e.retryable,
            partial_data=e.partial_data,
            dry_run=False,
        )
    except asyncio.CancelledError:
        raise
    except Exception as e:
        _log.exception("internal error in tool %s", tool_id)
        return build_tool_result_failure(
            tool_id=tool_id,
            code=ERR_INTERNAL,
            message=f"internal_error: {type(e).__name__}",
            dry_run=False,
        )

    return _normalize_handler_return(tool_id, result)


def _normalize_handler_return(tool_id: str, result: Any) -> dict[str, Any]:
    if isinstance(result, ToolSuccess):
        return build_tool_result_success(tool_id=tool_id, data=result.data, dry_run=False)
    if isinstance(result, ToolFailure):
        try:
            code: int | str = int(result.code)
        except (TypeError, ValueError):
            code = result.code
        return build_tool_result_failure(
            tool_id=tool_id,
            code=code,
            message=result.message,
            retryable=result.retryable,
            dry_run=False,
        )
    # Plain return value (dict / list / scalar) — wrap as success.
    return build_tool_result_success(tool_id=tool_id, data=result, dry_run=False)


def _required_capability_strings(domain: str, actions: list[str]) -> frozenset[str]:
    """Convention: tool requires `f"{domain}:{action}"` for each action.

    Adopters with a different separator convention pass a custom
    `ServerPolicy` that grants strings shaped to match their own tools.
    """
    if not domain or not actions:
        return frozenset()
    return frozenset(f"{domain}:{action}" for action in actions)


def _resolve_deadline(timeout_ms: int, fallback_s: float) -> float:
    if timeout_ms and timeout_ms > 0:
        return timeout_ms / 1000.0
    return fallback_s


def _generate_request_id() -> str:
    return f"req-{secrets.token_hex(8)}"
