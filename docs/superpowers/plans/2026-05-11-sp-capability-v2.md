# SP-capability-v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land UCAN-lite capability tokens for ATD — JWT-shape on the wire, Ed25519 only, `did:key` only, additive to SP-12's string allow-list. `Hello` gains optional `ucan_tokens`; `granted = granted_strings ∪ granted_ucan`. Sub-agent delegation chains (issuer → audience attenuation) become expressible. SP-12 adopters keep working untouched.

**Adopters:** **celia_phr** is the validation adopter — unblocks Hermes "orchestrator + N specialised children" workflow (spec §1.2). **healthkit_cli** is passive — `Hello.ucan_tokens = None` path must remain green at every phase.

**Architecture:** Pure-CPU UCAN verifier in `atd-runtime` (no network IO, no DEK touch). Additive `Hello` field; new error codes `ERR_UCAN_INVALID = 1010` / `EXPIRED = 1011` / `DELEGATION_TOO_DEEP = 1012` / `AUDIENCE_MISMATCH = 1013`. `TokenBroker::resolve_bearer` gains a `ucan-jwt` branch alongside existing opaque. No new trait methods.

**Tech Stack:** Rust 2021 (workspace edition), Tokio. New crate deps: `ed25519-dalek` (signature verification), `base64` (already in workspace), `serde_json` (already), `multibase` (did:key parsing) — pin exact versions before Task 1.

**Spec:** [`../specs/2026-05-11-sp-capability-v2-design.md`](../specs/2026-05-11-sp-capability-v2-design.md) — refer to spec §-numbers throughout this plan.

---

## Phase A — Protocol-level additive change

### Task 1: Add `Hello.ucan_tokens` + new error codes

**Files:**
- Modify: `crates/atd-protocol/src/messages.rs` (Hello struct + error code constants)
- Modify: `crates/atd-protocol/src/messages.rs` roundtrip test (verify back-compat with `ucan_tokens` absent)
- Modify: `docs/protocol/wire-format.md` (Hello message section + error codes table)
- Modify: `docs/protocol/error-codes.md` (4 new entries)
- Modify: `atd-protocol-schema.json` regen via `cargo run -p atd-protocol --bin gen-schema`

- [ ] **Step 1: Locate Hello + error-code definitions**

```bash
grep -nE 'pub const ERR_|struct Hello|requested_capabilities' crates/atd-protocol/src/messages.rs
```

Confirm: `Hello` at line 32-39 (spec §10 ref 3), `ERR_CAPABILITY_DENIED = 1001` at line 6 (spec §10 ref 4).

- [ ] **Step 2: TDD — RED — write failing roundtrip test first**

Append to `crates/atd-protocol/tests/types_roundtrip.rs`:

```rust
#[test]
fn hello_ucan_tokens_roundtrip() {
    let h = Request::Hello {
        client_id: "agent-B".into(),
        requested_capabilities: vec!["records:read".into()],
        ucan_tokens: vec!["dummy.jwt.compact".into()],
    };
    let j = serde_json::to_string(&h).unwrap();
    assert!(j.contains("ucan_tokens"));
    let back: Request = serde_json::from_str(&j).unwrap();
    if let Request::Hello { ucan_tokens, .. } = back {
        assert_eq!(ucan_tokens, vec!["dummy.jwt.compact"]);
    } else { panic!() }
}

#[test]
fn hello_ucan_tokens_back_compat_absent() {
    let json = r#"{"type":"hello","client_id":"X","requested_capabilities":[]}"#;
    let back: Request = serde_json::from_str(json).unwrap();
    if let Request::Hello { ucan_tokens, .. } = back {
        assert!(ucan_tokens.is_empty());
    } else { panic!() }
}
```

