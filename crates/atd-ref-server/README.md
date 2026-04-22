# atd-ref-server

Neutral reference server for the [ATD protocol](../../docs/design.md). Stands up a Unix-socket daemon that speaks the standard ATD wire format and exposes a small but real tool catalog. Designed to be **forked**: third parties writing their own ATD server can read the code and use it as a working template.

Zero dependency on any specific client SDK or agent framework. In particular, zero dependency on `atd-client`, `atd-mcp-bridge`, `atd-cli`, or any `anos-*` crate.

## Quick start

```bash
# build
cargo build --release -p atd-ref-server --bin atd-ref-server

# run (defaults to $HOME/.atd-ref/server.sock)
./target/release/atd-ref-server &

# drive it with the atd CLI (or any ATD-compatible client)
atd --sock $HOME/.atd-ref/server.sock doctor
atd --sock $HOME/.atd-ref/server.sock list
atd --sock $HOME/.atd-ref/server.sock call ref:echo.say --args '{"msg":"hi"}'
```

## How to add a tool

1. **Create the tool file** at `src/tools/<name>.rs`:

   ```rust
   use atd_types::ToolDefinition;
   use crate::context::CallContext;
   use crate::error::ToolCallError;
   use crate::registry::{CallFuture, Tool};

   pub struct MyTool;

   impl Tool for MyTool {
       fn definition(&self) -> &ToolDefinition { /* ... */ }

       fn call<'a>(
           &'a self,
           args: serde_json::Value,
           ctx: &'a CallContext,
       ) -> CallFuture<'a> {
           Box::pin(async move {
               // Your logic here. Return Ok(...) for success,
               // Err(ToolCallError::...) for failure.
           })
       }
   }
   ```

2. **Export from `tools/mod.rs`**:

   ```rust
   pub mod my_tool;
   ```

3. **Register in `builtin.rs`**:

   ```rust
   reg.register(Arc::new(my_tool::MyTool));
   ```

4. **Add unit tests** in the same file under `#[cfg(test)] mod tests`.

5. **`cargo test -p atd-ref-server`** — done.

## Architecture

```
Unix socket accept()
        │
        ▼
per-connection tokio task
        │
        ▼
   read_frame::<Request>
        │
        ▼
  dispatch ────lookup────▶  Registry (global, shared Arc)
        │
        ▼
CallContext (per-call, built from ServerConfig)
        │
        ▼
tool.call(args, &ctx).await
        │
        ▼
Response (ToolResult / Error)
        │
        ▼
   write_frame
```

Three state lifetimes:

| Layer | Lives | Example |
|---|---|---|
| Global | whole process | `Registry`, `ServerConfig` |
| Per-connection | one client session | (SP-2 adds `ReadTracker` here) |
| Per-call | one `run_tool` | `call_id`, `deadline`, args |

## Contracts a tool MUST honor

- **No panics.** Return `Err(ToolCallError::InternalError(...))` for unexpected conditions; the framework does not catch unwind.
- **Respect `ctx.max_output_bytes`.** If your output exceeds the budget, truncate and include a marker field in the returned JSON.
- **Respect `ctx.remaining_time()`** for network/subprocess operations. Wrap with `tokio::time::timeout(...)`.
- **Deterministic `definition()`.** Build once (e.g., in a `OnceLock`); don't allocate on every call.

## Error classification

| Situation | Return | Wire result |
|---|---|---|
| Args don't match schema | `Err(ToolCallError::InvalidArgs(msg))` | `error` response |
| Tool ran, business-level failure | `Err(ToolCallError::ExecutionFailed{code,message,retryable})` | `tool_result { success: false, result: {code, message, retryable} }` |
| Server-side bug | `Err(ToolCallError::InternalError(msg))` | `error` response |
| Success | `Ok(data)` | `tool_result { success: true, result: data }` |

## What SP-2+ adds

This crate is the framework layer (SP-1). Subsequent sub-projects add real tools:

- **SP-2:** `ref:fs.read`, `ref:fs.write`, `ref:fs.edit` + a `ReadTracker` per-connection state
- **SP-3:** `ref:shell.exec` (Bash) + `ref:shell.pwsh` (PowerShell)
- **SP-4:** `ref:fs.glob` + `ref:fs.grep`
- **SP-5:** `ref:web.fetch`

See `../../docs/superpowers/specs/2026-04-22-atd-ref-server-sp1-foundation.md` §10.

## License

Apache-2.0 (workspace default).
