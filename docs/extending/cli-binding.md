# Declarative CLI binding config (`CliBindingConfig`)

**Status:** schema-only (SP-cli-binding-v2 ships the typed shape; the runtime subprocess dispatcher that *consumes* this shape is a future SP, see §3).
**Audience:** authors wrapping arbitrary CLI tools (kubectl / gh / mycli / healthkit_cli) as ATD tools; future server implementers writing a generic subprocess dispatcher.

> If you're adding an **in-process Rust tool**, you don't need this — register your `Tool` with `NativeBinding` per [`tool.md`](tool.md).
>
> If you're adding a **new invocation transport** (gRPC, WASM, App Intent), you want the [`Binding` trait reference](binding.md), not this page.

## 1. What this is

`ToolBinding.config` is `serde_json::Value` on the wire — intentionally untyped so binding implementations carry whatever config they need without forcing protocol bumps. But the `BindingProtocol::Cli` case has a recurring shape — argv templating, env passthrough, output parsing, dry-run flag, exit-code mapping — that's worth codifying once, so adopter manifests and dispatchers agree on the same keys.

`atd_protocol::CliBindingConfig` is that typed shape. Use it when you'd otherwise write the same plumbing for each CLI you wrap.

The wire is unchanged: `ToolBinding.config` is still untyped JSON; `CliBindingConfig::from_value(&binding.config)` parses it on demand. Pre-SP-cli-binding-v2 `{"cmd": "..."}` configs parse and re-serialize byte-identical.

## 2. The schema

Canonical fields:

| Field | Type | Required | Purpose |
|---|---|:-:|---|
| `cmd` | string | ✅ | Executable name or absolute path |
| `args` | string[] | | Fixed prefix args prepended before `args_template` expansion |
| `args_template` | string | | Templated argv tail with placeholder substitution (§2.1) |
| `env` | map<string,string> | | Env vars injected at spawn; `"$ATD_BEARER"` opts into bearer passthrough |
| `output_format` | `json` \| `ndjson` \| `lines` | | How to parse stdout into `ToolResult.data` (default `json`) |
| `page_all_flag` | string | | CLI flag the dispatcher appends when fanning out for `call_all` pagination |
| `dry_run_flag` | string | | CLI flag passed when `RunTool.dry_run = true` |
| `exit_code_map` | map<int,string> | | Process exit-code → `ToolResult.code` mapping |

All non-`cmd` fields use serde `skip_serializing_if`, so a manifest that only sets `cmd` serializes to `{"cmd": "..."}` and round-trips byte-identical with pre-v2 servers.

The machine-readable schema lives in [`/atd-protocol-schema.json`](../../atd-protocol-schema.json) at `definitions.CliBindingConfig`; adopter tooling can `$ref` it.

### 2.1 Placeholder vocabulary

`args_template` placeholders the dispatcher MUST recognize:

| Placeholder | Substitution |
|---|---|
| `{tool_id}` | ATD tool id, e.g. `mycli:gmail.users.messages.list` |
| `{params_json}` | `RunTool.args` serialized as **one** JSON argv slot (the dispatcher handles quoting) |
| `{dry_run}` | Empty when `dry_run = false`; the value of `dry_run_flag` when `dry_run = true` |
| `{page_all}` | Empty for one-shot calls; the value of `page_all_flag` when the dispatcher is in `call_all` mode |

Servers MAY extend the vocabulary; dispatchers that don't recognize a placeholder MUST pass it through literally, so unknown placeholders surface as obvious template artifacts rather than silently dropping data.

### 2.2 `env` and bearer passthrough

`env` values are passed verbatim except for the magic literal `"$ATD_BEARER"`, which the dispatcher MUST replace with the connection's bearer token (carried by the SP-12 `hello` handshake, see [`docs/protocol/wire-format.md`](../protocol/wire-format.md) §4.6). This is the canonical hook for "let the CLI authenticate as the agent" without each adopter inventing its own env-var name.

Other `$NAME` syntax is not interpolated. If you want subshell-style expansion, do it before constructing the `ToolBinding`.

### 2.3 `exit_code_map`

The mapping is `i32 → string` where the string becomes `ToolResult.Error.code` (the wire-level domain code, see [`docs/protocol/error-codes.md`](../protocol/error-codes.md)). Exit code `0` always means success and never consults the map. Codes not in the map default to `"TOOL_FAILED"`.

Recommended convention: align with the standard ATD error vocabulary where possible (`"TIMEOUT"`, `"INVALID_ARGS"`, `"NOT_FOUND"`, `"EPERM"`) so client adapters can `is_retryable` correctly without per-binding knowledge.

## 3. What this does NOT do (yet)

