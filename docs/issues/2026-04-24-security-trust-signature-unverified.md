# `ToolTrust.signature` never verified; TrustLevel is honor system

**Layer:** security
**Status:** deferred-phase-2
**Effort:** ~3 days (design + PKI + signature scheme)
**Filed:** 2026-04-24

## Summary

Every tool's `ToolTrust` carries a `signature: Option<String>` field
and a `trust_level: TrustLevel` (L1 / L2-tested / L3-audited). The
runtime never validates the signature and never cross-checks the
`trust_level` against any verification authority. The fields serve as
documentation, not as a security control.

## Current state

```rust
// every built-in tool in crates/atd-ref-server/src/tools/
trust: ToolTrust {
    publisher: "atd-ref-server".into(),
    trust_level: TrustLevel::L2Tested,   // self-declared
    signature: None,                     // always None
},
```

- `signature` is `Option<String>` and always `None`
- `trust_level` is whatever the tool author wrote
- No PKI, no key registry, no signing ceremony
- Discovery returns `ToolTrust` fields verbatim; callers can read
  them but nothing confirms accuracy
- `L3Audited` is meaningful only by convention

## Gap

- No signing scheme (candidates: Ed25519, ECDSA-P256, cosign)
- No public-key registry (where do callers fetch the verification key?)
- No per-publisher key rotation policy
- No client-side "reject unsigned tools" flag
- No audit trail of who claimed what `trust_level` when

## Impact

- **Today:** a caller cannot distinguish a ref-server-maintained tool
  from an unknown third-party tool beyond reading the `publisher`
  string and believing it.
- **Tomorrow (tool marketplace):** without signatures, anyone running
  `atd-ref-server` can lie about `trust_level: L3Audited`. The whole
  tiered trust story is unenforceable.

## Why deferred

Implementing real signatures requires:

1. A clear threat model — what attack does signing prevent?
   (Supply-chain injection? Publisher impersonation? Both?)
2. A key-distribution story — which keys does a client trust by
   default? How does trust propagate? Is it TOFU, DNSSEC-rooted, Web
   of Trust, or a centralized Sigstore-style authority?
3. At least one publisher other than `atd-ref-server` so the
   verification actually differentiates.

None of these are answered by the MVP. Shipping fake signatures (e.g.,
an HMAC of the JSON body with a shared secret) would be worse than no
signatures — it would look like security without providing any.

## Interim guidance

- Keep the field; keep it `None`.
- Document `TrustLevel` as self-declared in
  `docs/protocol/wire-format.md` (currently the type table doesn't
  flag this — TODO: add a note).
- Callers that want to limit exposure should pin tool ids they trust
  (e.g., the `ref:` namespace for the reference server) rather than
  trusting `trust_level`.

## Recommended Phase 2 path

When adopter demand materializes:

1. Adopt Sigstore (cosign / fulcio / rekor) — proven OCI-world
   approach, keyless short-lived certs via OIDC
2. Add `atd verify` command to atd-cli that checks a tool's signature
   against a publisher's certificate chain
3. Add `AtdClient::connect(endpoint).require_signed_tools()` opt-in
   flag that filters `discover()` to signed tools only

## Related

- `crates/atd-types/src/tool.rs` (ToolTrust)
- `crates/atd-types/src/enums.rs` (TrustLevel)
- `docs/protocol/wire-format.md` §5 (type table — should note honor
  system)
- Companion: `2026-04-24-security-capability-tokens-deferred.md`
