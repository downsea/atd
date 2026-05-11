# SP-token-broker-phase2: HTTP bearer auth wire integration

| Status | Draft |
| Created | 2026-05-11 |
| Author | cross-project subagent (celia_phr ↔ atd-mvp coordination) |
| Phase | ATD post-v0.3.0; completes SP-streamable-http (SP-1.B) §4.4 |
| Related | SP-token-broker-phase1 (predecessor, `2026-04-27-sp-token-broker-phase1-design.md`); SP-streamable-http (sibling, `2026-05-11-sp-streamable-http-design.md`); Celia `ATD_FUTURE_ISSUES.md §2.A` bearer + `§1.A` UCAN (future); SP-12 canonical dispatch (`2026-04-25-sp12-canonical-dispatch.md`) |

---

## 1. Motivation

**1.1 Phase 1 solved secret routing but not authentication.** SP-token-broker-phase1 (commit `d61e449`) shipped `TokenBroker::resolve(caller_id) -> Option<Arc<SecretBundle>>` (`crates/atd-runtime/src/secrets.rs:87`), with `caller_id` populated from the SP-12 `Hello` handshake (`crates/atd-server/src/connection.rs:51-69`). That is fine for UDS: once a peer says "I am `client_id = agent-A`" inside a kernel-mediated socket bound `0600`, the operator decides whether to trust the assertion (file-permission as the access gate). On HTTP we lose that assertion — anybody who can reach `127.0.0.1:port` can send any `Hello.client_id` they like. The wire needs a *credential*, not a self-declared name, and the broker needs an entry point that consumes it.

**1.2 SP-streamable-http (SP-1.B) opened the hole without filling it.** SP-1.B §4.4 (`docs/superpowers/specs/2026-05-11-sp-streamable-http-design.md:103-178`) added a defaulted trait method `TokenBroker::resolve_bearer(bearer: &str) -> Result<Option<BearerIdentity>, BrokerError>` whose default impl returns `Err(BrokerError::NotConfigured)`, and a `BearerIdentity { caller_id, granted_capabilities, secrets }` struct. SP-1.B is deliberately silent on *token format*, *token lifecycle*, *revocation*, *cache invalidation*, *SSE long-connection refresh*, and a *reference broker implementation* — those are explicitly deferred to "a future SP". This is that SP.

**1.3 Celia is the first wire-bearer adopter and has the data model already.** `celia-cli/src/http_server.rs:300-306` parses `Authorization: Bearer ce_<64hex>` today but **does not validate** it ("Tier-0 implementation: any well-formed bearer is accepted because the celia serve process is already bound to a single (user, agent) via its Pattern A bootstrap"). The authoritative pairing-code → user/agent resolution lives in `apps/desktop/src-tauri/src/agent_bootstrap.rs:226-271`: strip `ce_`, take the first 16 hex chars as `token_short`, `SELECT user_id, grantee FROM consent WHERE status='active' AND grantee LIKE 'agent:%:<token_short>' AND effective_from<=now AND effective_until>=now`. Every additional ATD HTTP adopter will reinvent some shape of this lookup; SP-token-broker-phase2 makes the contract explicit so they share one trait surface.

## 2. Goals

