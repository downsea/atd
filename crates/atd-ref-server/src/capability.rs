//! Connection-scoped capability allow-list.
//!
//! SP-12 demonstrates the §VI least-privilege shape without committing to a
//! cryptographic token format. Capabilities are trusted free-form strings the
//! server operator declares at start time via `--grant-capability`; the
//! connection's `Hello` request asks for a subset, and the server replies with
//! what it granted. A future SP can swap this allow-list for UCAN verification
//! without changing `CapabilitySet`'s public surface.

use std::collections::BTreeSet;

/// Set of capabilities currently granted to a connection. Deterministically
/// ordered (BTreeSet) so `granted()` returns a stable sequence — important for
/// wire-level reproducibility and test assertions.
#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    granted: BTreeSet<String>,
}

impl CapabilitySet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        Self {
            granted: iter.into_iter().collect(),
        }
    }

    pub fn contains(&self, cap: &str) -> bool {
        self.granted.contains(cap)
    }

    /// Sorted, deterministic view of the granted set.
    pub fn granted(&self) -> Vec<String> {
        self.granted.iter().cloned().collect()
    }

    /// Intersect `requested` with the server's allow-list. Returns
    /// `(granted_subset, denied_subset)`, each deterministically ordered.
    /// Used during the `Hello` handshake to decide what to echo back.
    pub fn intersect(&self, requested: &[String]) -> (Vec<String>, Vec<String>) {
        let mut granted: Vec<String> = Vec::new();
        let mut denied: Vec<String> = Vec::new();
        let req_sorted: BTreeSet<&String> = requested.iter().collect();
        for r in req_sorted {
            if self.granted.contains(r) {
                granted.push(r.clone());
            } else {
                denied.push(r.clone());
            }
        }
        (granted, denied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_contains_nothing() {
        let s = CapabilitySet::empty();
        assert!(!s.contains("anything"));
        assert!(s.granted().is_empty());
    }

    #[test]
    fn from_iter_builds_set() {
        let s = CapabilitySet::from_iter(["read".to_string(), "exec".to_string()]);
        assert!(s.contains("read"));
        assert!(s.contains("exec"));
        assert!(!s.contains("admin"));
    }

    #[test]
    fn granted_returns_sorted_deterministic_order() {
        // Insert in reverse; output must be sorted.
        let s = CapabilitySet::from_iter(["zeta".to_string(), "alpha".to_string(), "mu".to_string()]);
        assert_eq!(s.granted(), vec!["alpha", "mu", "zeta"]);
    }

    #[test]
    fn intersect_with_empty_granted_denies_all() {
        let s = CapabilitySet::empty();
        let (granted, denied) = s.intersect(&["read".into(), "exec".into()]);
        assert!(granted.is_empty());
        assert_eq!(denied, vec!["exec", "read"]);
    }

    #[test]
    fn intersect_partial() {
        let s = CapabilitySet::from_iter(["read".to_string(), "exec".to_string()]);
        let (granted, denied) = s.intersect(&["read".into(), "admin".into(), "exec".into()]);
        assert_eq!(granted, vec!["exec", "read"]);
        assert_eq!(denied, vec!["admin"]);
    }

    #[test]
    fn intersect_full_grants_all_requested() {
        let s = CapabilitySet::from_iter(["a".to_string(), "b".to_string(), "c".to_string()]);
        let (granted, denied) = s.intersect(&["a".into(), "b".into()]);
        assert_eq!(granted, vec!["a", "b"]);
        assert!(denied.is_empty());
    }

    #[test]
    fn intersect_dedupes_via_sorted_input() {
        // BTreeSet in `intersect` deduplicates repeated requests.
        let s = CapabilitySet::from_iter(["read".to_string()]);
        let (granted, denied) = s.intersect(&["read".into(), "read".into(), "admin".into()]);
        assert_eq!(granted, vec!["read"]);
        assert_eq!(denied, vec!["admin"]);
    }
}
