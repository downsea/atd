# SP-12 — Canonical Dispatch Demo Design Spec

**Date:** 2026-04-25
**Status:** Draft; review pending before plan.
**Scope:** Extend `atd-ref-server` with the four minimal dispatch primitives required for atd-mvp to be a credible "reference implementation of the ATD protocol" rather than a client-SDK demo with a hashmap backend. Primitives: binding abstraction, capability allow-list gate, result-middleware pipeline, tier-aware deadlines. No v3 distributed features, no cryptographic tokens, no multi-device routing.
**Builds on:** `sp11-docs` — 9 docs shipped, 252 workspace tests green, ref-server at `crates/atd-ref-server/` with 9 native tools.
**Related:**
- `docs/design.md` §0 (protocol independence), §2.1 (layering — currently delegates dispatch to ANOS)
- `docs/whitepaper/atd-v3-skills-architecture-brief.md` §Slide 1 (the layer picture SP-12 partially materializes)
- `docs/whitepaper/atd-v3-multi-device.md` (v3 vision; explicitly **not** in scope)

---

## 1. Motivation

### 1.1 The gap between the README and the code

The project README positions atd-mvp as **"the reference implementation of the Agent Tool Dispatch (ATD) protocol."** The v3 brief (Slide 1) draws five layers between an agent and a tool: Agent → Skill → Client SDK → **Dispatch** → Bindings → Tools. SP-1 through SP-11 shipped four of those layers (Agent adapters, Client SDK, Bindings at the wire level, Tools). One layer is conspicuously absent from the code:

- `crates/atd-ref-server/src/server.rs:103` is the entire dispatch path: `registry.get(tool_id).call(args, &ctx)`. A `HashMap` lookup followed by a direct trait-method call.

A reader who approaches the codebase through the ATD-as-POSIX framing (design.md §0) expects dispatch to include, at minimum: a **binding** layer (tools are plural representations; dispatch resolves one), a **capability** check (§VI least-privilege in the Skills brief), some **result-pipeline** hook (middleware), and a **tier** signal that is actually used (not just a field in `ToolSummary`). None of these exist today. The result: the layer diagrams in `docs/whitepaper/` cannot be pointed to in source, and the "ATD = POSIX / Skill = Python stdlib" analogy loses weight — a POSIX reference without VFS, capabilities, or schedulers would not be called a reference.

### 1.2 Why not ship the v3 brief in full

v3 introduces device affinity across seven device classes, UCAN-attenuated capability tokens, distributed session migrate/fork/handoff, and a five-builtin middleware chain. Those belong to a **distributed-OS** design surface (more Plan 9 than POSIX) and require real hardware, real cryptography, and a multi-node testbed to implement honestly. Forcing them into Phase 0/1 would blow past the design.md §1.1 exit signals and lock in semantics the v3 brief itself labels **"非规范性概览"** (`atd-v3-skills-architecture-brief.md:244`).

### 1.3 The fix — canonical, minimal, observable

Ship four dispatch primitives, each chosen because:

1. It is a **structural** element the v3 architecture requires (so the code matches the diagrams), and
2. It can be demonstrated with a **single working example** in under ~200 lines, and
3. Its MVP form is forward-compatible with the v3 shape (a future SP can swap the allow-list for UCAN, the single middleware for a chain, the single binding for a matrix — no user-visible API break).

| # | Primitive | MVP form | v3 target (deferred) |
|---|----------|----------|----------------------|
| 1 | **Binding selection** | `Binding` trait; two impls (`NativeBinding`, `CliBinding`); one tool routed through `CliBinding` | Full matrix per §Slide 1 (CLI · MCP · REST · AppFunction · Distributed) |
| 2 | **Capability gate** | Connection-scoped allow-list declared via server CLI; each tool declares required capabilities; dispatch refuses on mismatch with a typed error | UCAN tokens, attenuation, revocation, audit log |
| 3 | **Result middleware** | `Middleware` trait; a `Vec<Arc<dyn Middleware>>` applied post-`tool.call`; one built-in `redact_paths` demonstrating the shape | The five builtins named in brief §2.7 + user-pluggable chain |
| 4 | **Tier-aware dispatch** | Three tiers (`Hot` / `Warm` / `Cold`); deadline and max-output defaults derived from tier; the `tier` field on `ToolDefinition` becomes load-bearing instead of decorative | H/W/C tier as routing + placement signal across devices |

