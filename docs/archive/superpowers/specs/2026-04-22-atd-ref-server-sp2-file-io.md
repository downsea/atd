# atd-ref-server — SP-2 File I/O Design Spec

**Date:** 2026-04-22
**Status:** Design approved; plan pending.
**Scope:** Sub-project 2 of the `atd-ref-server` initiative. Adds three real file-I/O tools + per-connection `ReadTracker` state, bringing atd-ref-server from "framework with echo" to "framework with a usable tool catalog."
**Builds on:** `docs/superpowers/specs/2026-04-22-atd-ref-server-sp1-foundation.md` (SP-1 Foundation, tag `sp1-ref-server-foundation`)

---

## 1. Motivation

SP-1 shipped the framework + echo. Everything works end-to-end in the wire sense, but a developer who connects to atd-ref-server can't actually do anything useful. SP-2 closes that gap with the three file-I/O tools that dominate real agent workflows: read a file, write a file, edit a file.

This also activates the **per-connection state layer** that SP-1 declared but didn't populate (spec §4.5). Edit needs a "you must have Read this file in this session" invariant — that's what `ReadTracker` provides.

Like SP-1, this is clean-room: designed from universal file-operation semantics and the Rust standard library. No proprietary source is read or referenced.

---

## 2. Scope

### 2.1 In scope

- **`ref:fs.read`** — read a file, return line-numbered content with optional offset/limit
- **`ref:fs.write`** — atomic (tempfile + rename) file write
- **`ref:fs.edit`** — exact-string find-and-replace with must-read-before-edit invariant
- **`ReadTracker`** — per-connection HashMap keyed by canonical path, storing mtime + size recorded at Read time
- **`CallContext.read_tracker: Option<Arc<ReadTracker>>`** — new field, backwards-compatible addition
- **Per-connection tracker construction** in `handle_connection`; shared into each `CallContext` via `Arc`
- **Shared helpers** in `tools/fs/shared.rs`: path resolution, line numbering, atomic write
- **Integration tests** covering cross-tool flows (Write→Read→Edit, must-read enforcement, external-modification detection)
- **README update** — new "Per-connection state" section + ReadTracker usage example

### 2.2 Explicitly deferred

- **File permissions / ownership** inspection — OS permissions gate access; tools surface OS errors
- **Binary file handling** — UTF-8 decode failure surfaces as `ExecutionFailed{code:"ENCODING"}`; no separate binary mode
- **Symlink control** — follow by default (Rust std behavior); no "no-follow" flag in SP-2
- **Recursive operations** (rmdir, copy-tree, etc.) — out of scope; single-file ops only
- **Path sandboxing** / confinement to `ctx.cwd` — Q2 answered: no. The reference server is dev-tool surface, not a sandbox
- **Watch / subscribe** — Phase 2 ATD feature
- **Locking across clients** — each connection gets its own tracker; cross-connection races are the OS's job

### 2.3 Prerequisites

- atd-ref-server at tag `sp1-ref-server-foundation`, 135 Rust workspace tests green
- No new workspace dependencies required (uses std + tokio + existing deps)

---

## 3. Locked decisions (from the brainstorm)

1. **ReadTracker strictness:** mtime OR size changed → Edit refuses with `ExecutionFailed{code:"FILE_MODIFIED"}`. No content hash comparison.
2. **Path model:** no confinement. Relative paths resolve against `ctx.cwd`. Absolute paths used as-is. OS permissions gate.
3. **Edit multi-match policy:** `replace_all=false` + N occurrences (N ≥ 2) → `InvalidArgs("N occurrences of old_string; supply more context or replace_all=true")`.
4. **Encoding:** UTF-8 required. Non-UTF-8 → `ExecutionFailed{code:"ENCODING"}`. No auto-detect, no lossy.
5. **Atomic write:** tempfile in same directory + rename. `std::fs::rename` is atomic on POSIX when source and destination are on the same filesystem (which they are by construction).
6. **No auto-create parent dirs.** Missing parent → `ExecutionFailed{code:"NO_PARENT"}`.
7. **Line numbering format:** `"    N\tcontent"` — right-padded 4-character-min line number, tab separator. 1-indexed. Matches the convention atd-cli's Read already uses.
8. **Tracker identity:** path canonicalized via `std::fs::canonicalize` for keying. This correctly handles symlinks and `./`/`../`.
9. **Tracker scope:** per-connection, no TTL, no size cap. Drops with the connection. SP-1's `ServerState` is global — read_tracker lives one scope narrower.
10. **`for_test()` default:** `read_tracker: None` (tools that need it can set explicitly in tests).

