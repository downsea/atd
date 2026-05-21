# Adding a built-in tool

**Purpose:** make a new capability callable through ATD by implementing the
`Tool` trait and registering it in the `Registry`.

## When to use this

Use this when you have logic that should run **in-process** inside an ATD
server — a filesystem operation, an API wrapper, a computation. If the logic
already lives behind a subprocess, a gRPC service, or a REST endpoint, you want
a [binding](binding.md) instead; if you want to rewrite *another* tool's output,
you want [middleware](middleware.md).

## The trait

`Tool` is defined in `crates/atd-runtime/src/registry.rs` and re-exported as
`atd_runtime::Tool`:

```rust
pub type CallFuture<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolCallError>> + Send + 'a>>;

pub type PaginatedCallFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PaginatedResult, ToolCallError>> + Send + 'a>>;

pub trait Tool: Send + Sync {
    /// Stable borrow of the tool's definition. Called once at registration.
    fn definition(&self) -> &ToolDefinition;

    /// Invoke the tool. `args` is the deserialized JSON from the wire.
    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a>;

    /// Pagination opt-in. Default `false`. Override to `true` only if you
    /// also override `call_paginated`.
    fn supports_pagination(&self) -> bool { false }

    /// Paginated variant. Default impl wraps `call` and returns
    /// `next_cursor: None`, so non-paginating tools need not touch it.
    fn call_paginated<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
        cursor: Option<&'a str>,
    ) -> PaginatedCallFuture<'a>;
}
```

`call` returns a **boxed future** rather than being `async fn` so the trait
stays dyn-compatible without `async_trait`. The body of every implementation is
`Box::pin(async move { ... })`.

## Step by step

The worked example is `crates/atd-tools-echo/src/lib.rs` — the smallest real
tool. Walk it:

1. **Build the `ToolDefinition` once.** Echo stores it in a
   `static DEFINITION: OnceLock<ToolDefinition>` and returns a `&'static`
   reference from `definition()`. Every implementer fills the same fields:

   | Field | What to put |
   |---|---|
   | `id` | Canonical id, `publisher:domain.action` — e.g. `ref:echo.say`. |
   | `name` / `description` | Human-facing; the `description` is what an LLM reads to pick the tool. |
   | `version` | SemVer string for the tool itself. |
   | `capability` | `ToolCapability { domain, actions, tags, intent_examples }` — discovery metadata. |
   | `input_schema` / `output_schema` | JSON Schema 2020-12 `serde_json::Value`. Must accurately describe `args` and the returned value. |
   | `bindings` | `Vec<ToolBinding>` — `protocol` + per-binding `config`. A native tool still declares one entry. |
   | `safety` | `ToolSafety { level, dry_run, side_effects, data_sensitivity }`. |
   | `resources` | `ToolResources { timeout_ms, max_concurrent, rate_limit_per_min, estimated_tokens }`. `max_concurrent` sizes the per-tool semaphore (`0` = unlimited). |
   | `trust` | `ToolTrust { publisher, trust_level, signature }`. |
   | `visibility` | `ToolVisibility::{Read,Write,Dangerous,System,Hidden}`. `Hidden` drops the tool from `ToolList` but keeps it callable. |
   | `required_capabilities` | `Vec<String>` the caller must hold; empty = unrestricted. Enforced by dispatch *before* `call`. |
   | `tier` | `Option<ToolTier>` — `Hot`/`Warm`/`Cold` latency class. `None` defaults to `Warm` at dispatch. |
   | `errors` | `Vec<ToolErrorDef>` — your tool's domain error catalog. |

2. **Implement `definition()`** — return the stored `&ToolDefinition`.

3. **Implement `call()`.** Return `Box::pin(async move { ... })`. Inside:
   - Validate `args` against your `input_schema`. On a bad shape return
     `Err(ToolCallError::InvalidArgs("...".into()))` — the tool's logic must
     not run.
   - Do the work. Read what you need from `ctx` (see below).
   - Honour `ctx.max_output_bytes` — if your honest result would exceed it,
     truncate and return a truncation marker (echo does exactly this), or opt
     into pagination.
   - Return `Ok(serde_json::Value)` on success, or `Err(ToolCallError::…)` on
     failure.

