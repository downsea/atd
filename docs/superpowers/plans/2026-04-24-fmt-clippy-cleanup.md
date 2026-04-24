# fmt + clippy Baseline Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean all pre-existing `cargo fmt` and `cargo clippy -- -D warnings` failures and lock the result via CI, so future work isn't penalized by a red baseline.

**Architecture:** Three bisect-clean commits: (C1) `cargo fmt --all` across 56 `.rs` files, (C2) four surgical clippy fixes (manual_find, should_implement_trait, 2× manual_clamp, redundant_guards), (C3) add `components: rustfmt, clippy` + two new CI steps to `.github/workflows/ci.yml`.

**Tech Stack:** Rust 2024, cargo 1.94.1, rustfmt 1.9.0, clippy 0.1.95. No new dependencies. No behavior change.

**Spec:** `docs/superpowers/specs/2026-04-24-fmt-clippy-cleanup-design.md`

**Preconditions:** Working tree clean on `master` (or target branch). Current HEAD is at or past `sp-refactor-v1`. `cargo test --workspace --all-targets` is green (297 tests).

---

## Task 0: Pre-flight baseline

**Files:** No code changes; only a tag.

- [ ] **Step 1: Verify working tree clean (ignoring known untracked files)**

Run: `git status --short`
Expected: no tracked changes. Untracked files like `CLAUDE.md`, `claude-code-source`, `docs/whitepaper/*`, or `docs/superpowers/plans/2026-04-2{1,2}-*.md` are pre-existing out-of-scope items; leave them alone.

- [ ] **Step 2: Verify starting gate (3-gate)**

Run:
```bash
cargo check --workspace --all-features
cargo test --workspace --all-targets
cargo build --release --workspace
```
Expected: all three exit 0; test count 297.

- [ ] **Step 3: Snapshot the failing fmt + clippy baseline for later diffing**

Run:
```bash
cargo fmt --all -- --check 2>&1 | grep -c "^Diff in" | xargs -I{} echo "fmt diff hunks: {}"
cargo clippy --workspace --all-features --no-deps 2>&1 | grep -c "^warning:" | xargs -I{} echo "clippy warnings: {}"
```
Expected: approximately `fmt diff hunks: 223` and `clippy warnings: 8` (clippy prints each warning twice — a summary and a per-crate "generated 1 warning" — so raw line count will be ≥4, typically 8). The exact numbers are informational, not gates.

- [ ] **Step 4: Tag the baseline**

```bash
git tag pre-fmt-clippy-cleanup
git log -1 --oneline
```
Expected: tag created on current HEAD. If anything goes wrong mid-SP: `git reset --hard pre-fmt-clippy-cleanup`.

- [ ] **Step 5: No commit for this task** — tag only.

---

## Task 1 (C1): Run `cargo fmt --all`

**Files:**
- Modify: 56 `.rs` files across the workspace (`crates/*/src/**/*.rs`, `crates/*/tests/**/*.rs`, `examples/*.rs`). Exact file list is whatever `cargo fmt --all -- --check` reports; rustfmt is the sole authority.

**Why large:** A workspace that has never been `cargo fmt`-ed produces ~223 hunks of re-formatting. All changes are whitespace / line-break / argument-wrapping; no logic changes.

- [ ] **Step 1: Apply cargo fmt across the workspace**

Run: `cargo fmt --all`
Expected: completes silently (no stdout) with exit 0. Working tree now has ~56 modified files.

- [ ] **Step 2: Verify fmt is clean**

Run: `cargo fmt --all -- --check`
Expected: exit 0, no stdout.

- [ ] **Step 3: Verify no unexpected files changed**

Run: `git status --short`
Expected: the changed files are all `.rs` files under `crates/` or `examples/`. No `.toml`, no `.md`, no CI files. If any non-`.rs` file appears in the diff, STOP and investigate — rustfmt should not touch those.

Also confirm no NEW tracked files appeared:
```bash
git status --short | awk '{print $1}' | sort | uniq -c
```
Expected: only `M` (modify) lines in the count; no `A` (add), `D` (delete), or `R` (rename).

- [ ] **Step 4: Run 3-gate**

```bash
cargo check --workspace --all-features
cargo test --workspace --all-targets
cargo build --release --workspace
```
Expected: all pass; test count still 297.

- [ ] **Step 5: Verify clippy count is unchanged**

Run: `cargo clippy --workspace --all-features --no-deps 2>&1 | grep "^warning:" | grep -v "generated" | wc -l`
Expected: `4` — same four warnings as baseline. `cargo fmt` should not affect clippy output; this is a sanity check.

- [ ] **Step 6: Commit**

