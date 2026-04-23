# SP-9 — Public Release v0.1.0 Design Spec

**Date:** 2026-04-24
**Status:** Design approved; plan pending.
**Scope:** Sub-project 9. Land `atd-mvp` as a public v0.1.0 release — GitHub push to the author's personal account (transfer to `atd-protocol` org deferred to Phase 2), crates.io publication of `atd-types` + `atd-client` + `atd-mcp-bridge`, minimum GitHub Actions CI, public-facing README, repo hygiene.
**Builds on:** SP-7 (`sp7-mcp-bridge-validated`) — 250 workspace tests, 6 SP tags shipped, 1 MVP arc completed.

---

## 1. Motivation

SP-1 through SP-7 built the reference implementation and proved it works end-to-end, including through the MCP bridge. Everything is in `/home/nan/proj/atd-mvp` on a local machine. For the work to produce value, outside developers need to be able to:

1. Read the code.
2. Build the code.
3. Depend on the code from their own projects — `cargo add atd-client` or `cargo install atd-mcp-bridge`, not "clone this repo and fiddle with path dependencies."
4. Have confidence the code actually passes its own tests (green CI badge).

SP-9 turns the local repo into a public v0.1.0 release: code on GitHub, libraries on crates.io, minimum CI, clean README. After SP-9, anyone can go from zero to "running a real ATD agent workflow" in three commands.

Scope is narrow by design. Announcement content (blog, Twitter, partner outreach) is out of scope — SP-9 produces the artifact; SP-10 or beyond handles the promotion.

---

## 2. Scope

### 2.1 In scope

1. **Workspace polish** — `Cargo.toml` `repository` field, `LICENSE` file verification, rename `CLAUDE.md` → `AGENTS.md`, strip `docs/issues/` (ANOS-gap notes).
2. **Per-crate publish prep** for `atd-types`, `atd-client`, `atd-mcp-bridge`:
   - `keywords`, `categories`, `readme`, `exclude` in each `Cargo.toml`
   - Per-crate `README.md` (~30-60 lines each) explaining what the crate is for
3. **GitHub Actions CI** — `.github/workflows/ci.yml` running `cargo test --workspace --all-targets` on push/PR. CI status badge in root README.
4. **Public README rewrite** — what is this, quick start, install, architecture, validation, license.
5. **CONTRIBUTING.md** — short (~30 lines) — welcoming contribution policy.
6. **Dry-run publish verification** — `cargo publish --dry-run` for all 3 crates.
7. **Tag `v0.1.0`** at the commit where everything is ready to push.
8. **Manual hand-off** — actual `cargo publish` + `git push` is done by the user (requires crates.io token + GitHub credentials).

### 2.2 Explicitly deferred (Phase 2+)

- **Announcement content** — blog post, Twitter thread, partner email templates.
- **crates.io publishing for `atd-ref-server` + `atd-cli`** — Phase 2.
- **PyPI publishing for the Python SDK** — requires pyproject polish, PyPI org claim. Phase 2.
- **Transfer to `atd-protocol` GitHub org** — Phase 2.
- **Multi-platform CI (macOS + Windows)** — Phase 2.
- **Semver enforcement tooling (`cargo semver-checks`)** — Phase 2 when semver drift becomes a risk.
- **Changelog discipline (`CHANGELOG.md`)** — first release; nothing to log yet.
- **Documentation site (docs.rs metadata + optional site)** — docs.rs picks up automatically on publish; no extra work this SP.

### 2.3 Prerequisites

- atd-mvp at tag `sp7-mcp-bridge-validated`, 250 tests green.
- User has a crates.io account with an API token ready for the manual publish step.
- User has a GitHub account with push access to `github.com/<user>/atd-mvp` (new repo — user creates).

---

## 3. Publication plan

### 3.1 Which crates

| Crate | Published to crates.io? | Rationale |
|---|---|---|
| `atd-types` | ✅ Yes (first) | Protocol types; other implementers depend on this as a library |
| `atd-client` | ✅ Yes (second) | Client SDK; the thing external Rust users actually consume |
| `atd-mcp-bridge` | ✅ Yes (third) | Binary — `cargo install atd-mcp-bridge` for non-Rust MCP users |
| `atd-ref-server` | ❌ Not in v0.1.0 | Reference implementation; confusion risk ("is this production-ready?"); build-from-source is fine for Phase 0 |
| `atd-cli` | ❌ Not in v0.1.0 | Convenience tool; non-critical-path |
| `atd-examples` | ❌ Already `publish = false` | Examples; never published |

