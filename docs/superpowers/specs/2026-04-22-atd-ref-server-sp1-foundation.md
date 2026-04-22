# atd-ref-server — SP-1 Foundation Design Spec

**Date:** 2026-04-22
**Status:** Design approved; plan pending.
**Scope:** Sub-project 1 of a multi-SP initiative to ship `atd-ref-server` — a standalone, neutral ATD server that ships with production-quality reference tools. SP-1 is the framework layer only.
**Related:** `docs/design.md` §0 (protocol independence), §1.1 (Goal A technical validation)

---

## 1. Motivation

### 1.1 The gap

The MVP's stated purpose (design.md §1.1 Goal A) is to prove the ATD protocol is usable by non-ANOS agents. Phase 0 shipped the Rust client SDK. Phase 0.5 validated that Hermes (a non-ANOS agent) can enumerate ATD tools through `atd-mcp-bridge`. But end-to-end tool **execution** is still blocked by a stub on the ANOS side (`run_tool` IPC handler returns `"not yet supported"` for non-dry-run calls — tracked in `docs/issues/2026-04-21-atd-run-tool-stub.md`).

Because ANOS is the only running ATD server in the ecosystem, every atd-mvp demo transitively depends on ANOS's feature completeness. This undermines the "neutral protocol" positioning: a skeptic can reasonably say *"you've proven the protocol works when ANOS is on both ends."*

### 1.2 The fix

Ship a **neutral reference server** under atd-mvp itself:

- Written in Rust, depends on `atd-types` only (no `anos-*`, no `atd-client`, no `atd-mcp-bridge`)
- Exposes a Unix socket speaking the standard ATD wire protocol
- Hosts real, production-quality tools (Read / Write / Edit / Bash / PowerShell / Glob / Grep / WebFetch — ported to Rust in SP-2 through SP-5)
- Lets a third party run a full demo without any ANOS dependency:

  ```
  atd-client (Rust or Python) ─── Unix socket ─── atd-ref-server
  ```

### 1.3 Why a "reference" server (not a "mock server")

A mock server is a test fixture. A reference server is **an asset** — readable code that a third party forks or studies when writing their own ATD server implementation. The difference shows in:

- Clean, well-commented code
- A documented `Tool` trait third parties can see themselves implementing
- Real tools (SP-2+), not `echo`-only
- A README with "how to add your own tool"
- Full test suite showing the conformance bar

### 1.4 Clean-room implementation boundary

This project implements tool semantics from first principles using:

- The public feature catalog at `docs/architecture/reference/tool-system.md` in the Claude Code documentation tree as a list of *which* tools are generic enough to belong in any agent framework
- Universal domain knowledge of what each tool does (Read reads files, Grep searches content, etc.)
- Rust's battle-tested ecosystem (ripgrep, globset, reqwest, tokio::process, etc.)

No TypeScript source from Claude Code is consulted during implementation. The Rust API shape, input/output schemas, error classification, and control flow are designed independently. "Strictly follow the design" means matching the **observable contract** a developer would expect from a tool with that name — not bitwise reproduction of undocumented behavior.

---

## 2. Scope (SP-1 only)

### 2.1 In scope

- New workspace crate `crates/atd-ref-server/` producing binary `atd-ref-server`
- `Tool` trait + `Registry` — how tools are defined and found
- `CallContext` — per-call state passed to every tool
- `ToolCallError` — error classification tools return
- Wire codec (length-prefixed JSON, byte-compatible with the Rust atd-client, independently re-implemented in ~70 LOC)
- Local `Request` / `Response` protocol types (independent from `atd-client::protocol`, same wire tags)
- Unix socket listener + per-connection handler task + request dispatcher
- `builtin_registry()` registering exactly **one** trivial test-anchor tool: `ref:echo.say` (echoes input args)
- CLI binary with args: `--sock PATH`, `--cwd PATH`, `--max-output-bytes N`, `--timeout-ms N`
- `README.md` with "how to add a new tool" guide
- Integration tests: raw Unix-socket client spawning the compiled binary

### 2.2 Explicitly deferred

