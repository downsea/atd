# SP-pagination-v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add protocol-level result pagination to ATD as an additive, fully back-compat primitive. New wire variants: `Request::RunToolContinue` + `Response::ToolResultResponse.next_cursor` + error codes `1020`/`1021`. New tool author API: `Tool::call_paginated` default method. New SDK ergonomics: `AtdClient::call_page` (per-page) + `AtdClient::call_all` (auto-loop). New runtime helper: `atd_runtime::cursor::CursorIssuer` (HMAC-SHA256-signed, stateless, 512-byte cap). MCP bridge degrades to single-page with structured truncation notice by default; HTTP transport surfaces cursor via `__cursor` argument. `atd-conformance` `paginated_dispatch` scenario gates the whole thing.

**Adopters:**
- **healthkit_cli** — primary migration target; `query_observations` and `query_workouts` migrate to `call_paginated` so 6-month summary queries don't blow the 1MB output budget. ~80 LoC per tool.
- **celia_phr** — opens new tool surface (`list_observations`, `list_conditions`, `list_medications`) that was previously blocked on lack of pagination. Existing `bulk_export` is unchanged (different shape — async out-of-band; not paginated dispatch).

**Architecture:** Six-axis intervention. (1) `atd-protocol`: add `RunToolContinue` request variant + `next_cursor` field + error codes. (2) `atd-runtime::cursor`: new module with `CursorIssuer` (HMAC-SHA256 over CBOR-encoded `CursorPayload`). (3) `atd-runtime::registry::Tool`: add `call_paginated` default trait method; dispatch routes `RunToolContinue` here. (4) `atd-sdk`: add `call_page`, `call_all`, `CallAllOptions`, `MergePolicy`. (5) `atd-server-http::mcp`: detect `__cursor` in args, surface `nextCursor` in result. (6) `atd-mcp-bridge::bridge`: append truncation notice or passthrough cursor via env flag. Conformance scenario validates 11-page round-trip.

**Tech Stack:** Rust 2021 (workspace). New deps: `ciborium` (CBOR encoding of `CursorPayload`, 50KB compiled), `hmac` + `sha2` (already present via other crates; reuse). No new transitive surface for adopters beyond runtime-internal use.

**Spec:** [`../specs/2026-05-12-sp-pagination-v1-design.md`](../specs/2026-05-12-sp-pagination-v1-design.md) — refer to spec §-numbers throughout this plan.

**Sequencing:** Wire variants (Phase B) → cursor module (Phase C) → dispatch + tool trait (Phase D) → SDK ergonomics (Phase E) → HTTP transport (Phase F) → MCP bridge (Phase G) → conformance scenario (Phase H) → docs + tag (Phase I). Phases B/C are independent and can land in parallel; D depends on both; E/F/G can land in any order after D.

---

## Phase B — Wire-format additions

### Task 1: Add `Request::RunToolContinue` + `Response::ToolResultResponse.next_cursor`

**Files:**
- Modify: `crates/atd-protocol/src/messages.rs` (new variant + new field)
- Modify: `crates/atd-protocol/src/lib.rs` (new error code constants)

- [ ] **Step 1: Add request variant**

In `messages.rs` `Request` enum (after the existing `RunTool` arm at line 80):

```rust
#[serde(rename = "run_tool_continue")]
RunToolContinue {
    tool_id: String,
    cursor: String,
},
```

- [ ] **Step 2: Add response field**

In `messages.rs` `Response::ToolResultResponse` (currently lines 113-118):

```rust
#[serde(rename = "tool_result")]
ToolResultResponse {
    tool_id: String,
    result: serde_json::Value,
    success: bool,
    dry_run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
},
```

- [ ] **Step 3: Add error code constants**

In `lib.rs` (after the existing 1010-1013 codes):

```rust
pub const ERR_CURSOR_EXPIRED: u16 = 1020;
pub const ERR_CURSOR_INVALID: u16 = 1021;
```

- [ ] **Step 4: Round-trip tests**

In `messages.rs::tests` add:
- `run_tool_continue_round_trips` — serialize `Request::RunToolContinue { tool_id: "x", cursor: "abc" }` → parse → assert variant match.
- `tool_result_response_without_next_cursor_omits_field` — serialize response with `next_cursor: None`; assert JSON does NOT contain `"next_cursor"` key.
- `tool_result_response_with_next_cursor_includes_field` — serialize with `Some("abc")`; assert JSON contains `"next_cursor":"abc"`.
- `tool_result_response_back_compat_default_when_missing` — parse `{"type":"tool_result","tool_id":"x","result":{},"success":true,"dry_run":false}`; assert `next_cursor == None`.