### 3.2 Order + rationale

Transitive deps force the order:
```
atd-types      (zero intra-repo deps)
  ↓
atd-client     (depends on atd-types)
  ↓
atd-mcp-bridge (depends on atd-types + atd-client)
```

Each step: `cargo publish -p <crate>` (not `--dry-run`), then wait ~60s for registry propagation before publishing the next. This is because Cargo resolves the downstream crate's dependency against the published registry, not the workspace path, after publish (once `path = ...` is accompanied by `version = "0.1.0"`, both resolve, but the publish RPC needs to find the dep on crates.io).

### 3.3 Cargo.toml required fields

Each published crate needs (in addition to what it already has):

```toml
[package]
name = "atd-types"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "Apache-2.0"
description = "..."                     # already present
repository = "https://github.com/<user>/atd-mvp"
readme = "README.md"                    # per-crate README
keywords = ["atd", "agent", "tool-dispatch", "mcp", "llm"]  # crates.io max 5
categories = ["api-bindings", "development-tools"]  # crates.io vocabulary
exclude = ["tests/fixtures/*", "benches/*"]  # drop bulk assets
```

`keywords` and `categories` must use crates.io's controlled vocabulary. Pick up to 5 each.

### 3.4 Per-crate README content

Each 30-60 lines. Sections:
1. **One-sentence purpose**
2. **Quickstart code block** (3-5 lines showing the most common use)
3. **Feature list** (bullet points)
4. **Links** — repository, full docs on docs.rs, related crates
5. **License** — Apache-2.0

No marketing copy. No "about the project" essay (that belongs in the root README).

---

## 4. Repo hygiene

### 4.1 Files to add

| Path | Purpose | Size |
|---|---|---|
| `.github/workflows/ci.yml` | GitHub Actions workflow | ~30 lines |
| `README.md` (rewritten) | Public face of the project | ~150-200 lines |
| `CONTRIBUTING.md` | Contribution policy | ~30 lines |
| `AGENTS.md` | Renamed from `CLAUDE.md` | unchanged content |
| `crates/atd-types/README.md` | Per-crate | ~40 lines |
| `crates/atd-client/README.md` | Per-crate | ~60 lines |
| `crates/atd-mcp-bridge/README.md` | Per-crate | ~50 lines |

### 4.2 Files to remove from public tree

