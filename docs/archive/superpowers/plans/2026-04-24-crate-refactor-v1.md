# ATD Crate Refactor v1 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the Rust workspace from 5 crates to 10 per `docs/atd-architecture.md` §8.4 target graph, with zero behavior change and zero wire-format change.

**Architecture:** Seven bisect-able commits (C1–C7) executed in dependency order bottom-up. Each commit leaves `cargo test --workspace --all-features` green. No crate is published; all renames are free. Python SDK and conformance suite are out of scope per the design doc.

**Tech Stack:** Rust 2024 edition, cargo workspaces. No new dependencies. Uses `sed -i` for mechanical import-path rewrites; `git mv` for directory renames; standard `cargo fmt`/`clippy`/`test`/`build`.

**Spec:** `docs/superpowers/specs/2026-04-24-crate-refactor-design.md`

**Preconditions:** Working tree clean on `master` (or the branch you want the refactor on). All existing workspace tests already green.

---

## Task 0: Pre-flight baseline

**Files:**
- No code changes; only a tag.

**Purpose:** Capture a known-green commit we can `git reset --hard` back to if the refactor goes wrong.

- [ ] **Step 1: Verify working tree is clean**

Run: `git status --short`
Expected: empty output (or only untracked files that are not part of the refactor scope — consult the user if anything shows up under `crates/` or `docs/`).

- [ ] **Step 2: Verify workspace is green on baseline**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```
Expected: all four commands succeed with exit code 0. Fix any pre-existing failure BEFORE starting the refactor — a refactor on a red baseline is untraceable.

- [ ] **Step 3: Tag the baseline**

Run:
```bash
git tag pre-refactor-v1
git log -1 --oneline
```
Expected: the tag is created on the current HEAD. Write down the commit SHA; if anything goes wrong: `git reset --hard pre-refactor-v1`.

- [ ] **Step 4: No commit for this task** — it's a tag only.

---

## Task 1 (C1): Rename `atd-types` → `atd-protocol` and scaffold 5 new crate skeletons

**Files:**
- Rename: `crates/atd-types/` → `crates/atd-protocol/` (directory move via `git mv`)
- Modify: `crates/atd-protocol/Cargo.toml` (package name, description)
- Modify: `Cargo.toml` (workspace members list)
- Modify: `crates/atd-client/Cargo.toml`, `crates/atd-ref-server/Cargo.toml`, `crates/atd-cli/Cargo.toml`, `crates/atd-mcp-bridge/Cargo.toml`, `examples/Cargo.toml` (replace `atd-types` dep with `atd-protocol`)
- Rewrite `use atd_types` → `use atd_protocol` in 32 `.rs` files (batch `sed`).
- Create: `crates/atd-runtime/{Cargo.toml,src/lib.rs}`, `crates/atd-tools-echo/{Cargo.toml,src/lib.rs}`, `crates/atd-tools-fs/{Cargo.toml,src/lib.rs}`, `crates/atd-tools-shell/{Cargo.toml,src/lib.rs}`, `crates/atd-tools-web/{Cargo.toml,src/lib.rs}` (stubs)

**Why large:** C1 is the largest commit by file count (32 source rewrites + 6 Cargo.toml changes). Accept this per design §6.3.

- [ ] **Step 1: Rename the directory**

Run:
```bash
git mv crates/atd-types crates/atd-protocol
```
Expected: `crates/atd-protocol/` exists; `crates/atd-types/` gone.

- [ ] **Step 2: Update `crates/atd-protocol/Cargo.toml` package name and description**

Edit `crates/atd-protocol/Cargo.toml`:

```toml
[package]
name = "atd-protocol"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Protocol types, wire codec, and sanitization rules for the ATD (Agent Tool Dispatch) reference implementation."
readme = "README.md"
keywords = ["atd", "agent", "tool-dispatch", "protocol", "mcp"]
categories = ["api-bindings", "development-tools"]
exclude = ["tests/fixtures/*"]

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
```

(Preserve any existing `[dev-dependencies]` block.)

- [ ] **Step 3: Update root `Cargo.toml` workspace members**

Edit `/home/nan/proj/atd-mvp/Cargo.toml`:

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
    "crates/atd-mcp-bridge",
    "crates/atd-ref-server-bin",
    "examples",
]
```

IMPORTANT: `atd-sdk` and `atd-ref-server-bin` will not exist yet at the end of C1. Temporarily keep their **old** names in the members list for C1; we'll rename the member entries in C3 and C6 when the directories actually get renamed. So for C1 specifically, use this transitional form:

```toml
members = [
    "crates/atd-protocol",
    "crates/atd-client",
    "crates/atd-runtime",
    "crates/atd-tools-echo",
    "crates/atd-tools-fs",
    "crates/atd-tools-shell",
    "crates/atd-tools-web",
    "crates/atd-cli",
    "crates/atd-mcp-bridge",
    "crates/atd-ref-server",
    "examples",
]
```

- [ ] **Step 4: Rewrite `atd-types` → `atd-protocol` in 5 downstream Cargo.toml files**

Run one by one (inspect each diff):
```bash
sed -i 's|atd-types = { path = "../atd-types"|atd-protocol = { path = "../atd-protocol"|' crates/atd-client/Cargo.toml
sed -i 's|atd-types = { path = "../atd-types"|atd-protocol = { path = "../atd-protocol"|' crates/atd-ref-server/Cargo.toml
sed -i 's|atd-types = { path = "../atd-types"|atd-protocol = { path = "../atd-protocol"|' crates/atd-cli/Cargo.toml
sed -i 's|atd-types = { path = "../atd-types"|atd-protocol = { path = "../atd-protocol"|' crates/atd-mcp-bridge/Cargo.toml
sed -i 's|atd-types = { path = "../crates/atd-types"|atd-protocol = { path = "../crates/atd-protocol"|' examples/Cargo.toml
```

Verify after each:
```bash
git diff --stat crates/atd-client/Cargo.toml  # (repeat per file)
```
Expected: each file has exactly 1 line changed.

- [ ] **Step 5: Rewrite `use atd_types` → `use atd_protocol` across all source files**

Run:
```bash
find crates/ examples/ -name '*.rs' -exec sed -i 's/\batd_types\b/atd_protocol/g' {} +
```

- [ ] **Step 6: Verify every replacement occurred**

Run: `grep -rn "atd_types" crates/ examples/`
Expected: **no matches**. If any remain, inspect and fix manually (they may be inside comments or string literals; those should be updated too for consistency).

Run: `grep -rn "atd_protocol" crates/ examples/ | wc -l`
Expected: ≥ 32 matches (same count as previous `atd_types` hits).

- [ ] **Step 7: Scaffold `atd-runtime` crate**

Create `crates/atd-runtime/Cargo.toml`:

```toml
[package]
name = "atd-runtime"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Server-side runtime for the ATD protocol: Tool trait, Registry, dispatch, Binding, Middleware, capability gate."

[dependencies]
atd-protocol = { path = "../atd-protocol", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
thiserror = { workspace = true }
```

Create `crates/atd-runtime/src/lib.rs`:

```rust
//! ATD runtime — server-side abstractions (Tool trait, Registry, dispatch).
//!
//! Populated in Task 4 (C4) of the crate refactor plan.
```

- [ ] **Step 8: Scaffold the four `atd-tools-*` crates**

For each of `echo`, `fs`, `shell`, `web`, create `crates/atd-tools-<name>/Cargo.toml`:

```toml
[package]
name = "atd-tools-<name>"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Built-in <name> tools for the ATD reference runtime."

[dependencies]
atd-protocol = { path = "../atd-protocol", version = "0.1.0" }
atd-runtime = { path = "../atd-runtime", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
```

And `crates/atd-tools-<name>/src/lib.rs`:

```rust
//! ATD built-in tool crate — <name>. Populated in Task 5 (C5).
```

