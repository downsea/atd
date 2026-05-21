# atd-ref-server SP-4 Search (Glob + Grep) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ref:fs.glob` + `ref:fs.grep` to `atd-ref-server` using ripgrep's Rust library stack (`ignore`, `globset`, `grep-searcher`, `grep-regex`), expanding the tool catalog from 6 → 8.

**Architecture:** Two new files under `tools/fs/`: `glob.rs` (walk + `GlobSet` filter + sorted paths) and `grep.rs` (walk + optional glob filter + regex matcher via `grep-searcher` with `BinaryDetection::quit`). Both honor `.gitignore` and skip hidden files because `ignore::Walk` does that by default. No shared helper extracted — the overlap between the two tools is small and extraction would cost readability.

**Tech Stack:** Rust 2024, MSRV 1.85 · `ignore = "0.4"`, `globset = "0.4"`, `grep-searcher = "0.1"`, `grep-regex = "0.1"` (all NEW direct deps).

**Spec:** `docs/superpowers/specs/2026-04-23-atd-ref-server-sp4-search.md`

**Scope boundary:**
- **In:** 4 new deps; `tools/fs/glob.rs`; `tools/fs/grep.rs`; `builtin.rs` update; 4 new integration tests; cascading test assertion updates (6→8 in `builtin.rs`, `server.rs`, `integration.rs::e2e_tool_list_returns_echo`); README shipped marker.
- **Out (Phase 2+):** streaming results, `invert`/`word_boundary`/`fixed_string`/`context`/`multiline` grep knobs, ripgrep JSON wire format, parallel walker, smart-case, submatch/capture extraction.

**Prerequisites:**
- `sp3-ref-server-shell` tag, 212 Rust workspace tests green.
- `bash` and (optionally) `pwsh` from SP-3 are irrelevant here — SP-4 is pure Rust library work.

**Exit criteria:**
1. `cargo build -p atd-ref-server --release` zero warnings.
2. `cargo test -p atd-ref-server` — expected ~134 tests (116 lib + 18 integration).
3. `cargo test --workspace --all-targets` — expected ~240 Rust tests (212 prior + ~28 new).
4. Independence check `cargo tree -p atd-ref-server | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )'` empty.
5. Live smoke: `atd call ref:fs.glob --args '{"pattern":"**/*.rs","path":"crates/atd-ref-server/src"}'` returns paths; `atd call ref:fs.grep --args '{"pattern":"pub fn","path":"crates/atd-ref-server/src"}'` returns line matches.
6. Tag `sp4-ref-server-search` created.

---

## File Structure

```
crates/atd-ref-server/
├── Cargo.toml                         (MODIFY — add 4 deps, Task 1)
├── README.md                          (MODIFY — Task 6)
└── src/
    ├── builtin.rs                     (MODIFY — register 2 new tools, Task 4)
    ├── server.rs                      (MODIFY — test count 6→8, Task 4)
    └── tools/
        └── fs/
            ├── mod.rs                 (MODIFY — add submodules, Tasks 2 + 3)
            ├── glob.rs                (NEW — Task 2, ~220 LOC)
            ├── grep.rs                (NEW — Task 3, ~280 LOC)
            └── (read/write/edit/shared unchanged)
└── tests/
    └── integration.rs                 (MODIFY — update count + add 4 tests, Task 4 + 5)
```

---

## Task 1: Add dependencies