Run: `cargo test -p atd-protocol hello_ucan` → expect compile fail (field doesn't exist).

- [ ] **Step 3: GREEN — add field + error constants**

In `crates/atd-protocol/src/messages.rs`:

```rust
pub const ERR_UCAN_INVALID: u16 = 1010;
pub const ERR_UCAN_EXPIRED: u16 = 1011;
pub const ERR_DELEGATION_TOO_DEEP: u16 = 1012;
pub const ERR_AUDIENCE_MISMATCH: u16 = 1013;

#[derive(Serialize, Deserialize, ...)]
pub enum Request {
    Hello {
        client_id: String,
        requested_capabilities: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ucan_tokens: Vec<String>,
    },
    // ... existing variants unchanged
}
```

- [ ] **Step 4: Run tests + regenerate schema**

```bash
cargo test -p atd-protocol --all-features
cargo run -p atd-protocol --bin gen-schema > atd-protocol-schema.json
git diff atd-protocol-schema.json  # expect: 4 new error codes + ucan_tokens field
```

- [ ] **Step 5: Update wire-format.md + error-codes.md**

Append spec §5.2 before/after example to `docs/protocol/wire-format.md` Hello section. Append the 4 codes to `docs/protocol/error-codes.md` table.

- [ ] **Step 6: Commit**

```
feat(atd-protocol): SP-capability-v2 Phase A — Hello.ucan_tokens + error codes 1010-1013

Additive field, default-empty (skip_serializing_if). Pre-SP-cap-v2 servers
parse Hello as before via serde default. Codes 1010-1013 reserved for the
runtime verifier landing in Phase B.

Spec: docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md §4.2 + §5.4
```

---

## Phase B — `atd-runtime::ucan` module (verifier + chain walker)

### Task 2: `ucan::parse` — JWT compact form decoder

**Files:**
- Create: `crates/atd-runtime/src/ucan/mod.rs`
- Create: `crates/atd-runtime/src/ucan/parse.rs`
- Create: `crates/atd-runtime/src/ucan/types.rs` (`UcanHeader`, `UcanPayload`, `UcanCapability`)
- Create: `crates/atd-runtime/src/ucan/error.rs` (`UcanParseError` variants)
- Modify: `crates/atd-runtime/src/lib.rs` (add `pub mod ucan;`)
- Modify: `crates/atd-runtime/Cargo.toml` (deps: `ed25519-dalek`, `multibase`)

- [ ] **Step 1: Pin deps + scaffold module**

Add to `crates/atd-runtime/Cargo.toml`:
```toml
ed25519-dalek = { version = "2", default-features = false, features = ["pkcs8"] }
multibase     = "0.9"
```

- [ ] **Step 2: TDD — write parse tests first (spec §8.1)**

Create `crates/atd-runtime/src/ucan/tests/parse.rs` covering spec §8.1 cases:
- `parse_well_formed_token_succeeds` — canonical 3-part JWT round-trips
- `parse_unsupported_alg_rejects` — `alg=RS256` → `Err(UnsupportedAlg)`
- `parse_non_did_key_issuer_rejects` — `iss=did:web:example.org` → `Err(UnsupportedDidMethod)`
- `parse_malformed_jwt_rejects` — missing `.` segments → `Err(MalformedJwt)`

Each test should construct the JWT compact form via a small `test_helpers::build_jwt(header, payload, sk)` you'll add in Step 3.

Run: `cargo test -p atd-runtime ucan::tests::parse` → expect compile fail.

- [ ] **Step 3: GREEN — implement parser + types**

Implement `ucan::parse::parse_jwt(token: &str) -> Result<UcanPayload, UcanParseError>` per spec §4.1 + §4.3:
- Split on `.`; reject if != 3 segments
- base64url-decode header; deserialize via `serde_json`; reject if `alg != "EdDSA"` or `typ != "ucan/1.0+jwt"`
- base64url-decode payload; deserialize; reject if `cmd != "atd-cap"` (spec §4.5)
- Reject if `iss` or `aud` doesn't start with `did:key:z` (spec §4.4)

The signature itself is NOT verified here — that's Task 3's chain walker. Parse is purely structural.

- [ ] **Step 4: Verify all 4 parse tests pass + format**

```bash
cargo test -p atd-runtime ucan::tests::parse
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 5: Commit**

```
feat(atd-runtime): SP-capability-v2 Phase B.1 — ucan::parse module

Parses UCAN-lite JWT compact form per spec §4.1. Structural validation only
(alg + typ + cmd + did:key prefix); signature + chain verification in Phase B.2.

Spec: docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md §4.1, §4.3, §4.4, §4.5
```

### Task 3: `ucan::verify` — signature + chain walker

**Files:**
- Create: `crates/atd-runtime/src/ucan/verify.rs`
- Create: `crates/atd-runtime/src/ucan/chain.rs` (chain walker)
- Modify: `crates/atd-runtime/src/ucan/mod.rs` (re-exports)
- Modify: `crates/atd-runtime/src/ucan/error.rs` (new `VerifyError` variants)

- [ ] **Step 1: TDD — chain walker test cases (spec §8.1 cont.)**

Add to `crates/atd-runtime/src/ucan/tests/verify.rs`:
- `verify_signature_with_wrong_key_rejects` — tampered payload fails
- `expired_token_returns_err_expired` — `exp` in the past → `Err(Expired)`
- `chain_depth_exceeded_rejects` — 6-deep chain with default `max_depth=5` → `Err(TooDeep)`
- `audience_mismatch_rejects` — chain's deepest `aud` ≠ connection `client_id` → `Err(AudienceMismatch)`
- `attenuation_intersect_succeeds` — root `[a,b,c]` → child `[a,b]` → grandchild `[a]` → effective `[a]`
- `attenuation_widening_rejects` — child claims `[a,b,c,d]` when parent grants `[a,b,c]` → `Err(Widening)`
- `revoked_cid_rejects` — stub revocation store contains CID; verifier returns `Err(Revoked)`

Use a `test_helpers::build_chain(depth, caps_per_link, ...)` builder.

- [ ] **Step 2: GREEN — implement `verify::verify_chain`**

Signature per spec §4.6:

```rust
pub struct VerifyConfig {
    pub max_chain_depth: u8,                 // default 5
    pub expected_audience: String,           // connection's client_id
    pub revocation_store: Option<Arc<dyn UcanRevocationStore>>,
}

pub fn verify_chain(
    leaf: &UcanPayload,
    proof_chain: &[UcanPayload],   // ordered root-first
    sigs: &[(&[u8], &str)],        // (signed-bytes, sig-base64) per token
    cfg: &VerifyConfig,
    now: SystemTime,
) -> Result<CapabilitySet, VerifyError>;
```

Walk chain root→leaf:
1. For each link: verify signature against `iss`'s did:key.
2. Verify `aud` of link N == `iss` of link N+1.
3. Verify each link's `args.caps ⊆ parent's args.caps` (attenuation, spec §4.5 + §4.6).
4. Verify `exp` for each link > `now`.
5. Verify leaf's `aud` == `cfg.expected_audience`.
6. Check chain depth ≤ `cfg.max_chain_depth`.
7. Check no link's CID is in `cfg.revocation_store` (spec §4.7).

Return effective `CapabilitySet` = leaf's `args.caps` (already attenuation-validated).

- [ ] **Step 3: Run all 7 verify tests + the 4 from Task 2**

```bash
cargo test -p atd-runtime ucan::
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: Commit**

```
feat(atd-runtime): SP-capability-v2 Phase B.2 — ucan::verify chain walker

Signature verification (Ed25519 only), audience pinning, attenuation check,
chain-depth limit, revocation-store consultation. All per spec §4.6 + §4.7.

Tests: 7 verify cases + 4 parse cases from Phase B.1 = full spec §8.1 unit
coverage.

Spec: docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md §4.6, §4.7
```

---

## Phase C — Dispatch integration

### Task 4: Wire UCAN verifier into `Hello` arm

**Files:**
- Modify: `crates/atd-runtime/src/dispatch.rs` (Hello arm, lines 129-142 per spec §10 ref 5)
- Modify: `crates/atd-runtime/src/config.rs` (or wherever `SharedServerConfig` lives — add `max_ucan_chain_depth: u8` + optional `revocation_store: Option<Arc<dyn UcanRevocationStore>>`)

- [ ] **Step 1: TDD — failing integration test**

Create `crates/atd-runtime/tests/dispatch_ucan_grants_union.rs`:

```rust
#[tokio::test]
async fn hello_with_ucan_grants_union_of_strings_and_ucan_caps() {
    let server = test_server::with_grant_capability("records:read").build();
    let chain = test_helpers::build_chain(/* root grants summary:read to client X */);
    let hello = Request::Hello {
        client_id: "X".into(),
        requested_capabilities: vec!["records:read".into()],
        ucan_tokens: vec![chain.into_jwt()],
    };
    let resp = server.dispatch(hello).await.unwrap();
    assert!(matches!(resp, Response::HelloAck { caps, .. }
        if caps.contains("records:read") && caps.contains("summary:read")));
}
```

- [ ] **Step 2: GREEN — extend Hello arm**

In `crates/atd-runtime/src/dispatch.rs::dispatch_request` Hello arm:

```rust
let granted_strings = intersect(requested_capabilities, &cfg.allow_list);

let granted_ucan = if !ucan_tokens.is_empty() {
    ucan::verify_tokens(
        &ucan_tokens,
        &VerifyConfig {
            max_chain_depth: cfg.max_ucan_chain_depth,
            expected_audience: client_id.clone(),
            revocation_store: cfg.ucan_revocation_store.clone(),
        },
        SystemTime::now(),
    ).map_err(|e| Response::Error {
        code: ucan_err_to_code(&e),
        message: e.to_string(),
        retryable: Some(false),
    })?
} else {
    CapabilitySet::empty()
};

let caps = granted_strings.union(&granted_ucan);
```

`ucan::verify_tokens` is a thin wrapper that takes each token in `ucan_tokens` as a separate root chain (spec §5.3 multi-root semantics), verifies each independently, and unions the resulting `CapabilitySet`s.

- [ ] **Step 3: Run + check no SP-12 regression**

```bash
cargo test -p atd-runtime              # all dispatch tests
cargo test -p atd-conformance          # cross-conformance: ERR_CAPABILITY_DENIED path unchanged
cargo test --workspace                 # everything else
```

- [ ] **Step 4: Commit**

```
feat(atd-runtime): SP-capability-v2 Phase C — dispatch Hello arm consumes UCAN

When Hello.ucan_tokens is non-empty, verifier produces a CapabilitySet
that union-merges with the SP-12 string-allow-list result. Empty ucan_tokens
path is byte-identical to pre-SP behaviour.

Spec: docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md §4.2 + §7.1 Phase C
```

---

## Phase D — `TokenBroker::resolve_bearer` UCAN branch

### Task 5: `InMemoryTokenBroker` accepts ucan-jwt

**Files:**
- Modify: `crates/atd-runtime/src/secrets.rs` (reference broker — `InMemoryTokenBroker::resolve_bearer`)
- Modify: `crates/atd-runtime/src/secrets.rs` (`accepted_token_formats` default impl + `InMemoryTokenBroker` override)

- [ ] **Step 1: TDD — broker test**

Add `crates/atd-runtime/tests/broker_ucan_jwt.rs`:

```rust
#[test]
fn resolve_bearer_ucan_jwt_returns_identity_from_aud_and_attenuated_caps() {
    let broker = InMemoryTokenBroker::new(/* with did_to_caller_id map */);
    let jwt = test_helpers::build_jwt_chain(/* U→A, args.caps=[records:read] */);
    let id = broker.resolve_bearer(&jwt).unwrap().unwrap();
    assert_eq!(id.caller_id, "agent:A");
    assert_eq!(id.granted_capabilities, ["records:read"].into_iter().collect());
}
```

- [ ] **Step 2: GREEN — implement branch**

```rust
impl TokenBroker for InMemoryTokenBroker {
    fn accepted_token_formats(&self) -> &'static [&'static str] {
        &["opaque", "ucan-jwt"]
    }

    async fn resolve_bearer(&self, raw: &str) -> Result<Option<BearerIdentity>, BrokerError> {
        if looks_like_jwt(raw) {
            let chain = ucan::parse_and_verify_single(raw, &self.ucan_cfg, SystemTime::now())
                .map_err(BrokerError::from)?;
            let caller_id = self.did_to_caller_id(&chain.leaf_aud)
                .ok_or(BrokerError::UnknownIdentity)?;
            Ok(Some(BearerIdentity {
                caller_id,
                granted_capabilities: chain.effective_caps,
                expires_at: chain.min_exp,
                secrets: Default::default(),
            }))
        } else {
            self.resolve_opaque(raw).await
        }
    }
}
```

- [ ] **Step 3: Test + commit**

```bash
cargo test -p atd-runtime
```

```
feat(atd-runtime): SP-capability-v2 Phase D — InMemoryTokenBroker UCAN-JWT branch

