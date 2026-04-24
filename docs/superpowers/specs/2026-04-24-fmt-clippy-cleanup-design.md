# fmt + clippy Baseline Cleanup — Design

**Date:** 2026-04-24
**Status:** Approved — ready for implementation plan
**Scope:** Rust workspace only. No behavior change.
**Parent:** Follows SP-refactor-v1 (tag `sp-refactor-v1`).

## 1. Context

The workspace has never run `cargo fmt` to completion, and `cargo clippy
-- -D warnings` has failed on the baseline since well before SP-refactor-v1.
SP-refactor-v1 verified this explicitly (the refactor gate used
`cargo check + cargo test + cargo build --release`, which matches CI, and
skipped fmt/clippy because their failures were pre-existing).

This SP pays down that debt in a single bisect-clean pass, then locks
the result in via CI so it cannot regress.

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q1 | SP scope? | A — Three-in-one: `cargo fmt`, clippy fixes, CI gate |
| Q2 | Commit granularity? | A — 3 bisect-able commits (fmt / clippy / CI YAML) |
| Q3 | Clippy strategy? | A — Fix all 4 warnings (none require `#[allow]`) |

## 3. Baseline state (captured 2026-04-24 on `sp-refactor-v1`)

**`cargo fmt --all -- --check`**: 56 `.rs` files need reformatting across
223 diff hunks. Affected crates: every crate in the workspace plus
`examples/`.

**`cargo clippy --workspace --all-features --no-deps`**: exactly 4
warnings, one per crate. They are:

1. `atd-protocol/src/sanitize.rs:39` — `manual_find`
2. `atd-runtime/src/capability.rs:25` — `should_implement_trait`
   (method `from_iter` shadows `FromIterator::from_iter`)
3. `atd-tools-web/src/fetch.rs:356,363` — `manual_clamp` (two sites in
   the same function)
4. `atd-tools-fs/src/grep.rs:160` — `redundant_guards`

All four are idiomatic cleanups with zero behavior change.

## 4. Commit plan

### 4.1 C1 — `cargo fmt --all`

Run `cargo fmt --all` across the workspace. The result is 56 `.rs`
files re-formatted by rustfmt defaults (no `.rustfmt.toml` in the repo
and none is introduced by this SP). 223 hunks of whitespace, line-break,
and argument-grouping changes. No logic change.

Gate:
```
cargo fmt --all -- --check      # must be clean after the reformat
cargo test --workspace --all-targets
cargo build --release --workspace
```

### 4.2 C2 — Fix 4 clippy warnings

#### 4.2.1 `atd-protocol/src/sanitize.rs:35-45` — `manual_find`

Replace the imperative loop with `Iterator::find`:

```rust
// before
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

// after
pub fn desanitize_tool_name<'a, I>(sanitized: &str, known: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    known
        .into_iter()
        .find(|id| sanitize_tool_name(id) == sanitized)
}
```

#### 4.2.2 `atd-runtime/src/capability.rs:25-29` — `should_implement_trait`

Replace the inherent method with an `impl FromIterator<String>` block.
All 7 existing call sites (`CapabilitySet::from_iter(iter)`) continue
to resolve via the trait method, unchanged.

```rust
// before — inherent method shadows FromIterator
impl CapabilitySet {
    pub fn empty() -> Self { Self::default() }

    pub fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self { granted: iter.into_iter().collect() }
    }
    // ... rest of impl
}

// after — inherent from_iter removed; trait impl added
impl CapabilitySet {
    pub fn empty() -> Self { Self::default() }
    // ... rest of impl
}

impl FromIterator<String> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self { granted: iter.into_iter().collect() }
    }
}
```

Bonus: `iter.collect::<CapabilitySet>()` becomes idiomatic.

#### 4.2.3 `atd-tools-web/src/fetch.rs:356-367` — two `manual_clamp` sites

Replace `.min(MAX).max(1)` chains with `.clamp(1, MAX)`:

```rust
// before
let max_bytes = args.max_bytes.unwrap_or(DEFAULT_MAX_BYTES)
    .min(ctx.max_output_bytes).max(1);
let timeout_ms = args.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)
    .min(MAX_TIMEOUT_MS).max(1);

// after
let max_bytes = args.max_bytes.unwrap_or(DEFAULT_MAX_BYTES)
    .clamp(1, ctx.max_output_bytes);
let timeout_ms = args.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS)
    .clamp(1, MAX_TIMEOUT_MS);
```

