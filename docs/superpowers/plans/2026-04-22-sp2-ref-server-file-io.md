# atd-ref-server SP-2 File I/O Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three real file-I/O tools (`ref:fs.read`, `ref:fs.write`, `ref:fs.edit`) plus a per-connection `ReadTracker` to `atd-ref-server`, taking it from "framework with echo" to "framework with a usable tool catalog."

**Architecture:** New `tracker.rs` module (per-connection HashMap of path → {mtime,size}). `CallContext` gains an `Option<Arc<ReadTracker>>` field (backwards-compatible addition). `server.rs::handle_connection` constructs one tracker per connection and threads it through `dispatch` into each `CallContext`. The three tools live under `src/tools/fs/`, sharing path resolution + line-number formatting + atomic write helpers via `src/tools/fs/shared.rs`. Seven new end-to-end integration tests exercise cross-tool flows (Write→Read→Edit, NOT_READ enforcement, FILE_MODIFIED detection, ambiguous-match handling). A new `examples/rw_cycle.rs` demonstrates the must-read-before-edit contract in a single-connection session.

**Tech Stack:** Rust 2024, MSRV 1.85 · tokio (existing) · std::fs + std::io (file ops) · thiserror (existing) · dev: tempfile (existing). No new dependencies.

**Spec:** `docs/superpowers/specs/2026-04-22-atd-ref-server-sp2-file-io.md`

**Scope boundary:**
- **In:** tracker module, CallContext extension + call-site migration, fs::shared helpers, fs::read / fs::write / fs::edit, server wire-up, builtin registration, 7 integration tests, rw_cycle example, README per-connection-state section.
- **Out (deferred to later SPs):** any non-fs tool (Bash/PowerShell = SP-3; Glob/Grep = SP-4; WebFetch = SP-5), permissions/ownership inspection, symlink-no-follow flag, directory operations, cross-connection locking, capability tokens.

**Prerequisites:**
- `sp1-ref-server-foundation` tag, 135 Rust workspace tests green
- `cargo build --workspace` clean
- atd-cli binary available for final smoke (Phase 0 weeks 2-3 ships it)

**Exit criteria (mirrors spec §9):**
1. `cargo build -p atd-ref-server --release` zero warnings.
2. `cargo test -p atd-ref-server` — ~93 tests (79 lib + 14 integration, up from 43).
3. `cargo test --workspace --all-targets` — ~186 Rust tests (135 prior + 51 new).
4. `cargo tree -p atd-ref-server | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )'` empty.
5. `cargo run -p atd-ref-server --example rw_cycle` runs to completion, prints the Write→Read→Edit cycle with each step's result.
6. `crates/atd-ref-server/README.md` has the new "Per-connection state" section.
7. Git tag `sp2-ref-server-file-io` created.

---

## File Structure

```
crates/atd-ref-server/
├── Cargo.toml                                    (unchanged)
├── README.md                                     (MODIFY — add "Per-connection state" section, Task 10)
├── examples/
│   └── rw_cycle.rs                               (NEW — Task 9)
└── src/
    ├── main.rs                                   (unchanged)
    ├── lib.rs                                    (MODIFY — new module declarations)
    ├── wire.rs                                   (unchanged)
    ├── protocol.rs                               (unchanged)
    ├── error.rs                                  (unchanged)
    ├── context.rs                                (MODIFY — add read_tracker field, Task 2)
    ├── tracker.rs                                (NEW — ReadTracker + ReadTrackerError, Task 1)
    ├── registry.rs                               (unchanged)
    ├── server.rs                                 (MODIFY — thread tracker through dispatch, Task 7)
    ├── builtin.rs                                (MODIFY — register 3 new tools, Task 7)
    └── tools/
        ├── mod.rs                                (MODIFY — export fs submodule)
        ├── echo.rs                               (MODIFY — 1 test gets read_tracker: None, Task 2)
        └── fs/                                   (NEW subtree)
            ├── mod.rs                            (Task 3)
            ├── shared.rs                         (Task 3)
            ├── read.rs                           (Task 4)
            ├── write.rs                          (Task 5)
            └── edit.rs                           (Task 6)
└── tests/
    └── integration.rs                            (MODIFY — add 7 new tests, Task 8)
```

---

## Task 1: `ReadTracker` module

**Files:**
- Create: `crates/atd-ref-server/src/tracker.rs`
- Modify: `crates/atd-ref-server/src/lib.rs`

ReadTracker lives in its own module — tools and server both use it, so it's not coupled to either. Uses `Mutex<HashMap>` (simpler than DashMap; contention is per-connection).

- [ ] **Step 1.1: Write the failing test + implementation**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tracker.rs`:

```rust
//! Per-connection record of which files have been Read + their observed
//! mtime/size at Read time. Edit uses this to enforce "you must Read before
//! Edit, and the file mustn't have changed since then."

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use thiserror::Error;

#[derive(Debug, Clone, Copy)]
struct ReadRecord {
    mtime: SystemTime,
    size: u64,
}

#[derive(Debug, Error)]
pub enum ReadTrackerError {
    #[error("file has not been read in this session: {path}")]
    NotRead { path: PathBuf },

    #[error("file modified since it was read: {path}")]
    Modified { path: PathBuf },
}

pub struct ReadTracker {
    entries: Mutex<HashMap<PathBuf, ReadRecord>>,
}

impl ReadTracker {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Record a successful read. `path` should already be canonicalized by
    /// the caller.
    pub fn record(&self, path: PathBuf, mtime: SystemTime, size: u64) {
        let mut g = self.entries.lock().expect("tracker mutex poisoned");
        g.insert(path, ReadRecord { mtime, size });
    }

