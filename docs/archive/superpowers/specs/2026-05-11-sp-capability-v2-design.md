# SP-capability-v2: UCAN-style capability tokens for ATD

| Status | Draft |
| Created | 2026-05-11 |
| Author | cross-project subagent (celia_phr ↔ atd-mvp coordination) |
| Phase | ATD post-v0.3.0; future capability layer |
| Related | SP-12 canonical dispatch (`2026-04-25-sp12-canonical-dispatch.md`); SP-streamable-http (`2026-05-11-sp-streamable-http-design.md`, commit `758ce40` / impl `0448aad`); SP-token-broker-phase2 (`2026-05-11-sp-token-broker-phase2-design.md`, commit `db3287c`); Celia `ATD_FUTURE_ISSUES.md §1.A` (this SP's motivating ticket) + `§3.A` (agent-identity); Celia patent §13.4 (multi-agent isolation) |

---

## 1. Motivation

**1.1 The string allow-list is the only thing ATD knows about authority.** SP-12 (`crates/atd-runtime/src/capability.rs:16-50`) shipped `CapabilitySet` as a `BTreeSet<String>` with an `intersect(requested, granted)` operation. Authority is declared once at server start (`--grant-capability <s>`), pinned per-connection during `Hello` (`crates/atd-runtime/src/dispatch.rs:129-142`), and never re-evaluated. There is **no token format, no signature, no expiry, no audience, no delegation chain, no revocation**. Capability.rs:6-8 even names this gap directly: *"a future SP can swap this allow-list for UCAN verification without changing `CapabilitySet`'s public surface."* This SP is that future SP.

**1.2 The first concrete demand is sub-agent delegation.** A statement Celia's roadmap names but cannot express today (Celia `ATD_FUTURE_ISSUES.md:30-32`): *"Agent A may delegate read-only access to Patient X to its sub-agent B."* Celia's RBAC is a flat exact-string match (`consent.grantee = 'agent:<name>:<token_short>'`, matched at `crates/celia-core/src/auth/rbac.rs:319-329`). Two agents collaborating on one PHR session must both appear in the `consent` table independently — the user must re-pair B from scratch even though A already holds a superset authority and wants to lend a subset. That's the wrong privacy posture: the user trusted A, not the ATD operator's pair-everyone-individually administrative path. Hermes's "orchestrator + N specialised children" workflow is held back by exactly this.

**1.3 ATD's patent claim §13.4 already imagines this layer.** `docs/patents/main.zh.md:369` reads: *"capability gate 以白名单或基于密码学令牌的形式约束每个连接可调用的工具子集"* — explicitly allowing for a future cryptographic-token form of the gate, and grounding it in the same FHIR `Consent.grantee` semantic as Celia's RBAC. The patent's "multi-agent isolation" property (line 373) — *one binding's grantee leakage MUST NOT cross to another binding's audit* — is naturally enforced by UCAN's `aud` field: a token addressed to agent B simply does not validate when presented by agent C. SP-capability-v2 promotes the patent's "或" (or) branch from latent to load-bearing, and gives the FHIR `Consent.grantee` link a chain-shaped extension.

## 2. Goals

- Define a **wire-additive** ATD `Hello` extension: an optional `ucan_tokens: Vec<String>` alongside the existing `requested_capabilities: Vec<String>` (`crates/atd-protocol/src/messages.rs:32-39`) — phase-1 clients and pre-SP-12 servers keep working unchanged.
- Specify a **single canonical UCAN profile** ATD accepts (algorithms, DID method, expiry semantics, attenuation rules), so cross-vendor interop is decidable without reading a 100-page spec.
- Specify the **mapping from ATD's string capabilities** (`records:read`, `fs.write`) to UCAN's `(cmd, args/policy)` capability shape, so existing tools and existing operator allow-lists don't need to be rewritten.
- Specify the **`atd-runtime` verification trait** — what the broker / verifier sees, what the listener sees, where signature checks happen, where time checks happen, where audience checks happen.
- Specify how `TokenBroker::resolve_bearer` (SP-token-broker-phase2 §5) ingests a UCAN: which token in the chain becomes `BearerIdentity.caller_id`, how `granted_capabilities` are derived from the chain's attenuated capability set, how `expires_at` is computed.
- Sketch a **Celia consent-schema migration** that lets the SQLite `consent` table represent a delegation chain (not just a flat grantee), with a worked example: A→B partial delegation in current vs. post-migration shape.
- Specify a **revocation model** that composes with SP-token-broker-phase2 §4.8's three-layer revocation (TTL, push-hook, future revocation list), without forcing on-chain anchoring.
- Identify a **migration path** that keeps every step of the §13.1 device-local volatile-key invariant verifiable (`pnpm --filter @celia/desktop test:dek` green on each step).
- Define a **conformance test plan**: 3-link delegation chain, signature verification, expiry, attenuation, audience mismatch — each its own failure-mode test.
- Identify the **future-SP carve-outs** so this SP can't accidentally pull in a DID resolver, a federation registry, or a ZK-snark proof.

## 3. Non-goals

- **Full UCAN v1.0 normative compliance.** UCAN v1.0 is a moving target with IPLD CIDv1 content addressing, DAG-CBOR canonical encoding, and a `did:key` / `did:web` / `did:plc` ecosystem. We pick a **profile** that is forward-compatible (a future SP can lift restrictions), not the entire spec surface.
- **`did:web` / `did:plc` / `did:agent` resolvers.** Only `did:key` is in scope. `did:key` is self-resolving (public key === DID); zero network IO at verification time. The forward-looking `did:agent:<vendor>:<instance>` Celia `ATD_FUTURE_ISSUES.md §3.A` proposes is a different SP.
- **Multi-algorithm crypto.** Ed25519 only. UCAN v1.0 allows P-256 and secp256k1; both can be added in a follow-up. JWT-style RS256 / ES256K is rejected because the JOSE algorithm-selection surface is too wide for a v1 verifier (CVE-2022-21449 class issues).
- **Offline token issuance service.** ATD does not mint UCANs. Issuance is an adopter concern (Celia Tauri wizard signs A→B delegations on user click).
- **Revocation list endpoints, blockchain anchoring, federation, introspection.** Out of scope; tracked in §9 as future SPs.
- **JWT compatibility shim.** UCAN v1.0 explicitly rejects JWT (DAG-CBOR + raw signature instead). We follow the spec, not the v0.x JWT-compact-form quirk.
- **Replacing SP-12 string capabilities.** UCAN is **additive**. A server in `--grant-capability records:read` mode keeps working; the UCAN path is opt-in per-connection.
- **ZK-proof attenuation, partial-trust quorum signatures, threshold delegation.** Single-signer chains only.

## 4. Design

This is ~50% of the SP. Each subsection answers one of the 8 decision points from the brief, with the chosen answer, evidence, and rejected alternatives.

### 4.1 UCAN profile — UCAN-lite (v1.0 subset, JWT-shaped wire) is chosen; full v1.0 IPLD/DAG-CBOR rejected for v1; ad-hoc-format rejected outright

**Decision.** ATD accepts a **UCAN v1.0 *profile*** with the following narrowing:
- Wire form: JWT-shaped compact (`<header>.<payload>.<signature>`, base64url URL-safe, three dots) — NOT DAG-CBOR + CID; the JOSE token shape ports cleanly to HTTP `Authorization` headers and SQLite `TEXT` columns, and every server-side library can decode it.
- Payload uses UCAN v1.0 mandatory fields verbatim (`iss`, `aud`, `sub`, `cmd`, `args`, `nonce`, `exp`) but encoded as JSON (`alg=EdDSA`, `typ=ucan/1.0+jwt`), not CBOR. This is a **deliberate divergence** from UCAN v1.0's "no JWT" stance, justified below.
- Capability format: ATD-native string capabilities (`records:read`) tunneled into UCAN's `cmd` field, with `args = {}` — see §4.5. Not the `/crud/read` UCAN hierarchical commands.
- Signature: Ed25519 only.
- DID method: `did:key` only.

**Why a JWT-shape profile rather than verbatim UCAN v1.0.** UCAN v1.0 mandates DAG-CBOR canonical encoding to enable CIDv1 content addressing — useful for offline storage and IPLD-native systems, but **gratuitous** for ATD's use case where (a) the token rides one `Authorization: Bearer` header per request, (b) the adopter stores tokens in a plain SQLite column, (c) content addressing is meaningless until federation exists. The JWT compact form costs ~30% less wire space (no base32-multibase CID prefix), is parseable by 50+ libraries across 12 languages, and the `<header>.<payload>.<sig>` form already passes through HTTP infrastructure (proxies, logs, length limits) without surprises. We keep UCAN's *semantics* (audience match, attenuation, chain verification) but adopt JOSE's *encoding*. A future SP can add a DAG-CBOR alternate wire if a federation use case demands content addressing.

**Why not ad-hoc.** Inventing a Celia-specific `{ delegator, delegate, scopes, exp, sig }` format would lock ATD out of the UCAN ecosystem (Bluesky, Fission, Storacha, ProtoSchool — all native UCAN). Adopters could not reuse off-the-shelf UCAN signing libraries on the client side. Reject.

**Why not OAuth 2.1 access tokens.** OAuth tokens are *bearers*, not *delegations* — they do not carry the issuer + audience + attenuation chain that the §1.2 sub-agent scenario requires. SP-token-broker-phase2 §1.2 already keeps OAuth introspection deferrable to a per-adopter broker layer; we don't need to revisit.

**Trade-off table.**

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Verbatim UCAN v1.0 (DAG-CBOR + CIDv1) | Spec-faithful; future federation cheap | Heavy encoding; minimal ecosystem; novel for HTTP-header transport | rejected (overkill for v1) |
| UCAN-lite (JWT-shaped, UCAN semantics) | Pragmatic encoding; reuses JOSE infra; spec-aligned semantics | Diverges from UCAN's "no JWT" stance | **chosen** |
| Macaroon / Biscuit | Caveat model; mature attenuation | Different conceptual model; no Bluesky/Fission alignment | rejected |
| Ad-hoc format | Smallest verifier | Eco lock-in; cannot share signing tools | rejected |

### 4.2 Wire field addition — additive Hello field, never replaces the string list

**Decision.** Extend `Request::Hello` (`crates/atd-protocol/src/messages.rs:34-39`) with one new optional field. The full post-SP shape:

```rust
Hello {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    /// SP-12 string allow-list (unchanged semantics).
    #[serde(default)]
    requested_capabilities: Vec<String>,
    /// SP-capability-v2 UCAN tokens, in canonical JWT compact form. If
    /// present and non-empty, the server attempts UCAN verification
    /// (§4.6); on success, the derived capability set is **unioned**
    /// with the string allow-list intersection. If verification fails
    /// for ANY token in the list, the entire Hello is rejected with
    /// `ERR_UCAN_INVALID` (§5.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ucan_tokens: Vec<String>,
}
```

**Union vs replacement.** When `ucan_tokens` is non-empty, the granted set is `granted_strings ∪ granted_ucan` where `granted_strings = server_allow_list ∩ requested_capabilities` (unchanged SP-12 path) and `granted_ucan = derive_caps_from_ucan_chain(ucan_tokens)` (§4.5). **Reasoning:** clients that hold a UCAN proving "I can also do `records:export`" and *separately* request `records:read` from the server-operator allow-list should get the union of both, not the intersection. The intersection would punish a client for asking for *less* than its UCAN proves. The union preserves UCAN's attenuation semantic — the UCAN payload already constrains what the client can claim; intersecting again would be double-counting.

**Pre-SP servers + post-SP clients.** `serde(default)` makes `ucan_tokens` invisible to a v0.3.0 server. A post-SP client that sends `ucan_tokens` to a pre-SP server gets the SP-12 string intersection only — no error, no surprise. A post-SP server that receives only `requested_capabilities` runs the existing SP-12 path verbatim. Wire-additive in both directions.

**Why not replace `requested_capabilities`.** Tempting (single source of truth), but rejected because (a) the SP-12 string path is the operator's local policy and may include capabilities a client should be able to request *without* holding a UCAN (e.g., `ping` or trivially-public tools), and (b) deprecating the string list now would force every existing adopter into the UCAN code path, blowing past the "additive" goal.

**HTTP transport (SP-streamable-http).** No new headers — UCANs ride the existing `Authorization: Bearer <token>` header. SP-token-broker-phase2 §4.1 left the token format opaque; that decision composes: a broker advertises `accepted_token_formats() = &["ucan-jwt"]` (§5) and parses the bearer as a UCAN JWT in `resolve_bearer`. HTTP-side ATD already calls the broker once per request; no extra round-trip. The `BearerIdentity.granted_capabilities` returned is the §4.5 derived set.

### 4.3 Signature algorithm — Ed25519 only

**Decision.** ATD's UCAN verifier accepts JWT `alg=EdDSA` with `crv=Ed25519` exclusively. JOSE algorithm negotiation is removed — no `RS256`, no `ES256K`, no `none`, no `HS256`. The verifier rejects any other `alg` value with `ERR_UCAN_INVALID` before signature check.

**Why one algorithm.** UCAN v1.0 §"Cryptographic Algorithms" recommends Ed25519 as preferred; matches `did:key`'s default; tiny, fast, no padding pitfalls. The JOSE multi-algorithm surface is the source of well-documented vulnerabilities (alg=none, algorithm confusion between RS256 and HS256). For a v1 implementation operating on health data, the right number of supported algorithms is one. Adopters who need RS256 / P-256 wait for SP-capability-v2.1.

**Rust crate choice.** `ed25519-dalek 2.x` for signature verification (already battle-tested, audited, dependency-light). For JWT parse + verify orchestration: a thin in-tree decoder, **not** the full `jsonwebtoken` crate — it pulls `ring` and a large algorithm matrix we want to avoid. Decoder is ~80 LoC: base64url-decode three parts, parse header, gate `alg/typ`, hash payload, verify signature. Same scope as `atd-runtime::secrets` redaction (`crates/atd-runtime/src/secrets.rs:29-39`).

**Why not the full JOSE crate.** Pulls in serde + base64 + ring + 6 RSA/ECDSA dialects + JWK structures. ~200 KB compile-time cost. We need 80 LoC and ed25519-dalek's ~50 KB.

### 4.4 DID method — `did:key` only; `did:web` deferred; `did:agent` left to SP-3.A

**Decision.** Issuer / audience / subject DIDs are exclusively `did:key:z<multibase-encoded-Ed25519-pubkey>`. The verifier:
1. Recognises the `did:key:z` prefix.
2. Decodes the multibase identifier to a 32-byte Ed25519 public key.
3. Uses the public key for `ed25519-dalek::Verifier::verify` directly.

No HTTPS resolution. No `.well-known` lookup. No cache. No TLS dependency on the verifier path. A single pure function: `did_key -> Result<VerifyingKey, ParseError>`.

**Why `did:key` only.** It is the only DID method that is *self-resolving* — the public key is *inside* the DID string. Zero network IO, zero attack surface beyond Ed25519 multibase parsing. UCAN v1.0 spec normatively requires `did:key` support; everything else is optional. For Celia's §13.1 device-local model, a `did:web` resolver would also need a network policy decision per call (which proxy? cache TTL? what if it times out mid-call?) — all out of scope. Even cross-vendor adopters benefit: `did:key` works on a closed network, on an offline device, on a Tauri app with no internet.

**Why not `did:web`.** Two problems: (a) introduces an HTTP dependency on the verifier critical path — every UCAN verify becomes a potential blocking IO; (b) puts a TLS root-trust decision into ATD itself (which CAs? operator-configurable? cached how?). Both are large surfaces; both are useful eventually; neither is required for the §1.2 sub-agent use case. Deferred.

**Why not `did:agent`.** Doesn't exist as a normative spec yet — Celia `ATD_FUTURE_ISSUES.md §3.A` calls for an *Agent Identity Working Group* to define it. We carve out a `Vec<String> reserved_did_methods` config field for forward-compat, defaulting to empty: when SP-3.A lands a spec, ATD adds the method handler additively (same pattern as SP-12's "swap allow-list for UCAN" prediction). The architecture is open to extension; the v1 verifier is not.