**Files:**
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/Cargo.toml`

This task adds four new direct dependencies. No code changes — following the pattern from SP-3 Task 1 of a pure scaffolding commit that stays buildable.

- [ ] **Step 1.1: Edit `Cargo.toml`**

Edit `/home/nan/proj/atd-mvp/crates/atd-ref-server/Cargo.toml`. In the `[dependencies]` section, append:

```toml
ignore = "0.4"
globset = "0.4"
grep-searcher = "0.1"
grep-regex = "0.1"
```

After the edit, the `[dependencies]` block should look like:

```toml
[dependencies]
atd-types = { path = "../atd-types", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
thiserror = { workspace = true }
ulid = { workspace = true }
clap = { version = "4", features = ["derive"] }
libc = "0.2"
ignore = "0.4"
globset = "0.4"
grep-searcher = "0.1"
grep-regex = "0.1"
```

Keep existing deps untouched. Don't add these to `[workspace.dependencies]` — direct pins match the SP-3 style.

- [ ] **Step 1.2: Build + test regression**

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server
cargo test --workspace --all-targets
```

Expected: build succeeds, Cargo.lock grows with the new crates and their transitives, 212 tests still pass (no code added yet, just deps in the graph).

- [ ] **Step 1.3: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add crates/atd-ref-server/Cargo.toml Cargo.lock
git commit -m "chore(atd-ref-server): add ignore/globset/grep-searcher/grep-regex deps"
```

(Include `Cargo.lock` so the lockfile's new crate entries are checked in.)

---

## Task 2: `tools/fs/glob.rs`

**Files:**
- Create: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/glob.rs`
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/mod.rs`

The `ref:fs.glob` tool: walks a directory (honoring `.gitignore` + skipping hidden files) and returns paths matching a glob.

- [ ] **Step 2.1: Write `glob.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/glob.rs` with this EXACT content:

```rust
//! `ref:fs.glob` — glob pattern → paths, honoring .gitignore + skipping hidden.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

const DEFAULT_MAX_MATCHES: usize = 1000;

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.glob".into(),
        name: "File Glob".into(),
        description: "Find files matching a glob pattern. Walks the tree honoring .gitignore and skipping hidden files/dirs. Returns paths relative to the searched root, lexicographically sorted.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["glob".into()],
            tags: vec!["fs".into(), "search".into(), "glob".into()],
            intent_examples: vec![
                "find all .rs files under src/".into(),
                "list Cargo manifests in the repo".into(),
            ],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern":     { "type": "string", "minLength": 1 },
                "path":        { "type": "string" },
                "max_matches": { "type": "integer", "minimum": 1 }
            },
            "required": ["pattern"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "paths":       { "type": "array", "items": { "type": "string" } },
                "truncated":   { "type": "boolean" },
                "root":        { "type": "string" },
                "duration_ms": { "type": "integer" }
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
            data_sensitivity: Some("directory layout".into()),
        },
        resources: ToolResources {
            timeout_ms: 30_000,
            max_concurrent: 10,
            rate_limit_per_min: None,
            estimated_tokens: Some(300),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Read,
    })
}

pub struct FsGlobTool;

impl FsGlobTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsGlobTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct GlobArgs {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    max_matches: Option<usize>,
}

/// Resolve `path` against `ctx.cwd` and canonicalize.
/// Returns `NOT_A_DIRECTORY` if the result isn't an existing directory.
fn resolve_root(ctx: &CallContext, path: Option<&str>) -> Result<PathBuf, ToolCallError> {
    let raw = match path {
        Some(p) if !p.is_empty() => {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                pb
            } else {
                ctx.cwd.join(pb)
            }
        }
        _ => ctx.cwd.clone(),
    };
    let canonical = std::fs::canonicalize(&raw).map_err(|_| ToolCallError::ExecutionFailed {
        code: "NOT_A_DIRECTORY".into(),
        message: format!("path does not exist: {}", raw.display()),
        retryable: false,
    })?;
    if !canonical.is_dir() {
        return Err(ToolCallError::ExecutionFailed {
            code: "NOT_A_DIRECTORY".into(),
            message: format!("not a directory: {}", canonical.display()),
            retryable: false,
        });
    }
    Ok(canonical)
}

fn build_globset(pattern: &str) -> Result<GlobSet, ToolCallError> {
    let glob = Glob::new(pattern)
        .map_err(|e| ToolCallError::InvalidArgs(format!("invalid glob `{pattern}`: {e}")))?;
    let mut builder = GlobSetBuilder::new();
    builder.add(glob);
    builder
        .build()
        .map_err(|e| ToolCallError::InvalidArgs(format!("glob build failed: {e}")))
}

fn walk_and_collect(
    root: &Path,
    globs: &GlobSet,
    max_matches: usize,
    max_output_bytes: usize,
) -> (Vec<String>, bool) {
    let mut results: Vec<String> = Vec::new();
    let mut byte_budget = max_output_bytes;
    let mut truncated = false;

    for entry in WalkBuilder::new(root).build().flatten() {
        let path = entry.path();
        // Skip the root itself and any directory entries.
        if path == root {
            continue;
        }
        let file_type = entry.file_type();
        if !matches!(file_type, Some(ft) if ft.is_file()) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if !globs.is_match(rel) {
            continue;
        }
        let rel_str = rel.to_string_lossy().into_owned();
        let cost = rel_str.len() + 2; // rough JSON overhead
        if cost > byte_budget {
            truncated = true;
            break;
        }
        byte_budget -= cost;
        results.push(rel_str);
        if results.len() >= max_matches {
            truncated = true;
            break;
        }
    }

    results.sort();
    (results, truncated)
}

