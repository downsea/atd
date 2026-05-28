# SP-listener-extract — Extract socket listener from `atd-ref-server` into a new `atd-server` crate

**Date:** 2026-04-25
**Status:** Draft — awaiting approval
**Anchor:** `docs/atd-architecture.md` §8.4 (target crate graph). Triggered by the first concrete second-vendor-server adopter signal: `healthkit_cli` (Huawei HMS HealthKit). Generalizes the listener layer that today only `atd-ref-server` consumes.

## 1. Context

Today's state (post-`sp-refactor-v1`, post-`sp-publish-v2`):

- `atd-runtime` provides pure dispatch core: `Tool`, `Registry`, `Binding`, `Middleware`, `CapabilityGate`, `CallContext`, `ReadTracker`, `AuditSink`. Zero transport.
- `atd-ref-server/src/server.rs` owns the actual ATD socket transport: `Server` struct, `ServerConfig`, Unix listener, accept loop, per-connection `tokio` task, frame I/O wiring against `atd-protocol`.

Result: any third party that wants to run an ATD-speaking server (e.g., a vendor wrapping their own HTTP API into ATD tools) has to either (a) copy the ~200-line `server.rs` from atd-ref-server, or (b) take a transitive dep on the entire `atd-ref-server` crate (which also pulls in 9 built-in tools they don't want).

`healthkit_cli` (Huawei HealthKit wrapper) is the first concrete adopter that needs this — it wants the listener but not the built-in tools. The cleanest answer is to extract the listener.

## 2. Decisions

| # | Question | Answer |
|---|---|---|
| Q1 | New crate name? | **`atd-server`**. Conflicts with the abstract concept "an ATD server" but disambiguates in prose with backticks (`atd-server` = the crate; "ATD server" = any process speaking the protocol). Alternative names (`atd-host`, `atd-listen`, `atd-transport`) lose API clarity (`atd_server::Server` is the most natural type path). |
| Q2 | Fold into `atd-runtime` instead? | **No.** Future `BindingProtocol::Mcp` server-side is stdio (no listener); REST binding is HTTP (different transport). Runtime must stay transport-agnostic so all transports can compose. Mixing tokio `net` deps into runtime also bloats Tool-only authors. |
| Q3 | What stays in `atd-ref-server` after the move? | `main.rs` (clap + config loading), `builtin.rs` (the 9 tool registrations), `conformance.rs` (SP-8.1/8.2 gated tools), `external/` (CliBinding demo). The `server.rs` file vanishes entirely. ~50-100 lines of code, plus tests, plus the README. |
| Q4 | Workspace version bump? | **0.2.0 → 0.2.1**. New crate appears but no breaking change to existing public API. Crates.io has nothing published yet, so semver convention is decorative; pick the smaller increment for honesty. |
| Q5 | Listener API — what exactly does `atd-server` expose? | `Server::new(config, registry)`, `Server::serve()`, `Server::shutdown()`, `ServerConfig` builder. Existing public fields/methods on `atd-ref-server`'s `Server` carry over verbatim. |
| Q6 | Tests — keep where they live, or move with the listener? | Tests on socket/listener behavior move with `server.rs` to `atd-server/tests/`. Tests on `builtin.rs` (tool registration) stay in `atd-ref-server`. Tests that drive Server-via-tools end-to-end (most of `crates/atd-ref-server/tests/dispatch_*.rs`) stay in `atd-ref-server` because they assert built-in tool behavior; they keep working since `atd-ref-server` still wires up a Server. |
| Q7 | Should `atd-mcp-bridge` or `atd-cli` start depending on `atd-server`? | **No.** They're clients, not servers. They depend on `atd-sdk`. Unchanged. |
| Q8 | Should this SP also write a `cargo install`-able demo / template for "build your own ATD server in 30 lines"? | Out of scope. Future SP — once `healthkit_cli` is the first real adopter, codify the pattern from there. |
| Q9 | Touch SP-publish-v2's tag? | No. v0.2.0 stays. This SP ends with v0.2.1 on top. |

## 3. Touch points

### Files to create

```
crates/atd-server/
├── Cargo.toml          (NEW)
├── README.md           (NEW)
├── src/
│   ├── lib.rs          (NEW — public API surface, re-exports)
│   ├── config.rs       (NEW — ServerConfig + builder, moved from atd-ref-server/src/server.rs)
│   ├── server.rs       (NEW — Server struct, serve(), shutdown(), moved from atd-ref-server/src/server.rs)
│   ├── connection.rs   (NEW — per-connection task, read_frame/write_frame loop, moved)
│   └── error.rs        (NEW — ServerError; bind failures, socket-exists, accept errors)
└── tests/
    └── e2e_minimal.rs  (NEW — start Server with a one-Tool registry, drive via atd-sdk, smoke test)
```

### Files to modify

| File | Change |
|---|---|
| `Cargo.toml` (workspace) | Add `crates/atd-server` to members; bump `version` 0.2.0 → 0.2.1 |
| `crates/atd-ref-server/Cargo.toml` | Add `atd-server = { path = "../atd-server", version = "0.2.1" }` dep; drop direct tokio `net` feature if it was only for the listener |
| `crates/atd-ref-server/src/lib.rs` | Drop `pub mod server;`; re-export `atd_server::{Server, ServerConfig}` if downstream tests need them via the crate |
| `crates/atd-ref-server/src/main.rs` | Replace `use crate::server::{Server, ServerConfig};` with `use atd_server::{Server, ServerConfig};` |
| `crates/atd-ref-server/src/builtin.rs` | If it references `crate::server::*` types, update import path |
| `crates/atd-ref-server/tests/*.rs` | Same — update imports for any direct Server use (most use atd-sdk to drive) |
| `crates/atd-ref-server/examples/rw_cycle.rs` | Update imports (`atd_ref_server::server::*` → `atd_server::*`) |
| All `crates/*/Cargo.toml` containing `version = "0.2.0"` path-dep literals | Bump to `"0.2.1"` |
| `docs/atd-architecture.md` §8.4 | Add `atd-server` to crate map; update arrows |
| `docs/atd-architecture.md` §10 | Add row marking listener extraction ✅ at SP-listener-extract |
| `crates/atd-server/README.md` | New, ~30 lines |

### Files NOT touched

- `crates/atd-protocol/` — unchanged (wire format and frame helpers stay where they are)
- `crates/atd-runtime/` — unchanged (transport-agnostic)
- `crates/atd-sdk/` — unchanged (client side)
- `crates/atd-tools-*/` — unchanged
- `crates/atd-cli/`, `crates/atd-mcp-bridge/`, `crates/atd-conformance/` — unchanged (clients/runners, not servers)
- `docs/superpowers/` historical archive — read-only

## 4. Approach (per-task overview)

7 tasks, ~1 day total:

- **T0** Pre-flight: tag baseline `pre-sp-listener-extract`; survey current `server.rs` to confirm no hidden coupling.
- **T1** Scaffold `atd-server` crate (empty lib + Cargo.toml + README); register in workspace.
- **T2** Move `server.rs` content into `atd-server/src/`, split into `config.rs` / `server.rs` / `connection.rs` / `error.rs`. Use `git mv` where possible to preserve history.
- **T3** Update `atd-ref-server` to depend on `atd-server`; delete the now-empty `server.rs`; fix imports in `main.rs`, `lib.rs`, `builtin.rs`, `tests/*`, `examples/rw_cycle.rs`.
- **T4** Bump workspace version to 0.2.1 (workspace + all path-dep literals).
- **T5** Write `atd-server/tests/e2e_minimal.rs`: minimal Server with a 1-tool registry, driven via atd-sdk; smoke-test discover + call.
- **T6** Update `docs/atd-architecture.md` §8.4 + §10; verify 334 tests pass, fmt + clippy clean, dry-run atd-server packaging clean; tag `v0.2.1` + `sp-listener-extract`; push.

## 5. Out of scope

- Actual `cargo publish` to crates.io (still deferred; 12 crates' worth of dry-runs only)
- Listener for non-Unix-socket transports (TCP, TLS, vsock) — Phase 2; not driven by current adopters
- A `cargo install`-able "blank ATD server template" generator — wait for healthkit-side patterns to emerge first
- Renaming `atd-ref-server` (it's still the reference / demo binary; name stays honest)
- Touching SP-publish-v2 (v0.2.0 stays as a historical milestone)

## 6. Risks

| Risk | Mitigation |
|---|---|
| `server.rs` has hidden coupling to `atd-ref-server` private modules (e.g., something it imports from `builtin.rs`) | T0 survey reads `server.rs` end-to-end and lists all `use crate::...` lines before T2 moves anything |
| Tests in `atd-ref-server/tests/` break because they import `atd_ref_server::server::*` | T3 step renames imports; the test bodies don't care which crate `Server` lives in |
| `atd-ref-server`'s lib re-export becomes stale (downstream code expects `atd_ref_server::server::Server`) | T3 either re-exports `pub use atd_server::{Server, ServerConfig}` from `atd-ref-server/src/lib.rs` (keeping the old path as a deprecated alias), or updates downstream — choose the simpler one based on what the survey shows |
| Workspace version bump 0.2.0 → 0.2.1 silently breaks consumers' lockfiles if they had pinned 0.2.0 | No external consumers exist yet (nothing published); tagged history makes it traceable |
| `atd-server` ends up too small to justify a crate | Listener + accept loop + connection task + error type + e2e test ≈ 200-300 lines; comparable to other workspace crates (atd-tools-echo is ~150). Acceptable size |

## 7. Exit criteria

1. New crate `atd-server` exists at `crates/atd-server/` with `Cargo.toml`, `README.md`, `src/{lib,config,server,connection,error}.rs`, and at least one integration test
2. `atd-ref-server/src/server.rs` no longer exists
3. `atd-ref-server` Cargo.toml depends on `atd-server`
4. All `Cargo.toml` versions = `0.2.1`
5. `cargo test --workspace --all-targets` — 334+ tests pass (the new e2e adds ≥ 1; existing 334 must not regress)
6. `cargo fmt --all -- --check` clean
7. `cargo clippy --workspace --all-features -- -D warnings` clean
8. `cargo publish -p atd-server --dry-run --registry crates-io` packages cleanly with no metadata warnings
9. `docs/atd-architecture.md` §8.4 reflects the 12-crate map; §10 has a ✅ row for listener extraction
10. Tags `v0.2.1` + `sp-listener-extract` exist at the SP HEAD and are pushed to `origin`
