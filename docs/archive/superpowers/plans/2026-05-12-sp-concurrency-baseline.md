# SP-concurrency-baseline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make concurrency a protocol-level invariant in ATD. Fix the three root causes of the 2026-05-12 celia 10-concurrent benchmark failure (`current_thread` ref-server, unbounded `read_frame`, no SDK connect retry), rewrite `JsonLinesAuditSink` to a non-blocking mpsc + writer-task model, expose runtime metrics counters, ship a `crates/atd-bench` criterion suite with regression gates, and codify a `concurrent_handshake_storm` scenario in `atd-conformance`. After this SP, an adopter spawning 50 simultaneous bridges against `atd-ref-server` hits p99 handshake < 200ms with zero connection-lost errors on commodity 4-core hardware.

**Adopters:**
- **celia_phr** — primary validation adopter; their `scripts/agent-eval-hermes-family.ts` 10-concurrent benchmark goes from 60% session-init failure to 0%. SP-pagination-v1 (sibling) extends the result side; that's tracked separately.
- **healthkit_cli** — passive consumer; recompiles `path = ../atd-mvp` deps and inherits the new defaults.

**Architecture:** Five-axis intervention. (1) `atd-ref-server` + `atd-mock-weather-server` switch to `flavor = "multi_thread"` with worker count via new `atd_runtime::default_worker_threads()` helper. (2) `atd-protocol::wire` gains `read_frame_with_deadline` / `write_frame_with_deadline` + typed `WireError` enum. (3) `atd-sdk::AtdClient::connect` gains exponential-backoff retry with jitter via new `ConnectOptions`. (4) `atd-runtime::audit::JsonLinesAuditSink` rewrites to bounded mpsc + dedicated writer task, exposing `drops()` counter. (5) `atd-runtime::metrics` (new module) exposes `MetricsCounters` + `MetricsSnapshot`. Conformance + bench gates measure the result.

**Tech Stack:** Rust 2021 (workspace edition); existing tokio, serde, serde_json. New deps: `criterion` (bench crate, dev-dep only), `rand` (already transitively via `getrandom`), `dashmap` (metrics error-code map, runtime-only). No new transitive surface for adopters.

**Spec:** [`../specs/2026-05-12-sp-concurrency-baseline-design.md`](../specs/2026-05-12-sp-concurrency-baseline-design.md) — refer to spec §-numbers throughout this plan.

**Sequencing:** Wire deadlines (Phase B) first — it's a leaf-level primitive the SDK and server both consume. SDK retry (Phase C) and server runtime flip (Phase D) are independent and can land in either order; D is the *visible* fix in benchmarks. Audit-sink rewrite (Phase E) is a separate file and can land any time. Metrics (Phase F) depends on D+E. Bench (Phase G) and conformance (Phase H) gate everything else.

---

## Phase B — Wire-level deadlines + typed `WireError`

### Task 1: Add `WireError` enum and `read_frame_with_deadline` / `write_frame_with_deadline`

**Files:**
- Modify: `crates/atd-protocol/src/wire.rs` (add types + new helpers; keep existing helpers as thin wrappers)
- Modify: `crates/atd-protocol/src/lib.rs` (re-export `WireError`)

- [ ] **Step 1: Define `WireError`**

In `wire.rs`, add at the top (after `MAX_FRAME_BYTES`):

```rust
#[derive(thiserror::Error, Debug)]
pub enum WireError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("frame length overflow: {0} bytes (max {})", MAX_FRAME_BYTES)]
    LengthOverflow(u32),
    #[error("timeout after {0:?}")]
    Timeout(std::time::Duration),
}

impl From<WireError> for std::io::Error {
    fn from(e: WireError) -> Self {
        match e {
            WireError::Io(io) => io,
            other => std::io::Error::new(std::io::ErrorKind::InvalidData, other.to_string()),
        }
    }
}
```

The `From<WireError> for io::Error` reverse is a compat shim for `handle_connection` callers that currently use `std::io::Result`; they can keep their signature while migrating internals to `WireError`. (Tactical, not load-bearing — remove in a later cleanup pass.)

- [ ] **Step 2: Add `read_frame_with_deadline`**