---

## 4. Architecture

### 4.1 Module layout additions

```
crates/atd-ref-server/src/
├── tracker.rs                 (NEW — ReadTracker + ReadTrackerError)
├── context.rs                 (MODIFY — add read_tracker field)
├── server.rs                  (MODIFY — build tracker per-connection)
├── builtin.rs                 (MODIFY — register 3 new tools)
└── tools/
    ├── mod.rs                 (MODIFY — export fs submodule)
    ├── echo.rs                (unchanged)
    └── fs/                    (NEW subtree)
        ├── mod.rs             (re-exports)
        ├── shared.rs          (resolve_path, format_with_line_numbers, atomic_write)
        ├── read.rs            (ref:fs.read)
        ├── write.rs           (ref:fs.write)
        └── edit.rs            (ref:fs.edit)
```

### 4.2 Data flow changes

```
accept()
   │
   ▼
handle_connection(state, stream)
   │
   ▼
  ┌────────────────────────────────────┐
  │  NEW in SP-2:                      │
  │  let tracker = Arc::new(           │
  │      ReadTracker::new());          │
  │  (one per connection; dropped at   │
  │   disconnect)                      │
  └────────────────────────────────────┘
   │
   ▼
loop { read_frame → dispatch → write_frame }
                          │
                          ▼
                   CallContext {
                       cwd, ..., 
                       read_tracker: Some(tracker.clone()),  ← NEW
                   }
                          │
                          ▼
                    tool.call(args, &ctx)
                          │
              ┌───────────┴───────────┐
              ▼                       ▼
         fs::read.rs              fs::edit.rs
         record in tracker        check tracker
         on success               before modifying
```

### 4.3 `ReadTracker` design

```rust
// tracker.rs
pub struct ReadTracker {
    entries: Mutex<HashMap<PathBuf, ReadRecord>>,
}

struct ReadRecord {
    mtime: SystemTime,
    size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ReadTrackerError {
    #[error("file has not been read in this session: {path}")]
    NotRead { path: PathBuf },
    #[error("file modified since it was read: {path}")]
    Modified { path: PathBuf },
}

impl ReadTracker {
    pub fn new() -> Self;
    
    /// Record a successful Read. `path` must be already canonicalized.
    pub fn record(&self, path: PathBuf, mtime: SystemTime, size: u64);
    
    /// Check that the path was read in this session AND hasn't changed.
    /// `path` must be already canonicalized; caller also provides current
    /// mtime + size (to avoid racing stat inside the lock).
    pub fn check(
        &self,
        path: &Path,
        current_mtime: SystemTime,
        current_size: u64,
    ) -> Result<(), ReadTrackerError>;
}
```

`Mutex<HashMap>` over `DashMap` / `RwLock` because: (a) operations are single writer per call, (b) contention is per-connection (single logical thread of work), (c) simpler.

### 4.4 `CallContext` extension

```rust
pub struct CallContext {
    pub cwd: PathBuf,
    pub max_output_bytes: usize,
    pub call_id: ulid::Ulid,
    pub deadline: Option<Instant>,
    pub read_tracker: Option<Arc<ReadTracker>>,   // NEW
}

impl CallContext {
    #[cfg(any(test, feature = "testing"))]
    pub fn for_test() -> Self {
        Self {
            // ... existing ...
            read_tracker: None,
        }
    }
}
```

