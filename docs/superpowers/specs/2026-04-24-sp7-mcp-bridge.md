# SP-7 — MCP Bridge End-to-End Validation Design Spec

**Date:** 2026-04-24
**Status:** Design approved; plan pending.
**Scope:** Sub-project 7. Flip `atd-mcp-bridge` default from ANOS to neutral (`--sock` or `ATD_SOCK`), add a deterministic CI e2e test that exercises the full `MCP client → bridge → atd-ref-server` chain, capture a Hermes LLM chat transcript as real-agent evidence, ship a validation doc.
**Builds on:** SP-6 (`sp6-ref-server-capstone`) — 245 workspace tests, 9 tools, hello_atd demos auto-spawn ref-server.

---

## 1. Motivation

SP-6 proved atd-client + atd-ref-server work end-to-end without ANOS. What's still missing is the cross-vendor MCP story: can a non-ANOS MCP client (Hermes, Claude Desktop, Cursor, any MCP-speaking agent) talk to our neutral ATD server via the bridge that already exists in the workspace? If yes, the "ATD is a neutral cross-vendor protocol" claim is validated at the full protocol-stack level — not just for Rust SDK users, but for the entire MCP ecosystem.

`atd-mcp-bridge` is already implemented (stdio-based JSON-RPC 2.0, forwards MCP `tools/list` + `tools/call` to an ATD server). Its only remaining ANOS coupling is `Endpoint::default_anos()` in `main.rs`. SP-7 removes that coupling, adds CI coverage proving the chain works without real agents in the loop, and captures a Hermes-with-LLM transcript for the "real agent" part of the story.

---

## 2. Scope

### 2.1 In scope

1. **Bridge default change** — `atd-mcp-bridge` requires `--sock PATH` or `ATD_SOCK` env var. Error 2 with clear message if neither.
2. **Deterministic e2e test** — `crates/atd-mcp-bridge/tests/integration_e2e.rs` with 4-5 tests covering MCP initialize handshake, tools/list, tools/call success + error paths. Spawns real `atd-ref-server` + real `atd-mcp-bridge` subprocesses, pipes real JSON-RPC through stdio.
3. **Hermes transcript** — one captured chat interaction where the LLM autonomously picks an ATD tool via MCP, gets a real response, and replies to the user. Prompt suggestion: *"What kernel is this machine running?"* → LLM picks `ref:shell.exec`. Transcript verbatim (truncate timestamps, redact API keys).
4. **Validation doc** at `docs/validation/2026-04-24-sp7-mcp-bridge.md` — claim, e2e evidence, Hermes transcript, bridge config snippet.
5. **Tag** `sp7-mcp-bridge-validated`.

### 2.2 Explicitly deferred (Phase 2+)

- MCP protocol extensions (ATD-specific capabilities in the handshake).
- Bidirectional notifications / streaming.
- Bridge daemonization / process supervisor.
- Claude Desktop / Cursor / OpenAI Codex compatibility matrix.
- Tool-call logging to a file (operators can redirect stderr).
- Multiple concurrent MCP clients sharing one bridge (current 1:1 is correct for MCP).

### 2.3 Prerequisites

- atd-ref-server at tag `sp6-ref-server-capstone`, 245 tests green.
- For the Hermes step only: Hermes CLI installed, configured with a working LLM backend (Anthropic API key or local Ollama). CI doesn't require this.

---

## 3. Bridge default change

Current `crates/atd-mcp-bridge/src/main.rs` (approximate shape):

```rust
let endpoint = match sock_path {
    Some(p) => Endpoint::unix(p),
    None => Endpoint::default_anos(),
};
```

After SP-7:

```rust
let sock = sock_path
    .or_else(|| std::env::var("ATD_SOCK").ok().map(PathBuf::from))
    .ok_or_else(|| {
        "atd-mcp-bridge: no target socket configured.\n\
         specify --sock PATH or set ATD_SOCK=/path/to/atd-server.sock"
    })?;
let endpoint = Endpoint::unix(sock);
```

- Error exit code: `2` (matches existing `atd-mcp-bridge: unknown arg: ...` exit 2).
- Error message written to stderr; stdout stays clean (MCP clients expect protocol framing on stdout).
- No change to `--help` behavior beyond mentioning the env var.

---

## 4. Integration test shape

Full file: `crates/atd-mcp-bridge/tests/integration_e2e.rs`.

### 4.1 Harness

Shared helper that spawns both binaries:

```rust
struct BridgeHarness {
    _ref_server: Child,       // killed in Drop
    bridge: Child,            // killed in Drop
    _tmp: TempDir,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl BridgeHarness {
    async fn spawn() -> Result<Self> {
        // 1. Build-guard: check target/release/atd-ref-server and atd-mcp-bridge exist.
        //    If missing, return Err with "build first: cargo build --release ..."
        // 2. tempdir; sock path = tempdir/sp7.sock
        // 3. Spawn atd-ref-server, wait for sock (100ms * 30)
        // 4. Spawn atd-mcp-bridge --sock <sock>, capture stdin/stdout
        //    stderr → null (bridge logs there but we don't care in tests)
        // 5. Return the handles
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        // Write JSON-RPC request to stdin (with line framing per MCP).
        // Read one response line from stdout.
        // Increment next_id.
    }
}

impl Drop for BridgeHarness {
    fn drop(&mut self) {
        // Best-effort kill of both children.
    }
}
```

### 4.2 Tests (4 or 5)

1. **`e2e_mcp_initialize_handshake`** — send `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}`. Expect response with matching id, `result.protocolVersion` field, `result.capabilities.tools` present.

