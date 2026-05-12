//! Lock-free atomic metrics counters surfaced via `Server::metrics_snapshot()`.
//!
//! SP-concurrency-baseline §5.7 — gives adopters per-server observability
//! without forcing a Prometheus-grade pipeline. Counters are updated on the
//! hot path (accept, dispatch, audit) using `Ordering::Relaxed` since per-
//! counter atomicity is enough — we never read multiple counters as one
//! consistent snapshot.
//!
//! Future SP-observability-v2 will layer latency histograms (p50 / p99) and
//! a `/metrics` Prometheus endpoint on top of this counter base.

use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Hot-path counters owned by `ServerState`. All fields are atomic so
/// every transport (UDS via `atd-server`, HTTP via `atd-server-http`) and
/// every dispatch path can bump them without coordinating.
#[derive(Default, Debug)]
pub struct MetricsCounters {
    /// Total UDS / HTTP connections accepted since startup.
    pub accepted_connections: AtomicU64,
    /// Total `Request` frames dispatched (Ping, Hello, ToolList, ToolSchema,
    /// RunTool — every variant counts).
    pub dispatched_requests: AtomicU64,
    /// Per-error-code counter, lazily allocated. Keyed by `Response::Error.code`
    /// (the same u16s `atd_protocol::ERR_*` constants define). Sparse: most
    /// codes are never hit on a given server.
    pub dispatch_errors_by_code: Mutex<BTreeMap<u16, u64>>,
    /// Total audit events successfully enqueued onto the audit sink.
    pub audit_events_total: AtomicU64,
    /// Total audit events dropped because the audit sink's queue was full
    /// (SP-concurrency-baseline §5.4 mpsc backpressure).
    pub audit_drops_total: AtomicU64,
}

impl MetricsCounters {
    /// Bump a dispatch-error code counter. Called from dispatch error
    /// branches. Lock contention is acceptable here: errors are by
    /// definition tail events; the success hot path doesn't touch this map.
    pub fn record_error(&self, code: u16) {
        if let Ok(mut map) = self.dispatch_errors_by_code.lock() {
            *map.entry(code).or_insert(0) += 1;
        }
    }

    /// Capture a snapshot suitable for serializing into a `/metrics` body
    /// or a health-check endpoint.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let errors_by_code = self
            .dispatch_errors_by_code
            .lock()
            .map(|m| m.clone())
            .unwrap_or_default();
        MetricsSnapshot {
            accepted_connections: self.accepted_connections.load(Ordering::Relaxed),
            dispatched_requests: self.dispatched_requests.load(Ordering::Relaxed),
            dispatch_errors_by_code: errors_by_code,
            audit_events_total: self.audit_events_total.load(Ordering::Relaxed),
            audit_drops_total: self.audit_drops_total.load(Ordering::Relaxed),
        }
    }
}

/// JSON-serialisable snapshot of [`MetricsCounters`]. Returned by
/// `Server::metrics_snapshot()` and `atd-conformance` scenarios that need
/// to assert post-storm invariants (e.g. `audit_drops_total == 0`).
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub accepted_connections: u64,
    pub dispatched_requests: u64,
    pub dispatch_errors_by_code: BTreeMap<u16, u64>,
    pub audit_events_total: u64,
    pub audit_drops_total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_zero() {
        let m = MetricsCounters::default();
        let s = m.snapshot();
        assert_eq!(s.accepted_connections, 0);
        assert_eq!(s.dispatched_requests, 0);
        assert_eq!(s.audit_events_total, 0);
        assert_eq!(s.audit_drops_total, 0);
        assert!(s.dispatch_errors_by_code.is_empty());
    }

    #[test]
    fn atomic_increments_visible_in_snapshot() {
        let m = MetricsCounters::default();
        m.accepted_connections.fetch_add(3, Ordering::Relaxed);
        m.dispatched_requests.fetch_add(7, Ordering::Relaxed);
        m.audit_events_total.fetch_add(11, Ordering::Relaxed);
        m.audit_drops_total.fetch_add(1, Ordering::Relaxed);
        let s = m.snapshot();
        assert_eq!(s.accepted_connections, 3);
        assert_eq!(s.dispatched_requests, 7);
        assert_eq!(s.audit_events_total, 11);
        assert_eq!(s.audit_drops_total, 1);
    }

    #[test]
    fn record_error_accumulates_per_code() {
        let m = MetricsCounters::default();
        m.record_error(1001); // ERR_CAPABILITY_DENIED
        m.record_error(1001);
        m.record_error(1002); // ERR_RATE_LIMITED
        let s = m.snapshot();
        assert_eq!(s.dispatch_errors_by_code.get(&1001), Some(&2));
        assert_eq!(s.dispatch_errors_by_code.get(&1002), Some(&1));
        assert_eq!(s.dispatch_errors_by_code.get(&9999), None);
    }

    #[test]
    fn snapshot_is_serialisable_to_json() {
        let m = MetricsCounters::default();
        m.accepted_connections.fetch_add(5, Ordering::Relaxed);
        m.record_error(1010);
        let s = m.snapshot();
        let j = serde_json::to_value(&s).expect("serialize snapshot");
        assert_eq!(j["accepted_connections"], 5);
        assert_eq!(j["dispatch_errors_by_code"]["1010"], 1);
    }
}
