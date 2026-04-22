from atd_client.errors import (
    AtdError,
    BindingUnavailable,
    CapabilityDenied,
    InvalidArguments,
    ProtocolError,
    ServerUnreachable,
    Timeout,
    ToolExecutionFailed,
    ToolNotFound,
)


def test_tool_not_found_suggests_candidate_when_present() -> None:
    e = ToolNotFound(tool_id="fs.red", suggestions=["fs.read"])
    assert e.suggest_fix() == "did you mean 'fs.read'?"
    assert not e.is_retryable()


def test_tool_not_found_hints_discovery_when_no_suggestions() -> None:
    e = ToolNotFound(tool_id="x", suggestions=[])
    assert "atd list" in (e.suggest_fix() or "")


def test_timeout_is_retryable() -> None:
    e = Timeout(tool_id="fs.read", after_ms=5000)
    assert e.is_retryable()


def test_server_unreachable_is_retryable() -> None:
    e = ServerUnreachable("connection refused")
    assert e.is_retryable()
    assert "daemon" in (e.suggest_fix() or "").lower()


def test_capability_denied_suggests_allow_command() -> None:
    e = CapabilityDenied(tool_id="fs.delete", required=["w"], granted=[])
    hint = e.suggest_fix() or ""
    assert "atd allow" in hint and "fs.delete" in hint


def test_protocol_error_has_no_default_hint() -> None:
    e = ProtocolError(expected="pong", got="hello")
    assert e.suggest_fix() is None
    assert not e.is_retryable()


def test_all_are_subclasses_of_atd_error() -> None:
    for cls in (
        ToolNotFound,
        InvalidArguments,
        CapabilityDenied,
        BindingUnavailable,
        ToolExecutionFailed,
        Timeout,
        ServerUnreachable,
        ProtocolError,
    ):
        assert issubclass(cls, AtdError), cls


def test_display_message_includes_tool_id_for_invalid_arguments() -> None:
    e = InvalidArguments(tool_id="fs.read", field="path", reason="must be string")
    s = str(e)
    assert "fs.read" in s
    assert "path" in s


def test_binding_unavailable_is_retryable() -> None:
    e = BindingUnavailable(tool_id="x", tried=["cli", "mcp"], reason="both down")
    assert e.is_retryable()
