# atd-protocol

Protocol types for the [Agent Tool Dispatch (ATD) protocol](https://github.com/downsea/atd-mvp).

## What's in here

- `ToolDefinition` — full metadata for a tool (id, schema, safety, trust, bindings)
- `ToolSummary` — compact form returned by `discover`
- `ToolResult` — success + error variants of a tool call outcome
- `ToolSafety`, `ToolCapability`, `ToolTrust`, `ToolBinding` — sub-structures
- Enums: `SafetyLevel`, `ToolVisibility`, `TrustLevel`, `BindingProtocol`

All types are `serde`-compatible with the ATD wire format (length-prefixed JSON over Unix sockets).

## Quick example

```rust
use atd_protocol::{ToolSummary, ToolSafety, SafetyLevel};

let safety = ToolSafety {
    level: SafetyLevel::Read,
    dry_run: false,
    side_effects: vec![],
    data_sensitivity: None,
};
```

## Related crates

- [`atd-sdk`](https://crates.io/crates/atd-sdk) — client SDK for Rust agents
- [`atd-mcp-bridge`](https://crates.io/crates/atd-mcp-bridge) — MCP bridge binary

## License

Apache-2.0. See [LICENSE](https://github.com/downsea/atd-mvp/blob/master/LICENSE).