`atd-runtime::CliBinding` today is the **hardcoded** binding that powers a single in-tree tool: it takes a `program`, `base_args`, and a `fn(&Value) -> Vec<String>` mapper at construction time. **It does not read `ToolBinding.config` at all.** See [`binding.md`](binding.md) for the existing trait surface.

`SP-cli-binding-v2` (this SP) ships only the typed shape and parser. The generic subprocess dispatcher that *reads* `CliBindingConfig` from `ToolBinding.config` and spawns subprocesses accordingly is `SP-cli-dispatcher-v1` (planned). Until then, `CliBindingConfig` is useful as:

- **Documentation** of the canonical CLI binding shape, so adopter manifests are aligned.
- **Schema** that adopter tooling (`atd-cli`, manifest validators, `mycli`) can use to verify their bindings before wiring them up.
- **Forward-compat target** — servers can already include `CliBindingConfig`-shaped config in their `ToolDefinition`s; SP-cli-dispatcher-v1 will start consuming it without manifest changes.

## 4. Manifest examples

### 4.1 Trivial — `cat` as a tool (v1 shape, parses unchanged)

```json
{
  "protocol": "Cli",
  "config": {
    "cmd": "cat"
  }
}
```

### 4.2 `kubectl get` — declarative wrap of an existing CLI

```json
{
  "protocol": "Cli",
  "config": {
    "cmd": "kubectl",
    "args": ["get"],
    "args_template": "{params_json} -o json {dry_run}",
    "dry_run_flag": "--dry-run=client",
    "output_format": "json",
    "exit_code_map": {
      "1": "TOOL_FAILED",
      "2": "INVALID_ARGS"
    }
  }
}
```

Tool input is the JSON object `{"resource_type": "pods", "namespace": "default"}`, which the dispatcher quotes into a single argv slot replacing `{params_json}`.

### 4.3 `gh issue list` — paginated, bearer passthrough

```json
{
  "protocol": "Cli",
  "config": {
    "cmd": "gh",
    "args": ["issue", "list"],
    "args_template": "--json number,title {params_json} {page_all}",
    "env": {
      "GH_TOKEN": "$ATD_BEARER"
    },
    "output_format": "json",
    "page_all_flag": "--paginate",
    "exit_code_map": {
      "1": "TOOL_FAILED",
      "4": "AUTH_REQUIRED"
    }
  }
}
```

### 4.4 `mycli` — agent-native CLI ([agentic-native-cli](https://github.com/downsea/agentic-native-cli))

```json
{
  "protocol": "Cli",
  "config": {
    "cmd": "mycli",
    "args_template": "{tool_id} --params {params_json} --format json {dry_run} {page_all}",
    "env": {
      "MYCLI_TOKEN": "$ATD_BEARER"
    },
    "output_format": "json",
    "page_all_flag": "--page-all",
    "dry_run_flag": "--dry-run",
    "exit_code_map": {
      "1": "TOOL_FAILED",
      "2": "AUTH_REQUIRED",
      "3": "INVALID_ARGS",
      "4": "TOOL_FAILED",
      "5": "INTERNAL"
    }
  }
}
```

Note: for `mycli` specifically, the cleaner path is **native ATD server mode** (`mycli --atd-serve <socket>`) which exposes the same tools without subprocess overhead per call. The declarative manifest above is the option for environments where you can't run mycli as a long-lived server.

## 5. Parsing in your dispatcher

```rust
use atd_protocol::{CliBindingConfig, ToolDefinition};

fn parse_cli_binding(def: &ToolDefinition) -> Option<CliBindingConfig> {
    def.bindings
        .iter()
        .find_map(|b| b.cli_config().ok().flatten())
}
```

`ToolBinding::cli_config()` returns:
- `Ok(Some(cfg))` — this is a CLI binding with valid config
- `Ok(None)` — this binding is not `Cli` (skip it)
- `Err(CliBindingConfigError)` — `Cli` protocol but config doesn't match the canonical shape; surface as `TOOL_DEFINITION_INVALID` or equivalent on tool registration, not at call time

## 6. See also

- [`binding.md`](binding.md) — the `Binding` trait that dispatchers implement (the *how*; this page is the *what*).
- [`tool.md`](tool.md) — registering native (in-process Rust) tools.
- [`protocol-and-schema.md`](protocol-and-schema.md) — how the wire schema is generated, including this typed config.
- [`docs/protocol/wire-format.md`](../protocol/wire-format.md) §5.2.2 — `ToolBinding` on the wire.
- [`docs/protocol/error-codes.md`](../protocol/error-codes.md) — canonical `ToolResult.code` values for `exit_code_map`.

---

*SP-cli-binding-v2; ships in atd-protocol 1.x as additive new types; no wire change.*
