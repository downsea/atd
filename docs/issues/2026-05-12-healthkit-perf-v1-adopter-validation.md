# healthkit_cli — perf-v1 adopter validation (concurrency + pagination)

**Layer:** adopter (cross-project: healthkit_cli ↔ atd-mvp)
**Status:** ready-for-healthkit
**Effort:** ~1 day total (15 min concurrency rebuild + ~6h pagination migration of 1-3 tools)
**Filed:** 2026-05-12
**Related SPs:** [`sp-concurrency-baseline`](../superpowers/specs/2026-05-12-sp-concurrency-baseline-design.md) (perf-v1 axis 1) · [`sp-pagination-v1`](../superpowers/specs/2026-05-12-sp-pagination-v1-design.md) (perf-v1 axis 2)
**Related ADRs:** [ADR-0002 — concurrency baseline](../adr/0002-concurrency-baseline.md) · [ADR-0003 — pagination v1](../adr/0003-pagination-v1.md)
**Sibling adopter issue:** [`2026-05-12-celia-concurrency-adopter-validation.md`](2026-05-12-celia-concurrency-adopter-validation.md)

## Summary

`atd-mvp` shipped the **perf-v1 iteration** on 2026-05-12, two SPs covering two axes:

- **SP-concurrency-baseline** (tag `sp-concurrency-baseline`) — multi-thread tokio + wire deadlines + SDK retry + audit mpsc + metrics. Fixes a 60% session-init failure mode that surfaced in celia's 10-concurrent benchmark. healthkit_cli is a passive consumer here (no source edits required).
- **SP-pagination-v1** (tag `sp-pagination-v1`) — protocol-level result pagination via HMAC-signed cursors. healthkit_cli is the **primary migration target** for this SP: `query_observations` and `query_workouts` over multi-month windows produce ~3MB+ JSON payloads that already exceed the 1MB advisory budget today.

This issue asks the healthkit_cli team to:

1. **Rebuild** their `path = ../atd-mvp/crates/...` workspace deps. Concurrency fixes land transparently; pagination types become available.
2. **Confirm no regression** on the existing single-client integration tests.
3. **Migrate 1-3 high-volume tools** to opt into `Tool::call_paginated` (the load-bearing adopter work for SP-pagination-v1).
4. **Document the migration** in `healthkit_cli/docs/` mirroring the `case-study-v1.4.0` pattern.

## Part 1 — SP-concurrency-baseline (passive consumer, ~15 min)

### What shipped on the ATD side

Five-axis intervention, all back-compat (no source edits required on the healthkit side; recompile suffices):

| Axis | Change | Adopter impact |
|---|---|---|
| Server runtime | `atd-ref-server` flipped from `current_thread` to `multi_thread` tokio | healthkit_cli's `atd-server` adopter binding ALREADY ran multi_thread (`#[tokio::main(flavor = "multi_thread", ...)]` per usual sidecar pattern); the new `atd_runtime::default_worker_threads()` helper is available if you want to defer worker-count config to env (`ATD_WORKER_THREADS`) |
| Wire deadlines | `WireError::Timeout` + `read_frame_with_deadline` / `write_frame_with_deadline` (5s handshake / 30s active default) | Transparent: stalled bridge connections close cleanly within deadline. No tool change needed |
| SDK retry | `AtdClient::connect` exponential-backoff + ±20% jitter | Transparent: `ATD_CONNECT_RETRIES=5` default. healthkit's hermes-bridge spawn pattern benefits if you ever go past single-session, but no immediate change needed |
| Audit sink | `JsonLinesAuditSink` rewritten to bounded `tokio::sync::mpsc` + dedicated drain task | Transparent: `on_call` is non-blocking; drops counter exposed via `Server::metrics_snapshot()` |
| Metrics | `MetricsCounters` + `Server::metrics_snapshot()` surface accepted_connections / dispatched_requests / dispatch_errors_by_code / audit_events_total / audit_drops_total | Optional: expose via a `/admin/metrics` route in the healthkit binary if useful for ops dashboards |

### What we need

```bash
cd /home/nan/code/healthkit_cli
cargo build --release
cargo nextest run --workspace
```

Expected: 100% green. The SP is back-compat. If anything in the rebuild breaks (missing field on `SharedServerConfig`, `ServerState`, etc.), `cargo clean -p atd-...` first.

If hermes-driven integration tests exist (`tests/atd_server_e2e.rs`, `tests/atd_server_helper_tools_e2e.rs` were green pre-SP), confirm they still pass. No new assertions needed — the SP doesn't change observable single-client behavior.

### Acceptance criteria for Part 1

- [ ] Rebuild against `sp-concurrency-baseline` tag without source edits.
- [ ] Existing test suite stays 100% green.
- [ ] Optional: smoke-test a high-concurrency hermes scenario (10+ parallel `hermes ask` invocations against the same socket) and confirm no `Connection lost` errors. Not gating this issue — celia owns the real concurrency benchmark.

