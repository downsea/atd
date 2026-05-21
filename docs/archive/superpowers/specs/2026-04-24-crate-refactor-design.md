# ATD Crate Refactor — Design

**Date:** 2026-04-24
**Status:** Approved — ready for implementation plan
**Scope:** Rust workspace only. Python SDK rename is a separate, later SP.
**Anchor:** `docs/architecture.md` §8.4 target-state crate graph

## 1. Context

The current workspace has 5 crates: `atd-types`, `atd-client`, `atd-cli`,
`atd-mcp-bridge`, `atd-ref-server`. The architecture doc (§8.1–8.4) names a
three-layer logical decomposition — **Protocol** (spec), **SDK** (client
side), **Runtime** (server side) — plus satellite tool / bridge / binary
crates. The current layout lumps Protocol into `atd-types + atd-client`,
and lumps Runtime into `atd-ref-server` together with all built-in tools
and the binary entry point.

§8.5 gates the refactor on two possible triggers:

- (a) a third-party server implementer asks for `atd-runtime` as a reusable
  library, or
- (b) multiple independent tool crates want to coexist.

Neither has fired. This SP is **pre-adopter cleanup**: do the clean
structural split now, while breaking changes are free (no crate is on
crates.io yet), so the target shape is in place before external pressure
forces a rushed refactor.

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q1 | Motivation? | C — Pre-adopter cleanup, no external trigger has fired |
| Q2 | Rename posture? | A — Nothing is on crates.io yet; free renames, no compat shims |
| Q3 | Scope? | A — Full §8.4 target minus `atd-conformance` (future) |
| Q4 | Python SDK included? | B — Rust only this SP; Python rename deferred to a later SP |
| Q5 | Cadence? | A — Big-bang single SP (with bisect-able internal commits) |
| Q6a | Where does `external/uname` live? | iii — Stays inside `atd-ref-server-bin`; not its own crate |
| Q6b | Docs updated in same SP? | x — Live docs yes; historical `plans/` + `specs/` never rewritten |
| Approach | Protocol crate shape? | §8.4 literal: `atd-protocol` = types + wire + sanitize (+ future schema). No `atd-sanitize` split. No `atd-binding` split. |

## 3. Target crate graph

Workspace goes from 5 crates to 10:

```
atd-protocol                          [spec layer · no deps on other ATD crates]
   │  types + wire + sanitize + messages
   ▲
   ├── atd-sdk                        [client layer; renamed from atd-client]
   │     │  AtdClient, Endpoint, CallOptions, DiscoverFilter
   │     │  adapters: openai / anthropic / langchain (feature-gated)
   │     ▲
   │     ├── atd-cli                  [binary — crate name unchanged, binary `atd`]
   │     └── atd-mcp-bridge           [binary — crate + binary name unchanged]
   │
   └── atd-runtime                    [server layer; extracted from atd-ref-server]
         │  Tool / Binding / Middleware / Registry / dispatch
         │  Context / Tracker / Tier / Capability / ToolCallError
         ▲
         ├── atd-tools-echo
         ├── atd-tools-fs
         ├── atd-tools-shell
         ├── atd-tools-web
         └── atd-ref-server-bin       [binary — crate renamed from atd-ref-server,
                                         binary name preserved as `atd-ref-server`]
```

Deviations from §8.4, explicitly:

1. `atd-conformance` is out of scope (§8.4 already marks it future).
2. `atd-sdk-py` is out of scope (Python rename is a separate SP).
3. `external/uname` lives inside `atd-ref-server-bin`, not a standalone
   `atd-tools-external` crate. It is a SP-12 CliBinding demo, not a reusable
   tool library.

Binary name preservation — crate rename ≠ executable rename:

| Crate name (new) | Binary name (unchanged) |
|---|---|
| `atd-cli` | `atd` |
| `atd-mcp-bridge` | `atd-mcp-bridge` |
| `atd-ref-server-bin` | `atd-ref-server` |

End users running `atd …`, `atd-mcp-bridge …`, `atd-ref-server …` see no
change.

## 4. File migration map

### 4.1 `atd-protocol` (new · consolidated spec layer)

