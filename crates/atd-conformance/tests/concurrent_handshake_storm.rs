//! SP-concurrency-baseline §5.5 — `concurrent_handshake_storm` conformance scenario.
//!
//! Spawns N simultaneous clients each running Hello + ToolList + ToolSchema × 5
//! against a real `atd-ref-server` bound on a tempdir UDS. Asserts the §4 SLO
//! table:
//!
//! - p99 wall-clock per client < 200ms on a 4-core developer host
//! - zero connection errors across all N clients
//! - zero audit drops (the §5.4 mpsc cap absorbs the burst)
//!
//! This is the test that closes the 2026-05-12 celia incident: pre-SP the
//! ref-server's single-thread runtime serialized handshakes through one OS
//! thread, hermes session-init timed out, 60% of sessions failed to load
//! tool schemas. After Phases B-F, 50 concurrent clients complete clean.
//!
//! CI runners (2 vCPU) can dial the storm size down via `ATD_CONFORMANCE_STORM_N`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use atd_ref_server::builtin::builtin_registry;
use atd_runtime::{AuditSink, JsonLinesAuditSink};
use atd_sdk::{AtdClient, ConnectOptions, DiscoverFilter, Endpoint};
use atd_server::{Server, ServerConfig};

fn storm_n() -> usize {
    std::env::var("ATD_CONFORMANCE_STORM_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50)
}

/// Returns (p50_ms, p99_ms) over a sorted set of per-client durations.
fn percentiles(mut durations: Vec<Duration>) -> (u64, u64) {
    durations.sort();
    let n = durations.len();
    if n == 0 {
        return (0, 0);
    }
    let p50_idx = n / 2;
    // For p99 we use the canonical ceil((n-1) * 0.99) which matches the
    // SLO table — the worst-case slowest 1% of clients.
    let p99_idx = ((n as f64 - 1.0) * 0.99).ceil() as usize;
    let p50 = durations[p50_idx].as_millis() as u64;
    let p99 = durations[p99_idx].as_millis() as u64;
    (p50, p99)
}

/// In-memory writer used as the audit sink target so the storm doesn't
/// fill a real file or stdout.
struct DiscardWriter;
impl std::io::Write for DiscardWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_handshake_storm() {
    let n = storm_n();
    let dir = tempfile::tempdir().unwrap();
    let sock: PathBuf = dir.path().join("storm.sock");

    // Real audit sink so the storm exercises the §5.4 mpsc path. We discard
    // the writes; the test only cares about the drop count.
    let audit_sink = Arc::new(JsonLinesAuditSink::new(Box::new(DiscardWriter)));
    let audit_arc: Arc<dyn AuditSink> = audit_sink.clone();

    // Full ref-server registry — 10 tools, including 1 hidden — matches the
    // shape of a real adopter (celia's 19 tools; we use 10 for ref-server,
    // close enough for the SLO assertion).
    let registry = builtin_registry(/* enable_conformance_tool = */ false);

    let cfg = ServerConfig {
        socket_path: sock.clone(),
        audit_sink: Some(audit_arc.clone()),
        ..ServerConfig::default()
    };
    let server = Server::new(registry, cfg);
    let server_task = tokio::spawn(server.run());

    // Wait for the socket to appear.
    let deadline = Instant::now() + Duration::from_secs(3);
    while !sock.exists() {
        if Instant::now() > deadline {
            panic!("server did not bind socket within 3s");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Per-client tool ids drawn from discover() — but since every client
    // discovers, we pre-collect once with a probe client so each storm
    // client doesn't pay the discover cost in its measured window.
    let probe = AtdClient::connect(Endpoint::unix(sock.clone()))
        .await
        .expect("probe connect");
    let tools = probe
        .discover(None, DiscoverFilter::default())
        .await
        .expect("probe discover");
    let tool_ids: Vec<String> = tools.iter().take(5).map(|t| t.id.clone()).collect();
    assert!(
        !tool_ids.is_empty(),
        "ref-server should expose at least 1 tool"
    );
    drop(probe);

    let connect_opts = ConnectOptions {
        max_attempts: 5,
        backoff_base_ms: 25,
        backoff_cap_ms: 200,
        connect_timeout_ms: 2000,
    };

    // Fire all N clients concurrently. Each measures its own wall clock
    // for the full Hello + discover + 5×describe sequence.
    let started_storm = Instant::now();
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        let sock = sock.clone();
        let tool_ids = tool_ids.clone();
        let opts = connect_opts.clone();
        handles.push(tokio::spawn(async move {
            let t0 = Instant::now();
            let client = AtdClient::connect_with_options(Endpoint::unix(sock), opts).await?;
            client
                .hello(Some(&format!("storm-{i}")), Vec::new())
                .await?;
            let _ = client.discover(None, DiscoverFilter::default()).await?;
            for tid in tool_ids.iter() {
                let _ = client.describe(tid).await?;
            }
            Ok::<Duration, atd_protocol::AtdError>(t0.elapsed())
        }));
    }

    let mut durations = Vec::with_capacity(n);
    let mut errors = 0u32;
    for h in handles {
        match h.await.expect("join") {
            Ok(d) => durations.push(d),
            Err(_e) => errors += 1,
        }
    }
    let wall = started_storm.elapsed();

    let (p50_ms, p99_ms) = percentiles(durations.clone());
    let drops = audit_arc.drops();

    eprintln!(
        "storm: n={n} wall={wall:?} p50={p50_ms}ms p99={p99_ms}ms errors={errors} audit_drops={drops}"
    );

    assert_eq!(errors, 0, "expected 0 errors across {n} clients");
    // SLO: p99 < 200ms on 4-core dev hardware; on slower CI we accept 2x.
    let slo_p99 = if std::env::var("CI").is_ok() {
        400
    } else {
        200
    };
    assert!(
        p99_ms < slo_p99,
        "p99 {p99_ms}ms exceeds SLO {slo_p99}ms (n={n}, p50={p50_ms}ms)"
    );
    assert_eq!(
        drops,
        0,
        "audit sink dropped {drops} events at default capacity 1024; queue should absorb {n}*7={total}",
        total = n * 7
    );

    server_task.abort();
}
