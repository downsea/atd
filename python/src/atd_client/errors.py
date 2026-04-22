"""Error hierarchy for the ATD client.

Mirrors the Rust `atd-types::AtdError` enum one-to-one. Each variant is a
subclass of :class:`AtdError` so callers can either ``except AtdError`` for a
catch-all or match specific types. ``suggest_fix()`` returns an actionable
hint; ``is_retryable()`` classifies transient failures.
"""

from __future__ import annotations


class AtdError(Exception):
    """Base class for all ATD client errors."""

    def is_retryable(self) -> bool:
        return False

    def suggest_fix(self) -> str | None:
        return None


class ToolNotFound(AtdError):
    def __init__(self, *, tool_id: str, suggestions: list[str]) -> None:
        super().__init__(f"tool not found: {tool_id}")
        self.tool_id = tool_id
        self.suggestions = suggestions

    def suggest_fix(self) -> str | None:
        if self.suggestions:
            return f"did you mean '{self.suggestions[0]}'?"
        return "try `atd list --query <keyword>` to find available tools"


class InvalidArguments(AtdError):
    def __init__(self, *, tool_id: str, field: str, reason: str) -> None:
        super().__init__(f"invalid arguments for {tool_id}: field `{field}` — {reason}")
        self.tool_id = tool_id
        self.field = field
        self.reason = reason


class CapabilityDenied(AtdError):
    def __init__(self, *, tool_id: str, required: list[str], granted: list[str]) -> None:
        super().__init__(
            f"capability denied for {tool_id}: required={required} granted={granted}"
        )
        self.tool_id = tool_id
        self.required = required
        self.granted = granted

    def suggest_fix(self) -> str | None:
        return f"run `atd allow {self.tool_id}` to grant for this session"


class BindingUnavailable(AtdError):
    def __init__(self, *, tool_id: str, tried: list[str], reason: str) -> None:
        super().__init__(
            f"no binding available for {tool_id}: tried={tried} ({reason})"
        )
        self.tool_id = tool_id
        self.tried = tried
        self.reason = reason

    def is_retryable(self) -> bool:
        return True


class ToolExecutionFailed(AtdError):
    def __init__(self, *, tool_id: str, inner: BaseException) -> None:
        super().__init__(f"tool execution failed: {tool_id}")
        self.tool_id = tool_id
        self.__cause__ = inner


class Timeout(AtdError):
    def __init__(self, *, tool_id: str, after_ms: int) -> None:
        super().__init__(f"timed out calling {tool_id} after {after_ms}ms")
        self.tool_id = tool_id
        self.after_ms = after_ms

    def is_retryable(self) -> bool:
        return True

    def suggest_fix(self) -> str | None:
        return f"increase timeout or retry; tool_id={self.tool_id}"


class ServerUnreachable(AtdError):
    def __init__(self, reason: str) -> None:
        super().__init__(f"server unreachable: {reason}")
        self.reason = reason

    def is_retryable(self) -> bool:
        return True

    def suggest_fix(self) -> str | None:
        return "is the ANOS daemon running? try `anos daemon status`"


class NotImplementedFeature(AtdError):
    def __init__(self, *, feature: str) -> None:
        super().__init__(f"not implemented: {feature}")
        self.feature = feature


class ProtocolError(AtdError):
    def __init__(self, *, expected: str, got: str) -> None:
        super().__init__(f"protocol error: expected {expected}, got {got}")
        self.expected = expected
        self.got = got