impl Tool for FsGlobTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            let args: GlobArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if args.pattern.trim().is_empty() {
                return Err(ToolCallError::InvalidArgs(
                    "pattern is empty or whitespace-only".into(),
                ));
            }
            let max_matches = args.max_matches.unwrap_or(DEFAULT_MAX_MATCHES).max(1);
            let root = resolve_root(ctx, args.path.as_deref())?;
            let globs = build_globset(&args.pattern)?;
            let max_bytes = ctx.max_output_bytes;

            let start = Instant::now();
            let (paths, truncated) = tokio::task::spawn_blocking(move || {
                walk_and_collect(&root, &globs, max_matches, max_bytes)
            })
            .await
            .map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("walker task failed: {e}"),
                retryable: true,
            })?;
            let duration_ms = start.elapsed().as_millis() as u64;

            // Recompute the canonical root for the response.
            let root_str = resolve_root(ctx, args.path.as_deref())?
                .to_string_lossy()
                .into_owned();

            Ok(serde_json::json!({
                "paths": paths,
                "truncated": truncated,
                "root": root_str,
                "duration_ms": duration_ms,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(p: &Path, contents: &str) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, contents).unwrap();
    }

    fn ctx_for(dir: &Path) -> CallContext {
        let mut c = CallContext::for_test();
        c.cwd = dir.to_path_buf();
        c
    }

    #[tokio::test]
    async fn basic_pattern_returns_matching_paths() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.rs"), "");
        write_file(&dir.path().join("b.rs"), "");
        write_file(&dir.path().join("c.txt"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "*.rs"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> =
            serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths, vec!["a.rs".to_string(), "b.rs".to_string()]);
        assert_eq!(r["truncated"], false);
    }

    #[tokio::test]
    async fn recursive_pattern() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("src/main.rs"), "");
        write_file(&dir.path().join("src/lib/util.rs"), "");
        write_file(&dir.path().join("README.md"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "**/*.rs"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> =
            serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.ends_with("main.rs")));
        assert!(paths.iter().any(|p| p.ends_with("util.rs")));
    }

    #[tokio::test]
    async fn gitignore_respected() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join(".gitignore"), "target/\n");
        write_file(&dir.path().join("src/main.rs"), "");
        write_file(&dir.path().join("target/debug/out.rs"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "**/*.rs"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> =
            serde_json::from_value(r["paths"].clone()).unwrap();
        assert!(paths.iter().any(|p| p.ends_with("main.rs")));
        assert!(
            !paths.iter().any(|p| p.contains("target")),
            "target/ should be ignored: {paths:?}"
        );
    }

    #[tokio::test]
    async fn hidden_skipped_by_default() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join(".hidden/foo.rs"), "");
        write_file(&dir.path().join("visible.rs"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "**/*.rs"}), &ctx)
            .await
            .unwrap();
        let paths: Vec<String> =
            serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths, vec!["visible.rs".to_string()]);
    }

    #[tokio::test]
    async fn max_matches_cap_sets_truncated() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..50 {
            write_file(&dir.path().join(format!("f{i:02}.rs")), "");
        }
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "*.rs", "max_matches": 10}),
                &ctx,
            )
            .await
            .unwrap();
        let paths: Vec<String> =
            serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths.len(), 10);
        assert_eq!(r["truncated"], true);
    }

    #[tokio::test]
    async fn path_scoping_honored() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("outside.rs"), "");
        write_file(&dir.path().join("sub/inside.rs"), "");
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "*.rs", "path": "sub"}),
                &ctx,
            )
            .await
            .unwrap();
        let paths: Vec<String> =
            serde_json::from_value(r["paths"].clone()).unwrap();
        assert_eq!(paths, vec!["inside.rs".to_string()]);
    }

    #[tokio::test]
    async fn invalid_glob_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_for(dir.path());
        let t = FsGlobTool::new();
        let err = t
            .call(serde_json::json!({"pattern": "["}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }
}
```

**Notes on implementation choices:**
- `WalkBuilder::new(root).build()` is the serial walker — deterministic ordering matters more than throughput at this scale.
- `entry.path().strip_prefix(root)` gives the relative path; we walk the iterator's `Result<DirEntry>` items via `.flatten()` to silently skip per-entry errors (e.g., a single unreadable file doesn't fail the whole search).
- The byte budget tracking is approximate — it charges `len + 2` per path for JSON overhead (opening and closing quotes). Close enough for the "don't blow the wire" goal.
- We resolve_root twice: once for the walker and once for the response. The canonicalize call is cheap and avoids threading the PathBuf through the spawn_blocking boundary.
- `spawn_blocking` matters: `ignore::Walk` is synchronous and can take seconds on big trees. Running it on a blocking thread keeps the tokio reactor free.

- [ ] **Step 2.2: Update `fs/mod.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/mod.rs` with:

```rust
//! File-I/O tools: ref:fs.read, ref:fs.write, ref:fs.edit, ref:fs.glob.