- [ ] **Step 5: Wire format docs**

Update `docs/protocol/wire-format.md`:
- Add `run_tool_continue` to the request table.
- Add `next_cursor` row to the `tool_result` response table.
- Add codes 1020/1021 to the error code table.

- [ ] **Step 6: Commit**

```
feat(atd-protocol): RunToolContinue + next_cursor + ERR_CURSOR_* (SP-pagination-v1 §4.1)
```

---

## Phase C — `atd_runtime::cursor` HMAC issuer + verifier

### Task 2: New cursor module

**Files:**
- Create: `crates/atd-runtime/src/cursor.rs`
- Modify: `crates/atd-runtime/src/lib.rs` (export)
- Modify: `crates/atd-runtime/Cargo.toml` (add `ciborium`, ensure `hmac` + `sha2` accessible)
- Modify: `crates/atd-runtime/src/dispatch.rs` (add `cursor_signing_key` + `cursor_ttl_seconds` to `SharedServerConfig`)

- [ ] **Step 1: Add deps**

```toml
ciborium = "0.2"
hmac = "0.12"
sha2 = "0.10"
base64 = "0.22"
```

- [ ] **Step 2: Define types per spec §4.5**

`crates/atd-runtime/src/cursor.rs` skeleton:

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;
use serde::{Deserialize, Serialize};
use base64::Engine;

type HmacSha256 = Hmac<Sha256>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CursorPayload {
    pub tool_id: String,
    pub caller_id: Option<String>,
    pub args_fingerprint: [u8; 32],
    pub page_index: u32,
    pub issued_at_unix: u64,
    pub server_session: [u8; 8],
    #[serde(default, with = "serde_bytes")]
    pub opaque_state: Vec<u8>,
}

pub struct CursorIssuer {
    key: [u8; 32],
    session_nonce: [u8; 8],
}

#[derive(thiserror::Error, Debug)]
pub enum CursorError {
    #[error("cursor expired")]            Expired,
    #[error("cursor signature invalid")]  InvalidSignature,
    #[error("cursor format invalid: {0}")] Format(String),
    #[error("cursor too large: {0} bytes (max 512)")] TooLarge(usize),
    #[error("opaque_state too large: {0} bytes (max 256)")] OpaqueStateTooLarge(usize),
}

