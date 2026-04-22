# atd-ref-server SP-5 Web Fetch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ref:web.fetch` — HTTP GET with SSRF guard, size/time caps, and content-type-aware body shaping (HTML→markdown; text passthrough; binary→metadata-only).

**Architecture:** New `tools/web/` subtree with `fetch.rs` holding the full tool implementation. SSRF guard via `std::net::ToSocketAddrs` + inline IP-class check (no new crate). reqwest with rustls-tls for TLS; html2md for HTML conversion; `url` crate for parsing. Unit tests spin up ad-hoc `tokio::net::TcpListener` instances — no external network.

**Tech Stack:** Rust 2024, MSRV 1.85 · `reqwest = "0.12"` (rustls-tls, gzip, brotli) · `html2md = "0.2"` · `url = "2"` · `tokio` already present.

**Spec:** `docs/superpowers/specs/2026-04-23-atd-ref-server-sp5-web.md`

**Scope boundary:**
- **In:** 3 new deps; `tools/web/{mod,fetch}.rs`; builtin registration; 2 new integration tests; cascading 8→9 count updates; README shipped marker.
- **Out (Phase 2+):** POST/PUT/DELETE, cookie jar, Authorization, proxy, HTTP/3, streaming responses, raw HTML mode, binary base64, redirect-chain recording, per-origin rate limiting.

**Prerequisites:**
- `sp4-ref-server-search` tag, 231 Rust workspace tests green.
- No system packages — rustls is pure Rust.

**Exit criteria:**
1. `cargo build -p atd-ref-server --release` zero warnings
2. `cargo test -p atd-ref-server` passes (~150 crate tests)
3. `cargo test --workspace --all-targets` passes ~242 tests
4. Independence check empty for anos/atd-client/atd-mcp-bridge/atd-cli
5. Live smoke: `atd call ref:web.fetch --args '{"url":"https://example.com"}'` returns markdown with "Example Domain" (or fall back to the integration tests as proof if offline)
6. Tag `sp5-ref-server-web` created

---

## File Structure

```
crates/atd-ref-server/
├── Cargo.toml                         (MODIFY — add 3 deps, Task 1)
├── README.md                          (MODIFY — mark SP-5 shipped, Task 5)
└── src/
    ├── builtin.rs                     (MODIFY — register 1 new tool, Task 3)
    ├── server.rs                      (MODIFY — tool_list test count 8→9, Task 3)
    └── tools/
        ├── mod.rs                     (MODIFY — add web submodule, Task 2)
        └── web/                       (NEW)
            ├── mod.rs                 (Task 2 — pub mod fetch;)
            └── fetch.rs               (Task 2 — ~500 LOC incl tests)
└── tests/
    └── integration.rs                 (MODIFY — Tasks 3 + 4)
```

---

## Task 1: Add dependencies

**Files:**
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/Cargo.toml`

- [ ] **Step 1.1: Append 3 deps**

Edit `/home/nan/proj/atd-mvp/crates/atd-ref-server/Cargo.toml`. In `[dependencies]`, append (after the SP-4 search deps):

```toml
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "gzip", "brotli"] }
html2md = "0.2"
url = "2"
```

Explicitly setting `default-features = false` drops `default-tls` which would pull OpenSSL system lib — we want pure-Rust rustls.

Leave all existing deps untouched. Don't add to `[workspace.dependencies]`.

- [ ] **Step 1.2: Build + test baseline**

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server
cargo test --workspace --all-targets
```

Expected: build succeeds (~40-50 new transitive crates in Cargo.lock — hyper, tokio-rustls, etc.), 231 tests still pass.

**If the build fails** because of a reqwest feature mismatch on `html2md` version 0.2 or the `url` dep conflict with reqwest's internal url re-export, STOP and ask. Don't silently change versions.

- [ ] **Step 1.3: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add crates/atd-ref-server/Cargo.toml Cargo.lock
git commit -m "chore(atd-ref-server): add reqwest/html2md/url deps"
```

Include `Cargo.lock`.

---

## Task 2: `tools/web/fetch.rs` — core implementation

**Files:**
- Create: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/web/mod.rs`
- Create: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/web/fetch.rs`
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/mod.rs`