pub mod edit;
pub mod glob;
pub mod read;
pub mod shared;
pub mod write;
```

(Alphabetical order. `grep` lands in Task 3.)

- [ ] **Step 2.3: Build + test + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib tools::fs::glob    # 7 passed
cargo test --workspace --all-targets                    # 212 + 7 = 219
git add crates/atd-ref-server/src/tools/fs/
git commit -m "feat(atd-ref-server): add ref:fs.glob tool"
```

Expected: 7 tests in `tools::fs::glob` pass. Workspace grows by +7, no regressions.

---

## Task 3: `tools/fs/grep.rs`

**Files:**
- Create: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/grep.rs`
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/mod.rs`

The `ref:fs.grep` tool: regex search across walked files, optional glob filter, binary files skipped, 1-indexed line numbers.

- [ ] **Step 3.1: Write `grep.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/grep.rs` with this EXACT content:

```rust
//! `ref:fs.grep` — regex search across files, honoring .gitignore + skipping hidden/binary.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkMatch};
use ignore::WalkBuilder;

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

const DEFAULT_MAX_MATCHES: usize = 1000;

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:fs.grep".into(),
        name: "File Grep".into(),
        description: "Regex search across files under a root. Honors .gitignore, skips hidden files and binary files. Optional glob filter narrows the walked files. Returns (path, 1-indexed line, line text) triples sorted by path then line.".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "fs".into(),
            actions: vec!["grep".into()],
            tags: vec!["fs".into(), "search".into(), "grep".into(), "regex".into()],
            intent_examples: vec![
                "find all TODO comments in src/".into(),
                "search for `fn foo` in Rust sources".into(),
            ],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern":          { "type": "string", "minLength": 1 },
                "path":             { "type": "string" },
                "glob":             { "type": "string" },
                "case_insensitive": { "type": "boolean" },
                "max_matches":      { "type": "integer", "minimum": 1 }
            },
            "required": ["pattern"]
        }),
        output_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "matches": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "line": { "type": "integer" },
                            "text": { "type": "string" }
                        }
                    }
                },
                "truncated":   { "type": "boolean" },
                "root":        { "type": "string" },
                "duration_ms": { "type": "integer" }
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
            data_sensitivity: Some("file contents (matched lines)".into()),
        },
        resources: ToolResources {
            timeout_ms: 30_000,
            max_concurrent: 10,
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

pub struct FsGrepTool;

impl FsGrepTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsGrepTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct GrepArgs {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    case_insensitive: Option<bool>,
    #[serde(default)]
    max_matches: Option<usize>,
}

#[derive(Clone)]
struct MatchRow {
    path: String,
    line: u64,
    text: String,
}

fn resolve_root(ctx: &CallContext, path: Option<&str>) -> Result<PathBuf, ToolCallError> {
    let raw = match path {
        Some(p) if !p.is_empty() => {
            let pb = PathBuf::from(p);
            if pb.is_absolute() {
                pb
            } else {
                ctx.cwd.join(pb)
            }
        }
        _ => ctx.cwd.clone(),
    };
    let canonical = std::fs::canonicalize(&raw).map_err(|_| ToolCallError::ExecutionFailed {
        code: "NOT_A_DIRECTORY".into(),
        message: format!("path does not exist: {}", raw.display()),
        retryable: false,
    })?;
    if !canonical.is_dir() {
        return Err(ToolCallError::ExecutionFailed {
            code: "NOT_A_DIRECTORY".into(),
            message: format!("not a directory: {}", canonical.display()),
            retryable: false,
        });
    }
    Ok(canonical)
}

fn build_optional_globset(glob: Option<&str>) -> Result<Option<GlobSet>, ToolCallError> {
    match glob {
        None => Ok(None),
        Some(g) if g.is_empty() => Ok(None),
        Some(g) => {
            let glob = Glob::new(g)
                .map_err(|e| ToolCallError::InvalidArgs(format!("invalid glob `{g}`: {e}")))?;
            let mut b = GlobSetBuilder::new();
            b.add(glob);
            let set = b.build().map_err(|e| {
                ToolCallError::InvalidArgs(format!("glob build failed: {e}"))
            })?;
            Ok(Some(set))
        }
    }
}

/// Sink that collects matches from one file, honoring a remaining-match budget.
struct CollectSink<'a> {
    rel_path: String,
    out: &'a mut Vec<MatchRow>,
    /// Budget in MATCH ROWS; decremented as we push.
    remaining: &'a mut usize,
    /// Budget in BYTES; we charge path.len() + text.len() + overhead per row.
    remaining_bytes: &'a mut usize,
    /// Set to true if any limit was hit while in this sink.
    truncated: &'a mut bool,
}

impl<'a> Sink for CollectSink<'a> {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        if *self.remaining == 0 {
            *self.truncated = true;
            return Ok(false);
        }
        let line = mat.line_number().unwrap_or(0);
        let raw = String::from_utf8_lossy(mat.bytes());
        let text = raw.trim_end_matches('\n').trim_end_matches('\r').to_string();
        let cost = self.rel_path.len() + text.len() + 40; // rough JSON overhead
        if cost > *self.remaining_bytes {
            *self.truncated = true;
            return Ok(false);
        }
        *self.remaining_bytes -= cost;
        *self.remaining -= 1;
        self.out.push(MatchRow {
            path: self.rel_path.clone(),
            line,
            text,
        });
        Ok(true)
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_and_search(
    root: &Path,
    matcher: &grep_regex::RegexMatcher,
    glob_filter: Option<&GlobSet>,
    max_matches: usize,
    max_output_bytes: usize,
) -> (Vec<MatchRow>, bool) {
    let mut results: Vec<MatchRow> = Vec::new();
    let mut remaining = max_matches;
    let mut remaining_bytes = max_output_bytes;
    let mut truncated = false;
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .build();

    'walker: for entry in WalkBuilder::new(root).build().flatten() {
        let path = entry.path();
        if path == root {
            continue;
        }
        if !matches!(entry.file_type(), Some(ft) if ft.is_file()) {
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if let Some(g) = glob_filter {
            if !g.is_match(rel) {
                continue;
            }
        }
        let rel_str = rel.to_string_lossy().into_owned();
        let mut sink = CollectSink {
            rel_path: rel_str,
            out: &mut results,
            remaining: &mut remaining,
            remaining_bytes: &mut remaining_bytes,
            truncated: &mut truncated,
        };
        // Per-file search. Individual IO errors are swallowed (don't fail
        // the whole grep for one unreadable file).
        let _ = searcher.search_path(matcher, path, &mut sink);
        if remaining == 0 || truncated {
            break 'walker;
        }
    }

    results.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    (results, truncated)
}

impl Tool for FsGrepTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            let args: GrepArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if args.pattern.trim().is_empty() {
                return Err(ToolCallError::InvalidArgs(
                    "pattern is empty or whitespace-only".into(),
                ));
            }
            let case_insensitive = args.case_insensitive.unwrap_or(false);
            let matcher = RegexMatcherBuilder::new()
                .case_insensitive(case_insensitive)
                .build(&args.pattern)
                .map_err(|e| {
                    ToolCallError::InvalidArgs(format!(
                        "invalid regex `{}`: {e}",
                        args.pattern
                    ))
                })?;
            let glob_set = build_optional_globset(args.glob.as_deref())?;
            let max_matches = args.max_matches.unwrap_or(DEFAULT_MAX_MATCHES).max(1);
            let root = resolve_root(ctx, args.path.as_deref())?;
            let max_bytes = ctx.max_output_bytes;

            let start = Instant::now();
            let root_for_task = root.clone();
            let (rows, truncated) = tokio::task::spawn_blocking(move || {
                walk_and_search(
                    &root_for_task,
                    &matcher,
                    glob_set.as_ref(),
                    max_matches,
                    max_bytes,
                )
            })
            .await
            .map_err(|e| ToolCallError::ExecutionFailed {
                code: "IO".into(),
                message: format!("grep task failed: {e}"),
                retryable: true,
            })?;
            let duration_ms = start.elapsed().as_millis() as u64;

            let matches_json: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|m| {
                    serde_json::json!({
                        "path": m.path,
                        "line": m.line,
                        "text": m.text,
                    })
                })
                .collect();

            Ok(serde_json::json!({
                "matches": matches_json,
                "truncated": truncated,
                "root": root.to_string_lossy(),
                "duration_ms": duration_ms,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(p: &Path, contents: &str) {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, contents).unwrap();
    }

    fn ctx_for(dir: &Path) -> CallContext {
        let mut c = CallContext::for_test();
        c.cwd = dir.to_path_buf();
        c
    }

    #[tokio::test]
    async fn basic_regex_finds_line() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir.path().join("src/main.rs"),
            "use std::io;\nfn foo() {}\nfn main() {}\n",
        );
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "fn\\s+\\w+"}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["path"], "src/main.rs");
        assert_eq!(matches[0]["line"], 2);
        assert_eq!(matches[0]["text"], "fn foo() {}");
        assert_eq!(matches[1]["line"], 3);
    }

    #[tokio::test]
    async fn case_insensitive_flag() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), "Hello\nhello\nworld\n");
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "hello", "case_insensitive": true}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[tokio::test]
    async fn glob_filter_narrows_search() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("main.rs"), "TODO rs\n");
        write_file(&dir.path().join("main.py"), "TODO py\n");
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "TODO", "glob": "*.rs"}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "main.rs");
    }

    #[tokio::test]
    async fn binary_files_skipped() {
        let dir = tempfile::tempdir().unwrap();
        // Construct a file with a NUL byte AND a literal match pattern; grep
        // should skip the whole file due to BinaryDetection::quit.
        let bytes: Vec<u8> = b"text before\x00matches here\n".to_vec();
        fs::write(dir.path().join("data.bin"), &bytes).unwrap();
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "matches"}), &ctx)
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 0, "binary file should be skipped");
    }

    #[tokio::test]
    async fn no_matches_returns_empty_array() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), "hello\n");
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "zzzzzz_not_present"}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert!(matches.is_empty());
        assert_eq!(r["truncated"], false);
    }

    #[tokio::test]
    async fn max_matches_cap_sets_truncated() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20 {
            write_file(
                &dir.path().join(format!("f{i:02}.txt")),
                "TODO 1\nTODO 2\nTODO 3\nTODO 4\nTODO 5\n",
            );
        }
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(
                serde_json::json!({"pattern": "TODO", "max_matches": 10}),
                &ctx,
            )
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 10);
        assert_eq!(r["truncated"], true);
    }

    #[tokio::test]
    async fn line_numbers_are_1_indexed() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.txt"), "hit\nmiss\n");
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let r = t
            .call(serde_json::json!({"pattern": "hit"}), &ctx)
            .await
            .unwrap();
        let matches: Vec<serde_json::Value> =
            serde_json::from_value(r["matches"].clone()).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["line"], 1, "first line is line 1, not line 0");
    }

    #[tokio::test]
    async fn invalid_regex_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx_for(dir.path());
        let t = FsGrepTool::new();
        let err = t
            .call(serde_json::json!({"pattern": "["}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }
}
```

