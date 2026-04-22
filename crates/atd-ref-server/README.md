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

### Shell tools

```bash
# Run a command:
atd --sock $HOME/.atd-ref/server.sock call ref:shell.exec \
  --args '{"command": "uname -a"}'

# PowerShell (if pwsh is installed):
atd --sock $HOME/.atd-ref/server.sock call ref:shell.pwsh \
  --args '{"command": "Get-Date"}'
```

Shell tools return `{exit_code, stdout, stdout_truncated, stderr, stderr_truncated, duration_ms}`. A nonzero `exit_code` is a normal business result — not a tool error. Timeouts (SIGTERM → grace → SIGKILL on Unix) and missing shells (`NOT_AVAILABLE`) ARE errors and come back as `success: false` tool_result.

### Search tools

```bash
# Find all Rust files under src/:
atd --sock $HOME/.atd-ref/server.sock call ref:fs.glob \
  --args '{"pattern": "**/*.rs", "path": "crates/atd-ref-server/src"}'

# Regex search with glob filter:
atd --sock $HOME/.atd-ref/server.sock call ref:fs.grep \
  --args '{"pattern": "pub fn", "path": "crates", "glob": "*.rs"}'
```

Both tools honor `.gitignore` / `.ignore` / `.rgignore` and skip hidden files by default. `ref:fs.grep` skips binary files entirely (detected by NUL byte). Results are capped by `max_matches` (default 1000) and `ctx.max_output_bytes` — when either limit hits, `truncated: true` is set.

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

## Per-connection state

Tools can access `ctx.read_tracker` for cross-call state that lives for the duration of a single client connection. Existing use: `ref:fs.edit` enforces "you must Read this file in this session, and it must not have changed since" via `ReadTracker`.

To use it in your own tool:

```rust
let tracker = ctx.read_tracker.as_ref().ok_or_else(|| {
    ToolCallError::InternalError("server did not attach a read_tracker".into())
})?;
tracker.check(&canonical_path, current_mtime, current_size)
    .map_err(|e| ToolCallError::ExecutionFailed {
        code: "NOT_READ".into(),
        message: e.to_string(),
        retryable: false,
    })?;
```

Lifetime: from connection `accept()` to `close`. Not persisted; not shared across connections. The tracker is dropped when the client disconnects, so NOT_READ errors are natural on new connections — see `examples/rw_cycle.rs` for a complete Write → Read → Edit walk-through on a single connection.

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

## What's shipped and what's next

- **SP-1 (shipped):** framework + `ref:echo.say`
- **SP-2 (shipped):** `ref:fs.read`, `ref:fs.write`, `ref:fs.edit` + `ReadTracker` per-connection state
- **SP-3 (shipped):** `ref:shell.exec` (Bash) + `ref:shell.pwsh` (PowerShell) + shared subprocess handler
- **SP-4 (shipped):** `ref:fs.glob` + `ref:fs.grep` — ripgrep-powered search tools
- **SP-5:** `ref:web.fetch`

See `../../docs/superpowers/specs/2026-04-22-atd-ref-server-sp1-foundation.md` and `sp2-*` for details on shipped sub-projects.

## License

Apache-2.0 (workspace default).