| Source | Destination | Notes |
|---|---|---|
| `atd-types/src/{tool,enums,error,result,summary}.rs` | `atd-protocol/src/{tool,enums,error,result,summary}.rs` | Verbatim move |
| `atd-client/src/wire.rs` | `atd-protocol/src/wire.rs` | Verbatim move |
| `atd-client/src/protocol.rs` | `atd-protocol/src/messages.rs` | Renamed to avoid `atd_protocol::protocol::Request` noise |
| `atd-client/src/sanitize.rs` | `atd-protocol/src/sanitize.rs` | Verbatim move |
| `atd-ref-server/src/{protocol,wire}.rs` | **deleted** | Independent duplicate implementations retire; `atd-ref-server-bin` / `atd-runtime` use `atd-protocol::*` |
| New `atd-protocol/src/lib.rs` | — | `pub use messages::*; pub use wire::*; pub use tool::*;` etc. — flatten for consumers |

Dependencies (Cargo): `serde`, `serde_json`, `thiserror`. No `tokio`,
no async. The spec layer is synchronous types and codec primitives only.

### 4.2 `atd-sdk` (renamed from `atd-client`)

| Source | Destination | Notes |
|---|---|---|
| `atd-client/src/{client,endpoint,options,lib}.rs` | `atd-sdk/src/` same names | Internal `use crate::wire` → `use atd_protocol::wire`; same for `protocol` → `messages`, `sanitize` |
| `atd-client/src/adapters/{openai,anthropic,langchain,mod}.rs` | `atd-sdk/src/adapters/` same structure | Feature flags preserved |

Dependencies: `atd-protocol`, `tokio`, `thiserror`. Features: `openai`,
`anthropic`, `langchain`, `adapters` (all preserved from `atd-client`).

### 4.3 `atd-runtime` (new · server-side abstractions)

| Source | Destination | Notes |
|---|---|---|
| `atd-ref-server/src/{binding,capability,context,middleware,registry,tier,tracker}.rs` | `atd-runtime/src/` same names | Internal `use crate::protocol::*` → `use atd_protocol::messages::*` |
| `atd-ref-server/src/error.rs` (`ToolCallError`) | `atd-runtime/src/error.rs` | Stays distinct from `atd-protocol::AtdError` (server vs client-side error classification — see current file header comment) |
| New `atd-runtime/src/lib.rs` | — | `pub use registry::{Tool, Registry}; pub use binding::*; ...` |

Dependencies: `atd-protocol`, `tokio`, `thiserror`. No `clap`, no `ulid`,
no `reqwest` — those belong to the binary or specific tool crates.

### 4.4 `atd-tools-echo` (reference minimal tool crate)

| Source | Destination |
|---|---|
| `atd-ref-server/src/tools/echo.rs` | `atd-tools-echo/src/lib.rs` |

Dependencies: `atd-protocol`, `atd-runtime`.

### 4.5 `atd-tools-fs`

| Source | Destination |
|---|---|
| `atd-ref-server/src/tools/fs/{edit,glob,grep,read,write,shared,mod}.rs` | `atd-tools-fs/src/` isomorphic |

Dependencies: `atd-protocol`, `atd-runtime`, `tokio`, `ignore`, `globset`,
`grep-searcher`, `grep-regex`, `regex`.

### 4.6 `atd-tools-shell`

| Source | Destination |
|---|---|
| `atd-ref-server/src/tools/shell/{exec,pwsh,shared,mod}.rs` | `atd-tools-shell/src/` isomorphic |

Dependencies: `atd-protocol`, `atd-runtime`, `tokio` (with `process`),
`libc` (unix cfg).

### 4.7 `atd-tools-web`

| Source | Destination |
|---|---|
| `atd-ref-server/src/tools/web/{fetch,mod}.rs` | `atd-tools-web/src/` isomorphic |

Dependencies: `atd-protocol`, `atd-runtime`, `reqwest`, `htmd`, `url`.

### 4.8 `atd-ref-server-bin` (renamed + slimmed)

| Source | Destination | Notes |
|---|---|---|
| `atd-ref-server/src/{main,server,builtin,lib}.rs` | `atd-ref-server-bin/src/` same names | `builtin.rs` changes `use crate::tools::…` → `use atd_tools_fs::*;` etc. |
| `atd-ref-server/src/tools/external/{mod,uname}.rs` | `atd-ref-server-bin/src/external/{mod,uname}.rs` | `#[cfg(unix)]` preserved; still wired via `builtin.rs` |
| `atd-ref-server/src/tools/mod.rs` | **deleted** | Nothing left in `tools/` after external relocates |

`Cargo.toml`:

```toml
[package]
name = "atd-ref-server-bin"

[[bin]]
name = "atd-ref-server"   # binary name preserved
path = "src/main.rs"
```

Dependencies: `atd-protocol`, `atd-runtime`, `atd-tools-echo`,
`atd-tools-fs`, `atd-tools-shell`, `atd-tools-web`, `tokio`, `clap`,
`ulid`, `libc` (for unix uname cfg).

