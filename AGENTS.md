# AGENTS.md

The authoritative guide for any AI coding agent — or human — working in the
**`atd`** repository. A fresh clone should be able to read this file and then
implement, verify, and extend ATD without further context.

> This file is the tracked, canonical agent entry point. (`CLAUDE.md`, if
> present, is a local-only working file and is git-ignored — do not rely on it.)

---

## 1. What this repository is

`atd` is the **reference implementation of the ATD (Agent Tool Dispatch)
protocol** — a neutral, cross-vendor wire protocol that lets any LLM agent, on
any framework, call any tool, on any platform, through a single typed RPC
surface.

The repository is a Rust workspace (16 crates) plus a Python package mirror.
It ships: the wire-type crate, a server runtime, a client SDK, two transports
(Unix socket + HTTP), middleware crates, built-in tools, an MCP bridge, a CLI,
a conformance suite, and a reference server binary.

**Authoritative architecture:** [`docs/atd-architecture.md`](docs/atd-architecture.md).
Read it before making any non-trivial change.

**Hard rule — the ANOS boundary.** ATD is a *neutral* protocol. This repository
must have **zero runtime dependency on any `anos-*` crate**. Pattern inspiration
is fine; a `[dependencies]` entry is not. Never add one.

---

## 2. Start here — reading order

| Order | Document | Why |
|---|---|---|
| 1 | [`docs/index.md`](docs/index.md) | The doc map — what every document is and when to read it. |
| 2 | [`docs/atd-architecture.md`](docs/atd-architecture.md) | The normative architecture: layers, dispatch, security, middleware, crate map. |
| 3 | [`docs/protocol/wire-format.md`](docs/protocol/wire-format.md) · [`error-codes.md`](docs/protocol/error-codes.md) | The byte-level wire contract + error taxonomy. |
| 4 | [`docs/extending/`](docs/extending/) | How to add a tool, binding, middleware, transport, etc. — read the one matching your task. |
| 5 | This file, §4–§6 | Build / test / verify commands and conventions. |

For implementers writing an ATD SDK or server in another language, the
authoritative pair is the wire format (§3 above) plus
[`/atd-protocol-schema.json`](atd-protocol-schema.json).

---

## 3. Repository map

```
atd/
├── AGENTS.md            ← you are here
├── README.md            project overview / quick start
├── CONTRIBUTING.md      contributor guide + full build/test/verify SOP
├── CHANGELOG.md         release history (truth for what changed)
├── Cargo.toml           workspace root; workspace.package.version
├── atd-protocol-schema.json   generated wire schema (build artifact, checked in)
├── crates/              the 16-crate Rust workspace
├── python/              Python package mirror (atd_client + atd_server)
├── examples/            runnable examples (atd-examples crate)
├── docs/                all documentation — start at docs/index.md
└── scripts/             developer helper scripts
```

### Crates

| Crate | Layer | Purpose |
|---|---|---|
| `atd-protocol` | Schema | Wire types, codec, sanitize. The schema's Rust source. |
| `atd-sdk` | Client | Rust client API: discover / describe / call / call_page / call_all / hello. |
| `atd-runtime` | Server core | `Tool`, `Registry`, dispatch, `Binding`, `Middleware`, `TokenBroker`, `AuditSink`, `CursorIssuer`, UCAN verifier. Transport-agnostic. |
| `atd-server` | Transport | Unix-socket listener. |
| `atd-server-http` | Transport | HTTP listener + MCP JSON-RPC translator + bearer auth + SSE refresh. |
| `atd-middleware-fhir` | Middleware | FHIR R4 egress validation. |
| `atd-middleware-pii-redact-medical` | Middleware | HIPAA Safe Harbor PHI redaction. |
| `atd-tools-echo` / `-fs` / `-shell` / `-web` | Built-in tools | Reference `Tool` implementations. `atd-tools-echo` is the documented template. |
| `atd-cli` | Binary | Reference CLI client — the `atd` command. |
| `atd-ref-server` | Binary | Reference server binary wiring runtime + tools + Unix server. |
| `atd-mcp-bridge` | Binary | MCP-over-stdio gateway to any ATD server. |
| `atd-conformance` | Test suite + bin | Reusable conformance scenarios; adopters dev-dep on it. |
| `atd-mock-weather-server` | Binary (`publish = false`) | Cross-vendor composition demo helper. |

All publishable crates share `workspace.package.version` — the workspace ships
as one coordinated version. See [`docs/atd-architecture.md`](docs/atd-architecture.md) §9.

---

## 4. Build, test, verify