**Breaking-change management:** SP-1 tests that build `CallContext { ... }` literal need updating (3 call sites: `context.rs` tests for remaining_time, `tools/echo.rs` truncation test, and `server.rs` dispatch tests might build one). This is in-crate, fully under our control.

**Why `Option<Arc<ReadTracker>>`:** tools that don't need it (echo, future tools with no must-read requirement) can ignore the field. Tools that need it do `.as_ref()` and fail gracefully if unset (test fixtures might set `None`).

### 4.5 Tool specifications

#### 4.5.1 `ref:fs.read`

**Input schema:**

```json
{
  "type": "object",
  "properties": {
    "path":   { "type": "string", "minLength": 1 },
    "offset": { "type": "integer", "minimum": 1, "description": "1-indexed start line" },
    "limit":  { "type": "integer", "minimum": 1, "description": "max lines to return" }
  },
  "required": ["path"]
}
```

**Output shape (success):**

```json
{
  "path": "/absolute/canonical/path",
  "content": "   1\tfirst line\n   2\tsecond line\n...",
  "line_count": 42,
  "total_lines": 42,
  "truncated": false
}
```

- `content`: lines joined by `\n`, each prefixed `   N\t` where N is the 1-indexed absolute line number (right-padded to min 4 chars). Final newline preserved from original file if present.
- `line_count`: number of lines returned.
- `total_lines`: total lines in the file (may differ from `line_count` if offset/limit/truncation applied).
- `truncated`: `true` when `ctx.max_output_bytes` forced early cutoff (at a line boundary).

**Behavior:**

1. Resolve path (absolute: as-is; relative: join with `ctx.cwd`).
2. Canonicalize via `std::fs::canonicalize` → `canonical_path`.
3. Read file bytes; UTF-8 decode. Failure → `ExecutionFailed{code:"ENCODING", retryable:false}`.
4. Split into lines (preserving trailing newline state), compute `total_lines`.
5. Apply offset (skip offset-1 lines). If offset > total_lines, return empty content with `line_count:0`.
6. Apply limit (take at most `limit` lines).
7. Format each line with `   N\t` prefix. If growing output would exceed `ctx.max_output_bytes`, stop at the current line boundary and set `truncated:true`.
8. Record in `ctx.read_tracker` (if present): `canonical_path`, current mtime, current size.
9. Return success.

**Error mapping:**

| OS/state | Tool error |
|---|---|
| File doesn't exist | `ExecutionFailed{code:"NOT_FOUND", retryable:false}` |
| Path is a directory | `ExecutionFailed{code:"IS_DIR", retryable:false}` |
| Permission denied | `ExecutionFailed{code:"EACCES", retryable:false}` |
| Non-UTF-8 bytes | `ExecutionFailed{code:"ENCODING", retryable:false}` |
| Other I/O error | `ExecutionFailed{code:"IO", message:<os msg>, retryable:true}` |
| offset < 1 or limit < 1 | `InvalidArgs("offset/limit must be >= 1")` |

#### 4.5.2 `ref:fs.write`

**Input schema:**

```json
{
  "type": "object",
  "properties": {
    "path":    { "type": "string", "minLength": 1 },
    "content": { "type": "string" }
  },
  "required": ["path", "content"]
}
```

**Output shape:**

```json
{
  "path": "/absolute/canonical/path",
  "bytes_written": 1234,
  "created": true
}
```

- `created`: `true` if the file didn't exist before.

**Behavior:**

1. Resolve path; absolute as-is, relative against `ctx.cwd`.
2. Check parent directory exists. If not → `ExecutionFailed{code:"NO_PARENT"}`. No auto-create.
3. Determine `created`: does the final path exist before the write?
4. Write to `<parent>/.atd-ref-write-<ulid>.tmp` with the content.
5. `std::fs::rename(tmp, final_path)` — atomic on POSIX.
6. Canonicalize the final path (for output `path`).
7. Does NOT record in ReadTracker — writes don't satisfy the read-before-edit contract.
8. Return success with `bytes_written = content.as_bytes().len()`.

