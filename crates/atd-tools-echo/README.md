# atd-tools-echo

The echo tool — the smallest real `Tool` implementation in the
[ATD (Agent Tool Dispatch)](https://github.com/downsea/atd) workspace, and the
**documented template for writing new tools**.

## Tool provided

| Tool id | Purpose |
|---|---|
| `ref:echo.say` | Echoes the input args back verbatim under `result.echoed`. A framework test anchor — its no-transformation behaviour makes it ideal for wire round-trip tests and documentation examples. |

## Usage

Pair this crate with [`atd-runtime`](https://crates.io/crates/atd-runtime) to
register the echo tool in your own server:

```rust
use atd_tools_echo::EchoTool;
use atd_runtime::registry::Registry;
use std::sync::Arc;

let mut registry = Registry::new();
registry.register(Arc::new(EchoTool::new()));
```

The reference server
[`atd-ref-server`](https://crates.io/crates/atd-ref-server) already wires this
in.

## The template for new tools

`EchoTool` is the canonical starting point when adding a built-in tool: one
`OnceLock<ToolDefinition>`, an `impl Tool`, and unit tests in the same file.
For the step-by-step walkthrough — the `ToolDefinition` fields, the
`CallContext`, registering, testing, and the invariants a tool must preserve —
see [`docs/extending/tool.md`](https://github.com/downsea/atd/blob/master/docs/extending/tool.md).

## License

Apache-2.0.
</content>
