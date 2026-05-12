//! SP-pagination-v1 §G10 — `paginated_dispatch` conformance scenario.
//!
//! Registers a synthetic 100-row generator tool that emits 10 rows per page.
//! Asserts the full ATD pagination contract end-to-end:
//!
//! - Initial `Request::RunTool` returns page 1 of 10 rows
//! - Each `Request::RunToolContinue` walks one more page
//! - Terminal page (10/10) omits `next_cursor`
//! - `AtdClient::call_all` returns all 100 rows concatenated
//! - Expired cursor returns `ERR_CURSOR_EXPIRED` (1020)
//! - Cross-tool cursor returns `ERR_CURSOR_INVALID` (1021)
//! - Audit events tag `cursor_page` correctly on continuations

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::cursor::CursorPayload;
use atd_runtime::registry::{CallFuture, PaginatedCallFuture, PaginatedResult, Registry, Tool};
use atd_sdk::{
    AtdClient, CallAllOptions, CallOptions, ConnectOptions, DiscoverFilter, Endpoint, MergePolicy,
};
use atd_server::{Server, ServerConfig};

const TOTAL_ROWS: u32 = 100;
const ROWS_PER_PAGE: u32 = 10;

/// 100-row generator: emits ROWS_PER_PAGE rows per page until exhausted.
/// Page index is threaded through `opaque_state` as a u32-BE.
struct RowGenerator {
    def: ToolDefinition,
}

impl RowGenerator {
    fn new() -> Self {
        Self {
            def: ToolDefinition {
                id: "conformance:page_gen".into(),
                name: "page_gen".into(),
                description: "100-row generator emitting 10 rows per page".into(),
                version: "0.1.0".into(),
                capability: ToolCapability {
                    domain: "conformance".into(),
                    actions: vec!["list".into()],
                    tags: vec![],
                    intent_examples: vec![],
                },
                input_schema: serde_json::json!({"type": "object"}),
                output_schema: serde_json::json!({"type": "array"}),
                bindings: vec![ToolBinding {
                    protocol: BindingProtocol::Cli,
                    config: serde_json::json!({}),
                }],
                safety: ToolSafety {
                    level: SafetyLevel::Read,
                    dry_run: false,
                    side_effects: vec![],
                    data_sensitivity: None,
                },
                resources: ToolResources {
                    timeout_ms: 1000,
                    max_concurrent: 0, // unbounded
                    rate_limit_per_min: None,
                    estimated_tokens: None,
                },
                trust: ToolTrust {
                    publisher: "atd-conformance".into(),
                    trust_level: TrustLevel::L0Unverified,
                    signature: None,
                },
                visibility: ToolVisibility::Read,
                required_capabilities: vec![],
                tier: None,
                errors: vec![],
            },
        }
    }
}

impl Tool for RowGenerator {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn supports_pagination(&self) -> bool {
        true
    }
    fn call<'a>(
        &'a self,
        _args: serde_json::Value,
        _ctx: &'a atd_runtime::CallContext,
    ) -> CallFuture<'a> {
        Box::pin(async { Ok(serde_json::json!({"err": "non-paginated call() should not be hit"})) })
    }
    fn call_paginated<'a>(
        &'a self,
        _args: serde_json::Value,
        ctx: &'a atd_runtime::CallContext,
        cursor: Option<&'a str>,
    ) -> PaginatedCallFuture<'a> {
        let issuer = ctx
            .cursor_issuer()
            .expect("dispatch must attach issuer for supports_pagination=true tools");
        let page_index = match cursor {
            None => 1u32,
            Some(c) => {
                let p = issuer.verify(c, 300).expect("dispatch pre-verified");
                u32::from_be_bytes(p.opaque_state[..4].try_into().unwrap())
            }
        };
        let start_row = (page_index - 1) * ROWS_PER_PAGE;
        let end_row = (start_row + ROWS_PER_PAGE).min(TOTAL_ROWS);
        let rows: Vec<serde_json::Value> = (start_row..end_row)
            .map(|i| serde_json::json!({"row": i}))
            .collect();
        let next_cursor = if end_row < TOTAL_ROWS {
            let payload = CursorPayload {
                tool_id: "conformance:page_gen".into(),
                caller_id: ctx.caller_id.clone(),
                args_fingerprint: [0u8; 32],
                page_index: page_index + 1,
                issued_at_unix: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                server_session: issuer.session_nonce(),
                opaque_state: (page_index + 1).to_be_bytes().to_vec(),
            };
            Some(issuer.issue(payload).expect("issue"))
        } else {
            None
        };
        Box::pin(async move {
            Ok(PaginatedResult {
                value: serde_json::Value::Array(rows),
                next_cursor,
            })
        })
    }
}

