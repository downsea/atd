# Binding type-surface overstates runtime capability

**Layer:** dispatch
**Status:** tracked
**Effort:** ~1 day (decision + implementation)
**Filed:** 2026-04-24

## Summary

`ToolBinding` + `BindingProtocol` (`Cli`, `Mcp`, `Rest`, `AppFunction`)
exist as first-class protocol types and appear in every registered tool's
definition. They imply that ATD can route the same logical tool through
different implementations depending on context. **In the ref-server
runtime, every tool is a single `impl Tool` in Rust, and the binding
field is informational only** — the dispatcher does not examine it.

## Current state

`crates/atd-ref-server/src/tools/*/`: every built-in tool hardcodes its
binding list to exactly one entry:

```rust
bindings: vec![ToolBinding {
    protocol: BindingProtocol::Cli,
    config: serde_json::json!({}),
}],
```

`Registry::dispatch()` routes by tool id → `Arc<dyn Tool>`; it never
reads the binding list. The `CallOptions::preferred_binding` field
also gets dropped (see companion issue
`2026-04-24-dispatch-preferred-binding-ignored.md`).

## Gap

- No multi-binding tools exist in ref-server (or anywhere)
- Dispatcher has no branch on `BindingProtocol`
- `REST`, `AppFunction` variants are never exercised
- A client writing `preferred_binding: BindingProtocol::Mcp` would see
  identical behavior to not setting it at all

## Impact

- **Misleading documentation:** every reader who encounters `ToolBinding`
  assumes dynamic multi-protocol routing exists
- **Schema/code drift:** future ATD clients built from
  `atd-protocol-schema.json` (once it exists — see
  `2026-04-24-schema-protocol-machine-readable-missing.md`) will
  type-generate `BindingProtocol` as a live feature
- **Conformance suite confusion:** SP-8 will have to either test the
  binding mechanism (which doesn't exist) or explicitly skip it

## Decision required

Pick one:

**Option A — Implement real binding dispatch.**
- Registry maps `(tool_id, BindingProtocol)` → implementation instead of
  `tool_id` → implementation
- Each tool can register ≥2 backends (e.g., `fs.read` via Cli + Mcp)
- `CallOptions::preferred_binding` actually selects
- Effort: ~2 days; opens design questions (capability negotiation, cost
  model, fallback order)

**Option B — Remove the multi-binding fiction from v0.1.x.**
- Keep `BindingProtocol::Cli` as a literal singleton
- Deprecate or remove `Rest`, `Mcp`, `AppFunction` variants from the
  enum
- Remove `CallOptions::preferred_binding` from the public API
- Add `ToolBinding` back when a concrete multi-backend adopter
  materializes
- Effort: ~0.5 day

**Option C — Document the aspirational status honestly.**
- Add docstring on `ToolBinding` / `BindingProtocol` explicitly stating
  "v0.1.x: informational only; dispatcher does not route on binding"
- Leave type surface intact as a preview of the Phase 2 shape
- Effort: ~1 hour

**Recommendation:** Option C for v0.1.x (honest, cheap). Revisit
between v0.1 and v1.0 once real adopters have multi-backend needs.

## Related

- `crates/atd-types/src/enums.rs` (BindingProtocol)
- `crates/atd-types/src/tool.rs` (ToolBinding)
- `crates/atd-ref-server/src/registry.rs` (dispatch)
- Companion: `2026-04-24-dispatch-preferred-binding-ignored.md`
