# SP-9 — Public Release v0.1.0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the local `atd-mvp` repo into a v0.1.0 public release — published to crates.io (3 crates), pushed to GitHub, minimum CI, clean public face. Actual `cargo publish` + `git push` are manual hand-offs to the user (require credentials).

**Architecture:** 5 tasks of repo + manifest polish + 1 task of verification-and-tag. No application code changes. New YAML, new README files, manifest edits, doc cleanup. The output is a tagged `v0.1.0` commit that the user can `cargo publish` + `git push` from.

**Tech Stack:** No new deps. Standard crates.io metadata, GitHub Actions YAML, Apache-2.0 license file, markdown READMEs.

**Spec:** `docs/superpowers/specs/2026-04-24-sp9-public-release.md`

**Scope boundary:**
- **In:** workspace polish (repo field, LICENSE, AGENTS.md rename, docs/issues/ strip); per-crate publish prep (3x Cargo.toml + 3x README); GitHub Actions; root README rewrite; CONTRIBUTING.md; dry-run publish verification; v0.1.0 tag.
- **Out:** actual `cargo publish` + `git push` (manual); announcement content; `atd-ref-server`/`atd-cli`/Python SDK publishing; multi-platform CI.

**Prerequisites:**
- `sp7-mcp-bridge-validated` tag, 250 tests green.
- User has a crates.io account + API token (used during manual publish step).
- User has chosen a GitHub destination (personal account; org transfer deferred).

**Exit criteria:**
1. 3 × `cargo publish -p <crate> --dry-run` succeed cleanly
2. `.github/workflows/ci.yml` valid
3. Root README rewritten; no ANOS in root README/AGENTS/crate READMEs
4. `docs/issues/` removed from tree
5. `CLAUDE.md` → `AGENTS.md` rename done
6. 3× per-crate READMEs present
7. `cargo test --workspace --all-targets` — 250 tests pass
8. Tag `v0.1.0` created

**Outside this plan (user does manually):**
- `cargo publish` 3 crates in order with 60s waits
- `git remote add origin git@github.com:<user>/atd-mvp.git && git push -u origin master --tags`
- Replace `<YOUR_USERNAME>` placeholders with real username BEFORE pushing (grep check in Task 1 flags these)

---

## File Structure

```
/
├── .github/workflows/
│   └── ci.yml                              (NEW — Task 3)
├── README.md                                (REWRITE — Task 4)
├── AGENTS.md                                (RENAME from CLAUDE.md — Task 1)
├── CONTRIBUTING.md                          (NEW — Task 4)
├── LICENSE                                  (VERIFY — Task 1)
├── Cargo.toml                               (MODIFY repository field — Task 1)
├── crates/atd-types/
│   ├── Cargo.toml                           (MODIFY — Task 2)
│   └── README.md                            (NEW — Task 2)
├── crates/atd-client/
│   ├── Cargo.toml                           (MODIFY — Task 2)
│   └── README.md                            (NEW — Task 2)
├── crates/atd-mcp-bridge/
│   ├── Cargo.toml                           (MODIFY — Task 2)
│   └── README.md                            (NEW — Task 2)
└── docs/
    └── (delete docs/issues/)
```

---

## Task 1: Workspace polish

**Files:**
- Modify: `/home/nan/proj/atd-mvp/Cargo.toml` (workspace-level `repository`)
- Verify: `/home/nan/proj/atd-mvp/LICENSE` exists with Apache-2.0 body
- Rename: `CLAUDE.md` → `AGENTS.md`
- Delete: `docs/issues/` directory

- [ ] **Step 1.1: LICENSE file check**

```bash
cd /home/nan/proj/atd-mvp
ls -la LICENSE
head -5 LICENSE
```

Expected: file exists, first lines read "Apache License" / "Version 2.0". If missing:

```bash
cd /home/nan/proj/atd-mvp
curl -sSfL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE
```

Verify the first line is `Apache License`.

- [ ] **Step 1.2: Workspace Cargo.toml `repository` field**

Read `/home/nan/proj/atd-mvp/Cargo.toml`. Find `[workspace.package]`. If it has a `repository = "..."` field already, leave it unless it references an old URL. If absent, add:

