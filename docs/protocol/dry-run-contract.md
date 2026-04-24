# Dry-run semantics (v1)

`Request::RunTool { dry_run: true }` is a **server-side short-circuit**
in v1. When a client sends `dry_run: true`, the server returns a
synthetic `tool_result` without invoking the tool:

```json
{
  "type": "tool_result",
  "tool_id": "<requested>",
  "success": true,
  "dry_run": true,
  "result": {
    "dry_run": true,
    "tool_id": "<requested>",
    "args_preview": <args echoed back>
  }
}
```

This is **uniform across all tools**. The tool is never invoked; no
binding (`NativeBinding` or `CliBinding`) runs.

## Interpretation of `ToolSafety.dry_run`

The `ToolSafety.dry_run: bool` field on each tool's `ToolDefinition`
is **informational**: it signals whether the tool *could in principle*
support a meaningful preview. It is metadata for clients, schema
generators, and future dispatch versions — the v1 server does not
read it.

### When to declare `dry_run: true`

Declare `true` if invoking the tool has side effects:
- Filesystem writes (fs.write, fs.edit)
- Subprocess execution (shell.exec, shell.pwsh)
- HTTP POST/PUT/DELETE (none in v1 — web.fetch is GET-only)

Declare `false` for read-only tools (echo, fs.read, fs.glob, fs.grep,
web.fetch, external.uname).

## Agent-side contract

Agents that rely on preview fidelity MUST NOT assume the `result`
field of a v1 dry-run response reflects tool-specific semantics. A
future SP (SP-operability-v2 candidate) may route `dry_run: true`
to tools declaring `ToolSafety.dry_run: true` and allow them to
return meaningful previews. At that point, version-gated clients
will need to branch on `schema_version` in the audit event (see
`docs/protocol/audit-events.md` when it lands) or on a new
`Response` field.

## Audit event correlation

A `dry_run: true` call emits a `CallEvent` with:
- Top-level `dry_run: true` field
- `outcome: { "kind": "success" }`

Operators wanting to distinguish real calls from dry-run drills in log
queries should match on the top-level `dry_run` flag rather than on
outcome. Example `jq`:

```bash
jq 'select(.dry_run == false) | .tool_id' audit.jsonl
```

## Forward-compatibility notes

- `ToolSafety.dry_run` becoming actionable in a future SP is a
  **non-breaking** wire change — clients that ignore it today keep
  working.
- The synthetic `result.args_preview` field in v1 short-circuit
  responses is **not** part of the stable contract; future v2
  dispatch that delegates to tools will replace it with
  tool-specific preview content.
