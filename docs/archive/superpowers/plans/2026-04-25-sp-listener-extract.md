# SP-listener-extract — Implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the Unix-socket listener from `atd-ref-server` into a new reusable `atd-server` crate, slimming `atd-ref-server` to a pure binary + built-in tool wiring layer. Bump workspace to 0.2.1.

**Spec:** `docs/superpowers/specs/2026-04-25-sp-listener-extract-design.md`

**Baseline tag (set in T0):** `pre-sp-listener-extract`

**Exit criteria:** see spec §7.

---

## Task 0: Pre-flight + baseline tag

- [ ] **0.1: Survey `atd-ref-server/src/server.rs`**

```bash
wc -l crates/atd-ref-server/src/server.rs
grep -E '^(use|pub use)' crates/atd-ref-server/src/server.rs
```

Document every `use crate::...` line — these are coupling points that must be resolved before extraction. Expected: imports from `atd_runtime`, `atd_protocol`, `tokio` only. If imports from `crate::builtin` or `crate::conformance` exist, the move is more complex and the spec needs amending.

- [ ] **0.2: Survey what re-exports `atd-ref-server/src/lib.rs` makes**

```bash
cat crates/atd-ref-server/src/lib.rs
```

Note any `pub use server::...` lines — these are downstream API contracts to preserve.

- [ ] **0.3: Tag baseline**

```bash
git tag pre-sp-listener-extract
```

- [ ] **0.4: Confirm clean baseline**

```bash
git status --short    # must be empty
cargo test --workspace --all-targets 2>&1 | grep '^test result' | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
```

Record passing-test count. Expected: 334 (post-SP-publish-v2 baseline).

---

## Task 1: Scaffold `atd-server` crate

- [ ] **1.1: Create directory structure**

```bash
mkdir -p crates/atd-server/src crates/atd-server/tests
```

- [ ] **1.2: Write `crates/atd-server/Cargo.toml`**

```toml
[package]
name = "atd-server"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Unix-socket listener and connection layer for ATD-speaking servers — pair with atd-runtime to build a server in ~30 lines."
readme = "README.md"
keywords = ["atd", "server", "agent", "tool-dispatch", "unix-socket"]
categories = ["api-bindings", "asynchronous", "network-programming"]

[dependencies]
atd-protocol = { path = "../atd-protocol", version = "0.2.0" }
atd-runtime = { path = "../atd-runtime", version = "0.2.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
atd-runtime = { path = "../atd-runtime", version = "0.2.0", features = ["testing"] }
atd-sdk = { path = "../atd-sdk", version = "0.2.0" }
tempfile = { workspace = true }
```

(Versions stay at 0.2.0 here — Task 4 bumps the entire workspace including these literals to 0.2.1 in one pass.)

- [ ] **1.3: Write minimal `src/lib.rs` placeholder**

```rust
//! ATD server transport — Unix-socket listener and per-connection task.
//!
//! Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime) to build a
//! server. See README for a 30-line example.

pub mod config;
pub mod connection;
pub mod error;
pub mod server;

pub use config::ServerConfig;
pub use error::ServerError;
pub use server::Server;
```

- [ ] **1.4: Write empty placeholder modules**

Each of `config.rs`, `connection.rs`, `error.rs`, `server.rs` gets a single-line `// placeholder — populated in T2`. The build must succeed at this stage.

- [ ] **1.5: Add to workspace members**

In root `Cargo.toml`, add `"crates/atd-server",` to the `members = [...]` list (alphabetical position: between `atd-sdk` and `atd-tools-echo`).

- [ ] **1.6: Build sanity**

```bash
cargo build -p atd-server 2>&1 | tail -3
cargo build --workspace 2>&1 | tail -3
```

Both must finish cleanly.

- [ ] **1.7: Commit**

```bash
git add crates/atd-server/ Cargo.toml
git commit -m "feat(atd-server): scaffold crate (T1 of SP-listener-extract)

Empty lib + Cargo.toml + workspace registration. Module bodies populated
in T2 by moving atd-ref-server/src/server.rs."
```

---

## Task 2: Move listener code from `atd-ref-server` into `atd-server`

- [ ] **2.1: Read `atd-ref-server/src/server.rs` in full**