impl CursorIssuer {
    pub fn new(key: [u8; 32]) -> Self {
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce).expect("OS RNG");
        Self { key, session_nonce: nonce }
    }
    pub fn session_nonce(&self) -> [u8; 8] { self.session_nonce }
    pub fn issue(&self, payload: CursorPayload) -> Result<String, CursorError> { /* impl */ }
    pub fn verify(&self, cursor: &str, ttl_seconds: u64) -> Result<CursorPayload, CursorError> { /* impl */ }
}
```

- [ ] **Step 3: Implement `issue`**

```rust
pub fn issue(&self, payload: CursorPayload) -> Result<String, CursorError> {
    if payload.opaque_state.len() > 256 {
        return Err(CursorError::OpaqueStateTooLarge(payload.opaque_state.len()));
    }
    let mut body = Vec::with_capacity(256);
    ciborium::into_writer(&payload, &mut body).map_err(|e| CursorError::Format(e.to_string()))?;
    let mut mac = HmacSha256::new_from_slice(&self.key).expect("hmac key");
    mac.update(&body);
    let tag = mac.finalize().into_bytes();
    let mut combined = body;
    combined.extend_from_slice(&tag);
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&combined);
    if encoded.len() > 512 {
        return Err(CursorError::TooLarge(encoded.len()));
    }
    Ok(encoded)
}
```

- [ ] **Step 4: Implement `verify`**

```rust
pub fn verify(&self, cursor: &str, ttl_seconds: u64) -> Result<CursorPayload, CursorError> {
    if cursor.len() > 512 {
        return Err(CursorError::TooLarge(cursor.len()));
    }
    let combined = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(cursor)
        .map_err(|e| CursorError::Format(e.to_string()))?;
    if combined.len() < 32 {
        return Err(CursorError::Format("missing HMAC tag".into()));
    }
    let (body, tag) = combined.split_at(combined.len() - 32);
    let mut mac = HmacSha256::new_from_slice(&self.key).expect("hmac key");
    mac.update(body);
    mac.verify_slice(tag).map_err(|_| CursorError::InvalidSignature)?;
    let payload: CursorPayload = ciborium::from_reader(body).map_err(|e| CursorError::Format(e.to_string()))?;
    if payload.server_session != self.session_nonce {
        return Err(CursorError::Expired); // restart-invalidated
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    if now.saturating_sub(payload.issued_at_unix) > ttl_seconds {
        return Err(CursorError::Expired);
    }
    Ok(payload)
}
```

- [ ] **Step 5: `SharedServerConfig` fields**

```rust
pub cursor_signing_key: [u8; 32],  // default: getrandom() at config-build time
pub cursor_ttl_seconds: u64,        // default 300
```

Update `for_test()` (`dispatch.rs:91-103`) to populate.

- [ ] **Step 6: Tests**

In `cursor.rs::tests`:
- `issue_round_trips` — issue a payload, verify, assert equality.
- `verify_rejects_tampered_body` — issue, flip one bit in the decoded body, re-encode, verify → `InvalidSignature`.
- `verify_rejects_after_ttl` — issue with `issued_at_unix = now - 400`, ttl=300, verify → `Expired`.
- `verify_rejects_wrong_session` — issue with issuer A, verify with issuer B (different `session_nonce`) → `Expired`.
- `issue_caps_oversized_opaque` — payload with 300-byte opaque_state → `OpaqueStateTooLarge`.
- `issue_caps_oversized_total` — payload with a tool_id of 600 chars → `TooLarge`.

- [ ] **Step 7: Commit**

```
feat(atd-runtime): cursor::CursorIssuer + CursorPayload (SP-pagination-v1 §4.5)
```

---

## Phase D — `Tool::call_paginated` trait method + dispatch routing

### Task 3: Extend `Tool` trait with default-impl `call_paginated`

**Files:**
- Modify: `crates/atd-runtime/src/registry.rs` (trait extension)
- Modify: `crates/atd-runtime/src/dispatch.rs` (route `RunTool` + `RunToolContinue` through `call_paginated`)
- Modify: `crates/atd-runtime/src/context.rs` (`CallContext::cursor_issuer` accessor; thread `CursorIssuer` through dispatch)

- [ ] **Step 1: Add `PaginatedResult` + `PaginatedCallFuture`**

In `registry.rs`:

```rust
#[derive(Debug)]
pub struct PaginatedResult {
    pub value: serde_json::Value,
    pub next_cursor: Option<String>,
}

pub type PaginatedCallFuture<'a> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<PaginatedResult, crate::error::ToolCallError>> + Send + 'a>>;
```

- [ ] **Step 2: Extend `Tool` trait**

```rust
pub trait Tool: Send + Sync {
    fn definition(&self) -> &ToolDefinition;
    fn call<'a>(&'a self, args: serde_json::Value, ctx: &'a CallContext) -> CallFuture<'a>;

    fn call_paginated<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a CallContext,
        _cursor: Option<&'a str>,
    ) -> PaginatedCallFuture<'a> {
        let fut = self.call(args, ctx);
        Box::pin(async move {
            let value = fut.await?;
            Ok(PaginatedResult { value, next_cursor: None })
        })
    }
}
```

The default impl ignores `_cursor` — tools that don't override never see continuation calls (dispatch returns `ERR_CURSOR_INVALID` if a non-paginated tool gets a `RunToolContinue`; see Step 4).

- [ ] **Step 3: Route `RunTool` through `call_paginated`**

In `dispatch.rs::dispatch_request` `Request::RunTool` arm: instead of calling `run_tool(...)` which goes through `call`, call a new `run_tool_paginated(state, tracker, tool_id, args, dry_run, ctx, cursor=None)` that returns `(value, next_cursor)`. Build `Response::ToolResultResponse { ..., next_cursor }`.

- [ ] **Step 4: Add `RunToolContinue` arm**

```rust
Request::RunToolContinue { tool_id, cursor } => {
    let issuer = CursorIssuer::new(state.config.cursor_signing_key);
    let payload = match issuer.verify(&cursor, state.config.cursor_ttl_seconds) {
        Ok(p) => p,
        Err(CursorError::Expired) => return Response::Error {
            message: "cursor expired; re-issue the original RunTool".into(),
            code: Some(atd_protocol::ERR_CURSOR_EXPIRED), retryable: Some(false),
        },
        Err(_) => return Response::Error {
            message: "cursor invalid".into(),
            code: Some(atd_protocol::ERR_CURSOR_INVALID), retryable: Some(false),
        },
    };
    if payload.tool_id != tool_id {
        return Response::Error {
            message: "cursor tool_id mismatch".into(),
            code: Some(atd_protocol::ERR_CURSOR_INVALID), retryable: Some(false),
        };
    }
    // Recover args from payload's args_fingerprint? No — args fingerprint
    // is integrity, not storage. The tool's call_paginated impl must be
    // stateless w.r.t. args; only the cursor carries state. So we pass
    // `args = json!(null)` and the tool reads its state from cursor only.
    // (For tools that need args replay, embed in opaque_state.)
    let (value, next_cursor) = run_tool_paginated(state, tracker, &tool_id, serde_json::Value::Null, false, ctx, Some(&cursor)).await?;
    Response::ToolResultResponse { tool_id, result: value, success: true, dry_run: false, next_cursor }
}
```

- [ ] **Step 5: Wire `cursor_issuer` into `CallContext`**

Add `pub cursor_issuer: Option<Arc<CursorIssuer>>` to `CallContext`. Dispatch builds it from `state.config.cursor_signing_key` per request and passes through.

- [ ] **Step 6: Tests**

In `dispatch.rs::tests`:
- `paginated_tool_returns_value_with_cursor` — register a `PageStub` that returns `value=[0,1,2], next_cursor=Some("abc")`; dispatch `RunTool`; assert response carries cursor.
- `non_paginated_tool_returns_no_cursor` — existing `EchoStub`; dispatch `RunTool`; assert `next_cursor == None`.
- `run_tool_continue_routes_through_call_paginated` — register `PageStub` that returns different values per page; dispatch `RunToolContinue { cursor: <issued cursor> }`; assert page 2 value.
- `run_tool_continue_rejects_expired_cursor` — set TTL = 1s, sleep 2s, continue → `ERR_CURSOR_EXPIRED`.
- `run_tool_continue_rejects_tampered_cursor` — flip a base64 character; assert `ERR_CURSOR_INVALID`.
- `run_tool_continue_rejects_cross_tool_cursor` — issue for tool A, continue with tool B → `ERR_CURSOR_INVALID`.

- [ ] **Step 7: Update `CallEvent` schema**

In `crates/atd-runtime/src/audit.rs`:
- Add `pub cursor_page: Option<u32>` to `CallEvent`.
- Bump `SCHEMA_VERSION` from 1 to 2.
- Update existing test fixtures (the audit.rs tests at lines 122-150) to include the new field.

- [ ] **Step 8: Commit**

```
feat(atd-runtime): Tool::call_paginated + RunToolContinue dispatch (SP-pagination-v1 §4.4)
```

---

## Phase E — SDK additions: `call_page`, `call_all`, `MergePolicy`

### Task 4: SDK ergonomics

**Files:**
- Modify: `crates/atd-sdk/src/client.rs` (new methods)
- Modify: `crates/atd-sdk/src/options.rs` (new options types)
- Modify: `crates/atd-sdk/src/lib.rs` (re-exports)
- Modify: `crates/atd-sdk/src/error.rs` (add `PaginationLimitExceeded` variant)

- [ ] **Step 1: Types**

In `options.rs`:

```rust
#[derive(Debug, Clone)]
pub struct CallAllOptions {
    pub max_pages: u32,
    pub max_total_bytes: usize,
    pub merge_policy: MergePolicy,
}

