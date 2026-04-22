# atd-ref-server SP-1 Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the framework layer of `atd-ref-server` — a standalone, neutral ATD server crate with a single test-anchor tool (`ref:echo.say`), serving as the foundation for SP-2+ real tools.

**Architecture:** New workspace crate `crates/atd-ref-server/` producing binary `atd-ref-server`. Independent `Tool` trait + `Registry` + `CallContext` + `ToolCallError`. Own wire codec (~70 LOC, byte-compatible with Rust client, independently re-implemented). Own `Request`/`Response` enums (independent of `atd-client::protocol` but sharing the same wire tags). Unix-socket listener + per-connection task + request dispatcher. Zero runtime/dev dep on `atd-client`, `atd-mcp-bridge`, `atd-cli`, or any `anos-*` crate — independence verified via `cargo tree`.

**Tech Stack:** Rust 2024, MSRV 1.85 · tokio (net, io-util, rt-multi-thread, macros, process, sync, time) · serde + serde_json · thiserror · ulid · clap (with derive) · dev: tempfile

**Spec:** `docs/superpowers/specs/2026-04-22-atd-ref-server-sp1-foundation.md`

**Scope boundary:**
- **In:** crate scaffold, wire codec, protocol types, Tool trait, Registry, CallContext, ToolCallError, Server + dispatch, binary main, `ref:echo.say` tool, README, integration test harness.
- **Out (deferred to SP-2+):** any real tool (Read/Write/Edit/Bash/PowerShell/Glob/Grep/WebFetch), ReadTracker per-connection state, per-tool `dry_run_preview`, capability tokens, session/cancel, stdio/HTTP transport, panic recovery, graceful shutdown.

**Prerequisites:**
- Repo at tag `phase1-python`, workspace passing 127 tests, zero `anos-*` deps.
- `cargo build --workspace` clean.
- atd-cli binary built (for manual smoke in Task 12) — `./target/release/atd` exists after Phase 0 weeks 2-3.

**Exit criteria (mirrors spec §9):**
1. `cargo build -p atd-ref-server --release` zero warnings.
2. `cargo test -p atd-ref-server` — ~32 tests green.
3. `cargo test --workspace` — no regressions (127 prior still pass).
4. `cargo tree -p atd-ref-server --prefix none | grep -E '^(anos-|atd-client|atd-mcp-bridge|atd-cli)'` empty.
5. Manual smoke: `atd-ref-server --sock /tmp/ref.sock` + `atd doctor --sock /tmp/ref.sock` shows 1 tool; `atd call ref:echo.say --args '{"msg":"hi"}' --sock /tmp/ref.sock` returns echo.
6. README has all 6 required sections (spec §8).
7. Tag `sp1-ref-server-foundation` created.

---

## File Structure

```
atd-mvp/
├── Cargo.toml                                   (MODIFY — add workspace member)
├── crates/atd-ref-server/                       (NEW crate)
│   ├── Cargo.toml
│   ├── README.md                                (Task 12)
│   └── src/
│       ├── main.rs                              (Task 10 — CLI + Server::run)
│       ├── lib.rs                               (Tasks 2,3,4,5,6,8,9 each append)
│       ├── wire.rs                              (Task 2 — ~70 LOC)
│       ├── protocol.rs                          (Task 3 — ~90 LOC)
│       ├── error.rs                             (Task 4 — ~30 LOC)
│       ├── context.rs                           (Task 5 — ~60 LOC)
│       ├── registry.rs                          (Task 6 — ~80 LOC)
│       ├── builtin.rs                           (Task 8 — ~20 LOC)
│       ├── server.rs                            (Task 9 — ~150 LOC)
│       └── tools/
│           ├── mod.rs                           (Task 7 — 1 line)
│           └── echo.rs                          (Task 7 — ~80 LOC)
│   └── tests/
│       └── integration.rs                       (Task 11 — ~200 LOC)
```

**File responsibilities (locked by spec §3.1):** one module per concern, each <200 LOC, no mixed responsibilities.

---

## Task 1: Crate scaffold + workspace registration

**Files:**
- Create: `crates/atd-ref-server/Cargo.toml`
- Create: `crates/atd-ref-server/src/main.rs` (placeholder)
- Create: `crates/atd-ref-server/src/lib.rs` (empty)
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1.1: Write `crates/atd-ref-server/Cargo.toml`**

```toml
[package]
name = "atd-ref-server"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Neutral reference server for the Agent Tool Dispatch (ATD) protocol — ships with real tools, zero dependency on any specific agent framework."

[lib]
name = "atd_ref_server"
path = "src/lib.rs"

[[bin]]
name = "atd-ref-server"
path = "src/main.rs"

[dependencies]
atd-types = { path = "../atd-types", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
thiserror = { workspace = true }
ulid = { workspace = true }
clap = { version = "4", features = ["derive"] }

[dev-dependencies]
tempfile = { workspace = true }
```

Note: `ulid` is no longer in workspace.dependencies (dropped in Phase 0 debt-4). Add it back to the workspace root.

- [ ] **Step 1.2: Add `ulid` back to the workspace `Cargo.toml` dependencies table**

Edit `/home/nan/proj/atd-mvp/Cargo.toml` and add `ulid = { version = "1", features = ["serde"] }` to `[workspace.dependencies]`:

```toml
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["net", "io-util", "rt-multi-thread", "macros", "sync", "time", "process"] }
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
ulid = { version = "1", features = ["serde"] }
tempfile = "3"
```

Note also: `tokio` features get `"process"` added (for SP-3 Bash, but adding now avoids a later workspace edit).

- [ ] **Step 1.3: Write placeholder `main.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/main.rs`:

```rust
//! `atd-ref-server` — neutral reference server for the ATD protocol.
//!
//! Real entry wiring lands in Task 10. This placeholder exits with a clear
//! message so the crate compiles during bootstrap.

fn main() {
    eprintln!("atd-ref-server: scaffold — real entry lands in Task 10");
    std::process::exit(1);
}
```

- [ ] **Step 1.4: Write empty `lib.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.
//!
//! Modules land across Tasks 2-9.
```

- [ ] **Step 1.5: Add crate to workspace `members`**

Edit `/home/nan/proj/atd-mvp/Cargo.toml` — extend the `members` line. From:

```toml
members = ["crates/atd-types", "crates/atd-client", "crates/atd-cli", "crates/atd-mcp-bridge", "examples"]
```

to:

```toml
members = ["crates/atd-types", "crates/atd-client", "crates/atd-cli", "crates/atd-mcp-bridge", "crates/atd-ref-server", "examples"]
```

