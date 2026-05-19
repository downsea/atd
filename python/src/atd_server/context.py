"""Per-connection state.

`Hello` is optional and may arrive at any point during a connection. When it
does, the per-connection handler replaces its `ConnectionContext` with a
fresh copy carrying the negotiated grants. This matches the Rust ref-server,
which is also stateless w.r.t. handshake order; cbrain's P2-9 issue about
session models is intentionally addressed at the protocol-doc level, not
here.
"""

from __future__ import annotations

from dataclasses import dataclass, field, replace


@dataclass(frozen=True)
class ConnectionContext:
    """Immutable snapshot of one connection's negotiated state.

    Before Hello (and for connections that never send Hello), the snapshot is
    the constructor-default: empty granted set, no client_id, no UCAN tokens,
    `handshaken=False`. Phase E's dispatch will use `granted_capabilities` to
    gate tool calls; the empty default means tools that require caps fail
    with `ERR_CAPABILITY_DENIED`.
    """

    remote_addr: str = ""
    client_id: str | None = None
    granted_capabilities: frozenset[str] = field(default_factory=frozenset)
    ucan_tokens: tuple[str, ...] = ()
    handshaken: bool = False

    def with_hello(
        self,
        *,
        client_id: str | None,
        granted_capabilities: frozenset[str],
        ucan_tokens: tuple[str, ...],
    ) -> ConnectionContext:
        """Return a new context carrying the post-Hello state."""
        return replace(
            self,
            client_id=client_id,
            granted_capabilities=granted_capabilities,
            ucan_tokens=ucan_tokens,
            handshaken=True,
        )
