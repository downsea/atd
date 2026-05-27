# ADR 0004 — Per-crate independent SemVer (deprecate workspace-lockstep release cadence)

- **Status:** Accepted
- **Date:** 2026-05-27
- **Deciders:** `atd` maintainers
- **Supersedes:** the "v0.3.0 走 workspace-wide 统一版本" decision (recorded in vault `20-Projects/Active/atd-mvp.md` 关键决策段, under "早期决策") — re-evaluated per its own "1.0 时复评" trigger, now that the workspace has stabilized at 15 publishable crates and the wire is frozen.
- **Related:** [`docs/release-plan-v1.0.md`](../release-plan-v1.0.md) · [`scripts/release.sh`](../../scripts/release.sh) · [`CHANGELOG.md`](../../CHANGELOG.md)

## 1. Context

Through 1.0, the workspace shipped **workspace-lockstep** versioning: a single `[workspace.package].version` in the root `Cargo.toml`, every member crate set `version.workspace = true`, every release re-published all 15 publishable crates at the same number. The original (v0.3.0-era) rationale:

> atd-protocol 类型贯穿 runtime/sdk/listener，统一版本强制 all-or-nothing 升级，避免 adopter 混用新旧 crate 破坏 wire 契约。

The 1.1.0 release (2026-05-27) made the cost concrete. The change set was a single additive type (`atd_protocol::CliBindingConfig`) confined to one crate. The lockstep policy still re-published 14 crates with **byte-identical source** at the new version number, cost 5 publish waves on `scripts/release.sh`, and added 14 ghost lines to the changelog that say "no behavioral change". The 1.0 decision recorded "1.0 时复评" — this ADR is that re-evaluation, made one release into the 1.x line so the policy doesn't fossilize.

## 2. Decision

**Per-crate independent SemVer**, with `atd-protocol` as the anchor for the ATD release identity.

### 2.1 Authority

- **`atd-protocol`'s version IS the ATD wire/protocol version.** When `atd-protocol` bumps (any of major/minor/patch), the workspace cuts an ATD release with the matching number. The annotated git tag `v<atd-protocol-version>` and the GitHub release continue to anchor here. The 1.x stability contract from [`docs/release-plan-v1.0.md`](../release-plan-v1.0.md) is unchanged — it was always about the wire, not the workspace as a whole.
- **Every other crate has independent SemVer cadence.** It bumps when *its own source* changes (or its declared deps change in a way that demands a re-publish). It can ship a patch release the same week atd-protocol is quiescent, and can lag behind on weeks atd-protocol bumps without doing anything.

### 2.2 Mechanics

- The `[workspace.package].version` field is **removed**. (Other shared workspace fields — edition, license, repository, authors, rust-version — stay.)
- Each crate's `Cargo.toml` carries an **explicit** `version = "X.Y.Z"` instead of `version.workspace = true`.
- Sibling-dep pins (`atd-protocol = { path = "...", version = "X.Y.Z" }`) record the **minimum required** version of that dep, not necessarily the latest. Update them only when the consumer crate actually needs the newer dep's API — caret-compatible resolution handles routine bumps automatically.
- `scripts/release.sh` reads each crate's version from its own `Cargo.toml`, checks crates.io for that version, and **skips publishing crates whose declared version already exists upstream**. New version on a crate → script publishes; unchanged version → no-op for that crate.
- The release tag derives from `atd-protocol`'s version. A release that doesn't change `atd-protocol` does not get a top-level tag — individual crate bumps record themselves in their crate-local CHANGELOG entry under the relevant atd-protocol release section (or under a "between-releases" section).

### 2.3 Adopter contract

Caret pins (`atd-protocol = "1"`, `atd-runtime = "1"`, etc.) **continue to resolve correctly**. Cargo's compatible-version rule means a `= "1"` pin on `atd-runtime` will pick up whatever atd-runtime versions exist in the 1.x range. The migration is transparent for adopters who use caret pins; adopters who pin exact patches will see fewer ghost-version churns.

The wire contract is unchanged. The 1.0 stability promise (no removed fields, no reshaped messages) continues to apply to `atd-protocol` — bumping just atd-runtime, atd-server, or any tool crate doesn't change what's on the wire.

