# SP-streamable-http: ATD HTTP transport

| Status | Draft |
| Created | 2026-05-11 |
| Author | cross-project subagent (celia_phr ↔ atd-mvp coordination) |
| Phase | ATD post-v0.3.0; Celia Phase K cut-over |
| Related | SP-7 (`2026-04-24-sp7-mcp-bridge.md`), SP-listener-extract (`2026-04-25-sp-listener-extract-design.md`), SP-token-broker-phase1 (`2026-04-27-sp-token-broker-phase1-design.md`), SP-12 (`2026-04-25-sp12-canonical-dispatch.md`), Celia `ATD_FUTURE_ISSUES.md §1.B` |

---

## 1. Motivation

**1.1 The PWA gap.** ATD v0.3.0 ships a single transport: Unix socket, length-prefixed JSON (`crates/atd-protocol/src/wire.rs:6-43`). A browser PWA cannot reach that transport — `fetch()` cannot open a Unix domain socket, cannot spawn a stdio child, cannot dial an `AF_UNIX` peer. The MCP spec (rev `2025-06-18`) introduces "Streamable HTTP" precisely to plug this hole: one HTTP endpoint that accepts JSON-RPC envelopes and optionally upgrades to an SSE stream. Without an `HttpTransport` in `atd-server`, browser-shaped agents are second-class citizens — they can never directly call ATD-registered tools.

**1.2 The remote-MCP gap.** Cursor, Claude.ai, and OpenAI Functions over HTTPS each speak MCP-over-HTTP today. The current `atd-mcp-bridge` (`crates/atd-mcp-bridge/src/main.rs`) is **stdio-only**: it expects to be spawned per session. That model breaks for serverless / multi-agent / fan-out shapes where N agents share one ATD-speaking backend. The bridge was scoped that way deliberately (SP-7 §2.1: "current 1:1 is correct for MCP"). Streamable HTTP is the second leg.

**1.3 The Celia precedent — paid for, but in the wrong repo.** `celia_phr/crates/celia-cli/src/http_server.rs` is 461 lines of working `POST /mcp` + `POST /chat/stream` + Origin gate + Bearer parse, written **outside** ATD because ATD had no HTTP transport when Celia needed one (see header comment `crates/celia-cli/src/http_server.rs:13-21`). Every future ATD adopter (`healthkit_cli`, `weather-mock`, third parties) will reinvent the same code. SP-streamable-http relocates the responsibility to where the protocol lives, so adopters get an HTTP-callable ATD server by composing `atd-server-http` + their `Registry`, just as today they compose `atd-server` + their `Registry` (see `crates/atd-server/Cargo.toml:9`).

## 2. Goals

- One process can listen on a Unix socket **and** an HTTP port simultaneously, both driving the **same** `Arc<Registry>`.
- Same authoritative wire schema for tool definitions and tool results across both transports — bytes from `Registry::dispatch` are identical regardless of which listener accepted the request.
- Browser PWA (`fetch()`) and remote MCP clients (Cursor, Claude.ai) can speak to ATD using **standard MCP Streamable HTTP** — no ATD-specific wire knowledge in the JS client.
- Origin allow-list + Bearer-token authentication enforceable at the listener boundary, before any `Registry::dispatch` call. Origin defaults are fail-closed (loopback + tauri only) per MCP §"Security Warning".
- The HTTP listener integrates with the **existing** ATD plumbing: `ServerConfig`, `CapabilitySet`, `TokenBroker`, `Middleware`, `AuditSink`. Nothing in the runtime layer changes.
- Forward-compatible with later sessions (`Mcp-Session-Id`), resumability (`Last-Event-ID`), OAuth 2.1 — but **none** of those land in this SP (see §9).
- Celia can delete `celia-cli/src/http_server.rs` after a documented 3-step cut-over, with §13.1 device-local invariants preserved at every step.
- Test parity: a deterministic CI test proves "same `RunTool` request through UDS vs HTTP produces byte-identical `ToolResult.result`".

## 3. Non-goals

- **Multi-tenant session state.** `Mcp-Session-Id` recognised but ignored — connections are stateless within this SP.
- **Resumability.** No `Last-Event-ID` replay; if a client disconnects mid-SSE, in-flight events are lost.
- **TLS termination.** Operators terminate TLS at a reverse proxy (nginx/Caddy/Tauri). The listener binds plaintext, defaults to `127.0.0.1`.
- **OAuth 2.1 token issuance / refresh.** Bearer is validated, not issued.
- **WebSocket transport.** Streamable HTTP only.
- **Streaming a single tool's progress over SSE.** All tool calls are request → single JSON-RPC response. Streaming entry points (analogous to Celia's `/chat/stream`) are *out of MCP scope*; we leave them to adopter-specific endpoints (see §5.5).
- **`atd-mcp-bridge` deprecation.** The stdio bridge stays — Claude Desktop / Hermes / OpenAI Codex CLI still use it (SP-7 §2.1).
- **HTTP method beyond `POST`.** No `GET /mcp` long-poll, no `DELETE /mcp/sessions/...`. Those belong to future session work.

## 4. Design

This is ~50% of the SP. Each subsection is one of the 8 decisions in the brief. Each gives the chosen answer, evidence from existing code/docs, and the rejected alternatives.

### 4.1 Listener placement — new crate `atd-server-http` (sibling of `atd-server`)

**Decision.** Ship a new crate `crates/atd-server-http/` that depends on `atd-runtime` (for `Registry`, `CapabilitySet`, `TokenBroker`, `Middleware`, `AuditSink`) and `atd-protocol` (for `Response`, `ToolDefinition`, the error codes). It depends on **`axum 0.8`** + **`tokio`** (already in workspace; `crates/atd-server/Cargo.toml:19` pulls tokio). It does **not** depend on `atd-server` (no circular need; Unix listener and HTTP listener are siblings).