Identify logical sections:
- `ServerConfig` struct + builder methods → `config.rs`
- `Server` struct + `new()` + `serve()` + `shutdown()` → `server.rs`
- per-connection handler (the loop that reads frames, dispatches, writes results) → `connection.rs`
- error types → `error.rs`

If `server.rs` is structured as one big file, decide module boundaries by following the natural function clusters.

- [ ] **2.2: Carve out `error.rs`**

Move any `#[derive(thiserror::Error)]` enums + impls related to bind / accept / I/O failures. Make the error type `pub enum ServerError`.

- [ ] **2.3: Carve out `config.rs`**

Move `ServerConfig` struct, `Default` impl, and any `with_*` builder methods. Make all fields `pub` for direct construction; builder methods stay for ergonomics.

- [ ] **2.4: Carve out `connection.rs`**

Move the per-connection async fn (likely named `handle_connection` or similar). It depends on `Registry` (from atd-runtime) + frame helpers (from atd-protocol). Should NOT depend on anything in `atd-ref-server`.

- [ ] **2.5: Carve out `server.rs`**

Move `Server` struct, `Server::new(config, registry)`, `Server::serve()` (the accept loop spawning connection tasks), `Server::shutdown()`. Imports should now be `use crate::{config::ServerConfig, connection, error::ServerError};`.

- [ ] **2.6: Update `atd-server/src/lib.rs`**

Replace the placeholder content with the real re-exports listed in T1.3 (already done in scaffold, but verify after the move that all public types are reachable).

- [ ] **2.7: Build atd-server in isolation**

```bash
cargo build -p atd-server 2>&1 | tail -10
```

Must compile. Errors here are import paths needing adjustment within atd-server.

- [ ] **2.8: Delete `crates/atd-ref-server/src/server.rs`**

```bash
git rm crates/atd-ref-server/src/server.rs
```

