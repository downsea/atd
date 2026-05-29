//! Structured per-call audit events + pluggable sinks.
//!
//! `AuditSink` is the observation hook called at dispatch return points.
//! It sits OUTSIDE `Middleware` (a wire-reply rewriter) because audit
//! observes *metadata* about every outcome — including failures — and never
//! carries the result/error body (no PHI exit). `Middleware` rewrites the
//! body the LLM sees (`on_result` for success / `ExecutionFailed`,
//! `on_error` for `Response::Error`); `AuditSink` records who/what/when.
//!
//! `JsonLinesAuditSink` writes one JSON object per line via a dedicated
//! **std thread** drain over a bounded `std::sync::mpsc::sync_channel`.
//! SP-concurrency-baseline §5.4 introduced the queue to decouple the
//! dispatch hot path from synchronous file I/O; SP-observability-
//! completeness-v1 Axis B made the queue-full policy selectable
//! ([`BackpressureStrategy`]: `Drop` default / `Block` / `FallbackSink`)
//! and moved the drain to a std thread, so construction no longer requires
//! a tokio runtime context.

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
/// - v3 (SP-observability-completeness-v1) — adds optional
///   `capability_provenance`. Same additive-optional rule: v2 readers
///   tolerate v3 events (ignore the field); v3 readers see `None` on v2
///   events. Records per-capability source so an operator can answer
///   "why did caller X have capability Y?" without re-deriving the chain.
pub const SCHEMA_VERSION: u32 = 3;

/// One per-call audit event. Emitted at every `Request::RunTool`
/// return point (success, invalid_args, execution_failed, cap_denied,
/// rate_limited, tool_not_found). Ping / Hello / ToolList / ToolSchema
/// do NOT emit events in v1.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
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
    /// SP-observability-completeness-v1 Axis C — per-capability source
    /// attribution. `None` when provenance wasn't tracked (back-compat,
    /// early-return paths with no capability context); `Some(vec)` when
    /// dispatch recorded which mechanism granted each capability. Lets an
    /// operator trace each granted capability to the operator string
    /// allow-list or a specific UCAN chain link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_provenance: Option<Vec<CapProvenance>>,
}

impl CallEvent {
    /// Stable constructor. `CallEvent` is `#[non_exhaustive]`: external crates
    /// — adopters that emit their own audit events through an `AuditSink`
    /// (e.g. celia's federation orchestrator) — MUST build it via `new` + the
    /// `with_*` setters rather than a struct literal, so that adding an audit
    /// field in a future minor is **not** a breaking change for them (the
    /// `capability_provenance`/`cursor_page` additions that broke struct-literal
    /// constructors are the motivation). Required fields are constructor args;
    /// everything else defaults — `caller_id=None`, `granted_capabilities=[]`,
    /// `dry_run=false`, `secrets_resolved=false`, `cursor_page=None`,
    /// `capability_provenance=None`, `schema_version=SCHEMA_VERSION`.
    pub fn new(
        ts: impl Into<String>,
        call_id: impl Into<String>,
        tool_id: impl Into<String>,
        duration_ms: u64,
        outcome: Outcome,
        tier: impl Into<String>,
    ) -> Self {
        Self {
            ts: ts.into(),
            call_id: call_id.into(),
            tool_id: tool_id.into(),
            caller_id: None,
            granted_capabilities: Vec::new(),
            duration_ms,
            outcome,
            tier: tier.into(),
            dry_run: false,
            schema_version: SCHEMA_VERSION,
            secrets_resolved: false,
            cursor_page: None,
            capability_provenance: None,
        }
    }

    pub fn with_caller_id(mut self, caller_id: Option<String>) -> Self {
        self.caller_id = caller_id;
        self
    }
    pub fn with_granted_capabilities(mut self, caps: Vec<String>) -> Self {
        self.granted_capabilities = caps;
        self
    }
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
    pub fn with_secrets_resolved(mut self, resolved: bool) -> Self {
        self.secrets_resolved = resolved;
        self
    }
    pub fn with_cursor_page(mut self, page: Option<u32>) -> Self {
        self.cursor_page = page;
        self
    }
    pub fn with_capability_provenance(mut self, provenance: Option<Vec<CapProvenance>>) -> Self {
        self.capability_provenance = provenance;
        self
    }
}

