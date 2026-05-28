# SP-8 Conformance Suite — Design

**Date:** 2026-04-24
**Status:** Approved — ready for implementation plan
**Scope:** New `atd-conformance` crate + self-conformance integration test.
**Parent:** Follows `sp-fmt-clippy-cleanup` (4-gate clean baseline).
**Architecture anchor:** `docs/atd-architecture.md` §10 roadmap row "Conformance suite (SP-8 original)".

## 1. Context

ATD claims to be a cross-vendor protocol. That claim needs a programmatic
verifier: any third-party ATD server — future HarmonyOS daemon, a self-
built vendor service, or a different-language reference — can run the
conformance suite against its Unix socket and receive a pass/fail report
on wire + behavior equivalence with the reference SDK.

The architecture doc flags a soft prerequisite ("benefits from protocol
schema being shipped first"). The brainstorm resolved this gate in favor
of shipping conformance first — reasoning: the reference `atd-protocol`
crate's Rust types already function as an implicit spec; a separate
machine-readable schema is only load-bearing once a non-Rust implementer
wants it, and none exist yet.

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q0 | Schema first or conformance first? | B — Conformance first. Schema SP deferred until a non-Rust implementer needs it. |
| Q1 | Coverage scope? | B — Wire + core behavior. Tier budgets, middleware, and CliBinding pathway are NOT tested (they're reference-default, not spec-normative). |
| Q2 | Driver shape? | C — Hybrid lib + thin bin. `pub fn run_conformance(opts) -> Report` for Rust `cargo test` consumers; `[[bin]] atd-conformance` for CLI / non-Rust implementers. |
| Q3 | Fixture format? | B — `fixtures/**/*.json` loaded at runtime. Matches existing `atd-sdk/tests/fixtures/anos_*.json` pattern. |
| Q4 | CI integration? | B — `tests/atd_mvp_self_conformance.rs` spawns ref-server + calls `run_conformance`. Picked up by existing `cargo test --workspace --all-targets` CI step. No YAML changes. |

## 3. Crate shape

New workspace member `atd-conformance`, 12th crate in the workspace.

### 3.1 Directory layout

```
crates/atd-conformance/
├── Cargo.toml
├── src/
│   ├── lib.rs               pub fn run_conformance(opts) -> Report
│   ├── case.rs              ConformanceCase enum + serde loader
│   ├── runner.rs            case execution dispatch
│   ├── wire.rs              low-level send/recv helpers (thin shim over atd-protocol::wire)
│   ├── report.rs            Report + text/json output formatters
│   └── main.rs              [[bin]] CLI wrapper (~30 lines)
├── fixtures/
│   ├── wire/                (~8-10 cases)
│   ├── sanitize/            (~10-12 cases)
│   └── behavior/            (~8-10 cases)
└── tests/
    └── atd_mvp_self_conformance.rs   spawn ref-server + run_conformance against it
```

### 3.2 Dependencies

Regular deps (keep minimal — the crate must be loadable by non-reference Rust consumers):

```toml
[dependencies]
atd-protocol = { path = "../atd-protocol", version = "0.1.0" }
atd-sdk = { path = "../atd-sdk", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
clap = { version = "4", features = ["derive"] }
```

Dev deps (for self-conformance integration test; dev-deps don't
propagate to downstream consumers):

```toml
[dev-dependencies]
atd-ref-server-bin = { path = "../atd-ref-server-bin", version = "0.1.0" }
tempfile = { workspace = true }
```

### 3.3 Dependency discipline

`atd-conformance` does **NOT** depend on `atd-runtime`, `atd-tools-*`, or
`atd-ref-server-bin` as regular deps. It speaks to any ATD server over a
Unix socket via `atd-sdk`; it has no business being coupled to the
reference server implementation.

The `atd-ref-server-bin` dev-dep exists only so the self-conformance
integration test can spawn the binary via a `ref_server_bin()` helper that
derives the path from `std::env::current_exe()`. (The earlier design-stage
sketch used `CARGO_BIN_EXE_<name>`, but that env var only exposes binaries
from the **same** package as the test — it's unset for dev-dep binaries in
other workspace crates. See `crates/atd-conformance/tests/atd_mvp_self_conformance.rs::ref_server_bin`
for the shipped pattern.)

## 4. `ConformanceCase` JSON schema

Serde tagged enum (`#[serde(tag = "category")]`) with three variants.

### 4.1 Rust types

```rust
#[derive(Debug, Deserialize)]
#[serde(tag = "category")]
pub enum ConformanceCase {
    #[serde(rename = "wire")]
    Wire(WireCase),
    #[serde(rename = "sanitize")]
    Sanitize(SanitizeCase),
    #[serde(rename = "behavior")]
    Behavior(BehaviorCase),
}

#[derive(Debug, Deserialize)]
pub struct CaseMeta {
    pub name: String,
    pub description: String,
    #[serde(default = "default_must_pass")]
    pub must: Must,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub enum Must {
    #[serde(rename = "pass")] Pass,
    #[serde(rename = "skip")] Skip,   // currently unused; reserved
}
```

Each `*Case` embeds its own `meta: CaseMeta` plus category-specific fields.

### 4.2 `wire` category — frame round-trip

Example file `fixtures/wire/ping_roundtrip.json`:

```json
{
  "category": "wire",
  "name": "ping_roundtrip",
  "description": "Client sends Request::Ping; server must reply Response::Pong.",
  "must": "pass",
  "send": { "type": "ping" },
  "expect_response_matches": { "type": "pong" }
}
```

`expect_response_matches` semantics: **deep subset match** over JSON. The
server's response must contain every field listed in expect with matching
values; it may have additional fields not listed in expect.

### 4.3 `wire` category — byte-level assertion (rare)

For a handful of cases that verify frame codec itself (BE u32 header,
max-frame handling), an optional `expect_wire_bytes_prefix_hex` field
asserts the first N raw bytes on the wire match:

```json
{
  "category": "wire",
  "name": "frame_length_big_endian_u32",
  "description": "Frame header is u32 big-endian length prefix.",
  "must": "pass",
  "send": { "type": "ping" },
  "expect_wire_bytes_prefix_hex": "00000016"
}
```

The runner intercepts raw bytes at socket level for this case class. Only
applies when `expect_wire_bytes_prefix_hex` is present.

### 4.4 `sanitize` category — pure function, no server

```json
{
  "category": "sanitize",
  "name": "basic_ref_fs_read",
  "description": "Tool id 'ref:fs.read' sanitizes to 'ref_fs_read' for LLM SDKs that forbid colons and dots.",
  "must": "pass",
  "input": "ref:fs.read",
  "expect_sanitized": "ref_fs_read"
}
```

Runner calls `atd_protocol::sanitize::sanitize_tool_name(case.input)` and
asserts equality. These cases don't spawn or contact a server — they
verify the local `atd-protocol` crate's sanitize implementation matches
the spec. Non-Rust implementers consume the same input/output pairs as
test vectors for their own sanitize implementation.

### 4.5 `behavior` category — protocol-level semantics

```json
{
  "category": "behavior",
  "name": "capability_denied_returns_code_1001",
  "description": "Calling a tool whose required_capabilities isn't a subset of granted set must return Response::Error { code: 1001, retryable: false }.",
  "must": "pass",
  "setup": {
    "hello": { "client_id": "conformance-test", "requested_capabilities": [] }
  },
  "send": {
    "type": "run_tool",
    "tool_id": "ref:fs.read",
    "args": { "path": "Cargo.toml" },
    "dry_run": false
  },
  "expect_response_matches": {
    "type": "error",
    "code": 1001,
    "retryable": false
  }
}
```

Optional `setup.hello` performs a Hello handshake before the main send,
putting the connection in a known capability state. Required for any
case that tests capability-scoped behavior.

### 4.6 Deep-subset match rules

The runner's `json_matches_subset(expect, actual) -> bool` implements:

- **Primitive** (string/number/bool/null): literal equality.
- **Object**: every key in `expect` must exist in `actual` and recursively match; extra keys in `actual` are allowed.
- **Array**: `expect.len() == actual.len()`; element-wise recursive match.
- **Wildcard**: the literal string `"*"` in `expect` means "any value present".

Rationale: allows a fixture to assert only the fields it cares about,
ignoring implementation-specific extras like timestamps or request IDs.

### 4.7 Estimated case counts (Q1 scope B)

| Category | Count | Examples |
|---|---|---|
| wire | 8-10 | ping, tool_list shape, tool_schema shape, run_tool success shape, run_tool error shape, BE u32 header, max-frame refusal, hello handshake shape |
| sanitize | 10-12 | basic id, colons+dots, digits-only, length boundary, collision detection (multiple originals → same sanitized), desanitize round-trip |
| behavior | 8-10 | capability_denied 1001, unknown tool id error, hello granted subset, hello requested superset, invalid args shape, dry_run acknowledgment, run_tool on missing tool, malformed request |
| **total** | **~28-32** | ~30 fixture files |

## 5. Runner execution model

### 5.1 Public API (lib)

```rust
pub struct Opts {
    pub target: Endpoint,
    pub filter: Option<String>,
    pub categories: Vec<Category>,
    pub stop_on_first_fail: bool,
}

pub async fn run_conformance(opts: Opts) -> Report;

pub struct Report {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub cases: Vec<CaseResult>,
}

pub struct CaseResult {
    pub name: String,
    pub category: Category,
    pub outcome: Outcome,
    pub duration_ms: u64,
}

pub enum Outcome {
    Pass,
    Fail { reason: String },
    Skip { why: String },
}
```

### 5.2 Per-case execution

```
1. All fixtures loaded at startup into Vec<ConformanceCase>.
   Fail-fast if any file is malformed.
2. For each case:
   a. Apply filter + category + must gates → maybe Skip.
   b. Dispatch by category:
      - sanitize: call atd_protocol::sanitize::sanitize_tool_name directly,
        assert expect_sanitized == output.
      - wire: open a new UnixStream to target; if setup.hello present,
        perform Hello first; send the message; read response; apply
        expect_response_matches (deep-subset); optionally assert
        expect_wire_bytes_prefix_hex.
      - behavior: same as wire but with setup.hello typically present.
   c. Record CaseResult.
   d. If stop_on_first_fail and outcome is Fail, break.
3. Return Report.
```

**Connection discipline**: wire/behavior cases open a **new connection
per case**. This ensures per-connection state (capability set, Hello
handshake record) doesn't leak between cases, and verifies the server's
connection lifecycle implements "each new connection starts fresh" —
itself a spec-normative requirement.

### 5.3 Wire bytes interception (for `expect_wire_bytes_prefix_hex`)

Default path (`expect_response_matches` only): use `atd-sdk::AtdClient`
normally.

Byte-level path: open a raw `tokio::net::UnixStream`, serialize the
request with `atd_protocol::wire::write_frame`, then `peek` or log the
written bytes before they go on the wire. Assert hex-prefix matches.
Response side similar: read bytes directly, check prefix, then
deserialize.

Implementation note: byte-level cases are ≤5% of the suite; keep them in
a separate runner path to avoid complicating the main dispatch loop.

## 6. CLI (thin bin)

`src/main.rs` (~30 lines via `clap` derive):

```bash
atd-conformance [OPTIONS] --target <TARGET>

Options:
  --target <TARGET>        Socket endpoint, e.g. unix:/tmp/atd.sock
  --filter <PATTERN>       Run only cases whose name matches (substring match)
  --category <CATEGORY>    Filter by category. Repeatable. [wire|sanitize|behavior]
  --report <FORMAT>        Output format. [default: text] [possible values: text, json]
  --stop-on-first-fail     Exit after the first failing case
  -h, --help
  -V, --version
```

Exit code: `0` if all cases pass, `1` if any case fails, `2` on loader /
connection failure.

### 6.1 Text output

```
atd-conformance 0.1.0 — target unix:/tmp/atd.sock

[wire]      (10/10 ✓)
  ✓ ping_roundtrip                       1ms
  ✓ tool_list_shape                      3ms
  ...
[sanitize]  (12/12 ✓)
  ✓ basic_ref_fs_read                    0ms
  ...
[behavior]  (7/8 ✗)
  ✓ capability_denied_returns_code_1001  2ms
  ✗ unknown_tool_id_returns_error        1ms
      expected: { "code": null }
      got: { "code": 42, "message": "..." }
  ...

30 cases: 29 passed, 1 failed, 0 skipped  (total 142ms)
```

### 6.2 JSON output

Same data as text, serialized as `Report` with `serde_json::to_string_pretty`.

JUnit XML output is **not implemented** in this SP (see §8 non-goals).

## 7. Self-conformance integration test

`crates/atd-conformance/tests/atd_mvp_self_conformance.rs` — design-stage sketch
(the shipped version replaces `env!("CARGO_BIN_EXE_atd-ref-server")` with a
`ref_server_bin()` helper, see errata note above):

```rust
use atd_conformance::{run_conformance, Opts, Outcome};
use atd_sdk::Endpoint;
use std::process::{Child, Command};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn atd_ref_server_passes_conformance_suite() {
    let sock_dir = tempfile::tempdir().unwrap();
    let sock = sock_dir.path().join("conformance.sock");

    let bin = env!("CARGO_BIN_EXE_atd-ref-server");   // see errata: shipped impl uses ref_server_bin()
    let mut child: Child = Command::new(bin)
        .arg("--sock").arg(&sock)
        .arg("--grant-capability").arg("read")
        .arg("--grant-capability").arg("write")
        .arg("--grant-capability").arg("exec")
        .spawn()
        .expect("spawn atd-ref-server");

    wait_for_socket(&sock, Duration::from_secs(3)).await;

    let report = run_conformance(Opts {
        target: Endpoint::unix(&sock),
        filter: None,
        categories: vec![],
        stop_on_first_fail: false,
    }).await;

    let _ = child.kill();
    let _ = child.wait();

    let failures: Vec<&str> = report.cases.iter()
        .filter_map(|c| match &c.outcome {
            Outcome::Fail { reason } => Some(format!("  - {}: {}", c.name, reason)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(report.failed, 0,
        "{} conformance case(s) failed:\n{}",
        report.failed, failures.join("\n"));
}

async fn wait_for_socket(path: &std::path::Path, timeout: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() { return; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("socket {:?} did not appear within {:?}", path, timeout);
}
```

### 7.1 CI consequence

`atd_mvp_self_conformance` runs automatically as part of `cargo test
--workspace --all-targets` in the existing CI step. No workflow YAML
changes. Every PR that causes the ref-server to drift from the spec
surfaces the regression immediately.

### 7.2 `--grant-capability` flags

The spawned ref-server gets three capabilities (`read`, `write`, `exec`)
by default so that cases testing the "capability granted" path work.
Cases testing the "capability denied" path request a capability the
server didn't grant — the fixture convention is the opaque string
`"conformance.denied"`, which the test harness never passes to
`--grant-capability`. By construction, denied cases always request a cap
the server didn't grant; no fixture depends on `admin` or any other
reserved name.

## 8. Non-goals

| Not doing | Why | When it opens |
|---|---|---|
| JUnit XML report | GitHub Actions consumes cargo test output natively; JSON covers custom CI | An external CI integration demands it |
| JSON schema file for `ConformanceCase` | YAGNI; fixtures serve as templates for contributors | Case count > 50 or IDE autocomplete demand |
| Tier-budget / deadline conformance cases | Tier values are reference defaults, not spec-normative | If the spec fixes tier budgets as normative |
| Middleware observable-effect cases | Middleware is pluggable, not spec-normative | Same |
| CliBinding pathway cases (SP-12 external/uname) | Binding choice is implementation-level | If binding protocol becomes spec-normative |
| Python/JS/Go driver | Each impl's maintainer can write their own driver over the same fixtures | First non-Rust impl appears |
| Machine-readable `atd-protocol-schema.json` | Separate SP; gated on first non-Rust impl | First non-Rust impl or explicit request |
| `Must::Skip` / negative assertion semantics | Initial suite is all-must-pass | Case count grows + optional-capability distinction needed |
| Fuzzing / randomized property tests | Out of scope; those belong in a dedicated fuzzing SP | A security-review SP |

## 9. Success criteria

SP complete when **all** hold:

1. New workspace member `atd-conformance` at `crates/atd-conformance/`; 4-gate clean (`cargo fmt --check` + `cargo clippy --workspace --all-features -- -D warnings` + `cargo test --workspace --all-targets` + `cargo build --release --workspace`).
2. `cargo run -p atd-conformance -- --target unix:/tmp/x.sock` (against a running ref-server) reports all cases pass.
3. `cargo test -p atd-conformance` includes the `atd_ref_server_passes_conformance_suite` integration test and passes.
4. Total case count in the 28-32 range; all three categories represented; at least 1 byte-level wire case.
5. Fixture loader fails fast on malformed JSON: a broken fixture file causes `run_conformance` to return a loader error rather than silently skipping.
6. `--report text` and `--report json` both work on CLI.
7. Workspace total test count increases by at least 1 (the self-conformance integration test).
8. Zero changes to `atd-protocol`, `atd-sdk`, `atd-runtime`, `atd-tools-*`, or `atd-ref-server-bin` public API.
9. No `.github/workflows/ci.yml` changes required.
10. SP tagged `sp-8-conformance-suite` on completion.

## 10. Rollback

Before starting: `git tag pre-sp-8-conformance-suite` on current HEAD.
Each commit independently revertible. Worst case: `git reset --hard
pre-sp-8-conformance-suite` removes the entire `atd-conformance/` crate
+ workspace member entry.

## 11. Next steps unlocked

- **`atd-protocol-schema.json` SP**: conformance fixtures become the
  reference implementation that the schema generator is validated
  against.
- **Non-Rust ATD implementations**: any future Go / Java / C++ / Python
  ATD server can be validated by running `atd-conformance --target
  unix:/path/to/their.sock`.
- **Published conformance badge**: crates.io listing for `atd-conformance`
  lets an adopter advertise "verified against ATD conformance v0.1.0".