4. **(Optional) opt into pagination.** If a single result can be huge, override
   both `supports_pagination()` → `true` and `call_paginated()`. On the first
   page `cursor` is `None`; on continuations dispatch has already HMAC-verified
   the cursor for you. Mint the next cursor with
   `ctx.cursor_issuer().issue(payload)` and return
   `PaginatedResult { value, next_cursor }`. See
   [`../architecture.md`](../architecture.md) §5.6 and `atd_runtime::cursor`.

## `CallContext` — what the runtime hands you

`CallContext` (`crates/atd-runtime/src/context.rs`, `#[non_exhaustive]`) is
passed by reference to every call. The fields a tool author reaches for:

- `ctx.cwd` — working directory for relative-path tools.
- `ctx.max_output_bytes` — advisory truncation budget; respect it.
- `ctx.remaining_time()` — `Option<Duration>` left before the deadline; pass it
  to `tokio::time::timeout` around slow work.
- `ctx.secrets()` — `Option<&SecretBundle>`; `ctx.secrets().and_then(|s|
  s.get("access_token"))` for a multi-tenant secret. `None` when no broker is
  configured. See [`token-broker.md`](token-broker.md).
- `ctx.cursor_issuer()` — `Option<&CursorIssuer>`; only relevant in
  `call_paginated`.
- `ctx.caller_id`, `ctx.capabilities`, `ctx.tier` — informational; dispatch has
  already enforced `required_capabilities` before `call` runs.

## Wiring it in

Tools are registered on a `Registry` before the server starts
(`crates/atd-runtime/src/registry.rs`):

```rust
let mut registry = Registry::new();
registry.register(Arc::new(MyTool::new()));        // default NativeBinding
// or, for a non-native execution strategy:
registry.register_with_binding(Arc::new(MyTool::new()), Arc::new(my_binding));
```

`register` panics on a duplicate `id` — startup misconfiguration fails loud,
not at request time. Hand the finished `Registry` to a listener
(`atd_server::Server::new`, `atd_server_http::Server::builder`).

## Testing it

Tools test without a socket. `CallContext::for_test()` (available under
`#[cfg(test)]` or the `testing` feature) builds a context with a 1 MiB budget,
`Warm` tier, empty capabilities, no deadline:

```rust
#[tokio::test]
async fn happy_path_echoes_args_verbatim() {
    let t = EchoTool::new();
    let ctx = CallContext::for_test();
    let args = serde_json::json!({"hello": "world"});
    let r = t.call(args.clone(), &ctx).await.unwrap();
    assert_eq!(r, serde_json::json!({"echoed": args}));
}
```

Mutate fields directly to test edge cases — echo sets `ctx.max_output_bytes =
32` to exercise its truncation path. Cover at least: a happy path, a bad-args
path returning `InvalidArgs`, and the `max_output_bytes` boundary.

## Invariants you must preserve

- **Id namespace.** `id` is `publisher:domain.action`. The publisher segment
  before `:` namespaces every tool you ship; `domain.action` may carry further
  dots. The SDK sanitises `:`/`.` for LLM slots — do not pre-sanitise yourself.
- **Schema accuracy.** `input_schema` and `output_schema` are the contract an
  LLM and a cross-language SDK rely on. They must match what `call` actually
  accepts and returns.
- **Honour `max_output_bytes`.** A tool that ignores it can blow an agent's
  context window. Truncate-with-marker or paginate.
- **Never panic.** Return `Err(ToolCallError)` for every failure, including
  internal bugs (`ToolCallError::InternalError`). A panic kills the connection.
- **`definition()` is stable.** Return the same `ToolDefinition` every call —
  the registry reads it once at registration.
- **`supports_pagination()` and `call_paginated()` move together.** Overriding
  one without the other is a logic error dispatch cannot catch.

## See also

- [`binding.md`](binding.md) — when the work is a subprocess / remote service.
- [`../architecture.md`](../architecture.md) §5 (dispatch), §6.3 (per-tool
  runtime controls).
- `crates/atd-tools-fs/src/` — a richer multi-action tool family.
