"""Capability-grant policy for Hello handshake.

`ServerPolicy` is a small Protocol: given the raw Hello payload and the UCAN
token list, return the subset of capabilities the server is willing to grant
for this connection. The default policy grants whatever the client requested
verbatim — fine for trusted in-tree adopters (cbrain-sim, internal demos);
production deployments MUST supply a real policy.

UCAN verification is out of scope for v1 — `default_policy` ignores the
`ucan_tokens` argument. `SP-server-py-ucan-v1` (future) adds a `UcanVerifier`
seam.
"""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class GrantedCapabilities:
    """What the policy decided to grant for one Hello handshake."""

    capabilities: frozenset[str]


HelloPayload = dict[str, Any]
ServerPolicy = Callable[[HelloPayload, tuple[str, ...]], Awaitable[GrantedCapabilities]]


async def default_policy(
    hello: HelloPayload,
    ucan_tokens: tuple[str, ...],
) -> GrantedCapabilities:
    """Grant `requested_capabilities` verbatim. UCAN tokens are ignored.

    Behavior matches `atd-ref-server`'s default and the cbrain shim's
    no-policy mode. Adopters who want least-privilege grants pass their own
    `ServerPolicy` to `AtdServer(policy=...)`.
    """
    del ucan_tokens  # default policy intentionally ignores UCAN
    requested = hello.get("requested_capabilities") or []
    if not isinstance(requested, list):
        return GrantedCapabilities(capabilities=frozenset())
    return GrantedCapabilities(capabilities=frozenset(str(c) for c in requested))
