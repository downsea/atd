# Adding an audit sink

**Purpose:** route ATD's per-call audit events to a destination of your choice
— Kafka, OpenTelemetry, a SIEM — by implementing the `AuditSink` trait.

## When to use this

Every dispatched `RunTool` emits a structured `CallEvent`. The shipped sink
(`JsonLinesAuditSink`) writes JSON Lines to a file or stdout/stderr. Implement
a custom `AuditSink` when you need events somewhere else: a log pipeline, a
metrics backend, a compliance store.

`AuditSink` is an **observation** hook, not a rewriter — it sits outside the
[`Middleware`](middleware.md) pipeline because audit must observe **every**
outcome including failures, whereas middleware only sees successes.

## The trait

`AuditSink` is defined in `crates/atd-runtime/src/audit.rs`, re-exported as
`atd_runtime::AuditSink`:

```rust
pub trait AuditSink: Send + Sync {
    /// Called at every RunTool dispatch return point. Must NOT block the
    /// dispatch hot path, and must NOT panic.
    fn on_call(&self, event: &CallEvent);

    /// Count of events dropped because the sink's queue was full.
    /// Default 0 for sinks that don't queue.
    fn drops(&self) -> u64 { 0 }
}
```

`on_call` is called synchronously on the dispatch path. The contract is
**non-blocking** — if your sink's destination can be slow (disk, network), do
the slow work on a background task and have `on_call` only hand off.

## The `CallEvent` struct

Every field, from `audit.rs`:

```rust
pub struct CallEvent {
    pub ts: String,                       // RFC 3339 UTC timestamp
    pub call_id: String,                  // ULID, unique per dispatch
    pub tool_id: String,                  // canonical tool id
    pub caller_id: Option<String>,        // skipped on the wire when None
    pub granted_capabilities: Vec<String>,// the call's intersected caps
    pub duration_ms: u64,                 // wall-clock dispatch duration
    pub outcome: Outcome,                 // see below
    pub tier: String,                     // "hot" | "warm" | "cold"
    pub dry_run: bool,
    pub schema_version: u32,              // currently 2
    pub secrets_resolved: bool,           // true iff a broker returned Some;
                                          // never key names or values
    pub cursor_page: Option<u32>,         // 1-based page for paginated calls;
                                          // None for non-paginated
}
```

`Outcome` is a tagged enum covering the full `RunTool` return space:
`Success`, `ExecutionFailed { code, retryable }`, `InvalidArgs { message }`,
`CapabilityDenied { missing }`, `RateLimited { retry_after_ms }`,
`ToolNotFound`.

Events are emitted for `RunTool` and `RunToolContinue` only — `Ping`, `Hello`,
`ToolList`, `ToolSchema` do **not** emit. `schema_version` is `2` as of
1.0; branch on it if you persist events across versions.

## The reference implementation

`JsonLinesAuditSink` (`audit.rs`) is the template for a non-blocking sink. Its
shape — the pattern any network-backed sink should copy:

- Construction (`new` / `new_with_capacity` / `file` / `stdout` / `stderr`)
  spawns a **dedicated tokio task** that owns the writer and drains a bounded
  `tokio::sync::mpsc` channel. **Construction requires a tokio runtime
  context.**
- `on_call` does a non-blocking `tx.try_send(event.clone())`. If the channel is
  full it **drops** the event and increments an `Arc<AtomicU64>` counter —
  log loss is preferred over a dispatch stall.
- `drops()` returns that counter; `Server::metrics_snapshot()` folds it in so
  operators can alarm on it.
- On drop, the channel closes and the drain task does a final flush.

Default channel capacity is `DEFAULT_AUDIT_QUEUE_CAPACITY` (1024).

## Step by step

1. **Define the struct.** For anything slower than memory, hold the *sender*
   side of a channel (or your client's async handle), not the destination
   directly.
2. **Spawn a drain task at construction.** It owns the slow resource and
   consumes events. (`JsonLinesAuditSink::new_with_capacity` is the model.)
3. **`impl AuditSink`.** `on_call` does only a fast hand-off — `try_send` or
   equivalent. On a full queue, drop and count; do **not** block, do **not**
   `.await` a slow path inside `on_call`.
4. **Override `drops()`** to return your dropped-event counter.
5. **Never panic** in `on_call` — a panic on the dispatch path kills the
   connection.

## Wiring it in

The sink is an `Option<Arc<dyn AuditSink>>` on the server config:

```rust
let cfg = atd_server::ServerConfig {
    audit_sink: Some(Arc::new(KafkaAuditSink::new(producer))),
    // …
};
let server = atd_server::Server::new(registry, cfg);
```

The HTTP transport carries it on `HttpServerConfig.shared.audit_sink`. With
`None`, dispatch's audit emission is a no-op. Build the sink inside a tokio
runtime context if it spawns a drain task.

## Testing it

Point the sink at an inspectable destination and assert what it received. The
`JsonLinesAuditSink` tests write to a shared in-memory `Vec<u8>` behind a
`Write` impl, then poll until N newline-terminated records appear:

```rust
#[tokio::test]
async fn writes_one_line_per_event() {
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let sink = JsonLinesAuditSink::new(Box::new(SharedBuf(buf.clone())));
    sink.on_call(&mk_event(Outcome::Success));
    sink.on_call(&mk_event(Outcome::ToolNotFound));
    // … wait for 2 lines, assert each parses as a CallEvent
}
```

Cover: one event per `on_call`, non-blocking behaviour under a burst (100
`on_call`s complete in well under 50 ms), the drop counter incrementing when the
queue saturates, and a final flush on drop.

## Invariants you must preserve

- **Never block the dispatch path.** `on_call` is synchronous on the hot path —
  hand off and return. Slow I/O belongs on a background task.
- **Never panic in `on_call`.**
- **Drop rather than stall.** Under backpressure, dropping events and counting
  them is correct; blocking dispatch is not. Surface the count via `drops()`.
- **Audit never contains secret values.** `CallEvent` deliberately carries
  `secrets_resolved: bool` and nothing more from the secret side. A custom sink
  must not enrich an event with token values, key names, or raw `args` — only
  `args_hash`-style derivations are acceptable.
- **Observe every outcome.** Audit covers success *and* every failure variant —
  do not filter outcomes inside the sink in a way that hides failures from
  operators.

## See also

- [`../atd-architecture.md`](../atd-architecture.md) §6.4 (audit), §6.5 (rate limiting
  and the metrics snapshot).
- [`token-broker.md`](token-broker.md) — the matching no-secrets rule for the
  secret side.
