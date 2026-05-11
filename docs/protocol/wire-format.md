# ATD Wire Format Reference

**Protocol version:** 0.1.0
**Source:** `crates/atd-protocol/src/` + `crates/atd-sdk/src/wire.rs` at tag `sp10-adapters`
**Machine-readable counterpart:** [`/atd-protocol-schema.json`](../../atd-protocol-schema.json) — generated from the Rust types in `atd-protocol`; CI gates drift.
**Transports:** Unix socket (implemented), stdio (planned), HTTP (Phase 2)

This document is the authoritative reference for the ATD wire protocol. Implementers
building a third-party ATD server or client should treat this document, together with
`docs/protocol/error-codes.md`, as the spec for v0.1.0 compatibility.

---

## 1. Overview

ATD (Agent Tool Dispatch) is a **request/response** protocol that lets an LLM agent
discover tools on a server, inspect their schemas, and invoke them. The protocol is:

- **Length-prefixed JSON** — every frame is a 4-byte big-endian `u32` length followed
  by a UTF-8 JSON object. There is no other framing (no HTTP headers, no XML, no
  custom binary encoding).
- **Transport-agnostic in design, Unix-socket-first in v0.1.0** — the reference
  implementation (`atd-sdk`) connects over a Unix domain socket. Stdio transport
  is planned for Phase 1; HTTP REST is Phase 2.
- **Synchronous request/response** — each request produces exactly one response.
  There is no multiplexing or streaming in v0.1.0. In-flight concurrency is achieved
  by the caller opening multiple connections.
- **Stateless sessions** — the server does not track session state between connections.
  A `session.start` / `session.end` message pair is planned for Phase 1 but is not
  implemented in v0.1.0.

### Non-goals for v0.1.0

- HTTP or WebSocket transport
- Server-initiated push / event streaming (`subscribe` message type)
- Multiplexed requests over a single connection
- TLS or authentication at the protocol layer (use OS-level socket permissions)
- Session continuity across reconnections

See [`../architecture.md`](../architecture.md) for the higher-level layer model this wire protocol implements. The architecture doc describes the three core mechanisms (schema, dispatch, security) and points back to this document for byte-level detail.

---

## 2. Framing

Every frame — both client-to-server (request) and server-to-client (response) — uses
the same framing structure:

```
┌─────────────────────────────────────────────────────┐
│  Length prefix  │            JSON body               │
│   4 bytes BE    │         len bytes UTF-8            │
└─────────────────────────────────────────────────────┘
```

### 2.1 Length prefix

A **4-byte big-endian unsigned 32-bit integer** (`u32`) encoding the byte length of
the JSON body that follows. The prefix encodes the number of bytes in the body only —
it does not include the 4 bytes of the prefix itself.

Maximum body size: **10 MiB** (`10 * 1024 * 1024` bytes). The reference client
(`crates/atd-sdk/src/wire.rs`) rejects frames larger than this limit before
allocating the receive buffer. Compliant servers must also enforce this limit on
incoming request frames.

### 2.2 JSON body

A UTF-8-encoded JSON object. The top-level field `"type"` (for request frames) or
`"type"` (for response frames) acts as the discriminant. All other fields depend on
the message type (see §4).

Serialization rules used throughout:

- `serde_json` with `serde(tag = "type")` — the type tag is always the JSON key `"type"`.
- Enums use `serde(rename_all = "snake_case")` unless otherwise noted.
- Optional fields with `serde(skip_serializing_if = "Option::is_none")` are omitted
  entirely when `None` — they do not appear as `null`.
- `#[serde(default)]` fields deserialize to their Rust default when absent.

### 2.3 Hex dump example

The simplest valid request is `ping`. Its JSON body is `{"type":"ping"}` — 15 bytes.

```
Offset  Bytes (hex)                        ASCII
00      00 00 00 0F                        ....   ← 4-byte BE u32, value = 15
04      7B 22 74 79 70 65 22 3A 22 70      {"type":"p
0E      69 6E 67 22 7D                     ing"}
```

Complete 19-byte frame on the wire:

```
00 00 00 0F 7B 22 74 79 70 65 22 3A 22 70 69 6E 67 22 7D
```

### 2.4 Implementation reference

The framing logic lives entirely in `crates/atd-sdk/src/wire.rs`:

```rust
// write_frame: serialize T to JSON, write 4-byte BE length, write body, flush.
pub async fn write_frame<W, T>(writer: &mut W, msg: &T) -> std::io::Result<()>

// read_frame: read 4-byte BE length, validate ≤ 10 MiB, read body, deserialize T.
pub async fn read_frame<R, T>(reader: &mut R) -> std::io::Result<T>
```

These two functions are the complete framing implementation. A third-party client or
server needs only to replicate this logic.

---

## 3. Connection Lifecycle

