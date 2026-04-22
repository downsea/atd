# atd-ref-server — SP-4 Search (Glob + Grep) Design Spec

**Date:** 2026-04-23
**Status:** Design approved; plan pending.
**Scope:** Sub-project 4 of atd-ref-server. Adds `ref:fs.glob` (ripgrep-style path discovery) + `ref:fs.grep` (line-level regex search) using the ripgrep Rust library stack. Expands the tool catalog from "read/write/exec" to "read/write/exec + search."
**Builds on:** SP-3 (`sp3-ref-server-shell`) — 212 Rust workspace tests, 6 tools registered.

---

## 1. Motivation

Agents ask "where is X?" constantly. Without `fs.glob`/`fs.grep`, every search turns into a `shell.exec` call to `find` / `rg` / `grep`. That has three failure modes: wildly different syntax per platform; shell quoting rules agents get wrong; output truncation policies that vary and are hard to reason about.

Native search tools give agents:
- **Deterministic schema** — JSON result shape is identical everywhere, agents don't parse ad-hoc text.
- **Deterministic defaults** — `.gitignore` respected, hidden files skipped, binary files skipped. Matches what `rg` does, which is what agents have seen in training.
- **Explicit truncation** — `truncated: true` flag + both `max_matches` and byte caps, so agents know when results were clipped.

Using ripgrep's *library crates* (`ignore`, `globset`, `grep-searcher`, `grep-regex`) is the pragmatic middle path: we get ripgrep's battle-tested walker + matcher without shelling out to a binary that may not be installed and without reinventing `.gitignore` parsing.

Like SP-1/2/3, this is clean-room: ripgrep's published library crates are used as a normal Rust dependency; no proprietary source is consulted for behavior porting.

---

## 2. Scope

### 2.1 In scope

- **`ref:fs.glob`** — glob pattern → paths. `ignore::WalkBuilder` (honors `.gitignore` / `.ignore` / hidden by default) + `globset::GlobSet` filter.
- **`ref:fs.grep`** — regex pattern + optional glob filter → line matches. Same walker; `grep-searcher` with `grep-regex` matcher; binary files skipped via `BinaryDetection::quit`.
- Registration in `builtin.rs` (2 new tools, count 6 → 8).
- **4 integration tests** — glob happy, grep happy, grep with glob filter, tool_list count = 8.
- README update — mark SP-4 shipped; add brief "Search tools" section.

### 2.2 Explicitly deferred (Phase 2+)

- **Streaming results** — wire response is one-shot JSON; ATD needs progressive-response support before this makes sense.
- **Richer grep knobs** — `invert`, `word_boundary`, `fixed_string`, `context_before`/`context_after`, `multiline`, `hidden`, `no_ignore`. Agents compose complex queries in the pattern itself; these are additive later.
- **`--json` / ripgrep-wire-compatible output** — our JSON is domain-specific, not a ripgrep drop-in replacement.
- **Parallel walking** — `ignore` supports it; we use the serial walker for predictable output ordering.
- **Smart case** — users pass `case_insensitive: true` explicitly; we do not heuristically flip based on pattern content.
- **Submatch / capture groups** — `text` is the whole line; capture extraction is agent's job.
- **Per-call `.gitignore` override** — honors repository's `.gitignore` as-found; no disable flag.

### 2.3 Prerequisites

- atd-ref-server at tag `sp3-ref-server-shell`, 212 Rust workspace tests green.
- No new system dependencies; `ignore` + `globset` + `grep-searcher` + `grep-regex` are pure Rust, cross-platform.

---

## 3. Tool definitions

### 3.1 `ref:fs.glob`

**ID:** `ref:fs.glob`
**Name:** `File Glob`
**Domain:** `fs` · **Actions:** `["glob"]` · **Tags:** `["fs", "search", "glob"]`
**Safety:** `SafetyLevel::Read` · **Visibility:** `ToolVisibility::Public` · **Trust:** `L2Tested`
**Side effects:** `["filesystem:read"]` (read-only directory traversal)

**Input schema:**

```json
{
  "type": "object",
  "properties": {
    "pattern":     { "type": "string", "minLength": 1 },
    "path":        { "type": "string" },
    "max_matches": { "type": "integer", "minimum": 1 }
  },
  "required": ["pattern"]
}
```