/// SP-observability-completeness-v1 Axis C — one capability + how it was
/// granted. The `granted_capabilities` field on `CallEvent` records the
/// *result* set; this records the *source* of each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapProvenance {
    pub cap: String,
    pub source: ProvSource,
}

/// Where a granted capability came from (architecture §5.2 — the two
/// composing mechanisms whose union forms `granted_capabilities`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProvSource {
    /// Granted by the operator string allow-list
    /// (`--grant-capability` ∩ `Hello.requested_capabilities`).
    StringAllowList,
    /// Granted by a UCAN-lite chain link. `issuer_did` is the link's
    /// `iss`; `chain_depth` is its position (0 = root).
    UcanChain { issuer_did: String, chain_depth: u8 },
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

/// SP-observability-completeness-v1 Axis B. How a sink behaves when its
/// internal queue is full at `on_call` time.
#[derive(Clone)]
pub enum BackpressureStrategy {
    /// Drop the event, increment `drops()`. The SP-concurrency-baseline
    /// default — protects dispatch throughput; correct for the 90%
    /// non-compliance case. "log loss >> dispatch stall."
    Drop,
    /// Block the dispatch path until the queue accepts the event. For
    /// compliance adopters (HIPAA §164.528) where a dropped audit record is
    /// unacceptable: dispatch slows under audit backpressure rather than
    /// losing the disclosure record. Requires a multi-thread runtime (the
    /// ref binaries use one) so a blocked worker doesn't starve accept.
    Block,
    /// On queue-full, write the event synchronously to a fallback sink
    /// (e.g. stderr / a second file) instead of dropping. Bounds the hot
    /// path (no indefinite block) with no silent loss. The fallback SHOULD
    /// be a synchronous sink, never another queueing sink (avoid chained
    /// blocking).
    FallbackSink(Arc<dyn AuditSink>),
}

impl std::fmt::Debug for BackpressureStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackpressureStrategy::Drop => f.write_str("Drop"),
            BackpressureStrategy::Block => f.write_str("Block"),
            BackpressureStrategy::FallbackSink(_) => f.write_str("FallbackSink(..)"),
        }
    }
}

/// Observer hook. `on_call` is invoked synchronously on the dispatch path;
/// its behaviour under queue pressure is the sink's
/// [`backpressure_strategy`](AuditSink::backpressure_strategy). Must not panic.
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
    /// SP-observability-completeness-v1 Axis B — the sink's queue-full
    /// policy. Default `Drop` (byte-compatible with pre-SP sinks); medical /
    /// compliance adopters override (or construct `JsonLinesAuditSink` via
    /// `with_strategy`) to get `Block` / `FallbackSink`.
    fn backpressure_strategy(&self) -> BackpressureStrategy {
        BackpressureStrategy::Drop
    }
}

/// Default channel capacity. 1024 events × ~500 bytes ≈ 512 KB peak buffer;
/// drains at the rate the wrapped writer can absorb (typical disk write
/// rate: 10k events/s sustained, transient bursts much higher).
pub const DEFAULT_AUDIT_QUEUE_CAPACITY: usize = 1024;

/// SP-concurrency-baseline §5.4 + SP-observability-completeness-v1 Axis B.
/// Writes one JSON object per line to the wrapped writer via a dedicated
/// **std thread** drain. Behaviour when the bounded channel is full is
/// selectable via [`BackpressureStrategy`]:
///
/// - `Drop` (default) — `try_send`; on full, drop + bump `drops()`. The
///   SP-concurrency-baseline behaviour (log loss >> dispatch stall).
/// - `Block` — blocking `send`; dispatch slows rather than losing an event
///   (HIPAA §164.528 no-loss audit). The drain is a dedicated OS thread, so
///   blocking the sync `on_call` blocks the calling thread, not an async
///   reactor primitive — use a multi-thread runtime (ref binaries do).
/// - `FallbackSink(fb)` — on full, synchronously write to `fb`.
///
/// Construction no longer requires a tokio runtime context (the drain is a
/// plain `std::thread`), unlike the prior tokio-mpsc implementation.
pub struct JsonLinesAuditSink {
    /// `Option` so `Drop` can take + drop the sender (closing the channel)
    /// before joining the drain thread.
    tx: Option<std::sync::mpsc::SyncSender<CallEvent>>,
    /// Retained so `Drop` can join the drain thread and guarantee the
    /// buffered tail flushes before the sink goes away (#4 no-loss-at-drop).
    drain: Option<std::thread::JoinHandle<()>>,
    drops: Arc<AtomicU64>,
    strategy: BackpressureStrategy,
}

