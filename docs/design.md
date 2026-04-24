# ATD Client SDK MVP — Design Spec

**Date:** 2026-04-21
**Status:** Approved design, pending implementation plan
**Owner:** ANOS project → future `atd-protocol` org
**Related whitepaper:** [`/home/nan/proj/anos/docs/research/toward-agent-tool-dispatch-v2.md`](/home/nan/proj/anos/docs/research/toward-agent-tool-dispatch-v2.md) (esp. §2.4, §7, Appendix G)

> **Note (2026-04-24):** This document is the original Phase 0 design spec from 2026-04-21. It has been **superseded by** [`docs/architecture.md`](architecture.md) as the normative architecture reference for the reference implementation. This file is retained for historical context — to understand the Phase 0 scoping decisions and the then-open questions (§10), read this doc. To understand the current architecture, crate layout, and evolution path, read `docs/architecture.md`.

---

## 0. Context and Independence

**atd-mvp is a new, independent codebase** at `/home/nan/proj/atd-mvp/`, separate from the ANOS workspace at `/home/nan/proj/anos/`. This reflects the ATD whitepaper's positioning as a neutral protocol — ATD cannot be credibly positioned as "the agent-era POSIX" while living inside one vendor's codebase.

**Relationship to ANOS:**
- ANOS at `/home/nan/proj/anos/` remains the **reference server** during Phase 0/1 (the daemon already implements the dispatch pipeline).
- `atd-mvp` pulls inspiration from `/home/nan/proj/anos/crates/anos-tool-dispatch/`, `/home/nan/proj/anos/crates/anos-cli/src/client.rs`, and `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs` — but reimplements the **protocol-level types** cleanly, without any `anos-*` crate dependency.
- CI must enforce: `atd-mvp` has **zero runtime dependency** on any `/home/nan/proj/anos/crates/*` crate. An `ANOS-free` test harness runs `atd-client` against a mock server to prove protocol independence.

## 1. Goals and Non-Goals

### 1.1 Goals (A + B from user)

| Priority | Goal | Exit signal |
|---------|------|------------|
| **A** | Technical validation — prove ATD protocol is usable by non-ANOS agents | One working demo: LangChain/Hermes/OpenClaw agent calls an ATD tool via atd-client, end to end |
| **B** | DX-first onboarding — 15 min install-to-first-call for agent framework developers | Third-party developer writes a 10-line example from `pip install` to working tool call, no support ticket |

### 1.2 Explicit Non-Goals (Phase 0/1)

- ❌ Skill runtime. `atd-client` does **not** parse SKILL.md, execute skill bodies, or manage progressive disclosure. Skill runtime is a future independent package (`atd-skill-runtime`), Phase 2+, likely a separate repo.
- ❌ SOUL.md / agent identity / personality injection.
- ❌ `subscribe` / event streaming — defer to Phase 2 (simplifies transport).
- ❌ HTTP/JSON transport — defer to Phase 2 (Unix socket + stdio covers Phase 0/1).
- ❌ AppFunction binding reference implementation — defer to Phase 2 (requires real hardware).
- ❌ Conformance test suite enforcement — Phase 2.
- ❌ v3 distributed dispatch — device affinity, UCAN token attenuation, session migrate/fork/handoff. **SP-12 ships the structural placeholders** (a single-node allow-list capability gate, a single `CliBinding`, a single `RedactPathsMiddleware`) so the four-layer v3 architecture is pointable-at-code; full v3 semantics remain Phase 2+.

## 2. Architecture

### 2.1 Layering

