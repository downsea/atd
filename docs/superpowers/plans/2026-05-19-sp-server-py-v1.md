# SP-server-py-v1: plan

| Spec | `docs/superpowers/specs/2026-05-19-sp-server-py-v1-design.md` |
| Filed | 2026-05-19 |
| Status | Phase A + B + C + D + E + F + G landed (96% coverage, 22/22 conformance subset); Phase H not started |
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

- [x] Create `python/src/atd_server/` directory + `__init__.py` (re-exports `AtdServer`, `Transport`, `UnixSocketTransport`).
- [x] Add `python/pyproject.toml` entry: `packages = ["src/atd_client", "src/atd_server"]` + mypy `packages = ["atd_client", "atd_server"]`.
- [x] `adapters/unix.py`: `UnixSocketTransport` wraps `asyncio.start_unix_server`; unlinks stale socket on bind + on close.
- [x] `adapters/__init__.py`: `Transport` Protocol (`start(on_connection)` + `close()`) + `ConnectionHandler` type alias.
- [x] `_runtime.py`: `install_signal_handlers(loop, on_stop)` (no-op on non-main-thread / Windows).
- [x] `server.py` minimal `AtdServer`:
  - Constructor: exactly one of `socket_path` or `transport` (asserted).
  - `register` / `middleware` stubs raising `NotImplementedError` with the Phase tag.
  - `serve()` binds → installs signal handlers → sets `_serving_event` → awaits `_stop_event`.
  - `wait_until_serving()` for tests (no `sleep` brittleness).
  - `stop()` + `_drain_and_close()` with configurable drain timeout.
  - Per-connection handler reads one frame via `atd_client.wire.read_frame`, echoes via `write_frame`, closes. (Throwaway — Phase C replaces.)
- [x] `python/tests/test_server_skeleton.py` (7 tests):
  - `test_round_trips_one_frame` — `{"type":"ping"}` byte-compat round-trip.
  - `test_stop_drains_quickly_with_no_clients` — drain <1s assertion.
  - `test_serve_twice_raises` — second `serve()` errors.
  - `test_constructor_requires_exactly_one_of_path_or_transport` — XOR validation.
  - `test_unlink_existing_clears_stale_socket` — pre-existing socket file cleared.
  - `test_register_and_middleware_are_phase_d_f_stubs` — stubs raise with phase tag.
  - `test_partial_frame_closes_cleanly` — half-header disconnect doesn't crash.
- [x] Run `pytest python/tests/test_server_skeleton.py -v`; green (7/7, 0.5s).
- [ ] Tag: `sp-server-py-v1-phase-b` (after commit lands).

**Exit criteria:** `nc -U /tmp/test.sock` round-trips one frame; cbrain can already point an `AtdClient` at it and see the frame echo (but no protocol semantics yet). ✅ met as of commit `<TBD>`.

---

## Phase C — Handshake + capability negotiation

Implement `Hello` → `HelloAck` with a configurable policy. Stash UCAN tokens on connection context (no verification yet).

**Spec correction landed in this phase:** the `Rust ref-server` does *not* enforce a "Hello first" state machine — `Request::Hello` is fully optional and may arrive at any point on a connection. The original Phase C plan said pre-Hello frames should error with `1005 not_handshaken`; that would have been a Python-only divergence and would have broken byte-compat. Phase C therefore treats Hello as optional: pre-Hello (and never-Hello) connections see `granted_capabilities=frozenset()`, and tool calls that require caps fail at dispatch time with `1001` in Phase E. No new error code; no state machine.

