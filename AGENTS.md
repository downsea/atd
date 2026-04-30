# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project state

**SP-6 capstone complete.** atd-ref-server ships with 9 tools across 4 domains (echo, fs, shell, web), 243+ workspace tests, and a `hello_atd.{rs,py}` demo that auto-spawns the ref-server — zero ANOS dependency in the default path. Tag: `sp6-ref-server-capstone`.

## Reading order

1. [`docs/design.md`](docs/design.md) — approved MVP design spec, **read this first**
2. [`docs/whitepaper/atd-v2-dual-track.md`](docs/whitepaper/atd-v2-dual-track.md) — primary whitepaper (decision-maker + developer)
3. [`docs/whitepaper/atd-v1-formal.md`](docs/whitepaper/atd-v1-formal.md) — formal/theoretical backing (CAP, H/W/C, POSIX analogy)
4. [`docs/reference/`](docs/reference/) — ATD architecture + ANOS dispatch module reference
5. [`docs/issues/`](docs/issues/) — 11 tracked gaps (dated 2026-04-21) in the ANOS reference server. These are **Phase 0/1 planning inputs**, not bugs to fix in this repo.

## Project identity

atd-mvp is the **independent reference implementation** of the ATD (Agent Tool Dispatch) protocol and client SDK. It is intentionally separate from the ANOS project at `/home/nan/proj/anos/` — ATD is positioned as a neutral cross-vendor protocol, and cannot live inside one vendor's repo.

## Relationship to ANOS

- **Reference server:** `crates/atd-ref-server` is atd-mvp's own neutral reference ATD server, shipped via SP-1 through SP-5 (tags `sp1-ref-server-foundation` through `sp5-ref-server-web`) and demo'd in SP-6. The `hello_atd` demos run against it by default. ANOS is still a valid server to speak to — set `ATD_SOCK=~/.anos/anos.sock` on any demo to demo against ANOS instead. Both backends speak the same wire protocol; that's the point.
- **Code reuse:** Pattern inspiration from ANOS crates is welcome, but **atd-mvp must have zero runtime dependency on any `anos-*` crate**. CI enforces this via an ANOS-free test harness.
- **Whitepapers:** Source-of-truth lives in `/home/nan/proj/anos/docs/research/`. Copies in `docs/whitepaper/` are snapshots — update from the source before major design work.

## Key reference files (absolute paths)

When the design doc or tasks reference ANOS implementation patterns, they mean:

- `/home/nan/proj/anos/crates/anos-cli/src/client.rs` — IPC client pattern
- `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs` — wire protocol (length-prefixed JSON)
- `/home/nan/proj/anos/crates/anos-types/src/tool.rs` — tool definition types (port cleanly, no `anos-*` deps)
- `/home/nan/proj/anos/crates/anos-llm-anthropic/src/provider.rs` — tool-name sanitization logic
- `/home/nan/proj/anos/crates/anos-tool-dispatch/src/` — dispatch core implementation

## Phase 0 scope (hard boundary)

Do not expand beyond these in the first 2-3 weeks:

- 3 APIs only: `discover` + `describe` + `call`
- 1 transport only: Unix socket
- 1 language only: Rust reference
- Phase 0 demo: capstone `hello_atd` exercising atd-ref-server — three tools, two language SDKs, zero ANOS dependency

Everything else (Python / TS SDK, stdio transport, MCP-compat, AppFunction binding, HTTP, events, skill runtime) is Phase 1+. See `docs/design.md` §7.

## Non-goals (explicit)

- ❌ Skill runtime — `atd-client` does not parse SKILL.md or execute skill bodies
- ❌ SOUL.md / identity / personality injection
- ❌ `subscribe` / event streaming (Phase 2+)
- ❌ HTTP transport (Phase 2+)
- ❌ AppFunction reference binding (Phase 2+, needs real hardware)
- ❌ Conformance test suite (Phase 2)

## Open questions (block Day 1 commit)

Per `docs/design.md` §10, confirm before first commit:

1. Create repo now or wait for first code? — current answer: create now, initialize with docs, push when `github.com/downsea/atd-mvp` org exists
2. Cargo workspace vs polyrepo? — current answer: Cargo workspace with Python/TS as sibling directories
3. License? — current answer: Apache-2.0
4. Versioning? — current answer: 0.1.0 semver, breaking changes allowed until 1.0
5. Governance / org ownership — current answer: individual during Phase 0, transfer to APWG at Phase 2

## Development workflow

Once implementation starts:

1. Every PR must pass `cargo test -p atd-<crate>` for the touched crate
2. `cargo check --workspace` must stay clean (per ANOS workflow convention)
3. The `ANOS-free` harness in `tests/integration/mock_server.rs` must pass — proves protocol independence
4. Align with design.md — if a change contradicts the design, update the design first, then the code

## Commit messages

Follow conventional commits (same as ANOS):

```
feat(atd-client): add stdio transport
fix(atd-types): correct ToolSummary serde
docs(design): clarify session semantics
```

## Not to be mixed with ANOS

Do not commit atd-mvp changes to the ANOS repo. Do not commit ANOS changes here. The two repos are deliberately separate for governance reasons.