**Notes on implementation choices:**
- `SearcherBuilder::new().binary_detection(BinaryDetection::quit(b'\x00')).build()` makes the searcher abort as soon as it sees a NUL, so we don't leak binary garbage into the JSON.
- The `CollectSink` holds mutable references to the running counters; returning `Ok(false)` from `matched()` stops searching the current file. The outer loop checks `remaining == 0 || truncated` to break out of the walker.
- Per-file IO errors from `search_path` are swallowed via `let _ = ...`. A single unreadable file shouldn't fail the whole grep — same policy as ripgrep's default.
- Sorting at the end gives deterministic output ordering. For small match sets (<1000) this is negligible cost.

- [ ] **Step 3.2: Update `fs/mod.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/fs/mod.rs` with:

```rust
//! File-I/O tools: ref:fs.read, ref:fs.write, ref:fs.edit, ref:fs.glob, ref:fs.grep.

pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod shared;
pub mod write;
```

- [ ] **Step 3.3: Build + test + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib tools::fs::grep    # 8 passed
cargo test --workspace --all-targets                   # 219 + 8 = 227
git add crates/atd-ref-server/src/tools/fs/
git commit -m "feat(atd-ref-server): add ref:fs.grep tool"
```

---

## Task 4: Register in builtin + cascading test updates

**Files:**
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/builtin.rs`
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/server.rs` (test only)
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs` (one existing test only)