```
Client                          Server
  │                               │
  │── connect (Unix socket) ─────▶│  (TCP accept / socket accept)
  │                               │
  │── ping ──────────────────────▶│
  │◀─ pong ────────────────────── │
  │                               │
  │── tool_list ─────────────────▶│
  │◀─ tool_list (response) ────── │
  │                               │
  │── tool_schema {tool_id} ─────▶│
  │◀─ tool_schema (response) ──── │
  │                               │
  │── run_tool {tool_id, args} ──▶│
  │◀─ tool_result ──────────────  │
  │                               │
  │── … (repeat) ────────────────▶│
  │                               │
  │── disconnect ────────────────▶│  (close socket)
  │                               │
```

### 3.1 Connection

The client opens a Unix domain socket to the path in `ATD_SOCK` (default:
`/tmp/atd.sock`). No handshake frame is required. The server accepts the connection
and is immediately ready to receive request frames.

### 3.2 Ping / liveness check

After connecting, the client may send a `ping` to verify the server is alive. This is
optional; `tool_list` is valid as the first frame.

### 3.3 Request/response ordering

Each connection carries at most one in-flight request at a time. The client sends a
request and then reads exactly one response before sending the next request. The
protocol does not assign request IDs in v0.1.0; multiplexing is not supported.

### 3.4 Disconnection

Closing the TCP/socket connection is the only disconnect signal. Servers must handle
`EOF` gracefully. There is no explicit disconnect message in v0.1.0.

### 3.5 Future: session framing (Phase 1)

`session.start` and `session.end` messages are planned. When implemented, they will
carry session metadata (client version, capability flags) and allow the server to
correlate multiple requests within a single session. Until then, each request is
stateless.

---

## 4. Message Types

All request types are variants of `enum Request` (`crates/atd-sdk/src/protocol.rs`).
All response types are variants of `enum Response`.

The `"type"` tag discriminates both enums.

### 4.1 `ping` / `pong`

**Purpose:** Liveness check. No payload.

Request:

```json
{"type": "ping"}
```

Response:

```json
{"type": "pong"}
```

Use this to verify connectivity before the first meaningful request, or to keep a
long-lived connection alive.

### 4.2 `tool_list` — discover tools

**Purpose:** Return the list of tools registered on the server, each as a `ToolSummary`.

Request:

```json
{"type": "tool_list"}
```

Response:

```json
{
  "type": "tool_list",
  "tools": [
    {
      "id": "ref:fs.read",
      "name": "Read File",
      "description": "Read a UTF-8 text file from disk.",
      "domain": "fs",
      "tags": ["filesystem", "read"],
      "visibility": "read",
      "tier": "warm",
      "input_schema": {
        "type": "object",
        "properties": {
          "path": {"type": "string"}
        },
        "required": ["path"]
      }
    }
  ]
}
```

`tools` is a JSON array of `ToolSummary` objects (see §5.1 for full field table).

The reference client wraps this as `AtdClient::discover()`. The method takes an
optional `DiscoverFilter` that post-processes the returned array client-side (by
domain, tag, or text query); the server always returns the full list.

**Note:** `input_schema` is omitted from `ToolSummary` when the server has no schema
for a tool. Clients must treat its absence as "schema unknown" and must not assume
`{}`.

### 4.3 `tool_schema` — describe a tool

**Purpose:** Return the full `ToolDefinition` for a specific tool, including its
complete input/output schemas, binding configuration, safety metadata, and trust level.

Request:

```json
{"type": "tool_schema", "tool_id": "ref:fs.read"}
```

Response (success):

```json
{
  "type": "tool_schema",
  "schema": {
    "id": "ref:fs.read",
    "name": "Read File",
    "description": "Read a UTF-8 text file from disk.",
    "version": "0.1.0",
    "capability": {
      "domain": "fs",
      "actions": ["read"],
      "tags": ["filesystem"],
      "intent_examples": ["read config.toml", "show /etc/hosts"]
    },
    "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]},
    "output_schema": {"type": "object", "properties": {"content": {"type": "string"}}},
    "bindings": [{"protocol": "Cli", "config": {"cmd": "cat"}}],
    "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
    "resources": {"timeout_ms": 5000, "max_concurrent": 8, "rate_limit_per_min": null, "estimated_tokens": 100},
    "trust": {"publisher": "anos", "trust_level": "L3Verified", "signature": null},
    "visibility": "read"
  }
}
```

The reference client wraps this as `AtdClient::describe(tool_id)`.

If the tool is not found, the server returns a response with `type: "error"` (see §4.5).

### 4.4 `run_tool` — call a tool

**Purpose:** Execute a tool with the given arguments and return its result.

Request:

```json
{
  "type": "run_tool",
  "tool_id": "ref:fs.read",
  "args": {"path": "/etc/hostname"},
  "dry_run": false
}
```

Fields:

| field | type | description |
|---|---|---|
| `tool_id` | `String` | Canonical tool identifier (e.g., `"ref:fs.read"`) |
| `args` | `Object` | Arguments matching the tool's `input_schema`. Must be a JSON object. |
| `dry_run` | `bool` | If `true`, validate but do not execute. Not all tools implement dry-run. |

Response (success):

```json
{
  "type": "tool_result",
  "tool_id": "ref:fs.read",
  "result": {"content": "atd-server\n"},
  "success": true,
  "dry_run": false
}
```

Response (error):

```json
{
  "type": "tool_result",
  "tool_id": "ref:fs.read",
  "result": {
    "status": "error",
    "code": "IO",
    "message": "No such file or directory (os error 2)",
    "reason": null,
    "retryable": false
  },
  "success": false,
  "dry_run": false
}
```

The reference client wraps this as `AtdClient::call(tool_id, args, options)`.

The `result` field contains a serialized `ToolResult` (see §5.3 for full field table).

### 4.5 Protocol-level `error` response

For request-parsing failures or unknown message types, the server returns an `error`
response independent of any specific tool:

```json
{
  "type": "error",
  "message": "unknown request type: foo",
  "code": 400,
  "retryable": false,
  "details": null
}
```

Fields:

| field | type | nullable | description |
|---|---|---|---|
| `message` | `String` | no | Human-readable error description |
| `code` | `u16` | yes | HTTP-like status code. Absent if not applicable. |
| `retryable` | `bool` | yes | Whether the caller should retry. Absent = unknown. |
| `details` | `Object` | yes | Additional structured context. Server-defined. |

### 4.6 `hello` / `hello_ack` — capability handshake (SP-12)

Optional connection-scoped handshake: the client declares the capabilities it
would like to hold, and the server replies with the subset its
`--grant-capability` allow-list authorizes. Subsequent `run_tool` calls use
this subset to enforce each tool's `required_capabilities`.

**Request:**

```json
{
  "type": "hello",
  "client_id": "my-agent-7",          // optional; free-form string for logs
  "requested_capabilities": ["exec", "read"]
}
```

**Response:**

```json
{
  "type": "hello_ack",
  "granted_capabilities": ["exec"],   // intersection of requested + allow-list
  "server_version": "atd-ref-server 0.2.0",
  "supported_tiers": ["hot", "warm", "cold"]
}
```

**Rules:**

- `hello` is optional. A client that skips it runs with an empty capability
  set — fine for tools declaring no `required_capabilities`, refused with
  `code: 1001` otherwise.
- `hello` is idempotent within a connection: re-sending with a different set
  **replaces** the stored set (does not union).
- Pre-SP-12 servers reply with a generic `type: "error"`. SDKs (`atd-sdk`,
  `atd_sdk`) demote this to "no capabilities granted" so a single
  `hello()` call works against any server version.

#### SP-capability-v2 additive: `ucan_tokens` field

A client holding one or more UCAN-lite tokens (JWT compact form, `alg=EdDSA`,
`typ=ucan/1.0+jwt`) MAY include them as an additional Hello field:

```json
{
  "type": "hello",
  "client_id": "agent-B",
  "requested_capabilities": ["records:read"],
  "ucan_tokens": ["<A-signed-B UCAN JWT compact>"]
}
```

When `ucan_tokens` is non-empty, the server verifies each chain
independently (signature, audience pinning, attenuation, depth limit,
revocation store) and unions the resulting capabilities with the SP-12
string-allow-list result:

```
granted = (server_allow_list ∩ requested_capabilities) ∪ ucan_derived_caps
```

`ucan_tokens` MUST be omitted from the wire form when empty (serde
`skip_serializing_if = "Vec::is_empty"`); pre-SP-capability-v2 servers
that don't recognize the field will accept the empty-omitted form
byte-identically to SP-12.

Verification failures map to four error codes (see §4.7 below):
- `1010 ERR_UCAN_INVALID` — parse, signature, alg, DID method, missing field, attenuation widening
- `1011 ERR_UCAN_EXPIRED` — any link's `exp <= now()`
- `1012 ERR_DELEGATION_TOO_DEEP` — chain depth > `ServerConfig.max_ucan_chain_depth` (default 5)
- `1013 ERR_AUDIENCE_MISMATCH` — deepest UCAN's `aud` ≠ connection `client_id`

Full UCAN-lite profile (Ed25519 only, `did:key` only, `cmd="atd-cap"`,
`args.caps: Vec<String>`) defined in `SP-capability-v2` spec §4.

### 4.7 Capability-denied error (SP-12)

When `run_tool` targets a tool whose `required_capabilities` are not a subset
of the connection's granted set, the server returns:

```json
{
  "type": "error",
  "code": 1001,
  "retryable": false,
  "message": "capability denied for ref:x: missing [\"exec\"]",
  "details": {
    "required": ["exec"],
    "granted": [],
    "missing": ["exec"]
  }
}
```

