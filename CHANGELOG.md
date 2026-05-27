# Changelog

All notable changes to ATD (`atd-protocol`, `atd-runtime`, `atd-sdk`, and
the middleware / server / tooling crates in this workspace) are
documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Workspace crates share a single version cadence (`workspace.package.version`),
so a `1.0.0` line below means **every** crate in `crates/` ships at
`1.0.0` — adopters pinning one crate get a consistent set across the
whole stack.

Each entry cites the tag where the change landed; full design rationale
lives at `docs/adr/` and (for pre-1.0 history) `docs/archive/superpowers/specs/`.

---

## [1.1.0] — 2026-05-27

First minor bump on the 1.x line. Purely additive — the wire is
unchanged, every 1.0.x adopter upgrades drop-in by changing
`atd-* = "1.0"` to `atd-* = "1.1"` (or already with `= "1"` caret).

### Added

- **`atd-protocol::CliBindingConfig`** — typed canonical shape for
  `ToolBinding.config` when `protocol = "Cli"`. Encodes the recurring
  CLI-binding fields (`cmd`, `args`, `args_template`, `env`,
  `output_format`, `page_all_flag`, `dry_run_flag`, `exit_code_map`)
  with serde `skip_serializing_if` so pre-v2 `{"cmd": "..."}` configs
  round-trip byte-identical. Plus `CliOutputFormat` enum (`Json` /
  `Ndjson` / `Lines`) and `ToolBinding::cli_config()` helper.
  Tag: `sp-cli-binding-v2`. Spec: `docs/extending/cli-binding.md`.
- **`atd-protocol-schema.json`** picks up `CliBindingConfig` +
  `CliOutputFormat` under `definitions/` so adopter manifests can
  `$ref` them.

### Notes

- `ToolBinding.config` on the wire remains untyped `serde_json::Value`
  — the new types are *peer* definitions, not a tightening of the
  existing field. Pre-1.1 servers and clients continue to function
  unchanged.
- The runtime subprocess dispatcher that consumes `CliBindingConfig`
  (i.e. a generic `CliBinding` in `atd-runtime` that reads
  `ToolBinding.config` instead of being hardcoded per-instance) is
  deferred to a future SP (`SP-cli-dispatcher-v1`). 1.1 ships the
  declarative *shape*; 1.2 or later will ship the dispatcher that
  consumes it.

### Compatibility

- **Wire**: no change. Pre-1.1 frames parse byte-identically on 1.1
  servers and vice versa.
- **API**: purely additive new types in `atd-protocol`. Caret-pinned
  adopters (`atd-* = "1"`) pick up the new types automatically.
- **Workspace cadence**: 15 publishable crates re-released at `1.1.0`
  for version unification per the v0.3.0 decision; `atd-mock-weather-
  server` stays `publish = false`.

---

## [1.0.0] — 2026-05-21

The stability release. ATD's wire format, JSON schema, and public
extension traits are now **frozen for the 1.x line** — see
[`docs/release-plan-v1.0.md`](docs/release-plan-v1.0.md) for the full
stability contract.

1.0 ships no new protocol features over `0.3.0`: it declares the `0.3.0`
surface stable, completes the documentation, and renames the repository.

### Stability commitment

- **Wire format frozen** for 1.x. Additive changes (new optional fields,
  new enum variants) are minor bumps; removing or reshaping a field is a
  major (2.0) bump.
- **`atd-protocol-schema.json` frozen** on the same contract — code
  generated from the 1.0 schema deserialises every 1.x message.
- **Public extension traits stable** — `Tool`, `Binding`, `Middleware`,
  `TokenBroker`, `AuditSink`. Extensions built against 1.0 keep
  compiling across the 1.x line.
- **`AtdError` variants and `ERR_*` wire codes stable.**
- MSRV `1.85`; workspace-lockstep versioning through 1.x.

### Changed

- **Repository renamed** `atd-mvp` → **`atd`**. The protocol, the brand,
  and every crate already used the `atd` name; the directory now matches.
  Crate names are unchanged.
- **Workspace version** bumped `0.3.0` → `1.0.0`.

### Documentation

A full overhaul to meet the agent-native bar — a code agent cloning the
repo can implement, verify, and extend ATD without external context:

- **`AGENTS.md`** — rewritten as the authoritative agent entry point
  (was a stale Phase-0 file).
- **`docs/index.md`** — new documentation map + authority hierarchy.
- **`docs/extending/`** — new: eight how-to guides, one per extension
  point (tool, binding, middleware, transport, token-broker,
  audit-sink, protocol-and-schema).
