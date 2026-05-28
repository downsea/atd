# Changing the wire protocol & schema

**Purpose:** the heavyweight path — adding a wire type, a field, an error code,
or a capability string to ATD's protocol itself.

## When to use this — and the warning first

**This is not a no-fork extension point.** The other seven guides attach
behaviour through `pub` traits without touching the wire; this one changes the
wire vocabulary. A change here re-shapes `atd-protocol-schema.json`, the single
artifact every cross-language SDK and every conformance test depends on.

Reach for this guide only when the capability genuinely cannot be expressed
within the existing types — and read the [1.0 stability rule](#the-10-stability-rule)
*before* you start. Some changes are not allowed in the 1.x line at all.

The protocol lives in `crates/atd-protocol/src/`:

| File | Holds |
|---|---|
| `messages.rs` | `Request` / `Response` envelope enums; the `ERR_*` numeric-code constants. |
| `tool.rs` | `ToolDefinition` and its sub-structs (`ToolCapability`, `ToolBinding`, `ToolSafety`, `ToolResources`, `ToolTrust`, `ToolErrorDef`). |
| `enums.rs` | `BindingProtocol`, `SafetyLevel`, `ToolTier`, `ToolVisibility`, `TrustLevel`. |
| `error.rs` | the `AtdError` enum. |
| `result.rs` / `summary.rs` / `wire.rs` / `sanitize.rs` | `ToolResult`, `ToolSummary`, the framing codec, the id sanitiser. |
| `src/bin/gen-schema.rs` | the schema generator. |

## Adding a wire type or a field

1. **Edit the Rust type** in the relevant `atd-protocol` file. Every wire type
   derives `Serialize, Deserialize` and is gated for `schemars::JsonSchema`
   behind `#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]` — keep
   that attribute on anything new.
2. **Make new fields optional and back-compatible.** Annotate with
   `#[serde(default)]` (and `skip_serializing_if = "Option::is_none"` /
   `"Vec::is_empty"` where appropriate) so a peer on the old shape still
   deserialises. `Request::Hello.ucan_tokens` and
   `Response::ToolResultResponse.next_cursor` are the model — both were added
   additively and old clients ignore them.
3. **Add a round-trip test** in the same file's `#[cfg(test)] mod tests`:
   serialize → deserialize → assert; and assert that omitting a new optional
   field deserialises to its default. `messages.rs` has the worked examples
   (`tool_result_response_back_compat_default_when_field_missing`).
4. **Regenerate the schema** (below) and commit the updated
   `atd-protocol-schema.json` in the same change.

## Adding an `AtdError` variant + numeric code

Two distinct things, often done together:

- **The `AtdError` enum** (`error.rs`) is the client-side Rust error type. Add a
  variant; if it carries a non-schema-describable field (a boxed `dyn Error`),
  mark it `#[cfg_attr(feature = "schema", schemars(skip))]` — `ToolExecutionFailed`
  and `ServerUnreachable` do this. Update `is_retryable()` and `suggest_fix()`
  if the new variant has a sensible retry posture or fix hint.
- **The numeric wire code** is a `pub const ERR_*: u16` in `messages.rs` — these
  travel in `Response::Error.code`. Existing families: `1001` capability denied,
  `1002` rate limited, `1003` broker failed, `1010–1013` UCAN, `1020–1021`
  cursor. Pick the **next free number in the right family band** (do not reuse a
  retired number, do not collide across families — `messages.rs` has a test
  asserting distinctness). Document the new constant with a `///` comment
  stating the wire meaning and `retryable` posture.

After either, **update [`../protocol/error-codes.md`](../protocol/error-codes.md)** —
the authoritative error table — in the same change. A new code without a
documented row is a drift bug.

## Adding a capability string

Capability strings (`healthkit:read`, `records:write`) are **free-form strings**
— they are not an enum, so adding one is *not* a schema change. The operator
declares them at server start (`--grant-capability`); a tool requires them via
`ToolDefinition.required_capabilities`. To introduce a new capability you simply
choose a string and document the convention. No `atd-protocol` edit, no schema
regeneration. (Contrast: adding a `BindingProtocol` or `ToolTier` *enum* variant
*is* a schema change — see the table above.)

## Regenerating the schema

`atd-protocol-schema.json` at the repo root is a checked-in build artifact
generated from the Rust types by `crates/atd-protocol/src/bin/gen-schema.rs`.
After any change to a `pub` wire type:

```bash
# Write the regenerated schema to disk:
cargo run -p atd-protocol --features schema --bin gen-schema

# Verify the committed file matches the Rust types (CI runs this):
cargo run -p atd-protocol --features schema --bin gen-schema -- --check
```

`gen-schema` walks every public root type (`Request`, `Response`,
`ToolSummary`, `ToolDefinition`, `ToolResult`, `ToolResultMetadata`,
`AtdError`), so a transitively-referenced new type lands in `definitions`
automatically. The generator also validates the output against the JSON Schema
2020-12 meta-schema — a malformed schema fails before any adopter sees it.

## The CI drift gate

`gen-schema -- --check` re-generates the schema in memory and byte-compares it
against the on-disk `atd-protocol-schema.json`. If they differ it prints
`atd-protocol-schema.json is stale` and exits non-zero. **CI runs exactly this
check.** A protocol change whose committed JSON was not regenerated fails the
build — so always regenerate and commit the schema in the same change as the
Rust edit. The full SOP is in [`../../AGENTS.md`](../../AGENTS.md) §4.

## The 1.0 stability rule

As of **1.0 the schema is frozen for the entire 1.x line** (see
[`../atd-architecture.md`](../atd-architecture.md) §2.5 and
[`../release-plan-v1.0.md`](../release-plan-v1.0.md)):

| Change | Classification | Allowed in 1.x? |
|---|---|---|
| New **optional** field (`#[serde(default)]`) | additive | Yes — minor bump |
| New enum **variant** (e.g. a `BindingProtocol`) | additive | Yes — minor bump |
| New `Request`/`Response` variant | additive | Yes — minor bump |
| New `ERR_*` code / `AtdError` variant | additive | Yes — minor bump |
| **Removing** a field or variant | breaking | No — major (2.0) |
| **Renaming** / **reshaping** / changing a field's type | breaking | No — major (2.0) |
| Making an optional field **required** | breaking | No — major (2.0) |

Code generated from `atd-protocol-schema.json` at 1.0 must keep deserialising
every 1.x message. That is the promise: additive only. A removal or a reshape is
a 2.0 change and goes through a deprecation cycle, not a minor release.

> **Some changes are never an extension at all.** Adding a `ToolTier` variant or
> reshaping the envelope is fork-level even though the table above shows enum
> *variants* as additive — the bar is whether existing 1.x consumers keep
> working. When in doubt, treat it as breaking and open an ADR
> ([`../adr/`](../adr/)) before writing code.

## Step by step

1. Confirm the change cannot be expressed within existing types (re-read the
   other seven guides).
2. Confirm the change is **additive** per the table — if not, it is a 2.0
   change; stop and open an ADR.
3. Edit the Rust type in `crates/atd-protocol`, keeping the `schema`-feature
   `cfg_attr` and `#[serde(default)]` back-compat annotations.
4. Add round-trip + missing-field-default tests in the file's `tests` module.
5. Run `cargo run -p atd-protocol --features schema --bin gen-schema` and commit
   the regenerated `atd-protocol-schema.json`.
6. Update [`../protocol/error-codes.md`](../protocol/error-codes.md) /
   [`../protocol/wire-format.md`](../protocol/wire-format.md) as the change
   touches them.
7. Run the four workspace gates plus `gen-schema -- --check`
   ([`../../AGENTS.md`](../../AGENTS.md) §4); run the `atd-conformance` suite.

## Invariants you must preserve

- **Additive only in 1.x.** No removals, no reshapes, no required-field
  promotions until 2.0.
- **Schema and Rust never drift.** Regenerate and commit
  `atd-protocol-schema.json` in the same change; `gen-schema -- --check` is the
  CI gate.
- **Never hand-edit `atd-protocol-schema.json`** — it is generated.
- **New optional fields carry `#[serde(default)]`** so old peers deserialise.
- **`ERR_*` codes are unique and family-banded** — next free number in the
  right band, never a reused number.
- **Document on the wire.** A new code without an [`error-codes.md`](../protocol/error-codes.md)
  row, or a new type without a schema entry, is a drift bug.

## See also

- [`../atd-architecture.md`](../atd-architecture.md) §2 (the unified schema), §4 (wire
  & types), §9.3 (the extension-point table — note "change the wire format" and
  "add a `ToolTier` variant" are marked *requires fork*).
- [`../protocol/wire-format.md`](../protocol/wire-format.md) ·
  [`../protocol/error-codes.md`](../protocol/error-codes.md) — the wire contract
  to keep in sync.
- [`../release-plan-v1.0.md`](../release-plan-v1.0.md) — the full 1.0 stability
  contract and versioning policy.
