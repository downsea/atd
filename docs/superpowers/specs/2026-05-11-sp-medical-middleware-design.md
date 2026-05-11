# SP-medical-middleware: ATD vertical middleware for healthcare

| Status | Draft |
| Created | 2026-05-11 |
| Author | cross-project subagent (celia_phr ↔ atd-mvp coordination) |
| Phase | ATD post-v0.3.0; depends on `atd_runtime::Middleware` infra (SP-12) |
| Related | SP-streamable-http (sibling, `2026-05-11-sp-streamable-http-design.md`); SP-12 canonical dispatch (`2026-04-25-sp12-canonical-dispatch.md`); ATD v3 whitepaper §2.7 + Appendix K (result middleware roadmap); Celia patent §13.1 (device-local volatile-key invariant) |

---

## 1. Motivation

**1.1 Healthcare is a regulated vertical that ATD has not met yet.** ATD v0.3.0 ships one generic result middleware (`RedactPathsMiddleware`, `crates/atd-runtime/src/middleware.rs:46-95`) and an explicit roadmap (v3 whitepaper §2.7 — *"Tool Result → [PII Redact] → [Injection Scan] → [Trim] → Agent Context"*) for a five-middleware suite. The redact-paths impl is a "low-effort demonstration of the pattern, not a comprehensive PII scrubber" (own comment, `middleware.rs:44-45`). Health data is the first concrete domain where the gap between "demonstration" and "comprehensive" has compliance teeth: HIPAA §164.514(b)(2) enumerates 18 PHI identifiers that audit trails and exports must strip; FHIR R4 mandates coded concepts from registered terminologies (LOINC, SNOMED CT, RxNorm, ICD-10, UCUM). A tool that *claims* to return FHIR but emits malformed JSON or an unregistered code system is silently wrong — exactly the failure mode middleware was designed to catch.

**1.2 ATD's first medical adopter (Celia) has already paid for both pieces — in the wrong layer.** Celia's `crates/celia-core/src/fhir/systems.rs:15-91` ships a hand-curated 70-URI coding whitelist; `crates/celia-core/src/fhir/validate.rs:43-67` enforces structural + coding-system rules on every write. Celia's `audit_log` table stores only `action / resource_type / resource_id / patient_id` (`crates/celia-core/src/audit/mod.rs:26-47`), self-redacting at the DB layer because the dispatcher never trusts the audit sink to hold PHI. Every future medical ATD adopter — a hospital HIS gateway, a private-PHR vendor, a wearable-device proxy — will reinvent both pieces. This SP relocates them to ATD-side reusable middleware so adopters compose `atd_runtime::Registry` + `atd-middleware-fhir` + `atd-middleware-pii-redact-medical` and get a compliant medical surface for free.

**1.3 Celia is the first adopter; the design must preserve §13.1.** Celia's patent claim §13.1 (device-local volatile-key invariant) requires the DEK live only in `KeyCache: Map<user_id, Arc<Zeroizing<Vec<u8>>>>`, lost on process restart. Nothing in this SP touches the DEK — both middleware operate on *already-decrypted* `serde_json::Value` flowing through `Middleware::on_result` (`middleware.rs:19`). The migration path (§7) cuts over Celia's self-redact in three steps with §13.1 verifiable at each.

## 2. Goals

