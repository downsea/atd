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

## 15-minute quickstart (Rust, Phase 0)

**Prerequisite:** the ANOS daemon is running and its socket is at `~/.anos/anos.sock`. Start it from `/home/nan/proj/anos/` with `cargo run -p anos-daemon` if it isn't already.

```bash
# 1. clone + build
git clone https://github.com/atd-protocol/atd-mvp
cd atd-mvp
cargo build -p atd-examples --bin hello_atd

# 2. run the example
ANOS_SOCK=$HOME/.anos/anos.sock \
  cargo run -p atd-examples --bin hello_atd
```

Expected output:

```
[atd] connecting to UnixSocket("/home/you/.anos/anos.sock")
[atd] connected
[atd] 3 tools discovered
        - anos:fs.read (Read File)
        - anos:fs.write (Write File)
        - anos:shell.exec (Run Shell Command)
[atd] describe(anos:fs.read) → domain=fs, bindings=1
[atd] call ok: {...}
```

**Your first call in 10 lines of Rust:**

```rust
use atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = AtdClient::connect(Endpoint::default_anos()).await?;
    let tools = client.discover(Some("fs"), DiscoverFilter::default()).await?;
    println!("{} fs tools", tools.len());
    let r = client.call(&tools[0].id, serde_json::json!({"path":"/tmp"}),
                        CallOptions::default()).await?;
    println!("{:?}", r);
    Ok(())
}
```

## CLI quickstart

```bash
# build the binary
cargo build --release -p atd-cli --bin atd

# peek at what's available
./target/release/atd list --limit 5

# inspect a specific tool
./target/release/atd schema anos:fs.read

# connectivity sanity check
./target/release/atd doctor
```

Full reference: [`docs/cli.md`](docs/cli.md).

## Development

```bash
cargo test --workspace              # unit + integration tests
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

The ANOS-free integration test lives in `crates/atd-client/tests/mock_server.rs` and runs automatically in CI — it proves the client talks to a server that has zero ANOS crate dependencies.

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