Note: `atd-tools-fs`, `atd-tools-shell`, `atd-tools-web` will gain their domain-specific deps (`ignore`, `globset`, `grep-*`, `reqwest`, etc.) in C5 when code moves in. For C1 the stub only needs the workspace basics above.

- [ ] **Step 9: Update root `Cargo.toml` to include the 5 new skeletons**

Update the `members` list to match the transitional set from Step 3 **plus** the five new crates:

```toml
[workspace]
resolver = "2"
members = [
    "crates/atd-protocol",
    "crates/atd-client",
    "crates/atd-runtime",
    "crates/atd-tools-echo",
    "crates/atd-tools-fs",
    "crates/atd-tools-shell",
    "crates/atd-tools-web",
    "crates/atd-cli",
    "crates/atd-mcp-bridge",
    "crates/atd-ref-server",
    "examples",
]
```

- [ ] **Step 10: Verify workspace compiles**

Run: `cargo check --workspace --all-features`
Expected: compiles cleanly. All warnings should be resolved; unused-crate warnings on the new stubs are acceptable.

- [ ] **Step 11: Run full regression gate**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```
Expected: all four succeed.

- [ ] **Step 12: Commit**

Run:
```bash
git add -A
git status --short  # sanity check: renames + 5 new crate dirs + Cargo.toml edits
git commit -m "refactor(crates): rename atd-types → atd-protocol; scaffold runtime + tool crates (C1)

- Rename crate directory and package name atd-types → atd-protocol.
- Update all downstream Cargo.toml deps (atd-client, atd-ref-server,
  atd-cli, atd-mcp-bridge, examples).
- Rewrite use atd_types → use atd_protocol across 32 source files.
- Scaffold empty atd-runtime + atd-tools-{echo,fs,shell,web} crates;
  wiring happens in C4 and C5.
- No code moves yet. Workspace green.

