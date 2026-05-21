# atd-tools-fs

Built-in filesystem tools for the
[ATD (Agent Tool Dispatch)](https://github.com/downsea/atd) reference runtime.

## Tools provided

| Tool id | Struct | Purpose |
|---|---|---|
| `ref:fs.read` | `FsReadTool` | Read a file's contents. |
| `ref:fs.write` | `FsWriteTool` | Write (create / overwrite) a file. |
| `ref:fs.edit` | `FsEditTool` | In-place edit; enforces read-before-edit via the per-connection `ReadTracker`. |
| `ref:fs.glob` | `FsGlobTool` | Glob-pattern file search (honours `.gitignore`). |
| `ref:fs.grep` | `FsGrepTool` | ripgrep-powered regex search; skips binary files. |

`ref:fs.glob` and `ref:fs.grep` respect `.gitignore` / `.ignore` / `.rgignore`
and skip hidden files by default; results are capped by `max_matches` and the
per-call output budget.

## Usage

Pair this crate with [`atd-runtime`](https://crates.io/crates/atd-runtime) in
your own server:

```rust
use atd_tools_fs::{FsReadTool, FsWriteTool, FsEditTool, FsGlobTool, FsGrepTool};
use atd_runtime::registry::Registry;
use std::sync::Arc;

let mut registry = Registry::new();
registry.register(Arc::new(FsReadTool::new()));
registry.register(Arc::new(FsGlobTool::new()));
// ...
```

Or use [`atd-ref-server`](https://crates.io/crates/atd-ref-server), which has
these tools registered out of the box.

For the pattern behind writing your own tool, see
[`atd-tools-echo`](../atd-tools-echo/README.md) — the documented template.

## License

Apache-2.0.
</content>