resolve_bearer now dispatches by JWT-shape heuristic: 3-segment dot-form
goes through ucan::parse_and_verify_single; everything else falls back
to the opaque path (unchanged). accepted_token_formats reports both.

Spec: docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md §4.5 + §7.1 Phase D
```

---

## Phase E — Revocation store

### Task 6: `InMemoryUcanRevocationStore`

**Files:**
- Create: `crates/atd-runtime/src/ucan/revocation.rs`
- Modify: `crates/atd-runtime/src/ucan/mod.rs` (re-export)
- Modify: `crates/atd-runtime/src/config.rs` (`SharedServerConfig.ucan_revocation_store: Option<Arc<dyn UcanRevocationStore>>`)

- [ ] **Step 1: Define trait + in-memory impl**

```rust
pub trait UcanRevocationStore: Send + Sync {
    fn is_revoked(&self, ucan_cid: &str) -> bool;
    fn revoke(&self, ucan_cid: String);
}

pub struct InMemoryUcanRevocationStore {
    revoked: Arc<RwLock<HashSet<String>>>,
}
```

- [ ] **Step 2: TDD — revocation propagation test**

```rust
#[test]
fn revoking_root_cid_blocks_subsequent_verification_of_descendant_chain() {
    let store: Arc<dyn UcanRevocationStore> = Arc::new(InMemoryUcanRevocationStore::new());
    let chain = test_helpers::build_chain_with_known_cids(...);
    assert!(verify_chain(&chain, &cfg(store.clone())).is_ok());
    store.revoke(chain.root_cid.clone());
    assert!(matches!(verify_chain(&chain, &cfg(store)),
        Err(VerifyError::Revoked(_))));
}
```

- [ ] **Step 3: GREEN + commit**

```
feat(atd-runtime): SP-capability-v2 Phase E — UcanRevocationStore trait + in-memory impl