- Define the exact contract of `TokenBroker::resolve_bearer` — input, output, error semantics, performance envelope, idempotency, concurrency.
- Specify a token-format **convention** (not a format spec) that lets adopters self-declare what they emit, so `atd-server-http` can route requests to the right broker without sniffing.
- Specify token *lifecycle* primitives (TTL, refresh, revocation) that brokers MAY implement and adopters MAY observe, with sane defaults that match Celia's current "process-lifetime opaque token" semantics.
- Ship a reference broker `InMemoryBearerBroker` (parallel to phase-1's `InMemoryTokenBroker`) suitable for unit tests + small deployments + integration test fixtures.
- Document the **precise time-sequence** of how `atd-server-http` consumes the broker per HTTP request, so SP-1.B implementers wire it without ambiguity.
- Specify how SSE long-connection auth refresh interacts with bearer expiry (Celia `/chat/stream` precedent, multi-minute sessions).
- Specify revocation propagation paths (TTL fallback, broker-side invalidation hooks, push-based revocation list) and pick a default.
- Ship a **complete pseudo-code draft** of `CeliaConsentTokenBroker` that lives in `celia_phr/crates/celia-cli/`, demonstrating the trait contract with the real `consent` SQL.

## 3. Non-goals

- **Token minting / issuance.** ATD does not generate bearer tokens. Adopters mint their own (Celia: Tauri wizard generates `ce_<64hex>` and stores `consent.grantee = "agent:<name>:<token_short>"`). The broker's job is validation only.
- **UCAN delegation chains.** Capability tokens with `{iss, aud, att, exp}` envelopes signed by issuer DIDs (Celia `ATD_FUTURE_ISSUES.md §1.A`) belong to SP-capability-v2. Phase 2 brokers see opaque-string bearers; capability is server-controlled.
- **OAuth 2.1 authorization flows.** Token endpoints, code-exchange grants, refresh-token mechanics, dynamic client registration — all explicitly deferred to a hypothetical SP-token-broker-oauth.
- **Cross-server token federation.** A bearer issued by broker A is unintelligible to broker B. No introspection endpoint, no JWKS, no SCIM. Operators wire one broker per server.
- **mTLS / client-cert auth.** A separate axis; not in scope. Bearer is the only auth credential ATD HTTP recognises in this SP.
- **JWT signature validation infra.** If an adopter chooses JWT bearers, they own the signing-key plumbing inside their broker — ATD-runtime does not bundle a `jsonwebtoken` dep.
- **Multi-broker chains.** One `Arc<dyn TokenBroker>` per `HttpServerConfig`. Chains / fallbacks / pipelines are an adopter pattern, not a runtime primitive.
- **Session-stickiness via Bearer + Mcp-Session-Id.** SP-1.B §4.7 reserves `Mcp-Session-Id` for a future SP; we do not piggyback session state here.

## 4. Design

This is roughly 55% of the SP. Each subsection addresses one of the 8 decision points from the brief; each gives the chosen answer, evidence from existing source, and the rejected alternatives.

### 4.1 Token format — **opaque random** is the default; JWT and UCAN-lite explicitly out

**Decision.** The trait contract takes an opaque `&str` bearer. The reference broker (`InMemoryBearerBroker`) accepts any non-empty string and looks it up by exact equality in an internal `HashMap<String, BearerIdentity>`. No structural parsing in `atd-runtime`. Adopter brokers (Celia's `CeliaConsentTokenBroker`) MAY impose format constraints (`ce_<64hex>` prefix-check) before doing the look-up; ATD-runtime does not care.

**Why opaque-random is the default.**
1. **Matches every existing wire-bearer pattern in our ecosystem.** Celia's `ce_<64hex>` (`agent_bootstrap.rs:227-233`) is opaque random; Hermes/Claude Desktop today consume it without parsing; the consent table is the authority. Anthropic API keys, OpenAI keys, GitHub PATs are all opaque-random; nobody parses them client-side.
2. **Smallest broker contract.** Validation is `HashMap::get(bearer)` — O(1), no crypto, no JOSE, no `jsonwebtoken` dep on `atd-runtime`. Adopters wanting JWT-style self-contained tokens implement that *inside* their broker (parse + verify + extract claims → `BearerIdentity`) without polluting the trait.
3. **Avoids a stalemate over JWT-vs-UCAN inside ATD.** The UCAN roadmap (Celia `ATD_FUTURE_ISSUES.md:23-45`) deliberately sits in SP-capability-v2, not here. Picking JWT now would either pre-empt UCAN or force two parallel verifications.

**Why not JWT as the trait default.**
- JWTs carry their own expiry / audience / signature requirements; `atd-runtime` would need a signing-key plumbing surface (rotating keys, JWKS endpoint, supported algorithms). That is a much bigger trait than `resolve_bearer(&str)`.
- The JWT-vs-PASETO-vs-Macaroon vs-Biscuit debate has no clear winner in the agent-tooling space today; locking in a choice via the default impl risks forcing adopters into a format they don't want.
- A JWT-using adopter still implements `resolve_bearer(bearer)` — they just parse the bearer as JWT inside the impl. The trait need not know.

**Why not UCAN-lite as the trait default.** UCAN's value is the delegation chain (`{iss, aud, att, exp}` with signed parents); that requires DID resolution + signature verification. Half the design surface lives outside `atd-runtime`. Celia `ATD_FUTURE_ISSUES.md:34-37` explicitly reserves UCAN for the *capability* axis, not the *authentication* axis. Conflating them here would foreclose SP-capability-v2.

**Self-declaration convention (§4.2-relevant).** A broker that parses tokens structurally (JWT, UCAN, custom prefix) SHOULD reject malformed inputs early via `Err(BrokerError::Lookup(reason))` rather than `Ok(None)` — see §4.4. The HTTP listener cannot tell the two cases apart by format, but the broker can, and a fast-reject on malformed input prevents a bad client from probing for valid tokens by trial.

**Trade-off table.**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Opaque-random (HashMap lookup) | Smallest contract; matches Celia today; adopter-agnostic | No self-contained claims; broker must hit storage every request | **chosen** |
| JWT as default | Self-contained; standard tooling | Pulls JOSE crate into atd-runtime; pre-empts UCAN; format-policy via runtime API | rejected |
| UCAN-lite | Delegation chains; future-proof for v2 | Half the spec lives in capability layer; DID resolution sprawl | rejected (defer to SP-capability-v2) |
| Multi-format (try JWT then opaque) | Flexibility | Sniffing surface; trait-side parser; ambiguous error semantics | rejected |

### 4.2 Token issuance — explicitly **out of ATD scope**; format-declaration convention only

**Decision.** ATD never mints bearers. Adopters do. A broker advertises which format(s) it accepts via a static metadata field on the trait, surfaced through a new `TokenBroker::accepted_token_formats() -> &'static [&'static str]` defaulted method (see §5). The listener does not gate on this — it is informational, used by `atd-ref-server --doctor` and `/initialize` server-info to help diagnose configuration mismatches.

**Why no minting endpoint.** ATD-runtime is a dispatch + validation + middleware substrate, not an identity provider. The two universes adopters live in:
1. **Pattern A: pairing-code adopters (Celia).** A trusted "wizard" UI in the adopter's own surface generates a code, stores it in adopter storage, hands it to the user as a string. ATD never sees the mint event. `apps/desktop/src-tauri/src/agent_bootstrap.rs:79-84` shows the Celia path — codes are generated and persisted entirely inside Tauri, before ATD HTTP boot.
2. **Pattern B: OAuth-shaped adopters.** Adopter runs `/oauth/token` somewhere; broker delegates to that adopter's OAuth introspection endpoint. Still out of ATD-runtime scope — the broker calls the endpoint, ATD calls the broker.

**Why a `accepted_token_formats()` hint instead of nothing.**
- Lets ATD-CLI doctor (`atd doctor`) surface "this server's broker accepts {ce_, eyJ}; you sent eyJ..., looks like JWT" misconfiguration messages.
- Lets `atd-server-http` initialize-response expose advertised formats for client introspection (no required behaviour; logged informationally).
- Costs near-zero: a defaulted method returning `&[]` (meaning "broker did not declare").

**Token issuance is therefore *adopter wizard + adopter storage*, not protocol.** This SP defines what brokers *consume*, not what generates the input.

### 4.3 Token lifecycle — TTL is broker-internal; trait exposes optional `expires_at` + `Status`

**Decision.** Token lifecycle is *broker-internal* (the broker decides whether to store TTL, refresh tokens, etc.) but exposed to the listener via two additions to `BearerIdentity`:

```rust
pub struct BearerIdentity {
    pub caller_id: String,
    pub granted_capabilities: Vec<String>,
    pub secrets: Option<Arc<SecretBundle>>,
    // NEW in phase 2:
    /// Absolute wall-clock instant after which the listener MUST refuse
    /// this token without re-validating. `None` = "broker doesn't track
    /// expiry" (Celia process-lifetime case).
    pub expires_at: Option<std::time::SystemTime>,
    /// When the broker considers this identity safely cacheable by the
    /// listener. `None` = "do not cache" (default).
    pub cache_until: Option<std::time::SystemTime>,
}
```

Plus a status discriminant for diagnostic error paths:

```rust
pub enum BrokerError {
    NotConfigured,                              // existing
    Lookup(String),                             // existing
    Internal(String),                           // existing
    // NEW in phase 2:
    /// Token parsed / found but past its expiry. Distinguishable from
    /// `Ok(None)` (unknown token) so the listener can return 401 vs 403
    /// with the right `WWW-Authenticate` hint.
    Expired,
    /// Token explicitly invalidated (revocation list hit, consent
    /// withdrawn). Listener returns 401 + `WWW-Authenticate: Bearer,
    /// error="invalid_token"`.
    Revoked(String),  // reason (e.g., "consent withdrawn 2026-05-11T08:00Z")
}
```

**Why two times (`expires_at` + `cache_until`).**
- `expires_at` is *authoritative*: the listener-side cache MUST evict before this. Even a long-cache adopter cannot keep using a stale identity past `expires_at`.
- `cache_until` is *advisory*: the broker tells the listener "you can skip me for the next N seconds if you cache". Most brokers will set `cache_until = min(expires_at, now + cache_window)`; some (Celia's process-lifetime case) set `cache_until = None` meaning "always ask me, never trust your cache" — because consent can be revoked from the UI at any instant and the user expects the agent to lose access immediately.

**Why both fields are `Option<SystemTime>`.**
- Celia's current model: the token has *no* expiry (it dies when the parent Tauri process restarts); broker returns `expires_at = None`.
- Future OAuth-shaped adopters: token has 1-hour TTL; broker returns `expires_at = Some(issued + 3600s)`.
- The listener treats `None` as "ask the broker every request" (worst case, cheapest broker is in-process HashMap; cost rounds to zero).

**Why no refresh-token concept.** The broker, not the listener, is the authority over refresh — a `resolve_bearer` call against an expired token returns `Err(Expired)`; the client gets 401; the client re-runs whatever flow yielded the original bearer (Celia: re-prompt user for new pairing code, or rebuild via `Tauri --re-pair`). ATD does not relay refresh tokens. Adopters wanting RFC 6749 §6 refresh semantics implement them entirely inside their broker + their own /token endpoint.

**Why no "renewal hook".** A bearer used inside an SSE stream that crosses its `expires_at` is a §4.6 concern; we resolve it there. The trait does not need a separate renewal call — `resolve_bearer` is the only entry point; the listener calls it whenever it needs a fresh authority decision.

### 4.4 `resolve_bearer` precise contract — input, output, errors, performance, idempotency

**Decision.** The trait, post-phase-2:

```rust
#[async_trait]
pub trait TokenBroker: Send + Sync {
    // Phase 1 (unchanged) — secret bag for a known caller_id.
    async fn resolve(
        &self,
        caller_id: Option<&str>,
    ) -> Result<Option<Arc<SecretBundle>>, BrokerError>;

    // Phase 1.5 (SP-1.B §4.4, default Err(NotConfigured)) — bearer →
    // identity. Phase 2 overrides this on every HTTP-capable broker.
    async fn resolve_bearer(
        &self,
        bearer: &str,
    ) -> Result<Option<BearerIdentity>, BrokerError> {
        let _ = bearer;
        Err(BrokerError::NotConfigured)
    }

    // Phase 2 NEW — format declaration (hint, non-load-bearing).
    fn accepted_token_formats(&self) -> &'static [&'static str] {
        &[]
    }
}
```

**Input contract.** `bearer: &str` — the substring after `Authorization: Bearer ` with `Bearer ` already stripped and one optional surrounding whitespace trimmed. Empty `&str` is **not** valid input — listener returns 400 before calling the broker.

**Output semantic table.**

| Return | Meaning | Listener behaviour |
|---|---|---|
| `Ok(None)` | Token is well-formed but unknown to the broker | 401 with `WWW-Authenticate: Bearer error="invalid_token"` |
| `Ok(Some(id))` with `id.expires_at = None` | Token valid, no expiry | 200; one-shot `CapabilitySet` from `id.granted_capabilities` |
| `Ok(Some(id))` with `id.expires_at = Some(t)` where `t > now` | Token valid, with deadline | 200; listener stores `t` for SSE refresh (§4.6) |
| `Ok(Some(id))` with `id.expires_at = Some(t)` where `t <= now` | **MUST NOT happen** — broker MUST return `Err(Expired)` instead | (Behaviour undefined — broker bug; listener treats as 500.) |
| `Err(NotConfigured)` | Broker isn't HTTP-aware (default impl) | 501 with body `{"error":"http_auth_not_configured"}` |
| `Err(Lookup(reason))` | Look-up failed transiently (DB down, network hiccup) | 503 with `Retry-After: 5` |
| `Err(Internal(reason))` | Broker internal bug | 500 |
| `Err(Expired)` | Token recognised, past expiry | 401 with `WWW-Authenticate: Bearer error="invalid_token", error_description="expired"` |
| `Err(Revoked(reason))` | Token explicitly invalidated | 401 with `WWW-Authenticate: Bearer error="invalid_token", error_description="revoked"` |

The ATD `Response::Error.code` field stays at `ERR_BROKER_FAILED = 1003` (`crates/atd-protocol/src/messages.rs:19`) for the `Err(_)` cases that surface inside a JSON-RPC envelope (when the listener cannot decide whether to map to HTTP 401/503/500 because the request is mid-envelope — practically: never; bearer is validated *before* request parsing). The HTTP status code is the primary signal.

**Performance envelope.**
- **Listener-side per-request cost target**: ≤ 5ms p99 for an in-process broker (HashMap lookup). The broker call is on the hot path of every HTTP request; anything ≥ 50ms p99 forces adopters to push caching into the broker.
- **Adopter-side cost target**: ≤ 50ms p99 for a SQLite-backed broker (Celia). Above that, the broker SHOULD cache internally; the listener does not.
- The trait is `async` so broker impls can hit IO (DB / HTTP / Vault / etc.) without blocking the executor — same rationale as phase 1 (`docs/superpowers/specs/2026-04-27-sp-token-broker-phase1-design.md:19`).
- **The listener does NOT cache `resolve_bearer` results.** Caching is the broker's responsibility (see §4.8). This keeps the listener stateless and matches the SP-1.B per-request capability-derivation rule (`2026-05-11-sp-streamable-http-design.md:86-101`).

**Idempotency.** `resolve_bearer(b)` must be *referentially transparent* for the duration `[t, min(expires_at, cache_until)]`: two calls with the same `b` in that window return the same `BearerIdentity` (modulo the broker noticing a revocation midstream — in which case the second call legitimately returns `Err(Revoked)`).

**Concurrency.** The trait is `Send + Sync`; the broker must tolerate concurrent calls. Reference impl uses `tokio::sync::RwLock<HashMap>` for the cheap mutation-on-revocation path.

**No `bearer` value in logs or `Debug` output.** The bearer is a secret. Brokers MUST NOT log the raw bearer; SHOULD log a truncated digest (`format!("{}...", &sha256(bearer)[..8])`). The `BearerIdentity` struct itself has no `Debug` derive emitting the bearer because the bearer is not stored on it — the listener forgets the bearer the moment `resolve_bearer` returns.

**Why distinguish `Ok(None)` from `Err(Expired)` from `Err(Revoked)`.**
- All three become 401 to the client, but the *audit log* and the *adopter-side UX* need the distinction. Celia's "agent revoked" toast wants `Revoked`; "code typo on first connect" wants the equivalent of `Ok(None)`; "code expired due to TTL" wants `Expired` so the UI can prompt for re-pairing.
- The audit `Outcome::ExecutionFailed { code, retryable }` already supports a string code field (`crates/atd-runtime/src/audit.rs` per phase-1 spec). We extend the strings: `"bearer_unknown"`, `"bearer_expired"`, `"bearer_revoked"`, `"broker_error"`. No new wire field.

### 4.5 Multi-tenant reference broker — `InMemoryBearerBroker`

**Decision.** Ship `InMemoryBearerBroker` alongside the existing `InMemoryTokenBroker` in `crates/atd-runtime/src/secrets.rs`. Same Rust file, same mod path. Same role: unit-test fixture + small-deployment quick start.

```rust
#[derive(Default)]
pub struct InMemoryBearerBroker {
    // bearer → identity. Wrapped in RwLock so a revocation path
    // (insert/remove) doesn't block the read fast-path.
    identities: tokio::sync::RwLock<HashMap<String, BearerIdentity>>,
    // Optional: secret bundles for caller_id (subsumes phase-1 broker).
    bundles: HashMap<String, Arc<SecretBundle>>,
}