impl JsonLinesAuditSink {
    /// Construct with default capacity (`DEFAULT_AUDIT_QUEUE_CAPACITY`) and
    /// the `Drop` strategy (pre-SP behaviour).
    pub fn new(writer: Box<dyn Write + Send + 'static>) -> Self {
        Self::new_with_capacity(writer, DEFAULT_AUDIT_QUEUE_CAPACITY)
    }

    /// Construct with an explicit channel capacity, `Drop` strategy.
    pub fn new_with_capacity(writer: Box<dyn Write + Send + 'static>, capacity: usize) -> Self {
        Self::with_strategy(writer, capacity, BackpressureStrategy::Drop)
    }

    /// SP-observability-completeness-v1 Axis B — construct with an explicit
    /// backpressure strategy. Spawns a std thread that owns the writer and
    /// drains the channel.
    pub fn with_strategy(
        writer: Box<dyn Write + Send + 'static>,
        capacity: usize,
        strategy: BackpressureStrategy,
    ) -> Self {
        // SP-observability-completeness-v1 #3 — Block blocks the calling
        // (sync) thread under backpressure. On a current_thread tokio runtime
        // that thread is the sole worker, so a stall starves accept. Warn
        // (best-effort) when we can detect that flavor at construction time.
        if matches!(strategy, BackpressureStrategy::Block) {
            if let Ok(h) = tokio::runtime::Handle::try_current() {
                if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::CurrentThread {
                    eprintln!(
                        "atd: WARNING — JsonLinesAuditSink Block strategy on a \
                         current_thread runtime; a blocked worker can stall accept \
                         under audit backpressure. Prefer a multi-thread runtime."
                    );
                }
            }
        }
        let (tx, rx) = std::sync::mpsc::sync_channel::<CallEvent>(capacity);
        let drops = Arc::new(AtomicU64::new(0));
        let mut writer = writer;
        let drain = std::thread::spawn(move || {
            while let Ok(ev) = rx.recv() {
                if let Ok(mut line) = serde_json::to_vec(&ev) {
                    line.push(b'\n');
                    let _ = writer.write_all(&line);
                    let _ = writer.flush();
                }
            }
            // All senders dropped + queue drained — final flush.
            let _ = writer.flush();
        });
        Self {
            tx: Some(tx),
            drain: Some(drain),
            drops,
            strategy,
        }
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
    /// `on_call` was invoked (or the drain thread had exited).
    pub fn drops(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
}

impl AuditSink for JsonLinesAuditSink {
    fn on_call(&self, event: &CallEvent) {
        let Some(tx) = self.tx.as_ref() else {
            // Sink is being torn down (tx taken in Drop); count as a drop.
            self.drops.fetch_add(1, Ordering::Relaxed);
            return;
        };
        match &self.strategy {
            BackpressureStrategy::Drop => {
                // Non-blocking; on full, drop + count. log loss >> stall.
                if tx.try_send(event.clone()).is_err() {
                    self.drops.fetch_add(1, Ordering::Relaxed);
                }
            }
            BackpressureStrategy::Block => {
                // Blocking send — no event lost; dispatch slows under
                // backpressure. Err only if the drain thread is gone
                // (shutdown), in which case count a drop rather than hang.
                if tx.send(event.clone()).is_err() {
                    self.drops.fetch_add(1, Ordering::Relaxed);
                }
            }
            BackpressureStrategy::FallbackSink(fb) => {
                if tx.try_send(event.clone()).is_err() {
                    fb.on_call(event);
                }
            }
        }
    }
    fn drops(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
    fn backpressure_strategy(&self) -> BackpressureStrategy {
        self.strategy.clone()
    }
}

impl Drop for JsonLinesAuditSink {
    /// SP-observability-completeness-v1 #4 — close the channel, then join the
    /// drain thread so the buffered tail flushes to the writer before the
    /// sink goes away. Without this, a short-lived process (or a Block
    /// "no-loss" adopter) could lose queued events at teardown. Joining can
    /// block if the writer is wedged — that is the price of the no-loss
    /// guarantee at shutdown.
    fn drop(&mut self) {
        self.tx.take(); // drop sender → drain loop ends after draining
        if let Some(h) = self.drain.take() {
            let _ = h.join();
        }
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
        CallEvent::new(
            now_rfc3339(),
            "01J000000000000000000000TEST",
            "ref:echo.say",
            17,
            outcome,
            "warm",
        )
        .with_caller_id(Some("test-client".into()))
        .with_granted_capabilities(vec!["read".into(), "write".into()])
    }

    #[test]
    fn callevent_builder_defaults_then_setters() {
        let e = CallEvent::new(now_rfc3339(), "cid", "tool:x", 5, Outcome::Success, "warm");
        assert_eq!(e.tool_id, "tool:x");
        assert_eq!(e.duration_ms, 5);
        assert_eq!(e.schema_version, SCHEMA_VERSION);
        assert!(e.caller_id.is_none());
        assert!(e.granted_capabilities.is_empty());
        assert!(!e.dry_run);
        assert!(!e.secrets_resolved);
        assert!(e.cursor_page.is_none());
        assert!(e.capability_provenance.is_none());
        let e2 = e
            .with_caller_id(Some("agent-A".into()))
            .with_cursor_page(Some(2))
            .with_secrets_resolved(true);
        assert_eq!(e2.caller_id.as_deref(), Some("agent-A"));
        assert_eq!(e2.cursor_page, Some(2));
        assert!(e2.secrets_resolved);
    }

    #[test]
    fn success_event_serializes() {
        let e = mk_event(Outcome::Success);
        let j: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&e).expect("serialize")).expect("parse");
        assert_eq!(j["tool_id"], "ref:echo.say");
        assert_eq!(j["outcome"]["kind"], "success");
        assert_eq!(j["schema_version"], 3);
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

    // ---- SP-observability-completeness-v1 Axis C: capability provenance ----

    #[test]
    fn capability_provenance_roundtrips_both_sources() {
        let mut e = mk_event(Outcome::Success);
        e.capability_provenance = Some(vec![
            CapProvenance {
                cap: "records:read".into(),
                source: ProvSource::StringAllowList,
            },
            CapProvenance {
                cap: "records:write".into(),
                source: ProvSource::UcanChain {
                    issuer_did: "did:key:zABC".into(),
                    chain_depth: 1,
                },
            },
        ]);
        let j: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&e).unwrap()).unwrap();
        let prov = j["capability_provenance"].as_array().unwrap();
        assert_eq!(prov[0]["cap"], "records:read");
        assert_eq!(prov[0]["source"]["kind"], "string_allow_list");
        assert_eq!(prov[1]["source"]["kind"], "ucan_chain");
        assert_eq!(prov[1]["source"]["issuer_did"], "did:key:zABC");
        assert_eq!(prov[1]["source"]["chain_depth"], 1);
    }

