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

/// Errors that can be returned by a [`TokenBroker::resolve`] or
/// [`TokenBroker::resolve_bearer`] call.
#[derive(Error, Debug)]
pub enum BrokerError {
    #[error("broker not configured for this server")]
    NotConfigured,
    #[error("lookup failed for caller: {0}")]
    Lookup(String),
    #[error("internal broker error: {0}")]
    Internal(String),
    /// Bearer was recognised but its advertised expiry has passed.
    /// SP-token-broker-phase2 §4.4. Maps to HTTP 401 at the listener.
    #[error("bearer expired")]
    Expired,
    /// Bearer was recognised but its underlying grant has been
    /// administratively revoked (status flipped server-side).
    /// SP-token-broker-phase2 §4.4 + §4.8.
    #[error("bearer revoked: {0}")]
    Revoked(String),
}

/// Owned-future return type for [`TokenBroker::resolve`]. Modeled on
/// `registry::CallFuture` to avoid pulling in `async_trait`.
pub type ResolveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<Arc<SecretBundle>>, BrokerError>> + Send + 'a>>;

/// Owned-future return type for [`TokenBroker::resolve_bearer`].
/// SP-streamable-http §4.4 + SP-token-broker-phase2 §5.
pub type ResolveBearerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<BearerIdentity>, BrokerError>> + Send + 'a>>;

/// Outcome of a successful bearer resolution. Returned by
/// [`TokenBroker::resolve_bearer`]; the HTTP listener consumes this to
/// build a `CallContext` per request (SP-streamable-http §4.3).
///
/// Fields are public so brokers in any crate can use struct-literal
/// construction. New fields, if added, will be a minor-version bump.
#[derive(Debug, Clone)]
pub struct BearerIdentity {
    /// Stable caller identifier. Same shape as the
    /// `CallContext::caller_id` populated from SP-12 `Hello.client_id`,
    /// so RBAC checks downstream of the listener treat HTTP callers
    /// uniformly with UDS callers.
    pub caller_id: String,

    /// Capabilities this bearer's caller is granted. The HTTP listener
    /// intersects these with the server's `granted_capabilities`
    /// allow-list before each `tools/call`
    /// (SP-streamable-http §4.3, SP-12 Hello semantics specialised
    /// per-request rather than per-connection).
    pub granted_capabilities: Vec<String>,

    /// Optional secret bundle, same role as the phase-1
    /// [`TokenBroker::resolve`] return. Brokers MAY supply both
    /// `secrets` and the bearer identity in one resolve_bearer call
    /// when the bearer carries enough info to pre-stage secrets;
    /// otherwise leave `None` and let the listener call `resolve`
    /// separately. Celia leaves this `None` because the DEK lives in
    /// `KeyCache` only (patent §13.1) and is never relayed.
    pub secrets: Option<Arc<SecretBundle>>,

    /// Absolute time at which this bearer ceases to be valid. `None`
    /// means "no advertised expiry" (Celia process-lifetime semantics
    /// — pairing codes live until the user revokes them in the wizard
    /// or the host process restarts). SSE listeners use this to
    /// schedule re-validation cadence per SP-token-broker-phase2 §4.7.
    pub expires_at: Option<std::time::SystemTime>,

    /// Hint to the broker's own cache layer: do not return this
    /// `BearerIdentity` from cache after this time without
    /// revalidating. `None` lets the broker choose freely.
    pub cache_until: Option<std::time::SystemTime>,
}

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

    /// Resolve a bearer token (from an HTTP `Authorization: Bearer …`
    /// header) to a [`BearerIdentity`]. The HTTP listener calls this
    /// once per request before dispatch (SP-streamable-http §4.3).
    ///
    /// - `Ok(None)` — bearer was syntactically acceptable but unknown
    ///   to this broker. Listener treats as anonymous (or 401, per
    ///   `require_bearer`).
    /// - `Ok(Some(identity))` — bearer validated; `identity` carries
    ///   caller id, capabilities, optional secrets + expiry hints.
    /// - `Err(BrokerError::Lookup)` — bearer is malformed (broker SHOULD
    ///   fast-reject so probing for valid tokens by trial doesn't hit
    ///   storage).
    /// - `Err(BrokerError::Expired)` — bearer recognised but past expiry.
    /// - `Err(BrokerError::Revoked)` — bearer recognised but its grant
    ///   was administratively revoked.
    /// - `Err(BrokerError::NotConfigured)` — broker does not support
    ///   bearer auth (default impl). Listener treats as anonymous mode.
    ///
    /// Default impl returns `Err(NotConfigured)` so phase-1 brokers
    /// (`InMemoryTokenBroker`) and third-party brokers compile
    /// unchanged — the only adopters who get HTTP bearer auth are the
    /// ones who override this method (SP-streamable-http §4.4,
    /// SP-token-broker-phase2 §5).
    fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
        Box::pin(async move { Err(BrokerError::NotConfigured) })
    }

    /// Hint to the operator + diagnostics paths about which token
    /// format(s) this broker accepts (e.g. `["ce-pairing-code"]`,
    /// `["jwt-rs256"]`, `["opaque"]`). Listener does NOT route on this
    /// — it is informational, surfaced through `atd-ref-server --doctor`
    /// and the `/initialize` server-info echo. Default `&[]` means
    /// "unspecified / introspect via try-resolve". SP-token-broker-phase2
    /// §4.2.
    fn accepted_token_formats(&self) -> &'static [&'static str] {
        &[]
    }
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

    #[tokio::test]
    async fn default_resolve_bearer_returns_not_configured() {
        // Phase-1 brokers (and any TokenBroker impl that doesn't override
        // resolve_bearer) MUST get the NotConfigured signal so HTTP
        // listeners can treat them as "anonymous mode" rather than
        // erroring out unexpectedly. SP-token-broker-phase2 §4.4.
        let broker = InMemoryTokenBroker::new();
        let err = broker.resolve_bearer("ce_0123456789abcdef").await;
        assert!(matches!(err, Err(BrokerError::NotConfigured)));
    }

    #[test]
    fn default_accepted_token_formats_is_empty() {
        let broker = InMemoryTokenBroker::new();
        assert!(broker.accepted_token_formats().is_empty());
    }
}
