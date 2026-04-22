"""atd-client — reference Python SDK for the Agent Tool Dispatch protocol."""

from atd_client.adapters import (
    as_anthropic_tools,
    as_openai_tools,
    desanitize_tool_name,
    sanitize_tool_name,
)
from atd_client.client import AtdClient
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
from atd_client.sync import AtdClientSync
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
    "as_anthropic_tools",
    "as_openai_tools",
    "AtdClient",
    "AtdClientSync",
    "AtdError",
    "BindingProtocol",
    "BindingUnavailable",
    "CapabilityDenied",
    "desanitize_tool_name",
    "InvalidArguments",
    "NotImplementedFeature",
    "ProtocolError",
    "SafetyLevel",
    "sanitize_tool_name",
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
