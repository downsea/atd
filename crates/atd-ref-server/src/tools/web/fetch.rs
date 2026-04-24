//! `ref:web.fetch` — HTTP GET with SSRF guard, size/time caps, and content-type-aware body shaping.

use std::collections::HashSet;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use url::Url;

use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Tool};

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
        required_capabilities: vec![],
        tier: None,
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
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_link_local()
                || v4.is_private()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.is_multicast()
                // 0.0.0.0/8 — current network (catches 0.x.y.z, not just 0.0.0.0)
                || o[0] == 0
                // 100.64.0.0/10 — Carrier-Grade NAT
                || (o[0] == 100 && (o[1] & 0xC0) == 64)
                // 192.0.0.0/24 — IETF protocol assignments
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                // 192.0.2.0/24 — TEST-NET-1
                || (o[0] == 192 && o[1] == 0 && o[2] == 2)
                // 198.18.0.0/15 — Benchmarking
                || (o[0] == 198 && (o[1] & 0xFE) == 18)
                // 198.51.100.0/24 — TEST-NET-2
                || (o[0] == 198 && o[1] == 51 && o[2] == 100)
                // 203.0.113.0/24 — TEST-NET-3
                || (o[0] == 203 && o[1] == 0 && o[2] == 113)
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
    // SECURITY NOTE: DNS rebinding is not prevented here. We resolve once and
    // check each IP, but an adversary controlling DNS could return a public IP
    // for this lookup and then switch the record to a private IP for reqwest's
    // own connect-time resolution. Accepted trade-off for MVP. Phase 2 fix
    // requires binding reqwest to a custom resolver that shares this result
    // OR adding a connect-time firewall layer.
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

/// Convert HTML to markdown, stripping script and style tag contents so
/// agents aren't fed JavaScript source or CSS as body text.
fn html_to_markdown(html: &str) -> String {
    use htmd::HtmlToMarkdown;
    let converter = HtmlToMarkdown::builder()
        .skip_tags(vec!["script", "style"])
        .build();
    converter.convert(html).unwrap_or_default()
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
            // Cap already reached. Drop the Response (via function return) to
            // close the connection — reqwest will RST, and we don't need any
            // further bytes. Drain-past-cap logic (useful for subprocess pipes)
            // would be a DoS vector here: a slow-drip server could hold our
            // reqwest connection open for the full timeout window.
            truncated = true;
            break;
        }
        let room = cap - buf.len();
        if chunk.len() <= room {
            buf.extend_from_slice(&chunk);
        } else {
            buf.extend_from_slice(&chunk[..room]);
            truncated = true;
            break;
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

            let redirect_chain: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
                std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let chain_for_policy = redirect_chain.clone();
            let allow_private_for_policy = allow_private;

            let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
                // Record the previous URL (where we came from).
                if let Some(prev) = attempt.previous().last() {
                    if let Ok(mut chain) = chain_for_policy.lock() {
                        chain.push(prev.to_string());
                    }
                }
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.error("too many redirects");
                }
                // Re-run the SSRF check on each redirect destination.
                if let Err(e) = check_ssrf(attempt.url(), allow_private_for_policy) {
                    return attempt.error(format!("redirect blocked: {e:?}"));
                }
                attempt.follow()
            });

            let client = reqwest::Client::builder()
                .redirect(redirect_policy)
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
                    let md = html_to_markdown(&text);
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
                "redirected_from": redirect_chain.lock()
                    .map(|v| serde_json::Value::Array(
                        v.iter().map(|s| serde_json::Value::String(s.clone())).collect()
                    ))
                    .unwrap_or_else(|_| serde_json::Value::Array(vec![])),
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
        // reqwest 0.12 does not expose a stable TLS-specific error variant.
        // We infer TLS failure from the formatted error string. This is fragile
        // but is the best available option without vendoring hyper-rustls or
        // mapping from private reqwest types.
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

    #[tokio::test]
    async fn zero_octet_ip_blocked() {
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(serde_json::json!({"url": "http://0.0.0.0:80"}), &ctx)
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "PRIVATE_ADDRESS_BLOCKED");
            }
            _ => panic!("expected PRIVATE_ADDRESS_BLOCKED, got {err:?}"),
        }
    }

    #[tokio::test]
    async fn test_net_range_blocked() {
        let t = WebFetchTool::new();
        let ctx = CallContext::for_test();
        let err = t
            .call(
                serde_json::json!({"url": "http://192.0.2.1:80"}),
                &ctx,
            )
            .await
            .unwrap_err();
        match err {
            ToolCallError::ExecutionFailed { code, .. } => {
                assert_eq!(code, "PRIVATE_ADDRESS_BLOCKED");
            }
            _ => panic!("expected PRIVATE_ADDRESS_BLOCKED, got {err:?}"),
        }
    }
}
