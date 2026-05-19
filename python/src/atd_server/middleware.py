"""Middleware: stage-ordered async wrappers around handler dispatch.

Three stages:

  - `pre_call`  : wrap handler invocation; can short-circuit by returning
                  without awaiting `call_next`. Outermost in the chain.
  - `post_call` : wrap handler invocation; typically awaits `call_next` then
                  mutates the response. Inner to pre_call, outer to handler.
  - `on_error`  : observes raised exceptions; return a `ToolFailure` (or
                  any value) to suppress, return `None` to fall through to
                  the next on_error or the default envelope.

Wrapping order (with pre1, pre2, post1, post2 registered in that order):

    pre1 → pre2 → post1 → post2 → handler → post2 unwinds → post1 unwinds
                                         → pre2 unwinds → pre1 unwinds

(I.e. pre and post both wrap in registration order; "unwinds" = code after
`await call_next()`.)

Adopter use cases (cbrain's Merkle audit, rate limiting, OTel tracing) all
fit the call_next pattern. Cbrain's P2-8 is satisfied by post_call.
"""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from typing import Any, Literal

from atd_server.context import CallContext

MiddlewareStage = Literal["pre_call", "post_call", "on_error"]

CallNext = Callable[[], Awaitable[Any]]
WrappingMiddlewareFn = Callable[[dict[str, Any], CallContext, CallNext], Awaitable[Any]]
ErrorMiddlewareFn = Callable[[dict[str, Any], CallContext, BaseException], Awaitable[Any]]


@dataclass(frozen=True)
class MiddlewareChain:
    """Immutable snapshot of registered middlewares for one dispatch."""

    pre_call: tuple[WrappingMiddlewareFn, ...] = field(default_factory=tuple)
    post_call: tuple[WrappingMiddlewareFn, ...] = field(default_factory=tuple)
    on_error: tuple[ErrorMiddlewareFn, ...] = field(default_factory=tuple)


def build_wrap_chain(
    *,
    pre_call: tuple[WrappingMiddlewareFn, ...],
    post_call: tuple[WrappingMiddlewareFn, ...],
    request: dict[str, Any],
    ctx: CallContext,
    innermost: CallNext,
) -> CallNext:
    """Compose pre_call ∘ post_call ∘ innermost into a single callable.

    Returns a no-arg awaitable that, when awaited, drives the full chain.
    """
    inner = innermost
    # Wrap post_call middlewares first (so they end up closer to the handler).
    for mw in reversed(post_call):
        inner = _wrap_one(mw, request, ctx, inner)
    # Then wrap pre_call middlewares (so they end up outermost).
    for mw in reversed(pre_call):
        inner = _wrap_one(mw, request, ctx, inner)
    return inner


def _wrap_one(
    mw: WrappingMiddlewareFn,
    request: dict[str, Any],
    ctx: CallContext,
    inner: CallNext,
) -> CallNext:
    async def wrapped() -> Any:
        return await mw(request, ctx, inner)

    return wrapped
