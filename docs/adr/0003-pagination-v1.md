# ADR 0003 — Pagination is a protocol-level primitive

- **Status:** Accepted
- **Date:** 2026-05-12
- **Deciders:** `atd` maintainers
- **Related:** [`docs/architecture.md`](../architecture.md) §10 + §11.5 · [`docs/archive/superpowers/specs/2026-05-12-sp-pagination-v1-design.md`](../archive/superpowers/specs/2026-05-12-sp-pagination-v1-design.md) · sibling [SP-concurrency-baseline](../archive/superpowers/specs/2026-05-12-sp-concurrency-baseline-design.md) (perf-v1 axis 1) · [ADR-0002](./0002-concurrency-baseline.md)

## 1. Context

ATD's 10 MB wire frame ceiling and 1 MB advisory `max_output_bytes` budget are reasonable for single-shot tool calls but break for legitimate medical/health workloads:

- `healthkit_cli`'s `query_observations` over a 6-month window produces ~9000 entries × ~300 bytes ≈ 2.7 MB — 2.7× the budget. Today's workaround is implicit window-narrowing in the tool's input schema, but the agent makes 6 sequential calls and pays 6 round-trips of dispatch + audit overhead.
- `celia_phr`'s `bulk_export` already solved the *async out-of-band* case with a manifest-of-URLs (HL7 FHIR Bulk Data spec). But interactive paths (`list_observations`, `list_conditions`, `list_medications`) have no answer — they fit in 1 MB or they don't ship.

Three bad options today: (a) silently truncate and lie to the LLM about completeness; (b) refuse and let the agent retry with narrower args it has to guess at; (c) split into out-of-band manifest URLs — works offline, useless mid-conversation.

The protocol is missing a continuation primitive.

## 2. Decision

**Pagination is a protocol-level primitive**, not a per-tool workaround. The eight-axis implementation:

1. **Wire format** (`atd-protocol`): `Request::RunToolContinue { tool_id, cursor }` + `Response::ToolResultResponse.next_cursor: Option<String>` + error codes `1020 ERR_CURSOR_EXPIRED` / `1021 ERR_CURSOR_INVALID`. All additions are `#[serde(default, skip_serializing_if = "Option::is_none")]`-back-compat.

2. **Cursor module** (`atd-runtime::cursor`): `CursorIssuer` mints HMAC-SHA256-signed CBOR-encoded `CursorPayload` bound to `(tool_id, caller_id, args_fingerprint, page_index, issued_at_unix, server_session)`. 512-byte wire cap, 256-byte opaque-state budget. Stateless verification — no shared cursor table, scales horizontally.

3. **Tool author API** (`atd-runtime::registry::Tool`): two new default-impl methods — `supports_pagination() -> bool` (opt-in flag) and `call_paginated(args, ctx, cursor) -> PaginatedResult`. Existing tools unchanged; paginating tools override both.

4. **Dispatch routing**: `dispatch::run_tool` branches on `tool.supports_pagination()` — paginating tools call `Tool::call_paginated` directly (bypassing the `Binding` layer, attaching `CursorIssuer` to `CallContext`). Non-paginating tools keep the existing binding path. `dispatch::run_tool_continue` (now `pub`) handles `Request::RunToolContinue` end-to-end: cursor verify → tool_id match → registry lookup → `supports_pagination` check → capability re-check → semaphore → `tool.call_paginated(Null, ctx, Some(cursor))` → audit emission with `cursor_page`.

5. **SDK ergonomics** (`atd-sdk`): `AtdClient::call_page` (per-page) + `AtdClient::call_all` (auto-loop with `MergePolicy::{ConcatArray, ConcatField, FirstPageOnly}`). Sanity-bounded by `max_pages: 100` + `max_total_bytes: 32 MiB`.

6. **HTTP transport** (`atd-server-http`): cursor rides as `arguments.__cursor` on the way in; `nextCursor` on the way out. MCP-compatible extension — cursor-unaware clients ignore unknown fields.

7. **MCP bridge** (`atd-mcp-bridge`): default appends a structured truncation notice when a cursor would otherwise be lost; `ATD_MCP_PASSTHROUGH_CURSOR=1` switches to native passthrough.

