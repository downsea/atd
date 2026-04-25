# atd-cli

Reference command-line client for the [ATD protocol](https://github.com/downsea/atd-mvp).

## Install

```bash
cargo install atd-cli
```

This installs the `atd` binary.

## Usage

```bash
atd discover --sock /tmp/atd.sock
atd describe ref:echo.say --sock /tmp/atd.sock
atd call ref:echo.say --sock /tmp/atd.sock --args '{"text":"hello"}'
```

For the full surface, run `atd --help` and `atd <subcommand> --help`.

## See also

- [`atd-sdk`](https://crates.io/crates/atd-sdk) — the underlying Rust SDK
- [`atd-ref-server`](https://crates.io/crates/atd-ref-server) — a server the
  CLI can talk to out of the box

## License

Apache-2.0.
