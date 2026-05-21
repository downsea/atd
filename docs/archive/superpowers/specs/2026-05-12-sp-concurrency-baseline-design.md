# SP-concurrency-baseline: concurrency as a protocol-level invariant

| Status | Draft |
| Created | 2026-05-12 |
| Author | cross-project subagent (celia_phr 10-concurrent benchmark incident ↔ atd-mvp coordination) |
| Phase | ATD post-`sp-medical-middleware`; depends on no other in-flight SP |
| Related | SP-listener-extract (`2026-04-25-sp-listener-extract-design.md`) — kept runtime transport-agnostic; this SP makes it concurrent-safe too. SP-streamable-http (`2026-05-11-sp-streamable-http-design.md`) — HTTP sibling whose runtime flavor is adopter-controlled; this SP aligns the UDS reference path. SP-12 (`2026-04-25-sp12-canonical-dispatch.md`) — canonical dispatch lands in a hot loop that this SP audits. |

---

## 1. Motivation

**1.1 Concurrency is the most basic property a tool-dispatch protocol must support, and `atd-mvp` does not deliver it today.** On 2026-05-12 the celia_phr team ran a 10-query × 10-concurrency benchmark against the ATD reference stack (DeepSeek V4 Pro ↔ Hermes Agent ↔ MCP stdio ↔ `atd-mcp-bridge` ↔ `atd-ref-server`). Six of ten sessions failed to load any tools: their `prompt_tokens` collapsed from the expected ~5200 (tool-schema-loaded baseline) to ~180-190 (no-tools fallback). Hermes's per-session log showed the smoking gun:

```
2026-05-12 10:55:19,321 Failed to connect to MCP server 'celia' (atd-mcp-bridge): Connection lost
2026-05-12 10:55:19,367 MCP server 'celia' failed initial connection after 3 attempts, giving up
```

Wall clock was 71s for 10 queries — ~3× speedup over a 200s serial baseline — yet 60% of the workload silently degraded. For a *reference implementation of a tool-dispatch protocol* that ships an MCP bridge as one of its three blessed adoption modes (`crates/atd-mcp-bridge`, `docs/integrations/hermes.md`), a 60% failure rate at 10 concurrency is a protocol-level credibility hit, not a tuning issue.

**1.2 Root cause is unambiguous and concentrates in three lines of code.** Investigation traced the failure to:

- `crates/atd-ref-server/src/main.rs:75` — `#[tokio::main(flavor = "current_thread")]`. The reference server's accept loop and every spawned `handle_connection` task share one OS thread. When ten bridges connect simultaneously, the per-task quantum on a contended host (10 LLM HTTPS clients also active) stretches past Hermes's initial-connect deadline (~3-5s typical for stdio MCP). Server is healthy; the thread is just slow to *visit* each socket.
- `crates/atd-protocol/src/wire.rs:25` — `read_frame` is unbounded. A bridge that completes `UnixStream::connect()` then calls `ping()` blocks forever if the server task is preempted; the bridge cannot self-diagnose "I'm stuck" and emit a fast retry.
- `crates/atd-sdk/src/client.rs:33-45` — `AtdClient::connect()` performs one `UnixStream::connect()` + one `ping()`, no retry, no jitter, no timeout. Transient EAGAIN-class failures during spawn-storm propagate as fatal errors to the MCP transport, which has its own retry budget (3 attempts in Hermes) but no backoff. All three retries hit the same single-threaded server and all three fail identically.
- `crates/atd-mock-weather-server/src/main.rs:32` — same `current_thread` flavor; the cross-vendor demo binary inherits the bug.

`atd-server-http` (SP-streamable-http) is fine — adopters provide their own `#[tokio::main]` and celia_phr already uses `multi_thread`. The bug is constrained to ref binaries + wire/SDK plumbing.

**1.3 A second-order bottleneck hides behind the first.** `crates/atd-runtime/src/audit.rs:67-108` ships `JsonLinesAuditSink` with a `std::sync::Mutex<Box<dyn Write + Send>>` and a synchronous `write_all` + `flush` inside `on_call`. Every successful dispatch acquires this mutex and blocks the calling tokio task on synchronous file I/O. Under `current_thread` runtime this stalls the **entire reactor** — accept loop included. Once §1.2 is fixed, this is the next concurrency cliff at ~50-100 concurrent calls per second per audit-enabled deployment. Fixing both at once means adopters never hit the cliff.

**1.4 The right shape of the answer is to make concurrency a *protocol-level invariant*, not a tuning suggestion.** SP-listener-extract codified "runtime is transport-agnostic." SP-12 codified "dispatch is canonical." This SP codifies "concurrency is baseline." We achieve that by:

1. Fixing the three lines + the audit-sink path.
2. Defining concrete SLOs and writing them into `atd-conformance` so any third-party ATD server impl must meet the same bar to claim conformance.
3. Adding a `crates/atd-bench` criterion suite so the SLOs are *measured*, not asserted, every commit.