**Error mapping:**

| Situation | Tool error |
|---|---|
| Parent missing | `ExecutionFailed{code:"NO_PARENT", retryable:false}` |
| Permission denied on parent | `ExecutionFailed{code:"EACCES", retryable:false}` |
| I/O failure during write | `ExecutionFailed{code:"IO", retryable:true}` |
| Other | `ExecutionFailed{code:"IO", retryable:true}` |

#### 4.5.3 `ref:fs.edit`

**Input schema:**

```json
{
  "type": "object",
  "properties": {
    "path":        { "type": "string", "minLength": 1 },
    "old_string":  { "type": "string", "minLength": 1 },
    "new_string":  { "type": "string" },
    "replace_all": { "type": "boolean", "default": false }
  },
  "required": ["path", "old_string", "new_string"]
}
```

**Output shape:**

```json
{
  "path": "/absolute/canonical/path",
  "replacements": 1,
  "bytes_written": 1234
}
```

**Behavior:**

1. Resolve + canonicalize path.
2. Check `ctx.read_tracker`:
   - If `None`: `InternalError("server did not attach a read_tracker")` — this is a config bug, not a user bug.
   - If present: `tracker.check(canonical_path, current_mtime, current_size)`:
     - `Err(NotRead)` → `ExecutionFailed{code:"NOT_READ", retryable:false, message:"call ref:fs.read on this path first"}`
     - `Err(Modified)` → `ExecutionFailed{code:"FILE_MODIFIED", retryable:false, message:"file changed since Read; call Read again"}`
     - `Ok(())` → proceed.
3. Read file (UTF-8 decode; encoding failure → same `ENCODING` error as Read).
4. Count occurrences of `old_string`.
   - 0 → `InvalidArgs("old_string not found in file")`
   - ≥ 2 and `replace_all=false` → `InvalidArgs("N occurrences of old_string; supply more context or set replace_all=true")` (N in the message)
5. Perform replacement:
   - `replace_all=true`: replace all occurrences, `replacements = N`
   - `replace_all=false` (and N=1): replace the sole occurrence, `replacements = 1`
6. Atomic write: tempfile + rename (same procedure as Write).
7. Record the **new** file state in `ctx.read_tracker` so immediate subsequent Edits on the same file work. (Without this, the second Edit would hit `FILE_MODIFIED` because Edit just changed the file.)
8. Return success.

### 4.6 `tools/fs/shared.rs`

Centralizes logic used by multiple fs tools:

```rust
pub fn resolve_path(cwd: &Path, input: &str) -> PathBuf;
pub fn canonicalize_existing(path: &Path) -> Result<PathBuf, std::io::Error>;

/// Format text with "   N\t" line prefixes. Returns (formatted, truncated, lines_shown).
pub fn format_with_line_numbers(
    text: &str,
    offset: usize,       // 1-indexed
    limit: Option<usize>,
    max_output_bytes: usize,
) -> LineFormatResult;

pub struct LineFormatResult {
    pub content: String,
    pub lines_shown: usize,
    pub total_lines: usize,
    pub truncated: bool,
}

/// Atomic write: tempfile in `path`'s parent + rename. Returns (bytes_written, created).
pub async fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<AtomicWriteResult>;

pub struct AtomicWriteResult {
    pub bytes_written: usize,
    pub created: bool,
}
```

Makes Read / Write / Edit thin. Keeps each tool file < 200 LOC.

---

## 5. Per-connection wiring

### 5.1 `server.rs::handle_connection`

```rust
async fn handle_connection(state: Arc<ServerState>, stream: UnixStream) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let tracker = Arc::new(crate::tracker::ReadTracker::new());  // NEW: per-connection
    loop {
        let req: Request = match read_frame(&mut reader).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let resp = dispatch(&state, &tracker, req).await;   // NEW: pass tracker
        write_frame(&mut writer, &resp).await?;
    }
}
```

### 5.2 `dispatch` signature change

