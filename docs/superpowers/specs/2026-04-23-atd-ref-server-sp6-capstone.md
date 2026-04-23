# atd-ref-server — SP-6 Capstone Design Spec

**Date:** 2026-04-23
**Status:** Design approved; plan pending.
**Scope:** Sub-project 6 (final). Cut the ANOS dependency from the default demo path. Rewrite `examples/hello_atd.{rs,py}` to auto-spawn `atd-ref-server` and exercise 3 representative tools. Ship a validation doc that proves end-to-end that atd-mvp has zero runtime dependency on ANOS.
**Builds on:** SP-5 (`sp5-ref-server-web`) — 243 Rust workspace tests, 9 tools registered (echo + 5 fs + 2 shell + 1 web).

---

## 1. Motivation

Since SP-1, `atd-ref-server` has grown into a real reference ATD server: 9 tools across 4 domains, clean-room implementation, Apache-2.0-compatible deps, full test + integration coverage. But the *story* of the project — "atd-mvp is the independent reference implementation of the ATD protocol" — still has one loose thread: the existing `hello_atd.rs` / `hello_atd.py` examples default to the ANOS daemon socket. A curious reader following the `README.md → Quick start` path currently needs a running ANOS instance to see anything work.

SP-6 fixes that. It makes "clone the repo, build, run the example" a two-command story — with `atd-ref-server` doing the actual serving, ANOS never installed, the ATD protocol proving itself end-to-end.

Once this ships, the project fully lives up to its identity statement in `CLAUDE.md`:
> atd-mvp is the independent reference implementation of the ATD protocol and client SDK.

---

## 2. Scope

### 2.1 In scope

1. **Rewrite `examples/hello_atd.rs`** — auto-spawn `atd-ref-server` as a tokio child process, wait for its Unix socket, connect via `atd-client`, exercise 3 tools (`ref:echo.say` → `ref:fs.glob` → `ref:shell.exec`), tear down on exit. Respects `ATD_SOCK` env override (skips spawn, connects to the given socket).
2. **Rewrite `python/examples/hello_atd.py`** — parallel shape: `asyncio.create_subprocess_exec` spawns the Rust binary, connects via the Python SDK, exercises the same 3 tools, same output format.
3. **Validation doc** at `docs/validation/2026-04-23-sp6-capstone.md` — evidence-based claim with transcripts, cargo tree independence, license audit, before/after example diffs.
4. **README + CLAUDE.md updates** — replace "Phase 0 demo depends on ANOS" language with capstone reality.
5. **Tag `sp6-ref-server-capstone`** — marks the end of the MVP's initial development arc.

### 2.2 Explicitly deferred

- **Demo video** — human task, not a coding deliverable.
- **Public GitHub push / release announcement** — governance/ops, not code.
- **Conformance test suite** — scoped to Phase 2 per original design.
- **SDK packaging for crates.io / PyPI** — Phase 2; the examples still use relative paths.
- **Cross-OS portability testing** — examples are tested on Linux only in this SP; macOS / Windows parity is a future concern.

### 2.3 Prerequisites

- atd-ref-server at tag `sp5-ref-server-web`, 243 workspace tests green.
- `atd-client` (Rust) and `atd_client` (Python SDK) already exist from Phase 0 / Phase 1. Both handle the length-prefixed JSON wire format over Unix sockets.
- `cargo build --release -p atd-ref-server` has been run before `cargo run --example hello_atd`. The example fails loudly with a build-instruction message if the binary is missing.

---

## 3. The demo

Both language variants print the same structured output. Exact template:

```
[atd] auto-spawning atd-ref-server → <socket path>
[atd] connected
[atd] 9 tools registered

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"echoed":"hello from ATD","call_id":"..."}

[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → <N> paths: Cargo.toml, crates/atd-types/Cargo.toml, ...

[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout="Linux\n"

[atd] done. Shutting down ref-server.
```

### 3.1 Why these three tools