impl InMemoryBearerBroker {
    pub fn new() -> Self { Self::default() }

    /// Register a token. If `expires_at` is set, the broker auto-expires
    /// it on the next `resolve_bearer` call past that instant; the entry
    /// stays in the map until then for stable `Err(Expired)` reporting.
    pub async fn insert_bearer(
        &self,
        bearer: impl Into<String>,
        identity: BearerIdentity,
    );

    /// Remove a token. Subsequent `resolve_bearer` returns
    /// `Err(Revoked(reason))`.
    pub async fn revoke(&self, bearer: &str, reason: impl Into<String>);

    /// Phase-1 secret-bundle helper, unchanged.
    pub fn insert_secret_bundle(&mut self, caller_id: impl Into<String>, bundle: SecretBundle);
}

impl TokenBroker for InMemoryBearerBroker {
    async fn resolve(
        &self,
        caller_id: Option<&str>,
    ) -> Result<Option<Arc<SecretBundle>>, BrokerError> {
        let Some(id) = caller_id else { return Ok(None); };
        Ok(self.bundles.get(id).cloned())
    }

    async fn resolve_bearer(
        &self,
        bearer: &str,
    ) -> Result<Option<BearerIdentity>, BrokerError> {
        let map = self.identities.read().await;
        let Some(id) = map.get(bearer) else { return Ok(None); };
        if let Some(exp) = id.expires_at {
            if exp <= SystemTime::now() {
                return Err(BrokerError::Expired);
            }
        }
        Ok(Some(id.clone()))
    }

