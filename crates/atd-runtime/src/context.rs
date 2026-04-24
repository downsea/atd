//! Per-call context passed to every `Tool::call` invocation.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::capability::CapabilitySet;
use crate::tier::ToolTier;
use crate::tracker::ReadTracker;

pub struct CallContext {
    /// Working directory for relative-path tools (Read / Bash / Glob / ...).
    pub cwd: PathBuf,
    /// Advisory truncation budget. Tools should respect this and return
    /// truncation markers when producing larger output. In SP-12 this is
    /// derived from the tool's tier via `TierPolicy::max_output`.
    pub max_output_bytes: usize,
    /// Unique id for tracing/logging; not emitted on the wire.
    pub call_id: ulid::Ulid,
    /// Absolute deadline. Tools that wrap long operations in tokio::time::timeout
    /// should pass `remaining_time()` as the budget.
    pub deadline: Option<Instant>,
    /// Shared-per-connection read tracker. `None` in isolated unit tests;
    /// server always attaches one via `Arc::clone` in per-connection state.
    pub read_tracker: Option<Arc<ReadTracker>>,
    /// Connection-scoped capability allow-list. Populated from the `Hello`
    /// handshake; shared across all calls on a single connection. Tools that
    /// gate side effects on caller authority may read this, though dispatch
    /// already enforces `required_capabilities` before invocation.
    pub capabilities: Arc<CapabilitySet>,
    /// Tier the current call resolved to. Informational for tools; dispatch
    /// uses the tier to pick the deadline / max_output budget above.
    pub tier: ToolTier,
}

impl CallContext {
    pub fn remaining_time(&self) -> Option<Duration> {
        self.deadline.map(|d| d.saturating_duration_since(Instant::now()))
    }
}

#[cfg(any(test, feature = "testing"))]
impl CallContext {
    /// Construct a sensible default for unit tests. cwd = current dir,
    /// 1 MiB output budget, fresh call_id, no deadline, no tracker,
    /// empty capability set, Warm tier.
    pub fn for_test() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            call_id: ulid::Ulid::new(),
            deadline: None,
            read_tracker: None,
            capabilities: Arc::new(CapabilitySet::empty()),
            tier: ToolTier::Warm,
        }
    }

    /// Test-only: build a CallContext with a fresh tracker attached.
    /// Returns both so tests can also record/check on the same tracker.
    pub fn for_test_with_tracker() -> (Self, Arc<ReadTracker>) {
        let tracker = Arc::new(ReadTracker::new());
        let ctx = Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            call_id: ulid::Ulid::new(),
            deadline: None,
            read_tracker: Some(tracker.clone()),
            capabilities: Arc::new(CapabilitySet::empty()),
            tier: ToolTier::Warm,
        };
        (ctx, tracker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_test_has_sensible_defaults() {
        let ctx = CallContext::for_test();
        assert!(ctx.cwd.exists(), "cwd should be a real directory");
        assert_eq!(ctx.max_output_bytes, 1_048_576);
        assert!(ctx.deadline.is_none());
        assert!(ctx.read_tracker.is_none());
    }

    #[test]
    fn for_test_with_tracker_shares_arc() {
        let (ctx, tracker) = CallContext::for_test_with_tracker();
        assert!(ctx.read_tracker.is_some());
        let ctx_tracker = ctx.read_tracker.as_ref().unwrap();
        assert!(Arc::ptr_eq(ctx_tracker, &tracker));
    }

    #[test]
    fn remaining_time_is_none_when_no_deadline() {
        let ctx = CallContext::for_test();
        assert!(ctx.remaining_time().is_none());
    }

    #[test]
    fn remaining_time_counts_down_from_deadline() {
        let ctx = CallContext {
            cwd: PathBuf::from("."),
            max_output_bytes: 1024,
            call_id: ulid::Ulid::new(),
            deadline: Some(Instant::now() + Duration::from_secs(5)),
            read_tracker: None,
            capabilities: Arc::new(CapabilitySet::empty()),
            tier: ToolTier::Warm,
        };
        let r = ctx.remaining_time().unwrap();
        assert!(r <= Duration::from_secs(5));
        assert!(r > Duration::from_secs(4));
    }

    #[test]
    fn remaining_time_saturates_to_zero_after_deadline() {
        let ctx = CallContext {
            cwd: PathBuf::from("."),
            max_output_bytes: 1024,
            call_id: ulid::Ulid::new(),
            deadline: Some(Instant::now() - Duration::from_secs(10)),
            read_tracker: None,
            capabilities: Arc::new(CapabilitySet::empty()),
            tier: ToolTier::Warm,
        };
        assert_eq!(ctx.remaining_time().unwrap(), Duration::ZERO);
    }

    #[test]
    fn for_test_has_empty_capabilities_and_warm_tier() {
        let ctx = CallContext::for_test();
        assert!(ctx.capabilities.granted().is_empty());
        assert_eq!(ctx.tier, ToolTier::Warm);
    }
}
