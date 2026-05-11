//! UCAN-lite parse / verify errors.
//!
//! Parse-stage errors (Phase B.1) all map to the wire-level
//! `ERR_UCAN_INVALID = 1010` constant in `atd-protocol`. Verify-stage
//! errors (Phase B.2) split across `ERR_UCAN_INVALID` (structural /
//! signature), `ERR_UCAN_EXPIRED` (1011), `ERR_DELEGATION_TOO_DEEP`
//! (1012), `ERR_AUDIENCE_MISMATCH` (1013).
//!
//! Spec: [`specs/2026-05-11-sp-capability-v2-design.md`] §4.1 + §5.4

use thiserror::Error;

/// Errors returned by [`crate::ucan::parse_jwt`].
///
/// All variants map to wire code `ERR_UCAN_INVALID = 1010` —
/// `retryable: false` (deterministic; same token → same failure).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UcanParseError {
    /// JWT compact form requires exactly 3 `.`-separated segments.
    #[error("malformed JWT: expected 3 segments, got {0}")]
    MalformedJwt(usize),

    /// Header or payload base64url-decode failed.
    #[error("base64url decode failed in {segment}: {reason}")]
    Base64Decode {
        segment: &'static str,
        reason: String,
    },

    /// Header or payload JSON deserialize failed.
    #[error("JSON parse failed in {segment}: {reason}")]
    JsonParse {
        segment: &'static str,
        reason: String,
    },

    /// `header.alg != "EdDSA"`. Spec §4.3.
    #[error("unsupported alg: expected EdDSA, got {0:?}")]
    UnsupportedAlg(String),

    /// `header.typ != "ucan/1.0+jwt"`. Spec §4.1.
    #[error("unsupported typ: expected ucan/1.0+jwt, got {0:?}")]
    UnsupportedTyp(String),

    /// `header.ucv != "1.0"`. Spec §4.1.
    #[error("unsupported ucv: expected 1.0, got {0:?}")]
    UnsupportedUcv(String),

    /// `payload.cmd != "atd-cap"`. Cross-system replay prevention,
    /// spec §4.5.
    #[error("non-atd-cap UCAN: expected cmd=\"atd-cap\", got {0:?}")]
    NonAtdCap(String),

    /// `iss` or `aud` doesn't start with `did:key:z`. Spec §4.4 —
    /// `did:web` / `did:agent` are deferred to follow-up SPs.
    #[error("unsupported DID method in {field}: {did:?} (only did:key:z... accepted)")]
    UnsupportedDidMethod { field: &'static str, did: String },
}

// Convenience aliases for the parser to use `.map_err(|e| ...)`.
impl UcanParseError {
    pub(crate) fn base64(segment: &'static str, e: impl std::fmt::Display) -> Self {
        Self::Base64Decode {
            segment,
            reason: e.to_string(),
        }
    }

    pub(crate) fn json(segment: &'static str, e: impl std::fmt::Display) -> Self {
        Self::JsonParse {
            segment,
            reason: e.to_string(),
        }
    }
}

/// Errors returned by [`crate::ucan::verify_jwt`] / `verify_tokens`.
///
/// Parse-stage failures bubble up as `Parse(_)` → wire code
/// `ERR_UCAN_INVALID = 1010`. Other verify-stage failures map to:
///
/// | Variant | Wire code |
/// |---|---|
/// | `Parse`, `BadSignature`, `WideningAttenuation`, `MultiParentNotSupported`, `MalformedDidKey`, `MalformedSignature`, `ChainBroken` | 1010 ERR_UCAN_INVALID |
/// | `Expired` | 1011 ERR_UCAN_EXPIRED |
/// | `ChainTooDeep` | 1012 ERR_DELEGATION_TOO_DEEP |
/// | `AudienceMismatch` | 1013 ERR_AUDIENCE_MISMATCH |
/// | `Revoked` | 1010 ERR_UCAN_INVALID (with revoked-cid hint) |
///
/// All `retryable: false` (deterministic).
#[derive(Debug, Error)]
pub enum UcanVerifyError {
    /// Underlying parse failure (any [`UcanParseError`] variant).
    #[error("UCAN parse error: {0}")]
    Parse(#[from] UcanParseError),

    /// Ed25519 signature verification failed for the named token CID.
    #[error("Ed25519 signature verification failed for token {cid}")]
    BadSignature { cid: String },

    /// Signature segment failed to base64url-decode or had wrong length.
    #[error("malformed signature on token {cid}: {reason}")]
    MalformedSignature { cid: String, reason: String },

    /// `did:key:z...` failed to decode (bad multibase, wrong multicodec
    /// prefix, or wrong key length).
    #[error("malformed did:key in {field}: {reason}")]
    MalformedDidKey { field: &'static str, reason: String },

    /// A link's `exp <= now()`. Distinct from `Parse` because the token
    /// was well-formed — it just lapsed.
    #[error("UCAN expired at link {cid}: exp={exp}, now={now}")]
    Expired { cid: String, exp: i64, now: i64 },

    /// Child claims caps the parent did not grant.
    #[error("attenuation widening at link {cid}: parent={parent:?}, child={child:?}")]
    WideningAttenuation {
        cid: String,
        parent: Vec<String>,
        child: Vec<String>,
    },

    /// Link N's `iss` doesn't match link N-1's `aud`. The chain is broken.
    #[error(
        "chain broken between {parent_cid} (aud={parent_aud}) and {child_cid} (iss={child_iss})"
    )]
    ChainBroken {
        parent_cid: String,
        parent_aud: String,
        child_cid: String,
        child_iss: String,
    },

    /// The leaf's `aud` doesn't match the configured expected audience.
    #[error("audience mismatch: leaf.aud={leaf_aud}, expected={expected}")]
    AudienceMismatch { leaf_aud: String, expected: String },

    /// The chain exceeds the configured `max_chain_depth`.
    #[error("chain too deep: depth={depth}, max={max}")]
    ChainTooDeep { depth: u8, max: u8 },

    /// A link's CID was in the revocation store.
    #[error("UCAN revoked: cid={cid}")]
    Revoked { cid: String },

    /// `prf` field has more than one parent. UCAN v1.0 spec allows
    /// multi-parent; SP-capability-v2 v1 supports single-chain only.
    #[error("multi-parent UCAN not supported in v1 (link {cid} has {n_parents} parents)")]
    MultiParentNotSupported { cid: String, n_parents: usize },
}

/// Wire-code mapping per [`UcanVerifyError`] variant.
///
/// Returns `(code, retryable)` for use by dispatch when converting a
/// verify error into a `Response::Error`. All verify errors are
/// non-retryable (deterministic on the same input).
pub fn wire_code(err: &UcanVerifyError) -> u16 {
    use atd_protocol::{
        ERR_AUDIENCE_MISMATCH, ERR_DELEGATION_TOO_DEEP, ERR_UCAN_EXPIRED, ERR_UCAN_INVALID,
    };
    match err {
        UcanVerifyError::Expired { .. } => ERR_UCAN_EXPIRED,
        UcanVerifyError::ChainTooDeep { .. } => ERR_DELEGATION_TOO_DEEP,
        UcanVerifyError::AudienceMismatch { .. } => ERR_AUDIENCE_MISMATCH,
        _ => ERR_UCAN_INVALID,
    }
}
