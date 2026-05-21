# `atd` — command-line reference

`atd` is the reference command-line client for the ATD protocol. It is a thin
convenience layer over the `atd-sdk` client API: `list` wraps `discover`,
`schema` wraps `describe`, `call` wraps `call`, plus a `doctor` connectivity
check and a `skills` sync helper.

Install it from the workspace:

```bash
cargo install --path crates/atd-cli --bin atd
```

## The socket

Every subcommand connects to an ATD server over a Unix socket. Pass
`--sock PATH` (a global flag, valid before or after the subcommand) to point at
a specific server. When `--sock` is omitted, `atd` falls back to its default
endpoint, `$HOME/.anos/anos.sock`.

> **Note:** `atd-ref-server` binds a *different* default path —
> `$HOME/.atd-ref/server.sock`. When you run the reference server with no
> `--sock` flag, you must give `atd` the matching path explicitly. The simplest
> habit is to pass `--sock` to **both** sides:
>
> ```bash
> atd-ref-server --sock /tmp/atd.sock &
> atd --sock /tmp/atd.sock list
> ```

The built-in tools served by `atd-ref-server` use the `ref:` publisher
namespace — `ref:echo.say`, `ref:fs.read`, `ref:fs.write`, `ref:fs.edit`,
`ref:fs.glob`, `ref:fs.grep`, `ref:shell.exec`, `ref:shell.pwsh`,
`ref:web.fetch` (and `ref:external.uname` on Unix). The examples below use those
ids; a third-party ATD server will advertise its own.

## `atd list` — discover tools

```
atd list [--query STR] [--domain STR] [--tier hot|warm|cold]
         [--visibility read|write|dangerous|system]
         [--limit N] [--json]
```

| Flag | Short | Effect |
|---|---|---|
| `--query` | `-q` | Substring match against id / name / description. |
| `--domain` | `-d` | Filter by domain, e.g. `fs`, `web`. |
| `--tier` | | Filter by tier: `hot`, `warm`, or `cold`. |
| `--visibility` | | Filter by visibility: `read`, `write`, `dangerous`, `system`. |
| `--limit` | `-l` | Cap the number of results. |
| `--json` | | Emit a JSON array of tool summaries instead of the table. |

Default output is a table — `ID NAME DOMAIN TIER VIS` — followed by a total
count. Filtering is applied client-side after the server returns the full list.

Example:

```bash
$ atd --sock /tmp/atd.sock list --query fs --limit 3
ID                                       NAME                     DOMAIN     TIER   VIS
ref:fs.read                              Read File                fs         warm   read
ref:fs.write                             Write File               fs         warm   write
ref:fs.glob                              Glob Files               fs         warm   read
3 tool(s) total
```

## `atd schema TOOL_ID` — inspect a tool

```
atd schema TOOL_ID [--json]
```

Fetches the full `ToolDefinition` for one tool — input/output schemas, safety
metadata, bindings, trust level, and capability requirements. Without `--json`,
output is pretty-printed with 2-space indentation; with `--json`, it is compact
single-line output suitable for piping into `jq`.

```bash
$ atd --sock /tmp/atd.sock schema ref:echo.say
{
  "id": "ref:echo.say",
  "name": "...",
  ...
}
```

## `atd call TOOL_ID` — invoke a tool

```
atd call TOOL_ID [--args JSON] [--dry-run] [--json]
```

| Flag | Effect |
|---|---|
| `--args` | JSON object passed to the tool. Defaults to `{}`. |
| `--dry-run` | Ask the server for a preview without executing side effects. |
| `--json` | Emit the full `ToolResult` envelope as JSON instead of pretty output. |

On success, `atd` prints `ok:` followed by the pretty-printed result data. On a
server-reported failure (`success: false`), `atd` exits non-zero and prints the
error code and message on stderr.

```bash
$ atd --sock /tmp/atd.sock call ref:echo.say --args '{"text":"hello"}'
ok:
{
  "echoed": {
    "text": "hello"
  }
}
```

`--dry-run` is most useful for tools whose `safety.dry_run` is `true` (the shell
tools, for example): the server validates the args and returns what it *would*
do, without running anything.

## `atd doctor` — connectivity check

```
atd doctor [--json]
```

Reports, for the resolved socket:

- the socket path
- whether the socket file exists
- whether `ping` succeeds
- how many tools `discover` returns

Run it first when a connection feels wrong — it isolates a missing socket from a
crashed server from a protocol mismatch.

```bash
$ atd --sock /tmp/atd.sock doctor
socket path:   /tmp/atd.sock
socket exists: true
ping:          ok
tool count:    10
```

## `atd skills sync` — pull skill files from a server

```
atd skills sync --target hermes|claude-code|stdout [--out-dir DIR] [--dry-run]
```

`skills sync` implements the skills meta-tool convention: it discovers every
`<publisher>:<service>.skills.list` tool the server advertises, fetches each
skill's Markdown via the paired `.skills.get` tool, and writes the results to a
per-platform install directory.

| Flag | Effect |
|---|---|
| `--target` | Required. `hermes` → `~/.hermes/skills/`, `claude-code` → `~/.claude/skills/`, `stdout` → print to stdout. |
| `--out-dir` | Override the target's default directory. Incompatible with `--target stdout`. |
| `--dry-run` | List what would be written without writing anything. |

```bash
$ atd --sock /tmp/atd.sock skills sync --target claude-code --dry-run
[would write] /home/you/.claude/skills/ref-echo-say/SKILL.md (412 bytes)
1 skill(s) synced from 1 publisher(s) to /home/you/.claude/skills
```

If the server advertises no `*.skills.list` tool, `skills sync` reports that and
exits cleanly — nothing to sync.

## See also

- [`quickstart/rust.md`](quickstart/rust.md) — the `atd-sdk` API that `atd` wraps.
- [`architecture.md`](architecture.md) — the dispatch model behind every call.
- [`protocol/wire-format.md`](protocol/wire-format.md) — the wire contract.