After this SP, an adopter spawning 100 simultaneous bridges against `atd-ref-server` should see p99 handshake latency under 200ms and zero connection-lost errors, on commodity hardware (4-core / 16GB), without any tuning.

## 2. Goals

- **G1: ref-server multi-thread by default.** `atd-ref-server` and `atd-mock-weather-server` switch to `flavor = "multi_thread"` with `worker_threads` defaulted to `min(cpus, 4)` and overridable by env (`ATD_WORKER_THREADS`).
- **G2: wire-level deadlines.** `read_frame` and `write_frame` accept an `Option<Duration>` and surface a typed `WireError::Timeout` distinct from `WireError::Io`. Default deadline is configurable via `SharedServerConfig.frame_deadline_ms` (server side, default 30s for the active state, 5s for the post-Hello handshake state) and via `AtdClient` builder (client side, default 10s).
- **G3: SDK connect retry with jitter.** `AtdClient::connect()` retries on EAGAIN / ECONNRESET / WouldBlock / TimedOut up to N times with exponential backoff + ±20% jitter. Defaults: 5 attempts, base 50ms, cap 800ms. Overridable via env (`ATD_CONNECT_RETRIES`, `ATD_CONNECT_BACKOFF_BASE_MS`, `ATD_CONNECT_BACKOFF_CAP_MS`). Fatal errors (path missing, permission denied) skip retry.
- **G4: audit-sink dedicated-writer task.** `JsonLinesAuditSink` is rewritten to enqueue `CallEvent`s onto a bounded `tokio::sync::mpsc` (default 1024 slots) drained by one dedicated task that owns the `Write` handle. `on_call` is non-blocking under contention; if the queue is full, the event is dropped and a `audit_drop` counter is incremented (the existing `log loss >> dispatch stall` invariant from `audit.rs:65-66` is preserved). The trait `AuditSink::on_call` remains `&self, &CallEvent` so existing impls don't break.
- **G5: concurrent-handshake conformance test.** `atd-conformance` gains a `concurrent_handshake_storm` scenario: spawn 50 simultaneous clients each running Hello + ToolList + ToolSchema × 5; assert p99 < 200ms, zero connection errors, zero dropped audit events. Pass criterion is reproducible on the GitHub Actions standard runner (2 vCPU / 7GB).
- **G6: bench crate.** New `crates/atd-bench` ships criterion benchmarks for: `ping_rtt`, `handshake_with_caps`, `tool_list_19_tools`, `tool_schema_lookup`, `run_tool_echo`, `concurrent_dispatch_10`. Output baselines are committed; pre-commit gate fails any commit that regresses by >20%.
- **G7: observability hooks.** `atd-runtime` exposes lock-free atomic counters (`AtomicU64`) reachable via `Server::metrics_snapshot()` — `accepted_connections`, `dispatched_requests`, `dispatch_errors_by_code`, `audit_events_total`, `audit_drops_total`, `dispatch_p50_us`, `dispatch_p99_us` (the percentiles via a small `quanta::Histogram` or hand-rolled HDR-lite). Adopters scrape this in their own /metrics endpoint.
- **G8: documentation.** `docs/architecture.md` gains a new §11 "Deployment shapes & concurrency" with the two blessed shapes (desktop UDS + cloud HTTP), the SLO table, and the failure mode that motivated this SP.

## 3. Non-goals

- **Async `AuditSink` trait migration.** Keeping `on_call(&self, &CallEvent)` synchronous is intentional — adopters with their own sinks (rdb, syslog, cloud loggers) implement whatever async model they want behind the trait without changing the public surface. The mpsc lives inside `JsonLinesAuditSink` only.
- **Connection pooling on the bridge side.** `atd-mcp-bridge` remains one-process-per-Hermes-session per MCP stdio convention. After §G1 the spawn-storm is no longer the bottleneck and a daemonized bridge is unjustified weight. Revisit if real workloads measure >100 sessions/host.
- **HTTP/2 multiplexing of `atd-server-http`.** SP-streamable-http already lands HTTP/1.1 keep-alive via axum/hyper, which is sufficient for the cloud-tenant case. HTTP/2 is a future SP if measured RTT × N becomes a real problem.
- **Reactor-style or io_uring-style transports.** Tokio's epoll on Linux is sufficient for our SLOs. We are not in the business of replacing it.
- **`async fn read_frame` deadline via cancellation tokens.** We add a `tokio::time::timeout` wrapper at the call site. A typed `WireError::Timeout` is enough; no new cancellation primitives.
- **Backwards-compat with pre-SP-12 servers under storm.** Pre-SP-12 servers do not exist in deployed adopters (celia_phr and healthkit_cli both run post-SP-12 builds). We do not test pre-SP-12 under storm.
- **Cluster-mode or multi-host ATD.** Out of scope for v0.3.x; revisit when the v3 multi-device whitepaper material lands as an SP.
- **Result pagination for large tool outputs.** Healthkit's `query_observations` and celia's `bulk_export` both return arrays that can exceed 1MB and stall the writer task during a frame write — same performance theme as this SP but a *protocol-shape* change (new `Request::RunToolContinue`, new `Response.next_cursor`, new `Tool::call_stream` author API). Lives in the sibling **`SP-pagination-v1`** (same `perf-v1` iteration umbrella, separate tag). Cross-link: `docs/superpowers/specs/2026-05-12-sp-pagination-v1-design.md`.
- **Tokio runtime tuning beyond `flavor` and `worker_threads`.** No `max_blocking_threads`, no `event_interval` knobs. Defaults work; complexity later.