```rust
pub async fn read_frame_with_deadline<R, T>(
    reader: &mut R,
    deadline: Option<std::time::Duration>,
) -> Result<T, WireError>
where
    R: tokio::io::AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let fut = async {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf);
        if len as usize > MAX_FRAME_BYTES {
            return Err(WireError::LengthOverflow(len));
        }
        let mut body = vec![0u8; len as usize];
        reader.read_exact(&mut body).await?;
        Ok::<T, WireError>(serde_json::from_slice(&body)?)
    };
    match deadline {
        None => fut.await,
        Some(d) => tokio::time::timeout(d, fut).await.map_err(|_| WireError::Timeout(d))?,
    }
}
```

- [ ] **Step 3: Add `write_frame_with_deadline`**

Mirror Step 2 — wrap `write_all(len) + write_all(body) + flush` in `tokio::time::timeout`. Same `WireError` return.

- [ ] **Step 4: Existing `read_frame` / `write_frame` become thin wrappers**

```rust
pub async fn read_frame<R, T>(reader: &mut R) -> std::io::Result<T>
where R: tokio::io::AsyncRead + Unpin, T: serde::de::DeserializeOwned,
{
    read_frame_with_deadline(reader, None).await.map_err(Into::into)
}
```

Same for `write_frame`. Existing call sites compile unchanged.

- [ ] **Step 5: Tests**

In `wire.rs::tests` add:
- `read_frame_with_deadline_returns_timeout_on_no_data` — pass a `tokio::io::empty()` reader with 50ms deadline; assert `WireError::Timeout`.
- `read_frame_with_deadline_succeeds_within_deadline` — write a real frame to an in-memory pipe; assert read completes.
- `write_frame_with_deadline_returns_timeout_on_blocked_writer` — use a `tokio::io::sink()` wrapped to be slow (use `tokio::io::DuplexStream` with tiny buffer + unread pending data); assert `WireError::Timeout`.
- `wire_error_into_io_error_preserves_kind` — assert `WireError::Timeout` → `io::Error` carries `InvalidData` kind with the timeout message.

- [ ] **Step 6: Commit**

```
feat(atd-protocol): add WireError + *_with_deadline helpers (SP-concurrency-baseline §5.2)
```

### Task 2: `SharedServerConfig` deadline fields + connection-side application

**Files:**
- Modify: `crates/atd-runtime/src/dispatch.rs` (add fields to `SharedServerConfig`)
- Modify: `crates/atd-server/src/connection.rs` (apply deadlines)

- [ ] **Step 1: Add config fields**

In `SharedServerConfig` (`dispatch.rs:57-85`):

```rust
pub frame_deadline_active_ms: u64,    // default 30_000
pub frame_deadline_handshake_ms: u64, // default 5_000
```

Update `for_test()` (`dispatch.rs:91-103`) to set both. Update all other construction sites discovered via `rg "SharedServerConfig\s*\{" crates/`.

- [ ] **Step 2: Apply deadlines in `handle_connection`**

In `crates/atd-server/src/connection.rs:23-39`, track Hello state:

```rust
let mut hello_seen = false;
loop {
    let deadline_ms = if hello_seen {
        state.config.frame_deadline_active_ms
    } else {
        state.config.frame_deadline_handshake_ms
    };
    let req: Request = match read_frame_with_deadline(&mut reader, Some(Duration::from_millis(deadline_ms))).await {
        Ok(r) => r,
        Err(WireError::Timeout(_)) => return Ok(()), // peer stalled; close cleanly
        Err(WireError::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if matches!(req, Request::Hello { .. }) {
        hello_seen = true;
    }
    let resp = dispatch(&state, &tracker, &mut caps, &mut caller_id, req).await;
    write_frame_with_deadline(&mut writer, &resp, Some(Duration::from_millis(deadline_ms))).await?;
}
```

- [ ] **Step 3: Integration test for handshake deadline**

In `crates/atd-server/tests/` add `handshake_deadline.rs`:
- Spawn server with `frame_deadline_handshake_ms: 200`.
- Connect a raw `UnixStream` and send nothing.
- Assert connection is closed by server within 500ms (read returns EOF / Closed).

- [ ] **Step 4: Commit**

```
feat(atd-runtime,atd-server): apply per-state frame deadlines (SP-concurrency-baseline §5.2)
```

---

## Phase C — SDK connect retry with backoff + jitter

### Task 3: Add `ConnectOptions` and `connect_with_options`

**Files:**
- Modify: `crates/atd-sdk/src/client.rs` (add `ConnectOptions`, `connect_with_options`, internal `connect_once`)
- Modify: `crates/atd-sdk/src/lib.rs` (re-export `ConnectOptions`)

