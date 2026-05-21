# atd-runtime

Transport-agnostic server core for the
[Agent Tool Dispatch (ATD) protocol](https://github.com/downsea/atd).

This is the crate you build an ATD server on. It owns the dispatch pipeline —
tool registration, capability checks, deadline enforcement, middleware, audit,
secret routing, and pagination — but no transport. A transport crate
([`atd-server`](https://crates.io/crates/atd-server) for Unix sockets,
[`atd-server-http`](https://crates.io/crates/atd-server-http) for HTTP) wraps it
with a listener; the same `Registry` reaches both without code duplication.

If you want to **call** an ATD server, use
[`atd-sdk`](https://crates.io/crates/atd-sdk) instead.

## What's in here

| Surface | Role |
|---|---|
| `Tool` trait | Implement once per tool. `definition()` + `call()` (+ optional `call_paginated()`). |
| `Registry` | Holds registered tools; resolves ids; produces summaries for `discover`. |
| `dispatch_request` / `run_tool` | The dispatch entry points a transport calls per request. |
| `Binding` (`NativeBinding`, `CliBinding`) | Adapter between a `Tool` and the runtime — in-process vs. subprocess. |
| `Middleware` (`RedactPathsMiddleware`) | Post-call result interceptors; compose via the transport's `set_middleware`. |
| `CapabilitySet` + capability gate | Checks a tool's `required_capabilities` against the caller's grants. |
| `TokenBroker` (`InMemoryTokenBroker`, `FileTokenBroker`) | Resolves secrets / bearer identity for tools that need credentials. |
| `AuditSink` (`JsonLinesAuditSink`) | Receives a `CallEvent` per dispatch for the audit trail. |
| `CursorIssuer` | Mints + verifies HMAC-signed pagination cursors. |
| `ucan` module | UCAN-lite capability-token parse / verify / revocation. |
| `MetricsCounters` | Lightweight dispatch counters (`MetricsSnapshot`). |
| `CallContext` | Per-call state: `call_id`, deadline, `read_tracker`, output budget. |
| `ReadTracker` | Per-connection cross-call state (e.g. `ref:fs.edit`'s read-before-edit rule). |

## Minimal usage

```rust
use atd_runtime::registry::Registry;
use atd_runtime::{CallContext, Tool};
use atd_runtime::registry::CallFuture;
use atd_protocol::ToolDefinition;
use std::sync::Arc;

struct MyTool { def: ToolDefinition }

impl Tool for MyTool {
    fn definition(&self) -> &ToolDefinition { &self.def }

    fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move { Ok(serde_json::json!({ "echoed": args })) })
    }
}

let mut registry = Registry::new();
// registry.register(Arc::new(MyTool { def: /* ... */ }));
```

Hand the `Registry` to a transport crate to expose it on the wire.

## Extension points

Every extension point in ATD is a `pub` trait in this crate, attachable
without forking the reference server: `Tool`, `Binding`, `Middleware`,
`TokenBroker`, `AuditSink`. Each has a step-by-step how-to guide — trait
signature, reference implementation, test pattern, invariants — under
[`docs/extending/`](https://github.com/downsea/atd/tree/master/docs/extending):
[`tool.md`](https://github.com/downsea/atd/blob/master/docs/extending/tool.md),
[`binding.md`](https://github.com/downsea/atd/blob/master/docs/extending/binding.md),
[`middleware.md`](https://github.com/downsea/atd/blob/master/docs/extending/middleware.md),
[`token-broker.md`](https://github.com/downsea/atd/blob/master/docs/extending/token-broker.md),
[`audit-sink.md`](https://github.com/downsea/atd/blob/master/docs/extending/audit-sink.md).

## License

Apache-2.0.
</content>
