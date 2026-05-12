//! Structured per-call audit events + pluggable sinks.
//!
//! `AuditSink` is the observation hook called at dispatch return points.
//! It sits OUTSIDE `Middleware` (which is a result-rewriter, success-only)
//! because audit needs to observe every outcome including failures.
//!
//! `JsonLinesAuditSink` is the default sink shipped in v1: one JSON
//! object per line. SP-concurrency-baseline §5.4: an internal bounded
//! `tokio::sync::mpsc` + dedicated drain task decouple the dispatch hot
//! path from synchronous file I/O, eliminating the §1.3 secondary cliff
//! (mutex-blocked reactor stall at ~50 concurrent dispatches per second).
//! Construction requires a tokio runtime context.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Audit schema version. Consumers should branch on this if future
/// breaking changes land.
///
/// - v1 (SP-operability-v1) — initial stable schema.
/// - v2 (SP-pagination-v1) — adds optional `cursor_page` field. The field
///   is `#[serde(default, skip_serializing_if = "Option::is_none")]` so
///   v1 consumers tolerate v2 events; v2 consumers reading v1 events see
///   `cursor_page: None`. The version bump records when the field landed,
///   not a breaking shape change.
pub const SCHEMA_VERSION: u32 = 2;

/// One per-call audit event. Emitted at every `Request::RunTool`
/// return point (success, invalid_args, execution_failed, cap_denied,
/// rate_limited, tool_not_found). Ping / Hello / ToolList / ToolSchema
/// do NOT emit events in v1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEvent {
    pub ts: String,
    pub call_id: String,
    pub tool_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_id: Option<String>,
    pub granted_capabilities: Vec<String>,
    pub duration_ms: u64,
    pub outcome: Outcome,
    pub tier: String,
    pub dry_run: bool,
    pub schema_version: u32,
    /// `true` iff a `TokenBroker` was configured AND it returned
    /// `Ok(Some(_))` for this caller (SP-token-broker-phase1). Always
    /// `false` for early-return paths (capability denied, dry-run,
    /// rate-limited, tool-not-found) and for servers without a broker.
    /// No key names or values are recorded.
    #[serde(default)]
    pub secrets_resolved: bool,
    /// SP-pagination-v1 — 1-based page index for paginated calls. `None`
    /// for non-paginated dispatches (the vast majority of events; saves
    /// bytes in the audit log). `Some(1)` for the initial `RunTool` that
    /// returned a cursor; `Some(2..)` for each `RunToolContinue`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_page: Option<u32>,
}

/// Outcome variants cover the full dispatch-return space for RunTool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Success,
    ExecutionFailed { code: String, retryable: bool },
    InvalidArgs { message: String },
    CapabilityDenied { missing: Vec<String> },
    RateLimited { retry_after_ms: Option<u64> },
    ToolNotFound,
}

/// Observer hook. Non-blocking: writes happen synchronously to the
/// sink's own backpressure (no queuing here). Must not panic.
pub trait AuditSink: Send + Sync {
    fn on_call(&self, event: &CallEvent);
    /// Total events dropped because the sink's queue was full. Default `0`
    /// for sinks that don't queue (custom synchronous adopter impls).
    /// `JsonLinesAuditSink` overrides with its `Arc<AtomicU64>` counter so
    /// `Server::metrics_snapshot()` (SP-concurrency-baseline §5.7) can
    /// surface the count without coupling the metrics module to the
    /// concrete sink type.
    fn drops(&self) -> u64 {
        0
    }
}

/// Default channel capacity. 1024 events × ~500 bytes ≈ 512 KB peak buffer;
/// drains at the rate the wrapped writer can absorb (typical disk write
/// rate: 10k events/s sustained, transient bursts much higher).
pub const DEFAULT_AUDIT_QUEUE_CAPACITY: usize = 1024;

/// SP-concurrency-baseline §5.4. Writes one JSON object per line to the
/// wrapped writer via a dedicated tokio task. `on_call` is non-blocking
/// (`try_send`); if the bounded channel is full the event is dropped and
/// the `audit_drops` counter increments — log loss >> dispatch stall.
///
/// **Construction requires a tokio runtime context** because it spawns a
/// drain task on `new`. All shipped helpers (`stdout` / `stderr` / `file`)
/// inherit this requirement. Adopters who construct sinks outside async
/// scope should use `tokio::runtime::Handle::current().block_on(...)` or
/// build their own sync sink implementing `AuditSink`.
pub struct JsonLinesAuditSink {
    tx: tokio::sync::mpsc::Sender<CallEvent>,
    drops: Arc<AtomicU64>,
}