```
┌──────────────────────────────────────────────────────────┐
│  Agent code (LangChain / Hermes / OpenClaw / custom)     │
├──────────────────────────────────────────────────────────┤
│  ATD Client SDK  (Rust ref · Python · TypeScript)        │  ← §7 whitepaper
│  discover · describe · call · session · cancel           │
├──────────────────────────────────────────────────────────┤
│  Client-side Transport (pluggable)                        │
│  ┌──────────────┬─────────────┬─────────────────────────┐│
│  │ ATD-native   │ MCP-compat  │ HTTP/JSON (Phase 2)     ││
│  │ unix socket  │ stdio       │                         ││
│  │  or stdio    │             │                         ││
│  └──────────────┴─────────────┴─────────────────────────┘│
├──────────────────────────────────────────────────────────┤
│             ATD Server / Dispatch Core                   │
│   reference = atd-ref-server (SP-12): capability gate,   │
│   tier policy, binding selection, result-middleware.     │
│   v3 distributed dispatch (device affinity, UCAN,        │
│   session handoff) remains Phase 2+.                     │
├──────────────────────────────────────────────────────────┤
│  Server-side Bindings                                    │
│  ┌─────────┬─────┬──────┬──────┬────────────────────────┐│
│  │ Native  │ CLI │ MCP  │ REST │ AppFunction (Phase 2) ││
│  │ (SP-12) │ (12)│ (*)  │ (*)  │                       ││
│  └─────────┴─────┴──────┴──────┴────────────────────────┘│
│  (*) wire binding today; native Binding impl is Phase 2. │
└──────────────────────────────────────────────────────────┘
```

**Two orthogonal dimensions** (frequently confused):

| Dimension | Question answered | MVP scope |
|-----------|-------------------|-----------|
| **Client Transport** | How does the agent talk to the ATD server? | stdio / Unix socket (Phase 0), + MCP-compat (Phase 1), + HTTP (Phase 2) |
| **Server Bindings** | How is a tool actually implemented behind ATD? | CLI + MCP + REST (Phase 0 via ANOS), + AppFunction (Phase 2) |

### 2.2 Wire formats

**ATD-native** (stdio and Unix socket): length-prefixed JSON. Messages:

```
discover  { query?, filter?, limit? }            → [ToolSummary, ...]
describe  { tool_id }                            → ToolDefinition
call      { tool_id, args, options }             → ToolResult
session.start { name } / session.end { id }      → SessionHandle
cancel    { call_id }                            → Ack
ping                                             → Pong
```

Inherits the length-prefixed JSON format already used by ANOS IPC (see `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs`). Client SDK and ANOS daemon share this wire format in Phase 0 — ANOS daemon serves as the reference ATD server without any server-side changes.

**MCP-compat** (stdio only): full MCP `initialize` handshake. ATD tools exposed via `tools/list` and `tools/call`. ATD-specific fields (tier, session_affinity, capability_token) carried in a `_atd` extension object — MCP clients that don't understand ATD extensions continue to function.

## 3. API Surface (client SDK)

### 3.1 Rust reference

```rust
// /home/nan/proj/atd-mvp/crates/atd-client/src/lib.rs
use atd_types::{ToolSummary, ToolDefinition, ToolResult, DiscoverFilter, CallOptions};

pub struct AtdClient { /* ... */ }

impl AtdClient {
    pub async fn connect(endpoint: Endpoint) -> Result<Self, AtdError>;
    pub async fn discover(&self, query: Option<&str>, filter: DiscoverFilter)
        -> Result<Vec<ToolSummary>, AtdError>;
    pub async fn describe(&self, tool_id: &str) -> Result<ToolDefinition, AtdError>;
    pub async fn call(&self, tool_id: &str, args: Value, opts: CallOptions)
        -> Result<ToolResult, AtdError>;
    pub async fn session(&self, name: &str) -> Result<SessionHandle, AtdError>;
    pub async fn cancel(&self, call_id: &str) -> Result<(), AtdError>;
}

pub enum Endpoint {
    UnixSocket(PathBuf),
    Stdio { cmd: String, args: Vec<String> },
    // Http { url: Url, bearer: Option<String> },  // Phase 2
}
```

### 3.2 Python (idiomatic, kwargs-first)

```python
# /home/nan/proj/atd-mvp/python/atd_client/__init__.py
class AtdClient:
    @classmethod
    async def connect(cls, endpoint: str) -> "AtdClient": ...
    async def discover(self, query: str = None, **filter) -> list[ToolSummary]: ...
    async def describe(self, tool_id: str) -> ToolDefinition: ...
    async def call(self, tool_id: str, **args) -> ToolResult: ...
    async def session(self, name: str) -> SessionHandle: ...
    async def cancel(self, call_id: str) -> None: ...
```

Sync wrapper provided: `AtdClientSync` (for pre-async LangChain code).