- [x] `context.py`: `ConnectionContext` frozen dataclass (`remote_addr` / `client_id` / `granted_capabilities` / `ucan_tokens` / `handshaken`) + `.with_hello(...)` returning a new context (immutable update). `CallContext` deferred to Phase E (handler-facing).
- [x] `policy.py`: `ServerPolicy` callable alias (`async (hello, ucan_tokens) -> GrantedCapabilities`); `default_policy()` grants all `requested_capabilities` verbatim, ignores UCAN; `GrantedCapabilities` frozen dataclass.
- [x] `handshake.py`: `negotiate_hello(raw, *, current_ctx, policy, server_version, supported_tiers)` → `(HelloAck dict, new ConnectionContext)`.
- [x] `server.py`: replace `_echo_one_frame` with `_serve_one_connection` (read → dispatch → write loop, strictly serial). `_dispatch`: `ping`→pong, `hello`→hello_ack via `negotiate_hello`, anything else → `Response::Error { code: 1099, message: "<type> not implemented in SP-server-py-v1 phase C" }`. Constructor grows `policy` / `server_version` / `supported_tiers` kwargs.
- [x] `__init__.py`: re-export `ConnectionContext`, `GrantedCapabilities`, `ServerPolicy`, `default_policy`.
- [x] `python/tests/test_server_handshake.py` (8 tests):
  - `test_ping_returns_pong` — vanilla round-trip.
  - `test_hello_default_policy_grants_all_requested` — `client_id` + `requested_capabilities=["fs.read", "fs.write"]` → granted equals requested; `server_version` + `supported_tiers` echo back.
  - `test_hello_custom_policy_can_deny_a_capability` — user-supplied policy filters out `*.write` capabilities.
  - `test_hello_passes_ucan_tokens_to_policy` — policy observes the `ucan_tokens` tuple.
  - `test_hello_can_be_resent_and_replaces_prior_grants` — second Hello on the same connection wins; matches Rust byte-compat.
  - `test_ping_works_before_hello` — Hello is optional; ping/hello/ping all interleave fine.
  - `test_unknown_message_type_returns_phase_c_stub_error` — Phase D/E surface returns 1099 with the type in the message.
  - `test_non_object_frame_returns_error` — JSON arrays / primitives are rejected with 1099 (defensive against malformed clients).
- [x] Phase B test cleanup: `test_round_trips_one_frame` and its `write_frame` import were specific to Phase B's throwaway echo handler — removed (Phase C `test_ping_returns_pong` is the replacement).
- [ ] Tag: `sp-server-py-v1-phase-c` (after commit lands).

**Exit criteria:** Python `AtdClient` can complete the handshake against `AtdServer` and read `granted_capabilities` back. ✅ met as of commit `<TBD>`.

---

## Phase D — Registry + tool_list + tool_schema

