# atd-tools-web

Built-in `ref:web.fetch` tool for the ATD reference runtime — HTTP/HTTPS GET with
SSRF guards (private IPs blocked by default), per-call timeouts, byte caps, and
HTML-to-Markdown conversion via `htmd`.

Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime), or use
[`atd-ref-server`](https://crates.io/crates/atd-ref-server) which has this tool
registered.

## License

Apache-2.0.