impl Default for CallAllOptions {
    fn default() -> Self {
        Self { max_pages: 100, max_total_bytes: 32 * 1024 * 1024, merge_policy: MergePolicy::ConcatArray }
    }
}

#[derive(Debug, Clone)]
pub enum MergePolicy {
    ConcatArray,
    ConcatField(String),
    FirstPageOnly,
}

#[derive(Debug)]
pub struct PaginatedSdkResult {
    pub value: serde_json::Value,
    pub next_cursor: Option<String>,
}
```

- [ ] **Step 2: `call_page`**

```rust
pub async fn call_page(
    &self,
    tool_id: &str,
    args: serde_json::Value,
    cursor: Option<&str>,
    options: CallOptions,
) -> Result<PaginatedSdkResult, AtdError> {
    let req = match cursor {
        None => Request::RunTool { tool_id: tool_id.into(), args, dry_run: options.dry_run },
        Some(c) => Request::RunToolContinue { tool_id: tool_id.into(), cursor: c.into() },
    };
    match self.request(&req).await? {
        Response::ToolResultResponse { result, success, next_cursor, .. } if success => {
            Ok(PaginatedSdkResult { value: result, next_cursor })
        }
        Response::ToolResultResponse { result, .. } => {
            Err(AtdError::ToolExecutionFailed { /* parse from result */ })
        }
        Response::Error { message, code, .. } => Err(AtdError::ProtocolError {
            expected: "tool_result".into(),
            got: format!("error code={code:?} message={message}"),
        }),
        other => Err(AtdError::ProtocolError { expected: "tool_result".into(), got: format!("{other:?}") }),
    }
}
```

- [ ] **Step 3: `call_all`**

```rust
pub async fn call_all(
    &self,
    tool_id: &str,
    args: serde_json::Value,
    options: CallAllOptions,
) -> Result<serde_json::Value, AtdError> {
    let mut accumulated: Option<serde_json::Value> = None;
    let mut bytes = 0usize;
    let mut cursor: Option<String> = None;
    for page_idx in 0..options.max_pages {
        let page = self.call_page(tool_id, if page_idx == 0 { args.clone() } else { serde_json::Value::Null }, cursor.as_deref(), CallOptions::default()).await?;
        let page_bytes = serde_json::to_vec(&page.value).map(|v| v.len()).unwrap_or(0);
        bytes += page_bytes;
        if bytes > options.max_total_bytes {
            return Err(AtdError::PaginationLimitExceeded {
                pages_fetched: page_idx + 1,
                bytes_fetched: bytes,
            });
        }
        accumulated = Some(merge(accumulated, page.value, &options.merge_policy)?);
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => return Ok(accumulated.unwrap()),
        }
    }
    Err(AtdError::PaginationLimitExceeded {
        pages_fetched: options.max_pages,
        bytes_fetched: bytes,
    })
}