- [ ] **Step 1.6: Build and smoke-run**

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server --bin atd-ref-server
./target/debug/atd-ref-server
echo "exit=$?"
```

Expected: builds with no warnings; stderr `atd-ref-server: scaffold — real entry lands in Task 10`; `exit=1`.

- [ ] **Step 1.7: Workspace regression**

```bash
cargo test --workspace --all-targets
```

Expected: 127 tests still green.

- [ ] **Step 1.8: Commit**

```bash
git add crates/atd-ref-server/ Cargo.toml Cargo.lock
git commit -m "feat(atd-ref-server): scaffold crate with bin + lib targets"
```

---

## Task 2: Wire codec

**Files:**
- Create: `crates/atd-ref-server/src/wire.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

Byte-compatible with `atd-client::wire`, independently re-implemented. 4-byte big-endian `u32` length + UTF-8 JSON body, 10 MiB cap.

- [ ] **Step 2.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/wire.rs`:

```rust
//! Length-prefixed JSON wire codec.
//!
//! Byte-compatible with `atd-client::wire` but independently implemented —
//! server and client never share code, only the format.

use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_FRAME_BYTES: usize = 10 * 1024 * 1024;

pub async fn write_frame<W, T>(writer: &mut W, msg: &T) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let body = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(body.len()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame too large: {} bytes", body.len()),
        )
    })?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_frame<R, T>(reader: &mut R) -> std::io::Result<T>
