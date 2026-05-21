# Contributing to `atd`

Thanks for contributing to the ATD (Agent Tool Dispatch) reference
implementation. This document is the build / test / verify standard operating
procedure. For the architecture, read [`docs/architecture.md`](docs/architecture.md)
first; for the documentation map, [`docs/index.md`](docs/index.md).

AI coding agents: [`AGENTS.md`](AGENTS.md) is your entry point — it summarises
this file plus the repo map and conventions.

## How to help

- **Bug reports** — open an issue with a minimal repro. For a misbehaving tool
  call, include the exact JSON request and response.
- **New tools / extensions** — follow the relevant guide in
  [`docs/extending/`](docs/extending/). Every extension point is a `pub` trait
  with a worked example; no fork required.
- **Third-party implementations** — if you implement an ATD SDK or server in
  another language, run it against [`crates/atd-conformance`](crates/atd-conformance)
  and tell us — we link interoperable implementations from the README.
- **Design feedback** — read [`docs/architecture.md`](docs/architecture.md) and
  the ADRs under [`docs/adr/`](docs/adr/), and push back on anything wrong.

## Toolchain

- **Rust** 1.85 (MSRV), edition 2024. `rust-toolchain.toml` pins it.
- **Python** (for the `python/` mirror) — use [`uv`](https://docs.astral.sh/uv/).
- Optional but recommended: [`cargo-nextest`](https://nexte.st/)
  (`cargo install cargo-nextest --locked`).

```bash
git clone https://github.com/downsea/atd
cd atd
cargo build --workspace
```

## The verification SOP

Four workspace gates. **All must pass before any commit** — CI runs the same.

```bash
cargo fmt --all -- --check                                  # 1. formatting
cargo clippy --workspace --all-features -- -D warnings      # 2. lints
cargo nextest run --workspace                               # 3. tests
cargo build --release --workspace                           # 4. release build
```

Gate 3 fallback without nextest: `cargo test --workspace --all-targets`.

### When you change wire types (`crates/atd-protocol/`)

Regenerate and re-check the schema — CI fails on drift:

```bash
cargo run -p atd-protocol --features schema --bin gen-schema          # regenerate
cargo run -p atd-protocol --features schema --bin gen-schema -- --check  # verify
```

Commit the updated [`atd-protocol-schema.json`](atd-protocol-schema.json)
alongside the type change. See
[`docs/extending/protocol-and-schema.md`](docs/extending/protocol-and-schema.md).

### When you change dispatch, a transport, or wire behaviour

Run the conformance suite — it verifies any ATD-speaking server against the
protocol's contractual behaviours:

```bash
cargo nextest run -p atd-conformance
```

To add or understand a conformance scenario, see
[`crates/atd-conformance/README.md`](crates/atd-conformance/README.md).

### Python mirror (`python/`)

```bash
cd python
uv sync
uv run ruff check .
uv run mypy src
uv run pytest
```

### Test discipline

The workspace test suite spawns real listeners and recompiles ~10 downstream
crates when `atd-runtime` changes. To keep iteration fast and the host responsive:

1. **Iterate narrow** — `cargo test -p <crate> --lib <module>` while developing;
   run the whole workspace only as the pre-commit gate.
2. **One workspace run at a time** — a workspace build/test takes tens of
   seconds; do not launch a second concurrently.
3. **Prefer `cargo nextest run --workspace`** — `.config/nextest.toml` caps
   test-threads at 4 and isolates bind-listener integration tests so one panic
   cannot poison siblings.
4. **Flaky `EADDRINUSE`** in integration tests (`e2e_bearer_*`,
   `ucan_*_via_http`, conformance scenarios bind real ports) — re-run with
   `--test-threads=4`.

## Coding style

- `cargo fmt` before committing (rustfmt defaults).
- Handle errors explicitly; never silently swallow. Validate input at system
  boundaries.
- Prefer many small, focused files over few large ones.
- If you change a crate's public API, add a test that exercises the new surface.
- **The ANOS boundary** — this repository must have **zero runtime dependency on
  any `anos-*` crate**. Pattern inspiration is fine; a `[dependencies]` entry is
  not.

## Commits

Conventional commits, scoped to the crate or area:

```
feat(atd-sdk): add stdio transport
fix(atd-protocol): correct ToolSummary serde
docs(extending): add the middleware guide
```

Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`. One
commit per logical change. Do not add `Co-Authored-By` footers unless asked.

## Recording decisions

Post-1.0, design decisions are recorded as ADRs in [`docs/adr/`](docs/adr/)
(Context / Options / Decision / Rationale). Tracked gaps live in
[`docs/issues/`](docs/issues/). The historical Superpowers (SP) spec/plan
process is frozen under [`docs/archive/superpowers/`](docs/archive/superpowers/).

## License

By contributing you agree your contributions are released under the
[Apache-2.0 license](LICENSE).