fn merge(acc: Option<Value>, page: Value, policy: &MergePolicy) -> Result<Value, AtdError> {
    match (acc, policy) {
        (None, _) => Ok(page),
        (Some(a), MergePolicy::ConcatArray) => match (a, page) {
            (Value::Array(mut va), Value::Array(vb)) => { va.extend(vb); Ok(Value::Array(va)) }
            _ => Err(AtdError::MergeFailed { reason: "ConcatArray requires Array-typed pages".into() }),
        },
        (Some(Value::Object(mut a)), MergePolicy::ConcatField(field)) => {
            if let (Some(Value::Array(va)), Some(Value::Array(vb))) = (a.get_mut(field).map(Value::take), page.get(field).cloned()) {
                let mut combined = va; combined.extend(match vb { Value::Array(v) => v, _ => return Err(AtdError::MergeFailed { reason: format!("field {field} not Array") }) });
                let mut obj = if let Value::Object(o) = page { o } else { return Err(AtdError::MergeFailed { reason: "page not Object".into() }); };
                obj.insert(field.clone(), Value::Array(combined));
                Ok(Value::Object(obj))
            } else { Err(AtdError::MergeFailed { reason: "ConcatField missing target field".into() }) }
        }
        (Some(a), MergePolicy::FirstPageOnly) => Ok(a),
        _ => Err(AtdError::MergeFailed { reason: "policy mismatch".into() }),
    }
}
```

- [ ] **Step 4: Tests**

In `client.rs::tests`:
- `call_page_initial_returns_value_and_cursor` — fake server returns `tool_result` with `next_cursor: "abc"`; assert SDK propagates both.
- `call_page_with_cursor_sends_run_tool_continue` — assert wire shape: request type is `run_tool_continue`.
- `call_all_concats_arrays_until_no_cursor` — fake server returns 3 pages of `[1,2]/[3,4]/[5,6]` with cursors then None; assert result is `[1,2,3,4,5,6]`.
- `call_all_respects_max_pages` — server always returns cursor; assert PaginationLimitExceeded after `max_pages` attempts.
- `call_all_respects_max_total_bytes` — server returns 5MB pages; `max_total_bytes = 10MB`; assert error after 2 pages.
- `call_all_concats_field_for_object_pages` — server returns `{ patient: "x", obs: [a,b], total: 4 }` then `{ patient: "x", obs: [c,d], total: 4 }`; with `ConcatField("obs")`, assert `obs: [a,b,c,d], total: 4` (last-page metadata wins).

- [ ] **Step 5: Commit**

```
feat(atd-sdk): call_page + call_all + MergePolicy (SP-pagination-v1 §4.8)
```

---

## Phase F — HTTP transport: surface cursor via MCP `__cursor`

### Task 5: `atd-server-http::mcp` translator

**Files:**
- Modify: `crates/atd-server-http/src/mcp.rs` (detect `__cursor`, surface `nextCursor`)
- Modify: `crates/atd-server-http/src/server.rs` (add `/atd/v1/run_tool_continue` route)

- [ ] **Step 1: Detect `__cursor` in `tools/call`**

In the existing `tools/call` handler, before dispatching:

```rust
let cursor = arguments.get("__cursor").and_then(|v| v.as_str()).map(String::from);
let mut args = arguments.clone();
if let Value::Object(ref mut m) = args { m.remove("__cursor"); }
let req = match cursor {
    None => Request::RunTool { tool_id, args, dry_run: false },
    Some(c) => Request::RunToolContinue { tool_id, cursor: c },
};
```

- [ ] **Step 2: Surface `nextCursor` in MCP result**

After dispatch returns `Response::ToolResultResponse { next_cursor, .. }`, attach to the MCP result envelope:

```json
{ "content": [...], "isError": false, "nextCursor": "abc" }
```

When `next_cursor.is_none()`, omit the field.

- [ ] **Step 3: Native ATD route `/atd/v1/run_tool_continue`**

Mirror the existing `/atd/v1/run_tool` handler; accept `{ tool_id, cursor }`, dispatch `RunToolContinue`, return the same shape.

- [ ] **Step 4: Tests**

In `crates/atd-server-http/tests/`:
- `mcp_tools_call_with_cursor_routes_to_run_tool_continue` — start server with PaginatedStub; first call returns cursor; second call with `arguments.__cursor` returns next page.
- `mcp_tools_call_omits_next_cursor_field_when_none` — assert JSON does NOT contain `"nextCursor"` key when null.
- `atd_v1_run_tool_continue_route_returns_paginated_result` — same coverage on the native route.

- [ ] **Step 5: Commit**

```
feat(atd-server-http): surface pagination via __cursor / nextCursor (SP-pagination-v1 §4.6)
```

---

## Phase G — MCP bridge: degrade-or-passthrough

### Task 6: `atd-mcp-bridge::bridge` cursor handling

**Files:**
- Modify: `crates/atd-mcp-bridge/src/bridge.rs` (cursor branch in `handle_tools_call`)
- Modify: `crates/atd-mcp-bridge/src/mcp.rs` (extend `ToolsCallResult` with optional `next_cursor`)

- [ ] **Step 1: Extend `ToolsCallResult`**

```rust
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCallResult {
    pub content: Vec<ContentBlock>,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}