These four together cover the brief's Slide 1 "Dispatch Layer" box — **structurally**. They do not cover what that box does in a multi-device world.

---

## 2. Scope

### 2.1 In scope

**Code (all in `crates/atd-ref-server/`):**
- New `binding.rs` — `Binding` trait + `NativeBinding` (wraps a `Tool` impl) + `CliBinding` (spawns a subprocess, marshals stdin/stdout JSON)
- New `capability.rs` — `Capability(String)` newtype; `CapabilitySet` on the connection; `required_capabilities()` on `ToolDefinition`; denial error path
- New `middleware.rs` — `trait Middleware { fn on_result(&self, tool_id: &str, result: &mut Value); }`; `RedactPathsMiddleware` built-in; ordered registration
- New `tier.rs` — `Tier` enum derived from `ToolSummary.tier`; `TierPolicy` (per-tier timeout/max-output defaults, overridable by CLI)
- Modifications to `registry.rs`: `Tool` trait gets an associated `Binding` instead of direct `call`; registration records the binding choice
- Modifications to `context.rs`: `CallContext` gains `capabilities: Arc<CapabilitySet>` and `tier: Tier`
- Modifications to `server.rs`: capability check before tool invocation; middleware chain after tool returns success; tier-derived deadline passed into `CallContext`
- One new tool demonstrating `CliBinding`: `ref:external.uname` (shells out to `/usr/bin/uname`) — deliberately trivial so the binding scaffolding is what's on display, not the tool logic
- New `Request::Hello { client_id: Option<String>, requested_capabilities: Vec<String> }` and `Response::HelloAck { granted: Vec<String>, server_version: String }` wire messages (additive; existing clients that skip `Hello` get the default empty capability set)
- CLI flags on `atd-ref-server`: `--grant-capability <name>` (repeatable), `--tier-override <tier>=<key>=<value>` (e.g. `hot=timeout_ms=500`), `--middleware <name>` (repeatable; defaults to `redact_paths`)
- `AtdClient::hello()` in `crates/atd-client/` (Rust) and Python `atd_client` — optional; callers that omit it still work but get no capabilities
- `AtdError::CapabilityDenied { tool_id, required, granted }` — already reserved in `design.md §3.5` enum; this SP wires it to a real trigger
- Tests: per-primitive unit tests + one end-to-end integration test per primitive + one cross-primitive test ("connection with `read` capability, calling `ref:fs.read` via `NativeBinding`, with `redact_paths` middleware on a `Warm`-tier tool")
- README additions: "Dispatch layer" section with four subsections, each pointing at the corresponding module
- `docs/design.md` §2.1 updated: the "Dispatch Core = ANOS daemon" line is replaced with "Canonical dispatch shipped in atd-ref-server; v3 distributed dispatch remains Phase 2+"

### 2.2 Explicitly out of scope