    #[test]
    fn provenance_skipped_when_none() {
        let e = mk_event(Outcome::Success);
        let s = serde_json::to_string(&e).unwrap();
        assert!(
            !s.contains("capability_provenance"),
            "None provenance must be omitted on the wire (back-compat), got: {s}"
        );
    }

    #[test]
    fn v2_event_without_provenance_deserializes_to_none() {
        // A v2 audit line (no capability_provenance field) must read into
        // a v3 consumer as None — adopters on old atd builds keep working.
        let j = r#"{"ts":"2026-05-29T00:00:00+00:00","call_id":"01J","tool_id":"x",
            "granted_capabilities":[],"duration_ms":1,"outcome":{"kind":"success"},
            "tier":"warm","dry_run":false,"schema_version":2,"secrets_resolved":false}"#;
        let e: CallEvent = serde_json::from_str(j).unwrap();
        assert!(e.capability_provenance.is_none());
        assert!(e.cursor_page.is_none());
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

    #[test]
    fn drops_counter_increments_when_channel_full() {
        // SlowBuf throttles the drain (2ms/write) so a 200-event Drop-strategy
        // burst on a capacity-4 channel is GUARANTEED to saturate it before the
        // drain catches up. Without the throttle the std-thread drain races the
        // producer and could keep up on a fast/idle box → flaky `drops == 0`.
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = JsonLinesAuditSink::new_with_capacity(
            Box::new(SlowBuf {
                inner: buf,
                delay: std::time::Duration::from_millis(2),
            }),
            4,
        );
        let ev = mk_event(Outcome::Success);
        for _ in 0..200 {
            sink.on_call(&ev);
        }
        assert!(
            sink.drops() > 0,
            "expected drops at capacity=4 with a 200-event burst against a slow drain, got 0"
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

    // ---- SP-observability-completeness-v1 Axis B: backpressure ----

    /// A `Write` that sleeps per write — simulates a slow audit disk so a
    /// burst outruns the drain and exercises the backpressure path.
    struct SlowBuf {
        inner: Arc<Mutex<Vec<u8>>>,
        delay: std::time::Duration,
    }
    impl Write for SlowBuf {
        fn write(&mut self, bs: &[u8]) -> std::io::Result<usize> {
            std::thread::sleep(self.delay);
            self.inner.lock().unwrap().extend_from_slice(bs);
            Ok(bs.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn bare_sink_defaults_to_drop_strategy() {
        struct Bare;
        impl AuditSink for Bare {
            fn on_call(&self, _: &CallEvent) {}
        }
        assert!(matches!(
            Bare.backpressure_strategy(),
            BackpressureStrategy::Drop
        ));
    }

    #[test]
    fn with_strategy_block_reports_block() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = JsonLinesAuditSink::with_strategy(
            Box::new(SharedBuf(buf)),
            16,
            BackpressureStrategy::Block,
        );
        assert!(matches!(
            sink.backpressure_strategy(),
            BackpressureStrategy::Block
        ));
    }

    #[test]
    fn block_strategy_loses_nothing_under_burst() {
        // Throttled writer + tiny capacity + Block: every event must land,
        // zero drops. Block makes on_call wait for queue space; dropping the
        // sink JOINS the drain thread (#4), flushing the tail — so the
        // assertion observes the final state directly instead of racing a
        // deadline/poll loop (the previous flaky shape).
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = JsonLinesAuditSink::with_strategy(
            Box::new(SlowBuf {
                inner: buf.clone(),
                delay: std::time::Duration::from_micros(50),
            }),
            4,
            BackpressureStrategy::Block,
        );
        let ev = mk_event(Outcome::Success);
        for _ in 0..100 {
            sink.on_call(&ev);
        }
        assert_eq!(sink.drops(), 0, "Block strategy must never drop");
        drop(sink); // joins the drain thread → all buffered events flushed
        let n = buf.lock().unwrap().iter().filter(|b| **b == b'\n').count();
        assert_eq!(
            n, 100,
            "Block must flush all 100 events by the time drop returns"
        );
    }

    #[test]
    fn fallback_strategy_routes_overflow_to_fallback() {
        struct CountSink(Arc<AtomicU64>);
        impl AuditSink for CountSink {
            fn on_call(&self, _: &CallEvent) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let fb_count = Arc::new(AtomicU64::new(0));
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let sink = JsonLinesAuditSink::with_strategy(
            Box::new(SlowBuf {
                inner: buf,
                delay: std::time::Duration::from_millis(5),
            }),
            1,
            BackpressureStrategy::FallbackSink(Arc::new(CountSink(fb_count.clone()))),
        );
        let ev = mk_event(Outcome::Success);
        for _ in 0..50 {
            sink.on_call(&ev);
        }
        assert_eq!(sink.drops(), 0, "fallback caught overflow; primary drops 0");
        assert!(
            fb_count.load(Ordering::Relaxed) > 0,
            "fallback sink must catch the overflow events"
        );
    }

    #[test]
    fn now_rfc3339_format_is_parseable() {
        let s = now_rfc3339();
        chrono::DateTime::parse_from_rfc3339(&s).expect("RFC 3339 parseable");
    }
}
