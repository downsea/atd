//! Bearer-token parsing + broker handoff.
//!
//! SP-streamable-http §4.4 + SP-token-broker-phase2 §4.4: extract
//! `Authorization: Bearer …`, look it up via the broker's
//! `resolve_bearer` extension point, surface the result as a typed
//! outcome the route handler maps to a precise HTTP status + headers.
//!
//! Phase-2 mapping (spec §4.4):
//!
//! | outcome | HTTP status | header |
//! |---|---|---|
//! | `Validated(_)` / `Anonymous` | 200 (continue) | — |
//! | `HeaderMissing` (require_bearer) | 401 | `WWW-Authenticate: Bearer` |
//! | `BearerEmpty` | 400 | — |
//! | `NoBrokerConfigured` | 501 | — |
//! | `Unknown` (broker `Ok(None)`) | 401 | `WWW-Authenticate: Bearer error="invalid_token"` |
//! | `Expired` (broker `Err(Expired)`) | 401 | `WWW-Authenticate: Bearer error="invalid_token", error_description="expired"` |
//! | `Revoked(_)` (broker `Err(Revoked)`) | 401 | `WWW-Authenticate: Bearer error="invalid_token", error_description="revoked"` |
//! | `BrokerNotConfigured` (broker `Err(NotConfigured)`) | 501 | — |
//! | `Lookup(_)` (broker `Err(Lookup)` — transient) | 503 | `Retry-After: 5` |
//! | `Internal(_)` (broker `Err(Internal)` — bug) | 500 | — |

use std::sync::Arc;

use atd_runtime::secrets::{BearerIdentity, BrokerError, TokenBroker};
use axum::http::{HeaderMap, StatusCode};

/// Outcome of a single bearer resolution.
///
/// The route handler in `server.rs` consumes this via [`Self::http_status`]
/// / [`Self::www_authenticate`] / [`Self::retry_after`] /
/// [`Self::rejection_message`] to build the response. Variants are
/// deliberately fine-grained to match SP-token-broker-phase2 §4.4's
/// observable-distinction contract: adopters' UIs (Celia's "agent
/// revoked" toast vs "code expired" prompt vs "you mistyped, try
/// again") need them.
#[derive(Debug)]
#[non_exhaustive]
pub enum BearerOutcome {
    /// No `Authorization` header present and `require_bearer == false`.
    /// Route handler proceeds with an empty `CapabilitySet`.
    Anonymous,
    /// Bearer header present and broker returned `Ok(Some(identity))`.
    Validated(BearerIdentity),

    // ---- non-OK outcomes (each maps to a distinct status + headers) ----
    /// `require_bearer == true` and no `Authorization` header.
    HeaderMissing,
    /// `Authorization: Bearer ` present but token after the prefix is
    /// empty — pure client error (per spec §4.4 input contract).
    BearerEmpty,
    /// Header present and recognised as `Bearer …` but no broker is
    /// wired into `HttpServerConfig.shared.token_broker`. Adopter-side
    /// deployment misconfiguration (501 — server cannot honor the
    /// contract it claims).
    NoBrokerConfigured,
    /// Broker returned `Ok(None)` — token is well-formed but unknown.
    Unknown,
    /// Broker returned `Err(Expired)`.
    Expired,
    /// Broker returned `Err(Revoked(reason))`.
    Revoked(String),
    /// Broker returned `Err(NotConfigured)` — broker is wired but its
    /// `resolve_bearer` is the default-impl stub. Distinct from
    /// `NoBrokerConfigured`: the broker exists, it just won't engage.
    BrokerNotConfigured,
    /// Broker returned `Err(Lookup(reason))` — transient look-up
    /// failure (DB hiccup, network blip). Spec §4.4 says 503 +
    /// `Retry-After: 5`. The 2026-04 phase-1 doc had this conflated
    /// with "malformed bearer"; phase-2 clarifies it as transient.
    Lookup(String),
    /// Broker returned `Err(Internal(reason))` — broker bug. 500.
    Internal(String),
}