- `ref:echo.say` — call with a deterministic argument, proves basic request/response works.
- `ref:fs.glob` — the `**/*.toml` pattern is deterministic (the project has a known small number of Cargo manifests), proves the walker.
- `ref:shell.exec` — `uname -s` returns a one-word platform string, proves subprocess + stdout capture.

These three span three different safety levels (`Read`, `Read`, `Destructive`) and three domains (`echo`, `fs`, `shell`). A reader sees: "this server can message, walk files, and shell out." One minute and they get the pitch.

No `web.fetch` call in the demo: requires network, can fail in sandboxed CI. Mentioned in the validation doc as "also available" with a link to SP-5 integration tests.

### 3.2 Auto-spawn pattern

Algorithm (both languages):

1. If `ATD_SOCK` is set, connect to it directly. Skip spawn. (Enables demoing against ANOS or any other ATD server.)
2. Otherwise, verify `target/release/atd-ref-server` exists. If not, error out with:
   ```
   error: atd-ref-server release binary not found at target/release/atd-ref-server.
   build it first: cargo build --release -p atd-ref-server
   ```
3. Create a tempdir. Path: `<tempdir>/demo.sock`.
4. Spawn: `atd-ref-server --sock <tempdir>/demo.sock`.
5. Poll for socket existence every 100 ms, up to 3 s total. Timeout → kill child, error out.
6. Connect via the SDK. Run demo. Print output.
7. On natural completion OR ctrl-C: kill child, remove tempdir.

Rust: `tokio::process::Child` + `tokio::signal::ctrl_c()` + `tempfile::tempdir()`.
Python: `asyncio.create_subprocess_exec` + `asyncio.Event` trap for SIGINT + `tempfile.TemporaryDirectory`.

### 3.3 Error surface

- Missing binary → exit 1 with the build-instruction message above.
- Spawn fails → exit 1, print spawn error.
- Socket doesn't appear within 3 s → exit 1, kill child, print "ref-server didn't bind its socket in time".
- Any tool call returns `success: false` → print the error and continue to the next tool. Full demo doesn't abort — a bad call is still a demo of the protocol working.
- ctrl-C → tear down cleanly, exit 130.

---

## 4. Validation doc

Path: `docs/validation/2026-04-23-sp6-capstone.md`. Structure (tight — target ~200 lines total):

### Section 1: Claim
Two paragraphs framing the "independence from ANOS" thesis and what this document delivers.

### Section 2: Evidence 1 — Rust E2E transcript
Full captured `cargo run --example hello_atd` output (the stdout from section 3). Two-line commentary on each of the 3 tool calls.

### Section 3: Evidence 2 — Python E2E transcript
Same shape for `uv run python examples/hello_atd.py`. Commentary limited to what *differs* from Rust output (UUID values, etc.).

### Section 4: Evidence 3 — dependency isolation
Shell-captured `cargo tree -p atd-ref-server --prefix none | head -30`. Prose note: no `anos-*`, no `atd-client`, no `atd-mcp-bridge`, no `atd-cli` in the tree.

### Section 5: Evidence 4 — license audit
Run `cargo license -p atd-ref-server --json | jq ...` (or fallback: `cargo tree --format='{p} {l}'`) and distill to a license-class table. All direct deps and their licenses listed. All must be MIT / Apache-2.0 / BSD-*.

### Section 6: Evidence 5 — diff
Before/after `examples/hello_atd.rs` diff. Annotated: the `ANOS_SOCK` and `Endpoint::default_anos()` lines disappear; their replacements are standard tempfile + subprocess code. No `anos-*` anywhere.

### Section 7: What remains for Phase 2+
Bulleted list: demo video, conformance suite, public release, cross-OS validation. Each with a one-line reason.

Short. Evidence-heavy. Intended to stand on its own so a third party can verify the claim without reading 5 prior design specs.

---

## 5. File structure

```
/
├── examples/
│   └── hello_atd.rs                       (MODIFY — Task 1)
├── python/examples/
│   └── hello_atd.py                       (MODIFY — Task 2)
├── docs/validation/
│   └── 2026-04-23-sp6-capstone.md         (NEW — Task 3)
├── README.md                               (MODIFY — Task 4)
└── CLAUDE.md                               (MODIFY — Task 4)
```