```

- [ ] **Step 2: Update `handle_tools_call`**

Switch from `client.call(...)` to `client.call_page(...)`. Detect `__cursor` in `params.arguments` (extract and remove). On result:

```rust
let passthrough = std::env::var("ATD_MCP_PASSTHROUGH_CURSOR").as_deref() == Ok("1");
let mcp_result = match result {
    Ok(PaginatedSdkResult { value, next_cursor: Some(cur) }) if passthrough => {
        ToolsCallResult { content: vec![text_block(&value)], is_error: false, next_cursor: Some(cur) }
    }
    Ok(PaginatedSdkResult { value, next_cursor: Some(_cur) }) => {
        let mut blocks = vec![text_block(&value)];
        blocks.push(ContentBlock::Text {
            text: "\n\n[NOTE: this server has more data available (cursor present) but your MCP client does not support continuation. Ask the user if they want the next page, or call this tool again with narrower args.]".into()
        });
        ToolsCallResult { content: blocks, is_error: false, next_cursor: None }
    }
    Ok(PaginatedSdkResult { value, next_cursor: None }) => {
        ToolsCallResult { content: vec![text_block(&value)], is_error: false, next_cursor: None }
    }
    Err(_) => { /* existing error path */ }
};
```

- [ ] **Step 3: Tests**

In `bridge.rs::tests`:
- `tools_call_appends_truncation_notice_when_cursor_present_default` — fake server returns cursor; assert bridge response has 2 content blocks (data + notice).
- `tools_call_passthrough_cursor_when_env_set` — set `ATD_MCP_PASSTHROUGH_CURSOR=1`; same fixture; assert single content block + `nextCursor` field present.
- `tools_call_detects_dunder_cursor_argument` — send `{"name":"x","arguments":{"__cursor":"abc","other":"val"}}`; fake server asserts it received `run_tool_continue` with cursor "abc" and args `{"other":"val"}` (no `__cursor`).
- `tools_call_no_cursor_path_unchanged` — non-paginated fixture; assert single content block, no notice, no nextCursor.

- [ ] **Step 4: Commit**

```
feat(atd-mcp-bridge): degrade-or-passthrough cursor handling (SP-pagination-v1 §4.7)
```

---

## Phase H — Conformance scenario

### Task 7: `paginated_dispatch` scenario

**Files:**
- Create: `crates/atd-conformance/src/scenarios/paginated_dispatch.rs`
- Create: `crates/atd-conformance/src/fixtures/page_generator.rs` (100-row generator stub tool)
- Modify: `crates/atd-conformance/src/scenarios/mod.rs` (register)

- [ ] **Step 1: Implement the generator tool**

`PageGenerator` tool: returns `value: Value::Array(0..10 of i64)` per page, cursor encodes `next_offset`. Stops at offset 100 with `next_cursor: None`.

- [ ] **Step 2: Scenario impl**

```rust
pub async fn paginated_dispatch(client: AtdClient, ...) -> ConformanceReport {
    // Call 1: initial
    let p1 = client.call_page("conformance:page_generator", json!({}), None, CallOptions::default()).await?;
    assert_eq!(p1.value.as_array().unwrap().len(), 10);
    assert!(p1.next_cursor.is_some());
    let mut cursor = p1.next_cursor.unwrap();
    let mut all_pages = vec![p1.value];
    // 9 continues
    for _ in 0..9 {
        let p = client.call_page("conformance:page_generator", json!(null), Some(&cursor), CallOptions::default()).await?;
        all_pages.push(p.value);
        cursor = p.next_cursor.expect("page should have cursor");
    }
    // 11th continue — last, should have no cursor
    let p11 = client.call_page("conformance:page_generator", json!(null), Some(&cursor), CallOptions::default()).await?;
    assert!(p11.next_cursor.is_none());
    // Cursor-expired check: re-use the page-2 cursor after waiting beyond TTL
    // (or with a fixture configured for TTL=1s)
    // ...
    ConformanceReport { ... }
}
```

- [ ] **Step 3: Cursor-expired sub-test**

Spawn a server with `cursor_ttl_seconds = 1`. Issue cursor, wait 2s, continue → assert `ERR_CURSOR_EXPIRED`.

- [ ] **Step 4: Audit-event count**

Snapshot `Server::metrics_snapshot()` before + after; assert `audit_events_total` grew by 11.

- [ ] **Step 5: Commit**

```
test(atd-conformance): paginated_dispatch scenario (SP-pagination-v1 §G10)
```

---

## Phase I — Docs + tag

### Task 8: Wire-format + architecture + integrations docs

**Files:**
- Modify: `docs/protocol/wire-format.md` (already touched in Phase B; expand the cursor lifecycle section)
- Modify: `docs/architecture.md` (add §11.5 "Large results & pagination" — depends on SP-concurrency-baseline landing §11 first)
- Modify: `docs/integrations/hermes.md` (document `ATD_MCP_PASSTHROUGH_CURSOR` env)
- Create: `docs/adr/0003-pagination-v1.md` (one-page summary)

- [ ] **Step 1: Architecture §11.5**

Add a new subsection under SP-concurrency-baseline's §11:

```
### 11.5 Large results & pagination

