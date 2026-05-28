# SP-protocol-schema Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate a machine-readable `atd-protocol-schema.json` from the `atd-protocol` Rust types, gate it with a CI drift + JSON Schema 2020-12 meta-schema check, and add the missing `errors[]` field on `ToolDefinition` in the same SP.

**Architecture:** Feature-gated `schemars` derives on every public `atd-protocol` type, plus a `gen-schema` bin with two modes (write / `--check`). Default consumers (SDK / runtime / tools / CLI / mcp-bridge) compile unchanged; only the bin opts into the `schema` feature. New `ToolErrorDef` struct + `ToolDefinition.errors: Vec<ToolErrorDef>` lands with `#[serde(default)]` so existing serialized definitions stay valid.

**Tech Stack:** Rust 1.85, `schemars = "0.8"` (feature-gated), `jsonschema = "0.18"` (feature-gated), serde / serde_json (already present), pydantic (Python mirror).

**Spec:** [`docs/superpowers/specs/2026-04-25-protocol-schema-design.md`](../specs/2026-04-25-protocol-schema-design.md)

---

## File map

| File | Role |
|---|---|
| `crates/atd-protocol/Cargo.toml` | Add `[features] schema`, optional deps, `[[bin]] gen-schema`. |
| `crates/atd-protocol/src/tool.rs` | New `ToolErrorDef`; new `errors: Vec<ToolErrorDef>` on `ToolDefinition`. |
| `crates/atd-protocol/src/lib.rs` | Re-export `ToolErrorDef`. |
| `crates/atd-protocol/src/{enums,error,messages,result,summary,tool}.rs` | `#[cfg_attr(feature="schema", derive(JsonSchema))]` on each pub type. |
| `crates/atd-protocol/src/bin/gen-schema.rs` | New bin; modes: write-default, `--check` for CI. |
| `crates/atd-protocol/tests/error_def_roundtrip.rs` | New test file: `ToolErrorDef` JSON + `ToolDefinition.errors` defaults. |
| `crates/atd-protocol/tests/schema_metaschema.rs` | New `#[cfg(feature="schema")]` test: 2020-12 meta-schema validation. |
| `atd-protocol-schema.json` (repo root) | Generated artifact, committed. |
| `crates/atd-tools-{echo,fs,shell,web}/src/*.rs` (10 sites) | Add `errors: vec![]` to each `ToolDefinition { ... }`. |
| `crates/atd-ref-server-bin/src/{server,conformance}.rs` | Same — `errors: vec![]` on every `ToolDefinition` literal. |
| `python/src/atd_client/types.py` | New `ToolErrorDef` pydantic model; `ToolDefinition.errors: list[ToolErrorDef] = []`. |
| `.github/workflows/ci.yml` | New step: `cargo run -p atd-protocol --features schema --bin gen-schema -- --check`. |
| `docs/protocol/wire-format.md` | Top: link to `/atd-protocol-schema.json`. |
| `README.md` | New "Schema artifact" subsection. |
| `docs/atd-architecture.md` | §3.2 / §3.4 / §10 status edits. |
| `docs/issues/2026-04-24-schema-protocol-machine-readable-missing.md` | Append resolution line. |

---

## Task 1: Add `ToolErrorDef` + `errors` field on `ToolDefinition`

**Files:**
- Modify: `crates/atd-protocol/src/tool.rs`
- Modify: `crates/atd-protocol/src/lib.rs`
- Test: `crates/atd-protocol/tests/error_def_roundtrip.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/atd-protocol/tests/error_def_roundtrip.rs`:

```rust
use atd_protocol::{ToolDefinition, ToolErrorDef};

#[test]
fn tool_error_def_roundtrips() {
    let e = ToolErrorDef {
        code: "FILE_NOT_FOUND".into(),
        description: "the path does not exist".into(),
        retryable: false,
    };
    let j = serde_json::to_string(&e).unwrap();
    let back: ToolErrorDef = serde_json::from_str(&j).unwrap();
    assert_eq!(back.code, "FILE_NOT_FOUND");
    assert_eq!(back.description, "the path does not exist");
    assert!(!back.retryable);
}

#[test]
fn tool_definition_without_errors_key_defaults_to_empty() {
    // Pre-SP serialized definitions never carried `errors`; must round-trip.
    let j = r#"{
        "id": "ref:fs.read",
        "name": "Read",
        "description": "d",
        "version": "0.1.0",
        "capability": {"domain":"fs","actions":[],"tags":[],"intent_examples":[]},
        "input_schema": {},
        "output_schema": {},
        "bindings": [],
        "safety": {"level":"Read","dry_run":false,"side_effects":[],"data_sensitivity":null},
        "resources": {"timeout_ms":1000,"max_concurrent":1,"rate_limit_per_min":null,"estimated_tokens":null},
        "trust": {"publisher":"x","trust_level":"L2Tested","signature":null}
    }"#;
    let def: ToolDefinition = serde_json::from_str(j).unwrap();
    assert!(def.errors.is_empty());
}

#[test]
fn tool_definition_errors_roundtrip() {
    let j = r#"{
        "id": "ref:fs.read",
        "name": "Read",
        "description": "d",
        "version": "0.1.0",
        "capability": {"domain":"fs","actions":[],"tags":[],"intent_examples":[]},
        "input_schema": {},
        "output_schema": {},
        "bindings": [],
        "safety": {"level":"Read","dry_run":false,"side_effects":[],"data_sensitivity":null},
        "resources": {"timeout_ms":1000,"max_concurrent":1,"rate_limit_per_min":null,"estimated_tokens":null},
        "trust": {"publisher":"x","trust_level":"L2Tested","signature":null},
        "errors": [{"code":"E1","description":"x","retryable":true}]
    }"#;
    let def: ToolDefinition = serde_json::from_str(j).unwrap();
    assert_eq!(def.errors.len(), 1);
    assert_eq!(def.errors[0].code, "E1");
    assert!(def.errors[0].retryable);
}
```

- [ ] **Step 2: Run test, verify failure**

Run: `cargo test -p atd-protocol --test error_def_roundtrip`
Expected: FAIL — `ToolErrorDef` unresolved, no `errors` field on `ToolDefinition`.

- [ ] **Step 3: Add `ToolErrorDef` + field**

In `crates/atd-protocol/src/tool.rs`, append:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolErrorDef {
    /// SCREAMING_SNAKE error code, e.g. "FILE_NOT_FOUND".
    pub code: String,
    pub description: String,
    pub retryable: bool,
}
```

Inside `ToolDefinition`, after the `tier` field, add:

```rust
    /// Domain errors this tool may emit. Optional; missing on the wire =
    /// empty. Surfaces only via `describe`, never via `discover` (kept off
    /// `ToolSummary`).
    #[serde(default)]
    pub errors: Vec<ToolErrorDef>,
