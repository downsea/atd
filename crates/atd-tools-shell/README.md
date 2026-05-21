# atd-tools-shell

Built-in shell tools for the
[ATD (Agent Tool Dispatch)](https://github.com/downsea/atd) reference runtime.

## Tools provided

| Tool id | Struct | Purpose |
|---|---|---|
| `ref:shell.exec` | `ShellExecTool` | Run a command via `/bin/sh -c` (Unix) or `cmd /C` (Windows). |
| `ref:shell.pwsh` | `ShellPwshTool` | Run a command via `pwsh -NoProfile -Command`, when PowerShell is available. |

Both tools enforce a timeout (SIGTERM → grace → SIGKILL on Unix), a hard byte
cap on stdout/stderr, and the runtime's capability gate. They return
`{exit_code, stdout, stdout_truncated, stderr, stderr_truncated, duration_ms}`
— a nonzero `exit_code` is a normal business result, not a tool error.
Timeouts and a missing shell are errors (`success: false`).

## Usage

Pair this crate with [`atd-runtime`](https://crates.io/crates/atd-runtime):

```rust
use atd_tools_shell::{ShellExecTool, ShellPwshTool};
use atd_runtime::registry::Registry;
use std::sync::Arc;

let mut registry = Registry::new();
registry.register(Arc::new(ShellExecTool::new()));
registry.register(Arc::new(ShellPwshTool::new()));
```

Or get them preregistered via
[`atd-ref-server`](https://crates.io/crates/atd-ref-server).

For the pattern behind writing your own tool, see
[`atd-tools-echo`](../atd-tools-echo/README.md) — the documented template.

## License

Apache-2.0.
</content>
