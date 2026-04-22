# `atd` — command-line reference

The `atd` binary is a thin convenience layer over the three Phase 0 ATD APIs (`discover`, `describe`, `call`) plus a `doctor` connectivity check. Install with:

```bash
cargo install --path crates/atd-cli --bin atd
```

Every command accepts `--sock PATH` to override the default endpoint (`$HOME/.anos/anos.sock`).

## `atd list` — discover tools

```
atd list [--query STR] [--domain STR] [--tier hot|warm|cold]
         [--visibility read|write|dangerous|system]
         [--limit N] [--json]
```

Default output is a table: `ID NAME DOMAIN TIER VIS` followed by a total count. With `--json`, emits a single JSON array of tool summaries.

Example:

```bash
$ atd list --query fs --limit 3
ID                                       NAME                     DOMAIN     TIER   VIS
anos:fs.read                             Read a file              fs         hot    read
anos:fs.write                            Write a file             fs         hot    write
anos:fs.list                             Directory List           fs         hot    read
3 tool(s) total
```

## `atd schema TOOL_ID` — inspect a tool

```
atd schema TOOL_ID [--json]
```

Without `--json`, pretty-prints the full `ToolDefinition` with 2-space indent. With `--json`, compact single-line output for piping into `jq`.

## `atd call TOOL_ID --args JSON` — invoke a tool

```
atd call TOOL_ID [--args JSON] [--dry-run] [--json]
```

`--args` takes a JSON object, defaulting to `{}`. `--dry-run` asks the server to describe what would happen without side effects.

On server-reported failure (`success:false`), `atd` exits non-zero and prints the error message on stderr.

**Known Phase 0 limitation:** the ANOS reference server's `run_tool` IPC is stubbed; expect `direct tool execution via IPC not yet supported` errors until that is wired up. See `docs/issues/2026-04-21-atd-run-tool-stub.md`.

## `atd doctor` — connectivity check

```
atd doctor [--json]
```

Reports:
- Resolved socket path
- Whether the socket file exists
- Whether `ping` succeeds
- How many tools `discover` returns

Useful for debugging setup issues — run it first when something feels wrong.