- **`docs/roadmap.md`** — new: evolution scope — deferred features,
  known limitations, post-1.0 direction.
- **`docs/release-plan-v1.0.md`** — new: the 1.0 stability contract +
  release procedure.
- **`docs/archive/`** — new home for frozen history: the Superpowers
  (SP) design archive, the Phase 0 `design.md`, validation logs, and the
  superseded 0.3.0 release plan.
- **Removed** `docs/whitepaper/` (external snapshots, not
  source-of-truth) and `docs/reference/` (ANOS-scoped content).
- Every surviving doc — architecture, protocol reference, quickstarts,
  integrations, crate READMEs — accuracy-checked against the 1.0 code.

### Fixed

- **CI release-build step** built a non-existent package
  `atd-ref-server-bin`; corrected to `atd-ref-server`.
- Schema `$id` and the protocol-reference version header advanced from
  `0.1.0` to the `1.0` line.

### Issues closed

- `docs/issues/2026-04-24-security-capability-tokens-deferred.md` —
  resolved by UCAN-lite capability tokens (shipped 0.3.0).
- `docs/issues/2026-04-24-security-audit-logging-missing.md` — resolved
  by the structured audit sink (shipped 0.3.0).

---

## [0.3.0] — 2026-05-13

The federation / multi-tenant / performance / medical-payload release.
85 commits since `v0.2.1`, organised below by the SP or Phase that
landed each cluster. See `docs/release-plan-v0.3.0.md` for the
adopter migration story.

### Added — new crates

- **`atd-middleware-fhir`** (SP-medical-middleware Phase A,
  `095e717`). FHIR R4 egress validation: `FhirMiddleware` +
  `FhirMiddlewareConfig` + `MismatchPolicy::{AnnotateAndPass,
  ReplaceWithError, StripOffending}` + `ALLOWED_SYSTEMS_DEFAULT`
  (75-entry coding-system whitelist, set-equal to celia's
  `whitelists.toml` via the Phase L.0 drift-guard).
- **`atd-middleware-pii-redact-medical`** (SP-medical-middleware
  Phase B, `597a8aa`). HIPAA PHI redaction:
  `PiiRedactConfig::{fhir_aware, disable_regex_phi, ...}`,
  regex-based PHI catchers (email / phone / SSN / URL / IP /
  …), FHIR-path strip strategies, 28 unit tests across the
  redact + regex + paths modules.
- **`atd-server-http`** (SP-streamable-http + SP-1.B,
  `758ce40` spec / `dcdfd92` runtime / `0448aad` body / `aebfa90`
  `/initialize` echo). HTTP transport for adopters who can't
  ship a UDS socket — origin gate, bearer auth, MCP JSON-RPC
  translator, SSE bearer-refresh helper.

### Added — protocol surface

- **UCAN-lite capability tokens** (SP-capability-v2,
  `1f25da6` → `66639b6`, tag `sp-capability-v2`). JWT compact-form
  bearer tokens with Ed25519 signatures, `did:key` audiences,
  attenuation chains, revocation store. Wire surface: `Hello.
  ucan_tokens: Vec<String>`, error codes 1010–1013. Server side:
  `atd_runtime::ucan::{parse_jwt, verify_jwt, VerifyConfig,
  UcanRevocationStore, InMemoryUcanRevocationStore}`. Granted
  capabilities at dispatch = `strings ∪ ucan` — additive to
  SP-12's allow-list (no breakage for existing adopters).
- **Bearer auth on the wire** (SP-token-broker-phase2,
  `3697b78` → `aebfa90`, tag `sp-token-broker-phase2`).
  `TokenBroker::resolve_bearer` + `BearerIdentity` + 11-variant
  `BearerOutcome` typed enum with per-variant HTTP status /
  `WWW-Authenticate` / `Retry-After` mapping. HTTP listener calls
  this once per request before dispatch; `caller_id` is the
  routing key downstream.
- **Pagination** (SP-pagination-v1, `729582f` → `9cf1d14` +
  `db315e8`, tag `sp-pagination-v1`). HMAC-SHA256-signed
  CBOR-encoded cursors bound to
  `(tool_id, caller_id, args_fingerprint, page_index, issued_at_unix,
  server_session)`. Wire: `Request::RunToolContinue { tool_id, cursor }`,
  `Response::ToolResultResponse.next_cursor: Option<String>`,
  error codes 1020/1021. Tool author API:
  `Tool::supports_pagination` + `Tool::call_paginated`. SDK
  ergonomics: `AtdClient::call_page` + `AtdClient::call_all` +
  `MergePolicy::{ConcatArray, ConcatField, FirstPageOnly}`.
  Cursor wire cap 512B, default TTL 5 min, stateless verify.
