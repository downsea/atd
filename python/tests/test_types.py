from __future__ import annotations

import pytest
from pydantic import ValidationError

from atd_client.types import (
    BindingProtocol,
    ToolDefinition,
    ToolFailure,
    ToolResult,
    ToolSuccess,
    ToolSummary,
    ToolTier,
    ToolVisibility,
)


def test_tool_summary_parses_minimal_anos_shape() -> None:
    raw = {
        "id": "anos:fs.read",
        "description": "Read a file",
        "tier": "hot",
        "visibility": "read",
        "lifecycle": "Active",
    }
    s = ToolSummary.model_validate(raw)
    assert s.id == "anos:fs.read"
    assert s.description == "Read a file"
    assert s.name == ""
    assert s.domain == ""
    assert s.tags == []
    assert s.tier == ToolTier.HOT
    assert s.visibility == ToolVisibility.READ


def test_tool_summary_accepts_pascalcase_enum_values() -> None:
    raw = {
        "id": "anos:fs.write",
        "description": "Write a file",
        "tier": "Hot",
        "visibility": "Write",
    }
    s = ToolSummary.model_validate(raw)
    assert s.tier == ToolTier.HOT
    assert s.visibility == ToolVisibility.WRITE


def test_tool_summary_roundtrips_via_json_in_snake_case() -> None:
    s = ToolSummary(
        id="anos:fs.read",
        name="Read",
        description="Read a file",
        domain="fs",
        tags=["filesystem"],
        visibility=ToolVisibility.READ,
        tier=ToolTier.HOT,
    )
    j = s.model_dump_json()
    assert '"tier":"hot"' in j
    assert '"visibility":"read"' in j
    back = ToolSummary.model_validate_json(j)
    assert back == s


def test_tool_definition_parses_full_anos_shape() -> None:
    raw = {
        "id": "anos:fs.read",
        "name": "File Read",
        "description": "Read the contents of a file",
        "version": "1.0.0",
        "capability": {
            "domain": "fs",
            "actions": ["read"],
            "tags": ["file", "read"],
            "intent_examples": ["read config.toml"],
        },
        "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}},
        "output_schema": {"type": "string"},
        "bindings": [
            {"protocol": "AppFunction", "config": {"function": "anos:fs.read"}}
        ],
        "safety": {
            "level": "Read",
            "dry_run": False,
            "side_effects": [],
            "data_sensitivity": None,
        },
        "resources": {
            "timeout_ms": 5000,
            "max_concurrent": 8,
            "rate_limit_per_min": None,
            "estimated_tokens": None,
        },
        "trust": {"publisher": "anos", "trust_level": "L3Verified", "signature": None},
        "visibility": "read",
    }
    d = ToolDefinition.model_validate(raw)
    assert d.id == "anos:fs.read"
    assert d.capability.domain == "fs"
    assert d.bindings[0].protocol == BindingProtocol.APP_FUNCTION


def test_tool_result_success_roundtrip() -> None:
    raw = {
        "status": "success",
        "data": {"content": "hello"},
        "metadata": {"tool_id": "anos:fs.read"},
    }
    r = ToolResult.validate_python(raw)
    assert isinstance(r, ToolSuccess)
    assert r.data == {"content": "hello"}
    assert r.metadata.tool_id == "anos:fs.read"
    assert r.metadata.timestamp is None
    assert r.metadata.request_id is None


def test_tool_result_error_roundtrip() -> None:
    raw = {
        "status": "error",
        "code": "EPERM",
        "message": "denied",
        "reason": None,
        "retryable": False,
    }
    r = ToolResult.validate_python(raw)
    assert isinstance(r, ToolFailure)
    assert r.code == "EPERM"
    assert not r.retryable


def test_invalid_enum_value_raises() -> None:
    with pytest.raises(ValidationError):
        ToolSummary.model_validate(
            {"id": "x", "description": "d", "tier": "lukewarm", "visibility": "read"}
        )
