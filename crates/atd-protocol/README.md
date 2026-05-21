# atd-protocol

Wire types, codec, and sanitization rules for the
[Agent Tool Dispatch (ATD) protocol](https://github.com/downsea/atd).

This is the schema crate — the Rust source of the ATD wire format. Every other
crate in the workspace depends on it. It carries no logic beyond
serialization, framing, and tool-name sanitization, so it can be shared by
both client (`atd-sdk`) and server (`atd-runtime`) sides without pulling in a
transport.

## What's in here

- `ToolDefinition` — full metadata for a tool (id, schema, safety, trust,
  resources, bindings)
- `ToolSummary` — compact form returned by `discover`
- `ToolResult` — success + error variants of a tool-call outcome
- `ToolSafety`, `ToolCapability`, `ToolTrust`, `ToolBinding`, `ToolResources`,
  `ToolErrorDef` — sub-structures
- Enums: `SafetyLevel`, `ToolVisibility`, `TrustLevel`, `BindingProtocol`,
  `ToolTier`
- `Request` / `Response` — the top-level wire envelope, plus the `ERR_*` error
  code constants
- `AtdError` — the protocol error taxonomy
- `read_frame` / `write_frame` (+ deadline variants) — the length-prefixed
  framing codec
- `sanitize_tool_name` / `desanitize_tool_name` / `detect_collisions` — the
  tool-id sanitization rules

All types are `serde`-compatible with the ATD wire format (length-prefixed
JSON). The `schema` feature derives `schemars::JsonSchema` and powers the
checked-in `atd-protocol-schema.json` build artifact.

## Quick example

```rust
use atd_protocol::{ToolSafety, SafetyLevel};

let safety = ToolSafety {
    level: SafetyLevel::Read,
    dry_run: false,
    side_effects: vec![],
    data_sensitivity: None,
};
```

## Related crates

- [`atd-sdk`](https://crates.io/crates/atd-sdk) — Rust client SDK
- [`atd-runtime`](https://crates.io/crates/atd-runtime) — server-side runtime
- [`atd-mcp-bridge`](https://crates.io/crates/atd-mcp-bridge) — MCP bridge binary

The byte-level contract is documented in
[`docs/protocol/wire-format.md`](https://github.com/downsea/atd/blob/master/docs/protocol/wire-format.md)
and [`docs/protocol/error-codes.md`](https://github.com/downsea/atd/blob/master/docs/protocol/error-codes.md).

## License

Apache-2.0. See [LICENSE](https://github.com/downsea/atd/blob/master/LICENSE).
</content>
