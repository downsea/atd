//! Token broker extension point for multi-tenant ATD servers.
//!
//! [`TokenBroker`] is the trait an operator implements to map a caller
//! identity (`CallContext::caller_id`, populated from the SP-12 Hello
//! handshake) to a [`SecretBundle`] that gets attached to the
//! [`CallContext`] before `Tool::call` runs. Tools that need secrets
//! read them via [`CallContext::secrets`]; tools that don't, ignore the
//! field — full back-compat with single-tenant deployments.
//!
//! Secrets are wrapped in [`RedactedString`], whose `Debug`/`Display`
//! impls refuse to print the value. Audit logs include only a
//! `secrets_resolved: bool` flag (no key names, no values).
//!
//! See `docs/superpowers/specs/2026-04-27-sp-token-broker-phase1-design.md`
//! for the design rationale; Phase 2 (adopter wiring in healthkit_cli)
//! and Phase 3 (live two-tenant demo) are separate SPs.

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use thiserror::Error;

/// String wrapper that refuses to render its value in `Debug` or
/// `Display`. The value is only accessible via [`Self::expose`] — by
/// convention, callers should not log the result of `expose()`.
pub struct RedactedString(String);

impl RedactedString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    /// Returns the underlying value. **Never log or audit the result.**
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RedactedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RedactedString(<redacted>)")
    }
}

impl fmt::Display for RedactedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Bag of named secrets resolved for one caller. Keys are
/// operator-defined (e.g., `"oauth_token"`, `"refresh_token"`,
/// `"api_key"`).
pub type SecretBundle = HashMap<String, RedactedString>;

/// Errors that can be returned by a [`TokenBroker::resolve`] call.
#[derive(Error, Debug)]
pub enum BrokerError {
    #[error("broker not configured for this server")]
    NotConfigured,
    #[error("lookup failed for caller: {0}")]
    Lookup(String),
    #[error("internal broker error: {0}")]
    Internal(String),
}

/// Owned-future return type for [`TokenBroker::resolve`]. Modeled on
/// `registry::CallFuture` to avoid pulling in `async_trait`.
pub type ResolveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<Arc<SecretBundle>>, BrokerError>> + Send + 'a>>;

/// Server-side extension point that resolves secrets for a caller.
///
/// Implementations should be cheap to call (per-`CallContext` overhead);
/// long-lived bundles ought to be cached by the broker itself.
pub trait TokenBroker: Send + Sync {
    /// Resolve a secret bundle for the given caller.
    ///
    /// - `Ok(None)` — no bundle is registered for this caller. Dispatch
    ///   proceeds with `ctx.secrets() = None`; the tool falls back to
    ///   whatever pre-broker mechanism it used (env vars, saved file).
    /// - `Ok(Some(bundle))` — bundle attached to the call.
    /// - `Err(_)` — hard failure; dispatch returns
    ///   `ERR_BROKER_FAILED (1003)` and `Tool::call` is not invoked.
    fn resolve<'a>(&'a self, caller_id: Option<&'a str>) -> ResolveFuture<'a>;
}

/// Reference broker for unit tests + small deployments. Production
/// adopters should implement their own `TokenBroker` against a real
/// secret manager (Vault, AWS Secrets Manager, Doppler, …).
#[derive(Default)]
pub struct InMemoryTokenBroker {
    bundles: HashMap<String, Arc<SecretBundle>>,
}

impl InMemoryTokenBroker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, caller_id: impl Into<String>, bundle: SecretBundle) {
        self.bundles.insert(caller_id.into(), Arc::new(bundle));
    }
}

impl TokenBroker for InMemoryTokenBroker {
    fn resolve<'a>(&'a self, caller_id: Option<&'a str>) -> ResolveFuture<'a> {
        Box::pin(async move {
            let Some(id) = caller_id else {
                return Ok(None);
            };
            Ok(self.bundles.get(id).cloned())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_string_debug_does_not_leak() {
        let s = RedactedString::new("super-secret-token");
        let dbg = format!("{:?}", s);
        assert!(!dbg.contains("super-secret-token"), "leaked: {dbg}");
        assert!(dbg.contains("redacted"));
    }

    #[test]
    fn redacted_string_display_does_not_leak() {
        let s = RedactedString::new("super-secret-token");
        let disp = format!("{}", s);
        assert!(!disp.contains("super-secret-token"), "leaked: {disp}");
        assert_eq!(disp, "<redacted>");
    }

    #[test]
    fn redacted_string_expose_returns_value() {
        let s = RedactedString::new("super-secret-token");
        assert_eq!(s.expose(), "super-secret-token");
    }

    #[test]
    fn secret_bundle_debug_does_not_leak_values() {
        let mut bundle = SecretBundle::new();
        bundle.insert(
            "oauth_token".to_string(),
            RedactedString::new("plaintext-token-value"),
        );
        let dbg = format!("{:?}", bundle);
        // Key may appear; value must NOT.
        assert!(!dbg.contains("plaintext-token-value"), "leaked: {dbg}");
        assert!(dbg.contains("oauth_token"));
    }

    #[tokio::test]
    async fn in_memory_broker_resolves_known_caller() {
        let mut broker = InMemoryTokenBroker::new();
        let mut bundle = SecretBundle::new();
        bundle.insert("oauth".into(), RedactedString::new("tok-A"));
        broker.insert("agent-A", bundle);
        let resolved = broker.resolve(Some("agent-A")).await.unwrap();
        let bundle = resolved.expect("bundle present");
        assert_eq!(bundle.get("oauth").unwrap().expose(), "tok-A");
    }

    #[tokio::test]
    async fn in_memory_broker_returns_none_for_unknown_caller() {
        let broker = InMemoryTokenBroker::new();
        assert!(broker.resolve(Some("unknown")).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn in_memory_broker_returns_none_for_anonymous_caller() {
        let mut broker = InMemoryTokenBroker::new();
        broker.insert("agent-A", SecretBundle::new());
        // Even with bundles registered, a None caller_id resolves to None.
        assert!(broker.resolve(None).await.unwrap().is_none());
    }
}