```bash
git add -A
git status --short  # sanity: only .rs files in crates/ and examples/
git commit -m "style: cargo fmt --all (workspace-wide C1)

Apply cargo fmt (rustfmt 1.9.0, 2024 edition) across all workspace
crates + examples/. 56 .rs files, 223 hunks — whitespace and
argument-grouping only, zero behavior change. First-ever fmt of the
workspace; closes part 1 of the fmt+clippy baseline cleanup.

Refs: docs/superpowers/specs/2026-04-24-fmt-clippy-cleanup-design.md §4.1"
```

---

## Task 2 (C2): Fix 4 clippy warnings

**Files:**
- Modify: `crates/atd-protocol/src/sanitize.rs` (lines 35-45, `desanitize_tool_name`)
- Modify: `crates/atd-runtime/src/capability.rs` (lines 20-29, `CapabilitySet::from_iter` → `impl FromIterator`)
- Modify: `crates/atd-tools-web/src/fetch.rs` (lines 356-367, two `.min/.max` → `.clamp` chains)
- Modify: `crates/atd-tools-fs/src/grep.rs` (line 160, match guard collapse)

Each fix is surgical (1-5 lines). No test changes required; existing tests cover the behavior.

### 2.1 `atd-protocol/src/sanitize.rs` — `manual_find`

- [ ] **Step 1: Read the current function**

Run: `sed -n '30,46p' crates/atd-protocol/src/sanitize.rs`
Verify you see the `for id in known { ... return Some(id); } None` imperative loop at lines 39-44.

- [ ] **Step 2: Replace the loop with `Iterator::find`**

In `crates/atd-protocol/src/sanitize.rs`, replace:

```rust
pub fn desanitize_tool_name<'a, I>(sanitized: &str, known: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    for id in known {
        if sanitize_tool_name(id) == sanitized {
            return Some(id);
        }
    }
    None
}
```

with:

```rust
pub fn desanitize_tool_name<'a, I>(sanitized: &str, known: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    known
        .into_iter()
        .find(|id| sanitize_tool_name(id) == sanitized)
}
```

- [ ] **Step 3: Verify the function still compiles**

Run: `cargo check -p atd-protocol --all-features`
Expected: exit 0, no warnings related to `desanitize_tool_name`.

- [ ] **Step 4: Verify sanitize tests pass**

Run: `cargo test -p atd-protocol --lib sanitize`
Expected: all pass. `cargo fmt --check` should still be clean (C1 already formatted this file; your edit should preserve formatting).

### 2.2 `atd-runtime/src/capability.rs` — `should_implement_trait`

- [ ] **Step 5: Read the current impl**

Run: `sed -n '18,40p' crates/atd-runtime/src/capability.rs`
Verify you see `impl CapabilitySet { pub fn empty(); pub fn from_iter(...); pub fn contains(...); pub fn granted(); ... }`.

- [ ] **Step 6: Remove the inherent `from_iter` and add `impl FromIterator<String>`**

In `crates/atd-runtime/src/capability.rs`, find:

```rust
impl CapabilitySet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self {
            granted: iter.into_iter().collect(),
        }
    }

    pub fn contains(&self, cap: &str) -> bool {
```

Remove the `pub fn from_iter` method (7 lines: signature + body + blank line after), leaving:

```rust
impl CapabilitySet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn contains(&self, cap: &str) -> bool {
```

Then add a new `impl FromIterator<String> for CapabilitySet` block. Place it immediately after the closing brace of `impl CapabilitySet {}` block (i.e., as a new impl at the top-level of the file, not nested):

```rust
impl FromIterator<String> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self {
            granted: iter.into_iter().collect(),
        }
    }
}
```

- [ ] **Step 7: Verify all 7 call sites still compile unchanged**

```bash
grep -rn "CapabilitySet::from_iter\|CapabilitySet::from_iter" crates/ examples/ | head
```
Expected: 7 hits in `crates/atd-runtime/src/capability.rs` (tests) and `crates/atd-ref-server-bin/src/server.rs`. These use the UFCS syntax `CapabilitySet::from_iter(iter)` which resolves to the trait method after the change.

Run: `cargo check --workspace --all-features`
Expected: clean.

- [ ] **Step 8: Verify tests pass (especially the capability tests)**

Run: `cargo test -p atd-runtime --lib capability`
Expected: all pass.

### 2.3 `atd-tools-web/src/fetch.rs` — two `manual_clamp` sites

- [ ] **Step 9: Read the current clamp logic**

Run: `sed -n '355,370p' crates/atd-tools-web/src/fetch.rs`
Verify you see two `.unwrap_or(...).min(...).max(1)` chains — one for `max_bytes`, one for `timeout_ms`.

