# SP-pagination-v1: protocol-level result pagination for large tool outputs

| Status | Draft |
| Created | 2026-05-12 |
| Author | cross-project subagent (healthkit_cli + celia_phr large-output incidents ↔ atd-mvp coordination) |
| Phase | ATD post-`sp-medical-middleware`; sibling of SP-concurrency-baseline; both ship under the `perf-v1` iteration umbrella |
| Related | **SP-concurrency-baseline** (`2026-05-12-sp-concurrency-baseline-design.md`) — sibling perf SP; this one addresses the *shape* of large results, that one addresses the *runtime* under load. SP-12 (`2026-04-25-sp12-canonical-dispatch.md`) — canonical dispatch we extend with a new `RunToolContinue` branch. SP-streamable-http (`2026-05-11-sp-streamable-http-design.md`) — HTTP transport that must thread cursors through MCP. ATD v3 whitepaper §K (result middleware) — middleware applies per-page, not per-call, after this SP. |

---

## 1. Motivation

**1.1 ATD has a 10 MB wire ceiling and a 1 MB tool-output budget; real medical / health workloads blow past both regularly.** `crates/atd-protocol/src/wire.rs:4` hard-caps every length-prefixed frame at `MAX_FRAME_BYTES = 10 * 1024 * 1024`. `crates/atd-runtime/src/dispatch.rs:59,94` advises tools to self-truncate to `max_output_bytes: 1 MB` by passing the budget through `CallContext`. Neither layer offers a *correct* answer for a tool whose honest result is 50,000 FHIR `Observation` rows over 30 days, ~200 bytes/row JSON-serialized ≈ 10 MB raw, ~40 MB once you include common FHIR boilerplate (`meta`, `subject`, `effectiveDateTime`, `code` blocks). Today such a tool has three bad options: (a) silently truncate and lie to the LLM about completeness; (b) refuse and let the agent retry with narrower windows it has to guess at; (c) split into out-of-band manifest URLs (celia's `crates/celia-core/src/bulk_export/mod.rs` pattern) which works for offline batch export but loses the conversational round-trip the LLM expects.

**1.2 The two adopters that consume `path = atd-mvp` already hit this wall, in different shapes.**

*healthkit_cli.* 26 helper tools live in `/home/nan/code/healthkit_cli/src/atd_server/helper_tools.rs:84` (`build_helper_tool_defs`). The translators at lines 413 (`dispatch_health_record`), 442 (`dispatch_activity_record`), 468 (`dispatch_daily`) each call into Apple HealthKit's `HKHealthStore` query API and return whatever HealthKit returns. A daily-step-summary query for a six-month window returns ~180 days × ~50 hourly buckets = 9000 entries; each `HKQuantitySample` JSON-projects to ~300 bytes; total ~2.7 MB, already 2.7× the recommended budget and well within "the LLM cannot reason over this verbatim" territory. Healthkit's current solution is implicit window-narrowing in the tool's prompt schema, but the agent then makes 6 sequential calls and pays 6 round-trips of dispatch + audit overhead.

*celia_phr.* `bulk_export/mod.rs:41-50` emits an `ExportManifestEntry { resource_type, url, count }` — a manifest of pre-rendered FHIR `Bundle.ndjson` files the client fetches over HTTPS *outside* the ATD dispatch path. This is the FHIR Bulk Data spec (HL7 fhir-bulk-data §3.2) and is the right answer for hour-long async exports, but a complete mismatch for "show me my last 30 days of glucose readings during this chat turn." Celia's interactive code paths don't have a paginated answer today; they either fit in 1MB or they don't ship.

**1.3 The protocol is missing a continuation primitive.** The current wire-format `Response::ToolResultResponse { tool_id, result, success, dry_run }` (`crates/atd-protocol/src/messages.rs:113-118`) is *terminal* — it carries one `serde_json::Value` and ends the dispatch. There is no second-frame mechanism, no cursor, no "more available" signal. A tool that knows it has more data has nowhere to put that knowledge. From the agent side, `AtdClient::call` (`crates/atd-sdk/src/client.rs`) is one-shot. The MCP bridge translates ATD tool calls to MCP `tools/call` which is similarly terminal (no MCP-spec cursor today, though MCP's draft "stream" extension flirts with the idea). Adding a continuation primitive at the ATD wire layer is the structurally correct fix: it benefits direct SDK consumers (langchain adapter, openai adapter), the MCP bridge can map it to MCP's resource subscription or simulate cursors at the bridge layer (§6 below), and the HTTP transport can natively expose it as a query parameter.

**1.4 ATD already chose JSON-over-frames; pagination is not a transport rewrite, it's an additive primitive.** This SP does *not* introduce streaming responses, SSE, or HTTP chunked transfer at the application layer. It defines a `next_cursor` field on `Response::ToolResultResponse` plus a `Request::RunToolContinue { tool_id, cursor }` to fetch the next page. Total wire-format delta: one optional field and one new request variant. Every existing tool continues to work; opting in is a per-tool decision driven by the new `Tool::call_paginated` author API (§4.4). The result middleware chain (`atd_runtime::Middleware::on_result`) runs once per page, preserving the v3 whitepaper §K pipeline shape per-page.

## 2. Goals

- **G1: wire-format additions, fully back-compat.** New `Response::ToolResultResponse.next_cursor: Option<String>`, new `Request::RunToolContinue { tool_id: String, cursor: String }`, new `Response::ContinueResponse` (alias for `ToolResultResponse` to keep response variants disjoint). Pre-pagination clients ignore `next_cursor` and behave identically; pre-pagination servers don't know `RunToolContinue` and return the existing "method not found" error which clients map to "this server doesn't paginate; degrade gracefully" (§4.7).
- **G2: tool author API — opt-in `Tool::call_paginated`.** Tools that want pagination implement a new optional trait method `call_paginated(&self, args, ctx, cursor: Option<&str>) -> Result<PaginatedResult, ToolCallError>` returning `PaginatedResult { value: Value, next_cursor: Option<String> }`. Existing `Tool::call` impls are unchanged and treated as "single-page complete." Migration is a per-tool decision; this SP does not migrate any built-in tool.
- **G3: cursor opacity contract.** Cursors are server-opaque strings (`String`), max 512 bytes, base64url-friendly. Clients treat them as opaque. Servers MAY embed encrypted state, offset/limit, or a database keyset; this SP normatively bans clients from parsing or constructing cursors. Servers MUST validate cursor authenticity (HMAC suggested in §4.5) so a forged cursor cannot leak data outside the original call's authorization.
- **G4: cursor lifetime — server-side TTL, expired cursors return ERR_CURSOR_EXPIRED.** Cursors are valid for `SharedServerConfig.cursor_ttl_seconds` (default 300s = 5min). Expired cursors return `Response::Error { code: Some(1020), message: "cursor expired", retryable: false }`. The client's correct response is to re-issue the original `RunTool` call (cursor state was never durable). Code 1020 is the next free slot after SP-capability-v2's 1010-1013.
- **G5: middleware applies per-page.** `Middleware::on_result` runs on each paginated frame independently — FHIR validation, PHI redaction, etc., per page. This is the v3 whitepaper §K.4 shape preserved unchanged. The middleware trait signature does *not* change; it always saw "one result at a time" and continues to.
- **G6: audit emits one event per page.** Each `RunTool` / `RunToolContinue` is one `CallEvent` with `tool_id`, `duration_ms`, `outcome`. A new `CallEvent.cursor_page: Option<u32>` field records the page index (1 for the initial RunTool that returned a cursor, 2/3/... for each RunToolContinue) so audit trails can reconstruct full-call dynamics. Schema version bumps to `SCHEMA_VERSION = 2` (currently `1`, `crates/atd-runtime/src/audit.rs`).
- **G7: SDK ergonomics — `AtdClient::call_all` convenience method.** New `AtdClient::call_all(tool_id, args, options) -> Result<Value, AtdError>` that internally loops `RunTool` + `RunToolContinue` until `next_cursor.is_none()`, concatenating page bodies under a documented merge policy (§4.8). For callers that want pages, the lower-level `call_page(tool_id, args, cursor: Option<&str>)` returns one page at a time. Both are additive; existing `call()` is unchanged.
- **G8: HTTP transport — cursor as query parameter.** `atd-server-http`'s MCP `tools/call` translator surfaces pagination as `POST /mcp tools/call` returning a result envelope with `nextCursor: string | null`. Continuation is `POST /mcp tools/call` with `{ name, arguments: { __cursor: "..." } }` per MCP draft extension proposal. The HTTP layer does not break the existing `/atd/v1/run_tool` route either; both routes get the same paginated semantics.
- **G9: MCP bridge — degrade to "single page only" by default, opt-in cursor passthrough.** The MCP spec (2025-11-25) does not define cursors for `tools/call`. The bridge default exposes the FIRST page only and appends a structured `[...truncated; this server supports continuation but Hermes/your MCP client does not]` notice to the text content when `next_cursor.is_some()`. An opt-in mode (`ATD_MCP_PASSTHROUGH_CURSOR=1`) extends the response with a non-standard `nextCursor` field for MCP clients that have been patched to handle it (Hermes is one target).
- **G10: conformance — `paginated_dispatch` scenario.** `atd-conformance` adds a scenario registering a synthetic "100-row generator" tool that returns 10 rows per page; the test asserts (a) initial call returns 10 rows + a cursor; (b) 10 continues fetch all 100 rows with cursors; (c) 11th continue returns no cursor (terminal); (d) cursor expiration returns code 1020; (e) audit emits 11 events with `cursor_page` 1-11.
- **G11: documentation.** `docs/protocol/wire-format.md` documents the new variants and the cursor opacity contract. `docs/atd-architecture.md` §11 (added in SP-concurrency-baseline) gains a §11.5 "Large results & pagination" subsection. `docs/integrations/hermes.md` documents the bridge's degrade-or-passthrough modes.

## 3. Non-goals

- **Streaming responses (chunked transfer, SSE, websockets).** A tool returning 100 MB of streaming sensor data wants chunked frames, not paginated JSON. Out of scope; future `SP-streaming-v1`. Pagination assumes each page fits in `max_output_bytes`.
- **Sorting/ordering guarantees across pages.** Cursors carry whatever ordering the tool's underlying source provides; ATD does not impose total order. A tool serving `SELECT * FROM observations` without `ORDER BY` may return rows in non-deterministic order across pages. Tool authors documentt their ordering; ATD does not police.
- **Two-way / bidirectional cursors.** Only `next_cursor` (forward). No `prev_cursor`. Conversational LLM agents almost always paginate forward; backward seek is YAGNI.
- **Cross-tool cursors.** A cursor from `tool A` cannot be resumed against `tool B`. Cursors are scoped `(tool_id, caller_id, original_args_fingerprint, server_session)`.
- **Cursor durability across server restarts.** Cursors are server-process-local. A restart invalidates outstanding cursors (`ERR_CURSOR_EXPIRED`). Adopters needing durable cursors implement their own checkpoint-and-resume in tool-args, not via the ATD cursor.
- **`call_all` size caps via the client.** The convenience method documents that callers are responsible for sanity-bounding aggregate size; this SP does not introduce client-side size caps. (Server-side per-page caps still apply.)
- **MCP standardization.** Pushing cursor semantics into the MCP spec is upstream's job. We ship a compatible-when-extended impl in our bridge and document the workaround. We do not block on the MCP standards body.
- **Inbound (request-side) pagination.** `Request::ToolList` is also a "list response" and could benefit from pagination, but no current adopter has >100 tools registered in one server; the existing single-shot variant is fine. If a future adopter needs `ToolList` pagination, that's an additive follow-up using the same cursor primitive.

## 4. Design

This is ~55% of the SP. Each subsection is one decision point with chosen answer, evidence, and rejected alternatives.

### 4.1 Wire-format additions — one optional field, one new request variant

**Decision.** Modify `crates/atd-protocol/src/messages.rs`:

```rust
#[serde(tag = "type")]
pub enum Request {
    // ... existing variants ...
    #[serde(rename = "run_tool")]
    RunTool {
        tool_id: String,
        args: serde_json::Value,
        dry_run: bool,
    },
    // NEW:
    #[serde(rename = "run_tool_continue")]
    RunToolContinue {
        tool_id: String,
        cursor: String,
    },
}

#[serde(tag = "type")]
pub enum Response {
    // ... existing variants ...
    #[serde(rename = "tool_result")]
    ToolResultResponse {
        tool_id: String,
        result: serde_json::Value,
        success: bool,
        dry_run: bool,
        // NEW (#[serde(default, skip_serializing_if = "Option::is_none")]):
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_cursor: Option<String>,
    },
}
```

`Response::ContinueResponse` is **not** a new variant — we reuse `ToolResultResponse` for both initial and continuation responses. The two are byte-identical on the wire; the only difference is what triggered them. Continuation responses with no further pages set `next_cursor = None` (the default; field omitted on the wire).

**Why reuse `ToolResultResponse`.** Two reasons:
1. Adapters that already pattern-match `Response::ToolResultResponse` need no edit to display continuation data — they already render `result`. The new field is `#[serde(default)]` so old clients ignore it.
2. SP-12 dispatch (`crates/atd-runtime/src/dispatch.rs`) already pipes both error and success paths through one match arm per request variant. Adding a sibling response variant would force two arms with 90% duplicated body; a single variant keeps the canonical dispatch surface clean.

**Evidence.** SP-12 §4.3 (canonical dispatch) §6 (response shape) explicitly anticipates additive optional fields: "Adding fields is back-compat as long as `#[serde(default)]` is applied; consumers see new fields, old consumers ignore them." (`docs/superpowers/specs/2026-04-25-sp12-canonical-dispatch-design.md` §6.) Pre-SP-capability-v2 servers handled `Hello.ucan_tokens` the same way — empty vec by default.

**Trade-offs:**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Reuse `ToolResultResponse` + optional `next_cursor` | Zero adapter migration; tiny wire delta | One field, future fields land here too — could bloat | **chosen** |
| Two variants `ToolResultResponse` / `PagedResultResponse` | Type-level disjointness | Doubles the dispatch match arms; old clients break on the new tag | rejected |
| Wrap in an envelope `{ data, cursor }` per response | DRY across future paged responses | Every existing adopter has to unwrap one extra layer | rejected |

### 4.2 Cursor format — server-opaque string, ≤512 bytes, HMAC-signed reference suggested

**Decision.** The wire-level cursor is `String`. ATD imposes a 512-byte cap (well within any reasonable HTTP header / log line) and a normative requirement that **clients MUST NOT parse cursors**. Server impls are free to choose:

- **Reference impl (ATD ref-server's built-in tools, when they get paginated):** HMAC-SHA256-signed JSON `{ tool_id, caller_id, args_fingerprint, page_index, opaque_state }`. The HMAC key is `SharedServerConfig.cursor_signing_key: [u8; 32]` (new field; defaulted to a random key generated at server startup so cursor forgery requires a process-state compromise, not just guessing). Forged cursors fail HMAC verify → `Response::Error { code: Some(1021), message: "invalid cursor signature" }`.
- **Adopter impls (celia, healthkit):** Free choice. celia might base64url-encode an encrypted FHIR continuation token; healthkit might base64url-encode `(query_id: u64, last_sample_uuid: String)`. ATD doesn't care.

**Why 512 bytes.** Cursors propagate through MCP `tools/call.arguments.__cursor` (§4.6 below) which is a JSON-string field, so any binary state needs base64url-encoded. 512 bytes of base64 ≈ 384 bytes of state — enough for an HMAC tag (32) + tool_id (~64) + caller_id (~36 ULID) + page_index (8) + a 200-byte opaque-state pointer. If adopters need more state, store it server-side keyed by a 16-byte cursor ID.

**Why HMAC over UUID-then-server-table-lookup.** Stateless. The server doesn't need a shared cursor table across worker threads; it just verifies the HMAC and trusts the embedded state. Stateful cursor tables are an optimization adopters can layer on; the reference impl stays simple.

**Cursor scope.** Cursors are scoped to `(tool_id, caller_id, args_fingerprint, server_session)`:
- `tool_id` — embedded in the cursor; mismatching `RunToolContinue.tool_id` rejected.
- `caller_id` — the Hello-set caller_id at the time the cursor was issued. If the connection's `caller_id` changes (it shouldn't — UDS handshake sets it once; HTTP per-request bearer might), the cursor is invalidated.
- `args_fingerprint` — SHA256 of the canonical-JSON-serialized `args` from the original `RunTool`. Continuing with mutated args is a protocol violation; ATD rejects.
- `server_session` — a server-startup-random nonce. Server restarts invalidate all cursors.

**Trade-offs:**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Stateless HMAC-signed cursor | No shared state; horizontally scalable; survives worker restart inside one process | 512-byte cap limits embedded state | **chosen** |
| Server-side cursor table (UUID → state) | Unlimited state | Requires shared `RwLock<HashMap>` across workers — exactly the kind of contention SP-concurrency-baseline §5.4 fights | rejected |
| Opaque + adopter-defined; ATD imposes no signing | Most flexible | Reference impl needs *something*; without HMAC the ref-server is forge-vulnerable | rejected for ref-server |

### 4.3 Cursor TTL — 300s default, expiry returns code 1020

**Decision.** `SharedServerConfig.cursor_ttl_seconds: u64` (new field, default `300`). Cursors carry an `issued_at_unix: u64` field; servers reject cursors where `now - issued_at > ttl`. Expired cursors return `Response::Error { code: Some(1020), message: "cursor expired; re-issue the original RunTool", retryable: false }`.

Reserve **`atd_protocol::ERR_CURSOR_EXPIRED = 1020`** and **`ERR_CURSOR_INVALID = 1021`** in `crates/atd-protocol/src/lib.rs` (next free slots after SP-capability-v2's 1010-1013).

**Evidence + why.** 300s is the conversational sweet spot: LLM agents typically issue continue-calls within seconds of the initial result (if they want them at all); a 5-minute window leaves room for one human "think" round-trip and reasonable network latency without indefinite server-side state (even though HMAC cursors are stateless, the *intent* of the cursor — "this query is still relevant" — has a natural decay). Longer TTL invites stale-data confusion (the underlying source may have changed); shorter TTL is annoying.

**Why `retryable: false`.** From the protocol's view, this specific cursor is dead-forever. The client's correct retry is *not* to re-send the same cursor (which will fail identically) but to re-issue the original `RunTool` to get a fresh cursor. Marking `retryable: false` makes the standard error-handling helper in adapters (langchain's retry-on-retryable, anthropic's tool_use retry) skip retry, which is what we want.

**Why a separate `ERR_CURSOR_INVALID`.** Signature failures, malformed cursors, and cross-tool-id mismatches are *different from expiry*. An invalid cursor suggests a bug or attack (forge attempt); an expired cursor is a normal lifecycle event. Two codes lets ops dashboards alert differently.

### 4.4 Tool author API — `call_paginated` as an optional trait method

**Decision.** Extend `atd_runtime::registry::Tool` (currently at `crates/atd-runtime/src/registry.rs` — the trait definition) with one optional method:

```rust
pub trait Tool: Send + Sync {
    fn definition(&self) -> &ToolDefinition;
    fn call<'a>(&'a self, args: Value, ctx: &'a CallContext) -> CallFuture<'a>;

    /// Optional: paginated variant. If implemented, the dispatch layer
    /// routes `Request::RunTool` and `Request::RunToolContinue` here
    /// instead of `call`. Default impl: degrade to `call` returning the
    /// single page with no cursor.
    fn call_paginated<'a>(
        &'a self,
        args: Value,
        ctx: &'a CallContext,
        cursor: Option<&'a str>,
    ) -> PaginatedCallFuture<'a> {
        let fut = self.call(args, ctx);
        Box::pin(async move {
            let value = fut.await?;
            Ok(PaginatedResult { value, next_cursor: None })
        })
    }
}

pub struct PaginatedResult {
    pub value: serde_json::Value,
    pub next_cursor: Option<String>,
}

pub type PaginatedCallFuture<'a> = std::pin::Pin<Box<dyn Future<Output = Result<PaginatedResult, ToolCallError>> + Send + 'a>>;
```

Existing impls (echo, fs, shell, web, the 26 healthkit helpers) work unchanged — they inherit the default which wraps `call`'s `Value` in a `PaginatedResult { value, next_cursor: None }`.

**Why a default method, not a separate trait.** A separate `PaginatedTool: Tool` trait would force dispatch to do `if let Some(p) = tool.as_paginated() ... else ...` — two paths, type-erased downcast. A default method keeps one trait, one dispatch path: dispatch always calls `call_paginated`, non-paginated tools just return `next_cursor: None`. The runtime cost (one `Pin<Box<dyn Future>>` allocation per call instead of zero for the non-paginated case) is < 100 ns, dwarfed by the actual tool work.

**Why expose `cursor: Option<&'a str>` and not a typed `Cursor` struct.** The tool decides what its cursors mean. ATD doesn't impose schema; passing the opaque string lets the tool encode whatever it needs. For tools using ATD's reference HMAC-signed cursor (§4.2), a helper `atd_runtime::cursor::verify_and_extract(cursor, config) -> Result<CursorState, _>` is provided.

**Trade-offs:**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Optional default trait method | Zero migration; one dispatch path | Default impl allocates one Pin<Box>; minor | **chosen** |
| Separate `PaginatedTool: Tool` super-trait | No allocation for non-paginated tools | Dispatch needs `dyn_cast` or registry stores two trait objects | rejected |
| `Tool::call` returns `PaginatedResult` always | Most uniform | Breaks every existing impl; 100+ test edits | rejected |

### 4.5 Reference cursor helper — `atd_runtime::cursor::{Issuer, verify}`

**Decision.** New module `crates/atd-runtime/src/cursor.rs` providing:

```rust
pub struct CursorIssuer {
    key: [u8; 32],
    session_nonce: [u8; 8],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CursorPayload {
    pub tool_id: String,
    pub caller_id: Option<String>,
    pub args_fingerprint: [u8; 32],
    pub page_index: u32,
    pub issued_at_unix: u64,
    pub server_session: [u8; 8],
    #[serde(default)]
    pub opaque_state: Vec<u8>, // capped at 256 bytes
}

impl CursorIssuer {
    pub fn new(key: [u8; 32]) -> Self { /* generates random session_nonce */ }
    pub fn from_config(config: &SharedServerConfig) -> Self { /* derives from cursor_signing_key */ }

    pub fn issue(&self, payload: CursorPayload) -> Result<String, CursorError> {
        // serialize payload (CBOR or msgpack — denser than JSON; chosen: CBOR via ciborium)
        // HMAC-SHA256 over serialized bytes using self.key
        // concat (payload_bytes || tag) and base64url-encode
        // assert total ≤ 512 bytes
    }

    pub fn verify(&self, cursor: &str, ttl: Duration) -> Result<CursorPayload, CursorError> {
        // decode, split payload/tag, verify HMAC (constant-time), check ttl, check server_session
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CursorError {
    #[error("cursor expired")]            Expired,
    #[error("cursor signature invalid")]  InvalidSignature,
    #[error("cursor format invalid: {0}")] Format(String),
    #[error("cursor too large: {0} bytes")] TooLarge(usize),
}
```

`atd-runtime::dispatch` injects the issuer into `CallContext` so tools that opt into pagination can call `ctx.cursor_issuer().issue(...)` without touching keys directly.

**Why CBOR (via `ciborium`) instead of JSON.** Cursors are byte-tight (512-byte cap, §4.2). JSON of the `CursorPayload` is 200+ bytes for the field names alone; CBOR packs the same data in ~80 bytes, leaving ~300 bytes for `opaque_state`. `ciborium` is `no_std`-friendly, 50KB compiled, no transitive deps that aren't already in our tree.

**Why HMAC-SHA256, not Ed25519.** Symmetric is faster (cursor verify on every continue-call must be cheap), and we don't need public-key verification across hosts — a single server's cursor is verified by that same server. Ed25519 fits if we ever cross trust boundaries.

**Why `[u8; 32]` and not `[u8; 16]`.** SHA-256's output is 32 bytes; truncating loses no security guarantee but loses the standard. Sticking with 32 bytes mirrors `secret-bootstrap`'s key sizes (`crates/atd-runtime/src/...` — symmetric KDF outputs).

### 4.6 HTTP transport — cursor as MCP-extension query parameter

**Decision.** `atd-server-http`'s MCP `tools/call` translator at `crates/atd-server-http/src/mcp.rs` (current version pre-SP-pagination-v1):

- **Initial call:** `POST /mcp` body `{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"x","arguments":{...}}}`. Response result includes `nextCursor` when set: `{"jsonrpc":"2.0","id":1,"result":{"content":[...], "isError":false, "nextCursor":"abc"}}`.
- **Continue:** Same shape but `arguments.__cursor: "abc"`. The translator detects the `__cursor` key, extracts and removes it from `args`, and dispatches `Request::RunToolContinue { tool_id, cursor }` instead of `RunTool`.
- **Native ATD route (`/atd/v1/run_tool`, `/atd/v1/run_tool_continue`):** Both supported. Continue is a separate POST endpoint to keep the route → request-variant mapping 1:1.

**Why `__cursor` in `arguments` (not a separate JSON-RPC field).** MCP 2025-11-25's `tools/call.params` schema only knows `name` and `arguments`. Adding a top-level `cursor` would be a JSON-RPC schema violation; embedding inside `arguments` keeps strict spec-compliance for non-cursor-aware clients while being trivially detectable for cursor-aware clients. The `__` prefix is the standard "ATD-extension; not part of the tool's own args" convention.

**Why mirror the route into a separate `/atd/v1/run_tool_continue`.** Clients using the native ATD HTTP API (no MCP translation) get a clean RPC-style mapping. SP-streamable-http §6 already runs both `/mcp` and `/atd/v1` against the same dispatch; this SP keeps that symmetry.

### 4.7 MCP bridge — degrade to single-page by default, opt-in passthrough

**Decision.** `atd-mcp-bridge`'s `handle_tools_call` (`crates/atd-mcp-bridge/src/bridge.rs:118`) gains cursor-handling logic:

```rust
async fn handle_tools_call(...) {
    // ... existing dispatch ...
    match self.client.call_page(&atd_id, params.arguments, cursor.as_deref(), ...).await {
        Ok(PaginatedSdkResult { value, next_cursor }) => {
            let passthrough = std::env::var("ATD_MCP_PASSTHROUGH_CURSOR").as_deref() == Ok("1");
            let mut result = ToolsCallResult {
                content: vec![ContentBlock::Text { text: serde_json::to_string(&value)? }],
                is_error: false,
            };
            if let Some(cur) = next_cursor {
                if passthrough {
                    // Inject non-standard `nextCursor` into the MCP response.
                    // Wrap in a custom serde shape (see §8 Q1 for the trait extension).
                    result.next_cursor = Some(cur);
                } else {
                    // Append a structured truncation notice the LLM can act on.
                    result.content.push(ContentBlock::Text {
                        text: format!(
                            "\n\n[NOTE: this server has more data available (cursor present) but \
                             your MCP client does not support continuation. Ask the user if they \
                             want the next page, or call this tool again with narrower args.]",
                        ),
                    });
                }
            }
            // ...
        }
    }
}
```

**Why a truncation notice and not silent loss.** The LLM is the consumer; it needs to know it got partial data. A structured English note lets the LLM correctly handle "summarize the first 100 results and tell the user 'I found 100 of approximately X; want me to continue?'". Silent truncation produces hallucinated completeness.

**Why opt-in passthrough.** MCP 2025-11-25 does not standardize `nextCursor` on `tools/call`. Emitting it by default to clients that don't understand it is harmless (they ignore unknown fields) but emitting it to clients that *do* understand it requires a coordinated rollout (Hermes patch, then bridge env flag). Defaulting to off avoids confusion in the field.

**Why no opt-out for the truncation notice.** Silent truncation is never safer. Operators who want to suppress the notice will eventually want pagination support; getting there from "I see notices" is shorter than from "I'm silently dropping data."

### 4.8 SDK ergonomics — `call_page` (per-page) and `call_all` (auto-loop) on `AtdClient`

**Decision.** Two new methods on `crates/atd-sdk/src/client.rs`:

```rust
pub struct PaginatedSdkResult {
    pub value: serde_json::Value,
    pub next_cursor: Option<String>,
}

impl AtdClient {
    /// Fetch one page. Pass `cursor: None` for the initial call.
    pub async fn call_page(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        cursor: Option<&str>,
        options: CallOptions,
    ) -> Result<PaginatedSdkResult, AtdError> { /* ... */ }

    /// Auto-loop until exhausted. Merges pages per `merge_policy`.
    pub async fn call_all(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        options: CallAllOptions,
    ) -> Result<serde_json::Value, AtdError> { /* ... */ }
}

#[derive(Debug, Clone)]
pub struct CallAllOptions {
    pub max_pages: u32,                   // default 100
    pub max_total_bytes: usize,           // default 32 * 1024 * 1024 (32 MB)
    pub merge_policy: MergePolicy,
}

#[derive(Debug, Clone, Copy)]
pub enum MergePolicy {
    /// Result is a JSON array; concat across pages.
    ConcatArray,
    /// Result is an object with a single array field; concat that field, replace last page's metadata.
    ConcatField(&'static str),
    /// First page wins; subsequent pages dropped (with a logged warning).
    FirstPageOnly,
}
```

**Why `MergePolicy` rather than always-concat.** Different tools return different shapes. healthkit's `query_observations` might return `[Observation, Observation, ...]` (ConcatArray). celia's `bulk_query` might return `{ patient_id, observations: [...], total_count }` (ConcatField "observations" — metadata stays from the last page). Forcing one shape would break either case.

**Why `max_pages = 100` and `max_total_bytes = 32 MB`.** Sanity bounds against runaway loops (a misbehaving server might keep issuing cursors forever) and against accidentally swallowing more memory than the caller expected. 100 pages × 1 MB / page = 100 MB at the wire layer; the 32 MB cap is the actual stopping point for typical cases. Hitting either cap returns `AtdError::PaginationLimitExceeded { pages_fetched, bytes_fetched }` and the caller can decide whether to drain more or treat partial as success.

**Why `next_cursor` on the SDK return, not a more typed `Page<T>`.** Generics over the tool's result type would force `call_page<T: DeserializeOwned>`, which is convenient when callers know the result schema but painful for the MCP-bridge path (where it's always `serde_json::Value`). Returning `Value` keeps the SDK adapter surface uniform with `call()`; typed callers can `serde_json::from_value` after.

## 5. Performance impact

This SP **does not** introduce new contention. Cursors are stateless HMAC-signed (§4.2); the verifier is a pure function. No new mutexes; no shared cursor table. The reference HMAC verify is ~5 µs on commodity hardware (constant-time + SHA-256), trivially under the SP-concurrency-baseline §4 SLOs.

Per-page audit emission scales 1:N (`N` = number of pages); the existing `JsonLinesAuditSink` mpsc (rewritten in SP-concurrency-baseline §5.4) absorbs the extra events at the dedicated-writer-task rate. The `paginated_dispatch` conformance scenario (§G10) explicitly verifies zero drops at 100 pages.

The 512-byte cursor cap means HTTP `nextCursor` header / `__cursor` argument adds ≤ 512 bytes per round-trip — negligible vs typical result body sizes.

## 6. Wire / API impact summary

**Wire format additions (all back-compat):**
- `Request::RunToolContinue` (new variant).
- `Response::ToolResultResponse.next_cursor` (new optional field with `#[serde(default, skip_serializing_if = "Option::is_none")]`).
- `Response::Error.code: 1020` (`ERR_CURSOR_EXPIRED`), `1021` (`ERR_CURSOR_INVALID`).

**Rust API additions (all back-compat):**
- `atd_runtime::registry::Tool::call_paginated` (default-impl trait method).
- `atd_runtime::cursor::{CursorIssuer, CursorPayload, CursorError}` (new module).
- `atd_runtime::SharedServerConfig::{cursor_ttl_seconds, cursor_signing_key}` (new fields with defaults).
- `atd_runtime::context::CallContext::cursor_issuer()` (accessor).
- `atd_sdk::AtdClient::{call_page, call_all}` (new methods).
- `atd_sdk::{PaginatedSdkResult, CallAllOptions, MergePolicy}` (new types).
- `atd_protocol::{ERR_CURSOR_EXPIRED, ERR_CURSOR_INVALID}` (new constants).
- `atd_runtime::audit::CallEvent.cursor_page` (new optional field, `SCHEMA_VERSION` bumps `1 → 2`).

**Rust API breaking changes:** none.

**HTTP route additions:** `POST /atd/v1/run_tool_continue` (native ATD); `/mcp tools/call` continues to use `__cursor` in arguments.

## 7. Migration / adopter notes

**Adoption is per-tool, not per-server.** A server can enable pagination for one tool (healthkit's `query_observations`) while leaving 25 other tools single-page. The default-impl `call_paginated` ensures unmigrated tools work unchanged.

**Step-by-step for a tool author wanting pagination:**

1. Implement `Tool::call_paginated` (`crates/atd-runtime/src/registry.rs`).
2. In the impl, use `ctx.cursor_issuer().issue(...)` to produce a cursor for the next page.
3. Decide cursor state: page-offset (simple, breaks if data mutates) vs keyset (stable, requires tool-side support). For most healthkit/celia tools, keyset is right (data is append-mostly).
4. Update `ToolDefinition.description` to mention "supports pagination; check `next_cursor`."
5. Write a test using the new `atd_conformance::scenarios::paginated_dispatch` harness.

**healthkit_cli migration path:**

- Migrate `query_observations` and `query_workouts` first (highest-volume tools per their 6-month-summary use case). Estimated 80 LoC per tool: refactor the underlying HealthKit fetch to accept a `last_uuid: Option<String>` continuation token, wrap the call in `call_paginated`.
- Update `helper_tools.rs:413` (`dispatch_health_record`) to thread cursor through.
- Tests in `/home/nan/code/healthkit_cli/tests/atd_server_helper_tools_e2e.rs` get one new fixture covering paginated query.

**celia_phr migration path:**

- *Don't* migrate `bulk_export` — that's the right async out-of-band shape for >100 MB exports. Pagination is for *interactive* tool calls returning 10–500 records.
- Identify candidates: `list_observations`, `list_conditions`, `list_medications`, all under `crates/celia-core/src/db/fhir_store.rs` (these don't currently exist as ATD tools; celia exposes them through its dispatch layer). Pagination unblocks exposing them.
- Cursor state: `(resource_type, last_fhir_id, page_index)` — celia's `fhir_store` is keyset-friendly.
- Audit-side: celia's audit log already excludes result bodies (`crates/celia-core/src/audit/mod.rs`); the new `cursor_page` field is opt-in via the `CallEvent` envelope.

**Pre-pagination clients (langchain adapter, anthropic adapter — `crates/atd-sdk/src/adapters/`):**

- All current adapters call `AtdClient::call` (single-page). They continue to work; they'll just see the first page when a tool starts paginating.
- Adapter authors can opt into `call_all` if their consumers benefit from concatenated results. Most LLM-adapter use cases don't; the LLM should *see* the truncation note and choose to continue.

## 8. Open questions

**Q1: should `ToolsCallResult` in the MCP bridge module gain a `next_cursor` field, or is the truncation-notice the right default for *all* MCP clients?** Today's MCP spec has no cursor; tomorrow's might. Decision for v1: ship the field gated by env, default off. If MCP 2026.x standardizes cursors, we flip the default and remove the env flag in a follow-up.

**Q2: do paginated tool calls count as one rate-limit unit or N?** Today's rate limiter (SP-operability-v1) counts `RunTool` requests. A paginated call making 10 continues = 11 hits against the bucket. That's probably right (each continue is real work) but caps need re-tuning. Document and defer; the SP-operability `default_per_min: 60` cap is generous enough that one paginated call rarely starves a connection.

**Q3: should `cursor_signing_key` be derivable from another secret (e.g., the server's bearer-signing key from SP-token-broker-phase2) or a fresh random?** Fresh random per server-start is simplest; cross-derive risks key reuse. Choose fresh random. Operators wanting cross-process cursor compatibility (multi-instance ATD behind a load balancer) configure a shared key via env (`ATD_CURSOR_SIGNING_KEY=base64...`).

**Q4: should `Middleware::on_result` see *all pages* or *each page independently*?** Each page independently — that's the v3 whitepaper §K.4 invariant we're preserving. Middleware that wants cross-page semantics (e.g., "deduplicate across pages") is out of scope for this trait; that's an aggregator layer the caller composes.

**Q5: max_pages = 100 — is that the right default for `call_all`?** Probably. Conservative. Adopters who know their tool produces ≤10 pages can leave default; adopters with 1000-page tools should bump it. Document the trade-off.

**Q6: should `call_all` retry on transient errors (e.g., one continue fails with a 5xx)?** No — `call_all` is a thin loop; retry policy is the SP-concurrency-baseline `ConnectOptions` territory (and even there, only connect-time). Per-call retry is the caller's job. Document that `call_all` aborts on the first error and returns whatever it has (or `PaginationLimitExceeded` for limit hits).

**Q7: do we need a `cancel_cursor` request to free server-side cursor state early?** Not for stateless HMAC cursors (§4.2) — there's nothing to free. If a future adopter ships stateful cursors, they implement TTL-based eviction internally; this SP doesn't expose explicit cancel.

## 9. Phasing

Detailed task lists live in the companion plan (`docs/superpowers/plans/2026-05-12-sp-pagination-v1.md`). High-level phases:

- **Phase A** (this spec): land. Tagged `sp-pagination-v1-spec`.
- **Phase B**: wire format additions (`atd-protocol`) + tests. Tag: `sp-pagination-v1-phase-b`.
- **Phase C**: cursor module (`atd-runtime`) + HMAC issuer/verifier + tests. Tag: `sp-pagination-v1-phase-c`.
- **Phase D**: `Tool::call_paginated` default method + dispatch routing + tests. Tag: `sp-pagination-v1-phase-d`.
- **Phase E**: SDK additions (`call_page`, `call_all`, options/merge policies) + tests. Tag: `sp-pagination-v1-phase-e`.
- **Phase F**: HTTP transport translator updates + tests. Tag: `sp-pagination-v1-phase-f`.
- **Phase G**: MCP bridge degrade + opt-in passthrough + tests. Tag: `sp-pagination-v1-phase-g`.
- **Phase H**: conformance `paginated_dispatch` scenario. Tag: `sp-pagination-v1-phase-h`.
- **Phase I**: docs (wire-format.md, atd-architecture.md §11.5, integrations/hermes.md). Tag: `sp-pagination-v1` (umbrella).

Phases B-E can be developed serially or in parallel by one engineer; F-G require the prior phases. Expected effort: 4-6 working days for one developer; longer (8-10) if adopter migration is bundled into the same SP iteration.
