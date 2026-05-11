//! Origin allow-list — fail-closed default per MCP Security Warning.
//!
//! SP-streamable-http §4.6: the default allow-list matches Celia's
//! production-hardened set
//! (`celia_phr/crates/celia-cli/src/http_server.rs:121-130`):
//!
//! - `http://127.0.0.1*` / `https://127.0.0.1*`
//! - `http://localhost*` / `https://localhost*`
//! - `tauri://*`
//!
//! Any origin not on the list (and not in the operator's `extra_origins`)
//! gets a 403 + JSON-RPC `-32001`. Requests without an `Origin` header
//! (curl / same-origin fetch) are allowed because the kernel's
//! loopback-bind already enforces the DNS-rebinding boundary for them;
//! same precedent as Celia.

use axum::http::HeaderMap;

/// Returns `true` when the request's `Origin` header is on the default
/// allow-list, on the `extras` list (verbatim match), or absent. Used by
/// the `/mcp` handler as the first gate before bearer auth.
pub fn origin_allowed(headers: &HeaderMap, extras: &[String]) -> bool {
    let origin = match headers.get("origin").and_then(|v| v.to_str().ok()) {
        Some(s) => s,
        // Same-origin / curl / fetch-with-no-origin is fine on a
        // loopback-only bind — the kernel boundary makes
        // DNS-rebinding irrelevant for these callers. Matches
        // celia-cli/src/http_server.rs:170-173 verbatim.
        None => return true,
    };
    if is_default_loopback_origin(origin) {
        return true;
    }
    extras.iter().any(|p| origin == p)
}

/// The five default loopback / Tauri patterns. Public for tests + the
/// `--doctor` self-check in adopters (e.g. `atd-ref-server --doctor`).
pub fn is_default_loopback_origin(origin: &str) -> bool {
    origin.starts_with("http://127.0.0.1")
        || origin.starts_with("http://localhost")
        || origin.starts_with("https://127.0.0.1")
        || origin.starts_with("https://localhost")
        || origin == "tauri://localhost"
        || origin.starts_with("tauri://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_origin(origin: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("origin", HeaderValue::from_str(origin).unwrap());
        h
    }

    #[test]
    fn default_allows_loopback_http() {
        assert!(origin_allowed(
            &headers_with_origin("http://127.0.0.1:5173"),
            &[]
        ));
        assert!(origin_allowed(
            &headers_with_origin("http://localhost:1420"),
            &[]
        ));
    }

    #[test]
    fn default_allows_loopback_https_and_tauri() {
        assert!(origin_allowed(
            &headers_with_origin("https://127.0.0.1"),
            &[]
        ));
        assert!(origin_allowed(
            &headers_with_origin("https://localhost:8443"),
            &[]
        ));
        assert!(origin_allowed(
            &headers_with_origin("tauri://localhost"),
            &[]
        ));
        assert!(origin_allowed(&headers_with_origin("tauri://custom"), &[]));
    }

    #[test]
    fn default_rejects_remote_origin() {
        assert!(!origin_allowed(
            &headers_with_origin("https://evil.example"),
            &[]
        ));
        assert!(!origin_allowed(
            &headers_with_origin("http://attacker.test:8080"),
            &[]
        ));
    }

    #[test]
    fn extras_admits_exact_match_only() {
        let extras = vec!["https://celia.health".to_string()];
        assert!(origin_allowed(
            &headers_with_origin("https://celia.health"),
            &extras
        ));
        // No prefix match — extra origins are verbatim.
        assert!(!origin_allowed(
            &headers_with_origin("https://celia.health.evil.example"),
            &extras
        ));
        // Default loopback still allowed in presence of extras.
        assert!(origin_allowed(
            &headers_with_origin("http://127.0.0.1:5173"),
            &extras
        ));
    }

    #[test]
    fn missing_origin_header_is_allowed() {
        // curl, same-origin fetch — kernel-side loopback bind covers
        // the DNS-rebinding boundary.
        let headers = HeaderMap::new();
        assert!(origin_allowed(&headers, &[]));
    }
}