### 3.3 TypeScript (generic return types)

```typescript
// /home/nan/proj/atd-mvp/typescript/src/client.ts
export class AtdClient {
    static async connect(endpoint: string | Endpoint): Promise<AtdClient>;
    discover(query?: string, filter?: DiscoverFilter): Promise<ToolSummary[]>;
    describe(toolId: string): Promise<ToolDefinition>;
    call<T = unknown>(toolId: string, args: object, opts?: CallOptions): Promise<ToolResult<T>>;
    session(name: string): Promise<SessionHandle>;
    cancel(callId: string): Promise<void>;
}
```

### 3.4 LLM-adapter helpers (DX-critical)

Each SDK ships a set of helpers that convert ATD tools to the shape expected by specific LLM providers:

```python
tools = await client.discover()
openai_tools    = await client.as_openai_tools(tools)      # OpenAI function format
anthropic_tools = await client.as_anthropic_tools(tools)   # Anthropic tool format
langchain_tools = await client.as_langchain_tools(tools)   # list[BaseTool]
```

Handles tool-name sanitization: `xiaomi:light.toggle` → `xiaomi_light_toggle` (LLM APIs require `[a-zA-Z0-9_-]`), with reverse mapping on call dispatch. Pattern borrowed from `/home/nan/proj/anos/crates/anos-llm-anthropic/src/provider.rs` (`sanitize_tool_name`).

### 3.5 Error model

```rust
// /home/nan/proj/atd-mvp/crates/atd-types/src/error.rs
pub enum AtdError {
    ToolNotFound { tool_id: String, suggestions: Vec<String> },
    InvalidArguments { tool_id: String, field: String, reason: String },
    CapabilityDenied { tool_id: String, required: Vec<String>, granted: Vec<String> },
    BindingUnavailable { tool_id: String, tried: Vec<String>, reason: String },
    ToolExecutionFailed { tool_id: String, inner: BoxedError },
    Timeout { tool_id: String, after_ms: u64 },
    ServerUnreachable(std::io::Error),
    NotImplemented { feature: String },
    ProtocolError { expected: String, got: String },
}

impl AtdError {
    pub fn is_retryable(&self) -> bool;
    pub fn suggest_fix(&self) -> Option<String>;
}
```

Every error includes a `suggest_fix()` returning an actionable hint (printed by CLI and SDK log). Mirrors the unified-error-class gap tracked in [`/home/nan/proj/anos/docs/issues/2026-04-21-atd-error-classification-not-unified.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-error-classification-not-unified.md). SDK layer defines the reference enum; ANOS server adoption follows in a later sprint.

### 3.6 Key API design decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| `discover()` returns summary or full schema? | Summary | Full schema scales poorly in large registries (H/W/C tier covers thousands) |
| Session: object or string id? | `SessionHandle` object | Clear lifetime, RAII close, type-safe reuse |
| `subscribe()` / events? | **Deferred** to Phase 2 | Bidirectional streaming complicates transport; unnecessary for A-goal proof |
| Capability token | Optional in Phase 0/1, enforced in Phase 2 | Don't block early adopters on security model; grow into it |
| `preferred_binding` | Exposed, opt-in | Debugging and testing need override; dispatch default is the norm |
| `dry_run` | Exposed, stubbed | Reference enum defined; ANOS server implementation tracked at [`/home/nan/proj/anos/docs/issues/2026-04-21-atd-dry-run-not-wired.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-dry-run-not-wired.md) |
| Async-only or + sync wrapper? | Both. `AtdClientSync` for Python/TS sync call sites | LangChain ecosystem still has many sync entry points |

## 4. Repository Layout (new, independent)

