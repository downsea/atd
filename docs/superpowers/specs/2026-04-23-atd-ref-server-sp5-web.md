# atd-ref-server — SP-5 Web Fetch Design Spec

**Date:** 2026-04-23
**Status:** Design approved; plan pending.
**Scope:** Sub-project 5 of atd-ref-server. Adds `ref:web.fetch` — HTTP GET with SSRF guard, size/time caps, and content-type-aware body shaping (HTML → markdown; text passthrough; binary → metadata-only). Expands the catalog from "file + shell + search" to "file + shell + search + web read."
**Builds on:** SP-4 (`sp4-ref-server-search`) — 231 Rust workspace tests, 8 tools registered.

---

## 1. Motivation

Agents need the web. Every real agent workflow hits a doc page, a GitHub README, an API reference, a Stack Overflow answer. Without a first-class fetch tool, every fetch routes through `shell.exec curl`, which means:
- platform-dependent flags and behavior
- no size cap — a page with a malicious megabyte bomb eats agent context
- no SSRF defense — `curl http://169.254.169.254/` works from inside a cloud VM
- no HTML→markdown — agents get raw HTML with `<script>` and nav chrome, wasting tokens on boilerplate

A proper `ref:web.fetch` fixes all of that in one tool with deterministic behavior, deterministic output shape, and a default-safe security posture.

Like SP-1/2/3/4, this is clean-room: designed from universal HTTP-client concepts + `reqwest` + `html2md` + `url` as normal Rust deps. No proprietary source consulted.

---

## 2. Scope

### 2.1 In scope

- **`ref:web.fetch`** — HTTP GET with content-type-aware body shaping.
- **SSRF guard** — reject RFC1918 / loopback / link-local / ULA resolutions by default. Opt-in via `allow_private: true`.
- **Header allowlist** — caller may override `Accept` / `Accept-Language` / `Referer` / `User-Agent`. Everything else rejected as `InvalidArgs` (in particular: no `Authorization`, no `Cookie`).
- **Caps:** body 10 MiB default (configurable per-call up to server max); timeout 30s default (configurable up to 120s); redirects capped at 5.
- **Registration** in `builtin.rs` (1 new tool, count 8 → 9).
- **3 integration tests** using an ad-hoc tokio TcpListener harness (no external network).
- **README update** — mark SP-5 shipped; add brief "Web tool" section.

### 2.2 Explicitly deferred (Phase 2+)

- **POST / PUT / DELETE** — needs a separate brainstorm on auth + CSRF model.
- **Cookie jar / session persistence** — each fetch is stateless.
- **Auth headers** — `Authorization` is deliberately blocked; agents needing auth shell out to `curl` via `shell.exec` (explicit, visible escalation).
- **Proxy support** / custom CA certs — rely on system defaults.
- **HTTP/3**.
- **Streaming responses to the client** — reqwest streams internally for size cap, but the tool returns one JSON blob.
- **Raw HTML mode** (`format: "raw"` arg) — ship Phase 2 if agents ask.
- **Per-origin rate limiting** — operators deploy their own ingress limits.
- **Binary body base64-encode** — agents can curl+base64 via `shell.exec`.
- **IPv6 literal URLs** — supported to the extent reqwest + std resolvers support them; no extra logic.

### 2.3 Prerequisites

- atd-ref-server at tag `sp4-ref-server-search`, 231 Rust workspace tests green.
- No system package prerequisites — `rustls-tls` is pure Rust and does not depend on OpenSSL.

---

## 3. Tool definition

**ID:** `ref:web.fetch`
**Name:** `Web Fetch`
**Domain:** `web` · **Actions:** `["fetch"]` · **Tags:** `["web", "http", "fetch"]`
**Safety:** `SafetyLevel::Read` · **Visibility:** `ToolVisibility::Read` · **Trust:** `L2Tested`
**Side effects:** `["network:outbound"]` · **Data sensitivity:** `"agent-visible URL fingerprint (User-Agent + source IP)"`
**Resources:** `timeout_ms: 120_000, max_concurrent: 10, estimated_tokens: 800`

