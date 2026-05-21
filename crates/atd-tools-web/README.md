# atd-tools-web

Built-in web-fetch tool for the
[ATD (Agent Tool Dispatch)](https://github.com/downsea/atd) reference runtime.

## Tool provided

| Tool id | Struct | Purpose |
|---|---|---|
| `ref:web.fetch` | `WebFetchTool` | HTTP/HTTPS GET with SSRF guards, per-call timeouts, byte caps, and HTML-to-Markdown conversion. |

`ref:web.fetch` blocks private / loopback / link-local addresses by default
(set `allow_private: true` to opt in). HTML bodies are converted to Markdown
via `htmd`; JSON and text pass through verbatim; binary responses return
metadata only. Request headers are allowlisted (`Accept`, `Accept-Language`,
`Referer`, `User-Agent`).

## Usage

Pair this crate with [`atd-runtime`](https://crates.io/crates/atd-runtime):

```rust
use atd_tools_web::WebFetchTool;
use atd_runtime::registry::Registry;
use std::sync::Arc;

let mut registry = Registry::new();
registry.register(Arc::new(WebFetchTool::new()));
```

Or use [`atd-ref-server`](https://crates.io/crates/atd-ref-server), which has
this tool registered.

For the pattern behind writing your own tool, see
[`atd-tools-echo`](../atd-tools-echo/README.md) — the documented template.

## License

Apache-2.0.
</content>