InMemoryUcanRevocationStore holds a HashSet<String> of revoked UCAN CIDs.
verify_chain consults the store on every link. Adopter brokers wire this
into their own revocation-management UI (e.g., celia's "revoke agent A"
Tauri command, spec §6).

Spec: docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md §4.7
```

---

## Phase F — Integration tests across transports

### Task 7: UDS + HTTP integration suite (spec §8.2)

**Files:**
- Create: `crates/atd-server/tests/ucan_hello_grants_union.rs`
- Create: `crates/atd-server-http/tests/ucan_audience_mismatch_via_http.rs`
- Create: `crates/atd-server-http/tests/ucan_chain_3_links_e2e.rs`

- [ ] **Step 1: Write each test per spec §8.2 exact wording**

- [ ] **Step 2: Run + verify all green**

```bash
cargo test -p atd-server --test ucan_hello_grants_union
cargo test -p atd-server-http
```

- [ ] **Step 3: Commit**

```
test(atd-server,atd-server-http): SP-capability-v2 Phase F — integration suite

3 integration tests per spec §8.2: UDS union, HTTP audience-mismatch (401),
3-link chain E2E (tools/list reflects attenuated caps).

Spec: docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md §8.2
```

---

## Phase G — Doc + conformance sync

### Task 8: architecture.md + conformance fixtures + tag

**Files:**
- Modify: `docs/architecture.md` (flip 🔨 → ✅ for the SP-capability-v2 row in §10; same for §5.3 / §9.3 cross-refs)
- Modify: `crates/atd-conformance/fixtures/` (add UCAN-aware fixtures if scoped — else file follow-up SP)
- Tag: `sp-capability-v2`

- [ ] **Step 1: Update architecture.md status glyphs**

Search-and-replace `🔨 (SP-capability-v2)` → `✅ (SP-capability-v2)` and update the §10 row's Window from "Q2 2026" to actual landing date.

- [ ] **Step 2: Update ADR-0001**

Mark 1.A row status "in flight" → "shipped (tag `sp-capability-v2`)". Bump amendment date.

- [ ] **Step 3: Final workspace check**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
```