- **Wire deadlines** (SP-concurrency-baseline,
  `92282c3` → `796f471`, tag `sp-concurrency-baseline`).
  `WireError` typed enum + `read_frame_with_deadline` /
  `write_frame_with_deadline`. Per-state deadlines: 5s
  handshake, 30s active. Adopter hook
  `Server::set_frame_deadlines`.
- **Tool visibility** (SP-tool-visibility-hidden, `59b8ffb`,
  tag `sp-tool-visibility-hidden`). `ToolVisibility::Hidden`
  variant — server filters Hidden tools out of `Request::ToolList`
  but they remain reachable via `Request::ToolSchema` and
  `Request::RunTool`. SDK's `DiscoverFilter::visibility = Hidden`
  returns empty (the server never emits them in discover).
- **Machine-readable protocol schema** (SP-protocol-schema,
  tag `sp-protocol-schema`). `/atd-protocol-schema.json` shipped
  as a build artifact via `gen-schema` bin; CI gates drift +
  metaschema validity. Closes the long-standing
  `2026-04-24-schema-protocol-machine-readable-missing` audit
  gap (third-party implementers no longer need to read Rust to
  build a TS / Go / Java implementation).
- **Skills meta-tool discovery convention**
  (SP-skills-discovery-convention, `4112b01` → `698316e`, tag
  `sp-skills-discovery-convention`). `atd skills sync` CLI
  subcommand with 3 sync targets, formalised
  `tool_id == "skills.list"` discovery convention, doc updates.

### Added — multi-tenant + identity foundation

- **`TokenBroker` Phase 1** (SP-token-broker-phase1, `d61e449`,
  tag `sp-token-broker-phase1`). `TokenBroker` trait +
  `InMemoryTokenBroker` reference impl + `BrokerError` +
  `RedactedString` (Debug/Display refuse to print).
  `CallContext::secrets: Option<Arc<SecretBundle>>` populated
  before `Tool::call`. Audit log includes only
  `secrets_resolved: bool` — never the values.
- **`FileTokenBroker`** (Phase L.0, `a7ee000`, tag `phase-l-0`).
  Disk-backed `TokenBroker` impl: per-bearer subdir under
  `${root}/${bearer_id}/{access_token,refresh_token,expires_at}.json`
  at mode 0700 / 0600 on Unix; per-bearer refresh mutex
  (`lock_refresh()`); `is_near_expiry()` no-IO predicate (5-min
  default window). Layout matches healthkit_cli v1.2.0's
  single-tenant on-disk shape — adopter migration is one `mv`
  per bearer.
- **Agent identity primitives** (SP-agent-identity spec only,
  `d347503`). `did:agent` + binary fingerprint VC design — no
  runtime code yet; tracks Phase D.
- **Secret bootstrap** (SP-secret-bootstrap spec only,
  `d55ca3c`). Parent-child secret injection pattern A
  generalised — no runtime code yet.

### Added — performance + observability

- **Multi-thread tokio runtime** (SP-concurrency-baseline §5.1,
  `1540b78`). Ref binaries flip from `current_thread` to
  `multi_thread` via `atd_runtime::default_worker_threads()`
  (default `min(cpus, 4)`).
- **SDK connect retry** (SP-concurrency-baseline §5.3,
  `2b8da2e`). `AtdClient::connect` with exponential backoff +
  ±20% jitter + `ConnectOptions::max_attempts`. Env-tunable via
  `ATD_CONNECT_RETRIES`.
- **Bounded mpsc audit sink** (SP-concurrency-baseline §5.4,
  `d2d9796`). `JsonLinesAuditSink` rewritten to
  `tokio::sync::mpsc` + dedicated drain task. Non-blocking
  `on_call`; drops counter exposed via metrics.
- **Metrics counters** (SP-concurrency-baseline §5.7,
  `98a922a`). `MetricsCounters` + `Server::metrics_snapshot()`.
  Adopter-visible call / error / cursor / audit-drop tallies.

### Added — adopter integrations

- **Healthkit case study** (`c0f2669`,
  `docs/integrations/healthkit.md` v1.1.0 → v1.2.0 → v1.2.1
  walkthrough). First live ATD adopter; cited extensively in
  README + introduction.