    /// Verify that `path` has been read in this session AND its current
    /// mtime + size match what was recorded.
    ///
    /// Caller passes the current stat to avoid racing a syscall inside the
    /// lock. `path` must already be canonicalized.
    pub fn check(
        &self,
        path: &Path,
        current_mtime: SystemTime,
        current_size: u64,
    ) -> Result<(), ReadTrackerError> {
        let g = self.entries.lock().expect("tracker mutex poisoned");
        match g.get(path) {
            None => Err(ReadTrackerError::NotRead { path: path.to_path_buf() }),
            Some(rec) => {
                if rec.mtime != current_mtime || rec.size != current_size {
                    Err(ReadTrackerError::Modified { path: path.to_path_buf() })
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl Default for ReadTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn t(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn check_unrecorded_path_returns_not_read() {
        let tr = ReadTracker::new();
        let err = tr.check(Path::new("/tmp/nope"), t(1), 10).unwrap_err();
        assert!(matches!(err, ReadTrackerError::NotRead { .. }));
    }

    #[test]
    fn record_then_check_same_stat_is_ok() {
        let tr = ReadTracker::new();
        let p = PathBuf::from("/tmp/f");
        tr.record(p.clone(), t(100), 42);
        tr.check(&p, t(100), 42).unwrap();
    }

    #[test]
    fn check_returns_modified_when_mtime_changed() {
        let tr = ReadTracker::new();
        let p = PathBuf::from("/tmp/f");
        tr.record(p.clone(), t(100), 42);
        let err = tr.check(&p, t(200), 42).unwrap_err();
        assert!(matches!(err, ReadTrackerError::Modified { .. }));
    }

    #[test]
    fn check_returns_modified_when_size_changed() {
        let tr = ReadTracker::new();
        let p = PathBuf::from("/tmp/f");
        tr.record(p.clone(), t(100), 42);
        let err = tr.check(&p, t(100), 100).unwrap_err();
        assert!(matches!(err, ReadTrackerError::Modified { .. }));
    }

    #[test]
    fn record_overwrites_prior_entry() {
        let tr = ReadTracker::new();
        let p = PathBuf::from("/tmp/f");
        tr.record(p.clone(), t(100), 42);
        tr.record(p.clone(), t(200), 84);
        // The new record is the one that matches.
        tr.check(&p, t(200), 84).unwrap();
        // The old one doesn't.
        let err = tr.check(&p, t(100), 42).unwrap_err();
        assert!(matches!(err, ReadTrackerError::Modified { .. }));
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
pub mod tracker;
pub mod wire;
```

- [ ] **Step 1.2: Run tests + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib tracker    # 5 passed
cargo test --workspace --all-targets          # 140 Rust tests (135 + 5)
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add ReadTracker per-connection module"
```

---

## Task 2: Extend `CallContext` with `read_tracker` field + fix SP-1 call sites

**Files:**
- Modify: `crates/atd-ref-server/src/context.rs`
- Modify: `crates/atd-ref-server/src/tools/echo.rs` (one test site)
- (server.rs production dispatch is touched in Task 7; leaving it intact as `None` for now via default is NOT possible because struct literal construction must list all fields. Task 7 is the first dispatch change.)

Adds the `Option<Arc<ReadTracker>>` field. Because Rust struct construction requires all fields, every literal `CallContext { ... }` call site in the crate must be updated. Task 2 handles the test call sites; Task 7 handles the production dispatcher.

- [ ] **Step 2.1: Write the extension + update call sites**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/context.rs`:

```rust
//! Per-call context passed to every `Tool::call` invocation.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::tracker::ReadTracker;

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
    /// Shared-per-connection read tracker. `None` in isolated unit tests;
    /// server always attaches one via `Arc::clone` in per-connection state.
    pub read_tracker: Option<Arc<ReadTracker>>,
}

impl CallContext {
    pub fn remaining_time(&self) -> Option<Duration> {
        self.deadline.map(|d| d.saturating_duration_since(Instant::now()))
    }
}

#[cfg(any(test, feature = "testing"))]
impl CallContext {
    /// Construct a sensible default for unit tests. cwd = current dir,
    /// 1 MiB output budget, fresh call_id, no deadline, no tracker.
    pub fn for_test() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            call_id: ulid::Ulid::new(),
            deadline: None,
            read_tracker: None,
        }
    }

    /// Test-only: build a CallContext with a fresh tracker attached.
    /// Returns both so tests can also record/check on the same tracker.
    pub fn for_test_with_tracker() -> (Self, Arc<ReadTracker>) {
        let tracker = Arc::new(ReadTracker::new());
        let ctx = Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            call_id: ulid::Ulid::new(),
            deadline: None,
            read_tracker: Some(tracker.clone()),
        };
        (ctx, tracker)
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
        assert!(ctx.read_tracker.is_none());
    }

    #[test]
    fn for_test_with_tracker_shares_arc() {
        let (ctx, tracker) = CallContext::for_test_with_tracker();
        assert!(ctx.read_tracker.is_some());
        let ctx_tracker = ctx.read_tracker.as_ref().unwrap();
        assert!(Arc::ptr_eq(ctx_tracker, &tracker));
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
            read_tracker: None,
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
            read_tracker: None,
        };
        assert_eq!(ctx.remaining_time().unwrap(), Duration::ZERO);
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/echo.rs` — find the `oversized_args_return_truncation_marker` test which currently does `CallContext { cwd, max_output_bytes, call_id, deadline }` and add `read_tracker: None` to that literal. Replace the test body with:

```rust
    #[tokio::test]
    async fn oversized_args_return_truncation_marker() {
        let t = EchoTool::new();
        // Tiny budget so even a small payload overflows.
        let ctx = CallContext {
            cwd: std::path::PathBuf::from("."),
            max_output_bytes: 32,
            call_id: ulid::Ulid::new(),
            deadline: None,
            read_tracker: None,
        };
        let big = "x".repeat(1_000);
        let args = serde_json::json!({"big": big});
        let r = t.call(args, &ctx).await.unwrap();
        assert_eq!(r["truncated"], serde_json::json!(true));
        assert!(r["original_bytes"].as_u64().unwrap() > 32);
        assert!(r.get("echoed").is_none());
    }
```

- [ ] **Step 2.2: Temporarily stub the production dispatch site**

`src/server.rs::dispatch` currently builds a `CallContext` literal without `read_tracker`. Compilation will break. Add `read_tracker: None,` to that literal — Task 7 will replace `None` with the real per-connection Arc.

Find in `src/server.rs` (inside the `Request::RunTool { ... dry_run: false } =>` branch):

```rust
            let ctx = CallContext {
                cwd: state.config.cwd.clone(),
                max_output_bytes: state.config.max_output_bytes,
                call_id: ulid::Ulid::new(),
                deadline: Some(
                    Instant::now() + Duration::from_millis(state.config.default_call_timeout_ms),
                ),
            };
```

Change to:

```rust
            let ctx = CallContext {
                cwd: state.config.cwd.clone(),
                max_output_bytes: state.config.max_output_bytes,
                call_id: ulid::Ulid::new(),
                deadline: Some(
                    Instant::now() + Duration::from_millis(state.config.default_call_timeout_ms),
                ),
                read_tracker: None,
            };
```

- [ ] **Step 2.3: Run tests**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib context     # 5 tests (4 + 1 new for_test_with_tracker)
cargo test -p atd-ref-server                    # full lib: 41 tests
cargo test --workspace --all-targets            # 141 (140 + 1)
```

Expected: all green. The echo truncation test still passes (just needed the new field).

- [ ] **Step 2.4: Commit**

```bash
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add read_tracker field to CallContext"
```

---

## Task 3: `tools/fs/shared.rs` + fs module scaffold

**Files:**
- Create: `crates/atd-ref-server/src/tools/fs/mod.rs`
- Create: `crates/atd-ref-server/src/tools/fs/shared.rs`
- Modify: `crates/atd-ref-server/src/tools/mod.rs`

Three helpers shared by Read / Write / Edit:
- `resolve_path(cwd, input)` — absolute-or-relative → PathBuf (no canonicalize; canonicalize happens at a known point after existence is established)
- `format_with_line_numbers(text, offset, limit, max_bytes)` — produces Read's output string with truncation info
- `atomic_write(path, bytes)` — tempfile in same dir + rename

- [ ] **Step 3.1: Create scaffold**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/mod.rs`:

```rust
//! File-I/O tools: ref:fs.read, ref:fs.write, ref:fs.edit.

pub mod shared;
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/mod.rs`:

```rust
//! Built-in tools. SP-1 ships the echo test-anchor; SP-2 adds file I/O.

pub mod echo;
pub mod fs;
```

- [ ] **Step 3.2: Write the failing tests + impl for `shared.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/shared.rs`:

```rust
//! Shared helpers for the fs toolset.

use std::path::{Path, PathBuf};

/// Resolve an input string as a filesystem path. Absolute paths are returned
/// as-is; relative paths are joined with `cwd`. No canonicalization here —
/// the caller does that at the right moment (after existence is known).
pub fn resolve_path(cwd: &Path, input: &str) -> PathBuf {
    let p = Path::new(input);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

/// Output of line-numbered formatting.
pub struct LineFormatResult {
    pub content: String,
    pub lines_shown: usize,
    pub total_lines: usize,
    pub truncated: bool,
}

/// Format `text` with `"   N\tline\n"` prefixes (N right-padded to 4 chars min).
/// Honors optional 1-indexed `offset` (skip offset-1 leading lines) and `limit`.
/// If appending a line would push output beyond `max_output_bytes`, stop at
/// the current line boundary and set `truncated=true`.
pub fn format_with_line_numbers(
    text: &str,
    offset: usize,
    limit: Option<usize>,
    max_output_bytes: usize,
) -> LineFormatResult {
    let lines: Vec<&str> = text.split('\n').collect();
    // If text ends with \n, the split produces a trailing empty string we
    // should not count as a "line."
    let total_lines = if text.is_empty() {
        0
    } else if text.ends_with('\n') {
        lines.len().saturating_sub(1)
    } else {
        lines.len()
    };

    let start = offset.saturating_sub(1); // 0-indexed
    let iter = lines
        .iter()
        .take(if text.ends_with('\n') {
            lines.len().saturating_sub(1)
        } else {
            lines.len()
        })
        .enumerate()
        .skip(start);

    let mut out = String::new();
    let mut lines_shown = 0usize;
    let mut truncated = false;
    for (zero_idx, line) in iter {
        if let Some(lim) = limit {
            if lines_shown >= lim {
                break;
            }
        }
        let n = zero_idx + 1;
        let prefix = format!("{:>4}\t", n);
        let line_bytes = prefix.len() + line.len() + 1; // + '\n'
        if out.len() + line_bytes > max_output_bytes {
            truncated = true;
            break;
        }
        out.push_str(&prefix);
        out.push_str(line);
        out.push('\n');
        lines_shown += 1;
    }

    LineFormatResult {
        content: out,
        lines_shown,
        total_lines,
        truncated,
    }
}

/// Result of an atomic write.
pub struct AtomicWriteResult {
    pub bytes_written: usize,
    pub created: bool,
}

/// Atomic write: create a tempfile in `path`'s parent, write bytes, rename
/// over `path`. Caller is responsible for parent-directory existence checks
/// — this function surfaces the underlying `std::io::Error` if the parent
/// doesn't exist.
pub async fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<AtomicWriteResult> {
    use std::io::ErrorKind;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(ErrorKind::InvalidInput, "path has no parent")
    })?;

    let created = !path.exists();
    let tmp_name = format!(".atd-ref-write-{}.tmp", ulid::Ulid::new());
    let tmp = parent.join(tmp_name);

    tokio::fs::write(&tmp, bytes).await?;
    match tokio::fs::rename(&tmp, path).await {
        Ok(()) => Ok(AtomicWriteResult {
            bytes_written: bytes.len(),
            created,
        }),
        Err(e) => {
            // Clean up the tempfile on rename failure.
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_absolute_path_unchanged() {
        let cwd = Path::new("/home/u");
        assert_eq!(resolve_path(cwd, "/etc/hostname"), PathBuf::from("/etc/hostname"));
    }

    #[test]
    fn resolve_relative_path_joined_to_cwd() {
        let cwd = Path::new("/home/u");
        assert_eq!(resolve_path(cwd, "proj/foo.txt"), PathBuf::from("/home/u/proj/foo.txt"));
    }

    #[test]
    fn format_with_line_numbers_basic() {
        let r = format_with_line_numbers("a\nb\nc\n", 1, None, 1_000_000);
        assert_eq!(r.content, "   1\ta\n   2\tb\n   3\tc\n");
        assert_eq!(r.lines_shown, 3);
        assert_eq!(r.total_lines, 3);
        assert!(!r.truncated);
    }

    #[test]
    fn format_with_line_numbers_no_trailing_newline() {
        // "a\nb" — 2 lines, no trailing newline
        let r = format_with_line_numbers("a\nb", 1, None, 1_000_000);
        assert_eq!(r.total_lines, 2);
        assert_eq!(r.lines_shown, 2);
        assert!(r.content.contains("   2\tb"));
    }

    #[test]
    fn format_with_line_numbers_offset() {
        let r = format_with_line_numbers("a\nb\nc\nd\n", 3, None, 1_000_000);
        assert_eq!(r.content, "   3\tc\n   4\td\n");
        assert_eq!(r.lines_shown, 2);
        assert_eq!(r.total_lines, 4);
    }

    #[test]
    fn format_with_line_numbers_limit() {
        let r = format_with_line_numbers("a\nb\nc\nd\n", 1, Some(2), 1_000_000);
        assert_eq!(r.content, "   1\ta\n   2\tb\n");
        assert_eq!(r.lines_shown, 2);
    }

    #[test]
    fn format_with_line_numbers_truncation_at_byte_budget() {
        // Make budget tiny so 2nd line won't fit.
        let r = format_with_line_numbers("xxxxx\nyyyyy\n", 1, None, 12);
        assert!(r.truncated);
        // Only the first line made it.
        assert_eq!(r.lines_shown, 1);
        assert!(r.content.starts_with("   1\t"));
    }

    #[test]
    fn format_with_line_numbers_offset_beyond_total_returns_empty() {
        let r = format_with_line_numbers("a\nb\n", 10, None, 1_000_000);
        assert_eq!(r.content, "");
        assert_eq!(r.lines_shown, 0);
        assert_eq!(r.total_lines, 2);
    }

    #[tokio::test]
    async fn atomic_write_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let r = atomic_write(&path, b"hello").await.unwrap();
        assert_eq!(r.bytes_written, 5);
        assert!(r.created);
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn atomic_write_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, b"old").unwrap();
        let r = atomic_write(&path, b"new!").await.unwrap();
        assert_eq!(r.bytes_written, 4);
        assert!(!r.created);
        assert_eq!(std::fs::read(&path).unwrap(), b"new!");
    }

    #[tokio::test]
    async fn atomic_write_fails_when_parent_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_such_dir").join("f.txt");
        let err = atomic_write(&path, b"x").await.unwrap_err();
        assert!(err.kind() == std::io::ErrorKind::NotFound || err.kind() == std::io::ErrorKind::InvalidInput);
    }
}
```

- [ ] **Step 3.3: Run + commit**

```bash
cargo test -p atd-ref-server --lib tools::fs::shared    # 10 passed
cargo test --workspace --all-targets                     # 151 Rust tests
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add fs::shared helpers (path, format, atomic_write)"
```

---

## Task 4: `ref:fs.read`

**Files:**
- Create: `crates/atd-ref-server/src/tools/fs/read.rs`
- Modify: `crates/atd-ref-server/src/tools/fs/mod.rs`

Real Read tool: resolve → canonicalize → read bytes → UTF-8 decode → line-number format → record in tracker.

- [ ] **Step 4.1: Write the failing tests + impl**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/read.rs`:

```rust
//! `ref:fs.read` — read a UTF-8 file with line numbers.

use std::sync::OnceLock;

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};
use crate::tools::fs::shared::{format_with_line_numbers, resolve_path};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.read".into(),
        name: "Read File".into(),
        description: "Read a UTF-8 text file with 1-indexed line numbers. Supports offset/limit and honors ctx.max_output_bytes via byte-budget truncation at line boundaries.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["read".into()],
            tags: vec!["file".into(), "filesystem".into(), "read".into()],
            intent_examples: vec![
                "read /etc/hostname".into(),
                "show me the file at src/main.rs".into(),
            ],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":   { "type": "string", "minLength": 1 },
                "offset": { "type": "integer", "minimum": 1 },
                "limit":  { "type": "integer", "minimum": 1 }
            },
            "required": ["path"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":        { "type": "string" },
                "content":     { "type": "string" },
                "line_count":  { "type": "integer" },
                "total_lines": { "type": "integer" },
                "truncated":   { "type": "boolean" }
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
            data_sensitivity: Some("file contents".into()),
        },
        resources: ToolResources {
            timeout_ms: 10_000,
            max_concurrent: 50,
            rate_limit_per_min: None,
            estimated_tokens: Some(500),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Read,
    })
}

pub struct FsReadTool;

impl FsReadTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct ReadArgs {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

impl Tool for FsReadTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move {
            let args: ReadArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if matches!(args.offset, Some(0)) || matches!(args.limit, Some(0)) {
                return Err(ToolCallError::InvalidArgs(
                    "offset/limit must be >= 1".into(),
                ));
            }

            let resolved = resolve_path(&ctx.cwd, &args.path);
            let canonical = match tokio::fs::canonicalize(&resolved).await {
                Ok(p) => p,
                Err(e) => return Err(io_to_tool_err(&resolved, e)),
            };

            let meta = match tokio::fs::metadata(&canonical).await {
                Ok(m) => m,
                Err(e) => return Err(io_to_tool_err(&canonical, e)),
            };
            if meta.is_dir() {
                return Err(ToolCallError::ExecutionFailed {
                    code: "IS_DIR".into(),
                    message: format!("path is a directory: {}", canonical.display()),
                    retryable: false,
                });
            }
            let size = meta.len();
            let mtime = meta.modified().map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("mtime: {e}"),
                retryable: true,
            })?;

            let bytes = match tokio::fs::read(&canonical).await {
                Ok(b) => b,
                Err(e) => return Err(io_to_tool_err(&canonical, e)),
            };
            let text = match std::str::from_utf8(&bytes) {
                Ok(s) => s.to_string(),
                Err(e) => {
                    return Err(ToolCallError::ExecutionFailed {
                        code: "ENCODING".into(),
                        message: format!("not valid UTF-8 at byte {}", e.valid_up_to()),
                        retryable: false,
                    });
                }
            };

            let offset = args.offset.unwrap_or(1);
            let formatted = format_with_line_numbers(&text, offset, args.limit, ctx.max_output_bytes);

            // Record in tracker (if any).
            if let Some(tracker) = &ctx.read_tracker {
                tracker.record(canonical.clone(), mtime, size);
            }

            Ok(serde_json::json!({
                "path": canonical.to_string_lossy(),
                "content": formatted.content,
                "line_count": formatted.lines_shown,
                "total_lines": formatted.total_lines,
                "truncated": formatted.truncated,
            }))
        })
    }
}

fn io_to_tool_err(path: &std::path::Path, e: std::io::Error) -> ToolCallError {
    use std::io::ErrorKind;
    let (code, retryable) = match e.kind() {
        ErrorKind::NotFound => ("NOT_FOUND", false),
        ErrorKind::PermissionDenied => ("EACCES", false),
        _ => ("IO", true),
    };
    ToolCallError::ExecutionFailed {
        code: code.into(),
        message: format!("{}: {}", path.display(), e),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn write_tmp(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[tokio::test]
    async fn read_happy_path() {
        let (_dir, path) = write_tmp("hello\nworld\n").await;
        let t = FsReadTool::new();
        let (ctx, _tr) = CallContext::for_test_with_tracker();
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["line_count"], 2);
        assert_eq!(r["total_lines"], 2);
        assert!(r["content"].as_str().unwrap().contains("   1\thello"));
        assert!(r["content"].as_str().unwrap().contains("   2\tworld"));
        assert_eq!(r["truncated"], serde_json::json!(false));
    }

    #[tokio::test]
    async fn read_with_offset_skips_leading_lines() {
        let (_dir, path) = write_tmp("a\nb\nc\nd\n").await;
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy(), "offset": 3}),
                &ctx,
            )
            .await
            .unwrap();
        let content = r["content"].as_str().unwrap();
        assert!(!content.contains("   1\ta"));
        assert!(content.contains("   3\tc"));
        assert!(content.contains("   4\td"));
    }

    #[tokio::test]
    async fn read_with_limit_caps_lines() {
        let (_dir, path) = write_tmp("a\nb\nc\nd\n").await;
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy(), "limit": 2}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["line_count"], 2);
        assert_eq!(r["total_lines"], 4);
    }

    #[tokio::test]
    async fn read_with_offset_and_limit() {
        let (_dir, path) = write_tmp("a\nb\nc\nd\n").await;
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy(), "offset": 2, "limit": 2}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["line_count"], 2);
        let content = r["content"].as_str().unwrap();
        assert!(content.contains("   2\tb"));
        assert!(content.contains("   3\tc"));
        assert!(!content.contains("   1\ta"));
        assert!(!content.contains("   4\td"));
    }

    #[tokio::test]
    async fn read_nonexistent_returns_not_found() {
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"path": "/tmp/atd-ref-does-not-exist-xxxxx"}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "NOT_FOUND");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn read_directory_returns_is_dir() {
        let dir = tempfile::tempdir().unwrap();
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"path": dir.path().to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "IS_DIR");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn read_non_utf8_returns_encoding_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bin.dat");
        std::fs::write(&path, &[0xff, 0xfe, 0xfd]).unwrap();
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"path": path.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "ENCODING");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn read_offset_zero_is_invalid_args() {
        let (_dir, path) = write_tmp("x\n").await;
        let t = FsReadTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"path": path.to_string_lossy(), "offset": 0}),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }

    #[tokio::test]
    async fn read_records_in_tracker() {
        let (_dir, path) = write_tmp("one\n").await;
        let t = FsReadTool::new();
        let (ctx, tr) = CallContext::for_test_with_tracker();
        t.call(
            serde_json::json!({"path": path.to_string_lossy()}),
            &ctx,
        )
        .await
        .unwrap();
        // After Read, tracker.check with current stat should succeed.
        let canonical = tokio::fs::canonicalize(&path).await.unwrap();
        let meta = tokio::fs::metadata(&canonical).await.unwrap();
        tr.check(&canonical, meta.modified().unwrap(), meta.len())
            .unwrap();
    }

    #[tokio::test]
    async fn read_truncates_when_over_max_output_bytes() {
        let big = "x".repeat(200);
        let (_dir, path) = write_tmp(&format!("{big}\n{big}\n")).await;
        let t = FsReadTool::new();
        // Budget tiny so second line can't fit.
        let mut ctx = CallContext::for_test();
        ctx.max_output_bytes = 220;
        let r = t
            .call(
                serde_json::json!({"path": path.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["truncated"], serde_json::json!(true));
        assert!(r["line_count"].as_u64().unwrap() < r["total_lines"].as_u64().unwrap());
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/mod.rs`:

```rust
//! File-I/O tools: ref:fs.read, ref:fs.write, ref:fs.edit.

pub mod read;
pub mod shared;
```

- [ ] **Step 4.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib tools::fs::read      # 10 passed
cargo test --workspace --all-targets                     # 161 Rust tests
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add ref:fs.read tool"
```

---

## Task 5: `ref:fs.write`

**Files:**
- Create: `crates/atd-ref-server/src/tools/fs/write.rs`
- Modify: `crates/atd-ref-server/src/tools/fs/mod.rs`

Atomic file write using the shared helper.

- [ ] **Step 5.1: Write tests + impl**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/write.rs`:

```rust
//! `ref:fs.write` — atomic write of a UTF-8 file.

use std::sync::OnceLock;

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};
use crate::tools::fs::shared::{atomic_write, resolve_path};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.write".into(),
        name: "Write File".into(),
        description: "Atomically write text content to a file (tempfile + rename). Parent directory must already exist.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["write".into()],
            tags: vec!["file".into(), "filesystem".into(), "write".into()],
            intent_examples: vec!["write config.toml".into()],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":    { "type": "string", "minLength": 1 },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":          { "type": "string" },
                "bytes_written": { "type": "integer" },
                "created":       { "type": "boolean" }
            }
        }),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Write,
            dry_run: true,
            side_effects: vec!["filesystem".into()],
            data_sensitivity: Some("file contents".into()),
        },
        resources: ToolResources {
            timeout_ms: 10_000,
            max_concurrent: 20,
            rate_limit_per_min: None,
            estimated_tokens: Some(200),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Write,
    })
}

pub struct FsWriteTool;

impl FsWriteTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
}

impl Tool for FsWriteTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move {
            let args: WriteArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;

            let resolved = resolve_path(&ctx.cwd, &args.path);
            let parent = resolved.parent().ok_or_else(|| ToolCallError::ExecutionFailed {
                code: "NO_PARENT".into(),
                message: format!("path has no parent: {}", resolved.display()),
                retryable: false,
            })?;
            if !parent.exists() {
                return Err(ToolCallError::ExecutionFailed {
                    code: "NO_PARENT".into(),
                    message: format!("parent directory does not exist: {}", parent.display()),
                    retryable: false,
                });
            }

            let result = atomic_write(&resolved, args.content.as_bytes())
                .await
                .map_err(|e| {
                    use std::io::ErrorKind;
                    let (code, retryable) = match e.kind() {
                        ErrorKind::PermissionDenied => ("EACCES", false),
                        _ => ("IO", true),
                    };
                    ToolCallError::ExecutionFailed {
                        code: code.into(),
                        message: format!("{}: {e}", resolved.display()),
                        retryable,
                    }
                })?;

            // Canonicalize AFTER the write so the result's `path` is stable.
            let canonical = tokio::fs::canonicalize(&resolved).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("canonicalize after write: {e}"),
                    retryable: true,
                }
            })?;

            Ok(serde_json::json!({
                "path": canonical.to_string_lossy(),
                "bytes_written": result.bytes_written,
                "created": result.created,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let t = FsWriteTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": "hello world"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["bytes_written"], 11);
        assert_eq!(r["created"], serde_json::json!(true));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn write_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.txt");
        std::fs::write(&path, "old").unwrap();
        let t = FsWriteTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": "new content"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["created"], serde_json::json!(false));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new content");
    }

    #[tokio::test]
    async fn write_fails_when_parent_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("no_such_dir").join("f.txt");
        let t = FsWriteTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": "x"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "NO_PARENT"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn write_bytes_written_matches_content_len() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("size.txt");
        let t = FsWriteTool::new();
        let ctx = CallContext::for_test();
        let content = "héllo"; // 6 UTF-8 bytes
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "content": content
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["bytes_written"], 6);
    }

    #[tokio::test]
    async fn write_does_not_record_in_tracker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.txt");
        let t = FsWriteTool::new();
        let (ctx, tr) = CallContext::for_test_with_tracker();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "content": "x"
            }),
            &ctx,
        )
        .await
        .unwrap();
        // Tracker should NOT have recorded anything — Write doesn't satisfy
        // the "read before edit" contract.
        let canonical = tokio::fs::canonicalize(&path).await.unwrap();
        let meta = tokio::fs::metadata(&canonical).await.unwrap();
        let err = tr
            .check(&canonical, meta.modified().unwrap(), meta.len())
            .unwrap_err();
        assert!(matches!(
            err,
            crate::tracker::ReadTrackerError::NotRead { .. }
        ));
    }

    #[tokio::test]
    async fn write_returns_canonical_path() {
        let dir = tempfile::tempdir().unwrap();
        // Path via cwd resolution
        let cwd = dir.path().to_path_buf();
        let t = FsWriteTool::new();
        let mut ctx = CallContext::for_test();
        ctx.cwd = cwd.clone();
        let r = t
            .call(
                serde_json::json!({
                    "path": "rel.txt",
                    "content": "rel"
                }),
                &ctx,
            )
            .await
            .unwrap();
        // The returned path should be canonical (absolute, no components).
        let ret = r["path"].as_str().unwrap();
        assert!(std::path::Path::new(ret).is_absolute());
        assert!(ret.ends_with("rel.txt"));
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/mod.rs`:

```rust
//! File-I/O tools: ref:fs.read, ref:fs.write, ref:fs.edit.

pub mod read;
pub mod shared;
pub mod write;
```

- [ ] **Step 5.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib tools::fs::write    # 6 passed
cargo test --workspace --all-targets                    # 167 Rust tests
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add ref:fs.write tool"
```

---

## Task 6: `ref:fs.edit`

**Files:**
- Create: `crates/atd-ref-server/src/tools/fs/edit.rs`
- Modify: `crates/atd-ref-server/src/tools/fs/mod.rs`

Exact-string find-and-replace with must-read-before-edit invariant.

- [ ] **Step 6.1: Write tests + impl**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/edit.rs`:

```rust
//! `ref:fs.edit` — exact-string find-and-replace with must-read-first invariant.

use std::sync::OnceLock;

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};
use crate::tools::fs::shared::{atomic_write, resolve_path};
use crate::tracker::ReadTrackerError;

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.edit".into(),
        name: "Edit File".into(),
        description: "Exact-string find-and-replace in a UTF-8 file. Requires the file to have been Read in this session and unchanged since. Ambiguous (multi-match) edits without replace_all=true are rejected.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["edit".into()],
            tags: vec!["file".into(), "filesystem".into(), "edit".into()],
            intent_examples: vec!["change 'old_name' to 'new_name' in main.rs".into()],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":        { "type": "string", "minLength": 1 },
                "old_string":  { "type": "string", "minLength": 1 },
                "new_string":  { "type": "string" },
                "replace_all": { "type": "boolean", "default": false }
            },
            "required": ["path", "old_string", "new_string"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path":          { "type": "string" },
                "replacements":  { "type": "integer" },
                "bytes_written": { "type": "integer" }
            }
        }),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Write,
            dry_run: true,
            side_effects: vec!["filesystem".into()],
            data_sensitivity: Some("file contents".into()),
        },
        resources: ToolResources {
            timeout_ms: 10_000,
            max_concurrent: 20,
            rate_limit_per_min: None,
            estimated_tokens: Some(300),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Write,
    })
}

pub struct FsEditTool;

impl FsEditTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsEditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct EditArgs {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

impl Tool for FsEditTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async move {
            let args: EditArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;

            let tracker = ctx.read_tracker.as_ref().ok_or_else(|| {
                ToolCallError::InternalError(
                    "server did not attach a read_tracker to CallContext".into(),
                )
            })?;

            let resolved = resolve_path(&ctx.cwd, &args.path);
            let canonical = tokio::fs::canonicalize(&resolved).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: match e.kind() {
                        std::io::ErrorKind::NotFound => "NOT_FOUND",
                        _ => "IO",
                    }
                    .into(),
                    message: format!("{}: {e}", resolved.display()),
                    retryable: matches!(e.kind(), std::io::ErrorKind::Interrupted),
                }
            })?;

            let meta = tokio::fs::metadata(&canonical).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("metadata: {e}"),
                    retryable: true,
                }
            })?;
            let size = meta.len();
            let mtime = meta.modified().map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("mtime: {e}"),
                retryable: true,
            })?;

            // Must-read-before-edit + unchanged-since-read checks.
            match tracker.check(&canonical, mtime, size) {
                Ok(()) => {}
                Err(ReadTrackerError::NotRead { .. }) => {
                    return Err(ToolCallError::ExecutionFailed {
                        code: "NOT_READ".into(),
                        message: format!(
                            "call ref:fs.read on {} first",
                            canonical.display()
                        ),
                        retryable: false,
                    });
                }
                Err(ReadTrackerError::Modified { .. }) => {
                    return Err(ToolCallError::ExecutionFailed {
                        code: "FILE_MODIFIED".into(),
                        message: format!(
                            "file {} changed since it was read; call ref:fs.read again",
                            canonical.display()
                        ),
                        retryable: false,
                    });
                }
            }

            // Read current contents.
            let bytes = tokio::fs::read(&canonical).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("read: {e}"),
                    retryable: true,
                }
            })?;
            let text = std::str::from_utf8(&bytes).map_err(|e| ToolCallError::ExecutionFailed {
                code: "ENCODING".into(),
                message: format!("not valid UTF-8 at byte {}", e.valid_up_to()),
                retryable: false,
            })?;

            // Count matches.
            let match_count = text.matches(&args.old_string).count();
            if match_count == 0 {
                return Err(ToolCallError::InvalidArgs(
                    "old_string not found in file".into(),
                ));
            }
            if match_count >= 2 && !args.replace_all {
                return Err(ToolCallError::InvalidArgs(format!(
                    "{match_count} occurrences of old_string; supply more context or set replace_all=true"
                )));
            }

            // Replace.
            let new_text = if args.replace_all {
                text.replace(&args.old_string, &args.new_string)
            } else {
                // exactly one match
                text.replacen(&args.old_string, &args.new_string, 1)
            };

            // Atomic write.
            let wr = atomic_write(&canonical, new_text.as_bytes())
                .await
                .map_err(|e| ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("write: {e}"),
                    retryable: true,
                })?;

            // Re-record the post-write state so immediate subsequent Edits on
            // the same file don't hit FILE_MODIFIED.
            let new_meta = tokio::fs::metadata(&canonical).await.map_err(|e| {
                ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("post-write metadata: {e}"),
                    retryable: true,
                }
            })?;
            let new_mtime = new_meta.modified().map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("post-write mtime: {e}"),
                retryable: true,
            })?;
            tracker.record(canonical.clone(), new_mtime, new_meta.len());

            Ok(serde_json::json!({
                "path": canonical.to_string_lossy(),
                "replacements": match_count,
                "bytes_written": wr.bytes_written,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn write_tmp(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f.txt");
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    /// Build a CallContext with a tracker pre-populated for `path` using its
    /// current on-disk stat — simulates "Read already happened".
    async fn ctx_with_read(path: &std::path::Path) -> (CallContext, std::sync::Arc<crate::tracker::ReadTracker>) {
        let (ctx, tr) = CallContext::for_test_with_tracker();
        let canonical = tokio::fs::canonicalize(path).await.unwrap();
        let meta = tokio::fs::metadata(&canonical).await.unwrap();
        tr.record(canonical, meta.modified().unwrap(), meta.len());
        (ctx, tr)
    }

    #[tokio::test]
    async fn edit_single_match_replaces() {
        let (_dir, path) = write_tmp("hello world\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "hello",
                    "new_string": "HI"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["replacements"], 1);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "HI world\n");
    }

    #[tokio::test]
    async fn edit_without_prior_read_returns_not_read() {
        let (_dir, path) = write_tmp("hello\n").await;
        // Tracker empty — no Read was recorded.
        let (ctx, _tr) = CallContext::for_test_with_tracker();
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "hello",
                    "new_string": "hi"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "NOT_READ"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn edit_multi_match_without_replace_all_is_invalid_args() {
        let (_dir, path) = write_tmp("foo foo foo\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "foo",
                    "new_string": "bar"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::InvalidArgs(msg) => {
                assert!(msg.contains("3"));
                assert!(msg.contains("replace_all"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn edit_multi_match_with_replace_all_succeeds() {
        let (_dir, path) = write_tmp("foo foo foo\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "foo",
                    "new_string": "bar",
                    "replace_all": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["replacements"], 3);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "bar bar bar\n");
    }

    #[tokio::test]
    async fn edit_zero_match_is_invalid_args() {
        let (_dir, path) = write_tmp("hello\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "nope",
                    "new_string": "x"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }

    #[tokio::test]
    async fn edit_detects_external_modification_after_read() {
        let (_dir, path) = write_tmp("hello\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        // Simulate external change after Read: overwrite + sleep briefly so
        // mtime moves (filesystems with 1s resolution need the sleep).
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        std::fs::write(&path, "externally changed\n").unwrap();
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "externally",
                    "new_string": "xxx"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "FILE_MODIFIED"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn edit_non_utf8_returns_encoding_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bin.dat");
        std::fs::write(&path, &[0xff, 0xfe, 0xfd]).unwrap();
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "x",
                    "new_string": "y"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => assert_eq!(code, "ENCODING"),
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn edit_re_records_so_second_edit_works() {
        let (_dir, path) = write_tmp("aaa bbb\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "aaa",
                "new_string": "AAA"
            }),
            &ctx,
        )
        .await
        .unwrap();
        // Without re-recording, this second edit would see FILE_MODIFIED.
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "bbb",
                "new_string": "BBB"
            }),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "AAA BBB\n");
    }

    #[tokio::test]
    async fn edit_without_tracker_attached_is_internal_error() {
        let (_dir, path) = write_tmp("hello\n").await;
        let ctx = CallContext::for_test(); // no tracker
        let t = FsEditTool::new();
        let err = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "hello",
                    "new_string": "hi"
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InternalError(_)));
    }

    #[tokio::test]
    async fn edit_bytes_written_matches_new_content() {
        let (_dir, path) = write_tmp("abcdef\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        let r = t
            .call(
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "old_string": "abc",
                    "new_string": "ABCDEF"
                }),
                &ctx,
            )
            .await
            .unwrap();
        // New content: "ABCDEFdef\n" = 10 bytes
        assert_eq!(r["bytes_written"], 10);
    }

    #[tokio::test]
    async fn edit_at_start_of_file_works() {
        let (_dir, path) = write_tmp("start middle end\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "start",
                "new_string": "BEGIN"
            }),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "BEGIN middle end\n");
    }

    #[tokio::test]
    async fn edit_at_end_of_file_works() {
        let (_dir, path) = write_tmp("start middle end").await; // no trailing newline
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "end",
                "new_string": "FINISH"
            }),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "start middle FINISH");
    }

    #[tokio::test]
    async fn edit_with_empty_new_string_deletes() {
        let (_dir, path) = write_tmp("keep_me_remove_me\n").await;
        let (ctx, _tr) = ctx_with_read(&path).await;
        let t = FsEditTool::new();
        t.call(
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "_remove_me",
                "new_string": ""
            }),
            &ctx,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep_me\n");
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/mod.rs`:

```rust
//! File-I/O tools: ref:fs.read, ref:fs.write, ref:fs.edit.

pub mod edit;
pub mod read;
pub mod shared;
pub mod write;
```

- [ ] **Step 6.2: Run + commit**

```bash
cargo test -p atd-ref-server --lib tools::fs::edit     # 12 passed
cargo test --workspace --all-targets                    # 179 Rust tests
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): add ref:fs.edit with must-read-first invariant"
```

---

## Task 7: Wire tools through server + register in builtin

**Files:**
- Modify: `crates/atd-ref-server/src/server.rs` (thread tracker into dispatch)
- Modify: `crates/atd-ref-server/src/builtin.rs` (register 3 new tools)

Make the tools reachable over the wire.

- [ ] **Step 7.1: Update `server.rs` to build + thread the tracker**

Replace the body of `handle_connection` and `dispatch` in `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/server.rs` with:

```rust
async fn handle_connection(state: Arc<ServerState>, stream: UnixStream) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let tracker = Arc::new(crate::tracker::ReadTracker::new());  // per-connection
    loop {
        let req: Request = match read_frame(&mut reader).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let resp = dispatch(&state, &tracker, req).await;
        write_frame(&mut writer, &resp).await?;
    }
}

pub(crate) async fn dispatch(
    state: &Arc<ServerState>,
    tracker: &Arc<crate::tracker::ReadTracker>,
    req: Request,
) -> Response {
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
                read_tracker: Some(tracker.clone()),   // NEW
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
```

Update the existing server.rs `mod tests` — every call to `dispatch(&s, ...)` needs a tracker arg. Add a helper at top of `mod tests`:

```rust
    fn dispatch_with_fresh_tracker<'a>(
        s: &'a Arc<ServerState>,
        req: Request,
    ) -> impl std::future::Future<Output = Response> + 'a {
        let tr = Arc::new(crate::tracker::ReadTracker::new());
        async move { dispatch(s, &tr, req).await }
    }
```

Then replace every `dispatch(&s, req).await` in existing tests with `dispatch_with_fresh_tracker(&s, req).await`. All 10 existing server tests need this rewrite. Build them by hand — the body of each test changes only in the dispatch call.

- [ ] **Step 7.2: Register the 3 fs tools in `builtin.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/builtin.rs`:

```rust
//! Built-in tool registration for `atd-ref-server`.
//!
//! To add a new tool:
//! 1. Create `src/tools/<name>.rs` implementing `Tool`.
//! 2. Export it from `tools/mod.rs` (and `tools/fs/mod.rs` for fs tools).
//! 3. Add `reg.register(Arc::new(<Name>Tool::new()))` below.

use std::sync::Arc;

use crate::registry::Registry;
use crate::tools::echo::EchoTool;
use crate::tools::fs::{edit::FsEditTool, read::FsReadTool, write::FsWriteTool};

pub fn builtin_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoTool::new()));
    reg.register(Arc::new(FsReadTool::new()));
    reg.register(Arc::new(FsWriteTool::new()));
    reg.register(Arc::new(FsEditTool::new()));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_tools() {
        let r = builtin_registry();
        assert_eq!(r.count(), 4);
        assert!(r.get("ref:echo.say").is_some());
        assert!(r.get("ref:fs.read").is_some());
        assert!(r.get("ref:fs.write").is_some());
        assert!(r.get("ref:fs.edit").is_some());
    }
}
```

- [ ] **Step 7.3: Run + commit**

```bash
cargo test -p atd-ref-server                     # ~80 lib tests (server 10 + builtin 1 + prior)
cargo test --workspace --all-targets             # ~180 Rust tests
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): wire fs tools into dispatch + register in builtin"
```

---

## Task 8: Integration tests — cross-tool flows over the wire

**Files:**
- Modify: `crates/atd-ref-server/tests/integration.rs`

Add 7 new end-to-end tests exercising Write→Read→Edit and error paths.

- [ ] **Step 8.1: Append integration tests**

Add the following tests to the existing `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs` (keep all existing tests and helpers intact). Add a small helper for multi-request connections near the top if not already present:

```rust
use std::time::Duration as StdDuration;

async fn send_on_stream(
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
```

Then add these tests at the end of the file:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_fs_write_then_read_roundtrip() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("roundtrip.txt");

    // Write
    let w = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.write",
            "args": {"path": path.to_string_lossy(), "content": "hello\nworld\n"},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(w["success"], serde_json::json!(true));
    assert_eq!(w["result"]["bytes_written"], 12);

    // Read (new connection OK; Read doesn't need tracker history)
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["line_count"], 2);
    assert!(r["result"]["content"].as_str().unwrap().contains("   1\thello"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_read_then_edit_same_connection_succeeds() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("rw.txt");
    std::fs::write(&path, "hello world\n").unwrap();

    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    // Read first (records in tracker)
    let r = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(r["success"], serde_json::json!(true));

    // Then Edit on the SAME connection
    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "hello",
                "new_string": "HI"
            },
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(e["success"], serde_json::json!(true));
    assert_eq!(e["result"]["replacements"], 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "HI world\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_edit_without_prior_read_returns_not_read() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("no-read-edit.txt");
    std::fs::write(&path, "hello\n").unwrap();

    // Fresh connection — tracker is empty. Edit must reject.
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "hello",
                "new_string": "hi"
            },
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(false));
    assert_eq!(r["result"]["code"], "NOT_READ");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_edit_after_external_modification_returns_file_modified() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("ext-mod.txt");
    std::fs::write(&path, "original\n").unwrap();

    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();

