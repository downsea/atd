# SP-server-py-v1: plan

| Spec | `docs/superpowers/specs/2026-05-19-sp-server-py-v1-design.md` |
| Filed | 2026-05-19 |
| Status | Phase A landed; Phase B not started |
| Target alpha | ~2 weeks from Phase B kickoff (cbrain W1 alignment) |
| Owner | TBD (whoever picks up Phase B first) |

---

## Phase A — Spec land (this commit)

- [x] Author `docs/superpowers/specs/2026-05-19-sp-server-py-v1-design.md`.
- [x] Author this plan.
- [x] Triage cbrain issue (`docs/issues/2026-05-19-cbrain-adopter-requirements.md` §9).
- [x] Re-status the cbrain issue in `docs/issues/README.md`.
- [ ] Tag: `sp-server-py-v1-spec` (after commit lands).

**Exit criteria:** spec + plan + triage merged on master; cbrain team has a referenceable design doc.

---

## Phase B — Skeleton

Land the package shell. No protocol logic; goal is an end-to-end "accept a connection, echo one frame, close" path so subsequent phases can fill in real behavior.

- [ ] Create `python/src/atd_server/` directory + `__init__.py` (empty re-exports for now).
- [ ] Add `[tool.uv.workspace]` / `pyproject.toml` entry for the new package; ensure `pip install -e python/` picks it up.
- [ ] `adapters/unix.py`: implement `UnixSocketTransport` (`bind`, `accept`, `close`); unlink stale socket file by default.
- [ ] `adapters/__init__.py`: define `Transport` Protocol.
- [ ] `_runtime.py`: `signal_handlers_install(loop, on_stop)` (Unix only; no-op on Windows).
- [ ] `server.py` minimal `AtdServer`:
  - Constructor accepts `socket_path: str` or `transport: Transport`.
  - `register` and `middleware` are stubs raising `NotImplementedError`.
  - `serve()` runs accept loop; per-connection task reads one frame, echoes it back, closes. (Throwaway — Phase C replaces.)
  - `stop()` works (sets stop event, closes transport).
- [ ] `python/tests/test_server_skeleton.py`:
  - Spawn `AtdServer`, connect via `socket.socket(AF_UNIX)`, send 4-byte length + JSON `{"type":"ping"}`, read back the echoed frame, assert equality.
  - Assert `server.stop()` drains in <1s.
- [ ] Run `pytest python/tests/test_server_skeleton.py -v`; green.
- [ ] Tag: `sp-server-py-v1-phase-b`.

**Exit criteria:** `nc -U /tmp/test.sock` round-trips one frame; cbrain can already point an `AtdClient` at it and see the frame echo (but no protocol semantics yet).

---

## Phase C — Handshake + capability negotiation

Implement `Hello` → `HelloAck` with a configurable policy. Stash UCAN tokens on connection context (no verification yet).

- [ ] `context.py`: implement `ConnectionContext` + `CallContext` frozen dataclasses.
- [ ] `policy.py`: `ServerPolicy` Protocol (`async def __call__(hello, ucan_tokens) -> GrantedCapabilities`); `default_policy()` returning "grant all requested"; `GrantedCapabilities` dataclass.
- [ ] `handshake.py`: `negotiate_hello(raw_msg, policy, server_id) -> tuple[HelloAck dict, ConnectionContext]`.
- [ ] `server.py`: replace echo with state machine — pre-Hello → post-Hello. Before Hello: only accept `hello` / `ping`; reject others with `1005 not_handshaken`. After Hello: dispatch via stub that still returns `1099 not_implemented`.
- [ ] `python/tests/test_server_handshake.py`:
  - Client sends `hello` with `client_id="test"` + `requested_capabilities=["fs.read"]`; expect `hello_ack` with `server_id` + `granted_capabilities=["fs.read"]`.
  - Client sends `tool_list` *before* Hello → expect `Response::Error { code: 1005 }`.
  - Custom `ServerPolicy` denies one capability → assert granted set excludes it.
  - UCAN tokens passed in Hello → assert `conn_ctx.ucan_tokens == ("token1", "token2")`.
- [ ] Tag: `sp-server-py-v1-phase-c`.

**Exit criteria:** Python `AtdClient` can complete handshake against `AtdServer` and read `granted_capabilities` back.

---

## Phase D — Registry + tool_list + tool_schema