```
/home/nan/proj/atd-mvp/
├── Cargo.toml                          # workspace manifest (Rust)
├── crates/
│   ├── atd-types/                      # protocol types (no ANOS dependency)
│   │   └── src/{tool.rs, capability.rs, error.rs, binding.rs}
│   ├── atd-client/                     # Rust reference client
│   │   └── src/{lib.rs, transport/{unix.rs, stdio.rs, mcp.rs}, adapters/}
│   ├── atd-cli/                        # `atd` command-line binary
│   │   └── src/{main.rs, list.rs, call.rs, schema.rs, doctor.rs, allow.rs}
│   └── atd-mcp-bridge/                 # Phase 1: ATD → MCP server bridge
│       └── src/{main.rs, server.rs}
├── python/                             # Python SDK
│   ├── pyproject.toml
│   ├── atd_client/
│   │   └── __init__.py, transport.py, adapters.py, sync.py
│   └── atd_langchain/                  # Phase 1 LangChain toolkit
│       └── __init__.py
├── typescript/                         # TypeScript SDK
│   ├── package.json
│   └── src/client.ts
├── skills/                             # Skills for skills.sh publication
│   └── atd-dispatch/
│       └── SKILL.md
├── bindings/                           # Phase 2 reference binding implementations
│   └── appfunction-harmonyos/          # deferred
├── examples/
│   ├── hello_atd.rs
│   ├── hello_atd.py
│   ├── hello_atd.ts
│   └── langchain_demo.py
├── docs/
│   ├── quickstart/{rust,python,typescript}.md
│   ├── integrations/{langchain,hermes,openclaw,claude-code}.md
│   ├── protocol/{wire-format,error-codes,bindings}.md
│   └── design.md                       # this document (copied from anos spec)
├── tests/
│   ├── conformance/                    # Phase 2: cross-implementation tests
│   └── integration/
│       └── mock_server.rs              # ANOS-free test harness
└── .github/
    └── workflows/ci.yml                # includes ANOS-free build test
```

## 5. Integration Adapters

Each target agent ecosystem has a **minimum-invasion path** (zero upstream coordination) and a **deep path** (upstream PR, more features).

| Target | Phase 0 path | Phase 1-2 path |
|--------|-------------|---------------|
| **OpenClaw** | Publish `atd-dispatch` skill to ClawHub | PR OpenClaw: dispatcher resolves `atd:*` tool ids via atd-client |
| **Hermes Agent** | Spawn `atd-mcp-bridge`; Hermes MCP config points at it | PR Hermes: add native `AtdClient` (gets session/capability/tier benefits) |
| **LangChain** | `pip install atd-langchain` toolkit | — |
| **Claude Code / Cursor / Codex** | ATD-MCP-bridge exposes ATD tools as MCP server | Already covered by MCP adoption |
| **Any SKILL.md platform (26+)** | Publish `atd-dispatch` skill to skills.sh | Push `atd-tools:` YAML extension to agentskills.io spec |

### 5.1 `atd-dispatch` skill on skills.sh (highest leverage)

One skill published to skills.sh gives any of 26+ SKILL.md-compatible platforms (Claude Code, Cursor, OpenAI Codex, VS Code Copilot, GitHub, Atlassian, Figma, ...) access to ATD tools without any code change on those platforms.

**File:** `/home/nan/proj/atd-mvp/skills/atd-dispatch/SKILL.md`

```yaml
---
name: atd-dispatch
description: |
  Dispatch tool calls to ATD-compatible servers. Unlocks cross-platform,
  cross-vendor tools (Xiaomi, HealthKit, HMS, Jira, etc.) in any
  SKILL.md-compatible agent.
version: 0.1.0
license: MIT
atd-tools:
  required: []
---

When the user needs a tool that isn't in the native toolset, check ATD:

1. `atd list --query "<domain>"` — discover candidates
2. `atd schema <tool_id>` — read input/output contract
3. `atd call <tool_id> --args '<json>'` — invoke

Every call returns JSON with `{ok, data, error, metadata}`. Pass `data` forward.
```

### 5.2 atd-mcp-bridge (Phase 1)

**File:** `/home/nan/proj/atd-mvp/crates/atd-mcp-bridge/src/main.rs`

Standalone binary that:
1. Speaks MCP protocol on stdio (or TCP)
2. Forwards MCP `tools/list` → ATD `discover`
3. Forwards MCP `tools/call` → ATD `call`
4. Exposes ATD-specific features (tier, session, capability) as optional `_atd` MCP extension fields

Hermes Agent, Claude Desktop, Cursor — any MCP-aware agent — can point its MCP config at `atd-mcp-bridge` and immediately get access to all ATD tools.

