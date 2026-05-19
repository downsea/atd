"""atd-server — Python server runtime for the Agent Tool Dispatch protocol.

See `docs/superpowers/specs/2026-05-19-sp-server-py-v1-design.md`.

Phase B–E are landed (the cbrain swap-over point is Phase E).
`middleware` (Phase F) still raises NotImplementedError.
"""

from atd_server.adapters import Transport
from atd_server.adapters.unix import UnixSocketTransport
from atd_server.context import CallContext, ConnectionContext
from atd_server.errors import (
    ERR_CAPABILITY_DENIED,
    ERR_DEADLINE_EXCEEDED,
    ERR_INTERNAL,
    ERR_INVALID_ARGS,
    ERR_TOOL_NOT_FOUND,
    ToolError,
)
from atd_server.middleware import MiddlewareChain, MiddlewareStage
from atd_server.policy import GrantedCapabilities, ServerPolicy, default_policy
from atd_server.registry import ToolRegistry
from atd_server.server import AtdServer

__version__ = "0.0.5"

__all__ = [
    "AtdServer",
    "CallContext",
    "ConnectionContext",
    "ERR_CAPABILITY_DENIED",
    "ERR_DEADLINE_EXCEEDED",
    "ERR_INTERNAL",
    "ERR_INVALID_ARGS",
    "ERR_TOOL_NOT_FOUND",
    "GrantedCapabilities",
    "MiddlewareChain",
    "MiddlewareStage",
    "ServerPolicy",
    "ToolError",
    "ToolRegistry",
    "Transport",
    "UnixSocketTransport",
    "__version__",
    "default_policy",
]
