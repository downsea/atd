# atd-mvp

**Status:** SP-6 capstone complete. atd-ref-server ships with 9 tools across 4 domains, 243+ workspace tests, and `hello_atd` demos that auto-spawn the ref-server — zero ANOS dependency in the default path. Tags: `sp1-ref-server-foundation` through `sp6-ref-server-capstone`.

ATD (Agent Tool Dispatch) Client SDK reference implementation — the minimum viable protocol and client SDK that lets any agent framework call any tool on any platform through any binding.

**Start here:** [`docs/design.md`](docs/design.md) — the approved MVP design spec.

## Why a separate repo

atd-mvp is intentionally independent from the ANOS project at `/home/nan/proj/anos/`. ATD is positioned as a neutral protocol ("the agent-era POSIX") — it cannot credibly claim neutrality while living inside a single vendor's codebase.

`crates/atd-ref-server` is atd-mvp's own neutral ATD reference server — no ANOS dependency required. Once atd-mvp has upstream adopters, governance transfers to the APWG (Agent Protocol Working Group) per whitepaper §4.3.

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

```bash
# 1. clone + build
git clone https://github.com/atd-protocol/atd-mvp
cd atd-mvp
cargo build --release -p atd-ref-server
cargo build -p atd-examples --example hello_atd

# 2. run the example (auto-spawns the ref-server, no external daemon needed)
cargo run --example hello_atd -p atd-examples
```

Expected output:

```
[atd] spawning ref-server at /tmp/atd-ref-XXXX/server.sock
[atd] connecting to UnixSocket("/tmp/atd-ref-XXXX/server.sock")
[atd] connected
[atd] 9 tools discovered
        - ref:echo.say (Echo)
        - ref:fs.glob (Glob)
        - ref:shell.exec (Run Shell Command)
        ...
[atd] call ok: {...}
[atd] ref-server process cleaned up
```

**Your first call in 10 lines of Rust:**

```rust
use atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = AtdClient::connect(Endpoint::unix_default()).await?;
    let tools = client.discover(Some("fs"), DiscoverFilter::default()).await?;
    println!("{} fs tools", tools.len());
    let r = client.call(&tools[0].id, serde_json::json!({"path":"/tmp"}),
                        CallOptions::default()).await?;
    println!("{:?}", r);
    Ok(())
}
```

### Capstone demo — proving independence

`atd-mvp` ships its own reference server (`atd-ref-server`) and uses it
for the `hello_atd` demos. The example auto-spawns the ref-server as a
child process, exercises three real tools (`ref:echo.say`,
`ref:fs.glob`, `ref:shell.exec`), then cleans up. No ANOS daemon needed.

```bash
cargo build --release -p atd-ref-server
cargo run --example hello_atd -p atd-examples        # Rust
uv run --project python python python/examples/hello_atd.py   # Python
```

Want to demo against a different ATD server (ANOS or otherwise)? Set
`ATD_SOCK=/path/to/socket` — the demo skips the spawn and connects to
your chosen socket instead. Same client, same SDK, same output.

For full evidence of independence, see
[`docs/validation/2026-04-23-sp6-capstone.md`](docs/validation/2026-04-23-sp6-capstone.md).

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

## Python SDK

```python
import asyncio
from atd_client import AtdClient

async def main():
    async with await AtdClient.connect() as client:
        tools = await client.discover(query="fs", limit=5)
        print(f"{len(tools)} tool(s)")

asyncio.run(main())
```

Full reference: [`python/README.md`](python/README.md).

## Reference server

An optional **neutral ATD server** ships at `crates/atd-ref-server/`. Runs standalone on a Unix socket with a built-in tool catalog. Meant as a fork-friendly template for third-party server implementers. No dependency on any specific client or agent framework.

```bash
cargo build --release -p atd-ref-server --bin atd-ref-server
./target/release/atd-ref-server &
atd --sock $HOME/.atd-ref/server.sock list
```

Full reference: [`crates/atd-ref-server/README.md`](crates/atd-ref-server/README.md).

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
| **Reference server** | `crates/atd-ref-server/` (this repo) | atd-mvp's own neutral ATD server; default target for `hello_atd` demos |
| **Alternative server** | `/home/nan/proj/anos/` | ANOS daemon also speaks ATD wire protocol; set `ATD_SOCK=~/.anos/anos.sock` to use it |
| **Whitepapers** | Source of truth in ANOS `docs/research/`; copied here as `docs/whitepaper/` | Historical record + theoretical foundation |

## License

Apache-2.0 (recommended per design `§10.3`; pending confirmation at Day 1 commit).

## References

- Whitepaper v2 (primary): [`docs/whitepaper/v2-dual-track.md`](docs/whitepaper/v2-dual-track.md)
- Whitepaper v1 (formal): [`docs/whitepaper/v1-formal.md`](docs/whitepaper/v1-formal.md)
- ATD architecture reference: [`docs/reference/atd-overview.md`](docs/reference/atd-overview.md)
- ANOS reference module: [`docs/reference/anos-tool-dispatch-module.md`](docs/reference/anos-tool-dispatch-module.md)
- Implementation gaps tracked: [`docs/issues/`](docs/issues/)
