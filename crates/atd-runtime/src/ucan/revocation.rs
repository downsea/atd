//! SP-capability-v2 revocation-store trait.
//!
//! Phase B.2 ships only the trait (consulted by [`crate::ucan::verify_jwt`]
//! during chain walking). Phase E lands `InMemoryUcanRevocationStore` and
//! the `revoke()` mutator. Adopters that already have a revocation table
//! (e.g. celia's `consent.status='revoked'` rows) wrap that table behind
//! this trait.
//!
//! Spec: [`specs/2026-05-11-sp-capability-v2-design.md`] §4.7

use std::fmt::Debug;

/// A store that can answer "has this UCAN been revoked?" for a CID.
///
/// CID format in v1: SHA-256(jwt_compact) hex-encoded. The verifier
/// computes this on every link; the store decides authoritatively.
///
/// Implementations must be `Send + Sync` because the verifier runs on
/// the per-connection task and the store may be shared via
/// `Arc<dyn UcanRevocationStore>`.
pub trait UcanRevocationStore: Send + Sync + Debug {
    /// Returns `true` if the CID is in the revocation set.
    fn is_revoked(&self, ucan_cid: &str) -> bool;
}