### 5.3 atd-langchain (Phase 1)

**File:** `/home/nan/proj/atd-mvp/python/atd_langchain/__init__.py`

```python
from atd_langchain import AtdToolkit

toolkit = await AtdToolkit.connect("unix:///home/me/.anos/anos.sock")
tools = await toolkit.get_tools(query="smart home", limit=20)
agent = create_react_agent(llm, tools, prompt)
```

Internal: `get_tools()` runs `client.discover()`, wraps each summary as a LangChain `BaseTool` whose `_run()` invokes `client.call()`. Schema is lazy-loaded on tool invocation, not at agent construction — keeps agent init fast even with large registries.

## 6. Developer Experience

### 6.1 Install → first call: 15-min path

```
t=0    pip install atd-client
t=2    write hello_atd.py (10 lines, copied from docs)
t=5    python hello_atd.py     # hits ANOS daemon, prints result
t=10   read docs/quickstart/python.md (one page)
t=15   write your own 3-tool chain, works
```

### 6.2 CLI as REPL for exploration

The `atd` binary ships with the SDK:

```bash
atd list --query "smart home" --tier hot
atd schema xiaomi:light.toggle
atd call xiaomi:light.toggle --args '{"device_id": "bedroom"}'
atd doctor          # endpoint reachable? which bindings? permissions?
atd allow fs.delete # grant dangerous tool access
```

### 6.3 Actionable errors

Every `AtdError` includes `suggest_fix()`:

```
error: tool not found: xiaomi:light.togle
       did you mean 'xiaomi:light.toggle'?
hint:  atd list --query xiaomi
```

```
error: capability denied: fs.delete requires 'fs.write.dangerous' grant
hint:  run 'atd allow fs.delete' to grant for this session
       or 'atd allow --persist fs.delete' to remember
```

### 6.4 Integration quickstart (4 one-pagers)

| File | Content | Target read time |
|------|---------|-----------------|
| `/home/nan/proj/atd-mvp/docs/integrations/langchain.md` | 15 lines Python, run a react agent | 3 min |
| `/home/nan/proj/atd-mvp/docs/integrations/hermes.md` | 5 lines TOML to configure mcp-bridge | 2 min |
| `/home/nan/proj/atd-mvp/docs/integrations/openclaw.md` | `/skill install atd-dispatch` + usage | 2 min |
| `/home/nan/proj/atd-mvp/docs/integrations/claude-code.md` | MCP server config snippet | 2 min |

## 7. Phasing

### 7.1 Phase 0 — Proof of Concept (2-3 weeks)

**Goal:** a non-ANOS agent successfully calls an ATD tool.

| Deliverable | Location | Source reference |
|------------|----------|-----------------|
| `atd-types` crate | `/home/nan/proj/atd-mvp/crates/atd-types/` | Reimplement from `/home/nan/proj/anos/crates/anos-types/src/tool.rs` |
| `atd-client` crate | `/home/nan/proj/atd-mvp/crates/atd-client/` | Pattern from `/home/nan/proj/anos/crates/anos-cli/src/client.rs` |
| Unix socket transport | `/home/nan/proj/atd-mvp/crates/atd-client/src/transport/unix.rs` | Based on `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs` |
| `atd` CLI | `/home/nan/proj/atd-mvp/crates/atd-cli/` | New |
| hello-world examples | `/home/nan/proj/atd-mvp/examples/hello_atd.{rs,py}` | New |
| LangChain demo | `/home/nan/proj/atd-mvp/examples/langchain_demo.py` | New |

**Exit criteria:**
- `cargo run --example hello_atd` succeeds
- First-call latency <100ms on local Unix socket
- README has the 15-min install story
- Demo video: LangChain agent cross-process-calls `fs.read` via atd-client → ANOS daemon

### 7.2 Phase 1 — DX Push (4-6 weeks)

**Goal:** 3 external agents use it in production or serious prototype.