- [ ] **Step 10: Replace both with `.clamp(1, MAX)`**

In `crates/atd-tools-web/src/fetch.rs`, replace:

```rust
            let max_bytes = args
                .max_bytes
                .unwrap_or(DEFAULT_MAX_BYTES)
                .min(ctx.max_output_bytes)
                .max(1);
            let timeout_ms = args
                .timeout_ms
                .unwrap_or(DEFAULT_TIMEOUT_MS)
                .min(MAX_TIMEOUT_MS)
                .max(1);
```

with:

```rust
            let max_bytes = args
                .max_bytes
                .unwrap_or(DEFAULT_MAX_BYTES)
                .clamp(1, ctx.max_output_bytes);
            let timeout_ms = args
                .timeout_ms
                .unwrap_or(DEFAULT_TIMEOUT_MS)
                .clamp(1, MAX_TIMEOUT_MS);
```

Note: `clamp(min, max)` requires `min ≤ max`. `ctx.max_output_bytes` is ≥ 1 by server-config convention; `MAX_TIMEOUT_MS` is a positive constant (120_000 per the existing code). Both pre-conditions are satisfied; this is why the `.min().max(1)` chain was safe in the first place.

- [ ] **Step 11: Verify fetch tests pass**

Run: `cargo test -p atd-tools-web`
Expected: all pass.

### 2.4 `atd-tools-fs/src/grep.rs` — `redundant_guards`

- [ ] **Step 12: Read the current match**

Run: `sed -n '155,175p' crates/atd-tools-fs/src/grep.rs`
Verify you see `Some(g) if g.is_empty() => Ok(None)` at line 160.

- [ ] **Step 13: Collapse the guard into a literal pattern**

In `crates/atd-tools-fs/src/grep.rs`, replace:

```rust
fn build_optional_globset(glob: Option<&str>) -> Result<Option<GlobSet>, ToolCallError> {
    match glob {
        None => Ok(None),
        Some(g) if g.is_empty() => Ok(None),
        Some(g) => {
```

with:

```rust
fn build_optional_globset(glob: Option<&str>) -> Result<Option<GlobSet>, ToolCallError> {
    match glob {
        None | Some("") => Ok(None),
        Some(g) => {
```

This removes the guard and merges two "no glob" arms into one. Behavior-equivalent: `&str::is_empty()` is `len() == 0`, same as matching the empty literal.

- [ ] **Step 14: Verify grep tests pass**

Run: `cargo test -p atd-tools-fs --lib grep`
Expected: all pass.

### 2.5 Final gate for C2

- [ ] **Step 15: Run the full regression gate + clippy gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```
Expected:
- fmt: clean (C1 already covered; C2 should not have broken fmt)
- clippy: clean (zero warnings after all 4 fixes)
- test: 297 passing
- build: clean

If clippy still emits warnings, re-read the warning output and inspect the un-fixed site. If new warnings appear that weren't in baseline, investigate (rare — clippy fixes usually don't cascade).

- [ ] **Step 16: Commit**

```bash
git add -A
git status --short  # sanity: 4 .rs files modified
git commit -m "style(clippy): fix 4 warnings to clean -D warnings baseline (C2)

- atd-protocol/sanitize.rs: manual_find — imperative loop → Iterator::find
- atd-runtime/capability.rs: should_implement_trait — inherent from_iter
  method → impl FromIterator<String>. All 7 call sites (UFCS syntax)
  resolve unchanged; bonus: .collect::<CapabilitySet>() now works.
- atd-tools-web/fetch.rs: manual_clamp — two .min(MAX).max(1) chains
  → .clamp(1, MAX). Semantically equivalent; pre-conditions (min ≤ max)
  hold in all call paths.
- atd-tools-fs/grep.rs: redundant_guards — Some(g) if g.is_empty() →
  Some(''). Byte-length-zero match semantics unchanged.

Zero behavior change. All 297 tests pass. Workspace now satisfies
cargo clippy --workspace --all-features -- -D warnings.

Refs: docs/superpowers/specs/2026-04-24-fmt-clippy-cleanup-design.md §4.2"
```

---

## Task 3 (C3): Add fmt + clippy to CI

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Read the current CI workflow**

Run: `cat .github/workflows/ci.yml`
Expected: a single `test` job with 5 steps (checkout, toolchain, cache, build release, test).

- [ ] **Step 2: Edit `.github/workflows/ci.yml`**

Replace the file's contents with:

```yaml
name: CI

on:
  push:
    branches: [master]
  pull_request:
    branches: [master]