```rust
pub(crate) async fn dispatch(
    state: &Arc<ServerState>,
    tracker: &Arc<crate::tracker::ReadTracker>,   // NEW
    req: Request,
) -> Response {
    // ... run_tool branch sets ctx.read_tracker = Some(tracker.clone())
}
```

**Test-site impact:** SP-1's `src/server.rs` `mod tests` calls `dispatch(&s, ...)` ~10 times. Each call-site needs a `tracker` argument. Fix: add a tests-local helper `fn dispatch_no_tracker(s, req) -> Response` that creates a throwaway tracker inline. Keeps existing tests intact.

---

## 6. Tests

### 6.1 Unit tests (approximate counts)

| Module | Tests | Notes |
|---|---|---|
| `tracker.rs` | 5 | record/check happy path, NotRead, Modified-by-mtime, Modified-by-size, empty-tracker |
| `context.rs` | +1 | for_test sets read_tracker=None |
| `tools/fs/shared.rs` | 8 | resolve (abs/rel), canonicalize on nonexistent error, line formatting with various offset/limit/budget, atomic_write create-vs-overwrite |
| `tools/fs/read.rs` | 10 | happy path, offset, limit, offset+limit, nonexistent, is-dir, encoding-fail, truncation-on-max-output-bytes, empty file, records in tracker |
| `tools/fs/write.rs` | 6 | create new, overwrite existing, no-parent, byte count correct, returns canonical path, does NOT record in tracker |
| `tools/fs/edit.rs` | 12 | single-match replace, multi-match+replace_all, multi-match+no-replace-all→error, zero-match→error, without prior Read→NotRead, with external mod→FILE_MODIFIED, encoding-fail, re-record after success, bytes_written correct, idempotent 2nd Edit, replacement at file boundaries |
| `server.rs` | +1 | dispatch now builds and passes tracker; existing 10 still green |
| `builtin.rs` | +1 | builtin_registry has 4 tools now (echo + 3 fs) |

**New lib unit tests: ~44.** (SP-1 lib was 36; SP-2 lib will be ~80.)

### 6.2 Integration tests

Add to `tests/integration.rs` (or new file `tests/fs_integration.rs`):

| # | Scenario |
|---|---|
| I-1 | Write(file, "hello\nworld") → Read(file) returns line-numbered "hello\nworld" |
| I-2 | Read(file) then Edit(file, "hello", "HI", false) → replacement succeeds |
| I-3 | Edit without prior Read → `isError=true` with NOT_READ code in content |
| I-4 | Read then external-touch then Edit → `isError=true` with FILE_MODIFIED |
| I-5 | Edit with `replace_all=false` on 3-occurrence file → `InvalidArgs` via wire `error` response |
| I-6 | Edit with `replace_all=true` on 3-occurrence file → success, replacements=3 |
| I-7 | Read with offset beyond file → empty content, line_count=0 |

**New integration tests: 7.** (SP-1 had 7; SP-2 total: 14.)

### 6.3 Workspace target

| State | Rust tests |
|---|---|
| Post SP-1 | 135 |
| SP-2 lib adds | ~44 |
| SP-2 integration adds | 7 |
| **Post SP-2** | **~186** |

---

## 7. `CallContext` breaking-change migration plan

SP-1 has three literal `CallContext { ... }` constructions in non-test code paths and several in tests:

1. `server.rs::dispatch` (production) — must set `read_tracker`. Plan task explicitly does this.
2. `context.rs::for_test` — set `None`.
3. `context.rs` tests (remaining_time) — set `None`.
4. `tools/echo.rs` truncation test — set `None`.
5. `server.rs::tests::state_with_failing_tool` — the tests construct CallContext inside dispatch; actual test wrapper builds ServerState, not CallContext. No change needed.

All 5 sites are in-crate. Plan task decomposes these as part of Task 2 ("CallContext extension").

---

## 8. README updates (Task 8 of the plan)

Add a new section to `crates/atd-ref-server/README.md` between **Contracts** and **Error classification**:

### "Per-connection state"

