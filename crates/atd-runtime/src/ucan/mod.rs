//! SP-capability-v2 UCAN-lite implementation.
//!
//! UCAN-lite is a profile of UCAN v1.0 narrowed for ATD's use case:
//! - JWT compact form on the wire (not DAG-CBOR / CIDv1).
//! - `alg = "EdDSA"`, `typ = "ucan/1.0+jwt"`, `ucv = "1.0"`.
//! - `did:key` issuer / audience only (no `did:web`, no `did:plc`).
//! - Capabilities tunneled as `cmd = "atd-cap"`, `args.caps: Vec<String>`,
//!   `args.with: Vec<{patient: String}>` (or other binding kinds reserved).
//!
//! Phasing (per [`plans/2026-05-11-sp-capability-v2.md`]):
//! - Phase B.1 ([`parse`]): structural decoder; no signature, no chain walk.
//! - Phase B.2 (`verify`, not yet landed): chain attenuation, signature
//!   verification (Ed25519), audience pinning, depth limit, revocation
//!   consultation. Lands in a follow-up commit on the same SP.
//!
//! Spec: [`specs/2026-05-11-sp-capability-v2-design.md`] §4.1–§4.7

pub mod error;
pub mod parse;
pub mod types;

pub use error::UcanParseError;
pub use parse::parse_jwt;
pub use types::{UcanCapability, UcanHeader, UcanPayload};
