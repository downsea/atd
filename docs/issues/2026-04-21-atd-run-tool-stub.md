# ANOS `run_tool` IPC handler is stubbed

**Status:** open
**Discovered:** 2026-04-21 during Phase 0.5 Hermes validation
**Affects:** ANOS daemon (any version as of 2026-04-21)
**Blocks:** Phase 0 Exit Criterion §7.1 ("Demo video: LangChain agent cross-process-calls fs.read via atd-client → ANOS daemon"), Phase 0.5 end-to-end tool execution

## Symptom

Sending a `{"type":"run_tool","tool_id":"anos:system.time","args":{},"dry_run":false}` message to the ANOS daemon on `~/.anos/anos.sock` returns:

```json
{"type":"error","message":"direct tool execution via IPC not yet supported — use RunTurn"}
```

This means the ATD `call` API cannot actually invoke tools in Phase 0/1. `discover` and `describe` work correctly.

## Impact on atd-mvp

- `atd-client::call()` surfaces this as `AtdError::ToolExecutionFailed` with the server's message, per design. No client-side bug.
- `atd-mcp-bridge::tools/call` correctly forwards the error as MCP `isError=true`. No bridge-side bug.
- A real Hermes/OpenClaw/LangChain demo with the LLM actually calling a tool will fail at the execution step. This blocks the Goal A demo video.

## Root cause (in ANOS)

The `run_tool` arm of the ANOS daemon's IPC handler (see `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs` `ClientMessage::RunTool`) is currently routed to a placeholder that says "use RunTurn". Tools are only invocable through the full LLM turn loop, not via direct dispatch.

## Proposed fix (ANOS-side)

Wire `ClientMessage::RunTool` to the existing `anos-tool-dispatch` crate's `dispatch_tool()` entry point, bypassing the turn loop. Return the resulting `ToolResult` as `DaemonMessage::ToolResultResponse`. Scope: ~30 lines in the daemon's message handler. This is ANOS-project work and should be tracked in ANOS's own issue list once atd-mvp's governance transfer (see atd-mvp CLAUDE.md open question #5) stabilizes.

## Workaround for Phase 0.5

None — validation scope narrowed to discovery only. Documented in `docs/validation/2026-04-21-hermes-e2e.md`.