All four must pass.

- [ ] **Step 4: Tag + push**

```bash
git tag sp-capability-v2
git push origin master --tags
```

- [ ] **Step 5: Commit**

```
docs(architecture,adr): SP-capability-v2 landed — 🔨 → ✅

Tag: sp-capability-v2. End-to-end: UDS + HTTP both accept Hello.ucan_tokens;
broker dispatches by JWT-shape; revocation store wired; 17 new tests green.

Adopter follow-up: celia_phr issue 2026-05-11-sp-capability-v2-adopter.md
(consent schema migration + Tauri delegation Tauri command + Playwright
sub-agent demo).
```

---

## Cross-project (celia + healthkit) — tracked as adopter issues, not in this plan

Per ADR-0001 §2.3 + §2.5: SP-capability-v2 implementation lands in atd-mvp only. Adopter work (celia consent migration, broker UCAN branch, Hermes orchestrator wiring; healthkit no-regression confirmation) is filed as separate issues against each downstream repo at SP-completion time. See:

- `~/code/pha/celia_phr/docs/issues/2026-05-11-sp-capability-v2-adopter.md` (filed by atd-mvp maintainers when this plan completes)
- `~/proj/healthkit_cli/docs/issues/2026-05-11-sp-capability-v2-no-regression.md` (filed by atd-mvp maintainers when this plan completes)