Register `FsGlobTool` + `FsGrepTool` in the default registry; update the three count-dependent assertions that would otherwise fail.

- [ ] **Step 4.1: Update `builtin.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/builtin.rs` with:

```rust
//! Built-in tool registration for `atd-ref-server`.
//!
//! To add a new tool:
//! 1. Create `src/tools/<name>.rs` implementing `Tool`.
//! 2. Export it from the appropriate `tools/*/mod.rs`.
//! 3. Add `reg.register(Arc::new(<Name>Tool::new()))` below.

use std::sync::Arc;

use crate::registry::Registry;
use crate::tools::echo::EchoTool;
use crate::tools::fs::{
    edit::FsEditTool, glob::FsGlobTool, grep::FsGrepTool, read::FsReadTool, write::FsWriteTool,
};
use crate::tools::shell::{exec::ShellExecTool, pwsh::ShellPwshTool};

pub fn builtin_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoTool::new()));
    reg.register(Arc::new(FsReadTool::new()));
    reg.register(Arc::new(FsWriteTool::new()));
    reg.register(Arc::new(FsEditTool::new()));
    reg.register(Arc::new(FsGlobTool::new()));
    reg.register(Arc::new(FsGrepTool::new()));
    reg.register(Arc::new(ShellExecTool::new()));
    reg.register(Arc::new(ShellPwshTool::new()));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_tools() {
        let r = builtin_registry();
        assert_eq!(r.count(), 8);
        assert!(r.get("ref:echo.say").is_some());
        assert!(r.get("ref:fs.read").is_some());
        assert!(r.get("ref:fs.write").is_some());
        assert!(r.get("ref:fs.edit").is_some());
        assert!(r.get("ref:fs.glob").is_some());
        assert!(r.get("ref:fs.grep").is_some());
        assert!(r.get("ref:shell.exec").is_some());
        assert!(r.get("ref:shell.pwsh").is_some());
    }
}
```

- [ ] **Step 4.2: Update `server.rs` test assertion**

Open `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/server.rs` and find the test named `tool_list_returns_registered_summaries`. It currently asserts `count == 6` and checks the 6 tools from SP-3. Update it to:

- Change `assert_eq!(... , 6)` to `assert_eq!(... , 8)`
- Add two more ID-presence asserts for `ref:fs.glob` and `ref:fs.grep`

If the existing test body uses a HashSet of ids, add both new ids to the assertion block.

Run `cargo test -p atd-ref-server --lib server::tests::tool_list_returns_registered_summaries` after the edit — it must pass.

- [ ] **Step 4.3: Update `integration.rs` tool-list test**

Open `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs` and find `e2e_tool_list_returns_echo`. The SP-3 form asserted `tools.len() == 6` and checked 6 ids in a HashSet. Update it:

- Change `assert_eq!(tools.len(), 6)` to `assert_eq!(tools.len(), 8)`
- Add `assert!(ids.contains("ref:fs.glob"));`
- Add `assert!(ids.contains("ref:fs.grep"));`

Keep the existing 6 assertions in place.

- [ ] **Step 4.4: Build + test + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib builtin     # 1 test passes (count=8)
cargo test -p atd-ref-server                    # full crate green
cargo test --workspace --all-targets           # ~227 tests, zero failures
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): register fs.glob + fs.grep in builtin"
```

---

## Task 5: New integration tests

**Files:**
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs`

Append 4 new e2e tests exercising the two new tools end-to-end via the Unix socket.

- [ ] **Step 5.1: Append 4 new tests**

At the end of `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs`, append:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_fs_glob_returns_paths() {
    use std::fs;
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.rs"), "").unwrap();
    fs::write(tmp.path().join("b.rs"), "").unwrap();
    fs::write(tmp.path().join("c.txt"), "").unwrap();

    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.glob",
            "args": {
                "pattern": "*.rs",
                "path": tmp.path().to_string_lossy(),
            },
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(true));
    let paths: Vec<String> =
        serde_json::from_value(r["result"]["paths"].clone()).unwrap();
    assert_eq!(paths, vec!["a.rs".to_string(), "b.rs".to_string()]);
    assert_eq!(r["result"]["truncated"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_fs_grep_finds_match() {
    use std::fs;
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("notes.txt"),
        "line one\nTODO fix this\nline three\n",
    )
    .unwrap();

    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.grep",
            "args": {
                "pattern": "TODO",
                "path": tmp.path().to_string_lossy(),
            },
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    let matches: Vec<serde_json::Value> =
        serde_json::from_value(r["result"]["matches"].clone()).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "notes.txt");
    assert_eq!(matches[0]["line"], 2);
    assert_eq!(matches[0]["text"], "TODO fix this");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_fs_grep_with_glob_filter() {
    use std::fs;
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("match.rs"), "TODO rs\n").unwrap();
    fs::write(tmp.path().join("match.py"), "TODO py\n").unwrap();

    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.grep",
            "args": {
                "pattern": "TODO",
                "glob": "*.rs",
                "path": tmp.path().to_string_lossy(),
            },
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["success"], serde_json::json!(true));
    let matches: Vec<serde_json::Value> =
        serde_json::from_value(r["result"]["matches"].clone()).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "match.rs");
    assert_eq!(matches[0]["text"], "TODO rs");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_fs_glob_invalid_pattern_returns_error() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:fs.glob",
            "args": {"pattern": "["},
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    // InvalidArgs maps to a wire `error` response (NOT a tool_result with
    // success=false). This mirrors the SP-2 pattern for fs.read with a bad
    // path argument.
    assert_eq!(r["type"], "error");
    let message = r["message"].as_str().unwrap_or("");
    assert!(message.contains("invalid glob") || message.contains("["));
}
```

**Note:** `tempfile::tempdir()` paths are NOT inside the repo's `.gitignore`, so ignore-file interference shouldn't apply. Still, since `ignore::Walk` looks at `.gitignore` relative to the walked root, and tempdirs don't have one, the walker treats every file as visible by default. Good.

**Note on the InvalidArgs wire shape:** If the ATD wire protocol emits `{"type": "error", "message": "..."}` for `InvalidArgs` (which SP-2 established), the 4th test is correct as written. If it instead emits `{"type": "tool_result", "success": false, "result": {...}}`, adjust the assertions accordingly. Check what the existing `e2e_fs_read_bad_path` or equivalent SP-2 test expects — whichever pattern it uses, match it.

- [ ] **Step 5.2: Run + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --test integration    # 4 new tests → 23 total (19 + 4)
cargo test --workspace --all-targets               # +4 more
git add crates/atd-ref-server/tests/integration.rs
git commit -m "test(atd-ref-server): integration tests for fs.glob + fs.grep"
```

---

## Task 6: README + independence check + tag

**Files:**
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/README.md`

- [ ] **Step 6.1: Update README — mark SP-4 shipped + add search example**

Edit `/home/nan/proj/atd-mvp/crates/atd-ref-server/README.md`. Two changes:

**(a)** Find the "What's shipped and what's next" section. There should already be SP-1/SP-2/SP-3 marked as `(shipped)`. Find the SP-4 bullet (likely `- **SP-4:** ref:fs.glob + ref:fs.grep`). Replace with:

```markdown
- **SP-4 (shipped):** `ref:fs.glob` + `ref:fs.grep` — ripgrep-powered search tools
```

If the list doesn't yet have an SP-4 line, add one in the appropriate spot.

**(b)** Append a new subsection at the end of `## Quick start`, AFTER the "Shell tools" subsection from SP-3 and BEFORE the next top-level heading:

````markdown
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
````

- [ ] **Step 6.2: Independence check**

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

Both must print OK.

- [ ] **Step 6.3: Live smoke**

```bash
cd /home/nan/proj/atd-mvp
cargo build --release -p atd-ref-server --bin atd-ref-server
cargo build --release -p atd-cli --bin atd

./target/release/atd-ref-server --sock /tmp/sp4-smoke.sock &
SRV_PID=$!
sleep 1

# Smoke 1: glob
./target/release/atd --sock /tmp/sp4-smoke.sock call ref:fs.glob \
  --args '{"pattern": "**/*.rs", "path": "crates/atd-ref-server/src"}'