This is the large task. Full tool implementation with SSRF guard, reqwest client, content-type dispatch, 8 unit tests.

- [ ] **Step 2.1: Create `tools/web/mod.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/web/mod.rs`:

```rust
//! Web tools: ref:web.fetch (HTTP GET with SSRF guard + HTML→markdown).

pub mod fetch;
```

- [ ] **Step 2.2: Create `tools/web/fetch.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/web/fetch.rs` with this EXACT content:

```rust
//! `ref:web.fetch` — HTTP GET with SSRF guard, size/time caps, and content-type-aware body shaping.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use atd_types::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::redirect::Policy;
use url::Url;

use crate::context::CallContext;
use crate::error::ToolCallError;
use crate::registry::{CallFuture, Tool};

static DEFINITION: OnceLock<ToolDefinition> = OnceLock::new();

const DEFAULT_MAX_BYTES: usize = 10_000_000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 120_000;
const MAX_REDIRECTS: usize = 5;
const MAX_URL_BYTES: usize = 2048;
const DEFAULT_UA: &str = "atd-ref-server/0.1 (+https://atd-protocol.org)";

fn allowed_headers() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        let mut s = HashSet::new();
        s.insert("accept");
        s.insert("accept-language");
        s.insert("referer");
        s.insert("user-agent");
        s
    })
}

fn definition() -> &'static ToolDefinition {
    DEFINITION.get_or_init(|| ToolDefinition {
        id: "ref:web.fetch".into(),
        name: "Web Fetch".into(),
        description: "HTTP GET a URL and return the body. HTML is converted to markdown; JSON/plain-text are returned verbatim; binary responses return metadata only. Enforces SSRF guard (blocks private/loopback IPs by default), size cap (default 10 MiB), timeout (default 30s, max 120s), and a 5-redirect cap. Request headers are restricted to an allowlist (accept, accept-language, referer, user-agent).".into(),
        version: "0.1.0".into(),
        capability: ToolCapability {
            domain: "web".into(),
            actions: vec!["fetch".into()],
            tags: vec!["web".into(), "http".into(), "fetch".into()],
            intent_examples: vec![
                "fetch https://example.com".into(),
                "read the README at https://example.com/repo/readme.md".into(),
            ],
        },
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "url":           { "type": "string",  "minLength": 1, "maxLength": 2048 },
                "headers":       { "type": "object",  "additionalProperties": { "type": "string" } },
                "max_bytes":     { "type": "integer", "minimum": 1 },
                "timeout_ms":    { "type": "integer", "minimum": 1 },
                "allow_private": { "type": "boolean" }
            },
            "required": ["url"]
        }),
        output_schema: serde_json::json!({
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
        }),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Read,
            dry_run: false,
            side_effects: vec!["network:outbound".into()],
            data_sensitivity: Some(
                "URL fingerprint + source IP visible to the target server".into(),
            ),
        },
        resources: ToolResources {
            timeout_ms: MAX_TIMEOUT_MS,
            max_concurrent: 10,
            rate_limit_per_min: None,
            estimated_tokens: Some(800),
        },
        trust: ToolTrust {
            publisher: "atd-ref-server".into(),
            trust_level: TrustLevel::L2Tested,
            signature: None,
        },
        visibility: ToolVisibility::Read,
    })
}

pub struct WebFetchTool;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct FetchArgs {
    url: String,
    #[serde(default)]
    headers: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    max_bytes: Option<usize>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    allow_private: Option<bool>,
}

fn ip_is_private(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_link_local()
                || v4.is_private()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast()
                // 100.64.0.0/10 Carrier-Grade NAT
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // Link-local fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // ULA fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // IPv4-mapped: check the embedded v4 for privacy
                || v6
                    .to_ipv4_mapped()
                    .map(|v4| ip_is_private(&IpAddr::V4(v4)))
                    .unwrap_or(false)
        }
    }
}

fn check_ssrf(url: &Url, allow_private: bool) -> Result<(), ToolCallError> {
    if allow_private {
        return Ok(());
    }
    let host = url
        .host_str()
        .ok_or_else(|| ToolCallError::InvalidArgs("URL has no host".into()))?;
    // If the host is a literal IP, parse directly.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip_is_private(&ip) {
            return Err(ToolCallError::ExecutionFailed {
                code: "PRIVATE_ADDRESS_BLOCKED".into(),
                message: format!("{ip} is a private/loopback/link-local address"),
                retryable: false,
            });
        }
        return Ok(());
    }
    // DNS resolve. Port doesn't matter for IP classification; use a dummy.
    let port = url.port_or_known_default().unwrap_or(80);
    let mut addrs = match (host, port).to_socket_addrs() {
        Ok(it) => it.peekable(),
        Err(e) => {
            return Err(ToolCallError::ExecutionFailed {
                code: "DNS_FAILED".into(),
                message: format!("dns lookup failed for {host}: {e}"),
                retryable: true,
            });
        }
    };
    if addrs.peek().is_none() {
        return Err(ToolCallError::ExecutionFailed {
            code: "DNS_FAILED".into(),
            message: format!("no addresses resolved for {host}"),
            retryable: true,
        });
    }
    for sa in addrs {
        let ip = sa.ip();
        if ip_is_private(&ip) {
            return Err(ToolCallError::ExecutionFailed {
                code: "PRIVATE_ADDRESS_BLOCKED".into(),
                message: format!("{host} resolves to private address {ip}"),
                retryable: false,
            });
        }
    }
    Ok(())
}

fn build_headers(
    input: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<HeaderMap, ToolCallError> {
    let mut hm = HeaderMap::new();
    let Some(map) = input else {
        return Ok(hm);
    };
    let allowed = allowed_headers();
    for (k, v) in map.iter() {
        let lower = k.to_lowercase();
        if !allowed.contains(lower.as_str()) {
            return Err(ToolCallError::InvalidArgs(format!(
                "header `{k}` is not in the allowlist (allowed: accept, accept-language, referer, user-agent)"
            )));
        }
        let name = HeaderName::from_bytes(lower.as_bytes())
            .map_err(|e| ToolCallError::InvalidArgs(format!("bad header name `{k}`: {e}")))?;
        let Some(s) = v.as_str() else {
            return Err(ToolCallError::InvalidArgs(format!(
                "header `{k}` must be a string"
            )));
        };
        let val = HeaderValue::from_str(s).map_err(|e| {
            ToolCallError::InvalidArgs(format!("bad header value for `{k}`: {e}"))
        })?;
        hm.insert(name, val);
    }
    Ok(hm)
}

fn classify_content_type(ct: &str) -> ContentKind {
    let lc = ct.to_ascii_lowercase();
    let base = lc.split(';').next().unwrap_or("").trim();
    if base == "text/html" || base == "application/xhtml+xml" {
        return ContentKind::Html;
    }
    if base == "application/json"
        || base == "application/xml"
        || base == "application/javascript"
        || base.starts_with("text/")
    {
        return ContentKind::Text;
    }
    ContentKind::Binary
}

enum ContentKind {
    Html,
    Text,
    Binary,
}

/// Stream-read bytes up to `cap`. Returns `(bytes, truncated)`.
async fn read_body_capped(
    mut response: reqwest::Response,
    cap: usize,
) -> Result<(Vec<u8>, bool), reqwest::Error> {
    let mut buf: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = response.chunk().await? {
        if buf.len() >= cap {
            // Already at cap; just keep draining so the server doesn't block.
            truncated = true;
            continue;
        }
        let room = cap - buf.len();
        if chunk.len() <= room {
            buf.extend_from_slice(&chunk);
        } else {
            buf.extend_from_slice(&chunk[..room]);
            truncated = true;
        }
    }
    Ok((buf, truncated))
}

impl Tool for WebFetchTool {
    fn definition(&self) -> &ToolDefinition {
        definition()
    }

    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async move {
            let args: FetchArgs = serde_json::from_value(args)
                .map_err(|e| ToolCallError::InvalidArgs(e.to_string()))?;
            if args.url.trim().is_empty() {
                return Err(ToolCallError::InvalidArgs(
                    "url is empty or whitespace-only".into(),
                ));
            }
            if args.url.len() > MAX_URL_BYTES {
                return Err(ToolCallError::InvalidArgs(format!(
                    "url exceeds {MAX_URL_BYTES} bytes"
                )));
            }
            let parsed = Url::parse(&args.url)
                .map_err(|e| ToolCallError::InvalidArgs(format!("invalid URL: {e}")))?;
            match parsed.scheme() {
                "http" | "https" => {}
                other => {
                    return Err(ToolCallError::InvalidArgs(format!(
                        "only http/https URLs are supported; got {other}"
                    )));
                }
            }
            let headers = build_headers(args.headers.as_ref())?;
            let allow_private = args.allow_private.unwrap_or(false);
            check_ssrf(&parsed, allow_private)?;

            let max_bytes = args
                .max_bytes
                .unwrap_or(DEFAULT_MAX_BYTES)
                .min(ctx.max_output_bytes)
                .max(1);
            let timeout_ms = args
                .timeout_ms
                .unwrap_or(DEFAULT_TIMEOUT_MS)
                .min(MAX_TIMEOUT_MS)
                .max(1);

            let client = reqwest::Client::builder()
                .redirect(Policy::limited(MAX_REDIRECTS))
                .timeout(Duration::from_millis(timeout_ms))
                .user_agent(DEFAULT_UA)
                .build()
                .map_err(|e| ToolCallError::ExecutionFailed {
                    code: "IO".into(),
                    message: format!("client build failed: {e}"),
                    retryable: false,
                })?;

            let start = Instant::now();
            let resp = client
                .get(parsed.clone())
                .headers(headers)
                .send()
                .await
                .map_err(map_reqwest_error)?;

            let final_url = resp.url().to_string();
            let status = resp.status().as_u16();
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            let (body_bytes, truncated) =
                read_body_capped(resp, max_bytes).await.map_err(map_reqwest_error)?;
            let content_length = body_bytes.len();
            let kind = classify_content_type(&content_type);
            let (content, binary) = match kind {
                ContentKind::Html => {
                    let text = String::from_utf8_lossy(&body_bytes).into_owned();
                    let md = html2md::parse_html(&text);
                    (md, false)
                }
                ContentKind::Text => {
                    (String::from_utf8_lossy(&body_bytes).into_owned(), false)
                }
                ContentKind::Binary => (String::new(), true),
            };
            let duration_ms = start.elapsed().as_millis() as u64;

            Ok(serde_json::json!({
                "url": final_url,
                "status": status,
                "content_type": content_type,
                "content": content,
                "content_length": content_length,
                "truncated": truncated,
                "binary": binary,
                "redirected_from": serde_json::Value::Array(vec![]),
                "duration_ms": duration_ms,
            }))
        })
    }
}

fn map_reqwest_error(e: reqwest::Error) -> ToolCallError {
    if e.is_timeout() {
        ToolCallError::ExecutionFailed {
            code: "TIMEOUT".into(),
            message: format!("{e}"),
            retryable: true,
        }
    } else if e.is_redirect() {
        ToolCallError::ExecutionFailed {
            code: "TOO_MANY_REDIRECTS".into(),
            message: format!("{e}"),
            retryable: false,
        }
    } else if e.is_connect() {
        let msg = format!("{e}");
        let code = if msg.to_lowercase().contains("tls") || msg.to_lowercase().contains("certificate") {
            "TLS_FAILED"
        } else {
            "IO"
        };
        ToolCallError::ExecutionFailed {
            code: code.into(),
            message: msg,
            retryable: code == "IO",
        }
    } else {
        ToolCallError::ExecutionFailed {
            code: "IO".into(),
            message: format!("{e}"),
            retryable: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Helper: spawn a one-shot HTTP server that returns the given response
    /// bytes verbatim for a single connection. Returns the bound port.
    async fn spawn_oneshot(response: Vec<u8>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                // Drain the request (don't care about it).
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let _ = sock.write_all(&response).await;
                let _ = sock.shutdown().await;
            }
        });
        port
    }

    /// Helper that returns both the port AND a shared buffer that captures
    /// the raw request. Useful for header-echo assertions.
    async fn spawn_capturing(response: Vec<u8>) -> (u16, Arc<tokio::sync::Mutex<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let buf = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let buf2 = buf.clone();
        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut chunk = [0u8; 4096];
                let mut guard = buf2.lock().await;
                // Read until we see a blank line (end of headers).
                loop {
                    match sock.read(&mut chunk).await {
                        Ok(0) => break,
                        Ok(n) => {
                            guard.extend_from_slice(&chunk[..n]);
                            if guard.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let _ = sock.write_all(&response).await;
                let _ = sock.shutdown().await;
            }
        });
        (port, buf)
    }

    fn http_ok(ctype: &str, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
        v.extend_from_slice(format!("Content-Type: {ctype}\r\n").as_bytes());
        v.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
        v.extend_from_slice(b"Connection: close\r\n\r\n");
        v.extend_from_slice(body);
        v
    }

    #[tokio::test]
    async fn rejects_non_http_scheme() {
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(serde_json::json!({"url": "file:///etc/passwd"}), &ctx)
            .await
            .unwrap_err();
        assert!(matches!(err, ToolCallError::InvalidArgs(_)));
    }

    #[tokio::test]
    async fn rejects_private_ip_by_default() {
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"url": "http://127.0.0.1:9"}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "PRIVATE_ADDRESS_BLOCKED");
            }
            _ => panic!("expected PRIVATE_ADDRESS_BLOCKED"),
        }
    }

    #[tokio::test]
    async fn allows_private_with_flag() {
        let body = b"<html><body><h1>Hi</h1></body></html>";
        let port = spawn_oneshot(http_ok("text/html; charset=utf-8", body)).await;
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({
                    "url": format!("http://127.0.0.1:{port}/"),
                    "allow_private": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["status"], 200);
        assert_eq!(r["binary"], false);
        let content = r["content"].as_str().unwrap();
        assert!(
            content.contains("Hi"),
            "markdown should contain 'Hi': {content:?}"
        );
    }

    #[tokio::test]
    async fn rejects_disallowed_request_header() {
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({
                    "url": "http://127.0.0.1:9",
                    "headers": {"Authorization": "Bearer xxx"},
                    "allow_private": true
                }),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::InvalidArgs(msg) => {
                assert!(msg.to_lowercase().contains("allowlist"));
            }
            _ => panic!("expected InvalidArgs, got {err:?}"),
        }
    }

    #[tokio::test]
    async fn accepts_allowed_request_header() {
        let (port, captured) =
            spawn_capturing(http_ok("text/plain", b"ok")).await;
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let _ = t
            .call(
                serde_json::json!({
                    "url": format!("http://127.0.0.1:{port}/"),
                    "headers": {"Accept": "application/json"},
                    "allow_private": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        // Give the capturing server a moment to finish reading.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let raw = captured.lock().await;
        let request_str = String::from_utf8_lossy(&raw);
        assert!(
            request_str
                .to_lowercase()
                .contains("accept: application/json"),
            "request should contain 'accept: application/json': {request_str:?}"
        );
    }

    #[tokio::test]
    async fn truncates_at_max_bytes() {
        let body = vec![b'x'; 10_000];
        let port = spawn_oneshot(http_ok("text/plain", &body)).await;
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({
                    "url": format!("http://127.0.0.1:{port}/"),
                    "max_bytes": 1024,
                    "allow_private": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["truncated"], true);
        let content = r["content"].as_str().unwrap();
        assert!(content.len() <= 1024);
    }

    #[tokio::test]
    async fn html_converted_to_markdown() {
        let body = b"<html><head><script>evil()</script></head><body><h1>Title</h1></body></html>";
        let port = spawn_oneshot(http_ok("text/html; charset=utf-8", body)).await;
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({
                    "url": format!("http://127.0.0.1:{port}/"),
                    "allow_private": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        let content = r["content"].as_str().unwrap();
        assert!(content.contains("Title"), "content should contain Title: {content:?}");
        assert!(
            !content.to_lowercase().contains("evil()"),
            "script body should be stripped: {content:?}"
        );
    }

    #[tokio::test]
    async fn binary_content_type_emits_empty_content() {
        let body = [0u8, 1, 2, 3, 4, 5];
        let port = spawn_oneshot(http_ok("image/png", &body)).await;
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let r = t
            .call(
                serde_json::json!({
                    "url": format!("http://127.0.0.1:{port}/"),
                    "allow_private": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(r["binary"], true);
        assert_eq!(r["content"], "");
        assert_eq!(r["content_length"], body.len());
    }
}
```

