# atd-runtime

Server-side runtime for the [Agent Tool Dispatch (ATD) protocol](https://github.com/downsea/atd-mvp).

This crate provides the building blocks for hosting ATD tools:

- `Tool` trait — implement once per tool
- `Registry` — register tools and dispatch incoming calls
- `Binding` (`NativeBinding`, `CliBinding`, future `McpBinding`) — adapter between a `Tool` and the runtime
- `Middleware` — pre/post-call interceptors (audit, redact, rate-limit are built-in)
- `CapabilityGate` — checks `required_capabilities` against the caller's grants

If you want to **build** an ATD-speaking server, use this crate.
If you want to **call** an ATD server, use [`atd-sdk`](https://crates.io/crates/atd-sdk).

See `docs/architecture.md` §4 (Dispatch Layer) in the repository for the conceptual model.

## License

Apache-2.0.