impl BearerOutcome {
    /// HTTP status code this outcome maps to. `Validated` / `Anonymous`
    /// return `200` as a sentinel — they should not be passed to the
    /// error response path.
    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::Validated(_) | Self::Anonymous => StatusCode::OK,
            Self::BearerEmpty => StatusCode::BAD_REQUEST,
            Self::HeaderMissing | Self::Unknown | Self::Expired | Self::Revoked(_) => {
                StatusCode::UNAUTHORIZED
            }
            Self::NoBrokerConfigured | Self::BrokerNotConfigured => StatusCode::NOT_IMPLEMENTED,
            Self::Lookup(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Value for the `WWW-Authenticate` header, or `None` if no
    /// auth-challenge should be sent.
    pub fn www_authenticate(&self) -> Option<&'static str> {
        match self {
            Self::HeaderMissing => Some(r#"Bearer"#),
            Self::Unknown => Some(r#"Bearer error="invalid_token""#),
            Self::Expired => Some(r#"Bearer error="invalid_token", error_description="expired""#),
            Self::Revoked(_) => {
                Some(r#"Bearer error="invalid_token", error_description="revoked""#)
            }
            _ => None,
        }
    }

    /// Value for the `Retry-After` header in seconds, or `None`. Per
    /// spec §4.4, only the transient `Lookup` outcome sends this.
    pub fn retry_after(&self) -> Option<u64> {
        match self {
            Self::Lookup(_) => Some(5),
            _ => None,
        }
    }

    /// Human-readable rejection message for the error response body.
    /// `None` for the success / continue outcomes.
    pub fn rejection_message(&self) -> Option<String> {
        match self {
            Self::Validated(_) | Self::Anonymous => None,
            Self::HeaderMissing => Some("Authorization: Bearer required".into()),
            Self::BearerEmpty => Some("bearer is empty after `Bearer ` prefix".into()),
            Self::NoBrokerConfigured => {
                Some("token broker not configured for HTTP bearer auth".into())
            }
            Self::Unknown => Some("bearer not recognised".into()),
            Self::Expired => Some("bearer expired".into()),
            Self::Revoked(msg) => Some(format!("bearer revoked: {msg}")),
            Self::BrokerNotConfigured => {
                Some("broker does not support bearer auth (override resolve_bearer)".into())
            }
            Self::Lookup(msg) => Some(format!("broker lookup failed (transient): {msg}")),
            Self::Internal(msg) => Some(format!("broker internal error: {msg}")),
        }
    }

    /// `true` if this outcome means "continue dispatch"; `false` if
    /// the response is an early-return error.
    pub fn is_admitted(&self) -> bool {
        matches!(self, Self::Validated(_) | Self::Anonymous)
    }
}

/// Parse the optional `Authorization: Bearer …` header from `headers`
/// and run the broker policy described in SP-streamable-http §4.4 +
/// SP-token-broker-phase2 §4.4. Returns a typed [`BearerOutcome`].
pub async fn resolve_bearer(
    headers: &HeaderMap,
    broker: Option<&Arc<dyn TokenBroker>>,
    require_bearer: bool,
) -> BearerOutcome {
    let raw = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let Some(token) = raw else {
        return if require_bearer {
            BearerOutcome::HeaderMissing
        } else {
            BearerOutcome::Anonymous
        };
    };

    // Spec §4.4 input contract: empty bearer is a 400-class client error,
    // handled before calling the broker.
    if token.trim().is_empty() {
        return BearerOutcome::BearerEmpty;
    }

    let Some(broker) = broker else {
        return BearerOutcome::NoBrokerConfigured;
    };

    match broker.resolve_bearer(token).await {
        Ok(Some(identity)) => BearerOutcome::Validated(identity),
        Ok(None) => BearerOutcome::Unknown,
        Err(BrokerError::NotConfigured) => BearerOutcome::BrokerNotConfigured,
        Err(BrokerError::Expired) => BearerOutcome::Expired,
        Err(BrokerError::Revoked(msg)) => BearerOutcome::Revoked(msg),
        Err(BrokerError::Lookup(msg)) => BearerOutcome::Lookup(msg),
        Err(BrokerError::Internal(msg)) => BearerOutcome::Internal(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_runtime::secrets::{ResolveBearerFuture, ResolveFuture};
    use axum::http::HeaderValue;

    fn headers_with_bearer(token: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
        );
        h
    }

    #[tokio::test]
    async fn missing_header_anonymous_mode_returns_anonymous() {
        let h = HeaderMap::new();
        let outcome = resolve_bearer(&h, None, false).await;
        assert!(matches!(outcome, BearerOutcome::Anonymous));
        assert_eq!(outcome.http_status(), StatusCode::OK);
        assert!(outcome.is_admitted());
    }

    #[tokio::test]
    async fn missing_header_strict_mode_returns_header_missing() {
        let h = HeaderMap::new();
        let outcome = resolve_bearer(&h, None, true).await;
        assert!(matches!(outcome, BearerOutcome::HeaderMissing));
        assert_eq!(outcome.http_status(), StatusCode::UNAUTHORIZED);
        assert_eq!(outcome.www_authenticate(), Some("Bearer"));
        assert!(outcome.retry_after().is_none());
        assert!(!outcome.is_admitted());
    }

    #[tokio::test]
    async fn bearer_present_but_empty_returns_bearer_empty() {
        let mut h = HeaderMap::new();
        h.insert("authorization", HeaderValue::from_static("Bearer   "));
        let outcome = resolve_bearer(&h, None, true).await;
        assert!(matches!(outcome, BearerOutcome::BearerEmpty));
        assert_eq!(outcome.http_status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn bearer_without_broker_returns_no_broker_configured() {
        let h = headers_with_bearer("xyz");
        let outcome = resolve_bearer(&h, None, true).await;
        assert!(matches!(outcome, BearerOutcome::NoBrokerConfigured));
        assert_eq!(outcome.http_status(), StatusCode::NOT_IMPLEMENTED);
        assert!(outcome.www_authenticate().is_none());
    }

    // ---- broker-error fixtures + status-mapping coverage ----

    struct FixedErrBroker(BrokerError);

    impl std::fmt::Debug for FixedErrBroker {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FixedErrBroker").finish()
        }
    }

    impl TokenBroker for FixedErrBroker {
        fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
            Box::pin(async { Ok(None) })
        }
        fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
            let err = match &self.0 {
                BrokerError::NotConfigured => BrokerError::NotConfigured,
                BrokerError::Expired => BrokerError::Expired,
                BrokerError::Revoked(s) => BrokerError::Revoked(s.clone()),
                BrokerError::Lookup(s) => BrokerError::Lookup(s.clone()),
                BrokerError::Internal(s) => BrokerError::Internal(s.clone()),
            };
            Box::pin(async move { Err(err) })
        }
    }

    fn arc_broker(err: BrokerError) -> Arc<dyn TokenBroker> {
        Arc::new(FixedErrBroker(err))
    }

    #[tokio::test]
    async fn broker_returns_none_maps_to_unknown_401_invalid_token() {
        struct NoneBroker;
        impl std::fmt::Debug for NoneBroker {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("NoneBroker").finish()
            }
        }
        impl TokenBroker for NoneBroker {
            fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
                Box::pin(async { Ok(None) })
            }
            fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
                Box::pin(async { Ok(None) })
            }
        }
        let h = headers_with_bearer("anything");
        let broker: Arc<dyn TokenBroker> = Arc::new(NoneBroker);
        let outcome = resolve_bearer(&h, Some(&broker), true).await;
        assert!(matches!(outcome, BearerOutcome::Unknown));
        assert_eq!(outcome.http_status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            outcome.www_authenticate(),
            Some(r#"Bearer error="invalid_token""#)
        );
    }

    #[tokio::test]
    async fn broker_expired_maps_to_401_with_error_description_expired() {
        let h = headers_with_bearer("anything");
        let broker = arc_broker(BrokerError::Expired);
        let outcome = resolve_bearer(&h, Some(&broker), true).await;
        assert!(matches!(outcome, BearerOutcome::Expired));
        assert_eq!(outcome.http_status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            outcome.www_authenticate(),
            Some(r#"Bearer error="invalid_token", error_description="expired""#)
        );
    }

    #[tokio::test]
    async fn broker_revoked_maps_to_401_with_error_description_revoked() {
        let h = headers_with_bearer("anything");
        let broker = arc_broker(BrokerError::Revoked("user revoked".into()));
        let outcome = resolve_bearer(&h, Some(&broker), true).await;
        match &outcome {
            BearerOutcome::Revoked(msg) => assert_eq!(msg, "user revoked"),
            other => panic!("expected Revoked, got {other:?}"),
        }
        assert_eq!(outcome.http_status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            outcome.www_authenticate(),
            Some(r#"Bearer error="invalid_token", error_description="revoked""#)
        );
    }

    #[tokio::test]
    async fn broker_lookup_maps_to_503_with_retry_after_5() {
        let h = headers_with_bearer("anything");
        let broker = arc_broker(BrokerError::Lookup("sqlite locked".into()));
        let outcome = resolve_bearer(&h, Some(&broker), true).await;
        assert!(matches!(outcome, BearerOutcome::Lookup(_)));
        assert_eq!(outcome.http_status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(outcome.retry_after(), Some(5));
        // No WWW-Authenticate; this is a server-side hiccup, not an
        // auth challenge.
        assert!(outcome.www_authenticate().is_none());
    }

    #[tokio::test]
    async fn broker_internal_maps_to_500_no_headers() {
        let h = headers_with_bearer("anything");
        let broker = arc_broker(BrokerError::Internal("oh no".into()));
        let outcome = resolve_bearer(&h, Some(&broker), true).await;
        assert!(matches!(outcome, BearerOutcome::Internal(_)));
        assert_eq!(outcome.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(outcome.www_authenticate().is_none());
        assert!(outcome.retry_after().is_none());
    }

    #[tokio::test]
    async fn broker_not_configured_maps_to_501() {
        let h = headers_with_bearer("anything");
        let broker = arc_broker(BrokerError::NotConfigured);
        let outcome = resolve_bearer(&h, Some(&broker), true).await;
        assert!(matches!(outcome, BearerOutcome::BrokerNotConfigured));
        assert_eq!(outcome.http_status(), StatusCode::NOT_IMPLEMENTED);
    }

    // ---- happy path ----

    struct FixedOkBroker(BearerIdentity);
    impl std::fmt::Debug for FixedOkBroker {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FixedOkBroker").finish()
        }
    }
    impl TokenBroker for FixedOkBroker {
        fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
            Box::pin(async { Ok(None) })
        }
        fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
            let id = self.0.clone();
            Box::pin(async move { Ok(Some(id)) })
        }
    }

    #[tokio::test]
    async fn good_bearer_returns_validated_identity() {
        let id = BearerIdentity {
            caller_id: "agent-A".into(),
            granted_capabilities: vec!["records:read".into()],
            secrets: None,
            expires_at: None,
            cache_until: None,
        };
        let h = headers_with_bearer("good-token");
        let broker: Arc<dyn TokenBroker> = Arc::new(FixedOkBroker(id));
        let outcome = resolve_bearer(&h, Some(&broker), true).await;
        match &outcome {
            BearerOutcome::Validated(id) => {
                assert_eq!(id.caller_id, "agent-A");
                assert_eq!(id.granted_capabilities, vec!["records:read"]);
            }
            other => panic!("expected Validated, got {other:?}"),
        }
        assert_eq!(outcome.http_status(), StatusCode::OK);
        assert!(outcome.is_admitted());
    }
}
