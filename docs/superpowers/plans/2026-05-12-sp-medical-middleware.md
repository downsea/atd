# SP-medical-middleware Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship two new sibling middleware crates in atd-mvp — `atd-middleware-fhir` (egress FHIR R4 shape + coding-system whitelist validation) and `atd-middleware-pii-redact-medical` (egress PHI redaction over HIPAA Safe Harbor 18 identifiers projected to FHIR R4 paths + 5 catch-all regex rules). Both implement the existing `atd_runtime::Middleware` trait with zero changes to dispatch wiring. Defaults match Celia's hand-curated `crates/celia-core/src/fhir/{systems.rs,validate.rs}` byte-for-byte so Celia's existing 159-test suite passes when the middleware is mounted.

**Adopters:**
- **celia_phr** — primary validation adopter; will migrate per spec §7 (4-step cut-over, ATD ships the primitives; celia owns the migration).
- **healthkit_cli** — passive; its tools don't currently return FHIR, but the middleware is opt-in via `Server::set_middleware`, so no regression risk.

**Architecture:** Two independent crates (`atd-middleware-fhir` + `atd-middleware-pii-redact-medical`), neither depends on the other (spec §4.1). Both consume `atd-runtime::Middleware` + `atd-protocol::ToolDefinition`. Mounted via the existing `Server::set_middleware(Vec<Arc<dyn Middleware>>)` adopter hook (`crates/atd-server/src/server.rs:77`). Composition: `[FhirMiddleware, PiiRedactMiddleware]` — FHIR validates structure first; PII redacts afterwards (spec §5.2 usage example, comment-justified).

**Tech Stack:** Rust 2021 (workspace edition), `atd-runtime` + `atd-protocol` (path deps), `serde_json` (already in workspace). New per-crate deps: `regex = "1"` + `sha2 = "0.10"` for the PII crate; nothing extra for FHIR.

**Spec:** [`../specs/2026-05-11-sp-medical-middleware-design.md`](../specs/2026-05-11-sp-medical-middleware-design.md) — refer to spec §-numbers throughout this plan.

**Sequencing:** FHIR crate first (smaller scope, fewer test fixtures), PII crate second (depends on no atd-mvp internal change, but adopters compose [fhir, pii] so shipping fhir first lets celia migrate step 1 + step 2 of spec §7 immediately). Cross-cutting integration tests + architecture flip + tag last.

---

## Phase A — `atd-middleware-fhir` crate scaffold + types

### Task 1: Create crate skeleton + register in workspace

**Files:**
- Create: `crates/atd-middleware-fhir/Cargo.toml` (per spec §5.1)
- Create: `crates/atd-middleware-fhir/src/lib.rs` (module decls + re-exports stub)
- Create: `crates/atd-middleware-fhir/README.md` (1-paragraph intent + ref to spec)
- Modify: workspace `Cargo.toml` (add to `[workspace].members`)

- [ ] **Step 1: Create directory + Cargo.toml**

```bash
mkdir -p crates/atd-middleware-fhir/src
```

Write Cargo.toml per spec §5.1 verbatim (deps: `atd-runtime`, `atd-protocol`, `serde`, `serde_json`, `thiserror`). Categories/keywords match.

- [ ] **Step 2: Write minimal src/lib.rs**

```rust
//! Egress FHIR R4 validation middleware for atd-runtime.
//!
//! SP-medical-middleware §4.3 + §5.1. Mount via
//! `Server::set_middleware(vec![Arc::new(FhirMiddleware::default())])`.

pub mod config;
pub mod middleware;
pub mod systems;
pub mod types;

pub use config::{FhirMiddlewareConfig, MismatchPolicy};
pub use middleware::FhirMiddleware;
pub use systems::ALLOWED_SYSTEMS_DEFAULT;
pub use types::FhirValidationError;
```

- [ ] **Step 3: Add the crate to workspace members**

In root `Cargo.toml`'s `[workspace] members = [...]` array, add `"crates/atd-middleware-fhir"` (alphabetical insertion).

- [ ] **Step 4: Verify workspace builds**

```bash
cargo build -p atd-middleware-fhir
```

Empty stubs should compile; expect "no main lib" warnings ignored.

- [ ] **Step 5: Commit**