- **Device affinity.** No `device.preferred`, no device_id in the protocol, no multi-device tests. `Binding` does **not** include a "pick a device" step.
- **Cryptographic capability tokens.** No UCAN, no signing, no attenuation chain, no revocation list. Capabilities are trusted strings declared by the server operator at start time. A future SP can replace the allow-list with token verification without changing the `CapabilitySet` surface.
- **Distributed sessions.** No `session.start`, no migrate/fork/handoff. `session` remains absent from the wire (already a design.md §1.2 non-goal).
- **Full middleware suite.** Of the five brief builtins (`pii_redact`, `source_device_tag`, `compress`, `audit_log`, `rate_shape`), SP-12 ships **one**: `redact_paths`. The trait surface is the demonstration; the suite belongs to a future SP or to external implementers.
- **MCP-side propagation of capabilities or tiers.** `atd-mcp-bridge` gains no new fields; capability gating is ATD-native only. (The bridge continues to work because it does not issue `Hello` and thus gets the default — but tools requiring capabilities will refuse, and the bridge's README will gain a one-paragraph note on this.)
- **Schema migration of `ToolDefinition`.** The `required_capabilities` and `tier` fields already exist on `ToolSummary` / `ToolDefinition`. SP-12 uses them; it does not redesign them.
- **AppFunction / REST bindings.** Only `Native` and `Cli` bindings are implemented.
- **Rewriting existing tools** to route through `CliBinding`. The 9 existing tools stay on `NativeBinding`. One **new** tool (`ref:external.uname`) demonstrates `CliBinding`.
- **A conformance test suite** for third-party dispatch implementations. That is a Phase 2 concern (`docs/design.md §1.2`).

### 2.3 Prerequisites

- `sp11-docs` tag; 252 workspace tests green.
- `atd-types` `ToolDefinition` already carries `tier`, `trust`, `capability` — confirmed in `crates/atd-types/src/tool.rs`.
- `ToolSummary.tier` already flows through `discover` — confirmed in `crates/atd-client/src/client.rs:131`.
- No changes needed to `atd-mcp-bridge`, `atd-cli`, or Python adapters beyond the optional `hello()` surface in the client.

---

## 3. Architecture

### 3.1 Module layout (after SP-12)

```
crates/atd-ref-server/
├── src/
│   ├── binding.rs           # NEW — Binding trait, NativeBinding, CliBinding (~180 LoC)
│   ├── capability.rs        # NEW — Capability, CapabilitySet, denial helpers (~120 LoC)
│   ├── middleware.rs        # NEW — Middleware trait, RedactPathsMiddleware (~150 LoC)
│   ├── tier.rs              # NEW — Tier enum, TierPolicy, CLI parsing (~130 LoC)
│   ├── context.rs           # MOD — CallContext gains capabilities + tier
│   ├── protocol.rs          # MOD — adds Hello / HelloAck variants
│   ├── registry.rs          # MOD — Tool trait: associated binding instead of direct call
│   ├── server.rs            # MOD — dispatch calls capability check → binding → middleware
│   ├── tools/
│   │   └── external/
│   │       └── uname.rs     # NEW — ref:external.uname, routed via CliBinding
│   └── ...
```

All modules stay well under the project's 400-line cohesion ceiling (per `rules/common/coding-style.md`).

### 3.2 Primitive 1 — Binding abstraction

**Trait (sketch):**

```rust
// crates/atd-ref-server/src/binding.rs
pub trait Binding: Send + Sync {
    fn name(&self) -> &'static str;
    fn call<'a>(
        &'a self,
        tool_def: &'a ToolDefinition,
        args: Value,
        ctx: &'a CallContext,
    ) -> CallFuture<'a>;
}

pub struct NativeBinding<T: Tool>(pub Arc<T>);
pub struct CliBinding {
    pub program: PathBuf,
    pub base_args: Vec<String>,
}
```

`Registry::register` is overloaded (or: a second constructor `register_with_binding`) so existing tools continue to register as `NativeBinding` implicitly.

**`ref:external.uname`** — the only tool routed through `CliBinding`:

```
ref:external.uname
  binding:   Cli
  program:   /usr/bin/uname
  args map:  { "flag": "-s" } → ["-s"]
  output:    { "stdout": "Linux\n" }
```

Why `uname`: it is universally available on Linux CI runners, produces deterministic output, needs no sandboxing, and the input is a single enum-like flag — so the spec stays about **the binding**, not the tool.

**Why this is the minimum:** dispatch now makes a visible **choice** ("which binding executes this tool"). Tools are no longer synonymous with in-process functions. The `bindings: Vec<BindingSpec>` field that already exists on `ToolDefinition` (see `crates/atd-client/src/client.rs:373`) becomes load-bearing.

### 3.3 Primitive 2 — Capability allow-list gate

**Wire:** `Hello` request on connect declares requested capabilities (free-form strings). Server intersects with its own `--grant-capability` set and returns the granted subset in `HelloAck`. Connection-scoped; no per-call token.

**Tool declaration:** `ToolDefinition` already carries `capability: CapabilityHint`. SP-12 adds a **new**, separate `required_capabilities: Vec<String>` field on `ToolDefinition`. Naming rationale: the existing `capability` field is a **descriptor** (domain, actions, intent examples); `required_capabilities` is an **enforcement** list. Keeping them separate avoids overloading the schema.

**Enforcement point:** in `server.rs:dispatch`, immediately before binding selection. If the required set is not a subset of the granted set, respond with `Response::Error { code: CAPABILITY_DENIED, ... }` carrying `{ required, granted }` for the client to surface through `AtdError::CapabilityDenied`.

**Default policy:** `ref-server` with no `--grant-capability` flag grants nothing. Tools with empty `required_capabilities` are always callable (so all 9 existing tools work unchanged — they declare no required capabilities in this SP; a later SP can opt them in).

**Why this is the minimum:** the §VI least-privilege hook point exists in the code and the wire. A future SP can swap the allow-list for UCAN without changing `required_capabilities`, without changing the error type, and without changing the `Hello`/`HelloAck` shape — only the **verifier** inside `capability.rs` changes.

### 3.4 Primitive 3 — Result-middleware pipeline

**Trait:**

```rust
// crates/atd-ref-server/src/middleware.rs
pub trait Middleware: Send + Sync {
    fn name(&self) -> &'static str;
    fn on_result(
        &self,
        tool_id: &str,
        tool_def: &ToolDefinition,
        result: &mut serde_json::Value,
    );
}
```

**Registration:** `Server::new(...)` takes `Vec<Arc<dyn Middleware>>`. Order matters (first registered runs first). Chain runs only on success; error results bypass middleware.

**Built-in: `RedactPathsMiddleware`.** Walks the result JSON; for every string field matching a regex (default: absolute paths under `$HOME`), replaces with `"<redacted:home>"`. Configurable via CLI (`--redact-pattern <regex>=<replacement>`, repeatable). This mirrors the brief's `pii_redact` at a shape level without committing to a specific PII taxonomy.

**Why this is the minimum:** the chain exists; the order is deterministic; a single built-in proves the shape end-to-end. Future middleware (audit log, rate shape) is a matter of adding types, not re-architecting.

### 3.5 Primitive 4 — Tier-aware dispatch

**Tiers:** `Hot` (target latency budget 500 ms, max output 64 KiB), `Warm` (5 s, 1 MiB — current default), `Cold` (60 s, 16 MiB). Numbers are defaults; each is overridable via `--tier-override <tier>=<key>=<value>` (e.g. `--tier-override hot=timeout_ms=300`).

**Derivation:** each tool's `ToolDefinition.tier` (already present) is the single input. If the field is absent or unparseable, tier defaults to `Warm` — matching current behavior.

**Wiring:** before calling the binding, `server.rs` computes the deadline as `Instant::now() + tier_policy.timeout(tier)` and puts it on `CallContext`. `max_output_bytes` on `CallContext` is likewise tier-derived. Both become tier-sensitive instead of globally-configured.

**Why this is the minimum:** the `tier` field is no longer decorative; running the same tool at different tiers produces observably different timeouts. A future SP can expand tier meaning (placement, priority) without changing the field on the definition.

### 3.6 Cross-primitive call flow (the new dispatch path)

```
handle_connection
 └─ (1) read Hello        → CapabilitySet (Arc, per-connection)
 └─ (2) read RunTool
     └─ lookup tool_def in registry (as today)
     └─ [NEW] check required_capabilities ⊆ granted → else CapabilityDenied
     └─ [NEW] derive tier → TierPolicy → deadline, max_output_bytes
     └─ build CallContext (now includes capabilities + tier)
     └─ [NEW] binding = registry.binding_for(tool_id)
     └─ result = binding.call(tool_def, args, &ctx).await
     └─ [NEW] if success: for mw in middleware: mw.on_result(..., &mut result)
     └─ write Response::ToolResult
```

Five insertion points (marked `[NEW]`). Each is a single line in `server.rs` plus its module.

---

## 4. Wire-protocol impact

### 4.1 New message variants (additive)

```rust
// crates/atd-ref-server/src/protocol.rs (additions)
Request::Hello {
    client_id: Option<String>,                // free-form; server logs it
    requested_capabilities: Vec<String>,
}

Response::HelloAck {
    granted_capabilities: Vec<String>,
    server_version: String,                   // e.g. "atd-ref-server 0.2.0"
    supported_tiers: Vec<String>,             // ["hot","warm","cold"]
}
```

**Backward compatibility:** `Hello` is optional. A connection that omits it gets an empty `CapabilitySet`. Clients pinned to SP-11 or earlier continue to work against SP-12 servers; they simply cannot call tools that require capabilities (but no existing tool does — see §3.3 default policy).

### 4.2 New error code

`Response::Error.code = CAPABILITY_DENIED` (integer TBD during plan; slot in the existing `u16` space). `AtdError::CapabilityDenied { required, granted }` deserializes from `details`.

### 4.3 No changes to

- `Ping` / `Pong`
- `ToolList` / `ToolSchema` response shapes
- `RunTool` request (args shape, `dry_run`)
- `ToolResult` response (middleware rewrites `result` in place before it goes on the wire; no new fields)

### 4.4 MCP bridge impact

None — the bridge continues to proxy `tools/list` and `tools/call`. Tools with empty `required_capabilities` (all 9 existing + `ref:external.uname`) remain callable via the bridge. A short note will be added to `crates/atd-mcp-bridge/README.md` explaining that capability-gated tools require a direct ATD client.

---

## 5. Test strategy

### 5.1 Unit tests (one module, one test module)

- `binding.rs`: `NativeBinding` calls underlying tool; `CliBinding` spawns, marshals, times out on a deadline; `CliBinding` surfaces non-zero exit as `ToolCallError::ExecutionFailed`.
- `capability.rs`: empty required ⊆ empty granted (pass); non-empty required not ⊆ granted (deny); deny error carries `required` and `granted` verbatim.
- `middleware.rs`: `RedactPathsMiddleware` rewrites absolute `$HOME` paths in string fields, leaves other fields unchanged, handles nested objects and arrays, is a no-op on non-string leaves.
- `tier.rs`: parsing `"hot"` / `"warm"` / `"cold"` / `null` → correct tier; `TierPolicy` overrides merge correctly; default preserves current 1 MiB / 60 s behavior when tier is absent (compat).

### 5.2 Integration tests (in `crates/atd-ref-server/tests/`)

1. **`dispatch_capability_denied_path.rs`** — connect, send `Hello { requested: [] }`, attempt to call a tool declaring `required_capabilities: ["exec"]` → expect `CAPABILITY_DENIED` with both sets populated.
2. **`dispatch_capability_granted_path.rs`** — server started with `--grant-capability exec`; client requests `["exec"]`; call proceeds.
3. **`dispatch_cli_binding_uname.rs`** — `ref:external.uname` returns `"Linux"` on CI, round-trips through wire.
4. **`dispatch_middleware_redacts_home.rs`** — result containing an absolute `$HOME` path comes back redacted.
5. **`dispatch_tier_hot_deadline.rs`** — tool declared `tier: hot`; server overridden to `hot=timeout_ms=100`; tool that sleeps 500 ms times out; warm-tier equivalent succeeds.
6. **`dispatch_end_to_end.rs`** — capability gate → tier deadline → binding → middleware, in one call, asserting all four primitives participated (observable via server logs + result contents).

### 5.3 Non-goals for testing

- No MCP bridge round-trip tests (unchanged surface).
- No cross-language tests (Python client gets its own follow-up SP; Rust E2E is sufficient to prove SP-12).
- No fuzz/soak tests (Phase 2).

---

## 6. Migration & compatibility

### 6.1 Within atd-mvp

- The 9 existing tools declare `required_capabilities: []` → no behavior change.
- `atd-cli`, `atd-mcp-bridge`, `hello_atd` example, `hello_langchain` examples: unchanged.
- Python `atd_client`: `hello()` added as a non-required call (default: do nothing).
- Rust `atd-client`: `hello()` added as a non-required call. The existing auto-`ping` on connect (`crates/atd-client/src/client.rs:41`) remains; `hello()` is separate and explicit.

### 6.2 For downstream consumers

- External ATD servers that pre-date SP-12: interoperate with SP-12 clients (clients that call `hello()` on a pre-SP-12 server get a `ProtocolError` on the Hello frame; this is trapped and demoted to "capability set = empty", preserving the SP-11 behavior). Captured in `client.rs` with a comment.
- External ATD clients that pre-date SP-12: interoperate with SP-12 servers (the server treats the absence of `Hello` as "no capabilities", matching the default grant).

### 6.3 Docs updates (inside SP-12)

- `README.md`: "Architecture at a glance" diagram gains a `Dispatch` box between `atd-client` and `ATD server`. Four bullets under it.
- `docs/design.md §2.1`: layering diagram updated; "Dispatch Core = ANOS daemon" line replaced with "Canonical dispatch shipped in atd-ref-server; v3 distributed semantics deferred to Phase 2+."
- `docs/design.md §1.2`: `CapabilityDenied` moved from "Phase 2 enforced" to "SP-12 enforced (allow-list); Phase 2 enforced (UCAN)".
- `docs/protocol/wire-format.md`: new §"Hello handshake" and §"Capability denial error".
- `crates/atd-mcp-bridge/README.md`: one-paragraph note that capability-gated tools require a direct ATD client.

---

## 7. Non-goals — explicit v3 deferrals

Called out separately because readers of the v3 brief will expect these:

| v3 feature | Why deferred | Where it lives instead |
|-----------|-------------|------------------------|
| `device.preferred = [watch, phone, ...]` routing | Requires multi-device testbed; no production signal yet | Whitepaper §2.5; future SP tied to hardware availability |
| UCAN token attenuation | Requires key management + crypto audit; out of Phase 0/1 scope | Capability allow-list is the forward-compatible placeholder |
| `session.migrate` / `session.fork` / `session.handoff` | Requires multi-node atd-ref-server fleet | Whitepaper §2.6 |
| Full five-middleware suite (`pii_redact`, `source_device_tag`, `compress`, `audit_log`, `rate_shape`) | Each is a domain problem; shape is proven by one | Future SP per middleware or external implementers |
| AppFunction / REST bindings | Requires real device SDKs or HTTP plumbing | Future SP; `Binding` trait is ready |
| Ergonomic DSL (§2.8, Appendix J) | Depends on all the above being stable | Future SP |

SP-12 does **not** deprecate or rename any v3-brief concept. It ships the **smallest forward-compatible subset** of each.

---

## 8. Open questions (for review before plan)

1. **Error code allocation.** Existing `Response::Error.code: Option<u16>` is sparsely assigned. Should SP-12 introduce a `AtdErrorCode` enum in `atd-types` now, or keep ad-hoc `u16` values until a dedicated error-code SP? *Leaning toward ad-hoc; enum-ification is its own refactor.*
2. **`Hello` vs. piggyback-on-`Ping`.** A variant is to add `requested_capabilities` to `Ping` rather than introduce `Hello`. Trade-off: `Ping` becomes non-trivial (losing its "heartbeat-only" semantics). *Leaning toward a separate `Hello`; `Ping` should remain a liveness check.*
3. **Should `ref:external.uname` live in `builtin_registry`?** It depends on `/usr/bin/uname` existing. CI on `ubuntu-latest` has it; Windows runners do not. *Leaning toward gating registration behind `#[cfg(unix)]` and skipping the integration test on Windows CI.*
4. **Middleware ordering on errors.** Current design: middleware runs only on success. Should `audit_log`-style middleware see errors too? *Leaning toward "yes, eventually, via a separate `on_error` hook"; SP-12 stays success-only to avoid designing that hook before a concrete consumer exists.*
5. **Tier default when `tool_def.tier` is absent.** Currently SP-11 tools carry no tier. Parsing as `Warm` preserves behavior; parsing as a hard error is safer for spec compliance. *Leaning toward `Warm` (compat); flip to error in a future SP once all builtin tools opt in.*

---

## 9. Out of this spec, into a follow-up

- Porting the 9 existing tools to declare realistic `required_capabilities` and `tier` values.
- A conformance suite runnable against any ATD server implementation (checks Hello, capability denial shape, tier semantics).
- Adding the four remaining v3 middlewares.
- Python / TypeScript client parity for `Hello` (Rust + Python stub only in SP-12).
- Multi-device dispatch (the full v3 brief).

---

**Summary.** Four primitives. One new tool. Six wire additions (two messages, one error code, three field touches). Five insertion points in `server.rs`. Six integration tests. The v3 diagrams become pointable-at-code. The "reference implementation" label becomes defensible. Everything that SP-12 does **not** do is explicitly listed so the v3 brief remains the forward plan, not a shipping promise.