| Deliverable | Location |
|------------|----------|
| Python SDK | `/home/nan/proj/atd-mvp/python/` → PyPI `atd-client` |
| TypeScript SDK | `/home/nan/proj/atd-mvp/typescript/` → npm `@atd-protocol/client` |
| stdio transport | `/home/nan/proj/atd-mvp/crates/atd-client/src/transport/stdio.rs` |
| MCP-compat transport | `/home/nan/proj/atd-mvp/crates/atd-client/src/transport/mcp.rs` |
| `atd-langchain` | `/home/nan/proj/atd-mvp/python/atd_langchain/` → PyPI |
| `atd-mcp-bridge` | `/home/nan/proj/atd-mvp/crates/atd-mcp-bridge/` → standalone binary |
| `atd-dispatch` skill | `/home/nan/proj/atd-mvp/skills/atd-dispatch/SKILL.md` → skills.sh |
| Integration guides | `/home/nan/proj/atd-mvp/docs/integrations/*.md` |

**Exit criteria:**
- Published on PyPI + npm + crates.io
- `atd-dispatch` skill downloads ≥50
- 3 external project repos depend on `atd-client`
- At least one third-party tutorial (YouTube/blog)

### 7.3 Phase 2 — Ecosystem Validation (8-12 weeks)

**Goal:** protocol acquires upstream adoption and covers multiple OSes.

| Deliverable | Location |
|------------|----------|
| HTTP/JSON transport | `/home/nan/proj/atd-mvp/crates/atd-client/src/transport/http.rs` |
| AppFunction reference binding | `/home/nan/proj/atd-mvp/bindings/appfunction-harmonyos/` |
| OpenClaw upstream PR | External repo `github.com/openclaw/openclaw` |
| Hermes native integration PR | External repo `github.com/nousresearch/hermes-agent` |
| Conformance test suite | `/home/nan/proj/atd-mvp/tests/conformance/` |
| Public site | `atd-protocol.org` |
| `atd-tools` RFC to agentskills.io | PR to `github.com/agentskills/agentskills` |

**Exit criteria:**
- ≥1 upstream PR merged to a major framework
- ≥10 external adopters
- Coverage: Linux, macOS, HarmonyOS (iOS deferred)
- atd-protocol.org has APWG bootstrap info + 30+ community-contributed binding definitions

## 8. Risk Register

| Risk | Severity | Mitigation |
|------|---------|------------|
| Extraction from `/home/nan/proj/anos/crates/anos-tool-dispatch/` leaks ANOS-specific assumptions | HIGH | Independent schema in `/home/nan/proj/atd-mvp/crates/atd-types/`; CI runs ANOS-free test harness in `/home/nan/proj/atd-mvp/tests/integration/mock_server.rs` |
| MCP-compat mode hides ATD's unique advantages (users stay on the MCP subset) | MEDIUM | Docs clearly mark ATD-extension fields; MCP-compat positioned as "onboarding ramp," native as goal |
| AppFunction binding needs real hardware | HIGH | Start with HarmonyOS (Huawei cloud emulator + remote device access); iOS deferred to Phase 3 |
| agentskills.io spec rejects `atd-tools` extension | MEDIUM | Fall back to ATD-internal `x-atd-tools:` namespace — does not affect SKILL.md portability |
| OpenClaw/Hermes upstream PR rejected | MEDIUM | Path 1 (zero-code skill publication) is independent of Path 2 (PR); one rejection doesn't block the other |
| Phase 0 scope creep (adding features beyond 3 core APIs) | MEDIUM | Hard-code Phase 0 to: Unix socket + `discover` + `describe` + `call`. Anything else → Phase 1 |

## 9. Go/No-Go Gates

| Gate | Condition | Decision |
|------|-----------|----------|
| Phase 0 → 1 | hello-world + ≥1 non-ANOS agent demo video | Proceed; else revisit protocol boundary |
| Phase 1 → 2 | ≥3 external adopters; PyPI/npm/crates.io all published | Proceed; else extend Phase 1 DX work |
| Phase 2 → v1.0 | ≥10 adopters + 1 upstream PR merged + AppFunction verified on real device | Release v1.0; else remain v0.x |

## 10. Open Questions

These require resolution before Phase 0 starts:

1. **Repo creation:** `/home/nan/proj/atd-mvp/` created now (alongside ANOS) or held until first real commit? **Recommendation:** create now, initialize with this design doc, then push to `github.com/downsea/atd-mvp`.
2. **Cargo workspace vs polyrepo:** single `Cargo.toml` workspace with Rust crates, while Python/TS are sibling directories (not in workspace). **Recommendation:** yes, this matches Rust-ecosystem convention.
3. **License:** MIT or Apache-2.0 or dual. **Recommendation:** Apache-2.0 (matches Hermes, allows upstream merge into Apache-licensed projects).
4. **Versioning:** 0.1.0 semver from Day 1. Breaking changes allowed <1.0, protocol stability promised at 1.0.
5. **Governance:** who owns `atd-protocol` GitHub org? Phase 0 owner = ANOS author individually; Phase 2 transfer to APWG (bootstrapped via whitepaper §4.3).

## 11. Concrete First Sprint (Phase 0, week 1)

Deliverables in order:

1. `mkdir /home/nan/proj/atd-mvp/` — initialize repo, add `README.md`, `LICENSE`, `Cargo.toml` workspace manifest
2. Copy this design doc to `/home/nan/proj/atd-mvp/docs/design.md`
3. Create `atd-types` crate: port `ToolDefinition`, `ToolSummary`, `CapabilityDescriptor` from `/home/nan/proj/anos/crates/anos-types/src/tool.rs` without `anos-*` dependencies
4. Create `atd-client` crate: minimum `AtdClient::connect` + `call` over Unix socket
5. Write `/home/nan/proj/atd-mvp/examples/hello_atd.rs`
6. Run against local ANOS daemon (no ANOS code changes required)
7. Write `/home/nan/proj/atd-mvp/README.md` — 15-min install story
8. Push initial commit to `github.com/downsea/atd-mvp` (pending org creation)

## 12. References

- Whitepaper v2: [`/home/nan/proj/anos/docs/research/toward-agent-tool-dispatch-v2.md`](/home/nan/proj/anos/docs/research/toward-agent-tool-dispatch-v2.md) (§2.4 Skills layering, §7 Agent framework author path, Appendix G `atd-tools` RFC)
- Whitepaper v1: [`/home/nan/proj/anos/docs/research/toward-agent-tool-dispatch.md`](/home/nan/proj/anos/docs/research/toward-agent-tool-dispatch.md) (§5.8 Skills as stdlib)
- ATD module overview: [`/home/nan/proj/anos/docs/modules/anos-tool-dispatch.md`](/home/nan/proj/anos/docs/modules/anos-tool-dispatch.md)
- ATD architecture: [`/home/nan/proj/anos/docs/architecture/atd-overview.md`](/home/nan/proj/anos/docs/architecture/atd-overview.md)
- Open issues tracked for reference-server gaps:
  - [`atd-native-cli-binding-missing.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-native-cli-binding-missing.md)
  - [`atd-appfunction-binding-not-started.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-appfunction-binding-not-started.md)
  - [`atd-semantic-discovery-not-connected.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-semantic-discovery-not-connected.md)
  - [`atd-pipe-composition-not-implemented.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-pipe-composition-not-implemented.md)
  - [`atd-dry-run-not-wired.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-dry-run-not-wired.md)
  - [`atd-error-classification-not-unified.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-error-classification-not-unified.md)
  - [`atd-ucan-capability-depth-unclear.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-ucan-capability-depth-unclear.md)
  - [`atd-tier-management-incomplete.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-tier-management-incomplete.md)
  - [`atd-benchmark-suite-missing.md`](/home/nan/proj/anos/docs/issues/2026-04-21-atd-benchmark-suite-missing.md)
- Reference implementation files (in ANOS):
  - `/home/nan/proj/anos/crates/anos-tool-dispatch/src/` — dispatch core
  - `/home/nan/proj/anos/crates/anos-cli/src/client.rs` — IPC client pattern
  - `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs` — wire protocol
  - `/home/nan/proj/anos/crates/anos-types/src/tool.rs` — tool type definitions
  - `/home/nan/proj/anos/crates/anos-llm-anthropic/src/provider.rs` — tool name sanitization
- External standards referenced:
  - agentskills.io — Anthropic Agent Skills open standard (SKILL.md)
  - MCP — Anthropic Model Context Protocol
  - UCAN — capability token format