### 4.9 `atd-cli` and `atd-mcp-bridge` (structure unchanged)

`Cargo.toml` dependency renames only:

- `atd-types` → `atd-protocol`
- `atd-client` → `atd-sdk`

Source-level `use` path renames:

- `use atd_client::*` → `use atd_sdk::*`
- `use atd_types::*` → `use atd_protocol::*`

Binary names and crate names preserved. Feature-flag passthroughs to
`atd-sdk` (e.g. `langchain = ["atd-sdk/langchain"]`) updated likewise.

### 4.10 `examples/` (workspace member, not published)

`examples/Cargo.toml` deps: `atd-client` → `atd-sdk`, `atd-types` →
`atd-protocol`. Source files (`hello_atd.rs`, `hello_langchain.rs`): same
`use` path renames as §4.9.

## 5. Public API surface delta

The net effect for external readers is **crate + module path renames
only**. Zero symbol renames. Zero trait signature changes. Zero wire
format changes. Zero binary name changes.

### 5.1 Import path mapping

| Old | New |
|---|---|
| `atd_types::{AtdError, ToolDefinition, ToolSummary, ToolResult, ToolTier, …}` | `atd_protocol::{AtdError, ToolDefinition, ToolSummary, ToolResult, ToolTier, …}` |
| `atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint}` | `atd_sdk::{AtdClient, CallOptions, DiscoverFilter, Endpoint}` |
| `atd_client::wire::*` | `atd_protocol::wire::*` |
| `atd_client::protocol::*` (Request/Response) | `atd_protocol::messages::*` |
| `atd_client::sanitize::*` | `atd_protocol::sanitize::*` |
| `atd_client::adapters::langchain::as_langchain_tools` | `atd_sdk::adapters::langchain::as_langchain_tools` |
| `atd_ref_server::{Tool, Registry, Context, Middleware, Binding, …}` | `atd_runtime::{Tool, Registry, Context, Middleware, Binding, …}` |
| `atd_ref_server::tools::fs::read::FsReadTool` (and siblings) | `atd_tools_fs::FsReadTool` (etc.) |
| `atd_ref_server::tools::echo::EchoTool` | `atd_tools_echo::EchoTool` |
| `atd_ref_server::builtin::builtin_registry` | `atd_ref_server_bin::builtin::builtin_registry` (or crate-internal) |

### 5.2 Migration guide (for any external reader)

1. `sed` crate names in `Cargo.toml`: `atd-types` → `atd-protocol`,
   `atd-client` → `atd-sdk`.
2. `sed` `use` paths: `atd_types` → `atd_protocol`, `atd_client` →
   `atd_sdk`.
3. If you use wire codec directly: `atd_client::wire` → `atd_protocol::wire`.
4. If you use Request/Response: `atd_client::protocol` →
   `atd_protocol::messages`.

## 6. Migration order inside the SP

Big-bang = single SP, but **not** single commit. Seven bisect-able
commits, each leaving `cargo test --workspace --all-features` green.

| # | Theme | Key actions | Scope |
|---|---|---|---|
| **C1** | Rename `atd-types` → `atd-protocol` + scaffold new skeletons | Rename `atd-types` crate (directory + `Cargo.toml` `name`) to `atd-protocol`. Update every downstream `Cargo.toml` (`atd-client`, `atd-ref-server`, `atd-cli`, `atd-mcp-bridge`, `examples`) and every source `use atd_types::*` → `use atd_protocol::*`. In the same commit, create 5 new empty crate skeletons (`atd-runtime`, `atd-tools-{echo,fs,shell,web}`) with stub `lib.rs` and minimal `Cargo.toml`; add all to workspace `members`. Commit ends with workspace-green. | Entire workspace Cargo + every `use atd_types` site |
| **C2** | Fill `atd-protocol` with wire + messages + sanitize | Move `wire.rs` / `protocol.rs` (→ `messages.rs`) / `sanitize.rs` from `atd-client` into `atd-protocol`. **Delete** `atd-ref-server/src/protocol.rs` + `wire.rs` (independent duplicate implementations retire). Rewire `atd-client` internal `use crate::{wire,protocol,sanitize}` → `atd_protocol::*`; rewire `atd-ref-server` internal `use crate::{protocol,wire}` → `atd_protocol::{messages,wire}`. Publish `pub use` re-exports at `atd-protocol/src/lib.rs`. | `atd-client` + `atd-ref-server` + `atd-protocol` |
| **C3** | `atd-client` → `atd-sdk` | Rename crate (directory + `Cargo.toml` `name`). Rewire `atd-cli`, `atd-mcp-bridge`, `examples`. | 4 downstream sites |
| **C4** | Extract `atd-runtime` | Move `{binding, capability, context, error, middleware, registry, tier, tracker}.rs` from `atd-ref-server/src` to `atd-runtime/src`. `atd-ref-server` gains dep on `atd-runtime`. Tool files unchanged. | ref-server + runtime |
| **C5** | Extract 4 `atd-tools-*` | Move `tools/{echo,fs,shell,web}` from `atd-ref-server` into per-domain tool crates. `atd-ref-server/src/builtin.rs` changes imports to `use atd_tools_fs::*` etc. Delete `atd-ref-server/src/tools/{mod,echo}.rs` and the `{fs,shell,web}/` subdirs. | ref-server + 4 tool crates |
| **C6** | `atd-ref-server` → `atd-ref-server-bin` | Rename crate. Preserve `[[bin]].name = "atd-ref-server"`. Move `tools/external/` → `src/external/` with `#[cfg(unix)]` intact. | ref-server-bin |
| **C7** | Live docs sync | Update crate names, module paths, `use` snippets, dependency diagrams in 10 live docs (see §8 below). Historical `plans/` + `specs/` untouched. | 10 docs, zero code |