`code = 1001` is the stable wire value (`ERR_CAPABILITY_DENIED`). Both the
Rust and Python SDKs map this to a typed `CapabilityDenied` exception
carrying both the required and granted lists verbatim.

### 4.8 Future message types (not in v0.1.0)

The following types are documented here for implementers planning forward compatibility.
The reference server returns `type: "error"` with `message: "not implemented"` if it
receives any of these:

| type | planned phase | purpose |
|---|---|---|
| `session.start` | Phase 1 | Open a named session; server returns a `session_id` |
| `session.end` | Phase 1 | Close session; server may flush per-session state |
| `cancel` | Phase 1 | Cancel an in-progress `run_tool` by request ID |
| `subscribe` | Phase 2 | Register for server-push events (streaming results) |

---

## 5. Full Type Definitions

Source: `crates/atd-protocol/src/` — all tables are derived from the Rust source at
commit `sp10-adapters`. Field names match the Rust struct field names; where serde
applies a rename, the wire name is shown separately.

### 5.1 `ToolSummary`

Source: `crates/atd-protocol/src/summary.rs`

The lightweight representation returned by `tool_list`. Intended for display and
filtering without pulling the full definition.

| field | wire key | type | serde default | description |
|---|---|---|---|---|
| `id` | `"id"` | `String` | required | Canonical tool identifier, e.g., `"ref:fs.read"` |
| `name` | `"name"` | `String` | `""` | Human-readable name |
| `description` | `"description"` | `String` | required | One-sentence description |
| `domain` | `"domain"` | `String` | `""` | Capability domain, e.g., `"fs"`, `"shell"`, `"web"` |
| `tags` | `"tags"` | `Vec<String>` | `[]` | Searchable keyword tags |
| `visibility` | `"visibility"` | `ToolVisibility` | `"read"` | Access level (see §5.6) |
| `tier` | `"tier"` | `ToolTier` | `"warm"` | Latency tier (see §5.7) |
| `input_schema` | `"input_schema"` | `Object \| null` | absent | JSON Schema for the tool's arguments. Omitted when not available. |

`input_schema` uses `skip_serializing_if = "Option::is_none"` — absent from the wire
when `None`. Clients must handle both present and absent cases.

### 5.2 `ToolDefinition`

Source: `crates/atd-protocol/src/tool.rs`

The full tool spec returned by `tool_schema`. Contains everything needed to call,
validate, and trust a tool.

| field | wire key | type | serde default | description |
|---|---|---|---|---|
| `id` | `"id"` | `String` | required | Canonical identifier |
| `name` | `"name"` | `String` | required | Human-readable name |
| `description` | `"description"` | `String` | required | Short description |
| `version` | `"version"` | `String` | required | SemVer string for the tool itself |
| `capability` | `"capability"` | `ToolCapability` | required | Domain + actions + tags + examples |
| `input_schema` | `"input_schema"` | `Object` | required | JSON Schema (draft-07 compatible) |
| `output_schema` | `"output_schema"` | `Object` | required | JSON Schema for the return value |
| `bindings` | `"bindings"` | `Vec<ToolBinding>` | required | One or more server-side bindings |
| `safety` | `"safety"` | `ToolSafety` | required | Safety level + side-effect declarations |
| `resources` | `"resources"` | `ToolResources` | required | Timeout + concurrency limits |
| `trust` | `"trust"` | `ToolTrust` | required | Publisher + trust level + optional signature |
| `visibility` | `"visibility"` | `ToolVisibility` | `"read"` | Access level |

#### 5.2.1 `ToolCapability`

| field | wire key | type | description |
|---|---|---|---|
| `domain` | `"domain"` | `String` | Broad capability category: `"fs"`, `"shell"`, `"web"`, `"echo"` |
| `actions` | `"actions"` | `Vec<String>` | Fine-grained action verbs: `["read"]`, `["exec"]` |
| `tags` | `"tags"` | `Vec<String>` | Free-form keyword tags |
| `intent_examples` | `"intent_examples"` | `Vec<String>` | Natural-language example prompts for LLM routing |

#### 5.2.2 `ToolBinding`

| field | wire key | type | description |
|---|---|---|---|
| `protocol` | `"protocol"` | `BindingProtocol` | How the server executes this tool (see §5.8) |
| `config` | `"config"` | `Object` | Protocol-specific config. Shape varies by protocol. |

#### 5.2.3 `ToolSafety`

| field | wire key | type | description |
|---|---|---|---|
| `level` | `"level"` | `SafetyLevel` | Risk classification (see §5.9) |
| `dry_run` | `"dry_run"` | `bool` | Whether dry-run mode is supported |
| `side_effects` | `"side_effects"` | `Vec<String>` | List of declared side effects |
| `data_sensitivity` | `"data_sensitivity"` | `String \| null` | Optional sensitivity label (e.g., `"PII"`) |

#### 5.2.4 `ToolResources`