No new crates, no new crate-level deps. Example crates already have `tokio`, `tempfile`, `serde_json`. Python already has `tempfile`, `asyncio`, `pathlib`.

---

## 6. Test plan

### 6.1 What IS tested

- `cargo run --example hello_atd` runs end-to-end against a freshly-built ref-server, produces 3 tool outputs, exits 0.
- `uv run python examples/hello_atd.py` parallel validation.
- `ATD_SOCK` override works: `ATD_SOCK=/tmp/foo.sock cargo run --example hello_atd` connects to the given path instead of spawning.
- Missing-binary error is human-readable.
- Unit test inside `examples/hello_atd.rs` (via `#[cfg(test)]`) that exercises the socket-wait helper against a disposable listener — verifies the poll loop terminates on success within budget.

### 6.2 What's NOT tested

- ctrl-C cleanup. Manual verification only (documented in the validation doc).
- Behavior when `target/release/atd-ref-server` exists but is corrupted / wrong architecture. Best-effort error message; users get whatever error the spawn emits.
- Cross-OS: Linux-only in this SP.

### 6.3 Expected test counts

- No new lib/integration tests for ATD crates. Workspace count stays at 243.
- One new unit test inside the Rust example (for the socket-wait helper). Doesn't register in workspace count because examples aren't in `--all-targets` test scope by default, but is runnable via `cargo test --example hello_atd`.

---

## 7. Plan task breakdown (preview)

1. **Task 1** — rewrite `examples/hello_atd.rs`. Auto-spawn + 3-tool demo + socket-wait helper with test.
2. **Task 2** — rewrite `python/examples/hello_atd.py`. Same shape, Python idioms.
3. **Task 3** — capture transcripts + write `docs/validation/2026-04-23-sp6-capstone.md`.
4. **Task 4** — update `README.md` + `CLAUDE.md` + tag `sp6-ref-server-capstone`.

No task exceeds ~150 LOC of new code.

---

## 8. Risks and non-risks

### 8.1 Risks

- **Binary path hardcoded.** The example assumes `target/release/atd-ref-server` exists at the project root's `target/` directory. If a user runs from a subdirectory with a separate target, it fails. Mitigation: `CARGO_MANIFEST_DIR` from the example's own manifest is available at compile time; use it to walk up to the workspace root.
- **Race on socket readiness.** atd-ref-server might take >3s to bind the socket on a slow CI machine. Mitigation: budget 3s feels safe (local runs bind in <200ms), bump to 5s if flakes occur.
- **tempfile cleanup on crash.** If the example panics between spawn and teardown, the child process outlives us. Mitigation: Rust's `Drop` on `Child` doesn't wait, but `Child::kill().await` is called on ctrl-C. A true SIGKILL to the example leaves an orphan — documented, accepted.

### 8.2 Non-risks

- **License audit.** Already done in SP-5; no new deps here.
- **Wire protocol drift.** SDK-server compat is already proven by 243 tests.
- **Tool behavior divergence.** The 3 chosen tools are deterministic by construction.

---

## 9. Exit criteria

1. `cargo build --release -p atd-ref-server && cargo run --example hello_atd` — runs end-to-end, prints all 3 tool sections, exits 0.
2. `cargo build --release -p atd-ref-server && uv run python examples/hello_atd.py` — parallel.
3. `ATD_SOCK=/tmp/x.sock cargo run --example hello_atd` against an external ref-server binds to the override.
4. `cargo test --workspace --all-targets` stays at 243.
5. `docs/validation/2026-04-23-sp6-capstone.md` committed with all 7 sections filled.
6. README and CLAUDE.md updated.
7. Tag `sp6-ref-server-capstone` created.
8. No mention of ANOS anywhere in the example code files (grep check).

---

## 10. Out of scope forever at this layer

- Long-running daemon demo (systemd unit, launchd plist)
- GUI / TUI demo
- Benchmarking suite
- Multi-server / failover demo

These aren't what a reference implementation demo is for.
