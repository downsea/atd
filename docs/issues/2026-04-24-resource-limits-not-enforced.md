# `rate_limit_per_min` and `max_concurrent` declared but unenforced

**Layer:** dispatch / security
**Status:** tracked
**Effort:** ~0.5 day (both together)
**Filed:** 2026-04-24

## Summary

Every tool's `ToolResources` declares `rate_limit_per_min` and
`max_concurrent`. The server has no code paths that honor either. A
malicious or buggy caller can hammer a tool at any rate; a tool
declaring `max_concurrent: 1` will happily serve 100 parallel calls.

## Current state

### Every built-in tool

```rust
// pattern across crates/atd-ref-server/src/tools/*/
ToolResources {
    timeout_ms: 60_000,
    max_concurrent: 10,             // ← declared
    rate_limit_per_min: None,       // ← usually None
    estimated_tokens: Some(500),
},
```

### Server-side enforcement

`grep -rn "max_concurrent\|rate_limit" crates/atd-ref-server/src/` —
returns only the declarations. No semaphore wrapping per-tool invocation.
No token-bucket limiter. No in-flight call tracking.

### `timeout_ms`

Note: `timeout_ms` IS honored for tools that read `ctx.deadline`
(e.g., `ref:shell.exec`, `ref:web.fetch`). The other two siblings in
`ToolResources` are not.

## Gap

- No per-tool in-flight call counter
- No per-tool rate-per-minute enforcement
- No reject-at-limit error code (no `TOO_MANY_REQUESTS` in the
  `AtdError` enum)
- No per-caller (agent_did) limit (dispatch doesn't track caller
  identity anyway — see `2026-04-24-security-capability-tokens-deferred.md`)

## Impact

- **DoS surface:** `ref:web.fetch` has SSRF guards, but nothing stops
  1000 parallel fetches hammering the same ATD server. Each fetch holds
  a tokio task + possibly a TCP connection.
- **Noisy neighbor:** one buggy agent can starve other callers of the
  same tool (no isolation).
- **Tool author intent ignored:** an author setting
  `rate_limit_per_min: Some(60)` on a tool calling an external paid
  API has no protection against being billed for overuse.

## Proposed approach

Small, cheap, incremental:

1. Add a per-tool `tokio::sync::Semaphore` initialized at `max_concurrent`
   in `Registry`. Acquire before dispatch, release after. Convert "wait
   forever" to "fail fast with `TOO_MANY_CALLS`" after a small timeout
   (e.g. 2s) to avoid unbounded queueing.
2. Add `governor` crate for rate limiting. Per-tool token bucket at
   `rate_limit_per_min / 60` RPS when set to `Some(n)`. Skip if `None`.
3. Add `AtdError::TooManyCalls { tool_id, limit }` variant. Map to wire
   `ToolResult::Error { code: "TOO_MANY_CALLS", retryable: true }`.
4. Document enforcement in `docs/protocol/error-codes.md`.

## Acceptance

- Unit test in `atd-ref-server`: spawn N > max_concurrent parallel
  calls to a slow tool; assert `N - max_concurrent` fail with
  `TOO_MANY_CALLS`.
- Unit test: set `rate_limit_per_min: Some(10)` on a fake tool; assert
  the 11th call in a minute fails.
- Existing tools' behavior unchanged (none currently set a nonzero
  rate_limit, so no regression).

## Related

- `crates/atd-ref-server/src/registry.rs` (dispatch location)
- `crates/atd-types/src/tool.rs` (ToolResources)
- `crates/atd-types/src/error.rs` (add TooManyCalls variant)
- `docs/protocol/error-codes.md` (update)