| field | wire key | type | description |
|---|---|---|---|
| `timeout_ms` | `"timeout_ms"` | `u64` | Maximum execution time in milliseconds |
| `max_concurrent` | `"max_concurrent"` | `u32` | Maximum concurrent invocations on this server |
| `rate_limit_per_min` | `"rate_limit_per_min"` | `u32 \| null` | Optional rate limit; absent when none |
| `estimated_tokens` | `"estimated_tokens"` | `u32 \| null` | Estimated LLM token cost; absent when unknown |

#### 5.2.5 `ToolTrust`

| field | wire key | type | description |
|---|---|---|---|
| `publisher` | `"publisher"` | `String` | Publisher identifier (e.g., `"anos"`, `"community"`) |
| `trust_level` | `"trust_level"` | `TrustLevel` | Verification level (see §5.10) |
| `signature` | `"signature"` | `[u8] \| null` | Optional binary signature; absent when not signed |

### 5.3 `ToolResult`

Source: `crates/atd-protocol/src/result.rs`

The serde discriminant is `"status"` (not `"type"`), using `serde(tag = "status", rename_all = "snake_case")`.

**Success variant:**

```json
{
  "status": "success",
  "data": { ... },
  "metadata": { "tool_id": "ref:fs.read", "latency_ms": 12, "binding": "Cli" }
}
```

| field | wire key | type | description |
|---|---|---|---|
| `status` | `"status"` | `"success"` | Discriminant tag |
| `data` | `"data"` | `Object` | Tool-specific output. Shape defined by `output_schema`. |
| `metadata` | `"metadata"` | `ToolResultMetadata` | Execution metadata |

**Error variant:**

```json
{
  "status": "error",
  "code": "IO",
  "message": "No such file or directory (os error 2)",
  "reason": null,
  "retryable": false
}
```

| field | wire key | type | description |
|---|---|---|---|
| `status` | `"status"` | `"error"` | Discriminant tag |
| `code` | `"code"` | `String` | Machine-readable error code (see `docs/protocol/error-codes.md` §3) |
| `message` | `"message"` | `String` | Human-readable description |
| `reason` | `"reason"` | `String \| null` | Optional extended explanation |
| `retryable` | `"retryable"` | `bool` | Whether the caller should retry the call |

### 5.4 `ToolResultMetadata`

Source: `crates/atd-protocol/src/result.rs`

All fields except `tool_id` are optional and server-populated. Clients must not
fabricate metadata fields they did not receive — doing so could silently masquerade
as server truth.

| field | wire key | type | serde | description |
|---|---|---|---|---|
| `tool_id` | `"tool_id"` | `String` | required | Server echoes the called tool's id |
| `version` | `"version"` | `String \| null` | omit when None | Tool version that executed |
| `binding` | `"binding"` | `BindingProtocol \| null` | omit when None | Which binding handled execution |
| `latency_ms` | `"latency_ms"` | `u64 \| null` | omit when None | Wall-clock execution time |
| `timestamp` | `"timestamp"` | `String \| null` | omit when None | ISO-8601 / RFC-3339 server time |
| `request_id` | `"request_id"` | `String \| null` | omit when None | Opaque request ID (ULID, UUID, etc.) |

Minimal wire form (server required to emit):

```json
{"tool_id": "ref:fs.read"}
```

### 5.5 `Request` enum

Source: `crates/atd-sdk/src/protocol.rs`. Discriminant field: `"type"`.

| variant | wire `"type"` | fields |
|---|---|---|
| `Ping` | `"ping"` | (none) |
| `ToolList` | `"tool_list"` | (none) |
| `ToolSchema` | `"tool_schema"` | `tool_id: String` |
| `RunTool` | `"run_tool"` | `tool_id: String`, `args: Object`, `dry_run: bool` |

### 5.6 `Response` enum

Source: `crates/atd-sdk/src/protocol.rs`. Discriminant field: `"type"`.

| variant | wire `"type"` | fields |
|---|---|---|
| `Pong` | `"pong"` | (none) |
| `ToolListResponse` | `"tool_list"` | `tools: Array<ToolSummary>` |
| `ToolSchemaResponse` | `"tool_schema"` | `schema: ToolDefinition` |
| `ToolResultResponse` | `"tool_result"` | `tool_id: String`, `result: ToolResult`, `success: bool`, `dry_run: bool` |
| `Error` | `"error"` | `message: String`, `code?: u16`, `retryable?: bool`, `details?: Object` |

### 5.7 `ToolVisibility` enum

Source: `crates/atd-protocol/src/enums.rs`. Serde: `rename_all = "snake_case"`, also accepts `PascalCase` aliases.

| variant | wire value | meaning |
|---|---|---|
| `Read` | `"read"` | Safe for read-only agents. Default. |
| `Write` | `"write"` | Requires write-capable agent. |
| `Dangerous` | `"dangerous"` | Potentially destructive; agents must opt-in. |
| `System` | `"system"` | Administrative; reserved for system agents. |