2. **`e2e_mcp_tools_list_returns_ref_server_tools`** — after init, send `tools/list`. Expect `result.tools` array with 9 items including `ref:echo.say`, `ref:fs.read`, `ref:shell.exec`, etc. Validate at least the names are present (don't over-specify schemas — the bridge forwards ATD types to MCP types, and we shouldn't make tests fragile to schema tweaks).

3. **`e2e_mcp_tools_call_echo_success`** — call `ref:echo.say` with `{"text":"hello mcp"}`. Expect `result.content` array with some text representation of the echoed payload. Validate `isError` is falsy / absent.

4. **`e2e_mcp_tools_call_shell_exec_real_command`** — call `ref:shell.exec` with `{"command":"echo capstone-ok"}`. Expect the result contains `capstone-ok` in the textual content.

5. **`e2e_mcp_tools_call_bad_args_returns_error`** — call `ref:fs.read` without required `path` param. Expect MCP error response OR `content` with `isError: true` — whichever shape the bridge emits. Validate SOMETHING signals error (the point is the bridge propagates tool errors, not how they're spelled).

### 4.3 Resilience

- Tests must pass regardless of build-host timing (spawn retry budget 3s for each binary).
- Drop on the harness must clean up subprocesses even if a test panics.
- If binaries aren't built, tests fail loudly with the build-first message — no silent skip.

---

## 5. Hermes transcript capture

Prerequisite (documented in the validation doc, NOT enforced by CI):

- Hermes CLI at `$HERMES_BIN` or on PATH.
- Hermes configured with an MCP server entry pointing to the bridge binary:

```toml
# ~/.config/hermes/mcp.toml (illustrative — exact format per Hermes docs)
[[mcp.server]]
name = "atd-ref"
command = "/path/to/atd-mvp/target/release/atd-mcp-bridge"
env.ATD_SOCK = "/tmp/sp7-demo.sock"
```

Demo session:

```
$ atd-ref-server --sock /tmp/sp7-demo.sock &
$ hermes chat
you> What kernel is this machine running? Use an ATD shell tool to check.
<LLM decides to call ref:shell.exec with "uname -s">
agent> uname -s returned "Linux". This machine is running Linux.
```

Capture the full transcript (including tool-call trace, typically printed by Hermes at verbosity level 1+) into the validation doc. Redact:
- Any API keys / secrets in env
- Full PIDs and socket paths (keep prefixes; `/tmp/sp7-demo.sock` is fine to show)
- Timestamps (optional — keep if they don't clutter)

---

## 6. Validation doc outline

Path: `docs/validation/2026-04-24-sp7-mcp-bridge.md`. Target 600-1000 words.

### Section 1 — Claim
Two paragraphs: (a) SP-6 closed the atd-client ↔ atd-ref-server independence gap; SP-7 closes the MCP-ecosystem ↔ atd-ref-server gap. (b) Evidence structure.

### Section 2 — e2e test transcript
`cargo test -p atd-mcp-bridge --test integration_e2e` output. 4-5 passing tests = deterministic CI proof.

### Section 3 — Hermes LLM transcript
Verbatim session (redacted). One paragraph commentary pointing out: the LLM chose the tool autonomously, the tool ran, the LLM synthesized a user-facing response.

### Section 4 — Bridge config snippet for external users
TOML (or whatever Hermes format, plus a generic MCP config format) showing how any MCP client can point at our bridge + ref-server.

### Section 5 — Dependency isolation
Quick `cargo tree -p atd-mcp-bridge | head -20` + note showing no `anos-*`.

### Section 6 — What remains
Phase 2 items: streaming, richer MCP capabilities, Claude Desktop / Cursor matrix.

---

## 7. Risks and non-risks

### 7.1 Risks

- **MCP protocol version drift.** The MCP spec has revisions; the bridge implementation may lag. Mitigation: tests use `protocolVersion: "2025-06-18"` (the version SP-0.5 was validated against); if Hermes ships newer, adjust.
- **Bridge stderr noise.** Bridge logs connection info to stderr. Tests should discard stderr (`Stdio::null()`), not mingle it with JSON-RPC on stdout.
- **Hermes environment drift.** The user's Hermes config may not exist on a fresh machine. Mitigation: validation doc shows exact config; assume Hermes already works locally (user has been using Hermes for the whole MVP).
- **LLM non-determinism in transcript.** The captured transcript is a snapshot, not a reproducible test. Accept this; the CI e2e tests give the reproducible proof.

### 7.2 Non-risks

- **atd-mcp-bridge correctness** — already covered by the crate's existing unit tests. SP-7 tests it end-to-end.
- **atd-ref-server correctness** — SP-1 to SP-5 shipped with 243 tests.
- **Dependency licensing** — atd-mcp-bridge imports atd-client + atd-types only; no new deps in SP-7.

---

## 8. Exit criteria

1. `atd-mcp-bridge --sock ...` or `ATD_SOCK=... atd-mcp-bridge` works; without either, bridge exits 2 with clear message.
2. `cargo test -p atd-mcp-bridge --test integration_e2e` — 4 or 5 tests pass.
3. `cargo test --workspace --all-targets` — 249-250 tests, 0 failures.
4. Hermes chat transcript captured and pasted into the validation doc.
5. `docs/validation/2026-04-24-sp7-mcp-bridge.md` committed.
6. Tag `sp7-mcp-bridge-validated` created.
7. `grep ANOS\|anos crates/atd-mcp-bridge/src/main.rs` empty.

---

## 9. Out of scope forever at this layer

- Bridge-level ACL / auth (MCP doesn't have this; out-of-band deployment concern).
- Tool result transformation (bridge is a pure forwarder, not a filter).
- Protocol version negotiation (bridge reports whatever the MCP lib supports).

SP-7 is narrow by design: flip a default, add e2e tests, capture one real transcript. Keep scope tight so the validation story stays crisp.