    fn accepted_token_formats(&self) -> &'static [&'static str] {
        &["opaque"]
    }
}
```

**Why this isn't "the" production broker.** Production adopters back to a real authority (Celia's SQLite, OAuth introspection endpoint, Vault). The reference exists to (a) make conformance tests run without external deps, (b) give adopters a known-good baseline to diff against, (c) parallel the phase-1 `InMemoryTokenBroker` precedent (`crates/atd-runtime/src/secrets.rs:93-117`).

**Why one struct instead of two.** `InMemoryBearerBroker` *subsumes* `InMemoryTokenBroker` by also implementing `resolve(caller_id)`. We keep `InMemoryTokenBroker` for back-compat — adopters with phase-1 wiring should compile clean. New adopters reach for `InMemoryBearerBroker` directly.

**Specta / specta binding implications.** None: brokers are server-side Rust types; the wire shape (HTTP 401 + ATD `Response::Error.code`) is unchanged. The `BearerIdentity` struct is internal to atd-runtime.

### 4.6 HTTP pipeline hook point — exact time-sequence

**Decision.** The HTTP listener calls `resolve_bearer` **before** parsing the JSON-RPC envelope body (so a missing/expired bearer fails fast without payload allocation), but **after** Origin gate (Origin is cheap and cheap-fails block DNS-rebinding before authentication is even attempted).

Full time-sequence for one POST `/mcp` request:

```
POST /mcp                                                                        
  Origin: http://localhost:5173                                                  
  Authorization: Bearer <token>                                                  
  Content-Type: application/json                                                 
  Mcp-Session-Id: <opt, logged, not used>                                        
  body = JSON-RPC envelope                                                       
                                                                                 
atd-server-http listener:                                                        
  ┌─────────────────────────────────────────────────────────────────────────┐    
  │ 1. tower_http::cors gate.                                                │    
  │    if Origin not allow-listed → 403, return.                            │    
  │    (SP-1.B §4.6)                                                         │    
  ├─────────────────────────────────────────────────────────────────────────┤    
  │ 2. Body-size gate.                                                       │    
  │    Content-Length > max_body_bytes → 413, return.                       │    
  │    (SP-1.B §5.6)                                                         │    
  ├─────────────────────────────────────────────────────────────────────────┤    
  │ 3. Bearer-parse middleware.                                              │    
  │    Authorization header absent + ServerConfig.require_bearer = false:   │    
  │      → bearer = None, skip step 4, continue with anonymous identity.    │    
  │    Authorization header absent + require_bearer = true:                 │    
  │      → 401 + WWW-Authenticate: Bearer, return.                          │    
  │    Authorization header present, not Bearer scheme: → 401.              │    
  │    Authorization header present, empty bearer: → 400.                   │    
  ├─────────────────────────────────────────────────────────────────────────┤    
  │ 4. Broker resolve.                                                       │    
  │    broker.resolve_bearer(&bearer).await → match:                        │    
  │      Ok(Some(id))     → continue with `id` as caller identity.          │    
  │      Ok(None)         → 401 invalid_token, return.                      │    
  │      Err(Expired)     → 401 invalid_token error_description=expired.    │    
  │      Err(Revoked(r))  → 401 invalid_token error_description=revoked.    │    
  │      Err(NotCfg)      → 501 http_auth_not_configured.                   │    
  │      Err(Lookup(_))   → 503 Retry-After: 5.                             │    
  │      Err(Internal(_)) → 500.                                            │    
  ├─────────────────────────────────────────────────────────────────────────┤    
  │ 5. Capability intersection.                                              │    
  │    granted = id.granted_capabilities ∩ HttpServerConfig.granted_caps    │    
  │    Build per-request CapabilitySet.                                     │    
  │    (Mirrors UDS Hello path, `connection.rs:51-69`)                      │    
  ├─────────────────────────────────────────────────────────────────────────┤    
  │ 6. JSON-RPC envelope parse.                                              │    
  │    Invalid JSON → 400 -32600.                                           │    
  │    Unknown method → 200 -32601.                                         │    
  │    Methods routed: initialize / notifications/initialized /             │    
  │                    tools/list / tools/call (SP-1.B §4.2)                │    
  ├─────────────────────────────────────────────────────────────────────────┤    
  │ 7. For tools/call: atd_runtime::dispatch::run_tool(                     │    
  │      state, tracker, &CapabilitySet, caller_id = id.caller_id,         │    
  │      tool_id, args, dry_run = false                                    │    
  │    ).await                                                              │    
  │    → returns Response.                                                  │    
  │    (Existing connection.rs:93-369 logic, factored per SP-1.B §4.3)     │    
  ├─────────────────────────────────────────────────────────────────────────┤    
  │ 8. MCP envelope wrap. (SP-1.B §5.3)                                    │    
  │    → JSON-RPC response back over HTTP 200.                             │    
  └─────────────────────────────────────────────────────────────────────────┘    
