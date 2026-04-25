# atd-tools-fs

Built-in filesystem tools (`ref:fs.read`, `ref:fs.write`, `ref:fs.edit`,
`ref:fs.glob`, `ref:fs.grep`) for the ATD reference runtime.

Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime) in your own
server, or use [`atd-ref-server`](https://crates.io/crates/atd-ref-server)
which has these tools registered out of the box.

Path safety is enforced by the runtime's capability gate; see
[`docs/protocol/wire-format.md`](https://github.com/downsea/atd-mvp/blob/master/docs/protocol/wire-format.md).

## License

Apache-2.0.