### 5.8 `ToolTier` enum

Source: `crates/atd-protocol/src/enums.rs`. Serde: `rename_all = "snake_case"`.

| variant | wire value | meaning |
|---|---|---|
| `Hot` | `"hot"` | In-memory / sub-millisecond latency |
| `Warm` | `"warm"` | Process-level (default) — typical shell/file tool |
| `Cold` | `"cold"` | Network-bound or on-demand startup |

Ordering: `Hot < Warm < Cold` (used when filtering by acceptable latency tier).

### 5.9 `BindingProtocol` enum

Source: `crates/atd-protocol/src/enums.rs`. Serde: `rename_all = "PascalCase"`.

| variant | wire value | meaning |
|---|---|---|
| `Cli` | `"Cli"` | Shell command execution |
| `Mcp` | `"Mcp"` | Forwards to a downstream MCP server |
| `Rest` | `"Rest"` | HTTP REST proxy (Phase 2) |
| `AppFunction` | `"AppFunction"` | Native application function binding (Phase 2) |

### 5.10 `SafetyLevel` enum

Source: `crates/atd-protocol/src/enums.rs`. Serde: PascalCase (no rename directive).

| variant | wire value | ordinal | meaning |
|---|---|---|---|
| `Read` | `"Read"` | 0 | No mutation. Safest. |
| `Write` | `"Write"` | 1 | Filesystem or state mutation |
| `Financial` | `"Financial"` | 2 | Monetary transactions |
| `Privacy` | `"Privacy"` | 3 | Personal data access or exfiltration risk |
| `Physical` | `"Physical"` | 4 | Physical world actuation |
| `Destructive` | `"Destructive"` | 5 | Irreversible deletion or damage |

Ordering is strictly monotonic: `Read < Write < … < Destructive`.

### 5.11 `TrustLevel` enum

Source: `crates/atd-protocol/src/enums.rs`. Serde: PascalCase (no rename directive).

| variant | wire value | ordinal | meaning |
|---|---|---|---|
| `L0Unverified` | `"L0Unverified"` | 0 | No verification. Use at own risk. |
| `L1SchemaValid` | `"L1SchemaValid"` | 1 | JSON schema is syntactically valid |
| `L2Tested` | `"L2Tested"` | 2 | Tested by the publisher |
| `L3Verified` | `"L3Verified"` | 3 | Reviewed and verified by a third party |
| `L4Certified` | `"L4Certified"` | 4 | Formally certified (process TBD) |

---

## 6. Server Bindings

A "binding" describes how the ATD server implements a tool's execution. Each
`ToolDefinition` carries a `bindings: Vec<ToolBinding>` array. In v0.1.0, the
reference server implements two binding protocols.

### 6.1 CLI binding (`"Cli"`)

The tool is implemented as a shell command invocation. The server runs the command,
captures stdout/stderr, and returns the result.

Example `ToolBinding.config` for a CLI tool:

```json
{
  "protocol": "Cli",
  "config": {"cmd": "cat"}
}
```

The reference server's `ref:shell.exec` tool is the primary CLI-binding exemplar.
It accepts `command: String` as its sole argument and runs it in a sandboxed process.

```json
{
  "type": "run_tool",
  "tool_id": "ref:shell.exec",
  "args": {"command": "uname -s"},
  "dry_run": false
}
```

Response:

```json
{
  "type": "tool_result",
  "tool_id": "ref:shell.exec",
  "result": {
    "status": "success",
    "data": {"exit_code": 0, "stdout": "Linux\n", "stderr": ""},
    "metadata": {"tool_id": "ref:shell.exec"}
  },
  "success": true,
  "dry_run": false
}
```

### 6.2 MCP binding (`"Mcp"`)

The ATD server acts as an MCP client and forwards the tool call to a downstream
MCP server. The `atd-mcp-bridge` is the inverse of this: it accepts MCP connections
and translates them to ATD protocol calls (see §10 for the MCP compatibility map).

MCP binding is declared in `ToolDefinition` but is not the primary execution path for
any reference server tool in v0.1.0. It is included in the type system for
future third-party server implementations that want to aggregate MCP tools behind an
ATD interface.

### 6.3 REST binding (`"Rest"`) — Phase 2

REST binding means the server proxies tool calls to an HTTP REST endpoint. This is
declared in `BindingProtocol` but not yet implemented in the reference server.

Phase 2 design: the `config` object will carry `url`, `method`, `headers_template`,
and `body_template` fields.

### 6.4 AppFunction binding (`"AppFunction"`) — Phase 2

AppFunction binding enables a tool to be implemented as a native application
function call — a hardware-or-OS-level hook for physical device control (e.g., smart
home actuators, robotic arms). This requires an AppFunction runtime that is out of
scope for v0.1.0.

### 6.5 Which tools use which bindings

The reference server (`atd-ref-server`) ships 9 tools in v0.1.0, all using `Cli`
binding:

| tool id | domain | binding |
|---|---|---|
| `ref:echo.say` | echo | `Cli` |
| `ref:fs.read` | fs | `Cli` |
| `ref:fs.write` | fs | `Cli` |
| `ref:fs.edit` | fs | `Cli` |
| `ref:fs.glob` | fs | `Cli` |
| `ref:fs.grep` | fs | `Cli` |
| `ref:shell.exec` | shell | `Cli` |
| `ref:shell.pwsh` | shell | `Cli` |
| `ref:web.fetch` | web | `Cli` |

---

## 7. Error Propagation

There are two error layers. See `docs/protocol/error-codes.md` for the complete
reference. The distinction is important:

### 7.1 Client-side errors (`AtdError`)

The Rust client SDK (`atd-sdk`) converts transport problems, protocol violations,
and high-level semantic errors into the `AtdError` enum (`crates/atd-protocol/src/error.rs`).
These are never serialized over the wire; they are synthesized by the client library
before or after deserialization.

Examples:
- `AtdError::ServerUnreachable` — `connect()` fails because the socket does not exist
- `AtdError::ProtocolError` — server response has unexpected shape
- `AtdError::Timeout` — `run_tool` exceeds the configured deadline

### 7.2 Server-side errors (`ToolResult::Error`)

When a tool invocation fails on the server, the server returns a `ToolResult` with
`status: "error"` containing a `code` string, a `message`, and a `retryable` flag.
These errors travel over the wire and are delivered to the caller as a `ToolResult`
variant, not as an `AtdError`.

Wire shape:

```json
{
  "status": "error",
  "code": "TIMEOUT",
  "message": "command timed out after 30000ms",
  "reason": null,
  "retryable": true
}
```

The set of code strings actually emitted by the reference server's tools is enumerated
in `docs/protocol/error-codes.md` §3.

### 7.3 Protocol-level errors

For requests the server cannot parse or route (unknown `"type"`, malformed JSON, frame
too large), the server returns a `Response::Error`:

```json
{"type": "error", "message": "...", "code": 400, "retryable": false}
```

This is distinct from a tool execution error — the tool was never invoked.

---

## 8. Versioning

### 8.1 Protocol version

v0.1.0 follows the **SemVer 0.x contract**: breaking changes are allowed between any
two 0.x releases. There is no in-band version negotiation in v0.1.0. A third-party
server claiming ATD 0.1.0 compatibility must implement all four request types
(`ping`, `tool_list`, `tool_schema`, `run_tool`) and return correctly-shaped responses.

At 1.0, the protocol stability promise kicks in: no required fields may be removed, no
existing wire values may change meaning.

### 8.2 What constitutes a breaking change

| change | breaking? |
|---|---|
| Adding a new required field to a request or response | Yes |
| Removing an existing field | Yes |
| Changing the wire name of an existing field | Yes |
| Adding an optional field (`skip_serializing_if = "Option::is_none"`) | No |
| Adding a new enum variant | No (unknown variants should be tolerated by well-written clients) |
| Adding a new request type | No |
| Changing `code` strings in `ToolResult::Error` | Yes (they are a stability surface) |

### 8.3 Version detection

There is no `version` field in the `ping`/`pong` exchange in v0.1.0. Clients that
need to detect server version should call `tool_list` and check for the presence of
known tools or fields. A version negotiation handshake is planned for Phase 1.

---

## 9. Extension Points

### 9.1 The `_atd` MCP extension object

When the `atd-mcp-bridge` converts an ATD `ToolDefinition` to an MCP tool description,
it adds an `_atd` field to the MCP `inputSchema` that carries ATD-specific metadata
not expressible in the standard MCP tool shape.

Example MCP tool description with `_atd` extension:

```json
{
  "name": "ref_fs_read",
  "description": "Read a UTF-8 text file from disk.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": {"type": "string"}
    },
    "required": ["path"],
    "_atd": {
      "tool_id": "ref:fs.read",
      "visibility": "read",
      "safety_level": "Read",
      "tier": "warm"
    }
  }
}
```

The `_atd` extension is informational. MCP clients that do not understand it silently
ignore it. The `atd-mcp-bridge` uses it on the return path to correlate the sanitized
MCP tool name back to the original ATD tool id.

### 9.2 Custom fields in `ToolBinding.config`

`ToolBinding.config` is an untyped `serde_json::Value`. Third-party server
implementations may add arbitrary fields here to carry binding-specific configuration.
The reference server uses this for CLI command strings. Future REST and AppFunction
bindings will carry their own config schemas.

### 9.3 Unknown fields in JSON objects

The ATD protocol uses `serde` with its default behavior of ignoring unknown fields on
deserialization. This means:

- A v0.1.1 server can add optional fields to its responses; a v0.1.0 client ignores them.
- A v0.1.1 client can send optional request fields; a v0.1.0 server ignores them.