```
feat(atd-middleware-fhir): scaffold new crate per SP-medical-middleware §5.1
```

### Task 2: Port Celia's 70-URI whitelist + drift test

**Files:**
- Create: `crates/atd-middleware-fhir/src/systems.rs` (the 70-entry `ALLOWED_SYSTEMS_DEFAULT: &[&str]`)

- [ ] **Step 1: Locate celia's source-of-truth list**

```bash
sed -n '15,91p' ~/code/pha/celia_phr/crates/celia-core/src/fhir/systems.rs
```

Read the 70 entries.

- [ ] **Step 2: TDD — write drift-guard test first**

In `crates/atd-middleware-fhir/src/systems.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_systems_match_celia_count() {
        // Spec §5.1 + drift guard mirroring celia's `systems.rs:104-110`.
        assert_eq!(
            ALLOWED_SYSTEMS_DEFAULT.len(),
            70,
            "ALLOWED_SYSTEMS_DEFAULT drifted from celia 70-entry baseline; \
             re-sync from ~/code/pha/celia_phr/crates/celia-core/src/fhir/systems.rs:15-91"
        );
    }

    #[test]
    fn no_duplicates() {
        let mut sorted: Vec<&str> = ALLOWED_SYSTEMS_DEFAULT.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ALLOWED_SYSTEMS_DEFAULT.len());
    }

    #[test]
    fn loinc_snomed_rxnorm_present() {
        for sys in [
            "http://loinc.org",
            "http://snomed.info/sct",
            "http://www.nlm.nih.gov/research/umls/rxnorm",
        ] {
            assert!(
                ALLOWED_SYSTEMS_DEFAULT.contains(&sys),
                "{sys} missing from default whitelist"
            );
        }
    }

    #[test]
    fn celia_legacy_uri_absent() {
        assert!(!ALLOWED_SYSTEMS_DEFAULT.contains(&"https://celia.health/fhir/codes"));
    }
}
```