    // Read to populate tracker.
    let _ = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await;

    // External modification + wait for mtime to move forward.
    tokio::time::sleep(StdDuration::from_millis(1100)).await;
    std::fs::write(&path, "externally changed\n").unwrap();

    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "externally",
                "new_string": "xxx"
            },
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(e["success"], serde_json::json!(false));
    assert_eq!(e["result"]["code"], "FILE_MODIFIED");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_edit_multi_match_without_replace_all_is_invalid_args() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("multi.txt");
    std::fs::write(&path, "foo foo foo\n").unwrap();

    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();
    // Populate tracker via Read.
    let _ = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await;

    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "foo",
                "new_string": "bar"
            },
            "dry_run": false,
        }),
    )
    .await;
    // InvalidArgs maps to wire `error` response (not a tool_result).
    assert_eq!(e["type"], "error");
    assert!(e["message"].as_str().unwrap().contains("replace_all"));
    assert!(e["message"].as_str().unwrap().contains("3"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_edit_multi_match_with_replace_all_succeeds() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("multi-ok.txt");
    std::fs::write(&path, "foo foo foo\n").unwrap();

    let mut stream = UnixStream::connect(&srv.sock).await.unwrap();
    let _ = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await;
    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": path.to_string_lossy(),
                "old_string": "foo",
                "new_string": "bar",
                "replace_all": true
            },
            "dry_run": false,
        }),
    )
    .await;
    assert_eq!(e["success"], serde_json::json!(true));
    assert_eq!(e["result"]["replacements"], 3);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "bar bar bar\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_read_with_offset_beyond_file_returns_empty() {
    let srv = spawn_server().await;
    let workdir = tempfile::tempdir().unwrap();
    let path = workdir.path().join("short.txt");
    std::fs::write(&path, "only two\nlines\n").unwrap();

    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": path.to_string_lossy(), "offset": 100},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["line_count"], 0);
    assert_eq!(r["result"]["total_lines"], 2);
    assert_eq!(r["result"]["content"], "");
}
```

- [ ] **Step 8.2: Run + commit**

```bash
cargo test -p atd-ref-server --test integration     # 14 passed (7 prior + 7 new)
cargo test --workspace --all-targets                 # ~186 Rust tests
git add crates/atd-ref-server/
git commit -m "test(atd-ref-server): integration tests for fs Write/Read/Edit flows"
```

---

## Task 9: `examples/rw_cycle.rs` — in-process demo

**Files:**
- Create: `crates/atd-ref-server/examples/rw_cycle.rs`

Single-process example that spawns Server in a tokio task, connects from the same process, and demonstrates Write → Read → Edit in sequence. Runnable via `cargo run -p atd-ref-server --example rw_cycle`.

- [ ] **Step 9.1: Write the example**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/examples/rw_cycle.rs`:

