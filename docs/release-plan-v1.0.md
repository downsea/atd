# Release Plan — v1.0.0

**Target:** ship the `atd` workspace — `atd-protocol`, `atd-runtime`,
`atd-sdk`, the listener crates, middleware crates, built-in tools, and
the binaries — as a coordinated **`1.0.0`** release on crates.io, under
a stability contract that holds for the whole 1.x line.

**Status:** Planning. The workspace is at `0.3.0`; this release bumps it
to `1.0.0`. Adopters (`healthkit_cli`, `celia_phr`, `cbrain`) consume
via `path =` deps against `master` HEAD today. The pre-release
checklist (§4) is the gate to publish.

**Authority chain.** [`CHANGELOG.md`](../CHANGELOG.md) is the truth for
*what changed*. This file is **Policy** tier (see [`index.md`](index.md))
— the truth for *how to ship it* and *what 1.0 promises*. The
normative behaviour itself is in [`architecture.md`](architecture.md)
and [`protocol/`](protocol/).

---

## Table of contents

1. [The 1.0 stability contract](#1-the-10-stability-contract)
2. [Release shape](#2-release-shape)
3. [Per-crate publication matrix](#3-per-crate-publication-matrix)
4. [Pre-release checklist](#4-pre-release-checklist)
5. [Publish order](#5-publish-order)
6. [Tagging + GitHub release](#6-tagging--github-release)
7. [Rollback](#7-rollback)
8. [Post-1.0 versioning policy](#8-post-10-versioning-policy)

---

## 1. The 1.0 stability contract

1.0 is the point where ATD stops being a moving target. Adopters who
build against 1.0 get the following guarantees for the **entire 1.x
line**:

### 1.1 The wire format is frozen

Every message defined in [`protocol/wire-format.md`](protocol/wire-format.md)
keeps its shape across all 1.x releases. A 1.0 client deserializes every
1.x server message; a 1.x server deserializes every 1.0 client message.

### 1.2 The schema is frozen — additive-only minors

`atd-protocol-schema.json` is frozen at 1.0. Within the 1.x line:

| Change | Allowed in 1.x? | Bump |
|---|---|---|
| New optional field | yes | minor |
| New enum variant (`SafetyLevel`, `ToolVisibility`, …) | yes | minor |
| New error code | yes | minor |
| New message type / new tool | yes | minor |
| Removing a field | **no** | 2.0 |
| Reshaping a type / changing a field's type | **no** | 2.0 |
| Removing or repurposing an enum variant | **no** | 2.0 |

Code generated from `atd-protocol-schema.json` at 1.0 keeps
deserializing every 1.x message. See [`architecture.md`](architecture.md)
§2.5.

### 1.3 The extension traits are stable

The `pub` traits that third-party code attaches to —
[`architecture.md`](architecture.md) §9.3 — keep their signatures
across 1.x:

| Trait | Crate | Extension guide |
|---|---|---|
| `Tool` | `atd-runtime` | [`extending/tool.md`](extending/tool.md) |
| `Binding` | `atd-runtime` | [`extending/binding.md`](extending/binding.md) |
| `Middleware` | `atd-runtime` | [`extending/middleware.md`](extending/middleware.md) |
| `TokenBroker` | `atd-runtime` | [`extending/token-broker.md`](extending/token-broker.md) |
| `AuditSink` | `atd-runtime` | [`extending/audit-sink.md`](extending/audit-sink.md) |

A new method on one of these traits is allowed only if it carries a
default impl (so existing implementors still compile) — that is an
additive minor. A method whose signature *changes*, or a method with no
default, waits for 2.0.

### 1.4 The error taxonomy is stable

`AtdError` variants and their numeric wire codes
([`protocol/error-codes.md`](protocol/error-codes.md)) are stable. New
codes may be *added* in a minor; existing codes never change meaning or
disappear within 1.x.

### 1.5 MSRV policy

MSRV is **Rust 1.85** (edition 2024). A 1.x minor may raise the MSRV
only to a still-widely-available stable Rust, and only when a dependency
or a worthwhile language feature requires it; an MSRV bump is called out
in [`CHANGELOG.md`](../CHANGELOG.md). MSRV is never lowered.

### 1.6 Workspace-lockstep versioning

Every publishable crate ships at one shared `workspace.package.version`.
An adopter pinning `atd-runtime = "1"` and `atd-sdk = "1"` always gets a
mutually consistent set. The whole stack moves as one version through
the 1.x line — see §8.

---

## 2. Release shape

| Property | Value |
|---|---|
| Version | `1.0.0` (workspace-wide; every crate inherits `version.workspace = true`) |
| Cadence | Single coordinated wave — not per-crate independent versioning |
| Tag | `v1.0.0` (semver-prefixed; existing `sp-*` / `phase-*` work tags are preserved) |
| Channel | Stable crates.io |
| MSRV | `1.85` (`workspace.package.rust-version`) |
| Edition | `2024` |
| License | `Apache-2.0` |
| Repository | `https://github.com/downsea/atd` |

### Why 1.0 now

The wire surface has been stable across `sp-capability-v2`,
`sp-token-broker-phase2`, `sp-pagination-v1`, and the medical
middleware — every addition since 0.2.1 has been *additive*. Two
production adopters (`healthkit_cli`, `celia_phr`) and a third
(`cbrain`) have validated against the line. The schema is published and
CI-gated. There is nothing left that a known adopter needs to *break*
the wire to get. 1.0 makes the de-facto stability a promise.

### The version-string change

The workspace root `Cargo.toml` `workspace.package.version` moves
`0.3.0` → `1.0.0` and `repository` updates to
`https://github.com/downsea/atd`. Every crate is `version.workspace =
true`, so the bump is a one-line change that propagates.

---

## 3. Per-crate publication matrix

15 publishable crates + 1 internal-only. Crate list verified against the
`Cargo.toml` `[workspace] members` array (the `examples` member is a dev
crate and is not published).

| Crate | publish? | Stable surface? | Notes |
|---|---|---|---|
| `atd-protocol` | ✅ | **yes** | Wire-type root. Every external implementer depends on it. The schema's Rust source. |
| `atd-runtime` | ✅ | **yes** | `Tool` / `Registry` / `Binding` / `Middleware` / `TokenBroker` / `FileTokenBroker` / `CursorIssuer` / `AuditSink` / UCAN verifier. |
| `atd-sdk` | ✅ | **yes** | Client API: `discover` / `describe` / `call` / `call_page` / `call_all` / `hello`. |
| `atd-server` | ✅ | **yes** | Unix-socket listener. Ships paired with `atd-runtime`. |
| `atd-server-http` | ✅ | **yes** | HTTP transport + MCP JSON-RPC translator + bearer auth + SSE refresh. |
| `atd-middleware-fhir` | ✅ | **yes** | FHIR R4 egress validation + 75-URI whitelist. |
| `atd-middleware-pii-redact-medical` | ✅ | **yes** | HIPAA Safe Harbor PHI redaction. |
| `atd-tools-echo` | ✅ | minor | Built-in echo tool; the documented `Tool` template. |
| `atd-tools-fs` | ✅ | minor | Built-in fs tool. |
| `atd-tools-shell` | ✅ | minor | Built-in shell tool. |
| `atd-tools-web` | ✅ | minor | Built-in web tool. |
| `atd-conformance` | ✅ | yes | Conformance scenarios; adopters dev-dep on it to test their implementation. |
| `atd-cli` | ✅ | yes | Reference CLI client — the `atd` command. |
| `atd-mcp-bridge` | ✅ | yes | MCP-over-stdio bridge; adopters install via `cargo install`. |
| `atd-ref-server` | ✅ | yes | Reference server binary. |
| `atd-mock-weather-server` | **❌ `publish = false`** | n/a | Cross-vendor demo helper; stays internal. |

"Stable surface" = covered by §1.3's trait-stability and §1.1's wire
guarantees. The `atd-tools-*` crates are marked "minor" — their *tools*
are reference implementations and may evolve more freely than the
protocol core, but they stay wire-compatible.

**Action before publish day:** confirm crates.io ownership for every
crate name (`cargo owner --list atd-protocol`, etc.). A squatter on any
`atd-*` name we do not yet own is a publish blocker.

---

## 4. Pre-release checklist

Run all of these from a clean `git pull origin master` with no
uncommitted changes. Status reflects the planning snapshot — **a human
executes and re-confirms each item on release day**; do not treat a
pre-ticked box as a substitute for re-running the gate.

### 4.1 The four workspace gates

- [ ] `cargo fmt --all -- --check` → clean
- [ ] `cargo clippy --workspace --all-features --all-targets -- -D warnings` → all green
- [ ] `cargo nextest run --workspace` → all green (or `cargo test --workspace --all-targets`)
- [ ] `cargo build --release --workspace` → no warnings beyond clippy-clean

### 4.2 Schema drift gate

- [ ] `cargo run -p atd-protocol --features schema --bin gen-schema -- --check` → committed `atd-protocol-schema.json` matches the Rust types
- [ ] The schema validates against the JSON Schema 2020-12 meta-schema (the `gen-schema` check covers this)

### 4.3 Conformance

- [ ] `atd-conformance` suite green — including `concurrent_handshake_storm` (50-client SLO: p99 < 200 ms, 0 errors, 0 audit drops), `paginated_dispatch` (100-row, 10-page cursor walk), and `phase_l_baseline` (5-AC cross-repo verification)

### 4.4 Docs + metadata

- [ ] [`CHANGELOG.md`](../CHANGELOG.md) has a `[1.0.0]` section describing every change since 0.3.0
- [ ] [`architecture.md`](architecture.md), [`roadmap.md`](roadmap.md), [`index.md`](index.md) cross-links resolve
- [ ] Each publishable `crates/*/Cargo.toml` has `description`, `license` (inherited Apache-2.0), `repository` (`https://github.com/downsea/atd`), `readme`, `keywords`, `categories`
- [ ] Each publishable crate has a `README.md` in its own directory
- [ ] `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS="-D warnings"` → no broken intra-doc links

### 4.5 `cargo publish --dry-run` per crate

- [ ] `cargo publish -p <crate> --dry-run` passes for all 15 publishable crates — flushes out missing `repository`, pure-path deps (every `path =` dep must carry a matching `version =`), and packaging errors *without burning the version*

### 4.6 Legal + security

- [ ] `LICENSE` (Apache-2.0) present at repo root — required for crates.io validation
- [ ] `cargo audit` → no high/critical advisories on direct deps
- [ ] `RedactedString` `Debug`/`Display` non-leak test green; `FileTokenBroker` `0600`/`0700` permission test green; `atd-middleware-fhir` whitelist drift-guard green

### 4.7 Adopter sign-off

- [ ] **`healthkit_cli`** — builds + full test suite passes against the 1.0 candidate (Unix-socket adopter)
- [ ] **`celia_phr`** — builds + tests pass against the 1.0 candidate (HTTP-transport adopter)
- [ ] **`cbrain`** — builds + tests pass against the 1.0 candidate (see [`issues/2026-05-19-cbrain-adopter-requirements.md`](issues/2026-05-19-cbrain-adopter-requirements.md))

All three adopters consume via `path =` deps today; sign-off confirms
the 1.0 surface is a drop-in semver-compatible upgrade for each.

---

## 5. Publish order

`cargo publish` requires bottom-up order — a dependency must already be
on crates.io before its dependents publish. Waves inferred from the
workspace dep graph:

```
Wave 1 — no inter-crate deps:
  atd-protocol

Wave 2 — depends on atd-protocol:
  atd-runtime

Wave 3 — depends on atd-runtime + atd-protocol:
  atd-server
  atd-sdk
  atd-middleware-fhir
  atd-middleware-pii-redact-medical

Wave 4 — depends on Wave 3:
  atd-server-http        # atd-server + atd-runtime + atd-protocol
  atd-conformance        # atd-sdk
  atd-tools-echo         # atd-runtime
  atd-tools-fs           # atd-runtime
  atd-tools-shell        # atd-runtime
  atd-tools-web          # atd-runtime

Wave 5 — binaries on top:
  atd-cli                # atd-sdk
  atd-mcp-bridge         # atd-sdk + atd-protocol
  atd-ref-server         # atd-server + all atd-tools-* + atd-middleware-*
```

Publish in any order *within* a wave, then wait for crates.io to index
(typically < 90 s) before the next wave.

```bash
# Wave 1
cargo publish -p atd-protocol --token "$CARGO_TOKEN"
sleep 90

# Wave 2
cargo publish -p atd-runtime --token "$CARGO_TOKEN"
sleep 90

# Wave 3 (parallel)
cargo publish -p atd-server --token "$CARGO_TOKEN" &
cargo publish -p atd-sdk --token "$CARGO_TOKEN" &
cargo publish -p atd-middleware-fhir --token "$CARGO_TOKEN" &
cargo publish -p atd-middleware-pii-redact-medical --token "$CARGO_TOKEN" &
wait
sleep 90

# Wave 4 (parallel)
cargo publish -p atd-server-http --token "$CARGO_TOKEN" &
cargo publish -p atd-conformance --token "$CARGO_TOKEN" &
cargo publish -p atd-tools-echo --token "$CARGO_TOKEN" &
cargo publish -p atd-tools-fs --token "$CARGO_TOKEN" &
cargo publish -p atd-tools-shell --token "$CARGO_TOKEN" &
cargo publish -p atd-tools-web --token "$CARGO_TOKEN" &
wait
sleep 90

# Wave 5 (parallel)
cargo publish -p atd-cli --token "$CARGO_TOKEN" &
cargo publish -p atd-mcp-bridge --token "$CARGO_TOKEN" &
cargo publish -p atd-ref-server --token "$CARGO_TOKEN" &
wait
```

`atd-mock-weather-server` is never published — its `Cargo.toml` carries
`publish = false`.

---

## 6. Tagging + GitHub release

After all 15 crates land on crates.io:

```bash
git tag -a v1.0.0 -m "v1.0.0 — first stable release; wire format frozen for the 1.x line.
See CHANGELOG.md and docs/release-plan-v1.0.md."
git push origin v1.0.0
```

Then open the GitHub release:

```bash
gh release create v1.0.0 \
  --repo downsea/atd \
  --title "v1.0.0 — first stable release" \
  --notes-file CHANGELOG.md \
  --verify-tag
```

Existing work-anchor tags (`sp-pagination-v1`, `sp-concurrency-baseline`,
`phase-l-0`, …) **stay** — they are not release tags. `v1.0.0` is the
release tag; `v0.3.0` (if cut) and earlier `v0.x` tags remain in history.

---

## 7. Rollback

If a published crate is found to have a critical bug post-publish,
`cargo yank` the affected version:

```bash
cargo yank --version 1.0.0 atd-runtime
```

`yank` does **not** delete — existing `Cargo.lock` files keep resolving
— but new `cargo add` / `cargo update` skip the yanked version. Then
ship `1.0.1` with the fix.

**Rules:**

- **Never yank without a successor.** A yanked version with no `1.0.1`
  strands adopters on whatever they had locked. Publish the fix first,
  then yank.
- **Workspace-wide yank** (if the whole release must be pulled): yank
  every crate at the version in **reverse publish order** — binaries
  first, `atd-protocol` last. This matches the dep-teardown direction.
- A yank is a stopgap, not a release process — the fix release `1.0.1`
  follows the same checklist (§4) and publish order (§5).

---

## 8. Post-1.0 versioning policy

**Through the 1.x line: workspace-lockstep.** Every publishable crate
stays at one shared `workspace.package.version`. A `1.1.0` means *every*
crate goes to `1.1.0`, whether or not that crate changed. The rationale
holds from 0.x and is now load-bearing for the stability contract: the
crates are tightly coupled — `atd-protocol` types flow through
`atd-runtime` → `atd-sdk` and the listeners — and per-crate versioning
would let an adopter mix a fresh `atd-protocol` with a stale
`atd-runtime` and silently break a wire contract. Lockstep forces
all-or-nothing upgrades, which is exactly what a frozen-wire promise
needs.

**Minor vs patch within 1.x:**

- **Patch** (`1.0.x`) — bug fixes, no surface change.
- **Minor** (`1.x.0`) — additive only: new optional field, new enum
  variant, new error code, new tool, new defaulted trait method. Never
  wire-breaking (see §1).
- **Major** (`2.0.0`) — any wire-breaking change. Batched; debt waits
  for one place. See [`roadmap.md`](roadmap.md) §4.

**Revisit per-crate independent versioning at 2.0.** Once a 2.0 is on
the table, whether the now-mature stable crates (`atd-protocol`,
`atd-runtime`, `atd-sdk`) can carry independent semver — so a pure
`atd-tools-fs` fix need not bump `atd-protocol` — is worth
reconsidering. It is explicitly *not* changed within 1.x: lockstep is
the 1.x contract.

---

## See also

- [`../CHANGELOG.md`](../CHANGELOG.md) — the truth for what changed.
- [`architecture.md`](architecture.md) — normative architecture; §2.5
  (schema stability), §9 (crate map + versioning).
- [`roadmap.md`](roadmap.md) — evolution scope; §4 is the wire-freeze
  rule this contract enforces.
- [`index.md`](index.md) — documentation map and authority tiers.
- [`protocol/wire-format.md`](protocol/wire-format.md) ·
  [`protocol/error-codes.md`](protocol/error-codes.md) — the frozen
  wire and error surfaces §1 commits.
- [`../AGENTS.md`](../AGENTS.md) · [`../CONTRIBUTING.md`](../CONTRIBUTING.md)
  — build / test / verify SOP behind §4's gates.
