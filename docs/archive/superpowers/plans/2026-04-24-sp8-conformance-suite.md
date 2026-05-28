# SP-8 Conformance Suite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a cross-impl conformance suite for the ATD protocol so any ATD-speaking server can be validated for wire + core-behavior equivalence with the reference SDK.

**Architecture:** New workspace member `atd-conformance` — hybrid library (`pub fn run_conformance`) + thin CLI binary (`atd-conformance --target <sock>`). Fixtures are JSON files organized into `wire/`, `sanitize/`, `behavior/` directories, loaded at runtime. The lib is consumable as a dev-dep by any Rust ATD server. Self-conformance is validated via an integration test that spawns `atd-ref-server` and runs the full suite.

**Tech Stack:** Rust 2024, cargo 1.94.1. Depends on `atd-protocol` + `atd-sdk` + `serde` + `serde_json` + `tokio` + `clap`. Dev-dep on `atd-ref-server-bin` (spawn binary via a `ref_server_bin()` helper that derives the path from `std::env::current_exe()`; the plan Task 8 body below sketched `env!("CARGO_BIN_EXE_atd-ref-server")` but that env var only exposes **same-package** binaries — the shipped implementation uses the `current_exe()` pattern in `crates/atd-conformance/tests/atd_mvp_self_conformance.rs::ref_server_bin`) + `tempfile`. No new external dependencies beyond these; they're all already in the workspace.

**Spec:** `docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md`

**Preconditions:** Working tree clean on master; 4-gate green (fmt + clippy + test + build); current HEAD at or past `sp-fmt-clippy-cleanup` (6f93679).

---

## Task 0: Pre-flight baseline

**Files:** No code changes; only a tag.

- [ ] **Step 1: Verify working tree clean**

Run: `git status --short | grep -vE "^\?\?"`
Expected: empty output. Untracked files like `CLAUDE.md`, `claude-code-source`, `docs/whitepaper/*`, `docs/superpowers/plans/2026-04-2{1,2}-*.md` are pre-existing and out-of-scope.

- [ ] **Step 2: Verify 4-gate green**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```
Expected: all four exit 0. Test count baseline: 297.

- [ ] **Step 3: Tag the baseline**

```bash
git tag pre-sp-8-conformance-suite
git log -1 --oneline
```

Expected: tag created on the current HEAD. Rollback path: `git reset --hard pre-sp-8-conformance-suite`.

- [ ] **Step 4: No commit for this task** — tag only.

---

## Task 1: Scaffold `atd-conformance` crate

**Files:**
- Create: `crates/atd-conformance/Cargo.toml`
- Create: `crates/atd-conformance/src/lib.rs`
- Create: `crates/atd-conformance/src/main.rs`
- Create: `crates/atd-conformance/fixtures/wire/` (directory)
- Create: `crates/atd-conformance/fixtures/sanitize/` (directory)
- Create: `crates/atd-conformance/fixtures/behavior/` (directory)
- Modify: `Cargo.toml` (workspace members list — add `"crates/atd-conformance"`)

- [ ] **Step 1: Create `crates/atd-conformance/Cargo.toml`**

```toml
[package]
name = "atd-conformance"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Conformance test suite for the ATD (Agent Tool Dispatch) protocol. Verifies any ATD-speaking server for wire + core-behavior equivalence with the reference SDK."
readme = "README.md"
keywords = ["atd", "conformance", "protocol", "testing", "spec"]
categories = ["development-tools::testing", "api-bindings"]

[lib]
name = "atd_conformance"
path = "src/lib.rs"

[[bin]]
name = "atd-conformance"
path = "src/main.rs"