(Don't `cargo build` the workspace yet — atd-ref-server is now broken; T3 fixes it.)

- [ ] **2.9: Commit**

```bash
git add crates/atd-server/src/
git commit -m "feat(atd-server): move Server/ServerConfig/connection from atd-ref-server (T2)

Extract the Unix-socket listener layer into the new atd-server crate.
Code is split across config.rs / connection.rs / error.rs / server.rs
along natural responsibility boundaries; behavior unchanged.

atd-ref-server's server.rs is removed in this commit; atd-ref-server
itself is fixed to depend on atd-server in T3 (next commit)."
```

---

## Task 3: Wire `atd-ref-server` to use `atd-server`

- [ ] **3.1: Add atd-server as a dependency in `crates/atd-ref-server/Cargo.toml`**

In `[dependencies]`, add:

```toml
atd-server = { path = "../atd-server", version = "0.2.0" }
```

Order alphabetically among other atd-* deps.

- [ ] **3.2: Update `crates/atd-ref-server/src/lib.rs`**

- Remove `pub mod server;`
- If T0.2 surfaced any `pub use server::...` lines, add equivalent `pub use atd_server::{Server, ServerConfig};` re-exports so downstream code that did `atd_ref_server::server::Server` still compiles via `atd_ref_server::Server`. Mark them with a comment: `// re-exported from atd-server for legacy import paths`.

- [ ] **3.3: Update `crates/atd-ref-server/src/main.rs`**

Replace `use crate::server::{Server, ServerConfig};` (or similar) with `use atd_server::{Server, ServerConfig};`. Anything else in main.rs stays.

- [ ] **3.4: Update `crates/atd-ref-server/src/builtin.rs`**

If it imports anything from `crate::server`, switch to `atd_server::*`. Most likely it doesn't (builtin only registers tools into a `Registry` from atd-runtime).

- [ ] **3.5: Update `crates/atd-ref-server/tests/*.rs`**

```bash
grep -l 'crate::server\|atd_ref_server::server\|atd_ref_server_bin::server' crates/atd-ref-server/tests/*.rs
```

For each match: replace with `atd_server::*` paths.

- [ ] **3.6: Update `crates/atd-ref-server/examples/rw_cycle.rs`**

Same import-path update.

- [ ] **3.7: Build and test the workspace**

```bash
cargo build --workspace 2>&1 | tail -5
cargo test --workspace --all-targets 2>&1 | grep '^test result' | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
```

Expected: 334 tests still pass. Any failure points to either a missed import update or a hidden public-API change.

- [ ] **3.8: Commit**

```bash
git add crates/atd-ref-server/
git commit -m "refactor(atd-ref-server): depend on atd-server for the listener layer (T3)

atd-ref-server now contains only:
- main.rs: clap CLI + Server::new(config, registry).serve()
- builtin.rs: register the 9 built-in tools
- conformance.rs: SP-8.1/8.2 gated tools
- external/: CliBinding demo

The listener implementation lives in atd-server. lib.rs re-exports
Server + ServerConfig from atd-server so downstream import paths
through atd_ref_server::* keep working.

334 tests pass."
```

---

## Task 4: Workspace version bump 0.2.0 → 0.2.1

- [ ] **4.1: Workspace version**

In `Cargo.toml`, change `[workspace.package].version = "0.2.0"` → `"0.2.1"`.

- [ ] **4.2: Path-dep version literals**

```bash
grep -rn 'version = "0.2.0"' crates/*/Cargo.toml
sed -i 's/version = "0.2.0"/version = "0.2.1"/g' crates/*/Cargo.toml
```

Verify post-substitution count matches pre-count (no over-replacement).

- [ ] **4.3: Build + test**

```bash
cargo build --workspace
cargo test --workspace --all-targets 2>&1 | grep '^test result' | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
```

Expected: 334 tests pass.

- [ ] **4.4: Commit**

```bash
git add Cargo.toml crates/*/Cargo.toml Cargo.lock
git commit -m "chore(release): bump workspace 0.2.0 → 0.2.1

New atd-server crate appears; no public API breakage on existing crates.
Patch-level bump is honest pre-1.0 signal: structurally different but
no consumer-affecting change."
```

---

## Task 5: `atd-server` integration test + README

- [ ] **5.1: Write `crates/atd-server/tests/e2e_minimal.rs`**

Smoke test:
1. Build a `Registry` containing one trivial `Tool` (e.g., echo)
2. Spawn `Server::new(config, registry).serve()` on a `tempfile::tempdir()`-derived socket
3. Drive via `atd_sdk::AtdClient::connect(...)` — call discover + the one tool
4. Assert the result round-trips
5. Cleanly shut down

Reuse patterns from `crates/atd-ref-server/tests/dispatch_end_to_end.rs` for the harness shape but make this test crate-local to atd-server (no atd-ref-server dep).

- [ ] **5.2: Write `crates/atd-server/README.md`**

```markdown
# atd-server

Unix-socket listener and per-connection task layer for [Agent Tool Dispatch (ATD)](https://github.com/downsea/atd-mvp) servers.

Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime) (which holds
the `Tool` trait and `Registry`) to host an ATD-speaking server in ~30 lines.

## Minimal example

```rust,no_run
use atd_runtime::Registry;
use atd_server::{Server, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = Registry::new();
    // registry.register(Arc::new(MyTool::new()));

    let config = ServerConfig::default()
        .with_sock_path("/tmp/my-atd.sock");

    Server::new(config, registry).serve().await?;
    Ok(())
}
```

## What's in the box

- `Server` — the accept loop, spawned per-connection tokio tasks
- `ServerConfig` — sock path, audit log path, max-concurrent default, …
- Per-connection frame I/O wired to `atd-protocol`'s wire codec
- `ServerError` — bind / accept / I/O variants

For a complete reference using this crate, see
[`atd-ref-server`](https://crates.io/crates/atd-ref-server) — it adds 9
built-in tools (echo + fs + shell + web) on top of `atd-server`.

## License

Apache-2.0.
```

- [ ] **5.3: Run the new test in isolation + as part of workspace**

```bash
cargo test -p atd-server 2>&1 | tail -10
cargo test --workspace --all-targets 2>&1 | grep '^test result' | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
```

Expected: workspace test count ≥ 335 (added at least one).

- [ ] **5.4: Commit**

```bash
git add crates/atd-server/README.md crates/atd-server/tests/
git commit -m "test(atd-server): minimal e2e + README (T5)

Spawn Server with a one-Tool registry, drive via atd-sdk, assert
discover + call round-trip. Smoke test covers bind, accept, frame I/O,
dispatch, and shutdown."
```

---

## Task 6: Architecture doc update + verification + tag + push

- [ ] **6.1: Update `docs/atd-architecture.md` §8.4 (crate map)**

Add `atd-server` to the crate diagram between `atd-runtime` and the tool/server consumers. Update arrows: `atd-runtime ← atd-server ← atd-ref-server` (and `← future vendor servers`).

- [ ] **6.2: Update `docs/atd-architecture.md` §10 evolution path**

Add a new ✅ row:

```markdown
| Extract socket listener from atd-ref-server into reusable `atd-server` crate | Dispatch (transport) | ✅ | SP-listener-extract | 2026-04-25 | Landed; Server/ServerConfig/connection moved to crates/atd-server. atd-ref-server reduced to binary + built-in tool wiring. Triggered by `healthkit_cli` adopter. |
```

- [ ] **6.3: Final verification gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-targets 2>&1 | grep '^test result' | awk '{p+=$4; f+=$6} END {print "passed:", p, "failed:", f}'
cargo publish -p atd-server --dry-run --allow-dirty --registry crates-io 2>&1 | grep -E '(warning|error|Packaged|Finished)' | head -10
```

All four must be clean. atd-server is the only crate that can be standalone-dry-run'd at this stage (its only atd-* dep is atd-protocol + atd-runtime; for the dry-run those need to be on crates-io, which they aren't, so this WILL fail with "no matching package" — record the failure as expected, the same way SP-publish-v2 did).

If atd-protocol got published between SPs, retry the dry-run. Otherwise, document the limit and move on.

- [ ] **6.4: Commit doc updates**

```bash
git add docs/atd-architecture.md
git commit -m "docs(architecture): §8.4 + §10 — atd-server crate added

Reflect SP-listener-extract: 12-crate map, listener marked ✅ in
the evolution path. Triggered by healthkit_cli first-vendor signal."
```

- [ ] **6.5: Tag + push**

```bash
git log --oneline | head -10

git tag -a v0.2.1 -m "v0.2.1 — listener extracted to atd-server

12-crate workspace:
- atd-protocol, atd-sdk, atd-runtime, atd-server (libraries)
- atd-tools-{echo,fs,shell,web} (built-in tools)
- atd-conformance (test runner)
- atd-cli, atd-mcp-bridge, atd-ref-server (binaries)

atd-server holds the Unix-socket listener and per-connection task
layer, extracted from atd-ref-server. atd-ref-server is now slim:
main.rs + builtin.rs + conformance.rs.

Triggered by the first vendor adopter (healthkit_cli) needing the
listener without the built-in tools."

git tag -a sp-listener-extract -m "SP-listener-extract complete"

git push origin master
git push origin v0.2.1 sp-listener-extract pre-sp-listener-extract
```

- [ ] **6.6: Final summary to user**

Print:

```
SP-listener-extract complete.

- 12-crate workspace; atd-server is the new transport library.
- atd-ref-server slimmed to ~50 lines main.rs + builtin tool wiring.
- 335+ tests pass (existing 334 + new e2e); fmt + clippy gates green.
- v0.2.1 + sp-listener-extract tagged and pushed.
- docs/atd-architecture.md §8.4 + §10 updated.

healthkit_cli can now depend on atd-protocol + atd-runtime + atd-server
(no atd-ref-server dep needed) for self-hosting an ATD server.
```

---

## Post-plan verification checklist

- [ ] `crates/atd-server/` exists with Cargo.toml, README.md, src/{lib,config,server,connection,error}.rs, tests/e2e_minimal.rs
- [ ] `crates/atd-ref-server/src/server.rs` no longer exists
- [ ] `crates/atd-ref-server/Cargo.toml` lists `atd-server` as a dep
- [ ] All 12 `crates/*/Cargo.toml` and root `Cargo.toml` use version `0.2.1` (no `0.2.0` literals remain in workspace)
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-features -- -D warnings` clean
- [ ] `cargo test --workspace --all-targets` — ≥ 335 tests pass
- [ ] `docs/atd-architecture.md` §8.4 reflects 12 crates
- [ ] `docs/atd-architecture.md` §10 has SP-listener-extract ✅ row
- [ ] Tags `v0.2.1`, `sp-listener-extract`, `pre-sp-listener-extract` exist locally and on origin
