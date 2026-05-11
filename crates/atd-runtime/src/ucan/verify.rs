//! SP-capability-v2 Phase B.2 — UCAN-lite signature + chain verifier.
//!
//! Walks a UCAN chain root→leaf, verifying:
//! 1. Each link's Ed25519 signature against its `iss`'s did:key
//! 2. Chain integrity: `parent.aud == child.iss` between adjacent links
//! 3. Attenuation: `child.args.caps ⊆ parent.args.caps`
//! 4. Each link's `exp > now()`
//! 5. The leaf's `aud == expected_audience` (audience pinning)
//! 6. Chain depth ≤ `max_chain_depth`
//! 7. No link's CID is in the revocation store
//!
//! On success returns the effective [`CapabilitySet`] — the leaf's
//! `args.caps`, which after attenuation walking is already the
//! intersection of all link caps.
//!
//! Spec: [`specs/2026-05-11-sp-capability-v2-design.md`] §4.6 + §4.7

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use super::error::UcanVerifyError;
use super::parse::parse_jwt;
use super::revocation::UcanRevocationStore;
use super::types::UcanPayload;
use crate::capability::CapabilitySet;

/// Configuration for chain verification.
#[derive(Clone)]
pub struct VerifyConfig {
    /// Hard cap on chain depth. Default 5 per spec §4.6. Prevents
    /// stack-exhaustion via pathologically deep proof chains.
    pub max_chain_depth: u8,

    /// The leaf's `aud` must equal this value. Bound by dispatch to
    /// the connection's `client_id` (UDS) or the bearer's caller (HTTP).
    pub expected_audience: String,

    /// Optional revocation store. When `None`, no revocation check is
    /// performed (suitable for tests / closed-system adopters). When
    /// `Some`, every link's CID is consulted.
    pub revocation_store: Option<Arc<dyn UcanRevocationStore>>,
}

impl VerifyConfig {
    /// Construct a config with the default `max_chain_depth = 5` and
    /// no revocation store.
    pub fn new(expected_audience: impl Into<String>) -> Self {
        Self {
            max_chain_depth: 5,
            expected_audience: expected_audience.into(),
            revocation_store: None,
        }
    }
}