- `docs/issues/*.md` (11 files) — ANOS reference-server gap notes; confusing for external readers; move to a local-only `_private/` directory or simply delete (they're archived in git history).
- `CLAUDE.md` — renamed, not deleted. `AGENTS.md` is the new name.

### 4.3 Files to keep

- `docs/whitepaper/*` — whitepapers are external-friendly and useful
- `docs/reference/*` — architecture reference
- `docs/superpowers/specs/*`, `docs/superpowers/plans/*` — design/plan trail (proves thought process, useful for contributors)
- `docs/validation/*` — capstone evidence (credibility signal)
- `docs/design.md` — core design
- `examples/`, `python/examples/` — usage samples
- Everything in `crates/` — the actual code

---

## 5. GitHub Actions workflow

`.github/workflows/ci.yml`:

```yaml
name: CI
on:
  push:
    branches: [master]
  pull_request:
    branches: [master]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: "1.85"
      - uses: Swatinem/rust-cache@v2
      - name: Build release binaries (needed by e2e tests)
        run: cargo build --release -p atd-ref-server -p atd-mcp-bridge
      - name: Run tests
        run: cargo test --workspace --all-targets
```

- `Swatinem/rust-cache@v2` caches `~/.cargo/registry` + `target/` — halves subsequent CI runtimes.
- Release-mode binaries are needed by the `atd-mcp-bridge/tests/integration_e2e.rs` and `examples/hello_atd` e2e — CI must build them before testing.
- No lint step (clippy) in v0.1.0 — Phase 2.
- No macOS / Windows matrix — Phase 2.

### 5.1 CI badge

Add to the top of the root README:
```markdown
![CI](https://github.com/<user>/atd-mvp/actions/workflows/ci.yml/badge.svg)
```

Badge shows green/red; clicking links to the Actions tab.

---

## 6. Public README structure

Target: ~180 lines of prose. Sections:

### 6.1 Header
- Project name, tagline
- CI badge
- Crates.io badge (once published)

### 6.2 What is this (2 paragraphs)
- The ATD protocol (one-sentence framing)
- atd-mvp as the neutral reference implementation

### 6.3 Quick start
```bash
git clone https://github.com/<user>/atd-mvp
cd atd-mvp
cargo build --release -p atd-ref-server
cargo run --example hello_atd -p atd-examples
```

Sample output (truncated).

### 6.4 Install as a library
```bash
# Rust library users
cargo add atd-client

# MCP client users
cargo install atd-mcp-bridge
```

Example of each.

### 6.5 Architecture at a glance
One paragraph + a tiny ASCII diagram:
```
┌──────────────┐  length-prefixed JSON  ┌──────────────────┐
│  atd-client  │ ←───────────────────→  │  ATD server      │
└──────────────┘    (Unix socket)       │  (atd-ref-server │
                                         │   or yours)      │
                                         └──────────────────┘

┌──────────────┐    MCP JSON-RPC    ┌────────────────┐      ┌──────────────┐
│  MCP client  │ ←─── stdio ──────→ │ atd-mcp-bridge │ ←──→ │  ATD server  │
│  (Hermes,    │                    │                │      │              │
│   Cursor)    │                    └────────────────┘      └──────────────┘
└──────────────┘
```

### 6.6 Validation
Link to `docs/validation/2026-04-23-sp6-capstone.md` + `2026-04-24-sp7-mcp-bridge.md`. One-sentence each.

### 6.7 Project status
"This is v0.1.0. Breaking changes allowed until 1.0 (semver 0.x contract). Scope is MVP — see `docs/superpowers/specs/` for the design trail."

### 6.8 License + contributing
- Apache-2.0 (LICENSE file in repo root)
- Contributions welcome — see CONTRIBUTING.md

---

## 7. Risks and non-risks

### 7.1 Risks

- **crates.io name squatting.** If `atd-types`, `atd-client`, `atd-mcp-bridge` are already taken, first `cargo publish` fails. Mitigation: user runs `cargo search atd-types atd-client atd-mcp-bridge` BEFORE starting the publish sequence; if taken, fall back to `atd-protocol-types` etc. and re-plan.
- **Broken `cargo install atd-mcp-bridge` UX.** Users who `cargo install` get only the binary; need `atd-ref-server` somewhere for the bridge to forward to. Mitigation: the bridge's help text + the crate's README clearly state "needs a running ATD server."
- **CI build time.** First run on GitHub Actions may take 10+ min for cold cache. Mitigation: `Swatinem/rust-cache@v2` amortizes subsequent runs to ~2 min.
- **Missing `rust-version` propagation.** Some downstream users on older toolchains hit MSRV errors. Mitigation: `rust-version = "1.85"` declared per-crate; crates.io UI shows it prominently.

### 7.2 Non-risks

- **License audit** — all deps MIT/Apache/BSD (verified in SP-5 + SP-6 validation docs)
- **Test suite quality** — 250 tests, known to pass locally + in a subagent sandbox
- **SemVer** — 0.1.x allows any change (per CLAUDE.md). No compatibility obligations yet.
- **docs.rs** — automatic from crates.io; no extra work.
- **Transfer to `atd-protocol` org** — GitHub supports repo transfer without URL breakage via redirects. Phase 2 move is cheap.

---

## 8. Exit criteria

1. `cargo publish -p atd-types --dry-run` exits 0, no warnings about missing metadata
2. `cargo publish -p atd-client --dry-run` exits 0
3. `cargo publish -p atd-mcp-bridge --dry-run` exits 0
4. `.github/workflows/ci.yml` syntactically valid (parseable by `actionlint` if available; else eyeball)
5. Root `README.md` rewritten; no `ANOS` references anywhere in public-facing root/README/AGENTS files
6. `docs/issues/` no longer in the tree
7. `CLAUDE.md` → `AGENTS.md` rename done; content preserved
8. Per-crate `README.md` exists in each of the 3 published crates
9. `cargo test --workspace --all-targets` still passes (250 tests)
10. Tag `v0.1.0` created at the commit where 1-9 hold

Manual work remaining (user, not subagent):
- Actual `cargo publish` of 3 crates in order, with 60s waits between
- `git remote add origin git@github.com:<user>/atd-mvp.git && git push -u origin master --tags`
- Verify the GitHub repo displays the CI badge + README correctly

---

## 9. Out of scope forever at this layer

- Public announcement / marketing content
- Documentation site (beyond docs.rs auto-generation)
- Paid support / commercial offerings
- Partner onboarding documents

SP-9 produces the artifact and the infrastructure for external use. Promotion and adoption are downstream concerns.