```toml
[workspace.package]
# ... existing fields ...
repository = "https://github.com/<YOUR_USERNAME>/atd-mvp"
```

**IMPORTANT**: The literal string `<YOUR_USERNAME>` is a PLACEHOLDER. User must replace before pushing. We leave it in intentionally; the verification grep in Task 5 will catch unreplaced placeholders.

Other workspace fields to verify/add:
- `authors` — workspace level, leave as-is if present
- `license = "Apache-2.0"` — workspace level
- `edition = "2024"` — workspace level
- `rust-version = "1.85"` — workspace level

- [ ] **Step 1.3: Rename CLAUDE.md → AGENTS.md**

```bash
cd /home/nan/proj/atd-mvp
git mv CLAUDE.md AGENTS.md
```

No content change. `AGENTS.md` is the conventional filename for "LLM collaborator guidance", signaling the file's audience to external readers.

- [ ] **Step 1.4: Strip docs/issues/**

```bash
cd /home/nan/proj/atd-mvp
ls docs/issues/
git rm -r docs/issues/
```

The ANOS-focused gap notes are archived in git history; no loss. They're confusing for external readers.

- [ ] **Step 1.5: Final grep for ANOS-mentioning files (non-test)**

```bash
cd /home/nan/proj/atd-mvp
grep -rEl 'ANOS\|anos\.sock\|default_anos' \
  --include='*.rs' --include='*.toml' --include='*.md' \
  --exclude-dir=target --exclude-dir=.git | sort -u
```

Expected: the ATD project's own spec/plan docs may still mention ANOS (as "see ANOS for prior art" etc. — this is honest context, fine). The key gate: no file under `crates/` or `examples/` or `python/` or root should contain ANOS references in code/comment form.

If any crate source still has ANOS references, flag and fix. If only docs mention it for context, leave as-is.

- [ ] **Step 1.6: Build check**

```bash
cd /home/nan/proj/atd-mvp
cargo build --workspace
cargo test --workspace --all-targets
```

Expected: 250 tests pass (no regressions from manifest + doc changes).

- [ ] **Step 1.7: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add -A
git commit -m "chore: workspace polish for v0.1.0 release prep"
```

---

## Task 2: Per-crate publish prep

**Files:**
- Modify: `crates/atd-types/Cargo.toml`, `crates/atd-client/Cargo.toml`, `crates/atd-mcp-bridge/Cargo.toml`
- Create: 3× new `README.md` in each crate dir

- [ ] **Step 2.1: `atd-types` Cargo.toml polish**

Edit `/home/nan/proj/atd-mvp/crates/atd-types/Cargo.toml`. Ensure `[package]` has:

```toml
[package]
name = "atd-types"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "Protocol types for the ATD (Agent Tool Dispatch) reference implementation."
readme = "README.md"
keywords = ["atd", "agent", "tool-dispatch", "mcp", "llm"]
categories = ["api-bindings", "development-tools"]
exclude = ["tests/fixtures/*"]
```

If any listed field is already present and correct, leave it. If `description` is generic/old, replace with the exact sentence above. Adjust `exclude` if the crate has other bulk assets (check for `benches/`, `data/`, etc.).

- [ ] **Step 2.2: `atd-types` README**

Create `/home/nan/proj/atd-mvp/crates/atd-types/README.md`:

```markdown
# atd-types

Protocol types for the [Agent Tool Dispatch (ATD) protocol](https://github.com/<YOUR_USERNAME>/atd-mvp).

## What's in here

- `ToolDefinition` — full metadata for a tool (id, schema, safety, trust, bindings)
- `ToolSummary` — compact form returned by `discover`
- `ToolResult` — success + error variants of a tool call outcome
- `ToolSafety`, `ToolCapability`, `ToolTrust`, `ToolBinding` — sub-structures
- Enums: `SafetyLevel`, `ToolVisibility`, `TrustLevel`, `BindingProtocol`

All types are `serde`-compatible with the ATD wire format (length-prefixed JSON over Unix sockets).

## Quick example

```rust
use atd_types::{ToolSummary, ToolSafety, SafetyLevel};

let safety = ToolSafety {
    level: SafetyLevel::Read,
    dry_run: false,
    side_effects: vec![],
    data_sensitivity: None,
};
```

## Related crates

- [`atd-client`](https://crates.io/crates/atd-client) — client SDK for Rust agents
- [`atd-mcp-bridge`](https://crates.io/crates/atd-mcp-bridge) — MCP bridge binary

## License

Apache-2.0. See [LICENSE](https://github.com/<YOUR_USERNAME>/atd-mvp/blob/master/LICENSE).
```

- [ ] **Step 2.3: `atd-client` Cargo.toml polish**

Edit `/home/nan/proj/atd-mvp/crates/atd-client/Cargo.toml`. Ensure `[package]` has:

```toml
[package]
name = "atd-client"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "Rust client SDK for the ATD (Agent Tool Dispatch) protocol — connect to any ATD-speaking server over Unix sockets."
readme = "README.md"
keywords = ["atd", "agent", "client", "sdk", "mcp"]
categories = ["api-bindings", "asynchronous"]
exclude = ["tests/fixtures/*", "benches/*"]
```

- [ ] **Step 2.4: `atd-client` README**

Create `/home/nan/proj/atd-mvp/crates/atd-client/README.md`:

```markdown
# atd-client

Rust client SDK for the [Agent Tool Dispatch (ATD) protocol](https://github.com/<YOUR_USERNAME>/atd-mvp).

Connect to any ATD-speaking server over a Unix socket, discover tools, describe them, and call them.

## Install

```bash
cargo add atd-client
```

## Quick example

```rust
use atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = AtdClient::connect(
        Endpoint::unix("/tmp/my-atd.sock")
    ).await?;

    let tools = client.discover(None, DiscoverFilter::default()).await?;
    println!("{} tools available", tools.len());

    let result = client.call(
        "ref:echo.say",
        serde_json::json!({"text": "hello"}),
        CallOptions { dry_run: false, preferred_binding: None },
    ).await?;

    println!("{result:?}");
    Ok(())
}
```

## Features

- `discover` + `describe` + `call` — the full ATD v0.1 surface
- Async (tokio)
- Length-prefixed JSON wire protocol over Unix sockets
- No server dependency — works against any ATD-speaking server (including
  the reference server, `atd-ref-server`)

## See also

- [`atd-types`](https://crates.io/crates/atd-types) — shared protocol types
- [`atd-mcp-bridge`](https://crates.io/crates/atd-mcp-bridge) — MCP bridge
  for third-party MCP clients like Claude Desktop, Cursor, Hermes

## License

Apache-2.0. See [LICENSE](https://github.com/<YOUR_USERNAME>/atd-mvp/blob/master/LICENSE).
```

- [ ] **Step 2.5: `atd-mcp-bridge` Cargo.toml polish**

Edit `/home/nan/proj/atd-mvp/crates/atd-mcp-bridge/Cargo.toml`. Ensure `[package]` has:

```toml
[package]
name = "atd-mcp-bridge"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
description = "MCP-over-stdio bridge forwarding tools/list and tools/call to any ATD (Agent Tool Dispatch) server. Lets Claude Desktop, Cursor, Hermes, and other MCP clients reach ATD-speaking tool servers."
readme = "README.md"
keywords = ["atd", "mcp", "bridge", "agent", "tool-dispatch"]
categories = ["command-line-utilities", "api-bindings"]
exclude = ["tests/fixtures/*"]
```

Note `command-line-utilities` category — reflects that this is primarily a binary.

- [ ] **Step 2.6: `atd-mcp-bridge` README**

Create `/home/nan/proj/atd-mvp/crates/atd-mcp-bridge/README.md`:

```markdown
# atd-mcp-bridge

MCP-over-stdio bridge that lets any MCP-speaking client (Claude Desktop,
Cursor, Hermes, OpenAI Codex, …) drive tools served by an
[ATD (Agent Tool Dispatch) server](https://github.com/<YOUR_USERNAME>/atd-mvp).

## Install

```bash
cargo install atd-mcp-bridge
```

## Usage

The bridge needs to point at a running ATD server (Unix socket). Two
ways to configure:

```bash
# 1. --sock flag
atd-mcp-bridge --sock /path/to/atd-server.sock

# 2. ATD_SOCK env var
ATD_SOCK=/path/to/atd-server.sock atd-mcp-bridge
```

The bridge reads MCP JSON-RPC 2.0 requests on stdin and writes responses
on stdout, as MCP spec requires.

## Example: Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "atd": {
      "command": "atd-mcp-bridge",
      "env": { "ATD_SOCK": "/tmp/my-atd.sock" }
    }
  }
}
```

Then run any ATD server at `/tmp/my-atd.sock` (e.g., `atd-ref-server`
from the `atd-mvp` repo) and restart Claude Desktop. The ATD tools will
appear in Claude's tool list.

## What you need elsewhere

- A running ATD server. Build one from source:
  ```bash
  git clone https://github.com/<YOUR_USERNAME>/atd-mvp
  cargo build --release -p atd-ref-server
  atd-ref-server --sock /tmp/my-atd.sock
  ```

## See also

- [`atd-types`](https://crates.io/crates/atd-types) — protocol types
- [`atd-client`](https://crates.io/crates/atd-client) — Rust client SDK

## License

Apache-2.0. See [LICENSE](https://github.com/<YOUR_USERNAME>/atd-mvp/blob/master/LICENSE).
```

- [ ] **Step 2.7: Build + test**

```bash
cd /home/nan/proj/atd-mvp
cargo build --workspace
cargo test --workspace --all-targets
```

Expected: 250 tests pass. New metadata fields don't affect tests.

- [ ] **Step 2.8: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add crates/atd-types/ crates/atd-client/ crates/atd-mcp-bridge/
git commit -m "chore: crates.io publish metadata for atd-types, atd-client, atd-mcp-bridge"
```

---

## Task 3: GitHub Actions CI

**Files:**
- Create: `/home/nan/proj/atd-mvp/.github/workflows/ci.yml`

- [ ] **Step 3.1: Create CI workflow**

```bash
cd /home/nan/proj/atd-mvp
mkdir -p .github/workflows
```

Create `/home/nan/proj/atd-mvp/.github/workflows/ci.yml`:

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

      - name: Cache cargo registry + target
        uses: Swatinem/rust-cache@v2

      - name: Build release binaries (e2e tests need these)
        run: cargo build --release -p atd-ref-server -p atd-mcp-bridge

      - name: Run tests
        run: cargo test --workspace --all-targets
```

- [ ] **Step 3.2: Validate YAML**

Eyeball the file. If `actionlint` is installed locally:
```bash
actionlint .github/workflows/ci.yml
```

If not, skip — GitHub will validate on push.

- [ ] **Step 3.3: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add .github/workflows/ci.yml
git commit -m "ci: add minimum GitHub Actions workflow"
```

---

## Task 4: Public README + CONTRIBUTING.md

**Files:**
- Rewrite: `/home/nan/proj/atd-mvp/README.md`
- Create: `/home/nan/proj/atd-mvp/CONTRIBUTING.md`

- [ ] **Step 4.1: Read current README**

```bash
cd /home/nan/proj/atd-mvp
cat README.md
```

Current README is internal-facing (dev notes). We're replacing it with a public-facing version.

- [ ] **Step 4.2: Rewrite root README**

Replace `/home/nan/proj/atd-mvp/README.md` ENTIRELY with:

````markdown
# atd-mvp

[![CI](https://github.com/<YOUR_USERNAME>/atd-mvp/actions/workflows/ci.yml/badge.svg)](https://github.com/<YOUR_USERNAME>/atd-mvp/actions/workflows/ci.yml)

**The reference implementation of the Agent Tool Dispatch (ATD) protocol.**

ATD is a neutral, cross-vendor wire protocol for letting any LLM agent
call any tool on any server. atd-mvp is the reference: a Rust client
SDK, a Rust reference server with 9 real tools, and an MCP bridge that
makes all of this work with Claude Desktop, Cursor, Hermes, and any
other MCP-speaking agent.

## Quick start

```bash
git clone https://github.com/<YOUR_USERNAME>/atd-mvp
cd atd-mvp
cargo build --release -p atd-ref-server
cargo run --example hello_atd -p atd-examples
```

Expected output:
```
[atd] auto-spawning atd-ref-server → /tmp/.../demo.sock
[atd] connected
[atd] 9 tools registered

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"echoed":{"text":"hello from ATD"}}
[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → 9 paths: Cargo.toml, crates/atd-cli/Cargo.toml, ...
[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout="Linux"

[atd] done.
```

No ANOS, no external daemon — everything runs from this repo.

## Install as a library

For Rust agents that want to speak ATD:

```bash
cargo add atd-client
```

For MCP clients (Claude Desktop, Cursor, Hermes, …) that want to reach
ATD tool servers:

```bash
cargo install atd-mcp-bridge
```

Then configure your MCP client to run the bridge — see
[`crates/atd-mcp-bridge/README.md`](crates/atd-mcp-bridge/README.md) for
examples.

## Architecture at a glance

```
┌──────────────┐  length-prefixed JSON  ┌──────────────────┐
│  atd-client  │ ←───────────────────→  │ ATD server       │
└──────────────┘    (Unix socket)       │ (atd-ref-server  │
                                         │  or yours)       │
                                         └──────────────────┘

┌──────────────┐   MCP JSON-RPC    ┌────────────────┐      ┌──────────────┐
│  MCP client  │ ← stdio ────────→ │ atd-mcp-bridge │ ←──→ │  ATD server  │
│ (Claude      │                   │                │      │              │
│  Desktop,    │                   └────────────────┘      └──────────────┘
│  Cursor,     │
│  Hermes)     │
└──────────────┘
```

- The ATD wire protocol is length-prefixed JSON over a Unix socket —
  trivial to implement in any language.
- The reference server `atd-ref-server` ships with 9 real tools:
  `ref:echo.say`, `ref:fs.{read,write,edit,glob,grep}`,
  `ref:shell.{exec,pwsh}`, `ref:web.fetch`.
- The MCP bridge is a thin forwarder — ~200 lines — letting any MCP
  client reach an ATD server.

## Validation

Two evidence docs prove the independence and cross-vendor claims:

- [`docs/validation/2026-04-23-sp6-capstone.md`](docs/validation/2026-04-23-sp6-capstone.md)
  — `hello_atd` runs with zero ANOS dependency; dep tree + license audit.
- [`docs/validation/2026-04-24-sp7-mcp-bridge.md`](docs/validation/2026-04-24-sp7-mcp-bridge.md)
  — MCP bridge end-to-end tests prove a non-ANOS MCP client can drive
  atd-ref-server through the bridge.

## Project status

This is v0.1.0. Under the SemVer 0.x contract, breaking changes are
allowed until 1.0 — API stability is a Phase 2 concern. The scope is
MVP. The design trail lives in
[`docs/superpowers/specs/`](docs/superpowers/specs/) and
[`docs/superpowers/plans/`](docs/superpowers/plans/); readers curious
about trade-offs will find them there.

## License

Apache-2.0. See [LICENSE](LICENSE).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Issues, PRs, and design feedback
welcome.
````

Replace `<YOUR_USERNAME>` placeholders with the repo owner's actual GitHub
handle BEFORE pushing (Task 5 grep-check catches unreplaced placeholders).

- [ ] **Step 4.3: Write CONTRIBUTING.md**

Create `/home/nan/proj/atd-mvp/CONTRIBUTING.md`:

```markdown
# Contributing to atd-mvp

Thanks for considering a contribution. This is an early-stage project; the
codebase is small, the design is evolving, and external input is welcome.

## How to help

- **Bug reports** — open an issue with a minimal repro. If a tool call
  misbehaves, include the exact JSON request and response.
- **Design feedback** — read [`docs/design.md`](docs/design.md) and the
  per-SP specs under [`docs/superpowers/specs/`](docs/superpowers/specs/).
  Push back on anything that looks wrong; the protocol is still pre-1.0.
- **New tools** — the reference server has 9 tools across 4 domains. Add
  one in a similar pattern (see `crates/atd-ref-server/src/tools/` for
  examples). TDD required: unit test + integration test.
- **Third-party server implementations** — the ATD wire format is
  straightforward. If you implement a server and it interoperates with
  `atd-client`, we'd love to link to it from the README.

## Development

```bash
git clone https://github.com/<YOUR_USERNAME>/atd-mvp
cd atd-mvp
cargo build --workspace
cargo test --workspace --all-targets
```

All 250+ tests should pass. CI runs the same command on every push.

## Coding style

- Rust 2024 edition, MSRV 1.85
- `cargo fmt` before committing (rustfmt default config)
- One commit per logical change; use conventional commits
  (`feat:`, `fix:`, `docs:`, `chore:`, `test:`, etc.)
- If you touch a crate's public API, add a test that exercises the new
  surface

## License

By contributing you agree your contributions will be released under the
[Apache-2.0 license](LICENSE).
```

- [ ] **Step 4.4: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add README.md CONTRIBUTING.md
git commit -m "docs: public-facing README + CONTRIBUTING for v0.1.0"
```

---

## Task 5: Dry-run publish verification + v0.1.0 tag

**Files:**
- None created/modified. Tag is the only artifact.

- [ ] **Step 5.1: Placeholder detection**

```bash
cd /home/nan/proj/atd-mvp
grep -rn '<YOUR_USERNAME>' \
  --include='*.md' --include='*.toml' --include='*.yml' \
  --exclude-dir=target --exclude-dir=.git \
  --exclude-dir=docs/superpowers
```

Expected: matches in:
- `README.md`
- `CONTRIBUTING.md`
- `AGENTS.md` (possibly — check CLAUDE.md's original content)
- `crates/*/README.md`
- `crates/*/Cargo.toml` (via the `repository` workspace field if hardcoded)
- `Cargo.toml` (workspace-level `repository` field)

**Action:** The placeholders are INTENTIONAL — the user replaces them with their actual GitHub username before pushing. Report the grep output as a list of files the user must edit. Do NOT replace them yourself.

The plan docs under `docs/superpowers/plans/` also contain placeholders —
those are fine as plan artifacts.

- [ ] **Step 5.2: ANOS reference check in public-facing files**

```bash
cd /home/nan/proj/atd-mvp
grep -rEn 'ANOS|default_anos|anos\.sock' \
  --include='*.rs' --include='*.md' --include='*.toml' --include='*.yml' \
  --exclude-dir=target --exclude-dir=.git \
  --exclude-dir=docs/whitepaper --exclude-dir=docs/reference \
  --exclude-dir=docs/superpowers --exclude-dir=docs/validation \
  --exclude='docs/design.md' \
  | grep -v 'ATD_SOCK'
```

Expected: a small number of hits in comments that say "ANOS-compatible",
"ANOS-free", or similar (honest context in the `atd-client`/`atd-cli`
docs). No hits in `Cargo.toml`, `README.md`, `AGENTS.md`, or crate
`README.md` files.

If a hit shows up in `README.md` / `AGENTS.md` / a crate README, that's a
real issue — flag it. Otherwise, proceed.

- [ ] **Step 5.3: `cargo publish --dry-run` for each crate**

Dry-run verifies packaging without actually uploading. Run in dependency order:

```bash
cd /home/nan/proj/atd-mvp
cargo publish -p atd-types --dry-run 2>&1 | tail -20
# Look for "Finished" + no "warning: " lines about missing metadata

cargo publish -p atd-client --dry-run 2>&1 | tail -20
# atd-client depends on atd-types by path; dry-run is OK with this.
# The REAL publish will require atd-types to be on crates.io first.

cargo publish -p atd-mcp-bridge --dry-run 2>&1 | tail -20
# Same — depends on atd-client + atd-types.
```

Expected: each dry-run finishes with a packaged `.crate` file and NO
warnings about missing `description`, `license`, `repository`, or
`keywords`. If any warning appears, go back and fix the corresponding
`Cargo.toml`.

- [ ] **Step 5.4: Workspace regression**

```bash
cd /home/nan/proj/atd-mvp
cargo build --workspace
cargo test --workspace --all-targets
```

Expected: clean build, 250 tests pass.

- [ ] **Step 5.5: Tag v0.1.0**

```bash
cd /home/nan/proj/atd-mvp
git log --oneline | head -10
git tag -a v0.1.0 -m "v0.1.0 — atd-mvp reference implementation

- atd-ref-server: 9 tools (echo + 5 fs + 2 shell + 1 web)
- atd-client (Rust) + atd_client (Python) SDKs
- atd-mcp-bridge for MCP ecosystem compatibility
- 250+ tests, Apache-2.0, clean dependency tree
- See docs/validation/ for end-to-end evidence
"
git tag | grep v0
```

- [ ] **Step 5.6: Manual hand-off note**

The actual publish + push happens OFF-PLAN — requires user credentials.

Print this summary for the user:

```
SP-9 subagent work complete. Tag v0.1.0 is at the current HEAD.
Manual steps remaining (user, with credentials):

1. Replace <YOUR_USERNAME> placeholders in:
   - README.md
   - CONTRIBUTING.md
   - AGENTS.md (if any)
   - crates/atd-types/README.md
   - crates/atd-client/README.md
   - crates/atd-mcp-bridge/README.md
   - Cargo.toml (workspace.repository)

   sed -i 's|<YOUR_USERNAME>|YOUR_REAL_USERNAME|g' \
     README.md CONTRIBUTING.md AGENTS.md \
     crates/atd-types/README.md crates/atd-client/README.md \
     crates/atd-mcp-bridge/README.md Cargo.toml

   Then amend the tag:
   git add -A
   git commit --amend --no-edit
   git tag -d v0.1.0
   git tag -a v0.1.0 -m "..." (same message)

2. Create the GitHub repo (empty) at:
   https://github.com/YOUR_USERNAME/atd-mvp

3. Push:
   git remote add origin git@github.com:YOUR_USERNAME/atd-mvp.git
   git push -u origin master
   git push origin v0.1.0

4. Publish to crates.io (in order, with 60s waits):
   cargo publish -p atd-types
   sleep 60
   cargo publish -p atd-client
   sleep 60
   cargo publish -p atd-mcp-bridge

5. Verify:
   - https://github.com/YOUR_USERNAME/atd-mvp — CI badge green, README renders
   - https://crates.io/crates/atd-types — page live
   - https://crates.io/crates/atd-client — page live
   - https://crates.io/crates/atd-mcp-bridge — page live
   - https://docs.rs/atd-types — docs auto-built
```

- [ ] **Step 5.7: No further commit**

Task 5 verifies; the tag is the artifact. No code commits beyond what
Tasks 1-4 produced.

---

## Post-Plan Verification Checklist

- [ ] `cargo publish -p atd-types --dry-run` passes with no missing-metadata warnings
- [ ] `cargo publish -p atd-client --dry-run` passes
- [ ] `cargo publish -p atd-mcp-bridge --dry-run` passes
- [ ] `.github/workflows/ci.yml` exists and is YAML-valid
- [ ] `README.md` rewritten; is public-facing
- [ ] `CONTRIBUTING.md` exists
- [ ] `AGENTS.md` exists (renamed from `CLAUDE.md`)
- [ ] `docs/issues/` absent
- [ ] `LICENSE` exists and is Apache-2.0
- [ ] Per-crate `README.md` exists in atd-types, atd-client, atd-mcp-bridge
- [ ] `cargo test --workspace --all-targets` — 250 tests pass
- [ ] Tag `v0.1.0` created at the final commit
- [ ] Placeholder `<YOUR_USERNAME>` grep reported for manual replacement

## What happens after v0.1.0

- Manual hand-off (user does): replace placeholders, push to GitHub, publish to crates.io
- SP-8 next: conformance suite (protocol-level tests third-party server implementations can run)
- Phase 2 roadmap: macOS/Windows CI, Python PyPI publishing, atd-ref-server + atd-cli crate publishing, transfer to `atd-protocol` GitHub org, announcement content