# Smoke 2: grep
./target/release/atd --sock /tmp/sp4-smoke.sock call ref:fs.grep \
  --args '{"pattern": "pub fn", "path": "crates/atd-ref-server/src"}'

kill $SRV_PID
wait $SRV_PID 2>/dev/null
rm -f /tmp/sp4-smoke.sock
```

Expected for smoke 1: array of rel paths including `lib.rs`, `server.rs`, `builtin.rs`, and files under `tools/`.
Expected for smoke 2: match array with `pub fn` hits across the server sources.

If `atd-cli` doesn't support `call` or its wire shape differs, skip the smoke and note it — the 4 integration tests already prove the wire path works.

- [ ] **Step 6.4: Final workspace regression**

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server --release
cargo test --workspace --all-targets
```

Expected: release build zero warnings; all workspace tests pass (~240 total).

- [ ] **Step 6.5: Commit + tag**

```bash
cd /home/nan/proj/atd-mvp
git add crates/atd-ref-server/README.md
git commit -m "docs(atd-ref-server): mark SP-4 shipped and add search quickstart"

git tag -a sp4-ref-server-search -m "SP-4: atd-ref-server search tools (fs.glob + fs.grep)"
git log --oneline | head -14
git tag
```

---

## Post-Plan Verification Checklist

- [ ] `cargo build -p atd-ref-server --release` zero warnings
- [ ] `cargo test -p atd-ref-server` passes (~134 tests — 116 lib + 18 integration)
- [ ] `cargo test --workspace --all-targets` passes ~240 Rust tests
- [ ] `cargo tree` independence check returns empty
- [ ] Live smoke: glob returns rel paths under `crates/atd-ref-server/src`
- [ ] Live smoke: grep returns `pub fn` hits with line numbers
- [ ] README has SP-4 marked shipped + search quickstart
- [ ] Tag `sp4-ref-server-search` created

## What's next after SP-4

- **SP-5:** `ref:web.fetch` via `reqwest` + `html2md` (network-bound, new safety territory — rate limits, redirect caps, size caps)
- **SP-6:** cross-crate E2E rewrite of `hello_atd.{rs,py}` against atd-ref-server (replacing the ANOS server dependency); validation doc with demo video
