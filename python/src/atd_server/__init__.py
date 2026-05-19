"""atd-server — Python server runtime for the Agent Tool Dispatch protocol.

See `docs/superpowers/specs/2026-05-19-sp-server-py-v1-design.md`.

Phase B (skeleton): only `AtdServer` is wired up. `register` and `middleware`
raise NotImplementedError until Phase C / D / E / F land them.
"""

from atd_server.adapters import Transport
from atd_server.adapters.unix import UnixSocketTransport
from atd_server.server import AtdServer

__version__ = "0.0.1"

__all__ = [
    "AtdServer",
    "Transport",
    "UnixSocketTransport",
    "__version__",
]