## 4. Performance SLOs

These are the contracted numbers — both `atd-bench` regression gates and `atd-conformance` assertions key off this table. They hold on a 4-core / 16GB commodity Linux host with `cargo build --release` binaries; CI runners (2 vCPU) accept 2× of these and the conformance test scales accordingly.

| Metric | SLO | Measured on |
|---|---|---|
| `ping_rtt` p50 | < 200 μs | `atd-bench` |
| `ping_rtt` p99 | < 1.5 ms | `atd-bench` |
| `handshake_with_caps` p99 (Hello + ToolList) | < 3 ms | `atd-bench` |
| `tool_schema_lookup` p99 (19 tools registered) | < 1 ms | `atd-bench` |
| `run_tool_echo` p99 (full dispatch + audit) | < 5 ms | `atd-bench` |
| Concurrent dispatch (10 clients × 1 RPC each, fan-in) wall | < 50 ms | `atd-bench` |
| Conformance storm (50 clients × Hello+List+5×Schema) p99 | < 200 ms | `atd-conformance` |
| Conformance storm — connection errors | 0 / 50 | `atd-conformance` |
| Conformance storm — audit drops | 0 / 50 | `atd-conformance` |
| Audit write — burst write 10k events, no flush stall | < 100 ms total | `atd-bench` |

The pre-commit gate is `cargo bench --bench atd-bench -- --baseline current` reporting <20% regression on every benchmark. Bench baselines live at `crates/atd-bench/baselines/` and are committed.

## 5. Design

This is ~50% of the SP. Each subsection is one of the eight decision points; each gives the chosen answer, evidence from existing source, and the rejected alternatives.

### 5.1 Server runtime: multi-thread by default, configurable via env

**Decision.** `atd-ref-server/src/main.rs` and `atd-mock-weather-server/src/main.rs` switch the macro to:

```rust
#[tokio::main(flavor = "multi_thread", worker_threads = atd_runtime::default_worker_threads())]
```

where `atd_runtime::default_worker_threads()` is a new helper:

```rust
pub fn default_worker_threads() -> usize {
    std::env::var("ATD_WORKER_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2))
}
```

Cap at 4 by default because the dispatch hot path is I/O-bound (frame read/write + tool call). Past 4 workers we see context-switch overhead beat marginal parallelism gain on the bench workload. Operator can lift the cap by setting `ATD_WORKER_THREADS=8` (or whatever) on hosts with more cores.

**Evidence + why.** The current single-thread flavor was a v0.1.x simplification ("one binary, no thread pool needed for 1 RPS"), but the bug at §1.2 shows it does not survive contact with realistic adopter workloads. Tokio's `multi_thread` runtime is the default for production network services for exactly this reason. The 4-worker cap mirrors common practice (axum's defaults, redis's IO threads cap).