Refs: docs/superpowers/specs/2026-04-24-crate-refactor-design.md §6 C1"
```

---

## Task 2 (C2): Fill `atd-protocol` with wire + messages + sanitize

**Files:**
- Move: `crates/atd-client/src/wire.rs` → `crates/atd-protocol/src/wire.rs`
- Move+rename: `crates/atd-client/src/protocol.rs` → `crates/atd-protocol/src/messages.rs`
- Move: `crates/atd-client/src/sanitize.rs` → `crates/atd-protocol/src/sanitize.rs`
- Delete: `crates/atd-ref-server/src/protocol.rs`, `crates/atd-ref-server/src/wire.rs`
- Modify: `crates/atd-protocol/src/lib.rs` (add `pub mod wire; pub mod messages; pub mod sanitize;` + re-exports)
- Modify: `crates/atd-client/src/lib.rs` (remove `pub mod wire; pub mod protocol; pub mod sanitize;`; add matching re-exports from `atd_protocol::*` to preserve the old public path for C2's temporary state — see Step 6)
- Modify internal imports in 5 `.rs` files: `crates/atd-client/src/{client,adapters/*}.rs` (switch `use crate::{wire,protocol,sanitize}` → `use atd_protocol::{wire,messages,sanitize}`)
- Modify internal imports in `crates/atd-ref-server/src/server.rs` (only file that uses `use crate::{protocol,wire}` in ref-server)
- Modify `crates/atd-protocol/Cargo.toml`: add `tokio = { workspace = true }` (needed by `wire.rs` async frame codec)

- [ ] **Step 1: Update `crates/atd-protocol/Cargo.toml` to add tokio dep**

Edit `crates/atd-protocol/Cargo.toml` `[dependencies]`:

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
```

(wire.rs uses `AsyncReadExt` / `AsyncWriteExt`, so tokio is required at the protocol layer.)

- [ ] **Step 2: Move the three source files**

Run:
```bash
git mv crates/atd-client/src/wire.rs      crates/atd-protocol/src/wire.rs
git mv crates/atd-client/src/protocol.rs  crates/atd-protocol/src/messages.rs
git mv crates/atd-client/src/sanitize.rs  crates/atd-protocol/src/sanitize.rs
```

- [ ] **Step 3: Delete the duplicate ref-server copies**

Run:
```bash
git rm crates/atd-ref-server/src/protocol.rs
git rm crates/atd-ref-server/src/wire.rs
```

- [ ] **Step 4: Update `crates/atd-protocol/src/lib.rs`**

Replace the entire contents of `crates/atd-protocol/src/lib.rs` with:

```rust
//! ATD protocol layer — the spec.
//!
//! Shared between SDK (`atd-sdk`) and runtime (`atd-runtime`); depends on
//! neither. Contains types, wire codec, and sanitization rules that third-
//! party implementations must match byte-for-byte.

pub mod enums;
pub mod error;
pub mod messages;
pub mod result;
pub mod sanitize;
pub mod summary;
pub mod tool;
pub mod wire;

pub use enums::{BindingProtocol, SafetyLevel, ToolTier, ToolVisibility, TrustLevel};
pub use error::AtdError;
pub use messages::{ERR_CAPABILITY_DENIED, Request, Response};
pub use result::{ToolResult, ToolResultMetadata};
pub use sanitize::{desanitize_tool_name, detect_collisions, sanitize_tool_name};
pub use summary::ToolSummary;
pub use tool::{ToolBinding, ToolCapability, ToolDefinition, ToolResources, ToolSafety, ToolTrust};
```

(Verify each re-export symbol actually exists in the moved files; adjust if a symbol has a different path. The pre-C2 `atd-client/src/lib.rs` and `atd-types/src/lib.rs` are the ground-truth sources for exports — combine them.)

- [ ] **Step 5: Rewrite `crates/atd-client/src/lib.rs` to drop moved modules**

Read `crates/atd-client/src/lib.rs` first. The pre-C2 file declares `pub mod wire; pub mod protocol; pub mod sanitize;` and re-exports from them. Remove those three `pub mod` lines. If there are `pub use sanitize::{...}` re-exports at crate root, convert them to re-exports from `atd_protocol` instead:

```rust
// BEFORE (pre-C2):
// pub mod wire;
// pub mod protocol;
// pub mod sanitize;
// pub use sanitize::{desanitize_tool_name, sanitize_tool_name};

// AFTER (post-C2):
pub use atd_protocol::{desanitize_tool_name, sanitize_tool_name};
pub use atd_protocol::wire;
pub use atd_protocol::messages as protocol;  // temporary alias preserves old path for external readers during C2
```

The `messages as protocol` alias is intentional: external consumers still write `atd_client::protocol::Request`; we'll drop it in C3 when we rename to `atd-sdk` (at which point old paths break anyway).

- [ ] **Step 6: Rewrite `atd-client` internal imports**

In each of the following files, find `use crate::{wire,protocol,sanitize}` patterns and rewrite to `atd_protocol`:

- `crates/atd-client/src/client.rs`
- `crates/atd-client/src/adapters/mod.rs`
- `crates/atd-client/src/adapters/openai.rs`
- `crates/atd-client/src/adapters/anthropic.rs`
- `crates/atd-client/src/adapters/langchain.rs`

Pattern:
```rust
// BEFORE
use crate::protocol::{Request, Response, ERR_CAPABILITY_DENIED};
use crate::wire::{read_frame, write_frame};
use crate::sanitize::sanitize_tool_name;

// AFTER
use atd_protocol::messages::{Request, Response, ERR_CAPABILITY_DENIED};
use atd_protocol::wire::{read_frame, write_frame};
use atd_protocol::sanitize::sanitize_tool_name;
```

Use targeted `sed` for the bulk change:
```bash
for f in crates/atd-client/src/client.rs crates/atd-client/src/adapters/*.rs; do
    sed -i 's|use crate::protocol::|use atd_protocol::messages::|g' "$f"
    sed -i 's|use crate::wire::|use atd_protocol::wire::|g' "$f"
    sed -i 's|use crate::sanitize::|use atd_protocol::sanitize::|g' "$f"
done
```

Verify:
```bash
grep -rn "use crate::\(wire\|protocol\|sanitize\)" crates/atd-client/src/
```
Expected: no matches.

- [ ] **Step 7: Rewrite `atd-ref-server` imports of its now-deleted local `protocol` / `wire` modules**

File: `crates/atd-ref-server/src/server.rs` (only offending file per grep).

```bash
sed -i 's|use crate::protocol::|use atd_protocol::messages::|g' crates/atd-ref-server/src/server.rs
sed -i 's|use crate::wire::|use atd_protocol::wire::|g' crates/atd-ref-server/src/server.rs
```

Any other file that internally imported `crate::protocol` or `crate::wire` inside ref-server must be updated too — run a final check:
```bash
grep -rn "use crate::\(protocol\|wire\)" crates/atd-ref-server/src/
```
Expected: no matches. If any appear, apply the same `sed` rewrite.

- [ ] **Step 8: Remove `pub mod protocol;` / `pub mod wire;` from `crates/atd-ref-server/src/lib.rs`**

Edit `crates/atd-ref-server/src/lib.rs` — delete the two lines `pub mod protocol;` and `pub mod wire;` (those modules were deleted in Step 3). Keep every other `pub mod` intact.

- [ ] **Step 9: Verify the workspace builds**

Run: `cargo check --workspace --all-features`
Expected: clean compile.

Common failure: a symbol re-exported by `crates/atd-protocol/src/lib.rs` doesn't actually exist under the path named. The compile error will pinpoint the offending `pub use` — fix by correcting the module path or removing the re-export if the symbol is internal-only.

- [ ] **Step 10: Run full regression gate**

Run:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```
Expected: all four succeed. In particular the round-trip wire tests in `crates/atd-client/tests/` and `crates/atd-ref-server/tests/` must still pass — they are the byte-compatibility guarantee.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "refactor(atd-protocol): consolidate wire + messages + sanitize (C2)

- Move wire.rs, protocol.rs (→messages.rs), sanitize.rs from atd-client
  into atd-protocol.
- Delete duplicate protocol.rs and wire.rs from atd-ref-server (ref-server
  now depends on atd-protocol for wire types, eliminating the independent
  duplicate implementation from the original design).
- atd-client keeps temporary 'protocol' alias (= messages) + wire +
  sanitize re-exports so external crate path atd_client::protocol::Request
  still resolves during C2. Alias is dropped when atd-client is renamed
  to atd-sdk in C3.
- Workspace green; wire round-trip tests unchanged.

Refs: docs/superpowers/specs/2026-04-24-crate-refactor-design.md §6 C2"
```

---

## Task 3 (C3): Rename `atd-client` → `atd-sdk`

**Files:**
- Rename: `crates/atd-client/` → `crates/atd-sdk/` (directory)
- Modify: `crates/atd-sdk/Cargo.toml` (`name`, description, keywords — update `atd-client` → `atd-sdk` wherever referenced)
- Modify: `Cargo.toml` (workspace members swap `crates/atd-client` → `crates/atd-sdk`)
- Modify: `crates/atd-cli/Cargo.toml`, `crates/atd-mcp-bridge/Cargo.toml`, `examples/Cargo.toml` (replace `atd-client` dep with `atd-sdk`)
- Rewrite `use atd_client` → `use atd_sdk` in 11 `.rs` files
- Delete the temporary `protocol` alias introduced in C2 Step 5 (no longer needed since path is already breaking)

- [ ] **Step 1: Rename the directory**

```bash
git mv crates/atd-client crates/atd-sdk
```

- [ ] **Step 2: Update `crates/atd-sdk/Cargo.toml`**

Edit `crates/atd-sdk/Cargo.toml`:

```toml
[package]
name = "atd-sdk"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Rust SDK for the ATD (Agent Tool Dispatch) protocol — connect to any ATD-speaking server over Unix sockets."
readme = "README.md"
keywords = ["atd", "agent", "sdk", "client", "mcp"]
categories = ["api-bindings", "asynchronous"]
exclude = ["tests/fixtures/*", "benches/*"]
```

Leave `[dependencies]`, `[features]`, `[dev-dependencies]` untouched.

- [ ] **Step 3: Update root `Cargo.toml` workspace members**

Change `"crates/atd-client"` to `"crates/atd-sdk"` in the `members` list.

- [ ] **Step 4: Update downstream Cargo.toml deps**

```bash
sed -i 's|atd-client = { path = "../atd-client"|atd-sdk = { path = "../atd-sdk"|' crates/atd-cli/Cargo.toml
sed -i 's|atd-client = { path = "../atd-client"|atd-sdk = { path = "../atd-sdk"|' crates/atd-mcp-bridge/Cargo.toml
sed -i 's|atd-client = { path = "../crates/atd-client"|atd-sdk = { path = "../crates/atd-sdk"|' examples/Cargo.toml
```

Feature flag passthroughs in `examples/Cargo.toml`:
```bash
sed -i 's|"atd-client/|"atd-sdk/|g' examples/Cargo.toml
```

Verify:
```bash
grep -rn "atd-client" crates/ examples/ Cargo.toml
```
Expected: no matches.

- [ ] **Step 5: Rewrite `use atd_client` → `use atd_sdk` across all source files**

```bash
find crates/ examples/ -name '*.rs' -exec sed -i 's/\batd_client\b/atd_sdk/g' {} +
```

Verify:
```bash
grep -rn "atd_client" crates/ examples/
```
Expected: no matches.

- [ ] **Step 6: Drop the temporary `protocol` alias from `crates/atd-sdk/src/lib.rs`**

Remove the line `pub use atd_protocol::messages as protocol;` that was added in C2 Step 5. The `use atd_protocol::messages::*` path is now the only way — no external consumer has stabilized on `atd_sdk::protocol` yet (SDK name is new).

- [ ] **Step 7: Update `atd-sdk` README** (if present)

If `crates/atd-sdk/README.md` exists (previously `atd-client/README.md`), edit occurrences of `atd-client` → `atd-sdk`. Do not touch historical change logs.

```bash
grep -n "atd-client" crates/atd-sdk/README.md 2>/dev/null
# If any matches, open the file and update them manually (preserve any historical context lines).
```

- [ ] **Step 8: Regression gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor(atd-sdk): rename atd-client → atd-sdk (C3)

- Rename crate dir, package name, README content.
- Update downstream Cargo.toml deps and feature passthroughs in atd-cli,
  atd-mcp-bridge, examples.
- Rewrite use atd_client → use atd_sdk in 11 source files.
- Drop temporary 'protocol' alias from lib.rs — consumers use
  atd_protocol::messages directly now.

Refs: docs/superpowers/specs/2026-04-24-crate-refactor-design.md §6 C3"
```

---

## Task 4 (C4): Extract `atd-runtime` from `atd-ref-server`

**Files:**
- Move 8 files from `crates/atd-ref-server/src/` to `crates/atd-runtime/src/`:
  - `binding.rs`, `capability.rs`, `context.rs`, `error.rs`, `middleware.rs`, `registry.rs`, `tier.rs`, `tracker.rs`
- Modify: `crates/atd-runtime/src/lib.rs` (populate with `pub mod` + `pub use` for the 8 moved files)
- Modify: `crates/atd-ref-server/src/lib.rs` (remove `pub mod` for the 8 modules; keep only `pub mod builtin; pub mod server; pub mod tools;`)
- Modify: `crates/atd-ref-server/Cargo.toml` (add `atd-runtime = { path = "../atd-runtime", version = "0.1.0" }`)
- Rewrite internal imports: any file inside `atd-ref-server` that imports `use crate::{binding,capability,context,error,middleware,registry,tier,tracker}` (15 files per grep) → `use atd_runtime::{…}`
- Rewrite integration tests: `crates/atd-ref-server/tests/*.rs` (5 files per grep) `use atd_ref_server::{…}` paths that reference the moved modules → `use atd_runtime::{…}`

- [ ] **Step 1: Move the 8 source files**

```bash
mkdir -p crates/atd-runtime/src
git mv crates/atd-ref-server/src/binding.rs    crates/atd-runtime/src/binding.rs
git mv crates/atd-ref-server/src/capability.rs crates/atd-runtime/src/capability.rs
git mv crates/atd-ref-server/src/context.rs    crates/atd-runtime/src/context.rs
git mv crates/atd-ref-server/src/error.rs      crates/atd-runtime/src/error.rs
git mv crates/atd-ref-server/src/middleware.rs crates/atd-runtime/src/middleware.rs
git mv crates/atd-ref-server/src/registry.rs   crates/atd-runtime/src/registry.rs
git mv crates/atd-ref-server/src/tier.rs       crates/atd-runtime/src/tier.rs
git mv crates/atd-ref-server/src/tracker.rs    crates/atd-runtime/src/tracker.rs
```

- [ ] **Step 2: Populate `crates/atd-runtime/src/lib.rs`**

Overwrite with:

```rust
//! ATD runtime — server-side abstractions.
//!
//! `Tool` trait, `Registry`, dispatch, `Binding`, `Middleware`, capability
//! gate, tier policy, read tracker. Depends only on `atd-protocol`.

pub mod binding;
pub mod capability;
pub mod context;
pub mod error;
pub mod middleware;
pub mod registry;
pub mod tier;
pub mod tracker;

pub use binding::{Binding, CliBinding, NativeBinding};
pub use capability::CapabilitySet;
pub use context::CallContext;
pub use error::ToolCallError;
pub use middleware::{Middleware, RedactPathsMiddleware};
pub use registry::{RegisteredTool, Registry, Tool};
pub use tier::{tier_from_opt_str, TierPolicy};
pub use tracker::{ReadTracker, ReadTrackerError};
```

(Verify each re-exported symbol exists; adjust if a module uses different names — the ground truth is the pre-move source files.)

- [ ] **Step 3: Add `atd-runtime` dep to `crates/atd-ref-server/Cargo.toml`**

Insert under `[dependencies]`:

```toml
atd-runtime = { path = "../atd-runtime", version = "0.1.0" }
```

- [ ] **Step 4: Remove the 8 `pub mod` lines from `crates/atd-ref-server/src/lib.rs`**

Edit `crates/atd-ref-server/src/lib.rs` — delete lines declaring the 8 now-moved modules. After edit, `lib.rs` should only declare `pub mod builtin; pub mod server; pub mod tools;` (and whatever else was there that is not in the moved list).

- [ ] **Step 5: Rewrite internal `use crate::{binding,capability,…}` in the 15 affected files**

Batch rewrite for each moved module name:

```bash
for mod in binding capability context error middleware registry tier tracker; do
    find crates/atd-ref-server/src -name '*.rs' -exec \
        sed -i "s|use crate::$mod::|use atd_runtime::$mod::|g" {} +
    find crates/atd-ref-server/src -name '*.rs' -exec \
        sed -i "s|crate::$mod::|atd_runtime::$mod::|g" {} +
done
```

The second form (without `use`) catches any fully-qualified path references.

Verify:
```bash
grep -rnE "crate::(binding|capability|context|error|middleware|registry|tier|tracker)" crates/atd-ref-server/src/
```
Expected: no matches.

- [ ] **Step 6: Rewrite `atd-ref-server` integration tests to use `atd_runtime` directly**

The 5 integration tests in `crates/atd-ref-server/tests/` import `use atd_ref_server::{binding,capability,middleware,tier,tracker,…}`. Redirect to `atd_runtime`:

```bash
for mod in binding capability context error middleware registry tier tracker; do
    find crates/atd-ref-server/tests -name '*.rs' -exec \
        sed -i "s|use atd_ref_server::$mod::|use atd_runtime::$mod::|g" {} +
    find crates/atd-ref-server/tests -name '*.rs' -exec \
        sed -i "s|atd_ref_server::$mod::|atd_runtime::$mod::|g" {} +
done
```

Also update the `rw_cycle` example inside the ref-server crate:
```bash
for mod in binding capability context error middleware registry tier tracker; do
    sed -i "s|use atd_ref_server::$mod::|use atd_runtime::$mod::|g" crates/atd-ref-server/examples/rw_cycle.rs
    sed -i "s|atd_ref_server::$mod::|atd_runtime::$mod::|g" crates/atd-ref-server/examples/rw_cycle.rs
done
```

Add `atd-runtime` to `[dev-dependencies]` of `crates/atd-ref-server/Cargo.toml`:
```toml
[dev-dependencies]
atd-runtime = { path = "../atd-runtime", version = "0.1.0" }
```
(If it's already present in `[dependencies]`, the dev-deps entry is not strictly needed — but keeping both explicit is clearer.)

Actually — when a crate depends on `atd-runtime` as a regular dep, dev tests can use it too. Skip duplicating unless `cargo test` reports "unresolved import".

Verify:
```bash
grep -rnE "atd_ref_server::(binding|capability|context|error|middleware|registry|tier|tracker)" crates/
```
Expected: no matches.

- [ ] **Step 7: Regression gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```

If tests fail with "unresolved import" for symbols like `Tool` or `Registry`: verify `crates/atd-runtime/src/lib.rs` Step 2 actually re-exports them at crate root.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(atd-runtime): extract from atd-ref-server (C4)

- Move 8 files to atd-runtime: binding, capability, context, error,
  middleware, registry, tier, tracker.
- atd-ref-server gains atd-runtime dep; 15 source sites + 5 test sites +
  rw_cycle example rewrite crate:: and atd_ref_server:: paths to
  atd_runtime::.
- lib.rs of atd-ref-server drops pub mod for moved items.
- No behavior change; same tests pass.

Refs: docs/superpowers/specs/2026-04-24-crate-refactor-design.md §6 C4"
```

---

## Task 5 (C5): Extract 4 `atd-tools-*` crates

**Files:**
- Move 10 files from `crates/atd-ref-server/src/tools/` into the per-domain tool crates:
  - `tools/echo.rs` → `atd-tools-echo/src/lib.rs` (single-file crate)
  - `tools/fs/{edit,glob,grep,read,write,shared,mod}.rs` (7 files) → `atd-tools-fs/src/`
  - `tools/shell/{exec,pwsh,shared,mod}.rs` (4 files) → `atd-tools-shell/src/`
  - `tools/web/{fetch,mod}.rs` (2 files) → `atd-tools-web/src/`
- Modify: the 4 new tool crates' `Cargo.toml` to add domain-specific deps
- Modify: `crates/atd-tools-<name>/src/lib.rs` — replace stubs with real module tree and `pub use`
- Modify: `crates/atd-ref-server/Cargo.toml` — add `atd-tools-{echo,fs,shell,web}` as deps; remove no-longer-used domain deps (`ignore`, `globset`, `grep-searcher`, `grep-regex`, `regex`, `reqwest`, `htmd`, `url`) that have moved into the per-tool crates
- Modify: `crates/atd-ref-server/src/builtin.rs` — rewrite `use crate::tools::{…}` → `use atd_tools_{…}::…`
- Modify: `crates/atd-ref-server/src/tools/mod.rs` — remove `pub mod echo; pub mod fs; pub mod shell; pub mod web;` (only `pub mod external;` should remain with its `#[cfg(unix)]` gate). OR — if `tools/external/` is the only survivor — delete `tools/mod.rs` and hoist `external/` up to `src/external/` (we do this in C6, not here).

**Why ordered this way:** `tools/external/uname.rs` currently imports `use crate::{binding,registry,…}` → those are now `atd_runtime::{…}` after C4. External stays inside ref-server through C5; we relocate it to `src/external/` in C6. In C5, only echo/fs/shell/web move out.

- [ ] **Step 1: Move `tools/echo.rs` → `atd-tools-echo/src/lib.rs`**

```bash
git mv crates/atd-ref-server/src/tools/echo.rs crates/atd-tools-echo/src/lib.rs
```

Open `crates/atd-tools-echo/src/lib.rs`. At the top, add a crate-level doc comment to replace the stub:

```rust
//! Echo tool — test-anchor reference tool.
//!
//! Ships with atd-ref-server; the smallest real `Tool` implementation,
//! useful for wire round-trip tests and documentation examples.
```

If any `use crate::{registry,context,…}` references remain in the file, rewrite them:
```bash
for mod in binding capability context error middleware registry tier tracker; do
    sed -i "s|use crate::$mod::|use atd_runtime::$mod::|g" crates/atd-tools-echo/src/lib.rs
    sed -i "s|crate::$mod::|atd_runtime::$mod::|g" crates/atd-tools-echo/src/lib.rs
done
```

- [ ] **Step 2: Update `crates/atd-tools-echo/Cargo.toml`**

The stub from C1 already has `atd-protocol`, `atd-runtime`, `serde`, `serde_json`, `tokio`. Echo needs nothing more.

- [ ] **Step 3: Move `tools/fs/*` → `atd-tools-fs/src/*`**

```bash
rmdir crates/atd-tools-fs/src 2>/dev/null || true  # remove stub src/ if empty
mkdir -p crates/atd-tools-fs/src
for f in edit glob grep read write shared mod; do
    if [ -f "crates/atd-ref-server/src/tools/fs/$f.rs" ]; then
        git mv "crates/atd-ref-server/src/tools/fs/$f.rs" "crates/atd-tools-fs/src/$f.rs"
    fi
done
# mod.rs contained `pub mod read; pub mod write; ...` — turn it into the lib.rs
git mv crates/atd-tools-fs/src/mod.rs crates/atd-tools-fs/src/lib.rs
```

If the old `mod.rs` lacked a crate-level doc comment, prepend one:
```rust
//! Filesystem tools: read, write, edit, glob, grep.
//!
//! Byte-exact semantics with sanitize rules in atd-protocol; tree-walk
//! uses `ignore` for gitignore-aware traversal and `grep-*` for content
//! search.
```

- [ ] **Step 4: Rewrite `atd-tools-fs` internal imports**

Inside each moved file, `use crate::{binding,context,registry,…}` refers to atd-runtime; any `use crate::tools::fs::shared` refers to sibling:

```bash
for mod in binding capability context error middleware registry tier tracker; do
    find crates/atd-tools-fs/src -name '*.rs' -exec \
        sed -i "s|use crate::$mod::|use atd_runtime::$mod::|g" {} +
    find crates/atd-tools-fs/src -name '*.rs' -exec \
        sed -i "s|crate::$mod::|atd_runtime::$mod::|g" {} +
done

# Sibling references: crate::tools::fs::shared → crate::shared now that we're inside atd-tools-fs
sed -i 's|crate::tools::fs::shared|crate::shared|g' crates/atd-tools-fs/src/*.rs
sed -i 's|crate::tools::fs::|crate::|g' crates/atd-tools-fs/src/*.rs
```

Verify:
```bash
grep -rnE "crate::(binding|capability|context|error|middleware|registry|tier|tracker|tools::fs)" crates/atd-tools-fs/src/
```
Expected: no matches.

- [ ] **Step 5: Update `crates/atd-tools-fs/Cargo.toml`**

Replace `[dependencies]` with:

```toml
[dependencies]
atd-protocol = { path = "../atd-protocol", version = "0.1.0" }
atd-runtime = { path = "../atd-runtime", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
ignore = "0.4"
globset = "0.4"
grep-searcher = "0.1"
grep-regex = "0.1"
regex = "1"
```

(Domain deps match what ref-server's pre-refactor `Cargo.toml` declared for fs tool work.)

- [ ] **Step 6: Move `tools/shell/*` → `atd-tools-shell/src/*` (analogous to Step 3–5 for shell)**

```bash
mkdir -p crates/atd-tools-shell/src
for f in exec pwsh shared mod; do
    if [ -f "crates/atd-ref-server/src/tools/shell/$f.rs" ]; then
        git mv "crates/atd-ref-server/src/tools/shell/$f.rs" "crates/atd-tools-shell/src/$f.rs"
    fi
done
git mv crates/atd-tools-shell/src/mod.rs crates/atd-tools-shell/src/lib.rs
```

Prepend crate-level doc if missing:
```rust
//! Shell tools: shell.exec (/bin/sh) and shell.pwsh (PowerShell).
//!
//! Subprocess execution with configurable timeouts; shared capture helper
//! reused across both tools.
```

Rewrite imports:
```bash
for mod in binding capability context error middleware registry tier tracker; do
    find crates/atd-tools-shell/src -name '*.rs' -exec \
        sed -i "s|use crate::$mod::|use atd_runtime::$mod::|g" {} +
    find crates/atd-tools-shell/src -name '*.rs' -exec \
        sed -i "s|crate::$mod::|atd_runtime::$mod::|g" {} +
done
sed -i 's|crate::tools::shell::shared|crate::shared|g' crates/atd-tools-shell/src/*.rs
sed -i 's|crate::tools::shell::|crate::|g' crates/atd-tools-shell/src/*.rs
```

Update `crates/atd-tools-shell/Cargo.toml`:
```toml
[dependencies]
atd-protocol = { path = "../atd-protocol", version = "0.1.0" }
atd-runtime = { path = "../atd-runtime", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["process", "io-util", "rt-multi-thread", "macros", "time"] }

[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

(`libc` was used by shell code for Unix-specific exit-code handling; verify by inspecting the moved `exec.rs`/`shared.rs`. If not needed, drop the target-specific dep block.)

- [ ] **Step 7: Move `tools/web/*` → `atd-tools-web/src/*`**

```bash
mkdir -p crates/atd-tools-web/src
for f in fetch mod; do
    if [ -f "crates/atd-ref-server/src/tools/web/$f.rs" ]; then
        git mv "crates/atd-ref-server/src/tools/web/$f.rs" "crates/atd-tools-web/src/$f.rs"
    fi
done
git mv crates/atd-tools-web/src/mod.rs crates/atd-tools-web/src/lib.rs
```

Prepend doc:
```rust
//! Web tools: web.fetch (HTTP GET with HTML-to-markdown conversion).
```

Rewrite:
```bash
for mod in binding capability context error middleware registry tier tracker; do
    find crates/atd-tools-web/src -name '*.rs' -exec \
        sed -i "s|use crate::$mod::|use atd_runtime::$mod::|g" {} +
    find crates/atd-tools-web/src -name '*.rs' -exec \
        sed -i "s|crate::$mod::|atd_runtime::$mod::|g" {} +
done
sed -i 's|crate::tools::web::|crate::|g' crates/atd-tools-web/src/*.rs
```

Update `crates/atd-tools-web/Cargo.toml`:
```toml
[dependencies]
atd-protocol = { path = "../atd-protocol", version = "0.1.0" }
atd-runtime = { path = "../atd-runtime", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "gzip", "brotli"] }
htmd = "0.5"
url = "2"
```

- [ ] **Step 8: Update `crates/atd-ref-server/src/tools/mod.rs`**

The pre-C5 `mod.rs` declared `pub mod echo; pub mod fs; pub mod shell; pub mod web;` plus `#[cfg(unix)] pub mod external;`. After C5, only `external` remains. Overwrite:

```rust
//! Remaining built-in tools shipped inside atd-ref-server-bin after
//! the tool-crate extraction (C5). The echo/fs/shell/web tools live in
//! their own `atd-tools-*` crates; only `external` (SP-12 CliBinding
//! demo) stays local — it's a binding demo, not a reusable tool crate.

#[cfg(unix)]
pub mod external;
```

(In C6 we move `tools/external/` → `src/external/` and delete the `tools/` directory entirely; for C5 we keep the intermediate shape.)

- [ ] **Step 9: Update `crates/atd-ref-server/src/builtin.rs`**

Rewrite the `use` imports at the top:

```rust
//! Built-in tool registration for `atd-ref-server`.

use std::sync::Arc;

use atd_runtime::registry::Registry;
use atd_tools_echo::EchoTool;
use atd_tools_fs::{FsEditTool, FsGlobTool, FsGrepTool, FsReadTool, FsWriteTool};
use atd_tools_shell::{ShellExecTool, ShellPwshTool};
use atd_tools_web::WebFetchTool;

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
    reg.register(Arc::new(WebFetchTool::new()));

    #[cfg(unix)]
    {
        use crate::tools::external::uname;
        let stub = Arc::new(uname::UnameStub::new());
        let binding = Arc::new(uname::cli_binding());
        reg.register_with_binding(stub, binding);
    }

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_tools() {
        let r = builtin_registry();
        #[cfg(unix)]
        assert_eq!(r.count(), 10);
        #[cfg(not(unix))]
        assert_eq!(r.count(), 9);
        assert!(r.get("ref:echo.say").is_some());
        assert!(r.get("ref:fs.read").is_some());
        assert!(r.get("ref:fs.write").is_some());
        assert!(r.get("ref:fs.edit").is_some());
        assert!(r.get("ref:fs.glob").is_some());
        assert!(r.get("ref:fs.grep").is_some());
        assert!(r.get("ref:shell.exec").is_some());
        assert!(r.get("ref:shell.pwsh").is_some());
        assert!(r.get("ref:web.fetch").is_some());
        #[cfg(unix)]
        {
            let entry = r
                .get("ref:external.uname")
                .expect("uname registered on unix");
            assert_eq!(entry.binding.name(), "cli");
        }
    }
}
```

The each `use atd_tools_<name>::<Tool>` assumes the corresponding tool crate re-exports the type at crate root. Verify by checking the four `crates/atd-tools-*/src/lib.rs` files: each should have `pub use read::FsReadTool;` (etc.) entries. If missing, add them per the enumeration used in `builtin.rs`.

- [ ] **Step 10: Add the four tool crate deps to `crates/atd-ref-server/Cargo.toml`**

Under `[dependencies]`:
```toml
atd-tools-echo = { path = "../atd-tools-echo", version = "0.1.0" }
atd-tools-fs = { path = "../atd-tools-fs", version = "0.1.0" }
atd-tools-shell = { path = "../atd-tools-shell", version = "0.1.0" }
atd-tools-web = { path = "../atd-tools-web", version = "0.1.0" }
```

Remove domain-specific deps no longer used inside ref-server proper (they've moved into the tool crates):
```toml
# DELETE these lines from crates/atd-ref-server/Cargo.toml:
# ignore = "0.4"
# globset = "0.4"
# grep-searcher = "0.1"
# grep-regex = "0.1"
# regex = "1"
# reqwest = { version = "0.12", default-features = false, features = [...] }
# htmd = "0.5"
# url = "2"
```

Keep: `atd-protocol`, `atd-runtime`, `atd-tools-*` (×4), `serde`, `serde_json`, `tokio`, `thiserror`, `ulid`, `clap`, `libc` (for unix uname cfg in `external/`).

- [ ] **Step 11: Regression gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```

Expected pitfalls:
- Tool symbol not re-exported at crate root → add `pub use <module>::<Type>;` to the tool crate's `lib.rs`.
- `libc` missing from `atd-tools-shell` if shell code uses unix exit-status APIs → add per Step 6.
- Path alias in any moved file that still references `crate::tools::<something>` → rerun Step 4/6/7 sed snippets.

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "refactor(atd-tools-*): split built-in tools out of atd-ref-server (C5)

- Move 13 files into atd-tools-echo (1), atd-tools-fs (7), atd-tools-shell
  (4), atd-tools-web (2).
- Each tool crate depends on atd-protocol + atd-runtime; domain deps
  (ignore/globset/grep-*/regex/reqwest/htmd/url) move with their tools.
- atd-ref-server gains four atd-tools-* deps; drops the moved domain deps.
- builtin.rs rewires to use atd_tools_* symbols.
- tools/external stays in-place (moves to src/external in C6).
- Workspace green; all integration tests unchanged.

Refs: docs/superpowers/specs/2026-04-24-crate-refactor-design.md §6 C5"
```

---

## Task 6 (C6): Rename `atd-ref-server` → `atd-ref-server-bin` + relocate `external/`

**Files:**
- Rename: `crates/atd-ref-server/` → `crates/atd-ref-server-bin/` (directory)
- Modify: `crates/atd-ref-server-bin/Cargo.toml` — `name = "atd-ref-server-bin"`; keep `[[bin]] name = "atd-ref-server"`.
- Modify: `Cargo.toml` (workspace members: `crates/atd-ref-server` → `crates/atd-ref-server-bin`)
- Move: `crates/atd-ref-server-bin/src/tools/external/` → `crates/atd-ref-server-bin/src/external/`
- Delete: `crates/atd-ref-server-bin/src/tools/` (now empty apart from external)
- Modify: `crates/atd-ref-server-bin/src/lib.rs` — remove `pub mod tools;` if present; add `#[cfg(unix)] pub mod external;`
- Modify: `crates/atd-ref-server-bin/src/builtin.rs` — `use crate::tools::external::uname;` → `use crate::external::uname;`
- Modify: `crates/atd-ref-server-bin/src/main.rs` — `use atd_ref_server::…` → `use atd_ref_server_bin::…` (5 lines per grep)
- Modify: `crates/atd-ref-server-bin/examples/rw_cycle.rs` — same self-reference rewrite (2 lines)
- Modify: `crates/atd-ref-server-bin/tests/*.rs` — any `use atd_ref_server::*` (after C4 rewrites, these should only be `atd_ref_server::server::*`, `atd_ref_server::builtin::*`, or similar bin-local paths) → `use atd_ref_server_bin::…`
- Modify: `crates/atd-mcp-bridge/tests/integration_e2e.rs` — update doc comments and `-p atd-ref-server` → `-p atd-ref-server-bin` (**keep** the binary path `target/release/atd-ref-server` — binary name preserved)

- [ ] **Step 1: Rename the directory**

```bash
git mv crates/atd-ref-server crates/atd-ref-server-bin
```

- [ ] **Step 2: Update `crates/atd-ref-server-bin/Cargo.toml`**

Change the `name` and leave `[[bin]]` untouched:

```toml
[package]
name = "atd-ref-server-bin"
# ... rest of existing metadata ...
description = "Reference server binary for the Agent Tool Dispatch (ATD) protocol — wires atd-runtime + atd-tools-* into an installable executable."

[lib]
name = "atd_ref_server_bin"
path = "src/lib.rs"

[[bin]]
name = "atd-ref-server"     # binary name preserved (end-user command unchanged)
path = "src/main.rs"
```

Ensure `[lib] name` is explicitly set so `use atd_ref_server_bin::*` works; default lib name would be `atd_ref_server_bin` anyway (hyphens → underscores), but being explicit avoids confusion with the binary name.

- [ ] **Step 3: Update root `Cargo.toml` workspace members**

`"crates/atd-ref-server"` → `"crates/atd-ref-server-bin"`.

- [ ] **Step 4: Hoist `tools/external/` → `src/external/`**

```bash
git mv crates/atd-ref-server-bin/src/tools/external crates/atd-ref-server-bin/src/external
# tools/ should now be empty except for mod.rs containing only `#[cfg(unix)] pub mod external;`
git rm crates/atd-ref-server-bin/src/tools/mod.rs
rmdir crates/atd-ref-server-bin/src/tools  # fails harmlessly if git already cleaned it
```

- [ ] **Step 5: Update `crates/atd-ref-server-bin/src/lib.rs`**

Remove `pub mod tools;` (if present). Add `#[cfg(unix)] pub mod external;`. Result should look like:

```rust
//! Reference server binary — wires atd-runtime + atd-tools-* + SP-12
//! CliBinding demo (external::uname, unix-only) into an executable.

pub mod builtin;
pub mod server;

#[cfg(unix)]
pub mod external;
```

- [ ] **Step 6: Update `crates/atd-ref-server-bin/src/builtin.rs`**

Inside the `#[cfg(unix)] { … }` block, change:
```rust
// BEFORE
use crate::tools::external::uname;
// AFTER
use crate::external::uname;
```

Run:
```bash
sed -i 's|use crate::tools::external::uname|use crate::external::uname|g' crates/atd-ref-server-bin/src/builtin.rs
```

- [ ] **Step 7: Rewrite `atd_ref_server::*` self-references → `atd_ref_server_bin::*`**

```bash
find crates/atd-ref-server-bin -name '*.rs' -exec \
    sed -i 's/\batd_ref_server\b/atd_ref_server_bin/g' {} +
```

Double-check none of these accidentally renamed something that should stay `atd_ref_server` (e.g., a string literal naming the binary). Grep for any `"atd_ref_server_bin"` (underscore form) inside string contexts — should be zero; if present, manually restore the literal to `"atd-ref-server"` or `"atd_ref_server"` as appropriate:
```bash
grep -rn '"atd_ref_server_bin"' crates/atd-ref-server-bin/
grep -rn '"atd-ref-server-bin"' crates/atd-ref-server-bin/
```
Expected: no matches in source/binary-name contexts (binary name stays `atd-ref-server`).

- [ ] **Step 8: Update `crates/atd-mcp-bridge/tests/integration_e2e.rs`**

Change `-p atd-ref-server` references to `-p atd-ref-server-bin` in the doc comments and error messages (these are prose, not code paths). **Keep** the binary path `target/release/atd-ref-server` (binary name preserved).

```bash
sed -i 's|-p atd-ref-server\b|-p atd-ref-server-bin|g' crates/atd-mcp-bridge/tests/integration_e2e.rs
```

Verify the only remaining `atd-ref-server` references in that file are the binary path (`target/release/atd-ref-server`) and user-facing error strings referring to the binary:
```bash
grep -n "atd-ref-server" crates/atd-mcp-bridge/tests/integration_e2e.rs
```

- [ ] **Step 9: Regression gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```

Expected binary artifacts:
```bash
ls target/release/atd target/release/atd-ref-server target/release/atd-mcp-bridge
```
All three binaries present with unchanged names.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "refactor(atd-ref-server-bin): rename crate; relocate external/ (C6)

- Crate renamed atd-ref-server → atd-ref-server-bin.
- Binary name preserved as atd-ref-server via [[bin]] name = ... .
- tools/external/ (SP-12 CliBinding demo) moved to src/external/;
  tools/ directory removed.
- lib.rs drops pub mod tools; adds #[cfg(unix)] pub mod external.
- builtin.rs rewires use path to src/external.
- atd-mcp-bridge integration_e2e.rs switches -p target to atd-ref-server-bin
  while keeping binary path target/release/atd-ref-server intact.
- Workspace green; binary layout unchanged from end-user perspective.

Refs: docs/superpowers/specs/2026-04-24-crate-refactor-design.md §6 C6"
```

---

## Task 7 (C7): Live-docs sync

**Files:** (10 live docs — historical plans/specs are not rewritten)
- Modify: `README.md`
- Modify: `docs/atd-architecture.md`
- Modify: `docs/design.md`
- Modify: `docs/protocol/wire-format.md`
- Modify: `docs/protocol/error-codes.md`
- Modify: `docs/integrations/langchain.md`
- Modify: `docs/integrations/hermes.md`
- Modify: `docs/integrations/claude-code.md`
- Modify: `docs/integrations/openclaw.md`
- Modify: `docs/integrations/overview.md`

**Non-scope:** `docs/superpowers/plans/*.md`, `docs/superpowers/specs/*.md`, `docs/whitepaper/*` — all left untouched (historical records).

- [ ] **Step 1: Bulk crate-name rewrite across the 10 live docs**

```bash
for f in README.md docs/atd-architecture.md docs/design.md \
         docs/protocol/wire-format.md docs/protocol/error-codes.md \
         docs/integrations/langchain.md docs/integrations/hermes.md \
         docs/integrations/claude-code.md docs/integrations/openclaw.md \
         docs/integrations/overview.md; do
    sed -i 's/\batd-types\b/atd-protocol/g; s/\batd_types\b/atd_protocol/g' "$f"
    sed -i 's/\batd-client\b/atd-sdk/g; s/\batd_client\b/atd_sdk/g' "$f"
    # atd-ref-server → atd-ref-server-bin ONLY as a crate name (cargo add, [dependencies]);
    # binary name and command invocations remain `atd-ref-server`. The blind rewrite below
    # is too aggressive — do this one manually per file instead, see Step 2.
done
```

- [ ] **Step 2: Manual pass for `atd-ref-server` references**

For each of the 10 docs, grep for `atd-ref-server` and decide per occurrence:

- Rust crate name (e.g., `cargo add atd-ref-server`, `atd-ref-server = { path = ... }`, `use atd_ref_server::...`): update to `atd-ref-server-bin` / `atd_ref_server_bin`.
- Binary / command name (e.g., `atd-ref-server --socket …`, `target/release/atd-ref-server`): **keep** as `atd-ref-server`.
- Prose mentions of the crate-as-a-concept: prefer `atd-ref-server-bin` (matches Cargo.toml and `cargo install` command).

Work through each file individually:
```bash
for f in README.md docs/atd-architecture.md docs/design.md \
         docs/protocol/wire-format.md docs/protocol/error-codes.md \
         docs/integrations/langchain.md docs/integrations/hermes.md \
         docs/integrations/claude-code.md docs/integrations/openclaw.md \
         docs/integrations/overview.md; do
    echo "=== $f ==="
    grep -n "atd-ref-server" "$f"
done
```

For each hit, apply a targeted `Edit` (not a global `sed`) — this is a manual reading pass, not a batch replace.

- [ ] **Step 3: Update `docs/atd-architecture.md` §8.2 status cells**

Open `docs/atd-architecture.md`, find §8.2 (the "Current → target mapping" table). The pre-refactor table has ⚠️ markers on the Protocol and Runtime rows:

```markdown
| **Protocol** (types, wire, sanitize) | `atd-types` + `atd-client::wire` + `atd-client::protocol` + `atd-client::sanitize` | ⚠️ split across crates | ... |
| **Runtime** (`Tool` trait, `Registry`, dispatch, ...) | `atd-ref-server/src/` (outside `tools/`) | ⚠️ lumped with tools + binary | ... |
| **Built-in tools** (echo, fs, shell, web) | `atd-ref-server/src/tools/` | ⚠️ lumped | ... |
```

Rewrite post-refactor — rows become:

```markdown
| **Protocol** (types, wire, sanitize) | `atd-protocol` | ✅ | Consolidated in SP-refactor-v1. |
| **Rust SDK** | `atd-sdk` | ✅ | Renamed from `atd-client`. Adapters feature-gated. |
| **Python SDK** | `python/src/atd_client/` | ⚠️ pending Python-mirror SP | Still named `atd_client`; rename deferred. |
| **Runtime** (`Tool` trait, `Registry`, dispatch, ...) | `atd-runtime` | ✅ | Extracted in SP-refactor-v1. |
| **Built-in tools** (echo, fs, shell, web) | `atd-tools-echo`, `atd-tools-fs`, `atd-tools-shell`, `atd-tools-web` | ✅ | Split per-domain in SP-refactor-v1. |
| **MCP bridge** | `atd-mcp-bridge` | ✅ | Binary |
| **CLI** | `atd-cli` | ✅ | Binary — `atd` command |
| **Ref-server binary** | `atd-ref-server-bin` (binary name `atd-ref-server`) | ✅ | Thin wrapper over atd-runtime + atd-tools-*. |
```

- [ ] **Step 4: Update `docs/atd-architecture.md` §8.3 / §8.4 current-vs-target reconciliation**

§8.3 showed the current (lumped) dep graph; §8.4 showed the target. After the refactor, §8.3 is the historical pre-refactor state. Two options:

1. Replace §8.3 with the §8.4 target diagram, delete §8.4, and add a note: "See git history (tag `pre-refactor-v1`) for the pre-refactor layout."
2. Keep both sections; relabel §8.3 as "Historical (pre-SP-refactor-v1)" and §8.4 as "Current layout".

Recommend (1) — the "current" graph should always be current. Do the edit:

```markdown
### 8.3 Dependency graph (current)

```
atd-protocol
   ▲
   ├── atd-sdk (client + adapters)
   │       ▲
   │       ├── atd-mcp-bridge
   │       └── atd-cli
   │
   └── atd-runtime (Tool/Binding/Middleware/Registry/dispatch)
           ▲
           ├── atd-tools-echo
           ├── atd-tools-fs
           ├── atd-tools-shell
           ├── atd-tools-web
           └── atd-ref-server-bin (wires runtime + tools into an installable binary)
```

Python SDK (`python/src/atd_client/`) mirrors `atd-protocol` + `atd-sdk` as a standalone Python package with its own sanitize + adapters. Python rename to `atd_sdk` is a deferred SP.
```

Delete the old §8.4 target-state heading; fold its content into the new §8.3.

Also update §8.5 "When to refactor" — the refactor has happened; rewrite:

```markdown
### 8.4 Refactor history

Target layout landed in `SP-refactor-v1` (tag `sp-refactor-v1`). Pre-
refactor state is available at tag `pre-refactor-v1` if someone needs the
historical crate-lumping for comparison.
```

Adjust section-number references elsewhere in the doc if `§8.3`/`§8.4` are cited (grep `docs/atd-architecture.md` for `§8.3` and `§8.4` mentions and renumber accordingly).

- [ ] **Step 5: Update `docs/design.md` supersede pointer header**

If `docs/design.md` currently has a note like "Superseded by `docs/atd-architecture.md`", preserve it but add a note that crate names in the design doc reflect the pre-refactor Phase 0 spec. Add an explicit update-or-delete todo for the design-doc author (or delete the stale crate-name examples if they no longer match). This file is allowed to stay partly historical — atd-architecture.md is the live source.

Minimum action: update any `use atd_client::*` or `atd-types` references in prose to the new names, keeping the "Phase 0 historical context" framing intact.

- [ ] **Step 6: Verify no stale crate-name references remain in live docs**

```bash
for f in README.md docs/atd-architecture.md docs/design.md \
         docs/protocol/wire-format.md docs/protocol/error-codes.md \
         docs/integrations/*.md; do
    echo "=== $f ==="
    grep -En "atd-types|atd-client\b|atd_client\b|atd_types\b" "$f" || echo "  clean"
done
```

Expected: every file prints "clean". If any legitimate reference remains (e.g., the Python SDK is still `atd_client`), document it with inline context so the reader knows it's intentional.

`atd-ref-server` hits should be classified per Step 2 (binary name = keep, crate name = rename).

- [ ] **Step 7: Regression gate (docs don't affect compile but still run it)**

```bash
cargo test --workspace --all-features
```

Expected: green. Docs changes shouldn't affect tests; this is a sanity check that nothing code-adjacent got stepped on by the `sed` runs.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "docs: sync live docs to new crate names (C7)

- README, architecture, design, protocol/wire-format, protocol/error-codes,
  integrations/{langchain,hermes,claude-code,openclaw,overview}: crate
  names, use paths, dependency diagrams aligned with post-refactor layout.
- atd-architecture.md §8 rewritten: §8.3 now reflects the current (post-
  refactor) graph; historical lump-layout available via git tag
  pre-refactor-v1. §8.2 status cells flipped ⚠️ → ✅ for Protocol, Runtime,
  built-in tools. Python SDK row kept ⚠️ (pending mirror SP).
- Binary names (atd, atd-ref-server, atd-mcp-bridge) unchanged in every
  doc — user-facing commands are stable.
- Historical plans/specs left untouched by design.

Refs: docs/superpowers/specs/2026-04-24-crate-refactor-design.md §8"
```

---

## Task 8: Post-flight smoke + milestone tag

**Files:** None modified; this task verifies and tags.

- [ ] **Step 1: Full clean build**

```bash
cargo clean
cargo build --release --workspace
```

Expected: every target crate + binary produced.

- [ ] **Step 2: Start the server and verify 10-tool discovery**

```bash
./target/release/atd-ref-server --socket /tmp/atd.sock &
sleep 1
./target/release/atd --socket /tmp/atd.sock list
```

Expected: 10 tools listed on Unix (9 native + `ref:external.uname`), 9 on Windows.

Cleanup:
```bash
pkill -f atd-ref-server
rm -f /tmp/atd.sock
```

- [ ] **Step 3: Round-trip `fs.read`**

```bash
./target/release/atd-ref-server --socket /tmp/atd.sock &
sleep 1
./target/release/atd --socket /tmp/atd.sock call ref:fs.read '{"path":"Cargo.toml"}'
```

Expected: JSON response containing the contents of `Cargo.toml`.

Cleanup as in Step 2.

- [ ] **Step 4: MCP bridge stdio round-trip**

```bash
./target/release/atd-ref-server --socket /tmp/atd.sock &
sleep 1
echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' \
    | ./target/release/atd-mcp-bridge --target /tmp/atd.sock
```

Expected: a JSON-RPC response with `result.tools` containing the tool list.

Cleanup.

- [ ] **Step 5: Run examples**

```bash
cargo run --example hello_atd
cargo run --example hello_langchain --features langchain
```

Both should complete without error.

- [ ] **Step 6: `cargo publish --dry-run` for the publishable crates**

```bash
cargo publish --dry-run -p atd-protocol
cargo publish --dry-run -p atd-sdk
cargo publish --dry-run -p atd-mcp-bridge
```

Expected: all three pass `--dry-run` without warning about missing required fields (description, license, readme, keywords, categories). If any fails: inspect the error, fix the missing metadata field, commit fix as a C7 amendment or a follow-up commit.

Note: `atd-sdk` will be a new crate on crates.io the first time it's actually published — the name is free (confirmed: nothing is on crates.io per Q2=A).

- [ ] **Step 7: Tag the milestone**

```bash
git tag sp-refactor-v1
git log --oneline pre-refactor-v1..sp-refactor-v1
```

Expected: 7 commits (C1–C7) listed.

- [ ] **Step 8: No commit for this task** — tag only.

---

## Self-review checklist (fill in after executing)

- [ ] All 7 commits (C1–C7) independently pass `cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test --workspace --all-features` + `cargo build --release --workspace`.
- [ ] `cargo publish --dry-run` clean for `atd-protocol`, `atd-sdk`, `atd-mcp-bridge`.
- [ ] Manual smoke (Task 8 steps 2–5) all pass.
- [ ] Binary names unchanged: `atd`, `atd-ref-server`, `atd-mcp-bridge` all install/run with pre-refactor names.
- [ ] Workspace has exactly 10 crates + `examples/`.
- [ ] Historical `docs/superpowers/plans/` and `docs/superpowers/specs/` untouched by this SP (verify via `git log --stat pre-refactor-v1..sp-refactor-v1 -- docs/superpowers/`).
- [ ] Python SDK at `python/src/atd_client/` unchanged (out of scope per Q4=B).
- [ ] Tags present: `pre-refactor-v1` at baseline, `sp-refactor-v1` at completion.