jobs:
  test:
    name: cargo test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: "1.85"
          components: rustfmt, clippy

      - name: Cache cargo registry + target
        uses: Swatinem/rust-cache@v2

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Clippy
        run: cargo clippy --workspace --all-features -- -D warnings

      - name: Build release binaries (e2e tests need these)
        run: cargo build --release -p atd-ref-server-bin -p atd-mcp-bridge

      - name: Run tests
        run: cargo test --workspace --all-targets
```

Key changes from the pre-C3 version:

1. `with: toolchain: "1.85"` gains a sibling `components: rustfmt, clippy` line.
2. Two new steps inserted between `Cache` and `Build release binaries`:
   - `Check formatting` → `cargo fmt --all -- --check`
   - `Clippy` → `cargo clippy --workspace --all-features -- -D warnings`
3. Existing `Build release binaries` and `Run tests` steps unchanged.

Step ordering is fail-fast: fmt (seconds) and clippy (a minute or two) block before the slow release build.

- [ ] **Step 3: Validate YAML syntax locally**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" 2>&1 | head -5`
Expected: no output (valid YAML). If Python 3 isn't available, fall back to `ruby -ryaml -e "YAML.load_file('.github/workflows/ci.yml')"` or any other YAML validator. If none available, inspect visually — the indentation must be 6 spaces for step keys under `steps:`.

- [ ] **Step 4: Simulate CI's gate locally**

Run (in order, exactly as CI would):
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo build --release -p atd-ref-server-bin -p atd-mcp-bridge
cargo test --workspace --all-targets
```
Expected: all four steps exit 0. This is the complete CI surface.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml
git status --short  # sanity: only .github/workflows/ci.yml modified
git commit -m "ci: add cargo fmt + clippy gates (C3)

Add rustfmt + clippy components to the toolchain installer and two new
CI steps (fmt --check, clippy -- -D warnings). Steps placed before the
release build for fail-fast.

Baseline was fixed in C1 (fmt) and C2 (clippy); this commit locks the
result so regressions can't land silently.

Refs: docs/superpowers/specs/2026-04-24-fmt-clippy-cleanup-design.md §4.3"
```

---

## Task 4: Post-flight + milestone tag

**Files:** None modified.

- [ ] **Step 1: Full 4-gate sanity sweep**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```
Expected: all four exit 0, test count 297.

- [ ] **Step 2: Inspect the commit history**

Run: `git log --oneline pre-fmt-clippy-cleanup..HEAD`
Expected: exactly 3 commits (C1, C2, C3), each with a `refactor/style/ci` type tag in its message.

- [ ] **Step 3: Verify no Cargo.toml touched**

```bash
git diff --stat pre-fmt-clippy-cleanup..HEAD -- '**/Cargo.toml' Cargo.toml
```
Expected: empty output (no Cargo.toml should have changed).

- [ ] **Step 4: Verify no source .rs file outside the 4 clippy-fix files had logic changes**

```bash
# Count non-whitespace diff lines per .rs file
git diff pre-fmt-clippy-cleanup..HEAD --numstat -- '*.rs' | sort -k1 -n -r | head -20
```
The 4 clippy-fix files (`sanitize.rs`, `capability.rs`, `fetch.rs`, `grep.rs`) should have small deltas (1-10 lines each). The other ~52 files will show larger deltas from fmt — but inspect any unexpected outliers.

Spot-check one clippy file:
```bash
git diff pre-fmt-clippy-cleanup..HEAD -- crates/atd-protocol/src/sanitize.rs
```
Expected: 5-ish lines of diff showing the `for` loop → `Iterator::find` change plus any fmt-related tweaks from C1.

- [ ] **Step 5: Tag the milestone**

```bash
git tag sp-fmt-clippy-cleanup
git log --oneline pre-fmt-clippy-cleanup..sp-fmt-clippy-cleanup
```
Expected: the same 3 commits listed.

- [ ] **Step 6: No commit for this task** — tag only.

---

## Self-review checklist (fill in after executing)

- [ ] All 3 commits (C1, C2, C3) pass the 4-gate (`fmt --check` + `clippy -- -D warnings` + `test --all-targets` + `build --release`) at HEAD.
- [ ] Each commit is revertible independently via `git revert`.
- [ ] `cargo test --workspace --all-targets` = 297 tests, unchanged from baseline.
- [ ] No Cargo.toml modified by this SP.
- [ ] `.github/workflows/ci.yml` has 7 steps total: checkout, toolchain (with components), cache, fmt, clippy, build release, test.
- [ ] `pre-fmt-clippy-cleanup` tag at baseline; `sp-fmt-clippy-cleanup` at completion.
- [ ] Spec file `docs/superpowers/specs/2026-04-24-fmt-clippy-cleanup-design.md` untouched by this SP (it's a record of decisions, not a live doc).