- [x] `registry.py`: `ToolRegistry` (register / summaries / describe / get / len); duplicate-id, sync-handler, empty-id errors; `_summary_from_definition` projects ToolDefinition → ToolSummary.
- [x] `server.py`: `@server.register(definition=...)` decorator wires through to registry.
- [x] Dispatch routing: `tool_list` → `registry.summaries(include_hidden=False)` → `{"type":"tool_list","tools":[...]}`. `tool_schema` → `registry.describe(tool_id)` → `{"type":"tool_schema","schema":...}` or `1000 not found: <id>` (the Python client's `describe()` substring-matches "not found").
- [x] **Drift fix:** `atd_client.types.ToolVisibility` was missing the `HIDDEN` variant that Rust serializes as `"hidden"` (per `crates/atd-protocol/src/enums.rs:86-95`). Without HIDDEN, the Pydantic enum's `_missing_` handler would reject any tool the Rust ref-server emitted with hidden visibility. One-line fix landed in this phase commit.
- [x] **Bug fix:** `_drain_and_close` log-counter was computing `len(self._connection_tasks) - len(pending)` AFTER `asyncio.wait` discarded done tasks via `done_callback` → could go negative. Snapshot `total = len(in_flight)` before the wait.
- [x] `python/tests/test_server_registry.py` (11 tests):
  - decorator transparency / duplicate id / sync handler / empty id rejection
  - `tool_list` returns registered summaries that re-parse as `ToolSummary`
  - HIDDEN tools excluded from `tool_list` but reachable via `tool_schema` by id
  - `tool_schema` unknown returns `1000 not found: ...`
  - missing `tool_id` returns `1099` envelope
  - `run_tool` still returns the Phase E stub error
  - `middleware` still raises NotImplementedError
- [x] `python/tests/test_server_lifecycle.py` (3 tests):
  - drain-with-idle-connection logs `drained 0 connections, 1 forced` (regression guard)
  - drain-with-no-connections logs the clean path
  - clean disconnect before stop counts cleanly (not forced)
- [x] `python/tests/_helpers.py` factored out — shared `spawn` / `stop_and_wait` / `round_trip` / `make_definition`.
- [ ] Tag: `sp-server-py-v1-phase-d` (after commit lands).

**Exit criteria:** Python `AtdClient.discover()` + `describe()` work end-to-end against `AtdServer` with registered tools. ✅ met as of commit `<TBD>`.

---

## Phase E — Dispatch (tier deadline + dry_run + capability gate + error envelope)

The meat of the SP. **cbrain can now start swapping the shim** after this phase.

- [x] `dispatch.py`:
  - `dispatch_run_tool(request, *, registry, conn_ctx, default_deadline_s)` → wire response dict.
  - Capability gate: required = `{f"{domain}:{action}" for action in capability.actions}`; missing = required - granted; deny ⇒ `1001` with details `{required, granted, missing}`.
  - `dry_run=True` short-circuit: returns `ToolSuccess(data={"args_preview": args})` before any handler/schema invocation.
  - JSONSchema validation of args (when `jsonschema` is installed via the `validation` extras) → `1005` on violation.
  - Deadline source: `definition.resources.timeout_ms` (with WARM=30s fallback when 0). v0.1.0 ToolDefinition has no `tier` field; the spec's HOT/WARM/COLD table is wired as fallback only.
  - `asyncio.wait_for(handler(args, ctx), timeout=deadline_s)`; `TimeoutError` → `1004`.
  - `ToolError(code, message, partial_data, retryable)` → typed envelope.
  - Unhandled `Exception` → `1099` envelope with `ExcClass` only (no traceback on wire); full traceback logged at ERROR.
  - `CancelledError` re-raised (don't swallow tokio-style cooperative cancellation).
  - Plain handler returns (dict / list / scalar) wrapped as `ToolSuccess`; `ToolSuccess` / `ToolFailure` returns unwrapped to the wire shape.
- [x] `errors.py`: `ToolError` exception + `ERR_*` constants + `build_error_response` / `build_tool_result_success` / `build_tool_result_failure` helpers.
- [x] `context.py`: `CallContext` frozen dataclass (`request_id`, `tool_id`, `granted_capabilities`, `connection`). `dry_run` intentionally absent — handlers never observe it (auto-short-circuit by dispatcher).
- [x] `server.py`: wired `dispatch_run_tool` into `_dispatch`.
- [x] `pyproject.toml`: `[project.optional-dependencies] validation = ["jsonschema>=4"]`. Without it, schema validation is skipped + debug log on import.
- [x] **Spec error-code drift fixed:** original §5.7 said `1002 invalid_arguments` and `1003 deadline_exceeded`. Both collided with Rust's `ERR_RATE_LIMITED=1002` / `ERR_BROKER_FAILED=1003`. Real allocation: `1004 DEADLINE_EXCEEDED`, `1005 INVALID_ARGS`. Spec §5.7 updated in this commit.
- [x] **Spec dry-run contract clarified:** the §4 cbrain example showed `if ctx.dry_run: return ToolSuccess(data={"args_preview": args})`. Per §G5 ("dry_run short-circuits without invoking handler"), `ctx.dry_run` would never be `True` inside a handler — so we removed `dry_run` from `CallContext` entirely. Adopter-controlled dry-run is out-of-scope for v1.
- [x] `python/tests/test_server_dispatch.py` (12 tests):
  - Happy path: handler return → success envelope; CallContext exposes `request_id`.
  - Handler returning `ToolSuccess` unwraps to `result.data`.
  - Handler returning `ToolFailure(code="2001", ...)` numerically coerces to `2001` on wire (cbrain 2000+ namespace ergonomic).
  - Capability denied with explicit details payload.
  - Capability denied even when no Hello sent at all (empty grant set).
  - Dry-run short-circuit: handler MUST NOT be invoked (asserted via a side-channel list).
  - 100ms timeout + handler sleep(0.5) → `1004 deadline exceeded`.
  - `ToolError(code=2042, message=..., partial_data=...)` round-trips with all fields.
  - `raise ValueError("boom")` → `1099`, message includes `ValueError`, "boom" text NOT on wire.
  - Unknown tool → `1000 not found`.
  - Missing `tool_id` → `1005`.
  - JSONSchema validation: `{"n": "not-an-int"}` against integer schema → `1005`.
- [x] Carry-over test cleanup: Phase D's `test_run_tool_is_phase_e_stub` (which expected the placeholder 1099) renamed and re-asserted as `test_run_tool_for_unknown_tool_returns_1000_after_phase_e`.
- [ ] Tag: `sp-server-py-v1-phase-e` (after commit lands).

**Exit criteria:** cbrain can register `perception.snapshot` / `manipulation.pick` / etc., call them from Hermes via `atd-mcp-bridge`, and see correct dispatch + deadlines + error envelopes. ✅ met as of commit `<TBD>`. **This is the cbrain swap-over point.**

---

## Phase F — Middleware (P2-8 bundled)

- [x] `middleware.py`: `MiddlewareStage` Literal type; `WrappingMiddlewareFn` / `ErrorMiddlewareFn` callable aliases; `MiddlewareChain` immutable container; `build_wrap_chain(...)` composes pre + post in registration order around the innermost handler call.
- [x] `server.py`: `@server.middleware(stage="pre_call" | "post_call" | "on_error")` decorator validates stage and appends to the appropriate list; `_dispatch` snapshots the lists into a `MiddlewareChain` per call and threads it through `dispatch_run_tool`.
- [x] `dispatch.py`: refactored exception handling around the chain. The handler invocation (now `innermost`) is wrapped by pre/post; the whole composed coroutine is wrapped in `asyncio.wait_for(deadline)`. Raised exceptions go through `_run_on_error_chain` first; if all return `None`, fall through to `_default_envelope_for_exception` which switches on `TimeoutError` / `ToolError` / other → `1004` / typed envelope / `1099`. `CancelledError` re-raised as before.
- [x] `python/tests/test_server_middleware.py` (8 tests):
  - `pre_call` returns without awaiting `call_next` → handler not invoked, typed failure on wire.
  - `post_call` mutates response (adds `_audited: True` marker).
  - **Ordering proof:** with `pre1`, `pre2`, `post1`, `post2` registered, the recorded event log is exactly `[pre1:enter, pre2:enter, post1:enter, post2:enter, handler, post2:exit, post1:exit, pre2:exit, pre1:exit]`.
  - `on_error` suppresses `ValueError` into a typed `ToolFailure` (and the original exception text MUST NOT appear on the wire).
  - `on_error` returning `None` falls through to the default `ToolError` envelope.
  - First non-None `on_error` short-circuits the rest of the chain.
  - An `on_error` middleware that itself raises is logged + skipped; subsequent middlewares get a chance to handle.
  - Unknown stage at decorator time → `ValueError`.
- [x] Phase B/D carry-over tests for the middleware stub updated (decorator is no longer a stub).
- [ ] Tag: `sp-server-py-v1-phase-f` (after commit lands).

**Exit criteria:** cbrain's Merkle audit example works against upstream `AtdServer` without a shim. ✅ met as of commit `<TBD>`. cbrain's P2-8 gap closes with this phase.

---

## Phase G — Tests + Python conformance subset

- [x] **Drift fix:** `atd_client.types.ToolDefinition` was missing the `required_capabilities: list[str]` field present in Rust `crates/atd-protocol/src/tool.rs:31`. Without it, capability gating couldn't follow the Rust convention of opaque strings compared directly (Rust gates on `definition.required_capabilities`, not on `capability.{domain, actions}`). Added as `Field(default_factory=list)`; existing payloads without the field continue to parse.
- [x] **Dispatch refactor:** `dispatch_run_tool` now uses `definition.required_capabilities` instead of computing `f"{domain}:{action}"`. Removed `_required_capability_strings`. Conformance fixtures' simpler `"read"` / `"conformance.denied"` strings now match.
- [x] `_helpers.make_definition` grows a `required_capabilities=[...]` kwarg (the structured `capability` block stays for metadata).
- [x] Phase E dispatch tests updated to use the flat string model (`required_capabilities=["read"]` + grant `["read"]`).
- [x] `python/tests/test_server_conformance.py` (22 fixtures + 1 meta-test = 23 tests):
  - Reads `crates/atd-conformance/fixtures/{wire,behavior}/*.json` directly (no copy).
  - Skips `rate_limited_returns_code_1002` (rate limit not in v1) and `frame_length_big_endian_u32` (codec test already covered by `atd_client.wire` unit tests).
  - Builds a fresh `AtdServer` with a reference policy (`granted_capabilities ⊆ {"read"}`) and four reference tools (`ref:echo.say` / `ref:fs.read` / `ref:conformance.denied_op` / `ref:conformance.hidden_op`) per parametrize.
  - Recursive `_matches_subset` honors the `"*"` wildcard convention.
  - Honors fixture extras: `setup.kind == "hello"`, `expect_tools_exclude`.
  - A meta-test asserts the runnable count stays ≥18; future fixture removal upstream is visible.
- [x] `python/tests/test_server_lifecycle.py` (3 tests landed in Phase D, expanded conceptually here): drain-with-idle-connection log assertion + clean-disconnect + no-in-flight path. The Phase G plan items about `SIGTERM` and `serve()-after-stop()` already covered by `test_serve_twice_raises` and signal-handler skip path in `_runtime.py`.
- [x] **Coverage gate**: `pytest --cov=atd_server` reports **96% on `atd_server/*`** (target was ≥80%). Missing 4% is hard-to-exercise defensive branches: signal-handler skip on non-main-thread, no-policy-bound-on-malformed-request defensive paths, log-only error branches.
- [ ] Tag: `sp-server-py-v1-phase-g` (after commit lands).

**Exit criteria:** Python server passes the subset of `atd-conformance` fixtures that exercise its surface; coverage gate green. ✅ met (22/22 conformance fixtures, 96% coverage) as of commit `<TBD>`.

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
- **`asyncio.wait_for` cancellation semantics.** Python 3.11 changed `wait_for` to wrap inner cancel into `CancelledError` *after* the inner task observes it. `python/pyproject.toml` floors at `>=3.10`, so v1 must work under both 3.10 (legacy `wait_for`) and 3.11+ (new semantics). Phase E's dispatch wrapper translates both behaviors into the same `1003 deadline_exceeded` envelope.
- **Windows.** No Unix sockets. v1 explicitly Linux/macOS only. cbrain runs on Linux; healthkit on macOS+Linux; not a real blocker.
- **`signal.add_signal_handler` not available inside non-main threads.** If an adopter calls `server.serve()` from a worker thread, signal handlers silently fail to install. Document; consider raising at `serve()` entry if `threading.current_thread() is not threading.main_thread()`.
- **Test flakiness from real UDS bind under parallel pytest.** Use `tempfile.TemporaryDirectory()` per test so socket paths don't collide. Mirror the Rust ref-server's `bind 127.0.0.1:0` pattern with UDS `tempdir/test.sock`.
