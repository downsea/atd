//! Per-call context passed to every `Tool::call` invocation.

use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct CallContext {
    /// Working directory for relative-path tools (Read / Bash / Glob / ...).
    pub cwd: PathBuf,
    /// Advisory truncation budget. Tools should respect this and return
    /// truncation markers when producing larger output.
    pub max_output_bytes: usize,
    /// Unique id for tracing/logging; not emitted on the wire.
    pub call_id: ulid::Ulid,
    /// Absolute deadline. Tools that wrap long operations in tokio::time::timeout
    /// should pass `remaining_time()` as the budget.
    pub deadline: Option<Instant>,
}

impl CallContext {
    pub fn remaining_time(&self) -> Option<Duration> {
        self.deadline.map(|d| d.saturating_duration_since(Instant::now()))
    }
}

#[cfg(any(test, feature = "testing"))]
impl CallContext {
    /// Construct a sensible default for unit tests. cwd = current dir,
    /// 1 MiB output budget, fresh call_id, no deadline.
    pub fn for_test() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_output_bytes: 1_048_576,
            call_id: ulid::Ulid::new(),
            deadline: None,
        }
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
        };
        assert_eq!(ctx.remaining_time().unwrap(), Duration::ZERO);
    }
}
