# SP-3.A agent identity: `did:agent` + binary fingerprint VCs

| Status | Position Paper / Draft |
| Created | 2026-05-11 |
| Author | cross-project subagent (celia_phr ↔ atd-mvp coordination) |
| Phase | ATD post-v0.3.0; cross-vendor standard, **not ATD-owned** |
| Related | SP-capability-v2 (`2026-05-11-sp-capability-v2-design.md`, commit `a5acbb6`) §4.4 reserved a `did:agent` slot; this SP fills it. Celia `ATD_FUTURE_ISSUES.md §3.A` (the motivating ticket); Celia W3C VC stack (`crates/celia-core/src/vc/issuer.rs`, `crates/celia-tools/src/dispatch.rs:124, 229-230`). W3C DID Core (https://www.w3.org/TR/did-core/); W3C VC Data Model 2.0 (https://www.w3.org/TR/vc-data-model-2.0/); UCAN v1.0; in-toto / SLSA binary attestation; Sigstore transparency log. |

---

## 1. Motivation

**1.1 Cross-vendor agent identity has no canonical handle.** Today an ATD `Hello` carries `client_id: Option<String>` (`crates/atd-protocol/src/messages.rs:34-39`) — a free-form, self-declared, unverified string. `CallContext.caller_id` (`crates/atd-runtime/src/context.rs:39`) copies that string into every audit event. Two structurally different actors — an Anthropic-hosted Claude 3.5 Sonnet instance and a self-built scraper script — cannot be told apart by the tool server, because nothing about the wire forces the claim to be cryptographically grounded. SP-capability-v2 (commit `a5acbb6`) introduced a `did:key`-based UCAN issuer/audience layer, but `did:key` is *self-asserting*: the public key *is* the identity. There is no place in the chain for a *vendor* claim ("this key belongs to Anthropic's production fleet") or a *binary* claim ("this agent process is running build `7f3a…` of `claude-cli`"). SP-capability-v2 §4.4 explicitly carved out the slot and named this SP as the filler.

**1.2 Regulated tool servers need vendor + binary attestation as a trust signal.** Celia is a Phase-G medical-record system where §13.4 of the patent (multi-agent isolation) and Celia's own RBAC (`crates/celia-core/src/auth/rbac.rs:319-329`, `consent_matches_caller`) currently authenticate callers by a flat grantee string — `agent:claude:abc123`. The string is asserted by the human user at pair-time; there is no continuous proof that the pair-time-asserted "claude" actually corresponds to a SOC-2-audited Anthropic instance at *call* time. A future hospital-grade policy might want to say: *"only let SOC-2-audited LLM vendors call `issue_health_credential` (`crates/celia-tools/src/dispatch.rs:124`); deny self-hosted unknown agents."* That policy needs the tool server to *verify*, at call time, an issuer signature rooted in a vendor's trust anchor. The string-grantee approach cannot express it. SP-capability-v2's `did:key`-only UCAN cannot express it either — `did:key:z6Mk…` is per-session ephemeral and tells you nothing about vendor identity.

**1.3 ATD does not own this standard.** Cross-vendor agent identity is a *protocol-shaped* concern that needs Anthropic, OpenAI, Google, Hermes, Cursor, Microsoft Copilot — and regulated-tool-server adopters like Celia — at one table. ATD-the-protocol is the call-dispatch layer; carrying the `did:agent` identifier on the wire is in-scope, *defining* the trust framework underneath it is not. This SP therefore positions itself as: **(a)** a position paper sketching the `did:agent:<vendor>:<instance>` DID-method shape with enough rigor to seed an inter-vendor working group; **(b)** a `DidResolver` runtime hook inside `atd-runtime` so adopters (Celia first) can plug *any* DID method — `did:agent`, `did:web`, future `did:plc` — into the existing SP-capability-v2 UCAN verifier and Celia VC stack without further ATD changes; **(c)** an integration path for Celia's existing W3C VC infrastructure (`crates/celia-core/src/vc/issuer.rs:84-104`) that lets `did:agent` issuers coexist with the per-session `did:key` issuer Celia already ships.

## 2. Goals

- **G1.** Specify a candidate **`did:agent:<vendor>:<instance>`** DID method shape with enough detail to seed an inter-vendor working group conversation — but **not enough to be normative**; the actual normative spec is the WG's deliverable, not ATD's.
- **G2.** Define a **`BinaryFingerprint` VC claim type** that any W3C VC issuer can embed (Celia's existing issuer included) without changes to `SignedCredential`'s wire shape (`crates/celia-core/src/vc/issuer.rs:46-53`).
- **G3.** Introduce a Rust **`DidResolver` trait** in `atd-runtime` — a single-method trait future SPs can implement for any DID method; SP-capability-v2's hard-coded `did:key` path becomes the **default impl**, not the only impl.
- **G4.** Specify a **forward-compatible integration path** with SP-capability-v2 UCAN tokens: a UCAN signed by a `did:agent:anthropic:…` issuer becomes verifiable once an adopter wires a resolver impl; the wire shape and verifier code path do not change.
- **G5.** Specify Celia's **VC issuer/subject upgrade path**: today's per-session `did:key` issuer keeps signing user-data VCs; a `did:agent` *attestation* VC can be issued by the vendor (out-of-band) and *chained* to the Celia VC via a new `attestations: Vec<SignedCredential>` field on `CredentialClaims` (or via a referenced credential pattern).
- **G6.** Lay out a **trust-anchor candidate menu** (vendor-distributed root pubkey, Sigstore-style transparency log, web-of-trust, TOFU-with-pinning) with trade-offs — explicitly without choosing one, since the WG owns that decision.
- **G7.** Propose a **governance routing** for the inter-vendor WG: candidate host bodies (W3C CCG, IETF, Linux Foundation Decentralized Trust, OpenSSF, new "Agent Protocol WG sibling"), with a recommendation and a 6-12 month roadmap.
- **G8.** Identify the **§13.1 device-local invariant** preservation rule for Celia's mobile/desktop targets: the Celia-side resolver impl must be pure CPU + no network IO on the call hot path (matches SP-capability-v2 §4.4's `did:key` posture).
- **G9.** Enumerate **future-SP carve-outs** so this SP cannot accidentally pull in an HTTPS resolver, a CA hierarchy, a federation registry, or a blockchain anchor.
- **G10.** Land a **minimal test plan** matching the position-paper status — trait-shape unit tests + a mock resolver + a Celia VC round-trip test that the existing `did:key` issuer is unaffected by the new types.

## 3. Non-goals

- **Full normative `did:agent` spec.** ATD writes a *candidate* shape; the inter-vendor WG owns the normative document.
- **Vendor namespace registry implementation.** Who controls `did:agent:anthropic:*` is a WG decision; this SP only sketches the candidate routes.
- **CA hierarchy / PKI infrastructure.** No X.509 chain, no certificate transparency, no OCSP, no CRL distribution.
- **Sigstore / SLSA build attestation pipeline.** The `BinaryFingerprint` claim is a *shape*; producing the fingerprint via reproducible builds is an adopter concern.
- **HTTPS / `did:web` resolver implementation.** SP-capability-v2.2 (already on the roadmap) handles that. Our `DidResolver` trait is the slot it plugs into.
- **Blockchain / on-chain anchoring / ENS / ION / `did:plc`.** Out of scope; pluggable later via the same trait.
- **ZK-proof identity, anonymous credentials, BBS+ signatures.** Out of scope; pluggable later if a WG ratifies a claim shape.
- **Replacing SP-capability-v2's `did:key` UCAN issuer.** `did:key` remains the *capability-issuance* layer (per-session). `did:agent` is an *attestation* layer that wraps or annotates the `did:key` issuer's keys.
- **Cross-vendor identity revocation network protocol.** Each vendor decides how to revoke their `did:agent:<vendor>:*` namespace; the trait surface allows a resolver to return `Revoked`, but no cross-vendor revocation gossip is specified.
- **Tool-server policy DSL** (e.g., `require did:agent:soc2-audited:*`). Tools that want SOC-2 gating read `did:agent` from `CallContext.caller_id` and apply their own policy; ATD itself stays policy-agnostic.
- **Concrete vendor KMS integration** (Anthropic Vault, AWS KMS, Google Cloud KMS). Vendors choose their own key-management; ATD only verifies signatures against resolver-returned public keys.
- **Mandatory `did:agent` enforcement.** A `client_id: None` Hello stays legal; a UCAN issuer that is `did:key` stays legal. `did:agent` is *opt-in additively*.

## 4. Design

Each subsection answers one of the 8 decision points from the brief. This is ~50% of the SP. Each section ends with a one-line decision summary the WG can iterate on.

### 4.1 SP scope — position paper + ATD `DidResolver` trait hook only; full method spec is WG's job

**Decision.** This SP delivers three artefacts and *only* three:
1. A **candidate DID-method sketch** for `did:agent` (§4.2) — enough syntax and operation semantics to start a WG conversation, **not** a normative spec.
2. A **Rust `DidResolver` trait** in `atd-runtime` (§4.7) — a single-method, async-safe, infallible-on-success trait that lets adopters inject DID method handlers.
3. A **Celia VC integration sketch** (§4.6) — additive claim type + chaining pattern, no breaking change to `SignedCredential` (`crates/celia-core/src/vc/issuer.rs:46-53`).

**Rationale.** ATD's job, in the cross-vendor identity domain, is to (a) expose the *plug shape* (the trait) and (b) be the *coordinating venue* that proposes the problem. The shape of `did:agent` itself is too consequential — and too far outside ATD's existing surface — for ATD to unilaterally settle. W3C DID Core §8 mandates four sections for any DID method (syntax, operations, security, privacy); we sketch syntax + security, leave operations + privacy to the WG. The `did:web` precedent is informative: that method's normative spec is a separate W3C CCG deliverable, not part of DID Core; same pattern fits here.

**Rejected alternative — full normative method spec.** Tempting (would let Celia adopt immediately), but ATD does not have the multi-vendor mandate to make `did:agent:anthropic:*` mean what Anthropic wants it to mean. A unilateral spec would get ignored or forked by the first vendor with a different opinion. Position paper + plug = standards-friendly path.

**Rejected alternative — runtime hook only, no position paper.** Also tempting (smallest ATD scope), but adopters reading the SP would have no idea what to plug *in*. Position paper anchors the trait's purpose; future SPs can refine.

**Decision summary.** ATD ships a trait + a candidate sketch + a WG-routing proposal. ATD does not ship a normative `did:agent` spec.

### 4.2 DID form — recommend `did:agent:<vendor>:<instance>` (flat 2-segment); 3-segment with `<model>` rejected as overdetermined; content-addressed `<binary-cid>` reserved as an alternate

**Candidate form.** `did:agent:<vendor>:<instance>` where:
- `<vendor>` is a method-specific identifier registered in the WG's vendor registry (§4.3) — e.g., `anthropic`, `openai`, `google`, `hermes`, `selfhost`.
- `<instance>` is a vendor-scoped opaque identifier the vendor controls (UUID, content-hash, KMS key reference — vendor's choice).

The ABNF (under W3C DID Core's grammar):
```
agent-did = "did:agent:" vendor ":" instance-id
vendor    = 1*( ALPHA / DIGIT / "-" )
instance-id = 1*idchar
```
Compliant with `did = "did:" method-name ":" method-specific-id` (DID Core §3.1) where `method-specific-id` here uses the `:` separator the spec permits.

**Why flat 2-segment.** Three considerations:
1. **Granularity match.** The two trust axes adopters actually want to gate on are (a) *which vendor* signs, and (b) *which instance* signed (so revocation is per-instance, not per-vendor-wide). Model name and binary hash are *claim payloads*, not identity components.
2. **Stability across model upgrades.** If `<model>` is in the DID, Anthropic shipping `claude-3.5-sonnet → claude-3.6` changes the DID for the same conceptual agent fleet, breaking pinned consents (Celia `consent.grantee` rows would orphan).
3. **DID Core §8.4 privacy.** Smaller DID surface = less correlatable information leaked at the identifier layer; richer claims go inside VC payloads (§4.4) where they are revocable separately.

**Why not `did:agent:<vendor>:<model>:<instance>`.** Overdetermined. Model identity is a fast-moving attribute (Anthropic ships new snapshots weekly); pinning it into the DID forces re-pairing on every vendor refresh.

**Why not content-addressed `did:agent:<vendor>:<binary-cid>`.** Tempting (DID *is* the binary hash → self-verifying), but: (a) reproducible-build assumptions outside vendor control; (b) one DID per binary release blocks aggregating audit events from a single conceptual agent across upgrades; (c) collides badly with Sigstore's transparency-log model where the *attestation* is hash-keyed, not the *identity*. **Reserved as an alternate form** the WG can adopt later if reproducible-build adoption matures, e.g., `did:agent:<vendor>:cid:<multihash>`.

**Decision summary.** Recommend flat 2-segment `did:agent:<vendor>:<instance>`. Treat model + binary hash as VC-payload claims, not DID components.

### 4.3 Vendor namespace governance — recommend "namespaced registry under a WG-controlled JSON file" (IANA-light); distributed self-claim rejected; full PKI registry deferred

**Candidate options surveyed.**

| Option | Form | Pros | Cons | Verdict |
|---|---|---|---|---|
| WG-controlled JSON registry (IANA-light) | A flat JSON file in a WG git repo mapping `<vendor>` → vendor display name + trust anchor pubkey + contact URI; PRs gated by WG review | Low operational cost; transparent history; familiar pattern (the IETF media-type registry, W3C DID method registry); diff-able | Single PR gate; not federation-friendly long-term | **recommended for v1** |
| Distributed self-claim (TXT records, `.well-known`) | Vendor proves control of `vendor.example.com` via DNS TXT; resolver fetches over HTTPS | Decentralised; no registry maintainer | Requires HTTPS resolver, TLS root trust, mutable DNS | deferred (`did:web` covers same case) |
| Blockchain registry | On-chain mapping `<vendor>` → root pubkey | Tamper-evident | Latency, settlement, key-rotation pain | rejected |
| Full PKI / CA hierarchy | X.509 chain rooted at WG-CA | Mature tooling | Heavyweight; CA-revocation pain | deferred (post-WG decision) |
| No registry — first-come, first-served | Any vendor can claim any string | Zero overhead | Squatting; no trust signal at all | rejected |

**Recommendation: WG-controlled JSON registry.** Mirrors the existing W3C DID Method Registry (https://www.w3.org/TR/did-spec-registries/) and IETF Media-Type Registry — both shipped successfully without blockchains or HTTPS resolvers. A single `vendor-registry.json` file in the WG repo, signed by the WG release engineer with a published transparency-log entry (Sigstore Rekor or equivalent), is enough infrastructure for v1. Vendors add themselves via PR; the registry contains:
- `vendor: "anthropic"`
- `display_name: "Anthropic, PBC"`
- `trust_anchor_pubkey: "did:key:z6Mk…"` (vendor-supplied; vendor signs everything underneath)
- `contact: "https://anthropic.com/.well-known/did-agent"`
- `policy_url: "https://anthropic.com/agent-attestation-policy"`

**Why JSON not HTTPS-fetched.** Adopters embed the registry at *build time* (cargo `include_str!`-style) or pull it from a known location at *startup* (no per-call IO). This preserves the `DidResolver` trait's "no network IO on the call hot path" invariant (§4.7).

**Failure mode.** A `did:agent:bogus_vendor:…` DID where `bogus_vendor` is not in the registry: resolver returns `DidResolverError::UnknownVendor` (§4.7). Adopter policy decides whether to fail-closed or downgrade to "untrusted vendor" semantics.

**Decision summary.** WG-controlled JSON registry, embedded or startup-fetched. PRs gate vendor registration; transparency log signs releases. Operationally identical to the existing W3C DID Method Registry.

### 4.4 Binary fingerprint claim — recommend embedded `BinaryFingerprint` VC claim with SLSA-style envelope; in-toto referenced as compatible; reproducibility carved as adopter problem

**Candidate claim shape.** A new W3C VC `type` value:
```json
{
  "@context": ["https://www.w3.org/ns/credentials/v2"],
  "type": ["VerifiableCredential", "AgentBinaryAttestation"],
  "issuer": "did:agent:anthropic:cli-fleet-prod-2026q2",
  "credentialSubject": {
    "id": "did:agent:anthropic:cli-fleet-prod-2026q2",
    "binaryFingerprint": {
      "type": "BinaryFingerprint",
      "digestAlgorithm": "sha256",
      "digestValue": "7f3a92e4c1d8…",
      "buildId": "claude-cli-1.4.7",
      "builtAt": "2026-05-02T14:00:00Z",
      "slsa": {
        "level": 3,
        "buildType": "https://anthropic.com/build/v1",
        "predicateUri": "https://anthropic.com/attest/cli-1.4.7.intoto.jsonl"
      }
    }
  },
  "validFrom": "2026-05-02T14:00:00Z",
  "validUntil": "2026-08-02T14:00:00Z",
  "proof": { "type": "Ed25519Signature2020", "…": "…" }
}
```

**Why this shape.** Three properties:
1. **W3C VC v2 compliance.** Uses the mandatory `@context`, `type`, `issuer`, `credentialSubject`, `validFrom`, `proof` (VC Data Model 2.0 §4). Adopters can verify with any off-the-shelf W3C VC library.
2. **Composable with Celia's existing signer.** `crates/celia-core/src/vc/issuer.rs:108-134` (`sign_credential`) already produces `SignedCredential` with arbitrary `credentialSubject: serde_json::Value`. The `BinaryFingerprint` payload is just a JSON sub-object — no Rust type changes required for Celia to *verify* such a VC. Issuance (from the vendor side) uses the same canonical-JSON signing flow (`crates/celia-core/src/vc/canonical.rs:9-35`).
3. **SLSA-referenced, not SLSA-embedded.** The full SLSA predicate (`https://slsa.dev/provenance/v1`) can be 10s of KB. We carry only the `slsa.{level, buildType, predicateUri}` triple inline; the full predicate is fetched out-of-band when needed (or cached by the adopter). Keeps the VC under 1 KB, suitable for `Authorization: Bearer` header transport even if a future SP allows VC-in-bearer.

**Why not in-toto envelope verbatim.** in-toto's `DSSE` envelope shape (`payloadType`, `payload`, `signatures`) is different from W3C VC's `proof` shape. The two are interoperable — a vendor can produce both from one build — but ATD's chosen lane is W3C VC because (a) Celia already speaks it, (b) it has broader adopter library coverage, (c) `credentialSubject.binaryFingerprint.slsa.predicateUri` can *point at* the in-toto DSSE envelope for adopters who want both. We are addressing-the-attestation, not duplicating it.

**Reproducibility carve-out.** "Was the binary actually built from this source?" is an *adopter* problem (vendors run reproducible builds or they don't). The VC merely *claims* a fingerprint; verifying the claim against actual binary on disk is a tool-server-side step. We do not specify how the tool server obtains the running binary's hash — `/proc/<pid>/exe + sha256sum` on Linux is one option, signed install manifests another. Out of scope for v1.

**Decision summary.** `BinaryFingerprint` VC claim type using W3C VC v2.0 + SLSA-referenced predicate. Celia's existing signer can both issue and verify these without code changes; one new claim shape is the only delta.

### 4.5 Trust anchor model — recommend "vendor-distributed root pubkey via WG registry"; Sigstore mentioned as compatible compliance route; TOFU explicitly rejected

**Candidate options.**

| Option | Description | Pros | Cons | Verdict |
|---|---|---|---|---|
| Vendor-distributed root pubkey via WG registry | Each vendor publishes a stable root pubkey via the §4.3 registry; vendor signs delegation chain to per-instance keys | Simple verifier: walk chain to root, check root in registry; offline-friendly | Vendor key rotation requires registry update | **recommended for v1** |
| Sigstore-style transparency log | Vendor logs each instance-key issuance to a public log (Rekor); verifier checks log inclusion proof | Tamper-evident audit trail; reuses Sigstore infra | New verifier dep (log inclusion proof verification); per-call log lookup not free | compatible with v1; deferred for ATD-side enforcement to a future SP |
| Web-of-trust (PGP-style) | Vendors cross-sign each other's roots; tool servers configure trust paths | No central registry | Combinatorial config burden; no clear adoption story | rejected |
| TOFU with pinning | First-seen vendor key is trusted; pinned for subsequent calls | Zero-config bootstrap | Vulnerable to first-call MITM; bad fit for medical-data adopter (Celia) | rejected |
| Static CA chain (X.509) | Vendors get certs from a WG-CA | Mature tooling | CA-revocation pain; pinning-trap classic | deferred |

**Recommendation: vendor-distributed root pubkey via WG registry (with Sigstore as an optional compliance overlay).**

- The §4.3 registry contains each vendor's `trust_anchor_pubkey` (e.g., a `did:key:z6Mk…` controlled by the vendor's HSM / KMS).
- The vendor signs `did:agent:<vendor>:<instance>` instance-key delegations using the root key. The delegation is itself a W3C VC: `issuer = trust_anchor`, `credentialSubject.id = <instance DID>`, `credentialSubject.verificationMethod = <instance pubkey>`.
- Verifier algorithm:
  1. Parse the `did:agent:<vendor>:<instance>` DID.
  2. Look up `<vendor>` in the WG registry → get `trust_anchor_pubkey`.
  3. Fetch the instance's delegation VC (out-of-band, cached, or inline in a UCAN `prf`).
  4. Verify Ed25519 signature on the VC using the trust anchor pubkey.
  5. Extract the instance pubkey from the VC's `credentialSubject.verificationMethod`.
- All steps are pure CPU (Ed25519 verify + JSON parse). No network IO on the verify hot path — adopters cache delegation VCs at startup or fetch on-demand to local cache.

**Sigstore optionality.** Adopters wanting tamper-evident vendor-side issuance can require that the trust anchor's instance-key issuances be logged to Rekor. The ATD verifier does not enforce this — it is a per-adopter policy gated by an `attestation_policy: Option<Arc<dyn AttestationPolicy>>` extension point (§4.7). Celia's medical-grade default would set `require_rekor_log: true`; a low-stakes ATD use case can leave it `None`.

**Why no TOFU.** Celia is medical data; first-call MITM (an attacker spawns a `did:agent:anthropic:rogue-1` and lets the user pair before Anthropic's real instance arrives) is a real threat. The user can plausibly accept "did:agent:anthropic:rogue-1" if there's no out-of-band verification path. Hard reject for any regulated-tool adopter.

**Decision summary.** WG-registry root pubkey, vendor-signed delegation VCs, optional Sigstore-log compliance overlay. Pure CPU verify path.

### 4.6 W3C VC integration with Celia — additive `attestations` field on `CredentialClaims`; today's `did:key` issuer untouched

**Decision.** Celia's existing per-session `did:key` issuer (`crates/celia-core/src/vc/issuer.rs:84-104`, `derive_issuer_keypair`) keeps issuing user-data VCs *exactly as today*: issuer DID is `did:key:z<base64url of Ed25519 pubkey>` (line 98), subject is `did:key:z…#self` (`crates/celia-tools/src/dispatch.rs:720`), proof is `Ed25519Signature2020`. Bit-for-bit unchanged.

**The additive shape: VC chaining via an `attestations` claim sub-field.** A user-data VC issued by Celia's per-session `did:key` issuer can carry an embedded vendor attestation:
```json
{
  "version": 1,
  "issuer": "did:key:z6MkUserSessionKey",
  "claims": {
    "subject": "did:key:z6MkUserSessionKey#self",
    "type": ["VerifiableCredential", "HealthSummary"],
    "credentialSubject": { /* health summary payload */ },
    "attestations": [
      {
        "version": 1,
        "issuer": "did:agent:anthropic:cli-fleet-prod-2026q2",
        "claims": {
          "subject": "did:key:z6MkUserSessionKey",
          "type": ["VerifiableCredential", "AgentBinaryAttestation"],
          "credentialSubject": { /* §4.4 binary fingerprint */ }
        },
        "proof": { /* Ed25519 signed by Anthropic's instance key */ }
      }
    ]
  },
  "proof": { /* Ed25519 signed by Celia's per-session did:key */ }
}
```

**Three properties.**
1. **Backward-compatible.** `attestations` defaults to omitted (current Celia VCs have no such field). `CredentialClaims.credential_subject` (`crates/celia-core/src/vc/issuer.rs:28-30`) is already `serde_json::Value`; the new field nests inside it as a sub-object — **zero Rust type changes** to ship the receive side. The send side adds an optional `attestations: Option<Vec<SignedCredential>>` field to `CredentialClaims` (or keeps it inside `credential_subject` for the most additive case).
2. **`credentialSubject.id` flexibility preserved.** W3C VC v2 allows any URI in `credentialSubject.id` (per §4.4 of our WebFetch summary). The inner attestation's `credentialSubject.id = "did:key:z6MkUserSessionKey"` — i.e., the attestation says "Anthropic attests that the agent process holding *this `did:key`* (the one running Celia's chat session) is running the SOC-2-audited claude-cli 1.4.7." This **binds** the vendor attestation to the session's `did:key` cryptographically.
3. **§13.1 invariant preserved.** Both signing and verification are pure CPU. The outer Celia VC signature uses the session-derived issuer key (already §13.1-compliant, lines 84-104). The inner attestation is verified against the §4.3 vendor registry's trust anchor — no KMS dep, no network IO.

**Tool dispatch side.** `crates/celia-tools/src/dispatch.rs:737-755` (`handle_present_credential`) already calls `verify_credential` on the outer VC. To verify *attestations*, the tool dispatcher (post-SP) iterates `claims.credential_subject.attestations[]`, calling `verify_credential` recursively on each, then resolves the attestation's issuer via the `DidResolver` (§4.7). One additional pass per credential, all pure CPU.

**Three-step Celia migration.**
1. **Step 1** — Add optional `attestations: Option<Vec<SignedCredential>>` field to `CredentialClaims` (or in `credentialSubject`). `serde(default, skip_serializing_if = "Option::is_none")`. No verify-side changes yet. Old VCs continue to parse. `pnpm --filter @celia/desktop test:dek` stays green.
2. **Step 2** — Add a `did_agent_resolver: Arc<dyn DidResolver>` field to `TokenConfig` / tool dispatcher state. Default is a no-op resolver that returns `Unsupported` for `did:agent:*`. Verification of attestations is gated behind `did_agent_resolver.is_some()`.
3. **Step 3** — Wire a real resolver in the Tauri shell + Capacitor plugin: ships the §4.3 registry as `include_str!("vendor_registry.json")` at build time. Mobile (`apps/mobile/native/capacitor-celia-core/`) gets the same registry baked into the UniFFI binary — no runtime fetch, preserving §13.1 device-local posture (CLAUDE.md "Why mobile-native instead of WASM").

**Decision summary.** Additive `attestations: Vec<SignedCredential>` claim sub-field. Celia's existing issuer unchanged. Tool dispatcher gets one recursive-verify pass + a resolver lookup per attestation, all pure CPU.

### 4.7 ATD-runtime `DidResolver` trait — single async-safe method; SP-capability-v2 verifier consumes it via dependency injection

**Decision.** Land in `atd-runtime/src/did_resolver.rs`:

```rust
//! Pluggable DID resolution. The default impl resolves `did:key:z<...>`
//! self-resolvingly (the DID *is* the public key). Adopters wire other
//! impls (did:agent via §4.3 registry, did:web via HTTPS, etc.) by
//! injecting `Arc<dyn DidResolver>` into `ServerConfig`.
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DidDocument {
    /// The DID this document describes.
    pub id: String,
    /// One or more Ed25519 verification methods. Currently `verify_*`
    /// callers pick the first; multi-method support is a future SP.
    pub verification_methods: Vec<VerifyingMethod>,
    /// Optional vendor-level trust anchor for `did:agent:*`. `None` for
    /// `did:key`.
    pub trust_anchor: Option<TrustAnchor>,
}

#[derive(Debug, Clone)]
pub struct VerifyingMethod {
    pub id: String,
    pub key_type: KeyType,
    /// Raw bytes (32 for Ed25519).
    pub public_key: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Ed25519,
}

#[derive(Debug, Clone)]
pub struct TrustAnchor {
    pub vendor: String,
    pub registry_version: String,
    pub anchor_pubkey: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum DidResolverError {
    #[error("unsupported did method: {0}")]
    UnsupportedMethod(String),
    #[error("malformed did: {0}")]
    Malformed(String),
    #[error("unknown vendor in registry: {0}")]
    UnknownVendor(String),
    #[error("revoked: {0}")]
    Revoked(String),
    #[error("network: {0}")]
    Network(String),
}

#[async_trait::async_trait]
pub trait DidResolver: Send + Sync + 'static {
    /// Resolve a DID to a document. Implementations of `did:key`
    /// resolution are pure CPU and synchronous in nature; the async
    /// signature accommodates resolvers that fetch (e.g. `did:web`).
    /// Adopters should ensure hot-path resolvers use a local cache
    /// (Celia bakes the §4.3 registry into the binary).
    async fn resolve(&self, did: &str) -> Result<DidDocument, DidResolverError>;
}

/// Default impl. Recognises `did:key:z<base64url-Ed25519-pubkey>`.
/// SP-capability-v2's hard-coded did:key path becomes this impl.
pub struct DefaultDidKeyResolver;
```

**SP-capability-v2 integration.** SP-capability-v2 §4.4 specifies a hard-coded `did:key` parser inside the UCAN verifier. Post-SP-3.A, that parser is *the body of `DefaultDidKeyResolver::resolve`*. The UCAN verifier holds `Arc<dyn DidResolver>` and calls `.resolve(&iss).await` instead of inlining the parse. **No wire change**, **no UCAN-format change**, **no Hello-field change** — purely an internal refactor in the verifier. Once the trait exists, an adopter can swap in a composite resolver:

```rust
struct CompositeResolver {
    did_key: DefaultDidKeyResolver,
    did_agent: Arc<DidAgentRegistryResolver>, // adopter-provided
}
```
that routes by DID method prefix. SP-capability-v2's `ServerConfig` grows one optional field; existing adopters keep working with the default.

**Why async even though `did:key` is sync.** Forward-compat: `did:web` requires HTTPS, `did:plc` requires a registry lookup, future `did:agent` resolvers may want to consult a transparency log. Making the trait async from day one avoids an `async`-conversion break later. The sync `did:key` impl is `async fn resolve(...) { /* pure CPU */ Ok(...) }` — zero overhead.

**Why infallible-on-success (`Result<DidDocument, DidResolverError>`).** Resolution is binary: either we have a verifying key for this DID or we don't. Partial-info DID Documents are out of scope.

**Composition with SP-secret-bootstrap.** Adopters that need their resolver to consult a parent-injected secret (e.g., a private vendor registry URL with an API key) wire the resolver impl as a downstream consumer of `secret_bootstrap` (SP-secret-bootstrap §4.1) — the resolver receives the secret in its constructor; ATD itself stays agnostic.

**Decision summary.** Single-method async trait `DidResolver`, default `did:key` impl, composite-routing pattern for adopters. Drop-in plug for the SP-capability-v2 UCAN verifier; no wire change.

### 4.8 Governance routing — recommend forming a *new* "Agent Identity WG" under Linux Foundation Decentralized Trust (LF DT); W3C CCG and IETF surveyed as alternatives

**Candidate host bodies.**

| Host | Pros | Cons | Verdict |
|---|---|---|---|
| **W3C Credentials Community Group (CCG)** | Owns DID Core, DID Method Registry, VC Data Model 2.0; clear standards-track path; obvious home for did:agent | Slow cadence (years to REC); membership skews academic + identity vendors, not LLM vendors | strong candidate; risk: too slow for the 2026-2027 inflection |
| **IETF (new working group)** | Authoritative for cross-vendor wire formats; precedent (OAuth, JWT, ACME) | High formalism overhead; pattern is RFC-shaped, not ecosystem-shaped; LLM vendors not represented in current IETF | possible but heavy; better fit for the *wire-format* sub-spec once the WG exists |
| **Linux Foundation Decentralized Trust (LF DT)** | Houses Hyperledger AnonCreds, Trust over IP; existing vendor-cooperative model; faster cadence than W3C; member-driven | Less name recognition than W3C; SDO-track requires later W3C/IETF handoff for normative status | **recommended host** |
| **OpenSSF (Open Source Security Foundation)** | Houses SLSA, Sigstore; natural fit for the binary-fingerprint side | Identity is not OpenSSF's chartered scope | partner WG, not host |
| **Anthropic / OpenAI joint convening (no SDO)** | Fastest start; minimal bureaucracy | Two-vendor design; perceived bias; no standards-track legitimacy | rejected |
| **"APWG sibling" inside the ATD-governance body** | Tight coordination with ATD itself | Requires APWG to exist first; circular dependency | reject for now (revisit after APWG forms) |
| **W3C VC Working Group** | Owns VC Data Model; natural fit for binary-fingerprint claim shape | Identity-method scope is CCG's lane, not WG's | partner, not host |
| **New independent industry consortium** | Maximum flexibility | High setup cost; perceived legitimacy gap | rejected — duplicates LF DT |

**Recommendation: Linux Foundation Decentralized Trust (LF DT) as host, with OpenSSF and W3C CCG as cooperating SDOs.**

Rationale:
1. **Cadence match.** LF DT has shipped two interop profiles in <18 months (Trust over IP v2.x). The cross-vendor identity gap will be load-bearing by 2027; W3C's REC cycle is too slow.
2. **Vendor cooperative model.** LF members already include cloud providers, identity vendors, and several AI-platform companies. The on-ramp for Anthropic / OpenAI / Google / Microsoft to participate as members is short.
3. **Standards-track handoff path.** LF DT's normal pattern is to incubate, then graduate to W3C (DID method) + IETF (wire format) once consensus stabilises. ATD does not lose the eventual standards-track outcome; we just take a faster on-ramp.
4. **OpenSSF partner.** Binary-fingerprint claim (§4.4) reuses SLSA; OpenSSF is the natural cross-pollination partner. No need to re-litigate SLSA inside the new WG.

**ATD's role in the WG.** Convener + first-mover, not normative author. ATD contributes (a) this SP as the position-paper input; (b) the `DidResolver` trait as the reference Rust implementation; (c) Celia as the first adopter (W3C VC test bed per `ATD_FUTURE_ISSUES.md §3.A`). ATD does **not** chair the WG long-term; we hand off to a vendor-neutral chair once the WG has ≥3 LLM vendor members.

**Roadmap (6-12 months).**
- **M0-M2 (now-2026-Q3):** Circulate this position paper to LF DT, OpenSSF, W3C CCG. Identify 2-3 LLM vendor sponsors. ATD-side: land the `DidResolver` trait + default `did:key` impl as a *separate, smaller* SP-3.A.0 (just the trait, no `did:agent`).
- **M3-M6 (2026-Q4 to 2027-Q1):** WG charter; first interop call; agree on registry shape (§4.3) and DID form (§4.2). ATD-side: ship Celia's reference adopter resolver impl (§4.6 step 3).
- **M7-M12 (2027-Q2 to Q3):** Draft normative spec under LF DT. Begin W3C CCG liaison. ATD-side: cross-vendor demo via SP-cross-vendor-mock-demo style (`2026-04-27-sp-cross-vendor-mock-demo-design.md`) but with real vendor attestation.

**Decision summary.** LF DT host, OpenSSF + W3C CCG partners. ATD convenes, hands off the chair. 6-12 month roadmap to a draft spec.

## 5. Wire / DID format reference

### 5.1 `did:agent` string examples
```
did:agent:anthropic:cli-fleet-prod-2026q2
did:agent:openai:assistants-east-12af
did:agent:hermes:bridge-ws-prod-3
did:agent:selfhost:scraper-7f3a92e4
```
All conform to W3C DID Core ABNF (`did = "did:" method-name ":" method-specific-id`, where `method-specific-id` may itself contain `:` separators).

### 5.2 Vendor registry JSON skeleton (§4.3)
```json
{
  "registry_version": "2026-05-11",
  "transparency_log": {
    "type": "rekor",
    "uri": "https://rekor.sigstore.dev/api/v1/log/entries/<sha>"
  },
  "vendors": [
    {
      "vendor": "anthropic",
      "display_name": "Anthropic, PBC",
      "trust_anchor_pubkey": "did:key:z6Mk_ANTHROPIC_ROOT_KEY_ABBREVIATED",
      "contact": "https://anthropic.com/.well-known/did-agent",
      "policy_url": "https://anthropic.com/agent-attestation-policy",
      "registered_at": "2026-05-11T00:00:00Z"
    }
  ]
}
```

### 5.3 Binary fingerprint VC example (§4.4)
See §4.4 — issuer = vendor's `did:agent:<vendor>:<instance>`, payload includes `binaryFingerprint.{digestAlgorithm, digestValue, buildId, builtAt, slsa.{level, buildType, predicateUri}}`. Signed Ed25519. Verifiable today by Celia's existing `verify_credential` (`crates/celia-core/src/vc/issuer.rs:139-...`) once the §4.7 resolver returns the issuer's pubkey.

### 5.4 Chained VC (Celia user-data outer + vendor attestation inner) (§4.6)
See §4.6 worked example. Outer issuer is Celia's per-session `did:key`; inner issuer is `did:agent:<vendor>:<instance>`; inner subject pins to the outer issuer's `did:key`. Both signed Ed25519. No new error codes — verification failures route through existing `VcError` (`crates/celia-core/src/vc/issuer.rs:62-75`).

### 5.5 Future ATD wire fit (sketch only)
No `Hello` field is added by this SP. A future SP could extend SP-capability-v2's `ucan_tokens: Vec<String>` to accept UCANs whose issuer DID is `did:agent:*`. SP-capability-v2 §4.4 already reserves this slot via the `reserved_did_methods: Vec<String>` config field. No wire change in this SP.

## 6. ATD-runtime resolver hook

See §4.7 for the full trait. Placement: `crates/atd-runtime/src/did_resolver.rs`, re-exported from `crates/atd-runtime/src/lib.rs`. Mirrors the `TokenBroker` shape (`crates/atd-runtime/src/secrets.rs:136-184`) — single trait, default impl, dependency-injection via `ServerConfig`. SP-capability-v2's verifier (post-SP-3.A.0) holds `Arc<dyn DidResolver>` and consults it before Ed25519 verification.

**Integration with SP-capability-v2's UCAN verifier.** The verifier currently parses `did:key:z<...>` inline (per SP-capability-v2 §4.4). Post-SP-3.A.0, it calls `did_resolver.resolve(&iss).await?` and uses the returned `DidDocument.verification_methods[0].public_key` for the Ed25519 verify. The default `DefaultDidKeyResolver` produces the exact same key bytes; behaviour unchanged for `did:key` chains. A composite resolver routing `did:agent:*` to a registry-aware impl unlocks vendor-issued UCANs additively.

## 7. Celia integration path (three-step migration)

See §4.6 for the worked migration. Summary:

1. **Step 1** — Add optional `attestations: Option<Vec<SignedCredential>>` claim field to `CredentialClaims` (or nest in `credential_subject`). `serde(default, skip_serializing_if)`. Old VCs unaffected. `pnpm --filter @celia/desktop test:dek` runs green; `cargo test -p celia-core` passes.
2. **Step 2** — Wire `did_agent_resolver: Option<Arc<dyn DidResolver>>` into Celia's tool dispatcher state (`crates/celia-tools/src/dispatch.rs`). Default `None` = current behaviour. When `Some`, `handle_present_credential` recursively verifies each attestation.
3. **Step 3** — Tauri shell + Capacitor plugin bake the §4.3 vendor registry into the binary via `include_str!`. Mobile FFI (`crates/celia-mobile-ffi/`) re-exports the resolver across UniFFI bindings. No network IO; §13.1 invariant preserved (mobile DEK invariant pinned per `apps/mobile/native/capacitor-celia-core/`).

**§13.1 guard.** At every step, the resolver path is pure CPU. No `rusqlite` access, no `KeyCache` touch, no DEK use. The resolver only reads in-memory pubkey bytes (registry or `did:key` parse). Mobile and desktop both stay bit-for-bit identical to the pre-SP behaviour for any VC that omits `attestations`.

## 8. Test plan

Position-paper status — minimal verifier-shape tests, not full conformance. Five tests:
- **`did_resolver_trait_default_did_key_round_trip`** — `DefaultDidKeyResolver::resolve("did:key:z<pubkey>")` returns `DidDocument` with matching `verification_methods[0].public_key`.
- **`did_resolver_trait_unsupported_method_error`** — `DefaultDidKeyResolver::resolve("did:agent:anthropic:foo")` returns `DidResolverError::UnsupportedMethod`.
- **`did_resolver_composite_routing`** — A test composite resolver routes `did:key:*` to default, `did:agent:*` to a mock; mock fixtures return a fixed `DidDocument`; route correctness verified.
- **`celia_vc_attestation_round_trip`** — Celia's `sign_credential` produces a VC with an `attestations` array containing a mock vendor-signed inner VC; `verify_credential` plus the new recursive-attestation-verify pass both succeed. Test uses `DefaultDidKeyResolver` for *both* outer and inner (i.e., the "inner" attestation is also `did:key`-signed in this test) — exercises the claim-shape path without depending on a `did:agent` resolver impl.
- **`celia_vc_backward_compat_no_attestations`** — A pre-SP-3.A VC (no `attestations` field) parses and verifies unchanged; round-trip equality on `SignedCredential` byte representation.

No conformance suite changes (`docs/superpowers/specs/2026-04-24-sp8-conformance-suite-design.md`); SP-3.A.0 (the trait-only follow-up) will add one. Cross-vendor mock-demo extensions land with the WG's first interop draft.

## 9. Out of scope (future SPs)

| Feature | Why deferred | Tracker / suggested SP |
|---|---|---|
| `did:web` resolver impl | Network IO + TLS root trust + cache policy | SP-capability-v2.2-did-web |
| Sigstore / SLSA build attestation pipeline | Adopter-side build-system concern | Out-of-band (OpenSSF) |
| ZK-proof identity / BBS+ anonymous credentials | Research-grade; no v1 adopter | Not on roadmap |
| Cross-vendor identity revocation gossip protocol | Each vendor revokes own namespace; no cross-vendor channel needed in v1 | SP-3.A.x once WG ratifies |
| Tool-server policy DSL (`require did:agent:soc2-audited:*`) | Adopter policy, not ATD core | Celia-side feature; possibly an `AttestationPolicy` trait sibling to `DidResolver` |
| Full trust framework / CA hierarchy | Heavier than v1 needs | Deferred; revisit if WG demands |
| `did:plc` / `did:ion` / on-chain registries | Blockchain dep | Pluggable via trait; no ATD-side SP needed |
| Multi-algorithm crypto (P-256, secp256k1) | Ed25519 covers v1 needs (same posture as SP-capability-v2 §4.3) | SP-capability-v2.1-multi-alg covers UCAN side; SP-3.A.x extends to resolver |
| Federation registry of resolvers | Two-broker concern | SP-federation-v1 |
| `DidResolver` extension methods (resolve+verify combined, batch resolve) | YAGNI; trait kept minimal | Additive future SP |
| UCAN issuer = `did:agent:*` end-to-end flow | Depends on §4.3 registry being live | SP-3.A.1 (post-WG draft) |

## 10. Governance proposal (position paper)

See §4.8 for the full analysis. Recommendation:

- **Host:** Linux Foundation Decentralized Trust (LF DT). Cadence + vendor-cooperative model + standards-track handoff path all match the timeline.
- **Partner SDOs:** OpenSSF (SLSA / Sigstore alignment for §4.4 binary fingerprint); W3C CCG (eventual DID Method Registry entry); IETF (eventual wire-format RFC if the registry shape stabilises).
- **Stakeholders to invite (5-8):** Anthropic, OpenAI, Google (Vertex AI), Microsoft (Copilot), Meta AI, Cursor / Codeium / similar agent-host vendors; plus regulated-tool adopters: Celia (medical), a hospital HIS gateway vendor, a healthkit_cli-shape mobile-data adopter.
- **ATD's role:** convener, position-paper author, reference resolver implementer (Rust). Not chair long-term — hand off once ≥3 LLM vendors are members.
- **6-12 month roadmap:** M0-M2 circulate position paper + ship SP-3.A.0 (trait only); M3-M6 WG charter + first interop call + registry shape agreement; M7-M12 draft normative spec + cross-vendor demo + W3C CCG liaison.

**Why this is the right venue, restated.** The W3C CCG is conceptually the right home but operationally too slow for the 2026-2027 inflection. LF DT has the throughput and the vendor diversity; it incubates and graduates to W3C/IETF for normative status. ATD does not lose standards-track legitimacy; we gain a 12-18 month head-start.

## 11. References

### atd-mvp source (line-precise; spot-check targets)

1. `crates/atd-protocol/src/messages.rs:34-39` — current `Hello.client_id` free-form string; the unverified-identity baseline this SP improves on. SP-3.A adds no wire field here (resolver-only).
2. `crates/atd-runtime/src/context.rs:36-39` — `CallContext.caller_id: Option<String>`; audit events copy this. Post-SP, a `did:agent:*` value flows through unchanged.
3. `crates/atd-runtime/src/capability.rs:1-8` — `CapabilitySet` and its own predictive comment about future cryptographic-token forms; this SP composes with the same forward-compat shape.
4. `crates/atd-runtime/src/dispatch.rs:129-142` — `Hello` arm where `client_id` lands in per-connection state; resolver plug-point sits adjacent to this code.
5. `crates/atd-runtime/src/secrets.rs:136-184` — `TokenBroker` trait shape; `DidResolver` (§4.7) deliberately mirrors this surface (single async-safe trait, default impl, DI via `ServerConfig`).
6. `docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md` §4.4 (around line 111-126) — explicit `did:agent` reservation that this SP fills.
7. `docs/superpowers/specs/2026-05-11-sp-capability-v2-design.md` §9 (out-of-scope row "did:web / did:plc / did:agent resolution") — names this SP as the future filler.
8. `docs/superpowers/specs/2026-05-11-sp-secret-bootstrap-design.md` §4.1 — module-shape precedent (`atd-runtime/src/<feature>/`); `did_resolver` follows.
9. `docs/superpowers/specs/2026-04-27-sp-cross-vendor-mock-demo-design.md` — precedent for cross-vendor demo style; future SP-3.A interop demo follows this shape.

### Celia source

10. `crates/celia-core/src/vc/issuer.rs:46-53` — `SignedCredential` struct shape; §4.4 + §4.6 leave this unchanged.
11. `crates/celia-core/src/vc/issuer.rs:84-104` — `derive_issuer_keypair`: produces `did:key:z<base64url(Ed25519 pubkey)>`; the per-session issuer Celia ships today. §4.6 layers attestations *around* this without touching the function.
12. `crates/celia-core/src/vc/issuer.rs:108-134` — `sign_credential`: canonical-JSON + Ed25519 sign flow; `BinaryFingerprint` claim (§4.4) uses this same flow on the vendor side.
13. `crates/celia-core/src/vc/canonical.rs:9-35` — canonical-JSON serializer reused for chained attestations (§4.6); zero behaviour change required.
14. `crates/celia-tools/src/dispatch.rs:124, 229-230, 737-755` — `issue_health_credential` + `present_credential` tool entry points; §4.6 step 2 recursively verifies attestations inside `handle_present_credential`.
15. `crates/celia-tools/src/dispatch.rs:720` — `subject: format!("{}#self", issuer.did)`; precedent for DID URLs in `credentialSubject`; chained attestation reuses the convention.
16. `crates/celia-core/src/auth/rbac.rs:319-329` — `consent_matches_caller` flat-grantee check; tool servers wanting vendor-gated policy read `did:agent` from a future column without changing this function.
17. `docs/ATD_FUTURE_ISSUES.md:181-201` — Issue §3.A, the motivating ticket; recommends "survey did:key / did:web / Anthropic's agent attestation thinking; write a position paper" — which is exactly this SP.

### External spec

18. W3C DID Core (https://www.w3.org/TR/did-core/) — DID syntax ABNF (§3.1); required method-spec sections (§8); informs §4.2 form choice.
19. W3C VC Data Model 2.0 (https://www.w3.org/TR/vc-data-model-2.0/) — mandatory fields (`@context`, `type`, `issuer`, `credentialSubject`, `proof`); informs §4.4 claim shape.
20. W3C DID Specification Registries (https://www.w3.org/TR/did-spec-registries/) — registry pattern for §4.3.
21. UCAN v1.0 specification — composes with §4.7; SP-capability-v2 already pinned `did:key`; this SP pluralises the method handler.
22. SLSA v1 (https://slsa.dev/) — predicate format referenced by §4.4 `BinaryFingerprint.slsa.predicateUri`.
23. in-toto attestation framework — interoperable alternate envelope for §4.4 binary attestations; adopters may produce both shapes from one build.
24. Sigstore Rekor transparency log — §4.5 optional compliance overlay for tamper-evident vendor-side issuance.
25. RFC 8032 — Ed25519 signature algorithm; same primitive Celia's `ed25519-dalek 2.x` already uses.

---

**Summary.** SP-3.A is a position paper + a single Rust trait. It proposes `did:agent:<vendor>:<instance>` as a 2-segment DID method, gated by a WG-controlled vendor registry (IANA-style JSON file, optionally Sigstore-logged), with binary-fingerprint attestation carried in a W3C VC v2.0 `AgentBinaryAttestation` claim. ATD's only normative contribution is a `DidResolver` trait in `atd-runtime` — async-safe, single-method, default `did:key` impl — that SP-capability-v2's UCAN verifier consumes via dependency injection. No wire change. Celia's existing `did:key` VC issuer (`crates/celia-core/src/vc/issuer.rs:84-104`) is untouched; vendor attestations chain into user-data VCs via an additive `attestations: Vec<SignedCredential>` claim sub-field, preserving §13.1 device-local volatile-key semantics bit-for-bit. Governance routes through Linux Foundation Decentralized Trust as host, with OpenSSF + W3C CCG as partners, on a 6-12 month roadmap to a draft spec. ATD convenes, contributes the reference resolver, and hands off the chair once ≥3 LLM vendors join. SP-capability-v2's `did:key` UCAN issuer remains the capability-issuance layer; `did:agent` is the orthogonal attestation layer that wraps it.