```

In `crates/atd-protocol/src/lib.rs`, extend the `pub use tool::{...}` line to add `ToolErrorDef`:

```rust
pub use tool::{ToolBinding, ToolCapability, ToolDefinition, ToolErrorDef, ToolResources, ToolSafety, ToolTrust};
```

- [ ] **Step 4: Run roundtrip test**

Run: `cargo test -p atd-protocol --test error_def_roundtrip`
Expected: PASS (3 tests).

- [ ] **Step 5: Verify whole workspace still compiles**

Run: `cargo build --workspace`
Expected: FAIL — every `ToolDefinition { ... }` literal in the workspace is now missing `errors`. Note the failing files; fix in Task 2.

- [ ] **Step 6: No commit yet** — Task 2 fixes the build before committing.

---

## Task 2: Add `errors: vec![]` to every `ToolDefinition` literal

**Files (all `Modify`):**
- `crates/atd-tools-echo/src/lib.rs`
- `crates/atd-tools-fs/src/read.rs`
- `crates/atd-tools-fs/src/write.rs`
- `crates/atd-tools-fs/src/edit.rs`
- `crates/atd-tools-fs/src/glob.rs`
- `crates/atd-tools-fs/src/grep.rs`
- `crates/atd-tools-shell/src/exec.rs`
- `crates/atd-tools-shell/src/pwsh.rs`
- `crates/atd-tools-web/src/fetch.rs`
- `crates/atd-ref-server-bin/src/server.rs`
- `crates/atd-ref-server-bin/src/conformance.rs`
- Existing tests inside `crates/atd-protocol/src/{tool,summary}.rs` that build `ToolDefinition`

- [ ] **Step 1: For each file, add `errors: vec![],` to every `ToolDefinition { ... }` literal**

Place the field immediately after `tier: ...` (or after `required_capabilities` if `tier` is absent), before the closing `}`. Example for `crates/atd-tools-echo/src/lib.rs`:

```rust
ToolDefinition {
    id: "ref:echo".into(),
    // ... unchanged ...
    visibility: ToolVisibility::Read,
    required_capabilities: vec![],
    tier: None,
    errors: vec![],
}
```

Repeat for every site. To find them: `rg "ToolDefinition\s*\{" crates/`.

- [ ] **Step 2: Verify whole workspace builds**

Run: `cargo build --workspace`
Expected: PASS.

- [ ] **Step 3: Run all tests**

Run: `cargo test --workspace`
Expected: PASS — existing roundtrips still pass because `errors` defaults to `vec![]` and contributes nothing to JSON.

- [ ] **Step 4: Commit**

```bash
git add crates/atd-protocol/src/tool.rs \
        crates/atd-protocol/src/lib.rs \
        crates/atd-protocol/tests/error_def_roundtrip.rs \
        crates/atd-tools-*/src/*.rs \
        crates/atd-ref-server-bin/src/server.rs \
        crates/atd-ref-server-bin/src/conformance.rs
git commit -m "feat(atd-protocol): add ToolErrorDef and ToolDefinition.errors[]"
```

---

## Task 3: Mirror `errors[]` in Python types

**Files:**
- Modify: `python/src/atd_client/types.py`

- [ ] **Step 1: Add `ToolErrorDef` + field**

In `python/src/atd_client/types.py`, immediately before `class ToolDefinition`, add:

```python
class ToolErrorDef(BaseModel):
    model_config = ConfigDict(extra="ignore")

    code: str
    description: str
    retryable: bool
```

Inside `class ToolDefinition`, after `visibility: ToolVisibility = ToolVisibility.READ`, add:

```python
    errors: list[ToolErrorDef] = Field(default_factory=list)
```

- [ ] **Step 2: Run Python tests**

Run: `cd python && pytest`
Expected: PASS — defaulting field is non-breaking.

- [ ] **Step 3: Commit**

```bash
git add python/src/atd_client/types.py
git commit -m "feat(python): mirror ToolErrorDef and ToolDefinition.errors[]"
```

---

## Task 4: Wire `schema` feature + optional deps

**Files:**
- Modify: `crates/atd-protocol/Cargo.toml`

- [ ] **Step 1: Edit `Cargo.toml`**

Replace the existing `[dependencies]` block with:

```toml
[features]
default = []
schema = ["dep:schemars", "dep:jsonschema"]

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
schemars = { version = "0.8", features = ["preserve_order"], optional = true }
jsonschema = { version = "0.18", default-features = false, optional = true }

[[bin]]
name = "gen-schema"
required-features = ["schema"]
```

- [ ] **Step 2: Verify default build is unchanged**

Run: `cargo build -p atd-protocol`
Expected: PASS — schemars not pulled.

- [ ] **Step 3: Verify schema feature compiles deps**

Run: `cargo build -p atd-protocol --features schema`
Expected: FAIL — `gen-schema` bin source does not exist yet. (deps resolve OK; bin not yet present.) If the failure is "no such bin target," that's expected; we'll create it in Task 6. To confirm deps resolve, run `cargo build -p atd-protocol --features schema --lib` instead, expected: PASS.

- [ ] **Step 4: Verify default-feature consumers untouched**

Run: `cargo tree -p atd-sdk | grep schemars`
Expected: empty output. Same for `atd-runtime`, `atd-cli`, `atd-mcp-bridge`.

- [ ] **Step 5: No commit yet** — bundle with Task 5 since the feature is half-wired without the derives.

---

## Task 5: Add `JsonSchema` derives behind `schema` feature

**Files (all `Modify`):**
- `crates/atd-protocol/src/enums.rs`
- `crates/atd-protocol/src/error.rs`
- `crates/atd-protocol/src/messages.rs`
- `crates/atd-protocol/src/result.rs`
- `crates/atd-protocol/src/summary.rs`
- `crates/atd-protocol/src/tool.rs`

- [ ] **Step 1: Add cfg_attr to every public type**

For each `pub struct` and `pub enum` in the files above, add this attribute immediately after the existing `#[derive(...)]` line:

```rust
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
```

Types to cover (full list — verify each with grep):

- `enums.rs`: `ToolVisibility`, `ToolTier`, `BindingProtocol`, `SafetyLevel`, `TrustLevel`
- `error.rs`: `AtdError`
- `messages.rs`: `Request`, `Response`
- `result.rs`: `ToolResult`, `ToolResultMetadata`
- `summary.rs`: `ToolSummary`
- `tool.rs`: `ToolDefinition`, `ToolCapability`, `ToolBinding`, `ToolSafety`, `ToolResources`, `ToolTrust`, `ToolErrorDef`

Sanity check: `rg "pub (struct|enum)" crates/atd-protocol/src/` should match each item above one-to-one.

- [ ] **Step 2: Verify default build still passes (no schemars)**

Run: `cargo build -p atd-protocol`
Expected: PASS — `cfg_attr` is inert without the feature.

- [ ] **Step 3: Verify schema feature lib compiles**

Run: `cargo build -p atd-protocol --features schema --lib`
Expected: PASS.

If a derive fails on `AtdError` because of the boxed `dyn Error` in `ToolExecutionFailed`, work around by adding `#[schemars(skip)]` on that variant — schemars cannot describe trait objects. Document the gap with a one-line comment:

```rust
#[cfg_attr(feature = "schema", schemars(skip))]
ToolExecutionFailed { ... },
```

- [ ] **Step 4: Run all tests**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/atd-protocol/Cargo.toml crates/atd-protocol/src/
git commit -m "feat(atd-protocol): feature-gated schemars derives on protocol types"
```

---

## Task 6: Implement `gen-schema` bin

**Files:**
- Create: `crates/atd-protocol/src/bin/gen-schema.rs`

- [ ] **Step 1: Write the bin**

```rust
//! Generates `atd-protocol-schema.json` at the repo root.
//!
//! Two modes:
//!   bare         → write the file (developer flow after type changes)
//!   --check      → re-generate in memory; byte-diff against on-disk;
//!                  validate result against JSON Schema Draft 2020-12
//!                  meta-schema; exit non-zero on any failure.

use schemars::gen::SchemaSettings;
use std::path::PathBuf;
use std::process::ExitCode;

fn build_schema_text() -> String {
    let settings = SchemaSettings::draft2020_12().with(|s| {
        s.inline_subschemas = false;
    });
    let mut gen = settings.into_generator();

    // Walk every public root type so transitive references land in `definitions`.
    gen.subschema_for::<atd_protocol::Request>();
    gen.subschema_for::<atd_protocol::Response>();
    gen.subschema_for::<atd_protocol::ToolSummary>();
    gen.subschema_for::<atd_protocol::ToolDefinition>();
    gen.subschema_for::<atd_protocol::ToolResult>();
    gen.subschema_for::<atd_protocol::ToolResultMetadata>();
    gen.subschema_for::<atd_protocol::AtdError>();

    let root_schema = gen.into_root_schema_for::<()>();
    let definitions = serde_json::to_value(&root_schema.definitions).unwrap();

    let root = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://atd.dev/schema/v0.1.0/atd-protocol-schema.json",
        "title": "ATD Protocol Schema",
        "description": "Wire types for the ATD reference implementation. Generated from atd-protocol Rust types via schemars; do not hand-edit.",
        "definitions": definitions,
    });
    let mut text = serde_json::to_string_pretty(&root).unwrap();
    text.push('\n');
    text
}

fn out_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../atd-protocol-schema.json")
}

fn run_check(text: &str) -> bool {
    let mut ok = true;
    let on_disk = match std::fs::read_to_string(out_path()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read atd-protocol-schema.json: {e}");
            return false;
        }
    };
    if on_disk != text {
        eprintln!(
            "error: atd-protocol-schema.json is stale.\n\
             run: cargo run -p atd-protocol --features schema --bin gen-schema"
        );
        ok = false;
    }

    // Meta-schema validation. Use the bundled draft-2020-12 meta-schema URL;
    // jsonschema crate ships it offline.
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    let metaschema = serde_json::json!({
        "$ref": "https://json-schema.org/draft/2020-12/schema"
    });
    match jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&metaschema)
    {
        Ok(validator) => {
            if let Err(errors) = validator.validate(&value) {
                for err in errors {
                    eprintln!("metaschema: {err}");
                }
                ok = false;
            }
        }
        Err(e) => {
            eprintln!("error: cannot compile draft-2020-12 metaschema: {e}");
            ok = false;
        }
    }
    ok
}

fn main() -> ExitCode {
    let check = std::env::args().any(|a| a == "--check");
    let text = build_schema_text();
    if check {
        if run_check(&text) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    } else {
        if let Err(e) = std::fs::write(out_path(), &text) {
            eprintln!("error: write failed: {e}");
            return ExitCode::FAILURE;
        }
        eprintln!("wrote {}", out_path().display());
        ExitCode::SUCCESS
    }
}
```

- [ ] **Step 2: Generate the file for the first time**

Run: `cargo run -p atd-protocol --features schema --bin gen-schema`
Expected: prints `wrote .../atd-protocol-schema.json`. Inspect with `head -20 atd-protocol-schema.json` — should show `$schema`, `$id`, `title`, `definitions`.

- [ ] **Step 3: Run the check mode against the just-generated file**

Run: `cargo run -p atd-protocol --features schema --bin gen-schema -- --check`
Expected: exit 0, no output.

- [ ] **Step 4: Provoke drift, confirm check fails**

```bash
echo "" >> atd-protocol-schema.json
cargo run -p atd-protocol --features schema --bin gen-schema -- --check
```
Expected: prints `error: atd-protocol-schema.json is stale...`, exit non-zero.

Restore: `cargo run -p atd-protocol --features schema --bin gen-schema`

- [ ] **Step 5: Commit**

```bash
git add crates/atd-protocol/src/bin/gen-schema.rs atd-protocol-schema.json
git commit -m "feat(atd-protocol): gen-schema bin + atd-protocol-schema.json artifact"
```

---

## Task 7: Add meta-schema validation as a `cargo test`

**Files:**
- Create: `crates/atd-protocol/tests/schema_metaschema.rs`

- [ ] **Step 1: Write the test**

```rust
//! Runs the same meta-schema validation that `gen-schema -- --check` does,
//! but as a `cargo test` so it reports inside the workspace test count.

#![cfg(feature = "schema")]

use schemars::gen::SchemaSettings;

#[test]
fn generated_schema_validates_against_draft_2020_12_metaschema() {
    let settings = SchemaSettings::draft2020_12();
    let mut gen = settings.into_generator();
    gen.subschema_for::<atd_protocol::Request>();
    gen.subschema_for::<atd_protocol::Response>();
    gen.subschema_for::<atd_protocol::ToolDefinition>();
    gen.subschema_for::<atd_protocol::ToolResult>();
    gen.subschema_for::<atd_protocol::AtdError>();
    let root = gen.into_root_schema_for::<()>();
    let value = serde_json::to_value(&root).unwrap();

    let metaschema = serde_json::json!({
        "$ref": "https://json-schema.org/draft/2020-12/schema"
    });
    let validator = jsonschema::JSONSchema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .compile(&metaschema)
        .expect("compile draft-2020-12 metaschema");
    if let Err(errs) = validator.validate(&value) {
        let msgs: Vec<String> = errs.map(|e| e.to_string()).collect();
        panic!("schema does not validate against draft-2020-12: {msgs:?}");
    }
}
```

- [ ] **Step 2: Run the test under the schema feature**

Run: `cargo test -p atd-protocol --features schema --test schema_metaschema`
Expected: PASS.

- [ ] **Step 3: Confirm default test runs ignore it**

Run: `cargo test -p atd-protocol`
Expected: PASS, with the new test silently excluded (feature off).

- [ ] **Step 4: Commit**

```bash
git add crates/atd-protocol/tests/schema_metaschema.rs
git commit -m "test(atd-protocol): meta-schema validation under schema feature"
```

---

## Task 8: CI — add `gen-schema --check` step

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Read existing workflow**

Run: `cat .github/workflows/ci.yml`. Identify the job that runs `cargo test`.

- [ ] **Step 2: Insert step after `cargo test`**

```yaml
      - name: Verify atd-protocol-schema.json is fresh and valid
        run: cargo run -p atd-protocol --features schema --bin gen-schema -- --check
```

Place it inside the same job, after the existing test step.

- [ ] **Step 3: Run the equivalent locally**

Run: `cargo run -p atd-protocol --features schema --bin gen-schema -- --check`
Expected: exit 0.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: gate atd-protocol-schema.json drift + metaschema validity"
```

---

## Task 9: Documentation updates

**Files (all `Modify`):**
- `docs/protocol/wire-format.md`
- `README.md`
- `docs/atd-architecture.md`
- `docs/issues/2026-04-24-schema-protocol-machine-readable-missing.md`

- [ ] **Step 1: `docs/protocol/wire-format.md`**

Find the top "Source" line. Append:

```markdown
**Machine-readable counterpart:** [`/atd-protocol-schema.json`](../../atd-protocol-schema.json) — generated from the Rust types in `atd-protocol`; CI gates drift.
```

- [ ] **Step 2: `README.md`**

Add a short subsection (place near build/test instructions):

```markdown
### Protocol schema

The wire types are mirrored as a JSON Schema 2020-12 artifact at the repo
root: [`atd-protocol-schema.json`](./atd-protocol-schema.json). Regenerate
after editing types in `crates/atd-protocol/`:

```bash
cargo run -p atd-protocol --features schema --bin gen-schema
```

CI verifies the committed file is fresh and meta-schema-valid via:

```bash
cargo run -p atd-protocol --features schema --bin gen-schema -- --check
```
```

- [ ] **Step 3: `docs/atd-architecture.md` §3.2**

In the §3.2 Current state table:

- Add row above "Python schema mirror":
  | `ToolDefinition.output_schema` | `crates/atd-protocol/src/tool.rs` | ✅ | tool roundtrip tests | Was previously unlisted; surfaced in describe responses. |
- Add row above "Python schema mirror":
  | `ToolErrorDef` / `ToolDefinition.errors[]` | `crates/atd-protocol/src/tool.rs` | ✅ | `tests/error_def_roundtrip.rs` | Added in SP-protocol-schema. Built-ins ship `errors: vec![]`; per-tool catalogs are a future SP. |
- Edit the existing "Machine-readable protocol schema" row: change `❌` → `✅`, source from `—` to `/atd-protocol-schema.json`, notes to `Generated by gen-schema bin (SP-protocol-schema). CI gates drift + metaschema validity.`

- [ ] **Step 4: `docs/atd-architecture.md` §3.4**

Drop the row "No machine-readable schema | Proposed SP: schema generation via `schemars` | Medium ...". Leave the other two rows.

- [ ] **Step 5: `docs/atd-architecture.md` §10**

Find row "Machine-readable `atd-protocol-schema.json`". Change status `❌` → `✅`, target SP `proposed SP` → `SP-protocol-schema`, window `Q2 2026` → `2026-04-25`, gate text → `Landed; gen-schema bin + CI drift check; see SP-protocol-schema.`.

- [ ] **Step 6: `docs/issues/2026-04-24-schema-protocol-machine-readable-missing.md`**

Append at the very top, immediately after the title:

```markdown
**Status:** resolved by SP-protocol-schema (2026-04-25; tag `sp-protocol-schema`).
```

- [ ] **Step 7: Verify formatting / no broken links**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-features -- -D warnings`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add docs/ README.md
git commit -m "docs: SP-protocol-schema — flip schema status to shipped, link artifact"
```

---

## Task 10: Final verification + tag

- [ ] **Step 1: Standard four-check**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```
Expected: all PASS.

- [ ] **Step 2: Schema check**

Run: `cargo run -p atd-protocol --features schema --bin gen-schema -- --check`
Expected: exit 0.

- [ ] **Step 3: Verify default consumers' dep graph unchanged**

```bash
cargo tree -p atd-sdk | grep -E "schemars|jsonschema" || echo "clean"
cargo tree -p atd-runtime | grep -E "schemars|jsonschema" || echo "clean"
cargo tree -p atd-cli | grep -E "schemars|jsonschema" || echo "clean"
cargo tree -p atd-mcp-bridge | grep -E "schemars|jsonschema" || echo "clean"
```
Expected: each prints `clean`.

- [ ] **Step 4: Tag**

```bash
git tag sp-protocol-schema
```

- [ ] **Step 5: Push (only when user asks)**

```bash
git push origin master
git push origin sp-protocol-schema
```
Hold this step until the user explicitly says push.

---

## Acceptance recap

- `/atd-protocol-schema.json` exists and validates against draft-2020-12.
- `cargo run -p atd-protocol --features schema --bin gen-schema -- --check` exits 0 clean, non-zero on stale or invalid.
- `cargo fmt / clippy / test / build --release` all green.
- `ToolDefinition.errors[]` round-trips; built-ins ship `errors: vec![]`.
- `cargo tree` for SDK / runtime / CLI / MCP bridge shows no schemars/jsonschema.
- architecture §3 + §10 reflect shipped state; issue file marked resolved.
- Tag `sp-protocol-schema` lands on the closing commit.
