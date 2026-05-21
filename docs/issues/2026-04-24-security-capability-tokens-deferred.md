# Capability tokens (UCAN-like) — deferred to Phase 2

**Layer:** security
**Status:** closed-verified
**Effort:** ~5-10 days (real UCAN + revocation + integration)
**Filed:** 2026-04-24
**Closed:** 2026-05-11

## Resolution

**Shipped** as **SP-capability-v2** (tag `sp-capability-v2`, 2026-05-11) —
UCAN-lite bearer capability tokens: JWT compact form, Ed25519 signatures,
`did:key` audiences, attenuation chains, and a `UcanRevocationStore`
revocation hook. Wire surface: `Hello.ucan_tokens` plus error codes
1010–1013. Granted capabilities at dispatch = string allow-list ∪
UCAN-derived caps (additive — pre-token adopters unaffected). See the
`[0.3.0]` entry in [`CHANGELOG.md`](../../CHANGELOG.md) ("UCAN-lite
capability tokens"), [`docs/architecture.md`](../architecture.md) §5.2 /
§6.2, and [ADR-0001](../adr/0001-celia-atd-roadmap-alignment.md) §1.A.
The body below is the original deferral rationale, kept as a record.

## Summary

ATD's design frames capability tokens (UCAN-style) as the primary
multi-tenant authorization mechanism. `docs/archive/design.md` §3.6 explicitly
defers enforcement to Phase 2: *"Optional in Phase 0/1, enforced in
Phase 2. Don't block early adopters on security model; grow into it."*
This issue tracks the aspirational shape for when Phase 2 work starts.

## Current state

- **No types.** `CapabilityToken`, `CapabilityDescriptor`, and
  `RevocationStore` do not exist in `atd-types`.
- **No client surface.** `AtdClient::call` has no token parameter.
- **No server enforcement.** Every call on a socket has equivalent
  access to every tool on that socket.
- **Interim:** operators separate concerns by running multiple ATD
  servers on different sockets (dev / prod / read-only), each with a
  distinct tool set. Documented in `docs/integrations/overview.md`.

## Gap vs Phase 2 target

- Cryptographically signed capability descriptors
- Delegation chains (agent A grants agent B a subset of its caps)
- Scope narrowing (e.g., `fs.read` allowed on `~/docs/` only)
- Revocation (caller-side + registry-side)
- Per-tool required-capability declarations (not just the free-form
  `side_effects` field)
- Capability-aware error (`AtdError::CapabilityDenied` exists but is
  never raised because no verification happens)

## Impact

- **Zero multi-tenant isolation today.** One socket = one trust
  boundary. Fine for single-user dev; inadequate for shared
  infrastructure.
- **Blocks certain adopters:** anyone deploying ATD in a shared
  environment cannot safely expose destructive tools (shell.exec,
  fs.write, fs.edit) to multiple agents without the interim "one
  socket per tier" workaround.

## Why deferred (not tracked)

This is explicitly scoped out of the MVP per `docs/archive/design.md` §10.4 and
§3.6. The right time to design it is when:

- A concrete multi-tenant deployment exists
- Real-world use cases inform the delegation depth question (tracked
  as an open question in design.md §3.6 and in the ANOS issue
  `atd-ucan-capability-depth-unclear.md` — now historical)
- The ecosystem has at least one reference adopter willing to accept
  ergonomic trade-offs for stronger safety

Shipping a half-designed capability system without these inputs risks
locking in wrong primitives.

## Interim guidance (already in docs)

`docs/integrations/overview.md` documents the workaround:

> Run separate ATD servers with different tool sets on different
> sockets (dev-socket, prod-socket, readonly-socket). Each consumer
> connects only to the sockets it's authorized to see.

This is socket-level ACL, not tool-level. Coarse but clean for v0.1.x.

## Related issues

- `2026-04-24-security-trust-signature-unverified.md` — related but
  narrower (signature verification on tool registration, not caller
  authorization)
- `2026-04-24-security-audit-logging-missing.md` — prerequisite to
  detect if/when token abuse occurs
- `2026-04-24-resource-limits-not-enforced.md` — another form of "per
  caller" limit that also waits for caller identity

## Related docs

- `docs/archive/design.md` §3.6 (explicit deferral)
- `docs/protocol/error-codes.md` (`AtdError::CapabilityDenied` variant —
  currently unreachable but defined)
