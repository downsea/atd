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