- [ ] `registry.py`: `ToolRegistry` (register / summaries / describe / get); duplicate-id error; sync-handler rejection.
- [ ] `server.py`: `@server.register(definition=...)` decorator wires through to registry.
- [ ] Dispatch routing: `tool_list` → `registry.summaries(include_hidden=False)` → wrap as `Response::ToolList`. `tool_schema` → `registry.describe(tool_id)` → wrap as `Response::ToolSchema` or `1000 tool_not_found`.
- [ ] `python/tests/test_server_registry.py`:
  - Register 3 tools (one HIDDEN); `tool_list` returns 2 summaries.
  - `tool_schema` on registered id returns definition; on unknown id returns `1000`.
  - Duplicate registration raises `ValueError`.
  - Sync handler registration raises `TypeError`.
- [ ] Tag: `sp-server-py-v1-phase-d`.

**Exit criteria:** Python `AtdClient.discover()` + `describe()` work end-to-end against `AtdServer` with registered tools.

---

## Phase E — Dispatch (tier deadline + dry_run + capability gate + error envelope)

The meat of the SP. cbrain can already start swapping the shim after this phase.

- [ ] `dispatch.py`:
  - `dispatch_run_tool(request, registry, conn_ctx, granted_caps, tier_deadlines, middleware_chain) -> Response dict`.
  - Capability gate: `definition.capabilities ⊆ granted_caps` else `1001`.
  - `dry_run=True` short-circuit: return `ToolSuccess(data={"args_preview": validated_args})`.
  - JSONSchema validation of args (if `jsonschema` is installed) → `1002` on violation.
  - `_tier_to_deadline(tier)` lookup with per-server override.
  - `asyncio.wait_for(handler(args, ctx), timeout=deadline_s)`; `TimeoutError` → `1003`.
  - `ToolError` → envelope with `exc.code` / `exc.message` / `exc.partial_data`.
  - Unhandled `Exception` → `1099 internal_error: <ExcClass>`; log full traceback at ERROR.
- [ ] `errors.py`: `ToolError` exception; `ERR_*` code constants mirrored from `atd_client.errors`; `_build_error_response(request_id, code, message)` helper.
- [ ] `server.py`: wire dispatch into per-connection state machine.
- [ ] Add optional dep on `jsonschema` in `python/pyproject.toml` extras; without it, schema validation is skipped + warn-once.
- [ ] `python/tests/test_server_dispatch.py`:
  - Register echo tool; client `call` returns success.
  - Register a tool needing `fs.write`; client granted only `fs.read` → `1001`.
  - `dry_run=True` returns `args_preview` without invoking handler (use a handler that raises if called).
  - Tool that sleeps 2s with `tier=HOT (1s)` → `1003 deadline_exceeded`.
  - Handler raises `ToolError(2001, "cbrain failure")` → envelope carries code=2001.
  - Handler raises `ValueError("boom")` → envelope carries `1099`, message includes "ValueError"; traceback logged but not on wire.
  - Args fail JSON schema → `1002`.
- [ ] Tag: `sp-server-py-v1-phase-e`.

**Exit criteria:** cbrain can register `perception.snapshot` / `manipulation.pick` / etc., call them from Hermes via `atd-mcp-bridge`, and see correct dispatch + deadlines + error envelopes. **This is the cbrain swap-over point.**

---

## Phase F — Middleware (P2-8 bundled)

- [ ] `middleware.py`:
  - `pre_call_mw` / `post_call_mw` / `on_error_mw` signatures (per spec §5.6).
  - Chain builder: wrap `run = handler(...)` inside-out with `post_call` then `pre_call`; `on_error` chain runs in registration order, first non-None return suppresses.
- [ ] `server.py`: `@server.middleware(stage="pre_call" | "post_call" | "on_error")` decorator.
- [ ] Dispatch integrates middleware chain.
- [ ] `python/tests/test_server_middleware.py`:
  - `pre_call` registered first sees request before the handler; can short-circuit by returning `ToolFailure`.
  - `post_call` can mutate response (e.g., add metadata).
  - Middleware execution order: pre 1 → pre 2 → handler → post 2 → post 1 (LIFO around handler).
  - `on_error` sees `ValueError`; returning `ToolFailure(1099, "caught")` suppresses; returning `None` re-raises (which falls back to the dispatch envelope).
  - cbrain-style Merkle audit example as a doctest in `docs/integrations/python-server.md`.
- [ ] Tag: `sp-server-py-v1-phase-f`.

**Exit criteria:** cbrain's Merkle audit example works against upstream `AtdServer` without a shim.

---

