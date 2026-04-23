# Contributing to atd-mvp

Thanks for considering a contribution. This is an early-stage project; the
codebase is small, the design is evolving, and external input is welcome.

## How to help

- **Bug reports** — open an issue with a minimal repro. If a tool call
  misbehaves, include the exact JSON request and response.
- **Design feedback** — read [`docs/design.md`](docs/design.md) and the
  per-SP specs under [`docs/superpowers/specs/`](docs/superpowers/specs/).
  Push back on anything that looks wrong; the protocol is still pre-1.0.
- **New tools** — the reference server has 9 tools across 4 domains. Add
  one in a similar pattern (see `crates/atd-ref-server/src/tools/` for
  examples). TDD required: unit test + integration test.
- **Third-party server implementations** — the ATD wire format is
  straightforward. If you implement a server and it interoperates with
  `atd-client`, we'd love to link to it from the README.

## Development

```bash
git clone https://github.com/<YOUR_USERNAME>/atd-mvp
cd atd-mvp
cargo build --workspace
cargo test --workspace --all-targets
```

All 250+ tests should pass. CI runs the same command on every push.

## Coding style

- Rust 2024 edition, MSRV 1.85
- `cargo fmt` before committing (rustfmt default config)
- One commit per logical change; use conventional commits
  (`feat:`, `fix:`, `docs:`, `chore:`, `test:`, etc.)
- If you touch a crate's public API, add a test that exercises the new
  surface

## License

By contributing you agree your contributions will be released under the
[Apache-2.0 license](LICENSE).