### 3.1 Input schema

```json
{
  "type": "object",
  "properties": {
    "url":           { "type": "string",  "minLength": 1, "maxLength": 2048 },
    "headers":       { "type": "object",  "additionalProperties": { "type": "string" } },
    "max_bytes":     { "type": "integer", "minimum": 1 },
    "timeout_ms":    { "type": "integer", "minimum": 1 },
    "allow_private": { "type": "boolean" }
  },
  "required": ["url"]
}
```

Defaults: `max_bytes = 10_000_000` (clamped to `ctx.max_output_bytes` at response time). `timeout_ms = 30_000` (clamped at 120_000). `allow_private = false`.

### 3.2 Output schema

```json
{
  "type": "object",
  "properties": {
    "url":             { "type": "string" },
    "status":          { "type": "integer" },
    "content_type":    { "type": "string" },
    "content":         { "type": "string" },
    "content_length":  { "type": "integer" },
    "truncated":       { "type": "boolean" },
    "binary":          { "type": "boolean" },
    "redirected_from": { "type": "array", "items": { "type": "string" } },
    "duration_ms":     { "type": "integer" }
  }
}
```

- `url` — final URL after following redirects.
- `status` — HTTP status (nonzero status is NOT a tool error — agents read `status` themselves).
- `content_type` — the `Content-Type` header as received.
- `content` — markdown if input was HTML; UTF-8-lossy text otherwise; empty string if binary.
- `content_length` — bytes read from the body (post-decompression; pre-truncation it is the amount stored, capped by `max_bytes`).
- `truncated` — true if response body was larger than `max_bytes`.
- `binary` — true if content type was detected as binary; `content` is empty, only metadata useful.
- `redirected_from` — URLs the fetch was redirected through, in order. Empty array if no redirect.
- `duration_ms` — wall clock.

### 3.3 Behavior

1. **Deserialize args.** Reject empty/whitespace `url`; reject `url` > 2048 bytes.
2. **Parse URL.** If scheme is not `http` or `https` → `InvalidArgs`. Other bad URL shapes → `InvalidArgs`.
3. **Header allowlist.** Iterate `headers` (if present). Lowercase each key; reject anything not in `{accept, accept-language, referer, user-agent}` → `InvalidArgs`.
4. **SSRF check (default).** `resolve_public_addrs(host, port)`: uses `std::net::ToSocketAddrs` to resolve all IPs for the target. If `allow_private == false` AND any resolved IP is loopback / link-local / RFC1918 / ULA → `ExecutionFailed { code: "PRIVATE_ADDRESS_BLOCKED", retryable: false }`. If `allow_private == true`, skip the check.
5. **Build client.** `reqwest::Client::builder().redirect(Policy::limited(5)).timeout(Duration::from_millis(timeout_ms)).user_agent(DEFAULT_UA).build()`. Content-encoding is handled transparently via features.
6. **Issue GET.** Apply any allowlisted headers; the default UA is overridable via the `User-Agent` header.
7. **Read body with cap.** Use `response.bytes_stream()` + accumulate into a `Vec<u8>` with a running byte count; once `max_bytes` is reached, set `truncated = true` and drain-and-discard the rest of the stream (never buffer past the cap).
8. **Dispatch on content type:**
   - `text/html*` → `html2md::parse_html(&utf8_str)` → markdown string → `content`
   - `application/json`, `text/plain`, `text/*`, `application/xml`, `application/javascript`, `application/xhtml+xml` → `String::from_utf8_lossy(bytes)` → `content`
   - anything else → `binary: true`, `content: ""`, but preserve `content_length` metadata
9. **Collect redirect chain.** reqwest exposes the final URL; the chain of intermediates can be observed via `RedirectPolicy::custom` or by using `Response::url()` combined with manual history tracking. Simplest practical approach: use `Client::builder().redirect(Policy::limited(5))` (auto follow) and expose only the final `url`, initializing `redirected_from: []`. Upgrade to a custom policy that records the chain in a Phase 2 iteration if agents need it.
   - **MVP simplification:** always emit `redirected_from: []` in SP-5. Plan Task 2 lands this as an empty vec; Phase 2 upgrades to a recording policy.
