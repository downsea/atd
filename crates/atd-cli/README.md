# atd-cli

Reference command-line client for the
[ATD (Agent Tool Dispatch) protocol](https://github.com/downsea/atd). It wraps
the [`atd-sdk`](https://crates.io/crates/atd-sdk) client surface in an `atd`
binary for interactive use and scripting.

## Install

```bash
cargo install atd-cli
```

This installs the `atd` binary.

## Usage

```bash
atd list   --sock /tmp/atd.sock                          # discover tools
atd schema ref:echo.say --sock /tmp/atd.sock             # show a tool's schema
atd call   ref:echo.say --sock /tmp/atd.sock --args '{"text":"hello"}'
atd doctor --sock /tmp/atd.sock                          # check connectivity
atd skills sync --sock /tmp/atd.sock                     # pull skill files
```

Subcommands: `list` (discover), `schema` (describe), `call` (invoke), `doctor`
(connectivity check), `skills` (sync skill files from a connected server).
`--sock` overrides the default socket path. Run `atd --help` and
`atd <subcommand> --help` for the full surface.

## See also

- [`atd-sdk`](https://crates.io/crates/atd-sdk) — the underlying Rust SDK
- [`atd-ref-server`](https://crates.io/crates/atd-ref-server) — a server the
  CLI can talk to out of the box
- [`docs/cli.md`](https://github.com/downsea/atd/blob/master/docs/cli.md) — the
  full CLI guide

## License

Apache-2.0.
</content>
