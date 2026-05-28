# SP-token-broker-phase1 — `TokenBroker` extension point in atd-runtime / atd-server

**Date:** 2026-04-27
**Status:** Approved — ready for implementation plan
**Parent:** Lands **Phase 1** of [atd-mvp#4](https://github.com/downsea/atd-mvp/issues/4) (Multi-tenant token broker). Phase 2 (`healthkit_cli` adoption with per-caller OAuth store) and Phase 3 (live two-Hermes-session demo) are separate SPs.
**Anchor:** SP-operability-v1 (commit `bc9f72c`) added `CallContext.caller_id` populated from `Hello.client_id`. This SP is the broker plug that consumes that field — completing the multi-tenant routing key.

## 1. Context

ATD's positioning vs raw CLI is now empirically validated end-to-end (v1.2.0 healthkit case study + cross-vendor mock demo). The remaining unproven operational claim is **multi-tenant token routing**: one ATD server, N callers (each with a different `Hello.client_id`), N OAuth/secret bundles, no caller leakage.

The protocol already supplies the routing key (`CallContext.caller_id`). What's missing is the extension point that maps `caller_id → SecretBundle` and propagates the resolved bundle to `Tool::call`. This SP adds exactly that — runtime side only, no protocol change, no adopter wiring (deferred to Phase 2).

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q1 | Scope this session? | **Phase 1 only** — atd-runtime trait + atd-server dispatch wiring + InMemory stub + tests. Adopter rewrites and live demo are separate SPs. |
| Q2 | Trait sync vs async? | **Async** — `async fn resolve(caller_id: Option<&str>) -> Result<Option<SecretBundle>, BrokerError>`. Brokers may hit secret managers (Vault, AWS SM, files with refresh-on-demand); sync forces blocking. |
| Q3 | Where does the resolved secret live? | **New optional field on `CallContext`: `secrets: Option<Arc<SecretBundle>>`.** Tools that need secrets read via the bundle; tools that don't, ignore it (full back-compat). |
| Q4 | `SecretBundle` shape? | **`HashMap<String, RedactedString>`** wrapped in `Arc`. `RedactedString` is a minimal in-crate newtype around `String` with custom `Debug`/`Display` that prints `"<redacted>"` instead of the value. No new dep. |
| Q5 | Where in dispatch does broker fire? | **Right after the capability gate check, before `Tool::call`.** If broker returns `Ok(None)`, `ctx.secrets` is `None` and the tool falls back to existing env-var/file paths. If broker returns `Err(...)`, the call fails with `ToolCallError::ExecutionFailed { code: "broker_error", retryable: true }`. |
| Q6 | Audit log policy | **Add `secrets_resolved: bool` (no key names, no values) to `CallEvent`.** True iff the broker returned `Ok(Some(_))` for this call. Unit test asserts `format!("{:?}", bundle)` contains no plaintext. |
| Q7 | Per-tool capability gate on secret access? | **No v1.** Server-operator-controlled at startup; broker resolves the full bundle for the caller; tools read what they need. Future SP can add `ToolDefinition::secrets_required` if multi-tenant operators want fine-grained gates. |
| Q8 | Backwards compat | **Servers without a broker behave exactly as today.** Plumbed via `ServerConfig::token_broker: Option<Arc<dyn TokenBroker>>` (mirrors the `audit_sink` shape). No protocol changes. |
| Q9 | Crate placement | **All in `atd-runtime`.** Trait, types, and `InMemoryTokenBroker` ship together. atd-server depends on atd-runtime already; just adds the field on `ServerConfig` + the dispatch hook. |
| Q10 | New errors | **`BrokerError` enum in atd-runtime** with three variants: `NotConfigured`, `Lookup(String)`, `Internal(String)`. Distinct from `ToolCallError` so atd-server can decide the wire-level mapping. |

## 3. Touch points

One commit. Five files.

| # | File | Change |
|---|---|---|
| 1 | `crates/atd-runtime/src/secrets.rs` (new) | `RedactedString` newtype, `SecretBundle` type alias, `TokenBroker` trait, `BrokerError` enum, `InMemoryTokenBroker` struct + impl. ~150 lines + ~6 unit tests (Debug doesn't leak, Display doesn't leak, lookup hit/miss, async impl). |
| 2 | `crates/atd-runtime/src/lib.rs` | `pub mod secrets;` and re-export the public types. |
| 3 | `crates/atd-runtime/src/context.rs` | Add `secrets: Option<Arc<SecretBundle>>` field to `CallContext`; extend `CallContext::new(...)` signature; add `pub fn secrets(&self) -> Option<&SecretBundle>` accessor. |
| 4 | `crates/atd-server/src/config.rs` | Add `token_broker: Option<Arc<dyn atd_runtime::TokenBroker>>` field to `ServerConfig` (mirrors `audit_sink`). |
| 5 | `crates/atd-server/src/connection.rs` | In `Request::RunTool` dispatch, after capability check + before deriving tier and entering tool dispatch: if `state.config.token_broker.is_some()`, call `broker.resolve(caller_id.as_deref()).await`; set `ctx.secrets` accordingly. On `Err`, return `Response::Error { code: "broker_error", retryable: true }`. Update audit emit to include `secrets_resolved: bool`. New `tokio::test` with a stub broker covers the happy path + the back-compat path (`token_broker: None`). |

**Not touched:**

- `atd-protocol` — no wire-format change, no new request/response.
- `atd-sdk` — clients don't need to know about brokers; secrets resolve server-side.
- `atd-cli` / `atd-mcp-bridge` / `atd-tools-*` / `atd-conformance` — no changes.
- `healthkit_cli` — adopter wiring is Phase 2.
- Workspace version — additive, no bump (still 0.3.0).

## 4. The trait + types (`atd-runtime/src/secrets.rs`)

```rust
//! Token broker extension point for multi-tenant ATD servers.
//!
//! See SP-token-broker-phase1 for the design rationale and Phase 2/3
//! follow-ups (adopter wiring + live demo).

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

/// String wrapper that refuses to render its value in `Debug` or
/// `Display`. The value is only accessible via `expose()` — by
/// convention, callers should not log the result of `expose()`.
pub struct RedactedString(String);

impl RedactedString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RedactedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RedactedString(<redacted>)")
    }
}

impl fmt::Display for RedactedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Bag of named secrets resolved for one caller. Keys are operator-defined
/// (e.g., `"oauth_token"`, `"refresh_token"`, `"api_key"`).
pub type SecretBundle = HashMap<String, RedactedString>;

#[derive(Error, Debug)]
pub enum BrokerError {
    #[error("broker not configured for this server")]
    NotConfigured,
    #[error("lookup failed for caller: {0}")]
    Lookup(String),
    #[error("internal broker error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait TokenBroker: Send + Sync {
    /// Resolve a secret bundle for the given caller. `Ok(None)` means
    /// no bundle is registered for this caller — the call proceeds and
    /// the tool falls back to whatever pre-broker mechanism it used
    /// (env vars, saved file). `Err(_)` is a hard failure.
    async fn resolve(
        &self,
        caller_id: Option<&str>,
    ) -> Result<Option<Arc<SecretBundle>>, BrokerError>;
}

/// Reference broker for unit tests + small deployments. Production
/// adopters should implement their own broker against a real secret
/// manager (Vault, AWS Secrets Manager, etc.).
#[derive(Default)]
pub struct InMemoryTokenBroker {
    bundles: HashMap<String, Arc<SecretBundle>>,
}

impl InMemoryTokenBroker {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, caller_id: impl Into<String>, bundle: SecretBundle) {
        self.bundles.insert(caller_id.into(), Arc::new(bundle));
    }
}

#[async_trait]
impl TokenBroker for InMemoryTokenBroker {
    async fn resolve(
        &self,
        caller_id: Option<&str>,
    ) -> Result<Option<Arc<SecretBundle>>, BrokerError> {
        let Some(id) = caller_id else {
            return Ok(None);
        };
        Ok(self.bundles.get(id).cloned())
    }
}
```

Unit tests:
- `redacted_string_debug_does_not_leak`
- `redacted_string_display_does_not_leak`
- `redacted_string_expose_returns_value`
- `in_memory_broker_resolves_known_caller`
- `in_memory_broker_returns_none_for_unknown_caller`
- `in_memory_broker_returns_none_for_anonymous_caller`

## 5. `CallContext` extension (`atd-runtime/src/context.rs`)

Add field + accessor:

```rust
pub struct CallContext {
    // ... existing fields ...
    secrets: Option<Arc<SecretBundle>>,
}

impl CallContext {
    pub fn new(
        // ... existing args ...
        secrets: Option<Arc<SecretBundle>>,
    ) -> Self { /* ... */ }

    pub fn secrets(&self) -> Option<&SecretBundle> {
        self.secrets.as_deref()
    }
}
```

Existing callers of `CallContext::new(...)` get one new arg. The struct is `#[non_exhaustive]` so consumers already migrate through `new(...)` per the existing convention. Spec §3 lists every callsite to touch.

## 6. `ServerConfig` + dispatch wiring (`atd-server`)

`ServerConfig` gains:

```rust
pub struct ServerConfig {
    // ... existing fields ...
    pub token_broker: Option<Arc<dyn atd_runtime::TokenBroker>>,
}
```

Dispatch in `connection.rs::dispatch`'s `Request::RunTool` arm:

```rust
// After capability check, before tier resolution.
let secrets = match state.config.token_broker.as_ref() {
    None => None,
    Some(broker) => match broker.resolve(caller_id.as_deref()).await {
        Ok(bundle) => bundle,
        Err(e) => {
            // Emit audit event with secrets_resolved: false.
            return Response::Error {
                message: format!("token broker error: {e}"),
                code: Some(1003), // new code: ERR_BROKER_FAILED (allocate in error-codes.md)
                retryable: Some(true),
                details: None,
            };
        }
    },
};
let secrets_resolved = secrets.is_some();
// ... build CallContext with `secrets` arg ...
// ... call entry.tool.call(args, &ctx).await ...
// emit audit with secrets_resolved
```

Audit `CallEvent` (in atd-runtime) gains `secrets_resolved: bool` field. Existing audit consumers see it as a new key; default `false` for back-compat (struct addition is additive).

Integration test in `connection.rs::tests`:
- `dispatch_with_broker_propagates_secrets_to_tool` — register a stub tool that asserts `ctx.secrets().is_some()`; build state with `InMemoryTokenBroker` populated for `"agent-A"`; call with `caller_id: Some("agent-A")`; assert success.
- `dispatch_without_broker_leaves_secrets_none` — same tool but no broker on `ServerConfig`; assert `ctx.secrets().is_none()`, success path.
- `dispatch_with_broker_lookup_failure_returns_broker_error` — broker returns `Err(...)`; assert `Response::Error { code: Some(1003), retryable: Some(true) }`.

## 7. Wire-level error code allocation

New code: **`ERR_BROKER_FAILED = 1003`** — for broker `Err(_)` returns. Document in `docs/protocol/error-codes.md` next to `ERR_RATE_LIMITED = 1002` (SP-8.2). Note in the doc: this code is server-side only; SDKs may surface it but won't generate it.

## 8. Audit event change

`CallEvent` (in `atd-runtime/src/audit.rs`) gains `pub secrets_resolved: bool`. Default `false`. Existing JSON serializers automatically include it. The `JsonLinesAuditSink::on_call` impl is unchanged (it serializes the whole struct). One unit test in audit.rs: `call_event_serializes_secrets_resolved_field`.

## 9. Versioning

| Crate | Before | After | Reason |
|---|---|---|---|
| `atd-runtime` | 0.3.0 | 0.3.0 | Additive — new module, new optional field on existing struct. No bump. |
| `atd-server` | 0.3.0 | 0.3.0 | Additive — new optional field on `ServerConfig`. No bump. |
| `atd-protocol` | 0.3.0 | 0.3.0 | **Untouched.** No wire change. |

If any future audit consumer treats unknown JSON keys as a hard error, the `secrets_resolved` field would be a soft break. atd-mvp's own audit consumers use `serde(default)` so this is fine.

## 10. Validation

Exit gates:

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features` — passes (current 368 + ~10 new unit tests + 3 dispatch integration tests = ~381)
- [ ] `cargo build --release --workspace`
- [ ] No new dep on the workspace (no `secrecy` / `redact` crate); `RedactedString` is rolled in-crate.

## 11. Out of scope (deferred)

- **Phase 2: healthkit_cli adoption** — implement a healthkit-specific broker (per-caller token store at `~/.config/healthkit/tokens/<caller_id>.json`); rewire `auth::get_token` to consult `ctx.secrets()` first, fall back to env/saved. Separate SP in healthkit_cli repo.
- **Phase 3: live two-tenant demo** — two Hermes sessions A/B with different `client_id`s, different OAuth tokens, both connect to one `healthkit serve`. Audit log proves isolation. Case study writeup. Separate SP, human-in-the-loop verification.
- **`ToolDefinition::secrets_required`** — per-tool capability gate. Wait until a real adopter wants finer-grained access control across tools sharing a server.
- **UCAN-style attestations** — caller-supplied capability tokens that the broker validates. Future architecture §9.3 work.
- **Broker-driven secret rotation** — refresh-on-demand vs background refresh. Adopter concern; broker trait already supports either via the `async fn resolve` signature.
- **Secret expiry / TTL semantics** — no opinion in the trait; brokers handle internally.

## 12. `atd-architecture.md` §10 row

Add after the SP-cross-vendor-mock-demo row:

```
| `TokenBroker` extension point (Phase 1) | Dispatch | ✅ | SP-token-broker-phase1 | 2026-04-27 | Landed; `atd-runtime::TokenBroker` async trait + `RedactedString` + `InMemoryTokenBroker` reference impl; plumbed through `ServerConfig::token_broker` and `CallContext::secrets`; new `ERR_BROKER_FAILED = 1003` wire code; audit `CallEvent` gains `secrets_resolved: bool`. No protocol change. Closes Phase 1 of #4; Phase 2 (healthkit_cli adoption) and Phase 3 (live two-tenant demo) deferred to follow-up SPs. |
```
