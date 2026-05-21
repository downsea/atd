# `CallOptions.dry_run` partially honored; inconsistent per-tool

**Layer:** security
**Status:** tracked
**Effort:** ~0.5 day
**Filed:** 2026-04-24

## Summary

`CallOptions.dry_run: bool` reaches the server through the wire. Each
tool is supposed to implement a dry-run preview that returns what the
tool *would* do without side effects. In practice, support is
incomplete and inconsistent: some tools honor the flag, others ignore
it silently, none surface "dry-run not supported" explicitly.

## Current state

Per-tool behavior (based on source inspection):

| Tool | dry_run support | Behavior with `dry_run: true` |
|---|---|---|
| `ref:echo.say` | partial | Echoes regardless |
| `ref:fs.read` | partial | Read is side-effect-free; dry-run is identity |
| `ref:fs.glob` | partial | Read-only walk; same as normal |
| `ref:fs.grep` | partial | Read-only; same as normal |
| `ref:fs.write` | **unclear** | Not verified that it honors dry-run |
| `ref:fs.edit` | **unclear** | Not verified |
| `ref:shell.exec` | **no** | Runs the command |
| `ref:shell.pwsh` | **no** | Runs the command |
| `ref:web.fetch` | **no** | Performs the GET |

The `ToolSafety.dry_run: bool` metadata field exists on
`ToolDefinition` and is declared per-tool — but it's descriptive, not
enforced. A tool author can set `dry_run: true` in the metadata and
not implement any preview behavior; callers would be misled.

## Gap

1. No server-level enforcement that `CallOptions.dry_run: true` must
   be honored when `ToolSafety.dry_run: true`
2. No "dry-run not supported" error — destructive tools called with
   `dry_run: true` silently execute
3. No preview-shape contract (what does a dry-run return? The tool's
   normal output schema? Some generic `{"would": "..."}`?)
4. No test coverage verifying each tool's dry-run semantics

## Impact

- **Caller surprise:** an agent using `dry_run: true` as a safety belt
  before a destructive call gets no protection from
  `ref:shell.exec "rm -rf /"`
- **Silent failure:** tool did the thing you thought it wouldn't
- **Contract drift:** `ToolSafety.dry_run` field is decorative, not
  binding

## Proposed approach

Two-step:

**Step 1 (immediate, low cost) — make it honest:**

1. Dispatcher checks `CallOptions.dry_run` and `ToolSafety.dry_run`
2. If `CallOptions.dry_run: true` and `ToolSafety.dry_run: false`:
   return `AtdError::NotImplemented { feature: "dry_run" }` before
   invoking the tool
3. If both are true: delegate to the tool (tool must do the right
   thing)
4. Add `honor_dry_run: bool` method on `Tool` trait; default `false`;
   ref-server tools opt in explicitly

Rationale: closing the "silent-execute" trap is the highest value; an
explicit NotImplemented is better than a surprising side effect.

**Step 2 (longer term) — define the preview contract:**

- What a dry-run tool returns: probably the ToolResult shape with a
  `_dry_run: true` marker in the data payload
- Some tools have obvious dry runs (fs.write returns the would-be
  size); others are nonsense (echo has no side effect to preview)
- Document this in `docs/protocol/wire-format.md` §5 or a new
  `docs/protocol/dry-run.md`

## Acceptance (step 1)

- `Tool` trait gains `fn honor_dry_run(&self) -> bool` with default
  `false`
- `Registry::dispatch` errors with `NotImplemented` when
  `dry_run: true` but the tool's method returns `false`
- At least one tool (proposed: `ref:echo.say`) opts in and passes a
  roundtrip test
- Existing tools unaffected (they all get the default `false` and
  therefore correctly reject dry-run requests instead of silently
  executing)

## Related

- `crates/atd-client/src/options.rs` (CallOptions)
- `crates/atd-types/src/tool.rs` (ToolSafety.dry_run)
- `crates/atd-types/src/error.rs` (`AtdError::NotImplemented`)
- `docs/archive/design.md` §3.6 ("dry_run: Exposed, stubbed")