### 6.1 Per-commit gate (automated)

Every commit must pass, before the next one begins:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```

### 6.2 End-of-SP gate (manual smoke)

One-time run after C7:

1. Start the server: `target/release/atd-ref-server --socket /tmp/atd.sock &`
2. Tool discovery: `target/release/atd --socket /tmp/atd.sock list` — expect
   10 tools (9 native + `ref:external.uname` on unix, 9 on windows).
3. Round-trip: `target/release/atd --socket /tmp/atd.sock call ref:fs.read
   '{"path":"Cargo.toml"}'` returns file contents.
4. MCP bridge: `echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' |
   target/release/atd-mcp-bridge --target /tmp/atd.sock` responds.
5. Examples: `cargo run --example hello_atd` and
   `cargo run --example hello_langchain --features langchain` both succeed.
6. Publish readiness: `cargo publish --dry-run` on the three publishable
   crates — `atd-protocol`, `atd-sdk`, `atd-mcp-bridge` — all clean (no
   actual publish).

### 6.3 Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| `atd-ref-server::protocol::Request` and `atd-client::protocol::Request` are two independently-defined types. Merging them in C2 could mask a subtle server-side code path that depends on variant shape specifics. | Low — types are designed byte-equivalent by construction, exercised by SP-12 wire tests. | After C2, run full wire round-trip test suite (SP-1 ping/tool_list + SP-12 capability-denied round-trip). |
| `#[cfg(unix)]` gate on `external/uname` might drop during C6 move. | Medium — windows CI would go red. | After C6, verify cfg is present in the new path via grep; if windows cross-toolchain installed, also `cargo check --target x86_64-pc-windows-gnu`. |
| C1 is the largest commit (renames `atd-types` and rewrites every `use atd_types` site across the workspace). C2 is second-largest (file moves + ref-server/client import rewires). | Review burden + large diff. | Accepted per Q5=A. Commit messages on C1 and C2 enumerate affected files with line-count deltas to aid manual review. |
| Mechanical `use` path rewrites can introduce typos. | Low — compiler catches them deterministically. | Use `cargo check` as ground truth after each rename; batch rewrites via `sd 'atd_types' 'atd_protocol' crates/ examples/`. |
| `examples/Cargo.toml` forgotten in the workspace sweep → `cargo test --workspace` still green but `cargo run --example` fails. | Low. | C2 and C3 explicitly include `examples/` in their rewrite scope; SP-end manual smoke step 5 catches regressions. |

### 6.4 Rollback

- Before SP starts: `git tag pre-refactor-v1`.
- Each of C1–C7 individually revertible via `git revert`.
- Worst case: `git reset --hard pre-refactor-v1`.
- After successful SP: `git tag sp-refactor-v1` as the architecture-v1
  landing milestone.

## 7. Non-goals (explicit)