`clamp(min, max)` requires `min ≤ max`. `ctx.max_output_bytes` and
`MAX_TIMEOUT_MS` are both positive-finite constants/runtime values ≥ 1
in every code path, satisfying the pre-condition.

#### 4.2.4 `atd-tools-fs/src/grep.rs:160` — `redundant_guards`

Collapse `Some(g) if g.is_empty()` into a literal pattern:

```rust
// before
match glob {
    None => Ok(None),
    Some(g) if g.is_empty() => Ok(None),
    Some(g) => { /* ... */ }
}

// after
match glob {
    None | Some("") => Ok(None),
    Some(g) => { /* ... */ }
}
```

`&str::is_empty()` is a byte-length-zero check, equivalent to the
`""` literal pattern for `Option<&str>`.

Gate for C2:
```
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
```

### 4.3 C3 — CI gate

Edit `.github/workflows/ci.yml`:

```yaml
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: "1.85"
          components: rustfmt, clippy       # NEW

      - name: Cache cargo registry + target
        uses: Swatinem/rust-cache@v2

      - name: Check formatting              # NEW step
        run: cargo fmt --all -- --check

      - name: Clippy                         # NEW step
        run: cargo clippy --workspace --all-features -- -D warnings

      - name: Build release binaries (e2e tests need these)
        run: cargo build --release -p atd-ref-server-bin -p atd-mcp-bridge

      - name: Run tests
        run: cargo test --workspace --all-targets
```

Step ordering is fail-fast: fmt and clippy are cheap and block faster
than the release build. All steps stay in the same `test` job — cargo
cache is reused across them.

Gate for C3: no local gate beyond the previous two commits' gates. CI
validates itself on the first `push origin master` after the commit.

## 5. Non-goals

| Not doing | Why | When it opens |
|---|---|---|
| Introduce `.rustfmt.toml` / `.clippy.toml` | YAGNI — defaults are fine; introducing config is a separate conversation | Future SP if a style choice becomes contentious |
| Bump `rust-version = "1.85"` | Not required by any fix; fmt/clippy 1.9/0.1.95 targeting 1.85 works | Only when a new feature requires a newer toolchain |
| Parallelize fmt/clippy into separate CI jobs | Runner cache overhead > speedup for a small workspace | When test matrix grows or runtime becomes painful |
| Rewrite tests that were reformatted | Tests stay semantically identical under rustfmt | N/A |
| Pre-commit git hook | Client-side enforcement is the user's local choice, not an SP deliverable | Future SP if desired |
| Apply `#[allow(clippy::…)]` suppressions anywhere | All 4 warnings are genuine improvements worth taking | Never for these 4 — future SPs may add allow-s for justified cases |

## 6. Success criteria

The SP is complete when:

1. `cargo fmt --all -- --check` is clean on HEAD.
2. `cargo clippy --workspace --all-features -- -D warnings` is clean on HEAD.
3. `cargo test --workspace --all-targets` passes (297 tests) unchanged.
4. `cargo build --release --workspace` clean.
5. `.github/workflows/ci.yml` includes fmt + clippy steps and a
   `components: rustfmt, clippy` directive on the toolchain installer.
6. Each of C1 / C2 / C3 is independently bisect-reverted-safe (i.e.,
   running the full gate at any of the three intermediate commits is
   green for the gates applicable at that point: C1 makes fmt-check
   green; C2 additionally makes clippy green).
7. Zero behavior change: wire format, binary names, CLI flags, tool
   set all unchanged; integration + e2e tests pass.
8. SP is tagged `sp-fmt-clippy-cleanup` (or similar) on completion.

## 7. Rollback

Before starting: `git tag pre-fmt-clippy-cleanup` on current HEAD.
Any of C1/C2/C3 is `git revert`-safe. Worst case `git reset --hard
pre-fmt-clippy-cleanup`.

## 8. Next steps unlocked

- **SP-8 conformance suite**: the gate cleanup is a soft prerequisite
  (so the conformance crate lands into a known-clean baseline).
- **Any future SP**: no longer pays the "pre-existing baseline is red"
  tax in its review cycle.