**Implementation notes:**
- `reqwest::redirect::Policy::limited(5)` handles the redirect cap; a `TOO_MANY_REDIRECTS` error surfaces as `e.is_redirect() == true` on the mapped error.
- `response.chunk().await` is the streaming read path; `read_body_capped` bounds the Vec while still draining the stream past the cap.
- `html2md::parse_html` is the library's entry point; it handles script/style stripping internally.
- SSRF check resolves via std::net (synchronous); on a huge DNS-tree this could block briefly, but `to_socket_addrs` is `cfg(not(target_os = "wasi"))` hot-path in the stdlib and generally sub-millisecond for cached lookups. No spawn_blocking needed.
- `html2md = "0.2"` is the current crate version at the time of this plan. If it's not available or has a breaking API change in `parse_html`, the subagent should STOP and ask before substituting.

- [ ] **Step 2.3: Update `tools/mod.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/tools/mod.rs` with:

```rust
//! Built-in tools.
//!
//! - SP-1: echo test-anchor
//! - SP-2: fs.{read,write,edit} + ReadTracker
//! - SP-3: shell.{exec,pwsh} + shared subprocess handler
//! - SP-4: fs.{glob,grep}
//! - SP-5: web.fetch

pub mod echo;
pub mod fs;
pub mod shell;
pub mod web;
```

