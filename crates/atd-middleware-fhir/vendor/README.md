# `vendor/celia-whitelists.toml` — vendored snapshot of celia's CodeSystem
allow-list.

**Source of truth:** the `celia_phr` repo at
`crates/celia-types/data/whitelists.toml`.

**Why vendor at all?** A cross-repo invariant requires that
`atd-middleware-fhir::ALLOWED_SYSTEMS_DEFAULT` and celia's
`ALLOWED_CODE_SYSTEMS` stay set-equal. The drift-guard test
[`tests::vendored_toml_matches_default`](../src/systems.rs) parses this file at
test time and asserts set equality against the Rust constant. Vendoring keeps
the test self-contained (no external checkout dependency).

## Sync protocol

1. When celia adds or removes a CodeSystem entry in `whitelists.toml`, the next
   drop into this repo re-copies the file:

   ```bash
   cp <celia_phr>/crates/celia-types/data/whitelists.toml \
      crates/atd-middleware-fhir/vendor/celia-whitelists.toml
   ```

2. Update `ALLOWED_SYSTEMS_DEFAULT` in
   `crates/atd-middleware-fhir/src/systems.rs` to match the new set.

3. Run `cargo test -p atd-middleware-fhir --lib systems` — both the set
   equality test and the count test must pass.

4. Commit both files in the same change with a `feat(fhir):` prefix so
   downstream adopters notice the additive semantic shift.

## What about the reverse direction?

celia's CI should also depend on `atd-middleware-fhir` and run a mirror
assertion: parse its own `whitelists.toml` (or the generated constant) and
compare against the public `ALLOWED_SYSTEMS_DEFAULT` re-exported here. Either
repo updating its set without the other will fail one of the two CI gates — the
system is symmetric.

## Last sync

- **Date:** 2026-05-12
- **Entry count:** 75 systems + 1 legacy meta URI
</content>
