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
    Base64Decode { segment: &'static str, reason: String },

    /// Header or payload JSON deserialize failed.
    #[error("JSON parse failed in {segment}: {reason}")]
    JsonParse { segment: &'static str, reason: String },

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
