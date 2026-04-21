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

The `dry_run: false` branch of the daemon's `ClientMessage::RunTool` handler at `/home/nan/proj/anos/crates/anos-cli/src/daemon.rs:1566-1614` is hard-coded to emit the error above. `dry_run: true`, `tool_list`, and `tool_schema` all work correctly.

The `anos-tool-dispatch` crate already exposes `Registry::dispatch(agent_did, tool_id, args, ...)` at `/home/nan/proj/anos/crates/anos-tool-dispatch/src/registry.rs:419` — the fix is to route the `else` branch there instead of the stub.

## Tracked in ANOS

Full fix proposal, design decisions, and verification steps live in the ANOS repo:
**`/home/nan/proj/anos/docs/issues/2026-04-21-run-tool-ipc-stubbed.md`**

## Workaround for Phase 0.5

None — validation scope narrowed to discovery only. Documented in `docs/validation/2026-04-21-hermes-e2e.md`.