## Phase G — Tests + Python conformance subset

- [ ] `python/tests/test_server_conformance.py`:
  - Read `crates/atd-conformance/fixtures/` JSON files (use a pytest fixture to glob them).
  - For each fixture in the v1-relevant subset (handshake / tool_list / tool_schema / dispatch / capability_denied / dry_run / visibility — approximately 10 of 36), drive `AtdServer` through the scripted requests and assert responses match expected.
  - Skip fixtures requiring rate-limit / pagination / UCAN-verify / audit-replay (out of v1 scope).
- [ ] `python/tests/test_server_lifecycle.py`:
  - `SIGTERM` triggers `stop()`; in-flight handlers see `asyncio.CancelledError` after `drain_timeout_s`.
  - `serve()` after `stop()` raises `RuntimeError("server already stopped")`.
  - Stale UDS file is unlinked by default; respect `unlink_existing=False`.
- [ ] `pytest python/tests/ --cov=atd_server --cov-report=term-missing` → ≥80% coverage on `atd_server/*`.
- [ ] Tag: `sp-server-py-v1-phase-g`.

**Exit criteria:** Python server passes the subset of `atd-conformance` fixtures that exercise its surface; coverage gate green.

---

## Phase H — Documentation

- [ ] `docs/integrations/python-server.md`: new file. Cbrain-style hello-world (tier, capability, middleware, error envelope, graceful shutdown). Cross-link to spec + cbrain issue.
- [ ] `docs/architecture.md`: §8 crate / package table grows a row for `atd_server` (Python).
- [ ] `python/README.md`: add "Server runtime" section pointing at the integrations doc.
- [ ] `CLAUDE.md`: append `atd_server` to the Python mirror line (currently mentions only `python/src/atd_client/`).
- [ ] Umbrella tag: `sp-server-py-v1`.
- [ ] Close cbrain issue's P0-1 sub-status; bump cbrain umbrella issue toward "P0-1 done; pending P0-2 / P1-3 / etc."

**Exit criteria:** cbrain swaps the shim → upstream within 1 week; deletes `cbrain/sim/cbrain_sim/atd_shim/`; cbrain issue P0-1 row goes green.

---

## Cross-phase invariants (preserve through every phase)

1. **Wire byte-compat with `crates/atd-protocol` v0.1.0.** No private fields, no Python-only message types, no relaxed validation. Every frame on the wire must be acceptable to the Rust `atd-sdk` and vice versa.
2. **Reuse `atd_client.wire` / `atd_client.protocol` constants.** Don't fork. If a Python-only helper is genuinely needed (e.g., a stricter JSON schema validator), put it in `atd_server/` not in `atd_client/`.
3. **No Phase-2 features sneak in.** If the implementation reaches for `request_id` routing, multi-request-per-connection, or chunked responses, stop — that's `SP-cancel-streaming-v1`'s territory. Leave the seam; don't fill it.
4. **Handler signature is `async def (args, ctx)`.** Sync handlers are rejected (TypeError at registration). Auto-wrapping sync into a threadpool hides blocking calls that stall the reactor.
5. **Errors carry code + message + class name only on wire.** Tracebacks log-only.
6. **Tests at every phase.** No phase tag without a green pytest run for that phase's surface. Coverage gate enforced at Phase G but `pytest` must stay green throughout.

---

## Risks / known unknowns

- **`jsonschema` dep weight.** ~150KB wheel; pulls `attrs`, `jsonschema-specifications`, `rpds-py`. If a Python-conscious adopter complains, demote to extras-only and skip-with-warning if missing.
- **`asyncio.wait_for` cancellation semantics.** Python 3.11 changed `wait_for` to wrap inner cancel into `CancelledError` *after* the inner task observes it. We target Python ≥3.11 (matches `python/pyproject.toml` today); document the gotcha for adopters porting from 3.10.
- **Windows.** No Unix sockets. v1 explicitly Linux/macOS only. cbrain runs on Linux; healthkit on macOS+Linux; not a real blocker.
- **`signal.add_signal_handler` not available inside non-main threads.** If an adopter calls `server.serve()` from a worker thread, signal handlers silently fail to install. Document; consider raising at `serve()` entry if `threading.current_thread() is not threading.main_thread()`.
- **Test flakiness from real UDS bind under parallel pytest.** Use `tempfile.TemporaryDirectory()` per test so socket paths don't collide. Mirror the Rust ref-server's `bind 127.0.0.1:0` pattern with UDS `tempdir/test.sock`.
