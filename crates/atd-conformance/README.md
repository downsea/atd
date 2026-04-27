# atd-conformance

Cross-implementation conformance suite for the ATD (Agent Tool Dispatch) protocol.

Any server that speaks ATD over a Unix socket can be validated with:

    atd-conformance --target unix:/path/to/server.sock

For the Rust SDK consumer path, depend on this crate as a dev-dep and call
`atd_conformance::run_conformance(opts)` from an integration test.

See the [SP-8 design doc](../../docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md)
for scope, fixture format, and how to contribute new cases.

## Fixture-format extensions

### `expect_tools_exclude` (behavior cases)

Optional array of tool ids that MUST NOT appear in the response's `tools`
field. Only meaningful for `tool_list` responses; if the response has no
`tools` array, the assertion fails with a clear reason.

```json
{
  "category": "behavior",
  "name": "hidden_visibility_excludes_from_tool_list",
  "send": { "type": "tool_list" },
  "expect_response_matches": { "type": "tool_list", "tools": "*" },
  "expect_tools_exclude": ["ref:conformance.hidden_op"]
}
```

Used to verify visibility filters (e.g., that `ToolVisibility::Hidden`
tools are excluded from discover). Added in SP-tool-visibility-hidden.
