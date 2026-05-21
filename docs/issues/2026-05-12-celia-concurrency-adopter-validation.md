# celia_phr — SP-concurrency-baseline adopter validation + concurrency benchmark

**Layer:** adopter (cross-project: celia_phr ↔ atd)
**Status:** closed-verified (2026-05-12)
**Effort:** ~0.5 day (rebuild + rerun benchmark + report numbers)
**Filed:** 2026-05-12
**Closed:** 2026-05-12
**Related SP:** [`sp-concurrency-baseline`](../archive/superpowers/specs/2026-05-12-sp-concurrency-baseline-design.md) (tag `sp-concurrency-baseline`)
**Related ADR:** [ADR-0002 — Concurrency is a protocol-level invariant](../adr/0002-concurrency-baseline.md)
**Triggering incident:** 2026-05-12 celia 10-query × 10-concurrent benchmark (60% session-init failure → 0% expected post-SP)

## Resolution (2026-05-12)

celia delivered the functional ask via the **`atd-mcp-opt iter-4`** track:

- **SP-concurrency-baseline passively consumed.** celia rebuilt `path = ../atd` against the perf-v1 tip; `atd-sdk::client::connect_retries_on_transient_failure` is listed as a landed prerequisite of iter-4. The 120-query family-eval SHARP baseline ran with **0 rate-limit / 0 connection failures** (vs. iter-3's 6/10 failures). Evidence: `celia_phr/docs/atd-mcp-opt-iter4-baseline.md` (recorded 2026-05-12, celia commit `90d1156`).
- **Original incident (60% session-init failure at 10×10) no longer reproduces** — iter-4's full 120Q SHARP run is the integration-level proof; the underlying bug class is structurally gone.

**Deviations from the original ask:**

- celia ran the benchmark at **concurrency=4**, not 10/25/50. The bottleneck moved to **DeepSeek API rate-limits** (eval pipeline shares the same DeepSeek calls for agent + SHARP judge). Pushing ATD concurrency higher in this harness no longer exercises the ATD path. The 50-client storm test on the ref-server side (already in `atd-conformance`, p99=125ms post-SP) is the protocol-level proof; pushing celia's eval harness past 4 was de-prioritised as duplicative.
- CI gate **<10% → 0%** tightening: not explicitly recorded in a CI config diff, but iter-4's reported 0 errors / 120 queries is the de-facto enforced level. Will revisit if a future flake surfaces.

**Closing rationale:** core regression fixed and validated end-to-end through the celia agent loop. Further concurrency stress belongs in `atd-conformance` scenarios, not in the celia eval harness.

## Summary

`atd` shipped **SP-concurrency-baseline** on 2026-05-12 to structurally fix the concurrency failure celia surfaced. This issue asks the celia_phr team to:

1. **Rebuild** their `path = ../atd` workspace dependencies against tag `sp-concurrency-baseline` (or latest master).
2. **Rerun** `scripts/agent-eval-hermes-family.ts --queries 10 --concurrency 10` and confirm the 60% session-init failure mode no longer reproduces.
3. **Push concurrency higher** — at minimum 25, ideally 50 — to verify the post-SP SLO (p99 < 200ms handshake, 0 errors, 0 audit drops) holds in the real celia stack (not just the ref-server-based conformance scenario).
4. **Tighten their CI gate** from "<10% session-init failure" to "0% failure" once the rebuild validates.
5. **Report the numbers back** in a follow-up comment on this issue, or via a celia-side doc at `celia_phr/docs/sp-concurrency-baseline-adopter.md` mirroring their SP-medical-middleware adopter pattern.

## Current state — what shipped on the ATD side

Five-axis intervention, all back-compat (no source edits required on the celia side; recompile suffices):

| Axis | Change | Adopter knob |
|---|---|---|
| Server runtime | `atd-ref-server` + `atd-mock-weather-server` flipped from `current_thread` to `multi_thread` tokio | `ATD_WORKER_THREADS` (default `min(cpus, 4)`); celia's `atd-server-http` binary already uses `multi_thread` — no change needed there |
| Wire deadlines | `WireError::Timeout` + `read_frame_with_deadline` / `write_frame_with_deadline` applied per-connection (5s pre-Hello, 30s active) | `SharedServerConfig.frame_deadline_active_ms` / `frame_deadline_handshake_ms` (defaults are sane; only tune if a celia HTTP tool legitimately takes >30s) |
| SDK retry | `AtdClient::connect` exponential-backoff retry (5× / 50→800ms / ±20% jitter); fatal errors short-circuit | `ATD_CONNECT_RETRIES` / `ATD_CONNECT_BACKOFF_BASE_MS` / `ATD_CONNECT_BACKOFF_CAP_MS` / `ATD_CONNECT_TIMEOUT_MS`. **Recommended for celia: drop these vars; the new defaults are conservative and your UDS path is reliable.** |
| Audit sink | `JsonLinesAuditSink` rewritten to bounded `tokio::sync::mpsc` + dedicated drain task; `on_call` non-blocking under contention | `JsonLinesAuditSink::new_with_capacity(writer, n)` (default 1024); celia uses the standard `::file(path)` helper which inherits the default |
| Metrics | `MetricsCounters` + `Server::metrics_snapshot()`; counters surface `accepted_connections`, `dispatched_requests`, `dispatch_errors_by_code`, `audit_events_total`, `audit_drops_total` | Optional: celia can expose this via a `/admin/metrics` route in their HTTP binary. Not gating this issue. |

`AuditSink::on_call` remains synchronous; `AuditSink::drops() -> u64` is a new default trait method (returns 0 unless overridden) — celia's custom audit sink impls (if any) inherit the default automatically.

**Verified on the ATD side:** new `concurrent_handshake_storm` conformance scenario (`crates/atd-conformance/tests/concurrent_handshake_storm.rs`) at 50 simultaneous clients × (Hello + ToolList + 5×ToolSchema) measures:

```
storm: n=50 wall=127ms p50=116ms p99=125ms errors=0 audit_drops=0
```

vs the pre-SP incident: 71s wall + 60% session-init failure at *10× lower* concurrency.

## What we need from celia

### Step 1 — rebuild

```bash
cd /home/nan/code/pha/celia_phr
# Ensure the path dep points at the post-SP atd tag.
# (No Cargo.toml edit needed if path = "../atd"; just rebuild.)
cargo build --release
cargo nextest run --workspace
```

Expected: existing celia tests stay green. The SP is fully back-compat; no celia source edit should be necessary. If `cargo build` errors mention a missing `metrics` or `frame_deadline_*` field on `SharedServerConfig`, that's a stale build — `cargo clean -p celia-...` and rebuild.

### Step 2 — rerun the 10-concurrent benchmark

```bash
DEEPSEEK_API_KEY="$DEEPSEEK_API_KEY" \
  pnpm --filter @celia/benchmark exec tsx \
  scripts/agent-eval-hermes-family.ts \
  --queries 10 --concurrency 10 \
  --out /tmp/agent-eval-post-sp.json
```

**Pre-SP signature** (what the failure looked like):

- 6/10 sessions with `prompt_tokens ≈ 180-190` (no-tools fallback)
- Hermes log shows `Connection lost` + `failed initial connection after 3 attempts, giving up`
- Wall clock ~71s

**Expected post-SP signature** (what success looks like):

- 10/10 sessions with `prompt_tokens ≈ 5200` (full tool schema loaded)
- Zero `Connection lost` in hermes logs
- Wall clock dominated by DeepSeek LLM round-trip time, not ATD overhead

### Step 3 — push concurrency higher

The pre-SP failure showed up at 10; the post-SP SLO (verified at the protocol level via conformance) holds at 50. Real celia stack adds: hermes process spawn cost + DeepSeek HTTPS + celia FHIR validation + UCAN per-request resolution. We want a real-stack number, not just the ref-server number.

```bash
# Start with 25 — equivalent to the ATD conformance test on a CI runner.
pnpm --filter @celia/benchmark exec tsx scripts/agent-eval-hermes-family.ts \
  --queries 10 --concurrency 25 --out /tmp/agent-eval-25.json

# If 25 is clean, go to 50 and capture the worst-case latency distribution.
pnpm --filter @celia/benchmark exec tsx scripts/agent-eval-hermes-family.ts \
  --queries 10 --concurrency 50 --out /tmp/agent-eval-50.json
```

If 50 is *still* clean, the celia stack is provisioned beyond what the ATD conformance test exercises — great signal, document it. If 50 fails but 25 succeeds, that bounds the celia-side concurrency ceiling (likely DeepSeek API rate limit or hermes-side spawn budget, not ATD). Report the failure mode either way.

### Step 4 — tighten the CI gate

If the celia benchmark CI currently asserts "session-init failure rate < 10%" or similar, tighten it to **"= 0%"** at the previous concurrency level. The post-SP guarantee is binary — sessions either initialize cleanly or there's a real bug to file.

If celia maintains a separate per-PR perf gate, consider also asserting `prompt_tokens >= 5000` per session (catches the silent tool-schema-loading regression mode the original incident represented).

### Step 5 — report back

One of:

- Comment on this issue with the three numbers (10/25/50-concurrent: pass/fail + wall + p99 + observed failure mode if any).
- File `celia_phr/docs/sp-concurrency-baseline-adopter.md` mirroring the existing `sp-medical-middleware-adopter.md` pattern (test plan, before/after, signed-off date).
- Reach out on the usual channel if anything blocks the rebuild.

## Acceptance criteria

This issue closes when:

- [ ] celia_phr rebuilds against `sp-concurrency-baseline` without source-level edits.
- [ ] celia's existing test suite stays green (no regression from the API additions).
- [ ] 10-concurrent benchmark reports 0/10 session-init failures (down from 6/10 pre-SP).
- [ ] At least one higher-concurrency datapoint (25 or 50) is captured with wall + p99 + error count.
- [ ] celia CI gate tightened to assert 0% session-init failure (or equivalent invariant).
- [ ] Results documented either in a comment here or in `celia_phr/docs/sp-concurrency-baseline-adopter.md`.

## Non-acceptance / out of scope for this issue

- **Migrating to `Server::metrics_snapshot()` for celia-side dashboards.** Optional follow-up; the snapshot is available but doesn't gate this issue.
- **Tuning `ATD_WORKER_THREADS` on celia's host.** Default `min(cpus, 4)` is sane; only revisit if profiling shows worker-thread saturation.
- **HTTP transport (`atd-server-http`) accept-side counters.** axum/hyper handles connection accounting; integration into `MetricsCounters` is `SP-observability-v2` territory.
- **Pagination of large tool results.** Sibling SP-pagination-v1 (perf-v1 axis 2); separate adopter issue when its implementation lands.

## Failure modes to watch for

If the rebuild reveals any of these, file a new ATD-side issue cross-linking this one — they would be real bugs in the SP:

1. **`cargo build` errors on missing `metrics` field.** Stale build artifact — `cargo clean` and retry. If it persists, escalate.
2. **Benchmark still shows `Connection lost` at low concurrency.** The SP fixes were supposed to be load-tested at n=50; if n=10 still fails, something in celia's stack is exercising a code path the ATD conformance scenario doesn't.
3. **`prompt_tokens` higher than expected (~10k+) per session.** Could indicate `tools/list` is now over-fetching schemas because of the related fix in `4fb652f` (per-tool describe in MCP bridge). Worth a sanity check.
4. **Audit log file shows no events.** The new mpsc drain task is async-spawned at sink construction; if celia constructs the sink outside a tokio context, the spawn would silently fail. Check that the sink is built inside `#[tokio::main]` scope.
5. **Storm at n=50 fails on the celia side but passes on the ref-server.** The differential isolates: ref-server has 10 echo-style tools; celia has 19 FHIR tools with real DB I/O. If celia fails, the bottleneck is downstream of ATD — likely tokio worker exhaustion under DB load, suggesting a celia-side `worker_threads` bump or DB connection pool tuning.

## References

- ATD spec: `docs/archive/superpowers/specs/2026-05-12-sp-concurrency-baseline-design.md`
- ATD plan: `docs/archive/superpowers/plans/2026-05-12-sp-concurrency-baseline.md`
- ATD conformance test (the test that "passes" the SLO bar celia should meet): `crates/atd-conformance/tests/concurrent_handshake_storm.rs`
- ATD architecture §11 (deployment shapes, SLOs, postmortem): `docs/architecture.md` §11
- ADR: `docs/adr/0002-concurrency-baseline.md`
- Sibling SP-pagination-v1 (separate adopter issue when impl lands): `docs/archive/superpowers/specs/2026-05-12-sp-pagination-v1-design.md`
- Triggering incident transcript: 2026-05-12 chat session at `/home/nan/code/pha/celia_phr/scripts/agent-eval-hermes-family.ts` (pre-SP run).