This is intentional. Server implementers should not break on unknown fields.

---

## 10. Reference: MCP Compatibility

The `atd-mcp-bridge` maps between the ATD protocol and MCP's JSON-RPC interface.
This section describes the mapping, which is useful for third-party implementers who
want to expose ATD tools via MCP or vice versa.

### 10.1 Request type mapping

| ATD request | MCP equivalent | notes |
|---|---|---|
| `ping` | (heartbeat / `tools/list` probe) | MCP has no explicit ping; clients poll |
| `tool_list` | `tools/list` | Returns `tools` array |
| `tool_schema` | `tools/list` + filter by name | MCP has no `describe` analog; schema is in the list |
| `run_tool` | `tools/call` | Payload differs (see §10.2) |

### 10.2 Tool call mapping (`run_tool` → `tools/call`)

ATD request:

```json
{"type": "run_tool", "tool_id": "ref:fs.read", "args": {"path": "/etc/hosts"}, "dry_run": false}
```

MCP request (via `atd-mcp-bridge`):

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "ref_fs_read",
    "arguments": {"path": "/etc/hosts"}
  }
}
```

The bridge desanitizes the MCP tool name (`ref_fs_read`) back to the ATD id
(`ref:fs.read`) using the known-id list from the last `tool_list` response.

### 10.3 Name sanitization rules

ATD tool ids use `:` and `.` for namespace/domain/action structure. MCP and LLM APIs
require names matching `[a-zA-Z0-9_-]`. The sanitization rule (from
`crates/atd-sdk/src/sanitize.rs`) is:

- Replace `:` with `_`
- Replace `.` with `_`
- Replace any other character outside `[a-zA-Z0-9_-]` with `_`

Examples:

| ATD tool id | Sanitized MCP name |
|---|---|
| `ref:fs.read` | `ref_fs_read` |
| `ref:shell.exec` | `ref_shell_exec` |
| `ref:echo.say` | `ref_echo_say` |
| `ref:web.fetch` | `ref_web_fetch` |

**Important:** sanitization is lossy. `a:b` and `a.b` both map to `a_b`. If a tool
registry contains two tools whose ids sanitize to the same string, a collision occurs.
Use `detect_collisions()` from `atd-sdk::sanitize` to check before exposing tools
via MCP.

### 10.4 Error mapping

| MCP error condition | ATD equivalent |
|---|---|
| `tools/call` returns MCP error code `-32601` (method not found) | `ToolResult::Error { code: "NOT_FOUND" }` |
| `tools/call` returns MCP error code `-32602` (invalid params) | `ToolResult::Error { code: "INVALID_ARGS" }` |
| Transport-level failure | `AtdError::ServerUnreachable` |

---

## 11. Skills meta-tool convention

This is a **convention**, not a wire-protocol message. ATD servers that
publish skill files (e.g., SKILL.md content for agent platforms) expose
two meta-tools at fixed ids:

### 11.1 `<publisher>:<service>.skills.list`

**Args:** `{}` (no fields)

**Returns:** `Vec<SkillSummary>` where each entry is:

```json
{"name": "healthkit-heartrate", "description": "Query heart rate data", "version": null}
```

- `name: String` — slug, unique within the service. Lookup key for `skills.get`.
- `description: String` — one-line summary; matches SKILL.md frontmatter `description` if present.
- `version: Option<String>` — reserved for future per-skill semver; servers MAY omit.

**Required capabilities:** none.

### 11.2 `<publisher>:<service>.skills.get`

**Args:** `{"name": "<slug>"}`

**Returns:**

```json
{"name": "healthkit-heartrate", "content_md": "---\nname: healthkit-heartrate\n---\n…"}
```

`content_md` is the full skill file content, UTF-8, markdown by convention.

**Errors:** Unknown name returns `ToolCallError::ExecutionFailed { code: "skill_not_found", retryable: false }`.

**Required capabilities:** none.

### 11.3 What this is NOT

- Not a wire-level `Request::SkillList` / `Request::SkillGet` — pure tool-id naming. Adoption is opt-in.
- Not a SKILL.md parsing contract — ATD does not validate frontmatter or markdown.
- Not version-aware in v0 — `version` field is reserved but not enforced.

### 11.4 Future evolution

If 2+ vendor servers adopt this convention without divergence, a future
SP can promote it to a wire-level `Request::SkillList` / `Request::SkillGet`.
Until then, convention-only.

### 11.5 See also

- `atd skills sync` subcommand (atd-cli) — pulls skills via this convention into per-platform directories (hermes / claude-code / stdout)
- `docs/superpowers/specs/2026-04-27-sp-skills-discovery-convention-design.md` — full design rationale
- First adopter: `healthkit_cli` v1.3.0 — exposes 26 SKILL.md via `huawei:hms.healthkit.skills.list/get`

---

*End of wire format reference. See `docs/protocol/error-codes.md` for the full error taxonomy.*