```

**Why bearer before envelope parse.** A 401 path that does not allocate / parse the body is cheaper, harder to DDoS via large invalid bodies, and matches RFC 7235 §2 — auth-related errors come before content negotiation. Origin is even before bearer because Origin failures are guaranteed-cheap (header lookup + string starts_with) and protect against DNS-rebinding regardless of bearer state.

**Why no broker call for `initialize` / `notifications/initialized`.** SP-1.B §4.2 specifies these MCP methods are server-synthesised and do not enter `Registry`. They still need bearer when `require_bearer = true` — failing 401 here is consistent with the agent never being authenticated. But the broker call returns a `BearerIdentity` that the listener just discards (no tool dispatch on this method). Net cost: one broker round-trip per `initialize`. Acceptable; `initialize` is per-session, not per-call.

**Concurrency story.** Each HTTP request is its own tokio task; broker calls run concurrently from N tasks. The broker MUST be `Send + Sync` (already required by phase-1). Reference broker uses `RwLock<HashMap>`; read fast-path is uncontended.

### 4.7 SSE long-connection refresh — bearer pinned at connection open, recheck on heartbeat

**Decision.** For SSE long connections (Celia `/chat/stream` precedent, `crates/celia-cli/src/http_server.rs:182-254`), the listener:
1. Resolves the bearer **once** at stream open (step 4 above). Records `id.expires_at`.
2. **Does NOT re-resolve** on every SSE event push (high-frequency; ≥10 events/sec would saturate the broker).
3. **Re-resolves on a periodic schedule**: every `min(expires_at - now, 60s)`, whichever is sooner. If `expires_at = None`, the re-resolve runs every 60s as a revocation check.
4. On re-resolve `Err(Expired | Revoked | Lookup)`: the listener emits a final SSE `event: auth_lost` frame, then closes the stream with `event: done`. The client (Celia frontend) reconnects with a fresh bearer (post user re-pairing) and starts a new stream.
5. On re-resolve `Ok(Some(updated_id))` whose `granted_capabilities` shrunk vs the cached set: tools/call requests dispatched *after* the re-resolve see the smaller set; in-flight tool dispatches keep their original `CapabilitySet`.

**Why pin-at-open, recheck-on-heartbeat instead of recheck-per-event.**
- Per-event recheck would 10×-100× the broker QPS for streaming workloads; Celia's chat stream typically emits 1 token/sec → 60 broker calls per minute per stream per Celia user. The 60s heartbeat is a 60× reduction with a 60s revocation window — acceptable; consent withdrawals get honoured within 60s of UI action.
- Per-event recheck is wasteful: 99.9% of events arrive long before `expires_at`, with no revocation in between.
- The "in-flight tool dispatch keeps its original capability set" rule mirrors UDS connection-scoped capability (`connection.rs:51-69`) — a tool call that begins authorised must complete with that authority, regardless of mid-call revocation, because canceling mid-execution is its own race-condition surface.

**Why 60s default + bound-by-expires_at.**
- 60s matches the typical SSE keep-alive ping cadence (`axum::response::sse::KeepAlive::default()` default = 15s per axum docs; we use 60s as our recheck schedule, decoupled from keep-alive).
- Bound-by-expires_at means a 5-minute-TTL token gets up to 5 chances to be revoked; a 1-second-TTL token (pathological) gets re-checked every iteration up to the wall-clock floor of 60s — broker still gets meaningful breathing room; the listener does not chase sub-second clocks.

**Why not stream-supplied refresh tokens.** SSE clients can technically send refresh tokens via the URL query or initial POST, but doing so leaks the token into server logs and proxy access logs. The bearer in `Authorization` is the only credential; if it expires mid-stream, the client reconnects.

**Why `auth_lost` is a custom event (not standard SSE).** SSE has no standard "auth expired" frame; the listener emits an application-level event the adopter wraps. Celia maps it to a frontend toast ("Session expired — please re-pair") with auto-reconnect once the user provides a fresh code. Other adopters can choose other UX.

**Open question explicitly answered.** Brief enumerated four options:
- ① Don't manage (stream runs to end regardless): unsafe — consent revocation does not stop in-flight stream.
- ② Heartbeat re-validate (every 60s): **chosen.**
- ③ Stream carries new token mid-flight: ergonomic foot-gun (leak to logs).
- ④ Adopter self-manages: surrenders the contract; reinvents the same logic in N adopters.

### 4.8 Revocation + cache invalidation — broker-internal TTL + push-based hook + revocation list

**Decision.** Three layered mechanisms; brokers pick which to implement; listener observes none of them directly.

**Layer 1 — TTL fallback (always present).** Every `BearerIdentity.expires_at`, if set, is the absolute cap. The listener honours it. This means in the absolute worst case (broker has no other invalidation mechanism), revocation is bounded by TTL — Celia for example could choose to expire pairing codes after 24 hours, capping the worst-case window.

**Layer 2 — Push-based invalidation hook (broker-internal).** The adopter's surface (Celia's Tauri UI) calls a broker-internal method like `broker.revoke(bearer)` synchronously when the user clicks "Revoke" in the agents list. Effect is immediate inside the broker process. **The listener and ATD-runtime are unaware** — they discover the revocation lazily on the next `resolve_bearer` call (returning `Err(Revoked)`).

For the SSE-stream case: the periodic re-resolve (§4.6) picks up the revocation within the heartbeat window. For new requests: zero latency from revoke-button to first 401.

**Layer 3 — Revocation list endpoint (out of scope this SP).** A future SP could add `broker.revocation_list_since(t)` for federation — not needed for the single-broker / single-adopter case Celia + ATD live in today. Explicit non-goal here.

**Why no broker → listener cache invalidation channel.** Trait stays small. The listener does not cache (§4.4 performance section). If a future SP introduces listener-side caching (e.g., for a Vault-backed slow broker), it can add an `Arc<RevocationNotifier>` to `HttpServerConfig`; we don't need it now.

**Cache invalidation semantics in three layers (graph):**

```
                  ┌── Adopter UI (Celia Tauri) ────────────────┐
                  │ user clicks "Revoke agent Hermes"          │
                  └────────────┬──────────────────────────────┘
                               │ adopter-local call
                  ┌────────────▼──────────────────────────────┐
                  │ CeliaConsentTokenBroker::revoke(bearer)   │
                  │  - UPDATE consent SET status='revoked'    │
                  │  - cache invalidate (internal)            │
                  └────────────┬──────────────────────────────┘
                               │
        ┌──────────────────────┼──────────────────────────────┐
        │                      │                              │
        ▼                      ▼                              ▼
  Next /mcp request       SSE stream heartbeat       in-flight tool call
   broker returns         broker returns Err(Revoked) keeps original CapSet
   Err(Revoked) →         → emit auth_lost, close      → completes; next
   listener 401           stream                       request gets 401
   (zero latency)         (within 60s window)
```

**Celia-specific revocation latency.** Adopter UI revoke → broker SQL UPDATE: ~ms. Broker invalidates its internal cache: ~µs. Next ATD request: revoked immediately. In-flight SSE: ≤ 60s. Total worst case ≤ 60s, matches the design target.

**Why this is the right split vs alternatives.**
- (a) "Listener queries adopter every call, no cache" — Celia's SQLite hit per call costs 1-5ms, fine for `/mcp` low-rate workloads, but the listener layer should not be the cache authority. Keeping caching in broker preserves trait clarity.
- (b) "Adopter notifies listener via channel" — added complexity; revocation already lazy-propagates via §4.6 heartbeat for streams and §4.6 step 4 for new calls.
- (c) "Revocation list endpoint" — federation concern; not in scope.

## 5. Trait contract — final shape

Full `secrets.rs` surface after phase 2 (additive):

```rust
//! Token broker extension point for multi-tenant ATD servers.
//! Phase 1 added `resolve(caller_id) -> SecretBundle`.
//! Phase 1.5 (SP-1.B) added defaulted `resolve_bearer(bearer)`.
//! Phase 2 (this SP) adds expires_at + cache_until on BearerIdentity,
//! Expired/Revoked error variants, and an accepted_token_formats hint.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use thiserror::Error;

pub struct RedactedString(String);  // unchanged (`secrets.rs:29-39`)

pub type SecretBundle = HashMap<String, RedactedString>;  // unchanged

#[derive(Error, Debug)]
pub enum BrokerError {
    #[error("broker not configured for this server")]
    NotConfigured,
    #[error("lookup failed: {0}")]
    Lookup(String),
    #[error("internal broker error: {0}")]
    Internal(String),
    // NEW (phase 2):
    #[error("bearer token has expired")]
    Expired,
    #[error("bearer token revoked: {0}")]
    Revoked(String),
}

/// Identity resolved from a Bearer token.
#[derive(Debug, Clone)]
pub struct BearerIdentity {
    pub caller_id: String,
    pub granted_capabilities: Vec<String>,
    pub secrets: Option<Arc<SecretBundle>>,
    pub expires_at: Option<SystemTime>,
    pub cache_until: Option<SystemTime>,
}

#[async_trait::async_trait]
pub trait TokenBroker: Send + Sync {
    /// Phase 1: secret bundle for a known caller_id (UDS Hello path).
    async fn resolve(
        &self,
        caller_id: Option<&str>,
    ) -> Result<Option<Arc<SecretBundle>>, BrokerError>;

    /// Phase 1.5 (SP-1.B §4.4) + phase 2 contract: bearer → identity.
    /// Default impl returns NotConfigured so phase-1 brokers compile.
    async fn resolve_bearer(
        &self,
        _bearer: &str,
    ) -> Result<Option<BearerIdentity>, BrokerError> {
        Err(BrokerError::NotConfigured)
    }

    /// Phase 2 hint: declare which token format(s) this broker accepts.
    /// Informational only — listener does not gate on this.
    /// Conventional values: "opaque", "jwt", "ucan-lite", or any
    /// adopter-specific tag (e.g. "ce-pairing-code").
    fn accepted_token_formats(&self) -> &'static [&'static str] {
        &[]
    }
}
```

The signature of `resolve_bearer` post-phase-1.5 is preserved; phase 2 only widens the *return body* (new fields on `BearerIdentity`, new error variants on `BrokerError`). All `BrokerError` variants are `#[non_exhaustive]` in the published crate so a v0.x adopter can match `_ => map_to_500()` without breaking.

## 6. Celia adopter — `CeliaConsentTokenBroker` draft

This goes in `celia_phr/crates/celia-cli/src/atd_broker.rs` (new file), wired from `celia-cli/src/serve.rs` when `--use-atd-server-http` is active (per SP-1.B §7 Step 2).

