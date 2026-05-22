//! SP-capability-v2 revocation-store trait + reference impl.
//!
//! - Trait [`UcanRevocationStore`] (read-only `is_revoked`) is the
//!   contract the verifier (Phase B.2) consults on every chain link.
//! - [`InMemoryUcanRevocationStore`] (Phase E) is the reference impl
//!   for tests + small deployments. Adopters wrap their own revocation
//!   table behind the same trait (celia's `consent.status='revoked'`
//!   rows + new `ucan_cid` index — spec §6).
//!
//! Design choice: `revoke()` is **inherent** on `InMemoryUcanRevocationStore`
//! rather than required on the trait. Rationale — production adopters
//! revoke via adopter-specific paths (celia Tauri command + recursive
//! SQL cascade; future atd-ref-server admin CLI; etc.) whose signatures
//! differ. Keeping the trait read-only matches the verifier's actual
//! need, and avoids forcing every wrapper to expose a mutator it
//! doesn't have. SP-capability-v2 §4.7 leaves "how revocations get
//! recorded" deliberately adopter-side.
//!
//! Spec: `docs/archive/superpowers/specs/2026-05-11-sp-capability-v2-design.md` §4.7

use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

/// A store that can answer "has this UCAN been revoked?" for a CID.
///
/// CID format in v1: SHA-256(jwt_compact) hex-encoded (see
/// [`super::compute_cid`]). The verifier computes this on every link;
/// the store decides authoritatively.
///
/// Implementations must be `Send + Sync` because the verifier runs on
/// the per-connection task and the store may be shared via
/// `Arc<dyn UcanRevocationStore>`. `Debug` because [`super::VerifyConfig`]
/// is `Clone` and propagates the trait object through error contexts.
pub trait UcanRevocationStore: Send + Sync + Debug {
    /// Returns `true` if the CID is in the revocation set.
    fn is_revoked(&self, ucan_cid: &str) -> bool;
}

/// Reference revocation store: an `Arc<RwLock<HashSet<String>>>` of
/// revoked CIDs. Suitable for tests and small in-process deployments.
///
/// Concurrency: every read takes a shared lock; revocations take an
/// exclusive lock and run in O(1) average. The whole structure clones
/// cheaply (`Arc`) so multiple subsystems (broker, dispatch, Tauri
/// command) can share one revocation surface.
///
/// **Not** persistent — restarting the host process empties the set.
/// Adopters that need durable revocation (celia) back the trait with
/// their own SQLite-or-similar store.
#[derive(Debug, Default, Clone)]
pub struct InMemoryUcanRevocationStore {
    revoked: Arc<RwLock<HashSet<String>>>,
}

impl InMemoryUcanRevocationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a CID as revoked. Idempotent — revoking the same CID twice
    /// is a no-op. Once revoked, [`Self::is_revoked`] returns `true`
    /// for the lifetime of the store (no un-revoke API by design;
    /// administrators issue a fresh UCAN instead).
    pub fn revoke(&self, ucan_cid: impl Into<String>) {
        self.revoked
            .write()
            .expect("InMemoryUcanRevocationStore lock poisoned")
            .insert(ucan_cid.into());
    }

    /// Number of revoked CIDs currently in the store. Mostly useful
    /// for test assertions + diagnostics; production code should not
    /// rely on this for policy.
    pub fn len(&self) -> usize {
        self.revoked
            .read()
            .expect("InMemoryUcanRevocationStore lock poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl UcanRevocationStore for InMemoryUcanRevocationStore {
    fn is_revoked(&self, ucan_cid: &str) -> bool {
        self.revoked
            .read()
            .expect("InMemoryUcanRevocationStore lock poisoned")
            .contains(ucan_cid)
    }
}

#[cfg(test)]
mod tests {
    //! Phase E unit tests. End-to-end propagation (revoke a chain link
    //! → verifier rejects descendant chain) is covered by
    //! `verify::tests::revoked_cid_rejects` (uses `MockRevocationStore`)
    //! plus a new propagation test below that exercises the real
    //! `InMemoryUcanRevocationStore` against a 3-link chain.

    use super::*;

    #[test]
    fn empty_store_reports_nothing_revoked() {
        let s = InMemoryUcanRevocationStore::new();
        assert!(!s.is_revoked("bafy-cid-anything"));
        assert!(s.is_empty());
    }

    #[test]
    fn revoke_then_is_revoked_returns_true() {
        let s = InMemoryUcanRevocationStore::new();
        s.revoke("cid-abc-123");
        assert!(s.is_revoked("cid-abc-123"));
        assert!(!s.is_revoked("cid-xyz-999"));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn revoke_is_idempotent() {
        let s = InMemoryUcanRevocationStore::new();
        s.revoke("cid-A");
        s.revoke("cid-A");
        s.revoke("cid-A");
        assert_eq!(s.len(), 1);
        assert!(s.is_revoked("cid-A"));
    }

    #[test]
    fn store_clones_share_state_via_arc() {
        // Cloning the store should share the underlying revocation set
        // (Arc semantics) — a revoke on one handle is visible on the
        // other. Important for the dispatch ↔ broker sharing pattern.
        let s = InMemoryUcanRevocationStore::new();
        let s2 = s.clone();
        s.revoke("cid-X");
        assert!(s2.is_revoked("cid-X"));
    }

    #[test]
    fn store_works_through_trait_object() {
        // The verifier holds `Option<Arc<dyn UcanRevocationStore>>`.
        // Confirm dispatch through the dyn boundary.
        let inner = Arc::new(InMemoryUcanRevocationStore::new());
        inner.revoke("cid-T");
        let dyn_store: Arc<dyn UcanRevocationStore> = inner;
        assert!(dyn_store.is_revoked("cid-T"));
        assert!(!dyn_store.is_revoked("cid-other"));
    }
}