- [ ] **Step 2.4: Build + test + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib tools::web::fetch    # 8 passed
cargo test --workspace --all-targets                      # 231 + 8 = 239
git add crates/atd-ref-server/src/tools/
git commit -m "feat(atd-ref-server): add ref:web.fetch tool with SSRF guard + HTML→markdown"
```

---

## Task 3: Register in builtin + cascading test updates

**Files:**
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/builtin.rs`
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/server.rs` (test only)
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs` (1 existing test only)

- [ ] **Step 3.1: Update `builtin.rs`**

Replace `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/builtin.rs` with:

```rust
//! Built-in tool registration for `atd-ref-server`.
//!
//! To add a new tool:
//! 1. Create `src/tools/<name>.rs` implementing `Tool`.
//! 2. Export it from the appropriate `tools/*/mod.rs`.
//! 3. Add `reg.register(Arc::new(<Name>Tool::new()))` below.

use std::sync::Arc;

use crate::registry::Registry;
use crate::tools::echo::EchoTool;
use crate::tools::fs::{
    edit::FsEditTool, glob::FsGlobTool, grep::FsGrepTool, read::FsReadTool, write::FsWriteTool,
};
use crate::tools::shell::{exec::ShellExecTool, pwsh::ShellPwshTool};
use crate::tools::web::fetch::WebFetchTool;

pub fn builtin_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(EchoTool::new()));
    reg.register(Arc::new(FsReadTool::new()));
    reg.register(Arc::new(FsWriteTool::new()));
    reg.register(Arc::new(FsEditTool::new()));
    reg.register(Arc::new(FsGlobTool::new()));
    reg.register(Arc::new(FsGrepTool::new()));
    reg.register(Arc::new(ShellExecTool::new()));
    reg.register(Arc::new(ShellPwshTool::new()));
    reg.register(Arc::new(WebFetchTool::new()));
    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry_contains_all_tools() {
        let r = builtin_registry();
        assert_eq!(r.count(), 9);
        assert!(r.get("ref:echo.say").is_some());
        assert!(r.get("ref:fs.read").is_some());
        assert!(r.get("ref:fs.write").is_some());
        assert!(r.get("ref:fs.edit").is_some());
        assert!(r.get("ref:fs.glob").is_some());
        assert!(r.get("ref:fs.grep").is_some());
        assert!(r.get("ref:shell.exec").is_some());
        assert!(r.get("ref:shell.pwsh").is_some());
        assert!(r.get("ref:web.fetch").is_some());
    }
}
```

- [ ] **Step 3.2: Update `server.rs` test**

In `/home/nan/proj/atd-mvp/crates/atd-ref-server/src/server.rs`, find `tool_list_returns_registered_summaries`. Update:
- Change `assert_eq!(.., 8)` → `assert_eq!(.., 9)`
- Add `assert!(ids.contains("ref:web.fetch"));` (matching the idiom used for existing checks)

Do NOT touch any other test.

- [ ] **Step 3.3: Update `integration.rs` tool-list test**

In `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs`, find `e2e_tool_list_returns_echo`. Update:
- Change `assert_eq!(tools.len(), 8)` → `assert_eq!(tools.len(), 9)`
- Add `assert!(ids.contains("ref:web.fetch"));`

Do NOT add new e2e tests (that's Task 4).

- [ ] **Step 3.4: Build + test + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --lib builtin                                     # 1 passed
cargo test -p atd-ref-server --lib server::tests::tool_list_returns_registered_summaries    # 1 passed
cargo test -p atd-ref-server --test integration e2e_tool_list_returns_echo    # 1 passed
cargo test --workspace --all-targets                                            # ~239 total
git add crates/atd-ref-server/
git commit -m "feat(atd-ref-server): register web.fetch in builtin"
```

---

## Task 4: Integration tests for web.fetch

**Files:**
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs`

Append 2 new e2e tests exercising `ref:web.fetch` end-to-end via the Unix socket + an ad-hoc HTTP listener.

- [ ] **Step 4.1: Append 2 tests**

At the END of `/home/nan/proj/atd-mvp/crates/atd-ref-server/tests/integration.rs`, append:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_web_fetch_localhost_happy() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    // Ad-hoc HTTP server returning HTML.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let body = b"<html><body><h1>Hello</h1></body></html>";
            let mut resp = Vec::new();
            resp.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
            resp.extend_from_slice(b"Content-Type: text/html; charset=utf-8\r\n");
            resp.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
            resp.extend_from_slice(b"Connection: close\r\n\r\n");
            resp.extend_from_slice(body);
            let _ = sock.write_all(&resp).await;
            let _ = sock.shutdown().await;
        }
    });

    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:web.fetch",
            "args": {
                "url": format!("http://127.0.0.1:{port}/"),
                "allow_private": true,
            },
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(true));
    assert_eq!(r["result"]["status"], 200);
    assert_eq!(r["result"]["binary"], false);
    let content = r["result"]["content"].as_str().unwrap();
    assert!(content.contains("Hello"), "content should contain 'Hello': {content:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_web_fetch_private_blocked() {
    let srv = spawn_server().await;
    let r = send_one_request(
        &srv.sock,
        &serde_json::json!({
            "type": "run_tool",
            "tool_id": "ref:web.fetch",
            "args": {
                "url": "http://127.0.0.1:9/",
            },
            "dry_run": false,
        }),
    )
    .await
    .unwrap();
    assert_eq!(r["type"], "tool_result");
    assert_eq!(r["success"], serde_json::json!(false));
    assert_eq!(r["result"]["code"], "PRIVATE_ADDRESS_BLOCKED");
}
```

- [ ] **Step 4.2: Run + commit**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-ref-server --test integration    # ~25 total (23 prior + 2 new)
cargo test --workspace --all-targets
git add crates/atd-ref-server/tests/integration.rs
git commit -m "test(atd-ref-server): integration tests for web.fetch"
```

---

## Task 5: README + independence check + tag

**Files:**
- Modify: `/home/nan/proj/atd-mvp/crates/atd-ref-server/README.md`

- [ ] **Step 5.1: Update README**

Read the current README first.

**(a)** Find the "What's shipped and what's next" section. Find the SP-5 bullet (likely `- **SP-5:** ref:web.fetch ...`). Replace with:

```markdown
- **SP-5 (shipped):** `ref:web.fetch` — HTTP GET with SSRF guard + HTML→markdown
```

Match the formatting convention of SP-1/2/3/4 entries.

**(b)** Append a "Web tool" subsection at the end of `## Quick start`, AFTER the "Search tools" subsection from SP-4 and BEFORE the next top-level heading:

````markdown
### Web tool

```bash
# Fetch a public URL and get markdown back:
atd --sock $HOME/.atd-ref/server.sock call ref:web.fetch \
  --args '{"url": "https://example.com"}'

# Localhost with the SSRF escape hatch:
atd --sock $HOME/.atd-ref/server.sock call ref:web.fetch \
  --args '{"url": "http://127.0.0.1:8080/", "allow_private": true}'
```

`ref:web.fetch` blocks private/loopback/link-local addresses by default (set `allow_private: true` to opt in). HTML bodies are stripped and converted to markdown; JSON and text pass through verbatim; binary responses return metadata only (`binary: true`, empty `content`). Limits: 10 MiB body, 30s timeout (configurable up to 120s), 5 redirects. Request headers are allowlisted — only `Accept`, `Accept-Language`, `Referer`, `User-Agent` are accepted; anything else (`Authorization`, `Cookie`, etc.) is rejected.
````

- [ ] **Step 5.2: Independence check**

```bash
cd /home/nan/proj/atd-mvp
cargo tree -p atd-ref-server --prefix none \
  | grep -E '^(anos-|atd-client |atd-mcp-bridge |atd-cli )' \
  && echo FAIL \
  || echo "OK: no client/bridge/cli/anos deps"

grep -E '^\s*(atd-client|atd-mcp-bridge|atd-cli|anos-)' crates/atd-ref-server/Cargo.toml \
  && echo FAIL \
  || echo "OK: manifest clean"
```

Both must print OK.

- [ ] **Step 5.3: Live smoke**

Online:
```bash
cd /home/nan/proj/atd-mvp
cargo build --release -p atd-ref-server --bin atd-ref-server
cargo build --release -p atd-cli --bin atd

./target/release/atd-ref-server --sock /tmp/sp5-smoke.sock &
SRV_PID=$!
sleep 1

./target/release/atd --sock /tmp/sp5-smoke.sock call ref:web.fetch \
  --args '{"url": "https://example.com"}'

kill $SRV_PID
wait $SRV_PID 2>/dev/null
rm -f /tmp/sp5-smoke.sock
```

Expected: JSON output with `status: 200`, `content` containing "Example Domain" (the markdown rendering of example.com's body).

Offline fallback: skip this step with the note "verified via integration tests (`e2e_web_fetch_localhost_happy`)".

- [ ] **Step 5.4: Final workspace regression**

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-ref-server --release
cargo test --workspace --all-targets
```

Expected: release build zero warnings; ~242 tests pass.

- [ ] **Step 5.5: Commit + tag**

```bash
cd /home/nan/proj/atd-mvp
git add crates/atd-ref-server/README.md
git commit -m "docs(atd-ref-server): mark SP-5 shipped and add web fetch quickstart"

git tag -a sp5-ref-server-web -m "SP-5: atd-ref-server web fetch (HTTP GET + SSRF guard + HTML→markdown)"
git log --oneline | head -12
git tag
```

---

## Post-Plan Verification Checklist

- [ ] `cargo build -p atd-ref-server --release` zero warnings
- [ ] `cargo test -p atd-ref-server` passes
- [ ] `cargo test --workspace --all-targets` ~242 tests pass
- [ ] `cargo tree` independence check empty
- [ ] Live smoke (online) OR integration tests (offline) prove end-to-end
- [ ] README has SP-5 marked shipped + web quickstart
- [ ] Tag `sp5-ref-server-web` created

## What's next after SP-5

- **SP-6:** cross-crate E2E rewrite of `hello_atd.{rs,py}` against atd-ref-server (replacing the ANOS server dependency); validation doc with demo video. This is the capstone — it proves the "neutral reference server" story end-to-end.