`cargo test -p atd-middleware-fhir --lib systems` → expect compile fail (constant doesn't exist).

- [ ] **Step 3: GREEN — copy the 70 entries**

```rust
pub const ALLOWED_SYSTEMS_DEFAULT: &[&str] = &[
    // ... 70 entries verbatim from celia's systems.rs:15-91
];
```

- [ ] **Step 4: Run tests + commit**

```bash
cargo test -p atd-middleware-fhir --lib systems
```

All 4 tests pass.

```
feat(atd-middleware-fhir): port celia 70-URI coding-system whitelist with drift guard
```

### Task 3: Config + types + middleware skeleton

**Files:**
- Create: `crates/atd-middleware-fhir/src/types.rs` (`FhirValidationError` enum)
- Create: `crates/atd-middleware-fhir/src/config.rs` (`FhirMiddlewareConfig` + `MismatchPolicy`)
- Create: `crates/atd-middleware-fhir/src/middleware.rs` (`FhirMiddleware` skeleton, on_result is no-op stub)

- [ ] **Step 1: Define types per spec §5.1**

types.rs:
```rust
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum FhirValidationError {
    #[error("unknown resourceType: {0}")]
    UnknownResourceType(String),
    #[error("required field missing on {resource_type}: {field}")]
    MissingRequiredField { resource_type: String, field: String },
    #[error("disallowed coding system: {0}")]
    DisallowedCodingSystem(String),
    #[error("FHIR result missing resourceType discriminator")]
    MissingResourceType,
}
```

config.rs:
```rust
use crate::types::FhirValidationError;
use crate::systems::ALLOWED_SYSTEMS_DEFAULT;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FhirMiddlewareConfig {
    pub extra_systems: Vec<String>,
    pub replace_systems: Option<Vec<String>>,
    pub known_resource_types: Vec<String>,
    pub on_mismatch: MismatchPolicy,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum MismatchPolicy {
    AnnotateAndPass,
    ReplaceWithError,
    StripOffending,
}

impl Default for FhirMiddlewareConfig {
    fn default() -> Self {
        Self {
            extra_systems: vec![],
            replace_systems: None,
            known_resource_types: celia_default_types(),
            on_mismatch: MismatchPolicy::AnnotateAndPass,
        }
    }
}

fn celia_default_types() -> Vec<String> {
    // Celia's 12 supported types per crates/celia-core/src/fhir/validate.rs:17-19
    [
        "Patient", "Observation", "Condition", "MedicationStatement",
        "Goal", "CarePlan", "DocumentReference", "AllergyIntolerance",
        "Procedure", "ServiceRequest", "DiagnosticReport", "Encounter",
    ].iter().map(|s| s.to_string()).collect()
}

impl FhirMiddlewareConfig {
    /// Resolve the active allow-list, applying `replace_systems` if Some
    /// else merging defaults with `extra_systems`.
    pub fn effective_systems(&self) -> Vec<String> {
        if let Some(ref r) = self.replace_systems {
            r.clone()
        } else {
            let mut out: Vec<String> = ALLOWED_SYSTEMS_DEFAULT.iter().map(|s| (*s).to_string()).collect();
            out.extend(self.extra_systems.iter().cloned());
            out
        }
    }
}
```

middleware.rs skeleton:
```rust
use std::sync::Arc;
use atd_protocol::ToolDefinition;
use atd_runtime::Middleware;
use serde_json::Value;
use crate::config::{FhirMiddlewareConfig, MismatchPolicy};

#[derive(Debug, Clone)]
pub struct FhirMiddleware {
    config: FhirMiddlewareConfig,
    // Pre-computed sets for hot-path:
    effective_systems: Arc<std::collections::HashSet<String>>,
    known_types: Arc<std::collections::HashSet<String>>,
}

impl FhirMiddleware {
    pub fn new(config: FhirMiddlewareConfig) -> Self {
        let effective_systems = Arc::new(
            config.effective_systems().into_iter().collect()
        );
        let known_types = Arc::new(
            config.known_resource_types.iter().cloned().collect()
        );
        Self { config, effective_systems, known_types }
    }
}

impl Default for FhirMiddleware {
    fn default() -> Self {
        Self::new(FhirMiddlewareConfig::default())
    }
}

impl Middleware for FhirMiddleware {
    fn name(&self) -> &'static str {
        "fhir_egress_validate"
    }
    fn on_result(&self, _tool_id: &str, _tool_def: &ToolDefinition, _result: &mut Value) {
        // TODO Task 4: validation pipeline
    }
}
```

- [ ] **Step 2: cargo build + cargo clippy --workspace -- -D warnings**

```bash
cargo build -p atd-middleware-fhir
cargo clippy -p atd-middleware-fhir -- -D warnings
```

- [ ] **Step 3: Commit**

```
feat(atd-middleware-fhir): config + types + Middleware impl skeleton (no-op on_result)
```

### Task 4: Required-fields table + validation walker

**Files:**
- Modify: `crates/atd-middleware-fhir/src/middleware.rs` (real `on_result` impl + helpers)
- Create: `crates/atd-middleware-fhir/src/required_fields.rs` (the 12-type table)

- [ ] **Step 1: TDD — write the 6 spec-§8.1 tests first**

In `crates/atd-middleware-fhir/src/middleware.rs::tests`:

```rust
// Spec §8.1 — 6 unit cases. Each constructs a synthetic ToolDefinition
// + serde_json::Value, calls on_result, asserts the annotation.

#[test]
fn passes_non_fhir_result_untouched() {
    let mw = FhirMiddleware::default();
    let mut v = json!({"echoed": "hi"});
    let snapshot = v.clone();
    mw.on_result("ref:echo.say", &stub_def(), &mut v);
    assert_eq!(v, snapshot);
}

#[test]
fn rejects_celia_legacy_uri() {
    let mw = FhirMiddleware::default();
    let mut v = json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"coding": [{"system": "https://celia.health/fhir/codes", "code": "x"}]}
    });
    mw.on_result("hms:observation.get", &stub_def(), &mut v);
    let errs = v["_fhir_validation_errors"].as_array().expect("annotated");
    assert!(errs.iter().any(|e| e.as_str().unwrap_or("").contains("celia.health")));
}

#[test]
fn accepts_loinc_snomed_rxnorm() {
    let mw = FhirMiddleware::default();
    for sys in ["http://loinc.org", "http://snomed.info/sct",
                "http://www.nlm.nih.gov/research/umls/rxnorm"] {
        let mut v = json!({
            "resourceType": "Observation", "status": "final",
            "code": {"coding": [{"system": sys, "code": "x"}]}
        });
        mw.on_result("t", &stub_def(), &mut v);
        assert!(v["_fhir_validation_errors"].is_null(), "expected no annotation for {sys}");
    }
}

#[test]
fn missing_required_field_per_type() {
    // Spec §8.1 — Observation without `status`, etc. (12 cases minimum,
    // assert each of the 12 types gets caught when its required field is absent)
    let mw = FhirMiddleware::default();
    for (rt, missing) in REQUIRED_FIELDS_TABLE {  // public for test
        let mut v = json!({"resourceType": rt});
        // intentionally omit `missing`
        mw.on_result("t", &stub_def(), &mut v);
        let errs = v["_fhir_validation_errors"].as_array().expect("annotated");
        assert!(
            errs.iter().any(|e| e.as_str().unwrap_or("").contains(missing)),
            "{rt} should report missing {missing}, got {errs:?}"
        );
    }
}

#[test]
fn policy_replace_with_error() {
    let mut cfg = FhirMiddlewareConfig::default();
    cfg.on_mismatch = MismatchPolicy::ReplaceWithError;
    let mw = FhirMiddleware::new(cfg);
    let mut v = json!({"resourceType": "Observation",
                       "code": {"coding": [{"system": "bad-sys", "code": "x"}]}});
    mw.on_result("t", &stub_def(), &mut v);
    assert_eq!(v["error"], "fhir_validation_failed");
    assert!(v["details"].is_array());
}

#[test]
fn unknown_resource_type_rejected() {
    let mw = FhirMiddleware::default();
    let mut v = json!({"resourceType": "DeviceMetric"});  // not in celia 12
    mw.on_result("t", &stub_def(), &mut v);
    let errs = v["_fhir_validation_errors"].as_array().expect("annotated");
    assert!(errs.iter().any(|e| e.as_str().unwrap_or("").contains("DeviceMetric")));
}
```

`stub_def()` — a minimal `ToolDefinition` helper for tests. Inline or shared with later e2e files.

- [ ] **Step 2: GREEN — implement walker**

required_fields.rs:
```rust
/// Per-resource required-field table from celia's
/// `crates/celia-core/src/fhir/validate.rs:117-166`.
pub const REQUIRED_FIELDS_TABLE: &[(&str, &str)] = &[
    ("Patient", "id"),
    ("Observation", "status"),
    ("Condition", "subject"),
    ("MedicationStatement", "status"),
    ("Goal", "lifecycleStatus"),
    ("CarePlan", "status"),
    ("DocumentReference", "status"),
    ("AllergyIntolerance", "code"),
    ("Procedure", "status"),
    ("ServiceRequest", "intent"),
    ("DiagnosticReport", "status"),
    ("Encounter", "status"),
];
```

In `middleware.rs::on_result`:
1. Skip if `result.get("resourceType").is_none()` (and not an array/Bundle).
2. Validate `resourceType` ∈ `known_types`.
3. Validate required field present per `REQUIRED_FIELDS_TABLE`.
4. Recursive walk for `coding[].system` entries; each must be in `effective_systems`.
5. Per `on_mismatch` policy: `AnnotateAndPass` → push to `result["_fhir_validation_errors"]` Vec; `ReplaceWithError` → rewrite result to `{"error": "fhir_validation_failed", "details": [...]}`; `StripOffending` → null out the offending coding entry.

- [ ] **Step 3: Run all 6 tests + commit**

```bash
cargo test -p atd-middleware-fhir --lib
```

Expected: 4 systems-drift + 6 validation = 10 unit tests green.

```
feat(atd-middleware-fhir): validation pipeline — shape, required fields, coding systems

Implements spec §4.3 + §4.4 validators behind the existing
`Middleware::on_result` trait. Default policy `AnnotateAndPass` appends
`_fhir_validation_errors: [...]` to the result; operators select
`ReplaceWithError` for fail-closed.
```

### Task 5: Integration test against atd-server UDS

**Files:**
- Create: `crates/atd-middleware-fhir/tests/e2e_with_ref_server.rs`

- [ ] **Step 1: Write end-to-end test**

Following `atd-server/tests/e2e_minimal.rs` shape:
1. Define a synthetic FHIR-returning tool (`stub_fhir_observation_tool`) — `Tool::call` returns `json!({"resourceType":"Observation","status":"final","code":{"coding":[{"system":"http://loinc.org","code":"15074-8"}]}})`.
2. Spin up `atd-server::Server` with that tool registered.
3. Call `server.set_middleware(vec![Arc::new(FhirMiddleware::default())])`.
4. Connect via `atd-sdk::AtdClient`, invoke the tool, assert the result JSON has no `_fhir_validation_errors`.
5. Repeat with a tool that returns a `system: "bad-sys"` payload → assert annotation present.

- [ ] **Step 2: Run + commit**

```bash
cargo test -p atd-middleware-fhir --test e2e_with_ref_server
```

```
test(atd-middleware-fhir): e2e via atd-server UDS — annotation reaches client
```

---

## Phase B — `atd-middleware-pii-redact-medical` crate

### Task 6: Scaffold crate + RedactionStrategy + default paths

**Files:**
- Create: `crates/atd-middleware-pii-redact-medical/Cargo.toml`
- Create: `crates/atd-middleware-pii-redact-medical/src/lib.rs`
- Create: `crates/atd-middleware-pii-redact-medical/src/strategy.rs` (`RedactionStrategy`)
- Create: `crates/atd-middleware-pii-redact-medical/src/paths.rs` (`DEFAULT_PHI_PATHS`)
- Create: `crates/atd-middleware-pii-redact-medical/src/config.rs` (`PiiRedactConfig`)
- Create: `crates/atd-middleware-pii-redact-medical/README.md`
- Modify: workspace `Cargo.toml`

- [ ] **Step 1: Cargo.toml per spec §5.2 + workspace member**

Deps: `atd-runtime`, `atd-protocol`, `serde`, `serde_json`, `thiserror`, `regex = "1"`, `sha2 = "0.10"`.

- [ ] **Step 2: Define types + 13-path default table**

strategy.rs:
```rust
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum RedactionStrategy {
    Strip,
    Token(&'static str),
    FirstCharPrefix,
    HashSha256Truncated,
    YearOnly,
    ZipPrefix3,
    LogOnly,
}
```

paths.rs:
```rust
use crate::strategy::RedactionStrategy;
use crate::strategy::RedactionStrategy::*;

/// 13 default JSON Pointers covering all 18 HIPAA Safe Harbor categories
/// (some categories share a path). Spec §4.5 + §4.6.
pub const DEFAULT_PHI_PATHS: &[(&str, RedactionStrategy)] = &[
    ("/name", Token("NAME")),
    ("/identifier", Token("ID")),
    ("/address/*/line", Strip),
    ("/address/*/district", Strip),
    ("/address/*/postalCode", ZipPrefix3),
    ("/birthDate", YearOnly),
    ("/deceasedDateTime", YearOnly),
    ("/telecom", Token("PHONE")),
    ("/contact/*/telecom", Token("PHONE")),
    ("/photo", Strip),
    ("/extension/*", Strip),
    ("/url", Token("URL")),
    ("/identifier/*/value", Token("ID")),
];
```

Note: `&str` patterns supporting `*` wildcard for array indices — implementer designs the walker in Task 7.

config.rs:
```rust
use std::collections::HashMap;
use crate::strategy::RedactionStrategy;

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PiiRedactConfig {
    pub extra_paths: Vec<(String, RedactionStrategy)>,
    pub override_strategies: HashMap<String, RedactionStrategy>,
    pub disable_regex_phi: bool,
    pub fhir_aware: bool,
    pub annotate_findings: bool,
}

impl Default for PiiRedactConfig {
    fn default() -> Self {
        Self {
            extra_paths: vec![],
            override_strategies: HashMap::new(),
            disable_regex_phi: false,
            fhir_aware: true,
            annotate_findings: false,
        }
    }
}
```

lib.rs:
```rust
pub mod config;
pub mod middleware;
pub mod paths;
pub mod redact;
pub mod strategy;

pub use config::PiiRedactConfig;
pub use middleware::PiiRedactMiddleware;
pub use paths::DEFAULT_PHI_PATHS;
pub use redact::redact_value;
pub use strategy::RedactionStrategy;
```

middleware.rs and redact.rs are stubs for Task 7.

- [ ] **Step 3: Compile clean; commit**

```
feat(atd-middleware-pii-redact-medical): scaffold + RedactionStrategy + 13 default paths
```

### Task 7: redact_value walker + regex layer + Middleware impl

**Files:**
- Modify: `crates/atd-middleware-pii-redact-medical/src/redact.rs` — `pub fn redact_value(v: &mut Value, cfg: &PiiRedactConfig) -> Vec<String>` (returns finding-paths for `annotate_findings`)
- Modify: `crates/atd-middleware-pii-redact-medical/src/middleware.rs` — thin Middleware impl calling `redact_value`

- [ ] **Step 1: TDD — write spec §8.1 PII tests first**

```rust
#[test]
fn default_paths_cover_18_hipaa_categories() {
    // Per spec §4.5: 13 paths + 5 regex rules cover A-R of HIPAA Safe Harbor.
    // Assert the path table has at least 13 entries; assert each HIPAA
    // category letter has at least one covering path or regex.
    assert!(DEFAULT_PHI_PATHS.len() >= 13);
    // Coverage assertion: build a {category → covered} map and verify
    // every letter from A-R appears. Categories spelled out in spec §4.5
    // table; helper local to test.
}

#[test]
fn patient_name_tokenized() {
    let mut v = json!({
        "resourceType": "Patient",
        "name": [{"family": "Smith", "given": ["John"]}]
    });
    let cfg = PiiRedactConfig::default();
    redact_value(&mut v, &cfg);
    // Expected: name array replaced with Token("NAME"). Per spec §5.2
    // the Token strategy emits "[REDACTED:NAME]".
    assert_eq!(v["name"], json!("[REDACTED:NAME]"));
}

#[test]
fn birthdate_truncated_to_year() {
    let mut v = json!({"resourceType": "Patient", "birthDate": "1955-03-15"});
    let cfg = PiiRedactConfig::default();
    redact_value(&mut v, &cfg);
    assert_eq!(v["birthDate"], "1955");
}

#[test]
fn ssn_regex_anywhere() {
    let mut v = json!({"resourceType": "Patient",
                       "note": [{"text": "Contact 555-12-3456 for follow-up"}]});
    let cfg = PiiRedactConfig::default();
    redact_value(&mut v, &cfg);
    let text = v["note"][0]["text"].as_str().unwrap();
    assert!(text.contains("[REDACTED:SSN]"));
    assert!(!text.contains("555-12-3456"));
}

#[test]
fn log_only_does_not_mutate() {
    let mut v = json!({"resourceType": "Patient", "name": [{"family": "Smith"}]});
    let mut cfg = PiiRedactConfig::default();
    cfg.override_strategies.insert("/name".into(), RedactionStrategy::LogOnly);
    cfg.annotate_findings = true;
    redact_value(&mut v, &cfg);
    assert_eq!(v["name"], json!([{"family": "Smith"}]));  // unchanged
    assert!(v["_phi_findings"].is_array());
}

#[test]
fn generic_json_mode() {
    let mut v = json!({"user": "alice", "email": "a@b.c"});
    let mut cfg = PiiRedactConfig::default();
    cfg.fhir_aware = false;
    redact_value(&mut v, &cfg);
    assert_eq!(v["user"], "alice");
    // Email gets regex-tokenized
    assert_eq!(v["email"], "[REDACTED:EMAIL]");
}

#[test]
fn zip_prefix_3_truncates() {
    let mut v = json!({"resourceType": "Patient",
                       "address": [{"postalCode": "94303"}]});
    let cfg = PiiRedactConfig::default();
    redact_value(&mut v, &cfg);
    assert_eq!(v["address"][0]["postalCode"], "943");
}

#[test]
fn disable_regex_phi_skips_regex_layer() {
    let mut v = json!({"resourceType": "Patient",
                       "note": [{"text": "SSN 555-12-3456"}]});
    let mut cfg = PiiRedactConfig::default();
    cfg.disable_regex_phi = true;
    redact_value(&mut v, &cfg);
    // Regex layer off → SSN survives
    assert!(v["note"][0]["text"].as_str().unwrap().contains("555-12-3456"));
}
```

- [ ] **Step 2: GREEN — implement walker**

Design:
- `redact_value(v: &mut Value, cfg: &PiiRedactConfig) -> Vec<String>` returns the JSON Pointer paths it touched (for `annotate_findings`).
- JSON Pointer walk: split path on `/`, walk via `serde_json::Value::pointer_mut`. For `*` wildcard segments, iterate array indices.
- Per-strategy application: `Strip` → `Value::Null`; `Token(t)` → `Value::String(format!("[REDACTED:{t}]"))`; `YearOnly` → take first 4 chars; `ZipPrefix3` → first 3 chars; etc.
- Regex layer: 5 compiled regexes (SSN, US license plate, IP, URL, email), applied to every `Value::String` encountered during a generic deep walk. Replace match with `Token("CATEGORY")`.
- `annotate_findings`: if true, push `_phi_findings: [paths...]` to result root.

`redact_value` is public — spec §4.7 commits to it being reusable as a free function for a future audit-side hook.

- [ ] **Step 3: PiiRedactMiddleware delegates to redact_value**

```rust
impl Middleware for PiiRedactMiddleware {
    fn name(&self) -> &'static str { "pii_redact_medical" }
    fn on_result(&self, _tool_id: &str, _tool_def: &ToolDefinition, result: &mut Value) {
        let _findings = crate::redact::redact_value(result, &self.config);
    }
}
```

- [ ] **Step 4: Run all 8 unit tests + commit**

```bash
cargo test -p atd-middleware-pii-redact-medical --lib
```

```
feat(atd-middleware-pii-redact-medical): redact_value walker + Middleware wrapper

Implements spec §4.5 + §4.6: 13 JSON-Pointer paths × 7 RedactionStrategies
+ 5 catch-all regexes (SSN, license plate, IP, URL, email). Generic-JSON
mode (fhir_aware=false) skips FHIR-shape paths and runs regex only.
log_only + annotate_findings provide migration-step-2 observability per
spec §7 step 2.
```

---

## Phase C — Integration tests + cross-cutting

### Task 8: Combined chain e2e + audit invariant test

**Files:**
- Create: `crates/atd-middleware-pii-redact-medical/tests/e2e_combined.rs`
- Create: `crates/atd-middleware-pii-redact-medical/tests/e2e_audit_invariant.rs`

- [ ] **Step 1: e2e_combined — both middleware in chain**

Replicate `e2e_with_ref_server.rs` shape but `set_middleware(vec![Arc::new(FhirMiddleware::default()), Arc::new(PiiRedactMiddleware::default())])`. Tool returns a Celia-shaped Patient with PHI; assert FHIR validation passes (no `_fhir_validation_errors`) AND name is `[REDACTED:NAME]`.

- [ ] **Step 2: e2e_audit_invariant — regression guard for AuditSink**

Install a custom `AuditSink` that fails the test if `serde_json::to_string(&event)` contains any string from a forbidden-list (`["John Smith", "555-12-3456"]`). Confirm no PHI leaks via `CallEvent` (spec §4.7 contract — the audit field schema has no result body today; the test guards against future drift).

- [ ] **Step 3: Run + commit**

```
test(atd-middleware-pii-redact-medical): e2e combined chain + AuditSink invariant guard
```

### Task 9: atd-ref-server cross-tool chain test

**Files:**
- Create: `crates/atd-ref-server/tests/e2e_medical_middleware_chain.rs`

- [ ] **Step 1: Mount chain on ref-server, assert non-medical tools unaffected**

Synthetic registry: 1 FHIR Observation tool + 1 ref:echo tool. Install both middleware in chain. Call ref:echo with `{"hi": "x"}` — assert response is bit-identical to no-middleware case (no `_fhir_validation_errors`, no `_phi_findings`, no Token replacements). Call FHIR tool — assert PHI redacted.

- [ ] **Step 2: Run + verify no regression on existing ref-server tests**

```bash
cargo test -p atd-ref-server
```

The existing 24 ref-server tests + the new chain test pass.

- [ ] **Step 3: Commit**

```
test(atd-ref-server): medical-middleware chain — non-medical tools unaffected
```

---

## Phase D — Doc + tag

### Task 10: architecture.md flip + adopter issues + tag

**Files:**
- Modify: `docs/architecture.md` (§10 evolution path row 📜 → ✅; potentially §6 component map adds the 2 new crates)
- Modify: `CLAUDE.md` (crate map 14 → 16; mention sp-medical-middleware tag)
- Create: `~/code/pha/celia_phr/docs/sp-medical-middleware-adopter.md` (4-step migration tracker following SP-cap-v2-adopter.md pattern)
- Create: `~/code/healthkit_cli/docs/sp-medical-middleware-no-regression.md` (no-op for healthkit; middleware is opt-in)

- [ ] **Step 1: Update architecture.md §10**

Find the row `| Medical middleware suite (FHIR validation + PHI redaction) | Dispatch (middleware) | 📜 | ...`; flip to ✅ with full body — cite both crates, the test counts, the spec, the plan.

- [ ] **Step 2: Add §6 component-map rows for the 2 crates** (if §6 has a component table)

- [ ] **Step 3: CLAUDE.md crate map**

Bump 14 → 16. Add 2 lines under crate list:
```
- `atd-middleware-fhir` — FHIR R4 egress validation middleware (SP-medical-middleware)
- `atd-middleware-pii-redact-medical` — HIPAA PHI redaction middleware (SP-medical-middleware)
```

- [ ] **Step 4: Adopter issue files**

`celia_phr/docs/sp-medical-middleware-adopter.md` — 4 acceptance criteria per spec §7.2 + checklist. Follow the SP-cap-v2-adopter.md format (status, criteria, phases, references).

`healthkit_cli/docs/sp-medical-middleware-no-regression.md` — opt-in middleware; healthkit tools don't return FHIR today; confirm no behavioural change after dep bump. 2 ACs (build green; no new test required).

- [ ] **Step 5: Final workspace gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
CARGO_BUILD_JOBS=4 cargo test --workspace -- --test-threads=4
```

All three clean.

- [ ] **Step 6: Tag + push**

```bash
git tag -a sp-medical-middleware -m "..."
git push origin master --tags
```

- [ ] **Step 7: Commit (combined doc + adopter issues)**

```
docs(architecture,adr,CLAUDE): SP-medical-middleware shipped — 📜 → ✅
```

---

## Cross-project (celia + healthkit) — tracked as adopter issues, not in this plan

Per ADR-0001 + the SP-cap-v2 precedent: SP-medical-middleware implementation lands in atd-mvp only. Adopter work (celia 4-step migration per spec §7, healthkit no-regression validation) is filed as separate issues against each downstream repo at SP-completion time. See Task 10 Step 4 files.

---

## Risk register

| Risk | Mitigation |
|---|---|
| `regex` crate version pinning collides with healthkit's already-installed regex usage | Pin `"1"` (loose) and let cargo resolve; if conflict, add a `[patch.crates-io]` block |
| HIPAA Safe Harbor §164.514(b)(2)(i)(C) edge cases (rare ZIPs with population < 20k must be further generalized to first 3 digits) | `ZipPrefix3` strategy already truncates to 3; population-aware filtering is spec §9 out-of-scope |
| JSON Pointer wildcard `*` semantic differs from RFC 6901 | Document the extension in `paths.rs` doc-comment; reference v3 whitepaper §K.2.1's JSONPath superset as future evolution |
| Coding-system whitelist drift between atd-middleware-fhir and celia | Drift guard test (Task 2 Step 2) flags any divergence in `len()`; re-sync procedure documented |
| PHI regex false positives (e.g., legit serial numbers matching SSN regex) | `disable_regex_phi: bool` config opt-out (Task 6 Step 2); document the trade-off in README |
| `redact_value` mutating large FHIR Bundles (10 MB NDJSON exports) takes >500 ms | Spec §4.3 acceptable per-call cost target (~200 ms / 10 MB); benchmark added later if real adopter pull |
| Celia's existing `validate_resource` and the new middleware double-validate → divergence over time | Mitigated by Task 2 drift guard + Task 9 cross-tool test asserting both layers see identical fixtures |

---

## Out of scope (this plan; future SPs)

Per spec §9 — NLP-based PHI detection (Presidio etc.), DICOM image stripping, region-specific code systems out-of-box, compliance certifications, schema-deep FHIR validation, `data_sensitivity: "phi"` per-tool opt-in wiring, audit-side middleware hook, PII-as-symmetric-encryption.