## 3. Consequences

### 3.1 Wins

- **No more ghost re-publishes.** A change that touches only one crate publishes only that crate; crates.io storage doesn't accumulate byte-identical duplicates with fresh version numbers.
- **Honest changelog.** Each crate's version reflects its own diff; the workspace CHANGELOG covers atd-protocol's release identity (= the ATD line bump). Other crates' incidental fixes live in crate-local notes.
- **Faster releases.** Scripts run only the waves containing changed crates. A typical "atd-runtime patch fix" release would touch one wave (~30s of publish + indexing) instead of five.
- **More accurate audit trail.** `cargo info atd-tools-fs` shows true release dates of changes to that crate, not whenever the workspace happened to ship.
- **Adopter SemVer is more meaningful.** When `atd-runtime` bumps to 1.2.0 it's because atd-runtime changed; when atd-protocol bumps to 1.2.0 it's because the protocol changed.

### 3.2 Costs

- **Coordination discipline.** Contributors must remember to bump the right crate(s) when making changes; CI/release script must check that bumped crates' source actually changed (a `cargo publish` rejects identical content at a new version, so the floor is enforced by the registry).
- **Wider sibling-dep pin surface.** Updating a dep across the workspace (e.g. "atd-runtime now requires atd-protocol 1.2 feature X") means hand-editing each consumer's `Cargo.toml`. Tooling already required this; nothing new.
- **Slightly more involved CHANGELOG curation.** The single workspace `CHANGELOG.md` continues to chronicle ATD releases (anchored on atd-protocol's version). Crate-specific notes can go inline ("atd-runtime: bumped 1.1.0 → 1.1.1 between 1.1 and 1.2 — see [...]") or in per-crate `CHANGES.md` files if volume demands. For now, single workspace CHANGELOG with per-crate sub-entries.
- **Bevy / Embassy-style "ATD 1.2 includes" narrative gets fuzzier.** A user asking "what's in ATD 1.2?" gets the answer from `atd-protocol`'s changelog + summaries of what's new in dep crates since `atd-protocol = 1.1`. Doable; less elegant than "everything is 1.2".

The wins outweigh the costs for ATD's current shape (15 crates, additive-mostly evolution, mature wire). For projects with violently coupled multi-crate changes (Bevy is the canonical case), lockstep makes more sense.

### 3.3 Migration

This ADR is the policy change. The mechanical migration is:

- Remove `version = "1.1.0"` from `[workspace.package]`.
- For each member crate, change `version.workspace = true` to `version = "1.1.0"` (its current value as of 1.1.0).
- `scripts/release.sh` refactored to read each crate's version individually.
- `examples` workspace member is not publishable (per existing exclusion) — unchanged.

Subsequent releases follow the new policy:
1. Author bumps the version of every crate whose source actually changed.
2. Updates sibling pins only when a new minimum is needed.
3. Runs `scripts/release.sh` — it publishes only the crates whose Cargo.toml versions don't yet exist on crates.io.
4. Tag/GitHub-release: if atd-protocol bumped, the script tags `v<atd-protocol-new>`; otherwise no top-level tag.

## 4. Open questions

- **Should each crate carry its own `CHANGELOG.md`?** Not in 1.x — the workspace `CHANGELOG.md` continues as the audit trail with per-crate sub-entries when a release touches multiple crates. Revisit if any single crate's release notes grow enough to drown out atd-protocol's.
- **Should the release script auto-detect "this crate's content didn't change but its version was bumped"?** crates.io enforces it from below (refuses identical bytes at new version) so we don't need a pre-flight check. But a `cargo publish --dry-run` per bumped crate before the real run would catch finger-trouble.
- **Cross-tagging:** beyond the top-level `v<atd-protocol>` tag, do we want per-crate tags (`atd-runtime-v1.1.1`)? Not initially; adoption surface is small enough that crates.io is the canonical release record.

## 5. Status

**Accepted.** Migration lands together with this ADR (the explicit-version Cargo.toml change + `scripts/release.sh` refactor). The 1.1.0 release stays as published — it was the last lockstep release.