- Two independently-installable crates under `crates/`: `atd-middleware-fhir` (egress FHIR R4 shape + coding-system validation) and `atd-middleware-pii-redact-medical` (egress PHI redaction).
- Both implement the existing `atd_runtime::Middleware` trait (`middleware.rs:16-20`) — zero changes to dispatch wiring (`crates/atd-server/src/connection.rs:284-289`).
- Default coding-system whitelist matches Celia's `ALLOWED_CODE_SYSTEMS` byte-for-byte (`systems.rs:15-91`) so Celia's existing test fixtures pass when the middleware is wrapped around `dispatch_for_caller`.
- Default PHI field list covers HIPAA Safe Harbor §164.514(b)(2) 18 identifiers projected to FHIR R4 element paths.
- Operator-configurable: extra coding systems (e.g., Chinese national ICD-10-CM), extra field paths, redaction strategy per-field (strip / hash / token).
- The PII crate has both a *generic-JSON* mode (works on any tool's result) and an opt-in *FHIR-aware* mode (when the result contains `resourceType`, traverse FHIR-shaped element paths) — no hard dep on `atd-middleware-fhir`.
- Audit-trail compatibility: `CallEvent` (`crates/atd-runtime/src/audit.rs:25-44`) already excludes result bodies, so PHI never reaches `AuditSink::on_call` today. Middleware preserves this invariant; this SP introduces no audit-side bypass.
- Test parity: Celia's 159 cargo tests + the `test:dek` gcore check pass with the middleware enabled.
- All public types are `#[non_exhaustive]` so future fields are additive (`crates/atd-runtime/src/registry.rs:45-46` precedent).

## 3. Non-goals

- **Full FHIR R4 schema validation.** ~150 resource types, ~5000 fields — schema-data ingest is its own project. We validate *structure* (presence of `resourceType` + non-empty required fields per Celia's 12 supported types) and *codings* (system whitelist); not cardinality, slicing, or invariant rules.
- **NLP-based PII detection in free-text fields.** `DocumentReference.content` may contain PHI in narrative form; detecting it needs a model. Out of scope; the v3 whitepaper §2.7 reserves this for a future external plugin (`detector_ref:` mechanism, `atd-v3-multi-device.md:3349`).
- **DICOM / image metadata stripping.** v3 whitepaper §K.2.5 (`image_strip_metadata`) is a separate middleware; this SP only ships text/JSON.
- **Region-specific code systems out of the box.** No Chinese ICD-10-CM国家版, no JLAC (Japan), no Read v3 (UK). Operator config can add them; defaults are international standards.
- **Compliance certification.** HIPAA SOC-2 / ISO 27001 attestation is for deployers; we ship technical primitives, not audit reports.
- **Inbound (tool-arg) validation.** v3 whitepaper §K and SP-12 both keep middleware on the *result* axis. Tool authors enforce input shape via `input_schema` (`crates/atd-protocol/src/tool.rs:14`) + their own runtime checks.
- **Bypass switches per-tool.** Middleware runs for every successful call (`connection.rs:284-289`); per-tool opt-out belongs to a future "policy" SP, not here.

## 4. Design

This is ~45% of the SP. Each subsection is one of the 8 decision points; each gives the chosen answer, evidence from existing source, and the rejected alternatives.

### 4.1 Crate split — two independent crates, not one bundled `atd-middleware-medical`

**Decision.** Ship two sibling crates: `crates/atd-middleware-fhir` and `crates/atd-middleware-pii-redact-medical`. Both depend on `atd-runtime` (for `Middleware` trait, `ToolDefinition`) and `serde_json`. Neither depends on the other; the PII crate detects FHIR shape by looking for `resourceType` and a JSON-pointer table, not by importing the FHIR crate.

**Evidence + why.** SP-listener-extract (`docs/superpowers/specs/2026-04-25-sp-listener-extract-design.md:23-24`) established the precedent: "Future bindings have different transport shape; runtime must stay transport-agnostic so all transports can compose." The same holds for middleware. A hospital HIS gateway adopter may want PHI redaction but speak HL7 v2 (not FHIR) — they should not pay the FHIR-crate weight. A wearable device proxy may emit pure FHIR `Observation` with no PHI (Patient is implicit) — they should not pay the PII-detector weight. Splitting keeps dep graphs honest, mirrors SP-streamable-http's "atd-server-http sibling of atd-server" pattern (`2026-05-11-sp-streamable-http-design.md:45-49`).

**Why not one crate.** Tempting (single `cargo add atd-middleware-medical` for Celia), but premature unification. The two middleware run on *different signals*: `atd-middleware-fhir` reads `resourceType` + `coding[].system`; `atd-middleware-pii-redact-medical` reads field-path-based JSON pointers. Conflating them inside one trait impl means the PII path has to import + parse FHIR codings even when only generic-PHI-stripping is requested. Operators get clearer install intent with two crates.

**Trade-off table:**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| One crate `atd-middleware-medical` | Single dep; obvious bundle for Celia | FHIR-only or PII-only adopters pay full weight; harder to evolve independently | rejected |
| Two crates, no shared internals | Honest deps; either alone is useful | Two `Cargo.toml`s; slight code duplication in `walk_strings`-style helpers | **chosen** |
| Two crates + a shared `atd-middleware-medical-core` for helpers | DRY | Three `Cargo.toml`s for ~50 LoC of shared walk logic — overhead beats payoff | deferred |

### 4.2 Middleware mount point — both crates use the existing post-dispatch `on_result` hook

**Decision.** Both middleware implement `atd_runtime::Middleware::on_result(&self, tool_id, tool_def, result: &mut Value)` (`crates/atd-runtime/src/middleware.rs:19`). They run on success only (`connection.rs:283-289`) — error paths bypass per SP-12 §8 Q4 (`middleware.rs:9-10`). No new hook is needed; no audit-side hook is needed (justified in §4.7 below).

**Why post-dispatch, not pre-dispatch.** Three reasons:
1. **Pre-dispatch is the tool author's job.** `input_schema` (`crates/atd-protocol/src/tool.rs:14`) declares the input contract; SP-12 validates this at the tool boundary, not the middleware boundary. Forcing FHIR shape on *inputs* would prevent a tool like `create_health_record` from accepting partial drafts (Celia explicitly allows this — `crates/celia-core/src/fhir/validate.rs:128-131` says "the agent may compose partial drafts"). Egress is the right axis because that's when claims must be honored.
2. **Audit-side mount point is unnecessary today.** `CallEvent` (`crates/atd-runtime/src/audit.rs:25-44`) carries only `tool_id`, `outcome`, `granted_capabilities`, `tier`, `duration_ms`, `secrets_resolved`. There is **no `args` field and no `result_preview` field** — PHI never reaches the audit sink today. Adding an audit-side mount point would create a new attack surface (where would the preview come from?) and is out of scope for this SP. The v3 whitepaper §K.5 audit log (`atd-v3-multi-device.md:3420-3433`) records `input_hash` / `output_hash` (sha256), not bodies — also no PHI flows.
3. **Post-dispatch is where the v3 spec lands.** Whitepaper §K.4 pipeline shape (`atd-v3-multi-device.md:3405-3416`): each `mw.apply(tool_id, current, config)` operates on `current: ToolResult`. The trait we have already matches.

**Implication for `atd-middleware-pii-redact-medical`.** Since `CallEvent` does not carry the result, "make audit log show redacted result" is a non-problem under the current design. What this middleware actually does: **rewrites the result that the LLM sees** before it returns over the wire. That is still load-bearing for HIPAA: when the LLM is a third-party service (Anthropic API, OpenAI, etc.), exporting unredacted PHI to it triggers HIPAA §164.502 disclosure rules. Egress-side stripping is the right line of defense.

**Why not invent a new `Middleware::on_audit` hook.** The current `AuditSink` trait (`crates/atd-runtime/src/audit.rs:60-62`) takes `&CallEvent` — no body to redact. If a future SP enriches `CallEvent` with a `result_preview` field, *that* SP can add an audit-side hook. We must not pre-build infrastructure for data that doesn't flow.

### 4.3 FHIR schema validation depth — light, with the 12-resource Celia subset hand-coded

**Decision.** `atd-middleware-fhir` validates:
1. **Object has `resourceType: String`** if any FHIR-shape claim is being made (configurable trigger — see §4.4).
2. **`resourceType` is in a known set** (default = Celia's 12: `Patient`, `Observation`, `Condition`, `MedicationStatement`, `Goal`, `CarePlan`, `DocumentReference`, `AllergyIntolerance`, `Procedure`, `ServiceRequest`, `DiagnosticReport`, `Encounter` — see `crates/celia-core/src/fhir/validate.rs:17-19`).
3. **Each known type has its required fields present and non-null** (table-driven from `validate.rs:117-166`).
4. **Every `coding[].system` is in the whitelist** (recursive walk, deny on miss — copy of `validate.rs:85-113`).

We do **not** validate cardinality, slicing, profile invariants, FHIRPath constraints, or any field beyond presence. Numeric ranges (`valueQuantity.value`), reference target types (`subject.reference must match Patient/<id>`), date formats — all skipped. Operators who need stronger validation use HL7 official validator out-of-band.

**Performance estimate.** Recursive JSON walk over a typical Celia `get_health_record` result (1-50 KB FHIR JSON) costs O(n) string compares against the 70-entry whitelist (linear search; hash-set lookup if proven needed). Worst-case Celia `export_all_records` returns NDJSON of all patient resources — say 10 MB. At ~50 MB/s for `serde_json::Value::walk` + hash lookups, ~200 ms per call. Acceptable: this matches the existing `RedactPathsMiddleware` regex walk cost.

**Why not full schema.** FHIR R4 has 145 resource types and ~5000 properties; bundling the schema is 8-12 MB of JSON or ~20K LoC of hand-coded checks. The v3 whitepaper does not commit to schema bundling (`atd-v3-multi-device.md:3329-3343` — `pii_redact` config is pattern-based, not schema-based). Celia's `validate.rs` is the *exact precedent*: 178 lines, 12 resource types, coding whitelist + required fields only. We replicate that scope, no more.

**Why a static known-set instead of "anything claimed by `resourceType`".** A claim like `"resourceType": "DeviceMetric"` (a real FHIR R4 type Celia does not currently support) would pass an "anything goes" middleware while breaking adopters whose tools assume Celia-subset. The known-set is the contract; operators extending it config-add explicitly.

**Trade-off table:**

| Option | LoC | Coverage | Verdict |
|---|---|---|---|
| Full R4 schema bundle | 20K + 8 MB data | 100% structural | rejected (cost > benefit) |
| Celia-subset hand-coded | ~250 | 12 types, required-only | **chosen** (matches precedent) |
| Pattern-only (no required-field check) | ~80 | Codings only | weakened defense; rejected |

### 4.4 Coding-system whitelist configurability — hard-coded default, operator append + replace

**Decision.** `atd-middleware-fhir` ships with a default `ALLOWED_SYSTEMS: &[&str]` mirroring Celia's `crates/celia-core/src/fhir/systems.rs:15-91` (70 entries, length-asserted in a drift test). The operator-facing config provides:
- `extra_systems: Vec<String>` — appended to the default (additive; for region-specific systems like Chinese ICD-10-CM国家版).
- `replace_systems: Option<Vec<String>>` — if `Some`, fully replaces the default (for highly-curated environments that want a strict subset).

Hard-coding the default keeps the *default trust set* legible — every adopter sees the same 70 URIs unless they explicitly override. Celia's same-defaults-as-now guarantees zero-test-churn during the migration (§7 Step 1).

**Why a default at all** (rather than empty + operator-must-fill). The single best lesson from `RedactPathsMiddleware::with_home_default()` (`middleware.rs:58-71`): give adopters a sensible default that just works for the 90% case. Celia's whitelist is the most-tested healthcare coding whitelist in the ATD ecosystem (159 cargo tests including `count_matches_ts`, `loinc_allowed`, `celia_legacy_rejected`, `unknown_systems_rejected`, `no_duplicates` per `systems.rs:104-138`). Anchoring the default to it ships proven behavior.

**Why not a startup-time config-file load** (e.g., `--coding-whitelist /etc/atd/codings.txt`). Possible operator escape hatch but adds I/O and parsing surface. We expose the same flexibility via `MwConfig::replace_systems` from Rust code; CLI integrations can wrap it. Out-of-band file loading is for a future SP if operator pull emerges.

**Why not a protocol-layer negotiable list** (e.g., client declares accepted systems during `Hello`). Wrong layer — the middleware enforces server-side correctness; clients don't get to widen the trust set. SP-12 `Hello` carries `requested_capabilities` (`crates/atd-protocol/src/messages.rs:34-52`), not server policy.

**Drift detection.** Like Celia's TS→Rust drift guard (`systems.rs:104-110`), this crate asserts `DEFAULT.len() == 70` in `cfg(test)`. When Celia adds a system, both `systems.rs` and `atd-middleware-fhir/src/default.rs` must update in lockstep; the test fails loud.

### 4.5 PII field default list — 18 HIPAA Safe Harbor identifiers projected to FHIR R4 paths

**Decision.** `atd-middleware-pii-redact-medical` ships with a default `DEFAULT_PHI_PATHS: &[&str]` covering all 18 HIPAA Safe Harbor identifiers per §164.514(b)(2)(i)(A-R). Each entry is a JSON Pointer (RFC 6901) walked into every nested object in the result. Below the path is the HIPAA identifier it covers:

| Default JSON Pointer | HIPAA §164.514(b)(2) identifier | FHIR locus |
|---|---|---|
| `/name` (whole array) | A — Names | `Patient.name`, `Practitioner.name`, `RelatedPerson.name` |
| `/identifier` (whole array) | B/I/J — MRN, SSN, account no., cert/license | `Patient.identifier`, `Encounter.identifier` |
| `/address` (whole array, but **city/state preserved** when `/address/0/postalCode` length ≤ 3 digits) | C — Geographic <state level | `Patient.address`, `Organization.address` |
| `/birthDate` (truncated to year `YYYY` only) | D — Dates of birth (year retained per §164.514(b)(2)(i)(C) safe harbor) | `Patient.birthDate` |
| `/deceasedDateTime` (truncated to year) | D — Dates of death | `Patient.deceasedDateTime` |
| `/telecom` (whole array — phone, fax, email) | E/F — Phone, fax | `Patient.telecom` |
| `/contact/*/telecom` | E/F (contacts) | `Patient.contact[].telecom` |
| `/photo` (whole array → null) | R — Full-face photos | `Patient.photo` |
| Any string matching `/^\d{3}-?\d{2}-?\d{4}$/` (SSN regex) | B — SSN | catch-all |
| Any string matching `/^[A-Z]{2}\d{6,10}$/` (US license plate) | M — Vehicle IDs | catch-all |
| `/extension/*` where `url` contains `device-id` or `IMEI` | N — Device identifiers/serial | catch-all |
| `/url` (extension URLs containing `biometric`) | P — Biometric IDs | catch-all |
| `/identifier/*/value` (URLs, IP literals) | O/Q — Web URLs / IP | catch-all |

13 default paths covering all 18 HIPAA categories (some categories share a path — e.g., HIPAA E and F both live in `telecom`). Coverage map is asserted in a unit test. The 5 "any-string regex" rules catch identifiers that don't bind to a stable FHIR path (SSN can appear in `note.text`, `identifier.value`, etc.).

**Why JSON Pointer, not field-name match.** The FHIR shape is recursive — `Patient.name[].family` differs from `Practitioner.qualification[].issuer.display`. JSON Pointer makes the path explicit; field-name match (`"name"`) would over-trigger. The v3 whitepaper §K.2.1 `pii_redact` config (`atd-v3-multi-device.md:3332-3337`) calls for JSONPath, which subsumes JSON Pointer — we choose Pointer because RFC 6901 is normative and `serde_json` has built-in support; JSONPath has multiple competing grammars.

**Why retain `address.city` and `birthDate.year`.** HIPAA Safe Harbor §164.514(b)(2)(i)(B) and (C) explicitly permit geographic information ≥ first 3 ZIP digits and year-of-birth. Retaining them preserves enough demographic shape that the LLM's reasoning about a record ("this elderly patient in California") isn't destroyed. Full strip would meet de-identification but degrade clinical reasoning beyond what HIPAA requires.

**Why include FHIR-shape paths AND catch-all regex.** Defense in depth: a malformed tool that puts SSN in `Patient.note.text` (wrong place for SSN per FHIR) still gets caught by the regex layer. Operators can disable regex via `MwConfig::disable_regex_phi: bool`.

### 4.6 PII redaction strategy — per-field policy, default `strip`, operator-configurable

**Decision.** Each path entry in `DEFAULT_PHI_PATHS` is a `(Pointer, Strategy)` pair. Default strategies:

```rust
pub enum RedactionStrategy {
    /// Replace value with JSON null. Used for `photo`, `signature`.
    Strip,
    /// Replace with the literal string `"[REDACTED:<category>]"` where
    /// category is one of "NAME", "ID", "ADDR", "DOB", "PHONE", "EMAIL",
    /// "DEVICE", "PHOTO", "URL", "IP", "BIOMETRIC".
    Token(&'static str),
    /// Replace with `<first-char>...` for strings, preserving length≤8.
    /// Useful for diagnostic preview without exposing full value.
    FirstCharPrefix,
    /// SHA-256 hex of the original (16-byte truncation). Lets downstream
    /// agents correlate "same patient across calls" without seeing PHI.
    HashSha256Truncated,
    /// Special: truncate ISO-8601 date to year only.
    YearOnly,
    /// Special: keep first 3 chars of ZIP, drop the rest.
    ZipPrefix3,
}
```

Default mapping: `name → Token("NAME")`, `identifier → Token("ID")`, `birthDate → YearOnly`, `address → ZipPrefix3 (for postalCode), Strip (for line, district)`, `telecom → Token("PHONE")` etc., `photo → Strip`, SSN regex match → `Token("SSN")`.

**Why per-field strategy.** A single global mode (whitepaper §K.2.1: `warn | transform | block`) is too coarse. LLM downstream reasoning needs `birthDate.year` for age-bracket queries; the same LLM has no need to know patient surname. The v3 whitepaper anticipated this — `pii_redact` field list is itself per-pattern (`atd-v3-multi-device.md:3334-3335`), suggesting per-path policy was always the right shape.

**Why `Token("NAME")` instead of full strip.** Tokens preserve cardinality (LLM still sees "Patient has one name") without leaking the content. Cleaner than `null` for downstream JSON consumers that branch on field presence. Matches the v3 whitepaper `transform` mode prescription (`atd-v3-multi-device.md:3341`).

**Why offer `HashSha256Truncated`.** Cross-call correlation for agents — they can recognize "this is the same patient I saw earlier" without ever knowing identity. Hospital HIS adopters specifically need this for longitudinal reasoning. Optional; not in the default to avoid linkability concerns.

**Operator config:**
```rust
pub struct PiiRedactConfig {
    pub extra_paths: Vec<(String, RedactionStrategy)>,  // append
    pub override_strategies: HashMap<String, RedactionStrategy>,  // path → new strategy
    pub disable_regex_phi: bool,  // turn off catch-all regex layer
    pub fhir_aware: bool,  // false = generic JSON; true = walk FHIR contact[]/extension[]/etc.
}
```

**Why not a single boolean `redact_pii`.** Wrong granularity; v3 whitepaper learned this lesson (§K.2.1 supports per-field). A toggle gives operators no path through the false-positive / false-negative trade-off.

### 4.7 Integration with `AuditSink` — no new trait method; `CallEvent` invariant preserved

**Decision.** **No changes** to `atd_runtime::AuditSink` (`crates/atd-runtime/src/audit.rs:60-62`). **No changes** to `CallEvent` (`audit.rs:25-44`). The PII middleware operates on the result that flows back to the wire (post-`on_result` in `connection.rs:283-289`); the audit sink receives `CallEvent`, which by construction has never contained the result body.

**Evidence.** Reading `audit.rs:25-44` line by line: fields are `ts`, `call_id`, `tool_id`, `caller_id`, `granted_capabilities`, `duration_ms`, `outcome`, `tier`, `dry_run`, `schema_version`, `secrets_resolved`. The outcome variant (`audit.rs:47-56`) has `ExecutionFailed { code, retryable }`, `InvalidArgs { message }`, `CapabilityDenied { missing }`, `RateLimited { retry_after_ms }`, `Success`, `ToolNotFound`. The only field that could carry a tool-author-supplied string is `Outcome::InvalidArgs { message }` — and that's the *input* shape, not the result. **No PHI exit point through audit exists in v0.3.0.**

**The brief's "audit log should see redacted event" framing is therefore a non-problem under the current schema.** What it *would* become if a future SP enriches `CallEvent.args_preview` or `CallEvent.result_preview`: we'd need an audit-side middleware hook. The right time to add that is when that data exists, not now. This SP explicitly *does not* pre-build the infrastructure — the v3 whitepaper §K.5 audit shape (`atd-v3-multi-device.md:3420-3433`) records `input_hash` / `output_hash` (sha256), not bodies, so even the v3 ambition doesn't require it.

**What we *do* commit to.** If/when an audit-side hook lands, this crate's `redact_value(v: &mut Value)` core function is reusable verbatim — it operates on `&mut serde_json::Value`, not on `Middleware` trait shape. The function lives in `atd_middleware_pii_redact_medical::redact` (public), so a future audit-side wrapper composes by import.

**Why not just add a hook now for symmetry.** Pre-built infrastructure for absent data is the worst kind of API debt — it ossifies the schema (`CallEvent` would have to add `result_preview: Option<Value>`) before we know the shape. SP-token-broker-phase1's "additive default impl" pattern (`docs/superpowers/specs/2026-04-27-sp-token-broker-phase1-design.md:14-30`) is the correct precedent: add the hook *when* the data exists, not before.

### 4.8 Celia migration — 4-step cut-over, §13.1 invariant verified at each

**Decision.** Celia adopts `atd-middleware-fhir` first (no behavior change — Celia already validates), then `atd-middleware-pii-redact-medical` second (genuinely new redaction layer). Celia's existing `audit_log` self-redaction stays — middleware is *additive defense*, not replacement.

**Step 1: Wrap Celia tool catalogue dispatch with `atd-middleware-fhir` (no behavior change).** Celia's `crates/celia-cli/src/serve.rs` registers `celia_tools::tool_catalogue()` (see `crates/celia-tools/src/tools.rs:83-225`). After SP-streamable-http step 1 lands, Celia's tools sit behind `atd_runtime::Registry`. Add the FHIR middleware to the chain: `server.set_middleware(vec![Arc::new(FhirMiddleware::default())])`. Verify §13.1: the middleware operates on already-decrypted FHIR JSON in `serde_json::Value`; the DEK in `KeyCache` is never touched. Run `pnpm --filter @celia/desktop test:dek` — passes unchanged. Run `cargo test -p celia-core` (159 tests) — passes unchanged because Celia's `validate_resource` already enforces the same rules; double-validation is idempotent.

**Step 2: Add `atd-middleware-pii-redact-medical` with `fhir_aware: true` in *log-only mode*.** Add a new `RedactionStrategy::LogOnly` (annotates the result with `_phi_findings: ["/name","/identifier"]` but doesn't strip) — for one release cycle, Celia operators see what *would* be redacted without behavior change. Verify §13.1: still no DEK contact. Cross-project: `apps/desktop test:e2e` Playwright smoke must pass; verify the LLM responses still contain identifiable info (because we're log-only). Audit the `_phi_findings` annotation count against expected on Celia's MIMIC-IV sample dataset (`docs/mimic-bench-2026-05-07-sample.md` exists; use as ground truth).

**Step 3: Flip to active redaction (`Token` strategies).** One release after Step 2's data has been studied. From this release, the LLM sees `[REDACTED:NAME]` instead of `"Mary Smith"`. Verify clinical reasoning quality: Celia's existing eval suite (`docs/fhir-bench-zh-50-2026-05-08-sample.md`, `docs/agent-eval-2026-05-07-sample.md`) regression — accept a ≤5% degradation on identity-bound questions; reject if reasoning fails on age/condition queries. §13.1 — DEK + KeyCache still untouched.

**Step 4: Decide whether to retire Celia's self-redact at the DB layer.** Celia's `audit_log` table doesn't store result bodies (`crates/celia-core/src/audit/mod.rs:26-47`), so there's nothing to retire there — it's already at the optimum (single field write, no body). The DB self-redact is a *separate layer* (audit-table schema choice, not middleware). It stays. **The middleware is a defense-in-depth layer outside the DB; the DB schema is the inner barrier. Two layers, intentional redundancy.**

**Rollback at each step.** Step 1: revert `set_middleware` line. Step 2: drop the PII middleware from the chain. Step 3: switch `Token` strategies back to `LogOnly`. None of these touch the `KeyCache`, `ServerState`, or any §13.1-relevant code.

**§13.1 invariant audit at each step.** The patent invariant: DEK derived from passphrase, held only in `KeyCache: Map<user_id, Arc<Zeroizing<Vec<u8>>>>`, lost on process restart. Both middleware operate on `serde_json::Value` arriving at `on_result` *after* `Tool::call` has already decrypted (because the dispatcher decrypts before returning to the wire — Celia's `dispatch_for_caller` reads from FHIR store with the DEK, then yields plaintext). The middleware never imports `KeyCache`, never holds an `Arc<ServerState>`. gcore can verify this with `objdump -t libatd_middleware_fhir.so | grep -i keycache` (zero matches expected); the test `pnpm --filter @celia/desktop test:dek` exercises the round trip and remains green.

## 5. Crate shapes

### 5.1 `atd-middleware-fhir`

**Cargo.toml (new file)**:
```toml
[package]
name = "atd-middleware-fhir"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "FHIR R4 egress validation middleware for atd-runtime: shape + coding-system whitelist."
readme = "README.md"
keywords = ["atd", "fhir", "healthcare", "middleware", "validation"]
categories = ["api-bindings"]

[dependencies]
atd-runtime = { path = "../atd-runtime", version = "0.3.0" }
atd-protocol = { path = "../atd-protocol", version = "0.3.0" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
```

**Top-level public API (Rust pseudo-code, no impl)**:
```rust
// crates/atd-middleware-fhir/src/lib.rs
pub mod config;
pub mod systems;

pub use config::{FhirMiddlewareConfig, MismatchPolicy};
pub use systems::ALLOWED_SYSTEMS_DEFAULT;

/// Egress FHIR R4 validation: shape + coding-system whitelist.
///
/// Activated only when `result.get("resourceType").is_some()` OR
/// `result` is an array/Bundle of objects with `resourceType`.
/// Other results pass through untouched.
#[derive(Debug, Clone)]
pub struct FhirMiddleware {
    config: FhirMiddlewareConfig,
}

impl FhirMiddleware {
    pub fn new(config: FhirMiddlewareConfig) -> Self;
    pub fn default() -> Self;  // Celia-equivalent 70-system whitelist
}

impl atd_runtime::Middleware for FhirMiddleware {
    fn name(&self) -> &'static str { "fhir_egress_validate" }
    fn on_result(&self, tool_id: &str, tool_def: &ToolDefinition, result: &mut Value);
}

// crates/atd-middleware-fhir/src/config.rs
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct FhirMiddlewareConfig {
    pub extra_systems: Vec<String>,
    pub replace_systems: Option<Vec<String>>,
    pub known_resource_types: Vec<String>,  // default = Celia 12
    pub on_mismatch: MismatchPolicy,
}

#[derive(Debug, Clone)]
pub enum MismatchPolicy {
    /// Append `_fhir_validation_errors: ["..."]` to the result; pass through.
    AnnotateAndPass,
    /// Replace the result with `{ "error": "fhir_validation_failed", "details": [...] }`.
    /// Tool's `success: true` flag is preserved at the wire (the middleware
    /// rewrites only the body — dispatch records the call as Success).
    ReplaceWithError,
    /// Strip the offending coding/field, keep everything else.
    StripOffending,
}
```

**Usage example**:
```rust
use atd_middleware_fhir::FhirMiddleware;
use atd_server::Server;
use std::sync::Arc;

let mut server = Server::new(registry, config);
server.set_middleware(vec![Arc::new(FhirMiddleware::default())]);
server.run().await?;
```

### 5.2 `atd-middleware-pii-redact-medical`

**Cargo.toml (new file)**:
```toml
[package]
name = "atd-middleware-pii-redact-medical"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "Healthcare PHI redaction middleware for atd-runtime: HIPAA Safe Harbor projection over FHIR R4 and generic JSON."
readme = "README.md"
keywords = ["atd", "fhir", "pii", "phi", "hipaa", "middleware"]
categories = ["api-bindings"]

[dependencies]
atd-runtime = { path = "../atd-runtime", version = "0.3.0" }
atd-protocol = { path = "../atd-protocol", version = "0.3.0" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
regex = "1"
sha2 = "0.10"  # for HashSha256Truncated strategy
```

**Top-level public API**:
```rust
// crates/atd-middleware-pii-redact-medical/src/lib.rs
pub mod config;
pub mod paths;
pub mod redact;
pub mod strategy;

pub use config::PiiRedactConfig;
pub use paths::DEFAULT_PHI_PATHS;
pub use redact::redact_value;  // standalone fn, no Middleware wrapper
pub use strategy::RedactionStrategy;

#[derive(Debug, Clone)]
pub struct PiiRedactMiddleware {
    config: PiiRedactConfig,
}

impl PiiRedactMiddleware {
    pub fn new(config: PiiRedactConfig) -> Self;
    pub fn default() -> Self;  // 13 default paths + 5 regex layer, fhir_aware=true
    pub fn log_only() -> Self;  // for migration step 2 (annotate, don't strip)
}

impl atd_runtime::Middleware for PiiRedactMiddleware {
    fn name(&self) -> &'static str { "pii_redact_medical" }
    fn on_result(&self, tool_id: &str, tool_def: &ToolDefinition, result: &mut Value);
}

// crates/atd-middleware-pii-redact-medical/src/config.rs
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct PiiRedactConfig {
    pub extra_paths: Vec<(String, RedactionStrategy)>,
    pub override_strategies: HashMap<String, RedactionStrategy>,
    pub disable_regex_phi: bool,
    pub fhir_aware: bool,
    pub annotate_findings: bool,  // if true, attach `_phi_findings: [...]`
}

// crates/atd-middleware-pii-redact-medical/src/strategy.rs
#[derive(Debug, Clone)]
pub enum RedactionStrategy {
    Strip,
    Token(&'static str),
    FirstCharPrefix,
    HashSha256Truncated,
    YearOnly,
    ZipPrefix3,
    LogOnly,  // annotate without modifying (migration step 2)
}
```

**Usage example (combined with `atd-middleware-fhir`)**:
```rust
use atd_middleware_fhir::FhirMiddleware;
use atd_middleware_pii_redact_medical::PiiRedactMiddleware;
use atd_server::Server;
use std::sync::Arc;

let mut server = Server::new(registry, config);
// Order matters: FHIR validates structure first; PII redacts afterwards.
// Reversing the order would let PII run on already-rejected payloads
// (waste); the chosen order also means findings annotations from one
// middleware don't accidentally validate the other's annotations.
server.set_middleware(vec![
    Arc::new(FhirMiddleware::default()),
    Arc::new(PiiRedactMiddleware::default()),
]);
server.run().await?;
```

## 6. Wire / behaviour contract

**Input.** Both middleware see exactly what `Middleware::on_result` sees: `&mut serde_json::Value` representing the tool's success payload (post-`Tool::call` success arm, `crates/atd-server/src/connection.rs:283`). The `tool_def` (`crates/atd-protocol/src/tool.rs:7-45`) carries `data_sensitivity: Option<String>` (`tool.rs:78`) — but this is *currently optional and rarely set*; middleware does **not** branch on it for v1 (treat-as-if-`None`). A future SP can introduce `data_sensitivity == "phi"` as an opt-in/opt-out signal.

**Output.** Same value, mutated in place. Both middleware respect the `Middleware` contract: "Must be deterministic and side-effect-free beyond the `result` mutation" (`middleware.rs:14-15`). No I/O, no logging, no `panic!`.

**ABI stability semver.** Both crates pin major version to `0.x` until the v3 whitepaper Appendix K result-middleware spec stabilizes. Within `0.x`:
- `FhirMiddlewareConfig` and `PiiRedactConfig` are `#[non_exhaustive]` — operators construct via `..Default::default()` (additive fields OK).
- `RedactionStrategy` and `MismatchPolicy` are `#[non_exhaustive]` — new variants are additive.
- `DEFAULT_PHI_PATHS` and `ALLOWED_SYSTEMS_DEFAULT` are `pub const &[...]` — adding entries is additive; removing/changing entries is a minor-version bump and triggers a release note.

**Error handling.** Middleware never returns errors today (`Middleware::on_result` returns `()`, `middleware.rs:19`). Both crates honor this:
- **FHIR mismatch** → `MismatchPolicy` (default `AnnotateAndPass`): the result gets `_fhir_validation_errors: [...]` field, dispatch continues, audit logs `Outcome::Success`. Operator picks `ReplaceWithError` for fail-closed (returns a `{error: "..."}` body to LLM but dispatch outcome is still `Success` because the *tool* succeeded; the *middleware* objected — this dual state is the v3 whitepaper §K.4 `Pass(r) | Warn(r, w) | Block(reason)` semantics specialized to our `()` return).
- **PII regex / pointer walk error** → silent pass-through (the value the operator can't describe with a Pointer is the value they can't redact). Library-level `log::warn!` to `target = "atd_middleware_pii_redact_medical"` for observability. Operators wire this through `tracing` if they want it.

**Fail-fast vs warn-and-pass.** Default is warn-and-pass (annotate). Operators who need fail-closed compose a thin wrapper middleware that consumes `_fhir_validation_errors` and rewrites the result. This SP does not ship that wrapper — it's a 20-line policy choice, not a primitive.

## 7. Migration path (Celia side)

Detailed in §4.8 above (4 steps with §13.1 verification per step). Summary checklist:

| Step | Adds | Removes | §13.1 check | Cross-project test |
|---|---|---|---|---|
| 1 | `FhirMiddleware::default()` to chain | nothing | gcore DEK eviction (`pnpm --filter @celia/desktop test:dek`) | `cargo test -p celia-core` (159 tests) |
| 2 | `PiiRedactMiddleware::log_only()` to chain | nothing | same | `apps/desktop test:e2e` + MIMIC-IV finding-count baseline |
| 3 | Flip step 2 to `default()` (Token strategies) | nothing | same | `docs/fhir-bench-zh-50-2026-05-08-sample.md` regression ≤5% degradation |
| 4 | nothing | (Celia's audit DB self-redact stays — defense in depth) | same | release-note documenting the dual-layer model |

Rollback per step: revert the `set_middleware` line; nothing else changes. Independent rollback per crate is possible because the two crates are independently composed in the `Vec<Arc<dyn Middleware>>`.

## 8. Test plan

### 8.1 Unit tests (per crate)

- **`atd-middleware-fhir::tests::default_systems_match_celia_count`** — `DEFAULT_SYSTEMS.len() == 70` (drift guard mirroring `systems.rs:104-110`).
- **`atd-middleware-fhir::tests::rejects_celia_legacy_uri`** — `https://celia.health/fhir/codes` triggers a mismatch annotation; mirrors `systems.rs:120-123`.
- **`atd-middleware-fhir::tests::accepts_loinc_snomed_rxnorm`** — three positive cases.
- **`atd-middleware-fhir::tests::passes_non_fhir_result_untouched`** — result without `resourceType` is unmodified (`{"echoed": "hi"}` round-trips bit-identical).
- **`atd-middleware-fhir::tests::missing_required_field_per_type`** — Observation without `status`, Goal without `lifecycleStatus`, etc. (12 types × 1 missing each = 12 cases; mirror `validate.rs:117-166`).
- **`atd-middleware-fhir::tests::policy_replace_with_error`** — `MismatchPolicy::ReplaceWithError` rewrites to `{"error":...}` shape.
- **`atd-middleware-pii-redact-medical::tests::default_paths_cover_18_hipaa_categories`** — assert the 13-path + 5-regex table maps to all of A-R.
- **`atd-middleware-pii-redact-medical::tests::patient_name_tokenized`** — input `{"resourceType":"Patient","name":[{"family":"Smith"}]}` → name array contains `Token("NAME")`.
- **`atd-middleware-pii-redact-medical::tests::birthdate_truncated_to_year`** — `1955-03-15` → `1955`.
- **`atd-middleware-pii-redact-medical::tests::ssn_regex_anywhere`** — SSN embedded in `note.text` gets tokenized.
- **`atd-middleware-pii-redact-medical::tests::log_only_does_not_mutate`** — `_phi_findings` annotation added, content unchanged.
- **`atd-middleware-pii-redact-medical::tests::generic_json_mode`** — `fhir_aware: false`, plain `{"user":"alice","email":"a@b.c"}` — email gets tokenized via regex, `user` field untouched.

### 8.2 Integration tests

- **`crates/atd-middleware-fhir/tests/e2e_with_ref_server.rs`** — register a synthetic FHIR-returning tool with `atd-ref-server`, attach `FhirMiddleware`, call via UDS, assert annotation is present in result.
- **`crates/atd-middleware-pii-redact-medical/tests/e2e_combined.rs`** — both middleware in the chain, call a Celia-shaped `Patient` tool fixture, assert FHIR validation passes AND PHI is tokenized.
- **`crates/atd-middleware-pii-redact-medical/tests/e2e_audit_invariant.rs`** — install a custom `AuditSink` that fails the test if `serde_json::to_string(&event)` contains any string from a forbidden-list (`["John Smith", "555-12-3456"]`). Confirm no PHI leaks via `CallEvent` either before or after middleware (the test is a regression guard against `audit.rs:25-44` field schema drift).

### 8.3 Cross-project (Celia)

- **`pnpm --filter @celia/desktop test:dek`** — passes at every migration step (the §13.1 invariant test).
- **`cargo test -p celia-core`** — 159 tests pass at step 1 (double-validation is idempotent because Celia's `validate.rs` produces no errors on Celia-valid resources; the middleware additionally produces none).
- **`apps/desktop test:e2e`** — 18 Playwright tests pass at step 1 and step 2; at step 3, retest with redacted responses (LLM may need prompt tuning).
- **`docs/agent-eval-2026-05-07-sample.md` regression** — accept ≤5% drop on identity-bound questions at step 3.

### 8.4 Cross-project (atd-mvp)

- **`crates/atd-ref-server/tests/e2e_medical_middleware_chain.rs`** (new) — synthetic registry with one FHIR tool + one PHI-leaking tool; assert the chain catches both and the existing 250+ tests pass with the chain installed (no regression on non-medical tools).
- **`crates/atd-conformance`** — extend the conformance suite with two new cases: "FHIR middleware annotates legacy URI" and "PII middleware tokenizes SSN" — gated on `--feature medical-middleware`.

## 9. Out of scope (future SPs)

| Feature | Why deferred | Sketch of future SP |
|---|---|---|
| NLP-based PHI detection in narrative text | Requires model dep (Presidio / spacy / Gemma); plugin-shaped per v3 §K.2.2 `detector_ref` | SP-medical-nlp-phi — external plugin via the v3 plugin loader |
| DICOM / image EXIF stripping | Different input shape (binary attachments); v3 §K.2.5 reserves this | SP-medical-image-strip — separate crate, binary-aware |
| Region-specific code systems (CN ICD-10-CM国家版, JLAC, Read v3) | Operator config covers it; defaults stay international | Out-of-tree adopter crates can publish region-tailored configs |
| Compliance audit certifications (HIPAA / SOC-2 / ISO 27001) | Operator-side activity, not a crate | Documentation-only SP attaching the conformance suite to certification narrative |
| Schema-based deep validation (cardinality, slicing, FHIRPath invariants) | Requires schema bundle; cost-benefit weak | SP-medical-fhir-schema — only if a hospital HIS gateway adopter demands it |
| `data_sensitivity: "phi"` opt-in/opt-out wiring | `ToolDefinition.safety.data_sensitivity` field exists (`crates/atd-protocol/src/tool.rs:78`) but no adopter sets it today | SP-medical-data-sensitivity — extend tool author UX |
| Audit-side middleware hook | `CallEvent` schema doesn't carry result bodies today; no data to redact | Future SP if v3 §K.5 `output_hash`/`input_hash` enrichment lands |
| PII-as-symmetric-encryption (decrypt-with-token instead of tokenize) | Use case ambiguous — operator could combine with VC | SP-medical-phi-vault — only with a vetted adopter pull |

## 10. References

### atd-mvp source (line-precise; spot-check targets)

1. `crates/atd-runtime/src/middleware.rs:16-20` — `Middleware` trait shape; both new crates impl this.
2. `crates/atd-runtime/src/middleware.rs:44-95` — `RedactPathsMiddleware` reference impl; new crates follow this pattern (regex/Pointer walk, deterministic, no I/O).
3. `crates/atd-runtime/src/middleware.rs:14-15` — "Must be deterministic and side-effect-free beyond the `result` mutation" — load-bearing contract for both new crates.
4. `crates/atd-runtime/src/audit.rs:25-44` — `CallEvent` schema; carries no `args` or `result_preview` — §4.7 grounds the "no audit-side hook" decision here.
5. `crates/atd-runtime/src/audit.rs:60-62` — `AuditSink::on_call(&CallEvent)` — receives metadata only; no body access.
6. `crates/atd-server/src/connection.rs:283-289` — middleware chain runs on success only; both new crates plug here unchanged.
7. `crates/atd-server/src/server.rs:44-51` — `Server::set_middleware`; the integration point for `Arc<FhirMiddleware>` + `Arc<PiiRedactMiddleware>`.
8. `crates/atd-protocol/src/tool.rs:74-79` — `ToolSafety.data_sensitivity: Option<String>`; reserved hook for future per-tool middleware opt-in.
9. `crates/atd-protocol/src/tool.rs:7-45` — `ToolDefinition` shape passed to `on_result`; middleware reads `tool_def` for diagnostic logging only.
10. `docs/whitepaper/atd-v3-multi-device.md:32` — v2→v3 delta table line item "Result middleware pipeline (PII / injection / trim / format / image meta)"; this SP delivers PII + FHIR-validation slices.
11. `docs/whitepaper/atd-v3-multi-device.md:3329-3343` — Appendix K.2.1 `pii_redact` spec; this SP refines it with per-field strategy + HIPAA Safe Harbor projection.
12. `docs/whitepaper/atd-v3-multi-device.md:3403-3416` — Appendix K.4 pipeline execution; our chain composition follows the same `Pass / Warn / Block` semantics specialized to `()` return.
13. `docs/whitepaper/atd-v3-multi-device.md:3420-3433` — Appendix K.5 audit log shape; `output_hash` (sha256), not body — §4.7 cites this as forward-compatible.
14. `docs/superpowers/specs/2026-04-25-sp12-canonical-dispatch.md` (entire SP-12) — middleware-on-success-only invariant; our crates respect it.
15. `docs/superpowers/specs/2026-04-25-sp-listener-extract-design.md:23-24` — sibling-crate principle; we apply it to medical middleware split.
16. `docs/superpowers/specs/2026-04-27-sp-token-broker-phase1-design.md:14-30` — additive-default-impl pattern; §4.7 uses it as precedent for not extending `AuditSink` until data flows.
17. `docs/superpowers/specs/2026-05-11-sp-streamable-http-design.md:45-49` — sibling-crate style guide; our two-crate split mirrors it.

### Celia source

18. `celia_phr/crates/celia-core/src/fhir/systems.rs:15-91` — the 70-entry `ALLOWED_CODE_SYSTEMS` list; `atd-middleware-fhir::ALLOWED_SYSTEMS_DEFAULT` copies it.
19. `celia_phr/crates/celia-core/src/fhir/systems.rs:104-110` — drift-guard test (`count_matches_ts`); our crate mirrors the pattern.
20. `celia_phr/crates/celia-core/src/fhir/validate.rs:43-67` — the validation entry point + recursive coding walk that `atd-middleware-fhir` ports.
21. `celia_phr/crates/celia-core/src/fhir/validate.rs:117-166` — per-type required-field map for the 12 Celia-supported resources; `atd-middleware-fhir` uses the same table.
22. `celia_phr/crates/celia-core/src/audit/mod.rs:26-47` — Celia's `AuditEntry` proves the audit-side self-redact has already been done at the DB schema layer (no body fields); §4.7 + §4.8 cite this.
23. `celia_phr/crates/celia-tools/src/tools.rs:155-214` — `get_health_record` + `issue_health_credential` tool definitions; these are the canonical tools whose FHIR results the new middleware will exercise.
24. `celia_phr/docs/TERMINOLOGY_MAPPING.md:15-24` — narrative explanation of the 5+ standard coding systems Celia requires; provides the spec text behind `ALLOWED_SYSTEMS_DEFAULT`.

### External spec

25. HIPAA §164.514(b)(2)(i)(A-R) — the 18 Safe Harbor identifiers projected to FHIR R4 paths in §4.5.
26. FHIR R4 spec — http://hl7.org/fhir/R4/ — element-path normative source for `Patient.name`, `Patient.telecom`, `Patient.address`, `Patient.birthDate`.
27. RFC 6901 — JSON Pointer; `DEFAULT_PHI_PATHS` entries are RFC 6901 strings.

---

**Summary.** Two sibling crates: `atd-middleware-fhir` enforces FHIR R4 shape + coding-system whitelist on egress (default = Celia's 70-URI list); `atd-middleware-pii-redact-medical` projects HIPAA Safe Harbor's 18 identifiers onto FHIR JSON paths plus a regex catch-all, with per-field redaction strategy (`Token`, `YearOnly`, `ZipPrefix3`, `Strip`, `HashSha256Truncated`, `LogOnly`). Both impl `atd_runtime::Middleware::on_result`; both run post-dispatch on success only (existing infra unchanged). Audit-side hook is *not* added — `CallEvent` doesn't carry result bodies today, so no PHI flow exists to redact. Celia migrates in 4 steps (FHIR enable → PII log-only → PII active → keep DB self-redact as defense in depth), §13.1 invariant verifiable at each. Parity with Celia's existing 159 cargo tests + `test:dek` is the contract.
