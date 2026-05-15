# Release Plan — v0.3.0

**Target:** ship `atd-protocol`, `atd-runtime`, `atd-sdk`,
middleware crates, and listener crates as a coordinated `0.3.0`
release on crates.io. Binary crates (`atd-cli`, `atd-ref-server`,
`atd-mcp-bridge`) ship in the same wave but their "release" is a
GitHub release artifact + the published crate — adopters typically
build from source.

**Status:** Planning. Adopters (celia_phr, healthkit_cli) already
validated against `master` HEAD; CHANGELOG and architecture doc are
green. Pre-release checklist is the gate.

**Authority chain:** `CHANGELOG.md` is the truth for what changed.
This file is the truth for **how to ship it**.

---

## Table of contents

1. [Release shape](#1-release-shape)
2. [Per-crate publication matrix](#2-per-crate-publication-matrix)
3. [Pre-release checklist](#3-pre-release-checklist)
4. [Publish order](#4-publish-order)
5. [Tagging + GitHub release](#5-tagging--github-release)
6. [Adopter notification](#6-adopter-notification)
7. [Rollback plan](#7-rollback-plan)
8. [Post-release follow-ups](#8-post-release-follow-ups)

---

## 1. Release shape

| Property | Value |
|---|---|
| Version | `0.3.0` (workspace-wide; every crate inherits `version.workspace = true`) |
| Cadence | Single coordinated wave — not per-crate independent versioning |
| Tag | `v0.3.0` (semver-prefixed; existing `phase-l-0`, `sp-*` work tags are preserved) |
| Channel | Stable crates.io |
| MSRV | `1.85` (from `workspace.package.rust-version`) |
| Edition | `2024` |
| License | `Apache-2.0` |

### Why workspace-wide, not per-crate

The crates are tightly coupled (`atd-protocol` types flow through
`atd-runtime` → `atd-sdk` and the listener crates). Per-crate
versioning would let adopters mix a fresh `atd-protocol` with a
stale `atd-runtime` and silently break wire contracts. Workspace-
shared versions force "all-or-nothing" upgrades, which is what we
actually want during 0.x.

We revisit this policy at 1.0.

### Why 0.3.0 and not 0.4.0

The workspace already moved from 0.2.1 to 0.3.0 in commit
`f75cde5` (chore: bump 0.2.x → 0.3.0 for SP-tool-visibility-
hidden). Subsequent SPs landed on the 0.3.0 line. Publishing the
already-bumped line keeps `Cargo.toml` and the published crate
version aligned.

If anyone argues for 0.4.0 because of SP-capability-v2's surface
breadth: the gate is **wire-incompatible removal**, not surface
breadth. UCAN-lite is *additive* (granted = strings ∪ ucan), so it
stays minor.

---

## 2. Per-crate publication matrix

15 publishable crates + 1 internal-only.

| Crate | publish? | Stable surface? | Notes |
|---|---|---|---|
| `atd-protocol` | ✅ | **yes** | The wire-type root. Every external implementer depends on it. |
| `atd-runtime` | ✅ | **yes** | Tool / Registry / Middleware / TokenBroker / FileTokenBroker / Cursor. |
| `atd-sdk` | ✅ | **yes** | Client API. `call_page`, `call_all`, hello, capability negotiation. |
| `atd-server` | ✅ | **yes** | UDS listener. Ship pairs with `atd-runtime`. |
| `atd-server-http` | ✅ | **yes** (new in 0.3) | HTTP transport, bearer auth, MCP translator. |
| `atd-middleware-fhir` | ✅ | **yes** (new in 0.3) | FHIR validation + 75-URI whitelist. |
| `atd-middleware-pii-redact-medical` | ✅ | **yes** (new in 0.3) | HIPAA PHI redaction. |
| `atd-mcp-bridge` | ✅ | yes | MCP-over-stdio bridge — adopters install via `cargo install` |
| `atd-cli` | ✅ | yes | Reference CLI client. |
| `atd-ref-server` | ✅ | yes | Reference server binary. |
| `atd-conformance` | ✅ | yes | Conformance suite — consumers depend on it as a dev-dep to test their implementation. |
| `atd-tools-echo` | ✅ | minor | Built-in echo tool. Adopters wire it as a dependency on `atd-ref-server`. |
| `atd-tools-fs` | ✅ | minor | Built-in fs tool. |
| `atd-tools-shell` | ✅ | minor | Built-in shell tool. |
| `atd-tools-web` | ✅ | minor | Built-in web tool. |
| `atd-mock-weather-server` | **❌ `publish = false`** | n/a | Cross-vendor demo helper. Keep internal. |

**Action:** confirm crates.io ownership for each crate name before
publish day. The protocol-layer names (`atd-protocol`, `atd-sdk`,
`atd-runtime`, etc.) must already be owned by the project — verify
via `cargo owner --list atd-protocol`. Any squatter on a name we
don't yet own becomes a blocker.

---

## 3. Pre-release checklist

Run all of these from a clean `git pull origin master`; no
uncommitted changes.

### 3.1 Code gates

- [ ] `cargo nextest run --workspace` → 620/620 (current baseline)
- [ ] `cargo clippy --workspace --all-features --all-targets -- -D warnings`
      → **currently fails** in `atd-server-http`, `atd-tools-fs`,
      `atd-ref-server` example, and one `mcp.rs` non-snake-case
      identifier. **Blocker** — must be cleaned up before publish.
      (Mechanical struct-literal rewrites + one `#[allow(non_snake_case)]`
      on the test fn; ~1 hour of work.)
- [ ] `cargo fmt --all -- --check` → clean
- [ ] `cargo build --workspace --release` → no warnings beyond
      clippy clean
- [ ] `cargo doc --workspace --no-deps` → no broken intra-doc
      links (cargo emits these as warnings; fail-on-warning gate
      via `RUSTDOCFLAGS="-D warnings"`)

### 3.2 Doc + metadata gates

- [ ] `CHANGELOG.md` describes every SP / Phase tag since v0.2.1
      (✅ shipped in this PR)
- [ ] Each publishable `crates/*/Cargo.toml` has:
    - `description = "..."` (✅ all set per audit)
    - `license = "Apache-2.0"` (✅ inherited from `workspace.package`)
    - `repository = "https://github.com/downsea/atd-mvp"` (✅
      inherited)
    - `readme = "README.md"` (verify each — two crates were
      missing their README files until `b204c21`)
    - `keywords` + `categories` (already present)
- [ ] Each publishable crate has at least a stub `README.md` in
      its own directory
- [ ] No `path = "../<crate>"` deps without a matching
      `version = "..."` field — crates.io rejects pure-path deps.
      Verify via `cargo publish --dry-run` per crate.
- [ ] `LICENSE` file at repo root (Apache-2.0) — must be present
      for `crates.io` validation
- [ ] License headers in every `.rs` file at the workspace root
      — out of scope for 0.3.0, but called out for 0.4.0

### 3.3 Adopter compat gates

- [ ] `celia_phr` builds against `path = "../atd-mvp/..."` deps
      pointed at master HEAD. **Validated** by `celia_phr/docs/atd-
      mcp-opt-iter4-baseline.md` (2026-05-12).
- [ ] `healthkit_cli` builds + 218 tests pass against master HEAD.
      **Validated** by `healthkit_cli/docs/sp-pagination-v1-adopter.md`.
- [ ] Phase L.0 cross-repo drift-guard passes both directions
      (atd → celia toml set-equal; celia's symmetric test left as
      an action item for celia — snippet in
      [celia_phr PR #38 comment](https://github.com/downsea/celia_phr/pull/38#issuecomment-4428905433))

### 3.4 Security sweep

- [ ] `cargo audit` — no high/critical advisories on direct deps
- [ ] No `unwrap()` on parsed wire input (audit `atd-protocol/src/`
      + `atd-runtime/src/dispatch.rs` paths)
- [ ] `RedactedString`'s Debug/Display still refuses to leak (test
      `redacted_string_debug_does_not_leak` should remain green)
- [ ] FileTokenBroker `unix_file_permissions_are_0600` test green
- [ ] `atd-middleware-fhir` set-equality drift-guard green

---

## 4. Publish order

`cargo publish` for crates with dependencies among themselves
requires bottom-up order — the dependent crate must already be
on crates.io when the dependee's `cargo publish` runs. Inferred
from the workspace dep graph:

```
Wave 1 (no inter-crate deps):
  atd-protocol

Wave 2 (depend on atd-protocol):
  atd-runtime

Wave 3 (depend on atd-runtime + atd-protocol):
  atd-server
  atd-sdk
  atd-middleware-fhir
  atd-middleware-pii-redact-medical

Wave 4 (depend on Wave 3):
  atd-server-http        # depends on atd-server + atd-runtime + atd-protocol
  atd-conformance        # depends on atd-sdk
  atd-tools-echo         # depends on atd-runtime
  atd-tools-fs           # depends on atd-runtime
  atd-tools-shell        # depends on atd-runtime
  atd-tools-web          # depends on atd-runtime

Wave 5 (binaries on top):
  atd-cli                # depends on atd-sdk
  atd-mcp-bridge         # depends on atd-sdk + atd-protocol
  atd-ref-server         # depends on atd-server + all atd-tools-* + atd-middleware-*
```

Each wave: publish in any order **within** the wave, then wait
until crates.io has indexed (typically <60s) before kicking off
the next wave. The release driver script:

```bash
# Wave 1
cargo publish -p atd-protocol --token "$CARGO_TOKEN"

# Pause for crates.io index propagation
sleep 90

# Wave 2
cargo publish -p atd-runtime --token "$CARGO_TOKEN"

sleep 90

# Wave 3 (run in parallel)
cargo publish -p atd-server --token "$CARGO_TOKEN" &
cargo publish -p atd-sdk --token "$CARGO_TOKEN" &
cargo publish -p atd-middleware-fhir --token "$CARGO_TOKEN" &
cargo publish -p atd-middleware-pii-redact-medical --token "$CARGO_TOKEN" &
wait

sleep 90

# Wave 4
cargo publish -p atd-server-http --token "$CARGO_TOKEN" &
cargo publish -p atd-conformance --token "$CARGO_TOKEN" &
cargo publish -p atd-tools-echo --token "$CARGO_TOKEN" &
cargo publish -p atd-tools-fs --token "$CARGO_TOKEN" &
cargo publish -p atd-tools-shell --token "$CARGO_TOKEN" &
cargo publish -p atd-tools-web --token "$CARGO_TOKEN" &
wait

sleep 90

# Wave 5
cargo publish -p atd-cli --token "$CARGO_TOKEN" &
cargo publish -p atd-mcp-bridge --token "$CARGO_TOKEN" &
cargo publish -p atd-ref-server --token "$CARGO_TOKEN" &
wait
```

**Before the script runs:** dry-run each wave's publish to flush
out crates.io validation errors (missing `repository`,
non-versioned path deps, etc.) without burning the version:

```bash
for c in atd-protocol atd-runtime atd-sdk ...; do
  cargo publish -p "$c" --dry-run
done
```

---

## 5. Tagging + GitHub release

After all 15 crates land on crates.io:

```bash
git tag -a v0.3.0 -m "v0.3.0 — federation + multi-tenant + performance + medical
See CHANGELOG.md and docs/release-plan-v0.3.0.md."
git push origin v0.3.0
```

Then open a GitHub release:

```bash
gh release create v0.3.0 \
  --title "v0.3.0 — federation + multi-tenant + performance + medical" \
  --notes-file CHANGELOG.md \
  --verify-tag
```

Existing tags (`phase-l-0`, `sp-pagination-v1`,
`sp-concurrency-baseline`, etc.) **stay**. They're work-anchors,
not release tags. `v0.3.0` is the only "this is a published
release" tag.

---

## 6. Adopter notification

The two known production adopters consume via path-dep but will
want to switch to crates.io once 0.3.0 is up. Notify on release
day:

### healthkit_cli

Issue / PR to file in `downsea/healthkit_cli`:

> ATD 0.3.0 is now on crates.io. The path-dep wiring in your
> `Cargo.toml` can switch to `atd-runtime = "0.3"`,
> `atd-sdk = "0.3"`, etc. once you're ready. No source changes
> needed — semver preserves the `Tool::call_paginated` +
> `TokenBroker::resolve_bearer` surfaces you already adopted.

### celia_phr

Comment on `downsea/celia_phr` (probably PR #38 follow-up):

> ATD 0.3.0 published. Phase L.0's primitives — `FileTokenBroker`,
> `ALLOWED_SYSTEMS_DEFAULT`, cursor signing — are now on crates.io
> for L.1's `AtdUpstreamIngest` to depend on. Path-dep wiring can
> stay for in-tree dev, but a release-stable line is now an
> option.

---

## 7. Rollback plan

If a published crate is discovered to have a critical bug post-
publish, `cargo yank` the specific version:

```bash
cargo yank --vers 0.3.0 atd-runtime
```

`yank` doesn't delete — existing `Cargo.lock` files continue to
resolve — but new `cargo add` / `cargo update` calls skip the
yanked version. Then ship `0.3.1` with the fix.

**Don't yank without first publishing the fix.** A yanked version
with no successor leaves adopters stranded on whatever they had
locked.

Workspace-wide yank: yank every crate at the version, in reverse
publish-order (bin crates first, `atd-protocol` last). This
matches the dep-tear-down direction.

---

## 8. Post-release follow-ups

Tracked separately; not blockers for the publish wave.

- **Per-crate independent versioning at 1.0.** Workspace-wide
  shared version stops being useful once we promise wire stability.
  Plan: cut a `0.3.0-stable` branch at release, then add a
  `release-please` config or equivalent to manage per-crate
  semver going forward.
- **Workspace clippy cleanup sweep** (`atd-server-http`,
  `atd-tools-fs`, `atd-ref-server` example, mcp.rs naming). Listed
  as a §3.1 blocker — must be done in 0.3.0 publish path.
- **License headers** in every `.rs` file at repo root. Out of
  scope for 0.3.0; track for 0.3.1 or 0.4.0.
- **`cargo doc` polish pass.** Inter-crate `[doc]` links currently
  resolve, but the rendered docs.rs output is uneven (some
  crates have lib-level overview, some don't). Plan: add a
  `//! ...` summary to every `lib.rs` for crates that lack one.
- **CI gating on Actions billing.** The atd-mvp#7 CI run failed
  with "The job was not started because recent account payments
  have failed or your spending limit needs to be increased."
  This must be resolved before 0.3.0 publish so the publish-wave
  GitHub Action (if we add one) can actually run.
- **Symmetric drift-guard on celia side.** Snippet provided in
  the celia_phr PR #38 comment; celia owns the implementation.
  Their CI then catches drift on their side too.
- **Workspace MSRV bump audit.** We currently pin to 1.85. Check
  whether any dep bumped its MSRV past 1.85 during the 0.3 line.
