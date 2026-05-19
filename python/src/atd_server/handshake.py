"""Hello → HelloAck negotiation.

Wire shapes (see `crates/atd-protocol/src/messages.rs`):

    Request::Hello {
      client_id: Option<String>,
      requested_capabilities: Vec<String>,
      ucan_tokens: Vec<String>,             # SP-capability-v2; absent on pre-v2 clients
    }

    Response::HelloAck {
      granted_capabilities: Vec<String>,
      server_version: String,
      supported_tiers: Vec<String>,
    }
"""

from __future__ import annotations

from typing import Any

from atd_server.context import ConnectionContext
from atd_server.policy import ServerPolicy


async def negotiate_hello(
    hello_msg: dict[str, Any],
    *,
    current_ctx: ConnectionContext,
    policy: ServerPolicy,
    server_version: str,
    supported_tiers: tuple[str, ...],
) -> tuple[dict[str, Any], ConnectionContext]:
    """Process a `Request::Hello` frame.

    Returns the `HelloAck` payload and the updated `ConnectionContext`. The
    caller writes the payload to the wire and rebinds its local connection
    state to the returned context.

    Hello is permitted at any point in the connection's lifetime; calling it
    again replaces the prior negotiated state. This matches the Rust ref-
    server's behavior (no `not_handshaken` enforcement at the protocol
    layer).
    """
    client_id_raw = hello_msg.get("client_id")
    client_id = str(client_id_raw) if isinstance(client_id_raw, str) else None

    tokens_raw = hello_msg.get("ucan_tokens") or []
    ucan_tokens: tuple[str, ...] = (
        tuple(str(t) for t in tokens_raw) if isinstance(tokens_raw, list) else ()
    )

    granted = await policy(hello_msg, ucan_tokens)

    new_ctx = current_ctx.with_hello(
        client_id=client_id,
        granted_capabilities=granted.capabilities,
        ucan_tokens=ucan_tokens,
    )

    ack: dict[str, Any] = {
        "type": "hello_ack",
        "granted_capabilities": sorted(granted.capabilities),
        "server_version": server_version,
        "supported_tiers": list(supported_tiers),
    }
    return ack, new_ctx
