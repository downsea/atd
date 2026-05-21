# SP-secret-bootstrap: parent-child secret injection (Pattern A generalised)

| Status | Draft |
| Created | 2026-05-11 |
| Author | cross-project subagent (celia_phr ↔ atd-mvp coordination) |
| Phase | ATD post-v0.3.0; complements SP-token-broker-phase{1,2} on the host-process axis |
| Related | SP-token-broker-phase1 (`2026-04-27-sp-token-broker-phase1-design.md`, server-side secret resolution); SP-token-broker-phase2 (`2026-05-11-sp-token-broker-phase2-design.md`, HTTP bearer wire); SP-capability-v2 (`2026-05-11-sp-capability-v2-design.md`, UCAN credential layer); Celia patent §13.1 + §13.5 (`docs/patents/main.zh.md:353`, `:367` — device-local volatile-key invariant); the Celia in-tree implementation this SP generalises: `apps/desktop/src-tauri/src/agent_bootstrap.rs` (parent listener) + `crates/celia-cli/src/parent_ipc.rs` (child client) + `crates/celia-cli/src/serve.rs:146-180` (caller). |

---

## 1. Motivation

**1.1 Patent §13.1 requires a transport that the `TokenBroker` extension point cannot provide.** Patent claim §13.1 (`/home/nan/code/pha/celia_phr/docs/patents/main.zh.md:353`) pins the encryption-at-rest invariant: *"…工具调用过程中的解密…均在所述用户终端计算设备本地完成"*, materialised in the codebase as a `KeyCache: Map<user_id, Arc<Zeroizing<Vec<u8>>>>` that lives only in the parent process's address space and is lost on restart. When the parent spawns a child server (`celia serve --atd` and its HTTP twin), the child needs the DEK to decrypt FHIR rows, but the parent cannot reasonably embed the DEK into argv (visible via `ps eww`), env (visible via `/proc/PID/environ`, breaks §13.1 when the child relays env to a debugger), or a temp file (lives on disk, breaks §13.5's "any binding's decrypt path must preserve §13.1"). SP-token-broker-phase1/2 solve a structurally different problem: a *running* server uses a broker to resolve secrets *for inbound callers*. They do not address how the server *itself* obtains its bootstrap secret from a parent host process at startup.

**1.2 Celia has shipped a Pattern A implementation that several upcoming adopters will copy-paste.** `apps/desktop/src-tauri/src/agent_bootstrap.rs:1-271` (parent listener, 271 LoC) and `crates/celia-cli/src/parent_ipc.rs:1-124` (child client, 124 LoC) together implement: a Unix domain socket at `$XDG_RUNTIME_DIR/celia-agent-bootstrap-<pid>.sock` bound `0600` (`agent_bootstrap.rs:79-84`, `:117-121`); a one-round-trip newline-delimited JSON handshake (`agent_bootstrap.rs:166-199`); a `KeyCache::put` immediately followed by zeroing the 32-byte scratch buffer in the child (`serve.rs:182-196`); and an env-scrub in the client (`parent_ipc.rs:69-70`). Every invariant — file permissions, single round-trip, env-scrub-after-success, zeroing scratch buffers — is one PR away from being subtly broken when the next adopter retypes it. **healthkit_cli** wants the same pattern for the user's HealthKit OAuth refresh token (cached in macOS Keychain by the host wrapper, injected into the headless `healthkit_cli serve` child). A **hospital HIS gateway** wants it for an X.509 client cert that pins the gateway to the hospital VPN. A **private-PHR vendor** wants it for a Doppler-resolved API key whose value should never appear on the spawned child's command line. All three are about to copy `agent_bootstrap.rs` and lose one or more of its invariants.

**1.3 The orthogonal-to-broker positioning is the whole point.** `TokenBroker` (`crates/atd-runtime/src/secrets.rs:136-184`) answers *"server has callers; how does the server get the right secret for each caller?"*. SP-secret-bootstrap answers *"how does the parent process hand the server itself its own secret(s) at spawn time, with §13.1-class transport guarantees?"* The two axes compose: a Celia `serve` child receives its DEK + `agent_id` via SP-secret-bootstrap at startup, then uses SP-token-broker-phase2's `CeliaConsentTokenBroker` to authenticate inbound HTTP bearers. Nothing in this SP replaces, weakens, or is replaced by the broker; the broker keeps doing its job, and `secret_bootstrap` is the lifecycle stage before any tool dispatch.

## 2. Goals

- Promote Pattern A from a Celia-internal hand-rolled implementation to an `atd-runtime::secret_bootstrap` module that any user-premise ATD adopter can wire.
- Define a single wire protocol (one round-trip, newline-delimited JSON over Unix domain socket) that preserves every existing Celia invariant: 0600 file mode, env-scrub-after-success in the child, zeroing scratch buffers, no on-disk artefact for the secret payload.
- Provide both ends as ergonomic building blocks: `secret_bootstrap::client` (one async function the child calls; `Ok(None)` when env not set, `Ok(Some(payload))` after a successful handshake), and `secret_bootstrap::server` (a builder around `tokio::net::UnixListener` that takes a user-supplied `Handler` closure resolving the pairing code).
- Preserve the schema-flexibility Celia needs (DEK + user_id + agent_id + db_path) while letting healthkit_cli reuse the same trait with a different payload (OAuth refresh token), via a typed extension point.
- Keep the trait surface in `atd-runtime` (no new crate). Mirror `TokenBroker`'s placement and Cargo footprint — only adds `tokio::net` usage already present transitively via `tokio` `net` feature.
- Document a 3-step Celia migration path with the §13.1 verification gate (`pnpm --filter @celia/desktop test:dek` + `crates/celia-cli/scripts/serve-pattern-a-test.sh`) running green at every step.
- Land a conformance test plan: client + server unit tests in `atd-runtime`; cross-project integration test in Celia post-migration; a `RedactedString`-style audit-safety test that asserts the secret payload's `Debug` impl never leaks values.
- Stay explicit about the v1 scope: Unix only, one-shot listener, one secret payload per spawn. Windows / persistent listener / cross-host / attestation are §9 carve-outs.

