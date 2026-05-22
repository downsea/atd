//! UCAN-lite payload types.
//!
//! Decoded from a JWT compact form by [`crate::ucan::parse_jwt`]. Pure
//! data — no behaviour, no allocations beyond the strings these fields
//! own.
//!
//! Spec: `docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md` §4.1, §4.5, §5.1

use serde::{Deserialize, Serialize};

/// JWT header — the part before the first `.` in the compact form.
///
/// Spec §4.1 + §4.3: `alg` must be `EdDSA`, `typ` must be `ucan/1.0+jwt`,
/// `ucv` must be `1.0`. Anything else is rejected at parse time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UcanHeader {
    pub alg: String,
    pub typ: String,
    pub ucv: String,
}

/// UCAN capability inside `payload.args` — a flat list of ATD capability
/// strings (`records:read`, `fs.write`, ...) plus optional resource
/// bindings (e.g. `{"patient": "Patient/X"}`).
///
/// Spec §4.5 — `with` reserved for future binding kinds; v1 supports
/// `{"patient": "..."}` only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UcanCapability {
    pub caps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub with: Vec<serde_json::Value>,
}

/// UCAN payload — the middle segment of the JWT compact form.
///
/// Spec §5.1: full canonical "A→B delegates read-only Patient/X access"
/// example. `prf` carries parent UCAN(s) inline so verification stays
/// self-contained (no out-of-band fetches).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UcanPayload {
    /// Issuer DID (the principal granting authority).
    pub iss: String,

    /// Audience DID (the principal authorised to act).
    pub aud: String,

    /// Subject DID — the resource owner / root authority.
    pub sub: String,

    /// Reserved namespace sentinel; must be `"atd-cap"` for ATD-bound
    /// tokens. Cross-system replay (e.g. a Bluesky UCAN) is structurally
    /// prevented by this discriminator (spec §4.5).
    pub cmd: String,

    /// Capabilities + optional resource bindings.
    pub args: UcanCapability,

    /// 16-byte random nonce (base64url-encoded).
    pub nonce: String,

    /// Unix-seconds expiry. Verified against `SystemTime::now()` at
    /// chain-walk time (Phase B.2).
    pub exp: i64,

    /// Parent UCANs (each itself a JWT compact form). Forms the
    /// delegation chain. Empty for root UCANs signed by the resource
    /// owner.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prf: Vec<String>,
}
