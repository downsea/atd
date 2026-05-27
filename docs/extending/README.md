# Extending ATD

How to add capability to `atd` without forking the reference implementation.

This directory is the contract for "any code agent can build a consistent
extension with zero deviation." Each guide gives the exact trait signature, a
reference implementation to copy, a numbered procedure, the wiring point, the
test pattern, and the invariants the extension must preserve.

Read [`../architecture.md`](../architecture.md) §3 (layer model) and §9.3
(extension-point table) first — this directory is the how-to companion to that
normative description.

---

## Extend vs. fork

ATD draws one hard line:

- **Extending** — attaching new behaviour through a `pub` trait in
  `atd-runtime` (or a new listener crate). The wire format does not change; an
  existing client keeps working byte-for-byte. **No fork.**
- **Forking** — changing the wire vocabulary itself: a new `Request`/`Response`
  variant, a new field on `ToolDefinition`, a new `AtdError` variant, a new
  `ToolTier`. This re-shapes `atd-protocol-schema.json` and is a protocol
  change, not an extension. See [`protocol-and-schema.md`](protocol-and-schema.md).

The seven guides below are all the no-fork path. If your change is not on this
list, you are changing the protocol — go to the eighth guide and read the 1.0
stability rule before you start.

## The seven extension points

| # | You want to… | Guide | Reference implementation |
|---|---|---|---|
| 1 | Add a built-in tool | [`tool.md`](tool.md) | `crates/atd-tools-echo` |
| 2 | Add an invocation binding (gRPC, WASM, REST…) | [`binding.md`](binding.md) | `NativeBinding`, `CliBinding` in `atd-runtime` |
| 2a | Declaratively wrap a CLI as an ATD tool | [`cli-binding.md`](cli-binding.md) | `CliBindingConfig` in `atd-protocol` (typed shape; SP-cli-binding-v2) |
| 3 | Add result middleware (validation, redaction…) | [`middleware.md`](middleware.md) | `atd-middleware-fhir`, `RedactPathsMiddleware` |
| 4 | Add a transport / listener (WebSocket, vsock…) | [`transport.md`](transport.md) | `atd-server` (UDS), `atd-server-http` (HTTP) |
| 5 | Add an auth / secret scheme | [`token-broker.md`](token-broker.md) | `FileTokenBroker`, `InMemoryTokenBroker` |
| 6 | Add an audit sink (Kafka, OTel…) | [`audit-sink.md`](audit-sink.md) | `JsonLinesAuditSink` in `atd-runtime` |
| 7 | Change the wire protocol itself | [`protocol-and-schema.md`](protocol-and-schema.md) | `crates/atd-protocol` — **fork-level** |

## The universal pattern

Every no-fork extension (points 1–6) follows the same three steps:

1. **Implement a `pub` trait** — `Tool`, `Binding`, `Middleware`, `TokenBroker`,
   or `AuditSink`. All five live in `atd-runtime`, all are `Send + Sync`, and
   none requires the `async_trait` macro (futures are returned boxed).
2. **Wire it at server construction** — register or install before the accept
   loop starts. `Registry::register` for tools; `Server::set_middleware` for
   middleware; the `ServerConfig` / `SharedServerConfig` struct fields for
   token brokers and audit sinks. Every `Server::set_*` mutator must be called
   **before** `Server::run()` — once connection tasks spawn, the shared state
   is frozen behind an `Arc`.
3. **Test it in isolation** — every trait has a unit-test entry point.
   `CallContext::for_test()` builds a ready-to-use context; sinks and brokers
   are plain async functions. You do not need a running socket to test an
   extension.

A transport (point 4) is the one exception: instead of implementing a trait it
adds a new listener crate that translates its framing into `ClientMessage` /
`ServerMessage` and calls `atd_runtime::dispatch::dispatch_request`.

## Invariants every extension must preserve

- **Never panic in a request path.** Tools return `Err(ToolCallError)`;
  middleware and sinks must not panic. A panic in a dispatch task takes down
  one connection and is a bug.
- **Never leak a secret.** Credentials are wrapped in `RedactedString` whose
  `Debug`/`Display` refuse to print. Audit events carry `secrets_resolved:
  bool` — never key names or values.
- **Stay additive.** New traits and new impls are minor-version changes.
  Reshaping a `pub` type or the wire format is a major (2.0) change — see the
  stability rule in [`protocol-and-schema.md`](protocol-and-schema.md).

## See also

- [`../architecture.md`](../architecture.md) — normative architecture.
- [`../../AGENTS.md`](../../AGENTS.md) — build / test / verify SOP.
- [`../protocol/wire-format.md`](../protocol/wire-format.md) ·
  [`../protocol/error-codes.md`](../protocol/error-codes.md) — the wire contract.
