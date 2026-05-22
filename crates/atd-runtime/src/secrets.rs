//! Token broker extension point for multi-tenant ATD servers.
//!
//! [`TokenBroker`] is the trait an operator implements to map a caller
//! identity (`CallContext::caller_id`, populated from the SP-12 Hello
//! handshake) to a [`SecretBundle`] that gets attached to the
//! [`crate::CallContext`] before `Tool::call` runs. Tools that need secrets
//! read them via [`crate::CallContext::secrets`]; tools that don't, ignore the
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
    /// - `Err(BrokerError::Lookup)` — transient look-up failure (DB
    ///   down, network blip). HTTP listener maps to 503 + `Retry-After: 5`
    ///   per SP-token-broker-phase2 §4.4. Brokers using `Lookup` for
    ///   "malformed bearer" should switch to a synchronous reject path
    ///   (e.g. return `Ok(None)`) so the listener emits 401 +
    ///   `WWW-Authenticate: Bearer error="invalid_token"` instead.
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
///
/// SP-capability-v2 (2026-05-11): gained a UCAN-JWT branch in
/// [`TokenBroker::resolve_bearer`]. Adopters register a mapping from
/// a UCAN `did:key:z...` audience to the caller id they want assigned
/// to that DID via [`Self::register_ucan_audience`]. JWT-shape bearers
/// then resolve to that caller id with the chain's attenuated caps.
/// Non-JWT bearers continue to return [`BrokerError::NotConfigured`]
/// (unchanged from phase 1).
#[derive(Default)]
pub struct InMemoryTokenBroker {
    bundles: HashMap<String, Arc<SecretBundle>>,
    /// SP-capability-v2: `did:key:z...` → `caller_id` mapping. Populated
    /// at pairing time; consulted on every UCAN bearer presentation.
    ucan_audiences: HashMap<String, String>,
    /// SP-capability-v2: per-broker UCAN verifier config. Default uses
    /// `max_chain_depth = 5`, no revocation store. Adopters that want
    /// to share a `SharedServerConfig`-level revocation store with this
    /// broker can override post-construction via [`Self::with_max_chain_depth`]
    /// and [`Self::with_revocation_store`].
    ucan_max_chain_depth: u8,
    ucan_revocation_store: Option<Arc<dyn crate::ucan::UcanRevocationStore>>,
}

impl InMemoryTokenBroker {
    pub fn new() -> Self {
        Self {
            bundles: HashMap::new(),
            ucan_audiences: HashMap::new(),
            ucan_max_chain_depth: 5,
            ucan_revocation_store: None,
        }
    }

    pub fn insert(&mut self, caller_id: impl Into<String>, bundle: SecretBundle) {
        self.bundles.insert(caller_id.into(), Arc::new(bundle));
    }

    /// Register a UCAN `did:key:z...` audience as a known identity for
    /// this broker. A UCAN bearer whose leaf `aud` matches `did_key`
    /// will resolve to [`BearerIdentity::caller_id`] = `caller_id`.
    ///
    /// SP-capability-v2 §4.6 + §6 — celia's analogous mapping is the
    /// new `consent.did_to_grantee` column.
    pub fn register_ucan_audience(
        &mut self,
        did_key: impl Into<String>,
        caller_id: impl Into<String>,
    ) {
        self.ucan_audiences.insert(did_key.into(), caller_id.into());
    }

    /// Set the verifier's max chain depth (default `5`).
    pub fn with_max_chain_depth(mut self, depth: u8) -> Self {
        self.ucan_max_chain_depth = depth;
        self
    }

    /// Attach a revocation store consulted on every UCAN link.
    pub fn with_revocation_store(
        mut self,
        store: Arc<dyn crate::ucan::UcanRevocationStore>,
    ) -> Self {
        self.ucan_revocation_store = Some(store);
        self
    }
}