/// Compute the canonical UCAN CID for a JWT compact form.
///
/// v1: SHA-256(jwt_bytes) hex-encoded (lowercase). Not a true IPLD
/// CIDv1 — the spec deliberately diverges from UCAN v1.0's DAG-CBOR
/// path (see SP-capability-v2 §4.1 rationale). The hash is sufficient
/// for revocation-store keying and is deterministic across implementations.
pub fn compute_cid(jwt: &str) -> String {
    let mut h = Sha256::new();
    h.update(jwt.as_bytes());
    hex_encode(&h.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Extract an Ed25519 public key from a `did:key:z<base58btc>` DID.
///
/// Multicodec prefix for Ed25519 in did:key is `0xed 0x01`; the
/// remaining 32 bytes are the raw public key. Any other prefix
/// (P-256, secp256k1, RSA) is rejected — UCAN-lite v1 is Ed25519-only.
fn parse_did_key(did: &str, field: &'static str) -> Result<VerifyingKey, UcanVerifyError> {
    let suffix = did
        .strip_prefix("did:key:")
        .ok_or_else(|| UcanVerifyError::MalformedDidKey {
            field,
            reason: "missing did:key: prefix".into(),
        })?;
    let (base, bytes) =
        multibase::decode(suffix).map_err(|e| UcanVerifyError::MalformedDidKey {
            field,
            reason: format!("multibase decode: {e}"),
        })?;
    if base != multibase::Base::Base58Btc {
        return Err(UcanVerifyError::MalformedDidKey {
            field,
            reason: format!("expected base58btc multibase prefix 'z', got {base:?}"),
        });
    }
    if bytes.len() != 34 {
        return Err(UcanVerifyError::MalformedDidKey {
            field,
            reason: format!(
                "expected 34 bytes (2-byte multicodec + 32-byte key), got {}",
                bytes.len()
            ),
        });
    }
    if bytes[0..2] != [0xed, 0x01] {
        return Err(UcanVerifyError::MalformedDidKey {
            field,
            reason: format!(
                "non-Ed25519 multicodec prefix: 0x{:02x}{:02x} (Ed25519 is 0xed01)",
                bytes[0], bytes[1]
            ),
        });
    }
    let key_arr: [u8; 32] = bytes[2..34]
        .try_into()
        .expect("just bounds-checked: bytes has 34 bytes, [2..34] is 32");
    VerifyingKey::from_bytes(&key_arr).map_err(|e| UcanVerifyError::MalformedDidKey {
        field,
        reason: format!("Ed25519 key bytes invalid: {e}"),
    })
}

/// Verify the Ed25519 signature on a JWT compact form against the
/// public key encoded in its `iss` did:key.
fn verify_signature(jwt: &str, payload: &UcanPayload) -> Result<(), UcanVerifyError> {
    let cid = compute_cid(jwt);
    let parts: Vec<&str> = jwt.split('.').collect();
    // parse_jwt already guaranteed 3 segments by the time we get here,
    // but assert defensively rather than panic.
    if parts.len() != 3 {
        return Err(UcanVerifyError::MalformedSignature {
            cid: cid.clone(),
            reason: format!(
                "expected 3 JWT segments at verify-time, got {}",
                parts.len()
            ),
        });
    }
    let signed_bytes = format!("{}.{}", parts[0], parts[1]).into_bytes();
    let sig_bytes =
        URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|e| UcanVerifyError::MalformedSignature {
                cid: cid.clone(),
                reason: format!("signature base64url decode: {e}"),
            })?;
    if sig_bytes.len() != 64 {
        return Err(UcanVerifyError::MalformedSignature {
            cid: cid.clone(),
            reason: format!(
                "Ed25519 signature must be 64 bytes, got {}",
                sig_bytes.len()
            ),
        });
    }
    let sig =
        Signature::from_slice(&sig_bytes).map_err(|e| UcanVerifyError::MalformedSignature {
            cid: cid.clone(),
            reason: format!("signature parse: {e}"),
        })?;

    let pubkey = parse_did_key(&payload.iss, "iss")?;
    pubkey
        .verify(&signed_bytes, &sig)
        .map_err(|_| UcanVerifyError::BadSignature { cid })?;
    Ok(())
}

/// One parsed link in the chain — keeps the raw JWT around for CID
/// computation + signature verification re-checks.
struct Link {
    jwt: String,
    payload: UcanPayload,
}

impl Link {
    fn cid(&self) -> String {
        compute_cid(&self.jwt)
    }
}

/// Walk a leaf UCAN's `prf` chain bottom-up (leaf → root) and return
/// the links ordered ROOT-FIRST.
///
/// Rejects multi-parent UCANs (`prf.len() > 1`) — v1 supports
/// single-chain only. Rejects chains longer than `max_chain_depth`.
fn collect_chain(leaf_jwt: &str, max_depth: u8) -> Result<Vec<Link>, UcanVerifyError> {
    let mut leaf_first: Vec<Link> = Vec::new();
    let mut cur_jwt = leaf_jwt.to_string();
    loop {
        let payload = parse_jwt(&cur_jwt)?;
        if payload.prf.len() > 1 {
            return Err(UcanVerifyError::MultiParentNotSupported {
                cid: compute_cid(&cur_jwt),
                n_parents: payload.prf.len(),
            });
        }
        let next = payload.prf.first().cloned();
        leaf_first.push(Link {
            jwt: cur_jwt,
            payload,
        });
        // Depth check: bail before we waste a parse on link N+1 when
        // we've already accepted `max_depth` links.
        if leaf_first.len() as u8 > max_depth {
            return Err(UcanVerifyError::ChainTooDeep {
                depth: leaf_first.len() as u8,
                max: max_depth,
            });
        }
        match next {
            Some(parent_jwt) => cur_jwt = parent_jwt,
            None => break,
        }
    }
    leaf_first.reverse(); // → root-first
    Ok(leaf_first)
}

/// Verify a single leaf UCAN-lite JWT and return the effective
/// capability set.
///
/// Performs all seven checks in §4.6 in this order: chain assembly +
/// depth → per-link signature → expiry → chain integrity (aud ↔ iss
/// between adjacent links) → attenuation → audience pin (leaf.aud) →
/// revocation. Failures short-circuit at the first violation.
pub fn verify_jwt(
    leaf_jwt: &str,
    cfg: &VerifyConfig,
    now: SystemTime,
) -> Result<CapabilitySet, UcanVerifyError> {
    let chain = collect_chain(leaf_jwt, cfg.max_chain_depth)?;
    let now_secs: i64 = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // (1) Per-link signature verification.
    for link in &chain {
        verify_signature(&link.jwt, &link.payload)?;
    }

    // (2) Expiry.
    for link in &chain {
        if link.payload.exp <= now_secs {
            return Err(UcanVerifyError::Expired {
                cid: link.cid(),
                exp: link.payload.exp,
                now: now_secs,
            });
        }
    }

    // (3) Chain integrity + (4) attenuation, walking root → leaf.
    for i in 1..chain.len() {
        let parent = &chain[i - 1];
        let child = &chain[i];
        if parent.payload.aud != child.payload.iss {
            return Err(UcanVerifyError::ChainBroken {
                parent_cid: parent.cid(),
                parent_aud: parent.payload.aud.clone(),
                child_cid: child.cid(),
                child_iss: child.payload.iss.clone(),
            });
        }
        // Attenuation: every child cap must appear in the parent's caps.
        let parent_caps: &Vec<String> = &parent.payload.args.caps;
        let child_caps: &Vec<String> = &child.payload.args.caps;
        for c in child_caps {
            if !parent_caps.contains(c) {
                return Err(UcanVerifyError::WideningAttenuation {
                    cid: child.cid(),
                    parent: parent_caps.clone(),
                    child: child_caps.clone(),
                });
            }
        }
    }

    // (5) Audience pin on leaf.
    let leaf = chain
        .last()
        .expect("collect_chain returns at least one link");
    if leaf.payload.aud != cfg.expected_audience {
        return Err(UcanVerifyError::AudienceMismatch {
            leaf_aud: leaf.payload.aud.clone(),
            expected: cfg.expected_audience.clone(),
        });
    }

    // (6) Revocation check on every link.
    if let Some(store) = &cfg.revocation_store {
        for link in &chain {
            let cid = link.cid();
            if store.is_revoked(&cid) {
                return Err(UcanVerifyError::Revoked { cid });
            }
        }
    }

    // (7) Effective caps = leaf's caps (already attenuation-validated).
    Ok(CapabilitySet::from_iter(leaf.payload.args.caps.clone()))
}

/// Verify multiple independent UCAN chains and union their capability
/// sets. Spec §5.3 — a Hello carrying N roots gets each verified
/// independently; granted = ∪ each_chain's_effective_caps.
///
/// All chains must verify successfully; one failure rejects the whole
/// Hello (no partial grants).
pub fn verify_tokens(
    tokens: &[String],
    cfg: &VerifyConfig,
    now: SystemTime,
) -> Result<CapabilitySet, UcanVerifyError> {
    let mut acc = CapabilitySet::default();
    for tok in tokens {
        let chain_caps = verify_jwt(tok, cfg, now)?;
        acc = acc.union(&chain_caps);
    }
    Ok(acc)
}

// =================== TESTS ===================

#[cfg(test)]
mod tests {
    //! Phase B.2 verify-stage tests — spec §8.1 cases 5-11 (the chain
    //! walker / signature / attenuation / revocation set).
    //!
    //! `test_helpers::build_chain` signs each link with a fresh Ed25519
    //! keypair derived deterministically from the link's depth so test
    //! assertions can name specific DIDs in advance.

    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    use std::sync::Mutex;
    use std::time::Duration;

    // ---- helpers -----------------------------------------------------------

    fn signing_key_for_seed(seed: u8) -> SigningKey {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        SigningKey::from_bytes(&bytes)
    }

    /// Encode `<multicodec-prefix><raw-32-byte-pubkey>` as
    /// `did:key:z<base58btc-multibase>`.
    fn did_key_for(sk: &SigningKey) -> String {
        let raw = sk.verifying_key().to_bytes();
        let mut prefixed = Vec::with_capacity(34);
        prefixed.extend_from_slice(&[0xed, 0x01]);
        prefixed.extend_from_slice(&raw);
        let mb = multibase::encode(multibase::Base::Base58Btc, &prefixed);
        format!("did:key:{mb}")
    }

    /// Build one UCAN JWT compact form signed by `sk`. The header is
    /// canonical (alg=EdDSA, typ=ucan/1.0+jwt, ucv=1.0); `payload` is
    /// passed in to allow per-test mutation. The signature is computed
    /// over `<header>.<payload>` per JWT compact-form rules.
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        now + 3600
    }

    /// A minimal payload for cases where the test doesn't care about
    /// most fields.
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
            "nonce": "test-nonce-fixed-value",
            "exp":  exp,
            "prf":  prf
        })
    }

    /// A fake revocation store for tests. Lock-protected so the test
    /// can mutate the revoked set.
    #[derive(Debug, Default)]
    struct MockRevocationStore {
        revoked: Mutex<Vec<String>>,
    }
    impl MockRevocationStore {
        fn revoke(&self, cid: &str) {
            self.revoked.lock().unwrap().push(cid.to_string());
        }
    }
    impl UcanRevocationStore for MockRevocationStore {
        fn is_revoked(&self, cid: &str) -> bool {
            self.revoked.lock().unwrap().iter().any(|c| c == cid)
        }
    }

    // ---- spec §8.1 cases ---------------------------------------------------

    #[test]
    fn verify_well_formed_single_link_chain_succeeds() {
        // Baseline — a root UCAN signed by the resource owner, with no prf.
        let sk_a = signing_key_for_seed(1);
        let sk_b = signing_key_for_seed(2);
        let p = payload_with(
            &did_key_for(&sk_a),
            &did_key_for(&sk_b),
            &["records:read"],
            &[],
            future_exp(),
        );
        let jwt = build_jwt(p, &sk_a);
        let cfg = VerifyConfig::new(did_key_for(&sk_b));
        let caps = verify_jwt(&jwt, &cfg, SystemTime::now()).expect("baseline must verify");
        assert!(caps.contains("records:read"));
    }

    #[test]
    fn verify_signature_with_wrong_key_rejects() {
        // Issuer DID claims sk_a, but the JWT was signed by sk_x.
        let sk_a = signing_key_for_seed(1);
        let sk_b = signing_key_for_seed(2);
        let sk_x = signing_key_for_seed(99);
        let p = payload_with(
            &did_key_for(&sk_a), // iss claims to be A
            &did_key_for(&sk_b),
            &["records:read"],
            &[],
            future_exp(),
        );
        let jwt = build_jwt(p, &sk_x); // but signed by X
        let cfg = VerifyConfig::new(did_key_for(&sk_b));
        match verify_jwt(&jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::BadSignature { .. }) => {}
            other => panic!("expected BadSignature, got {other:?}"),
        }
    }

    #[test]
    fn expired_token_returns_err_expired() {
        let sk_a = signing_key_for_seed(1);
        let sk_b = signing_key_for_seed(2);
        let past_exp = (SystemTime::now() - Duration::from_secs(3600))
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let p = payload_with(
            &did_key_for(&sk_a),
            &did_key_for(&sk_b),
            &["records:read"],
            &[],
            past_exp,
        );
        let jwt = build_jwt(p, &sk_a);
        let cfg = VerifyConfig::new(did_key_for(&sk_b));
        match verify_jwt(&jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::Expired { exp, now, .. }) => {
                assert!(exp <= now, "exp should be ≤ now in the Expired error");
            }
            other => panic!("expected Expired, got {other:?}"),
        }
    }

    #[test]
    fn chain_depth_exceeded_rejects() {
        // Build a 6-deep chain (root → 5 children); default max is 5.
        let sks: Vec<SigningKey> = (0..6).map(|i| signing_key_for_seed(i + 1)).collect();
        let exp = future_exp();
        // Build root → leaf, accumulating prf.
        let mut current_prf: Vec<String> = vec![];
        let mut latest_jwt = String::new();
        for i in 0..6 {
            let iss = did_key_for(&sks[i]);
            let aud = did_key_for(&sks[(i + 1) % 6]); // arbitrary; chain validity isn't what we're testing
            let p = payload_with(&iss, &aud, &["records:read"], &current_prf, exp);
            latest_jwt = build_jwt(p, &sks[i]);
            current_prf = vec![latest_jwt.clone()];
        }
        let cfg = VerifyConfig::new(did_key_for(&sks[0])); // bogus audience for this test
        match verify_jwt(&latest_jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::ChainTooDeep { depth, max }) => {
                assert!(depth > max, "depth={depth} must exceed max={max}");
                assert_eq!(max, 5);
            }
            other => panic!("expected ChainTooDeep, got {other:?}"),
        }
    }

    #[test]
    fn audience_mismatch_rejects() {
        let sk_a = signing_key_for_seed(1);
        let sk_b = signing_key_for_seed(2);
        let sk_c = signing_key_for_seed(3);
        let p = payload_with(
            &did_key_for(&sk_a),
            &did_key_for(&sk_b),
            &["records:read"],
            &[],
            future_exp(),
        );
        let jwt = build_jwt(p, &sk_a);
        let cfg = VerifyConfig::new(did_key_for(&sk_c)); // expect C, but leaf.aud = B
        match verify_jwt(&jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::AudienceMismatch { leaf_aud, expected }) => {
                assert_eq!(leaf_aud, did_key_for(&sk_b));
                assert_eq!(expected, did_key_for(&sk_c));
            }
            other => panic!("expected AudienceMismatch, got {other:?}"),
        }
    }

    #[test]
    fn attenuation_intersect_succeeds() {
        // U → A grants [records:read, summary:read, fs.write]
        //   → B grants [records:read, summary:read]   (drops fs.write)
        //     → C grants [records:read]               (drops summary:read)
        // Effective: [records:read]
        let sk_u = signing_key_for_seed(1);
        let sk_a = signing_key_for_seed(2);
        let sk_b = signing_key_for_seed(3);
        let sk_c = signing_key_for_seed(4);
        let exp = future_exp();

        let root = payload_with(
            &did_key_for(&sk_u),
            &did_key_for(&sk_a),
            &["records:read", "summary:read", "fs.write"],
            &[],
            exp,
        );
        let root_jwt = build_jwt(root, &sk_u);

        let mid = payload_with(
            &did_key_for(&sk_a),
            &did_key_for(&sk_b),
            &["records:read", "summary:read"],
            &[root_jwt.clone()],
            exp,
        );
        let mid_jwt = build_jwt(mid, &sk_a);

        let leaf = payload_with(
            &did_key_for(&sk_b),
            &did_key_for(&sk_c),
            &["records:read"],
            &[mid_jwt.clone()],
            exp,
        );
        let leaf_jwt = build_jwt(leaf, &sk_b);

        let cfg = VerifyConfig::new(did_key_for(&sk_c));
        let caps = verify_jwt(&leaf_jwt, &cfg, SystemTime::now())
            .expect("3-link attenuated chain must verify");
        assert!(caps.contains("records:read"));
        assert!(!caps.contains("summary:read"));
        assert!(!caps.contains("fs.write"));
    }

    #[test]
    fn attenuation_widening_rejects() {
        // Parent grants [a, b, c]; child claims [a, b, c, d]. Widening
        // — child must not gain a cap the parent didn't grant.
        let sk_u = signing_key_for_seed(1);
        let sk_a = signing_key_for_seed(2);
        let sk_b = signing_key_for_seed(3);
        let exp = future_exp();

        let root = payload_with(
            &did_key_for(&sk_u),
            &did_key_for(&sk_a),
            &["a", "b", "c"],
            &[],
            exp,
        );
        let root_jwt = build_jwt(root, &sk_u);

        let leaf = payload_with(
            &did_key_for(&sk_a),
            &did_key_for(&sk_b),
            &["a", "b", "c", "d"], // adds "d" — widening
            &[root_jwt.clone()],
            exp,
        );
        let leaf_jwt = build_jwt(leaf, &sk_a);

        let cfg = VerifyConfig::new(did_key_for(&sk_b));
        match verify_jwt(&leaf_jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::WideningAttenuation { parent, child, .. }) => {
                assert_eq!(parent, vec!["a", "b", "c"]);
                assert_eq!(child, vec!["a", "b", "c", "d"]);
            }
            other => panic!("expected WideningAttenuation, got {other:?}"),
        }
    }

    #[test]
    fn revoked_cid_rejects() {
        let sk_a = signing_key_for_seed(1);
        let sk_b = signing_key_for_seed(2);
        let p = payload_with(
            &did_key_for(&sk_a),
            &did_key_for(&sk_b),
            &["records:read"],
            &[],
            future_exp(),
        );
        let jwt = build_jwt(p, &sk_a);
        let cid = compute_cid(&jwt);

        let store = Arc::new(MockRevocationStore::default());
        store.revoke(&cid);

        let mut cfg = VerifyConfig::new(did_key_for(&sk_b));
        cfg.revocation_store = Some(store as Arc<dyn UcanRevocationStore>);

        match verify_jwt(&jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::Revoked { cid: c }) => assert_eq!(c, cid),
            other => panic!("expected Revoked, got {other:?}"),
        }
    }

    // ---- Phase E: real InMemoryUcanRevocationStore × multi-link chain ----

    #[test]
    fn revoking_root_cid_via_in_memory_store_rejects_3_link_descendant() {
        // U → A → B → C 3-link chain. Revoke the ROOT (U's UCAN) and
        // confirm the LEAF (C's request) rejects with the root's CID
        // surfaced. Exercises the real InMemoryUcanRevocationStore impl
        // (vs the MockRevocationStore in `revoked_cid_rejects`) and the
        // verifier's "consult on every link" guarantee.
        use super::super::InMemoryUcanRevocationStore;

        let sk_u = signing_key_for_seed(1);
        let sk_a = signing_key_for_seed(2);
        let sk_b = signing_key_for_seed(3);
        let sk_c = signing_key_for_seed(4);
        let exp = future_exp();

        let root = payload_with(
            &did_key_for(&sk_u),
            &did_key_for(&sk_a),
            &["records:read"],
            &[],
            exp,
        );
        let root_jwt = build_jwt(root, &sk_u);
        let root_cid = compute_cid(&root_jwt);

        let mid = payload_with(
            &did_key_for(&sk_a),
            &did_key_for(&sk_b),
            &["records:read"],
            &[root_jwt.clone()],
            exp,
        );
        let mid_jwt = build_jwt(mid, &sk_a);

        let leaf = payload_with(
            &did_key_for(&sk_b),
            &did_key_for(&sk_c),
            &["records:read"],
            &[mid_jwt],
            exp,
        );
        let leaf_jwt = build_jwt(leaf, &sk_b);

        // Pre-revoke: full 3-link chain verifies.
        let store = Arc::new(InMemoryUcanRevocationStore::new());
        let mut cfg = VerifyConfig::new(did_key_for(&sk_c));
        cfg.revocation_store = Some(store.clone() as Arc<dyn UcanRevocationStore>);
        assert!(verify_jwt(&leaf_jwt, &cfg, SystemTime::now()).is_ok());

        // Revoke ROOT → leaf request rejects with root's CID.
        store.revoke(&root_cid);
        match verify_jwt(&leaf_jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::Revoked { cid }) => assert_eq!(cid, root_cid),
            other => panic!("expected Revoked at root cid, got {other:?}"),
        }
    }

    // ---- additional coverage ----------------------------------------------

    #[test]
    fn chain_broken_when_parent_aud_ne_child_iss_rejects() {
        // Build a chain where parent's aud is sk_X but child's iss is sk_Y
        // (sk_Y signs the child to make the signature valid, but the
        // delegation chain is broken).
        let sk_u = signing_key_for_seed(1);
        let sk_x = signing_key_for_seed(10); // legitimate child of U
        let sk_y = signing_key_for_seed(20); // a different agent
        let sk_b = signing_key_for_seed(30);
        let exp = future_exp();

        let root = payload_with(
            &did_key_for(&sk_u),
            &did_key_for(&sk_x),
            &["records:read"],
            &[],
            exp,
        );
        let root_jwt = build_jwt(root, &sk_u);

        // Child's iss claims to be Y (not X). Y signs it — signature OK.
        // But parent.aud (X) ≠ child.iss (Y) → ChainBroken.
        let leaf = payload_with(
            &did_key_for(&sk_y),
            &did_key_for(&sk_b),
            &["records:read"],
            &[root_jwt],
            exp,
        );
        let leaf_jwt = build_jwt(leaf, &sk_y);

        let cfg = VerifyConfig::new(did_key_for(&sk_b));
        match verify_jwt(&leaf_jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::ChainBroken { .. }) => {}
            other => panic!("expected ChainBroken, got {other:?}"),
        }
    }

    #[test]
    fn multi_parent_prf_rejects() {
        let sk_a = signing_key_for_seed(1);
        let sk_b = signing_key_for_seed(2);
        // Build a payload with two parent JWTs in prf — unsupported in v1.
        let leaf = json!({
            "iss":   did_key_for(&sk_a),
            "aud":   did_key_for(&sk_b),
            "sub":   did_key_for(&sk_a),
            "cmd":   "atd-cap",
            "args":  { "caps": ["records:read"], "with": [] },
            "nonce": "nonce",
            "exp":   future_exp(),
            "prf":   ["parent1.jwt.placeholder", "parent2.jwt.placeholder"]
        });
        let jwt = build_jwt(leaf, &sk_a);
        let cfg = VerifyConfig::new(did_key_for(&sk_b));
        match verify_jwt(&jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::MultiParentNotSupported { n_parents, .. }) => {
                assert_eq!(n_parents, 2);
            }
            other => panic!("expected MultiParentNotSupported, got {other:?}"),
        }
    }

    #[test]
    fn verify_tokens_unions_multi_root_results() {
        // Spec §5.3: two independent root chains, each granting one cap.
        // Granted set = union.
        let sk_u1 = signing_key_for_seed(1);
        let sk_u2 = signing_key_for_seed(2);
        let sk_b = signing_key_for_seed(99);
        let exp = future_exp();

        let p1 = payload_with(
            &did_key_for(&sk_u1),
            &did_key_for(&sk_b),
            &["records:read"],
            &[],
            exp,
        );
        let jwt1 = build_jwt(p1, &sk_u1);

        let p2 = payload_with(
            &did_key_for(&sk_u2),
            &did_key_for(&sk_b),
            &["summary:read"],
            &[],
            exp,
        );
        let jwt2 = build_jwt(p2, &sk_u2);

        let cfg = VerifyConfig::new(did_key_for(&sk_b));
        let caps = verify_tokens(&[jwt1, jwt2], &cfg, SystemTime::now())
            .expect("two independent valid chains must verify and union");
        assert!(caps.contains("records:read"));
        assert!(caps.contains("summary:read"));
    }

    #[test]
    fn malformed_did_key_at_iss_rejects_at_verify_stage() {
        // parse_jwt requires only the "did:key:z" prefix — invalid
        // multibase payload after that prefix is caught at signature-
        // verify time when we try to extract the public key.
        let sk_a = signing_key_for_seed(1);
        let sk_b = signing_key_for_seed(2);
        let p = json!({
            "iss":   "did:key:zNOTAREALKEY", // valid prefix; garbage payload
            "aud":   did_key_for(&sk_b),
            "sub":   did_key_for(&sk_a),
            "cmd":   "atd-cap",
            "args":  { "caps": ["records:read"], "with": [] },
            "nonce": "nonce",
            "exp":   future_exp(),
        });
        let jwt = build_jwt(p, &sk_a);
        let cfg = VerifyConfig::new(did_key_for(&sk_b));
        match verify_jwt(&jwt, &cfg, SystemTime::now()) {
            Err(UcanVerifyError::MalformedDidKey { .. }) => {}
            other => panic!("expected MalformedDidKey, got {other:?}"),
        }
    }
}