The `available_parallelism().min(4)` formula trades two-cell-phone concerns: (a) on 16+-core dev machines we don't want 16 workers per ref-server burning CPU when one client is connected; (b) on 1-core VMs we don't want to spawn N workers on a single core (tokio does this fine but it's wasteful). Capping at 4 covers 95% of cases; the env override covers the rest.

**Why not `flavor = "multi_thread"` with default `worker_threads`.** Tokio's default is `num_cpus()`, which on a 32-core dev machine spawns 32 idle worker threads per ref-server. For a *reference* binary frequently launched by adopter tests (celia spawns one per integration test), that's hostile. Bounded default + env override is the right defensive default.

**Why not `current_thread` + busy-wait optimization.** Tempting on paper (zero scheduling overhead, no MPSC channels between workers). But the failure mode is fundamentally "ten things want CPU at once and there is one CPU"; no scheduler tweak on a one-CPU runtime fixes it. Multi-thread is the structural answer.

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Keep `current_thread`, document tuning | Zero code change | Adopters keep tripping; protocol credibility hit | rejected |
| `multi_thread` with `worker_threads = num_cpus()` | Trivial | Wasteful on big dev machines; tests spawn 32-thread reactors | rejected |
| `multi_thread` with `worker_threads = min(cpus, 4)` + env override | Sane default + escape hatch | One helper fn in atd-runtime | **chosen** |
| Spawn-blocking model with dedicated dispatch pool | Predictable | Complexity not justified; multi_thread already solves it | deferred |

### 5.2 Wire deadlines: `tokio::time::timeout` at the read/write call site, typed error

**Decision.** `atd-protocol/src/wire.rs` keeps the current `read_frame` / `write_frame` signatures untouched (back-compat for in-tree callers) and adds two new variants:

```rust
pub async fn read_frame_with_deadline<R, T>(
    reader: &mut R,
    deadline: Option<Duration>,
) -> Result<T, WireError>
where R: AsyncRead + Unpin, T: DeserializeOwned;

pub async fn write_frame_with_deadline<W, T>(
    writer: &mut W,
    msg: &T,
    deadline: Option<Duration>,
) -> Result<(), WireError>
where W: AsyncWrite + Unpin, T: Serialize;
```

`WireError` is a new typed enum (replacing today's `std::io::Result`) with variants `Io(std::io::Error)`, `Timeout(Duration)`, `Decode(serde_json::Error)`, `LengthOverflow(u32)`. The existing untyped helpers become thin wrappers passing `deadline = None`.

Server side: `crates/atd-server/src/connection.rs:32` switches to `read_frame_with_deadline(&mut reader, Some(state.config.frame_deadline()))`. Client side: `crates/atd-sdk/src/client.rs:154-155` similarly.

`SharedServerConfig` gains two new fields:

```rust
pub struct SharedServerConfig {
    // ... existing ...
    pub frame_deadline_active_ms: u64,    // default 30_000 (during active dispatch)
    pub frame_deadline_handshake_ms: u64, // default 5_000 (during pre-Hello window)
}
```

The handshake window means the per-connection task at `connection.rs:23-39` tracks "has Hello completed yet" and applies the shorter deadline before it lands. A stuck handshake fails fast; an active long-running tool (e.g., `host:media.convert` taking 25s) gets the longer deadline.

**Evidence + why.** The §1.2 bug is partly that bridge's `ping()` after `connect()` is unbounded — the bridge has no fast path back to "retry the connect." Adding a 5s deadline on the freshly-connected socket means the bridge will see `WireError::Timeout` instead of hanging until SIGTERM. The asymmetric handshake-vs-active deadline pattern is standard (gRPC's `initial_connection_window_ms` distinct from per-stream).

**Why a new `WireError` enum and not `io::Error::Timeout`.** `io::Error` is fine for plumbing but callers can't distinguish "frame parse failed because peer sent garbage" from "frame parse failed because timed out before peer wrote." Both are actionable in different ways (the latter is retryable; the former is fatal). A typed enum makes the distinction visible at every call site.

**Why not `tokio::io::AsyncReadExt::read_exact` with `tokio::time::timeout` inline.** That's effectively what the impl is; the helper hides the awkward `tokio::select!` ceremony and centralizes the typed error.

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Keep `io::Result`, add `timeout` at call sites | No new types | Callers reinvent the timeout wrapper; lossy error info | rejected |
| Replace existing helpers with deadline-mandatory signatures | Forces every caller to pass a deadline | Breaks adopter call sites (downstream `path =` deps) | rejected |
| Add sibling `*_with_deadline` helpers + typed enum | Back-compat; opt-in | Two helper variants per direction | **chosen** |
| Async-stream-based codec (tokio-util `Framed`) | Idiomatic for long-lived streams | More dep weight; we already have a working codec | deferred |

### 5.3 SDK connect retry: exponential backoff + jitter, fatal-error short-circuit

**Decision.** `AtdClient::connect` grows a private `connect_with_retry` helper:

```rust
async fn connect_with_retry(endpoint: Endpoint, opts: ConnectOptions) -> Result<Self, AtdError> {
    let mut delay_ms = opts.backoff_base_ms;
    let mut last_err = None;
    for attempt in 0..opts.max_attempts {
        match Self::connect_once(&endpoint).await {
            Ok(c) => return Ok(c),
            Err(e) if is_fatal_connect_error(&e) => return Err(e), // path missing, permission denied
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < opts.max_attempts {
                    let jitter = rand::random::<f64>() * 0.4 - 0.2; // ±20%
                    let wait = (delay_ms as f64 * (1.0 + jitter)) as u64;
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                    delay_ms = (delay_ms * 2).min(opts.backoff_cap_ms);
                }
            }
        }
    }
    Err(last_err.unwrap())
}
```

`ConnectOptions` is a new public struct with `Default` returning the env-configured values (`ATD_CONNECT_RETRIES=5`, `ATD_CONNECT_BACKOFF_BASE_MS=50`, `ATD_CONNECT_BACKOFF_CAP_MS=800`, `ATD_CONNECT_TIMEOUT_MS=10000` — the last one wraps each `connect_once` attempt in a `tokio::time::timeout`).

`Self::connect_once` is the current `UnixStream::connect()` + `ping()` body. `is_fatal_connect_error` matches `AtdError::Io(e)` where `e.kind() ∈ {NotFound, PermissionDenied}` — paths that won't fix themselves with retry.

**Public surface change.** `AtdClient::connect(endpoint)` becomes `AtdClient::connect(endpoint)` (unchanged) but reads defaults from env. New `AtdClient::connect_with_options(endpoint, opts)` for explicit control. Existing call sites in `atd-mcp-bridge/src/main.rs:57` and the test suite need no edit.

**Evidence + why.** Three of three Hermes retries in the §1.1 log failed identically because the single-threaded server was equally starved on each attempt. Backoff + jitter spreads the second attempt past the spawn-storm window, when the server is already past the worst contention. The `rand` crate is already a transitive dep (via `getrandom`).

**Why env-configurable, not constructor-only.** Adopter tests want to override defaults *without* code edits. celia_phr's CI runs against a slower socket-activated test fixture and wants `ATD_CONNECT_RETRIES=10`; their test command becomes `ATD_CONNECT_RETRIES=10 cargo test` instead of patching test setup code. Env + builder is the standard escape hatch.

**Why ±20% jitter.** Standard mitigation against retry-storm synchronization (RFC 3445 §3, AWS Architecture blog). 20% is enough to break lockstep without making the backoff curve unpredictable.

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| No retry; let MCP transport handle it | Simpler SDK | Hermes-side 3 attempts all hit the same starved server; insufficient | rejected |
| Fixed-delay retry (e.g., 100ms × 5) | Trivial | Synchronized retry storms; misses the "second wave hits already-warm server" benefit of backoff | rejected |
| Exponential backoff + jitter + fatal short-circuit | Industry standard; respects "path missing" as terminal | Slight code complexity; one `rand` import | **chosen** |
| Token-bucket-rate-limited retry | Smooths burst | Premature; client-driven smoothing belongs in the caller | deferred |

### 5.4 AuditSink hot path: dedicated writer task drained from bounded mpsc

**Decision.** `JsonLinesAuditSink` is rewritten:

```rust
pub struct JsonLinesAuditSink {
    tx: tokio::sync::mpsc::Sender<CallEvent>,
    drops: Arc<AtomicU64>,
}

impl JsonLinesAuditSink {
    pub fn new_with_writer(writer: Box<dyn Write + Send + 'static>) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<CallEvent>(1024);
        let drops = Arc::new(AtomicU64::new(0));
        let drops_for_task = drops.clone();
        tokio::spawn(async move {
            let mut writer = writer;
            while let Some(ev) = rx.recv().await {
                if let Ok(mut line) = serde_json::to_vec(&ev) {
                    line.push(b'\n');
                    let _ = writer.write_all(&line);
                    let _ = writer.flush();
                }
            }
            // channel closed — sink dropped; one final flush
            let _ = writer.flush();
            let _ = drops_for_task;
        });
        Self { tx, drops }
    }

    pub fn drops(&self) -> u64 { self.drops.load(Ordering::Relaxed) }
}

impl AuditSink for JsonLinesAuditSink {
    fn on_call(&self, event: &CallEvent) {
        match self.tx.try_send(event.clone()) {
            Ok(()) => {}
            Err(_) => {
                self.drops.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}
```

`try_send` is non-blocking: full-queue means drop the event and bump the counter. The "log loss >> dispatch stall" invariant in `audit.rs:65-66` is preserved by-design.

**Adopter impact.** `JsonLinesAuditSink::new(writer)` constructor changes signature (was `&self` builder; now spawns a tokio task — requires a tokio runtime context). The shipped helpers `::stdout()`, `::stderr()`, `::file(path)` work identically from a caller perspective. The two adopters audited so far (celia_phr and healthkit_cli) call only the shipped helpers; no migration needed.

**Why `try_send` and not `blocking_send` with a fallback.** Blocking send under load defeats the purpose. The whole point is `on_call` must be non-blocking on the dispatch hot path. Drops are observable via the counter, exposed in `Server::metrics_snapshot()` (G7).

**Why a synchronous trait wrapping an async sender.** Keeps the `AuditSink` trait usable from synchronous adopter code (e.g., a rusqlite-backed audit sink that does its own threading). The mpsc lives inside the JSON-lines impl only.

**Channel sizing.** 1024 events × ~500 bytes/event = 512 KB peak buffer. At 10k events/sec sustained (a stress workload), the channel drains in 0.1s; transient bursts are absorbed. Operator can override via `JsonLinesAuditSink::new_with_capacity(writer, n)`.

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Keep `std::sync::Mutex<Writer>` + sync I/O | Trivial | Stalls the reactor; §1.3 cliff | rejected |
| Mutex + `spawn_blocking` per event | Drops mutex contention | One blocking-pool task per call; pool starvation under storm | rejected |
| `tokio::sync::Mutex` + async writer | Better than std::Mutex | Still serializes the lock; doesn't help under storm | rejected |
| Bounded mpsc + dedicated writer task | Non-blocking on hot path; bounded memory; observable drops | One spawned task per sink (cheap) | **chosen** |
| Lock-free ring buffer (e.g., `crossbeam`) | Faster than mpsc | New dep weight; mpsc is sufficient at our SLO | deferred |

### 5.5 Conformance: `concurrent_handshake_storm` scenario

**Decision.** `atd-conformance/src/scenarios/concurrent_handshake_storm.rs` is a new scenario. Pseudocode:

```
let n = 50; // operator-overridable via env ATD_CONFORMANCE_STORM_N
let mut joins = Vec::new();
let started = Instant::now();
for i in 0..n {
    joins.push(tokio::spawn(async move {
        let t0 = Instant::now();
        let client = AtdClient::connect(endpoint.clone()).await?;
        client.hello(Some(&format!("storm-{i}")), vec![]).await?;
        client.discover(None, DiscoverFilter::default()).await?;
        for tool_id in tool_ids.iter().take(5) {
            client.describe(tool_id).await?;
        }
        Ok::<_, AtdError>(t0.elapsed())
    }));
}
let results: Vec<Result<Duration, AtdError>> = futures::future::join_all(joins).await.into_iter().map(|j| j.unwrap()).collect();
// Assertions:
assert_eq!(results.iter().filter(|r| r.is_err()).count(), 0);
let durations: Vec<Duration> = results.iter().map(|r| r.unwrap()).collect();
assert!(p99(&durations) < Duration::from_millis(200));
assert_eq!(server.metrics_snapshot().audit_drops_total, 0);
```

The fixture spawns one `atd-ref-server` instance + 19 dummy tools (mirroring celia's registry size from §1.1) before the storm. Output is a structured `conformance_report.json` with per-client durations, percentiles, drop counts.

**Why 50 not 10.** §1.1 showed 10 already breaks today; the conformance bar must be ambitious. 50 is what we'd expect a single celia_phr host to absorb during a real benchmark run. CI runners (2 vCPU) cannot hit p99 < 200ms at 50 concurrency; the env override `ATD_CONFORMANCE_STORM_N=25` is documented and CI uses it. The local-machine assertion is the published SLO.

**Why include audit-drops in the assertion.** §5.4's mpsc with a 1024 capacity should not drop a single event under 50 concurrent clients × 7 RPCs = 350 events. If it does, either the mpsc impl is buggy or the channel size is wrong; either way we want the test to fail visibly.

**Why include the Hello in the loop.** §5.1 caps the handshake deadline at 5s; the storm test exercises this path with realistic Hello + cap-grant + UCAN-empty handshake.

### 5.6 Bench crate: criterion-based perf regression gate

**Decision.** New `crates/atd-bench` crate (not a workspace lib, just a `[[bench]]` target collection):

```
crates/atd-bench/
├── Cargo.toml          # dev-dependencies: criterion, tokio, atd-* path deps
├── README.md
├── benches/
│   ├── ping_rtt.rs
│   ├── handshake.rs
│   ├── tool_list.rs
│   ├── tool_schema.rs
│   ├── run_tool_echo.rs
│   ├── concurrent_dispatch.rs
│   └── audit_burst.rs
└── baselines/
    ├── ping_rtt.json
    ├── handshake.json
    ├── ... (one per bench)
    └── README.md       # "How to update baselines; require maintainer review"
```

Each bench spins up a single `atd-ref-server` against a tempdir UDS, registers an echo stub, and exercises the path. Criterion produces JSON output; we commit the *median* as the baseline. CI computes `(current / baseline - 1.0) * 100`; >20% regression on any bench fails the build.

**Bench list mirrors §4 SLOs.** Bench names ↔ SLO rows one-to-one. If the SLO table grows, a bench is added; if a bench is removed, the SLO row is too.

**Why criterion, not custom.** Criterion is the de-facto Rust micro-benchmarking standard; ships HDR-histogram-ish output and stability tests. Adopters likely already have it in their dep tree.

**Why `target/criterion` not committed.** Per-PR runs produce noisy detail; only the committed baselines are durable.

**Why a separate crate not `cargo bench` inside each crate.** Benches that need a *running server* must orchestrate startup; that orchestration is shared. Splitting per-crate forces duplication. One bench crate that imports `atd-server`, `atd-runtime`, `atd-sdk`, `atd-tools-*` is right.

### 5.7 Observability: lock-free atomic counters surfaced via `Server::metrics_snapshot()`

**Decision.** `atd-runtime/src/metrics.rs` (new module) defines:

```rust
#[derive(Default)]
pub struct MetricsCounters {
    pub accepted_connections: AtomicU64,
    pub dispatched_requests: AtomicU64,
    pub dispatch_errors_by_code: DashMap<u16, AtomicU64>, // sparse; lazy insert
    pub audit_events_total: AtomicU64,
    pub audit_drops_total: AtomicU64,
}

#[derive(Serialize, Debug, Clone)]
pub struct MetricsSnapshot {
    pub accepted_connections: u64,
    pub dispatched_requests: u64,
    pub dispatch_errors_by_code: BTreeMap<u16, u64>,
    pub audit_events_total: u64,
    pub audit_drops_total: u64,
    pub uptime_seconds: u64,
}
```

`ServerState` (currently `crates/atd-server/src/server.rs:32-46`) gains a `pub metrics: Arc<MetricsCounters>`. Dispatch increments on entry; audit-sink writes increment its counters; the conformance/bench tests scrape via `server.metrics_snapshot()`.

**Latency percentiles deferred.** §4's `dispatch_p50_us` / `dispatch_p99_us` need a histogram; `quanta::Instant` + a small in-memory ring buffer is the lightest impl, but it bloats this SP. Phase H ships counters; a follow-up `SP-observability-v2` ships latency histos and a `/metrics` Prometheus endpoint via `atd-server-http`.

**Why `DashMap` for error-code → counter.** Sparse: most codes are never hit. `Mutex<HashMap>` would block on entry; `DashMap` is lock-free per-bucket and adds ~30KB to the dep tree (acceptable).

**Why no metrics on the SDK side.** Adopters wrap `AtdClient::call` themselves with their own metrics; SDK-side counters would duplicate state and confuse the per-call vs per-connection axis. Keep state on the server.

### 5.8 Documentation: architecture.md §11 "Deployment shapes & concurrency"

**Decision.** New section in `docs/architecture.md` (after the current §10 status table). Outline:

```
## 11. Deployment shapes & concurrency

ATD ships two blessed transport shapes. Pick by deployment context:

### 11.1 Desktop / sidecar (UDS via `atd-server` + `atd-mcp-bridge`)
[diagram: hermes/claude-code → stdio → atd-mcp-bridge → UDS → atd-ref-server]

* Process model: one bridge subprocess per LLM session.
* Concurrency: ref-server runs multi-thread (4 workers default; ATD_WORKER_THREADS to override).
* SLO: p99 handshake < 200ms at 50 concurrent sessions on 4-core hardware.
* When to use: local LLM tooling, single-user dev environments, on-device PHR/HRA agents.

### 11.2 Cloud / multi-tenant (HTTP via `atd-server-http`)
[diagram: agent fleet → HTTPS → axum → atd-server-http → atd-runtime]

* Process model: one long-lived server process per host; clients are HTTP requests.
* Concurrency: adopter-controlled tokio runtime (celia_phr uses multi_thread, num_cpus).
* SLO: same handshake bar; throughput limited by tokio's HTTP keep-alive ceiling (axum default).
* When to use: hosted SaaS, fleet-of-agents on shared infra, when stdio fanout is wasteful.

### 11.3 Concurrency invariants (protocol level)
- `read_frame` / `write_frame` are deadline-bounded (§5.2).
- `AtdClient::connect` retries with jitter (§5.3).
- `AuditSink::on_call` is non-blocking on the dispatch path (§5.4).
- `atd-conformance` storm test enforces all of the above for any server claiming conformance.

### 11.4 The incident that motivated this section
[3-paragraph postmortem of the 2026-05-12 celia 10-concurrent benchmark]
```

The §11.4 postmortem is short but visible — protocol designers learn from their own bug reports, and adopters reading the doc see the proof that we take concurrency seriously.

## 6. Wire / API impact

**Wire format: zero change.** Same length-prefixed JSON frames, same `Request` / `Response` variants. New deadlines are local timer wrappers; peer behavior is unchanged.

**Public API additions:**
- `atd_protocol::wire::WireError` (typed enum) + `read_frame_with_deadline` + `write_frame_with_deadline`.
- `atd_sdk::ConnectOptions` + `AtdClient::connect_with_options`.
- `atd_runtime::default_worker_threads()` helper.
- `atd_runtime::metrics::{MetricsCounters, MetricsSnapshot}` + `ServerState.metrics` field + `Server::metrics_snapshot()`.
- `atd_runtime::audit::JsonLinesAuditSink::{new_with_capacity, drops}`.
- `SharedServerConfig.frame_deadline_active_ms` + `.frame_deadline_handshake_ms` fields.

**Public API breaking changes:** none. All additions are additive; existing call sites continue to compile.

**Adopter `cargo update` impact.** celia_phr and healthkit_cli pull `path =` deps; they recompile and inherit the new defaults. The defaults are strictly improvements (faster, more resilient); no adopter migration is required for §G1-G3. For §G4 (audit sink), adopters using `JsonLinesAuditSink::file(path)` continue to work; the only observable change is that audit writes are now async (one tokio task spawned). Adopters using custom `impl AuditSink` are untouched.

## 7. Migration / adopter notes

**healthkit_cli.** Pure consumer of `path = ../atd-mvp/...` deps; recompiles with no source edits. Their hermes-driven concurrency tests (currently single-client) will continue to pass; if they later run a multi-client suite, they get the new behavior for free.

**celia_phr.** Two impact axes:

1. **Their `atd-server-http` binary** already runs `multi_thread`; no change.
2. **Their benchmark harness (`scripts/agent-eval-hermes-family.ts`)** will see the §1.1 failure go away on next rebuild. Their CI gate (currently asserts <10% session-init failure at 10 concurrency) can be tightened to 0% after the bump. They should also bump `ATD_CONNECT_RETRIES=3` (the new default of 5 is conservative; their UDS path is reliable).

**Both adopters.** Their next ATD bump should be in tandem (one PR per adopter rebasing onto the new ATD tag) to make the post-SP storm metrics easy to attribute. We tag this SP as `sp-concurrency-baseline-v1` so they can pin.

**Bridge-side rollout.** `atd-mcp-bridge` is a binary; rebuilds via `cargo install --path crates/atd-mcp-bridge`. The bridge no longer needs to expect "Connection lost" as a transient; if it sees one, that's a real bug to file.

## 8. Open questions

**Q1: should `atd-server-http` get the same `default_worker_threads()` treatment?** Probably yes — adopters who build their own binary call `#[tokio::main(flavor = "multi_thread")]` and might not know about the 4-cap heuristic. But changing celia_phr's binary is outside this SP's blast radius. Defer to celia_phr's own decision; document `default_worker_threads()` as a stable helper they can opt into.

**Q2: do we need a `cargo bench` CI job?** A bench takes ~30s; 7 benches × CI matrix is ~5min. Acceptable on a per-PR basis. But criterion outputs are noisy on CI VMs (high variance). Solution: run benches with `--measurement-time 10 --warm-up-time 3` to settle, and require 3 consecutive PRs of >20% regression before failing the build. Settle in Phase G.

**Q3: should we expose `dispatch_p99_us` in §G7 even without histogram?** A streaming "online p99" via an HDR-lite ring buffer is ~300 LoC. Tempting to land here. Decision: defer to `SP-observability-v2`. This SP ships counters; histograms are next.

**Q4: do we need a `ConcurrencyConfig` struct?** §5.1 + §5.2 + §5.3 each have ~2 env vars; that's 6 env vars total. A single struct deserialized from a `[concurrency]` toml section would be cleaner. Decision: env-only for v1 because adopters drive ATD config via env today (no toml). Revisit when an operator config file emerges.

**Q5: should `AuditSink::on_call` move to `async fn`?** §G4 keeps it sync. Long-term, async lets adopters (rdb sinks, cloud loggers) skip the mpsc indirection. Decision: sync now; revisit in `SP-async-traits-v1` once `async fn in trait` is stable in our MSRV.

**Q6: bench against a real LLM-driven load, or synthetic?** Synthetic only in this SP — `atd-bench` exercises ATD itself, not LLM-driven flows. celia_phr's benchmark harness is the realistic integration test; we don't duplicate it.

## 9. Phasing

Detailed task lists live in the companion plan (`docs/superpowers/plans/2026-05-12-sp-concurrency-baseline.md`). High-level phases:

- **Phase A** (this spec): land. Tagged `sp-concurrency-baseline-spec`.
- **Phase B**: wire deadlines + `WireError` enum. `atd-protocol` change; backward-compat helpers retained. Tag: `sp-concurrency-baseline-phase-b`.
- **Phase C**: SDK connect retry. `atd-sdk` change. Tag: `sp-concurrency-baseline-phase-c`.
- **Phase D**: server runtime flip + `default_worker_threads()` helper. `atd-ref-server` + `atd-mock-weather-server` + `atd-runtime`. Tag: `sp-concurrency-baseline-phase-d`.
- **Phase E**: audit-sink rewrite. `atd-runtime`. Tag: `sp-concurrency-baseline-phase-e`.
- **Phase F**: metrics counters. `atd-runtime` + `atd-server`. Tag: `sp-concurrency-baseline-phase-f`.
- **Phase G**: bench crate. `crates/atd-bench` new. Tag: `sp-concurrency-baseline-phase-g`.
- **Phase H**: conformance storm scenario. `atd-conformance`. Tag: `sp-concurrency-baseline-phase-h`.
- **Phase I**: docs (architecture.md §11 + this SP archive). Tag: `sp-concurrency-baseline` (the umbrella tag).

Each phase is independently committable; the umbrella tag closes after celia_phr and healthkit_cli have both consumed Phase D-E and confirmed no regression. Expected effort: 3-5 working days for one developer.
