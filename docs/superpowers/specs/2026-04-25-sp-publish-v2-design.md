# SP-publish-v2 — Publish all 11 crates to crates.io (post-refactor)

**Date:** 2026-04-25
**Status:** Draft — awaiting approval
**Parent:** Replaces the now-stale SP-9 (`docs/superpowers/plans/2026-04-24-sp9-public-release.md`), which targeted the pre-refactor 3-crate layout.
**Anchor:** `docs/architecture.md` §8.4 (target crate graph) — landed at tag `sp-refactor-v1`.

## 1. Context

SP-9 wrote the path to v0.1.0: GitHub push, repo polish, and `cargo publish --dry-run` for **3 crates** (`atd-types` / `atd-client` / `atd-mcp-bridge`). The git tag `v0.1.0` was created but the actual `cargo publish` step (manual, requires user credentials) was never executed.

Since v0.1.0 the workspace went through SP-refactor-v1, splitting and renaming crates to match `docs/architecture.md` §8.4. The crates.io target surface is now **11 crates** with renamed and newly-extracted ones. SP-9's plan no longer matches reality:

- `atd-types` → renamed to **`atd-protocol`** (and absorbed wire + sanitize)
- `atd-client` → renamed to **`atd-sdk`**
- old `atd-ref-server` (pre-refactor) → split into **`atd-runtime`** + **`atd-tools-{echo,fs,shell,web}`** + **`atd-ref-server`** (renamed from the post-refactor `atd-ref-server-bin` in T0 of this SP)
- New: **`atd-conformance`** (SP-8), **`atd-cli`** (already existed but now publishable)
- `atd-mcp-bridge` — name unchanged, still publishable

This SP plans the v2 publish: bump version, polish remaining metadata, write missing READMEs, dry-run-verify, and tag a fresh release.

## 2. Decisions