- [ ] **Step 1: Define `ConnectOptions`**

```rust
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub max_attempts: u32,
    pub backoff_base_ms: u64,
    pub backoff_cap_ms: u64,
    pub connect_timeout_ms: u64,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            max_attempts: std::env::var("ATD_CONNECT_RETRIES").ok().and_then(|s| s.parse().ok()).unwrap_or(5),
            backoff_base_ms: std::env::var("ATD_CONNECT_BACKOFF_BASE_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(50),
            backoff_cap_ms: std::env::var("ATD_CONNECT_BACKOFF_CAP_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(800),
            connect_timeout_ms: std::env::var("ATD_CONNECT_TIMEOUT_MS").ok().and_then(|s| s.parse().ok()).unwrap_or(10_000),
        }
    }
}
```

- [ ] **Step 2: Split `connect` into `connect_once` + `connect_with_retry`**

Extract the current `connect` body (`UnixStream::connect` + `ping`) into `async fn connect_once(endpoint: &Endpoint) -> Result<Self, AtdError>`. Add `connect_with_options(endpoint, opts)` wrapping with the retry loop (spec §5.3 body). Modify `connect` to call `connect_with_options(endpoint, ConnectOptions::default())`.

`is_fatal_connect_error`:

```rust
fn is_fatal_connect_error(err: &AtdError) -> bool {
    if let AtdError::Io(io) = err {
        matches!(io.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied)
    } else { false }
}
```

- [ ] **Step 3: Add `tokio::time::timeout` around each `connect_once`**

Inside the retry loop, wrap `connect_once` in `tokio::time::timeout(Duration::from_millis(opts.connect_timeout_ms), connect_once(&endpoint))`. Timeout → treat as retryable error (loops back).

- [ ] **Step 4: Tests**

In `client.rs::tests`:
- `connect_retries_on_econnrefused` — point at `/tmp/no-such-socket`; intercept by injecting a deterministic "fail 3, then succeed" mock. Assert 4 attempts total.
- `connect_short_circuits_on_not_found` — pass invalid path; assert ONE attempt then error.
- `connect_respects_max_attempts` — fail all 5; assert exactly 5 attempts.
- `connect_options_from_env` — set `ATD_CONNECT_RETRIES=2`, call `ConnectOptions::default()`; assert `max_attempts == 2`. Use `temp-env` crate or wrap in serial-test.

- [ ] **Step 5: Commit**

```
feat(atd-sdk): retry connect with exp backoff + jitter (SP-concurrency-baseline §5.3)
```

---

## Phase D — Server runtime flip + `default_worker_threads()` helper

### Task 4: Add helper to `atd-runtime`

**Files:**
- Modify: `crates/atd-runtime/src/lib.rs` (add module + re-export)
- Create: `crates/atd-runtime/src/runtime.rs` (the helper)

- [ ] **Step 1: Implement helper**

```rust
//! Tokio runtime helpers.
//!
//! `default_worker_threads()` chooses a sensible default for ATD reference
//! binaries: `min(available_parallelism, 4)`, overridable via env.

pub fn default_worker_threads() -> usize {
    std::env::var("ATD_WORKER_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().min(4))
                .unwrap_or(2)
        })
}
```