impl JsonLinesAuditSink {
    /// Construct with default capacity (`DEFAULT_AUDIT_QUEUE_CAPACITY`).
    /// Spawns a tokio task that owns the writer and drains the channel.
    pub fn new(writer: Box<dyn Write + Send + 'static>) -> Self {
        Self::new_with_capacity(writer, DEFAULT_AUDIT_QUEUE_CAPACITY)
    }

    /// Construct with an explicit channel capacity. Smaller capacities
    /// drop sooner under burst load; larger capacities hold more bytes
    /// in memory at peak.
    pub fn new_with_capacity(writer: Box<dyn Write + Send + 'static>, capacity: usize) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<CallEvent>(capacity);
        let drops = Arc::new(AtomicU64::new(0));
        let mut writer = writer;
        tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                if let Ok(mut line) = serde_json::to_vec(&ev) {
                    line.push(b'\n');
                    let _ = writer.write_all(&line);
                    let _ = writer.flush();
                }
            }
            // Channel closed (sink dropped) — final flush so the last batch
            // hits disk even if the runtime is about to shut down.
            let _ = writer.flush();
        });
        Self { tx, drops }
    }

    pub fn stdout() -> Self {
        Self::new(Box::new(std::io::stdout()))
    }

    pub fn stderr() -> Self {
        Self::new(Box::new(std::io::stderr()))
    }

    /// Open `path` for append; creates the file if missing.
    pub fn file(path: &Path) -> std::io::Result<Self> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self::new(Box::new(f)))
    }

    /// Count of events dropped because the channel was full when
    /// `on_call` was invoked. Exposed for the SP-concurrency-baseline
    /// §G7 metrics snapshot and for adopter dashboards.
    pub fn drops(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
}

impl AuditSink for JsonLinesAuditSink {
    fn on_call(&self, event: &CallEvent) {
        match self.tx.try_send(event.clone()) {
            Ok(()) => {}
            // Channel full or closed — drop the event and bump the counter.
            // Sync dispatch path must not block; log loss >> dispatch stall.
            Err(_) => {
                self.drops.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn drops(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
}

/// Produce an RFC 3339 UTC timestamp string suitable for `CallEvent::ts`.
/// Dispatch sites use this rather than calling chrono directly so the
/// format stays consistent.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn mk_event(outcome: Outcome) -> CallEvent {
        CallEvent {
            ts: now_rfc3339(),
            call_id: "01J000000000000000000000TEST".into(),
            tool_id: "ref:echo.say".into(),
            caller_id: Some("test-client".into()),
            granted_capabilities: vec!["read".into(), "write".into()],
            duration_ms: 17,
            outcome,
            tier: "warm".into(),
            dry_run: false,
            schema_version: SCHEMA_VERSION,
            secrets_resolved: false,
            cursor_page: None,
        }
    }

    #[test]
    fn success_event_serializes() {
        let e = mk_event(Outcome::Success);
        let j: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&e).expect("serialize")).expect("parse");
        assert_eq!(j["tool_id"], "ref:echo.say");
        assert_eq!(j["outcome"]["kind"], "success");
        assert_eq!(j["schema_version"], 2);
        assert_eq!(j["dry_run"], false);
    }

