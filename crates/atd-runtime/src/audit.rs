//! Structured per-call audit events + pluggable sinks.
//!
//! `AuditSink` is the observation hook called at dispatch return points.
//! It sits OUTSIDE `Middleware` (which is a result-rewriter, success-only)
//! because audit needs to observe every outcome including failures.
//!
//! `JsonLinesAuditSink` is the default sink shipped in v1: one JSON
//! object per line, thread-safe, writes to any `Write + Send`.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

/// Audit schema version. Consumers should branch on this if future
/// breaking changes land. v1 is the initial stable schema.
pub const SCHEMA_VERSION: u32 = 1;

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
}

/// Writes one JSON object per line to the wrapped writer. Thread-safe
/// via a mutex around the writer. Write errors are silently dropped
/// (log loss >> dispatch stall).
pub struct JsonLinesAuditSink {
    writer: Mutex<Box<dyn Write + Send>>,
}

impl JsonLinesAuditSink {
    pub fn new(writer: Box<dyn Write + Send>) -> Self {
        Self {
            writer: Mutex::new(writer),
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
}

impl AuditSink for JsonLinesAuditSink {
    fn on_call(&self, event: &CallEvent) {
        let Ok(mut line) = serde_json::to_vec(event) else {
            return;
        };
        line.push(b'\n');
        let Ok(mut w) = self.writer.lock() else {
            return;
        };
        let _ = w.write_all(&line);
        let _ = w.flush();
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
    use std::sync::Arc;

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
        }
    }

    #[test]
    fn success_event_serializes() {
        let e = mk_event(Outcome::Success);
        let j: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&e).expect("serialize")).expect("parse");
        assert_eq!(j["tool_id"], "ref:echo.say");
        assert_eq!(j["outcome"]["kind"], "success");
        assert_eq!(j["schema_version"], 1);
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

    #[test]
    fn json_lines_sink_writes_one_line_per_event() {
        let buf: Vec<u8> = Vec::new();
        let buf_arc = Arc::new(Mutex::new(buf));
        let cloned = buf_arc.clone();

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

        let sink = JsonLinesAuditSink::new(Box::new(SharedBuf(buf_arc)));
        sink.on_call(&mk_event(Outcome::Success));
        sink.on_call(&mk_event(Outcome::ToolNotFound));

        let out = cloned.lock().unwrap().clone();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.split_terminator('\n').collect();
        assert_eq!(lines.len(), 2, "expected 2 lines, got: {:?}", lines);
        for line in &lines {
            let _: CallEvent = serde_json::from_str(line).expect("each line parses as CallEvent");
        }
    }

    #[test]
    fn now_rfc3339_format_is_parseable() {
        let s = now_rfc3339();
        chrono::DateTime::parse_from_rfc3339(&s).expect("RFC 3339 parseable");
    }
}