- [ ] **Step 2: Unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test] #[serial]
    fn defaults_to_min_4() {
        std::env::remove_var("ATD_WORKER_THREADS");
        let n = default_worker_threads();
        assert!(n >= 1 && n <= 4);
    }

    #[test] #[serial]
    fn env_override_respected() {
        std::env::set_var("ATD_WORKER_THREADS", "8");
        assert_eq!(default_worker_threads(), 8);
        std::env::remove_var("ATD_WORKER_THREADS");
    }
}
```

- [ ] **Step 3: Commit**

```
feat(atd-runtime): default_worker_threads helper (SP-concurrency-baseline §5.1)
```

### Task 5: Flip `atd-ref-server` and `atd-mock-weather-server` to multi_thread

**Files:**
- Modify: `crates/atd-ref-server/src/main.rs:75` (flavor + worker_threads)
- Modify: `crates/atd-mock-weather-server/src/main.rs:32` (same)

- [ ] **Step 1: Update `atd-ref-server`**

```rust
#[tokio::main(flavor = "multi_thread", worker_threads = atd_runtime::default_worker_threads())]
async fn main() {
```

If `worker_threads` attribute doesn't accept a function call (depends on tokio macro version), fall back to a constant + a runtime startup log line confirming worker count.

- [ ] **Step 2: Update `atd-mock-weather-server`**

Same change.

- [ ] **Step 3: Verify startup log**

Both binaries should log `atd-{ref|mock-weather}-server: tokio multi_thread, N workers` at startup so operators can see the effective count.

- [ ] **Step 4: Commit**

```
feat(atd-ref-server,atd-mock-weather-server): multi_thread tokio runtime (SP-concurrency-baseline §5.1)
```

---

## Phase E — `JsonLinesAuditSink` mpsc rewrite

### Task 6: Rewrite `JsonLinesAuditSink` with bounded channel + writer task

**Files:**
- Modify: `crates/atd-runtime/src/audit.rs` (rewrite `JsonLinesAuditSink`)

- [ ] **Step 1: Replace struct definition**

Replace the current `Mutex<Box<dyn Write + Send>>` field with the mpsc + drops counter (spec §5.4 body). Constructor `new_with_writer(writer)` spawns the dedicated drain task. Add `new_with_capacity(writer, n)`.

- [ ] **Step 2: Update `on_call`**

Switch from `mut w = lock; write_all` to `self.tx.try_send(event.clone())`; on `Err`, bump `drops`.

- [ ] **Step 3: Add `drops()` accessor**

```rust
impl JsonLinesAuditSink {
    pub fn drops(&self) -> u64 { self.drops.load(std::sync::atomic::Ordering::Relaxed) }
}
```

- [ ] **Step 4: Existing public helpers (`stdout`, `stderr`, `file`) compile unchanged**

They were calling `new(writer)`. Rename the constructor to `new_with_writer` and keep `new` as `pub fn new(writer: Box<dyn Write + Send + 'static>) -> Self { Self::new_with_writer(writer) }` (one-line shim, deprecated-tagged for adopter migration in v0.4.x).

- [ ] **Step 5: Tests**

In `audit.rs::tests`:
- `on_call_is_non_blocking_under_burst` — spawn 100 tokio tasks each calling `on_call` simultaneously; assert all return within 10ms total wall.
- `drops_counter_increments_when_channel_full` — use `new_with_capacity(writer, 2)`, push 100 events fast, assert `drops()` > 90.
- `events_eventually_drain_to_writer` — using `SharedBuf` (existing test fixture), push 10 events, await 100ms, assert the buffer contains 10 newline-separated JSON objects.
- `dropping_sink_flushes_pending` — push 5 events, drop the sink, assert the writer received exactly 5 lines (clean shutdown via channel close).

- [ ] **Step 6: Commit**

```
refactor(atd-runtime): JsonLinesAuditSink uses bounded mpsc + writer task (SP-concurrency-baseline §5.4)
```

---

## Phase F — Metrics counters

### Task 7: `atd_runtime::metrics` module + `Server::metrics_snapshot()`

**Files:**
- Create: `crates/atd-runtime/src/metrics.rs` (`MetricsCounters`, `MetricsSnapshot`)
- Modify: `crates/atd-runtime/src/lib.rs` (export)
- Modify: `crates/atd-runtime/src/dispatch.rs` (`ServerState` field + bump on dispatch)
- Modify: `crates/atd-server/src/server.rs` (accept loop bumps `accepted_connections`)

- [ ] **Step 1: Define `MetricsCounters` + `MetricsSnapshot`**

Spec §5.7 body. Add `dashmap = "5"` to `atd-runtime/Cargo.toml`.

- [ ] **Step 2: Wire into `ServerState`**

Add `pub metrics: Arc<MetricsCounters>` field to `ServerState`. Default-construct on each test state. Bump `dispatched_requests` at the top of `dispatch_request`. Bump `dispatch_errors_by_code` in the error arms.

- [ ] **Step 3: Wire into accept loop**

In `crates/atd-server/src/server.rs:134`, after `let (stream, _) = listener.accept().await?;`, add `self.state.metrics.accepted_connections.fetch_add(1, Ordering::Relaxed);`.

- [ ] **Step 4: `Server::metrics_snapshot()`**

Add public method on `Server` that captures and returns `MetricsSnapshot` (a `Clone` of the current counter values + `uptime_seconds = (now - startup_instant).as_secs()`).

- [ ] **Step 5: Tests**

- `metrics_accepted_connections_increments_per_accept` — 10 connects, assert counter == 10.
- `metrics_dispatched_requests_increments_per_request` — connect + 5 pings, assert counter == 5 (Hello/Ping/etc. all count).
- `metrics_dispatch_errors_by_code_lazily_initializes` — dispatch tool-not-found 3 times, snapshot, assert `dispatch_errors_by_code[404 or whatever code]` == 3.
- `metrics_audit_drops_total_reflects_sink_drops` — install a tiny-capacity audit sink, fire 100 dispatches, snapshot, assert `audit_drops_total > 0`.

- [ ] **Step 6: Commit**

```
feat(atd-runtime,atd-server): metrics counters + Server::metrics_snapshot (SP-concurrency-baseline §5.7)
```

---

## Phase G — `crates/atd-bench` criterion suite

### Task 8: New bench crate scaffold

**Files:**
- Create: `crates/atd-bench/Cargo.toml`
- Create: `crates/atd-bench/README.md`
- Create: `crates/atd-bench/benches/ping_rtt.rs`
- Create: `crates/atd-bench/benches/handshake.rs`
- Create: `crates/atd-bench/benches/tool_list.rs`
- Create: `crates/atd-bench/benches/tool_schema.rs`
- Create: `crates/atd-bench/benches/run_tool_echo.rs`
- Create: `crates/atd-bench/benches/concurrent_dispatch.rs`
- Create: `crates/atd-bench/benches/audit_burst.rs`
- Create: `crates/atd-bench/baselines/README.md`
- Modify: workspace `Cargo.toml` (add to `[workspace].members`)

- [ ] **Step 1: Crate scaffold**

`crates/atd-bench/Cargo.toml`:

```toml
[package]
name = "atd-bench"
version = "0.0.1"
edition = "2021"
publish = false

[dev-dependencies]
atd-protocol = { path = "../atd-protocol" }
atd-sdk = { path = "../atd-sdk" }
atd-runtime = { path = "../atd-runtime" }
atd-server = { path = "../atd-server" }
atd-tools-echo = { path = "../atd-tools-echo" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
criterion = { version = "0.5", features = ["async_tokio"] }
tempfile = "3"
serde_json = "1"

[[bench]]
name = "ping_rtt"
harness = false

[[bench]]
name = "handshake"
harness = false

# ... one [[bench]] block per bench file
```

- [ ] **Step 2: Bench harness common setup**

Each bench file starts with a shared setup helper (extract to a `benches/common.rs` if it grows): spin up `atd-ref-server`-equivalent in-process (use `Server::new(...).run()` on a tempdir socket), register echo tool, create an `AtdClient` against it.

- [ ] **Step 3: Implement each bench**

For each of the 7 SLOs in spec §4:
- `ping_rtt.rs`: 10k iterations of `client.ping()`.
- `handshake.rs`: per-iter: create fresh client + `hello + discover`.
- `tool_list.rs`: register 19 echo-stub tools, measure `discover`.
- `tool_schema.rs`: measure `describe("ref:echo.say")`.
- `run_tool_echo.rs`: measure `call("ref:echo.say", {"x":1})`.
- `concurrent_dispatch.rs`: fan-out 10 clients × 1 call; measure wall.
- `audit_burst.rs`: install audit sink, fire 10k calls, measure write-stall.

- [ ] **Step 4: Baselines**

Run `cargo bench` on a clean tree, commit `target/criterion/*/base/estimates.json` → `crates/atd-bench/baselines/<bench>.json` (one per bench). `crates/atd-bench/baselines/README.md` documents update protocol: "Re-bench on a quiet machine; commit baselines with a separate PR titled `perf: refresh atd-bench baselines on YYYY-MM-DD`."

- [ ] **Step 5: Pre-commit gate (optional in v1)**

`scripts/bench-gate.sh` runs `cargo bench`, parses `target/criterion`, compares to `baselines/`, fails if any bench's median >20% above baseline. Not auto-installed; documented as `cargo bench && ./scripts/bench-gate.sh` for adopters who want it.

- [ ] **Step 6: Commit**

```
feat(atd-bench): criterion bench suite for ATD perf SLOs (SP-concurrency-baseline §5.6)
```

---

## Phase H — Conformance `concurrent_handshake_storm` scenario

### Task 9: New conformance scenario

**Files:**
- Create: `crates/atd-conformance/src/scenarios/concurrent_handshake_storm.rs`
- Modify: `crates/atd-conformance/src/scenarios/mod.rs` (register)
- Modify: `crates/atd-conformance/README.md` (document SLOs)

- [ ] **Step 1: Scenario impl**

Follow spec §5.5 pseudocode. Fixture: 19 echo-stub tools (mirror celia's registry size). Output: structured `ConformanceReport { scenario_id, durations_ms: Vec<u64>, p50, p99, errors: u32, audit_drops: u64 }`.

- [ ] **Step 2: Assertions**

```rust
assert_eq!(report.errors, 0);
assert!(report.p99_ms < 200, "p99 {}ms exceeds SLO 200ms", report.p99_ms);
assert_eq!(report.audit_drops, 0);
```

- [ ] **Step 3: Env override for CI runners**

`ATD_CONFORMANCE_STORM_N` lets CI dial down from 50 to 25 on 2-vCPU GitHub runners. Document in README.md.

- [ ] **Step 4: Tests for the scenario itself**

Yes, a meta-test: assert the scenario passes against the ref-server (after Phase D lands). Run it in `crates/atd-conformance/tests/storm.rs` so `cargo test --workspace` exercises it.

- [ ] **Step 5: Commit**

```
test(atd-conformance): concurrent_handshake_storm scenario (SP-concurrency-baseline §5.5)
```

---

## Phase I — Docs + tag

### Task 10: `docs/architecture.md` §11 + adopter notification

**Files:**
- Modify: `docs/architecture.md` (add §11 with subsections per spec §5.8)
- Modify: `CLAUDE.md` (note new bench gate + storm conformance)
- Create: `docs/adr/0002-concurrency-baseline.md` (one-page summary for adopters)

- [ ] **Step 1: Write §11**

Use spec §5.8's outline verbatim. The `[diagram]` placeholders should land as ASCII boxes-and-arrows (not external images) so they render in plain markdown.

- [ ] **Step 2: ADR-0002**

One-pager: "Why concurrency is a baseline ATD invariant (was: a tuning suggestion). Adopters: rebuild `path = atd-mvp` deps; no source edits needed. Verify with `cargo nextest run --test storm`."

- [ ] **Step 3: Update `CLAUDE.md`**

Add to "Standard commands" a `cargo bench` line. Add to "Workspace test discipline" a note: "the storm conformance test spawns 50 concurrent clients against a real bind-listener; expect 10-15s wall on local. Cap parallelism if running alongside other test workloads."

- [ ] **Step 4: Notify adopters**

Open issues in healthkit_cli and celia_phr repos (manually, not by this plan — leave a TODO comment for the maintainer in the commit message): "ATD ships SP-concurrency-baseline; rebuild and confirm no regression. If you previously tuned `ATD_CONNECT_RETRIES`, the new default is 5; you may be able to drop the env var."

- [ ] **Step 5: Final tag**

After all phases green:

```bash
git tag sp-concurrency-baseline
git push origin sp-concurrency-baseline
```

Update `CLAUDE.md`'s "Recent SPs shipped" list with `sp-concurrency-baseline`.

- [ ] **Step 6: Commit + tag**

```
docs(architecture): SP-concurrency-baseline shipped — §11 deployment shapes
```

---

## Final acceptance criteria (echoes spec §G1-G8 and §4 SLOs)

- [ ] `cargo nextest run --workspace` passes (no regression in 487+ existing tests).
- [ ] `cargo nextest run --test storm -p atd-conformance` passes with p99 < 200ms, 0 errors, 0 drops.
- [ ] `cargo bench -p atd-bench` runs all 7 benches; output committed to `crates/atd-bench/baselines/` as the new baseline.
- [ ] celia_phr's 10-concurrent benchmark (`scripts/agent-eval-hermes-family.ts --queries 10 --concurrency 10`) reports 0 connection-lost errors and 100% tool-schema-loaded sessions on a rebuilt-against-this-tag tree.
- [ ] `docs/architecture.md` §11 is published with two deployment-shape diagrams + the SLO table + postmortem of the celia incident.
- [ ] `CLAUDE.md` "Recent SPs shipped" lists `sp-concurrency-baseline`.
- [ ] git tag `sp-concurrency-baseline` exists and is pushed.

**Expected wall-clock effort:** 3-5 working days for one developer, of which ~1 day is Phase G (bench harness boilerplate) and ~1 day is Phase H (the storm scenario stability tuning on CI).