MSRV is **Rust 1.85**, edition 2024. The four workspace gates — **all must pass
before any commit**:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo nextest run --workspace          # or: cargo test --workspace --all-targets
cargo build --release --workspace
```

Plus, when you touch wire types in `crates/atd-protocol/`:

```bash
cargo run -p atd-protocol --features schema --bin gen-schema -- --check
```

This regenerates [`atd-protocol-schema.json`](atd-protocol-schema.json) and
fails if the committed file drifts from the Rust types. CI runs the same check.

The full step-by-step verification SOP — when each gate applies, how to run the
conformance suite, the Python gates — is in
[`CONTRIBUTING.md`](CONTRIBUTING.md).

### Test discipline (read before running the workspace suite)

The test suite spawns real listeners and recompiles ~10 downstream crates when
`atd-runtime` changes. To keep the host responsive:

1. **Iterate narrow.** Use `cargo test -p <crate> --lib <module>` while
   developing; run the whole workspace only as the pre-commit gate.
2. **One workspace run at a time.** A workspace build/test can take tens of
   seconds — do not launch a second concurrently.
3. **Prefer `cargo nextest run --workspace`.** The repo ships
   `.config/nextest.toml` (caps test-threads at 4); nextest isolates
   bind-listener integration tests so one panic can't poison siblings.
4. Integration tests (`e2e_bearer_*`, `ucan_*_via_http`, conformance
   scenarios) bind real ports. A flaky `EADDRINUSE` → re-run with
   `--test-threads=4`.

### Quick run

```bash
cargo run --example hello_atd -p atd-examples   # auto-spawns the ref server
```

---

## 5. How to extend ATD

ATD is designed so third-party code attaches **without forking** the reference
server. Each extension point is a `pub` trait in `atd-runtime` with a how-to
guide. Read the guide for your task — it gives the trait signature, a reference
implementation to copy, the test pattern, and the invariants the extension must
preserve.

| You want to… | Guide | Reference impl |
|---|---|---|
| Add a built-in tool | [`docs/extending/tool.md`](docs/extending/tool.md) | `crates/atd-tools-echo` |
| Add an invocation binding | [`docs/extending/binding.md`](docs/extending/binding.md) | `NativeBinding`, `CliBinding` in `atd-runtime` |
| Add result middleware | [`docs/extending/middleware.md`](docs/extending/middleware.md) | `atd-middleware-fhir` |
| Add a transport / listener | [`docs/extending/transport.md`](docs/extending/transport.md) | `atd-server`, `atd-server-http` |
| Add an auth / secret scheme | [`docs/extending/token-broker.md`](docs/extending/token-broker.md) | `FileTokenBroker` in `atd-runtime` |
| Add an audit sink | [`docs/extending/audit-sink.md`](docs/extending/audit-sink.md) | `JsonLinesAuditSink` in `atd-runtime` |
| Add a wire type / error code / capability | [`docs/extending/protocol-and-schema.md`](docs/extending/protocol-and-schema.md) | `crates/atd-protocol` |

Changing the wire format itself, or adding a `ToolTier` variant, is **not** an
extension point — it is a protocol change. See
[`docs/extending/protocol-and-schema.md`](docs/extending/protocol-and-schema.md)
and [`docs/atd-architecture.md`](docs/atd-architecture.md) §9.3.

---

## 6. Conventions

### Documentation authority

When two documents disagree, the higher tier wins:

| Tier | Documents | Role |
|---|---|---|
| **Normative** | `docs/atd-architecture.md`, `docs/protocol/*`, `atd-protocol-schema.json` | The contract. Source of truth. |
| **Policy** | `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `docs/release-plan-v1.0.md` | How the project runs. |
| **How-to** | `docs/extending/*`, `docs/quickstart/*`, `docs/integrations/*` | Task guides. |
| **Archive** | `docs/archive/*` | Frozen history. **Never** authoritative; never edit. |

### Commits

Conventional commits — `feat`, `fix`, `refactor`, `docs`, `test`, `chore`,
`perf`, `ci` — scoped to the crate or area:

```
feat(atd-sdk): add stdio transport
fix(atd-protocol): correct ToolSummary serde
docs(extending): add the middleware guide
```

Do not add `Co-Authored-By` footers unless explicitly asked.

### Forward design process

Post-1.0, new work is recorded as:

- **Decisions** → an ADR in [`docs/adr/`](docs/adr/) (Context / Options /
  Decision / Rationale).
- **How-to** → a guide in [`docs/extending/`](docs/extending/).
- **Tracked gaps** → an issue in [`docs/issues/`](docs/issues/).

The historical Superpowers (SP) spec/plan process is archived under
[`docs/archive/superpowers/`](docs/archive/superpowers/) — read-only history,
not a live workflow.

### What not to do

- Do not add an `anos-*` dependency (see §1).
- Do not edit anything under `docs/archive/` — it is frozen.
- Do not hand-edit `atd-protocol-schema.json` — it is generated (§4).
- Do not commit secrets; `RedactedString` exists because credentials must never
  reach a log or the wire.
