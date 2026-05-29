# ATD Architecture

**Scope:** Normative architecture for the **reference implementation** —
the `atd-*` crate family in this repository. Describes the system as it
stands today, not how it got here. (Historical / by-release notes live
in [`CHANGELOG.md`](../CHANGELOG.md); per-SP design rationale is archived
under [`docs/archive/superpowers/specs/`](archive/superpowers/specs/).)

**Authority:** This document is the single source of truth for ATD's
architecture. For the byte-level wire contract see
[`docs/protocol/`](protocol/); for evolution scope and deferred work see
[`docs/roadmap.md`](roadmap.md).

**License:** Apache-2.0.

---

## Table of contents

1. [What ATD is](#1-what-atd-is)
2. [The unified schema](#2-the-unified-schema)
3. [The layer model](#3-the-layer-model)
4. [Wire & types](#4-wire--types)
5. [Dispatch](#5-dispatch)
6. [Security](#6-security)
7. [Middleware](#7-middleware)
8. [Skills layer (adjacent)](#8-skills-layer-adjacent)
9. [Component & crate map](#9-component--crate-map)
10. [Non-goals](#10-non-goals)

---

## 1. What ATD is

**ATD (Agent Tool Dispatch)** is a wire protocol that lets any LLM
agent, on any framework, call any tool, on any platform — through a
single typed RPC surface. The reference implementation in this repo is
the Rust source of truth: a `atd-protocol` types crate, a
`atd-runtime` server runtime, an `atd-sdk` client SDK, two listener
crates (`atd-server` for Unix sockets, `atd-server-http` for HTTP),
middleware crates, built-in tools, and adopter integrations.

The four "any"s frame the interoperability claim:

| Dimension | Fragmentation today | ATD's answer |
|---|---|---|
| Any tool | CLI, REST, MCP, native SDK — incompatible shapes | One `ToolDefinition` maps to multiple bindings |
| Any platform | Linux / macOS / Windows / iOS / Android / HarmonyOS each have distinct call surfaces | Binding selection is server-side at dispatch time |
| Any agent | Claude Code can't consume OpenAI function-calling shapes without a shim | All agents call the same SDK; adapters render per-provider dicts |
| Any framework | LangChain tool ≠ MCP tool ≠ Apple App Intent | One definition, many framework consumers |

Three audiences read this document:

- **External protocol implementers** — authors of Go / Java / Swift /
  TypeScript / ArkTS SDKs, or tool-server implementers in languages
  this repository does not ship. Read §2 (unified schema), §4 (wire
  types), §5 (dispatch contract).
- **Internal contributors** working against the reference
  implementation. Read §3 (layers) and §9 (crate map) to find where
  to make changes.
- **Decision-makers** evaluating adoption. Read §1, §2, §10
  (non-goals).

**Not this document:**

- Not the byte-level wire reference — see
  [`docs/protocol/wire-format.md`](protocol/wire-format.md).
- Not the release history — see [`CHANGELOG.md`](../CHANGELOG.md) and
  the tag list for what landed when.
- Not the evolution roadmap — see [`docs/roadmap.md`](roadmap.md) for
  deferred features and long-term direction.

---

## 2. The unified schema

> *"Does ATD follow a unified schema?"* — Yes, and it's published as a
> first-class artifact that any language can consume directly.

The atomic claim ATD makes — and the one feature most worth understanding
first — is that **every message on the wire, in every direction, in
every transport (UDS or HTTP), serialises to a shape defined by a
single machine-readable schema**: `/atd-protocol-schema.json`. This
schema is generated from the Rust type definitions in `atd-protocol`
via `schemars` and validated against the [JSON Schema 2020-12
meta-schema](https://json-schema.org/draft/2020-12/schema). CI gates
drift between the Rust source and the published JSON.

### 2.1 What the schema covers

The published schema describes the complete wire vocabulary:

| Layer | Schema-covered types |
|---|---|
| Envelope | `ClientMessage` (=`Request`), `ServerMessage` (=`Response`) |
| Handshake | `Hello`, `HelloAck`, `Ping`, `Pong` |
| Discovery | `ToolList` request / response; `ToolSchema` request / response; `ToolSummary` (per-tool entry); `DiscoverFilter` |
| Invocation | `RunTool` request, `RunToolContinue` request (cursor continuation), `ToolResultResponse` (success/error union), `CallOptions` |
| Tool description | `ToolDefinition`, `ToolCapability`, `ToolBinding`, `ToolSafety`, `ToolResources`, `ToolTrust`, `ToolErrorDef` |
| Enums | `SafetyLevel`, `ToolVisibility`, `TrustLevel`, `ToolTier`, `BindingProtocol` |
| Errors | `AtdError` taxonomy with wire codes (see `docs/protocol/error-codes.md`) |
| Pagination | `CursorPayload`, `next_cursor` field on `ToolResultResponse` |
| Capability negotiation | `CapabilitySet`, `Hello.requested_capabilities`, `HelloAck.granted_capabilities`, `Hello.ucan_tokens` |

If a Rust type in `atd-protocol` is `pub`, it's in the schema. If a
field crosses the wire, it's in the schema. There is no per-transport
divergence: the same `RunTool` envelope flows over a Unix socket (via
`atd-server`) or over HTTP/MCP-JSON-RPC (via `atd-server-http`); both
listeners deserialise into the same Rust types.

### 2.2 Why a single schema matters

- **Cross-language SDK parity.** A TypeScript SDK, a Go SDK, or a
  Swift SDK generated from `atd-protocol-schema.json` is automatically
  type-compatible with the Rust SDK and the Rust server. No
  hand-port-and-pray for matching shapes.
- **Cross-transport parity.** UDS and HTTP listeners share the
  same `atd-runtime::dispatch::dispatch_request` entry point and the
  same type-checked envelope handling. Adding a third transport (say,
  WebSocket) means writing a new listener that calls the same
  `dispatch_request` — no schema changes.
- **Audit & analysis.** Any field name that appears in an audit log,
  any error code an agent surfaces, any tool metadata an LLM sees —
  all of these are traceable to the published schema. There's no
  hidden field reachable only by reading the Rust source.
- **Conformance testability.** The `atd-conformance` crate ships test
  scenarios that verify any ATD-speaking server against the schema's
  contractual behaviours. Pass the suite and you're interoperable.

### 2.3 Sanitization

Tool ids like `ref:fs.read` carry a colon and a dot, which break LLM /
MCP function-name slots. The schema's identity rule is paired with a
canonical bidirectional sanitiser in `atd-sdk::sanitize`:
`ref:fs.read` ↔ `ref_fs_read`. Both forms appear in protocol traffic —
the canonical (`ref:fs.read`) over the wire, the sanitised
(`ref_fs_read`) inside LLM tool calls. The sanitiser is part of the
SDK so every consumer applies it identically.

### 2.4 Schema as build artifact

The schema is checked into the repo at `/atd-protocol-schema.json`. A
`gen-schema` binary regenerates it from the Rust types; CI runs the
regenerator and rejects PRs whose committed JSON drifts from what the
Rust types currently emit. The schema is also validated against the
JSON Schema 2020-12 meta-schema — malformed schema content fails CI
before any adopter sees it.

The Python SDK at `python/src/atd_client/types.py` is currently
hand-ported; a follow-up SP will switch it to schema-generated. The
hand-port is currently gated by integration tests against the Rust
server; drift bugs would surface there.

### 2.5 Stability commitment

As of **1.0 the schema is frozen for the 1.x line**: additive changes
(new optional fields, new enum variants) are minor bumps; removing a
field or changing a shape is a major (2.0) bump. Code generated from
`atd-protocol-schema.json` at 1.0 keeps deserialising every 1.x message.
The full stability contract is in
[`docs/release-plan-v1.0.md`](release-plan-v1.0.md).

---

## 3. The layer model

Three core mechanisms plus two extension mechanisms:

```
┌────────────────────────────────────────────────────────────────┐
│  User intent (voice · text · trigger)                          │
└────────────────────────────┬───────────────────────────────────┘
                             │
┌────────────────────────────▼───────────────────────────────────┐
│  Agent framework                                               │
│  (Claude Code · Cursor · Hermes · LangChain · custom)          │
└────────────┬──────────────────────────────┬────────────────────┘
             │                              │
   via Skill │                              │ direct tool call
             ▼                              ▼
┌──────────────────────────────┐  ┌───────────────────────────┐
│  Skills layer (§8 — adjacent)│  │  (no Skill intermediary)  │
│  SKILL.md · atd-tools · body │  │  simple / one-shot tasks  │
└──────────────┬───────────────┘  └──────────────┬────────────┘
               │                                 │
               └──────────────┬──────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│  Client SDK                                                    │
│  discover · describe · call · call_page · call_all             │
└────────────────────────────┬───────────────────────────────────┘
                             │
                             ▼
┌────────────────────────────────────────────────────────────────┐
│  Dispatch (§5)                                                 │
│  capability gate · tier · binding · cursor · middleware        │
└────────────────────────────┬───────────────────────────────────┘
                             │
              ┌──────────────┴───────────────┐
              ▼                              ▼
   ┌─────────────────────┐         ┌─────────────────────┐
   │  Unix socket        │         │  HTTP / MCP JSON-RPC│
   │  (atd-server)       │         │  (atd-server-http)  │
   └─────────────────────┘         └─────────────────────┘
                             │
                             ▼
┌────────────────────────────────────────────────────────────────┐
│  Tool universe (§5.4 bindings + extension points)              │
│  ref:echo, ref:fs.*, ref:shell.*, ref:web.fetch, ...           │
└────────────────────────────────────────────────────────────────┘
```

**Core mechanisms** (one section each):

- **Schema (§2 + §4)** — unified, machine-readable, single source of truth for every wire shape.
- **Dispatch (§5)** — deterministic pipeline: capability gate → tier-aware deadlines → binding selection → tool invocation → cursor / middleware.
- **Security (§6)** — classification + per-tool runtime controls + capability allow-listing + UCAN-lite tokens + multi-tenant secret routing + audit.

**Extension mechanisms:**

- **Bindings (§5.4)** — pluggable invocation back-ends. The reference impl ships `NativeBinding` and `CliBinding`; the trait is open.
- **Middleware (§7)** — egress-validation / redaction pipeline. The reference impl ships path-redaction, FHIR validation, and HIPAA PHI redaction; the trait is open.

### 3.1 Two call graph examples

**Direct agent → ATD** (one-shot):

```
agent.llm picks tool_id = "ref:shell.exec", args = {"command": "uname -s"}
  ↓
atd_sdk::AtdClient::call(tool_id, args, CallOptions { .. })
  ↓ length-prefixed JSON over Unix socket
atd-server accepts connection (or atd-server-http, same dispatch downstream)
  ↓ dispatch: Hello capability gate → registry → tier → binding → tool
NativeBinding::invoke(&args)
  ↓ executes
ToolResult { success: true, data: { stdout: "Linux\n", exit_code: 0, .. } }
  ↓ middleware pipeline (RedactPathsMiddleware rewrites $HOME paths)
  ↓ serialise, length-prefixed JSON back
atd_sdk delivers ToolResult to agent
```

**Skill body → ATD** (multi-step):

```
Skills runtime loads skill @acme/morning-briefing per user intent
  ↓ install-time: required atd-tools verified discoverable on socket
skill body executes in agent context
  ↓ step 1: call hms:health.sleep.get for yesterday
atd_sdk::AtdClient::call("hms:health.sleep.get", { "date": "2026-04-23" }, ..)
  ↓ identical dispatch path as Example A
  ... (body continues with step 2, step 3)
skill returns to agent with synthesised output
```

Both paths traverse identical ATD dispatch. The Skills layer is an
agent-side orchestrator on top; it does not modify dispatch.

---

## 4. Wire & types

The wire is **length-prefixed JSON** over a duplex byte stream. The
two listener crates (`atd-server` over Unix sockets,
`atd-server-http` over HTTP+SSE) translate transport-level framing
into the same in-memory `ClientMessage` / `ServerMessage` types from
`atd-protocol`. Tool servers depending on `atd-runtime` see the same
type surface regardless of transport.

### 4.1 The top-level envelope

`ClientMessage` (the request union) variants:

| Variant | Purpose |
|---|---|
| `Hello` | Connection handshake. Carries `client_id`, `requested_capabilities`, optional `ucan_tokens`. Server replies with `HelloAck` containing the intersected `granted_capabilities`. |
| `Ping` | Heartbeat / liveness check. Server replies `Pong`. |
| `ToolList` | Discovery. Returns `Vec<ToolSummary>` filtered per `DiscoverFilter` (visibility, capability requirements, tier). |
| `ToolSchema` | Per-tool deep-describe. Returns the full `ToolDefinition` including JSON input/output schemas and intent examples. |
| `RunTool` | Invocation. Carries `tool_id`, `args: serde_json::Value`, `CallOptions`. Returns `ToolResultResponse` (success-with-data or error-with-code). |
| `RunToolContinue` | Pagination continuation. Carries the opaque `cursor` returned by a prior `RunTool` / `RunToolContinue`. |

`ServerMessage` mirrors with response variants (`HelloAck`, `Pong`,
`ToolListResponse`, `ToolSchemaResponse`, `ToolResultResponse`). The
`ToolResultResponse` envelope carries `result: ToolResult` (data or
error) plus an optional `next_cursor: Option<String>`.

### 4.2 The tool description

A `ToolDefinition` returned by `ToolSchema` exposes the contract an
agent uses to decide whether and how to call the tool:

```rust
pub struct ToolDefinition {
    pub id: String,                       // canonical e.g. "ref:fs.read"
    pub name: String,                     // human-friendly
    pub description: String,
    pub version: String,
    pub capability: ToolCapability,       // domain, actions, tags, intent_examples
    pub input_schema: serde_json::Value,  // JSON Schema 2020-12
    pub output_schema: serde_json::Value,
    pub bindings: Vec<ToolBinding>,       // protocol + per-binding config
    pub safety: ToolSafety,               // level, dry_run support, side_effects
    pub resources: ToolResources,         // timeout_ms, max_concurrent, ...
    pub trust: ToolTrust,                 // publisher, trust_level, signature
    pub visibility: ToolVisibility,       // Read / Write / Dangerous / System / Hidden
    pub required_capabilities: Vec<String>,
    pub tier: Option<ToolTier>,           // Hot / Warm / Cold
    pub errors: Vec<ToolErrorDef>,        // tool-specific error catalog
}
```

Every field is part of the published schema. `ToolSummary` (returned
by `ToolList`) is a thinner projection — id, name, description,
visibility, capability shorthand, input_schema — designed for LLM
context efficiency.

### 4.3 Error taxonomy

ATD has two distinct error layers — keep them apart:

- **`AtdError`** — the client-side Rust error enum in `atd-protocol`
  (`ToolNotFound`, `InvalidArguments`, `CapabilityDenied`,
  `BindingUnavailable`, execution failure, `PaginationLimitExceeded`,
  `MergeFailed`, …). This is what `atd-sdk` returns to agent code; the
  enum itself carries no numeric code.
- **Numeric wire codes** — `ERR_*` `u16` constants in
  `atd_protocol::messages`, carried in the `Response::Error.code` field
  on the wire:

| Code | Constant | Meaning |
|---|---|---|
| 1001 | `ERR_CAPABILITY_DENIED` | Caller lacks a required capability |
| 1002 | `ERR_RATE_LIMITED` | Per-tool semaphore refused (retryable) |
| 1003 | `ERR_BROKER_FAILED` | `TokenBroker` errored during resolve |
| 1010–1013 | `ERR_UCAN_*` | UCAN invalid / expired / delegation-too-deep / audience-mismatch |
| 1020 | `ERR_CURSOR_EXPIRED` | Continuation cursor past TTL |
| 1021 | `ERR_CURSOR_INVALID` | Cursor signature verification failed |

The full taxonomy — every `AtdError` variant, every wire code, and the
retry guidance — is [`docs/protocol/error-codes.md`](protocol/error-codes.md).

### 4.4 Cursor pagination

For tools whose honest result exceeds the 1 MB advisory output
budget (large FHIR exports, multi-month query windows), the wire
carries an opaque, HMAC-signed `next_cursor` string on
`ToolResultResponse`. Clients re-invoke via `RunToolContinue { tool_id,
cursor }`. The cursor binds to `(tool_id, caller_id, args_fingerprint,
page_index, issued_at_unix, server_session)` so it can't be replayed
against tampered args or stolen across callers; verification is
stateless. Default TTL is 5 minutes; wire cap is 512 bytes (CBOR
encoding fits comfortably).

The reference impl is `atd_runtime::cursor::{CursorIssuer,
CursorPayload, args_fingerprint}`; tools opt in by overriding
`Tool::supports_pagination` + `Tool::call_paginated`. The SDK exposes
`AtdClient::call_page` (single page) and `AtdClient::call_all`
(auto-walks the cursor chain with `MergePolicy::{ConcatArray,
ConcatField, FirstPageOnly}`). Non-paginating tools serve a single
`RunTool` response with `next_cursor = None` and never see the
continuation path.

### 4.5 Sanitization

Tool ids contain `:` and `.`, which break LLM / MCP function-name
slots. `atd-sdk::sanitize::sanitize_tool_name` returns
`ref_fs_read` for `ref:fs.read`; `desanitize_tool_name` inverts.
Both forms appear in protocol traffic — canonical on the wire, sanitised
inside LLM tool-calling shapes. The MCP bridge applies the same rule
so a tool id is unambiguous regardless of which slot it landed in.

---

## 5. Dispatch

Every call traverses a deterministic pipeline:

```
accept connection
  → Hello handshake (capability gate, optional UCAN verify)
  → receive RunTool / RunToolContinue
  → registry.get(tool_id)
  → capability check (refuse if required_capabilities ⊄ granted)
  → tier-aware deadline + max_output_bytes resolution
  → TokenBroker::resolve(caller_id) → CallContext::secrets
  → binding.invoke(args, &ctx)         // or call_paginated when cursor set
  → middleware pipeline (RedactPaths, FHIR, PII, ...)
  → serialise ToolResultResponse + optional next_cursor
```

Dispatch is transport-agnostic: both `atd-server` (UDS) and
`atd-server-http` (HTTP) call into the same
`atd_runtime::dispatch::dispatch_request` entry point.

### 5.1 The core APIs

| API | Purpose | SDK form |
|---|---|---|
| `discover` | Enumerate visible tools | `AtdClient::discover(filter) -> Vec<ToolSummary>` |
| `describe` | Get the full `ToolDefinition` for one tool | `AtdClient::describe(tool_id) -> ToolDefinition` |
| `call` | Invoke and return a single result | `AtdClient::call(tool_id, args, CallOptions) -> ToolResult` |
| `call_page` | One page of a paginated tool | `AtdClient::call_page(tool_id, args, Option<&cursor>, CallOptions)` |
| `call_all` | Auto-walks the cursor chain | `AtdClient::call_all(tool_id, args, CallAllOptions)` |
| `ping` | Liveness | `AtdClient::ping()` |
| `hello` | Capability negotiation | `AtdClient::hello(Some(client_id), requested_caps) -> Vec<String>` |

The Python SDK at `python/src/atd_client/` mirrors the API surface
with both sync and async flavours (`AtdClient` / `AtdClientSync`).

A sibling Python **server runtime** lives at `python/src/atd_server/`
(SP-server-py-v1, 2026-05-19) — adopters whose tool host must live in
a Python process (e.g. an embodied-agent simulator co-located with
MuJoCo state) use this instead of the Rust `atd-server`. It speaks the
same wire format byte-for-byte; verified by passing 22/24 of the
`atd-conformance` fixtures. See
[`docs/integrations/python-server.md`](integrations/python-server.md).

### 5.2 Capability gate

Two complementary mechanisms compose at Hello time:

**1. Operator allow-list (strings).** The server is started with a set
of capability strings it offers (e.g. `--grant-capability healthkit:read
--grant-capability healthkit:write`). The client `Hello.
requested_capabilities` is intersected with the offer; any capability
in `Hello.requested_capabilities` not offered by the server is
silently dropped. The resulting `granted_capabilities` is returned in
`HelloAck`. Tools declaring `required_capabilities: ["healthkit:read"]`
are refused (`CapabilityDenied`, code 1001) when called by a client
whose `granted_capabilities` does not contain `healthkit:read`.

The intersection is strict subset in both directions:

- Requested but not offered → not granted.
- Offered but not requested → not granted.
- Requested ∧ offered → granted.

**2. UCAN-lite bearer tokens.** When a client sends one or more
JWT-shape `Hello.ucan_tokens`, the server's UCAN verifier
(`atd_runtime::ucan::verify_jwt`) walks the attenuation chain
(`prf[]` linking child to parent), checks Ed25519 signatures via
`did:key` audience pins, and emits the capability subset the leaf
token actually carries. The dispatch-level `granted_capabilities` for
that call becomes `strings ∪ ucan_capabilities`. Revocation is
consulted on every chain link via the `UcanRevocationStore` trait.
Bounded chain depth (default 5).

UCAN-lite is **additive**: clients that don't supply tokens see only
the string allow-list path. Both paths produce the same
`granted_capabilities` shape at dispatch time, so tools never know
which path their caller used.

### 5.3 Tier-aware deadlines

Each tool declares a `ToolTier` (`Hot` / `Warm` / `Cold`). The
dispatch layer resolves a per-call deadline + max-output-bytes budget
from the tool's tier, overridable per-call via `CallOptions::deadline_ms`
and per-server via `--tier-override`. Tier is a latency/cost class
signal — not a lifecycle policy.

| Tier | Default deadline | Typical use |
|---|---|---|
| `Hot` | sub-second | Sync side-effect-free queries (time, environment) |
| `Warm` | seconds | Most tools — file IO, shell, web fetch |
| `Cold` | minutes | Slow imports, large exports, model inference |

Cursor-paginated tools pay the tier deadline **per page**, so a Cold
tool can stream over a long total wall-time without violating its
page-level SLO.

### 5.4 Bindings

A binding is the abstract way `dispatch` turns an `(args,
CallContext)` pair into a `Result<Value, ToolCallError>`. The trait
is `Binding`; the two reference impls:

| Binding | Behaviour |
|---|---|
| `NativeBinding` | Delegates to a `Tool` impl in the same Rust process. Default for every registered built-in. |
| `CliBinding` | Spawns a subprocess, maps JSON args to argv, captures stdout/stderr, honours `ctx.deadline` with SIGTERM-then-SIGKILL grace. Demo tool: `ref:external.uname`. |

The trait is open. A `GrpcBinding`, `WasmBinding`, or hypothetical
`McpBinding` would implement the same `Binding::invoke` signature; the
dispatcher selects via `ToolBinding::protocol` on the tool's
declaration. v1 always routes to the first binding a tool declares;
multi-binding selection (honouring `CallOptions::preferred_binding`)
is a small dispatcher upgrade if real multi-binding tools land.

### 5.5 Secret routing (TokenBroker)

Adopters running multi-tenant — one server process serving many
distinct OAuth users via one socket — need per-caller secrets without
each caller seeing each other's tokens. The `TokenBroker` trait
(`atd_runtime::secrets::TokenBroker`) is the extension point:

```rust
pub trait TokenBroker: Send + Sync {
    fn resolve(&self, caller_id: Option<&str>) -> ResolveFuture;
    fn resolve_bearer(&self, bearer: &str) -> ResolveBearerFuture;
    fn accepted_token_formats(&self) -> &'static [&'static str];
}
```

Reference implementations:

- **`InMemoryTokenBroker`** — unit-test fixture / single-process
  setup. UCAN-JWT branch dispatches via `register_ucan_audience()`.
- **`FileTokenBroker`** (`atd_runtime::file_token_broker`) —
  disk-backed. Persists per-bearer subdirs at
  `${root}/${bearer_id}/{access_token,refresh_token,expires_at}.json`
  with mode 0700 / 0600 on Unix. Holds a per-bearer refresh mutex
  (`lock_refresh()`) so concurrent OAuth refresh attempts for the
  same bearer can't double-round-trip. `is_near_expiry()` is a no-IO
  predicate (default 5-minute window) adopters call to decide whether
  to take the refresh path. Layout matches the existing single-tenant
  on-disk scheme some adopters already use; migration is a one-`mv`-
  per-bearer operation.

Production deployments wrap a vault or secrets-manager behind a
custom `impl TokenBroker`. The trait is `pub` and stable.

`CallContext::secrets: Option<Arc<SecretBundle>>` is populated by the
dispatcher before `Tool::call` runs. Tools that need secrets read them
via `ctx.secrets().get("access_token")`; tools that don't, ignore the
field. `SecretBundle` wraps values in `RedactedString` —
`Debug`/`Display` impls refuse to print, so accidental log lines
don't leak credentials. Audit events include only
`secrets_resolved: bool`, never the key names or values.

**HTTP bearer auth** is the same trait's `resolve_bearer` arm. The
HTTP listener parses `Authorization: Bearer ...` headers, calls
`broker.resolve_bearer(token)`, gets back a typed `BearerOutcome` with
11 variants (Ok / OkShrunk / Expired / Revoked / Unknown / Internal /
Lookup / ...) each mapped to a specific HTTP status +
`WWW-Authenticate` + optional `Retry-After`. SSE bearer-refresh
helper (`atd_server_http::sse_refresh`) does 60s heartbeat
re-resolution and emits `RefreshEvent::{Refreshed, AuthLost}` for
long-lived streams.

### 5.6 Cursor pagination

Already described in §4.4 from the wire-format angle. Dispatch-side
specifics:

- `CursorIssuer` is constructed once per server with a random
  HMAC-SHA256 key + a `server_session` id; tools never see the
  signing key. `ctx.cursor_issuer().issue(payload) -> String` mints a
  signed cursor for the next page.
- `Tool::supports_pagination` defaults to `false`; tools that opt
  in override `call_paginated(args, ctx, cursor)` instead of `call`.
- The dispatcher pre-verifies an incoming cursor against the same
  issuer before invoking the tool: HMAC mismatch → `CursorInvalid`
  (1021); past TTL → `CursorExpired` (1020); cross-tool reuse →
  rejected at fingerprint comparison.

The conformance scenario `paginated_dispatch` exercises a 100-row
generator over 10 pages, asserts cross-tool and expired-cursor
rejection, and verifies the SDK's `call_all` concatenation walks the
full chain.

**Operational note — HMAC key rotation (v1 gap).** The signing key is
per-process: random at startup, or pinned via `ATD_CURSOR_SIGNING_KEY`.
A server restart therefore invalidates every outstanding cursor (the
`server_session` nonce changes) → the next `RunToolContinue` gets
`1020 ERR_CURSOR_EXPIRED`, and the client must re-issue the original
`RunTool` to get a fresh cursor. For long federation syncs (a Phase-L
adopter walking months of FHIR over many pages) one restart forces a
full re-fetch — real bandwidth / API-quota cost. **Cross-restart cursor
continuity (key persistence to disk, or a `kid`-tagged rotation window
that retains the last N keys) is adopter-side in v1; no spec.** A
federation adopter that feels the re-fetch cost is the trigger for an
SP-cursor-key-rotation; until then the gap is documented, not closed.

### 5.7 Sessions and cancellation

Not in v1. The design surface (state scope, wire mechanism,
idempotency, concurrency semantics) is wide; deferring preserves the
option to design against a concrete adopter requirement rather than
guessing.

---

## 6. Security

### 6.1 Three-axis classification

Every tool declares three orthogonal classifications as part of its
`ToolDefinition`. These are **descriptive metadata** — used by callers
and operators to reason about risk; not enforcement mechanisms on
their own (§5.2 capability gate + §6.3 per-tool runtime controls are
the actual enforcement).

| Classification | Values | Field |
|---|---|---|
| Safety | `Read` / `Write` / `Financial` / `Privacy` / `Physical` / `Destructive` | `ToolSafety::level` |
| Visibility | `Read` / `Write` / `Dangerous` / `System` / `Hidden` | `ToolVisibility` |
| Trust | `L0Unverified` / `L1SchemaValid` / `L2Tested` / `L3Verified` / `L4Certified` | `ToolTrust::trust_level` |

LLM adapters surface `SafetyLevel` and `Visibility` to agent-framework
tool pickers where supported. `Visibility::Hidden` excludes a tool
from `ToolList` discovery but keeps it reachable via `ToolSchema` and
`RunTool` (use for raw vendor endpoints, debug helpers, integration-
test tools).

`ToolTrust::signature` is currently declarative; signature verification
is a non-goal (see §10.3).

### 6.2 Capability allow-listing

Described in §5.2. The mechanism enforces:

- Operator-declared capability strings at server start
- Client-requested subset via `Hello.requested_capabilities`
- UCAN-lite token capabilities via `Hello.ucan_tokens`
- Per-tool `required_capabilities: Vec<String>`

A tool whose `required_capabilities` is not a subset of the call's
intersected `granted_capabilities` is refused with `CapabilityDenied`
(code 1001).

### 6.3 Per-tool runtime controls

Defences that live inside specific tools, not at the dispatch layer.
Each guards an attack surface that tool's category exposes:

| Control | Applies to | Location |
|---|---|---|
| **SSRF guard** (loopback + RFC1918 + link-local + CGN + TEST-NET + 0.0.0.0/8 + IPv4-mapped-private; re-checked on every redirect hop) | `ref:web.fetch` | `crates/atd-tools-web/src/fetch.rs::check_ssrf` |
| **Header allow-list** (Accept, Accept-Language, Referer, User-Agent only; Authorization + Cookie rejected with `InvalidArgs`) | `ref:web.fetch` | same file, `build_headers` |
| **Must-read-before-edit** (mtime + size proof required before `fs.edit` will apply) | `ref:fs.edit` | `crates/atd-runtime/src/tracker.rs` + `crates/atd-tools-fs/src/edit.rs` |
| **SIGTERM → grace → SIGKILL subprocess timeout** | `ref:shell.exec` / `ref:shell.pwsh` | `crates/atd-tools-shell/src/shared.rs` |
| **Per-tool semaphore** (honours `ToolResources::max_concurrent`) | All tools | `crates/atd-runtime/src/registry.rs` |
| **Request-arg schema validation** | All tools | per-tool `call` impls + serde |

### 6.4 Audit

Every dispatched call emits a structured `CallEvent` to the configured
`AuditSink`:

```rust
pub struct CallEvent {
    pub ts: String,                  // RFC3339 timestamp
    pub call_id: String,
    pub tool_id: String,
    pub caller_id: Option<String>,
    pub granted_capabilities: Vec<String>,
    pub duration_ms: u64,
    pub outcome: Outcome,            // Success / ExecutionFailed / InvalidArgs / ...
    pub tier: String,
    pub dry_run: bool,
    pub schema_version: u32,         // currently 3
    pub secrets_resolved: bool,      // never the key names or values
    pub cursor_page: Option<u32>,    // 1-based page index; None when not paginated
    pub capability_provenance: Option<Vec<CapProvenance>>, // SP-observability-completeness-v1 Axis C
}
```

`schema_version` is `3` since SP-observability-completeness-v1, which
added the optional `capability_provenance` field: per-capability source
attribution (`StringAllowList` or `UcanChain { issuer_did, chain_depth }`)
so an operator can answer "why did caller X have capability Y?" without
re-deriving the UCAN chain. Additive optional — v2 readers ignore it, v3
readers see `None` on v2 events.

The reference sink `JsonLinesAuditSink` writes JSONL to a configured
path via a dedicated **std-thread** drain over a bounded
`std::sync::mpsc::sync_channel`. The queue-full behaviour is selectable
(SP-observability-completeness-v1 Axis B) via `BackpressureStrategy`:

| Strategy | Behaviour | Use |
|---|---|---|
| `Drop` (default) | `try_send`; on full, drop + bump `drops()` | throughput-first; the 90% non-compliance case |
| `Block` | blocking `send`; dispatch slows, no event lost | HIPAA §164.528 no-loss audit (requires multi-thread runtime) |
| `FallbackSink(fb)` | on full, write to `fb` synchronously | bounded hot path with no silent loss |

`JsonLinesAuditSink::with_strategy` selects it; `AuditSink::
backpressure_strategy()` defaults to `Drop` (byte-compatible with
pre-SP sinks). The `drops` counter is exposed via
`Server::metrics_snapshot()`. Construction no longer requires a tokio
runtime context (the drain is a plain `std::thread`).

Adopters needing different sinks (Kafka, OpenTelemetry, ...) implement
`AuditSink` against their own pipeline. The trait is `pub` and stable.

### 6.5 Rate limiting and concurrency

| Mechanism | Behaviour |
|---|---|
| `ToolResources::max_concurrent` per-tool semaphore | Enforced in `Registry`; refuses with `RateLimited` (1002, retryable) when permits are exhausted. |
| Multi-thread tokio runtime | Ref binaries default to `multi_thread` with `min(cpus, 4)` workers via `atd_runtime::default_worker_threads()`. The accept loop is no longer starved by a single in-flight call. |
| Per-state frame deadlines on UDS connections | 5 s handshake, 30 s active; configurable via `Server::set_frame_deadlines`. |
| SDK connect retry with exponential backoff + ±20% jitter | `AtdClient::connect_with_options` configurable via `ConnectOptions` or `ATD_CONNECT_RETRIES` env. |
| Server-side rate-limiter (token bucket via `governor`) | Not in v1. `ToolResources::rate_limit_per_min` is currently declarative only. |

The 50-client `concurrent_handshake_storm` conformance scenario
verifies the SLO: p99 < 200 ms per client (measured 125 ms on a
4-core developer host), 0 errors, 0 audit drops.

### 6.6 Dry-run

`CallOptions::dry_run: bool` is a wire field. Server-side, the
dispatcher short-circuits a `dry_run: true` call with a synthetic
`tool_result` without invoking the tool, so
`ref:shell.exec("rm -rf /", dry_run=true)` no longer runs the
command. `ToolSafety::dry_run: true` on a tool's metadata signals
that the tool itself has a meaningful dry-run preview path; routing
to that path is a follow-up. v1 is pure server-side short-circuit.

---

## 7. Middleware

The middleware pipeline is the egress-side hook between a tool's
return and the wire reply. The `Middleware` trait
(`atd_runtime::Middleware`) has two hooks:

- `on_result(tool_id, &ToolDefinition, &mut serde_json::Value)` — the
  **success** path, and the `ExecutionFailed` exit (whose wire shape is a
  `ToolResultResponse { success: false, result }`, i.e. a result Value).
  Implementations rewrite the value, strip sensitive sub-trees, or reject
  by mutating to an error envelope.
- `on_error(tool_id, &ToolDefinition, &mut String, &mut Option<Value>)` —
  the `Response::Error` path (`InvalidArgs` / `InternalError`), whose wire
  shape is a bare `message` + optional `details`. Default no-op;
  security-sensitive middleware (PHI/PII redaction) override it.

Both hooks were unified in SP-observability-completeness-v1 Axis A: before
it, error paths bypassed middleware entirely, leaking a tool's failure
text (an arg echo, a panic message naming a patient) to the LLM
unredacted — a real PHI defect. Now every wire reply, success or failure,
runs egress redaction.

Pipelines are composed at `Server::new` time:

```rust
let mut server = Server::new(registry, cfg);
server.set_middleware(vec![
    Arc::new(FhirMiddleware::default()),
    Arc::new(PiiRedactMiddleware::default()),
    Arc::new(RedactPathsMiddleware::default()),
]);
```

### 7.1 Built-in middleware

| Middleware | Crate | What it does |
|---|---|---|
| `RedactPathsMiddleware` | `atd-runtime` | Strips or masks JSON-Pointer paths in arbitrary result trees (e.g. removing `$HOME` paths from shell output). |
| `FhirMiddleware` | `atd-middleware-fhir` | FHIR R4 egress validation. Confirms `resourceType` is in the 12-resource known set; verifies coding-system URIs against `ALLOWED_SYSTEMS_DEFAULT` (75 URIs, kept set-equal to celia's `whitelists.toml` via the I1 drift-guard); enforces required-field presence per resource. Three `MismatchPolicy` variants: `AnnotateAndPass` (default — attaches `_fhir_validation_errors`), `ReplaceWithError` (rewrites payload to a structured error envelope — fail-closed semantics, used by adopters with strict invariants), `StripOffending` (drops the offending sub-tree, keeps the rest). |
| `PiiRedactMiddleware` | `atd-middleware-pii-redact-medical` | HIPAA Safe Harbor PHI redaction. 18 identifier categories × 13 JSON-Pointer paths × 7 `RedactionStrategy` variants + 5 catch-all regex (SSN / driver's license / IP / URL / email). `PiiRedactConfig::{fhir_aware, disable_regex_phi, ...}` flags for opt-out paths. |

Both medical middlewares live in standalone crates so adopters that
don't ship FHIR-shaped or PHI-bearing payloads don't pull the deps.

### 7.2 The whitelist invariant (I1)

`atd_middleware_fhir::ALLOWED_SYSTEMS_DEFAULT` is the canonical set of
permitted CodeSystem URIs at FHIR egress. It is **kept set-equal** to
a vendored copy of celia's source-of-truth
`crates/celia-types/data/whitelists.toml`, located at
`crates/atd-middleware-fhir/vendor/celia-whitelists.toml`. A unit test
in `crates/atd-middleware-fhir/src/systems.rs` parses the vendored
toml via `include_str!` and asserts set equality at every `cargo
test`. If either side drifts, the test fails with the exact set
difference printed.

The reverse direction (celia → atd) is enforced symmetrically: celia
imports `ALLOWED_SYSTEMS_DEFAULT` via `use atd_middleware_fhir::
ALLOWED_SYSTEMS_DEFAULT;` and runs the same set-equality assertion
against its generated constant. Either repo updating its set in
isolation fails one of the two CI gates.

### 7.3 Writing new middleware

Implement the trait:

```rust
impl Middleware for MyMiddleware {
    fn on_result(
        &self,
        tool_id: &str,
        def: &ToolDefinition,
        result: &mut serde_json::Value,
    );
}
```

Register via `Server::set_middleware(vec![...])`. The trait is `pub`
and stable; nothing prevents a third party publishing
`atd-middleware-<topic>` crates. Middleware order matters — pipelines
run top-down — so adopters compose deterministically.

---

## 8. Skills layer (adjacent)

The Skills layer (SKILL.md files + `atd-tools:` dependency
declarations + progressive-disclosure skill bodies) sits *above* ATD
in the layer model. From a protocol standpoint, Skills is an
**upstream consumer** of ATD, not part of ATD itself.

### 8.1 Division of concern

| Concern | Owner |
|---|---|
| SKILL.md authoring, validation, install | Skills runtime (Anthropic Skills, OpenClaw ClawHub, third parties) |
| Progressive disclosure into agent context | Skills runtime |
| `atd-tools:` dependency declarations | SKILL.md format; ATD's contribution is stable tool ids |
| Invoking ATD tools from a skill body | Skills runtime calls the ATD SDK like any other agent |
| The `discover` / `describe` / `call` API the skill body relies on | ATD (this project) |

### 8.2 ATD's Skills-side commitments

- Stable `discover` / `describe` / `call` semantics
- Stable `AtdError` taxonomy
- Stable tool-id conventions (namespace + dot-segments + sanitization rules)
- Stable meta-tool naming convention for skills discovery

### 8.3 The meta-tool convention

ATD servers that expose skills do so via two reserved tool ids:

- `<publisher>:<service>.skills.list` — returns a list of skill
  manifests (id, name, summary).
- `<publisher>:<service>.skills.get` — returns one skill's body +
  metadata by id.

This is a **convention**, not a wire-level message. Clients call it
via the standard `RunTool` path; servers register two tools like any
other. The `atd-cli skills sync` subcommand walks any server
implementing the convention and writes SKILL.md files into
per-platform target directories (hermes, claude-code, stdout).

### 8.4 What ATD does not commit to

- ATD does not parse SKILL.md.
- ATD does not own per-platform install paths.
- ATD does not retain skill state across calls.

---

## 9. Component & crate map

### 9.1 Layering

```
atd-protocol (wire types, schema, sanitize)
   ▲
   ├── atd-sdk (client API; discover/describe/call/call_page/call_all/hello)
   │       ▲
   │       ├── atd-mcp-bridge        (MCP-over-stdio → ATD)
   │       ├── atd-cli               (reference CLI client)
   │       └── atd-conformance       (reusable test suite + bin)
   │
   └── atd-runtime (Tool/Binding/Middleware/Registry/dispatch;
                    TokenBroker + InMemoryTokenBroker + FileTokenBroker;
                    UCAN-lite verifier; CursorIssuer; AuditSink;
                    MetricsCounters)
           ▲
           ├── atd-tools-echo
           ├── atd-tools-fs
           ├── atd-tools-shell
           ├── atd-tools-web
           ├── atd-middleware-fhir              (FHIR R4 egress validation;
           │                                     ALLOWED_SYSTEMS_DEFAULT;
           │                                     vendored celia whitelists.toml)
           ├── atd-middleware-pii-redact-medical (HIPAA Safe Harbor PHI redaction)
           │
           ├── atd-server          (Unix-socket listener + connection task)
           │       ▲
           │       ├── atd-ref-server          (reference binary;
           │       │       ▲                    wires runtime + tools + server)
           │       │       └── atd-mock-weather-server  (cross-vendor demo bin;
           │       │                                     publish = false)
           │       │
           │       └── + vendor servers (e.g. healthkit_cli)
           │           depend directly on atd-runtime + atd-server,
           │           skip atd-ref-server entirely
           │
           └── atd-server-http     (HTTP listener + MCP JSON-RPC translator +
                                    bearer auth + origin gate +
                                    SSE bearer-refresh helper)
                   ▲
                   └── + vendor HTTP servers (e.g. celia_phr)
                       depend on atd-runtime + atd-server-http
```

### 9.2 Per-crate purpose

| Crate | Layer | Purpose |
|---|---|---|
| `atd-protocol` | Schema | Wire types, codec, sanitize. The schema's Rust source. |
| `atd-sdk` | Client | Rust client API. Discover / describe / call / call_page / call_all / hello. |
| `atd-runtime` | Server core | `Tool` trait, `Registry`, dispatch pipeline, `Binding` + `Middleware` + `CursorIssuer` + `TokenBroker` (+ `FileTokenBroker`) + `AuditSink` + UCAN verifier + `MetricsCounters`. Transport-agnostic. |
| `atd-server` | Transport | Unix-socket listener; per-connection task with handshake + frame deadlines. |
| `atd-server-http` | Transport | HTTP listener + MCP JSON-RPC translator + bearer auth + origin gate + SSE bearer-refresh helper. |
| `atd-middleware-fhir` | Middleware | FHIR R4 egress validation. `ALLOWED_SYSTEMS_DEFAULT` (75 URIs) kept set-equal to celia via vendored toml + drift-guard. |
| `atd-middleware-pii-redact-medical` | Middleware | HIPAA Safe Harbor PHI redaction. 18 categories × 13 paths × 7 strategies + 5 regex. |
| `atd-tools-echo` / `-fs` / `-shell` / `-web` | Built-in tools | Reference tool implementations; depend on `atd-runtime`. |
| `atd-mcp-bridge` | Bridge bin | MCP-over-stdio gateway forwarding to any ATD server. |
| `atd-cli` | Bin | Reference CLI client — `atd` command, including `atd skills sync`. |
| `atd-ref-server` | Bin | Reference server binary wiring runtime + tools + Unix server. |
| `atd-mock-weather-server` | Bin (`publish = false`) | Cross-vendor composition demo helper; boots alongside other ATD servers. |
| `atd-conformance` | Test suite + bin | Reusable conformance scenarios. Includes `concurrent_handshake_storm`, `paginated_dispatch`, `phase_l_baseline` (5-AC cross-repo verification). Adopters dev-dep on it to test their implementation. |

### 9.3 Extension points

Where third-party code attaches without forking the reference server:

| You want to... | Surface | Requires fork? |
|---|---|---|
| Add a new tool | `Tool` trait impl + `Registry::register` | No |
| Add a new binding | `Binding` trait impl | No |
| Add a new middleware | `Middleware` trait impl + `Server::set_middleware` | No |
| Add a new auth scheme | `TokenBroker` trait impl + `ServerConfig::token_broker` | No |
| Add a new audit sink | `AuditSink` trait impl + `ServerConfig::audit_sink` | No |
| Add a new transport | New listener crate calling `atd_runtime::dispatch::dispatch_request` | No |
| Add SDK-side aliases | `AtdClient::with_aliases` (planned, SDK-only) | No |
| Change the wire format | — | Yes (not an extension point) |
| Add a new `ToolTier` variant | — | Yes |

### 9.4 Versioning

**Per-crate independent SemVer** ([ADR 0004](adr/0004-per-crate-versioning.md),
2026-05-27, superseding the 1.0/1.1-era workspace-lockstep policy).

- **`atd-protocol`'s version IS the ATD wire/protocol version.** When it
  bumps, the workspace cuts an ATD release with the matching number; the
  annotated tag `v<atd-protocol-version>` and the GitHub release anchor
  here. The 1.x stability contract in
  [`docs/release-plan-v1.0.md`](release-plan-v1.0.md) was always about the
  wire, not the workspace as a whole.
- **Every other crate bumps on its own source change.** A crate can ship a
  patch the same week `atd-protocol` is quiescent, and can lag on weeks it
  bumps. Sibling pins (`atd-protocol = { path = "...", version = "X.Y.Z" }`)
  record the *minimum required* version, not necessarily the latest;
  caret-compatible resolution handles routine bumps.
- The `[workspace.package].version` field is removed; each crate carries an
  explicit `version` in its own `Cargo.toml`.

`atd-mock-weather-server` is the only `publish = false` crate — it's a
demo-only bin.

---

## 10. Non-goals

ATD intentionally does NOT pursue the following — neither in v1 nor as
extension points. Each non-goal has a rationale; an adopter signal can
move any of them onto the roadmap, but the bar is concrete need rather
than aspiration.

### 10.1 Multi-device routing

ATD dispatches to one socket per connection. It does not route a call
to "whichever device the user is using right now"; that's an
agent-framework concern. The protocol gives every device a clean ATD
endpoint and stops there.

### 10.2 Distributed sessions (migrate / fork / handoff)

A session in ATD scopes to one connection. Migrating a session
across processes, forking it for parallel exploration, or handing it
off across hosts — all out of scope. Adopters that need this build it
on top.

### 10.3 Tool signature verification

`ToolTrust::signature` is a declarative field; verification is
non-goal. A signature scheme requires PKI infrastructure
(publisher keys, key rotation, revocation) the protocol does not
currently specify. When an adopter ships a real signing pipeline, the
verification logic can be added without changing the wire shape.

### 10.4 REST / AppFunction / distributed bindings

The `Binding` trait can host any of these, but the reference impl
ships only `NativeBinding` and `CliBinding`. Production adopters
needing REST or platform-specific bindings implement the trait
themselves; the protocol does not bless one rendering.

### 10.5 Native Skills-layer support

ATD is intentionally separate from the Skills runtime; it provides
stable primitives the runtime consumes. SKILL.md parsing, install
path management, and progressive disclosure live in the Skills
runtime (Anthropic Skills / OpenClaw / ...), not here.

### 10.6 Per-tool dry-run preview semantics

v1's dry-run is a server-side short-circuit (returns a synthetic
`tool_result` without invoking the tool). Routing `dry_run: true`
to tools whose `ToolSafety::dry_run` is `true` so they can produce
tool-specific previews is a future axis, not currently on the roadmap.

### 10.7 Per-tool rate-limiter enforcement

`ToolResources::rate_limit_per_min` is declarative only. The
`max_concurrent` axis is enforced via per-tool semaphores;
adding a token-bucket rate limiter (via the `governor` crate) is
straightforward when an adopter needs it, but the v1 line stops at
semaphores.

### 10.8 Cross-vendor capability federation

ATD supports cross-vendor *composition* — one agent connects to N ATD
servers and sees a merged catalog (§5, the cross-vendor pattern). It does
**not** support cross-vendor *capability federation*: every authority
mechanism is scoped to a single server. The string allow-list, the
`caller_id`, the `TokenBroker`, the audit log, and — critically — a
UCAN-lite token's `did:key` audience pin are all per-server. An agent
holding a UCAN delegated for server A's audience cannot present it to
server B; the user would have to mint a second token for B's audience,
and no component knows B's `did:key`.

Consequence: a multi-agent flow where a parent delegates "read patient X"
to a child that then spans *two* vendors' ATD servers does not work out of
the box — it needs either two separately-minted delegations or a
federation registry that brokers audiences across servers. The registry
is out of scope (it implies cross-vendor trust roots, audience discovery,
and a revocation fabric the protocol does not specify). Single-vendor
multi-agent delegation ships fully (SP-capability-v2); cross-vendor
multi-agent collaboration is adopter-built. The keystone delegation
scenario (share-with-Dr-Wang) lives inside one vendor's server, so this
boundary doesn't block it — but a "pull my heart-rate from vendor A AND my
labs from vendor B, both under one delegated child" flow does cross it.

---

## See also

- [`docs/index.md`](index.md) — the full documentation map.
- [`CHANGELOG.md`](../CHANGELOG.md) — what landed in each release.
- [`docs/release-plan-v1.0.md`](release-plan-v1.0.md) — the 1.0 release
  contract, per-crate publication matrix, and pre-release checklist.
- [`docs/protocol/wire-format.md`](protocol/wire-format.md) — byte-level
  wire reference; supplements §4.
- [`docs/protocol/error-codes.md`](protocol/error-codes.md) — full
  `AtdError` taxonomy table.
- [`/atd-protocol-schema.json`](../atd-protocol-schema.json) — the
  unified machine-readable schema (§2).
- [`docs/extending/`](extending/) — how to extend each layer.
- [`docs/roadmap.md`](roadmap.md) — evolution scope and deferred work.
- [`docs/issues/`](issues/) — tracked gaps and adopter validation
  records.
- [`docs/archive/superpowers/`](archive/superpowers/) — per-SP design
  rationale (frozen history).
