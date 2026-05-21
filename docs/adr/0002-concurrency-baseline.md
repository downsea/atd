# ADR 0002 — Concurrency is a protocol-level invariant

- **Status:** Accepted
- **Date:** 2026-05-12
- **Deciders:** `atd` maintainers
- **Related:** [`docs/architecture.md`](../architecture.md) §10 + §11 · [`docs/archive/superpowers/specs/2026-05-12-sp-concurrency-baseline-design.md`](../archive/superpowers/specs/2026-05-12-sp-concurrency-baseline-design.md) · sibling [`SP-pagination-v1`](../archive/superpowers/specs/2026-05-12-sp-pagination-v1-design.md)

## 1. Context

On 2026-05-12 the `celia_phr` adopter ran a 10-query × 10-concurrent benchmark against the ATD reference stack (DeepSeek V4 Pro ↔ Hermes Agent ↔ MCP stdio ↔ `atd-mcp-bridge` ↔ `atd-ref-server`). Six of ten sessions failed to load any tools (`prompt_tokens` collapsed from ~5200 to ~180-190). Hermes's log:

```
Failed to connect to MCP server 'celia' (atd-mcp-bridge): Connection lost
MCP server 'celia' failed initial connection after 3 attempts, giving up
```

Investigation found three root causes in three lines of code, plus a hidden fourth cliff:

1. `crates/atd-ref-server/src/main.rs:75` — `#[tokio::main(flavor = "current_thread")]` serialized all per-connection tasks through one OS thread.
2. `crates/atd-protocol/src/wire.rs:25` — `read_frame` was unbounded; a stalled handshake held its worker indefinitely.
3. `crates/atd-sdk/src/client.rs:33` — `AtdClient::connect` had no retry; transient EAGAIN propagated to the MCP transport which exhausted its 3-attempt budget against the same starved server.
4. `crates/atd-runtime/src/audit.rs` — `JsonLinesAuditSink` held a `std::sync::Mutex<Writer>` and did synchronous file I/O on every dispatch; would stall the reactor at ~50 dispatches/sec once the first three were fixed.

For a reference implementation of a *tool-dispatch protocol* that ships an MCP bridge as one of its three blessed adoption modes, a 60% failure rate at 10 concurrency is a protocol-level credibility hit — not a tuning issue.

## 2. Decision

**Concurrency is now a protocol-level invariant**, not a deployment tuning suggestion. The five-axis intervention:

1. **Multi-thread reference binaries.** `atd-ref-server` and `atd-mock-weather-server` switch to `multi_thread` tokio via `atd_runtime::default_worker_threads()` (default `min(cpus, 4)`, env-overridable `ATD_WORKER_THREADS`).
2. **Wire-level deadlines.** `WireError::Timeout` typed variant + `read_frame_with_deadline` / `write_frame_with_deadline`. Per-connection state machine applies tighter (5s) deadline before Hello, looser (30s) after.
3. **SDK connect retry.** `AtdClient::connect` retries 5× with 50→800ms exponential backoff + ±20% jitter. Fatal errors (`NotFound`, `PermissionDenied`) short-circuit. Env-tunable.
4. **Non-blocking audit.** `JsonLinesAuditSink` rewritten to bounded `tokio::sync::mpsc` + dedicated drain task. `on_call` is `try_send`; channel full → drop + bump counter. `log loss >> dispatch stall` invariant preserved.
5. **Observability counters.** `atd_runtime::MetricsCounters` + `Server::metrics_snapshot()` surface `accepted_connections`, `dispatched_requests`, `dispatch_errors_by_code`, `audit_events_total`, `audit_drops_total`.

Enforcement: `atd-conformance::concurrent_handshake_storm` runs 50 simultaneous clients × 7 RPCs each on every `cargo nextest run --workspace` invocation. Any future ATD server impl claiming conformance must pass.

## 3. Consequences

**Verified on 4-core dev hardware (2026-05-12):**

```
storm: n=50 wall=127ms p50=116ms p99=125ms errors=0 audit_drops=0
```

vs the pre-SP incident: 71s wall, 60% session-init failure at *10×* lower concurrency. The SP-defined SLOs hold with significant headroom.

**Adopter impact:**

- `celia_phr` — rebuild `path = atd` deps; their `scripts/agent-eval-hermes-family.ts` 10-concurrent benchmark goes from 60% session-init failure to 0%. Their CI gate can tighten from "<10% failure" to "0% failure." Their `atd-server-http` binary already uses `multi_thread`; the §5.4 audit mpsc inherits via the runtime upgrade.
- `healthkit_cli` — passive consumer; recompiles and inherits the new defaults. Their hermes-driven test suite is currently single-client so the storm fix is transparent.

**Public API additions** (all back-compat):

- `atd_protocol::{WireError, read_frame_with_deadline, write_frame_with_deadline}`
- `atd_sdk::{ConnectOptions, AtdClient::connect_with_options}`
- `atd_runtime::{default_worker_threads, MetricsCounters, MetricsSnapshot}`
- `atd_runtime::audit::JsonLinesAuditSink::{new_with_capacity, drops}`
- `atd_runtime::AuditSink::drops()` default trait method
- `atd_server::Server::{metrics_snapshot, set_frame_deadlines}`
- `SharedServerConfig.frame_deadline_active_ms` / `.frame_deadline_handshake_ms`

**Public API breaking changes:** none.

**Deferred / tracked as follow-up:**

- `crates/atd-bench` criterion regression-gate suite (SP §5.6 + §G7): not load-bearing for adopter unblocking; tracked for the next perf-touching SP author.
- `dispatch_p50_us` / `dispatch_p99_us` latency histograms — `SP-observability-v2` territory; this SP ships counters only.
- HTTP transport listener accept-side counters — the HTTP path goes through axum/hyper which has its own connection accounting; integration into `MetricsCounters` is a `SP-observability-v2` follow-up.

## 4. Alternatives considered

- **Document the workaround instead of fixing it.** ("Set `ATD_WORKER_THREADS` before launching ref-server.") Rejected: adopters discovering production failures via tuning workarounds is the failure mode this ADR exists to prevent.
- **Daemonize `atd-mcp-bridge` so one bridge multiplexes N hermes sessions.** Rejected as scope creep: MCP-over-stdio is one process per session by spec; the §5.1 multi-thread fix solved the concurrency problem at the right layer without proposing a new bridge architecture.
- **Replace the bounded-mpsc audit pattern with `crossbeam` lock-free ring buffer.** Deferred: mpsc is sufficient at the measured SLO; lock-free would add dep weight without measured benefit.

## 5. References

- Spec: `docs/archive/superpowers/specs/2026-05-12-sp-concurrency-baseline-design.md`
- Plan: `docs/archive/superpowers/plans/2026-05-12-sp-concurrency-baseline.md`
- Conformance test: `crates/atd-conformance/tests/concurrent_handshake_storm.rs`
- Architecture deployment-shapes section: `docs/architecture.md` §11
- Sibling SP for the result-pagination axis of the same perf-v1 iteration: `docs/archive/superpowers/specs/2026-05-12-sp-pagination-v1-design.md`