**Why a new crate, not a module inside `atd-server`.** SP-listener-extract (`docs/superpowers/specs/2026-04-25-sp-listener-extract-design.md:23-24`) carved `atd-server` out of `atd-ref-server` for exactly this reason: "Future `BindingProtocol::Mcp` server-side is stdio (no listener); REST binding is HTTP (different transport). Runtime must stay transport-agnostic so all transports can compose." The same argument repeats one level up. A user wanting only HTTP (no Unix socket) should not be forced to take `tokio::net::UnixListener` (`crates/atd-server/src/server.rs:6`); a user wanting only UDS should not be forced to take axum. Two sibling crates keep dep graphs honest.

**Why not a `Transport` trait abstraction.** Tempting (it'd let `atd-server` host both), but premature: UDS and HTTP have radically different connection lifetimes (`UnixListener::accept` returns a long-lived stream; HTTP is request-scoped after upgrade-to-SSE), different per-conn state (capability set is *per connection* on UDS, *per request* on HTTP — see §4.3), different Hello semantics (§4.2). A `Transport` trait that papers over both is a leaky abstraction whose first concrete user (this SP) would already need an escape hatch. Defer until a third transport (vsock? quic?) demands it.

**Trade-off table:**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| New module in `atd-server` | One crate; obvious co-location | Pulls axum into every `atd-server` consumer; mixes UDS-stream and HTTP-request lifetimes in one file | rejected |
| New crate `atd-server-http` | Clean dep graph; mirrors SP-listener-extract precedent; UDS-only / HTTP-only / both compositions all stay clean | One more `Cargo.toml`; `atd-ref-server` gains a tiny new dep line | **chosen** |
| `Transport` trait, `atd-server` hosts both impls | Single API surface for adopters | Trait shape forced by two unequal transports; refactor cost > current need | deferred (see §9) |

The `atd-ref-server` binary will gain a `--http-listen ADDR` flag that, when set, spawns the HTTP listener with the same `Arc<Registry>` it already passes to `Server::new` (`crates/atd-server/src/server.rs:24-33`).

### 4.2 Wire on the HTTP leg — MCP JSON-RPC, with ATD `Hello` mapped to MCP `initialize`

**Decision.** On HTTP, speak **MCP Streamable HTTP JSON-RPC 2.0** verbatim. POST `/mcp` accepts JSON-RPC envelopes; the methods `initialize`, `notifications/initialized`, `tools/list`, `tools/call` map to ATD operations. ATD-native `Request::Hello` / `Request::ToolList` / `Request::RunTool` (`crates/atd-protocol/src/messages.rs:34-52`) do **not** appear on the wire; the listener translates.

**Why MCP-on-HTTP, not ATD-native-wire-over-HTTP.** Three reasons.
1. **Tool-call sites we want to enable** (Cursor, Claude.ai, OpenAI Functions, browser PWAs) speak MCP. ATD's wire is bespoke (length-prefixed JSON envelopes per `docs/protocol/wire-format.md:50-54`); no off-the-shelf agent speaks it. Forcing MCP clients through `atd-mcp-bridge` (stdio) loses the precise property — HTTP reachability — we are building.
2. **Celia already pays the translation cost** (`celia-cli/src/http_server.rs:284-461`): MCP `tools/call` → `dispatch_for_caller` → wrap as MCP `{ content: [{type:"text", text}], isError }`. SP-7 (`docs/superpowers/specs/2026-04-24-sp7-mcp-bridge.md:1-3`) validates the same translation on the stdio bridge. The translation is well-understood; centralising it inside `atd-server-http` is one-time work.
3. **HTTP-side state mismatch.** ATD `Hello` is connection-scoped: capability set is set once, used for many subsequent `RunTool`s on the same socket (`crates/atd-server/src/connection.rs:22, 51-69`). MCP `initialize` is similar but session-scoped (which we explicitly disallow in §9). Mapping `Hello` → `initialize` *response shape only* (server-info, supported capabilities), and re-deriving the *per-request* capability set from the Bearer token via `TokenBroker` (§4.4), preserves the right semantics.

**Translation table (HTTP side: from MCP method → ATD `Registry` op):**

| MCP method (over HTTP) | ATD-native op | Notes |
|---|---|---|
| `initialize` | (no-op; synthesised by listener) | Returns server name, version, `capabilities.tools = {}` mirroring `celia-cli/src/http_server.rs:352-369`. Does **not** call into Registry. |
| `notifications/initialized` | (no-op; ack only) | One-way notification per MCP spec. |
| `tools/list` | `Registry::summaries()` + filter | Filter `ToolVisibility::Hidden`, same as `crates/atd-server/src/connection.rs:70-79`. |
| `tools/call` | `Registry::dispatch` equivalent (capability check → broker → semaphore → binding → middleware → audit) | All the logic in `crates/atd-server/src/connection.rs:93-369` is factored into a shared `dispatch()` fn the HTTP listener also calls — see §4.3. |

**Why not also run ATD-native wire over HTTP** (e.g. POST `/atd` with length-prefixed JSON in the body)? It would solve no problem we have. The HTTP-reachable clients (Celia PWA, Cursor, Claude.ai) all speak MCP. The ATD-native-wire-aware clients (`atd-sdk`, `atd-cli`) already speak UDS. A third niche — speaking ATD-native over HTTP — exists in nobody's mind. Don't ship it.

**Who owns the MCP↔ATD translation.** `atd-server-http` directly, in module `crates/atd-server-http/src/mcp.rs`. Not `atd-mcp-bridge` — that crate is the *stdio* bridge and stays a 1:1 process model (SP-7 §2.2 explicitly defers daemonisation). The translation code is short (~150 LoC; cf. Celia's lines 308-461 covering the same surface) and lives where it's evaluated.

### 4.3 Registry sharing — one `Arc<Registry>`, per-request capability derivation

**Decision.** Both listeners hold the **same** `Arc<Registry>`. The HTTP listener does **not** maintain per-connection state (HTTP is stateless by transport contract); instead, every incoming JSON-RPC request derives its `CallContext` afresh from the Bearer token via `TokenBroker` lookup.

**How.** Refactor `crates/atd-server/src/connection.rs::dispatch` (line 38) so that everything from line 93 (`Request::RunTool` arm) through line 369 lives in a transport-neutral function in **`atd-runtime`** — call it `atd_runtime::dispatch::run_tool(state, ctx_in, run_tool_args) -> Response`. Both `atd-server` and `atd-server-http` call this fn. The per-connection state currently held by `connection.rs` (the `tracker`, `caps`, `caller_id` triple at lines 19-26) becomes an input parameter, not an implicit closure. On UDS, that triple is per-connection. On HTTP, that triple is per-request.

**Capability set per-request, not per-session.** On UDS, the SP-12 `Hello` handshake (`crates/atd-server/src/connection.rs:51-69`) sets `caps` once, used for many `RunTool`s. On HTTP, the listener:
1. extracts Bearer from `Authorization: Bearer <token>`,
2. calls `TokenBroker::resolve(&token)` (see §4.4 — broker is extended to map `token → (caller_id, CapabilitySet, SecretBundle)`),
3. builds a one-shot `CapabilitySet` for that request,
4. passes it to `run_tool` alongside the parsed args.

This is the right shape: HTTP requests can land on any connection in any order, possibly from different agents; tying capability to connection-id would let one agent's capabilities leak into another agent's tool call. Per-request derivation prevents the leak by construction.

**Why not require an `initialize` round-trip first** (mirroring UDS `Hello`)? Two reasons. (1) MCP clients already do `initialize` once per session, but it does not carry capabilities in our wire — capabilities are derived server-side from the Bearer; making `initialize` mandatory adds round-trips without changing the security model. (2) Stateless HTTP means we cannot guarantee the same listener instance handles two requests from the same client, so any state we'd cache from `initialize` would be either useless or sticky-session-dependent.

**`ReadTracker` (`crates/atd-runtime/src/lib.rs:28`).** Currently per-connection on UDS. On HTTP, we mint one tracker per request — read-budget enforcement still works, it's just scoped tighter. Adopters who depend on cross-call read tracking within a session must use UDS (or, post-SP-future-sessions, sticky sessions over HTTP).

### 4.4 Authentication — Bearer token via `TokenBroker`, two trait additions

**Decision.** Bearer tokens map to ATD identity through an **extended** `TokenBroker`. Currently `TokenBroker::resolve(caller_id) -> SecretBundle` (`crates/atd-runtime/src/secrets.rs:104-114`, paraphrased; signature shown in SP-token-broker-phase1 §4). We add **one** trait method:

```rust
// in atd-runtime/src/secrets.rs — additive, non-breaking
#[async_trait]
pub trait TokenBroker: Send + Sync {
    // existing — unchanged
    async fn resolve(
        &self,
        caller_id: Option<&str>,
    ) -> Result<Option<Arc<SecretBundle>>, BrokerError>;

    // NEW — HTTP-side bearer token resolution.
    //
    // Default impl returns NotConfigured so existing brokers compile
    // unchanged. HTTP-aware brokers override.
    async fn resolve_bearer(
        &self,
        _bearer: &str,
    ) -> Result<Option<BearerIdentity>, BrokerError> {
        Err(BrokerError::NotConfigured)
    }
}

pub struct BearerIdentity {
    pub caller_id: String,
    pub granted_capabilities: Vec<String>,
    pub secrets: Option<Arc<SecretBundle>>,
}
```

**Why a default impl, not a new trait.** All five SP-token-broker-phase1 decisions (Q1-Q10 in `2026-04-27-sp-token-broker-phase1-design.md:14-30`) optimise for non-breakage. Adding a defaulted method preserves that: `InMemoryTokenBroker` and any third-party broker continue to compile. Operators who deploy `atd-server-http` simply have to override `resolve_bearer` on their broker.

**Handshake time sequence (text):**

```
Browser PWA / Cursor                  atd-server-http                     atd-runtime
─────────────────────                  ───────────────                     ───────────
POST /mcp ─────────────────────────►
  Authorization: Bearer ce_<64hex>
  Origin: http://localhost:5173
  Content-Type: application/json
  body = {"jsonrpc":"2.0", "id":1,
          "method":"tools/call",
          "params":{"name":"ref:echo.say",
                    "arguments":{"text":"hi"}}}

                                       ─► origin_allowed(origin)? else 403
                                       ─► bearer present? else 401
                                       ─► broker.resolve_bearer(bearer).await
                                          ─►─►─►─►─►─►─►─►─►─►─►─►─►─►─►─►─►
                                                                          BearerIdentity {
                                                                             caller_id: "agent-Cursor-42",
                                                                             granted_capabilities: ["fs.read","echo"],
                                                                             secrets: Some(Arc<{...}>),
                                                                           }
                                       ─► mcp.rs: parse JSON-RPC, route to tools/call
                                       ─► atd_runtime::dispatch::run_tool(
                                            state, CapabilitySet, caller_id, secrets,
                                            tool_id="ref:echo.say", args={...}, dry_run=false
                                          ).await
                                                                          ─► same code as connection.rs lines 145-368
                                                                          ─► Response::ToolResultResponse{...}
                                       ◄────────────────────────────────
                                       ─► wrap as MCP envelope
                                          {"content":[{"type":"text","text":"<json>"}], "isError":false}
                                       ─► JSON-RPC envelope:
                                          {"jsonrpc":"2.0","id":1,"result":{...above...}}
◄───────────────────────────── HTTP 200 application/json
```

**Anonymous mode.** If neither operator-configured `TokenBroker` nor `--require-bearer` flag is set, requests without `Authorization` are accepted with `caller_id = None`, empty `CapabilitySet`, `secrets = None`. This matches the UDS default-empty-Hello behaviour (`crates/atd-server/src/connection.rs:22`) and Celia's current Tier-0 trust model (`celia-cli/src/http_server.rs:295-306`). Operators opt into strict auth via `ServerConfig`.

**Why not require a separate token cache** (e.g. listener-local LRU on `bearer → BearerIdentity`)? Premature. The broker is the authoritative cache. If a particular broker is slow, it can cache internally. Listener does not micro-optimise this yet.

### 4.5 SSE — out of scope for `tools/call`; opt-in only for adopter "stream" endpoints

**Decision.** `POST /mcp` returns a **single** JSON-RPC response (`Content-Type: application/json`). Even for tools that take seconds, the listener waits and returns one envelope. No SSE for `tools/call`. Adopter-specific stream endpoints (e.g. Celia's `/chat/stream`) can layer on top of `atd-server-http` but are **not** part of the MCP-shaped surface.

**Why not the MCP §"Sending Messages to the Server" cases 5-6 SSE response shape.** Two reasons:
1. **ATD tools have synchronous semantics.** `Tool::call -> CallFuture<'a>` (`crates/atd-runtime/src/registry.rs:14-15`) returns one `Result<Value, ToolCallError>`. No intermediate progress events exist in the runtime contract. Faking them at the listener (e.g. periodic "still working" frames) is theatre, not data.
2. **Single-response keeps the HTTP path symmetric with UDS.** A UDS `RunTool` produces one `Response::ToolResultResponse`. Matching one-to-one on HTTP makes the parity test (§8) bytes-identical, not just semantically-equivalent.

**Where SSE *does* land in this SP.** Adopters who want streaming endpoints (Celia: token-by-token chat, healthkit: bulk-export progress) define their own routes alongside `/mcp`. `atd-server-http` exposes the axum `Router` so they can extend it:

```rust
// adopter code
let (router, server) = atd_server_http::Server::builder(registry).build();
let router = router.route("/chat/stream", post(my_chat_stream_handler));
server.serve(router).await?;
```

The `/chat/stream` route in `celia-cli/src/http_server.rs:182-254` migrates to this pattern: Celia keeps its endpoint, but no longer reimplements origin gate or bearer parse — those are middleware in `atd-server-http`.

**Why this is the *right* split.** `tools/call` is a protocol contract — must be MCP-compatible. Streaming is an *application* concern (Celia's chat events are not tool calls; they are LLM token + tool-trace mux). Conflating them inside ATD would force every ATD adopter to ship Celia-style chat semantics. Don't.

### 4.6 Origin / CORS — fail-closed default, explicit `--allow-origin`, OPTIONS handled by middleware

**Decision.** Default Origin allow-list: `http://127.0.0.1*`, `http://localhost*`, `https://127.0.0.1*`, `https://localhost*`, `tauri://*`. Matches Celia's defaults (`celia-cli/src/http_server.rs:121-130`) which the MCP §"Security Warning" prescribes (DNS-rebinding defence). Additional origins via `ServerConfig::extra_origins: Vec<String>` (initialised from `--allow-origin` CLI flag, repeatable). Enforcement is **axum middleware** registered on the `Router`, not per-handler code. Preflight `OPTIONS` answered automatically by axum's `tower-http::cors` layer with `Access-Control-Allow-{Origin,Methods: POST,Headers: Authorization,Content-Type,Mcp-Session-Id}`.

**Why middleware, not per-handler.** Three reasons. (1) DRY: every route gets the gate, including future adopter routes (§4.5). (2) Adopters cannot accidentally bypass the gate by forgetting the check in a new handler. (3) Easier audit: one site to grep for.

**Why no wildcard `*` default.** MCP spec mandates DNS-rebinding defence. We match it. Operators who need cross-origin (rare; CDN-hosted PWA → home-network ATD server) opt in explicitly.

**CORS preflight semantics.** `tower-http::cors::CorsLayer::very_permissive().allow_origin(<allowlist>)`. Headers permitted: `authorization, content-type, mcp-session-id` (last one reserved; see §9). Methods: `POST, OPTIONS`. Credentials: `false` — bearer auth doesn't need cookies. Max-age: `3600`.

### 4.7 Sessions and resumability — out of scope, but interface-stable

**Decision.** `Mcp-Session-Id` header **recognised on the wire** (parsed, logged) but **not load-bearing** for routing or capability scoping in this SP. `Last-Event-ID` ignored. Behaviour is identical with or without the header — every request stands alone.

**Why recognise it without using it.** Clients that pin to the MCP spec already send `Mcp-Session-Id: <uuid>` on follow-up requests. Stripping it would force them into nonconforming branches. Logging it (audit field, optional) preserves the bread-crumb for the future SP that adds session-stickiness, without committing to it now.

**Future-extension contract (so the future SP doesn't break wire).** When sessions land:
- `Mcp-Session-Id` becomes the per-session capability-set key (replacing per-request derivation in §4.3).
- `GET /mcp` long-poll opens an SSE channel keyed by session id; server can push to it.
- `DELETE /mcp` releases the session.
- `Last-Event-ID: <ulid>` on reconnect replays from the audit-tracked event log.

None of this lands here. The decision is to *leave the door unlocked* (header name reserved, route paths reserved) so the future SP doesn't change the URL space.

### 4.8 Celia migration — three-step cut-over, §13.1 invariant preserved at each step

**Decision.** Celia keeps `celia-cli/src/http_server.rs` until ATD v0.4.0 ships with `atd-server-http`. Then a 3-PR cut-over:

**Step 1: Build adapter (no behaviour change).** Celia adds `Arc<celia_tools::Registry>` (already implicit — `celia_tools::tool_catalogue()` and `dispatch_for_caller` at `celia-cli/src/http_server.rs:419-428`) and wraps it in an `atd_runtime::Registry`-compatible shape. Existing `/mcp` and `/chat/stream` routes unchanged. Verify: §13.1 says DEK is volatile, lives in `KeyCache<user_id>`. Test: existing `cargo test -p celia-cli` smoke + the gcore DEK eviction test (`pnpm --filter @celia/desktop test:dek`) both still pass.

**Step 2: Side-by-side mode (feature flag).** Celia gains a flag `--use-atd-server-http` that switches the `/mcp` route from Celia's `handle_mcp` to ATD's. `/chat/stream` stays Celia-owned (per §4.5). Verify: with the flag off, current behaviour. With the flag on, parity tests (§8) prove same bytes back. §13.1: the DEK + `KeyCache` ownership doesn't move; the tool dispatcher inside `atd_runtime::Registry` still holds `Arc<ServerState>` for the DB+DEK plumb. The HTTP listener never touches the DEK directly — only the registered tools do, when called.

**Step 3: Default-on, then delete.** After 1 release cycle of side-by-side with parity tests green, flip the default. One release later, delete `celia-cli/src/http_server.rs` entirely. The `--use-atd-server-http` flag becomes a no-op (warn + ignore for one release), then removed.

**Rollback at each step.** Step 1: revert the adapter commit; nothing else changed. Step 2: revert the flag default; user-facing routes were unchanged. Step 3: cherry-pick the deleted file back from history.

**§13.1 invariant audit.** The patent's device-local volatile-key invariant says: DEK is derived from passphrase, held only in `KeyCache: Map<user_id, Arc<Zeroizing<Vec<u8>>>>`, lost on process restart. This SP changes **none** of that. The HTTP listener is a transport; it does not own the DEK. Tool registration owns the DEK (via the dispatcher's captured `Arc<ServerState>` in Celia's case, or via `TokenBroker`-resolved `SecretBundle` for other adopters). Cut-over Step 1 keeps the dispatcher unchanged; Step 2 swaps the *route handler* not the dispatcher; Step 3 deletes Celia's HTTP code, leaving the dispatcher (and its `KeyCache` reference) untouched. gcore-verifiable DEK eviction stays bit-for-bit identical pre- and post-cut.

## 5. Wire details

### 5.1 Endpoints

| Verb | Path | Purpose | Body | Returns |
|---|---|---|---|---|
| POST | `/mcp` | MCP JSON-RPC 2.0 (single response) | `JsonRpcRequest` | `application/json` `JsonRpcResponse` |
| OPTIONS | `/mcp` | CORS preflight | (empty) | 204 with CORS headers |
| (reserved) GET | `/mcp` | SSE pull — **future SP** | n/a | n/a |
| (reserved) DELETE | `/mcp` | Session release — **future SP** | n/a | n/a |
| (adopter-defined) POST | `/*` | Adopter stream routes (Celia `/chat/stream` etc.) | adopter shape | adopter shape |

### 5.2 JSON-RPC request body

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "ref:echo.say",
    "arguments": { "text": "hello" }
  }
}
```

Methods recognised: `initialize`, `notifications/initialized`, `tools/list`, `tools/call`. Unknown methods return JSON-RPC error `-32601 method not found`.

### 5.3 JSON-RPC response — success

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      { "type": "text", "text": "{\"echoed\":{\"text\":\"hello\"}}" }
    ],
    "isError": false
  }
}
```

The `text` field carries the JSON-serialised `Response::ToolResultResponse.result` (`crates/atd-protocol/src/messages.rs:80-85`) — exact bytes match what UDS returns. `isError = (success == false)`.

### 5.4 Headers — request

| Header | Required | Purpose |
|---|---|---|
| `Content-Type: application/json` | yes | Only JSON bodies accepted. |
| `Accept: application/json` | recommended | Server returns 406 if `Accept` rejects `application/json`. Default if absent: accept. |
| `Origin: <url>` | yes (browser); ignored (curl) | Checked against allow-list. If present and not allowed: 403. |
| `Authorization: Bearer <token>` | conditional | Required iff `ServerConfig.require_bearer == true`. Otherwise optional. |
| `Mcp-Session-Id: <ulid>` | optional | Logged, not used (§4.7). |

### 5.5 Headers — response

| Header | Purpose |
|---|---|
| `Content-Type: application/json` | All MCP responses. |
| `Access-Control-Allow-Origin: <echo>` | When request `Origin` is allow-listed. |
| `Mcp-Session-Id: <echo>` | Echoed back if present on request. |

### 5.6 Error mapping — HTTP status × JSON-RPC error code × ATD code

| Trigger | HTTP status | JSON-RPC `error.code` | ATD `Response::Error.code` | Notes |
|---|---|---|---|---|
| Origin not allowed | 403 | `-32001` | n/a (rejected pre-dispatch) | Body is empty / minimal JSON-RPC error. |
| Missing Bearer when required | 401 | `-32002` | n/a | `WWW-Authenticate: Bearer` header. |
| Broker error (`BrokerError::Lookup`) | 200 | `-32603` | `ERR_BROKER_FAILED = 1003` | (`crates/atd-protocol/src/messages.rs:19`) |
| Capability denied | 200 | `-32603` | `ERR_CAPABILITY_DENIED = 1001` | (`crates/atd-protocol/src/messages.rs:6`) |
| Tool not found | 200 | `-32601` | (no ATD code today) | Maps "method not found"-ish. |
| Rate limited (semaphore saturated) | 429 | `-32603` | `ERR_RATE_LIMITED = 1002` | (`crates/atd-protocol/src/messages.rs:11`) `Retry-After: <ms>` header if known. |
| Tool returned `success: false` (`ExecutionFailed`) | 200 | `null` (result-carrying) | n/a | `result.isError = true`, `result.content` carries the error JSON — mirrors `connection.rs:326-334`. |
| Invalid JSON-RPC envelope | 400 | `-32600` | n/a | |
| Body too large (> 10 MiB) | 413 | `-32600` | n/a | Matches `atd-protocol/src/wire.rs:4` cap. |
| Internal error | 500 | `-32603` | n/a | No stack trace in body. |

**Note**: ATD's existing JSON-RPC-adjacent error codes (`ERR_CAPABILITY_DENIED = 1001`, `ERR_RATE_LIMITED = 1002`, `ERR_BROKER_FAILED = 1003`, see `crates/atd-protocol/src/messages.rs:1-19`) live in the **ATD response payload** (`Response::Error.code: Option<u16>`), not in the JSON-RPC envelope's `error.code` field. The MCP envelope uses standard JSON-RPC -32xxx codes; the ATD numeric carries through inside the result/error payload, preserving information for clients that introspect it.

## 6. Crate / module shape

### 6.1 Directory tree (new files only; existing files untouched unless flagged)

```
crates/
├── atd-runtime/
│   └── src/
│       ├── lib.rs                          # MOD: pub mod dispatch;
│       ├── dispatch.rs                     # NEW (~200 LoC, factored from
│       │                                   #   atd-server/src/connection.rs:38-369;
│       │                                   #   the per-conn state becomes input args)
│       └── secrets.rs                      # MOD: add resolve_bearer default
│                                           #      + BearerIdentity struct
│
├── atd-server/                             # UNCHANGED (except: connection.rs
│   └── src/connection.rs                   #   calls atd_runtime::dispatch::run_tool
│                                           #   instead of inlining)
│
├── atd-server-http/                        # NEW CRATE
│   ├── Cargo.toml                          # NEW
│   ├── README.md                           # NEW (~40 lines, mirrors atd-server)
│   └── src/
│       ├── lib.rs                          # NEW (~30 LoC public surface)
│       ├── config.rs                       # NEW (~60 LoC; HttpServerConfig)
│       ├── server.rs                       # NEW (~100 LoC; axum::serve harness)
│       ├── mcp.rs                          # NEW (~150 LoC; JSON-RPC ↔ ATD translation)
│       ├── origin.rs                       # NEW (~50 LoC; allow-list + CorsLayer)
│       ├── bearer.rs                       # NEW (~50 LoC; Authorization parse + broker lookup)
│       └── error.rs                        # NEW (~40 LoC; HttpServerError)
│
├── atd-ref-server/
│   ├── Cargo.toml                          # MOD: add optional dep atd-server-http
│   ├── src/
│   │   ├── main.rs                         # MOD: --http-listen ADDR flag;
│   │   │                                   #      spawn HTTP listener if set
│   │   └── lib.rs                          # MOD: re-export atd_server_http::Server
│   │                                       #      under feature `http`
│   └── tests/
│       └── e2e_http_parity.rs              # NEW: bytes-identical UDS vs HTTP test
│
└── atd-mcp-bridge/                         # UNCHANGED (stdio bridge stays)
```

### 6.2 New public type signatures (Rust pseudo-code; no impl)

```rust
// crates/atd-server-http/src/lib.rs
pub mod config;
pub mod server;
pub mod error;

pub use config::HttpServerConfig;
pub use server::{Server, ServerBuilder};
pub use error::HttpServerError;

// crates/atd-server-http/src/config.rs
pub struct HttpServerConfig {
    pub listen: SocketAddr,                          // default 127.0.0.1:0
    pub extra_origins: Vec<String>,                  // appended to default allow-list
    pub require_bearer: bool,                        // default false (anonymous mode)
    pub max_body_bytes: usize,                       // default 10 MiB (matches atd-protocol wire cap)
    pub server_version: String,                      // echoed in `initialize` response
    pub audit_sink: Option<Arc<dyn AuditSink>>,      // shared with UDS server
    pub token_broker: Option<Arc<dyn TokenBroker>>,  // same broker as UDS server
    pub middleware: Vec<Arc<dyn Middleware>>,        // result-middleware chain
    pub tier_policy: TierPolicy,                     // tier-derived deadlines
    pub granted_capabilities: Vec<String>,           // operator allow-list,
                                                     //   intersected per-request
                                                     //   with broker output
}

// crates/atd-server-http/src/server.rs
pub struct Server {
    /* private */
}

pub struct ServerBuilder {
    /* private */
}

impl Server {
    pub fn builder(registry: Arc<Registry>) -> ServerBuilder;
}

impl ServerBuilder {
    pub fn config(self, cfg: HttpServerConfig) -> Self;
    /// Returns (router, server) so adopters can extend the Router
    /// with their own routes (e.g. Celia /chat/stream) before serve().
    pub fn build(self) -> (axum::Router, Server);
}

impl Server {
    pub async fn serve(self, router: axum::Router) -> Result<(), HttpServerError>;
    pub fn local_addr(&self) -> Option<SocketAddr>;
    pub fn shutdown(&self);
}

// crates/atd-runtime/src/dispatch.rs (factored from connection.rs)
pub struct DispatchInputs<'a> {
    pub state: &'a Arc<ServerState>,                 // ServerState type lives in atd-runtime now
    pub tracker: &'a Arc<ReadTracker>,
    pub caps: &'a CapabilitySet,
    pub caller_id: Option<&'a str>,
    pub req: Request,
}

pub async fn run_tool(inputs: DispatchInputs<'_>) -> Response;

// crates/atd-runtime/src/secrets.rs (additions)
pub struct BearerIdentity {
    pub caller_id: String,
    pub granted_capabilities: Vec<String>,
    pub secrets: Option<Arc<SecretBundle>>,
}

// trait extension shown in §4.4
```

### 6.3 Relationship to existing `atd-server`

`atd-server` keeps its current public surface (`Server`, `ServerConfig`, `ServerError` per `crates/atd-server/src/lib.rs:11-13`). Internally, the `connection.rs::dispatch` body (`crates/atd-server/src/connection.rs:38-369`) shrinks to a thin call into `atd_runtime::dispatch::run_tool`. UDS-specific concerns (Hello connection state, frame reads/writes via `read_frame`/`write_frame`) stay in `atd-server`. The refactor is mechanical and verified by the existing connection.rs test suite (`crates/atd-server/src/connection.rs:374-1033`) — those tests assert dispatch outcomes; they pass unchanged after the move.

`ServerState` itself currently lives in `atd-server::server` (`crates/atd-server/src/server.rs:16-21`). It moves to `atd-runtime::dispatch::ServerState`. The migration is straightforward — it already references `atd_runtime::Registry`, `atd_runtime::TierPolicy`, `atd_runtime::Middleware`. Only `ServerConfig` is `atd-server`-specific; we split into `atd_runtime::SharedConfig` (audit_sink, token_broker, server_version, granted_capabilities, max_output_bytes, default_call_timeout_ms, cwd) and `atd_server::UdsConfig` (just `socket_path`) + `atd_server_http::HttpServerConfig` (HTTP-specific).

## 7. Migration path (Celia side)

### Step 1 — Adapter (no behaviour change)

**What.** Celia adds a `celia-cli/src/atd_registry.rs` that adapts `celia_tools::dispatch::tool_catalogue()` to an `atd_runtime::Registry`. Each `celia_tools` ToolDefinition wraps in a `struct CeliaToolWrapper { def: ToolDefinition, dispatcher: Arc<dyn Fn>}` and impls `atd_runtime::Tool`. No HTTP server changes; `celia-cli/src/http_server.rs` keeps its hand-written `mcp_tools_list` + `mcp_tools_call`.

**Verify §13.1.** The dispatcher closure captures `Arc<ServerState>` (DB path, `KeyCache`, `user_id`, `agent_id`), so the DEK still lives only in `KeyCache` and is fetched per call. Run `pnpm --filter @celia/desktop test:dek` (gcore DEK eviction check) — passes unchanged because the wrapper doesn't change where DEK is read.

**Rollback.** `git revert` the adapter file; nothing else touched.

### Step 2 — Side-by-side mode (`--use-atd-server-http`)

**What.** Celia adds a CLI flag. With it on, the `/mcp` route is mounted by `atd_server_http::Server::builder(adapter_registry).build()` instead of Celia's `Router::new().route("/mcp", post(handle_mcp))` (`celia-cli/src/http_server.rs:90-92`). The `/chat/stream` route stays Celia-owned (per §4.5). Origin gate is now ATD-provided; Celia's `origin_allowed` becomes dead code (commented, not yet deleted).

**Verify §13.1.** DEK access path: previous `mcp_tools_call` (`celia-cli/src/http_server.rs:392-461`) calls `state.cache.get(&state.user_id)`. New path: ATD HTTP listener → `atd_runtime::dispatch::run_tool` → `RegisteredTool.binding.call(...)` → `NativeBinding(CeliaToolWrapper).call(args, ctx)` → wrapper closure → `state.cache.get(...)`. Same `Arc<KeyCache>` instance, same `user_id` lookup. Volatile-only, evicted on process exit. Confirm by running the gcore DEK check + adding `cargo test --test e2e_http_parity -p atd-ref-server` (§8) — must show byte-identical responses for the same `RunTool` over both UDS and HTTP.

**Rollback.** Default-off; users opt in. Flip default in step 3.

### Step 3 — Default-on, then delete

**Release N**: `--use-atd-server-http` defaults to `true`. The legacy Celia handlers are still compiled but unused. Internal canaries (Celia's own E2E suite, including `apps/desktop test:e2e` Playwright smoke) run against the new path.

**Release N+1**: Delete `celia-cli/src/http_server.rs` (all 461 LoC). Replace with a 30-line file that builds the ATD HTTP server + mounts `/chat/stream`. The flag becomes a deprecation warning.

**Release N+2**: Remove the flag.

**Rollback per release.** Each release is its own PR; cherry-pick revert if a regression is found in the wild. The parity test from §8 must remain green at every step.

## 8. Test plan

### 8.1 Unit tests

- **`atd-server-http::origin`** — `origin_allowed(headers, [])` accepts loopback variants + `tauri://`; rejects `https://evil.example`; `extra_origins = ["https://celia.health"]` accepts that origin and only that origin.
- **`atd-server-http::bearer`** — `Authorization: Bearer foo` parses; `Authorization: Basic foo` rejects; absent header → anonymous iff `require_bearer == false`.
- **`atd-server-http::mcp`** — `tools/list` envelope encodes the same `ToolSummary` array shape as UDS (use `serde_json::from_str(j1) == serde_json::from_str(j2)`). `tools/call` envelope wraps `ToolResultResponse.result` correctly.
- **`atd-runtime::dispatch`** — moved tests from `crates/atd-server/src/connection.rs::tests` (currently 16+ tests at lines 374-1033) — must all pass post-refactor.
- **`atd-runtime::secrets::resolve_bearer`** — `InMemoryTokenBroker` default `Err(NotConfigured)`; a `BearerIdentity`-returning custom broker round-trips.

### 8.2 Integration tests

- **`crates/atd-server-http/tests/e2e_basic.rs`** — start `atd-server-http` with an `EchoStub` registry on `127.0.0.1:0`; POST `/mcp` with `initialize` then `tools/list` then `tools/call`. Assert all 3 succeed; assert `isError = false`.
- **`crates/atd-server-http/tests/e2e_origin.rs`** — same setup, POST with `Origin: https://evil.example` → 403.
- **`crates/atd-server-http/tests/e2e_bearer.rs`** — `require_bearer = true`, omit Authorization → 401. Provide a broker that returns `BearerIdentity { caller_id: "agent-A", granted_capabilities: ["echo"], ... }` for the test bearer → 200.
- **`crates/atd-server-http/tests/e2e_capability_denied.rs`** — tool requires `["fs.write"]`; bearer grants only `["echo"]` → JSON-RPC `result.isError = true`, embedded ATD `code = 1001`.
- **`crates/atd-ref-server/tests/e2e_http_uds_parity.rs`** — **the key parity test.** Start one `atd-ref-server` listening on both UDS and HTTP, same `Arc<Registry>`. Make identical `tools/call` for `ref:echo.say` via both paths. Assert the unwrapped `ToolResultResponse.result` JSON is byte-identical between transports. Repeat for `ref:shell.exec`, `ref:fs.read` (with mock cwd), `ref:web.fetch` (against a localhost test server).

### 8.3 Cross-project (Celia)

- **`apps/desktop/test:e2e`** (Playwright) — runs against the new ATD HTTP path once Celia migration Step 2 lands. Smoke: launch Celia → tool list returns 15 Celia tools → call `phr.read_patient` via HTTP → response shape unchanged.
- **`pnpm --filter @celia/desktop test:dek`** (gcore DEK eviction) — must pass at every migration step. This is the §13.1 guard.
- **`apps/web` ChatPage SSE smoke** — Celia retains `/chat/stream` (per §4.5); test that browser SSE still streams `ChatStreamEvent` after the cut-over, because the route is unchanged.

### 8.4 Conformance

- Extend `atd-conformance` to gain a `--transport http` flag (currently UDS-only). Run the existing conformance suite end-to-end over HTTP. Every case that passes on UDS must pass on HTTP byte-for-byte (where wire shapes are comparable; some UDS-only cases like raw frame framing are skipped).

## 9. Out of scope (future SPs)

| Feature | Why deferred | Sketch of future SP |
|---|---|---|
| `Mcp-Session-Id` sticky sessions | Requires server-side state store; HTTP listener loses cleanliness | SP-streamable-http-sessions — adds `SessionStore` trait, in-memory + Redis impls |
| Resumability (`Last-Event-ID`) | Requires per-session event log; couples to audit infra | Same SP as sessions; adds replay endpoint |
| HTTPS / TLS termination | Operators terminate at reverse proxy; ATD process stays plaintext-on-loopback | SP-streamable-http-tls — only if a use case emerges where co-locating cert handling makes sense |
| OAuth 2.1 token issuance | Bearer is validated, not minted; minting belongs to identity layer | SP-token-broker-oauth — extends `TokenBroker` with OAuth flow primitives |
| WebSocket transport | MCP doesn't mandate it; SSE covers the streaming use case adequately | Not currently planned |
| `BindingProtocol::Mcp` server-side stdio | Different transport shape; `atd-mcp-bridge` already does the inverse | SP-mcp-stdio-server — only if an MCP client demands a server-side stdio role |
| Tool-level progress streaming over SSE | Requires `Tool::call` to yield intermediate values; runtime contract change | SP-streaming-tools — design TBD; needs adopter pull |

## 10. References

### atd-mvp source (line-precise; spot-check targets)

1. `crates/atd-server/src/lib.rs:11-13` — current `pub use` surface (`Server`, `ServerConfig`, `ServerError`); `atd-server-http` mirrors this shape.
2. `crates/atd-server/src/server.rs:6` — `tokio::net::UnixListener` import; only UDS listener exists today.
3. `crates/atd-server/src/server.rs:24-33` — `Server::new(registry, config)` signature; HTTP server uses the same `Arc<Registry>`.
4. `crates/atd-server/src/connection.rs:19-26` — per-connection state (`tracker`, `caps`, `caller_id`); §4.3 refactors this into per-request inputs.
5. `crates/atd-server/src/connection.rs:38-369` — `dispatch` body that moves to `atd-runtime::dispatch::run_tool`.
6. `crates/atd-server/src/connection.rs:51-69` — `Hello` handshake (SP-12); its capability-intersection logic is preserved per-request on HTTP.
7. `crates/atd-server/src/connection.rs:241-262` — `TokenBroker::resolve` call site; §4.4 adds the `resolve_bearer` sibling.
8. `crates/atd-server/src/config.rs:8-35` — `ServerConfig` shape; HTTP variant mirrors all shared fields.
9. `crates/atd-server/Cargo.toml:14-21` — current deps (`atd-protocol`, `atd-runtime`, `tokio`); HTTP crate adds `axum`, `tower-http`.
10. `crates/atd-runtime/src/lib.rs:17-28` — public re-exports including `TokenBroker`, `CapabilitySet`, `Middleware`; HTTP listener uses every one of these.
11. `crates/atd-runtime/src/registry.rs:14-30` — `Tool::call` returns one `CallFuture<'a>` → one result; §4.5 grounds the "no SSE for `tools/call`" decision.
12. `crates/atd-protocol/src/messages.rs:1-19` — error code constants (`ERR_CAPABILITY_DENIED = 1001`, `ERR_RATE_LIMITED = 1002`, `ERR_BROKER_FAILED = 1003`); §5.6 maps each to HTTP+JSON-RPC.
13. `crates/atd-protocol/src/messages.rs:34-52` — `Request::Hello { client_id, requested_capabilities }`; HTTP listener synthesises equivalent state per request.
14. `crates/atd-protocol/src/wire.rs:4` — 10 MiB frame cap; HTTP body size enforces the same.
15. `docs/protocol/wire-format.md:6` — "HTTP (Phase 2)" annotation; this SP delivers what that line forecast.
16. `docs/superpowers/specs/2026-04-25-sp-listener-extract-design.md:23-24` — the principle that runtime stays transport-agnostic; `atd-server-http` honours it.
17. `docs/superpowers/specs/2026-04-25-sp12-canonical-dispatch.md:152-161` — capability denial wire shape; reused verbatim on HTTP.
18. `docs/superpowers/specs/2026-04-27-sp-token-broker-phase1-design.md:14-30` — broker design decisions; `resolve_bearer` is the additive Phase 1.5 extension.
19. `docs/superpowers/specs/2026-04-24-sp7-mcp-bridge.md:2-3` — current MCP↔ATD bridge is stdio-only; HTTP closes the second leg.

### Celia source

20. `celia_phr/crates/celia-cli/src/http_server.rs:1-41` — module header explaining the current Phase I.3 implementation and the gaps SP-streamable-http closes.
21. `celia_phr/crates/celia-cli/src/http_server.rs:114-131` — origin allow-list; `atd-server-http` adopts this default verbatim.
22. `celia_phr/crates/celia-cli/src/http_server.rs:284-329` — MCP method routing (`initialize`, `notifications/initialized`, `tools/list`, `tools/call`); the translation `atd-server-http::mcp.rs` will replicate.
23. `celia_phr/crates/celia-cli/src/http_server.rs:392-461` — `mcp_tools_call` body; mirrors what `atd_runtime::dispatch::run_tool` does generically.
24. `celia_phr/crates/celia-cli/src/http_server.rs:295-306` — Tier-0 bearer model that becomes the new broker `resolve_bearer` default for anonymous mode.

### External spec

25. https://modelcontextprotocol.io/specification/2025-06-18/basic/transports — Streamable HTTP §"Sending Messages to the Server" (cases 1-6) and §"Security Warning" (DNS-rebinding, Origin validation, loopback binding).

---

**Summary.** New crate `atd-server-http` siblings `atd-server`. HTTP wire is MCP JSON-RPC; one `Arc<Registry>` shared with the UDS listener. Capability set is per-request (broker-derived from Bearer). Origin gate is axum middleware with fail-closed defaults. SSE is reserved for adopter-defined routes, not `tools/call`. Sessions / resumability / TLS / OAuth are explicit non-goals with reserved interfaces. Celia migrates in 3 steps preserving §13.1 throughout. Parity test (UDS vs HTTP byte-identical) is the contract.
