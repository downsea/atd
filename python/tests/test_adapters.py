from __future__ import annotations

from atd_client import ToolSummary, ToolTier, ToolVisibility
from atd_client.adapters import (
    as_anthropic_tools,
    as_openai_tools,
    desanitize_tool_name,
    sanitize_tool_name,
)


def _sample_summaries() -> list[ToolSummary]:
    return [
        ToolSummary(
            id="anos:fs.read",
            name="Read File",
            description="Read a file from disk",
            domain="fs",
            tags=[],
            visibility=ToolVisibility.READ,
            tier=ToolTier.HOT,
        ),
        ToolSummary(
            id="host:media.convert",
            name="Convert Media",
            description="Convert a media file",
            domain="media",
            tags=[],
            visibility=ToolVisibility.DANGEROUS,
            tier=ToolTier.WARM,
        ),
    ]


def test_sanitize_replaces_colon_and_dot() -> None:
    assert sanitize_tool_name("anos:fs.read") == "anos_fs_read"
    assert sanitize_tool_name("host:media.convert") == "host_media_convert"


def test_desanitize_recovers_id_for_known_namespaces() -> None:
    assert desanitize_tool_name("anos_fs_read") == "anos:fs.read"
    assert desanitize_tool_name("host_media_convert") == "host:media.convert"
    assert desanitize_tool_name("weird_thing") == "weird_thing"


def test_as_openai_tools_emits_function_shape() -> None:
    out = as_openai_tools(_sample_summaries())
    assert len(out) == 2
    first = out[0]
    assert first["type"] == "function"
    assert first["function"]["name"] == "anos_fs_read"
    assert first["function"]["description"] == "Read a file from disk"
    assert first["function"]["parameters"]["type"] == "object"


def test_as_anthropic_tools_emits_native_shape() -> None:
    out = as_anthropic_tools(_sample_summaries())
    assert len(out) == 2
    assert out[0]["name"] == "anos_fs_read"
    assert out[0]["description"] == "Read a file from disk"
    assert out[0]["input_schema"]["type"] == "object"


def test_sanitize_desanitize_roundtrip() -> None:
    for tid in ["anos:fs.read", "anos:web.search", "host:media.convert"]:
        assert desanitize_tool_name(sanitize_tool_name(tid)) == tid