ATD tool calls return one JSON `Value` per response, capped at MAX_FRAME_BYTES (10 MB).
For results that don't fit (FHIR Observation history, healthkit 6-month summaries):

- Tool author opts in: implement `Tool::call_paginated`, return `PaginatedResult { value, next_cursor }`.
- Cursors are HMAC-signed, 512-byte opaque strings; 5min default TTL.
- Wire: `Request::RunToolContinue { tool_id, cursor }`; `Response::ToolResultResponse.next_cursor: Option<String>`.
- SDK: `AtdClient::call_page` (per-page) or `AtdClient::call_all` (auto-loop with merge policy).
- HTTP: `__cursor` in args, `nextCursor` in result.
- MCP bridge: degrades to first-page-plus-notice by default; opt-in passthrough via env.

See `docs/superpowers/specs/2026-05-12-sp-pagination-v1-design.md` for full design.
```

- [ ] **Step 2: ADR-0003**

One-pager: "ATD adds pagination as protocol primitive. Tools opt in. Cursors stateless HMAC. MCP bridge degrades gracefully. Adopters: rebuild deps; new tools can use `call_paginated`; existing tools unchanged."

- [ ] **Step 3: Hermes integration doc**

Add to `docs/integrations/hermes.md`:

```
### Pagination (SP-pagination-v1)