where
    R: AsyncReadExt + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes"),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct M {
        kind: String,
        n: u32,
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() {
        let msg = M { kind: "ping".into(), n: 7 };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();

        let mut cursor = std::io::Cursor::new(&buf);
        let back: M = read_frame(&mut cursor).await.unwrap();
        assert_eq!(back, msg);
    }

    #[tokio::test]
    async fn frame_uses_big_endian_u32_prefix() {
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &M { kind: "x".into(), n: 1 }).await.unwrap();
        let body_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        assert_eq!(body_len, buf.len() - 4);
    }

    #[tokio::test]
    async fn oversized_frame_errors() {
        let mut header = Vec::new();
        let bogus_len: u32 = 20 * 1024 * 1024;
        header.extend_from_slice(&bogus_len.to_be_bytes());
        let mut cursor = std::io::Cursor::new(header);
        let err = read_frame::<_, M>(&mut cursor).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.

pub mod wire;
```

- [ ] **Step 2.2: Run**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib wire
```

Expected: `3 passed; 0 failed`.

- [ ] **Step 2.3: Commit**

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add length-prefixed JSON wire codec"
```

---

## Task 3: Protocol types

**Files:**
- Create: `crates/atd-ref-server/src/protocol.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

Local `Request` / `Response` enums. Serde tags match `atd-client::protocol` verbatim so both sides speak the same JSON, but the type definitions are independent.

- [ ] **Step 3.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/protocol.rs`:

```rust
//! Wire message types.
//!
//! Tag names match the Rust atd-client (`ping`, `tool_list`, `tool_schema`,
//! `run_tool`, `pong`, `error`) so both sides speak the same JSON. Type
//! definitions are independent — this server has no dep on atd-client.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "tool_list")]
    ToolList,

    #[serde(rename = "tool_schema")]
    ToolSchema { tool_id: String },

    #[serde(rename = "run_tool")]
    RunTool {
        tool_id: String,
        args: serde_json::Value,
        dry_run: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "pong")]
    Pong,

    #[serde(rename = "tool_list")]
    ToolList { tools: serde_json::Value },

    #[serde(rename = "tool_schema")]
    ToolSchema { schema: serde_json::Value },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_id: String,
        result: serde_json::Value,
        success: bool,
        dry_run: bool,
    },

    #[serde(rename = "error")]
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retryable: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_serializes_with_type_tag() {
        assert_eq!(
            serde_json::to_string(&Request::Ping).unwrap(),
            r#"{"type":"ping"}"#
        );
    }

    #[test]
    fn tool_list_request_is_unit_variant_on_wire() {
        let j = serde_json::to_string(&Request::ToolList).unwrap();
        assert_eq!(j, r#"{"type":"tool_list"}"#);
    }

    #[test]
    fn tool_schema_carries_tool_id() {
        let r = Request::ToolSchema { tool_id: "ref:echo.say".into() };
        let j = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&j).unwrap();
        match back {
            Request::ToolSchema { tool_id } => assert_eq!(tool_id, "ref:echo.say"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn run_tool_roundtrip_with_all_fields() {
        let r = Request::RunTool {
            tool_id: "ref:echo.say".into(),
            args: serde_json::json!({"a": 1, "b": [2]}),
            dry_run: true,
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&j).unwrap();
        match back {
            Request::RunTool { tool_id, args, dry_run } => {
                assert_eq!(tool_id, "ref:echo.say");
                assert_eq!(args["a"], 1);
                assert!(dry_run);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tool_result_serializes_with_success_flag() {
        let r = Response::ToolResult {
            tool_id: "ref:echo.say".into(),
            result: serde_json::json!({"echoed": {}}),
            success: true,
            dry_run: false,
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains(r#""type":"tool_result""#));
        assert!(j.contains(r#""success":true"#));
    }

    #[test]
    fn error_response_omits_null_optionals_when_missing() {
        let r = Response::Error {
            message: "boom".into(),
            code: None,
            retryable: None,
            details: None,
        };
        let j = serde_json::to_string(&r).unwrap();
        assert_eq!(j, r#"{"type":"error","message":"boom"}"#);
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.

pub mod protocol;
pub mod wire;
```

- [ ] **Step 3.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib protocol
```

Expected: `6 passed`.

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add Request/Response protocol types"
```

---

## Task 4: `ToolCallError`

**Files:**
- Create: `crates/atd-ref-server/src/error.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

Local error classification. Four variants matching spec §4.2.

- [ ] **Step 4.1: Write the type and its failing Display tests**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/error.rs`:

```rust
//! Errors a tool may return.
//!
//! Axes chosen to map cleanly to the wire protocol:
//! - InvalidArgs / InternalError → wire `error` response
//! - ExecutionFailed → wire `tool_result { success: false }` response
//!
//! Named `ToolCallError` (not reusing `atd-types::AtdError`) because
//! client-side and server-side errors classify different concerns.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ToolCallError {
    /// Schema validation failed or args couldn't be coerced to the expected
    /// shape. The tool's own logic did not execute.
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    /// Tool ran to completion but reports a failure outcome. This is the
    /// domain-level "the operation didn't succeed" case, not a server error.
    #[error("execution failed ({code}): {message}")]
    ExecutionFailed {
        code: String,
        message: String,
        retryable: bool,
    },

    /// Server-side bug or unexpected condition during tool invocation.
    #[error("internal error: {0}")]
    InternalError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_args_display_format() {
        let e = ToolCallError::InvalidArgs("missing field `path`".into());
        assert_eq!(format!("{e}"), "invalid arguments: missing field `path`");
    }

    #[test]
    fn execution_failed_display_includes_code_and_message() {
        let e = ToolCallError::ExecutionFailed {
            code: "EPERM".into(),
            message: "denied".into(),
            retryable: false,
        };
        let s = format!("{e}");
        assert!(s.contains("EPERM"));
        assert!(s.contains("denied"));
    }

    #[test]
    fn internal_error_display_format() {
        let e = ToolCallError::InternalError("logic bug".into());
        assert_eq!(format!("{e}"), "internal error: logic bug");
    }

    #[test]
    fn enum_is_non_exhaustive_at_api_boundary() {
        // This test exists to document that consumers outside the crate must
        // match `_ =>` — we don't commit to the variant set forever.
        let e = ToolCallError::InvalidArgs("x".into());
        match e {
            ToolCallError::InvalidArgs(_) => {}
            ToolCallError::ExecutionFailed { .. } => {}
            ToolCallError::InternalError(_) => {}
        }
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.

pub mod error;
pub mod protocol;
pub mod wire;
```

- [ ] **Step 4.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib error
```

Expected: `4 passed`.

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add ToolCallError classification"
```

---

## Task 5: `CallContext`

**Files:**
- Create: `crates/atd-ref-server/src/context.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

Per-call state: cwd, output budget, call_id, optional deadline. `for_test()` gated on `#[cfg(any(test, feature = "testing"))]`.

- [ ] **Step 5.1: Add the `testing` feature to `Cargo.toml`**

Edit `/home/nan/proj/atd-mvp/crates/atd-ref-server/Cargo.toml` — add after `[dev-dependencies]`:

```toml
[features]
testing = []
```

- [ ] **Step 5.2: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/context.rs`:

```rust
//! Per-call context passed to every `Tool::call` invocation.

use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct CallContext {
    /// Working directory for relative-path tools (Read / Bash / Glob / ...).
    pub cwd: PathBuf,
    /// Advisory truncation budget. Tools should respect this and return
    /// truncation markers when producing larger output.
    pub max_output_bytes: usize,
    /// Unique id for tracing/logging; not emitted on the wire.
    pub call_id: ulid::Ulid,
    /// Absolute deadline. Tools that wrap long operations in tokio::time::timeout
    /// should pass `remaining_time()` as the budget.
    pub deadline: Option<Instant>,
}

impl CallContext {
    pub fn remaining_time(&self) -> Option<Duration> {
        self.deadline.map(|d| d.saturating_duration_since(Instant::now()))
    }
}

#[cfg(any(test, feature = "testing"))]
impl CallContext {
    /// Construct a sensible default for unit tests. cwd = current dir,
    /// 1 MiB output budget, fresh call_id, no deadline.
    pub fn for_test() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            call_id: ulid::Ulid::new(),
            deadline: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_test_has_sensible_defaults() {
        let ctx = CallContext::for_test();
        assert!(ctx.cwd.exists(), "cwd should be a real directory");
        assert_eq!(ctx.max_output_bytes, 1_048_576);
        assert!(ctx.deadline.is_none());
    }

    #[test]
    fn remaining_time_is_none_when_no_deadline() {
        let ctx = CallContext::for_test();
        assert!(ctx.remaining_time().is_none());
    }

    #[test]
    fn remaining_time_counts_down_from_deadline() {
        let ctx = CallContext {
            cwd: PathBuf::from("."),
            max_output_bytes: 1024,
            call_id: ulid::Ulid::new(),
            deadline: Some(Instant::now() + Duration::from_secs(5)),
        };
        let r = ctx.remaining_time().unwrap();
        assert!(r <= Duration::from_secs(5));
        assert!(r > Duration::from_secs(4));
    }

    #[test]
    fn remaining_time_saturates_to_zero_after_deadline() {
        let ctx = CallContext {
            cwd: PathBuf::from("."),
            max_output_bytes: 1024,
            call_id: ulid::Ulid::new(),
            deadline: Some(Instant::now() - Duration::from_secs(10)),
        };
        assert_eq!(ctx.remaining_time().unwrap(), Duration::ZERO);
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.

pub mod context;
pub mod error;
pub mod protocol;
pub mod wire;
```

- [ ] **Step 5.3: Run + commit**

```bash
cargo test -p atd-ref-server --lib context
```

Expected: `4 passed`.

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add CallContext with for_test helper"
```

---

## Task 6: Tool trait + Registry

**Files:**
- Create: `crates/atd-ref-server/src/registry.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

The trait that defines what a tool is, and the registry that holds them.

- [ ] **Step 6.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/registry.rs`:

```rust
//! `Tool` trait + `Registry` — the contract third-party implementers see.

use std::collections::HashMap;
use std::sync::Arc;

use atd_types::{ToolDefinition, ToolSummary};

use crate::context::CallContext;
use crate::error::ToolCallError;

/// A tool. One `impl Tool for MyTool` per tool; registered once at startup.
/// Tools MUST NOT panic; they return `Err(ToolCallError)` instead.
pub trait Tool: Send + Sync {
    /// Stable borrow of the tool's definition. Registry calls this once at
    /// registration time (for summaries/schema lookup) — implementers
    /// typically store a single `ToolDefinition` in the struct.
    fn definition(&self) -> &ToolDefinition;

    /// Invoke the tool. Args are the deserialized JSON from the wire.
    async fn call(
        &self,
        args: serde_json::Value,
        ctx: &CallContext,
    ) -> Result<serde_json::Value, ToolCallError>;
}

pub struct Registry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Registry {
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    /// Register a tool. Panics on duplicate tool_id — startup misconfiguration
    /// should fail loud, not at request time.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let id = tool.definition().id.clone();
        if self.tools.contains_key(&id) {
            panic!("duplicate tool registration: {id}");
        }
        self.tools.insert(id, tool);
    }

    pub fn get(&self, tool_id: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(tool_id)
    }

    pub fn summaries(&self) -> Vec<ToolSummary> {
        self.tools
            .values()
            .map(|t| ToolSummary::from(t.definition()))
            .collect()
    }

    pub fn count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_types::{
        BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources, ToolSafety,
        ToolTrust, ToolVisibility, TrustLevel,
    };

    struct StubTool {
        def: ToolDefinition,
    }

    impl StubTool {
        fn new(id: &str) -> Self {
            Self {
                def: ToolDefinition {
                    id: id.into(),
                    name: id.into(),
                    description: "stub".into(),
                    version: "0.0.0".into(),
                    capability: ToolCapability {
                        domain: "stub".into(),
                        actions: vec![],
                        tags: vec![],
                        intent_examples: vec![],
                    },
                    input_schema: serde_json::json!({}),
                    output_schema: serde_json::json!({}),
                    bindings: vec![ToolBinding {
                        protocol: BindingProtocol::Cli,
                        config: serde_json::json!({}),
                    }],
                    safety: ToolSafety {
                        level: SafetyLevel::Read,
                        dry_run: false,
                        side_effects: vec![],
                        data_sensitivity: None,
                    },
                    resources: ToolResources {
                        timeout_ms: 1000,
                        max_concurrent: 1,
                        rate_limit_per_min: None,
                        estimated_tokens: None,
                    },
                    trust: ToolTrust {
                        publisher: "test".into(),
                        trust_level: TrustLevel::L0Unverified,
                        signature: None,
                    },
                    visibility: ToolVisibility::Read,
                },
            }
        }
    }

    impl Tool for StubTool {
        fn definition(&self) -> &ToolDefinition {
            &self.def
        }
        async fn call(
            &self,
            _args: serde_json::Value,
            _ctx: &CallContext,
        ) -> Result<serde_json::Value, ToolCallError> {
            Ok(serde_json::json!({}))
        }
    }

    #[test]
    fn register_and_get_returns_the_tool() {
        let mut r = Registry::new();
        r.register(Arc::new(StubTool::new("test:a")));
        assert!(r.get("test:a").is_some());
        assert!(r.get("test:missing").is_none());
    }

    #[test]
    fn summaries_projects_registered_tools() {
        let mut r = Registry::new();
        r.register(Arc::new(StubTool::new("test:a")));
        r.register(Arc::new(StubTool::new("test:b")));
        let sums = r.summaries();
        assert_eq!(sums.len(), 2);
        let ids: std::collections::HashSet<_> = sums.iter().map(|s| s.id.clone()).collect();
        assert!(ids.contains("test:a"));
        assert!(ids.contains("test:b"));
    }

    #[test]
    #[should_panic(expected = "duplicate tool registration: test:a")]
    fn duplicate_registration_panics() {
        let mut r = Registry::new();
        r.register(Arc::new(StubTool::new("test:a")));
        r.register(Arc::new(StubTool::new("test:a")));
    }

    #[test]
    fn empty_registry_reports_zero() {
        let r = Registry::new();
        assert_eq!(r.count(), 0);
        assert!(r.summaries().is_empty());
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.

pub mod context;
pub mod error;
pub mod protocol;
pub mod registry;
pub mod wire;
```

- [ ] **Step 6.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib registry
```

Expected: `4 passed`.

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add Tool trait and Registry"
```

---

## Task 7: `ref:echo.say` tool

**Files:**
- Create: `crates/atd-ref-server/src/tools/mod.rs`
- Create: `crates/atd-ref-server/src/tools/echo.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

The one test-anchor tool for SP-1. Echoes args. Truncates when serialized output exceeds `ctx.max_output_bytes`.

- [ ] **Step 7.1: Create `tools/mod.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/mod.rs`:

```rust
//! Built-in tools. SP-1 ships only the echo test-anchor; SP-2+ add real tools.

pub mod echo;
```

- [ ] **Step 7.2: Write the failing test + echo tool**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/echo.rs`:

```rust
//! `ref:echo.say` — echoes its args. The SP-1 test-anchor tool.
//!
//! Intentionally trivial. Its role is to prove the framework wires up
//! end-to-end (registry → dispatch → wire → client). Real tools land in SP-2+.

use std::sync::OnceLock;

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::Tool;

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:echo.say".into(),
        name: "Echo".into(),
        description: "Echoes input args back verbatim. Framework test anchor.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "echo".into(),
            actions: vec!["say".into()],
            tags: vec!["test".into(), "framework".into()],
            intent_examples: vec!["echo this".into()],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": true,
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "echoed": {},
                "truncated": { "type": "boolean" },
                "original_bytes": { "type": "integer" }
            }
        }),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Read,
            dry_run: false,
            side_effects: vec![],
            data_sensitivity: None,
        },
        resources: ToolResources {
            timeout_ms: 5_000,
            max_concurrent: 100,
            rate_limit_per_min: None,
            estimated_tokens: Some(10),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Read,
    })
}

pub struct EchoTool;

impl EchoTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EchoTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for EchoTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    async fn call(
        &self,
        args: serde_json::Value,
        ctx: &CallContext,
    ) -> Result<serde_json::Value, ToolCallError> {
        // Estimate output size: serialized length of `{"echoed": <args>}`.
        let serialized = serde_json::to_vec(&args)
            .map_err(|e| ToolCallError::InternalError(format!("serialize args: {e}")))?;
        let estimated = serialized.len() + 16; // envelope overhead
        if estimated > ctx.max_output_bytes {
            // Return a truncation marker instead of the full echo.
            return Ok(serde_json::json!({
                "truncated": true,
                "original_bytes": serialized.len(),
                "max_output_bytes": ctx.max_output_bytes,
            }));
        }
        Ok(serde_json::json!({ "echoed": args }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn happy_path_echoes_args_verbatim() {
        let t = EchoTool::new();
        let ctx = CallContext::for_test();
        let args = serde_json::json!({"hello": "world", "n": 42});
        let r = t.call(args.clone(), &ctx).await.unwrap();
        assert_eq!(r, serde_json::json!({"echoed": args}));
    }

    #[tokio::test]
    async fn empty_args_echoed_as_empty_object() {
        let t = EchoTool::new();
        let ctx = CallContext::for_test();
        let r = t.call(serde_json::json!({}), &ctx).await.unwrap();
        assert_eq!(r, serde_json::json!({"echoed": {}}));
    }

    #[tokio::test]
    async fn oversized_args_return_truncation_marker() {
        let t = EchoTool::new();
        // Tiny budget so even a small payload overflows.
        let ctx = CallContext {
            cwd: std::path::PathBuf::from("."),
            max_output_bytes: 32,
            call_id: ulid::Ulid::new(),
            deadline: None,
        };
        let big = "x".repeat(1_000);
        let args = serde_json::json!({"big": big});
        let r = t.call(args, &ctx).await.unwrap();
        assert_eq!(r["truncated"], serde_json::json!(true));
        assert!(r["original_bytes"].as_u64().unwrap() > 32);
        assert!(r.get("echoed").is_none());
    }

    #[test]
    fn definition_has_expected_id_and_domain() {
        let t = EchoTool::new();
        let d = t.definition();
        assert_eq!(d.id, "ref:echo.say");
        assert_eq!(d.capability.domain, "echo");
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.

pub mod context;
pub mod error;
pub mod protocol;
pub mod registry;
pub mod tools;
pub mod wire;
```

- [ ] **Step 7.3: Run + commit**

```bash
cargo test -p atd-ref-server --lib tools::echo
```

Expected: `4 passed`.

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add ref:echo.say test-anchor tool"
```

---

## Task 8: Builtin registration

**Files:**
- Create: `crates/atd-ref-server/src/builtin.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

One function that builds the startup Registry. SP-2+ adds more tools here.

- [ ] **Step 8.1: Write the test + impl**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/builtin.rs`:

```rust
//! Built-in tool registration for `atd-ref-server`.
//!
//! To add a new tool:
//! 1. Create `src/tools/<name>.rs` implementing `Tool`.
//! 2. Export it from `tools/mod.rs`.
//! 3. Add `reg.register(Arc::new(<Name>Tool::new()))` below.

use std::sync::Arc;

use crate::registry::Registry;
use crate::tools::echo::EchoTool;

pub fn builtin_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoTool::new()));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_echo() {
        let r = builtin_registry();
        assert_eq!(r.count(), 1);
        assert!(r.get("ref:echo.say").is_some());
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.

pub mod builtin;
pub mod context;
pub mod error;
pub mod protocol;
pub mod registry;
pub mod tools;
pub mod wire;
```

- [ ] **Step 8.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib builtin
```

Expected: `1 passed`.

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): builtin_registry() registers echo tool"
```

---

## Task 9: `Server` struct + dispatcher

**Files:**
- Create: `crates/atd-ref-server/src/server.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

The Server struct, per-connection handler, and the dispatcher that maps requests to responses per spec §5.2/§5.3.

- [ ] **Step 9.1: Write the failing test + impl**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/server.rs`:

```rust
//! Server loop + request dispatcher.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::{UnixListener, UnixStream};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::protocol::{Request, Response};
use crate::registry::Registry;
use crate::wire::{read_frame, write_frame};

pub struct ServerConfig {
    pub socket_path: PathBuf,
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub default_call_timeout_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            socket_path: PathBuf::from(home).join(".atd-ref").join("server.sock"),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            default_call_timeout_ms: 60_000,
        }
    }
}

pub struct Server {
    state: Arc<ServerState>,
}

struct ServerState {
    registry: Registry,
    config: ServerConfig,
}

impl Server {
    pub fn new(registry: Registry, config: ServerConfig) -> Self {
        Self {
            state: Arc::new(ServerState { registry, config }),
        }
    }

    pub async fn run(self) -> std::io::Result<()> {
        let sock = &self.state.config.socket_path;

        // Ensure parent dir exists.
        if let Some(parent) = sock.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Remove stale socket.
        if sock.exists() {
            std::fs::remove_file(sock)?;
        }

        let listener = UnixListener::bind(sock)?;
        // Unix 0600: owner-only (best we can do portably via std::fs::Permissions)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(sock, perms);
        }

        eprintln!(
            "atd-ref-server: listening on {:?} ({} tool(s) registered)",
            sock,
            self.state.registry.count()
        );

        loop {
            let (stream, _) = listener.accept().await?;
            let state = self.state.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(state, stream).await {
                    eprintln!("atd-ref-server: connection error: {e}");
                }
            });
        }
    }
}

async fn handle_connection(state: Arc<ServerState>, stream: UnixStream) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    loop {
        let req: Request = match read_frame(&mut reader).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let resp = dispatch(&state, req).await;
        write_frame(&mut writer, &resp).await?;
    }
}

pub(crate) async fn dispatch(state: &Arc<ServerState>, req: Request) -> Response {
    match req {
        Request::Ping => Response::Pong,
        Request::ToolList => {
            let summaries = state.registry.summaries();
            Response::ToolList {
                tools: serde_json::to_value(&summaries).unwrap_or_else(|_| serde_json::json!([])),
            }
        }
        Request::ToolSchema { tool_id } => match state.registry.get(&tool_id) {
            Some(tool) => Response::ToolSchema {
                schema: serde_json::to_value(tool.definition())
                    .unwrap_or_else(|_| serde_json::json!({})),
            },
            None => Response::Error {
                message: format!("tool not found: {tool_id}"),
                code: None,
                retryable: Some(false),
                details: None,
            },
        },
        Request::RunTool { tool_id, args, dry_run } => {
            if dry_run {
                // SP-1 uniform dry-run: framework synthesizes a preview, does
                // not call the tool. SP-2+ adds per-tool Tool::dry_run_preview.
                return Response::ToolResult {
                    tool_id: tool_id.clone(),
                    result: serde_json::json!({
                        "dry_run": true,
                        "tool_id": tool_id,
                        "args_preview": args,
                    }),
                    success: true,
                    dry_run: true,
                };
            }
            let tool = match state.registry.get(&tool_id) {
                Some(t) => t.clone(),
                None => {
                    return Response::Error {
                        message: format!("tool not found: {tool_id}"),
                        code: None,
                        retryable: Some(false),
                        details: None,
                    };
                }
            };
            let ctx = CallContext {
                cwd: state.config.cwd.clone(),
                max_output_bytes: state.config.max_output_bytes,
                call_id: ulid::Ulid::new(),
                deadline: Some(
                    Instant::now() + Duration::from_millis(state.config.default_call_timeout_ms),
                ),
            };
            match tool.call(args, &ctx).await {
                Ok(data) => Response::ToolResult {
                    tool_id,
                    result: data,
                    success: true,
                    dry_run: false,
                },
                Err(ToolCallError::InvalidArgs(msg)) => Response::Error {
                    message: format!("invalid args for {tool_id}: {msg}"),
                    code: None,
                    retryable: Some(false),
                    details: None,
                },
                Err(ToolCallError::ExecutionFailed { code, message, retryable }) => {
                    Response::ToolResult {
                        tool_id,
                        result: serde_json::json!({
                            "code": code,
                            "message": message,
                            "retryable": retryable,
                        }),
                        success: false,
                        dry_run: false,
                    }
                }
                Err(ToolCallError::InternalError(msg)) => Response::Error {
                    message: format!("internal error in {tool_id}: {msg}"),
                    code: None,
                    retryable: Some(false),
                    details: None,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::builtin_registry;

    fn test_state() -> Arc<ServerState> {
        Arc::new(ServerState {
            registry: builtin_registry(),
            config: ServerConfig {
                socket_path: PathBuf::from("/tmp/unused-in-dispatch-tests.sock"),
                cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                max_output_bytes: 1_048_576,
                default_call_timeout_ms: 60_000,
            },
        })
    }

    #[tokio::test]
    async fn ping_returns_pong() {
        let s = test_state();
        let r = dispatch(&s, Request::Ping).await;
        assert!(matches!(r, Response::Pong));
    }

    #[tokio::test]
    async fn tool_list_returns_registered_summaries() {
        let s = test_state();
        let r = dispatch(&s, Request::ToolList).await;
        match r {
            Response::ToolList { tools } => {
                let arr = tools.as_array().unwrap();
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0]["id"], "ref:echo.say");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn tool_schema_found_returns_definition() {
        let s = test_state();
        let r = dispatch(
            &s,
            Request::ToolSchema { tool_id: "ref:echo.say".into() },
        )
        .await;
        match r {
            Response::ToolSchema { schema } => {
                assert_eq!(schema["id"], "ref:echo.say");
                assert_eq!(schema["capability"]["domain"], "echo");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn tool_schema_not_found_returns_error() {
        let s = test_state();
        let r = dispatch(
            &s,
            Request::ToolSchema { tool_id: "ref:missing".into() },
        )
        .await;
        match r {
            Response::Error { message, .. } => {
                assert!(message.contains("tool not found"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn run_tool_success_wraps_data() {
        let s = test_state();
        let r = dispatch(
            &s,
            Request::RunTool {
                tool_id: "ref:echo.say".into(),
                args: serde_json::json!({"k": "v"}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::ToolResult { result, success, dry_run, .. } => {
                assert!(success);
                assert!(!dry_run);
                assert_eq!(result["echoed"]["k"], "v");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn run_tool_dry_run_returns_preview_without_calling_tool() {
        let s = test_state();
        let r = dispatch(
            &s,
            Request::RunTool {
                tool_id: "ref:echo.say".into(),
                args: serde_json::json!({"x": 1}),
                dry_run: true,
            },
        )
        .await;
        match r {
            Response::ToolResult { result, success, dry_run, .. } => {
                assert!(success);
                assert!(dry_run);
                assert_eq!(result["dry_run"], serde_json::json!(true));
                assert_eq!(result["args_preview"]["x"], 1);
                // Notably: no "echoed" key — tool was NOT called.
                assert!(result.get("echoed").is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn run_tool_unknown_id_returns_error() {
        let s = test_state();
        let r = dispatch(
            &s,
            Request::RunTool {
                tool_id: "ref:missing".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::Error { message, .. } => {
                assert!(message.contains("tool not found"));
            }
            _ => panic!("wrong variant"),
        }
    }

    // --- ToolCallError → Response mapping tests (spec §5.3) ---

    #[derive(Clone, Copy)]
    enum FailureMode {
        InvalidArgs,
        ExecutionFailed,
        InternalError,
    }

    struct FailingTool {
        def: atd_types::ToolDefinition,
        mode: FailureMode,
    }

    impl FailingTool {
        fn new(id: &str, mode: FailureMode) -> Self {
            use atd_types::{
                BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolResources,
                ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
            };
            Self {
                def: atd_types::ToolDefinition {
                    id: id.into(),
                    name: id.into(),
                    description: "test failure tool".into(),
                    version: "0.0.0".into(),
                    capability: ToolCapability {
                        domain: "test".into(),
                        actions: vec![],
                        tags: vec![],
                        intent_examples: vec![],
                    },
                    input_schema: serde_json::json!({}),
                    output_schema: serde_json::json!({}),
                    bindings: vec![ToolBinding {
                        protocol: BindingProtocol::Cli,
                        config: serde_json::json!({}),
                    }],
                    safety: ToolSafety {
                        level: SafetyLevel::Read,
                        dry_run: false,
                        side_effects: vec![],
                        data_sensitivity: None,
                    },
                    resources: ToolResources {
                        timeout_ms: 1000,
                        max_concurrent: 1,
                        rate_limit_per_min: None,
                        estimated_tokens: None,
                    },
                    trust: ToolTrust {
                        publisher: "test".into(),
                        trust_level: TrustLevel::L0Unverified,
                        signature: None,
                    },
                    visibility: ToolVisibility::Read,
                },
                mode,
            }
        }
    }

    impl crate::registry::Tool for FailingTool {
        fn definition(&self) -> &atd_types::ToolDefinition {
            &self.def
        }
        async fn call(
            &self,
            _args: serde_json::Value,
            _ctx: &CallContext,
        ) -> Result<serde_json::Value, ToolCallError> {
            match self.mode {
                FailureMode::InvalidArgs => Err(ToolCallError::InvalidArgs("bad field".into())),
                FailureMode::ExecutionFailed => Err(ToolCallError::ExecutionFailed {
                    code: "EPERM".into(),
                    message: "denied".into(),
                    retryable: false,
                }),
                FailureMode::InternalError => Err(ToolCallError::InternalError("bug".into())),
            }
        }
    }

    fn state_with_failing_tool(id: &str, mode: FailureMode) -> Arc<ServerState> {
        let mut reg = Registry::new();
        reg.register(Arc::new(FailingTool::new(id, mode)));
        Arc::new(ServerState {
            registry: reg,
            config: ServerConfig {
                socket_path: PathBuf::from("/tmp/unused.sock"),
                cwd: PathBuf::from("."),
                max_output_bytes: 1024,
                default_call_timeout_ms: 1000,
            },
        })
    }

    #[tokio::test]
    async fn run_tool_invalid_args_error_maps_to_error_response() {
        let s = state_with_failing_tool("test:invalid", FailureMode::InvalidArgs);
        let r = dispatch(
            &s,
            Request::RunTool {
                tool_id: "test:invalid".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::Error { message, .. } => {
                assert!(message.contains("invalid args for test:invalid"));
                assert!(message.contains("bad field"));
            }
            _ => panic!("wrong variant, expected Response::Error"),
        }
    }

    #[tokio::test]
    async fn run_tool_execution_failed_maps_to_tool_result_success_false() {
        let s = state_with_failing_tool("test:exec", FailureMode::ExecutionFailed);
        let r = dispatch(
            &s,
            Request::RunTool {
                tool_id: "test:exec".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::ToolResult { result, success, dry_run, tool_id } => {
                assert!(!success);
                assert!(!dry_run);
                assert_eq!(tool_id, "test:exec");
                assert_eq!(result["code"], "EPERM");
                assert_eq!(result["message"], "denied");
                assert_eq!(result["retryable"], serde_json::json!(false));
            }
            _ => panic!("wrong variant, expected Response::ToolResult"),
        }
    }

    #[tokio::test]
    async fn run_tool_internal_error_maps_to_error_response() {
        let s = state_with_failing_tool("test:internal", FailureMode::InternalError);
        let r = dispatch(
            &s,
            Request::RunTool {
                tool_id: "test:internal".into(),
                args: serde_json::json!({}),
                dry_run: false,
            },
        )
        .await;
        match r {
            Response::Error { message, .. } => {
                assert!(message.contains("internal error in test:internal"));
                assert!(message.contains("bug"));
            }
            _ => panic!("wrong variant, expected Response::Error"),
        }
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/lib.rs`:

```rust
//! Library façade for `atd-ref-server`.

pub mod builtin;
pub mod context;
pub mod error;
pub mod protocol;
pub mod registry;
pub mod server;
pub mod tools;
pub mod wire;
```

- [ ] **Step 9.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib server
cargo test -p atd-ref-server
```

Expected: `server` module 10 passed (7 core + 3 error-path mapping). Full lib suite: 3 wire + 6 protocol + 4 error + 4 context + 4 registry + 4 echo + 1 builtin + 10 server = 36 passing.

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): implement Server + dispatch with 7 branch tests"
```

---

## Task 10: Binary `main.rs`

**Files:**
- Modify: `crates/atd-ref-server/src/main.rs`

Replaces the scaffold with a clap-driven entry point.

- [ ] **Step 10.1: Replace `main.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/main.rs`:

```rust
//! `atd-ref-server` — neutral reference server for the ATD protocol.
//!
//! Runs a Unix-socket server that speaks the standard ATD wire protocol and
//! serves the built-in tool registry. Meant as a fork-friendly reference
//! implementation for third parties writing their own ATD servers.

use std::path::PathBuf;

use atd_ref_server::builtin::builtin_registry;
use atd_ref_server::server::{Server, ServerConfig};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "atd-ref-server",
    version,
    about = "Neutral reference server for the Agent Tool Dispatch (ATD) protocol."
)]
struct Args {
    /// Unix socket path. Default: $HOME/.atd-ref/server.sock
    #[arg(long)]
    sock: Option<PathBuf>,

    /// Working directory for relative-path tools. Default: current directory.
    #[arg(long)]
    cwd: Option<PathBuf>,

    /// Per-call output budget in bytes (advisory; tools honor it).
    #[arg(long, default_value_t = 1_048_576)]
    max_output_bytes: usize,

    /// Per-call deadline in milliseconds.
    #[arg(long, default_value_t = 60_000)]
    timeout_ms: u64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();

    let mut config = ServerConfig::default();
    if let Some(p) = args.sock {
        config.socket_path = p;
    }
    if let Some(p) = args.cwd {
        config.cwd = p;
    }
    config.max_output_bytes = args.max_output_bytes;
    config.default_call_timeout_ms = args.timeout_ms;

    let registry = builtin_registry();
    let server = Server::new(registry, config);

    match server.run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("atd-ref-server: fatal: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
```

- [ ] **Step 10.2: Build + smoke-run**

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server --bin atd-ref-server
./target/debug/atd-ref-server --help 2>&1 | head -20
```

Expected: help output shows `--sock`, `--cwd`, `--max-output-bytes`, `--timeout-ms`.

- [ ] **Step 10.3: Workspace regression + commit**

```bash
cargo test --workspace --all-targets
```

Expected: 127 prior + 36 new = 163 tests passing.

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): wire main.rs with clap + Server::run"
```

---

## Task 11: Integration tests

**Files:**
- Create: `crates/atd-ref-server/tests/integration.rs`

End-to-end: spawn the compiled binary against a tempdir socket, connect with a raw Unix-socket client (self-contained, no dep on `atd-client`), run scripted requests.

- [ ] **Step 11.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs`:

```rust
//! End-to-end integration: spawn the `atd-ref-server` binary and drive it
//! over a real Unix socket with a self-contained client. Deliberately no
//! dependency on `atd-client` — this verifies the server is reachable by
//! any correct ATD client, not a specific SDK.

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atd-ref-server"))
}

/// Roll our own tiny client. Same pattern as atd-client's mock_server.rs but
/// inverted: here the client is in the test file and the server is the
/// production binary we just built.
async fn send_one_request(
    sock: &std::path::Path,
    req: &serde_json::Value,
) -> std::io::Result<serde_json::Value> {
    let mut stream = UnixStream::connect(sock).await?;
    let body = serde_json::to_vec(req).unwrap();
    let len = (body.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;

    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let n = u32::from_be_bytes(header) as usize;
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await?;
    Ok(serde_json::from_slice(&buf).unwrap())
}

struct ServerHandle {
    _child: Child,
    pub sock: PathBuf,
    _tempdir: tempfile::TempDir,
}

async fn spawn_server() -> ServerHandle {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("server.sock");

    let mut child = Command::new(bin_path())
        .arg("--sock")
        .arg(&sock)
        .kill_on_drop(true)
        .spawn()
        .expect("spawn atd-ref-server");

    // Poll for socket file to appear (max ~5 s).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if sock.exists() {
            // Give the listener a tick to be accept()-ready.
            tokio::time::sleep(Duration::from_millis(20)).await;
            return ServerHandle {
                _child: child,
                sock,
                _tempdir: dir,
            };
        }
        // If the child died, surface that.
        if let Ok(Some(status)) = child.try_wait() {
            panic!("server exited before creating socket: status {status:?}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not create socket within 5s at {sock:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_ping_returns_pong() {
    let srv = spawn_server().await;
    let r = send_one_request(&srv.sock, &serde_json::json!({"type": "ping"}))
        .await
        .unwrap();
    assert_eq!(r["type"], "pong");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_tool_list_returns_echo() {
    let srv = spawn_server().await;
    let r = send_one_request(&srv.sock, &serde_json::json!({"type": "tool_list"}))
        .await
        .unwrap();
    assert_eq!(r["type"], "tool_list");
    let tools = r["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["id"], "ref:echo.say");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_tool_schema_returns_full_definition() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({"type": "tool_schema", "tool_id": "ref:echo.say"}),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_schema");
    assert_eq!(r["schema"]["id"], "ref:echo.say");
    assert_eq!(r["schema"]["capability"]["domain"], "echo");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_tool_schema_not_found_returns_error() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({"type": "tool_schema", "tool_id": "ref:missing"}),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "error");
    assert!(r["message"].as_str().unwrap().contains("tool not found"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_run_tool_success_echoes_args() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:echo.say",
            "args": {"hello": "world"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["echoed"]["hello"], "world");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_run_tool_dry_run_returns_preview() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:echo.say",
            "args": {"x": 1},
            "dry_run": true,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["dry_run"], serde_json::json!(true));
    assert_eq!(r["result"]["dry_run"], serde_json::json!(true));
    assert_eq!(r["result"]["args_preview"]["x"], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_multiple_requests_on_one_connection() {
    let srv = spawn_server().await;
    // Open ONE stream, send two requests in sequence, read two responses.
    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    async fn one(
        stream: &mut UnixStream,
        req: serde_json::Value,
    ) -> serde_json::Value {
        let body = serde_json::to_vec(&req).unwrap();
        stream.write_all(&(body.len() as u32).to_be_bytes()).await.unwrap();
        stream.write_all(&body).await.unwrap();
        stream.flush().await.unwrap();

        let mut header = [0u8; 4];
        stream.read_exact(&mut header).await.unwrap();
        let n = u32::from_be_bytes(header) as usize;
        let mut buf = vec![0u8; n];
        stream.read_exact(&mut buf).await.unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    let r1 = one(&mut stream, serde_json::json!({"type": "ping"})).await;
    assert_eq!(r1["type"], "pong");
    let r2 = one(&mut stream, serde_json::json!({"type": "tool_list"})).await;
    assert_eq!(r2["type"], "tool_list");
}
```

- [ ] **Step 11.2: Run the integration tests**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --test integration
```

Expected: `7 passed`.

If the tests hang on `spawn_server`, the binary isn't creating the socket — check `cargo build -p atd-ref-server` succeeds and the bin exists at `target/debug/atd-ref-server`.

- [ ] **Step 11.3: Full workspace regression + commit**

```bash
cargo test --workspace --all-targets
```

Expected: 127 prior + 36 lib + 7 integration = 170 total. Lib breakdown: 3 wire + 6 protocol + 4 error + 4 context + 4 registry + 4 echo + 1 builtin + 10 server = 36.

```bash
git add crates/atd-ref-server/
git commit -m "test(atd-ref-server): add end-to-end integration test spawning binary"
```

---

## Task 12: README + ANOS-free check + manual smoke + tag

**Files:**
- Create: `crates/atd-ref-server/README.md`
- Modify: `/home/nan/proj/atd-mvp/README.md` (root — add atd-ref-server blurb)

The README is part of the deliverable — this is a reference server, not a private binary.

- [ ] **Step 12.1: Write the crate README**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/README.md`:

```markdown
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
   use crate::registry::Tool;

   pub struct MyTool;

   impl Tool for MyTool {
       fn definition(&self) -> &ToolDefinition { /* ... */ }

       async fn call(
           &self,
           args: serde_json::Value,
           ctx: &CallContext,
       ) -> Result<serde_json::Value, ToolCallError> {
           // Your logic here. Return Ok(...) for success,
           // Err(ToolCallError::...) for failure.
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

See `docs/superpowers/specs/2026-04-22-atd-ref-server-sp1-foundation.md` §10.

## License

Apache-2.0 (workspace default).
```

- [ ] **Step 12.2: Add a paragraph to the root README**

Edit `/home/nan/proj/atd-mvp/README.md`. Find the `## Python SDK` section. Immediately after it (before `## Development`), insert:

```markdown
## Reference server

An optional **neutral ATD server** ships at `crates/atd-ref-server/`. Runs standalone on a Unix socket with a built-in tool catalog. Meant as a fork-friendly template for third-party server implementers. No dependency on any specific client or agent framework.

```bash
cargo build --release -p atd-ref-server --bin atd-ref-server
./target/release/atd-ref-server &
atd --sock $HOME/.atd-ref/server.sock list
```

Full reference: [`crates/atd-ref-server/README.md`](crates/atd-ref-server/README.md).
```

- [ ] **Step 12.3: ANOS-free + independence check**

```bash
cd /home/nan/proj/atd-mvp
cargo tree -p atd-ref-server --prefix none \
  | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )' \
  && echo FAIL \
  || echo "OK: ref-server has no client/bridge/cli/anos deps"
```

Expected: `OK: ref-server has no client/bridge/cli/anos deps`.

Note: the space after `atd-client` / `atd-mcp-bridge` / `atd-cli` in the grep pattern prevents false matches against `atd-client` inside `atd-client = "..."` version strings in Cargo.toml (which shouldn't appear in `cargo tree` anyway, but belt-and-suspenders).

- [ ] **Step 12.4: Live smoke against the real binary with atd-cli**

Build the release binary if not already:

```bash
cargo build --release -p atd-ref-server --bin atd-ref-server
cargo build --release -p atd-cli --bin atd
```

Terminal 1:

```bash
./target/release/atd-ref-server --sock /tmp/atd-ref-smoke.sock
```

Terminal 2:

```bash
./target/release/atd --sock /tmp/atd-ref-smoke.sock doctor
./target/release/atd --sock /tmp/atd-ref-smoke.sock list
./target/release/atd --sock /tmp/atd-ref-smoke.sock schema ref:echo.say
./target/release/atd --sock /tmp/atd-ref-smoke.sock call ref:echo.say --args '{"msg":"hi"}'
```

Expected for each:
- `doctor` → `socket exists: true`, `ping: ok`, `tool count: 1`
- `list` → table row for `ref:echo.say`
- `schema` → pretty JSON of the ToolDefinition
- `call` → `ok:` and `{"echoed":{"msg":"hi"}}`

Capture the outputs in your Task 12 report.

Clean up:

```bash
# Terminal 2
pkill -x atd-ref-server || true
rm -f /tmp/atd-ref-smoke.sock
```

- [ ] **Step 12.5: Final workspace regression**

```bash
cargo test --workspace --all-targets
```

Expected: 170 tests passing (127 prior + 43 new: 36 lib + 7 integration).

- [ ] **Step 12.6: Commit + tag**

```bash
git add crates/atd-ref-server/README.md README.md
git commit -m "docs(atd-ref-server): add crate README and link from root"

git tag -a sp1-ref-server-foundation -m "SP-1: atd-ref-server foundation (framework + echo tool)"
git log --oneline | head -15
git tag
```

Expected: `sp1-ref-server-foundation` in tag list, alongside `phase0-week1` / `phase0.5-hermes` / `phase0-weeks2-3` / `phase1-python`.

---

## Post-Plan Verification Checklist

- [ ] `cargo build -p atd-ref-server --release` zero warnings
- [ ] `cargo test -p atd-ref-server` 43 tests pass (36 lib + 7 integration)
- [ ] `cargo test --workspace --all-targets` 170 tests pass
- [ ] `cargo tree -p atd-ref-server | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )'` empty
- [ ] `./target/release/atd-ref-server --help` prints usage
- [ ] `atd doctor` against live ref-server shows 1 tool
- [ ] `atd call ref:echo.say --args '{"m":"x"}'` returns echoed
- [ ] `crates/atd-ref-server/README.md` has all 6 required sections (spec §8)
- [ ] Tag `sp1-ref-server-foundation` created

## What's next after SP-1

- **SP-2:** `ref:fs.read` / `ref:fs.write` / `ref:fs.edit` with `ReadTracker` per-connection state (2-3 days)
- **SP-3:** `ref:shell.exec` + `ref:shell.pwsh` sharing a subprocess-handler module (1-2 days)
- **SP-4:** `ref:fs.glob` + `ref:fs.grep` via `ignore` / `grep` crates (ripgrep's lib, not shell-out) (1-2 days)
- **SP-5:** `ref:web.fetch` with reqwest + html2md + size budget (1 day)
- **SP-6:** cross-crate E2E: atd-client → atd-ref-server, `hello_atd.py` rewired, validation doc, demo video (half day)

Each sub-project gets its own spec → plan → implementation cycle.