```rust
//! Celia's broker implementation for atd-server-http.
//!
//! Pairing-code (ce_<64hex>) → BearerIdentity translation, backed by
//! the SQLite consent table. Mirrors the resolve logic already in
//! agent_bootstrap.rs:226-271 (Tauri side) but adapted for an
//! always-on broker process — the celia-cli HTTP server queries its
//! own consent table directly, no parent IPC.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use atd_runtime::secrets::{BearerIdentity, BrokerError, SecretBundle, TokenBroker};
use rusqlite::{params, OptionalExtension};
use tokio::sync::RwLock;

const CACHE_TTL: Duration = Duration::from_secs(30);  // ≤60s revocation window per §4.7

pub struct CeliaConsentTokenBroker {
    db_path: std::path::PathBuf,
    /// Process-local cache: bearer → (identity, cached_at).
    /// Re-validates against SQLite after CACHE_TTL.
    cache: RwLock<std::collections::HashMap<String, (BearerIdentity, SystemTime)>>,
}

impl CeliaConsentTokenBroker {
    pub fn new(db_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            cache: RwLock::new(Default::default()),
        }
    }

    /// Adopter-side revocation hook. Celia Tauri's revoke UI calls this
    /// synchronously after writing `UPDATE consent SET status='revoked'`,
    /// invalidating the broker's process-local cache.
    pub async fn invalidate(&self, bearer: &str) {
        self.cache.write().await.remove(bearer);
    }
}

#[async_trait::async_trait]
impl TokenBroker for CeliaConsentTokenBroker {
    async fn resolve(
        &self,
        _caller_id: Option<&str>,
    ) -> Result<Option<Arc<SecretBundle>>, BrokerError> {
        // Celia does not pre-stage SecretBundles for callers (DEK lives
        // in KeyCache per patent §13.1; not relayed via SecretBundle).
        Ok(None)
    }

    async fn resolve_bearer(
        &self,
        bearer: &str,
    ) -> Result<Option<BearerIdentity>, BrokerError> {
        // ── Format gate ───────────────────────────────────────────────
        // Celia pairing codes: ce_<64hex>. Reject anything else early
        // so the cache and DB don't get probed by trial bearers.
        let token = bearer
            .strip_prefix("ce_")
            .ok_or_else(|| BrokerError::Lookup("missing ce_ prefix".into()))?;
        if token.len() < 16 || !token.chars().take(16).all(|c| c.is_ascii_hexdigit()) {
            return Err(BrokerError::Lookup("malformed pairing code".into()));
        }
        let token_short = &token[..16];

        // ── Cache fast-path ──────────────────────────────────────────
        let now = SystemTime::now();
        {
            let read = self.cache.read().await;
            if let Some((id, cached_at)) = read.get(bearer) {
                if now.duration_since(*cached_at).unwrap_or(CACHE_TTL) < CACHE_TTL {
                    // Still warm; re-check intra-broker expiry rules.
                    if let Some(exp) = id.expires_at {
                        if exp <= now { return Err(BrokerError::Expired); }
                    }
                    return Ok(Some(id.clone()));
                }
            }
        }

        // ── Cache miss / stale — go to SQLite ────────────────────────
        // Mirror agent_bootstrap.rs:226-271. SELECT the consent row
        // whose grantee ends with the token_short fragment, is active,
        // and in its effective_from/until window.
        let db_path = self.db_path.clone();
        let token_short_owned = token_short.to_string();
        let row = tokio::task::spawn_blocking(move || -> Result<_, BrokerError> {
            let conn = rusqlite::Connection::open(&db_path)
                .map_err(|e| BrokerError::Lookup(format!("open db: {e}")))?;
            let now_iso = celia_core::db::current_iso8601();
            // Three SELECT cases we distinguish:
            //   (a) row with status='active' and time window valid    → BearerIdentity
            //   (b) row matched on token_short but status != active   → Err(Revoked)
            //   (c) row matched but past effective_until              → Err(Expired)
            //   (d) no row at all                                     → Ok(None)
            //
            // Pull (status, grantee, scope, effective_until) and decide
            // in Rust so we get the precise error variant.
            let pattern = format!("agent:%:{token_short_owned}");
            let row: Option<(String, String, Option<String>, Option<String>)> = conn
                .query_row(
                    "SELECT status, grantee, scope, effective_until \
                     FROM consent \
                     WHERE grantee LIKE ?1 \
                     ORDER BY \
                       CASE status WHEN 'active' THEN 0 ELSE 1 END, \
                       created_at DESC \
                     LIMIT 1",
                    params![pattern],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .map_err(|e| BrokerError::Lookup(format!("query: {e}")))?;
            Ok((row, now_iso))
        })
        .await
        .map_err(|e| BrokerError::Internal(format!("spawn_blocking: {e}")))??;

        let (row_opt, _now_iso) = row;
        let Some((status, grantee, scope_opt, effective_until)) = row_opt else {
            return Ok(None); // Case (d): unknown bearer.
        };

        // Case (b): pairing code structurally matched but consent
        // status != 'active' (revoked, withdrawn, expired-explicit).
        if status != "active" {
            return Err(BrokerError::Revoked(format!("consent status: {status}")));
        }
        // Case (c): time window past.
        if let Some(until) = effective_until.as_deref() {
            let until_t = celia_core::db::parse_iso8601(until)
                .map_err(|e| BrokerError::Internal(format!("parse effective_until: {e}")))?;
            if until_t <= now {
                return Err(BrokerError::Expired);
            }
        }
        // Case (a): build BearerIdentity from grantee + scope.
        // grantee format is "agent:<name>:<token_short>"; caller_id =
        // the full grantee string (matches consent_matches_caller's
        // exact-equality contract at rbac.rs:319-329).
        let caller_id = grantee;
        let granted_capabilities: Vec<String> = scope_opt
            .as_deref()
            .unwrap_or("")
            .split([',', ' ', '\t'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        let expires_at = effective_until.as_deref().and_then(|s| {
            celia_core::db::parse_iso8601(s).ok()
        });

        let identity = BearerIdentity {
            caller_id,
            granted_capabilities,
            secrets: None,  // DEK stays in KeyCache; not relayed (§13.1)
            expires_at,
            cache_until: Some(now + CACHE_TTL),
        };

        // Write-through cache.
        self.cache.write().await.insert(bearer.to_string(), (identity.clone(), now));
        Ok(Some(identity))
    }

    fn accepted_token_formats(&self) -> &'static [&'static str] {
        &["ce-pairing-code"]
    }
}
```

**Key alignment notes with Celia code base.**
- `caller_id = full grantee string` ensures `rbac::consent_matches_caller` (`crates/celia-core/src/auth/rbac.rs:319-329`) still works on the in-process dispatcher path: that fn matches by exact equality on `agent_id` against the `consent.grantee` column. Returning the bare token would break the existing RBAC pipeline. **The LIKE-pattern `'agent:%:<token_short>'` is used only for the look-up direction (bearer → row); the equality direction (row → caller) uses the full grantee string.**
- `granted_capabilities` is derived from `consent.scope` (CSV) — matches how `get_allowed_tools_for_caller` (`rbac.rs:285-293`) decodes the same column.
- `secrets: None` honours patent §13.1: DEK travels through `KeyCache` only, never via `SecretBundle`. The Celia dispatcher reaches `state.cache.get(&user_id)` directly (`crates/celia-cli/src/http_server.rs:215-220`) — broker doesn't carry it.
- The `tokio::task::spawn_blocking` wrap is because `rusqlite::Connection::open` is synchronous; same pattern as `mcp_tools_call` (`crates/celia-cli/src/http_server.rs:407-411`).