- **Any real tool** — Read/Write/Edit (SP-2), Bash/PowerShell (SP-3), Glob/Grep (SP-4), WebFetch (SP-5), polish (SP-6)
- **Permissions / capability tokens** — Phase 2 (`atd allow`, UCAN, etc.)
- **Session / cancel / subscribe** APIs — deferred to Phase 2 across the board
- **stdio / HTTP transport** — Unix socket only for SP-1
- **Graceful shutdown / signal handling** — Ctrl-C kills the process; no SIGTERM draining
- **Tool panic recovery** — contract-level prohibition in the README; no `catch_unwind`
- **Per-tool dry_run previews** — framework handles `dry_run=true` uniformly (see §5)

### 2.3 Prerequisites

- atd-mvp at `phase1-python` tag, 127 tests passing
- Workspace MSRV 1.85, edition 2024
- Local Rust 1.94 (Fedora system package) sufficient for build + test; CI handles fmt/clippy

---

## 3. Architecture

### 3.1 Module layout

```
crates/atd-ref-server/
├── Cargo.toml
├── README.md                  # includes "how to add a tool" section
└── src/
    ├── main.rs                # CLI entry: clap → build config → Server::run
    ├── lib.rs                 # re-exports for integration tests + future downstream
    ├── wire.rs                # length-prefixed JSON codec (~70 LOC)
    ├── protocol.rs            # Request / Response enums (~90 LOC)
    ├── context.rs             # CallContext + #[cfg(test)] for_test()
    ├── error.rs               # ToolCallError
    ├── registry.rs            # Tool trait + Registry
    ├── server.rs              # Server::run + handle_connection + dispatch
    ├── builtin.rs             # builtin_registry() registers the echo tool
    └── tools/
        ├── mod.rs
        └── echo.rs            # ref:echo.say + its unit tests
└── tests/
    └── integration.rs         # spawn binary, raw socket client, e2e
```

Total estimate: ~860 LOC Rust + ~200 LOC integration tests. ~2 days implementation.

### 3.2 Layering & data flow

```
CLI args / env  ─→  main.rs  ─→  ServerConfig
                                      │
                                      ▼
                             Server::new(registry, config)
                                      │
                        ┌─────────────┴──────────────┐
                        ▼                             ▼
                  UnixListener::bind          builtin_registry()
                        │                             │
      ┌─────────────────┘                             │
      ▼                                                │
 (accept loop)                                         │
      │                                                │
 tokio::spawn(handle_connection(Arc<ServerState>))     │
                        │                              │
                        ▼                              │
                   read_frame::<Request>               │
                        │                              │
                        ▼                              │
                      dispatch ─────────lookup────────▶│
                        │                              │
                        ▼                              ▼
                 CallContext::new()          Registry::get(tool_id)
                        │                              │
                        └──────┬───────────────────────┘
                               ▼
                      tool.call(args, &ctx).await
                               │
                               ▼
                    Response::ToolResult { ... }
                               │
                               ▼
                         write_frame
```

### 3.3 Crate dependencies

| Dep | Why | Runtime / Dev |
|-----|-----|----|
| `atd-types` (path) | Shared `ToolDefinition`, `ToolSummary`, `ToolResult` wire types (the only legitimate cross-crate contract) | Runtime |
| `tokio` (workspace) | Unix socket + async runtime + `process` (SP-3) | Runtime |
| `serde` + `serde_json` | Wire codec + tool arg/result JSON | Runtime |
| `thiserror` | `ToolCallError` derivation | Runtime |
| `ulid` | `call_id` generation | Runtime |
| `clap` (with `derive`) | CLI arg parsing | Runtime |
| `tempfile` | integration tests (tempdir for socket) | Dev |

