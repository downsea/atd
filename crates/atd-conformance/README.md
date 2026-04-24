# atd-conformance

Cross-implementation conformance suite for the ATD (Agent Tool Dispatch) protocol.

Any server that speaks ATD over a Unix socket can be validated with:

    atd-conformance --target unix:/path/to/server.sock

For the Rust SDK consumer path, depend on this crate as a dev-dep and call
`atd_conformance::run_conformance(opts)` from an integration test.

See the [SP-8 design doc](../../docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md)
for scope, fixture format, and how to contribute new cases.
