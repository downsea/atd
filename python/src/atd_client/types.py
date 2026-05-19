"""Protocol-level types mirroring the Rust `atd-types` crate.

Enums accept both snake_case (canonical on the wire) and PascalCase (what the
ANOS daemon actually emits today). Serialization always uses snake_case to
match the Rust client, so a Python-emitted JSON payload is byte-compatible with
the Rust contract fixtures.
"""

from __future__ import annotations

from enum import Enum
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter


class ToolVisibility(str, Enum):
    READ = "read"
    WRITE = "write"
    DANGEROUS = "dangerous"
    SYSTEM = "system"
    HIDDEN = "hidden"  # SP-tool-visibility-hidden; excluded from tool_list, reachable by id

    @classmethod
    def _missing_(cls, value: object) -> ToolVisibility | None:
        if isinstance(value, str):
            lowered = value.lower()
            return cls(lowered) if lowered in cls._value2member_map_ else None
        return None


class ToolTier(str, Enum):
    HOT = "hot"
    WARM = "warm"
    COLD = "cold"

    @classmethod
    def _missing_(cls, value: object) -> ToolTier | None:
        if isinstance(value, str):
            lowered = value.lower()
            return cls(lowered) if lowered in cls._value2member_map_ else None
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
    input_schema: dict[str, Any] | None = None


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


class ToolErrorDef(BaseModel):
    model_config = ConfigDict(extra="ignore")

    code: str
    description: str
    retryable: bool


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
    errors: list[ToolErrorDef] = Field(default_factory=list)
    # Opaque capability strings the dispatcher requires; gate passes iff
    # all are in the connection's granted_capabilities set. Matches
    # `crates/atd-protocol/src/tool.rs:31`. Empty list = no gate.
    required_capabilities: list[str] = Field(default_factory=list)


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


_ToolResultUnion = ToolSuccess | ToolFailure
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
