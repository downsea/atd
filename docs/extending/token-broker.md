# Adding an auth / secret scheme

**Purpose:** route per-caller secrets — OAuth tokens, API keys — into tool
execution, and validate HTTP bearer tokens, by implementing the `TokenBroker`
trait.

## When to use this

Use a custom `TokenBroker` when one ATD server process serves **many distinct
identities** — multiple OAuth users on one socket, multiple tenants behind one
HTTP endpoint — and each caller's tools need that caller's own secrets without
seeing anyone else's. Production deployments wrap a real secret manager (Vault,
AWS Secrets Manager, Doppler) behind this trait.

If your server is single-tenant and tools read secrets from env vars or a saved
file, you do not need a broker at all — leave `token_broker: None`.

## The trait

`TokenBroker` is defined in `crates/atd-runtime/src/secrets.rs`, re-exported as
`atd_runtime::TokenBroker`:

```rust
pub trait TokenBroker: Send + Sync {
    /// Resolve a secret bundle for a caller identity (UDS Hello.client_id).
    /// Ok(None)  → no bundle; tool falls back to env/file.
    /// Ok(Some)  → bundle attached to the call.
    /// Err(_)    → hard failure; dispatch returns ERR_BROKER_FAILED (1003).
    fn resolve<'a>(&'a self, caller_id: Option<&'a str>) -> ResolveFuture<'a>;

    /// Resolve an HTTP `Authorization: Bearer …` token to a BearerIdentity.
    /// Default impl returns Err(BrokerError::NotConfigured) — only brokers
    /// that override this get HTTP bearer auth.
    fn resolve_bearer<'a>(&'a self, bearer: &'a str) -> ResolveBearerFuture<'a>;

    /// Informational hint: which token formats this broker accepts
    /// (e.g. ["ucan-jwt", "opaque"]). Default &[]. Surfaced via diagnostics.
    fn accepted_token_formats(&self) -> &'static [&'static str];
}
```

`ResolveFuture` resolves to `Result<Option<Arc<SecretBundle>>, BrokerError>`;
`ResolveBearerFuture` resolves to `Result<Option<BearerIdentity>, BrokerError>`.
`resolve_bearer` and `accepted_token_formats` have default impls — a UDS-only
broker can implement just `resolve`.

## The supporting types

- **`SecretBundle`** = `HashMap<String, RedactedString>`. Keys are
  operator-defined (`"access_token"`, `"refresh_token"`, `"api_key"`).
- **`RedactedString`** — a string wrapper whose `Debug` renders
  `RedactedString(<redacted>)` and whose `Display` renders `<redacted>`. The
  value comes out **only** via `.expose() -> &str`. Never log the result of
  `expose()`.
- **`BrokerError`** — `NotConfigured` / `Lookup(String)` / `Internal(String)` /
  `Expired` / `Revoked(String)`. The variant chosen drives the HTTP status the
  listener returns (below).
- **`BearerIdentity`** — what `resolve_bearer` returns on success:
  `caller_id`, `granted_capabilities`, optional `secrets`, optional
  `expires_at` and `cache_until` hints.

## How secrets reach a tool

During dispatch of a `RunTool` (`crates/atd-runtime/src/dispatch.rs`,
`run_tool`):

1. If `SharedServerConfig.token_broker` is set, dispatch calls
   `broker.resolve(caller_id)`.
2. `Ok(Some(bundle))` → the bundle is placed on `CallContext::secrets`.
   `Ok(None)` → `ctx.secrets()` is `None`. `Err(_)` → dispatch returns
   `Response::Error` with code `ERR_BROKER_FAILED` (1003, retryable) and the
   tool never runs.
3. The tool reads a secret with `ctx.secrets().and_then(|s|
   s.get("access_token"))` and `.expose()` to use the value.
4. The audit event records only `secrets_resolved: bool` — never key names or
   values.

> **Note (continuations).** Broker resolution runs on the initial `RunTool`
> only. `RunToolContinue` (paginated continuations) passes `secrets: None` —
> the tool reads continuation state from the cursor's `opaque_state`, not from a
> re-resolved bundle. See `run_tool_continue` in `dispatch.rs`.

## HTTP bearer: `resolve_bearer` and `BearerOutcome`

For the HTTP transport, `resolve_bearer` is the bearer-auth arm. The HTTP
listener (`crates/atd-server-http/src/bearer.rs`) parses
`Authorization: Bearer …`, calls `resolve_bearer`, and turns the result into a
typed `BearerOutcome` — each variant maps to a specific HTTP status,
`WWW-Authenticate` header, and optional `Retry-After`:

| `resolve_bearer` returns | Meaning | HTTP mapping |
|---|---|---|
| `Ok(Some(identity))` | Validated | proceed with `identity` |
| `Ok(None)` | Well-formed but unrecognised / verify failed | 401 `invalid_token` |
| `Err(Expired)` | Recognised, past expiry | 401 |
| `Err(Revoked(_))` | Recognised, grant revoked | 401 |
| `Err(Lookup(_))` | **Transient** backend failure | 503 + `Retry-After` |
| `Err(NotConfigured)` | Broker does not support bearer auth | anonymous mode |

Critical rule: reserve `Err(Lookup)` for *transient* failures only (DB down,
network blip). A malformed or unrecognised token must be `Ok(None)` so the
listener emits 401, not 503.

## The two reference implementations

- **`InMemoryTokenBroker`** (`secrets.rs`) — unit-test fixture / single-process
  setup. `insert(caller_id, bundle)` registers a bundle; `resolve` looks it up.
  Its `resolve_bearer` has a UCAN-JWT branch: register a `did:key` →
  `caller_id` mapping with `register_ucan_audience(...)`, and JWT-shape bearers
  resolve to that caller with the chain's attenuated capabilities. Non-JWT
  bearers return `Err(NotConfigured)`.
- **`FileTokenBroker`** (`crates/atd-runtime/src/file_token_broker.rs`) —
  disk-backed. `FileTokenBroker::new(root)` persists per-bearer subdirs at
  `${root}/${bearer_id}/{access_token,refresh_token,expires_at}.json` with mode
  `0700`/`0600` on Unix. Holds a per-bearer refresh mutex (`lock_refresh()`) so
  concurrent OAuth refreshes for one bearer don't double-round-trip;
  `is_near_expiry()` (default 5-minute window, `with_refresh_window` to tune) is
  a no-I/O predicate adopters check before taking the refresh path.

## Step by step

1. **Define the struct** holding your secret-manager handle.
2. **`impl TokenBroker`.** `resolve` maps a `caller_id` to an
   `Option<Arc<SecretBundle>>`. Wrap every secret value in `RedactedString::new(…)`.
3. **(HTTP) override `resolve_bearer`** to validate your bearer format and
   return a `BearerIdentity`. Map errors per the table above — `Ok(None)` for
   bad tokens, `Err(Lookup)` only for transient backend failures.
4. **Override `accepted_token_formats`** to advertise your format(s).
5. **Keep `resolve` cheap.** Dispatch calls it per `RunTool` — cache long-lived
   bundles inside the broker.

## Wiring it in

The broker goes on the config:

```rust
let cfg = atd_server::ServerConfig {
    token_broker: Some(Arc::new(MyVaultBroker::new(vault_client))),
    // …
};
let server = atd_server::Server::new(registry, cfg);
```

The HTTP transport carries it on `HttpServerConfig.shared.token_broker`.

## Testing it

Brokers are plain async — test without a socket:

```rust
#[tokio::test]
async fn resolves_known_caller() {
    let mut broker = InMemoryTokenBroker::new();
    let mut bundle = SecretBundle::new();
    bundle.insert("oauth".into(), RedactedString::new("tok-A"));
    broker.insert("agent-A", bundle);
    let resolved = broker.resolve(Some("agent-A")).await.unwrap();
    assert_eq!(resolved.unwrap().get("oauth").unwrap().expose(), "tok-A");
}
```

Cover: a known caller, an unknown caller (`Ok(None)`), `None` caller, and — for
a bearer broker — a valid token, an invalid token (`Ok(None)`), and a transient
failure (`Err(Lookup)`). Also assert a `RedactedString` `Debug` does **not**
contain the secret.

## Invariants you must preserve

- **`RedactedString` is the only secret container.** Never put a bare `String`
  secret on a `CallContext`, an audit event, or a log line.
- **`Ok(None)` vs `Err`.** No bundle for this caller is `Ok(None)` (the tool
  falls back gracefully) — not an error. `Err` is a *hard* failure and returns
  `ERR_BROKER_FAILED`.
- **`Err(Lookup)` is transient only.** Mapping a malformed bearer to `Lookup`
  wrongly returns HTTP 503 instead of 401.
- **Audit carries `secrets_resolved: bool` only** — never names, never values.
- **Cross-caller isolation.** A caller must never resolve to another caller's
  bundle. Test it.

## See also

- [`../atd-architecture.md`](../atd-architecture.md) §5.5 (secret routing), §6.4
  (audit), §5.2 (UCAN-lite capability gate).
- [`audit-sink.md`](audit-sink.md) — the matching no-secrets rule for audit.