- What it is: tools can access `ctx.read_tracker` for cross-call state that's bounded to one client session.
- Existing use: `ref:fs.edit` enforces "read before edit" via `ReadTracker`.
- How to use it in your own tool:
  ```rust
  if let Some(tracker) = &ctx.read_tracker {
      tracker.check(&path, current_mtime, current_size)
          .map_err(|e| ToolCallError::ExecutionFailed { 
              code: "NOT_READ".into(), 
              message: e.to_string(), 
              retryable: false 
          })?;
  }
  ```
- Lifetime: from connection accept to connection close. Not persisted, not shared across connections.
- When to add new per-connection state: if a tool needs to remember something from an earlier call in the same session that isn't appropriate for global server state.

---

## 9. Exit criteria

1. `cargo build -p atd-ref-server --release` zero warnings.
2. `cargo test -p atd-ref-server` — ~80 tests (44 new lib + 36 SP-1 lib + 14 integration).
3. `cargo test --workspace --all-targets` — ~186 Rust tests (135 prior + 51 new).
4. Independence check passes: `cargo tree -p atd-ref-server | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )'` empty.
5. Manual smoke with atd-cli against live ref-server:
   - `atd call ref:fs.write --args '{"path":"/tmp/sp2-smoke.txt","content":"line1\nline2\n"}'` → success
   - `atd call ref:fs.read --args '{"path":"/tmp/sp2-smoke.txt"}'` → content with `   1\tline1`, etc.
   - `atd call ref:fs.edit --args '{"path":"/tmp/sp2-smoke.txt","old_string":"line1","new_string":"LINE_1"}'` — **on a fresh connection** → NOT_READ error; in the same atd-cli session this isn't reachable because each `atd` invocation is a new connection. Acknowledge this in the README (see §10 below).
6. README has the new "Per-connection state" section.
7. Git tag `sp2-ref-server-file-io` created.

### 9.1 The atd-cli-tests-must-read-before-edit caveat

atd-cli's current shape is "one-shot command per invocation → new connection every time". So `atd call ref:fs.edit` from a fresh shell will always hit NOT_READ. This is the correct protocol behavior, but makes manual smoke awkward.

**Mitigation:** the SP-2 plan adds a tiny `examples/rw_cycle.rs` (~40 LOC) that opens ONE connection and does Write → Read → Edit in sequence, demonstrating the must-read invariant works when the session is maintained. Live smoke in exit criteria #5 is **this binary**, not raw `atd`.

This is cheap to do, it's legitimately useful as a reference example for third parties, and it belongs in the ref-server crate as `examples/rw_cycle.rs` (crate examples, different from the existing workspace `examples/` dir).

---

## 10. Design decisions locked (reference)

1. ReadTracker detects modification by mtime OR size change; no content hashing.
2. No path confinement; cwd prepends only for relative paths.
3. Edit refuses ambiguous multi-match without `replace_all=true`.
4. UTF-8 strict; non-UTF-8 files raise ENCODING error.
5. Atomic write: tempfile + rename; same-filesystem requirement satisfied by construction.
6. No auto-create parent dirs; missing parent is NO_PARENT error.
7. Line format `   N\tcontent`, 1-indexed.
8. Canonicalize tracker keys with `std::fs::canonicalize`.
9. Tracker per-connection, dropped on disconnect.
10. `CallContext::read_tracker: Option<Arc<ReadTracker>>` — Option because unit tests may omit it.

---

## 11. Open questions (none blocking)

All answered in brainstorm; none remain gating.

Forward-looking, non-blocking:

- **Permission bits preservation on write/edit:** Currently we write with default umask; Edit may re-create files with different perms than original. Probably fine for Phase 0. SP-6 validation may surface real-world cases that push for preservation.
- **Atomic write across filesystems:** If `cwd` is on a different fs than tempdir, rename fails. We use same-dir tempfiles specifically to avoid this, but users could point us at weird mounts. If that bites, fall back to copy+unlink — adds complexity, skip for now.