**Explicitly not depended on:** `atd-client`, `atd-mcp-bridge`, `atd-cli`, any `anos-*` crate. Independence is tested via `cargo tree` (SP-1 exit criterion #4).

---

## 4. Key types & their contracts

### 4.1 `Tool` trait

```rust
pub trait Tool: Send + Sync {
    /// Stable reference to the tool's definition. Registry calls this at
    /// registration time and caches it; do NOT rebuild on every call.
    fn definition(&self) -> &ToolDefinition;

    /// Invoke the tool. Args are the deserialized JSON from the wire.
    /// Tools MUST NOT panic; they return Err(ToolCallError) instead.
    async fn call(
        &self,
        args: serde_json::Value,
        ctx: &CallContext,
    ) -> Result<serde_json::Value, ToolCallError>;
}
```

Uses Rust 1.75+ native async-fn-in-trait (MSRV 1.85 accommodates). No `#[async_trait]` macro.

### 4.2 `ToolCallError`

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ToolCallError {
    /// Schema validation failed or args couldn't be coerced.
    /// Maps to wire Response::Error.
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    /// Tool ran to completion but reports a failure outcome.
    /// Maps to wire Response::ToolResult { success: false, ... }.
    #[error("execution failed ({code}): {message}")]
    ExecutionFailed {
        code: String,
        message: String,
        retryable: bool,
    },

    /// Server-side bug or unexpected condition.
    /// Maps to wire Response::Error.
    #[error("internal error: {0}")]
    InternalError(String),
}
```

Named *ToolCallError* rather than reusing `atd-types::AtdError` because the error axes differ: client-side errors classify network/protocol failures; server-side errors classify tool-internal failures.

### 4.3 `Registry`

```rust
pub struct Registry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Registry {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, tool: Arc<dyn Tool>);
    pub fn get(&self, tool_id: &str) -> Option<&Arc<dyn Tool>>;
    pub fn summaries(&self) -> Vec<ToolSummary>;
}
```

- `Arc<dyn Tool>` allows concurrent dispatch across connections without cloning tools.
- `register` panics on duplicate `tool_id` (fail loud at startup, not mid-flight).
- `summaries()` is snapshot-producing; future tiering work can override.

### 4.4 `CallContext` (SP-1 shape)

```rust
pub struct CallContext {
    pub cwd: PathBuf,                   // relative-path root for Read/Bash/etc.
    pub max_output_bytes: usize,        // truncation budget tools respect
    pub call_id: ulid::Ulid,            // tracing/logging id
    pub deadline: Option<Instant>,      // absolute timeout
}

impl CallContext {
    pub fn remaining_time(&self) -> Option<Duration> { ... }
    #[cfg(any(test, feature = "testing"))]
    pub fn for_test() -> Self { ... }
}
```

SP-1 ships these four fields. SP-2 adds `read_tracker: Option<Arc<ReadTracker>>` (for Edit's must-read-first invariant) as a backwards-compatible field addition — no `Tool::call` signature change needed.

### 4.5 Three state-lifetime layers

| Layer | Lives | Holds | Construction |
|-------|-------|-------|--------------|
| **Global** | Process | `Registry`, `ServerConfig` | `Server::new` → `Arc<ServerState>` |
| **Per-connection** | One TCP/Unix session | *(SP-2 adds `ReadTracker` here)* | Per-connection handler local |
| **Per-call** | One `run_tool` | `call_id`, `deadline`, `args` | Built in dispatcher; dropped on return |

---

## 5. Wire protocol mapping

### 5.1 Request / Response types

Locally defined in `protocol.rs`. Tag names match atd-client::protocol exactly so both sides speak the same JSON.

**Supported requests:** `ping`, `tool_list`, `tool_schema`, `run_tool`

**Response variants:** `pong`, `tool_list`, `tool_schema`, `tool_result`, `error`

### 5.2 Request dispatch table

| Request | Dispatcher action | Response |
|---|---|---|
| `Ping` | — | `Pong` |
| `ToolList` | `registry.summaries()` | `ToolList { tools }` |
| `ToolSchema { tool_id }` | `registry.get(tool_id)` | Found → `ToolSchema { schema }`; Not found → `Error { message: "tool not found: …" }` |
| `RunTool { tool_id, args, dry_run: true }` | (no tool call) | `ToolResult { success: true, result: {"dry_run": true, "tool_id": …, "args_preview": args}, dry_run: true }` |
| `RunTool { tool_id, args, dry_run: false }` | Build `CallContext`, call `tool.call(args, &ctx)` | See §5.3 |

### 5.3 `run_tool` error mapping

| Source | Wire response | Client sees |
|---|---|---|
| Tool ID not found | `Error { message: "tool not found: …" }` | `AtdError::ToolNotFound` (via existing "not found" heuristic) |
| `Err(InvalidArgs(msg))` | `Error { message: "invalid args: …" }` | `AtdError::ProtocolError` / `InvalidArguments` |
| `Ok(data)` | `ToolResult { success: true, result: data, dry_run: false }` | `ToolSuccess { data, metadata }` |
| `Err(ExecutionFailed { code, message, retryable })` | `ToolResult { success: false, result: {code, message, retryable}, dry_run: false }` | `ToolFailure { code, message, retryable, reason: <raw JSON preserved> }` |
| `Err(InternalError(msg))` | `Error { message: "internal error: …" }` | `AtdError::ProtocolError` |
| **Tool panic** | Connection dies; next client request gets `AtdError::ServerUnreachable` | (Tool contract forbids panics; SP-1 does not `catch_unwind`) |

### 5.4 `dry_run` handling rationale

Framework-uniform dry-run (no `Tool::dry_run_preview` method) for SP-1 because:

1. SP-1's only tool (`echo`) has no meaningful "preview vs execute" distinction.
2. Adding `dry_run_preview` to the `Tool` trait now commits third-party implementers to it forever. Deferring keeps the trait minimal.
3. SP-2+ real tools (Bash, Write, Edit) will want per-tool previews, and that's the right time to extend the trait — the concrete needs will shape the signature correctly.

SP-2 extension path (non-breaking):

```rust
pub trait Tool: Send + Sync {
    // ... existing methods ...

