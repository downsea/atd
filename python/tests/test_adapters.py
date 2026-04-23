from __future__ import annotations

import sys
from unittest.mock import AsyncMock, patch

import pytest

from atd_client import ToolSummary, ToolTier, ToolVisibility
from atd_client.adapters import (
    as_anthropic_tools,
    as_langchain_tools,
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


# ---------------------------------------------------------------------------
# LangChain adapter tests
# ---------------------------------------------------------------------------


def _fake_summary(
    id: str = "ref:echo.say",
    desc: str = "echo test",
) -> ToolSummary:
    """Build a fake ToolSummary with a simple input_schema."""
    return ToolSummary(
        id=id,
        name=id,
        description=desc,
        domain="test",
        tier=ToolTier.WARM,
        visibility=ToolVisibility.READ,
        tags=[],
        input_schema={
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"],
        },
    )


@pytest.mark.skipif(
    not pytest.importorskip("langchain_core", reason="langchain_core not installed"),
    reason="langchain_core not installed",
)
class TestLangChainAdapter:
    def test_returns_structured_tool_list(self) -> None:
        tools = as_langchain_tools([_fake_summary()])
        assert len(tools) == 1
        # Tool name must be the sanitized form.
        assert tools[0].name == "ref_echo_say"

    @pytest.mark.asyncio
    async def test_arun_invokes_client_call_with_original_id(self) -> None:
        from atd_client.types import ToolResultMetadata, ToolSuccess

        mock_client = AsyncMock()
        mock_client.call.return_value = ToolSuccess(
            data={"echoed": "hi"},
            metadata=ToolResultMetadata(tool_id="ref:echo.say"),
        )

        tools = as_langchain_tools([_fake_summary()], client=mock_client)
        result = await tools[0].coroutine(text="hi")

        assert result == {"echoed": "hi"}
        # Must have called with the ORIGINAL (unsanitized) tool id.
        mock_client.call.assert_awaited_once_with("ref:echo.say", {"text": "hi"})


def test_missing_extras_raises_helpful_import_error() -> None:
    """Simulate langchain_core being unavailable by blocking its import."""
    # Cache any currently loaded langchain_core modules so we can restore them.
    cached = {
        k: v for k, v in sys.modules.items() if k == "langchain_core" or k.startswith("langchain_core.")
    }
    for k in cached:
        del sys.modules[k]

    import builtins

    real_import = builtins.__import__

    def fake_import(name: str, *args: object, **kwargs: object) -> object:
        if name.startswith("langchain_core"):
            raise ImportError("No module named 'langchain_core'")
        return real_import(name, *args, **kwargs)

    with patch("builtins.__import__", side_effect=fake_import):
        with pytest.raises(ImportError, match=r"pip install.*atd-client\[langchain\]"):
            as_langchain_tools([_fake_summary()])

    # Restore cached modules.
    for k, v in cached.items():
        sys.modules[k] = v