**Failure mode.** A token signed by a non-`did:key` issuer fails at parse-time with `ERR_UCAN_INVALID, reason: "unsupported_did_method"`. Operators see this clearly in audit logs; clients get a deterministic 401.

### 4.5 Capability syntax — string-as-cmd convention; `cmd = "atd-cap"`, `args = { caps: [...] }`

**Decision.** Each UCAN payload's capability list (the `att` field in classic UCAN, the `cmd`/`args` pair in v1.0) carries one or more *atomic ATD capabilities* as strings. The chosen encoding:

```json
{
  "iss": "did:key:z<A's pubkey>",
  "aud": "did:key:z<B's pubkey>",
  "sub": "did:key:z<A's pubkey>",
  "cmd": "atd-cap",
  "args": {
    "caps": ["records:read"],
    "with": [
      { "patient": "Patient/abc123" }
    ]
  },
  "nonce": "<random 16 bytes, base64url>",
  "exp": 1736208000
}
```

**Reasoning for the encoding shape.**
- **`cmd = "atd-cap"`** is a fixed sentinel under the `/atd/cap@1.0` UCAN subnamespace (UCAN v1.0 §"Reserved namespaces" — `/ucan/*` is reserved; everything else is fair game). A non-`atd-cap` `cmd` is rejected at parse time so a UCAN intended for a different system (Bluesky, Storacha) cannot accidentally grant ATD authority. Cross-system replay is structurally prevented.
- **`args.caps: Vec<String>`** carries the SP-12 string capabilities verbatim — `records:read`, `fs.write`, `summary:read`, etc. This **maps 1-to-1** to Celia's existing `consent.scope` CSV (`crates/celia-core/src/auth/rbac.rs:285-292`); zero string transformation between UCAN and Celia RBAC. The verifier extracts `args.caps` and union-merges across the chain (subject to attenuation, §4.6).
- **`args.with`** is an optional resource-binding list. The single supported binding for v1 is `{ "patient": "<FHIR reference>" }`. The dispatcher matches this against Celia's `consent.patient_filter` column (`crates/celia-core/src/auth/rbac.rs:342-...`) — same semantic, just travelling over UCAN instead of a SQLite row. Other binding kinds (`{ "tool": "..." }`, `{ "dataset": "..." }`) are reserved for future SPs.

