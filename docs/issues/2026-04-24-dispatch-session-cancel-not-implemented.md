# `session()` and `cancel()` not implemented

**Layer:** dispatch
**Status:** blocked-by-design
**Effort:** ~2-3 days (including design work)
**Filed:** 2026-04-24

## Summary

`docs/archive/design.md` §3.1 lists `session()` and `cancel()` among Phase 0's
client SDK API methods. Neither is implemented — no wire messages, no
client SDK methods, no server-side state machine.

## Current state

### Client SDK

```rust
// crates/atd-client/src/client.rs
impl AtdClient {
    pub async fn connect(endpoint: Endpoint) -> ...;
    pub async fn ping(&self) -> ...;
    pub async fn discover(...) -> ...;
    pub async fn describe(...) -> ...;
    pub async fn call(...) -> ...;
    // no session, no cancel
}
```

### Wire protocol

```rust
// crates/atd-client/src/protocol.rs
pub enum ClientMessage {
    #[serde(rename = "tool_list")]    ToolList { ... },
    #[serde(rename = "tool_schema")]  ToolSchema { ... },
    #[serde(rename = "run_tool")]     RunTool { ... },
    // no session.start, no session.end, no cancel
}
```

### Server

No session store. No cancellation token infrastructure. `Registry::dispatch`
takes a `&CallContext` per call with no cross-call state.

## Gap

Design contract:

- `session(name) -> SessionHandle` — for stateful tool invocations
  across multiple calls (e.g., a long-running shell, a DB transaction)
- `cancel(call_id) -> Ack` — for stopping an in-flight call
  (e.g., user hit Ctrl-C during a 60s web fetch)

Reality: no session state, no call-id tracking across invocations, no
in-flight cancel mechanism.

## Impact

- **Long-running calls:** `ref:shell.exec` with a 60s timeout cannot be
  cancelled mid-execution; the client waits or closes the connection
  (ungraceful).
- **Stateful workflows:** an agent cannot open a persistent shell and
  send multiple commands through the same session; each `shell.exec`
  call is isolated.
- **Cancellation patterns:** any interactive UI that wants to "stop"
  a call has to close the whole Unix socket and reconnect.

## Why blocked-by-design

### `session()` design questions

1. What is a session's *state*? Cross-call cwd? Cross-call env vars?
   A pseudo-terminal? An open DB transaction? A ULID scope for grouping
   telemetry? These are four different designs.
2. Who decides what state a session carries? Tool author? Server
   operator? Caller?
3. Wire protocol: is a session a server-side handle (integer id) or a
   caller-derived key (string name)?
4. Lifecycle: explicit `session.end`, or TTL-based expiry?

### `cancel()` design questions

1. What if a tool doesn't support cancellation (pure CPU tool)? Best
   effort / no-op / error?
2. Idempotency — cancelling a completed call: error or no-op?
3. Client correlation: how does the client get the `call_id` *before*
   the call returns? (Currently the call is a synchronous await — no
   way to receive an id mid-flight.)

None of this has a concrete MVP use case with enough detail to design
against. Revisit when someone needs it for real.

## Recommended interim

- **Document as Phase 0+ deferred** in `docs/quickstart/rust.md` and
  `docs/quickstart/python.md` (they don't mention these methods
  currently, which is correct — let's make sure they stay silent).
- **Do not add stubs** to the SDK — a `todo!()` method makes the API
  surface worse than not having it.
- **Reserve the wire types** by leaving space in `ClientMessage`'s
  serde rename set (e.g., `#[serde(rename_all = "snake_case", other)]`
  somewhere to forward-compat).

## Related

- `docs/archive/design.md` §3.1
- `crates/atd-client/src/protocol.rs`
- `crates/atd-client/src/client.rs`
- `docs/protocol/wire-format.md` §4 (already notes these as "planned")