**Migration step (per SP-1.B §7).** Step 2's `--use-atd-server-http` flag instantiates this broker, wires it into `HttpServerConfig.token_broker`. Step 3 removes the dead bearer-parse code from `crates/celia-cli/src/http_server.rs:294-306`. The §13.1 invariant holds throughout because the broker never touches `KeyCache`.

## 7. Migration path

### 7.1 atd-mvp side

| Step | Change | Tests affected | §13.1 / invariant check |
|---|---|---|---|
| 1 | Add `BearerIdentity.expires_at` + `cache_until` fields and `BrokerError::{Expired, Revoked}` variants. `#[non_exhaustive]` everywhere. | `crates/atd-runtime/src/secrets.rs` unit tests: 2 new cases for the new variants. | None |
| 2 | Ship `InMemoryBearerBroker` in same `secrets.rs`. | 4 new unit tests: insert/resolve happy path, missing token, expired, revoked. | None |
| 3 | Wire `atd-server-http` (SP-1.B) to call `broker.resolve_bearer` per §4.6 pipeline. Use new error variants for the 401/503/500 mapping table in §4.4. | New integration tests in `crates/atd-server-http/tests/e2e_bearer.rs` (extending SP-1.B's planned test): expired path, revoked path, NotConfigured path. | None |
| 4 | Add `TokenBroker::accepted_token_formats` defaulted method. | One test: `InMemoryBearerBroker::accepted_token_formats() == ["opaque"]`. | None |
| 5 | SSE refresh helper in `atd-server-http` for adopter-route reuse (§4.7). | `crates/atd-server-http/tests/e2e_sse_refresh.rs` (new): 60s heartbeat triggers re-resolve. | None |

No protocol-wire changes. No `atd-protocol` crate touches. No new ATD error codes (`ERR_BROKER_FAILED = 1003` covers JSON-RPC envelope-side broker errors; HTTP status codes carry the auth-side ones).

### 7.2 Celia side (depends on §7.1 + SP-1.B §7)

| Step | Change | §13.1 check |
|---|---|---|
| 1 | Add `crates/celia-cli/src/atd_broker.rs` with `CeliaConsentTokenBroker`. Compile-only, not yet wired. | gcore: `objdump -t libcelia_cli.so | grep -i keycache` — broker has no KeyCache reference. |
| 2 | Behind `--use-atd-server-http` flag (SP-1.B Step 2): instantiate broker, wire into `HttpServerConfig.token_broker`. Old `handle_mcp` bearer-parse code (`http_server.rs:294-306`) stays as dead code, compiled out behind a feature gate. | `pnpm --filter @celia/desktop test:dek` passes (broker never touches KeyCache). |
| 3 | Tauri revoke UI calls `broker.invalidate(bearer)` from the same code path that runs `UPDATE consent SET status='revoked'`. | Manual: revoke → request → 401 within ms; revoke → SSE stream → 401 frame within 60s. |
| 4 | Default-on per SP-1.B Step 3; remove dead bearer-parse from `http_server.rs`. | `pnpm --filter @celia/desktop test:e2e` Playwright smoke must pass with broker path. |

### 7.3 §13.1 invariant audit at every step

- **Step 1 (broker file compiled)**: zero `KeyCache` references; broker stores only `consent`-derived data.
- **Step 2 (broker wired)**: HTTP listener calls `broker.resolve_bearer` and gets `secrets: None`. DEK access path inside tool dispatch (`mcp_tools_call` → `state.cache.get`) is bypassed by the broker; identical to phase-1 path.
- **Step 3 (revoke wired)**: Tauri's revoke UI calls broker.invalidate; that fn touches only the in-broker `RwLock<HashMap>`, never the `KeyCache`.
- **Step 4 (default-on)**: removed code is the bearer-parse stub at `http_server.rs:294-306`, not the dispatcher; `KeyCache` plumbing inside `mcp_tools_call` (lines 407-461) is untouched.

`gcore` verifiability per phase-1 precedent: `objdump -t libceliaclibrocker.* | grep -c key_cache` = 0 at every step.

## 8. Test plan

### 8.1 atd-mvp unit tests (in `crates/atd-runtime/src/secrets.rs`)

- `in_memory_bearer_broker_resolves_known_bearer` — insert / resolve / assert `caller_id` + `granted_capabilities` round-trip.
- `in_memory_bearer_broker_returns_none_for_unknown_bearer` — empty broker → `Ok(None)`.
- `in_memory_bearer_broker_expired_returns_expired_error` — insert with `expires_at = now - 1s`, resolve, assert `Err(Expired)`.
- `in_memory_bearer_broker_revoked_returns_revoked_error` — insert, revoke, resolve, assert `Err(Revoked("..."))`.
- `bearer_identity_debug_redacts_secrets_field` — assert `format!("{:?}", id)` does not leak any `RedactedString` value (phase-1 contract preserved).
- `accepted_token_formats_default_returns_empty_slice` — default impl test.
- `accepted_token_formats_in_memory_broker_declares_opaque` — declared formats test.

### 8.2 atd-mvp integration tests (`crates/atd-server-http/tests/e2e_bearer*.rs`)

- `e2e_bearer_happy_path` — broker has bearer → POST /mcp tools/call → 200 with expected result.
- `e2e_bearer_unknown_returns_401` — broker returns `Ok(None)` → 401 + `WWW-Authenticate: Bearer error="invalid_token"`.
- `e2e_bearer_expired_returns_401_expired` — broker returns `Err(Expired)` → 401 + `error_description=expired`.
- `e2e_bearer_revoked_returns_401_revoked` — same, `Err(Revoked)`.
- `e2e_bearer_broker_internal_returns_500` — broker returns `Err(Internal)` → 500.
- `e2e_bearer_broker_lookup_returns_503` — broker returns `Err(Lookup)` → 503 + `Retry-After: 5`.
- `e2e_sse_refresh_revokes_mid_stream` — adopter test-route opens SSE; bearer revoked at t=10s; assert `auth_lost` event within 60s, then `done`.
- `e2e_anonymous_mode` — `require_bearer = false`, no Authorization header → request succeeds with anonymous `CapabilitySet`.
- `e2e_bearer_format_declaration` — `/initialize` response includes broker's accepted_token_formats.

### 8.3 Cross-project (Celia, depends on SP-1.B test scaffolding)

- `cargo test -p celia-cli broker_celia_bearer_round_trips` — unit test for `CeliaConsentTokenBroker::resolve_bearer` against an in-memory SQLite with seeded consent row; assert `BearerIdentity.caller_id == "agent:hermes:<short>"` and `granted_capabilities` derives correctly from `consent.scope`.
- `cargo test -p celia-cli broker_celia_revoked_consent_yields_revoked_err` — insert active consent, then update to `status='revoked'`, resolve_bearer, assert `Err(Revoked)`.
- `cargo test -p celia-cli broker_celia_past_effective_until_yields_expired` — insert with `effective_until = '1999-01-01...'`, resolve, assert `Err(Expired)`.
- `pnpm --filter @celia/desktop test:dek` — gcore DEK eviction check; passes at every migration step.
- `pnpm --filter @celia/desktop test:e2e` — Playwright smoke against `--use-atd-server-http` mode; tool list + tool call still work.

### 8.4 Cross-project — SHARP eval / agent eval regression

- Run Celia's `docs/agent-eval-2026-05-07-sample.md` regression with broker-on vs broker-off; tolerate ≤ 2% degradation (broker adds ~ms per call; should be invisible).
- Verify per-call latency increment ≤ 5ms p99 via existing benchmark scaffold; matches §4.4 budget.

## 9. Out of scope (future SPs)

| Feature | Why deferred | Sketch of future SP |
|---|---|---|
| UCAN delegation chains (`{iss, aud, att, exp}` signed envelopes) | Capability semantics; pre-empts SP-capability-v2 (Celia `ATD_FUTURE_ISSUES §1.A`) | SP-capability-v2 / SP-ucan-bearer — replaces `granted_capabilities: Vec<String>` with parsed UCAN attenuation |
| OAuth 2.1 token minting / refresh flows (/token, /authorize endpoints) | Identity-provider concern; broker can wrap an external introspection endpoint without ATD knowing | SP-token-broker-oauth — adds `TokenBroker::refresh(refresh_token)` and `/token` reverse-proxy helper |
| Cross-broker federation (introspection / JWKS / SCIM) | Single-broker / single-adopter case (Celia) does not need it | SP-token-broker-federation — adds `BrokerCluster` trait |
| mTLS client-cert auth | Different transport-axis; bearer is the only credential here | SP-mtls-transport — adds cert pinning to `HttpServerConfig` |
| JWT signature key plumbing inside atd-runtime | Crate sprawl; force-pick of `jsonwebtoken` / `josekit` / etc. | Adopter-side concern; not an ATD-runtime SP |
| Push-based revocation list endpoint (`broker.revocation_list_since(t)`) | Single-broker case does not need it; lazy `Err(Revoked)` propagation is sufficient | SP-token-broker-revocation-list |
| Session-stickiness via `Mcp-Session-Id` + bearer | Reserved by SP-1.B §4.7; sessions are a different SP | SP-streamable-http-sessions |
| Tool-level secret-access gates (`ToolDefinition::secrets_required`) | Phase-1 §11 already deferred; same concern | SP-tool-secrets-gate |
| Broker-driven secret rotation (`SecretBundle` refresh mid-call) | Adopter concern; broker can refresh internally between `resolve()` calls | Not currently planned |

## 10. `architecture.md` §10 row

Add after the phase-1 row:

```
| `TokenBroker` HTTP bearer integration (Phase 2) | Dispatch + atd-server-http | ✅ | SP-token-broker-phase2 | 2026-05-11 | Landed; `BearerIdentity` gains `expires_at` + `cache_until`; `BrokerError` gains `Expired` + `Revoked`; `InMemoryBearerBroker` ships in `secrets.rs`; `atd-server-http` consumes `resolve_bearer` per §4.6 pipeline + §4.7 SSE 60s recheck. No protocol change. First adopter: Celia (`CeliaConsentTokenBroker` against `consent` SQL table). Closes SP-1.B §4.4. UCAN delegation chains deferred to SP-capability-v2. |
```

## 11. References

### atd-mvp source (line-precise; spot-check targets)

1. `crates/atd-runtime/src/secrets.rs:29-39` — `RedactedString` wrapper; phase-2 reuses unchanged.
2. `crates/atd-runtime/src/secrets.rs:78-88` — `TokenBroker::resolve` (phase-1) trait shape; phase-2 widens via additive defaulted methods.
3. `crates/atd-runtime/src/secrets.rs:93-117` — `InMemoryTokenBroker` reference impl; phase-2 ships `InMemoryBearerBroker` as parallel sibling.
4. `crates/atd-server/src/connection.rs:51-69` — SP-12 Hello capability intersection; phase-2 HTTP pipeline mirrors this *per-request* per SP-1.B §4.3.
5. `crates/atd-server/src/connection.rs:241-262` — phase-1 `broker.resolve(caller_id)` dispatch call site; phase-2 adds the symmetric `broker.resolve_bearer(bearer)` call earlier in the pipeline.
6. `crates/atd-protocol/src/messages.rs:13-19` — `ERR_BROKER_FAILED = 1003`; phase-2 reuses for `Err(_)` cases that surface inside JSON-RPC envelope; HTTP status codes carry pre-envelope auth errors.
7. `crates/atd-protocol/src/messages.rs:34-52` — `Request::Hello`; HTTP path synthesises equivalent state per request from broker output.
8. `docs/protocol/wire-format.md:6` — declared "HTTP (Phase 2)" annotation; phase-2 token broker is what fills the auth side of that prophecy.
9. `docs/superpowers/specs/2026-04-27-sp-token-broker-phase1-design.md:14-30` — phase-1 decision matrix (Q1-Q10); phase-2 follows the same additive-default-impl convention.
10. `docs/superpowers/specs/2026-04-27-sp-token-broker-phase1-design.md:104-114` — phase-1 trait signature; phase-2 widens via `BearerIdentity` fields and `BrokerError` variants only.
11. `docs/superpowers/specs/2026-05-11-sp-streamable-http-design.md:103-178` — SP-1.B §4.4 declares `resolve_bearer` with default `NotConfigured`; phase-2 supplies the missing semantics.
12. `docs/superpowers/specs/2026-05-11-sp-streamable-http-design.md:86-101` — SP-1.B §4.3 per-request capability derivation; phase-2 broker is the substrate for it.
13. `docs/superpowers/specs/2026-05-11-sp-streamable-http-design.md:303-317` — SP-1.B §5.6 error mapping; phase-2 extends with `Expired` / `Revoked` rows.
14. `docs/superpowers/specs/2026-04-25-sp-listener-extract-design.md:23-24` — sibling-crate principle; phase-2 keeps brokers in `atd-runtime` not `atd-server-http` (matches phase-1 placement).

### Celia source

15. `crates/celia-cli/src/http_server.rs:294-306` — current Tier-0 bearer parse without validation; phase-2 broker replaces this dead code with real validation in Step 4 of §7.2.
16. `crates/celia-cli/src/http_server.rs:182-254` — `/chat/stream` SSE handler; phase-2 §4.7 SSE refresh proposal is verified against this concrete shape.
17. `crates/celia-core/src/auth/rbac.rs:319-329` — `consent_matches_caller` exact-equality contract; `CeliaConsentTokenBroker.caller_id` returns the full grantee string to keep this invariant.
18. `crates/celia-core/src/auth/rbac.rs:223-306` — `get_allowed_tools_for_caller` — phase-2 broker's capability derivation mirrors the same `consent.scope` CSV split (`rbac.rs:285-293`).
19. `apps/desktop/src-tauri/src/agent_bootstrap.rs:226-271` — Tauri pairing-code → consent SQL lookup; `CeliaConsentTokenBroker::resolve_bearer` is the always-on equivalent in the celia-cli process.
20. `docs/ATD_FUTURE_ISSUES.md:23-45` — UCAN roadmap (Issue 1.A); phase-2 explicitly defers UCAN, leaves door open for SP-capability-v2.

### External

21. RFC 6750 — Bearer Token Usage; phase-2 `WWW-Authenticate: Bearer error="..."` shape derives from §3.
22. RFC 7235 — HTTP Authentication; phase-2 places bearer-validate before envelope-parse per §2 fail-fast guidance.
23. RFC 6901 — JSON Pointer (orthogonal but cited by sibling SP-medical-middleware §4.5; included for consistency of references corpus).

---

**Summary.** Phase-2 widens `TokenBroker` with concrete bearer-auth semantics: opaque-random format default, broker-internal TTL exposed via `BearerIdentity.expires_at`, explicit `BrokerError::{Expired, Revoked}` variants, accepted-format hint, and an `InMemoryBearerBroker` reference. HTTP pipeline calls `resolve_bearer` between Origin gate and JSON-RPC parse; SSE long connections re-check every `min(expires_at - now, 60s)`. Revocation flows lazily — adopter UI revokes synchronously into the broker, listener discovers it on the next `resolve_bearer`. Celia's `CeliaConsentTokenBroker` pseudo-code in §6 demonstrates the contract against the real `consent` SQL with §13.1 DEK isolation preserved.
