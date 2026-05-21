# ATD Roadmap — Evolution Scope

**Scope:** This document defines ATD's *evolution scope* — where the
protocol and reference implementation are heading. It is the companion
to [`architecture.md`](architecture.md), which describes the system *as
it stands today*.

**Why this document exists.** ATD's 1.0 acceptance bar requires that an
agent can build "consistent extensions within ATD's design **and
evolution scope**." [`architecture.md`](architecture.md) §9.3 and
[`extending/`](extending/) cover the design scope — the `pub` traits and
how to attach to them. This file covers the *evolution* scope: the
features ATD has deliberately deferred, the work that has been designed
but not yet built, the limitations 1.0 ships with, and the principle by
which a 2.0 will eventually break the wire. An extension is "consistent"
when it neither contradicts a deferred decision nor pre-empts a known
direction.

**Authority.** This is a **Context**-tier document (see
[`index.md`](index.md)). It is not normative — when it disagrees with
[`architecture.md`](architecture.md) or the wire spec, the higher tier
wins. Deferred work moves onto the roadmap only on a concrete adopter
signal, recorded as an issue in [`issues/`](issues/) or an ADR in
[`adr/`](adr/).

---

## Table of contents

1. [Deferred features](#1-deferred-features)
2. [Designed but unimplemented](#2-designed-but-unimplemented)
3. [Known limitations at 1.0](#3-known-limitations-at-10)
4. [Post-1.0 direction](#4-post-10-direction)

---

## 1. Deferred features

These are [`architecture.md`](architecture.md) §10 non-goals plus §5.7.
Each is *intentionally* absent — neither shipped nor an extension point.
**The bar to add any of them is a concrete adopter need**, not
aspiration. An extension that quietly assumes one of these exists is
*not* consistent with ATD's scope.

| Feature | What it is | Why deferred — bar to add |
|---|---|---|
| **Multi-device routing** (§10.1) | Routing a call to "whichever device the user is on right now" instead of one fixed socket. | An agent-framework concern. ATD gives every device a clean endpoint and stops. Bar: an adopter whose dispatch genuinely cannot be expressed as one-endpoint-per-connection. |
| **Distributed sessions** (§10.2, §5.7) | Migrating a session across processes, forking it for parallel exploration, handing it off across hosts. | A session scopes to one connection. The design surface (state scope, wire mechanism, idempotency, concurrency) is wide; guessing it now risks a wire shape no adopter wants. Bar: an adopter with a real migrate/fork/handoff requirement to design against. |
| **Tool signature verification** (§10.3) | Cryptographically verifying `ToolTrust::signature` against a publisher key. | Requires PKI the protocol does not specify — publisher keys, rotation, revocation. Bar: an adopter shipping a real signing pipeline. The wire shape (`signature: Option<String>`) is already reserved, so verification can be added without a wire break. |
| **REST / AppFunction / distributed bindings** (§10.4) | `Binding` impls beyond `NativeBinding` and `CliBinding` — HTTP REST, platform App Intents, gRPC. | The `Binding` trait can host any of these; the reference impl blesses none. Bar: an adopter implementing the trait against a real REST or platform surface. The trait is `pub` and stable — this is a no-fork extension, just not one ATD ships. |
| **Per-tool dry-run preview** (§10.6) | Routing `dry_run: true` into tools whose `ToolSafety::dry_run` is `true` so they produce a tool-specific preview. | v1's dry-run is a server-side short-circuit (synthetic `tool_result`, no tool invoked). Per-tool preview is a richer contract. Bar: an adopter with tools that have a meaningful preview path worth wiring. See [`protocol/dry-run-contract.md`](protocol/dry-run-contract.md). |
| **Per-tool rate-limiter enforcement** (§10.7) | Token-bucket enforcement of `ToolResources::rate_limit_per_min`. | The `max_concurrent` axis *is* enforced (per-tool semaphore); adding a `governor`-style rate limiter is straightforward but unbuilt. Bar: an adopter that needs rate-per-minute, not just concurrency, capped. |

Two further non-goals are structural, not feature-shaped, and will not
move onto the roadmap: **native Skills-layer support** (§10.5 — ATD
provides primitives the Skills runtime consumes; SKILL.md parsing lives
in the runtime, not here) and **adding `BindingProtocol` / `ToolTier`
enum variants** (§9.3 — a wire change, gated to 2.0; see §4).

---

## 2. Designed but unimplemented

Three SP specs were written to full design depth but never reached
implementation. They are archived under
[`archive/superpowers/specs/`](archive/superpowers/specs/) — frozen
history, not a live workflow. Each remains a *candidate future
direction*: the design thinking is sound, but landing it still needs an
adopter signal and a fresh plan against the 1.0 crate layout.

### 2.1 Agent identity — `did:agent` + binary fingerprint VCs

A cross-vendor identity layer above today's free-form, unverified
`Hello.client_id`. The spec proposes a `did:agent:<vendor>:<instance>`
DID method and a `BinaryFingerprint` W3C VC claim, so a regulated tool
server could gate on *which vendor signed* and *which build is running*
— not just a self-asserted string. ATD's only normative contribution
would be a `DidResolver` trait in `atd-runtime` (mirroring `TokenBroker`
— single async method, default `did:key` impl, dependency-injected);
the DID method itself is positioned as an inter-vendor working-group
deliverable, not ATD-owned. No wire change.

→ [`archive/superpowers/specs/2026-05-11-sp-agent-identity-design.md`](archive/superpowers/specs/2026-05-11-sp-agent-identity-design.md)

### 2.2 Secret bootstrap — parent-child secret injection

A startup-time secret transport, orthogonal to `TokenBroker`. Where the
broker answers "how does a *running* server get the right secret for an
*inbound caller*," secret-bootstrap answers "how does a *parent
process* hand a freshly-spawned child server its own bootstrap secret
(a DEK, an OAuth refresh token, a client cert) without it touching
argv, env, or disk." The spec generalises a pattern already shipped in
an adopter — a `0600` Unix-socket, one-round-trip newline-JSON
handshake — into an `atd-runtime::secret_bootstrap` module with
`client` / `server` submodules and a typed `SecretPayload` trait.
Unix-only in the proposed v1.

→ [`archive/superpowers/specs/2026-05-11-sp-secret-bootstrap-design.md`](archive/superpowers/specs/2026-05-11-sp-secret-bootstrap-design.md)

### 2.3 Streamable HTTP

> **Note:** this spec's *core* — the `atd-server-http` crate, MCP
> JSON-RPC translation, bearer auth, origin gate — *did* ship (SP-1.B /
> SP-token-broker-phase2; see [`CHANGELOG.md`](../CHANGELOG.md) 0.3.0).
> What remains unimplemented is the spec's explicitly-deferred §9 tail:
> **`Mcp-Session-Id` sticky sessions** (the header is parsed and logged
> but not load-bearing), **resumability** via `Last-Event-ID` replay,
> in-listener **TLS termination**, and **OAuth 2.1 token issuance**
> (bearer is validated, never minted). The URL space and header names
> for sessions are reserved so a future SP can add them additively.

→ [`archive/superpowers/specs/2026-05-11-sp-streamable-http-design.md`](archive/superpowers/specs/2026-05-11-sp-streamable-http-design.md)

---

## 3. Known limitations at 1.0

1.0 is a stable, production-adopted release — but a few surfaces are
narrower than their type signatures imply. These are *acknowledged*
limitations: extensions should not assume the richer behaviour. Each is
tracked in [`issues/`](issues/).

| Limitation | Detail | Tracked |
|---|---|---|
| **`rate_limit_per_min` declarative-only** | `ToolResources::rate_limit_per_min` is part of the schema and every tool declares it, but no code path enforces it. `max_concurrent` *is* enforced via per-tool semaphores. | [`issues/2026-04-24-resource-limits-not-enforced.md`](issues/2026-04-24-resource-limits-not-enforced.md) |
| **Single-binding routing** | A `ToolDefinition` carries `Vec<ToolBinding>` and `CallOptions::preferred_binding` exists on the wire, but dispatch always routes to the *first* declared binding and never reads `preferred_binding`. Multi-binding selection is a small dispatcher upgrade gated on real multi-binding tools landing. | [`issues/2026-04-24-dispatch-binding-single-impl.md`](issues/2026-04-24-dispatch-binding-single-impl.md) · [`...preferred-binding-ignored.md`](issues/2026-04-24-dispatch-preferred-binding-ignored.md) |
| **Tool signature declarative-only** | `ToolTrust::signature` and `TrustLevel` are descriptive metadata. The runtime never verifies the signature against a publisher key — trust level is honor-system. | [`issues/2026-04-24-security-trust-signature-unverified.md`](issues/2026-04-24-security-trust-signature-unverified.md) |
| **Dry-run is server-side short-circuit only** | `dry_run: true` is honored by the dispatcher (synthetic `tool_result`, tool never invoked) — uniformly safe. But it does *not* route into a tool's own preview path; `ToolSafety::dry_run` advertises a capability nothing consumes yet. | [`issues/2026-04-24-security-dry-run-inconsistent.md`](issues/2026-04-24-security-dry-run-inconsistent.md) |
| **No session / cancel** | There is no `session()` or `cancel()` in the SDK and no server-side session state machine — a connection *is* the session. | [`issues/2026-04-24-dispatch-session-cancel-not-implemented.md`](issues/2026-04-24-dispatch-session-cancel-not-implemented.md) |
| **Python types hand-ported** | `python/src/atd_client/types.py` is hand-written, not generated from `atd-protocol-schema.json`. Drift is caught only by integration tests against the Rust server, not by a generator gate. Switching to schema-generated types is post-1.0 work. | tracked via [`architecture.md`](architecture.md) §2.4 |

None of these is a *bug* — each is a deliberate stop-line at 1.0. They
are listed so an extension author knows the real edge of the
implemented surface.

---

## 4. Post-1.0 direction

### 4.1 The wire-stability rule

**The 1.x line freezes the wire.** Any change that an existing 1.0
client could not deserialize — removing a field, reshaping a type,
removing an enum variant, repurposing an error code — waits for **2.0**.
The full contract is in
[`release-plan-v1.0.md`](release-plan-v1.0.md); the one-line summary:

- **Additive** (new optional field, new enum variant, new error code,
  new tool, new `pub` trait): ships in a **1.x minor**.
- **Wire-breaking** (removal / reshape / semantic repurpose): waits for
  **2.0**.

This is what makes the deferred work in §1 and §2 safe to defer:
because all of it was designed to be *additive* (a new trait, a new
optional field, a new capability), none of it forces a major bump. An
adopter can build on 1.0 today and pick up agent identity, secret
bootstrap, or per-tool dry-run as minor upgrades if and when they land.

### 4.2 What a 2.0 might bring

2.0 is the release where ATD is allowed to *break* the wire to pay down
debt the 1.x freeze accumulates. No 2.0 is planned or scheduled; this is
the shape it would take if adopter pressure justified it:

- **Multi-binding dispatch as a first-class contract** — making
  `preferred_binding` load-bearing and possibly reshaping `ToolBinding`
  selection, rather than the first-binding-wins rule of 1.x.
- **A `BindingProtocol` / `ToolTier` enum expansion** — new variants
  are wire-breaking for strict deserializers, so they batch into a major.
- **Distributed sessions** — if §1's session work lands, a session
  identity on the wire is a structural addition large enough to
  reconsider the envelope.
- **Per-crate independent versioning** — 1.x ships workspace-lockstep
  (every crate at one version); 2.0 is the natural point to revisit
  whether stable crates can version independently. See
  [`release-plan-v1.0.md`](release-plan-v1.0.md) §8.
- **Schema reshape** — any field rename or removal in
  `atd-protocol-schema.json` that 1.x's additive-only rule forbade.

A 2.0 would ship its own roadmap and release plan; this section only
fixes the *principle* — debt waits, and it waits in one place.

---

## See also

- [`architecture.md`](architecture.md) — the normative architecture;
  §10 is the non-goals source for §1 here.
- [`release-plan-v1.0.md`](release-plan-v1.0.md) — the 1.0 stability
  contract and the wire-freeze rule §4.1 summarises.
- [`index.md`](index.md) — the documentation map and authority tiers.
- [`CHANGELOG.md`](../CHANGELOG.md) — what shipped when.
- [`issues/`](issues/) — the tracked-gap log; §3's limitations link here.
- [`adr/`](adr/) — the live decision log; deferred work that moves onto
  the roadmap gets an ADR.
- [`archive/superpowers/specs/`](archive/superpowers/specs/) — the
  frozen SP design archive; §2's three candidate directions live here.