/// Heuristic: a JWT compact form is `<header>.<payload>.<signature>` —
/// exactly 3 dot-separated, non-empty, base64url-shaped segments.
/// Used by [`InMemoryTokenBroker::resolve_bearer`] to dispatch between
/// the UCAN-JWT branch and the legacy-opaque branch. Cheap; runs once
/// per bearer presentation.
fn looks_like_jwt(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    parts.iter().all(|seg| {
        !seg.is_empty()
            && seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
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

    fn accepted_token_formats(&self) -> &'static [&'static str] {
        // Reference broker advertises both: opaque has a hint-only role
        // (existing phase-1 callers see the same NotConfigured return)
        // and ucan-jwt is the post-SP-capability-v2 path.
        &["ucan-jwt", "opaque"]
    }

    fn resolve_bearer<'a>(&'a self, bearer: &'a str) -> ResolveBearerFuture<'a> {
        Box::pin(async move {
            if !looks_like_jwt(bearer) {
                // Phase-1 semantics preserved: non-JWT bearers are not
                // recognised by the reference broker.
                return Err(BrokerError::NotConfigured);
            }
            // Parse just enough to extract the leaf's audience (the
            // signature is re-verified in full by verify_jwt below).
            // SP-token-broker-phase2 §4.4 wire mapping update: parse
            // failures, unregistered audiences, and verify failures
            // (signature / attenuation / etc.) are all "well-formed-
            // looking but invalid token" → `Ok(None)` → HTTP 401
            // `invalid_token`. Reserve `Err(Lookup)` for transient
            // broker storage failures only; reserve `Err(Internal)`
            // for broker bugs.
            let leaf_payload = match crate::ucan::parse_jwt(bearer) {
                Ok(p) => p,
                Err(_) => return Ok(None),
            };

            let Some(caller_id) = self.ucan_audiences.get(&leaf_payload.aud).cloned() else {
                return Ok(None);
            };

            // The broker pins audience to the JWT's own aud (which we
            // just confirmed maps to a registered caller). Dispatch can
            // further constrain via its own VerifyConfig if needed
            // (Phase C does so on UDS via Hello.client_id).
            let mut cfg = crate::ucan::VerifyConfig::new(leaf_payload.aud.clone());
            cfg.max_chain_depth = self.ucan_max_chain_depth;
            cfg.revocation_store = self.ucan_revocation_store.clone();

            let caps = match crate::ucan::verify_jwt(bearer, &cfg, std::time::SystemTime::now()) {
                Ok(c) => c,
                Err(crate::ucan::UcanVerifyError::Expired { .. }) => {
                    return Err(BrokerError::Expired);
                }
                Err(crate::ucan::UcanVerifyError::Revoked { cid }) => {
                    return Err(BrokerError::Revoked(cid));
                }
                Err(_) => return Ok(None),
            };

            let secrets = self.bundles.get(&caller_id).cloned();
            Ok(Some(BearerIdentity {
                caller_id,
                granted_capabilities: caps.granted(),
                secrets,
                // Phase D leaves expiry hints as None; a follow-up SP
                // can plumb min_exp through ucan::verify_jwt's return
                // shape if real adopters need SSE re-validation cadence.
                expires_at: None,
                cache_until: None,
            }))
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
    async fn non_jwt_bearer_returns_not_configured() {
        // The reference broker's UCAN-JWT branch (SP-capability-v2 Phase
        // D) only engages for 3-segment dot-delimited base64url-shaped
        // input. Opaque tokens like Celia's `ce_<hex>` still fall through
        // to NotConfigured so adopters with their own opaque schemes
        // override resolve_bearer themselves. SP-token-broker-phase2 §4.4.
        let broker = InMemoryTokenBroker::new();
        let err = broker.resolve_bearer("ce_0123456789abcdef").await;
        assert!(matches!(err, Err(BrokerError::NotConfigured)));
    }

    #[test]
    fn default_accepted_token_formats_lists_ucan_jwt_and_opaque() {
        // SP-capability-v2 Phase D: InMemoryTokenBroker now advertises
        // ucan-jwt as its primary accepted format. Listeners surface
        // this through their introspection endpoint.
        let broker = InMemoryTokenBroker::new();
        let formats = broker.accepted_token_formats();
        assert!(formats.contains(&"ucan-jwt"));
        assert!(formats.contains(&"opaque"));
    }

    // ==================== SP-capability-v2 Phase D ====================

    mod ucan_jwt_branch {
        //! Tests for [`InMemoryTokenBroker::resolve_bearer`]'s UCAN-JWT
        //! dispatch. JWT-shape input → parse → audience lookup →
        //! signature/chain verify → BearerIdentity.

        use super::*;
        use base64::Engine;
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use ed25519_dalek::{Signer, SigningKey};
        use serde_json::json;
        use std::time::UNIX_EPOCH;

        fn signing_key_for_seed(seed: u8) -> SigningKey {
            let mut bytes = [0u8; 32];
            bytes[0] = seed;
            SigningKey::from_bytes(&bytes)
        }

        fn did_key_for(sk: &SigningKey) -> String {
            let raw = sk.verifying_key().to_bytes();
            let mut prefixed = Vec::with_capacity(34);
            prefixed.extend_from_slice(&[0xed, 0x01]);
            prefixed.extend_from_slice(&raw);
            let mb = multibase::encode(multibase::Base::Base58Btc, &prefixed);
            format!("did:key:{mb}")
        }

        fn build_jwt(payload: serde_json::Value, sk: &SigningKey) -> String {
            let header = json!({"alg": "EdDSA", "typ": "ucan/1.0+jwt", "ucv": "1.0"});
            let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
            let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
            let signed = format!("{h}.{p}");
            let sig = sk.sign(signed.as_bytes());
            let s = URL_SAFE_NO_PAD.encode(sig.to_bytes());
            format!("{h}.{p}.{s}")
        }

        fn future_exp() -> i64 {
            (std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600) as i64
        }

        fn past_exp() -> i64 {
            (std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 3600) as i64
        }

        fn payload_with(
            iss: &str,
            aud: &str,
            caps: &[&str],
            prf: &[String],
            exp: i64,
        ) -> serde_json::Value {
            json!({
                "iss":  iss,
                "aud":  aud,
                "sub":  iss,
                "cmd":  "atd-cap",
                "args": { "caps": caps, "with": [] },
                "nonce": "test-nonce-fixed",
                "exp":  exp,
                "prf":  prf
            })
        }

        #[tokio::test]
        async fn resolve_bearer_ucan_jwt_returns_identity_from_registered_audience() {
            let sk_user = signing_key_for_seed(1);
            let sk_agent = signing_key_for_seed(2);
            let agent_did = did_key_for(&sk_agent);

            let p = payload_with(
                &did_key_for(&sk_user),
                &agent_did,
                &["records:read"],
                &[],
                future_exp(),
            );
            let jwt = build_jwt(p, &sk_user);

            let mut broker = InMemoryTokenBroker::new();
            broker.register_ucan_audience(&agent_did, "agent:A:hk-9001");

            let identity = broker.resolve_bearer(&jwt).await.unwrap().unwrap();
            assert_eq!(identity.caller_id, "agent:A:hk-9001");
            assert_eq!(identity.granted_capabilities, vec!["records:read"]);
        }

        #[tokio::test]
        async fn resolve_bearer_ucan_jwt_unregistered_aud_rejects_with_lookup() {
            // Audience DID has not been registered → Lookup error (the
            // bearer is well-formed but the broker doesn't know who its
            // intended recipient is).
            let sk_user = signing_key_for_seed(1);
            let sk_agent = signing_key_for_seed(2);
            let p = payload_with(
                &did_key_for(&sk_user),
                &did_key_for(&sk_agent),
                &["records:read"],
                &[],
                future_exp(),
            );
            let jwt = build_jwt(p, &sk_user);

            let broker = InMemoryTokenBroker::new(); // no register_ucan_audience
            // SP-token-broker-phase2 §4.4: well-formed but unrecognised
            // → Ok(None) → 401 invalid_token. (Previously this was
            // BrokerError::Lookup; corrected in phase-2 to match the
            // semantic of "Lookup = transient backend failure".)
            let r = broker.resolve_bearer(&jwt).await;
            assert!(
                matches!(r, Ok(None)),
                "expected Ok(None) for unregistered audience, got {r:?}"
            );
        }

        #[tokio::test]
        async fn resolve_bearer_ucan_jwt_expired_returns_expired() {
            let sk_user = signing_key_for_seed(1);
            let sk_agent = signing_key_for_seed(2);
            let agent_did = did_key_for(&sk_agent);

            let p = payload_with(
                &did_key_for(&sk_user),
                &agent_did,
                &["records:read"],
                &[],
                past_exp(), // expired
            );
            let jwt = build_jwt(p, &sk_user);

            let mut broker = InMemoryTokenBroker::new();
            broker.register_ucan_audience(&agent_did, "agent:A");

            let r = broker.resolve_bearer(&jwt).await;
            assert!(
                matches!(r, Err(BrokerError::Expired)),
                "expected BrokerError::Expired, got {r:?}"
            );
        }

        #[tokio::test]
        async fn resolve_bearer_ucan_jwt_bad_signature_returns_ok_none() {
            // Signed by sk_x but iss claims to be sk_user → BadSignature.
            // SP-token-broker-phase2 §4.4: signature/attenuation/etc.
            // verify failures are "well-formed-looking but invalid token"
            // → Ok(None) → HTTP 401 invalid_token. (Previously surfaced
            // as BrokerError::Lookup, which would have mapped to 503.)
            let sk_user = signing_key_for_seed(1);
            let sk_agent = signing_key_for_seed(2);
            let sk_x = signing_key_for_seed(99);
            let agent_did = did_key_for(&sk_agent);

            let p = payload_with(
                &did_key_for(&sk_user),
                &agent_did,
                &["records:read"],
                &[],
                future_exp(),
            );
            let jwt = build_jwt(p, &sk_x); // wrong signer

            let mut broker = InMemoryTokenBroker::new();
            broker.register_ucan_audience(&agent_did, "agent:A");

            let r = broker.resolve_bearer(&jwt).await;
            assert!(
                matches!(r, Ok(None)),
                "expected Ok(None) for bad signature, got {r:?}"
            );
        }

        #[tokio::test]
        async fn resolve_bearer_ucan_jwt_secrets_attach_when_caller_has_bundle() {
            // The reference broker still supplies secrets via the same
            // bundle table indexed by caller_id — so a UCAN-resolved
            // caller_id with a registered bundle gets that bundle.
            let sk_user = signing_key_for_seed(1);
            let sk_agent = signing_key_for_seed(2);
            let agent_did = did_key_for(&sk_agent);

            let p = payload_with(
                &did_key_for(&sk_user),
                &agent_did,
                &["records:read"],
                &[],
                future_exp(),
            );
            let jwt = build_jwt(p, &sk_user);

            let mut broker = InMemoryTokenBroker::new();
            broker.register_ucan_audience(&agent_did, "agent:A");
            let mut bundle = SecretBundle::new();
            bundle.insert("hms_oauth".into(), RedactedString::new("tok-A"));
            broker.insert("agent:A", bundle);

            let identity = broker.resolve_bearer(&jwt).await.unwrap().unwrap();
            let secrets = identity.secrets.expect("bundle should be attached");
            assert_eq!(secrets.get("hms_oauth").unwrap().expose(), "tok-A");
        }

        #[test]
        fn looks_like_jwt_accepts_three_base64url_segments() {
            assert!(super::super::looks_like_jwt("aaa.bbb.ccc"));
            assert!(super::super::looks_like_jwt("a-b_c.dd-ee_.ffG"));
            assert!(!super::super::looks_like_jwt("only.two"));
            assert!(!super::super::looks_like_jwt("a.b.c.d"));
            assert!(!super::super::looks_like_jwt(".b.c")); // empty seg
            assert!(!super::super::looks_like_jwt("a.b.c$x")); // illegal char
            assert!(!super::super::looks_like_jwt("ce_0123456789abcdef")); // opaque
        }
    }
}