```rust
//! Single-connection Write → Read → Edit cycle against atd-ref-server.
//!
//! This example starts a Server instance in the current process, opens ONE
//! connection to it, and performs a full round-trip to illustrate the
//! must-read-before-edit invariant working over the wire.

use std::sync::Arc;
use std::time::Duration;

use atd_ref_server::builtin::builtin_registry;
use atd_ref_server::server::{Server, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

async fn send_on_stream(
    stream: &mut UnixStream,
    req: serde_json::Value,
) -> std::io::Result<serde_json::Value> {
    let body = serde_json::to_vec(&req).unwrap();
    stream.write_all(&(body.len() as u32).to_be_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let n = u32::from_be_bytes(header) as usize;
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf).await?;
    Ok(serde_json::from_slice(&buf).unwrap())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Workdir for the demo
    let workdir = tempfile::tempdir()?;
    let sock = workdir.path().join("rw_cycle.sock");
    let file_path = workdir.path().join("demo.txt");

    // Start server in a background task
    let mut config = ServerConfig::default();
    config.socket_path = sock.clone();
    config.cwd = workdir.path().to_path_buf();
    let server = Server::new(builtin_registry(), config);
    let _server_handle = Arc::new(tokio::spawn(async move {
        let _ = server.run().await;
    }));

    // Wait for socket to appear
    for _ in 0..50 {
        if sock.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !sock.exists() {
        return Err("server did not create socket".into());
    }

    let mut stream = UnixStream::connect(&sock).await?;

    println!("[rw_cycle] 1. Write");
    let w = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.write",
            "args": {
                "path": file_path.to_string_lossy(),
                "content": "hello world\nline two\n"
            },
            "dry_run": false,
        }),
    )
    .await?;
    println!("    result: {}", serde_json::to_string(&w["result"])?);

    println!("[rw_cycle] 2. Read");
    let r = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": file_path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await?;
    println!(
        "    {} lines total, content:\n{}",
        r["result"]["line_count"],
        r["result"]["content"].as_str().unwrap()
    );

    println!("[rw_cycle] 3. Edit (replace 'hello' → 'HI')");
    let e = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.edit",
            "args": {
                "path": file_path.to_string_lossy(),
                "old_string": "hello",
                "new_string": "HI"
            },
            "dry_run": false,
        }),
    )
    .await?;
    println!("    result: {}", serde_json::to_string(&e["result"])?);

    println!("[rw_cycle] 4. Verify (Read again)");
    let r2 = send_on_stream(
        &mut stream,
        serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.read",
            "args": {"path": file_path.to_string_lossy()},
            "dry_run": false,
        }),
    )
    .await?;
    println!(
        "    final content:\n{}",
        r2["result"]["content"].as_str().unwrap()
    );

    println!("[rw_cycle] done.");
    Ok(())
}
```

