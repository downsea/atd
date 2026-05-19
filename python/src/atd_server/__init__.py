"""atd-server — Python server runtime for the Agent Tool Dispatch protocol.

See `docs/superpowers/specs/2026-05-19-sp-server-py-v1-design.md`.

Phase B (skeleton) + Phase C (handshake) + Phase D (registry / list / schema)
are landed. `middleware` and `run_tool` still raise / stub until Phase F / E.
"""

from atd_server.adapters import Transport
from atd_server.adapters.unix import UnixSocketTransport
from atd_server.context import ConnectionContext
from atd_server.policy import GrantedCapabilities, ServerPolicy, default_policy
from atd_server.registry import ToolRegistry
from atd_server.server import AtdServer

__version__ = "0.0.3"

__all__ = [
    "AtdServer",
    "ConnectionContext",
    "GrantedCapabilities",
    "ServerPolicy",
    "ToolRegistry",
    "Transport",
    "UnixSocketTransport",
    "__version__",
    "default_policy",
]
