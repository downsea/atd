# atd-mvp

**Status:** Pre-implementation. Design approved 2026-04-21. No code yet.

ATD (Agent Tool Dispatch) Client SDK reference implementation — the minimum viable protocol and client SDK that lets any agent framework call any tool on any platform through any binding.

**Start here:** [`docs/design.md`](docs/design.md) — the approved MVP design spec.

## Why a separate repo

atd-mvp is intentionally independent from the ANOS project at `/home/nan/proj/anos/`. ATD is positioned as a neutral protocol ("the agent-era POSIX") — it cannot credibly claim neutrality while living inside a single vendor's codebase.

During Phase 0/1, the ANOS daemon serves as the **reference server implementation** (no changes required). atd-client in this repo talks to ANOS via Unix socket. Once atd-mvp has upstream adopters, governance transfers to the APWG (Agent Protocol Working Group) per whitepaper §4.3.

## Directory layout

```
atd-mvp/
├── README.md                               # this file
├── docs/
│   ├── design.md                           # MVP design spec (approved 2026-04-21)
│   ├── whitepaper/
│   │   ├── v1-formal.md                    # formal/theoretical whitepaper (CAP theorem, H/W/C proofs, POSIX analogy)
│   │   └── v2-dual-track.md                # dual-track whitepaper (decision-maker + developer)
│   ├── reference/
│   │   ├── atd-overview.md                 # ATD architecture reference (from ANOS docs)
│   │   └── anos-tool-dispatch-module.md    # ANOS implementation module doc
│   └── issues/                             # gaps in the ANOS reference server, tracked for Phase 0/1 planning
│       └── 2026-04-21-atd-*.md
└── (code directories TBD per design.md §4)
```

## Phase 0 Week 1 (concrete first steps)

From `docs/design.md` §11:

1. Initialize Rust workspace `Cargo.toml`, `LICENSE` (Apache-2.0), `.gitignore`
2. Create `atd-types` crate — port `ToolDefinition` / `ToolSummary` / `CapabilityDescriptor` from `/home/nan/proj/anos/crates/anos-types/src/tool.rs` **without** any `anos-*` dependency
3. Create `atd-client` crate — minimum `AtdClient::connect` + `call` over Unix socket (wire format from `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs`)
4. Write `examples/hello_atd.rs` — 10-line minimum working example
5. Run against local ANOS daemon (no ANOS code changes required)
6. Write this README's "15-min install story"
7. Initialize `github.com/atd-protocol/atd-mvp` (pending org creation)

## Relationship to ANOS

| Role | Path | Purpose |
|------|------|---------|
| **Protocol + Client SDK** | `/home/nan/proj/atd-mvp/` (this repo) | Independent, neutral protocol reference |
| **Reference server** | `/home/nan/proj/anos/` | ANOS daemon serves as the dispatch server for Phase 0/1 testing |
| **Whitepapers** | Source of truth in ANOS `docs/research/`; copied here as `docs/whitepaper/` | Historical record + theoretical foundation |

## License

Apache-2.0 (recommended per design `§10.3`; pending confirmation at Day 1 commit).

## References

- Whitepaper v2 (primary): [`docs/whitepaper/v2-dual-track.md`](docs/whitepaper/v2-dual-track.md)
- Whitepaper v1 (formal): [`docs/whitepaper/v1-formal.md`](docs/whitepaper/v1-formal.md)
- ATD architecture reference: [`docs/reference/atd-overview.md`](docs/reference/atd-overview.md)
- ANOS reference module: [`docs/reference/anos-tool-dispatch-module.md`](docs/reference/anos-tool-dispatch-module.md)
- Implementation gaps tracked: [`docs/issues/`](docs/issues/)