- [ ] **Step 9.2: Build + run + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server --example rw_cycle
cargo run  -p atd-ref-server --example rw_cycle
```

Expected:
- Build succeeds
- Output shows Write bytes_written=21, Read line_count=2 with `   1\thello world`, Edit replacements=1, final content has `HI world`

```bash
git add crates/atd-ref-server/
git commit -m "docs(atd-ref-server): add rw_cycle example (Write/Read/Edit in one connection)"
```

---

## Task 10: README update + independence check + tag

**Files:**
- Modify: `crates/atd-ref-server/README.md` (add "Per-connection state" section)

- [ ] **Step 10.1: Insert the new section**

Open `/home/nan/proj/atd-mvp/crates/atd-ref-server/README.md`. Find the heading `## Contracts a tool MUST honor`. Immediately BEFORE that heading, insert:

```markdown
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

```

(The closing triple-backtick of this markdown snippet is the one that terminates this task's own fenced code block. The ```rust ... ``` inside is the README content.)

Also update the "What SP-2+ adds" bullet list — mark SP-2 as done:

```markdown
## What's shipped and what's next

- **SP-1 (shipped):** framework + `ref:echo.say`
- **SP-2 (shipped):** `ref:fs.read`, `ref:fs.write`, `ref:fs.edit` + `ReadTracker` per-connection state
- **SP-3:** `ref:shell.exec` (Bash) + `ref:shell.pwsh` (PowerShell)
- **SP-4:** `ref:fs.glob` + `ref:fs.grep`
- **SP-5:** `ref:web.fetch`
```

Find the existing "What SP-2+ adds" section and replace with the above.

- [ ] **Step 10.2: Independence check**

```bash
cd /home/nan/proj/atd-mvp
cargo tree -p atd-ref-server --prefix none \
  | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )' \
  && echo FAIL \
  || echo "OK: no client/bridge/cli/anos deps"

grep -E '^\s*(atd-client|atd-mcp-bridge|atd-cli|anos-)' crates/atd-ref-server/Cargo.toml \
  && echo FAIL \
  || echo "OK: manifest clean"
```

Expected: both OK.

- [ ] **Step 10.3: Final regression**

```bash
cargo test --workspace --all-targets
```

Expected: ~186 Rust tests passing (35 Python unaffected).

- [ ] **Step 10.4: Commit + tag**

```bash
git add crates/atd-ref-server/README.md
git commit -m "docs(atd-ref-server): document per-connection state and mark SP-2 shipped"

git tag -a sp2-ref-server-file-io -m "SP-2: atd-ref-server file I/O (Read/Write/Edit + ReadTracker)"
git log --oneline | head -15
git tag
```

---

## Post-Plan Verification Checklist

- [ ] `cargo build -p atd-ref-server --release` zero warnings
- [ ] `cargo test -p atd-ref-server` ~93 tests pass (79 lib + 14 integration)
- [ ] `cargo test --workspace --all-targets` ~186 Rust tests pass
- [ ] `cargo run -p atd-ref-server --example rw_cycle` completes and prints Write → Read → Edit round-trip
- [ ] `cargo tree` independence check returns empty
- [ ] `crates/atd-ref-server/README.md` has the "Per-connection state" section
- [ ] Tag `sp2-ref-server-file-io` created

## What's next after SP-2

- **SP-3:** `ref:shell.exec` + `ref:shell.pwsh` — subprocess execution with timeout + output capture
- **SP-4:** `ref:fs.glob` + `ref:fs.grep` — filesystem search using `globset` + `grep` (ripgrep library)
- **SP-5:** `ref:web.fetch` — network tool with reqwest + html2md
- **SP-6:** cross-crate E2E rewrite: Python `hello_atd.py` and Rust `hello_atd.rs` point at atd-ref-server instead of ANOS; validation doc + tag
