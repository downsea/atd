# Phase 1 — Python SDK Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `atd-client` on PyPI — an idiomatic Python SDK for the ATD protocol with an async `AtdClient`, a sync wrapper for legacy call sites, and LLM-adapter helpers that convert ATD tools to OpenAI / Anthropic function-calling formats.

**Architecture:** Single-package Python 3.10+ library at `python/` (next to the existing `crates/` Rust tree, per design.md §4). Pydantic v2 models mirror the Rust `atd-types` shapes and reuse the same wire format (length-prefixed JSON over Unix socket). The async core (`AtdClient`) uses `asyncio` + `asyncio.open_unix_connection`. `AtdClientSync` is a thin wrapper that owns its own event loop for pre-async call sites (LangChain entrypoints, notebooks). LLM adapters are pure functions that take `list[ToolSummary]` and emit provider-specific JSON, with `tool_id` → `mcp_name` sanitization matching what `atd-mcp-bridge` already does.

**Tech Stack:** Python 3.10+ · asyncio (stdlib) · Pydantic v2 (protocol types) · pytest + pytest-asyncio (tests) · ruff (lint) · mypy (type-check) · uv (packaging/env). No Pydantic-settings, no Trio, no LangChain — LangChain belongs in a sibling package (`atd-langchain`) deferred to its own plan.

**Wire interop:** Rust fixture JSON captured in Phase 0.5 (`crates/atd-client/tests/fixtures/anos_tool_list.json` and `anos_tool_schema_fs_read.json`) is the canonical contract. The Python SDK must parse both verbatim — this catches schema drift in the same way the Rust contract test does.

**Scope boundary:**
- **In scope:** Package scaffold, protocol types (Pydantic), wire codec, async `AtdClient` with `ping`/`discover`/`describe`/`call`, sync wrapper, OpenAI + Anthropic adapters, mock-server integration test, ANOS-fixture contract test, `hello_atd.py` example, PyPI-ready `pyproject.toml`.
- **Out of scope (explicit defers):**
  - `session` / `cancel` / `subscribe` APIs — ANOS server doesn't expose them over IPC (design.md §3.6 defers to Phase 2; also see `docs/issues/2026-04-21-atd-run-tool-stub.md`).
  - stdio transport, MCP-compat transport — separate plan; Unix socket is Phase 1.
  - LangChain adapter + `atd-langchain` package — depends on `langchain-core` dep; own plan.
  - TypeScript SDK — parallel work, separate plan.
  - PyPI publishing (actual `twine upload`) — done out-of-band by the maintainer once the org is created.

**Prerequisites:**
- atd-mvp at `phase0-weeks2-3` tag (92 Rust tests passing).
- Local: Python ≥ 3.10 (3.14 verified on this env), `uv` installed, pydantic 2.x reachable.
- ANOS daemon running at `~/.anos/anos.sock` for optional live smoke — not required for tests.

**Exit criteria:**
1. `cd python && uv sync` installs deps cleanly.
2. `cd python && uv run pytest` passes all tests (≥ 25 new tests).
3. `cd python && uv run mypy src` reports no errors.
4. `cd python && uv run ruff check` reports no violations.
5. `uv run python examples/hello_atd.py` against live ANOS prints 108 tools (same result as Rust `hello_atd`).
6. `python/README.md` has a 10-line Python quickstart.
7. `pyproject.toml` declares `[project]` metadata suitable for `uv build` → wheel+sdist.
8. Zero regressions: `cargo test --workspace` still 92 passing.

---

## File Structure

```
atd-mvp/
├── python/                                       (NEW tree)
│   ├── pyproject.toml                            (package metadata, deps, tool configs)
│   ├── README.md                                 (Python quickstart + API overview)
│   ├── .python-version                           (uv pin: 3.10)
│   ├── src/
│   │   └── atd_client/
│   │       ├── __init__.py                       (re-exports public API)
│   │       ├── errors.py                         (AtdError + subclass hierarchy)
│   │       ├── types.py                          (Pydantic: ToolSummary/Definition/Result)
│   │       ├── wire.py                           (async length-prefixed JSON codec)
│   │       ├── protocol.py                       (Request/Response message shapes)
│   │       ├── transport.py                      (Unix socket connect)
│   │       ├── client.py                         (AtdClient async)
│   │       ├── sync.py                           (AtdClientSync wrapper)
│   │       └── adapters.py                       (as_openai_tools, as_anthropic_tools)
│   ├── tests/
│   │   ├── conftest.py                           (pytest-asyncio + mock-server fixture)
│   │   ├── test_errors.py
│   │   ├── test_types.py
│   │   ├── test_wire.py
│   │   ├── test_client.py                        (uses mock-server fixture)
│   │   ├── test_sync.py
│   │   ├── test_adapters.py
│   │   └── test_anos_fixture.py                  (contract test, reuses Rust fixtures)
│   └── examples/
│       └── hello_atd.py                          (matches Rust examples/hello_atd.rs)
└── README.md                                     (MODIFY — add Python quickstart link)
```

**Responsibility rationale:**
- Single module per concern (errors / types / wire / protocol / transport / client / sync / adapters), each < 200 lines. Matches the Rust `atd-client` split exactly so a reader can cross-read.
- Pydantic v2 in `types.py` gives automatic JSON roundtrip and field validation — saves ~200 lines of hand-written `from_dict`/`to_dict` boilerplate and catches bad server responses at the boundary.
- `protocol.py` holds Request/Response envelope types (serialized as `type`-tagged JSON, matching the Rust `protocol.rs`).
- `client.py` is async-first; `sync.py` owns a private event loop so callers with no asyncio experience can use `AtdClientSync.call(...)` straight from a script.
- Tests mirror module names 1:1 so failure locality is obvious.
- Contract test imports the exact same JSON fixtures that Rust uses — single source of truth for "what ANOS actually sends".

---

## Task 1: Package Scaffold + Dependencies

**Files:**
- Create: `python/pyproject.toml`
- Create: `python/.python-version`
- Create: `python/src/atd_client/__init__.py` (empty marker for now)
- Create: `python/tests/__init__.py` (empty)
- Create: `python/.gitignore`

- [ ] **Step 1.1: Write `pyproject.toml`**

Create `/home/nan/proj/atd-mvp/python/pyproject.toml`:

```toml
[project]
name = "atd-client"
version = "0.1.0"
description = "Reference Python client SDK for the Agent Tool Dispatch (ATD) protocol."
readme = "README.md"
requires-python = ">=3.10"
license = "Apache-2.0"
authors = [{ name = "ATD Protocol Contributors" }]
keywords = ["atd", "agent", "tool", "llm", "mcp"]
classifiers = [
    "Development Status :: 3 - Alpha",
    "Intended Audience :: Developers",
    "License :: OSI Approved :: Apache Software License",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3 :: Only",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: 3.13",
]
dependencies = [
    "pydantic>=2.0,<3",
]

[project.urls]
Homepage = "https://github.com/atd-protocol/atd-mvp"
Repository = "https://github.com/atd-protocol/atd-mvp"

[dependency-groups]
dev = [
    "pytest>=8",
    "pytest-asyncio>=0.23",
    "pytest-cov>=5",
    "ruff>=0.6",
    "mypy>=1.10",
]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
packages = ["src/atd_client"]

[tool.pytest.ini_options]
asyncio_mode = "auto"
testpaths = ["tests"]
addopts = "-ra --strict-markers"

[tool.ruff]
line-length = 100
target-version = "py310"

[tool.ruff.lint]
select = ["E", "F", "I", "N", "UP", "B", "SIM", "ASYNC"]
ignore = ["E501"]  # let the formatter handle line length

[tool.mypy]
python_version = "3.10"
strict = true
warn_return_any = true
warn_unused_configs = true
disallow_untyped_defs = true
packages = ["atd_client"]
mypy_path = "src"
```

- [ ] **Step 1.2: Write `.python-version`**

Create `/home/nan/proj/atd-mvp/python/.python-version`:

```
3.10
```

(Note: local env has Python 3.14 — that's fine; `.python-version` is a floor, uv picks the newest matching.)

- [ ] **Step 1.3: Write empty marker files**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/__init__.py`:

```python
"""atd-client — reference Python SDK for the Agent Tool Dispatch protocol."""

__version__ = "0.1.0"
```

Create `/home/nan/proj/atd-mvp/python/tests/__init__.py` (empty file):

```python
```

- [ ] **Step 1.4: Write `.gitignore`**

Create `/home/nan/proj/atd-mvp/python/.gitignore`:

```
__pycache__/
*.py[cod]
*$py.class
.pytest_cache/
.mypy_cache/
.ruff_cache/
.coverage
htmlcov/
dist/
build/
*.egg-info/
.venv/
.python-version-*
```

- [ ] **Step 1.5: Sync dependencies**

```bash
cd /home/nan/proj/atd-mvp/python
uv sync
```

Expected output includes:
- `Resolved N packages`
- `Installed N packages`
- A new `.venv/` directory created.

Verify `.venv` was created but is git-ignored:

```bash
ls -la .venv | head -3
git status python/
```

`.venv/` should NOT appear under `git status` untracked files.

- [ ] **Step 1.6: Smoke-check the installed package**

```bash
uv run python -c "import atd_client; print(atd_client.__version__)"
```

Expected: `0.1.0`.

- [ ] **Step 1.7: Run tooling once to confirm it works**

```bash
uv run ruff check src tests
uv run mypy src
uv run pytest
```

Expected:
- ruff: `All checks passed!` (nothing to check yet, but runs clean)
- mypy: `Success: no issues found in 0 source files` or similar
- pytest: `no tests ran`

- [ ] **Step 1.8: Update root `.gitignore` to ignore `python/.venv/`**

The atd-mvp repo root already has a `.gitignore`. Add these lines at the bottom (if not already present):

```
# Python
python/.venv/
python/__pycache__/
python/**/__pycache__/
python/.pytest_cache/
python/.mypy_cache/
python/.ruff_cache/
python/dist/
python/build/
python/*.egg-info/
```

Check:

```bash
cd /home/nan/proj/atd-mvp
grep -n "python/.venv" .gitignore
```

Expected: prints a line showing the pattern is now ignored.

- [ ] **Step 1.9: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/ .gitignore
git commit -m "feat(python): scaffold atd-client package with uv + pydantic + pytest"
```

---

## Task 2: Errors (`errors.py`)

**Files:**
- Create: `python/src/atd_client/errors.py`
- Create: `python/tests/test_errors.py`
- Modify: `python/src/atd_client/__init__.py`

Mirror the Rust `AtdError` enum as a Python exception hierarchy. Each variant becomes a subclass; all inherit from `AtdError`. `suggest_fix()` returns an optional actionable hint matching the Rust behavior.

- [ ] **Step 2.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/python/tests/test_errors.py`:

```python
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
```

- [ ] **Step 2.2: Run the test to confirm it fails**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_errors.py -x
```

Expected: collection error (no `atd_client.errors` module yet).

- [ ] **Step 2.3: Write `errors.py`**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/errors.py`:

```python
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
```

Update `/home/nan/proj/atd-mvp/python/src/atd_client/__init__.py`:

```python
"""atd-client — reference Python SDK for the Agent Tool Dispatch protocol."""

from atd_client.errors import (
    AtdError,
    BindingUnavailable,
    CapabilityDenied,
    InvalidArguments,
    NotImplementedFeature,
    ProtocolError,
    ServerUnreachable,
    Timeout,
    ToolExecutionFailed,
    ToolNotFound,
)

__version__ = "0.1.0"

__all__ = [
    "AtdError",
    "BindingUnavailable",
    "CapabilityDenied",
    "InvalidArguments",
    "NotImplementedFeature",
    "ProtocolError",
    "ServerUnreachable",
    "Timeout",
    "ToolExecutionFailed",
    "ToolNotFound",
    "__version__",
]
```

- [ ] **Step 2.4: Run tests — now pass**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_errors.py -v
```

Expected: `9 passed`.

- [ ] **Step 2.5: Type-check + lint**

```bash
uv run mypy src
uv run ruff check src tests
```

Expected: both clean.

- [ ] **Step 2.6: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/
git commit -m "feat(python): add AtdError hierarchy with suggest_fix + is_retryable"
```

---

## Task 3: Protocol Types (`types.py`)

**Files:**
- Create: `python/src/atd_client/types.py`
- Create: `python/tests/test_types.py`
- Modify: `python/src/atd_client/__init__.py`

Pydantic v2 models for ToolSummary, ToolDefinition, ToolResult, and their nested types. Must accept both snake_case (`"hot"`) and PascalCase (`"Hot"`) for enum values to handle ANOS's actual wire output — same fix as the Rust enums in Phase 0.5.

- [ ] **Step 3.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/python/tests/test_types.py`:

```python
from __future__ import annotations

import pytest

from atd_client.types import (
    BindingProtocol,
    ToolDefinition,
    ToolResult,
    ToolSuccess,
    ToolFailure,
    ToolSummary,
    ToolTier,
    ToolVisibility,
)


def test_tool_summary_parses_minimal_anos_shape() -> None:
    # ANOS's tool_list entries: no name, no domain, no tags. Lowercase enums.
    raw = {
        "id": "anos:fs.read",
        "description": "Read a file",
        "tier": "hot",
        "visibility": "read",
        "lifecycle": "Active",  # unknown field — should be ignored
    }
    s = ToolSummary.model_validate(raw)
    assert s.id == "anos:fs.read"
    assert s.description == "Read a file"
    assert s.name == ""  # defaults to empty
    assert s.domain == ""
    assert s.tags == []
    assert s.tier == ToolTier.HOT
    assert s.visibility == ToolVisibility.READ


def test_tool_summary_accepts_pascalcase_enum_values() -> None:
    raw = {
        "id": "anos:fs.write",
        "description": "Write a file",
        "tier": "Hot",      # ANOS actually emits this form
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
    # Optional metadata fields all None by default
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
    with pytest.raises(Exception):
        ToolSummary.model_validate(
            {"id": "x", "description": "d", "tier": "lukewarm", "visibility": "read"}
        )
```

- [ ] **Step 3.2: Run the test to confirm it fails**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_types.py -x
```

Expected: collection error (no `atd_client.types` module).

- [ ] **Step 3.3: Write `types.py`**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/types.py`:

```python
"""Protocol-level types mirroring the Rust `atd-types` crate.

Enums accept both snake_case (canonical on the wire) and PascalCase (what the
ANOS daemon actually emits today). Serialization always uses snake_case to
match the Rust client, so a Python-emitted JSON payload is byte-compatible with
the Rust contract fixtures.
"""

from __future__ import annotations

from enum import Enum
from typing import Any, Literal, Union

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter, field_validator


class ToolVisibility(str, Enum):
    READ = "read"
    WRITE = "write"
    DANGEROUS = "dangerous"
    SYSTEM = "system"

    @classmethod
    def _missing_(cls, value: object) -> ToolVisibility | None:
        if isinstance(value, str):
            return cls(value.lower()) if value.lower() in cls._value2member_map_ else None
        return None


class ToolTier(str, Enum):
    HOT = "hot"
    WARM = "warm"
    COLD = "cold"

    @classmethod
    def _missing_(cls, value: object) -> ToolTier | None:
        if isinstance(value, str):
            return cls(value.lower()) if value.lower() in cls._value2member_map_ else None
        return None


class BindingProtocol(str, Enum):
    # PascalCase on the wire per the Rust enum's `#[serde(rename_all = "PascalCase")]`.
    CLI = "Cli"
    MCP = "Mcp"
    APP_FUNCTION = "AppFunction"
    REST = "Rest"


class SafetyLevel(str, Enum):
    READ = "Read"
    WRITE = "Write"
    FINANCIAL = "Financial"
    PRIVACY = "Privacy"
    PHYSICAL = "Physical"
    DESTRUCTIVE = "Destructive"


class TrustLevel(str, Enum):
    L0_UNVERIFIED = "L0Unverified"
    L1_SCHEMA_VALID = "L1SchemaValid"
    L2_TESTED = "L2Tested"
    L3_VERIFIED = "L3Verified"
    L4_CERTIFIED = "L4Certified"


# ---------- ToolSummary ----------

class ToolSummary(BaseModel):
    model_config = ConfigDict(extra="ignore", use_enum_values=False)

    id: str
    name: str = ""
    description: str
    domain: str = ""
    tags: list[str] = Field(default_factory=list)
    visibility: ToolVisibility = ToolVisibility.READ
    tier: ToolTier = ToolTier.WARM


# ---------- ToolDefinition family ----------

class ToolCapability(BaseModel):
    model_config = ConfigDict(extra="ignore")

    domain: str
    actions: list[str]
    tags: list[str]
    intent_examples: list[str]


class ToolBinding(BaseModel):
    model_config = ConfigDict(extra="ignore")

    protocol: BindingProtocol
    config: dict[str, Any]


class ToolSafety(BaseModel):
    model_config = ConfigDict(extra="ignore")

    level: SafetyLevel
    dry_run: bool
    side_effects: list[str]
    data_sensitivity: str | None = None


class ToolResources(BaseModel):
    model_config = ConfigDict(extra="ignore")

    timeout_ms: int
    max_concurrent: int
    rate_limit_per_min: int | None = None
    estimated_tokens: int | None = None


class ToolTrust(BaseModel):
    model_config = ConfigDict(extra="ignore")

    publisher: str
    trust_level: TrustLevel
    signature: list[int] | None = None


class ToolDefinition(BaseModel):
    model_config = ConfigDict(extra="ignore")

    id: str
    name: str
    description: str
    version: str
    capability: ToolCapability
    input_schema: dict[str, Any]
    output_schema: dict[str, Any]
    bindings: list[ToolBinding]
    safety: ToolSafety
    resources: ToolResources
    trust: ToolTrust
    visibility: ToolVisibility = ToolVisibility.READ


# ---------- ToolResult (tagged union on "status") ----------

class ToolResultMetadata(BaseModel):
    model_config = ConfigDict(extra="ignore")

    tool_id: str
    version: str | None = None
    binding: BindingProtocol | None = None
    latency_ms: int | None = None
    timestamp: str | None = None
    request_id: str | None = None


class ToolSuccess(BaseModel):
    model_config = ConfigDict(extra="ignore")

    status: Literal["success"] = "success"
    data: Any
    metadata: ToolResultMetadata


class ToolFailure(BaseModel):
    model_config = ConfigDict(extra="ignore")

    status: Literal["error"] = "error"
    code: str
    message: str
    reason: str | None = None
    retryable: bool


# Pydantic union with discriminator gives O(1) dispatch by "status" field.
_ToolResultUnion = Union[ToolSuccess, ToolFailure]
_TOOL_RESULT_ADAPTER: TypeAdapter[_ToolResultUnion] = TypeAdapter(_ToolResultUnion)


class ToolResult:
    """Namespace for parsing tagged-union ToolResult payloads.

    Not a class instance — use :meth:`validate_python` / :meth:`validate_json`
    to turn raw data into the appropriate :class:`ToolSuccess` or
    :class:`ToolFailure` instance.
    """

    @staticmethod
    def validate_python(raw: Any) -> _ToolResultUnion:
        return _TOOL_RESULT_ADAPTER.validate_python(raw)

    @staticmethod
    def validate_json(raw: str | bytes) -> _ToolResultUnion:
        return _TOOL_RESULT_ADAPTER.validate_json(raw)
```

Update `/home/nan/proj/atd-mvp/python/src/atd_client/__init__.py` — extend exports:

```python
"""atd-client — reference Python SDK for the Agent Tool Dispatch protocol."""

from atd_client.errors import (
    AtdError,
    BindingUnavailable,
    CapabilityDenied,
    InvalidArguments,
    NotImplementedFeature,
    ProtocolError,
    ServerUnreachable,
    Timeout,
    ToolExecutionFailed,
    ToolNotFound,
)
from atd_client.types import (
    BindingProtocol,
    SafetyLevel,
    ToolBinding,
    ToolCapability,
    ToolDefinition,
    ToolFailure,
    ToolResources,
    ToolResult,
    ToolResultMetadata,
    ToolSafety,
    ToolSuccess,
    ToolSummary,
    ToolTier,
    ToolTrust,
    ToolVisibility,
    TrustLevel,
)

__version__ = "0.1.0"

__all__ = [
    "AtdError",
    "BindingProtocol",
    "BindingUnavailable",
    "CapabilityDenied",
    "InvalidArguments",
    "NotImplementedFeature",
    "ProtocolError",
    "SafetyLevel",
    "ServerUnreachable",
    "Timeout",
    "ToolBinding",
    "ToolCapability",
    "ToolDefinition",
    "ToolExecutionFailed",
    "ToolFailure",
    "ToolNotFound",
    "ToolResources",
    "ToolResult",
    "ToolResultMetadata",
    "ToolSafety",
    "ToolSuccess",
    "ToolSummary",
    "ToolTier",
    "ToolTrust",
    "ToolVisibility",
    "TrustLevel",
    "__version__",
]
```

- [ ] **Step 3.4: Run tests**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_types.py -v
uv run mypy src
```

Expected: `7 passed`, mypy clean.

- [ ] **Step 3.5: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/
git commit -m "feat(python): add Pydantic protocol types (summary/definition/result)"
```

---

## Task 4: Wire Codec (`wire.py`)

**Files:**
- Create: `python/src/atd_client/wire.py`
- Create: `python/tests/test_wire.py`

Async length-prefixed JSON codec matching the Rust wire (big-endian `u32` length + UTF-8 JSON body, 10 MiB cap).

- [ ] **Step 4.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/python/tests/test_wire.py`:

```python
from __future__ import annotations

import asyncio
import io
import struct

import pytest

from atd_client.wire import MAX_FRAME_BYTES, read_frame, write_frame


class _BytesReader:
    """Minimal asyncio-compatible reader wrapping a bytes buffer."""

    def __init__(self, data: bytes) -> None:
        self._buf = io.BytesIO(data)

    async def readexactly(self, n: int) -> bytes:
        b = self._buf.read(n)
        if len(b) != n:
            raise asyncio.IncompleteReadError(b, n)
        return b


class _BytesWriter:
    def __init__(self) -> None:
        self.buf = bytearray()

    def write(self, data: bytes) -> None:
        self.buf.extend(data)

    async def drain(self) -> None:
        pass


@pytest.mark.asyncio
async def test_write_then_read_roundtrip() -> None:
    writer = _BytesWriter()
    msg = {"kind": "ping", "n": 7}
    await write_frame(writer, msg)

    reader = _BytesReader(bytes(writer.buf))
    back = await read_frame(reader)
    assert back == msg


@pytest.mark.asyncio
async def test_frame_uses_big_endian_u32_prefix() -> None:
    writer = _BytesWriter()
    await write_frame(writer, {"x": 1})
    prefix = bytes(writer.buf[:4])
    body_len = struct.unpack(">I", prefix)[0]
    assert body_len == len(writer.buf) - 4


@pytest.mark.asyncio
async def test_oversized_frame_raises() -> None:
    # Craft a header claiming 20 MiB; reader must refuse before allocating.
    bogus = struct.pack(">I", 20 * 1024 * 1024)
    reader = _BytesReader(bogus)
    with pytest.raises(Exception) as excinfo:
        await read_frame(reader)
    assert "too large" in str(excinfo.value)


@pytest.mark.asyncio
async def test_max_frame_bytes_matches_rust_constant() -> None:
    assert MAX_FRAME_BYTES == 10 * 1024 * 1024
```

- [ ] **Step 4.2: Run to confirm failure**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_wire.py -x
```

Expected: collection error.

- [ ] **Step 4.3: Write `wire.py`**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/wire.py`:

```python
"""Length-prefixed JSON codec over asyncio streams.

Wire format is byte-compatible with the Rust `atd-client::wire`:
- 4-byte big-endian ``u32`` length prefix (max 10 MiB)
- UTF-8 JSON body

Used for both Unix socket and future stdio transports.
"""

from __future__ import annotations

import json
import struct
from typing import Any, Protocol

MAX_FRAME_BYTES = 10 * 1024 * 1024


class _AsyncReader(Protocol):
    async def readexactly(self, n: int) -> bytes: ...


class _AsyncWriter(Protocol):
    def write(self, data: bytes) -> None: ...
    async def drain(self) -> None: ...


async def write_frame(writer: _AsyncWriter, msg: Any) -> None:
    body = json.dumps(msg, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    if len(body) > MAX_FRAME_BYTES:
        raise ValueError(f"frame too large: {len(body)} bytes")
    writer.write(struct.pack(">I", len(body)))
    writer.write(body)
    await writer.drain()


async def read_frame(reader: _AsyncReader) -> Any:
    header = await reader.readexactly(4)
    (length,) = struct.unpack(">I", header)
    if length > MAX_FRAME_BYTES:
        raise ValueError(f"frame too large: {length} bytes")
    body = await reader.readexactly(length)
    return json.loads(body.decode("utf-8"))
```

- [ ] **Step 4.4: Run — now pass**

```bash
uv run pytest tests/test_wire.py -v
uv run mypy src
```

Expected: `4 passed`, mypy clean.

- [ ] **Step 4.5: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/
git commit -m "feat(python): add length-prefixed JSON wire codec (asyncio)"
```

---

## Task 5: Protocol Messages + Transport (`protocol.py`, `transport.py`)

**Files:**
- Create: `python/src/atd_client/protocol.py`
- Create: `python/src/atd_client/transport.py`

These are small glue modules. We don't unit-test them in isolation — they're exercised by the `AtdClient` tests in Task 6. (This is an intentional exception to the "failing test first" rule: the module interfaces are dead simple and fully covered by downstream tests.)

- [ ] **Step 5.1: Write `protocol.py`**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/protocol.py`:

```python
"""JSON-RPC-less protocol envelope used by atd-client ↔ ANOS daemon.

Matches the Rust `atd-client::protocol`. Requests/responses are plain JSON
objects with a ``type`` tag. We keep the tags as string constants because the
message set is small and we don't want the overhead of separate classes for
what are essentially dict shapes.
"""

from __future__ import annotations

from typing import Any


# Request types (client → server).
REQ_PING = "ping"
REQ_TOOL_LIST = "tool_list"
REQ_TOOL_SCHEMA = "tool_schema"
REQ_RUN_TOOL = "run_tool"

# Response types (server → client).
RESP_PONG = "pong"
RESP_TOOL_LIST = "tool_list"
RESP_TOOL_SCHEMA = "tool_schema"
RESP_TOOL_RESULT = "tool_result"
RESP_ERROR = "error"


def ping_request() -> dict[str, Any]:
    return {"type": REQ_PING}


def tool_list_request() -> dict[str, Any]:
    return {"type": REQ_TOOL_LIST}


def tool_schema_request(tool_id: str) -> dict[str, Any]:
    return {"type": REQ_TOOL_SCHEMA, "tool_id": tool_id}


def run_tool_request(tool_id: str, args: Any, dry_run: bool) -> dict[str, Any]:
    return {
        "type": REQ_RUN_TOOL,
        "tool_id": tool_id,
        "args": args,
        "dry_run": dry_run,
    }
```

- [ ] **Step 5.2: Write `transport.py`**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/transport.py`:

```python
"""Transport layer — Unix socket for Phase 1. Future: stdio, HTTP."""

from __future__ import annotations

import asyncio
from pathlib import Path


async def connect_unix(path: Path | str) -> tuple[asyncio.StreamReader, asyncio.StreamWriter]:
    """Open a Unix domain socket connection.

    Raises :class:`OSError` on connect failure; the caller wraps into
    :class:`atd_client.errors.ServerUnreachable`.
    """
    return await asyncio.open_unix_connection(path=str(path))


def default_sock_path() -> Path:
    """Default ANOS daemon socket: ``$HOME/.anos/anos.sock``."""
    home = Path.home()
    return home / ".anos" / "anos.sock"
```

- [ ] **Step 5.3: Sanity build**

```bash
cd /home/nan/proj/atd-mvp/python
uv run mypy src
```

Expected: clean (both new files pass strict mypy).

- [ ] **Step 5.4: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/
git commit -m "feat(python): add protocol envelope + Unix socket transport"
```

---

## Task 6: `AtdClient` Async Core (`client.py`)

**Files:**
- Create: `python/src/atd_client/client.py`
- Create: `python/tests/conftest.py`
- Create: `python/tests/test_client.py`
- Modify: `python/src/atd_client/__init__.py`

The main class. `connect` opens a Unix socket and does the `ping` handshake (matching Rust `AtdClient::connect`). `discover`/`describe`/`call` are the three APIs. Single-writer/single-reader: all requests serialize via an `asyncio.Lock` owned by the client.

- [ ] **Step 6.1: Write conftest with mock-server fixture**

Create `/home/nan/proj/atd-mvp/python/tests/conftest.py`:

```python
"""Shared pytest fixtures for the ATD client test suite."""

from __future__ import annotations

import asyncio
import json
import struct
import tempfile
from collections.abc import AsyncIterator, Callable
from pathlib import Path
from typing import Any

import pytest_asyncio


async def _serve_one_client(
    reader: asyncio.StreamReader,
    writer: asyncio.StreamWriter,
    handler: Callable[[dict[str, Any]], dict[str, Any]],
) -> None:
    try:
        while True:
            try:
                header = await reader.readexactly(4)
            except asyncio.IncompleteReadError:
                return
            (length,) = struct.unpack(">I", header)
            body = await reader.readexactly(length)
            req = json.loads(body.decode("utf-8"))
            resp = handler(req)
            out = json.dumps(resp, separators=(",", ":")).encode("utf-8")
            writer.write(struct.pack(">I", len(out)))
            writer.write(out)
            await writer.drain()
    finally:
        writer.close()
        try:
            await writer.wait_closed()
        except Exception:
            pass


@pytest_asyncio.fixture
async def mock_server() -> AsyncIterator[Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Path]]:
    """Factory that spawns a mock ANOS-like server with the caller's handler.

    Yields a callable ``make(handler) -> socket_path``. Multiple mock servers
    can coexist — each gets its own tempdir. Servers are torn down when the
    outer test ends.
    """
    tempdirs: list[tempfile.TemporaryDirectory[str]] = []
    servers: list[asyncio.Server] = []

    async def make(handler: Callable[[dict[str, Any]], dict[str, Any]]) -> Path:
        d = tempfile.TemporaryDirectory()
        tempdirs.append(d)
        sock_path = Path(d.name) / "mock.sock"

        async def cb(r: asyncio.StreamReader, w: asyncio.StreamWriter) -> None:
            await _serve_one_client(r, w, handler)

        srv = await asyncio.start_unix_server(cb, path=str(sock_path))
        servers.append(srv)
        # Give the event loop a tick so the server's accept task is scheduled.
        await asyncio.sleep(0)
        return sock_path

    try:
        yield make
    finally:
        for s in servers:
            s.close()
            try:
                await s.wait_closed()
            except Exception:
                pass
        for d in tempdirs:
            d.cleanup()
```

- [ ] **Step 6.2: Write the failing test**

Create `/home/nan/proj/atd-mvp/python/tests/test_client.py`:

```python
from __future__ import annotations

from pathlib import Path
from typing import Any, Callable

import pytest

from atd_client import AtdClient, ProtocolError, ToolFailure, ToolSuccess


def _handler_all_ok(req: dict[str, Any]) -> dict[str, Any]:
    t = req.get("type")
    if t == "ping":
        return {"type": "pong"}
    if t == "tool_list":
        return {
            "type": "tool_list",
            "tools": [
                {
                    "id": "anos:fs.read",
                    "description": "Read a file",
                    "tier": "hot",
                    "visibility": "read",
                },
                {
                    "id": "anos:fs.write",
                    "description": "Write a file",
                    "tier": "hot",
                    "visibility": "write",
                },
            ],
        }
    if t == "tool_schema":
        return {
            "type": "tool_schema",
            "schema": {
                "id": req["tool_id"],
                "name": "Read",
                "description": "Read a file.",
                "version": "0.1.0",
                "capability": {
                    "domain": "fs",
                    "actions": ["read"],
                    "tags": [],
                    "intent_examples": [],
                },
                "input_schema": {"type": "object"},
                "output_schema": {"type": "string"},
                "bindings": [{"protocol": "Cli", "config": {}}],
                "safety": {
                    "level": "Read",
                    "dry_run": False,
                    "side_effects": [],
                    "data_sensitivity": None,
                },
                "resources": {
                    "timeout_ms": 1000,
                    "max_concurrent": 1,
                    "rate_limit_per_min": None,
                    "estimated_tokens": None,
                },
                "trust": {
                    "publisher": "anos",
                    "trust_level": "L2Tested",
                    "signature": None,
                },
                "visibility": "read",
            },
        }
    if t == "run_tool":
        return {
            "type": "tool_result",
            "tool_id": req["tool_id"],
            "result": {"echo": req.get("args")},
            "success": True,
            "dry_run": bool(req.get("dry_run")),
        }
    return {"type": "error", "message": f"unexpected: {t}"}


async def test_connect_succeeds_and_pings(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        # Ping is implicit in connect — if we got here, it worked.
        assert client.is_connected()
    finally:
        await client.close()


async def test_discover_returns_summaries(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        summaries = await client.discover()
        assert len(summaries) == 2
        ids = {s.id for s in summaries}
        assert ids == {"anos:fs.read", "anos:fs.write"}
        # name + domain derived by client because server omits them
        for s in summaries:
            assert s.name, f"name should be filled, got empty for {s.id}"
            assert s.domain, f"domain should be filled, got empty for {s.id}"
    finally:
        await client.close()


async def test_discover_filters_client_side(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        summaries = await client.discover(query="read", limit=1)
        assert len(summaries) == 1
        assert summaries[0].id == "anos:fs.read"
    finally:
        await client.close()


async def test_describe_returns_full_definition(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        d = await client.describe("anos:fs.read")
        assert d.id == "anos:fs.read"
        assert d.capability.domain == "fs"
    finally:
        await client.close()


async def test_call_success(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock = await mock_server(_handler_all_ok)
    client = await AtdClient.connect(sock)
    try:
        r = await client.call("anos:fs.read", {"path": "/tmp/x"})
        assert isinstance(r, ToolSuccess)
        assert r.data == {"echo": {"path": "/tmp/x"}}
    finally:
        await client.close()


async def test_call_failure_becomes_tool_failure(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    def h(req: dict[str, Any]) -> dict[str, Any]:
        if req.get("type") == "ping":
            return {"type": "pong"}
        if req.get("type") == "run_tool":
            return {
                "type": "tool_result",
                "tool_id": req["tool_id"],
                "result": {"code": "EPERM", "message": "denied", "retryable": False},
                "success": False,
                "dry_run": False,
            }
        return {"type": "error", "message": "no"}

    sock = await mock_server(h)
    client = await AtdClient.connect(sock)
    try:
        r = await client.call("anos:fs.read", {})
        assert isinstance(r, ToolFailure)
        assert r.code == "EPERM"
        # raw payload preserved in reason
        assert r.reason is not None and "EPERM" in r.reason
    finally:
        await client.close()


async def test_ping_error_when_server_sends_wrong_response(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    def h(req: dict[str, Any]) -> dict[str, Any]:
        # Return tool_list to a ping — client should reject.
        return {"type": "tool_list", "tools": []}

    sock = await mock_server(h)
    with pytest.raises(ProtocolError):
        await AtdClient.connect(sock)
```

- [ ] **Step 6.3: Run to confirm failure**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_client.py -x
```

Expected: collection error.

- [ ] **Step 6.4: Write `client.py`**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/client.py`:

```python
"""Async ATD client.

Mirrors the Rust `atd-client::AtdClient`. One client owns one Unix socket
connection; concurrent callers serialize through an ``asyncio.Lock``.
"""

from __future__ import annotations

import asyncio
import json
from pathlib import Path
from typing import Any

from atd_client import protocol
from atd_client.errors import (
    AtdError,
    InvalidArguments,
    ProtocolError,
    ServerUnreachable,
    ToolExecutionFailed,
    ToolNotFound,
)
from atd_client.transport import connect_unix, default_sock_path
from atd_client.types import (
    ToolDefinition,
    ToolFailure,
    ToolResult,
    ToolResultMetadata,
    ToolSuccess,
    ToolSummary,
    ToolTier,
    ToolVisibility,
)
from atd_client.wire import read_frame, write_frame


def _derive_domain(tool_id: str) -> str:
    """Parse ``anos:fs.read`` → ``"fs"``."""
    if ":" not in tool_id:
        return ""
    _, rest = tool_id.split(":", 1)
    return rest.split(".", 1)[0] if "." in rest else rest


def _derive_name(s: ToolSummary) -> str:
    if s.name:
        return s.name
    if s.description:
        return s.description
    return s.id


class AtdClient:
    """Async client. Use :meth:`connect` to construct.

    Example::

        client = await AtdClient.connect()      # default ~/.anos/anos.sock
        tools = await client.discover(query="fs")
        result = await client.call("anos:fs.read", {"path": "/tmp/x"})
        await client.close()
    """

    _reader: asyncio.StreamReader
    _writer: asyncio.StreamWriter
    _lock: asyncio.Lock
    _closed: bool

    def __init__(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        self._reader = reader
        self._writer = writer
        self._lock = asyncio.Lock()
        self._closed = False

    @classmethod
    async def connect(cls, sock: Path | str | None = None) -> AtdClient:
        path = Path(sock) if sock is not None else default_sock_path()
        try:
            reader, writer = await connect_unix(path)
        except OSError as e:
            raise ServerUnreachable(str(e)) from e

        client = cls(reader, writer)
        try:
            await client._ping()
        except BaseException:
            await client.close()
            raise
        return client

    def is_connected(self) -> bool:
        return not self._closed

    async def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        self._writer.close()
        try:
            await self._writer.wait_closed()
        except Exception:
            pass

    async def _request(self, req: dict[str, Any]) -> dict[str, Any]:
        if self._closed:
            raise ServerUnreachable("client is closed")
        async with self._lock:
            try:
                await write_frame(self._writer, req)
                resp = await read_frame(self._reader)
            except (OSError, asyncio.IncompleteReadError) as e:
                raise ServerUnreachable(str(e)) from e
        if not isinstance(resp, dict):
            raise ProtocolError(expected="json object", got=repr(resp))
        return resp

    async def _ping(self) -> None:
        resp = await self._request(protocol.ping_request())
        if resp.get("type") != protocol.RESP_PONG:
            raise ProtocolError(expected="pong", got=str(resp.get("type")))

    # ---------- public API ----------

    async def discover(
        self,
        query: str | None = None,
        *,
        domain: str | None = None,
        tier: ToolTier | None = None,
        visibility: ToolVisibility | None = None,
        limit: int | None = None,
    ) -> list[ToolSummary]:
        resp = await self._request(protocol.tool_list_request())
        if resp.get("type") == protocol.RESP_ERROR:
            raise ProtocolError(
                expected="tool_list", got=f"error: {resp.get('message')}"
            )
        if resp.get("type") != protocol.RESP_TOOL_LIST:
            raise ProtocolError(expected="tool_list", got=str(resp.get("type")))

        raw = resp.get("tools")
        if not isinstance(raw, list):
            raise ProtocolError(expected="array of tool summaries", got=repr(raw))

        out: list[ToolSummary] = []
        for entry in raw:
            if not isinstance(entry, dict):
                continue
            try:
                s = ToolSummary.model_validate(entry)
            except Exception:
                # Tolerate full ToolDefinition entries by projecting down.
                try:
                    d = ToolDefinition.model_validate(entry)
                except Exception:
                    continue
                s = ToolSummary(
                    id=d.id,
                    name=d.name,
                    description=d.description,
                    domain=d.capability.domain,
                    tags=list(d.capability.tags),
                    visibility=d.visibility,
                )
            out.append(s)

        # Fill derived defaults (ANOS omits name/domain).
        for i, s in enumerate(out):
            if not s.name or not s.domain:
                out[i] = s.model_copy(
                    update={
                        "name": _derive_name(s),
                        "domain": s.domain or _derive_domain(s.id),
                    }
                )

        if query is not None:
            q = query.lower()
            out = [
                s
                for s in out
                if q in s.name.lower() or q in s.description.lower() or q in s.id.lower()
            ]
        if domain is not None:
            out = [s for s in out if s.domain == domain]
        if tier is not None:
            out = [s for s in out if s.tier == tier]
        if visibility is not None:
            out = [s for s in out if s.visibility == visibility]
        if limit is not None:
            out = out[:limit]
        return out

    async def describe(self, tool_id: str) -> ToolDefinition:
        resp = await self._request(protocol.tool_schema_request(tool_id))
        t = resp.get("type")
        if t == protocol.RESP_TOOL_SCHEMA:
            schema = resp.get("schema")
            try:
                return ToolDefinition.model_validate(schema)
            except Exception as e:
                raise ProtocolError(
                    expected="ToolDefinition", got=f"deserialize error: {e}"
                ) from e
        if t == protocol.RESP_ERROR:
            msg = str(resp.get("message", ""))
            if "not found" in msg.lower():
                raise ToolNotFound(tool_id=tool_id, suggestions=[])
            raise ProtocolError(expected="tool_schema", got=f"error: {msg}")
        raise ProtocolError(expected="tool_schema", got=str(t))

    async def call(
        self,
        tool_id: str,
        args: Any = None,
        *,
        dry_run: bool = False,
    ) -> ToolSuccess | ToolFailure:
        if args is None:
            args = {}
        if not isinstance(args, (dict, list, str, int, float, bool, type(None))):
            raise InvalidArguments(
                tool_id=tool_id,
                field="args",
                reason="must be a JSON-serializable value",
            )

        resp = await self._request(protocol.run_tool_request(tool_id, args, dry_run))
        t = resp.get("type")
        if t == protocol.RESP_TOOL_RESULT:
            success = bool(resp.get("success"))
            result = resp.get("result")
            resp_tool_id = str(resp.get("tool_id", tool_id))
            if success:
                return ToolSuccess(
                    data=result,
                    metadata=ToolResultMetadata(tool_id=resp_tool_id),
                )
            # Failure — extract structured fields and preserve raw JSON in reason.
            code = (
                str(result.get("code"))
                if isinstance(result, dict) and "code" in result
                else "UNKNOWN"
            )
            message = (
                str(result.get("message"))
                if isinstance(result, dict) and "message" in result
                else "tool call failed"
            )
            retryable = bool(result.get("retryable")) if isinstance(result, dict) else False
            return ToolFailure(
                code=code,
                message=message,
                reason=json.dumps(result) if result is not None else None,
                retryable=retryable,
            )
        if t == protocol.RESP_ERROR:
            raise ToolExecutionFailed(
                tool_id=tool_id,
                inner=RuntimeError(
                    f"{resp.get('message')} (retryable={resp.get('retryable', False)})"
                ),
            )
        raise ProtocolError(expected="tool_result", got=str(t))

    async def __aenter__(self) -> AtdClient:
        return self

    async def __aexit__(self, *_: Any) -> None:
        await self.close()


__all__ = ["AtdClient"]
```

Update `/home/nan/proj/atd-mvp/python/src/atd_client/__init__.py` — add `AtdClient`:

```python
# Top of file (after existing imports):
from atd_client.client import AtdClient
# And append "AtdClient" to __all__ (alphabetical before "AtdError")
```

The full `__all__` becomes (alphabetical):

```python
__all__ = [
    "AtdClient",
    "AtdError",
    "BindingProtocol",
    "BindingUnavailable",
    "CapabilityDenied",
    "InvalidArguments",
    "NotImplementedFeature",
    "ProtocolError",
    "SafetyLevel",
    "ServerUnreachable",
    "Timeout",
    "ToolBinding",
    "ToolCapability",
    "ToolDefinition",
    "ToolExecutionFailed",
    "ToolFailure",
    "ToolNotFound",
    "ToolResources",
    "ToolResult",
    "ToolResultMetadata",
    "ToolSafety",
    "ToolSuccess",
    "ToolSummary",
    "ToolTier",
    "ToolTrust",
    "ToolVisibility",
    "TrustLevel",
    "__version__",
]
```

- [ ] **Step 6.5: Run tests**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_client.py -v
uv run mypy src
```

Expected: `7 passed`, mypy clean.

- [ ] **Step 6.6: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/
git commit -m "feat(python): add async AtdClient with discover/describe/call"
```

---

## Task 7: Sync Wrapper (`sync.py`)

**Files:**
- Create: `python/src/atd_client/sync.py`
- Create: `python/tests/test_sync.py`
- Modify: `python/src/atd_client/__init__.py`

`AtdClientSync` owns a private event loop on a background thread so sync callers can invoke `discover`/`describe`/`call` without writing async code. Matches the design.md §3.2 note: "Sync wrapper provided: AtdClientSync (for pre-async LangChain code)."

- [ ] **Step 7.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/python/tests/test_sync.py`:

```python
from __future__ import annotations

from pathlib import Path
from typing import Any, Callable

from atd_client import AtdClientSync, ToolSuccess


def _handler(req: dict[str, Any]) -> dict[str, Any]:
    t = req.get("type")
    if t == "ping":
        return {"type": "pong"}
    if t == "tool_list":
        return {
            "type": "tool_list",
            "tools": [
                {"id": "anos:fs.read", "description": "r", "tier": "hot", "visibility": "read"}
            ],
        }
    if t == "run_tool":
        return {
            "type": "tool_result",
            "tool_id": req["tool_id"],
            "result": {"ok": True},
            "success": True,
            "dry_run": False,
        }
    return {"type": "error", "message": "no"}


async def test_sync_wrapper_discover_and_call(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    sock: Path = await mock_server(_handler)

    # Although this test is async (so it can use the async fixture to spin the
    # server), we use the sync client inside a thread to verify it works from
    # a fully synchronous caller.
    import asyncio

    def run_sync_work() -> tuple[int, Any]:
        client = AtdClientSync.connect(sock)
        try:
            tools = client.discover()
            result = client.call("anos:fs.read", {})
        finally:
            client.close()
        return len(tools), result

    count, result = await asyncio.to_thread(run_sync_work)
    assert count == 1
    assert isinstance(result, ToolSuccess)
    assert result.data == {"ok": True}
```

- [ ] **Step 7.2: Run to confirm failure**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_sync.py -x
```

Expected: ImportError or AttributeError for `AtdClientSync`.

- [ ] **Step 7.3: Write `sync.py`**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/sync.py`:

```python
"""Synchronous wrapper around :class:`AtdClient`.

Runs a dedicated event loop on a background daemon thread, so sync call sites
(LangChain tool loaders, Jupyter cells, CLI scripts) can drive the async core
without writing ``async def`` or managing loops themselves.
"""

from __future__ import annotations

import asyncio
import threading
from pathlib import Path
from typing import Any

from atd_client.client import AtdClient
from atd_client.types import (
    ToolDefinition,
    ToolFailure,
    ToolSuccess,
    ToolSummary,
    ToolTier,
    ToolVisibility,
)


class _LoopThread:
    """A dedicated asyncio event loop running on a daemon thread.

    Use :meth:`submit` to schedule a coroutine and block until it completes.
    """

    def __init__(self) -> None:
        self._loop: asyncio.AbstractEventLoop | None = None
        self._ready = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()
        self._ready.wait()

    def _run(self) -> None:
        loop = asyncio.new_event_loop()
        self._loop = loop
        asyncio.set_event_loop(loop)
        self._ready.set()
        loop.run_forever()

    def submit(self, coro: Any) -> Any:
        assert self._loop is not None
        fut = asyncio.run_coroutine_threadsafe(coro, self._loop)
        return fut.result()

    def stop(self) -> None:
        if self._loop is not None and not self._loop.is_closed():
            self._loop.call_soon_threadsafe(self._loop.stop)
            self._thread.join(timeout=1.0)


class AtdClientSync:
    """Synchronous façade. Internally drives an :class:`AtdClient` on a
    dedicated background-thread event loop. Not thread-safe for concurrent
    calls from multiple threads — use separate instances if you need that.
    """

    _loop: _LoopThread
    _inner: AtdClient

    def __init__(self, loop: _LoopThread, inner: AtdClient) -> None:
        self._loop = loop
        self._inner = inner

    @classmethod
    def connect(cls, sock: Path | str | None = None) -> AtdClientSync:
        loop = _LoopThread()
        inner = loop.submit(AtdClient.connect(sock))
        return cls(loop, inner)

    def close(self) -> None:
        try:
            self._loop.submit(self._inner.close())
        finally:
            self._loop.stop()

    def discover(
        self,
        query: str | None = None,
        *,
        domain: str | None = None,
        tier: ToolTier | None = None,
        visibility: ToolVisibility | None = None,
        limit: int | None = None,
    ) -> list[ToolSummary]:
        return self._loop.submit(
            self._inner.discover(
                query,
                domain=domain,
                tier=tier,
                visibility=visibility,
                limit=limit,
            )
        )

    def describe(self, tool_id: str) -> ToolDefinition:
        return self._loop.submit(self._inner.describe(tool_id))

    def call(
        self,
        tool_id: str,
        args: Any = None,
        *,
        dry_run: bool = False,
    ) -> ToolSuccess | ToolFailure:
        return self._loop.submit(self._inner.call(tool_id, args, dry_run=dry_run))

    def __enter__(self) -> AtdClientSync:
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()
```

Update `/home/nan/proj/atd-mvp/python/src/atd_client/__init__.py` — add `AtdClientSync`:

```python
from atd_client.sync import AtdClientSync
```

And in `__all__`, insert `"AtdClientSync"` alphabetically (after `AtdClient`).

- [ ] **Step 7.4: Run the test**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_sync.py -v
uv run mypy src
```

Expected: `1 passed`, mypy clean.

- [ ] **Step 7.5: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/
git commit -m "feat(python): add AtdClientSync wrapper for pre-async call sites"
```

---

## Task 8: LLM Adapters (`adapters.py`)

**Files:**
- Create: `python/src/atd_client/adapters.py`
- Create: `python/tests/test_adapters.py`
- Modify: `python/src/atd_client/__init__.py`

Convert `list[ToolSummary]` to OpenAI and Anthropic tool-use JSON. Sanitize `anos:fs.read` → `anos_fs_read` so names pass the `[a-zA-Z0-9_-]{1,128}` constraint both APIs enforce.

LangChain adapter is explicitly deferred — it would require a `langchain-core` dep and a separate PyPI package.

- [ ] **Step 8.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/python/tests/test_adapters.py`:

```python
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
    # Pass-through for unknown namespace
    assert desanitize_tool_name("weird_thing") == "weird_thing"


def test_as_openai_tools_emits_function_shape() -> None:
    out = as_openai_tools(_sample_summaries())
    assert len(out) == 2
    first = out[0]
    assert first["type"] == "function"
    assert first["function"]["name"] == "anos_fs_read"
    assert first["function"]["description"] == "Read a file from disk"
    # Minimal stub schema, same as atd-mcp-bridge's policy
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
```

- [ ] **Step 8.2: Run to confirm failure**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_adapters.py -x
```

Expected: collection error.

- [ ] **Step 8.3: Write `adapters.py`**

Create `/home/nan/proj/atd-mvp/python/src/atd_client/adapters.py`:

```python
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
            rest = mcp_name[len(prefix):]
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
```

Update `/home/nan/proj/atd-mvp/python/src/atd_client/__init__.py` — add adapter re-exports:

```python
from atd_client.adapters import (
    as_anthropic_tools,
    as_openai_tools,
    desanitize_tool_name,
    sanitize_tool_name,
)
```

And add these 4 names to `__all__` (alphabetical).

- [ ] **Step 8.4: Run tests**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_adapters.py -v
uv run mypy src
```

Expected: `5 passed`, mypy clean.

- [ ] **Step 8.5: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/
git commit -m "feat(python): add OpenAI + Anthropic tool-format adapters"
```

---

## Task 9: Contract Test Against Rust Fixtures

**Files:**
- Create: `python/tests/test_anos_fixture.py`

Reuse the exact JSON fixtures captured from the live ANOS daemon in Phase 0.5 (they're checked in at `crates/atd-client/tests/fixtures/`). This is the single source of truth for "what ANOS actually emits" — any schema drift will break Python and Rust tests simultaneously.

- [ ] **Step 9.1: Write the test**

Create `/home/nan/proj/atd-mvp/python/tests/test_anos_fixture.py`:

```python
"""Contract test: the Python SDK must parse the same live-ANOS responses that
the Rust SDK parses. Fixtures live in the Rust crate tree (single source of
truth); refresh with `scripts/capture_anos_fixtures.sh`."""

from __future__ import annotations

import json
import struct
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest

from atd_client import AtdClient, ToolDefinition

_FIXTURE_DIR = Path(__file__).resolve().parents[2] / "crates" / "atd-client" / "tests" / "fixtures"


def _load(name: str) -> dict[str, Any]:
    with (_FIXTURE_DIR / name).open("r", encoding="utf-8") as f:
        return json.load(f)


async def test_discover_against_real_anos_tool_list_fixture(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    tool_list = _load("anos_tool_list.json")
    assert tool_list["type"] == "tool_list"
    tool_count = len(tool_list["tools"])
    assert tool_count >= 50, f"fixture should have many tools, got {tool_count}"

    def handler(req: dict[str, Any]) -> dict[str, Any]:
        if req.get("type") == "ping":
            return {"type": "pong"}
        if req.get("type") == "tool_list":
            return tool_list
        return {"type": "error", "message": "unexpected"}

    sock: Path = await mock_server(handler)
    client = await AtdClient.connect(sock)
    try:
        summaries = await client.discover()
        assert len(summaries) >= 50
        fs_read = next((s for s in summaries if s.id == "anos:fs.read"), None)
        assert fs_read is not None, "fixture must contain anos:fs.read"
        assert fs_read.domain == "fs"
        assert fs_read.name, "name should be derived from description or id"
    finally:
        await client.close()


async def test_describe_against_real_anos_tool_schema_fixture(
    mock_server: Callable[[Callable[[dict[str, Any]], dict[str, Any]]], Any],
) -> None:
    tool_schema = _load("anos_tool_schema_fs_read.json")
    assert tool_schema["type"] == "tool_schema"

    def handler(req: dict[str, Any]) -> dict[str, Any]:
        if req.get("type") == "ping":
            return {"type": "pong"}
        if req.get("type") == "tool_schema":
            return tool_schema
        return {"type": "error", "message": "unexpected"}

    sock: Path = await mock_server(handler)
    client = await AtdClient.connect(sock)
    try:
        d = await client.describe("anos:fs.read")
        assert d.id == "anos:fs.read"
        assert d.capability.domain == "fs"
        assert d.bindings, "expected at least one binding"
    finally:
        await client.close()
```

- [ ] **Step 9.2: Run the test**

```bash
cd /home/nan/proj/atd-mvp/python
uv run pytest tests/test_anos_fixture.py -v
```

Expected: `2 passed`.

If either fails with "missing field", the ANOS schema drifted from what the Python types handle — fix `types.py` to match. Do NOT patch the fixture.

- [ ] **Step 9.3: Full regression**

```bash
uv run pytest -v
uv run ruff check src tests
uv run mypy src
```

Expected: full test count (≥ 28 passing), ruff clean, mypy clean.

- [ ] **Step 9.4: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/
git commit -m "test(python): contract test against captured live-ANOS fixtures"
```

---

## Task 10: Example + README + Live Smoke + Tag

**Files:**
- Create: `python/examples/hello_atd.py`
- Create: `python/README.md`
- Modify: `README.md` (root — add Python link)

- [ ] **Step 10.1: Write the example**

Create `/home/nan/proj/atd-mvp/python/examples/hello_atd.py`:

```python
"""Minimum working example — Python SDK.

Mirrors the Rust `examples/hello_atd.rs`: connect, discover up to 3 tools,
describe the first, call with dry_run=True, print at each step.

Run:
    ANOS_SOCK=~/.anos/anos.sock uv run python examples/hello_atd.py
"""

from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path

from atd_client import AtdClient, ToolFailure, ToolSuccess


async def main() -> None:
    sock_env = os.environ.get("ANOS_SOCK")
    sock = Path(sock_env) if sock_env else None

    print(f"[atd] connecting to {sock or '<default>'}")
    async with await AtdClient.connect(sock) as client:
        print("[atd] connected")

        tools = await client.discover(limit=3)
        print(f"[atd] {len(tools)} tool(s) discovered")
        for t in tools:
            print(f"        - {t.id} ({t.name})")

        if not tools:
            print("[atd] no tools to describe/call — done.")
            return

        first = tools[0]
        d = await client.describe(first.id)
        print(
            f"[atd] describe({d.id}) → domain={d.capability.domain}, "
            f"bindings={len(d.bindings)}"
        )

        r = await client.call(first.id, {}, dry_run=True)
        if isinstance(r, ToolSuccess):
            print(f"[atd] call ok: {json.dumps(r.data)}")
        elif isinstance(r, ToolFailure):
            print(f"[atd] call error: [{r.code}] {r.message}")


if __name__ == "__main__":
    asyncio.run(main())
```

- [ ] **Step 10.2: Write `python/README.md`**

Create `/home/nan/proj/atd-mvp/python/README.md`:

````markdown
# atd-client (Python)

Reference Python SDK for the [ATD protocol](../docs/design.md). Async-first with a sync wrapper for pre-async call sites.

## Install

From source (during Phase 1, before PyPI publish):

```bash
cd atd-mvp/python
uv sync
uv run python examples/hello_atd.py
```

Once published:

```bash
pip install atd-client
# or: uv add atd-client
```

## Quickstart — async

```python
import asyncio
from atd_client import AtdClient

async def main():
    async with await AtdClient.connect() as client:     # uses ~/.anos/anos.sock
        tools = await client.discover(query="fs", limit=5)
        for t in tools:
            print(t.id, "—", t.name)
        result = await client.call("anos:fs.read", {"path": "/etc/hostname"})
        print(result)

asyncio.run(main())
```

## Quickstart — sync (LangChain / notebooks)

```python
from atd_client import AtdClientSync

client = AtdClientSync.connect()
tools = client.discover(query="fs", limit=5)
result = client.call("anos:fs.read", {"path": "/etc/hostname"})
client.close()
```

## LLM adapters

```python
from atd_client import AtdClient, as_openai_tools, as_anthropic_tools

async with await AtdClient.connect() as client:
    summaries = await client.discover()
    openai_tools = as_openai_tools(summaries)          # OpenAI function-calling shape
    anthropic_tools = as_anthropic_tools(summaries)    # Anthropic tool-use shape
```

## Development

```bash
uv sync                    # install dev deps
uv run pytest              # full test suite
uv run mypy src            # type check
uv run ruff check src tests
```

Contract tests under `tests/test_anos_fixture.py` replay live-ANOS responses captured at `../crates/atd-client/tests/fixtures/`. Refresh with `../scripts/capture_anos_fixtures.sh`.

## Phase 0 known limitation

`call()` currently returns an error against the ANOS reference server because its `run_tool` IPC is stubbed. See `../docs/issues/2026-04-21-atd-run-tool-stub.md`.

## License

Apache-2.0.
````

- [ ] **Step 10.3: Link from root README**

Edit `/home/nan/proj/atd-mvp/README.md`. Find the `## CLI quickstart` section. Immediately after that section (before `## Development`), insert:

````markdown
## Python SDK

```python
import asyncio
from atd_client import AtdClient

async def main():
    async with await AtdClient.connect() as client:
        tools = await client.discover(query="fs", limit=5)
        print(f"{len(tools)} tool(s)")

asyncio.run(main())
```

Full reference: [`python/README.md`](python/README.md).
````

- [ ] **Step 10.4: Live smoke (optional, skip if no ANOS daemon)**

With ANOS running:

```bash
cd /home/nan/proj/atd-mvp/python
uv run python examples/hello_atd.py
```

Expected output:
```
[atd] connecting to <default>
[atd] connected
[atd] 3 tool(s) discovered
        - anos:agent.ask (Ask Agent)
        - anos:agent.list (List Sub-Agents)
        - anos:agent.spawn (Spawn Sub-Agent)
[atd] describe(anos:agent.ask) → domain=agent, bindings=1
[atd] call error: [UNKNOWN] tool call failed      (or: ToolExecutionFailed thrown — either is fine)
```

The `call` step may either return a ToolFailure or raise ToolExecutionFailed depending on how ANOS wraps its stub error — document whichever occurs in the Task 10 report. Both are expected until the ANOS-side `run_tool` stub is fixed.

If ANOS is not running, this step is skipped — note it in the report.

- [ ] **Step 10.5: Final workspace regression**

```bash
cd /home/nan/proj/atd-mvp
cargo test --workspace --all-targets
```

Expected: 92 passing (Rust unchanged).

```bash
cd python
uv run pytest
uv run mypy src
uv run ruff check src tests
```

Expected: ≥ 28 Python tests pass, mypy + ruff clean.

- [ ] **Step 10.6: Commit + tag**

```bash
cd /home/nan/proj/atd-mvp
git add python/examples/ python/README.md README.md
git commit -m "docs(python): add hello_atd example + README + root link"

git tag -a phase1-python -m "Phase 1 (first half): Python SDK with async + sync + LLM adapters"
git log --oneline | head -15
```

---

## Post-Plan Verification Checklist

- [ ] `uv sync && uv run pytest` passes (all Python tests)
- [ ] `uv run mypy src` clean
- [ ] `uv run ruff check src tests` clean
- [ ] `uv run python examples/hello_atd.py` against live ANOS runs to completion (call may error per Phase 0 gap)
- [ ] `cargo test --workspace` still 92 passing (no Rust regression)
- [ ] `python/README.md` has async, sync, adapters, dev sections
- [ ] Root `README.md` has Python SDK quickstart + link
- [ ] Tag `phase1-python` exists

## What's Out of Scope (later plans)

- **atd-langchain** — separate PyPI package depending on `langchain-core` + this SDK
- **TypeScript SDK** at `typescript/` — parallel work to Python, own plan
- **Python stdio transport** — add after TS so both languages match
- **Session / cancel / subscribe APIs** — waiting on ANOS server support
- **Async context manager pool** (if one connection per `AtdClient` becomes limiting) — Phase 2 streaming refactor
- **PyPI publish** — manual step once `atd-protocol` GitHub org is created