- **Cross-vendor mock demo** (SP-cross-vendor-mock-demo,
  `94b9a8e`, tag `sp-cross-vendor-mock-demo`).
  `atd-mock-weather-server` bin + `crates/atd-mock-weather-server/`
  (publish = false). Demo "agent queries 2+ ATD servers in one
  session" — referenced by `docs/integrations/cross-vendor-
  pattern.md`.
- **MCP bridge cursor handling** (SP-pagination-v1 §4.7,
  `1ad1699`). Degrade-or-passthrough behaviour controlled by
  `ATD_MCP_PASSTHROUGH_CURSOR=1`. Degrade mode (default) walks
  cursors on the agent's behalf; passthrough mode emits MCP
  `nextCursor` for clients that support it.
- **MCP bridge richer schema in `tools/list`** (`4fb652f` +
  `553005a`). Per-tool `describe()` so MCP clients see full
  `input_schema` instead of empty stubs.

### Added — cross-repo invariants (Phase L.0)

- **Whitelist drift-guard** (Phase L.0, `d976cd5`, tag
  `phase-l-0`). `ALLOWED_SYSTEMS_DEFAULT` is now set-equal to
  celia's source-of-truth `whitelists.toml`. The toml is
  vendored at `crates/atd-middleware-fhir/vendor/celia-
  whitelists.toml` and a `include_str!`-loaded unit test
  parses + asserts set equality at every `cargo test`. 70 → 75
  entries: added CVX (vaccines), HL7 v2-0203, allergy
  intolerance-clinical / -verification, Synthea generator URL.
- **L.0 5-AC verification integration test** (`4d2fdd4`).
  `crates/atd-conformance/tests/phase_l_baseline.rs` proves the
  five primitives celia's Phase L plan depends on compose
  end-to-end:
    - AC1 — `call_all` walks cursors transparently *(covered
      by `paginated_dispatch::call_all_walks_all_pages_via_concat_array`,
      cited in the header.)*
    - AC2 — `args_fingerprint` HMAC binds cursors to args
      *(covered by `paginated_dispatch::cross_tool_cursor_returns_invalid`
      + lib-level fingerprint stability tests.)*
    - AC3 — `TokenBroker` routes per `BearerIdentity` (new
      e2e exercising `FileTokenBroker` over the wire).
    - AC4 — `FhirMiddleware::MismatchPolicy::ReplaceWithError`
      rewrites payloads to a structured error envelope (new
      e2e; celia I8 fail-closed needs this on the wire).
    - AC5 — `CapabilitySet` Hello-time strict-subset
      negotiation (new e2e).

### Changed

- **Workspace version bump** to `0.3.0` (`f75cde5`). All crates
  follow `version.workspace = true` — adopters get a consistent
  cross-crate version.
- **Listener extracted** (SP-listener-extract, tag
  `sp-listener-extract`). Connection-handling code lifted into
  a reusable module so `atd-server` + `atd-server-http` can
  share semantics.
- **Conformance suite expanded** (SP-8.x + cross-cutting):
    - `expect_tools_exclude` primitive (`68fd0fb`) for visibility
      tests.
    - `concurrent_handshake_storm` scenario
      (`3fbe98c`): 50 simultaneous clients × Hello + ToolList +
      5×ToolSchema. Asserts SP-concurrency-baseline §4 SLO —
      p99 < 200ms, 0 errors, 0 audit drops. Measured at
      **wall=127ms / p50=116ms / p99=125ms** post-SP (vs the
      pre-SP 71s wall + 60% session-init failure at 10× lower
      concurrency).
    - `paginated_dispatch` scenario (`9cf1d14`): 100-row
      generator end-to-end, 10 pages × 10 rows, per-page audit
      tagging, cross-tool cursor rejection, expired-cursor
      rejection.
    - `phase_l_baseline` (`4d2fdd4`, see "Added — cross-repo
      invariants" above).

### Fixed

- **`atd-tools-fs` glob honours `.gitignore` outside git repos**
  (`e8d6b06`). Previous behaviour mistakenly disabled the
  gitignore filter when no `.git/` was detected; this surfaced
  on adopter installs that drop a flat tree without a repo init.
- **`atd-mcp-bridge` per-tool `describe()` in `tools/list`**
  (`4fb652f`). Pre-fix: MCP clients received a placeholder
  empty schema and had to call `tools/get` per tool. Post-fix:
  full `input_schema` ships in the initial enumeration.
- **`atd-cli` skills-sync collapsible `str_replace`** (`d87862c`).
- **Pre-existing clippy `field_reassign_with_default`** in
  `atd-middleware-fhir` (`d976cd5`) + `atd-middleware-pii-
  redact-medical` (`f4e4275`). Mechanical struct-literal
  rewrites. Remaining workspace-wide `-D warnings` failures in
  `atd-server-http` / `atd-tools-fs` / `atd-ref-server` example
  / one `mcp.rs` non-snake-case identifier are out of v0.3.0
  scope and tracked for a follow-up sweep.

### Closed adopter / cross-repo issues

- `docs/issues/2026-04-24-schema-protocol-machine-readable-missing.md`
  — resolved by SP-protocol-schema.
- `docs/issues/2026-05-12-celia-concurrency-adopter-validation.md`
  — closed-verified (`4ef4a37`). Evidence:
  `celia_phr/docs/atd-mcp-opt-iter4-baseline.md` (120Q SHARP, 0
  rate-limit / 0 connection failures).
- `docs/issues/2026-05-12-healthkit-perf-v1-adopter-validation.md`
  — closed-verified (`4ef4a37`). Evidence:
  `healthkit_cli/docs/sp-pagination-v1-adopter.md`
  (Activities + HealthRecord helpers paginated, 218 tests green).
- atd-mvp#6 (`[L.0] Protocol baseline check + FileTokenBroker +
  drift-guard test`) — closed by PR #7.

### Documentation

- `docs/architecture.md` — major refresh across §3 (Schema),
  §4 (Dispatch), §5 (Security), §10 (Evolution path). Each SP
  in this release has a row in §10.
- `docs/adr/` introduced. Three ADRs land:
  `0001-celia-atd-roadmap-alignment.md`,
  `0002-concurrency-baseline.md`,
  `0003-pagination-v1.md`.
- `docs/atd-introduction.md` — top-level intro grounded in the
  healthkit v1.4.0 case study (`4b91055`).
- `docs/whitepaper/` — 14-slide intro deck refresh
  (`f0b44fb` → `36009d8`), plus
  `docs/whitepaper/atd-vs-mcp.md` positioning paper
  (`289b170`).
- Naming alignment pass (`b204c21`): brand is **Agent Tool
  Dispatch (ATD)**, every published crate uses the `atd-`
  prefix; the repo directory `atd-mvp` is an internal handle
  retained for path-dep stability.
- Two previously-missing crate READMEs added.

### Operational notes for adopters

- **Path-dep adopters** (celia_phr, healthkit_cli) pick up all
  of the above by running `cargo update -p <atd-crate>` on the
  v0.3.0 line. Both adopters validated against this release —
  see `docs/integrations/healthkit.md` and the closed adopter
  validation issues above.
- **HTTP-transport adopters** can now run the same `atd-server`
  semantics behind `atd-server-http` (origin gate + bearer
  auth + MCP JSON-RPC translation).
- **Multi-tenant adopters** should consider migrating from
  `InMemoryTokenBroker` to the new `FileTokenBroker` for
  process-restart durability; layout is back-compat with
  healthkit_cli v1.2.0's single-tenant scheme.

### Tags landed in this release

`sp-tool-visibility-hidden`, `sp-skills-discovery-convention`,
`sp-cross-vendor-mock-demo`, `sp-token-broker-phase1`,
`sp-publish-v2`, `sp-listener-extract`, `sp-8-conformance-suite`,
`sp-8.1-capability-denied-gated-tool`,
`sp-8.2-rate-limit-conformance`, `sp-protocol-schema`,
`sp-medical-middleware`, `sp-streamable-http`,
`sp-capability-v2`, `sp-token-broker-phase2`,
`sp-concurrency-baseline`, `sp-pagination-v1`, `phase-l-0`.

---

## [0.2.1] — 2026-04-24

Last point release of the SP-12 canonical-dispatch line.
Pre-multi-tenant, pre-cross-repo-federation. See
`git log v0.2.0..v0.2.1` for the rollup.

## [0.2.0] — earlier

Canonical-dispatch landing (SP-12). See `git log v0.1.0..v0.2.0`.

## [0.1.0] — earlier

Initial workspace + reference server.

[1.0.0]: https://github.com/downsea/atd/releases/tag/v1.0.0
[0.3.0]: https://github.com/downsea/atd/compare/v0.2.1...phase-l-0
[0.2.1]: https://github.com/downsea/atd/releases/tag/v0.2.1
[0.2.0]: https://github.com/downsea/atd/releases/tag/v0.2.0
[0.1.0]: https://github.com/downsea/atd/releases/tag/v0.1.0
