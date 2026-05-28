# Adding a transport / listener

**Purpose:** make ATD reachable over a new wire medium — WebSocket, vsock,
QUIC, a message queue — by writing a listener crate that translates the
medium's framing into ATD messages and calls the shared dispatcher.

## When to use this

Use this when ATD needs to be reachable over a medium the two shipped
transports do not cover. The two references:

- **`atd-server`** — Unix-socket listener.
- **`atd-server-http`** — HTTP + MCP JSON-RPC listener.

A transport is the **one extension point that is not a trait impl**. There is
no `Transport` trait — a transport is a *crate* that owns an accept loop and
calls into `atd-runtime`. The seam is small and explicit, which is the point.

## The key seam

```
your medium → frame → ClientMessage  ──►  atd_runtime::dispatch::dispatch_request
                                              │   (or run_tool for a single call)
                ServerMessage ← frame  ◄──────┘
```

A listener does exactly three things:

1. accept connections on its medium,
2. decode the medium's framing into `atd_protocol::Request` (`ClientMessage`)
   and encode `atd_protocol::Response` (`ServerMessage`) back,
3. call `atd_runtime::dispatch::dispatch_request` (or `run_tool`) and ship the
   response.

Everything past the seam — capability gate, tier deadlines, binding selection,
token-broker resolution, middleware, cursor handling, audit, error mapping — is
shared. A new transport adds **zero** dispatch logic. The wire shape it produces
is byte-identical to the other transports because they all deserialise into the
same `atd-protocol` types.

## The two dispatch entry points

Both live in `crates/atd-runtime/src/dispatch.rs`:

```rust
// Full Request state machine: Ping / Hello / ToolList / ToolSchema /
// RunTool / RunToolContinue. `caps` and `caller_id` are &mut because a
// Hello rewrites them for the lifetime of a stream-oriented connection.
pub async fn dispatch_request(
    state: &Arc<ServerState>,
    tracker: &Arc<ReadTracker>,
    caps: &mut Arc<CapabilitySet>,
    caller_id: &mut Option<String>,
    req: Request,
) -> Response;

// The RunTool arm alone — for transports that derive capabilities and
// caller identity fresh per request rather than per connection.
pub async fn run_tool(
    state: &Arc<ServerState>,
    tracker: &Arc<ReadTracker>,
    caps: &Arc<CapabilitySet>,
    caller_id: Option<&str>,
    tool_id: String,
    args: serde_json::Value,
    dry_run: bool,
) -> Response;
```

**Stream-oriented transports** (UDS, WebSocket, vsock) keep per-connection
state across many requests on one connection — use `dispatch_request`, which
mutates `caps`/`caller_id` in place when a `Hello` arrives. **Request-oriented
transports** (HTTP) derive identity fresh per request from a bearer token — call
`run_tool` directly after building a per-request `CapabilitySet`.

## `ServerState` and `SharedServerConfig`

Every listener holds one `Arc<ServerState>` (`dispatch.rs`):

```rust
pub struct ServerState {
    pub registry: Registry,
    pub config: SharedServerConfig,
    pub tier_policy: TierPolicy,
    pub middleware: Vec<Arc<dyn Middleware>>,
    pub metrics: Arc<MetricsCounters>,
    pub cursor_issuer: Arc<CursorIssuer>,
}
```

`SharedServerConfig` carries the transport-neutral fields — `cwd`,
`max_output_bytes`, `granted_capabilities`, `audit_sink`, `token_broker`,
`server_version`, the UCAN/cursor/frame-deadline settings. Transport-*specific*
config does **not** go here: `atd-server` keeps `socket_path` on its own
`ServerConfig`, `atd-server-http` keeps `listen` / `extra_origins` /
`require_bearer` on `HttpServerConfig` and composes a `SharedServerConfig` in a
`shared` field. Your transport follows the same shape: a per-crate config struct
that composes `SharedServerConfig`.

The `cursor_issuer` is built **once per server instance** so its random
`session_nonce` stays stable across paginated round-trips — build it at server
construction, never per request.

## Walking the two references

**`atd-server` (UDS)** — `crates/atd-server/src/`:
- `server.rs` — `Server::new` builds the `SharedServerConfig` snapshot and one
  `Arc<ServerState>`; `run()` binds the `UnixListener`, sets socket mode `0600`,
  and spawns one task per accepted connection.
- `connection.rs` — `handle_connection` runs the per-connection read loop:
  read a frame, `dispatch_request`, write a frame; `caps`/`caller_id` persist
  across the loop so a `Hello` is sticky.
- Frame deadlines: `frame_deadline_handshake_ms` (5 s, pre-Hello) and
  `frame_deadline_active_ms` (30 s, post-Hello) bound each read/write. Override
  via `Server::set_frame_deadlines(active, handshake)`.

**`atd-server-http` (HTTP)** — `crates/atd-server-http/src/`:
- `server.rs` — `ServerBuilder::build` returns `(Router, Server)`; the default
  router is `POST /mcp`. `handle_mcp_post` runs the three-step pipeline:
  origin gate → bearer resolution → method dispatch.
- `mcp.rs` — translates MCP JSON-RPC `tools/call` into a `run_tool` call.
- Per request the listener resolves a bearer to a `BearerIdentity`, builds a
  fresh `CapabilitySet`, and calls `run_tool` — no connection state.

## Step by step

1. **Create the crate** `atd-server-<medium>`. Depend on `atd-runtime` and
   `atd-protocol`.
2. **Define a config struct** with your medium's settings, composing a
   `SharedServerConfig` (`shared` field or equivalent).
3. **Build `ServerState` once** at construction — registry, config, tier
   policy, middleware vec, a fresh `MetricsCounters`, and one `CursorIssuer`
   from `config.cursor_signing_key`. Wrap it in an `Arc`.
4. **Write the accept loop.** For each connection/request: decode the medium's
   framing into a `Request`.
5. **Call the dispatcher** — `dispatch_request` for a stream (carry
   `caps`/`caller_id`/a per-connection `ReadTracker`), or `run_tool` for a
   single request.
6. **Encode the `Response`** back into your medium's framing.
7. **Apply frame deadlines** so a stalled peer fails fast instead of pinning a
   task.

## Testing it

Bind on an ephemeral address, drive a real client, assert the round-trip.
`atd-server-http` ships a UDS↔HTTP parity test: the same `RunTool` over both
transports must produce a byte-identical `Response` — that is the contract a new
transport must also pass. Integration tests bind real ports, so cap parallelism
(`--test-threads=4`) — see [`../../AGENTS.md`](../../AGENTS.md) §4.

## Invariants you must preserve

- **Call the shared dispatcher.** Never re-implement the capability gate, tier
  logic, middleware, or audit. The whole point of the seam is that those run
  exactly once, in `atd-runtime`.
- **Byte-identical wire shape.** Your transport must deserialise into / serialise
  from `atd-protocol`'s `Request`/`Response`. No per-transport field divergence.
- **One `CursorIssuer` per server instance** — build it at construction; never
  per request, or paginated continuations fail with `ERR_CURSOR_EXPIRED`.
- **Enforce frame deadlines** so a slow or hostile peer cannot starve the
  accept loop.
- **Handshake before tools.** A stream transport must complete `Hello` (or its
  per-request bearer equivalent) before honouring `RunTool` — `dispatch_request`
  gives you this for free; do not bypass it.

## See also

- [`../atd-architecture.md`](../atd-architecture.md) §4 (wire & types), §5 (dispatch),
  §9.1 (the crate layering diagram).
- [`../protocol/wire-format.md`](../protocol/wire-format.md) — the byte-level
  framing the UDS transport uses.
