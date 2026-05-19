# The ATD Adopter Design Philosophy

*Principles for building tool servers that consume — and are consumed by — the ATD protocol.*

**Author**: ATD maintainers
**Date**: 2026-05-19
**Status**: Living document — additions welcome via PR

---

## Premise

ATD (Agent Tool Dispatch) is a protocol for connecting AI agents to tools. The protocol itself is small (one wire spec, one schema, four message types). The interesting design decisions live one level up: how an **adopter** — the team writing the tool server an agent will actually call — structures their work so the result holds up across agent platforms, vendors, and time.

This document codifies the patterns. Each is illustrated by what the existing reference adopters (`healthkit_cli`, `celia_phr`, `cbrain`) do, with concrete file paths. Anti-patterns are called out by adopter incident where they apply.

The companion doc to read first is [`docs/architecture.md`](architecture.md) (the system view). This doc is the *why*; that one is the *what*.

---

## Table of Contents

1. [The Three Consumers](#1-the-three-consumers)
2. [Seven Principles](#2-seven-principles)
3. [Principle 1: The ToolDefinition is the source of truth](#principle-1-the-tooldefinition-is-the-source-of-truth)
4. [Principle 2: Skills travel with tools, not with bridges](#principle-2-skills-travel-with-tools-not-with-bridges)
5. [Principle 3: Capabilities are negotiated, never hardcoded](#principle-3-capabilities-are-negotiated-never-hardcoded)
6. [Principle 4: Errors are typed and namespaced](#principle-4-errors-are-typed-and-namespaced)
7. [Principle 5: Tools are stateless across connections by default](#principle-5-tools-are-stateless-across-connections-by-default)
8. [Principle 6: Discovery is canonical — never hardcode tool ids in agent prompts](#principle-6-discovery-is-canonical--never-hardcode-tool-ids-in-agent-prompts)
9. [Principle 7: Dispatch is bounded and observable](#principle-7-dispatch-is-bounded-and-observable)
10. [Anti-Pattern Summary](#3-anti-pattern-summary)
11. [Adopter Design Checklist](#4-adopter-design-checklist)
12. [The Bigger Picture](#5-the-bigger-picture)

---

## 1. The Three Consumers

An ATD tool server serves three consumers simultaneously:

| Consumer | Needs | Channel |
|---|---|---|
| **The LLM Agent** | Discoverable tool surface, typed error envelopes, predictable arg shape | `tool_list` / `tool_schema` / `run_tool` over the wire |
| **The Human Operator** | Audit trail, ops control, structured logs, capability denial visibility | `AuditSink` events, server logs, metrics counters |
| **The Agent Platform Bridge** (Hermes / Claude Code / MCP) | Stable handshake, capability negotiation, transport that doesn't surprise | `Hello`/`HelloAck` + UCAN-lite, length-prefixed JSON over UDS/HTTP/stdio |

These do not conflict if you separate them cleanly. **Wire frames are for the LLM.** **The audit sink is for the human.** **The handshake is for the bridge.** Same server, three pipes, no flags or modes.

The key consequence: every design decision below has to hold up against all three readings. A choice that makes the LLM's life easier but breaks the bridge's handshake is not a tradeoff — it's a bug.

---

## 2. Seven Principles

| # | Principle | In One Sentence |
|---|---|---|
| 1 | The ToolDefinition is the source of truth | Generate summaries, args validation, skills, adapters, and docs from one canonical `ToolDefinition` — never hand-maintain parallel copies. |
| 2 | Skills travel with tools, not with bridges | Expose `skills.list` / `skills.get` meta-tools; let `atd skills sync` install them per platform. Don't hand-copy SKILL.md into agent-platform config. |
| 3 | Capabilities are negotiated, never hardcoded | Declare `required_capabilities`, intersect with `Hello.granted_capabilities`, gate at dispatch with `ERR_CAPABILITY_DENIED` (1001). Don't bake auth checks into handlers. |
| 4 | Errors are typed and namespaced | Protocol uses 1000-1099 (per `crates/atd-protocol/src/messages.rs`); adopters take 2000+ ranges per `SP-error-namespace-v1`. No free-form error strings. |
| 5 | Tools are stateless across connections by default | Per-connection `ConnectionContext` carries `Hello`-negotiated state; shared world state is opt-in and declared. Never assume connection affinity. |
| 6 | Discovery is canonical — never hardcode tool ids in agent prompts | Agents call `discover` at runtime; new tools appear without prompt changes; renames don't break flows. |
| 7 | Dispatch is bounded and observable | Tier deadlines via `resources.timeout_ms`; middleware for tracing / audit / rate; no silent retries inside the server; failures observable via the audit sink. |

---

## Principle 1: The ToolDefinition is the source of truth

### The Rule

Every fact about a tool — its name, its args shape, its required capabilities, its visibility, its tier deadline, its safety classification — lives in exactly one place: the `ToolDefinition` struct (`crates/atd-protocol/src/tool.rs` for Rust, `python/src/atd_client/types.py:ToolDefinition` for Python). Summaries are projected from it. JSONSchema validation reads from it. Skills meta-tools serve it. LLM-adapter shapes (OpenAI / Anthropic) generate from it. There is no second copy.

### Why

The moment a fact about a tool lives in two places — say, args declared in `ToolDefinition.input_schema` AND in a hand-written `SKILL.md` example — they drift. The drift is silent: the LLM sees the SKILL.md, the server validates against the schema, and a `1005 invalid_arguments` failure leaves both the human and the agent confused. Worse, the wrong copy becomes "authoritative" by accident (whichever one a maintainer happens to update next).

### Implementation

- **One `ToolDefinition` literal per tool**, registered via `@server.register(definition=ToolDefinition(...))` (Python) or the Rust equivalent. All other artifacts are *derived*:
  - `ToolSummary` is produced by the registry from the definition (e.g. `ToolRegistry._summary_from_definition` in `python/src/atd_server/registry.py`).
  - `tool_schema` responses serve the definition straight (`definition.model_dump(mode="json")`).
  - SKILL.md frontmatter `description` should be the same string as `ToolDefinition.description`. If they diverge, the SKILL is wrong, not the definition.
  - LLM-adapter outputs (`as_openai_tools(summaries)`, `as_anthropic_tools(summaries)`) read from the definition without a second mapping layer.
- **No "convenience" duplication.** If the definition's `input_schema` requires `{"path": "string"}`, do NOT also write a separate docstring saying "args: {path: str}". The docstring will rot.

### Anti-Pattern: Hand-maintained args description

```python
# ❌ Args shape lives in TWO places — they will drift:
@server.register(definition=ToolDefinition(
    input_schema={"type": "object", "properties": {"path": {"type": "string"}},
                  "required": ["path"]},
    description="Read a file from `path` (required) and return its contents.",  # ← will rot
))
async def read(args, ctx):
    ...
```

When the schema later adds an optional `encoding` field, only the schema changes; the description still says "from `path`" only. The agent reads the description and guesses encoding from context.

### Adopter check

- ✅ **healthkit_cli** generates SKILL.md content + CLI commands + OpenAPI schema from the Huawei HealthKit OpenAPI spec; the same spec drives `ToolDefinition`s emitted to ATD (see `healthkit_cli/docs/Agent_Native_CLI_Design_Philosophy.md` §6, the closest sibling principle).
- ✅ **celia_phr** consumes the FHIR R4 schema as source of truth; `atd-middleware-fhir` validates against the same shapes the tools advertise.
- 🟡 **cbrain** is at risk: with `hermes-config/skills/` planned as the SKILL.md home, the SKILL content would become a second source of truth alongside ToolDefinition. **Recommended fix:** expose `cbrain:sim.skills.list` / `cbrain:sim.skills.get` meta-tools that read SKILL.md content from the same place the tools are defined (see Principle 2).

---

## Principle 2: Skills travel with tools, not with bridges

### The Rule

A tool's SKILL.md content is *part of the tool*, not part of the agent platform. It lives in the tool server's repository (alongside the `ToolDefinition` that backs it), is served via the `<publisher>:<service>.skills.list` and `<publisher>:<service>.skills.get` meta-tools (per `SP-skills-discovery-convention`), and is installed onto each agent platform by `atd skills sync --target {hermes,claude-code,...}`. The agent platform's skill directory is a **cache**, not a source.

### Why

When an adopter writes SKILL.md directly into `~/.hermes/skills/` or `~/.claude/skills/`, the SKILL travels with the human who set up that agent platform, not with the tool version that's actually being called. Upgrading the tool server doesn't refresh the SKILL. Adding a second agent platform requires duplicating the SKILL. Switching agent platforms drops the SKILL on the floor. The agent gets stale guidance and the maintainer can't tell.

The convention from `SP-skills-discovery-convention` (2026-04-27) — meta-tool publish + sync helper install — fixes all three failure modes with the same mechanism that already exists for tool discovery.

### Implementation

```python
# In your tool server (cbrain example):
from pathlib import Path

SKILL_ROOT = Path(__file__).parent / "skills"

@server.register(definition=ToolDefinition(
    id="cbrain:sim.skills.list",
    description="List available skills published by the cbrain-sim server.",
    visibility=ToolVisibility.READ,
    required_capabilities=[],   # public meta-tool per SP-skills-discovery-convention Q6
    # ...
))
async def list_skills(args, ctx) -> dict:
    out = []
    for path in sorted(SKILL_ROOT.glob("*/SKILL.md")):
        name = path.parent.name
        # Parse frontmatter description; full parse omitted for brevity
        out.append({"name": name, "description": _read_description(path), "version": None})
    return out

@server.register(definition=ToolDefinition(
    id="cbrain:sim.skills.get",
    # ...
))
async def get_skill(args, ctx) -> dict:
    name = args["name"]
    md = (SKILL_ROOT / name / "SKILL.md").read_text(encoding="utf-8")
    return {"name": name, "content_md": md}
```

Then on each agent host:

```bash
atd skills sync --target hermes      # → ~/.hermes/skills/cbrain-sim-<name>/SKILL.md
atd skills sync --target claude-code # → ~/.claude/skills/cbrain-sim-<name>/SKILL.md
```

### Anti-Pattern: SKILL.md hand-copied into agent-platform config

```
❌  cbrain/hermes-config/skills/cbrain-perception/SKILL.md     # written by hand
❌  cbrain/hermes-config/skills/cbrain-manipulation/SKILL.md   # written by hand
    ^ when cbrain-sim ships v0.2 with a new "world.reset" skill,
      these files don't update; Hermes shows v0.1 guidance forever.
```

The current cbrain layout (`/home/nan/code/cbrain/hermes-config/skills/` — directory exists, currently empty) is at the fork-in-the-road. **Recommendation**: leave it empty; expose `cbrain:sim.skills.list/get` meta-tools instead; let `atd skills sync` populate `~/.hermes/skills/cbrain-sim-*/SKILL.md` on the actual agent host. Hand-written skill files in the cbrain repo are pre-broken.

### Adopter check

- ✅ **healthkit_cli** ships SKILL.md files at `healthkit_cli/skills/healthkit-{shared,steps,heartrate,sleep,healthkit}/SKILL.md` AND exposes them via `huawei:hms.healthkit.skills.list/get` meta-tools per the convention. Verified working end-to-end in `SP-skills-discovery-convention`.
- ✅ **celia_phr** exposes equivalent meta-tools for its FHIR helpers.
- 🟡 **cbrain** has `hermes-config/skills/` directory created but empty. The fork-in-the-road moment is now. Following Principle 2 = empty dir stays empty + tool-side meta-tools added.

---

## Principle 3: Capabilities are negotiated, never hardcoded

### The Rule

A tool declares opaque capability strings in `ToolDefinition.required_capabilities: list[str]`. The connection negotiates `granted_capabilities: list[str]` at `Hello` time via a `ServerPolicy` callback. Dispatch gates each `run_tool` by computing `missing = required - granted`; if non-empty, return `ERR_CAPABILITY_DENIED` (1001) with `details = {required, granted, missing}`. Handlers themselves do NOT check capabilities — the dispatcher already did.

### Why

Hardcoding "this tool requires admin" inside a handler creates four problems:

1. The check is invisible to the LLM until the call fails — `tool_list` doesn't reflect it.
2. Different handlers drift in how they check (some by env var, some by header, some by client_id).
3. Audit / observability sees the failure too late — only after the handler started running.
4. A future bridge that wants to negotiate capabilities upfront (e.g., for a UI permission prompt) has nothing to query.

Externalizing capabilities to `ToolDefinition.required_capabilities` + dispatcher gating fixes all four. The LLM sees the requirements in `tool_schema`. The check is uniform. The audit sink sees `ERR_CAPABILITY_DENIED` before any handler runs. Bridges can pre-fetch the schema.

### Implementation

```python
# Tool declaration:
@server.register(definition=ToolDefinition(
    id="cbrain:manipulation.pick",
    required_capabilities=["manipulation"],     # opaque string, adopter convention
    # ...
))
async def pick(args, ctx) -> dict:
    # No `if not has_cap(...)` here. The dispatcher already passed.
    return await sim.pick(args["target"])

# Server policy (production version):
_OFFER = frozenset({"perception", "manipulation", "world.read"})

async def my_policy(hello, ucan_tokens):
    requested = hello.get("requested_capabilities") or []
    granted = {str(c) for c in requested if c in _OFFER}
    return GrantedCapabilities(capabilities=frozenset(granted))

server = AtdServer(socket_path=..., policy=my_policy)
```

### Anti-Pattern: Per-handler hardcoded auth checks

```python
# ❌ Different handlers, different checks, agent has to guess:
@server.register(definition=ToolDefinition(id="x:write", required_capabilities=[]))
async def write(args, ctx):
    if ctx.connection.client_id != "admin":
        return ToolFailure(code="403", message="forbidden")
    ...
```

The LLM has no way to know `client_id != "admin"` will fail until it tries. The dispatch path can't fast-fail. Cap declared as `[]` means `tool_schema` lies. Move the check to `required_capabilities=["write"]` + a `ServerPolicy` that only grants `"write"` to admin clients.

### Adopter check

- ✅ **healthkit_cli** uses `["records:read", "records:write"]` allow-list intersection per the Rust runtime's `SharedServerConfig.granted_capabilities`.
- ✅ **celia_phr** mirrors the same pattern.
- 🟡 **cbrain** should pick a convention now (proposed: `perception`, `manipulation`, `world.read`, `world.reset`, `task.lifecycle.write`) and run with it. Separator (`:` vs `.`) is adopter-free per `SP-capability-naming-v1` (queued); pick one and be consistent.

---

## Principle 4: Errors are typed and namespaced

### The Rule

Every failure carries a numeric `code` in one of two ranges:

- **1000-1099** — protocol-level (defined in `crates/atd-protocol/src/messages.rs`). Allocated: 1000 `TOOL_NOT_FOUND`, 1001 `CAPABILITY_DENIED`, 1002 `RATE_LIMITED`, 1003 `BROKER_FAILED`, 1004 `DEADLINE_EXCEEDED`, 1005 `INVALID_ARGS`, 1010-1013 UCAN, 1020-1021 cursor, 1099 `INTERNAL`. Free slots: 1006-1009 / 1014-1019 / 1022-1098.
- **2000+** — adopter range (per `SP-error-namespace-v1`, queued). Proposed allocation: cbrain 2000-2099, healthkit 3000-3099, celia 4000-4099.

Free-form error strings are not allowed. `ToolError(code=2042, message="cbrain skill aborted", partial_data={...})` is the right shape. `raise Exception("something broke")` falls through to `1099 INTERNAL` and is logged as a maintainer-action-required event.

### Why

Numeric codes survive translation. An LLM looking at `code: 1001` can recover (request the missing capability via a richer Hello, or back off). A free-form `"forbidden"` message requires the LLM to read prose; different adopters phrase the same condition differently; recovery becomes unreliable.

Namespacing prevents collisions. Without it, two adopters both pick `code: 42` for "thing failed", and the receiving bridge can't tell them apart.

### Implementation

```python
from atd_server import ToolError

@server.register(definition=ToolDefinition(
    id="cbrain:manipulation.pick",
    errors=[
        ToolErrorDef(code="2042", description="Skill aborted by safety override", retryable=False),
        ToolErrorDef(code="2043", description="Target out of reach", retryable=True),
    ],
))
async def pick(args, ctx):
    if not safe_to_pick(args["target"]):
        raise ToolError(code=2042, message="aborted: safety override fired", partial_data={"step": "approach"})
    ...
```

The `errors` field on `ToolDefinition` advertises the codes the tool may emit. The LLM can map them to recovery strategies declaratively.

### Anti-Pattern: Free-form error message as the primary signal

```python
# ❌ Recovery requires the LLM to parse English:
return ToolFailure(code="ERR", message="couldn't reach the device, maybe try again later")
```

vs:

```python
# ✅ Numeric code + adopter namespace + retryable bit:
return ToolFailure(code="2043", message="target out of reach", retryable=True)
```

### Adopter check

- ✅ **Phase E of SP-server-py-v1** allocated `1004 DEADLINE_EXCEEDED` and `1005 INVALID_ARGS` after catching that the original spec's `1002 invalid_arguments` / `1003 deadline_exceeded` collided with Rust's `ERR_RATE_LIMITED` / `ERR_BROKER_FAILED`. The spec was corrected in commit `dd9116d`. **SP-error-namespace-v1 should propagate the 1004/1005 constants to Rust `messages.rs`** so cross-impl semantics align (see cbrain issue §9.5 spec-corrections table).
- 🟡 **cbrain** should claim 2000-2099 explicitly in their tool definitions' `errors` field before shipping; otherwise the namespace becomes informal and collisions creep in.

---

## Principle 5: Tools are stateless across connections by default

### The Rule

Each connection gets a fresh `ConnectionContext` carrying only the state negotiated by `Hello` (`client_id`, `granted_capabilities`, `ucan_tokens`). Two connections from the same agent process get two independent contexts. Server-side shared state (a singleton simulator, a session pool, a token cache) is **opt-in and declared** — adopters that need it document the model and ideally signal it via the `session_model` field that `SP-session-model-doc` (queued) will add to `HelloAck`.

### Why

Most agent platforms reconnect freely — bridges crash, sessions expire, LB drops a connection. A tool server that assumes "this is the same client as last time" gives wrong answers when that assumption breaks. The right default is statelessness; deviations from it should be loud.

The exception (shared world state — a MuJoCo simulator, a database connection pool, an in-memory KV) is real and useful. cbrain-sim is the archetype: `MjData` is one singleton, all clients see the same physics. Making this **declared rather than implicit** prevents adopters from being surprised when a "stateless" tool turns out to mutate a shared buffer.

### Implementation

```python
# Stateless tool (the easy case):
@server.register(definition=ToolDefinition(id="x:hash"))
async def hash(args, ctx):
    return {"sha256": hashlib.sha256(args["data"].encode()).hexdigest()}
    # No shared state. ctx isn't even used.

# Shared-world tool (cbrain-sim, declared):
SIM = MuJoCoSimulator()       # module-global singleton

@server.register(definition=ToolDefinition(
    id="cbrain:manipulation.pick",
    description="Pick the target object. WARNING: mutates shared simulator state visible to ALL connected agents.",
    # ...
))
async def pick(args, ctx):
    # SIM is shared across all connections. Document it; future Hello
    # negotiation (SP-session-model-doc) will let adopters advertise this.
    await SIM.pick(args["target"])
    return {"completed": True}
```

### Anti-Pattern: Implicit per-connection state held in module globals

```python
# ❌ Two connections hit this; the second corrupts the first's view:
_LAST_CONFIG: dict | None = None    # module-global, intent: per-connection

@server.register(definition=ToolDefinition(id="x:configure"))
async def configure(args, ctx):
    global _LAST_CONFIG
    _LAST_CONFIG = args["config"]
    return {"ok": True}

@server.register(definition=ToolDefinition(id="x:apply"))
async def apply(args, ctx):
    return run_with(_LAST_CONFIG)    # reads the LAST connection's config
```

Either commit to "shared world" (document it, accept that two clients interleave) or move state into `ctx.connection` and accept that it's per-connection.

### Adopter check

- ✅ **healthkit_cli** is stateless per-call (each tool call resolves the token afresh, hits Huawei REST, returns).
- ✅ **celia_phr** runs `atd-server-http` with one server process per host; tools are stateless across requests.
- 🟢 **cbrain** is intentionally shared-world (one MuJoCo `MjData` for all connected agents). This is correct — but should be loudly documented in every tool definition's `description` field until `SP-session-model-doc` ships `HelloAck.session_model: "shared_world"`.

---

## Principle 6: Discovery is canonical — never hardcode tool ids in agent prompts

### The Rule

Agents discover tools at runtime via `tool_list` → `tool_schema`. Agent prompts MUST NOT contain a hardcoded list of tool ids. New tools appear automatically. Renames don't break flows (clients re-discover on each session). The `id` field of a `ToolSummary` is the only stable handle; everything else (name, description) is human-facing prose that can change without breaking the agent.

### Why

A hardcoded list of tool ids in a prompt is the agent equivalent of a hardcoded API endpoint list in a service. It rots the moment the server changes; nobody notices until the agent silently picks the wrong tool because its preferred id 404s into a fallback. The discovery path exists exactly to avoid this.

The version of the prompt that doesn't hardcode ids is also simpler: "look at the tools you have access to, pick the one that matches this task" is one line. The version that hardcodes is a maintenance liability.

### Implementation

```python
# Agent-side (pseudo-code):
async def handle_user_request(task: str):
    tools = await atd_client.discover()            # always fresh
    summaries = [{"id": t.id, "name": t.name, "description": t.description}
                 for t in tools]
    chosen_id = await llm_pick_tool(task, summaries)
    schema = await atd_client.describe(chosen_id)  # always fresh
    args = await llm_fill_args(task, schema)
    return await atd_client.call(chosen_id, args)
```

Cache discover() / describe() results for the lifetime of one *agent session* if needed, but never longer than one server connection.

### Anti-Pattern: Tool id baked into the system prompt

```
❌  System prompt:
    "You may call cbrain:perception.snapshot, cbrain:manipulation.pick,
     cbrain:world.reset. Always start by calling perception.snapshot."
```

When cbrain-sim adds `cbrain:perception.depth_snapshot`, the system prompt doesn't know about it. When `manipulation.pick` is renamed to `manipulation.grasp` (for clearer semantics), every agent breaks at once. Both cases are nonissues with discovery-driven prompting.

### Adopter check

- 🟢 **All three adopters** rely on bridge-side discovery (Hermes / Claude Code / `atd-mcp-bridge` call `tool_list` at session start). The risk is on the *agent-prompt* side, which is the platform team's responsibility, not the tool server's. Adopters can help by keeping tool descriptions self-explanatory enough that an agent can pick from the list without needing prose memorization.

---

## Principle 7: Dispatch is bounded and observable

### The Rule

Every tool call is wrapped in three contracts:

- **Bounded**: a deadline derived from `definition.resources.timeout_ms` (with a `30s` fallback for unset). Exceeding it returns `1004 DEADLINE_EXCEEDED`. No tool runs forever.
- **Observable**: middleware (`pre_call` / `post_call` / `on_error`) sees every dispatch. Adopters that need audit (cbrain's Merkle trace), tracing (OpenTelemetry), rate limiting, or metrics implement them as middleware. The dispatch path itself does not silently swallow.
- **No silent retries**: the server never retries a tool call internally. If the tool fails transiently, return `retryable=True` and let the client decide. Silent server-side retries hide failures from the audit sink and double-charge for side effects.

### Why

Unbounded calls deadlock the agent stack. The agent waits on the bridge waits on the server waits on a tool. A 5-second tool that occasionally takes 30 minutes is indistinguishable from a hang; agents time out and humans page on-call.

Unobservable calls make incidents unfixable. When a Merkle audit shows a gap, the maintainer needs to know whether the call was rejected at capability gate, timed out, or actually ran and the audit emitter dropped the event. The three look identical without middleware.

Silent retries break the "tool calls are at-most-once" contract that adopters rely on for non-idempotent operations (cbrain's `world.reset`, healthkit's record-create).

### Implementation

```python
# Bounded:
@server.register(definition=ToolDefinition(
    id="cbrain:perception.snapshot",
    resources=ToolResources(timeout_ms=2000, max_concurrent=1),  # 2s deadline
))
async def snapshot(args, ctx):
    return await sim.render()

# Observable:
@server.middleware(stage="post_call")
async def merkle_audit(request, ctx, call_next):
    response = await call_next()
    audit_chain.append({
        "request_id": ctx.request_id,
        "tool_id": ctx.tool_id,
        "timestamp": time.time_ns(),
        "request_hash": sha256_dict(request),
        "response_hash": sha256_dict(response),
    })
    return response

# No silent retries — surface them:
@server.register(definition=ToolDefinition(id="x:flaky"))
async def flaky(args, ctx):
    try:
        return await external_call(args)
    except TransientError as e:
        return ToolFailure(code="2099", message=str(e), retryable=True)  # client decides
```

### Anti-Pattern: Implicit retry loop inside the handler

```python
# ❌ Side effects fire 3× when the third attempt finally returns 200:
async def create_record(args, ctx):
    for attempt in range(3):
        try:
            return await api.create(args)
        except TransientError:
            await asyncio.sleep(0.5 * (2 ** attempt))
    raise ToolError(code=2099, message="exhausted retries")
```

vs:

```python
# ✅ Return retryable; client (with idempotency key) decides:
async def create_record(args, ctx):
    try:
        return await api.create(args)
    except TransientError as e:
        return ToolFailure(code="2099", message=str(e), retryable=True)
```

### Adopter check

- ✅ **healthkit_cli** uses per-tool `timeout_ms`; surfaces transient failures with `retryable=True`.
- ✅ **celia_phr** uses `atd-server-http` deadlines + the audit sink shipped in `SP-concurrency-baseline` (mpsc-buffered, observable via `Server::metrics_snapshot()`).
- 🟢 **cbrain** has middleware (post-Phase F) for the Merkle audit + can wire OpenTelemetry tracing on the same stage; bounded via `resources.timeout_ms`.

---

## 3. Anti-Pattern Summary

Quick reference for the things to NOT do (one line each):

- ❌ **Hand-copying SKILL.md into agent-platform config dirs** (Principle 2). Use `<publisher>:<service>.skills.list/get` meta-tools instead.
- ❌ **Hand-writing args descriptions that duplicate `input_schema`** (Principle 1). They drift silently.
- ❌ **Per-handler hardcoded auth / capability checks** (Principle 3). Move to `required_capabilities` + `ServerPolicy`.
- ❌ **Returning free-form error strings without a numeric code** (Principle 4). The LLM can't recover without one.
- ❌ **Module-global state intended as per-connection** (Principle 5). Either commit to "shared world" (document loudly) or use `ctx.connection`.
- ❌ **Tool ids baked into agent system prompts** (Principle 6). Make agents discover at session start.
- ❌ **Implicit retry loops inside handlers** (Principle 7). Return `retryable=True` and let the client decide.
- ❌ **`raise Exception("...")` as the primary failure path** (Principle 4 + 7). Becomes `1099 INTERNAL`; logged as maintainer-action-required.
- ❌ **Catching `asyncio.CancelledError` and continuing** (Principle 7). Re-raise; cancellation is a contract.
- ❌ **Wrapping wire frames with a per-platform shim** (cross-cutting). The wire is byte-compat across implementations by design; shims defeat that.

---

## 4. Adopter Design Checklist

When building (or auditing) an ATD tool server, verify each:

### Schema as source of truth
- [ ] Every tool fact (name, description, args, errors, caps, deadlines) lives in exactly one `ToolDefinition`.
- [ ] `ToolSummary` is *derived* from `ToolDefinition`, never hand-maintained.
- [ ] SKILL.md `description` frontmatter matches `ToolDefinition.description` (one source).
- [ ] LLM-adapter shapes (OpenAI / Anthropic) generate from `ToolSummary` without a parallel mapping table.

### Skills + discovery
- [ ] Tool server exposes `<publisher>:<service>.skills.list` + `.skills.get` meta-tools (per `SP-skills-discovery-convention`).
- [ ] No SKILL.md files live in agent-platform config directories (`~/.hermes/skills/`, `~/.claude/skills/`); those are populated by `atd skills sync`.
- [ ] Agent prompts do NOT hardcode tool ids; agents call `discover` at session start.

### Capabilities
- [ ] Every cap-requiring tool declares `required_capabilities: list[str]`.
- [ ] `ServerPolicy` intersects `requested_capabilities` with an allow-list (not "grant everything").
- [ ] No handler contains an `if not has_cap(...)` check — gating happens at dispatch.
- [ ] `tool_schema` response includes `required_capabilities` so the LLM can see them.

### Errors
- [ ] Every `ToolError` / `ToolFailure` carries a numeric `code` (no `"ERR"` / `"FAIL"` strings).
- [ ] Adopter codes fall in the adopter's namespace (cbrain 2000-2099 / healthkit 3000-3099 / celia 4000-4099 per `SP-error-namespace-v1`).
- [ ] `ToolDefinition.errors` advertises the codes the tool may emit.
- [ ] `retryable` is honest — `True` only when the client can safely re-call.
- [ ] No `raise Exception(...)` in the primary failure path; reserve for unexpected bugs (which become `1099`).

### State
- [ ] Each tool's state model is documented: stateless / per-connection / shared-world.
- [ ] Shared-world tools say so in their `description` (until `SP-session-model-doc` ships `HelloAck.session_model`).
- [ ] No module-global variables intended as per-connection state.

### Observability + bounds
- [ ] Every tool sets `resources.timeout_ms` (or accepts the 30s default consciously).
- [ ] Middleware exists for audit / tracing / rate limiting; the dispatch path is observable.
- [ ] No silent retry loops inside handlers; return `retryable=True` instead.
- [ ] `asyncio.CancelledError` is always re-raised (or explicitly propagated).

### Wire
- [ ] No platform-specific shims wrap the wire frames; bytes are byte-compat with `crates/atd-protocol` v0.1.0.
- [ ] If you author a server in a new language, exercise the `atd-conformance` fixture corpus before claiming compatibility (Python's `test_server_conformance.py` is the reference shape).

---

## 5. The Bigger Picture

ATD is a small protocol surrounded by a much larger set of design choices. The protocol commits to a wire format, a handshake, and a discovery shape. Everything else — how skills are published, how errors are named, how state is scoped, how deadlines are set — is *adopter convention*.

Conventions in software degrade silently. The schema-source-of-truth that holds in v1 erodes when a maintainer adds "just one quick docstring duplication" in v2. The skills-via-meta-tools that ship in v3 quietly turn back into hand-copied SKILL.md when a new adopter joins in v5 and doesn't read the SP. Discovery-canonical lasts only until someone optimizes a system prompt with "always start by calling X".

The way conventions don't degrade is by being **written down, illustrated with adopter examples, and re-read at the start of every adopter integration**. This document is that artifact. PR additions welcome — especially new anti-patterns observed in the wild.

### Related reading order (for a new adopter)

1. [`docs/atd-introduction.md`](atd-introduction.md) — what ATD is and why (5 min).
2. [`docs/architecture.md`](architecture.md) — system view (20 min).
3. **This doc** — adopter principles (15 min).
4. [`docs/integrations/overview.md`](integrations/overview.md) — bridge / adoption paths.
5. [`docs/integrations/python-server.md`](integrations/python-server.md) **or** the Rust `crates/atd-server/README.md` — server runtime hello-world.
6. The integration recipe for your target bridge (`integrations/hermes.md` / `claude-code.md` / `openclaw.md` / etc.).
7. [`docs/protocol/wire-format.md`](protocol/wire-format.md) — reference; consult as needed.

The first three are the *philosophy*. The rest are *execution*. The drift this document is meant to prevent comes from skipping straight to execution.