- `pattern` — glob like `**/*.rs` or `src/*.toml`. Follows `globset` syntax (similar to `gitignore` globs, but anchored at `path`).
- `path` — absolute or relative to `ctx.cwd`. Default: `ctx.cwd`. Canonicalized before walking; must be a directory.
- `max_matches` — default 1000. Hard cap on returned paths.

**Output schema:**

```json
{
  "paths":        ["string"],
  "truncated":    "boolean",
  "root":         "string",
  "duration_ms":  "integer"
}
```

- `paths` — relative to `root`, sorted lexicographically (so output is deterministic across runs).
- `truncated` — true if `max_matches` or `ctx.max_output_bytes` was hit.
- `root` — canonical absolute path that was walked (so callers can rejoin if needed).
- `duration_ms` — wall-clock.

**Behavior:**

1. Resolve `path` against `ctx.cwd`, canonicalize. If not a directory → `ExecutionFailed { code: "NOT_A_DIRECTORY" }`.
2. Compile `pattern` into `GlobSet`. If invalid → `InvalidArgs(...)`.
3. Build an `ignore::WalkBuilder` on `root`:
   - `.gitignore` / `.ignore` / `.rgignore` respected
   - hidden files / dirs skipped
   - no symlink following (default)
4. Iterate serially. For each entry:
   - Skip if entry's path relative to root doesn't match `GlobSet`.
   - Push relative path to result vec.
   - Stop if `len >= max_matches` → set `truncated = true`, drain iterator.
   - Stop if total serialized bytes would exceed `ctx.max_output_bytes` → same.
5. Sort paths. Return.

**Error mapping:**

| Internal error | Tool error |
|---|---|
| `GlobError` (compile) | `InvalidArgs` |
| `path` doesn't exist / not a dir | `ExecutionFailed { code: "NOT_A_DIRECTORY", retryable: false }` |
| Walker IO | `ExecutionFailed { code: "IO", retryable: true }` |

### 3.2 `ref:fs.grep`

**ID:** `ref:fs.grep`
**Name:** `File Grep`
**Domain:** `fs` · **Actions:** `["grep"]` · **Tags:** `["fs", "search", "grep", "regex"]`
**Safety:** `SafetyLevel::Read` · **Visibility:** `ToolVisibility::Public` · **Trust:** `L2Tested`
**Side effects:** `["filesystem:read"]`

**Input schema:**

```json
{
  "type": "object",
  "properties": {
    "pattern":          { "type": "string", "minLength": 1 },
    "path":             { "type": "string" },
    "glob":             { "type": "string" },
    "case_insensitive": { "type": "boolean" },
    "max_matches":      { "type": "integer", "minimum": 1 }
  },
  "required": ["pattern"]
}
```

- `pattern` — regex in ripgrep's default flavor (Rust `regex` syntax, extended where `grep-regex` exposes). Use `regex::escape` on agent side for literal search.
- `path` — same as glob (default `ctx.cwd`, must be dir).
- `glob` — optional filename filter (e.g., `*.rs`). Applied after walker, before matching. Omitted = match every walked file.
- `case_insensitive` — default false.
- `max_matches` — default 1000. Hard cap on **match rows** (not files).

**Output schema:**

```json
{
  "matches": [
    { "path": "string", "line": "integer", "text": "string" }
  ],
  "truncated":   "boolean",
  "root":        "string",
  "duration_ms": "integer"
}
```

- `path` — relative to `root`.
- `line` — 1-indexed.
- `text` — matching line with trailing `\r?\n` stripped; UTF-8-lossy decoded.
- Ordering: by `path` lexicographic, then by `line` ascending.
- `truncated` — true if `max_matches` or output-byte cap was hit.

**Behavior:**

1. Resolve and canonicalize `path` same as glob.
2. Compile the regex via `grep_regex::RegexMatcherBuilder`. If `case_insensitive`, set `case_insensitive(true)`. Invalid regex → `InvalidArgs`.
3. Compile optional `glob` into a single-glob `GlobSet`.
4. Build the walker same as glob (`.gitignore` + hidden-skip + no symlinks).
5. For each entry:
   - Skip if `glob` is set and path doesn't match.
   - Open a `Searcher` with `BinaryDetection::quit(b'\x00')` — binary files are skipped entirely (not just their binary portions).
   - For each match line:
     - UTF-8-lossy decode the line bytes.
     - Push `{path, line, text}` into results.
     - If results length hits `max_matches` → set `truncated = true`, short-circuit remaining files.
     - If running byte total would exceed `ctx.max_output_bytes` → same.