## Part 2 — SP-pagination-v1 (active migration, ~6h for 1-3 tools)

### Why healthkit needs pagination

The 26 helper tools in `src/atd_server/helper_tools.rs` route through `HelperClass`:

| HelperClass variant | Tool count | Pagination value |
|---|---|---|
| `Polymerize { data_type }` | ~6 | Low — payloads typically small (aggregated metrics) |
| `HealthRecord { data_type, .. }` | ~10 | **High** — raw observation arrays over multi-month windows blow the 1MB budget |
| `ActivityRecord` | 1 | **High** — same shape (raw workout/activity arrays) |
| `Daily` | ~6 | Medium — daily summaries fit usually, but 6-month windows × 50 buckets/day = ~9000 rows × 300B ≈ 2.7MB |
| `Overview` | 1 | None — pre-cached static payload |

Today's workaround: the helper tools' input schemas implicitly narrow the time window. Agents end up making 6 sequential calls for a 6-month summary, paying 6× dispatch + audit overhead. Native pagination collapses this to one logical request that walks pages server-side.

### What's available on the ATD side

`atd-runtime::registry::Tool` gains two new **default-impl** trait methods (existing tools work unchanged):

```rust
pub trait Tool: Send + Sync {
    fn definition(&self) -> &ToolDefinition;
    fn call<'a>(&'a self, args: Value, ctx: &'a CallContext) -> CallFuture<'a>;

    // NEW — opt-in flag
    fn supports_pagination(&self) -> bool { false }

    // NEW — default wraps `call`, returning `next_cursor: None`
    fn call_paginated<'a>(
        &'a self,
        args: Value,
        ctx: &'a CallContext,
        cursor: Option<&'a str>,
    ) -> PaginatedCallFuture<'a> { /* ... */ }
}
```

Tools that want pagination override both methods. Inside `call_paginated`:
- `cursor: None` → produce page 1
- `cursor: Some(s)` → dispatch already HMAC-verified the cursor; decode `payload.opaque_state` to resume

Mint the next-page cursor via `ctx.cursor_issuer().issue(payload)`. The opaque_state field (256-byte budget) carries the tool's continuation token — a HealthKit `HKQueryAnchor`, a `(start_time, last_uuid)` keyset, an offset, whatever.

See `atd-conformance::tests::paginated_dispatch::RowGenerator` for a reference paginating tool (~100 lines).

### Recommended migration order

Pick **one** tool first to land the pattern, then propagate. Recommended order by ROI:

1. **`HealthRecord { data_type: BloodPressure, .. }`** (or similar high-volume vital) — concrete, measured pain point. The HKQuantityTypeIdentifier observation queries can use `HKAnchoredObjectQuery` with `HKQueryAnchor` as continuation state; serialize the anchor into `opaque_state`.

2. **`ActivityRecord`** — `HKWorkoutQuery` returns full workout objects; even a quarter's worth can easily exceed 1MB.

3. **`Daily { data_type, .. }`** for the bigger date ranges — although these aggregate, the per-day granularity × multi-month windows still hit limits.

The `Polymerize` and `Overview` tools should NOT migrate (small payloads, not worth the migration cost).

### Step-by-step for a single tool

```rust
// src/atd_server/helper_tools.rs (or wherever the Tool impl lives)
impl Tool for HelperTool {
    fn definition(&self) -> &ToolDefinition { &self.def }

    fn supports_pagination(&self) -> bool {
        // Only true for HealthRecord / ActivityRecord / Daily variants.
        matches!(
            self.class,
            HelperClass::HealthRecord { .. }
            | HelperClass::ActivityRecord
            | HelperClass::Daily { .. },
        )
    }

    fn call<'a>(&'a self, args: Value, ctx: &'a CallContext) -> CallFuture<'a> {
        // Existing impl unchanged — non-paginating clients still get
        // single-shot results via this path.
    }

    fn call_paginated<'a>(
        &'a self,
        args: Value,
        ctx: &'a CallContext,
        cursor: Option<&'a str>,
    ) -> PaginatedCallFuture<'a> {
        let issuer = ctx.cursor_issuer().expect("dispatch attaches for paginated tools");
        Box::pin(async move {
            // 1. Decode cursor into HKQueryAnchor (or whatever your
            //    continuation token shape is). cursor=None → first page.
            let anchor = match cursor {
                None => None,
                Some(c) => {
                    let payload = issuer.verify(c, 300).expect("dispatch pre-verified");
                    Some(deserialize_anchor(&payload.opaque_state)?)
                }
            };

            // 2. Run the HealthKit query with the anchor + a server-side
            //    page size limit (e.g., HKAnchoredObjectQueryLimit = 200).
            let (rows, next_anchor) = run_anchored_query(args, anchor).await?;

            // 3. Mint next cursor IFF the query reported more rows available.
            let next_cursor = if let Some(na) = next_anchor {
                let payload = CursorPayload {
                    tool_id: self.def.id.clone(),
                    caller_id: ctx.caller_id.clone(),
                    args_fingerprint: atd_runtime::cursor::args_fingerprint(&args),
                    page_index: /* increment from cursor */,
                    issued_at_unix: now_unix(),
                    server_session: issuer.session_nonce(),
                    opaque_state: serialize_anchor(&na),
                };
                Some(issuer.issue(payload)?)
            } else {
                None
            };

            Ok(PaginatedResult {
                value: serde_json::to_value(rows)?,
                next_cursor,
            })
        })
    }
}
```