8. **Conformance** (`atd-conformance`): `paginated_dispatch` scenario — 100-row generator, 10 pages × 10 rows, terminal-page cursor omission, cross-tool cursor rejection, per-page audit tagging.

## 3. Consequences

**Test count:** 598/598 workspace tests pass; 47+ new tests across the eight phases.

**Adopter impact:**

- `healthkit_cli`: can migrate `query_observations` and `query_workouts` to `call_paginated`. ~80 LoC per tool: refactor the underlying HealthKit fetch to accept a `last_uuid` continuation, wrap in `call_paginated`, set `supports_pagination() -> true`. Spec §7 (pagination-v1 spec) documents the migration path.
- `celia_phr`: opens new tool surface (`list_observations`, `list_conditions`, `list_medications`) that was previously blocked on lack of pagination. Existing `bulk_export` is unchanged — different shape (async out-of-band; not paginated dispatch).

**Public API additions** (all back-compat):
- `atd_protocol::{Request::RunToolContinue, Response::ToolResultResponse.next_cursor, ERR_CURSOR_EXPIRED, ERR_CURSOR_INVALID}`
- `atd_runtime::cursor::{CursorIssuer, CursorPayload, CursorError, random_signing_key, args_fingerprint}`
- `atd_runtime::registry::{PaginatedResult, PaginatedCallFuture}` + `Tool::{supports_pagination, call_paginated}` default-impl methods
- `atd_runtime::CallContext::{cursor_issuer, with_cursor_issuer}`
- `atd_runtime::audit::CallEvent.cursor_page` + `SCHEMA_VERSION` bumped 1 → 2
- `atd_runtime::ServerState.cursor_issuer` + `SharedServerConfig.{cursor_signing_key, cursor_ttl_seconds}`
- `atd_runtime::dispatch::run_tool_continue` (pub)
- `atd_sdk::{PaginatedSdkResult, CallAllOptions, MergePolicy, AtdClient::{call_page, call_all}}`
- `atd_protocol::AtdError::{PaginationLimitExceeded, MergeFailed}`
- `atd_mcp_bridge::mcp::ToolsCallResult.next_cursor`

**Public API breaking changes:** none.

**v1 constraints** (documented for future SP revisits):

- Paginated tools execute through native (in-process) semantics, bypassing `Binding`. CLI / future MCP / REST bindings cannot paginate — subprocess boundaries don't survive cursor state. Future SP can add stateful continuation protocols if a CLI-backed paginated tool emerges.
- Single-process cursor signing key. Multi-instance deployments behind a load balancer should share a key via env (`ATD_CURSOR_SIGNING_KEY=base64...`); the listener crates' `Server::new` random-key default targets single-instance deployments.

## 4. Alternatives considered

- **Streaming responses (chunked transfer, SSE, websockets).** Out of scope. Pagination assumes each page fits in `max_output_bytes`. A future `SP-streaming-v1` would target tools producing 100 MB+ continuous data (sensor streams, large LLM outputs).
- **Two-way / bidirectional cursors.** YAGNI for conversational LLM agents; only forward `next_cursor`.
- **Stateful cursor tables (UUID → server-side state).** Rejected as the default. Stateless HMAC scales horizontally without shared state; adopters needing unlimited state can layer a stateful index keyed by a 16-byte cursor ID inside the 256-byte `opaque_state`.
- **Pushing cursor semantics into the MCP spec.** Out of scope (upstream's job). We ship a compatible-when-extended impl in the bridge and document the env-flag workaround; we don't block on the MCP standards body.

## 5. References

- Spec: `docs/archive/superpowers/specs/2026-05-12-sp-pagination-v1-design.md`
- Plan: `docs/archive/superpowers/plans/2026-05-12-sp-pagination-v1.md`
- Wire format: `docs/protocol/wire-format.md` §4.4.1
- Error codes: `docs/protocol/error-codes.md` §2.3g + §2.3h
- Conformance test: `crates/atd-conformance/tests/paginated_dispatch.rs`
- Architecture deployment-shapes section: `docs/architecture.md` §11.5
- Sibling SP for the runtime/concurrency axis of the same perf-v1 iteration: `docs/adr/0002-concurrency-baseline.md`
