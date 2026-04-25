# SP-protocol-schema — Machine-readable `atd-protocol-schema.json`

**Date:** 2026-04-25
**Status:** Approved — ready for implementation plan
**Closes:** [`docs/issues/2026-04-24-schema-protocol-machine-readable-missing.md`](../../issues/2026-04-24-schema-protocol-machine-readable-missing.md)
**Anchor:** architecture §3.3 v1 target-state item 1; §10 evolution row "Machine-readable atd-protocol-schema.json".

## 1. Context

`atd-protocol` defines the wire types but ships no machine-readable
artifact. Third-party SDK authors (TS / Go / Swift / ArkTS) must read
Rust source or markdown to know a valid `ToolDefinition`,
`ToolSummary`, `ToolResult`, or `AtdError`. The Python mirror in
`python/src/atd_client/types.py` is hand-ported and drift-prone for
the same reason.

This SP generates `atd-protocol-schema.json` from the Rust types via
`schemars`, publishes it at the repo root, and gates CI against drift
and JSON Schema 2020-12 meta-schema validity. As a co-shipped fix,
`ToolDefinition` gains an `errors: Vec<ToolErrorDef>` field that the
v3 whitepaper App. A names but the protocol crate has never carried.

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q1 | Which v3 fields to pull forward? | `errors[]` only. `output_schema` is already in `ToolDefinition` (architecture §3.2 just doesn't list it). All other v3-aspirational fields (device, distributed, output_hint, ergonomic_aliases, structured rate_limit object, capability sub-object, fallback) stay out — they belong to other SPs that touch their owning layer. |
| Q2 | `schemars` always-on, feature-gated, or wrapper crate? | B — feature-gated `schema` on `atd-protocol`. Default consumers (SDK, runtime, tools, CLI, mcp-bridge) compile with zero added deps; only `gen-schema` bin activates the feature. |
| Q3 | `ToolErrorDef` shape? | A — minimal: `code: String` (SCREAMING_SNAKE), `description: String`, `retryable: bool`. Extensible additively later. |
| Q4 | Where does `atd-protocol-schema.json` live? | A — repo root (`/atd-protocol-schema.json`). Matches the original issue file's proposal; lets external implementers find it via the standard "look at the repo root" path. |
| Q5 | CI drift + meta-schema validation shape? | A — one bin, two modes: bare invocation overwrites the file; `-- --check` re-generates in memory, byte-diffs against the on-disk version, and validates the result against the JSON Schema 2020-12 meta-schema. CI runs the `--check` form. |

## 3. Touch points

| # | File | Change |
|---|---|---|
| 1 | `crates/atd-protocol/Cargo.toml` | Add `[features] schema`; add `schemars` + `jsonschema` as optional deps; declare `[[bin]] gen-schema` with `required-features = ["schema"]`. |
| 2 | `crates/atd-protocol/src/tool.rs` | New `ToolErrorDef` struct (code/description/retryable). Add `pub errors: Vec<ToolErrorDef>` to `ToolDefinition` with `#[serde(default)]`. |
| 3 | `crates/atd-protocol/src/lib.rs` | `pub use tool::ToolErrorDef;`. |
| 4 | `crates/atd-protocol/src/{enums,error,messages,result,summary,tool}.rs` | Add `#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]` on every pub `struct` / `enum`. No serde / API change. |
| 5 | `crates/atd-protocol/src/bin/gen-schema.rs` | New bin; two modes (write / `--check`). |
| 6 | `crates/atd-protocol/tests/schema_metaschema.rs` | New `#[cfg(feature = "schema")]` test: build root schema, validate against Draft 2020-12 meta-schema. |
| 7 | `crates/atd-protocol/tests/error_def_roundtrip.rs` | New: `ToolErrorDef` JSON roundtrip + `ToolDefinition` with missing `errors` deserializes to empty vec. |
| 8 | All `atd-tools-*/src/*.rs` `definition()` builders + `crates/atd-ref-server-bin/src/conformance.rs` | Add `errors: vec![]` to each `ToolDefinition { ... }`. Mechanical; ~12 sites. |
| 9 | `python/src/atd_client/types.py` | Mirror: new `ToolErrorDef` dataclass; `ToolDefinition.errors: list[ToolErrorDef] = field(default_factory=list)`. |
| 10 | `/atd-protocol-schema.json` | New artifact at repo root, generated. |
| 11 | `.github/workflows/ci.yml` (or equivalent) | Add `cargo run -p atd-protocol --features schema --bin gen-schema -- --check` step alongside fmt/clippy/test/build. |
| 12 | `docs/protocol/wire-format.md` | Top section: link to `/atd-protocol-schema.json` as machine-readable source-of-truth. |
| 13 | `README.md` | New "Schema artifact" subsection: artifact path + local regenerate command. |
| 14 | `docs/architecture.md` | §3.2 flip "Machine-readable protocol schema" `❌`→`✅`; add `output_schema` row (✅ — was missing); add `ToolErrorDef` / `errors[]` row (✅). §3.4 drop the schema gap row. §10 flip the schema row to `✅`. |
| 15 | `docs/issues/2026-04-24-schema-protocol-machine-readable-missing.md` | Append `**Status:** resolved by SP-protocol-schema (commit <SHA>)`. |

Not touched: `atd-sdk`, `atd-runtime`, `atd-mcp-bridge`, `atd-cli`, `atd-conformance`. Their compile graphs do not see the `schema` feature.

## 4. `ToolErrorDef` + `ToolDefinition.errors`

```rust
// crates/atd-protocol/src/tool.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolErrorDef {
    /// SCREAMING_SNAKE error code, e.g. "FILE_NOT_FOUND".
    pub code: String,
    pub description: String,
    pub retryable: bool,
}
```

```rust
// In ToolDefinition (added field):
#[serde(default)]
pub errors: Vec<ToolErrorDef>,
```

**Back-compat:** `#[serde(default)]` lets pre-SP definitions (no `errors` key) deserialize to `vec![]`. Mirrors the pattern used for `required_capabilities`.

**Not in `ToolSummary`:** discover stays lean; only describe surfaces error declarations.

**Built-ins:** every existing tool's `definition()` adds `errors: vec![]`. This SP does not author real per-tool error catalogs — that's a future per-tool SP.

## 5. `Cargo.toml` shape

```toml
[features]
default = []
schema = ["dep:schemars", "dep:jsonschema"]

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
schemars = { version = "0.8", optional = true }
jsonschema = { version = "0.18", optional = true, default-features = false }

[[bin]]
name = "gen-schema"
required-features = ["schema"]
```

## 6. `gen-schema` bin

`crates/atd-protocol/src/bin/gen-schema.rs`:

```rust
use schemars::gen::SchemaSettings;
use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let settings = SchemaSettings::draft2020_12();
    let mut gen = settings.into_generator();

    // Reach every public root type so schemars walks the full graph.
    gen.subschema_for::<atd_protocol::Request>();
    gen.subschema_for::<atd_protocol::Response>();
    gen.subschema_for::<atd_protocol::ToolSummary>();
    gen.subschema_for::<atd_protocol::ToolDefinition>();
    gen.subschema_for::<atd_protocol::ToolResult>();
    // (enums, error, etc. reachable transitively)

    let root = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://atd.dev/schema/v0.1.0/atd-protocol-schema.json",
        "title": "ATD Protocol Schema",
        "description": "Wire types for the ATD reference implementation. Generated from atd-protocol Rust types.",
        "definitions": gen.into_root_schema_for::<()>().definitions,
    });
    let mut text = serde_json::to_string_pretty(&root).unwrap();
    text.push('\n');

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = manifest_dir.join("../../atd-protocol-schema.json");

    if check {
        let on_disk = std::fs::read_to_string(&out).expect("schema file missing");
        let mut ok = true;
        if on_disk != text {
            eprintln!("error: atd-protocol-schema.json is stale; run `cargo run -p atd-protocol --features schema --bin gen-schema` to regenerate");
            ok = false;
        }
        // Meta-schema validation
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let metaschema_url = "https://json-schema.org/draft/2020-12/schema";
        let validator = jsonschema::JSONSchema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .compile(&serde_json::json!({"$ref": metaschema_url}))
            .expect("compile metaschema");
        if let Err(errs) = validator.validate(&value) {
            for e in errs { eprintln!("metaschema: {e}"); }
            ok = false;
        }
        if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::FAILURE }
    } else {
        std::fs::write(&out, &text).expect("write schema");
        std::process::ExitCode::SUCCESS
    }
}
```

(Implementation plan refines: bring meta-schema offline if `jsonschema` insists on network fetch — use the bundled meta-schema definition instead.)

## 7. CI integration

Add one step to the existing workflow alongside fmt/clippy/test/build:

```yaml
- name: Verify atd-protocol-schema.json is fresh
  run: cargo run -p atd-protocol --features schema --bin gen-schema -- --check
```

Local developer flow when types change:

```bash
cargo run -p atd-protocol --features schema --bin gen-schema
git add atd-protocol-schema.json
```

## 8. Tests

| File | Purpose |
|---|---|
| `crates/atd-protocol/tests/schema_metaschema.rs` | Feature-gated. Build the root schema, validate against the Draft 2020-12 meta-schema. Same logic the bin runs in `--check` mode, but as a `cargo test` so it shows up in the workspace test count without needing the bin. |
| `crates/atd-protocol/tests/error_def_roundtrip.rs` | (a) `ToolErrorDef` serialize → deserialize → equality. (b) `ToolDefinition` JSON without `errors` key deserializes to empty vec. (c) `ToolDefinition` with non-empty errors round-trips. |

Existing roundtrip tests in `summary.rs` / `tool.rs` keep passing because `errors` defaults to `vec![]` and contributes nothing to default JSON output.

## 9. Documentation updates

**`docs/architecture.md` §3.2** — three row edits:

| Component | Status change |
|---|---|
| `ToolDefinition.output_schema` | new ✅ row (was unlisted) |
| `ToolDefinition.errors[]` / `ToolErrorDef` | new ✅ row |
| Machine-readable protocol schema | `❌` → `✅`, source = `/atd-protocol-schema.json`, notes = SP-protocol-schema |

**§3.4 Gap → SP table:** drop the "No machine-readable schema" row.

**§10 Evolution path table:** flip the schema row `❌` → `✅`, target SP = SP-protocol-schema, window = 2026-04-25.

**`docs/protocol/wire-format.md`** — top "Source" line: add "machine-readable counterpart: [`/atd-protocol-schema.json`](../../atd-protocol-schema.json)".

**`README.md`** — new short subsection pointing at the artifact + the local regenerate command.

**`docs/issues/2026-04-24-schema-protocol-machine-readable-missing.md`** — append resolution line, keep history intact.

## 10. Out of scope (explicit)

- Pulling forward v3 fields beyond `errors[]` (device, distributed, output_hint, ergonomic_aliases, structured rate_limit object, capability sub-object, fallback). Each belongs to a future SP touching its owning layer.
- Authoring real per-tool error catalogs in built-ins. Built-ins ship `errors: vec![]` in this SP; populating them is a per-tool concern.
- Generating TS/Go/Swift bindings from the schema. The schema makes that possible; the work itself is downstream-consumer territory.
- Changes to `atd-conformance`. Schema artifact is a static file; conformance is wire-level. SP-8.x can decide later whether to add a schema-based fixture.
- Renaming Python mirror to `atd_sdk`. Out of scope per architecture §8.2 (deferred SP).

## 11. Acceptance

- `/atd-protocol-schema.json` exists, is generated, and validates against JSON Schema Draft 2020-12 meta-schema.
- `cargo run -p atd-protocol --features schema --bin gen-schema -- --check` exits 0 on a clean tree, non-zero if the file is stale or invalid.
- All 4 standard checks still pass: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --workspace --all-features`, `cargo build --release --workspace`.
- `ToolDefinition` round-trips with and without `errors` populated.
- Default-feature builds of `atd-sdk`, `atd-runtime`, `atd-tools-*`, `atd-cli`, `atd-mcp-bridge` show no new transitive deps in `cargo tree`.
- architecture §3 + §10 status tables updated.
- Issue file marked resolved.
- Tag `sp-protocol-schema` lands on the closing commit.