10. **Return** the JSON object above.

### 3.4 Error mapping

| Internal event | Tool error |
|---|---|
| Bad URL | `InvalidArgs("invalid URL: ...")` |
| Non-http(s) scheme | `InvalidArgs("only http/https URLs are supported")` |
| Disallowed header | `InvalidArgs("header `<name>` is not in the allowlist")` |
| `max_bytes` exceeds server's `ctx.max_output_bytes` | clamp silently (document in tool description) |
| `timeout_ms` exceeds 120_000 | clamp silently to 120_000 |
| DNS failure | `ExecutionFailed { code: "DNS_FAILED", retryable: true }` |
| Private-address block | `ExecutionFailed { code: "PRIVATE_ADDRESS_BLOCKED", retryable: false }` |
| Connect/read timeout | `ExecutionFailed { code: "TIMEOUT", retryable: true }` |
| TLS handshake failure | `ExecutionFailed { code: "TLS_FAILED", retryable: false }` |
| Redirect cap exceeded | `ExecutionFailed { code: "TOO_MANY_REDIRECTS", retryable: false }` |
| Other I/O | `ExecutionFailed { code: "IO", retryable: true }` |

Non-2xx HTTP status is **not** a tool error — the tool returns success with `status: 404`/`5xx` so agents can inspect.

---

## 4. File structure

```
crates/atd-ref-server/
├── Cargo.toml                         (MODIFY — add 3 deps, Task 1)
├── README.md                          (MODIFY — mark SP-5 shipped, Task 5)
└── src/
    ├── builtin.rs                     (MODIFY — register 1 new tool, Task 3)
    ├── server.rs                      (MODIFY — test count 8→9, Task 3)
    └── tools/
        ├── mod.rs                     (MODIFY — add web submodule, Task 2)
        └── web/                       (NEW)
            ├── mod.rs                 (Task 2 — pub mod fetch;)
            └── fetch.rs               (Task 2 — ~500 LOC incl tests)
└── tests/
    └── integration.rs                 (MODIFY — count 8→9 + 2 new tests, Task 3 + 4)
```

---

## 5. Dependencies

```toml
[dependencies]
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "gzip", "brotli"] }
html2md = "0.2"
url = "2"
```

**Why these flags:**
- `default-features = false` drops `default-tls` (which pulls OpenSSL system lib).
- `rustls-tls` — pure-Rust TLS, no OpenSSL dependency.
- `gzip` + `brotli` — most real web traffic is gzip-compressed; reqwest handles decompression transparently with these features.
- `url = "2"` — direct dep for parsing. reqwest re-exports `url::Url`; we import directly to avoid leaking reqwest types into the validator.

**Estimated transitive footprint:** ~50 crates (reqwest pulls hyper, tokio-rustls, rustls, h2, http, mime, serde_urlencoded, etc.). Larger than SP-4's ripgrep additions but all well-maintained, Apache-2.0/MIT, and are already common across the Rust ecosystem.

**Independence check updated:** `cargo tree -p atd-ref-server | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )'` must still return empty. `reqwest` / `hyper` / `rustls` are neutral infrastructure, not protocol-coupling.

---

## 6. Test plan

### 6.1 Unit tests — `tools/web/fetch.rs` (8 tests)

All use an ad-hoc `tokio::net::TcpListener` bound to `127.0.0.1:0` for the server-dependent cases. Each test spins up a single listener accepting one request, returning a canned HTTP/1.1 response. Tests run in parallel, each on a different ephemeral port — no conflicts.