6. Sort results. Return.

**Error mapping:**

| Internal error | Tool error |
|---|---|
| Invalid regex | `InvalidArgs` |
| Invalid glob | `InvalidArgs` |
| `path` doesn't exist / not a dir | `ExecutionFailed { code: "NOT_A_DIRECTORY", retryable: false }` |
| Per-file IO (unreadable file) | Skipped silently — don't fail the whole search |
| Walker IO | `ExecutionFailed { code: "IO", retryable: true }` |

---

## 4. File structure

```
crates/atd-ref-server/
├── Cargo.toml                         (MODIFY — add ignore, globset, grep-searcher, grep-regex)
├── README.md                          (MODIFY — mark SP-4 shipped, Quick start section)
└── src/
    ├── builtin.rs                     (MODIFY — register 2 new tools)
    └── tools/
        └── fs/
            ├── mod.rs                 (MODIFY — add glob + grep submodules)
            ├── glob.rs                (NEW — ~180 LOC)
            ├── grep.rs                (NEW — ~230 LOC)
            ├── read.rs  (unchanged)
            ├── write.rs (unchanged)
            ├── edit.rs  (unchanged)
            └── shared.rs (unchanged)
└── tests/
    └── integration.rs                 (MODIFY — add 4 new tests, update count)
```

No new top-level module (reuses existing `tools/fs/`), no shared-search helper (the overlap is small and extraction would cost readability).

---

## 5. Dependencies

All pure-Rust crates pulled via Cargo. Cross-platform.

```toml
[dependencies]
ignore = "0.4"
globset = "0.4"
grep-searcher = "0.1"
grep-regex = "0.1"
```

Not workspace-shared (yet): no other atd-mvp crate needs them. Direct pins match the style used for `libc` in SP-3.

**Independence check extended:** `cargo tree -p atd-ref-server` must still show zero `anos-*`, `atd-client`, `atd-mcp-bridge`, `atd-cli` dependencies. These four new deps transitively pull in `regex`, `aho-corasick`, etc. — all standard infrastructure, not protocol-coupling.

---

## 6. Test plan

### 6.1 Unit tests — `tools/fs/glob.rs` (7 tests)

1. `basic_pattern_returns_matching_paths` — tempdir with `a.rs`, `b.rs`, `c.txt`; `*.rs` → `["a.rs", "b.rs"]`
2. `recursive_pattern` — nested dirs; `**/*.rs` walks deep
3. `gitignore_respected` — drop `.gitignore` with `target/`; `**/*.rs` doesn't return `target/foo.rs`
4. `hidden_skipped_by_default` — `.hidden/foo.rs` is not returned by `**/*.rs`
5. `max_matches_cap_sets_truncated` — 50 files, cap 10 → 10 results + `truncated: true`
6. `path_scoping_honored` — `path` points at subdir; returned paths are relative to that subdir, not cwd
7. `invalid_glob_rejected` — `pattern: "["` → `InvalidArgs`

### 6.2 Unit tests — `tools/fs/grep.rs` (8 tests)

1. `basic_regex_finds_line` — file `src/main.rs` with `fn foo()`; `pattern: "fn\\s+\\w+"` → 1 match with correct line number
2. `case_insensitive_flag` — `Hello\nhello`; `pattern: "hello", case_insensitive: true` → 2 matches
3. `glob_filter_narrows_search` — `main.rs` + `main.py` both contain `TODO`; `glob: "*.rs"` → only rs hits
4. `binary_files_skipped` — `data.bin` contains NUL + pattern bytes; result is empty
5. `no_matches_returns_empty_array` — pattern that matches nothing → `{matches: [], truncated: false}`
6. `max_matches_cap_sets_truncated` — 20 files with 5 matches each, cap 10 → 10 results + `truncated: true`
7. `line_numbers_are_1_indexed` — match on line 1 reports `line: 1`, not 0
8. `invalid_regex_rejected` — `pattern: "["` → `InvalidArgs`

### 6.3 Integration tests — `tests/integration.rs` (4 new + 1 updated)