**Why not UCAN's `/crud/read` hierarchical command form.** Tempting (matches the spec's idiomatic style: `/atd/records/read`), but rejected because (a) ATD's existing 11 capability strings are flat — converting them all is a breaking change of the SP-12 string-list contract; (b) hierarchical commands force a tree-walking verifier that is more code than a flat-string contains-check; (c) the conversion of `records:read` → `/atd/records/read` is a synonym pair the adopter community would have to learn for no functional gain. The flat-string-in-args encoding keeps SP-12's contract intact and side-steps a renaming campaign across every ATD adopter.

**Why a list (`caps: Vec<String>`) inside `args` instead of one cap per UCAN.** Cuts wire size and signature cost when an issuer delegates 5 caps at once (a common case — Celia's `Observer` preset is 4 caps). Attenuation still works: a child UCAN can strip caps but cannot add them (§4.6).

**Why no `with` for non-patient bindings now.** YAGNI. Celia's patient-filter is the one resource binding that exists today and matches a real RBAC column. Forcing every future adopter to learn a tool-binding shape they don't use is unnecessary. The schema is open: `args.with: Vec<HashMap<String, String>>` admits new binding kinds additively.

### 4.6 Delegation depth — unbounded chain; configurable max via `ServerConfig.max_ucan_chain_depth: u8` (default 5)

**Decision.** UCAN's normative attenuation rules apply unchanged: each delegation must restate or narrow the parent's authority; chain validation walks proof CIDs (referenced via a `prf: Vec<String>` payload field — the in-line proofs encoded as nested JWT compact strings, since we're not using IPLD). ATD adds **one** policy lever: a server-side maximum chain depth, configurable via `SharedServerConfig` (`crates/atd-runtime/src/dispatch.rs:100-105`) defaulting to `5`.

**Why a depth cap at all.** Two reasons. (a) Defensive bound: a malicious client could submit a 10⁶-link chain to exhaust verifier CPU. The v1 verifier is `O(chain_depth × signature_verify_cost)`; one Ed25519 verify is ~40µs, so a 10⁶ chain is 40 seconds — DoS surface. (b) Audit clarity: a 5-deep chain is human-traceable (`A→B→C→D→E`); a 50-deep chain has no operator-comprehensible authority story. The 5 default matches Macaroon community wisdom (Google production caps Macaroon attenuation around 5-7 caveats).

**Why configurable.** Adopters with a known need for deeper chains (federation experiments) can raise the cap. Operators with a strict policy (Celia's "user trusts A; A trusts B; that's two links, full stop") can lower it to 2.

**Why unbounded as a structural property.** A capability *system* with a hard-coded chain limit is brittle. The protocol allows arbitrary attenuation; the operator policy chooses the limit. SP-12 already follows this principle: capability-set size is unbounded by protocol but operator-bounded by `--grant-capability` count.

**Failure mode.** Exceeding the depth returns `ERR_DELEGATION_TOO_DEEP` (§5.4) at Hello time — the entire Hello fails closed, no partial grant.

### 4.7 Revocation — TTL is the canonical bound; broker-internal revoke-list composes with SP-token-broker-phase2 §4.8; no on-chain anchoring

**Decision.** Revocation has two tiers:

**Tier 1 — TTL (mandatory).** Every UCAN's `exp` field is enforced. The verifier rejects with `ERR_UCAN_EXPIRED` (§5.4) if `exp <= now()` for *any* link in the chain (UCAN spec: invocation time must lie within all links' validity intervals). Default issuer-side TTL guidance: 24 hours for human-issued delegations, ≤ 60s for service-to-service tokens. The TTL is the **worst-case revocation window**.

**Tier 2 — Broker-side revocation list (optional, broker-internal).** The verifier accepts an injected `Arc<dyn UcanRevocationStore>` (default = empty store; nothing revoked). The store exposes one method:

```rust
trait UcanRevocationStore: Send + Sync {
    /// Returns true iff this UCAN's CID has been revoked. The CID is
    /// computed as sha256 of the canonical JWT signing input (header.
    /// payload), base64url-encoded.
    fn is_revoked(&self, cid: &str) -> bool;
}
```

The verifier consults this store for each link in the chain. **The check is local to the broker / verifier process** — no HTTP fetch, no cross-server query. Composes with SP-token-broker-phase2 §4.8 layer-2 (broker-internal push hook): when Celia's Tauri UI revokes a consent, it pushes the corresponding UCAN's CID into the revocation store; the next Hello fails closed.

**Why no on-chain anchoring.** Adds blockchain trust assumptions, a network dep, a settlement-latency story, and a key-management problem (who signs the on-chain revocation tx?). UCAN v1.0 doesn't require it; Celia's threat model doesn't require it; no v1 adopter is asking for it. Defer to a federation SP if ever.

**Why no adoption of `/ucan/revoke` reserved-namespace commands.** UCAN v1.0 §"Reserved Namespaces" reserves `/ucan/revoke` for an issuer-signed revocation token format. Adopting it is consistent with the spec, but it forces ATD to recognise a second token kind (`cmd = "/ucan/revoke"`) on the Hello path — extra parser surface for zero v1 use case. Adopters who need cross-server revocation publish revocation tokens via their own channel; the broker injects CIDs into the store. Deferred (additive future SP).

**Composition with SP-token-broker-phase2's `BrokerError::Revoked`.** When the broker discovers a UCAN revocation (any link in the chain), it returns `Err(BrokerError::Revoked(reason))` — same error variant SP-token-broker-phase2 §4.4 defined. The HTTP listener returns 401 with `WWW-Authenticate: Bearer, error="invalid_token", error_description="revoked"`. **No new error variant required**; the existing one composes.

**Latency budget.** Operator UI revoke → Celia broker `revocation_store.insert(cid)` → next Hello fails: ~ms total. SSE long-stream: SP-token-broker-phase2 §4.7 already specifies ≤60s heartbeat re-validation; the UCAN store check is reused on heartbeat. Total worst-case end-to-end ≤ 60s — same as SP-token-broker-phase2's target.

### 4.8 Celia consent-schema integration — new `consent.parent_consent_id` column + new `consent.ucan_jwt` column; chain materialised in SQL, not just in-memory

**Decision.** Add two columns to `consent` (additive ALTER, schema migration `0004_capability_v2_ucan_chain.sql`):

```sql
ALTER TABLE consent ADD COLUMN parent_consent_id text;   -- nullable; NULL = root grant
ALTER TABLE consent ADD COLUMN ucan_jwt text;            -- nullable; the signed UCAN JWT
ALTER TABLE consent ADD COLUMN ucan_cid text;            -- nullable; sha256 of header.payload, indexed
CREATE INDEX consent_parent_idx ON consent (parent_consent_id);
CREATE INDEX consent_ucan_cid_idx ON consent (ucan_cid);
```

The two columns are nullable so every existing row (current flat `grantee = 'agent:<name>:<token>'` shape) keeps validating without rewrite. New chained rows fill both: `parent_consent_id` points at the parent row (or NULL for a root user→agent grant); `ucan_jwt` stores the signed token; `ucan_cid` indexes for revocation lookup.

**Worked example. Today's flat shape.** User U authorises agent A with steward preset:
```
INSERT INTO consent (id='c_A', user_id='U', grantee='agent:A:abc123', scope='records:read records:write …', patient_filter='*', status='active', …);
```

**Post-migration: A delegates patient-X read to sub-agent B.** Two new rows; the second references the first; both carry the UCAN that proves the chain.

```
INSERT INTO consent (
  id='c_A',
  user_id='U',
  grantee='agent:A:abc123',
  scope='records:read records:write records:delete export:read',
  patient_filter='*',
  parent_consent_id=NULL,
  ucan_jwt=<U-signed UCAN A>,
  ucan_cid='cid_U_to_A',
  status='active', …);

INSERT INTO consent (
  id='c_B',
  user_id='U',
  grantee='agent:B:def456',
  scope='records:read',                       -- attenuated
  patient_filter='Patient/X',                 -- attenuated
  parent_consent_id='c_A',
  ucan_jwt=<A-signed UCAN delegating to B>,
  ucan_cid='cid_A_to_B',
  status='active', …);
```

When agent B presents `[<U-signed A>, <A-signed B>]` as `ucan_tokens` in Hello, the broker:
1. Parses both JWTs; verifies signatures (`did:key` chain: U→A→B).
2. Looks up `cid_A_to_B` in revocation store → not revoked.
3. Looks up the consent rows by `ucan_cid` → both rows return, both active.
4. Computes attenuated capability set: `{records:read} ∩ {records:read, records:write, records:delete, export:read} = {records:read}`.
5. Computes attenuated patient filter: `Patient/X ∩ * = Patient/X`.
6. Returns `BearerIdentity { caller_id: "agent:B:def456", granted_capabilities: ["records:read"], … }`.

Downstream RBAC (`rbac.rs:319-329`) does its existing exact-equality check on `caller_id = "agent:B:def456"` — unchanged. Patient filter check matches `Patient/X` — unchanged. **Zero logic change in `rbac.rs`**; only the broker's `resolve_bearer` body grows the UCAN parse/verify branch.

**Why store the chain in SQL, not just trust the in-memory verification.** Three reasons. (a) Audit trail — the chain is durable; revocation review can inspect "who delegated what to whom" via SQL. (b) Offline operation — Celia can validate a UCAN that the user paired hours ago against the same row, with no need to re-verify against an online issuer. (c) Revocation enforcement — when the user revokes consent `c_A`, the broker SQL-cascades `UPDATE consent SET status='revoked' WHERE id='c_A' OR parent_consent_id IN (SELECT id FROM ...)` — children of a revoked grant inherit revocation. This is the structural equivalent of UCAN's chain-walking revocation, anchored in the adopter's storage.

**`CeliaConsentTokenBroker` evolution.** The current `resolve_bearer` (`crates/celia-cli/src/atd_broker.rs:74-201`) gets a sibling branch:

```rust
if bearer.starts_with("ce_") {
    // existing flat-grantee SP-token-broker-phase2 path
    return self.resolve_pairing_code(bearer).await;
}
if bearer.matches('.').count() == 2 {
    // looks like JWT compact form; try UCAN
    return self.resolve_ucan(bearer).await;
}
Err(BrokerError::Lookup("unknown bearer format".into()))
```

Both paths produce the same `BearerIdentity` shape — `caller_id` is the deepest UCAN's `aud` (translated back to `agent:<name>:<token_short>` via a new `did_to_grantee` column on `consent`, populated at pair-time). RBAC sees the same string either way.

## 5. Wire format reference

### 5.1 Full UCAN example (canonical "A→B delegates read-only Patient/X access")

Header:
```json
{ "alg": "EdDSA", "typ": "ucan/1.0+jwt", "ucv": "1.0" }
```
Payload (UCAN signed by A → audience B):
```json
{
  "iss":  "did:key:z6MkA_abbreviated",
  "aud":  "did:key:z6MkB_abbreviated",
  "sub":  "did:key:z6MkU_abbreviated",
  "cmd":  "atd-cap",
  "args": {
    "caps": ["records:read"],
    "with": [{"patient": "Patient/X"}]
  },
  "nonce": "Mv3K…16-bytes…",
  "exp":   1736208000,
  "prf":   [ "<base64 JWT compact of U-signed-A UCAN>" ]
}
```
Signature: Ed25519 over base64url(header) `.` base64url(payload).

The `prf` field carries the parent UCAN inline (JWT compact). This keeps verification self-contained: one request, no out-of-band fetches.

### 5.2 Hello message before/after

**Before** (`messages.rs:34-39`):
```json
{ "type": "hello", "client_id": "agent-B", "requested_capabilities": ["records:read"] }
```

**After** (additive):
```json
{
  "type": "hello",
  "client_id": "agent-B",
  "requested_capabilities": ["records:read"],
  "ucan_tokens": ["<A-signed-B UCAN JWT compact form>"]
}
```

### 5.3 Multiple roots in a single Hello

A client holding two independent root chains (e.g., one for `records:read` from user U₁, one for `summary:read` from user U₂) presents both as separate elements of `ucan_tokens`. Each chain is verified independently; the granted set is the union of all valid chains. **No cross-chain inference**: a `records:read` from U₁ cannot satisfy a `records:read` request bound to U₂'s patient.

### 5.4 New error codes

Add to `crates/atd-protocol/src/messages.rs` (alongside `ERR_CAPABILITY_DENIED = 1001`, `ERR_RATE_LIMITED = 1002`, `ERR_BROKER_FAILED = 1003`):

```rust
/// The Hello included a malformed UCAN (parse error, wrong alg,
/// unsupported DID method, bad signature, missing required field).
pub const ERR_UCAN_INVALID: u16 = 1010;

/// A link in the UCAN chain has `exp <= now()`.
pub const ERR_UCAN_EXPIRED: u16 = 1011;

/// The UCAN chain exceeds `ServerConfig.max_ucan_chain_depth`.
pub const ERR_DELEGATION_TOO_DEEP: u16 = 1012;

/// The deepest UCAN's `aud` does not match the connection's
/// `client_id` (or the bearer's caller). Prevents intercepted-token
/// replay by a third party.
pub const ERR_AUDIENCE_MISMATCH: u16 = 1013;
```

All four map to `Response::Error { code, retryable: Some(false), … }` — UCAN errors are deterministic; retry without changing the token is pointless.

## 6. Celia consent schema upgrade — full migration draft

The schema delta is small (three columns + two indexes); the **policy delta** is non-trivial. Migration `0004_capability_v2_ucan_chain.sql`:

```sql
-- Phase K — SP-capability-v2 UCAN chain support.
-- All columns nullable; pre-existing flat-grantee rows untouched.
ALTER TABLE consent ADD COLUMN parent_consent_id text;
ALTER TABLE consent ADD COLUMN ucan_jwt          text;
ALTER TABLE consent ADD COLUMN ucan_cid          text;
ALTER TABLE consent ADD COLUMN did_to_grantee    text;
   -- maps did:key:z<...> → 'agent:<name>:<token_short>' for RBAC compat
CREATE INDEX IF NOT EXISTS consent_parent_idx   ON consent (parent_consent_id);
CREATE INDEX IF NOT EXISTS consent_ucan_cid_idx ON consent (ucan_cid);
```

**Read path.** `rbac.rs::get_allowed_tools_for_caller` is unchanged at the SQL level — it joins on `user_id + status + effective_until + grantee` exactly as today. The UCAN chain has *already* been collapsed to a single `grantee` string by the broker (`agent:<name>:<token_short>`), and the row carries the SQL-shaped scope CSV. **§13.1 invariant preserved**: no DEK touches this path; UCAN signature verification is pure CPU + PBKDF2-free.

**Write path (pair-time).** When user U authorises agent A in the Tauri wizard, the existing path inserts `consent` row with `grantee='agent:A:<token>'`. Post-SP, it also generates a UCAN signed by U → A, computes the CID, fills `ucan_jwt + ucan_cid`. The UCAN issuance signing key is a new ephemeral Ed25519 key derived from the user's DEK on login (per-session, never persisted) — patent §13.1's "user can sign on their device, key dies with the session" property naturally extends here.

**Sub-agent path (the new use case).** Agent A's surface (e.g. Hermes orchestrator) signs a delegation A → B with `args.caps = ["records:read"]`, `args.with = [{"patient":"Patient/X"}]`. Hermes calls Celia's new Tauri command `celia_consent_record_delegation(parent_cid, child_ucan_jwt)`, which:
1. Verifies the chain against the current `consent` table.
2. Inserts a child row with `parent_consent_id` pointing at the parent's `id`.
3. Returns the child consent row id to Hermes, which hands the UCAN to sub-agent B.

**Revocation path.** When user U clicks "revoke agent A" in the wizard, the existing path runs `UPDATE consent SET status='revoked' WHERE id=?`. Post-SP, an additional cascade is needed:
```sql
WITH RECURSIVE chain(id) AS (
  SELECT id FROM consent WHERE id = ?
  UNION ALL
  SELECT c.id FROM consent c, chain WHERE c.parent_consent_id = chain.id
)
UPDATE consent SET status='revoked' WHERE id IN (SELECT id FROM chain);
```
Plus a push to the in-memory revocation store keyed by `ucan_cid`. **Latency**: the SQL recursive CTE runs in <1ms for chains up to depth 5; the in-memory hash push is <1µs.

## 7. Migration path

### 7.1 ATD-side

| Phase | Code change | Wire change | §13.1 audit |
|---|---|---|---|
| A | Add `ucan_tokens: Vec<String>` to `Request::Hello` (additive `serde(default)`); no parser yet | New optional field; non-empty triggers `ERR_UCAN_INVALID` ("verifier not configured") | Unchanged — DEK never touches Hello |
| B | Land `atd-runtime::ucan::{Verifier, ParseError, Chain}` module + new error codes 1010-1013 | None | Unchanged |
| C | Wire `Verifier` into `dispatch_request::Hello` arm: union `verified_chain.caps` with `granted_strings`; populate `caps` accordingly | UCAN-aware servers grant more on success | Unchanged |
| D | Extend `TokenBroker::resolve_bearer` reference impl with UCAN-JWT branch (alongside existing opaque) | HTTP-side bearer auth gains UCAN | Unchanged |
| E | Ship `InMemoryUcanRevocationStore`, optional `Arc<dyn UcanRevocationStore>` on `SharedServerConfig` | None | Unchanged |

Each phase ships independently; each leaves `cargo test --workspace` green; each is reversible (revert one PR).

### 7.2 Celia-side

| Phase | Code change | Test gate |
|---|---|---|
| 1 | Apply migration `0004_capability_v2_ucan_chain.sql`; no code uses the new columns yet | `cargo test -p celia-core` (159 tests) + `pnpm --filter @celia/desktop test:dek` |
| 2 | Wizard signs user→agent root UCAN on pair; stores `ucan_jwt + ucan_cid` alongside existing `grantee` | Existing pairing E2E (Playwright 18 tests) green; new row carries both shapes |
| 3 | `CeliaConsentTokenBroker::resolve_bearer` adds UCAN-JWT branch; flat `ce_<hex>` branch unchanged | 11 smoke tests pass + 4 new UCAN tests (happy / signature-bad / expired / chain-too-deep) |
| 4 | New Tauri command `celia_consent_record_delegation`; Hermes wires sub-agent creation through it | New 5-test integration suite for A→B delegation; `test:dek` green |
| 5 | Revocation cascade SQL added; revocation-store push from `consent_revoke` Tauri command | Revoke-A-revokes-B test; observation: B's next Hello returns `ERR_UCAN_INVALID`/Revoked within <100ms |

§13.1 audit at every step: `KeyCache::insert / KeyCache::evict` call-site count unchanged. The verifier is pure CPU; it does not allocate `Zeroizing<Vec<u8>>`, does not call `pbkdf2`, does not touch SQLite encrypted columns. The cross-project test `pnpm --filter @celia/desktop test:dek` (the volatile-key invariant guard) must stay green at every step — gating CI.

## 8. Test plan

### 8.1 Unit tests (atd-runtime)

- `ucan::tests::parse_well_formed_token_succeeds` — one canonical 3-part JWT round-trips.
- `ucan::tests::parse_unsupported_alg_rejects` — `alg=RS256` returns `ERR_UCAN_INVALID`.
- `ucan::tests::parse_non_did_key_issuer_rejects` — `iss=did:web:example.org` returns `ERR_UCAN_INVALID`.
- `ucan::tests::verify_signature_with_wrong_key_rejects` — tampered payload fails.
- `ucan::tests::expired_token_returns_err_expired` — `exp` in the past triggers `ERR_UCAN_EXPIRED`.
- `ucan::tests::chain_depth_exceeded_rejects` — 6-deep chain with default `max_ucan_chain_depth=5` → `ERR_DELEGATION_TOO_DEEP`.
- `ucan::tests::audience_mismatch_rejects` — chain's deepest `aud` ≠ connection `client_id` → `ERR_AUDIENCE_MISMATCH`.
- `ucan::tests::attenuation_intersect_succeeds` — root `[a, b, c]` → child `[a, b]` → grandchild `[a]` → effective `[a]`.
- `ucan::tests::attenuation_widening_rejects` — child claims `[a, b, c, d]` when parent grants `[a, b, c]` → `ERR_UCAN_INVALID`.
- `ucan::tests::revoked_cid_rejects` — store contains CID; verifier returns `Err(Revoked)`.

### 8.2 Integration tests (atd-server / atd-server-http)

- `tests/ucan_hello_grants_union.rs` — UDS server with `--grant-capability records:read`; client sends `ucan_tokens` proving `summary:read`; granted = `{records:read, summary:read}`.
- `tests/ucan_audience_mismatch_via_http.rs` — POST /mcp with a UCAN whose `aud` ≠ bearer subject; expect 401 `ERR_AUDIENCE_MISMATCH`.
- `tests/ucan_chain_3_links_e2e.rs` — synthesise U→A→B chain; B Hellos with both UCANs; verifier walks chain; `tools/list` reflects attenuated caps.

### 8.3 Cross-project (Celia)

- `crates/celia-cli/tests/broker_ucan_e2e.rs` — fresh SQLite + migrations 0000-0004 + `consent` row pre-populated with `ucan_jwt`; broker resolves; identity matches pair.
- `apps/desktop/playwright/specs/sub_agent_delegation.spec.ts` — Tauri pairs A; chat invokes Hermes-shaped "delegate to B"; B starts; B's tool call hits Celia and succeeds; revoke A → B fails ≤60s.
- `pnpm --filter @celia/desktop test:dek` — must stay green at every Celia migration phase.

## 9. Out of scope (future SPs)

| Feature | Why deferred | Tracker / suggested SP |
|---|---|---|
| `did:web` / `did:plc` / `did:agent` resolution | Network IO surface + TLS-root-trust policy; orthogonal to v1 use case | SP-capability-v2.2-did-web; SP-3.A (`ATD_FUTURE_ISSUES.md §3.A`) |
| RS256 / ES256K / multi-algorithm | JOSE algorithm-confusion CVE class; Ed25519 covers v1 needs | SP-capability-v2.1-multi-alg |
| DAG-CBOR native UCAN wire | Heavy; mostly useful for IPLD federation | SP-capability-v3-ipld |
| Token introspection endpoint | Federation primitive; one-broker case doesn't need it | SP-capability-v2-introspection |
| Cross-server federation / token portability | Two-broker case; needs JWKS or DID-method-web | SP-federation-v1 |
| On-chain revocation anchoring | Blockchain dep; latency / settlement complexity | Not on roadmap |
| `/ucan/revoke` reserved-namespace tokens | Cross-server revocation; not needed for single-broker | SP-capability-v2.3-ucan-revoke |
| ZK-proof attenuation / quorum delegation | Research-grade; no v1 use case | Not on roadmap |
| Per-tool capability bindings (`args.with = [{tool: "..."}]`) | Adopter pattern; no concrete demand | Additive when an adopter asks |
| UCAN-aware audit-log enrichment (chain visualisation in `CallEvent`) | Audit schema currently field-fixed; SP-medical-middleware §4.2 deferred a similar enrichment | SP-audit-v2 |
| Pairing-code → UCAN auto-issue helper | Tauri UI concern; not a protocol primitive | Celia-side feature |

## 10. References

### atd-mvp source (line-precise; spot-check targets)

1. `crates/atd-runtime/src/capability.rs:1-50` — `CapabilitySet` current shape; the surface this SP layers over without breaking.
2. `crates/atd-runtime/src/capability.rs:6-8` — own comment explicitly predicting "a future SP can swap this allow-list for UCAN verification" — this SP is that prediction.
3. `crates/atd-protocol/src/messages.rs:32-39` — `Hello.requested_capabilities`; §4.2 extends with `ucan_tokens` additively.
4. `crates/atd-protocol/src/messages.rs:6` (`ERR_CAPABILITY_DENIED = 1001`) — error-code shape; new codes 1010-1013 follow the same numeric pattern.
5. `crates/atd-runtime/src/dispatch.rs:129-142` — current `Hello` arm; §4.6 + §7.1 phase C extend it with chain verification.
6. `crates/atd-runtime/src/secrets.rs:60-77` — `BrokerError` enum including `Revoked(String)`; §4.7 reuses this without adding new variants.
7. `crates/atd-runtime/src/secrets.rs:96-130` — `BearerIdentity` shape; UCAN-aware broker fills `caller_id` from deepest `aud`, `granted_capabilities` from attenuated chain caps.
8. `crates/atd-runtime/src/secrets.rs:136-170` — `TokenBroker::resolve` + `resolve_bearer` trait; §4.5 plus this SP add no new trait methods, only a wire-format branch inside `resolve_bearer`.
9. `crates/atd-server/src/connection.rs:25-39` — per-connection `caps + caller_id` state machine; UDS path receives UCAN-derived caps via the same `*caps = Arc::new(CapabilitySet::from_iter(granted))` write.
10. `docs/superpowers/specs/2026-04-25-sp12-canonical-dispatch.md:36-43` — SP-12's own roadmap row "UCAN tokens, attenuation, revocation, audit log" as the deferred v3 target; this SP claims that row.
11. `docs/superpowers/specs/2026-05-11-sp-streamable-http-design.md:103-178` — SP-1.B §4.4 defaulted `resolve_bearer`; composes with UCAN-JWT branch.
12. `docs/superpowers/specs/2026-05-11-sp-token-broker-phase2-design.md:371-414` — three-layer revocation; §4.7 reuses layer-2 (broker push hook) verbatim.
13. `docs/superpowers/specs/2026-05-11-sp-token-broker-phase2-design.md:343-369` — SSE long-connection heartbeat re-validate; UCAN revocation checks slot into the same heartbeat.

### Celia source

14. `crates/celia-core/src/auth/rbac.rs:319-329` — `consent_matches_caller`: exact-equality grantee check; UCAN broker preserves this by translating deepest `aud` → `agent:<name>:<token_short>` via the new `did_to_grantee` column.
15. `crates/celia-core/src/auth/rbac.rs:285-292` — scope-CSV split logic; ATD UCAN `args.caps: Vec<String>` decodes to the same string set, zero transformation.
16. `crates/celia-core/migrations/0000_initial_schema.sql:16-31` — current `consent` columns (`grantee`, `effective_from/until`, `scope`); §6 extends additively with `parent_consent_id`, `ucan_jwt`, `ucan_cid`, `did_to_grantee`.
17. `crates/celia-core/migrations/0003_phase_i_agent_plane.sql:8-9` — precedent for additive `ALTER TABLE consent`; §6 follows the same pattern.
18. `crates/celia-cli/src/atd_broker.rs:74-201` — current `CeliaConsentTokenBroker::resolve_bearer`; §4.8 + §7.2 phase 3 add a sibling UCAN-JWT branch.
19. `docs/ATD_FUTURE_ISSUES.md:23-45` — Issue §1.A motivating this SP; word-for-word the constraint "Agent A may delegate read-only access to Patient X to its sub-agent B."
20. `docs/ATD_FUTURE_ISSUES.md:181-201` — Issue §3.A `did:agent` proposal; §4.4 carves a forward-compat reserved-DID-methods slot.
21. `docs/patents/main.zh.md:369-373` — patent §13.4 "capability gate 以白名单或基于密码学令牌的形式"; this SP promotes the "或" branch from latent to load-bearing.

### External spec

22. UCAN v1.0 specification — https://github.com/ucan-wg/spec — normative source for `iss/aud/sub/cmd/args/nonce/exp/prf`, attenuation rules, audience-match rule, `did:key` requirement.
23. UCAN v1.0 §"Cryptographic Algorithms" — Ed25519 as preferred signature algorithm; we narrow to Ed25519-only.
24. UCAN v1.0 §"Reserved Namespaces" — `/ucan/*` reserved; ATD uses unreserved `atd-cap` sentinel under §4.5.
25. RFC 7519 — JWT compact serialization; our §4.1 profile borrows the encoding while keeping UCAN semantics.
26. RFC 8032 — Ed25519 signature algorithm; ed25519-dalek 2.x implements this.
27. did:key method spec — https://w3c-ccg.github.io/did-method-key/ — self-resolving public-key-as-DID; §4.4 normative source.

---

**Summary.** SP-capability-v2 promotes ATD's string allow-list into a UCAN-shaped capability chain, additively. A new `Hello.ucan_tokens: Vec<String>` field carries one or more JWT-compact UCAN tokens; the verifier walks the chain (Ed25519 + did:key only), enforces attenuation, expiry, audience, and a configurable max depth (default 5). Capabilities are tunnelled in `cmd="atd-cap" / args.caps: Vec<String>` so SP-12's existing string list works verbatim. Celia's `consent` table grows three nullable columns (`parent_consent_id`, `ucan_jwt`, `ucan_cid`) plus a `did_to_grantee` translation column — enough to represent "agent A delegated patient-X read to sub-agent B" as two related rows the broker can validate offline. Revocation reuses SP-token-broker-phase2 §4.8's broker-push hook keyed by UCAN CID; SSE long-stream revocation honours the existing 60s heartbeat. No DID resolver, no JWKS, no blockchain, no multi-algorithm crypto in v1 — those carve out as future SPs. Patent §13.1 invariant: the verifier is pure CPU, never touches the DEK; patent §13.4 multi-agent isolation gets a structural enforcement layer (audience-pinned UCANs cannot be re-presented by a different agent). Migration: 5 ATD-side phases + 5 Celia-side phases, each independently revertible, each gated by `pnpm --filter @celia/desktop test:dek`.