1. `rejects_non_http_scheme` — `file:///etc/passwd` → `InvalidArgs`
2. `rejects_private_ip_by_default` — `http://127.0.0.1:9` with `allow_private=false` → `PRIVATE_ADDRESS_BLOCKED`
3. `allows_private_with_flag` — ad-hoc listener, `allow_private=true`, returns 200 + `<html><body><h1>Hi</h1></body></html>` → `content = "# Hi"`-ish (markdown)
4. `rejects_disallowed_request_header` — `Authorization: Bearer xxx` → `InvalidArgs`
5. `accepts_allowed_request_header` — `Accept: application/json` flows through (verified via test-server echo of the header value)
6. `truncates_at_max_bytes` — response body of 10_000 bytes with `max_bytes=1024` → `truncated=true`, content length ≤ 1024 (for text) or empty (if binary detection fires)
7. `html_converted_to_markdown` — `<html><h1>T</h1><script>bad</script></html>` → content contains `T` / `# T` but NOT `<script>`
8. `binary_content_type_emits_empty_content` — server returns `Content-Type: image/png` with some bytes → `binary=true`, `content=""`, `content_length` preserved

### 6.2 Integration tests — `tests/integration.rs` (2 new + 1 updated)

- Update `e2e_tool_list_returns_echo` — assert count 9, add `ref:web.fetch` id check
- `e2e_web_fetch_localhost_happy` — ad-hoc listener, `allow_private=true`, markdown body comes back
- `e2e_web_fetch_private_blocked` — same listener but without the flag → wire error response

### 6.3 Expected test counts

- `tools::web::fetch`: 8
- `builtin`: updated count=9
- `server`: updated count=9
- integration: 23 prior + 2 new = 25
- Workspace total target: ~242 tests (231 prior + 11 new)

---

## 7. Risks and non-risks

### 7.1 Risks

- **SSRF false positives.** DNS resolution can be slow or flaky; if `getaddrinfo` returns 0 IPs transiently, we'd incorrectly fail-open if we weren't careful. Mitigation: treat "0 addresses resolved" as `DNS_FAILED`, never as "no private IPs = safe".
- **DNS rebinding.** If we resolve, check "public", then reqwest does its own resolve, a malicious DNS server could return public then private. Mitigation for MVP: accept this trade-off — `reqwest` uses the OS resolver, same source; in practice DNS rebinding attacks require control over DNS AND ~1s timing window AND a target URL that expects a second resolution, not the reference server's own scenario. Document as acceptable.
- **html2md injection-style weirdness.** If agent passes the tool-returned markdown into a subsequent shell/code call, odd control characters could survive. Mitigation: clients are responsible for escaping before use in other tools. Not our problem at fetch time.
- **Redirect to private address.** Current design resolves the ORIGINAL host. If the public URL 302s to `http://127.0.0.1`, reqwest follows it. Mitigation: for MVP, accept this gap and document it; Phase 2 switches to a custom redirect policy that re-runs the SSRF check on each hop.

### 7.2 Non-risks

- **OpenSSL CVEs** — rustls-tls bypass means no OpenSSL. Fewer CVE-chase cycles.
- **Cookie theft** — no cookie jar.
- **Large binary downloads eating memory** — size cap enforced via streaming; max 10 MiB bounded.
- **Infinite redirect loops** — reqwest caps at 5 (configured).

---

## 8. Exit criteria

1. `cargo build -p atd-ref-server --release` zero warnings
2. `cargo test -p atd-ref-server` passes (~125 lib + 25 integration = ~150 tests in crate)
3. `cargo test --workspace --all-targets` ~242 tests
4. Independence check empty for anos/atd-client/atd-mcp-bridge/atd-cli
5. Live smoke: `atd call ref:web.fetch --args '{"url": "https://example.com"}'` returns markdown containing "Example Domain" (requires internet — if run offline, fall back to ad-hoc listener integration tests as proof)
6. Tag `sp5-ref-server-web` created

---

## 9. Out of scope for SP-5 (Phase 2+ ideas)

- `ref:web.post` / `ref:web.put` (separate safety model)
- `ref:web.download` — store to file with integrity check (combines fs.write + fetch)
- `ref:web.screenshot` — headless browser (needs chromium / wkhtml)
- `ref:web.search` — search engine API wrapper (vendor-dependent)
- Redirect chain recording (MVP emits `[]`; Phase 2 enables)
- Per-host rate limit table
- Custom CA cert injection for corporate proxies