## 3. Non-goals

- **Cross-host secret bootstrap.** Parent and child run on the same OS instance, share a filesystem, and trust `XDG_RUNTIME_DIR`'s 0700 directory permission as the perimeter. Cross-host (a parent on host A spawning a child container on host B) needs mTLS + attestation + a key-exchange handshake — a different problem in a different SP.
- **Windows named-pipe support.** The Celia codebase already declares this out-of-scope at `parent_ipc.rs:119-124`. v1 covers Linux + macOS (both have UDS). A future SP-secret-bootstrap-windows can mirror the wire over `\\.\pipe\…` with the same JSON envelope.
- **Replacing `TokenBroker`.** Phase-1 + Phase-2 brokers stay exactly as today. This SP adds an orthogonal lifecycle stage before any broker is invoked.
- **Generic secret management (Vault, KMS, AWS Secrets Manager, Doppler).** Those are *resolver* concerns — they belong inside the parent process's `Handler` impl (the parent decides how it knows what to return); SP-secret-bootstrap is the *transport*.
- **TPM / attestation / SGX.** Out of scope. Adopters that need attestation layer it inside the `Handler` (e.g., validate the child binary's signature before responding); the wire stays the same.
- **Persistent listener / post-handshake reuse.** Each Celia child spawn calls Pattern A once and the connection closes (`agent_bootstrap.rs:8-9` and `parent_ipc.rs:11`). v1 SP keeps that — one accept-handle-close per child. A future SP can add a long-lived management channel; mixing it into v1 muddies the §13.1 surface.
- **Secret rotation while the child runs.** If the parent rotates the DEK, it restarts the child. Same shape as Celia's current model.
- **Audit log of bootstrap events.** The parent already owns the audit decision; v1 emits no `secret_bootstrap` event to `AuditSink`. A future SP can add an `on_bootstrap` hook on `AuditSink` if adopters want it.

## 4. Design

This is ~50% of the SP. Each subsection is one of the 8 decision points from the brief: chosen answer, evidence, rejected alternatives, trade-off table.

### 4.1 Module shape — `atd-runtime::secret_bootstrap` with `client.rs` + `server.rs` submodules; no new crate

**Decision.** Land the two ends inside `atd-runtime/src/secret_bootstrap/` as `mod.rs` + `client.rs` + `server.rs`. Re-export the four user-facing types from `atd-runtime/src/lib.rs`: `SecretBootstrapClient`, `SecretBootstrapServer`, `SecretPayload` (trait), `SecretBootstrapError`. No new Cargo crate; no new top-level dependency (the module uses `tokio::net::UnixListener` + `tokio::io::{AsyncBufReadExt, AsyncWriteExt}` + `serde_json`, all already in `atd-runtime`'s closure transitively via `tokio` workspace dep, see `crates/atd-runtime/Cargo.toml:14-23`).

**Why one module, not a new crate.**
1. **It is a runtime concern, not a transport concern.** `atd-server` (UDS) and `atd-server-http` (HTTP) are *binding-shaped* crates that handle wire frames. `secret_bootstrap` runs *before* any binding accepts traffic — it is part of the runtime lifecycle, sibling to `dispatch.rs` / `secrets.rs`. Placing it in `atd-runtime` keeps the lifecycle visible from one `cargo doc --open`.
2. **No mandatory binding dependency.** A child server that uses `atd-server-http` only (no UDS dispatch) still wants `secret_bootstrap` to receive its startup secret. If `secret_bootstrap` lived in `atd-server`, that HTTP-only deployment would have to drag in the UDS listener crate just to read its DEK. Keeping it in `atd-runtime` avoids the forced UDS dep.
3. **Mirrors `secrets.rs` placement.** `TokenBroker` lives in `atd-runtime/src/secrets.rs` (`crates/atd-runtime/src/secrets.rs:1-184`) and is consumed by both `atd-server` and `atd-server-http`. `secret_bootstrap` follows the same precedent — runtime extension point + reference impl, both bindings consume.

**Why not its own crate (`atd-secret-bootstrap`).** Tempting for a "this is a security primitive; isolate it" argument, but premature. The trait surface is small (`Handler` + payload trait); there is no v2-without-runtime-coupling risk to design around. Splitting now creates a second `Cargo.toml`, a second `lib.rs`, and a published crate per release without bringing modular benefit. SP-token-broker-phase1 §3 made the same call ("All in `atd-runtime`. Trait, types, and `InMemoryTokenBroker` ship together"). Same logic applies.

**Why submodules instead of one flat file.** Pattern A's client and server are conceptually one protocol but two execution roles. Splitting `client.rs` (~70 LoC of `tokio::net::UnixStream::connect` + JSON read) and `server.rs` (~140 LoC of `UnixListener` + accept loop + per-client task) makes both ends understandable in isolation. Test files co-locate: `client.rs` tests stub a server via `tokio::spawn`; `server.rs` tests stub a client via raw socket bytes.

**Trade-off table.**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| New crate `atd-secret-bootstrap` | Strong isolation; independently versioned | +Cargo.toml, +lib.rs, no modular payoff in v1; opposite of SP-broker-phase1 precedent | rejected |
| Module in `atd-runtime`, single file | Smallest surface | client + server roles tangle | rejected |
| Module in `atd-runtime`, `client.rs` + `server.rs` submodules | Mirrors `secrets.rs`; clear roles | None worth listing | **chosen** |
| Inside `atd-server` (UDS listener) | Reuses existing UDS code | Forces HTTP-only adopters to depend on UDS listener crate | rejected |

### 4.2 Wire protocol — keep Celia's newline-delimited JSON; lock the schema with a versioned envelope

**Decision.** v1 wire: a single newline-delimited JSON object request, a single newline-delimited JSON object response, then the connection closes. Same shape as `agent_bootstrap.rs:166-199`. Add a top-level `"v": 1` field on both request and response so future schema changes do not require a flag day. The reference shape (full example in §5):

```
Request:   {"v":1,"pairingCode":"<opaque-string>"}\n
Response:  {"v":1,"ok":true,"payload":{...}}\n
           or
           {"v":1,"ok":false,"error":"reason"}\n
```

`pairingCode` stays a black-box string from `atd-runtime`'s perspective (decision §4.5). `payload` is JSON-shaped (decision §4.3).

**Why newline-delimited JSON.**
1. **Already battle-tested in Celia.** `agent_bootstrap.rs:173-176` and `parent_ipc.rs:88-91` both rely on `read_line` semantics. Three production runs (Linux dev, macOS dev, the CI smoke test at `crates/celia-cli/scripts/serve-pattern-a-test.sh`) and the Python supervisor stand-in in that test all interop on plain newlines.
2. **One protocol library across all current adopters.** `serde_json` is already a workspace dep (`crates/atd-runtime/Cargo.toml:18`). Adding a binary framing layer (length-prefix + CBOR / msgpack) would push `bincode` or `ciborium` into the dep graph for marginal wire savings on a payload that runs once per spawn.
3. **Human-debuggable.** When the wire breaks, an operator can `socat - UNIX-CONNECT:$socket` and see exactly what's flying. A binary framing would need a custom tool.

**Why not reuse `atd-protocol`'s `Request` / `Response` envelopes.** Tempting (one envelope = one parser), but the protocol envelope (`Hello`, `RunTool`, etc., `crates/atd-protocol/src/messages.rs`) is shaped around tool dispatch — `request_id`, `client_id`, `requested_capabilities`. The bootstrap handshake happens *before* any of those concepts exist; pretending it's a tool call would be a misclassification. A separate envelope keeps the two protocols decoupled (v3 ATD protocol changes do not ripple into bootstrap).

**Why `"v": 1`.** Pattern A's current wire has no version (`agent_bootstrap.rs:41-61`). When the second adopter wants a field (e.g., healthkit_cli wants `refreshExpiresAt`), the parent must distinguish v1-shaped vs v2-shaped requests. A two-byte cost up front avoids a future flag day.

**Cross-OS note.** v1 ships Unix only (decision §3 carve-out). When SP-secret-bootstrap-windows lands, it will reuse the same JSON envelope over named pipes — only the transport differs. Including `"v": 1` from day one means that future SP can be additive without breaking parsing on the Unix side.

**Trade-off table.**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Newline-delimited JSON, versioned | Mirrors Celia today; debuggable; one parser | One extra `"v": 1` byte | **chosen** |
| `atd-protocol` envelope reuse | Code reuse | Misclassifies bootstrap as a tool call; tight coupling | rejected |
| Length-prefixed CBOR | Compact wire | Operator can't `socat`; +dep | rejected |
| Frameless raw bytes (DEK as 32 bytes) | Minimal | No error path; no extensibility | rejected |

### 4.3 Secret schema — trait-based payload, with a `CeliaSecretPayload` reference and a `RawSecretBundle` escape hatch

**Decision.** `atd-runtime::secret_bootstrap` defines a marker trait:

```rust
pub trait SecretPayload: serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug {
    fn redact(self) -> Self; // returns a Debug-safe variant for logging
}
```

The client function and server `Handler` are generic over `P: SecretPayload`. ATD-runtime ships **no concrete payload type** beyond a generic `RawSecretBundle = HashMap<String, RedactedString>` escape hatch for adopters that want the phase-1 broker bag shape. Celia defines `CeliaSecretPayload { user_id, agent_id, db_path, dek_hex }` inside `celia-cli` (the existing `parent_ipc::Bootstrap` struct, `crates/celia-cli/src/parent_ipc.rs:24-30`, gets a `#[derive(Deserialize, Serialize)]` and an `impl SecretPayload`).

**Why a trait, not a struct.**
1. **Adopters' secrets are different shapes.** Celia's payload has `dek_hex` (32 bytes hex). healthkit_cli's payload would have `oauth_refresh_token` + `oauth_access_token` + `oauth_expires_at`. A hospital HIS gateway's payload would be `client_cert_pem` + `client_key_pem` + `ca_chain_pem`. Forcing all into one struct either ends in a flat-`HashMap<String, String>` (untyped, easy to mistype field names) or a fat `enum` that pulls every adopter into `atd-runtime`'s dep graph (untenable).
2. **Type safety locally, opaqueness at the trait boundary.** Each adopter gets compile-time field checking inside their own crate; `atd-runtime` ships no concrete shape it needs to evolve.
3. **The trait is small.** Only `redact` is non-obvious — it returns a sibling value with secret fields replaced by `<redacted>` so callers can `tracing::info!("bootstrap got: {:?}", payload.redact())` without leaking. Celia's `CeliaSecretPayload::redact` would zero `dek_hex`; healthkit's would zero `oauth_refresh_token` + `oauth_access_token`.

**Why also a `RawSecretBundle` (untyped) escape hatch.** Some adopters genuinely want a string-keyed bag — they're already speaking `SecretBundle` from SP-token-broker-phase1 and only need bootstrap-time delivery. We ship `secret_bootstrap::RawSecretBundle = HashMap<String, RedactedString>` with a stock `impl SecretPayload` so they pick it up via `secret_bootstrap::client::<RawSecretBundle>()` instead of defining a one-off type. This costs one type alias and zero runtime cost.

**Why not Celia-specific struct in atd-runtime.** Putting `CeliaSecretPayload` in `atd-runtime` would (a) make every non-Celia adopter compile Celia-specific fields, (b) tie ATD's version cadence to Celia's schema evolution, (c) violate the "ATD is vendor-neutral" stance SP-listener-extract established (`docs/superpowers/specs/2026-04-25-sp-listener-extract-design.md:23-24`).

**Trade-off table.**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Adopter-specific generic via trait | Type-safe per-adopter; ATD-neutral | Slight `dyn`-cost in trait dispatch (negligible at one round-trip per spawn) | **chosen** |
| Single `CeliaSecretPayload` in atd-runtime | Concrete + obvious | Couples ATD to Celia; healthkit_cli has wrong fields | rejected |
| Untyped `HashMap<String, RedactedString>` only | Smallest API | Field-name typos compile; loses Celia's `dek_hex.len() == 32` check | rejected (we ship as opt-in `RawSecretBundle`) |
| Sum-type enum `Payload::{Celia, HealthKit, HISGateway, ...}` | One type | Every new adopter PRs `atd-runtime` | rejected |

### 4.4 Path discovery — `ATD_BOOTSTRAP_SOCKET` env var by default; adopter override is one constant

**Decision.** The client looks up two env vars by default:
- `ATD_BOOTSTRAP_SOCKET` — absolute path to the parent's listener socket.
- `ATD_BOOTSTRAP_PAIRING_CODE` — opaque string the child presents to the parent.

Both can be overridden per adopter via the builder: `SecretBootstrapClient::builder().socket_env_var("CELIA_BOOTSTRAP_SOCKET").pairing_code_env_var("CELIA_PAIRING_CODE").build()`. Celia keeps its existing constants (`parent_ipc::ENV_SOCKET_PATH = "CELIA_BOOTSTRAP_SOCKET"`, `:ENV_PAIRING_CODE = "CELIA_PAIRING_CODE"`, `parent_ipc.rs:19-20`) so the migration is a search-and-replace of the *implementation* without touching the parent's spawn config.

The default socket path computation, mirroring `agent_bootstrap.rs:79-84`:
```
$XDG_RUNTIME_DIR/<adopter-prefix>-secret-bootstrap-<pid>.sock      # Linux
$TMPDIR/<adopter-prefix>-secret-bootstrap-<pid>.sock               # macOS + fallback
```
`<adopter-prefix>` defaults to `atd` and is settable on the server builder.

**Why env var, not config file.**
1. **Parent must hand the path to child at spawn time.** Config file path → config file → child reads → connect is three I/O hops where env var → connect is one. The point of Pattern A is *minimum surface for the secret*; minimising the discovery surface is consistent.
2. **No on-disk artefact for the discovery side either.** A discovery file at `~/.config/celia/bootstrap.json` would mean the parent must clean it up on crash; env var disappears when the child does.
3. **Same shape as today.** `parent_ipc.rs:57-62` already does this for Celia. Generalising preserves the operational model.

**Why an env var per adopter (not a single global one).** Multiple adopters may run on the same host (Celia + healthkit_cli + an HIS gateway in different shells). Sharing one env var name causes accidental cross-pairing. Per-adopter env var defaults to `ATD_BOOTSTRAP_SOCKET` so a single-adopter host works out of the box, with the override path open for multi-adopter coexistence.

**Why not CLI flag.** CLI flags leak to `ps eww` exactly like argv DEK does. The pairing code is not the secret itself, but principle of least exposure: keep it out of argv too.

### 4.5 Pairing-code protocol — ATD treats it as opaque; adopter optionally validates format

**Decision.** From `atd-runtime`'s perspective, `pairing_code: String` is opaque. The client transmits whatever the env var contained; the server passes it to the adopter's `Handler::resolve(pairing_code)` and inspects the return. ATD-runtime does **not** prescribe a format. Celia continues to use `ce_<64hex>` (with the `agent_bootstrap.rs:227-233` prefix-check + 16-hex-char-fragment lookup), but Celia's validation lives in Celia's `Handler` impl — not in `secret_bootstrap`. `secret_bootstrap` ships a `validate_pairing_code_length(min: usize, max: usize)` helper for the common case (basic DoS protection — a 10-MB pairing code is a memory attack regardless of format), but format is otherwise the adopter's call.

**Why opaque.**
1. **Different adopters mint differently.** Celia mints `ce_<64hex>` from a Tauri-side `crypto.getRandomValues(32)`. healthkit_cli mints a UUIDv7. A hospital HIS gateway might use a Kerberos ticket (long base64). Picking one format excludes the others.
2. **SP-token-broker-phase2 §4.1 already settled this for the bearer wire.** `TokenBroker` takes `&str`, and the format is the adopter's call. SP-secret-bootstrap inherits that pattern across the parent-child boundary.
3. **The actual security gate is the parent's `Handler`.** Even if every adopter agreed on `ce_<64hex>`, the security comes from "parent looks up pairing code against an in-process state and returns the right secret iff there's a match". The format is post-decoration.

**Length-bounding helper.** A length check is *not* format prescription — it's basic input hygiene. A pairing code over 4 KB is almost certainly an attack (the largest reasonable mint, a fully-signed UCAN, is ~2 KB). `secret_bootstrap::server::accept_with_limits(max_pairing_code_bytes: 4096)` is the parameterised hook; default is 4096.

### 4.6 Lifetime semantics — one-shot per spawn (matches Celia today); §13.1 invariant preserved

**Decision.** v1 lifecycle:
1. Parent spawns listener task in `tokio::spawn`. Socket is created at `bind` time, permissioned `0600`, and cleaned up when the cancellation token fires *or* when the listener task panics.
2. Listener accepts **multiple** child connections over its lifetime (some adopters spawn multiple workers from one parent — e.g., Celia might run both an ATD serve + an HTTP serve, both children of the same Tauri). Each connection presents its own pairing code; the `Handler` resolves them independently.
3. **Each connection is one round-trip then close.** Parent does not retain per-connection state.
4. Pairing codes are not single-use in `atd-runtime` — that's the adopter's call. (Celia's current code does not invalidate consent rows on use, and the test `crates/celia-cli/scripts/serve-pattern-a-test.sh` relies on the multi-use property.) Adopters who want single-use semantics implement them in their `Handler` (e.g., delete the row on first successful resolve).
5. Parent's listener task lives for the parent process's lifetime. On parent shutdown, the cancellation token fires (`agent_bootstrap.rs:123-124` + `:134-138`), the listener stops, and the socket file is removed (`:157`).

**Why multi-connection per listener (not one-shot).** Celia's current listener is multi-connection (the accept loop is a `loop`, `agent_bootstrap.rs:133-156`). Restricting to one-shot would force a parent that spawns multiple workers to start multiple listeners with multiple socket paths — adopter complexity for no security gain.

**Why one round-trip per connection.** This is the §13.1 invariant. A persistent post-handshake channel would mean the DEK lives in *two* address spaces with an open kernel buffer between them — auditable, but a larger blast radius if the child process is compromised. One round-trip, close, secret in child memory only.

**Why no parent re-key while child runs.** If the parent rotates the DEK (user changes password), it kills the child and re-spawns. That's how Celia works today (`celia-core::auth::login` clears the `KeyCache` and re-derives). Mid-flight re-key is a §13.1-adjacent invariant nightmare and is correctly out of scope.

**Invariant relation to §13.1.** §13.1 says decryption happens on the device-local key cache. SP-secret-bootstrap puts the key into the child's `KeyCache` exactly once at startup. The wire (UDS) is kernel-mediated and file-permission-gated; never on disk, never in argv, never in env (env-scrub runs after handshake, `parent_ipc.rs:69-70`). Multi-connection on the parent side does not weaken this — each child gets exactly one key, and the parent's `KeyCache` is the device-local source.

### 4.7 Server-side trait — `Handler::resolve(&str) -> Result<P, Error>`, mirroring `TokenBroker` shape

**Decision.** The parent supplies an `Arc<dyn SecretBootstrapHandler<Payload = P>>`:

```rust
pub trait SecretBootstrapHandler: Send + Sync {
    type Payload: SecretPayload;
    fn resolve<'a>(
        &'a self,
        pairing_code: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Payload, BootstrapError>> + Send + 'a>>;
}
```

Same return-future shape as `TokenBroker::resolve` (`crates/atd-runtime/src/secrets.rs:81-87`) — owned-future, no `async_trait` dep. The server builder consumes the handler:

```rust
let server = SecretBootstrapServer::builder()
    .adopter_prefix("celia-agent")              // socket name segment
    .handler(Arc::new(MyHandler::new(...)))
    .build()?;
let handle = server.spawn(cancel_token).await?;
```

`handle.socket_path()` and `handle.shutdown()` mirror Celia's `AgentBootstrap` API (`agent_bootstrap.rs:92-104`).

**Why a trait, not a closure.**
1. **Handlers carry state.** Celia's handler needs the SQLite connection pool + `KeyCache` Arc + `consent` lookup logic. A trait with associated `Payload` type is clearer than a `Fn(&str) -> Fut` boxed-closure.
2. **`Payload` lives on the trait, not the function.** The associated type lets `SecretBootstrapServer<H>` infer the wire payload from the handler, so the user does not write the generic param twice.
3. **Mirrors `TokenBroker`.** A pattern that worked once should work twice. Operators learning `atd-runtime` see the same shape.

**Why owned-future return, not `async fn`.** Native `async fn in trait` (RPIT-in-traits) is stable in 1.85, but `TokenBroker` (`crates/atd-runtime/src/secrets.rs:136-184`) deliberately picked owned-`Pin<Box<dyn Future>>` for consistency with the project's MSRV stance and to avoid the `Send`-bound dance. We follow that precedent.

**Why a builder, not a `new(handler, config)` constructor.** Three optional knobs (`adopter_prefix`, `max_pairing_code_bytes`, custom socket path) push past the readable point of positional args. The builder pattern is consistent with `atd-server-http::HttpServerConfig::builder()` precedent.

### 4.8 Celia migration path — three steps, each with §13.1 verification

**Decision.** Cut over Celia in three commits, each one buildable and §13.1-verifiable.

**Step 1: Land `atd-runtime::secret_bootstrap` (atd-mvp side, no Celia change).**
- New module + `lib.rs` re-exports.
- Unit tests in `secret_bootstrap/{client.rs, server.rs}`: round-trip happy path, malformed JSON rejection, oversized pairing-code rejection, env-scrub helper test, `RedactedString`-style `Debug` non-leak test.
- §13.1 verification: N/A (atd-mvp doesn't have a DEK; tests instead assert the `RedactedString` non-leak).

**Step 2: Celia child (`crates/celia-cli/src/parent_ipc.rs`) → atd-runtime.**
- Replace `parent_ipc.rs:24-124` with a `use atd_runtime::secret_bootstrap` re-shaping. Define `CeliaSecretPayload` (the existing `Bootstrap` struct + `impl SecretPayload`).
- Keep the env var names (`CELIA_BOOTSTRAP_SOCKET` / `CELIA_PAIRING_CODE`) as constants; pass them to the builder.
- `try_socket_bootstrap()` becomes a thin wrapper around `secret_bootstrap::client::connect::<CeliaSecretPayload>()` with the same env-scrub-after-success semantics (which the runtime helper provides).
- §13.1 verification: `pnpm --filter @celia/desktop test:dek` (gcore eviction check) — should not change because the DEK transport is unchanged from the parent's perspective; only the *client crate's source* changed. The 32-byte zero-after-`KeyCache::put` step in `serve.rs:182-196` stays in Celia.
- Plus: `bash crates/celia-cli/scripts/serve-pattern-a-test.sh` continues to pass (the Python supervisor stand-in is wire-compatible because the JSON envelope is byte-for-byte identical apart from the new `"v":1` field, which a forward-compatible parser tolerates).

**Step 3: Celia parent (`apps/desktop/src-tauri/src/agent_bootstrap.rs`) → atd-runtime.**
- Replace 271 LoC of hand-rolled listener with `SecretBootstrapServer::builder().adopter_prefix("celia-agent").handler(Arc::new(CeliaConsentHandler::new(db_path, key_cache))).build()?`.
- `CeliaConsentHandler::resolve` carries the existing SQL (`agent_bootstrap.rs:226-271`): strip `ce_`, take 16-hex token short, `SELECT user_id, grantee FROM consent WHERE … LIMIT 1`, look up `KeyCache::get(user_id)`, return `CeliaSecretPayload`. No semantics change.
- The `AgentBootstrap` type (`agent_bootstrap.rs:86-105`) becomes a thin newtype around `SecretBootstrapServerHandle` so Tauri's `AppState` API stays binary-compatible.
- §13.1 verification at this step: rerun gcore eviction check **AND** the full Phase J serve-pattern-a-test.sh, since both sides have now changed.

**Why this order.** Land the new code in atd-mvp first so Celia can `cargo update` and pull it in. Migrate the child before the parent — child migration is a smaller surface (1 file, ~125 LoC) and shipping it ensures the wire is stable. Migrate the parent last so the gcore + Pattern-A test running on the post-migration parent is the authoritative regression.

**Why three commits, not one.** Each commit is independently revertable. If step 3 breaks an invariant the gcore test missed, step 2 still works and Celia keeps shipping. SP-broker-phase1 used the same "land trait, then adopt" pattern.

**Trade-off table for the migration order.**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Three commits: runtime → child → parent | Each independently verifiable; smallest blast | Three review cycles | **chosen** |
| One mega-commit | One review | One bug means three reverts | rejected |
| Parent first, then child | Forces wire freeze on parent side | Wire change without consumers; can't verify on Celia | rejected |

## 5. Wire format reference

### 5.1 Request envelope

```
{"v":1,"pairingCode":"ce_0123…64hex"}\n
```

- `v`: u8 = 1. Servers MUST reject `v` values they do not recognise.
- `pairingCode`: non-empty string, ≤ 4 KiB by default. Opaque to ATD; format-validated by the adopter.

### 5.2 Response envelope (success)

```
{"v":1,"ok":true,"payload":{ <P-shaped JSON> }}\n
```

`payload` is the JSON-serialised `SecretPayload` `P`. For Celia's `CeliaSecretPayload`:

```
{"v":1,"ok":true,"payload":{
  "userId":"<uuid-or-cuid>",
  "agentId":"agent:hermes:abc0123456789def",
  "dbPath":"/home/user/.local/share/celia/celia.db",
  "dekHex":"<64-hex-chars-placeholder>"
}}\n
```

(The `dekHex` value is intentionally illustrated as a placeholder. In production it is a 64-character hex string the child decodes into a 32-byte buffer, calls `KeyCache::put`, then zeros the scratch.)

### 5.3 Response envelope (error)

```
{"v":1,"ok":false,"error":"pairing code does not match any active consent"}\n
```

- `error`: free-form string. ATD MUST NOT include secret values in error strings (the trait surface guarantees this is the adopter handler's responsibility, but the `BootstrapError` enum's `Display` impl scrubs known-secret patterns).

### 5.4 Error codes

| `BootstrapError` variant | Wire `error` shape | Client behaviour |
|---|---|---|
| `InvalidPairingCode { reason }` | `"invalid pairing code: <reason>"` | Hard fail; do not retry. |
| `PairingNotFound` | `"pairing code does not match any active consent"` | Hard fail; do not retry. |
| `SecretNotAvailable { detail }` | `"secret not available: <detail>"` | Hard fail; surface to user (e.g., "ask user to unlock Celia first"). |
| `Internal { detail }` | `"internal handler error"` (detail redacted from wire) | Retryable; log `detail` on the parent side only. |

### 5.5 Connection lifecycle

1. Client `connect` → server `accept`.
2. Client writes request line, then half-shutdowns write side (or just keeps writing; server reads to first `\n`).
3. Server writes response line + `\n` + flushes.
4. Server drops the stream; client `read_line` returns; client drops the stream.

No multiplexing, no keepalive, no graceful close negotiation.

## 6. atd-runtime module shape

### 6.1 Directory

```
crates/atd-runtime/src/
├── secret_bootstrap/
│   ├── mod.rs        — re-exports, error type, RawSecretBundle, SecretPayload trait
│   ├── client.rs     — SecretBootstrapClient + connect()
│   └── server.rs     — SecretBootstrapServer + SecretBootstrapHandler trait + handle
```

### 6.2 Trait + struct signatures (pseudo-Rust, no impls)

```rust
// crates/atd-runtime/src/secret_bootstrap/mod.rs

pub trait SecretPayload:
    serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug + Send + 'static
{
    /// Return a sibling value safe to log. Implementors replace
    /// secret-bearing fields with `<redacted>` markers.
    fn redact(&self) -> Self;
}

pub type RawSecretBundle = std::collections::HashMap<String, crate::RedactedString>;
impl SecretPayload for RawSecretBundle { /* derive-redact */ }

#[derive(thiserror::Error, Debug)]
pub enum BootstrapError {
    #[error("invalid pairing code: {reason}")]
    InvalidPairingCode { reason: String },
    #[error("pairing code does not match any active consent")]
    PairingNotFound,
    #[error("secret not available: {detail}")]
    SecretNotAvailable { detail: String },
    #[error("internal handler error")]
    Internal { detail: String },
    #[error("transport: {0}")]
    Transport(String),
}
```

```rust
// crates/atd-runtime/src/secret_bootstrap/client.rs

pub struct SecretBootstrapClient {
    socket_env_var: Cow<'static, str>,         // default "ATD_BOOTSTRAP_SOCKET"
    pairing_code_env_var: Cow<'static, str>,   // default "ATD_BOOTSTRAP_PAIRING_CODE"
    max_pairing_code_bytes: usize,             // default 4096
    scrub_env_after_success: bool,             // default true
}

impl SecretBootstrapClient {
    pub fn builder() -> SecretBootstrapClientBuilder { /* ... */ }

    /// Returns `Ok(None)` when env vars are unset (caller may fall back).
    /// Returns `Err` when env vars ARE set but handshake fails — caller MUST NOT
    /// silently fall back, to avoid masking a misconfigured parent.
    pub async fn connect<P: SecretPayload>(&self) -> Result<Option<P>, BootstrapError>;
}
```

```rust
// crates/atd-runtime/src/secret_bootstrap/server.rs

pub trait SecretBootstrapHandler: Send + Sync {
    type Payload: SecretPayload;
    fn resolve<'a>(
        &'a self,
        pairing_code: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Self::Payload, BootstrapError>> + Send + 'a>>;
}

pub struct SecretBootstrapServer<H: SecretBootstrapHandler> { /* ... */ }
pub struct SecretBootstrapServerHandle { /* socket_path, cancel */ }

impl<H: SecretBootstrapHandler + 'static> SecretBootstrapServer<H> {
    pub fn builder() -> SecretBootstrapServerBuilder<H> { /* ... */ }
    pub async fn spawn(self, cancel: CancellationToken)
        -> Result<SecretBootstrapServerHandle, BootstrapError>;
}

impl SecretBootstrapServerHandle {
    pub fn socket_path(&self) -> &Path;
    pub fn is_running(&self) -> bool;
    pub fn shutdown(&self);
}
```

### 6.3 Adopter usage (~10 lines)

```rust
// Child (e.g., celia-cli/src/serve.rs Pattern A path):
use atd_runtime::secret_bootstrap::{SecretBootstrapClient};

let client = SecretBootstrapClient::builder()
    .socket_env_var("CELIA_BOOTSTRAP_SOCKET")
    .pairing_code_env_var("CELIA_PAIRING_CODE")
    .build();
let payload: Option<CeliaSecretPayload> = client.connect().await?;

// Parent (e.g., apps/desktop/src-tauri/src/agent_bootstrap.rs):
use atd_runtime::secret_bootstrap::SecretBootstrapServer;

let server = SecretBootstrapServer::builder()
    .adopter_prefix("celia-agent")
    .handler(Arc::new(CeliaConsentHandler::new(db_path, key_cache)))
    .build();
let handle = server.spawn(cancel_token).await?;
std::env::set_var("CELIA_BOOTSTRAP_SOCKET", handle.socket_path());
```

## 7. Migration path (Celia side)

Three commits, each with the §13.1 invariant verified before the next.

| # | Commit | Files | §13.1 verification |
|---|---|---|---|
| 1 | `feat(atd-runtime): secret_bootstrap module (parent-child secret injection)` | `crates/atd-runtime/src/secret_bootstrap/{mod,client,server}.rs` (~250 LoC); `crates/atd-runtime/src/lib.rs` re-exports | atd-runtime side: `cargo test -p atd-runtime` covers RedactedString-style non-leak in `Debug`/`Display`. No DEK involved. |
| 2 | `refactor(celia-cli): parent_ipc → atd-runtime::secret_bootstrap::client` | `crates/celia-cli/src/parent_ipc.rs` slim down to ~25 LoC (env-scrub stays here for backcompat); add `CeliaSecretPayload` (impl SecretPayload); `crates/celia-cli/src/serve.rs` no change (calls `try_socket_bootstrap` which now delegates) | Run `pnpm --filter @celia/desktop test:dek` + `bash crates/celia-cli/scripts/serve-pattern-a-test.sh`. Both must remain green. (The wire is byte-compatible aside from the `"v":1` field, which the Python supervisor stand-in must be updated to echo back.) |
| 3 | `refactor(celia-desktop): agent_bootstrap → atd-runtime::secret_bootstrap::server` | `apps/desktop/src-tauri/src/agent_bootstrap.rs` shrinks from 271 LoC → ~60 LoC (handler + thin newtype around the runtime handle); new `CeliaConsentHandler` struct carries the SQL + KeyCache | Same two verifications. Plus: ensure Tauri's `AppState::AgentBootstrap` API surface (`socket_path()`, `is_running()`, `shutdown()`) is unchanged. |

The `crates/celia-cli/scripts/serve-pattern-a-test.sh` Python supervisor stand-in needs **one line** updated: echo back `"v": 1` in the response JSON (its current shape lacks the version; it must add it for v1 wire). That's the only test-side change.

After step 3, the LoC budget on Celia drops from 395 (`agent_bootstrap.rs` 271 + `parent_ipc.rs` 124) to ~85 (handler + payload + thin wrappers), with the security-sensitive 0600-mode-set + accept-loop + env-scrub-after-success logic now centralised + unit-tested in `atd-runtime`.

## 8. Test plan

### 8.1 Unit (in `atd-runtime`)

- `client.rs`: env-var-unset → `Ok(None)`; env-var-set + valid socket + valid response → `Ok(Some(payload))`; env-var-set + connect fail → `Err(Transport)`; env-var-set + malformed response → `Err(Transport)`; env-scrub-after-success: assert `std::env::var(...)` returns `Err(NotPresent)` after a successful return.
- `server.rs`: bind socket → 0600 mode confirmed via `fs::metadata`; accept multiple clients sequentially; oversized pairing code → fast-reject `InvalidPairingCode`; handler error → wire `ok: false` envelope; shutdown via cancel-token removes socket file.
- `mod.rs`: `RawSecretBundle` round-trips through JSON; `RedactedString` value never appears in `format!("{:?}", payload)`.

### 8.2 Integration (cross-task, in `crates/atd-runtime/tests/`)

- `secret_bootstrap_round_trip.rs`: spawn server with a stub handler, spawn client in a Tokio task, assert payload matches.
- `secret_bootstrap_disk_invariant.rs`: after a successful round-trip, assert (a) the socket file's parent directory does not contain any new files matching `*.{dek,key,secret}`, (b) `/proc/<child-pid>/environ` (Linux only) does not contain the pairing-code value after `connect().await` resolves.

### 8.3 Cross-project (Celia side, post-migration)

- `pnpm --filter @celia/desktop test:dek` (gcore double-dump DEK eviction).
- `bash crates/celia-cli/scripts/serve-pattern-a-test.sh` (full Python supervisor + real `celia serve --atd` round-trip).
- Celia `cargo test --workspace` — all 159 tests green.

## 9. Out of scope (future SPs)

- **SP-secret-bootstrap-windows.** Named-pipe transport on Windows; same JSON envelope, different listener.
- **SP-secret-bootstrap-attestation.** TPM / SGX / signed-child-binary handshake before the parent releases the secret.
- **SP-secret-bootstrap-persistent.** Long-lived management channel reusing the socket post-handshake (e.g., for live revocation, rekey, child-status push).
- **SP-secret-bootstrap-cross-host.** Parent on host A, child on host B (container, remote VM); requires mTLS + an attestation key-exchange.
- **SP-secret-bootstrap-audit.** Hook on `AuditSink::on_bootstrap` so the parent can record `{spawn_id, pairing_code_short, payload_redacted, outcome}`. Defer until at least one adopter asks.
- **SP-secret-bootstrap-vault-resolver.** Reference handlers that talk to HashiCorp Vault / AWS Secrets Manager / Doppler / 1Password CLI. Adopter-side concern; not in `atd-runtime`.
- **SP-secret-bootstrap-uds-passing.** Pass an open file descriptor (e.g., a SQLite WAL fd) over `SCM_RIGHTS` alongside the JSON payload. Useful for adopters who want the child to inherit an opened DB without re-opening on disk; orthogonal to the secret wire.

## 10. References

**atd-mvp source cites (≥ 8):**
- `crates/atd-runtime/src/secrets.rs:29-51` — `RedactedString` precedent we mirror in `BootstrapError::Internal { detail }`'s scrub.
- `crates/atd-runtime/src/secrets.rs:81-87` — owned-future return type for `TokenBroker::resolve`; same shape for `SecretBootstrapHandler::resolve`.
- `crates/atd-runtime/src/secrets.rs:136-184` — `TokenBroker` trait + default-impl precedent for adopters overriding selectively.
- `crates/atd-runtime/src/secrets.rs:204-213` — `InMemoryTokenBroker` ergonomic pattern referenced for the future `InMemorySecretBootstrapHandler` reference impl.
- `crates/atd-runtime/src/lib.rs:6-31` — module re-export pattern we extend by adding `pub mod secret_bootstrap;` + four public types.
- `crates/atd-runtime/Cargo.toml:14-23` — confirms `tokio` + `serde_json` + `thiserror` are already in scope; no new dep needed.
- `docs/superpowers/specs/2026-04-27-sp-token-broker-phase1-design.md:14-27` — Q1-Q10 decisions whose shape we mirror.
- `docs/superpowers/specs/2026-05-11-sp-token-broker-phase2-design.md:13-39` — opaque-token decision (§4.1) we inherit for the pairing-code-is-opaque stance.
- `docs/superpowers/specs/2026-04-25-sp-listener-extract-design.md:23-24` — "runtime must stay transport-agnostic" precedent that justifies module placement.
- `docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md:46-69` — UCAN-lite trade-off table format we mirror.
- `crates/atd-server/src/server.rs:38-60` — `Server::new` builder pattern referenced for `SecretBootstrapServer::builder()`.

**celia_phr source cites (≥ 4):**
- `apps/desktop/src-tauri/src/agent_bootstrap.rs:79-84` — `default_socket_path` we generalise into `<adopter-prefix>-secret-bootstrap-<pid>.sock`.
- `apps/desktop/src-tauri/src/agent_bootstrap.rs:117-121` — `chmod 0o600` after bind; runtime helper performs this once for all adopters.
- `apps/desktop/src-tauri/src/agent_bootstrap.rs:166-199` — `handle_client` round-trip JSON read/write we lift into `secret_bootstrap::server`.
- `apps/desktop/src-tauri/src/agent_bootstrap.rs:226-271` — Celia-specific `resolve_bootstrap` that becomes `CeliaConsentHandler::resolve`.
- `crates/celia-cli/src/parent_ipc.rs:19-20` — env-var constants; Celia keeps these via the builder override.
- `crates/celia-cli/src/parent_ipc.rs:56-73` — `try_socket_bootstrap` env-scrub-after-success that the runtime helper subsumes.
- `crates/celia-cli/src/parent_ipc.rs:76-116` — `handshake` body that becomes the runtime client's `connect`.
- `crates/celia-cli/src/serve.rs:136-197` — `ServerState::from_env` + `from_bootstrap` showing exactly how the runtime payload is consumed (`KeyCache::put` + zero scratch buffer).
- `crates/celia-cli/scripts/serve-pattern-a-test.sh` — regression test that must pass on each migration step.
- `docs/patents/main.zh.md:353` — §13.1 device-local volatile-key claim text.
- `docs/patents/main.zh.md:367` — §13.5 multi-binding decrypt-path invariant we preserve.
