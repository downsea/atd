# `CallOptions::preferred_binding` dropped by server

**Layer:** dispatch
**Status:** tracked
**Effort:** ~1 hour (to honestly remove) or bundle with
`2026-04-24-dispatch-binding-single-impl.md` decision
**Filed:** 2026-04-24

## Summary

`CallOptions::preferred_binding: Option<BindingProtocol>` is a public
field on the Rust `atd-client` API and on the wire `run_tool` message.
The ref-server receives it through the wire layer, deserializes it
successfully, and then **never reads it**. Setting this field has
zero runtime effect.

## Current state

Client side:

```rust
// crates/atd-client/src/options.rs
pub struct CallOptions {
    pub dry_run: bool,
    pub preferred_binding: Option<BindingProtocol>,
}
```

Wire layer: the `run_tool` message carries `preferred_binding` through
serde.

Server side: `Registry::dispatch` never inspects it. All tools have
exactly one binding entry (`Cli`), and the dispatcher routes purely by
`tool_id`. See companion issue `2026-04-24-dispatch-binding-single-impl.md`.

## Gap

- Caller's declared preference has no effect
- No warning / no error when `preferred_binding` references a binding
  the tool doesn't provide
- Test coverage passes the field through but never asserts it changed
  execution

## Impact

- **Caller confusion:** an agent that tries `preferred_binding: Mcp`
  expecting MCP routing gets the same execution path as omitting the
  field
- **Silent failure class:** no error, just different expectation vs
  reality

## Decision required

Tied to the binding-single-impl decision. If that issue picks
**Option A** (implement real binding dispatch), this field gains
meaning automatically. If it picks **Option B/C** (remove or document
as informational), this field should be removed or documented in lock
step.

## Interim action (cheap, honest)

Until the companion decision lands:

1. Docstring the field as "v0.1.x: informational only, no runtime
   effect; reserved for Phase 2 multi-binding dispatch."
2. Add a trace log in the server on first call with a non-None
   `preferred_binding` so we notice if anyone actually uses it.

## Related

- `crates/atd-client/src/options.rs`
- `crates/atd-client/src/client.rs::call`
- `crates/atd-ref-server/src/server.rs` (wire handler)
- Companion: `2026-04-24-dispatch-binding-single-impl.md`