ATD supports tool-result pagination. The bridge handles MCP-client mismatch:

- **Default:** First page only + structured notice appended to content. Compatible with all MCP clients.
- **`ATD_MCP_PASSTHROUGH_CURSOR=1`:** Bridge propagates `nextCursor` as a non-standard field in the MCP result. Use only with Hermes >= X.Y.Z (or patched clients).

For native ATD SDK callers (not via MCP), use `AtdClient::call_all` for auto-loop, or `call_page` for per-page control.
```

- [ ] **Step 4: Adopter notification (TODO comment in commit)**

healthkit_cli + celia_phr: ATD ships SP-pagination-v1. New tool API available; no breaking changes. To migrate a tool, replace `impl Tool { fn call ... }` with `impl Tool { fn call_paginated ... }`. See spec §7 "Migration / adopter notes."

- [ ] **Step 5: Final tag**

```bash
git tag sp-pagination-v1
git push origin sp-pagination-v1
```

Update `CLAUDE.md`'s "Recent SPs shipped" list with `sp-pagination-v1`.

- [ ] **Step 6: Commit**

```
docs(architecture): SP-pagination-v1 shipped — §11.5 large results
```

---

## Final acceptance criteria (echoes spec §G1-G11)

- [ ] `cargo nextest run --workspace` passes (no regression in 487+ existing tests, plus ~30 new tests from this SP).
- [ ] `cargo nextest run --test paginated_dispatch -p atd-conformance` passes: 11-page round-trip, expired cursor returns 1020, audit events = 11.
- [ ] Wire-format docs (`docs/protocol/wire-format.md`) document `run_tool_continue`, `next_cursor`, 1020/1021.
- [ ] `docs/architecture.md` §11.5 is published.
- [ ] healthkit_cli + celia_phr can rebuild against `path = atd-mvp` and pass all existing tests (no migration required; only opt-in tools change).
- [ ] git tag `sp-pagination-v1` exists and is pushed.

**Expected wall-clock effort:** 4-6 working days for one developer (in addition to SP-concurrency-baseline's 3-5 days). Total "perf-v1 iteration" budget: ~10 days for one engineer, ~7 days for two engineers working B/C and D/E in parallel.

## Cross-link: SP-concurrency-baseline

This SP composes with SP-concurrency-baseline:

- The audit-mpsc-rewrite (SP-concurrency-baseline §5.4) absorbs the 11-event-per-paginated-call audit volume without drops.
- The multi-thread runtime (SP-concurrency-baseline §5.1) lets paginated-call sessions run concurrently with single-shot dispatch without serializing through one worker.
- The `Server::metrics_snapshot()` (SP-concurrency-baseline §5.7) reports `audit_events_total` which this SP's conformance scenario uses to verify the 11-event count.

Phase ordering across the two SPs: SP-concurrency-baseline phases B-E should land first (wire deadlines, SDK retry, multi-thread, audit mpsc). SP-pagination-v1 phases B-D depend on B-D of the sibling for clean tests. Phases F-H of both can land in any order. Final tags (`sp-concurrency-baseline` and `sp-pagination-v1`) should land within the same day to mark the close of the `perf-v1` iteration.
