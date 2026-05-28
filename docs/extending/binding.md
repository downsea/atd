# Adding an invocation binding

**Purpose:** teach dispatch a new *way to execute* a tool — a subprocess, a
gRPC service, a WASM module — by implementing the `Binding` trait.

## When to use this

A `Tool` says *what* a capability is (its definition + in-process logic). A
`Binding` says *how* a tool's semantics are realised. Use a binding when the
real work does **not** run as Rust in the server process:

- subprocess (`CliBinding` — already shipped),
- a remote gRPC / REST service,
- a WASM sandbox,
- a platform call (Apple App Intent, Android AppFunction).

If the work *is* in-process Rust, you do not need a custom binding — register
the tool with the default `NativeBinding` (see [`tool.md`](tool.md)).

## The trait

`Binding` is defined in `crates/atd-runtime/src/binding.rs`, re-exported as
`atd_runtime::Binding`:

```rust
pub type BindingFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolCallError>> + Send + 'a>>;

pub trait Binding: Send + Sync {
    /// Short discriminator: "native", "cli", "grpc", … — used by tests and
    /// observability hooks.
    fn name(&self) -> &'static str;

    fn call<'a>(
        &'a self,
        tool_def: &'a ToolDefinition,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> BindingFuture<'a>;
}
```

`BindingFuture` has the same shape as `Tool`'s `CallFuture`, so a binding can
freely delegate to a `Tool` (that is exactly what `NativeBinding` does).

## The two reference implementations

Both are in `crates/atd-runtime/src/binding.rs`.

- **`NativeBinding`** — wraps an `Arc<dyn Tool>` and its `call` simply forwards
  to `tool.call(args, ctx)`. Assigned automatically by `Registry::register`.
  `name()` → `"native"`.
- **`CliBinding`** — spawns a subprocess. Its fields:
  ```rust
  pub struct CliBinding {
      pub program: PathBuf,
      pub base_args: Vec<String>,
      pub args_mapper: fn(&serde_json::Value) -> Vec<String>,
  }
  ```
  `call` builds `argv = base_args ++ args_mapper(&args)`, runs
  `tokio::process::Command`, wraps it in `tokio::time::timeout` against
  `ctx.remaining_time()` (5 s fallback), and maps the outcome:
  non-zero exit → `ToolCallError::ExecutionFailed { code: "EXIT_<n>", … }`,
  timeout → `ExecutionFailed { code: "TIMEOUT", … }`, spawn failure →
  `InternalError`. `name()` → `"cli"`. `args_mapper` is a `fn` pointer (not a
  closure) so `CliBinding` stays `Send + Sync` with no interior mutability.

## How a binding is selected

A tool declares one or more `ToolBinding` entries in its `ToolDefinition`:

```rust
pub struct ToolBinding {
    pub protocol: BindingProtocol,   // Cli | Mcp | AppFunction | Rest
    pub config: serde_json::Value,   // per-binding configuration blob
}
```

`BindingProtocol` (`crates/atd-protocol/src/enums.rs`) is the *declared* class.
At runtime, the **registry pairing** is what dispatch actually uses:
`Registry::register` attaches a `NativeBinding`; `Registry::register_with_binding`
attaches the binding you pass. Dispatch resolves `tool_id` to a `RegisteredTool`
and calls `entry.binding.call(...)`.

> **v1 selection rule.** Dispatch routes every (non-paginated) tool to the one
> binding stored in its `RegisteredTool`. The `ToolDefinition.bindings` vec is
> declarative metadata; multi-binding *selection* (honouring a
> `CallOptions::preferred_binding`) is a small dispatcher upgrade deferred until
> a real multi-binding tool lands. Today: one tool, one registered binding.
>
> Paginated tools (`supports_pagination() == true`) bypass the `Binding` layer
> entirely and call `Tool::call_paginated` directly — pagination state cannot
> survive a subprocess boundary. A binding therefore only ever sees
> non-paginated calls.

## Step by step: a `GrpcBinding`

1. **Define the struct** holding what the binding needs — a channel/endpoint,
   any static config. Keep it `Send + Sync` (use `fn` pointers or `Arc`, not
   bare closures, for callbacks).
2. **`impl Binding`.** `name()` returns a stable `&'static str` (`"grpc"`).
3. **Write `call`.** Return `Box::pin(async move { ... })`. Map `args` to the
   request message; bound the RPC with `ctx.remaining_time()`; translate the
   response to `serde_json::Value`. Map every failure to a `ToolCallError`
   variant — `ExecutionFailed` for a remote-reported failure (give it a stable
   `code`), `InternalError` for a transport bug, `InvalidArgs` for a bad `args`
   shape.
4. **Pair it at registration:** `registry.register_with_binding(tool, Arc::new(
   GrpcBinding::new(endpoint)))`.

A `WasmBinding` is the same shape — instantiate the module, call its export,
serialise the result, honour the deadline.

## Testing it

`binding.rs`'s own tests are the template. Build a stub `Tool` for the
definition, construct your binding, drive it with `CallContext::for_test()`:

```rust
#[tokio::test]
async fn cli_binding_runs_true_program_succeeds() {
    let tool_def = stub_tool().def;
    let binding = CliBinding {
        program: PathBuf::from("/bin/true"),
        base_args: vec![],
        args_mapper: |_| vec![],
    };
    let ctx = CallContext::for_test();
    let r = binding.call(&tool_def, serde_json::json!({}), &ctx).await.unwrap();
    assert_eq!(r["exit_code"], 0);
}
```

Cover: a success path, a remote-failure path mapped to `ExecutionFailed`, and a
deadline path — set `ctx.deadline = Some(Instant::now() + …)` and assert the
`TIMEOUT` mapping.

## Invariants you must preserve

- **Honour `ctx.remaining_time()`.** A binding that ignores the deadline lets a
  hung subprocess or RPC pin a dispatch slot. Always wrap the call in a
  timeout; on expiry return `ExecutionFailed { code: "TIMEOUT", … }`.
- **Map errors, never panic.** Every failure becomes a `ToolCallError`. A
  spawn/transport failure is `InternalError`; a remote-reported failure is
  `ExecutionFailed` with a stable `code`.
- **Stay `Send + Sync` with no interior mutability** — follow `CliBinding`'s
  `fn`-pointer pattern, or wrap shared state in `Arc`.
- **`name()` is stable** — it appears in tests and observability; do not change
  it across versions.
- **Do not change the wire.** Adding a binding is a no-fork extension. Adding a
  new `BindingProtocol` enum variant *is* a protocol change — see
  [`protocol-and-schema.md`](protocol-and-schema.md).

## See also

- [`tool.md`](tool.md) — the `Tool` half of the pair.
- [`../atd-architecture.md`](../atd-architecture.md) §5.4 (bindings), §10.4 (why REST /
  AppFunction bindings are not shipped).