- `e2e_tool_list_returns_echo` — update assertion 6 → 8, add `ref:fs.glob` + `ref:fs.grep` to id set (mirrors SP-3 Task 5 cascading fix)
- `e2e_fs_glob_returns_paths` — end-to-end wire call, `**/*.rs` in tempdir with known layout
- `e2e_fs_grep_finds_match` — end-to-end wire call, `pattern: "TODO"` in tempdir with one known line
- `e2e_fs_grep_with_glob_filter` — combined `pattern` + `glob`, returns only filtered file's matches
- `e2e_fs_glob_invalid_pattern_returns_error` — `pattern: "["` → wire `error` response with InvalidArgs

### 6.4 Expected test counts

- `tools::fs::glob`: 7
- `tools::fs::grep`: 8
- `builtin`: `builtin_registry_contains_all_tools` asserts count=8
- `server`: `tool_list_returns_registered_summaries` asserts count=8
- Integration: 19 prior + 4 new = 23 (the 5th item above modifies in place)

Workspace total target: ~240 tests (212 prior + 7 + 8 + 4 + fine-tuning).

---

## 7. Plan task breakdown (preview)

1. **Task 1** — Cargo.toml deps + `tools/fs/mod.rs` empty-placeholder scaffold for `glob` and `grep` submodules. Zero new tests. Keeps every commit buildable.
2. **Task 2** — `tools/fs/glob.rs` with 7 unit tests.
3. **Task 3** — `tools/fs/grep.rs` with 8 unit tests.
4. **Task 4** — `builtin.rs` register (count 6 → 8) + cascading updates in `server.rs::tool_list_returns_registered_summaries` and `tests/integration.rs::e2e_tool_list_returns_echo`.
5. **Task 5** — 4 new integration tests.
6. **Task 6** — README + independence check + live smoke + tag `sp4-ref-server-search`.

No task is expected to exceed 200 LOC of new behavioral code.

---

## 8. Risks and non-risks

### 8.1 Risks

- **`grep-searcher` / `grep-regex` API churn:** these crates are pre-1.0 (0.1.x) and ripgrep's own release cadence can break minor versions. Mitigation: pin to an exact minor (`0.1`), and if upgrade is forced, the blast radius is one file (`grep.rs`) containing <230 LOC.
- **`ignore::Walk` performance on huge repos:** we use the serial walker for deterministic output. A 1M-file repo may take seconds. Acceptable — `max_matches` cap short-circuits long walks; real use is scoped.
- **Regex DoS (ReDoS):** `regex` crate has linear-time matching guarantees by design. Not an ambient risk, but document it so a future reviewer doesn't panic.

### 8.2 Non-risks

- **ignore file edge cases** (nested `.gitignore`, negation patterns, etc.) — delegated entirely to `ignore::Walk`. Same behavior as ripgrep.
- **Platform differences** — all four crates work on Linux/macOS/Windows. `ignore`'s hidden-file detection does the right thing per-OS.
- **Binary file corruption** — `BinaryDetection::quit` drops them before any output; no risk of bad bytes on the wire.

---

## 9. Exit criteria

1. `cargo build -p atd-ref-server --release` zero warnings
2. `cargo test -p atd-ref-server` passes (expected ~116 lib + 18 integration = 134 tests)
3. `cargo test --workspace --all-targets` passes ~240 Rust tests (212 prior + ~28 new)
4. Independence check: `cargo tree -p atd-ref-server | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )'` empty
5. Manifest check: `grep -E '^\s*(atd-client|atd-mcp-bridge|atd-cli|anos-)' crates/atd-ref-server/Cargo.toml` empty
6. Live smoke:
   - `atd ... call ref:fs.glob --args '{"pattern": "**/*.rs", "path": "crates/atd-ref-server/src"}'` returns paths including `lib.rs`, `server.rs`, etc.
   - `atd ... call ref:fs.grep --args '{"pattern": "pub fn", "path": "crates/atd-ref-server/src"}'` returns line hits with accurate numbers
7. Tag `sp4-ref-server-search` created

---

## 10. Out of scope for SP-4 (Phase 2+)

- `ref:fs.find` (mtime/size predicates)
- `ref:fs.du` (disk usage summary)
- `ref:fs.stat` (single-file metadata — partially covered by read's existing tracker side-effects, but no explicit tool yet)
- `ref:fs.sed` (in-place line transforms — edit.rs already covers)
- ripgrep-compatible JSON output format (`--json` replica)

These are all defensible additions later; none is load-bearing for MVP goals.