[dependencies]
atd-protocol = { path = "../atd-protocol", version = "0.1.0" }
atd-sdk = { path = "../atd-sdk", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
clap = { version = "4", features = ["derive"] }

[dev-dependencies]
atd-ref-server-bin = { path = "../atd-ref-server-bin", version = "0.1.0" }
tempfile = { workspace = true }
```

- [ ] **Step 2: Create minimal `crates/atd-conformance/src/lib.rs`**

```rust
//! ATD conformance test suite.
//!
//! Drives a target ATD server through wire-format, sanitize, and
//! behavioral conformance cases loaded from JSON fixtures. Reports
//! pass/fail per case. Implementation-agnostic: any server that
//! speaks ATD over a Unix socket can be validated.
//!
//! See `docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md`
//! for the design.

// Modules are populated in subsequent tasks:
// pub mod case;      (Task 2)
// pub mod runner;    (Tasks 3-5)
// pub mod wire;      (Task 4)
// pub mod report;    (Task 6)

// run_conformance entry is added in Task 7.
```

- [ ] **Step 3: Create minimal `crates/atd-conformance/src/main.rs`**

```rust
//! CLI entry for atd-conformance. Populated in Task 7.

fn main() {
    eprintln!("atd-conformance: not yet implemented (scaffold only)");
    std::process::exit(2);
}
```

- [ ] **Step 4: Create the three fixture directories with `.gitkeep` markers**

```bash
mkdir -p crates/atd-conformance/fixtures/wire
mkdir -p crates/atd-conformance/fixtures/sanitize
mkdir -p crates/atd-conformance/fixtures/behavior
touch crates/atd-conformance/fixtures/wire/.gitkeep
touch crates/atd-conformance/fixtures/sanitize/.gitkeep
touch crates/atd-conformance/fixtures/behavior/.gitkeep
```

The `.gitkeep` markers are temporary — they get deleted as fixtures are added in subsequent tasks. Git needs something to track the directories.

- [ ] **Step 5: Create minimal `crates/atd-conformance/README.md`**

```markdown
# atd-conformance

Cross-implementation conformance suite for the ATD (Agent Tool Dispatch) protocol.

Any server that speaks ATD over a Unix socket can be validated with:

```
atd-conformance --target unix:/path/to/server.sock
```

For the Rust SDK consumer path, depend on this crate as a dev-dep and call
`atd_conformance::run_conformance(opts)` from an integration test.

See the [SP-8 design doc](../../docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md)
for scope, fixture format, and how to contribute new cases.
```

- [ ] **Step 6: Add `"crates/atd-conformance"` to root `Cargo.toml` workspace members**

Edit `/home/nan/proj/atd-mvp/Cargo.toml`. In the `members` list, add `"crates/atd-conformance",` in alphabetical position (between `atd-cli` and `atd-mcp-bridge`). Final list should be 12 members:

```toml
[workspace]
resolver = "2"
members = [
    "crates/atd-protocol",
    "crates/atd-sdk",
    "crates/atd-runtime",
    "crates/atd-tools-echo",
    "crates/atd-tools-fs",
    "crates/atd-tools-shell",
    "crates/atd-tools-web",
    "crates/atd-cli",
    "crates/atd-conformance",
    "crates/atd-mcp-bridge",
    "crates/atd-ref-server-bin",
    "examples",
]
```

- [ ] **Step 7: Verify 4-gate green**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```
Expected: all pass. Test count: still 297 (scaffold has no tests). The new crate compiles but has no public items.

- [ ] **Step 8: Commit**

```bash
git add crates/atd-conformance Cargo.toml
git status --short
git commit -m "feat(atd-conformance): scaffold crate (Task 1)

New workspace member atd-conformance as a 12th crate. Hybrid lib +
thin bin layout. Empty stubs for lib.rs (modules populated by
Tasks 2-6) and main.rs (CLI populated by Task 7). Fixture directories
created with .gitkeep markers (removed as fixtures land).

Deps: atd-protocol + atd-sdk + serde + tokio + clap. Dev-dep on
atd-ref-server-bin for the self-conformance test in Task 8.

Refs: docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md §3"
```

---

## Task 2: `case.rs` — ConformanceCase types + JSON loader

**Files:**
- Create: `crates/atd-conformance/src/case.rs`
- Modify: `crates/atd-conformance/src/lib.rs` (declare `pub mod case;`)

- [ ] **Step 1: Write the case.rs module — types first**

Create `crates/atd-conformance/src/case.rs`:

```rust
//! Types describing a single conformance case, plus the JSON loader.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A single conformance case. Three variants keyed by `category`.
#[derive(Debug, Deserialize)]
#[serde(tag = "category")]
pub enum ConformanceCase {
    #[serde(rename = "wire")]
    Wire(WireCase),
    #[serde(rename = "sanitize")]
    Sanitize(SanitizeCase),
    #[serde(rename = "behavior")]
    Behavior(BehaviorCase),
}

impl ConformanceCase {
    pub fn name(&self) -> &str {
        match self {
            Self::Wire(c) => &c.name,
            Self::Sanitize(c) => &c.name,
            Self::Behavior(c) => &c.name,
        }
    }

    pub fn category(&self) -> Category {
        match self {
            Self::Wire(_) => Category::Wire,
            Self::Sanitize(_) => Category::Sanitize,
            Self::Behavior(_) => Category::Behavior,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Wire(c) => &c.description,
            Self::Sanitize(c) => &c.description,
            Self::Behavior(c) => &c.description,
        }
    }

    pub fn must(&self) -> Must {
        match self {
            Self::Wire(c) => c.must,
            Self::Sanitize(c) => c.must,
            Self::Behavior(c) => c.must,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Wire,
    Sanitize,
    Behavior,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Wire => "wire",
            Self::Sanitize => "sanitize",
            Self::Behavior => "behavior",
        }
    }
}

/// Whether a case is required to pass or is optional.
/// Only `Pass` is used in the v1 suite; `Skip` is reserved for future
/// use when optional-capability distinctions arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Must {
    #[serde(rename = "pass")]
    Pass,
    #[serde(rename = "skip")]
    Skip,
}

fn default_must_pass() -> Must {
    Must::Pass
}

/// Wire-frame round-trip case.
#[derive(Debug, Deserialize)]
pub struct WireCase {
    pub name: String,
    pub description: String,
    #[serde(default = "default_must_pass")]
    pub must: Must,
    /// JSON value matching the `atd_protocol::Request` enum shape.
    pub send: serde_json::Value,
    /// Expected subset of the server's `atd_protocol::Response`.
    /// Deep-subset match: every key in expect must appear in actual.
    #[serde(default)]
    pub expect_response_matches: Option<serde_json::Value>,
    /// Optional raw-byte prefix assertion (hex-encoded), used for
    /// frame-codec correctness (BE u32 length, etc.). Rare.
    #[serde(default)]
    pub expect_wire_bytes_prefix_hex: Option<String>,
    /// Optional Hello handshake to perform before the main send.
    #[serde(default)]
    pub setup: Option<SetupStep>,
}

/// Pure-function sanitize case. Doesn't contact any server.
#[derive(Debug, Deserialize)]
pub struct SanitizeCase {
    pub name: String,
    pub description: String,
    #[serde(default = "default_must_pass")]
    pub must: Must,
    pub input: String,
    pub expect_sanitized: String,
}

/// Behavior case — like Wire but typically with a Hello handshake setup
/// and assertion on semantics like error codes.
#[derive(Debug, Deserialize)]
pub struct BehaviorCase {
    pub name: String,
    pub description: String,
    #[serde(default = "default_must_pass")]
    pub must: Must,
    #[serde(default)]
    pub setup: Option<SetupStep>,
    pub send: serde_json::Value,
    pub expect_response_matches: serde_json::Value,
}

/// Pre-send setup — currently only Hello handshake.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupStep {
    Hello {
        #[serde(default)]
        client_id: Option<String>,
        #[serde(default)]
        requested_capabilities: Vec<String>,
    },
}

/// Error type returned by the loader when a fixture file is malformed.
#[derive(Debug)]
pub struct LoadError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for LoadError {}

/// Load every `.json` fixture under `fixtures_root` recursively.
/// Returns the loaded cases, or a list of per-file errors.
/// Fails fast on the first malformed file — `cases` is always empty on error.
pub fn load_fixtures(fixtures_root: &Path) -> Result<Vec<ConformanceCase>, LoadError> {
    let mut cases = Vec::new();
    load_dir_recursive(fixtures_root, &mut cases)?;
    cases.sort_by(|a, b| a.name().cmp(b.name()));
    Ok(cases)
}

fn load_dir_recursive(dir: &Path, out: &mut Vec<ConformanceCase>) -> Result<(), LoadError> {
    let entries = std::fs::read_dir(dir).map_err(|e| LoadError {
        path: dir.to_path_buf(),
        message: format!("read_dir failed: {}", e),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| LoadError {
            path: dir.to_path_buf(),
            message: format!("read_dir entry failed: {}", e),
        })?;
        let path = entry.path();

        if path.is_dir() {
            load_dir_recursive(&path, out)?;
        } else if path.extension().map(|e| e == "json").unwrap_or(false) {
            let content = std::fs::read_to_string(&path).map_err(|e| LoadError {
                path: path.clone(),
                message: format!("read failed: {}", e),
            })?;
            let case: ConformanceCase = serde_json::from_str(&content).map_err(|e| LoadError {
                path: path.clone(),
                message: format!("JSON parse failed: {}", e),
            })?;
            out.push(case);
        }
        // Non-JSON files (e.g., .gitkeep, README.md) are silently skipped.
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn mk_tempdir_with(cases: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in cases {
            let p = dir.path().join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(content.as_bytes()).unwrap();
        }
        dir
    }

    #[test]
    fn load_empty_dir_returns_empty_vec() {
        let dir = tempfile::tempdir().unwrap();
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 0);
    }

    #[test]
    fn load_sanitize_case_parses() {
        let dir = mk_tempdir_with(&[(
            "sanitize/basic.json",
            r#"{
                "category": "sanitize",
                "name": "basic",
                "description": "basic test",
                "input": "ref:fs.read",
                "expect_sanitized": "ref_fs_read"
            }"#,
        )]);
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 1);
        match &cases[0] {
            ConformanceCase::Sanitize(s) => {
                assert_eq!(s.name, "basic");
                assert_eq!(s.input, "ref:fs.read");
                assert_eq!(s.expect_sanitized, "ref_fs_read");
                assert_eq!(s.must, Must::Pass);
            }
            _ => panic!("expected Sanitize variant"),
        }
    }

    #[test]
    fn load_wire_case_parses() {
        let dir = mk_tempdir_with(&[(
            "wire/ping.json",
            r#"{
                "category": "wire",
                "name": "ping",
                "description": "ping test",
                "send": {"type": "ping"},
                "expect_response_matches": {"type": "pong"}
            }"#,
        )]);
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 1);
        assert!(matches!(cases[0], ConformanceCase::Wire(_)));
    }

    #[test]
    fn load_behavior_case_with_setup_parses() {
        let dir = mk_tempdir_with(&[(
            "behavior/cap_denied.json",
            r#"{
                "category": "behavior",
                "name": "cap_denied",
                "description": "capability denial",
                "setup": {
                    "kind": "hello",
                    "client_id": "test",
                    "requested_capabilities": []
                },
                "send": {"type": "run_tool", "tool_id": "x", "args": {}, "dry_run": false},
                "expect_response_matches": {"type": "error", "code": 1001}
            }"#,
        )]);
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 1);
        match &cases[0] {
            ConformanceCase::Behavior(b) => {
                assert!(b.setup.is_some());
            }
            _ => panic!("expected Behavior variant"),
        }
    }

    #[test]
    fn load_malformed_json_returns_error() {
        let dir = mk_tempdir_with(&[(
            "wire/bad.json",
            r#"{this is not valid json"#,
        )]);
        let err = load_fixtures(dir.path()).unwrap_err();
        assert!(err.message.contains("JSON parse failed"));
    }

    #[test]
    fn load_unknown_category_returns_error() {
        let dir = mk_tempdir_with(&[(
            "wire/weird.json",
            r#"{"category": "unknown", "name": "x", "description": "x"}"#,
        )]);
        let err = load_fixtures(dir.path()).unwrap_err();
        assert!(err.message.contains("JSON parse failed"));
    }

    #[test]
    fn load_recursive_traversal() {
        let dir = mk_tempdir_with(&[
            (
                "wire/a.json",
                r#"{"category": "wire", "name": "a", "description": "a",
                    "send": {"type": "ping"}}"#,
            ),
            (
                "behavior/b.json",
                r#"{"category": "behavior", "name": "b", "description": "b",
                    "send": {"type": "ping"},
                    "expect_response_matches": {"type": "pong"}}"#,
            ),
        ]);
        let cases = load_fixtures(dir.path()).unwrap();
        assert_eq!(cases.len(), 2);
        // Alphabetical sort by name
        assert_eq!(cases[0].name(), "a");
        assert_eq!(cases[1].name(), "b");
    }
}
```

- [ ] **Step 2: Declare the module in `lib.rs`**

Edit `crates/atd-conformance/src/lib.rs`, replacing the module-block comment with:

```rust
//! ATD conformance test suite.
//!
//! Drives a target ATD server through wire-format, sanitize, and
//! behavioral conformance cases loaded from JSON fixtures. Reports
//! pass/fail per case. Implementation-agnostic: any server that
//! speaks ATD over a Unix socket can be validated.
//!
//! See `docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md`
//! for the design.

pub mod case;

// Modules populated in subsequent tasks:
// pub mod runner;   (Tasks 3-5)
// pub mod wire;     (Task 4)
// pub mod report;   (Task 6)

// run_conformance entry added in Task 7.
```

- [ ] **Step 3: Run the case.rs tests**

```bash
cargo test -p atd-conformance --lib case
```

Expected: 7 tests pass:
- `load_empty_dir_returns_empty_vec`
- `load_sanitize_case_parses`
- `load_wire_case_parses`
- `load_behavior_case_with_setup_parses`
- `load_malformed_json_returns_error`
- `load_unknown_category_returns_error`
- `load_recursive_traversal`

- [ ] **Step 4: Run the 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Workspace test count: 297 + 7 = 304.

- [ ] **Step 5: Commit**

```bash
git add crates/atd-conformance
git status --short
git commit -m "feat(atd-conformance): ConformanceCase types + JSON loader (Task 2)

- case.rs: ConformanceCase enum (Wire/Sanitize/Behavior) with serde
  #[serde(tag = \"category\")] tagged union. Each variant carries name,
  description, must (Pass|Skip), plus category-specific payload.
- SetupStep::Hello for pre-send handshake (used by behavior cases).
- load_fixtures: recursive directory walk, fail-fast on malformed JSON.
  Returns a LoadError with file path context.
- 7 unit tests: empty, per-variant parse, malformed, unknown category,
  recursive traversal with alphabetical sort.

Refs: docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md §4"
```

---

## Task 3: `sanitize` category — runner + fixtures

**Files:**
- Create: `crates/atd-conformance/src/runner.rs` (module skeleton + sanitize path)
- Create: 12 fixture files under `crates/atd-conformance/fixtures/sanitize/`
- Delete: `crates/atd-conformance/fixtures/sanitize/.gitkeep`
- Modify: `crates/atd-conformance/src/lib.rs` (declare `pub mod runner;`)

**Sanitize is the simplest category** — pure function over local atd-protocol, no server needed. Start here to prove the plumbing.

- [ ] **Step 1: Create `crates/atd-conformance/src/runner.rs`**

```rust
//! Per-case runner dispatch.
//!
//! Each case category has its own runner path; this module exposes
//! `run_case` which dispatches by category. Higher-level orchestration
//! (loading fixtures, aggregating results) lives in `lib.rs::run_conformance`.

use crate::case::{Category, ConformanceCase, SanitizeCase};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CaseResult {
    pub name: String,
    pub category: Category,
    pub outcome: Outcome,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Pass,
    Fail { reason: String },
    Skip { why: String },
}

impl Outcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, Outcome::Pass)
    }
    pub fn is_fail(&self) -> bool {
        matches!(self, Outcome::Fail { .. })
    }
    pub fn is_skip(&self) -> bool {
        matches!(self, Outcome::Skip { .. })
    }
}

/// Execute a single case. Wire/behavior cases connect to `target`;
/// sanitize cases ignore `target` and run purely locally.
pub async fn run_case(
    case: &ConformanceCase,
    target: &atd_sdk::Endpoint,
) -> CaseResult {
    let name = case.name().to_string();
    let category = case.category();
    let start = Instant::now();

    let outcome = match case {
        ConformanceCase::Sanitize(s) => run_sanitize_case(s),
        // Wire path implemented in Task 4:
        ConformanceCase::Wire(_) => Outcome::Skip {
            why: "wire runner not yet implemented (Task 4)".into(),
        },
        // Behavior path implemented in Task 5:
        ConformanceCase::Behavior(_) => Outcome::Skip {
            why: "behavior runner not yet implemented (Task 5)".into(),
        },
    };

    // `target` is unused for sanitize; silence the warning until wire is added.
    let _ = target;

    CaseResult {
        name,
        category,
        outcome,
        duration: start.elapsed(),
    }
}

fn run_sanitize_case(case: &SanitizeCase) -> Outcome {
    let actual = atd_protocol::sanitize::sanitize_tool_name(&case.input);
    if actual == case.expect_sanitized {
        Outcome::Pass
    } else {
        Outcome::Fail {
            reason: format!(
                "sanitize_tool_name({:?}) = {:?}, expected {:?}",
                case.input, actual, case.expect_sanitized
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case::Must;

    #[tokio::test]
    async fn sanitize_pass() {
        let case = ConformanceCase::Sanitize(SanitizeCase {
            name: "basic".into(),
            description: "x".into(),
            must: Must::Pass,
            input: "ref:fs.read".into(),
            expect_sanitized: "ref_fs_read".into(),
        });
        let target = atd_sdk::Endpoint::unix("/tmp/unused-for-sanitize.sock");
        let r = run_case(&case, &target).await;
        assert!(r.outcome.is_pass(), "unexpected outcome: {:?}", r.outcome);
    }

    #[tokio::test]
    async fn sanitize_fail_reports_mismatch() {
        let case = ConformanceCase::Sanitize(SanitizeCase {
            name: "wrong".into(),
            description: "x".into(),
            must: Must::Pass,
            input: "ref:fs.read".into(),
            expect_sanitized: "definitely_wrong".into(),
        });
        let target = atd_sdk::Endpoint::unix("/tmp/unused-for-sanitize.sock");
        let r = run_case(&case, &target).await;
        match r.outcome {
            Outcome::Fail { reason } => {
                assert!(reason.contains("ref_fs_read"));
                assert!(reason.contains("definitely_wrong"));
            }
            other => panic!("expected Fail, got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Declare the module in `lib.rs`**

Edit `crates/atd-conformance/src/lib.rs`:

```rust
pub mod case;
pub mod runner;

// Modules populated in subsequent tasks:
// pub mod wire;     (Task 4)
// pub mod report;   (Task 6)
```

- [ ] **Step 3: Verify runner unit tests pass**

```bash
cargo test -p atd-conformance --lib runner
```

Expected: 2 tests pass (`sanitize_pass`, `sanitize_fail_reports_mismatch`).

- [ ] **Step 4: Delete the sanitize .gitkeep**

```bash
git rm crates/atd-conformance/fixtures/sanitize/.gitkeep
```

- [ ] **Step 5: Create 12 sanitize fixtures**

Each fixture is a separate file under `crates/atd-conformance/fixtures/sanitize/`. Write them one by one:

**`basic_ref_fs_read.json`:**
```json
{
  "category": "sanitize",
  "name": "basic_ref_fs_read",
  "description": "Tool id 'ref:fs.read' sanitizes to 'ref_fs_read'.",
  "input": "ref:fs.read",
  "expect_sanitized": "ref_fs_read"
}
```

**`basic_anos_search.json`:**
```json
{
  "category": "sanitize",
  "name": "basic_anos_search",
  "description": "Tool id 'anos:search.semantic' sanitizes dots and colons uniformly.",
  "input": "anos:search.semantic",
  "expect_sanitized": "anos_search_semantic"
}
```

**`alphanumeric_passthrough.json`:**
```json
{
  "category": "sanitize",
  "name": "alphanumeric_passthrough",
  "description": "Tool ids already matching [a-zA-Z0-9_] pass through unchanged.",
  "input": "plain_tool_name",
  "expect_sanitized": "plain_tool_name"
}
```

**`digit_prefix_preserved.json`:**
```json
{
  "category": "sanitize",
  "name": "digit_prefix_preserved",
  "description": "Tool ids starting with digits are not remapped; sanitization only touches illegal chars.",
  "input": "v2:tool",
  "expect_sanitized": "v2_tool"
}
```

**`hyphen_becomes_underscore.json`:**
```json
{
  "category": "sanitize",
  "name": "hyphen_becomes_underscore",
  "description": "Hyphens sanitize to underscores to comply with LLM SDK identifier rules.",
  "input": "my-tool",
  "expect_sanitized": "my_tool"
}
```

**`slash_becomes_underscore.json`:**
```json
{
  "category": "sanitize",
  "name": "slash_becomes_underscore",
  "description": "Forward slashes sanitize to underscores.",
  "input": "ns/tool",
  "expect_sanitized": "ns_tool"
}
```

**`multiple_separators_collapse.json`:**
```json
{
  "category": "sanitize",
  "name": "multiple_separators_collapse",
  "description": "Mixed separators all map to underscores; no separator collapsing.",
  "input": "a:b.c/d-e",
  "expect_sanitized": "a_b_c_d_e"
}
```

**`empty_input.json`:**
```json
{
  "category": "sanitize",
  "name": "empty_input",
  "description": "Empty input yields empty output.",
  "input": "",
  "expect_sanitized": ""
}
```

**`single_colon.json`:**
```json
{
  "category": "sanitize",
  "name": "single_colon",
  "description": "A single colon between two segments is a standard case.",
  "input": "a:b",
  "expect_sanitized": "a_b"
}
```

**`trailing_separator.json`:**
```json
{
  "category": "sanitize",
  "name": "trailing_separator",
  "description": "Trailing separator preserves the underscore (no trimming).",
  "input": "tool.",
  "expect_sanitized": "tool_"
}
```

**`leading_separator.json`:**
```json
{
  "category": "sanitize",
  "name": "leading_separator",
  "description": "Leading separator preserves the underscore.",
  "input": ":tool",
  "expect_sanitized": "_tool"
}
```

**`nested_namespace.json`:**
```json
{
  "category": "sanitize",
  "name": "nested_namespace",
  "description": "Deeply-nested namespace with mixed separators.",
  "input": "vendor:product.module:submodule.action",
  "expect_sanitized": "vendor_product_module_submodule_action"
}
```

- [ ] **Step 6: Verify all 12 sanitize fixtures load and pass against the reference implementation**

Write a one-off verifier test. Append to `crates/atd-conformance/src/runner.rs` inside the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn all_sanitize_fixtures_pass_against_reference() {
        let fixtures_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("sanitize");
        let cases = crate::case::load_fixtures(&fixtures_root)
            .expect("load sanitize fixtures");
        assert!(!cases.is_empty(), "no sanitize fixtures found");

        let target = atd_sdk::Endpoint::unix("/tmp/unused-for-sanitize.sock");
        for case in &cases {
            let r = run_case(case, &target).await;
            assert!(
                r.outcome.is_pass(),
                "sanitize case {} failed: {:?}",
                case.name(),
                r.outcome
            );
        }
    }
```

- [ ] **Step 7: Run the new test**

```bash
cargo test -p atd-conformance --lib runner::tests::all_sanitize_fixtures_pass_against_reference
```

Expected: the test passes, iterating over all 12 fixtures. If any fixture's `expect_sanitized` doesn't match `sanitize_tool_name(input)`, the test fails with a clear message — fix the fixture.

- [ ] **Step 8: Run the full 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Workspace test count: 304 + 2 (runner sanitize unit tests) + 1 (all_sanitize_fixtures_pass_against_reference) = 307.

- [ ] **Step 9: Commit**

```bash
git add crates/atd-conformance
git status --short
git commit -m "feat(atd-conformance): sanitize category — runner + 12 fixtures (Task 3)

- runner.rs: CaseResult, Outcome, and run_case dispatch. Sanitize path
  calls atd_protocol::sanitize::sanitize_tool_name and asserts.
- Wire + Behavior paths stubbed as Skip {why: 'not yet implemented'}.
- 12 sanitize fixtures covering: basic ids, alphanumeric passthrough,
  digit prefix, hyphen/slash/colon/dot separators, multiple mixed
  separators, empty input, trailing/leading separators, nested
  namespace.
- all_sanitize_fixtures_pass_against_reference integration test:
  loads all 12 fixtures from fixtures/sanitize/, asserts every case
  passes against atd-protocol's sanitize_tool_name (the reference impl).

Refs: docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md §4.4"
```

---

## Task 4: `wire` category — wire.rs helpers + runner + 10 fixtures

**Files:**
- Create: `crates/atd-conformance/src/wire.rs`
- Modify: `crates/atd-conformance/src/runner.rs` (replace Wire Skip with real dispatch)
- Create: 10 fixture files under `crates/atd-conformance/fixtures/wire/`
- Delete: `crates/atd-conformance/fixtures/wire/.gitkeep`
- Modify: `crates/atd-conformance/src/lib.rs` (declare `pub mod wire;`)

- [ ] **Step 1: Create `crates/atd-conformance/src/wire.rs`**

```rust
//! Thin shim over atd-protocol::wire plus deep-subset JSON matching.

use crate::case::{SetupStep, WireCase};
use crate::runner::Outcome;
use atd_protocol::wire;
use serde_json::Value;
use std::io;
use std::path::Path;
use std::time::Duration;
use tokio::net::UnixStream;

/// Default per-case wire deadline. Cases are expected to complete in
/// well under 1s; this is a protective upper bound.
pub const WIRE_TIMEOUT: Duration = Duration::from_secs(3);

/// Open a new Unix socket connection to `target` and, if `setup` is
/// present, perform its handshake. Returns the open stream ready for
/// the case's main send.
pub async fn open_and_setup(
    target: &Path,
    setup: &Option<SetupStep>,
) -> io::Result<UnixStream> {
    let mut stream = UnixStream::connect(target).await?;
    if let Some(SetupStep::Hello {
        client_id,
        requested_capabilities,
    }) = setup
    {
        let hello = serde_json::json!({
            "type": "hello",
            "client_id": client_id,
            "requested_capabilities": requested_capabilities,
        });
        wire::write_frame(&mut stream, &hello).await?;
        // Drain the hello_ack response; we don't assert on it here — the
        // assertion is about the main send/response pair.
        let _ack: Value = wire::read_frame(&mut stream).await?;
    }
    Ok(stream)
}

/// Run a wire case end-to-end against the target socket.
pub async fn run_wire_case(case: &WireCase, target: &Path) -> Outcome {
    let res = tokio::time::timeout(WIRE_TIMEOUT, async {
        let mut stream = open_and_setup(target, &case.setup).await?;

        // If expect_wire_bytes_prefix_hex is set, capture the serialized
        // frame bytes before writing and assert against them.
        if let Some(hex) = &case.expect_wire_bytes_prefix_hex {
            let body = serde_json::to_vec(&case.send)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let len = u32::try_from(body.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "frame too large")
            })?;
            let mut framed = Vec::with_capacity(4 + body.len());
            framed.extend_from_slice(&len.to_be_bytes());
            framed.extend_from_slice(&body);
            let got_hex = hex::encode(&framed[..hex.len() / 2]);
            if got_hex != hex.to_lowercase() {
                return Ok::<Outcome, io::Error>(Outcome::Fail {
                    reason: format!(
                        "wire-byte prefix mismatch: expected {}, got {}",
                        hex.to_lowercase(),
                        got_hex
                    ),
                });
            }
            // Continue and write the frame so we can read the response.
            use tokio::io::AsyncWriteExt;
            stream.write_all(&framed).await?;
            stream.flush().await?;
        } else {
            wire::write_frame(&mut stream, &case.send).await?;
        }

        let response: Value = wire::read_frame(&mut stream).await?;

        if let Some(expect) = &case.expect_response_matches {
            if let Err(reason) = json_matches_subset(expect, &response) {
                return Ok(Outcome::Fail { reason });
            }
        }
        Ok(Outcome::Pass)
    })
    .await;

    match res {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(io_err)) => Outcome::Fail {
            reason: format!("io error: {}", io_err),
        },
        Err(_elapsed) => Outcome::Fail {
            reason: format!("wire timeout after {:?}", WIRE_TIMEOUT),
        },
    }
}

/// Deep-subset match: every key in `expect` must appear in `actual`
/// with a matching value (recursively). Extra keys in `actual` are
/// allowed. The literal string `"*"` in `expect` matches any value.
///
/// Arrays require length equality and element-wise subset matching.
pub fn json_matches_subset(expect: &Value, actual: &Value) -> Result<(), String> {
    match (expect, actual) {
        (Value::String(s), a) if s == "*" => {
            // Wildcard: any value present passes, but null-vs-missing is
            // still distinguished at the object level; wildcard here
            // means "any value present here".
            if a.is_null() {
                Err("wildcard '*' matched null (null should be explicit)".into())
            } else {
                Ok(())
            }
        }
        (Value::Null, Value::Null) => Ok(()),
        (Value::Bool(a), Value::Bool(b)) if a == b => Ok(()),
        (Value::Number(a), Value::Number(b)) if a == b => Ok(()),
        (Value::String(a), Value::String(b)) if a == b => Ok(()),
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Err(format!(
                    "array length mismatch: expect {}, actual {}",
                    e.len(),
                    a.len()
                ));
            }
            for (i, (ei, ai)) in e.iter().zip(a.iter()).enumerate() {
                json_matches_subset(ei, ai)
                    .map_err(|r| format!("[{}]: {}", i, r))?;
            }
            Ok(())
        }
        (Value::Object(e), Value::Object(a)) => {
            for (key, ev) in e {
                let av = a.get(key).ok_or_else(|| {
                    format!("missing key {:?} in actual", key)
                })?;
                json_matches_subset(ev, av)
                    .map_err(|r| format!("{}: {}", key, r))?;
            }
            Ok(())
        }
        (e, a) => Err(format!("mismatch: expect {}, got {}", e, a)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subset_matches_identical() {
        assert!(json_matches_subset(
            &json!({"type": "pong"}),
            &json!({"type": "pong"})
        ).is_ok());
    }

    #[test]
    fn subset_allows_extra_keys_in_actual() {
        assert!(json_matches_subset(
            &json!({"type": "pong"}),
            &json!({"type": "pong", "extra_field": "ok"})
        ).is_ok());
    }

    #[test]
    fn subset_rejects_missing_key() {
        let err = json_matches_subset(
            &json!({"type": "pong", "required": true}),
            &json!({"type": "pong"}),
        ).unwrap_err();
        assert!(err.contains("missing key"));
    }

    #[test]
    fn subset_rejects_value_mismatch() {
        let err = json_matches_subset(
            &json!({"type": "pong"}),
            &json!({"type": "error"}),
        ).unwrap_err();
        assert!(err.contains("mismatch"));
    }

    #[test]
    fn subset_array_length_enforced() {
        let err = json_matches_subset(
            &json!([1, 2, 3]),
            &json!([1, 2]),
        ).unwrap_err();
        assert!(err.contains("array length mismatch"));
    }

    #[test]
    fn subset_wildcard_matches_any_non_null() {
        assert!(json_matches_subset(
            &json!({"id": "*"}),
            &json!({"id": 42}),
        ).is_ok());
        assert!(json_matches_subset(
            &json!({"id": "*"}),
            &json!({"id": "arbitrary"}),
        ).is_ok());
    }

    #[test]
    fn subset_wildcard_rejects_null() {
        let err = json_matches_subset(
            &json!({"id": "*"}),
            &json!({"id": null}),
        ).unwrap_err();
        assert!(err.contains("wildcard"));
    }

    #[test]
    fn subset_nested() {
        assert!(json_matches_subset(
            &json!({"type": "error", "inner": {"code": 1001}}),
            &json!({"type": "error", "inner": {"code": 1001, "extra": 1}, "x": 2}),
        ).is_ok());
    }
}
```

- [ ] **Step 2: Add `hex` to Cargo.toml dev-only dependency? No — inline the hex encoder.**

Update the `hex::encode` call in `wire.rs::run_wire_case` to use a local function instead. Replace the body of the first `if let Some(hex) = ...` block with:

```rust
            let body = serde_json::to_vec(&case.send)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let len = u32::try_from(body.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "frame too large")
            })?;
            let mut framed = Vec::with_capacity(4 + body.len());
            framed.extend_from_slice(&len.to_be_bytes());
            framed.extend_from_slice(&body);
            let prefix_bytes = hex.len() / 2;
            let got_hex = encode_hex(&framed[..prefix_bytes]);
            if got_hex != hex.to_lowercase() {
                return Ok::<Outcome, io::Error>(Outcome::Fail {
                    reason: format!(
                        "wire-byte prefix mismatch: expected {}, got {}",
                        hex.to_lowercase(),
                        got_hex
                    ),
                });
            }
            use tokio::io::AsyncWriteExt;
            stream.write_all(&framed).await?;
            stream.flush().await?;
```

And add a tiny `encode_hex` helper at the bottom of the module (before `#[cfg(test)]`):

```rust
/// Minimal lowercase hex encoder. Avoids pulling in the `hex` crate
/// for one call site.
fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}
```

- [ ] **Step 3: Declare `pub mod wire;` in lib.rs**

Edit `crates/atd-conformance/src/lib.rs`:

```rust
pub mod case;
pub mod runner;
pub mod wire;

// Modules populated in subsequent tasks:
// pub mod report;   (Task 6)
```

- [ ] **Step 4: Update runner.rs to dispatch wire cases via `wire::run_wire_case`**

Edit `crates/atd-conformance/src/runner.rs`, replacing the Wire arm of `run_case`:

```rust
    let outcome = match case {
        ConformanceCase::Sanitize(s) => run_sanitize_case(s),
        ConformanceCase::Wire(w) => {
            let path = target_to_path(target);
            crate::wire::run_wire_case(w, &path).await
        }
        ConformanceCase::Behavior(_) => Outcome::Skip {
            why: "behavior runner not yet implemented (Task 5)".into(),
        },
    };
```

Add a helper `target_to_path` at the bottom of runner.rs (before `#[cfg(test)]`):

```rust
/// Extract the Unix socket path from an atd_sdk::Endpoint.
/// The conformance suite is Unix-socket-only in v1 (HTTP/stdio not
/// in scope; atd-architecture.md §9.7). Current Endpoint enum has only
/// `UnixSocket`, so this match is exhaustive; if new variants are
/// added upstream, the compiler will force us to decide here.
fn target_to_path(endpoint: &atd_sdk::Endpoint) -> std::path::PathBuf {
    match endpoint {
        atd_sdk::Endpoint::UnixSocket(p) => p.clone(),
    }
}
```

Remove the `let _ = target;` line (target is now used).

- [ ] **Step 5: Delete the wire .gitkeep**

```bash
git rm crates/atd-conformance/fixtures/wire/.gitkeep
```

- [ ] **Step 6: Create 10 wire fixtures**

**`ping_roundtrip.json`:**
```json
{
  "category": "wire",
  "name": "ping_roundtrip",
  "description": "Client sends Request::Ping; server replies Response::Pong.",
  "send": { "type": "ping" },
  "expect_response_matches": { "type": "pong" }
}
```

**`tool_list_shape.json`:**
```json
{
  "category": "wire",
  "name": "tool_list_shape",
  "description": "Request::ToolList returns Response::ToolListResponse with a tools array.",
  "send": { "type": "tool_list" },
  "expect_response_matches": { "type": "tool_list", "tools": "*" }
}
```

**`tool_schema_shape.json`:**
```json
{
  "category": "wire",
  "name": "tool_schema_shape",
  "description": "Request::ToolSchema for a known tool returns ToolSchemaResponse with a schema object.",
  "send": { "type": "tool_schema", "tool_id": "ref:echo.say" },
  "expect_response_matches": { "type": "tool_schema", "schema": "*" }
}
```

**`run_tool_echo_success_shape.json`:**
```json
{
  "category": "wire",
  "name": "run_tool_echo_success_shape",
  "description": "Running ref:echo.say returns a tool_result with success=true.",
  "send": {
    "type": "run_tool",
    "tool_id": "ref:echo.say",
    "args": { "text": "hello" },
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "tool_result",
    "tool_id": "ref:echo.say",
    "success": true,
    "dry_run": false
  }
}
```

**`run_tool_echo_dry_run_shape.json`:**
```json
{
  "category": "wire",
  "name": "run_tool_echo_dry_run_shape",
  "description": "Dry-run on ref:echo.say preserves the dry_run=true flag in the response.",
  "send": {
    "type": "run_tool",
    "tool_id": "ref:echo.say",
    "args": { "text": "hello" },
    "dry_run": true
  },
  "expect_response_matches": {
    "type": "tool_result",
    "tool_id": "ref:echo.say",
    "dry_run": true
  }
}
```

**`hello_handshake_shape.json`:**
```json
{
  "category": "wire",
  "name": "hello_handshake_shape",
  "description": "Hello handshake returns a hello_ack with granted_capabilities array (may be empty).",
  "send": {
    "type": "hello",
    "client_id": "conformance",
    "requested_capabilities": []
  },
  "expect_response_matches": {
    "type": "hello_ack",
    "granted_capabilities": []
  }
}
```

**`hello_no_client_id.json`:**
```json
{
  "category": "wire",
  "name": "hello_no_client_id",
  "description": "Hello with no client_id is accepted (field is optional).",
  "send": {
    "type": "hello",
    "requested_capabilities": []
  },
  "expect_response_matches": {
    "type": "hello_ack",
    "granted_capabilities": []
  }
}
```

**`frame_length_big_endian_u32.json`:**
```json
{
  "category": "wire",
  "name": "frame_length_big_endian_u32",
  "description": "Frame is prefixed by a 4-byte big-endian u32 length. For {\"type\":\"ping\"} (body 16 bytes = 0x10), the expected prefix is 00000010.",
  "send": { "type": "ping" },
  "expect_wire_bytes_prefix_hex": "00000010"
}
```

**`unknown_tool_id_returns_error.json`:**
```json
{
  "category": "wire",
  "name": "unknown_tool_id_returns_error",
  "description": "Calling a tool that the server doesn't register returns a Response::Error (no tool_result).",
  "send": {
    "type": "run_tool",
    "tool_id": "nonexistent:tool.missing",
    "args": {},
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "error"
  }
}
```

**`echo_result_shape_has_echoed_field.json`:**
```json
{
  "category": "wire",
  "name": "echo_result_shape_has_echoed_field",
  "description": "The ref:echo.say tool result's data contains an 'echoed' field reflecting the args.",
  "send": {
    "type": "run_tool",
    "tool_id": "ref:echo.say",
    "args": { "text": "round-trip" },
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "tool_result",
    "success": true,
    "data": { "echoed": { "text": "round-trip" } }
  }
}
```

- [ ] **Step 7: Run the wire.rs unit tests (json_matches_subset)**

```bash
cargo test -p atd-conformance --lib wire::tests
```

Expected: 8 tests pass. These test the subset matcher without any server.

- [ ] **Step 8: Full 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count: 307 + 8 (wire subset matcher) = 315. Wire fixtures aren't yet exercised against a server — that happens in the self-conformance integration test (Task 8).

- [ ] **Step 9: Commit**

```bash
git add crates/atd-conformance
git status --short
git commit -m "feat(atd-conformance): wire category — runner + 10 fixtures (Task 4)

- wire.rs: open_and_setup (new UnixStream + optional Hello), run_wire_case
  (full round-trip with 3s timeout), json_matches_subset (deep-subset
  matcher with '*' wildcard and null-distinction rules).
- runner.rs: dispatch wire cases through wire::run_wire_case; behavior
  still stubbed until Task 5.
- 10 wire fixtures: ping, tool_list/schema/run_tool shapes, dry_run,
  Hello handshake (with + without client_id), frame-codec byte-level
  (BE u32), unknown tool id → error, echo round-trip data shape.
- 8 unit tests for the subset matcher cover: identical, extra keys,
  missing keys, value mismatch, array length, wildcard, null-vs-wildcard,
  nested.

Refs: docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md §4.2, §5.2, §5.3"
```

---

## Task 5: `behavior` category — runner + 10 fixtures

**Files:**
- Modify: `crates/atd-conformance/src/wire.rs` (add `run_behavior_case`)
- Modify: `crates/atd-conformance/src/runner.rs` (replace Behavior Skip with real dispatch)
- Create: 10 fixture files under `crates/atd-conformance/fixtures/behavior/`
- Delete: `crates/atd-conformance/fixtures/behavior/.gitkeep`

- [ ] **Step 1: Add `run_behavior_case` to `wire.rs`**

Inside `crates/atd-conformance/src/wire.rs`, add (after `run_wire_case`, before `json_matches_subset`):

```rust
/// Run a behavior case. Behavior ≈ wire with required
/// expect_response_matches and (typically) a Hello setup.
pub async fn run_behavior_case(case: &crate::case::BehaviorCase, target: &Path) -> Outcome {
    let res = tokio::time::timeout(WIRE_TIMEOUT, async {
        let mut stream = open_and_setup(target, &case.setup).await?;
        wire::write_frame(&mut stream, &case.send).await?;
        let response: Value = wire::read_frame(&mut stream).await?;
        if let Err(reason) = json_matches_subset(&case.expect_response_matches, &response) {
            return Ok::<Outcome, io::Error>(Outcome::Fail { reason });
        }
        Ok(Outcome::Pass)
    })
    .await;

    match res {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(io_err)) => Outcome::Fail {
            reason: format!("io error: {}", io_err),
        },
        Err(_elapsed) => Outcome::Fail {
            reason: format!("behavior timeout after {:?}", WIRE_TIMEOUT),
        },
    }
}
```

- [ ] **Step 2: Update `runner.rs` to dispatch behavior cases**

Replace the Behavior arm in `run_case`:

```rust
        ConformanceCase::Behavior(b) => {
            let path = target_to_path(target);
            crate::wire::run_behavior_case(b, &path).await
        }
```

- [ ] **Step 3: Delete the behavior .gitkeep**

```bash
git rm crates/atd-conformance/fixtures/behavior/.gitkeep
```

- [ ] **Step 4: Create 10 behavior fixtures**

**`capability_denied_returns_code_1001.json`:**
```json
{
  "category": "behavior",
  "name": "capability_denied_returns_code_1001",
  "description": "Calling a tool whose required_capabilities isn't a subset of the granted set returns Error{code:1001, retryable:false}.",
  "setup": {
    "kind": "hello",
    "client_id": "conformance",
    "requested_capabilities": ["conformance.denied"]
  },
  "send": {
    "type": "run_tool",
    "tool_id": "ref:fs.read",
    "args": { "path": "Cargo.toml" },
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "error",
    "code": 1001,
    "retryable": false
  }
}
```

**`hello_granted_subset.json`:**
```json
{
  "category": "behavior",
  "name": "hello_granted_subset",
  "description": "Hello returns granted_capabilities as a subset of what was requested. Requesting 'read' when server grants 'read' yields ['read'].",
  "setup": null,
  "send": {
    "type": "hello",
    "client_id": "conformance",
    "requested_capabilities": ["read"]
  },
  "expect_response_matches": {
    "type": "hello_ack",
    "granted_capabilities": ["read"]
  }
}
```

**`hello_requested_superset_yields_only_granted.json`:**
```json
{
  "category": "behavior",
  "name": "hello_requested_superset_yields_only_granted",
  "description": "Requesting capabilities the server doesn't grant yields only the ones the server actually grants — granted is a subset of requested.",
  "setup": null,
  "send": {
    "type": "hello",
    "client_id": "conformance",
    "requested_capabilities": ["read", "conformance.denied"]
  },
  "expect_response_matches": {
    "type": "hello_ack",
    "granted_capabilities": ["read"]
  }
}
```

**`capability_granted_allows_call.json`:**
```json
{
  "category": "behavior",
  "name": "capability_granted_allows_call",
  "description": "After Hello grants a capability, the tool requiring it runs successfully.",
  "setup": {
    "kind": "hello",
    "client_id": "conformance",
    "requested_capabilities": ["read"]
  },
  "send": {
    "type": "run_tool",
    "tool_id": "ref:fs.read",
    "args": { "path": "Cargo.toml" },
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "tool_result",
    "success": true
  }
}
```

**`unknown_tool_returns_error_not_result.json`:**
```json
{
  "category": "behavior",
  "name": "unknown_tool_returns_error_not_result",
  "description": "Calling a tool id the server doesn't register returns a Response::Error (type=error), NOT a tool_result with success=false.",
  "setup": null,
  "send": {
    "type": "run_tool",
    "tool_id": "completely:unknown.tool",
    "args": {},
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "error"
  }
}
```

**`invalid_args_returns_error_or_failure.json`:**
```json
{
  "category": "behavior",
  "name": "invalid_args_returns_error_or_failure",
  "description": "Calling a tool with invalid args (missing required 'path' for ref:fs.read) returns either a Response::Error OR a tool_result with success=false.",
  "setup": {
    "kind": "hello",
    "client_id": "conformance",
    "requested_capabilities": ["read"]
  },
  "send": {
    "type": "run_tool",
    "tool_id": "ref:fs.read",
    "args": {},
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "error"
  }
}
```

**`tool_list_returns_known_reference_tools.json`:**
```json
{
  "category": "behavior",
  "name": "tool_list_returns_known_reference_tools",
  "description": "Tool list from the reference server includes ref:echo.say. This case is reference-server-specific — third-party servers may skip it or substitute their own known tool.",
  "setup": null,
  "send": { "type": "tool_list" },
  "expect_response_matches": {
    "type": "tool_list",
    "tools": "*"
  }
}
```

**`tool_schema_unknown_returns_error.json`:**
```json
{
  "category": "behavior",
  "name": "tool_schema_unknown_returns_error",
  "description": "Requesting the schema of a tool that doesn't exist returns a Response::Error.",
  "setup": null,
  "send": {
    "type": "tool_schema",
    "tool_id": "completely:unknown.tool"
  },
  "expect_response_matches": {
    "type": "error"
  }
}
```

**`echo_preserves_args_verbatim.json`:**
```json
{
  "category": "behavior",
  "name": "echo_preserves_args_verbatim",
  "description": "ref:echo.say returns the exact args under data.echoed, no transformation (this is the 'test anchor' property of echo).",
  "setup": null,
  "send": {
    "type": "run_tool",
    "tool_id": "ref:echo.say",
    "args": { "text": "unique-canary-value-12345" },
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "tool_result",
    "success": true,
    "data": { "echoed": { "text": "unique-canary-value-12345" } }
  }
}
```

**`dry_run_echo_preserves_dry_run_flag.json`:**
```json
{
  "category": "behavior",
  "name": "dry_run_echo_preserves_dry_run_flag",
  "description": "A dry-run call to ref:echo.say preserves dry_run=true in the response and does not produce a different result shape.",
  "setup": null,
  "send": {
    "type": "run_tool",
    "tool_id": "ref:echo.say",
    "args": { "text": "dry-run-test" },
    "dry_run": true
  },
  "expect_response_matches": {
    "type": "tool_result",
    "tool_id": "ref:echo.say",
    "dry_run": true
  }
}
```

- [ ] **Step 5: Run the 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count: still 315 (no new unit tests; the behavior fixtures are exercised in Task 8's self-conformance test).

- [ ] **Step 6: Commit**

```bash
git add crates/atd-conformance
git status --short
git commit -m "feat(atd-conformance): behavior category — runner + 10 fixtures (Task 5)

- wire.rs: run_behavior_case — new connection, optional Hello handshake,
  send main request, assert expect_response_matches as deep-subset of
  response. Timeout-wrapped like run_wire_case.
- runner.rs: dispatch behavior cases through wire::run_behavior_case.
- 10 behavior fixtures:
  * capability_denied_returns_code_1001 (SP-12 normative error code)
  * hello granted subset / superset (2 cases)
  * capability granted allows call
  * unknown tool → error (not success=false)
  * invalid args → error
  * tool_list shape sanity
  * tool_schema on unknown tool → error
  * echo preserves args verbatim
  * dry_run preserves flag

All 10 fixtures use the opaque 'conformance.denied' capability string
for denial cases (convention defined in spec §7.2); ref-server test
harness does NOT grant this cap, so denied behavior is guaranteed.

Refs: docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md §4.5"
```

---

## Task 6: `report.rs` — Report struct + text/json formatters

**Files:**
- Create: `crates/atd-conformance/src/report.rs`
- Modify: `crates/atd-conformance/src/lib.rs` (declare `pub mod report;`)

- [ ] **Step 1: Create `crates/atd-conformance/src/report.rs`**

```rust
//! Report aggregation and output formatting.

use crate::case::Category;
use crate::runner::{CaseResult, Outcome};
use std::time::Duration;

#[derive(Debug)]
pub struct Report {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cases: Vec<CaseResult>,
    pub total_duration: Duration,
}

impl Report {
    pub fn from_results(cases: Vec<CaseResult>) -> Self {
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut total_duration = Duration::ZERO;
        for c in &cases {
            total_duration += c.duration;
            match &c.outcome {
                Outcome::Pass => passed += 1,
                Outcome::Fail { .. } => failed += 1,
                Outcome::Skip { .. } => skipped += 1,
            }
        }
        Self {
            total: cases.len(),
            passed,
            failed,
            skipped,
            cases,
            total_duration,
        }
    }

    /// Human-readable text format; used by the CLI's default output.
    pub fn to_text(&self, target_display: &str) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let version = env!("CARGO_PKG_VERSION");
        writeln!(&mut out, "atd-conformance {} — target {}", version, target_display).unwrap();
        writeln!(&mut out).unwrap();

        for category in [Category::Wire, Category::Sanitize, Category::Behavior] {
            let in_cat: Vec<&CaseResult> = self.cases.iter()
                .filter(|c| c.category == category)
                .collect();
            if in_cat.is_empty() {
                continue;
            }
            let passed = in_cat.iter().filter(|c| c.outcome.is_pass()).count();
            let failed = in_cat.iter().filter(|c| c.outcome.is_fail()).count();
            let marker = if failed == 0 { "✓" } else { "✗" };
            writeln!(
                &mut out,
                "[{:<9}] ({}/{} {})",
                category.as_str(),
                passed,
                in_cat.len(),
                marker
            ).unwrap();
            for c in in_cat {
                let (mark, suffix) = match &c.outcome {
                    Outcome::Pass => ("✓".to_string(), String::new()),
                    Outcome::Fail { reason } => {
                        ("✗".to_string(), format!("\n      {}", reason))
                    }
                    Outcome::Skip { why } => {
                        ("~".to_string(), format!(" (skip: {})", why))
                    }
                };
                writeln!(
                    &mut out,
                    "  {} {:<45} {}ms{}",
                    mark,
                    c.name,
                    c.duration.as_millis(),
                    suffix
                ).unwrap();
            }
            writeln!(&mut out).unwrap();
        }

        writeln!(
            &mut out,
            "{} cases: {} passed, {} failed, {} skipped  (total {}ms)",
            self.total,
            self.passed,
            self.failed,
            self.skipped,
            self.total_duration.as_millis()
        ).unwrap();

        out
    }

    /// JSON format; used by CI consumers.
    pub fn to_json(&self) -> String {
        let val = serde_json::json!({
            "total": self.total,
            "passed": self.passed,
            "failed": self.failed,
            "skipped": self.skipped,
            "total_duration_ms": self.total_duration.as_millis(),
            "cases": self.cases.iter().map(|c| {
                let outcome = match &c.outcome {
                    Outcome::Pass => serde_json::json!("pass"),
                    Outcome::Fail { reason } => serde_json::json!({
                        "fail": { "reason": reason }
                    }),
                    Outcome::Skip { why } => serde_json::json!({
                        "skip": { "why": why }
                    }),
                };
                serde_json::json!({
                    "name": c.name,
                    "category": c.category.as_str(),
                    "outcome": outcome,
                    "duration_ms": c.duration.as_millis(),
                })
            }).collect::<Vec<_>>(),
        });
        serde_json::to_string_pretty(&val).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_case(name: &str, category: Category, outcome: Outcome, ms: u64) -> CaseResult {
        CaseResult {
            name: name.into(),
            category,
            outcome,
            duration: Duration::from_millis(ms),
        }
    }

    #[test]
    fn from_results_counts() {
        let cases = vec![
            mk_case("a", Category::Wire, Outcome::Pass, 1),
            mk_case("b", Category::Wire, Outcome::Fail { reason: "x".into() }, 2),
            mk_case("c", Category::Sanitize, Outcome::Skip { why: "y".into() }, 0),
        ];
        let r = Report::from_results(cases);
        assert_eq!(r.total, 3);
        assert_eq!(r.passed, 1);
        assert_eq!(r.failed, 1);
        assert_eq!(r.skipped, 1);
    }

    #[test]
    fn text_report_mentions_target_and_counts() {
        let cases = vec![mk_case("a", Category::Wire, Outcome::Pass, 5)];
        let r = Report::from_results(cases);
        let t = r.to_text("unix:/tmp/x.sock");
        assert!(t.contains("unix:/tmp/x.sock"));
        assert!(t.contains("1 cases"));
        assert!(t.contains("1 passed"));
    }

    #[test]
    fn text_report_shows_failure_reason() {
        let cases = vec![mk_case(
            "failing",
            Category::Wire,
            Outcome::Fail { reason: "expected X got Y".into() },
            1,
        )];
        let r = Report::from_results(cases);
        let t = r.to_text("unix:/x");
        assert!(t.contains("✗ failing"));
        assert!(t.contains("expected X got Y"));
    }

    #[test]
    fn json_report_parses_back() {
        let cases = vec![
            mk_case("a", Category::Wire, Outcome::Pass, 1),
            mk_case("b", Category::Behavior, Outcome::Fail { reason: "r".into() }, 2),
        ];
        let r = Report::from_results(cases);
        let j = r.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(parsed["total"], 2);
        assert_eq!(parsed["passed"], 1);
        assert_eq!(parsed["failed"], 1);
        assert_eq!(parsed["cases"][0]["outcome"], "pass");
        assert_eq!(parsed["cases"][1]["outcome"]["fail"]["reason"], "r");
    }
}
```

- [ ] **Step 2: Declare `pub mod report;` in `lib.rs`**

Edit `crates/atd-conformance/src/lib.rs`:

```rust
pub mod case;
pub mod runner;
pub mod wire;
pub mod report;

// run_conformance entry added in Task 7.
```

- [ ] **Step 3: Run the report tests**

```bash
cargo test -p atd-conformance --lib report
```

Expected: 4 tests pass.

- [ ] **Step 4: 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: test count 315 + 4 = 319.

- [ ] **Step 5: Commit**

```bash
git add crates/atd-conformance
git status --short
git commit -m "feat(atd-conformance): report.rs — Report + text/json formatters (Task 6)

- Report struct aggregates per-case results into total/passed/failed/
  skipped counts + total duration.
- Report::to_text: human-readable grouped-by-category output with
  pass/fail markers and failure reasons inline.
- Report::to_json: serde_json output for CI consumers; each case has
  name, category, outcome (tagged), and duration_ms.
- 4 unit tests: counting, text target+counts, text failure reason,
  json round-trip via parse-back.

Refs: docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md §6.1-6.2"
```

---

## Task 7: `lib.rs::run_conformance` + CLI `main.rs`

**Files:**
- Modify: `crates/atd-conformance/src/lib.rs` (add `pub async fn run_conformance`)
- Modify: `crates/atd-conformance/src/main.rs` (implement CLI)

- [ ] **Step 1: Implement `run_conformance` in `lib.rs`**

Replace the current scaffolded `lib.rs` with:

```rust
//! ATD conformance test suite.
//!
//! Drives a target ATD server through wire-format, sanitize, and
//! behavioral conformance cases loaded from JSON fixtures. Reports
//! pass/fail per case. Implementation-agnostic: any server that
//! speaks ATD over a Unix socket can be validated.
//!
//! See `docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md`
//! for the design.

pub mod case;
pub mod report;
pub mod runner;
pub mod wire;

use crate::case::{Category, ConformanceCase};
use crate::report::Report;
use crate::runner::{run_case, CaseResult, Outcome};
use std::path::PathBuf;

/// Options controlling a conformance run.
pub struct Opts {
    /// Target server endpoint. Unix socket only in v1.
    pub target: atd_sdk::Endpoint,
    /// Optional substring filter on case name.
    pub filter: Option<String>,
    /// Only run these categories. Empty Vec = run all.
    pub categories: Vec<Category>,
    /// Stop after the first failing case.
    pub stop_on_first_fail: bool,
    /// Path to the fixtures directory. Default: `fixtures/` relative to
    /// `CARGO_MANIFEST_DIR`. Callers in a consuming-crate test should
    /// pass the path explicitly because `CARGO_MANIFEST_DIR` won't
    /// point here.
    pub fixtures_root: PathBuf,
}

impl Opts {
    /// Construct Opts with fixtures_root defaulted to the atd-conformance
    /// crate's fixtures/ directory. Only valid when called from within
    /// atd-conformance itself (e.g., the CLI binary or unit tests).
    pub fn with_default_fixtures(target: atd_sdk::Endpoint) -> Self {
        Self {
            target,
            filter: None,
            categories: Vec::new(),
            stop_on_first_fail: false,
            fixtures_root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures"),
        }
    }
}

/// Run the full suite against the target. Returns a Report.
///
/// Loader errors (malformed JSON) are surfaced as a single synthetic
/// "loader" case with Outcome::Fail. This keeps the Report type
/// simple — callers should still check `report.failed == 0`.
pub async fn run_conformance(opts: Opts) -> Report {
    let cases = match case::load_fixtures(&opts.fixtures_root) {
        Ok(c) => c,
        Err(e) => {
            let loader_fail = CaseResult {
                name: "_fixture_loader".into(),
                category: Category::Wire,
                outcome: Outcome::Fail {
                    reason: format!("fixture loader failed: {}", e),
                },
                duration: std::time::Duration::ZERO,
            };
            return Report::from_results(vec![loader_fail]);
        }
    };

    let mut results = Vec::with_capacity(cases.len());
    for case in &cases {
        if let Some(skip_reason) = should_skip(case, &opts) {
            results.push(CaseResult {
                name: case.name().to_string(),
                category: case.category(),
                outcome: Outcome::Skip { why: skip_reason },
                duration: std::time::Duration::ZERO,
            });
            continue;
        }

        let r = run_case(case, &opts.target).await;

        let should_stop = opts.stop_on_first_fail && r.outcome.is_fail();
        results.push(r);
        if should_stop {
            break;
        }
    }

    Report::from_results(results)
}

fn should_skip(case: &ConformanceCase, opts: &Opts) -> Option<String> {
    if !opts.categories.is_empty() && !opts.categories.contains(&case.category()) {
        return Some(format!("category filter excludes {}", case.category().as_str()));
    }
    if let Some(filter) = &opts.filter {
        if !case.name().contains(filter.as_str()) {
            return Some(format!("name filter {:?} does not match", filter));
        }
    }
    None
}
```

- [ ] **Step 2: Implement CLI in `main.rs`**

Replace `crates/atd-conformance/src/main.rs`:

```rust
//! atd-conformance CLI — runs the conformance suite against a target
//! ATD server over a Unix socket.

use atd_conformance::case::Category;
use atd_conformance::{run_conformance, Opts};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "atd-conformance", version, about = "ATD protocol conformance suite")]
struct Args {
    /// Target server endpoint. Example: `unix:/tmp/atd.sock`.
    #[arg(long)]
    target: String,

    /// Substring filter on case name.
    #[arg(long)]
    filter: Option<String>,

    /// Restrict to one or more categories. Repeatable. Default: all.
    #[arg(long, value_enum)]
    category: Vec<CategoryArg>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = ReportFormat::Text)]
    report: ReportFormat,

    /// Exit on first failure.
    #[arg(long)]
    stop_on_first_fail: bool,

    /// Override fixtures directory. Defaults to the bundled fixtures.
    #[arg(long)]
    fixtures_root: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CategoryArg {
    Wire,
    Sanitize,
    Behavior,
}

impl From<CategoryArg> for Category {
    fn from(c: CategoryArg) -> Self {
        match c {
            CategoryArg::Wire => Category::Wire,
            CategoryArg::Sanitize => Category::Sanitize,
            CategoryArg::Behavior => Category::Behavior,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ReportFormat {
    Text,
    Json,
}

fn parse_target(s: &str) -> Result<atd_sdk::Endpoint, String> {
    let s = s.strip_prefix("unix:").unwrap_or(s);
    Ok(atd_sdk::Endpoint::unix(s))
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let target = match parse_target(&args.target) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("atd-conformance: invalid --target: {}", e);
            std::process::exit(2);
        }
    };
    let target_display = args.target.clone();

    let fixtures_root = args.fixtures_root.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    });

    let opts = Opts {
        target,
        filter: args.filter,
        categories: args.category.into_iter().map(Into::into).collect(),
        stop_on_first_fail: args.stop_on_first_fail,
        fixtures_root,
    };

    let report = run_conformance(opts).await;

    match args.report {
        ReportFormat::Text => {
            print!("{}", report.to_text(&target_display));
        }
        ReportFormat::Json => {
            println!("{}", report.to_json());
        }
    }

    if report.failed > 0 {
        std::process::exit(1);
    }
}
```

- [ ] **Step 3: 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count: 319 (no new unit tests in this task).

- [ ] **Step 4: Smoke-test the CLI locally**

In one terminal, run the ref-server:
```bash
./target/release/atd-ref-server \
    --sock /tmp/conformance-smoke.sock \
    --grant-capability read \
    --grant-capability write \
    --grant-capability exec &
sleep 1
```

In another:
```bash
./target/release/atd-conformance --target unix:/tmp/conformance-smoke.sock
```

Expected: text report with ~30 cases — all sanitize + wire + behavior pass. Sample output:
```
atd-conformance 0.1.0 — target unix:/tmp/conformance-smoke.sock

[wire]      (10/10 ✓)
  ✓ echo_result_shape_has_echoed_field       ...
  ...

30 cases: 30 passed, 0 failed, 0 skipped  (total ...ms)
```

Cleanup:
```bash
pkill -f 'atd-ref-server --sock /tmp/conformance-smoke'
rm -f /tmp/conformance-smoke.sock
```

- [ ] **Step 5: Also test `--report json`**

```bash
./target/release/atd-ref-server --sock /tmp/conformance-smoke.sock \
    --grant-capability read --grant-capability write --grant-capability exec &
sleep 1
./target/release/atd-conformance --target unix:/tmp/conformance-smoke.sock --report json \
  | python3 -c "import sys, json; d = json.load(sys.stdin); print(f'total={d[\"total\"]}, passed={d[\"passed\"]}, failed={d[\"failed\"]}')"
pkill -f 'atd-ref-server --sock /tmp/conformance-smoke'
rm -f /tmp/conformance-smoke.sock
```

Expected: `total=30, passed=30, failed=0` (or similar).

If any case fails: inspect the text output for details; it's most likely a fixture that doesn't match the reference server's actual response shape. Fix the fixture.

- [ ] **Step 6: Commit**

```bash
git add crates/atd-conformance
git status --short
git commit -m "feat(atd-conformance): run_conformance API + CLI binary (Task 7)

- lib.rs: pub async fn run_conformance(opts: Opts) -> Report. Opts
  carries target endpoint, filters (name substring + category), a
  stop_on_first_fail switch, and fixtures_root path. Loader errors
  surface as a synthetic '_fixture_loader' Fail case.
- main.rs: clap-derive CLI. Flags: --target (unix: prefix optional),
  --filter, --category (repeatable), --report {text|json}, --stop-on-first-fail,
  --fixtures-root. Exit 0 on all-pass, 1 on any fail, 2 on invalid
  target. Default fixtures_root is \$CARGO_MANIFEST_DIR/fixtures.

Smoke-tested both CLI modes against a running atd-ref-server: 30
cases, all pass.

Refs: docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md §5.1, §6"
```

---

## Task 8: Self-conformance integration test

**Files:**
- Create: `crates/atd-conformance/tests/atd_mvp_self_conformance.rs`

- [ ] **Step 1: Create the integration test**

```rust
//! Spawns atd-ref-server-bin and runs the full conformance suite
//! against it. If the reference server drifts from the spec, this
//! test fails on the next PR's `cargo test --workspace`.

use atd_conformance::{run_conformance, Opts};
use atd_conformance::runner::Outcome;
use atd_sdk::Endpoint;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atd_ref_server_passes_conformance_suite() {
    let sock_dir = tempfile::tempdir().expect("create tempdir");
    let sock = sock_dir.path().join("conformance.sock");

    let bin = env!("CARGO_BIN_EXE_atd-ref-server");
    let mut child = spawn_server(bin, &sock);

    if let Err(e) = wait_for_socket(&sock, Duration::from_secs(5)).await {
        let _ = child.kill();
        let _ = child.wait();
        panic!("server socket did not appear: {}", e);
    }

    let opts = Opts {
        target: Endpoint::unix(&sock),
        filter: None,
        categories: Vec::new(),
        stop_on_first_fail: false,
        fixtures_root: fixtures_root(),
    };

    let report = run_conformance(opts).await;

    // Clean shutdown
    let _ = child.kill();
    let _ = child.wait();

    if report.failed > 0 {
        let failures: Vec<String> = report.cases.iter()
            .filter_map(|c| match &c.outcome {
                Outcome::Fail { reason } => {
                    Some(format!("  [{}] {}: {}", c.category.as_str(), c.name, reason))
                }
                _ => None,
            })
            .collect();
        panic!(
            "{}/{} conformance case(s) failed:\n{}\n\n\
             (total: {} passed, {} failed, {} skipped)",
            report.failed, report.total,
            failures.join("\n"),
            report.passed, report.failed, report.skipped
        );
    }

    assert!(
        report.total >= 28 && report.total <= 35,
        "expected ~28-32 cases, got {} (design spec §4.7)",
        report.total
    );
    assert_eq!(report.failed, 0, "all cases must pass");
    assert!(
        report.passed >= 28,
        "expected at least 28 passing cases, got {}",
        report.passed
    );
}

fn spawn_server(bin: &str, sock: &std::path::Path) -> Child {
    Command::new(bin)
        .arg("--sock").arg(sock)
        .arg("--grant-capability").arg("read")
        .arg("--grant-capability").arg("write")
        .arg("--grant-capability").arg("exec")
        // Suppress the server's startup log so the test output isn't
        // polluted. On a failure, panic! will include the conformance
        // failures themselves.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn atd-ref-server binary")
}

async fn wait_for_socket(path: &std::path::Path, timeout: Duration) -> Result<(), String> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("socket {:?} did not appear within {:?}", path, timeout))
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}
```

- [ ] **Step 2: Run the new test**

```bash
cargo test -p atd-conformance --test atd_mvp_self_conformance
```

Expected: the test passes. The ref-server starts, all 30 fixtures are driven through it, and every case reports Pass.

If any case fails: the panic message lists exactly which case(s) failed and why. Fix fixtures to match actual server behavior (or fix the server if the fixture is right and the server is wrong — the latter triggers a server bug SP, not a fixture change).

- [ ] **Step 3: Full 4-gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Workspace test count: 319 + 1 = 320.

- [ ] **Step 4: Commit**

```bash
git add crates/atd-conformance
git status --short
git commit -m "feat(atd-conformance): self-conformance integration test (Task 8)

- tests/atd_mvp_self_conformance.rs: spawns atd-ref-server (via
  CARGO_BIN_EXE_atd-ref-server) with read/write/exec capabilities
  granted, waits for its Unix socket to appear, then runs
  run_conformance against it. Asserts report.failed == 0 and total
  cases in 28-35 range.
- Picked up automatically by 'cargo test --workspace --all-targets'
  in CI — no .github/workflows/ci.yml changes required.
- If the reference server ever drifts from the spec, this test fails
  on the next PR.

Refs: docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md §7"
```

---

## Task 9: Post-flight + milestone tag

**Files:** No code changes.

- [ ] **Step 1: Full 4-gate on HEAD**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

Expected: all pass. Test count ≈ 320 (base 297 + 7 case.rs + 2 runner.rs + 1 all_sanitize + 8 wire subset + 4 report + 1 self-conformance = 320).

- [ ] **Step 2: Run the CLI end-to-end one more time against ref-server**

```bash
./target/release/atd-ref-server --sock /tmp/conformance-final.sock \
    --grant-capability read --grant-capability write --grant-capability exec &
sleep 1
./target/release/atd-conformance --target unix:/tmp/conformance-final.sock
pkill -f 'atd-ref-server --sock /tmp/conformance-final'
rm -f /tmp/conformance-final.sock
```

Expected: all ~30 cases pass.

- [ ] **Step 3: Verify `cargo publish --dry-run` metadata**

```bash
cargo publish --dry-run --registry crates-io -p atd-conformance 2>&1 | tail -10
```

Expected: packaging succeeds. The dry-run will fail at the upload step because `atd-protocol` + `atd-sdk` aren't on crates.io yet — that's the chicken-and-egg from SP-refactor-v1's post-flight, not a metadata issue.

- [ ] **Step 4: Inspect commit history**

```bash
git log --oneline pre-sp-8-conformance-suite..HEAD
```

Expected: exactly 8 commits (Tasks 1 through 8; Task 0 and Task 9 are tag-only).

- [ ] **Step 5: Tag the milestone**

```bash
git tag sp-8-conformance-suite
git log --oneline pre-sp-8-conformance-suite..sp-8-conformance-suite
```

Expected: 8 commits listed between the tags.

- [ ] **Step 6: No commit for this task** — tag only.

---

## Self-review checklist (fill in after executing)

- [ ] All 8 commits (Tasks 1-8) independently pass the 4-gate at HEAD.
- [ ] `cargo test --workspace --all-targets` passes at ≈ 320 tests.
- [ ] `cargo run -p atd-conformance -- --target unix:<sock>` against ref-server yields all-pass for ~30 cases.
- [ ] `cargo test -p atd-conformance --test atd_mvp_self_conformance` passes.
- [ ] Total case count in 28-32 range (12 sanitize + 10 wire + 10 behavior = 32; acceptable).
- [ ] Fixture loader fails fast on malformed JSON (verified by `load_malformed_json_returns_error` unit test in Task 2).
- [ ] Both `--report text` and `--report json` work.
- [ ] Zero changes to `atd-protocol`, `atd-sdk`, `atd-runtime`, `atd-tools-*`, or `atd-ref-server-bin` public API.
- [ ] Zero changes to `.github/workflows/ci.yml`.
- [ ] Tags: `pre-sp-8-conformance-suite` at baseline; `sp-8-conformance-suite` at completion.