    #[test]
    fn capability_denied_outcome_tagged_correctly() {
        let e = mk_event(Outcome::CapabilityDenied {
            missing: vec!["conformance.denied".into()],
        });
        let j: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&e).unwrap()).unwrap();
        assert_eq!(j["outcome"]["kind"], "capability_denied");
        assert_eq!(j["outcome"]["missing"][0], "conformance.denied");
    }

    #[test]
    fn execution_failed_carries_code_and_retryable() {
        let e = mk_event(Outcome::ExecutionFailed {
            code: "FS_NOT_FOUND".into(),
            retryable: false,
        });
        let j: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&e).unwrap()).unwrap();
        assert_eq!(j["outcome"]["kind"], "execution_failed");
        assert_eq!(j["outcome"]["code"], "FS_NOT_FOUND");
        assert_eq!(j["outcome"]["retryable"], false);
    }

    #[test]
    fn rate_limited_outcome_with_null_retry_after() {
        let e = mk_event(Outcome::RateLimited {
            retry_after_ms: None,
        });
        let j: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&e).unwrap()).unwrap();
        assert_eq!(j["outcome"]["kind"], "rate_limited");
        assert!(j["outcome"]["retry_after_ms"].is_null());
    }

    #[test]
    fn caller_id_skipped_when_none() {
        let mut e = mk_event(Outcome::Success);
        e.caller_id = None;
        let s = serde_json::to_string(&e).unwrap();
        assert!(
            !s.contains("caller_id"),
            "caller_id None should be skipped, got: {}",
            s
        );
    }

    /// Shared in-memory buffer wrapped behind a `Write` impl. Used as the
    /// sink's target so tests can inspect what got written without touching
    /// the filesystem. Cloning the `Arc<Mutex<...>>` outside the box lets
    /// the test read while the drain task writes.
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);
    impl Write for SharedBuf {
        fn write(&mut self, bs: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bs);
            Ok(bs.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Spin until the buffer accumulates `target_lines` newline-terminated
    /// records or `timeout` elapses. Returns the buffer's accumulated bytes.
    async fn wait_for_lines(
        buf: &Arc<Mutex<Vec<u8>>>,
        target_lines: usize,
        timeout: std::time::Duration,
    ) -> Vec<u8> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            {
                let guard = buf.lock().unwrap();
                let count = guard.iter().filter(|b| **b == b'\n').count();
                if count >= target_lines || std::time::Instant::now() > deadline {
                    return guard.clone();
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    #[tokio::test]
    async fn json_lines_sink_writes_one_line_per_event() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = JsonLinesAuditSink::new(Box::new(SharedBuf(buf.clone())));
        sink.on_call(&mk_event(Outcome::Success));
        sink.on_call(&mk_event(Outcome::ToolNotFound));

        let out = wait_for_lines(&buf, 2, std::time::Duration::from_millis(500)).await;
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines.len(), 2, "expected 2 lines, got: {lines:?}");
        for line in &lines {
            let _: CallEvent = serde_json::from_str(line).expect("each line parses as CallEvent");
        }
    }

    // ---- SP-concurrency-baseline §5.4 mpsc rewrite tests ----

    #[tokio::test]
    async fn on_call_is_non_blocking_under_burst() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = JsonLinesAuditSink::new(Box::new(SharedBuf(buf)));
        let ev = mk_event(Outcome::Success);
        // 100 synchronous on_call invocations on the dispatch hot path must
        // complete in well under 10ms total — sub-millisecond per call once
        // the channel is warm.
        let started = std::time::Instant::now();
        for _ in 0..100 {
            sink.on_call(&ev);
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "100 on_call invocations took {elapsed:?}; expected <50ms"
        );
    }

    #[tokio::test]
    async fn drops_counter_increments_when_channel_full() {
        // Capacity 4 — the drain task can't keep up with a burst of 200
        // events, so the channel saturates and try_send fails.
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = JsonLinesAuditSink::new_with_capacity(Box::new(SharedBuf(buf)), 4);
        let ev = mk_event(Outcome::Success);
        for _ in 0..200 {
            sink.on_call(&ev);
        }
        // Some of those 200 must have been dropped (the drain task is a
        // single task; under tight capacity it can't service 200 sends back-
        // to-back without scheduler yields between them).
        let dropped = sink.drops();
        assert!(
            dropped > 0,
            "expected some drops at capacity=4 with 200-event burst, got 0"
        );
    }

    #[tokio::test]
    async fn events_eventually_drain_to_writer() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = JsonLinesAuditSink::new(Box::new(SharedBuf(buf.clone())));
        let ev = mk_event(Outcome::Success);
        for _ in 0..10 {
            sink.on_call(&ev);
        }
        let out = wait_for_lines(&buf, 10, std::time::Duration::from_millis(500)).await;
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines.len(), 10, "expected 10 lines, got {}", lines.len());
    }

    #[tokio::test]
    async fn dropping_sink_drains_pending_then_exits() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        {
            let sink = JsonLinesAuditSink::new(Box::new(SharedBuf(buf.clone())));
            for _ in 0..5 {
                sink.on_call(&mk_event(Outcome::Success));
            }
            // Drop sink at end of block → tx closes → drain task finishes.
        }
        // Give the drain task time to consume the remaining queue and exit.
        let out = wait_for_lines(&buf, 5, std::time::Duration::from_millis(500)).await;
        let lines: Vec<&str> = std::str::from_utf8(&out)
            .unwrap()
            .split_terminator('\n')
            .collect();
        assert_eq!(lines.len(), 5, "drop should flush the last 5 events");
    }

    #[test]
    fn now_rfc3339_format_is_parseable() {
        let s = now_rfc3339();
        chrono::DateTime::parse_from_rfc3339(&s).expect("RFC 3339 parseable");
    }
}