async fn wait_for_sock(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !path.exists() {
        if Instant::now() > deadline {
            panic!("server did not bind socket within 3s");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn fast_connect() -> ConnectOptions {
    ConnectOptions {
        max_attempts: 3,
        backoff_base_ms: 5,
        backoff_cap_ms: 20,
        connect_timeout_ms: 2000,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paginated_dispatch_full_walk_yields_all_rows() {
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("page.sock");
    let mut reg = Registry::new();
    reg.register(Arc::new(RowGenerator::new()));
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        ..ServerConfig::default()
    };
    let server = Server::new(reg, cfg);
    let task = tokio::spawn(server.run());
    wait_for_sock(&sock).await;

    let client = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();

    // Manual page walk: initial RunTool + N RunToolContinue calls.
    let initial = client
        .call_page(
            "conformance:page_gen",
            serde_json::json!({}),
            None,
            CallOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        initial.value.as_array().unwrap().len(),
        ROWS_PER_PAGE as usize
    );
    assert!(
        initial.next_cursor.is_some(),
        "initial page must carry cursor"
    );

    let mut rows: Vec<serde_json::Value> = initial.value.as_array().unwrap().clone();
    let mut cursor = initial.next_cursor;
    let mut page_count = 1u32;
    while let Some(c) = cursor.clone() {
        let page = client
            .call_page(
                "conformance:page_gen",
                serde_json::Value::Null,
                Some(&c),
                CallOptions::default(),
            )
            .await
            .unwrap();
        rows.extend(page.value.as_array().unwrap().clone());
        cursor = page.next_cursor;
        page_count += 1;
        assert!(page_count <= 20, "guard against runaway pagination");
    }

    assert_eq!(
        rows.len(),
        TOTAL_ROWS as usize,
        "walk must yield all {TOTAL_ROWS} rows"
    );
    assert_eq!(
        page_count,
        TOTAL_ROWS / ROWS_PER_PAGE,
        "10 pages × 10 rows = 100"
    );
    // First / last row identity check.
    assert_eq!(rows[0]["row"], 0);
    assert_eq!(rows[(TOTAL_ROWS - 1) as usize]["row"], TOTAL_ROWS - 1);

    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn call_all_walks_all_pages_via_concat_array() {
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("call_all.sock");
    let mut reg = Registry::new();
    reg.register(Arc::new(RowGenerator::new()));
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        ..ServerConfig::default()
    };
    let server = Server::new(reg, cfg);
    let task = tokio::spawn(server.run());
    wait_for_sock(&sock).await;

    let client = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();

    let opts = CallAllOptions {
        max_pages: 20,
        max_total_bytes: 1024 * 1024,
        merge_policy: MergePolicy::ConcatArray,
    };
    let all = client
        .call_all("conformance:page_gen", serde_json::json!({}), opts)
        .await
        .unwrap();
    let arr = all.as_array().expect("ConcatArray returns Value::Array");
    assert_eq!(
        arr.len(),
        TOTAL_ROWS as usize,
        "call_all + ConcatArray must concat all pages"
    );
    assert_eq!(arr[0]["row"], 0);
    assert_eq!(arr[(TOTAL_ROWS - 1) as usize]["row"], TOTAL_ROWS - 1);

    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_cursor_returns_1020_after_short_ttl_window() {
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("expired.sock");
    let mut reg = Registry::new();
    reg.register(Arc::new(RowGenerator::new()));
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        ..ServerConfig::default()
    };
    let mut server = Server::new(reg, cfg);
    // 1s TTL via the Phase I mutator so the test exercises the real
    // server-side expiry check rather than relying on the dispatch
    // unit test alone.
    server.set_cursor_ttl_seconds(1);
    // Grab the issuer Arc BEFORE run() consumes self — we'll mint a
    // backdated cursor against it so the verify path actually fires the
    // TTL branch (not the session_nonce branch).
    let issuer = server.cursor_issuer();
    let task = tokio::spawn(server.run());
    wait_for_sock(&sock).await;

    // Mint a cursor whose issued_at_unix is 5 seconds in the past — older
    // than the 1s TTL we configured above, so verify returns Expired.
    let payload = CursorPayload {
        tool_id: "conformance:page_gen".into(),
        caller_id: None,
        args_fingerprint: [0u8; 32],
        page_index: 2,
        issued_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .saturating_sub(5),
        server_session: issuer.session_nonce(),
        opaque_state: 2u32.to_be_bytes().to_vec(),
    };
    let stale_cursor = issuer.issue(payload).expect("issue stale");

    let client = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();
    let result = client
        .call_page(
            "conformance:page_gen",
            serde_json::Value::Null,
            Some(&stale_cursor),
            CallOptions::default(),
        )
        .await;

    match result {
        Err(e) => {
            use std::error::Error as StdError;
            let mut chain = format!("{e}");
            let mut src: Option<&dyn StdError> = e.source();
            while let Some(s) = src {
                chain.push_str(" | ");
                chain.push_str(&format!("{s}"));
                src = s.source();
            }
            assert!(
                chain.contains("1020")
                    || chain.contains("cursor expired")
                    || chain.contains("re-issue"),
                "expected ERR_CURSOR_EXPIRED (1020) signal, got chain: {chain}"
            );
        }
        Ok(p) => panic!("expected expired-cursor rejection, got page: {p:?}"),
    }

    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_tool_cursor_returns_1021_via_wire() {
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("cross.sock");
    let mut reg = Registry::new();
    reg.register(Arc::new(RowGenerator::new()));
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        ..ServerConfig::default()
    };
    let server = Server::new(reg, cfg);
    let task = tokio::spawn(server.run());
    wait_for_sock(&sock).await;

    let client = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();

    // Get a real cursor for the page_gen tool.
    let initial = client
        .call_page(
            "conformance:page_gen",
            serde_json::json!({}),
            None,
            CallOptions::default(),
        )
        .await
        .unwrap();
    let real_cursor = initial.next_cursor.expect("initial page has cursor");

    // Attempt continuation against a DIFFERENT tool — must be rejected.
    // discover() first to see what other tool id exists; we don't have
    // one registered, so we use an unknown id and expect ERR_CURSOR_INVALID
    // (tool_id mismatch comes BEFORE tool-lookup-not-found in
    // run_tool_continue's flow).
    let result = client
        .call_page(
            "other:not_page_gen",
            serde_json::Value::Null,
            Some(&real_cursor),
            CallOptions::default(),
        )
        .await;

    // Server returns Response::Error → SDK maps to ToolExecutionFailed
    // with the code embedded in the inner source. We walk the error
    // source chain to find the 1021 indicator.
    match result {
        Err(e) => {
            use std::error::Error as StdError;
            let mut chain = format!("{e}");
            let mut src: Option<&dyn StdError> = e.source();
            while let Some(s) = src {
                chain.push_str(" | ");
                chain.push_str(&format!("{s}"));
                src = s.source();
            }
            assert!(
                chain.contains("1021")
                    || chain.contains("tool_id mismatch")
                    || chain.contains("cursor invalid"),
                "expected ERR_CURSOR_INVALID with tool_id mismatch, got chain: {chain}"
            );
        }
        Ok(p) => panic!("expected cross-tool continuation to fail, got page: {p:?}"),
    }

    let _ = DiscoverFilter::default(); // suppress unused import
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_events_tag_cursor_page_for_continuations() {
    use atd_runtime::JsonLinesAuditSink;
    use std::sync::Mutex;

    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("audit.sock");
    let mut reg = Registry::new();
    reg.register(Arc::new(RowGenerator::new()));

    // In-memory audit sink so the test can inspect emitted events.
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for SharedBuf {
        fn write(&mut self, bs: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bs);
            Ok(bs.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let sink = JsonLinesAuditSink::new(Box::new(SharedBuf(buf.clone())));
    let sink_arc: Arc<dyn atd_runtime::AuditSink> = Arc::new(sink);

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        audit_sink: Some(sink_arc),
        ..ServerConfig::default()
    };
    let server = Server::new(reg, cfg);
    let task = tokio::spawn(server.run());
    wait_for_sock(&sock).await;

    let client = AtdClient::connect_with_options(Endpoint::unix(sock.clone()), fast_connect())
        .await
        .unwrap();

    let opts = CallAllOptions {
        max_pages: 20,
        ..Default::default()
    };
    let _ = client
        .call_all("conformance:page_gen", serde_json::json!({}), opts)
        .await
        .unwrap();

    // Give the mpsc drain task time to flush.
    tokio::time::sleep(Duration::from_millis(150)).await;

    let text = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    let lines: Vec<&str> = text.split_terminator('\n').collect();
    // 10 pages = 1 RunTool + 9 RunToolContinue = 10 audit events.
    assert_eq!(
        lines.len(),
        (TOTAL_ROWS / ROWS_PER_PAGE) as usize,
        "expected one audit event per page, got {} lines:\n{text}",
        lines.len()
    );

    // RunTool path (page 1) emits no cursor_page tag — that's wired only
    // in run_tool_continue. Pages 2..10 should carry cursor_page = 2..10.
    let cursor_pages: Vec<Option<u64>> = lines
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("parse");
            v.get("cursor_page").and_then(|c| c.as_u64())
        })
        .collect();
    // Page 1 (initial RunTool) has no cursor_page (None).
    assert_eq!(
        cursor_pages[0], None,
        "first event must have no cursor_page"
    );
    // Pages 2..10 should all have cursor_page = their page index.
    for (i, cp) in cursor_pages[1..].iter().enumerate() {
        let expected = (i as u64) + 2; // 2, 3, ..., 10
        assert_eq!(*cp, Some(expected), "audit event {i} cursor_page mismatch");
    }

    task.abort();
}
