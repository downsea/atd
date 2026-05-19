"""Per-connection and per-call state.

`Hello` is optional and may arrive at any point during a connection. When it
does, the per-connection handler replaces its `ConnectionContext` with a
fresh copy carrying the negotiated grants. This matches the Rust ref-server,
which is also stateless w.r.t. handshake order; cbrain's P2-9 issue about
session models is intentionally addressed at the protocol-doc level, not
here.

`CallContext` is constructed per `run_tool` dispatch and handed to handlers
as the second positional arg: `async def handler(args, ctx): ...`.
"""

from __future__ import annotations

from dataclasses import dataclass, field, replace


@dataclass(frozen=True)
class ConnectionContext:
    """Immutable snapshot of one connection's negotiated state."""

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


@dataclass(frozen=True)
class CallContext:
    """Per-call context handed to `async def handler(args, ctx)`.

    `request_id` is server-generated in v1 (clients have no way to set it on
    the wire). `SP-cancel-streaming-v1` will add a client-supplied id so
    cancellation/multiplexing can route. Handlers should treat `request_id`
    as opaque.

    Note: `dry_run` is NOT on this context. Dry-run is short-circuited by the
    dispatcher BEFORE the handler is called (per SP §G5), so handler code
    never observes `dry_run=True`. Adopters who want handler-level dry-run
    branching can register a separate handler or wrap their tool factory.
    """

    request_id: str
    tool_id: str
    granted_capabilities: frozenset[str]
    connection: ConnectionContext