| # | Question | Answer |
|---|---|---|
| Q1 | New version number? | **`0.2.0`** — `cargo publish` content materially differs from the `v0.1.0` git tag (renamed crates, new crates, breaking API changes). Stay on 0.x; semver-breaks allowed under 0.x contract. |
| Q2 | Publish all 11 crates, or only library crates? | **Publish all 11.** All four binaries (`atd-cli`, `atd-conformance`, `atd-mcp-bridge`, `atd-ref-server`) are independently `cargo install`-able and serve real users (CLI for devs, conformance runner for third-party server authors, MCP bridge for MCP clients, ref-server for end-to-end demos). |
| Q3 | Publish ordering with crates.io propagation lag? | Strict sequential, 60s waits between each. ~11 minutes total. Parallel publish would race propagation; SP-9 already chose sequential. |
| Q4 | Bump strategy: workspace `version = "0.2.0"` once, or per-crate? | Workspace-level bump. All crates already use `version.workspace = true`. One edit, propagates everywhere. |
| Q5 | What about the old v0.1.0 git tag — leave or move? | Leave. Tag a new `v0.2.0` at the publish HEAD. The two tags reflect two distinct project states; rewriting history is wrong. |
| Q6 | Path-only deps? | All current path deps already include `version = "0.1.0"`. **Bump these to `0.2.0`** in the same workspace bump (Cargo doesn't auto-rewrite version pins inside `[dependencies]` — must update each `version = "..."` literal manually). |
| Q7 | Python SDK rename (`atd_client` → `atd_sdk`)? | **Out of scope.** Per CLAUDE.md, deferred to its own SP. `python/pyproject.toml` stays at `atd-client` for now. |
| Q8 | Multi-platform CI before publish? | **Out of scope.** Existing CI (Linux ubuntu-latest) is sufficient gate; macOS/Windows is Phase 2 per `docs/architecture.md` §10. |
| Q9 | Rename the `-bin`-suffixed package before publish? | **Yes** — last chance to drop the awkward suffix. Pre-rename package = `atd-ref-server-bin`, post-rename = `atd-ref-server` (matches the binary name, which was already `atd-ref-server`). Done as a dedicated T0 commit so the rename bisects cleanly separately from version/metadata changes. |
| Q10 | Actually `cargo publish`? | **No.** This SP stops at dry-run verification. The user explicitly chose not to upload to crates.io at this time. No manual `cargo publish` hand-off note is included. |
| Q11 | `git push` to GitHub at the end? | **Yes** — the SP includes pushing master + the new tags to `origin`. Credentials are already configured (SSH key for `git@github.com:downsea/atd-mvp.git`). |

## 3. Touch points

**T0 — Rename `atd-ref-server-bin` → `atd-ref-server`** (separate commit, ahead of version bump):
- `git mv crates/atd-ref-server-bin crates/atd-ref-server`
- `Cargo.toml` (workspace members list)
- `crates/atd-ref-server/Cargo.toml`: `name`, `[lib].name`
- `crates/atd-ref-server/src/main.rs`: `use atd_ref_server_bin::...` → `use atd_ref_server::...` (2 lines)
- `crates/atd-ref-server/tests/*.rs`: `atd_ref_server_bin` → `atd_ref_server` (7 files, 10 occurrences)
- `crates/atd-conformance/Cargo.toml`: path-dep entry `atd-ref-server-bin = { path = "../atd-ref-server-bin", ... }` → `atd-ref-server = { path = "../atd-ref-server", ... }`
- `crates/atd-conformance/tests/atd_mvp_self_conformance.rs`: 4 references
- `crates/atd-mcp-bridge/tests/integration_e2e.rs`: 2 references (comments + cargo build instructions)
- `examples/hello_atd.rs`: 2 references (comments + cargo build instructions)
- `crates/atd-runtime/src/error.rs`: 1 doc-comment reference
- Live docs: `README.md`, `docs/architecture.md`, `docs/design.md`, `docs/protocol/error-codes.md`, `docs/integrations/*.md`, `crates/atd-mcp-bridge/README.md`
- **Historical SP plans/specs are NOT touched** (project rule: "Historical plans/specs are read-only")

**T1+ Cargo.toml / README changes** (post-rename):

| # | File | Change |
|---|---|---|
| 1 | `Cargo.toml` (workspace) | `version = "0.1.0"` → `"0.2.0"` |
| 2 | All 11 `crates/*/Cargo.toml` | Bump path-dep `version = "0.1.0"` literals → `"0.2.0"` (5 files contain these — atd-cli, atd-conformance, atd-mcp-bridge, atd-ref-server, atd-tools-*) |
| 3 | `crates/atd-runtime/Cargo.toml` | Add `readme = "README.md"`, `keywords`, `categories` |
| 4 | `crates/atd-tools-echo/Cargo.toml` | Same — readme + keywords + categories |
| 5 | `crates/atd-tools-fs/Cargo.toml` | Same |
| 6 | `crates/atd-tools-shell/Cargo.toml` | Same |
| 7 | `crates/atd-tools-web/Cargo.toml` | Same |
| 8 | `crates/atd-cli/Cargo.toml` | Same |
| 9 | `crates/atd-ref-server/Cargo.toml` | Same |
| 10 | `crates/atd-runtime/README.md` | New — describes Tool/Registry/Binding/Middleware extension surface |
| 11 | `crates/atd-tools-echo/README.md` | New — describes the tool, points to atd-runtime for context |
| 12 | `crates/atd-tools-fs/README.md` | New |
| 13 | `crates/atd-tools-shell/README.md` | New |
| 14 | `crates/atd-tools-web/README.md` | New |
| 15 | `crates/atd-cli/README.md` | New — describes `atd` binary, install + usage |
| 16 | `crates/atd-ref-server/README.md` | New — describes `atd-ref-server` binary, install + usage |

Existing READMEs (`atd-protocol`, `atd-sdk`, `atd-mcp-bridge`, `atd-conformance`) get a light pass for v0.2.0 cross-links if they reference sibling crates by old names — verified case-by-case in the plan.

## 4. Approach (per-task overview)

The plan splits into 5 task groups:

**T1 — Version bump.** Single workspace edit + 5 path-dep version literal edits. Verify `cargo build --workspace` passes. One commit.

**T2 — Cargo.toml metadata fill-in.** 7 crates get readme + keywords + categories. One commit per group of related crates (3 commits: tools-*, runtime/cli, ref-server-bin) for clean bisect.

**T3 — README.md authoring.** 7 new README files. One commit each (or grouped tools commit), keeping each commit focused.

**T4 — Cross-link audit on existing 4 READMEs.** Verify atd-protocol, atd-sdk, atd-mcp-bridge, atd-conformance READMEs reference sibling crates correctly post-refactor. Patch in one commit if needed.

**T5 — Dry-run + tag.** Run `cargo publish -p <crate> --dry-run` for all 11 in dep order. Verify zero metadata warnings. Tag `v0.2.0` and `sp-publish-v2`. Print hand-off note.

## 5. Out of scope (explicit non-goals)

- Actual `cargo publish` upload (requires user credentials; manual hand-off, same as SP-9)
- `git push origin v0.2.0` (manual; user has credentials)
- Python `atd_client` → `atd_sdk` rename (separate SP)
- Multi-platform CI matrix (Phase 2)
- New crate features or API changes (this is publish-prep, not feature work)
- Updating `docs/architecture.md` §10 (already shipped via prior commit)
- Announcement / blog content (not a code SP)

## 6. Risks

| Risk | Mitigation |
|---|---|
| Forgotten path-dep version literal causes publish failure halfway through | T1 build-check covers this — Cargo will reject mismatched version pins at compile time |
| crates.io rejects a crate name as squatted/conflicting | Pre-flight: `curl https://crates.io/api/v1/crates/<name>` before T5; spec already verified all 4 are 404 (atd-protocol/sdk/mcp-bridge/runtime). Plan adds the same check for the remaining 7 |
| Rename `atd-ref-server-bin` → `atd-ref-server` introduces churn in tests and downstream snippets | T0 is a single bisect-clean commit; `grep`-driven find-replace covers the closed set of references; CI test gate catches anything missed |
| User runs the manual publish steps in the wrong order | Hand-off note at end of T5 lists exact 11-step sequential `cargo publish` commands with sleep waits |

## 7. Exit criteria

1. Workspace `version = "0.2.0"`; all 5 path-dep literals match
2. All 11 `Cargo.toml` files have `description`, `readme`, `keywords`, `categories`
3. All 11 `crates/*/README.md` files exist
4. `cargo test --workspace --all-targets` — 334 tests still pass
5. `cargo publish --dry-run` succeeds with **zero** warnings for all 11 crates
6. `cargo fmt --all -- --check` + `cargo clippy --workspace --all-features -- -D warnings` clean
7. Tag `v0.2.0` + `sp-publish-v2` at publish-prep HEAD
8. Hand-off note printed to user listing the 11 manual `cargo publish` commands in order
