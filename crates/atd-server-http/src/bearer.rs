//! Bearer-token parsing + broker handoff.
//!
//! SP-streamable-http §4.4: extract `Authorization: Bearer …`, look it up
//! via the broker's `resolve_bearer` extension point, surface the result
//! as a four-state outcome the route handler maps to JSON-RPC + HTTP
//! status. Anonymous mode (no header, `require_bearer == false`) and
//! strict mode (no header, `require_bearer == true`) are both routed
//! here so the policy stays single-sourced.

use std::sync::Arc;

use atd_runtime::secrets::{BearerIdentity, BrokerError, TokenBroker};
use axum::http::HeaderMap;

/// Outcome of a single bearer resolution. The `/mcp` handler maps this
/// into either a continuation (`Anonymous` / `Validated`) or an error
/// response (`Rejected`).
#[derive(Debug)]
pub enum BearerOutcome {
    /// No `Authorization` header present and `require_bearer == false`.
    /// The route handler proceeds with an empty `CapabilitySet` and
    /// `caller_id = None` — same defaults as the UDS pre-Hello state
    /// (`atd-server::connection.rs:22-26`).
    Anonymous,
    /// Bearer header present and broker returned `Ok(Some(identity))`.
    /// The route handler builds a per-request `CapabilitySet` from
    /// `identity.granted_capabilities`.
    Validated(BearerIdentity),
    /// Bearer rejected — listener sends back HTTP 401 + JSON-RPC
    /// `-32002` with the contained message.
    Rejected(String),
}

/// Parse the optional `Authorization: Bearer …` header from `headers`,
/// then run the broker policy described in SP-streamable-http §4.4 +
/// §5.6:
///
/// - No header + `require_bearer == false` ⇒ `Anonymous`.
/// - No header + `require_bearer == true` ⇒ `Rejected("Authorization:
///   Bearer required")`.
/// - Header present + `broker.is_some()`: forward to
///   `broker.resolve_bearer`. Map `Ok(Some)` to `Validated`, `Ok(None)`
///   to `Rejected("bearer not recognised")`, and each `BrokerError`
///   variant to the corresponding `Rejected` message (mirrors
///   `celia-cli/src/http_server.rs:154-164`).
/// - Header present + `broker.is_none()` ⇒ `Rejected("token broker not
///   configured")` — the operator made a mistake configuring the
///   server; visible mismatch is better than silent acceptance.
pub async fn resolve_bearer(
    headers: &HeaderMap,
    broker: Option<&Arc<dyn TokenBroker>>,
    require_bearer: bool,
) -> BearerOutcome {
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "));

    let Some(token) = bearer else {
        if require_bearer {
            return BearerOutcome::Rejected("Authorization: Bearer required".into());
        }
        return BearerOutcome::Anonymous;
    };

    let Some(broker) = broker else {
        return BearerOutcome::Rejected(
            "token broker not configured for HTTP bearer auth".into(),
        );
    };

    match broker.resolve_bearer(token).await {
        Ok(Some(identity)) => BearerOutcome::Validated(identity),
        Ok(None) => BearerOutcome::Rejected("bearer not recognised".into()),
        Err(BrokerError::NotConfigured) => {
            // Broker is installed but does not implement bearer flow —
            // operator misconfiguration, not a user error.
            BearerOutcome::Rejected(
                "broker does not support bearer auth (override resolve_bearer)".into(),
            )
        }
        Err(BrokerError::Expired) => BearerOutcome::Rejected("bearer expired".into()),
        Err(BrokerError::Revoked(msg)) => BearerOutcome::Rejected(format!("bearer revoked: {msg}")),
        Err(BrokerError::Lookup(msg)) => BearerOutcome::Rejected(format!("bearer malformed: {msg}")),
        Err(BrokerError::Internal(msg)) => {
            BearerOutcome::Rejected(format!("broker internal error: {msg}"))
        }
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
    }

    #[tokio::test]
    async fn missing_header_strict_mode_returns_rejected() {
        let h = HeaderMap::new();
        let outcome = resolve_bearer(&h, None, true).await;
        match outcome {
            BearerOutcome::Rejected(msg) => assert!(msg.contains("Bearer required")),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bearer_without_broker_rejects() {
        let h = headers_with_bearer("ce_test");
        let outcome = resolve_bearer(&h, None, false).await;
        match outcome {
            BearerOutcome::Rejected(msg) => assert!(msg.contains("token broker not configured")),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    /// Broker that returns a known identity for one token, None otherwise.
    struct FixedBroker {
        good_token: &'static str,
    }
    impl TokenBroker for FixedBroker {
        fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
            Box::pin(async { Ok(None) })
        }
        fn resolve_bearer<'a>(&'a self, bearer: &'a str) -> ResolveBearerFuture<'a> {
            let good = self.good_token;
            let bearer = bearer.to_string();
            Box::pin(async move {
                if bearer == good {
                    Ok(Some(BearerIdentity {
                        caller_id: "agent-X".into(),
                        granted_capabilities: vec!["echo".into()],
                        secrets: None,
                        expires_at: None,
                        cache_until: None,
                    }))
                } else {
                    Ok(None)
                }
            })
        }
    }

    #[tokio::test]
    async fn good_bearer_returns_validated_identity() {
        let broker: Arc<dyn TokenBroker> = Arc::new(FixedBroker {
            good_token: "ce_valid",
        });
        let h = headers_with_bearer("ce_valid");
        let outcome = resolve_bearer(&h, Some(&broker), false).await;
        match outcome {
            BearerOutcome::Validated(id) => {
                assert_eq!(id.caller_id, "agent-X");
                assert_eq!(id.granted_capabilities, vec!["echo".to_string()]);
            }
            other => panic!("expected Validated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_bearer_returns_rejected() {
        let broker: Arc<dyn TokenBroker> = Arc::new(FixedBroker {
            good_token: "ce_valid",
        });
        let h = headers_with_bearer("ce_unknown");
        let outcome = resolve_bearer(&h, Some(&broker), false).await;
        match outcome {
            BearerOutcome::Rejected(msg) => assert!(msg.contains("not recognised")),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    /// Broker that always emits `BrokerError::Expired`. Covers the
    /// SP-streamable-http §5.6 mapping for stale tokens.
    struct ExpiredBroker;
    impl TokenBroker for ExpiredBroker {
        fn resolve<'a>(&'a self, _caller_id: Option<&'a str>) -> ResolveFuture<'a> {
            Box::pin(async { Ok(None) })
        }
        fn resolve_bearer<'a>(&'a self, _bearer: &'a str) -> ResolveBearerFuture<'a> {
            Box::pin(async { Err(BrokerError::Expired) })
        }
    }

    #[tokio::test]
    async fn expired_bearer_maps_to_rejected_with_reason() {
        let broker: Arc<dyn TokenBroker> = Arc::new(ExpiredBroker);
        let h = headers_with_bearer("ce_anything");
        let outcome = resolve_bearer(&h, Some(&broker), false).await;
        match outcome {
            BearerOutcome::Rejected(msg) => assert!(msg.contains("expired")),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn non_bearer_authorization_header_treated_as_missing() {
        let mut h = HeaderMap::new();
        h.insert("authorization", HeaderValue::from_static("Basic Zm9vOmJhcg=="));
        // No "Bearer " prefix → treated as if header absent.
        let outcome = resolve_bearer(&h, None, false).await;
        assert!(matches!(outcome, BearerOutcome::Anonymous));
        let outcome = resolve_bearer(&h, None, true).await;
        assert!(matches!(outcome, BearerOutcome::Rejected(_)));
    }
}