| Not doing | Why | When it opens |
|---|---|---|
| Python SDK rename (`atd_client` → `atd_sdk`) | Q4=B: PyPI publish cadence independent of crates.io | A subsequent Python-mirror SP |
| `atd-conformance` crate | §8.4 marks it future; SP-8 has its own plan | When SP-8 kicks off |
| `atd-protocol` JSON schema generation | §8.4 says "ready-to-generate", not "generate now"; no external SDK author has requested export yet | First external-SDK author requests schema export |
| `atd-tools-external` as standalone crate | Q6a=iii: uname is a binding demo, not a reusable tool library | When a third CliBinding demo appears and they cluster naturally |
| Any functional or behavioral change | Pure refactor = zero semantic change; mixing features would contaminate bisect | Dedicated feature SPs |
| Rewriting historical `plans/` and `specs/` | Q6b=x: historical records should remain a faithful snapshot | Never |
| Actual `cargo publish` execution | SP-9 established this as manual hand-off | User runs manually |
| CI configuration changes | Workspace tests are the refactor's green-light authority; CI YAML is a separate concern | Dedicated CI SP |
| `atd-runtime` plugin/dylib loader | `docs/architecture.md` §9 non-goal until a future trigger | When an external plugin demand appears |

## 8. Live docs to update in C7

Files touched in C7. Historical `docs/superpowers/plans/` and
`docs/superpowers/specs/` are **not** rewritten.

| File | What changes |
|---|---|
| `README.md` | Installation/usage snippets referring to crate names; dependency table in "Related crates" sections |
| `docs/architecture.md` | §8.2 status column flips from ⚠️ lumped → ✅; §8.3 current diagram replaced with §8.4 target diagram (and §8.3 kept as "historical — pre-refactor" note, or deleted); status flip elsewhere in the doc |
| `docs/design.md` | Adjust the supersede pointer header to note the refactor landed; update any crate-name references |
| `docs/protocol/wire-format.md` | References to `atd-client::wire` or `atd-ref-server::wire` → `atd-protocol::wire` |
| `docs/protocol/error-codes.md` | `atd-types::AtdError` → `atd-protocol::AtdError`; `ToolCallError` crate path update |
| `docs/integrations/langchain.md` | `atd-client` → `atd-sdk` in adapter code snippets |
| `docs/integrations/hermes.md` | Crate names |
| `docs/integrations/claude-code.md` | Crate names; MCP bridge instructions unchanged (binary name preserved) |
| `docs/integrations/openclaw.md` | Crate names |
| `docs/integrations/overview.md` | Dependency diagram + crate-map section alignment with the new graph |

## 9. Success criteria

The SP is complete when all of the following hold simultaneously:

1. Workspace contains exactly 10 crates: `atd-protocol`, `atd-sdk`,
   `atd-runtime`, `atd-tools-echo`, `atd-tools-fs`, `atd-tools-shell`,
   `atd-tools-web`, `atd-ref-server-bin`, `atd-cli`, `atd-mcp-bridge`
   (plus `examples/` as unpublished workspace member).
2. Each of C1–C7 independently passes `cargo fmt --check` + `cargo clippy
   --workspace --all-features -- -D warnings` + `cargo test --workspace
   --all-features` + `cargo build --release`.
3. `cargo publish --dry-run` succeeds for `atd-protocol`, `atd-sdk`, and
   `atd-mcp-bridge`.
4. End-of-SP manual smoke (six steps in §6.2) all pass.
5. `docs/architecture.md` §8.2 status cells for Protocol / Runtime / Tools
   rows flip from ⚠️ to ✅; §8.3/§8.4 are reconciled with observed reality.
6. Live docs in §8 above carry no stale crate-name or `use`-path references.
7. `atd`, `atd-ref-server`, `atd-mcp-bridge` binary names unchanged;
   wire format bytes unchanged (SP-12 capability-denied round-trip +
   earlier SP wire tests all pass).
8. Zero new features, zero semantic changes — grep confirms only import
   paths, Cargo metadata, directory structure, and docs were modified.

## 10. Next SPs unlocked

- **Python-mirror SP** — deferred per Q4=B. Can start as soon as this SP
  lands; uses this SP's final Rust layout as the mirror target.
- **SP-8 conformance suite** — architecture doc already planned it;
  post-refactor, `atd-protocol` is the single canonical spec crate that
  conformance tests target.
- **Trigger (a) unblock** — any future third-party server implementer can
  now `cargo add atd-runtime` and pick individual `atd-tools-*` crates
  without forking `atd-ref-server`.

Improvements this SP does **not** deliver but enables downstream
(`docs/architecture.md` §10 roadmap ❌ items that become easier):

- Audit-logging middleware → can ship as a dedicated crate once
  `atd-runtime` exists as a library.
- Rate-limit middleware → same.
- Dry-run consistency → easier to test with middleware extracted.