Both downstream repos report results back via PR comments referencing this SP tag.

---

## Risk register

| Risk | Mitigation |
|---|---|
| `ed25519-dalek` v2 has a breaking API vs v1 used elsewhere in workspace | Pin exactly; if conflict, add a `[patch.crates-io]` block or wrap behind a small `crypto` module |
| Chain verification CPU cost — pathological 5-deep chain on every Hello | Spec §4.6 caps depth at 5 by default; depth-1 (no delegation) costs one Ed25519 verify ≈ 50µs; acceptable per call |
| Wire-format drift: pre-SP server receiving Hello with `ucan_tokens` populated should ignore, not error | `#[serde(default, skip_serializing_if = "Vec::is_empty")]` on the new field handles this; verify with a back-compat conformance fixture (Task 8 follow-up) |
| celia's consent table grows large with delegated rows | Spec §6 recursive CTE benchmark required during celia adopter work; not blocking this plan |
| Hermes orchestrator scenario not yet wired in any existing test harness | Celia adopter work (cross-repo) implements `sub_agent_delegation.spec.ts` Playwright test (spec §8.3); atd-mvp ships `ucan_chain_3_links_e2e.rs` as the synthetic equivalent |

---

## Out of scope (this plan; future SPs)

Per spec §9 — `did:web`, multi-algorithm (RS256 / ES256K), DAG-CBOR native UCAN, token introspection, cross-server federation, on-chain revocation, ZK attenuation, audit-log chain-visualisation, pairing-code → UCAN auto-issue helper. Each filed as its own follow-up SP when an adopter need surfaces.