### Wire-level visibility

After migration, agents calling via the MCP bridge see:

- **Default mode** (`hermes mcp add` without env flag): first page of data + a structured truncation notice block ("more data available; ask user or narrow args; operator can enable passthrough with ATD_MCP_PASSTHROUGH_CURSOR=1"). LLM can either narrow the time window or prompt the user.
- **Passthrough mode** (`ATD_MCP_PASSTHROUGH_CURSOR=1` on the bridge): `nextCursor` surfaces in the MCP tools/call result; cursor-aware MCP clients re-issue `tools/call` with `arguments.__cursor` to fetch subsequent pages.

For direct SDK callers (no MCP bridge), `AtdClient::call_all(tool_id, args, opts)` auto-loops with `MergePolicy::ConcatArray` (raw row lists) or `MergePolicy::ConcatField("observations")` (envelope with metadata).

### Acceptance criteria for Part 2

- [ ] At least one tool (recommended: `HealthRecord` variant) overrides `supports_pagination` + `call_paginated`.
- [ ] A unit test in `helper_tools.rs` (mirroring `atd-server::connection::tests::run_tool_continue_*`) covers the new tool's paginated path: cursor minting, opaque_state round-trip, terminal page omits cursor.
- [ ] An integration test in `tests/` walks the full paginated query end-to-end against a real `atd-server` instance.
- [ ] Live smoke: `hermes ask "show me my blood pressure for the last 6 months"` returns a single coherent answer (LLM either summarizes the first page + asks for the rest, or fetches all pages via passthrough — both shapes are acceptable).
- [ ] Migration documented in `healthkit_cli/docs/sp-pagination-v1-adopter.md` (or `case-study-v1.5.0/` if you follow the existing case-study pattern).
- [ ] Optional: open follow-up issues for the remaining high-volume tools (`ActivityRecord`, `Daily`) once the first migration ships.

## Failure modes to watch for

If the rebuild or migration reveals any of these, file a new ATD-side issue cross-linking this one — they would be real bugs in the SP:

1. **`cargo build` errors on missing field.** Stale build artifact — `cargo clean` and retry. If it persists, escalate.
2. **`Tool::call_paginated` default-impl deadlock or panic.** The default wraps `call` and returns `next_cursor: None`; if it ever panics, that's an atd-runtime bug.
3. **`ctx.cursor_issuer()` returns `None` inside a `supports_pagination = true` tool.** Dispatch is supposed to attach the issuer for paginated tools (`dispatch.rs:441-458`). If you hit this, that's a dispatch bug.
4. **Continuation page doesn't run middleware** (e.g., if you ever wire `atd-middleware-pii-redact-medical` on the healthkit side). Fixed in the perf-v1 final commit (`db315e8`) — verify your build is past that commit. If not, escalate.
5. **Hermes MCP client shows empty content + only the truncation notice.** That means your tool returned an empty array for the first page. Check the HealthKit query is honoring the time window / args correctly.
6. **Cursors valid across `healthkit serve` restarts.** They should NOT be — `server_session` random nonce on `Server::new` invalidates outstanding cursors. If a stale cursor verifies post-restart, that's a `CursorIssuer` bug.

## References

- ATD spec (concurrency): `docs/superpowers/specs/2026-05-12-sp-concurrency-baseline-design.md`
- ATD spec (pagination): `docs/superpowers/specs/2026-05-12-sp-pagination-v1-design.md`
- ATD plan (pagination): `docs/superpowers/plans/2026-05-12-sp-pagination-v1.md` (§Phase D walkthrough is the closest fit for the adopter migration)
- Reference paginating tool: `crates/atd-conformance/tests/paginated_dispatch.rs::RowGenerator` (100 lines, copy-paste-friendly)
- Reference dispatch tests: `crates/atd-server/src/connection.rs::tests::run_tool_continue_*`
- Architecture deployment shapes: `docs/architecture.md` §11 (concurrency) + §11.5 (pagination)
- ADRs: `docs/adr/0002-concurrency-baseline.md` · `docs/adr/0003-pagination-v1.md`
- Hermes integration with passthrough env: `docs/integrations/hermes.md` § "Pagination (SP-pagination-v1)"
