"""Server-side errors and error-code constants.

Numeric error-code allocation reconciled with `crates/atd-protocol/src/messages.rs`:

    1001  CAPABILITY_DENIED      (Rust: ERR_CAPABILITY_DENIED)
    1002  RATE_LIMITED           (Rust: ERR_RATE_LIMITED)            — not yet emitted by atd_server
    1003  BROKER_FAILED          (Rust: ERR_BROKER_FAILED)            — not yet emitted by atd_server
    1004  DEADLINE_EXCEEDED      (new in atd_server v1; spec drift fixed in Phase E commit)
    1005  INVALID_ARGS           (new in atd_server v1; spec drift fixed in Phase E commit)
    1000  TOOL_NOT_FOUND         (no Rust constant; server-side convention)
    1099  INTERNAL_ERROR         (no Rust constant; server-side convention)
"""

from __future__ import annotations

from typing import Any

ERR_TOOL_NOT_FOUND = 1000
ERR_CAPABILITY_DENIED = 1001
ERR_RATE_LIMITED = 1002
ERR_BROKER_FAILED = 1003
ERR_DEADLINE_EXCEEDED = 1004
ERR_INVALID_ARGS = 1005
ERR_INTERNAL = 1099


class ToolError(Exception):
    """Raise from a handler to return a typed failure envelope.

    Adopter-namespace codes (cbrain 2000-2099, healthkit 3000-3099, celia
    4000-4099 per SP-error-namespace-v1) should use `code` in those ranges.
    The server does not validate adopter codes in v1.
    """

    def __init__(
        self,
        code: int,
        message: str,
        *,
        partial_data: Any = None,
        retryable: bool = False,
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.partial_data = partial_data
        self.retryable = retryable


def build_error_response(
    *,
    code: int,
    message: str,
    retryable: bool = False,
    details: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Build a top-level `Response::Error` envelope.

    Use for protocol-level failures (capability denial, tool not found,
    malformed request). Handler-level failures use `build_tool_result_failure`.
    """
    out: dict[str, Any] = {
        "type": "error",
        "code": code,
        "message": message,
        "retryable": retryable,
    }
    if details is not None:
        out["details"] = details
    return out


def build_tool_result_success(
    *,
    tool_id: str,
    data: Any,
    dry_run: bool,
) -> dict[str, Any]:
    return {
        "type": "tool_result",
        "tool_id": tool_id,
        "result": data,
        "success": True,
        "dry_run": dry_run,
    }


def build_tool_result_failure(
    *,
    tool_id: str,
    code: int | str,
    message: str,
    retryable: bool = False,
    partial_data: Any = None,
    dry_run: bool = False,
) -> dict[str, Any]:
    """Build a `Response::ToolResult { success: false }` envelope.

    The Python client (`AtdClient.call`) reads `result.code` / `result.message`
    / `result.retryable` and constructs a `ToolFailure`. We mirror that shape.
    `partial_data` is folded into `result.partial_data` when present.
    """
    inner: dict[str, Any] = {
        "code": code,
        "message": message,
        "retryable": retryable,
    }
    if partial_data is not None:
        inner["partial_data"] = partial_data
    return {
        "type": "tool_result",
        "tool_id": tool_id,
        "result": inner,
        "success": False,
        "dry_run": dry_run,
    }
