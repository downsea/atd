# No machine-readable protocol schema

**Layer:** schema
**Status:** closed-verified
**Effort:** ~0.5 day
**Filed:** 2026-04-24
**Closed:** 2026-04-25
**Related tag:** `sp11-docs`

## Resolution

**Shipped** as **SP-protocol-schema** (tag `sp-protocol-schema`,
2026-04-25). `/atd-protocol-schema.json` is generated from the Rust
types in `atd-protocol` via `schemars`, validated against the JSON
Schema 2020-12 meta-schema, and CI-gated for drift. Third-party
implementers no longer need to read Rust to build a TS / Go / Java
implementation. See the `[0.3.0]` entry in
[`CHANGELOG.md`](../../CHANGELOG.md) ("Machine-readable protocol
schema") and [`docs/atd-architecture.md`](../atd-architecture.md) §2. The body
below is the original gap report, kept as a record.

## Summary

The ATD wire protocol has no canonical machine-readable schema artifact.
Third-party implementers (TypeScript / Go / Java / .NET) must read Rust
source code or markdown reference docs to know what a valid
`ToolSummary`, `ToolDefinition`, `ToolResult`, or `AtdError` looks like.

## Current state

Sources of truth today:

1. **Rust types** in `crates/atd-types/src/{summary,tool,result,error,enums}.rs`
   — authoritative for Rust consumers, opaque to anyone else.
2. **Python types** in `python/src/atd_client/types.py` — hand-ported
   mirror of the Rust types; drift-prone.
3. **Markdown** in `docs/protocol/wire-format.md` — narrative
   reference with type tables pasted from Rust source; also drift-prone.

None of these is consumable by code-generation tools like
`json-schema-to-typescript`, `quicktype`, or `openapi-generator`.

## Gap

- No `atd-protocol-schema.json` (or `.yaml`) in the repo
- No CI drift check between the Rust types and any schema artifact
- `ToolDefinition.input_schema` is JSON Schema at the tool level, but
  the envelope types (the messages themselves) have no schema
  declaration

## Impact

- **Direct:** writing an ATD client in any language other than Rust /
  Python means reading two Rust files and hoping they're current.
- **Indirect:** SP-8 conformance suite (when shipped) will need
  machine-readable fixtures to test responses against; without a schema
  source-of-truth, the conformance suite has to hand-code them.

## Proposed approach

1. Add `schemars = "0.8"` as a dep on `atd-types` (feature-gated so default
   builds don't pay the cost).
2. `#[derive(JsonSchema)]` on every `pub struct` / `pub enum` in
   atd-types (behind `#[cfg(feature = "schema")]`).
3. New bin `crates/atd-types/src/bin/gen-schema.rs` that emits
   `atd-protocol-schema.json` to the repo root.
4. CI step: regenerate, diff against committed file, fail if they drift.
5. Reference the generated schema from `docs/protocol/wire-format.md`.

## Acceptance

- `atd-protocol-schema.json` exists and validates against the
  [JSON Schema 2020-12 meta-schema](https://json-schema.org/draft/2020-12/schema)
- `cargo run --bin gen-schema` produces identical output to the committed
  file
- CI fails if the committed schema is stale
- `docs/protocol/wire-format.md` links to the generated schema as the
  machine-readable source-of-truth

## Related

- [`docs/protocol/wire-format.md`](../protocol/wire-format.md)
- [`crates/atd-types/`](../../crates/atd-types/)
- Design rationale originally suggested in a post-SP-11 gap discussion