    /// Default: return a generic preview. Tools with side effects override.
    fn dry_run_preview(&self, args: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "dry_run": true,
            "tool_id": self.definition().id,
            "args_preview": args,
        })
    }
}
```

---

## 6. Server configuration

### 6.1 CLI args

```
atd-ref-server [--sock PATH] [--cwd PATH]
               [--max-output-bytes N] [--timeout-ms N]

  --sock PATH               Unix socket path. Default: $HOME/.atd-ref/server.sock
  --cwd PATH                Working directory for relative-path tools.
                            Default: current working directory
  --max-output-bytes N      Advisory truncation budget (default: 1048576 = 1 MiB)
  --timeout-ms N            Per-call deadline (default: 60000 = 60 s)
```

### 6.2 Socket hygiene

- Startup removes stale socket file if present (common pattern for sockets that daemons re-use)
- Unix permissions 0600 — only the owning user can connect
- 10 MiB frame size limit (DoS baseline, matches atd-client)

---

## 7. Testing strategy

### 7.1 Three layers

| Layer | Location | What it proves |
|-------|----------|----------------|
| Unit | `src/*.rs` `#[cfg(test)] mod tests` | Individual module correctness: wire codec, Registry ops, dispatcher branches, Echo behavior |
| In-process integration | `src/server.rs` tests | Assembled Server handles each method over in-memory `tokio::io::duplex` |
| Binary-spawn E2E | `tests/integration.rs` | Compiled binary + real Unix socket + raw-wire client |

### 7.2 SP-1 test inventory (~32 tests)

| Area | # | Description |
|---|---|---|
| Wire codec | 3 | write_read_roundtrip, big_endian_prefix, oversized_frame_rejected |
| Protocol | 6 | Each Request variant roundtrips; each Response variant serializes with right tag |
| Registry | 4 | register / get / duplicate / summaries |
| CallContext | 2 | for_test defaults, remaining_time |
| ToolCallError mapping | 4 | Each variant maps to the right wire response |
| Echo tool | 3 | happy path, empty args, large-args truncation |
| Dispatch | 6 | ping, tool_list, tool_schema found, tool_schema not-found, unknown method, run_tool with tool not found |
| E2E binary spawn | 7 | connect, ping, tool_list, tool_schema, call-success, call-dry_run, connection-close |

### 7.3 Integration-test isolation

`tests/integration.rs` rolls its own ~40-line raw Unix-socket client. It does **not** depend on `atd-client`. Mirrors the pattern used in `atd-client/tests/mock_server.rs` (which rolls its own server for the same reason).

This keeps the two crates genuinely independent — neither has any production or test dependency on the other. Cross-crate end-to-end (atd-client talks to atd-ref-server) is deferred to SP-2, where there are real tools to exercise.

### 7.4 `CallContext::for_test()`

Gated behind `#[cfg(any(test, feature = "testing"))]`. Provides sensible defaults (cwd = current, max_output_bytes = 1 MiB, call_id = new ULID, deadline = None). Exposed for downstream test use via optional `testing` feature.

---

## 8. `README.md` — how to add a tool

The README is **part of the deliverable**. Without it, the crate is "just another binary". With it, the crate serves its reference-implementation purpose.

Required sections:

1. **What this is** — one-paragraph positioning (neutral ATD server, ship-with-tools, fork-friendly)
2. **Quick start** — build, run, connect with `atd` CLI
3. **How to add a tool** — concrete numbered steps:
   1. Create `src/tools/<name>.rs`
   2. Define `ToolDefinition` (id, name, description, schema, etc.)
   3. Implement `Tool` trait
   4. Add to `builtin.rs::builtin_registry()`
   5. Add unit tests in the module
   6. `cargo test -p atd-ref-server` — done
4. **Architecture diagram** — the data-flow diagram from §3.2
5. **Contracts you must honor** — tools must not panic; tools must respect `max_output_bytes`; tools must return on deadline
6. **What SP-2+ adds** — preview of where this is heading (so readers don't worry SP-1 is final)

---

## 9. Exit criteria

1. `cargo build -p atd-ref-server --release` succeeds with zero warnings.
2. `cargo test -p atd-ref-server` — all ~32 tests pass.
3. `cargo test --workspace` — no regressions (previous 127 tests all still pass).
4. `cargo tree -p atd-ref-server --prefix none | grep -E '^(anos-|atd-client|atd-mcp-bridge|atd-cli)'` returns empty (independence verified).
5. Manual cross-crate smoke:
   - Terminal 1: `./target/release/atd-ref-server --sock /tmp/ref.sock`
   - Terminal 2:
     - `atd doctor --sock /tmp/ref.sock` shows `socket exists: true`, `ping: ok`, `tool count: 1`
     - `atd list --sock /tmp/ref.sock` shows `ref:echo.say` row
     - `atd call ref:echo.say --args '{"message":"hello"}' --sock /tmp/ref.sock` returns the echo payload
6. `README.md` contains all 6 required sections from §8.
7. Git tag `sp1-ref-server-foundation` created on the final commit.

---

## 10. What's next (SP-2 through SP-6 preview)

| SP | Scope | Est |
|---|---|---|
| SP-2 | File I/O: Read, Write, Edit + must-read-before-edit guard | 2-3 days |
| SP-3 | Execution: Bash + PowerShell (shared subprocess handler module) | 1-2 days |
| SP-4 | Search: Glob + Grep (via ripgrep library linkage, not shell-out) | 1-2 days |
| SP-5 | Network: WebFetch (reqwest + html2md + size budget) | 1 day |
| SP-6 | Cross-crate E2E: `hello_atd.py` + Rust `hello_atd` rewired to target atd-ref-server. New `docs/validation/` evidence. Demo video recording (if governance permits). | half day |

Each SP gets its own spec → plan → implementation cycle. SP-1's framework is stable enough that later SPs shouldn't need to touch it (except adding CallContext fields, which is backwards-compatible).

---

## 11. Design decisions locked in (don't revisit without cause)

1. **Clean-room implementation from public catalog + universal tool semantics + Rust ecosystem crates.** No reading of Claude Code source; no byte-level reproduction.
2. **Tool trait uses Rust 1.75+ native `async fn`** (not `#[async_trait]` macro).
3. **`ToolCallError` is local to atd-ref-server**; not a reuse of `atd-types::AtdError`. Rationale: different error axes.
4. **Registry uses `Arc<dyn Tool>`.** Duplicate-registration panics.
5. **Three state-lifetime layers: global / per-connection / per-call.** SP-1 uses only global + per-call; SP-2 activates the per-connection layer via a new `CallContext` field.
6. **Framework-uniform `dry_run` in SP-1.** `Tool::dry_run_preview` is added in SP-2+ as a defaulted, backwards-compatible method.
7. **Integration tests are self-contained** — no dependency on atd-client or any sibling crate. Cross-crate E2E is an SP-2+ activity.
8. **Tool panics are a contract violation**, documented in README. SP-1 does not `catch_unwind`.
9. **Unix socket only**; stdio / HTTP are Phase 2+.
10. **Max frame 10 MiB, socket permissions 0600** — matches atd-client defaults.

---

## 12. Open questions (none blocking)

All design questions surfaced during brainstorming have been resolved and locked in §11. No gating questions remain for SP-1 to proceed to the implementation plan.

Two forward-looking notes (non-blocking, recorded so future SPs don't re-litigate):

- **ServerConfig hot-reload:** Not in scope. Config is build-once at startup; for now `--max-output-bytes` et al. are read from CLI args only.
- **Multi-tenant support:** Out of scope forever at this layer. If needed, it belongs in a wrapper (auth proxy in front of atd-ref-server), not in the server itself.
