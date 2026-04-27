# SP-cross-vendor-mock-demo — `atd-mock-weather-server` + cross-vendor composition demo

**Date:** 2026-04-27
**Status:** Approved — ready for implementation plan
**Parent:** Closes [atd-mvp#5](https://github.com/downsea/atd-mvp/issues/5) (option (a) — mock weather server, not real adopter). Companion to the healthkit_cli case study at [`docs/integrations/healthkit.md`](../../integrations/healthkit.md), demonstrating the second axis of "ATD beats CLI": **client-side composition across multiple vendor servers**.

**Anchor:** Architecture §1.1 ("Any agent / Any framework") and the "five integration paths" framing in [`docs/integrations/overview.md`](../../integrations/overview.md). This SP cashes the architectural claim with a runnable demo.

## 1. Context

The v1.2.0 healthkit case study proved ATD ≥ CLI on **per-vendor agent ergonomics**. Two operational claims remain unproven by code:
- **Multi-tenant token routing** (one server, N callers, N tokens) — [#4](https://github.com/downsea/atd-mvp/issues/4), Tier 3, deferred
- **Client-side cross-vendor composition** (one agent session, N vendor servers, one prompt that spans them) — this SP

A CLI-only path forces N separate binaries + N stdio bridges + N audit logs. ATD makes each vendor server a sibling on the agent's discover surface: tool ids `<publisher_a>:<service>.<x>` and `<publisher_b>:<service>.<y>` are equally callable. The agent doesn't need to know which server hosts which id.

This SP ships the smallest meaningful demo: a mock weather server alongside the existing healthkit server, with a script + doc that prove both surfaces are reachable from one client and a hand-off point for a human to capture the Hermes/LLM transcript.

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q1 | Separate binary, or extend atd-ref-server? | **Separate binary `atd-mock-weather-server`.** Whole point of the demo is "two distinct servers, two sockets." A multi-namespace single server doesn't prove cross-vendor composition. |
| Q2 | Crate layout — split lib + bin, or flat? | **Flat single bin crate** (`crates/atd-mock-weather-server/`). Mock weather is a stand-in; no reusable lib surface needed. Two crates for a half-day demo is overkill. |
| Q3 | Tool count + ids? | **Three, `mock` publisher.** `mock:weather.now`, `mock:weather.forecast.hourly`, `mock:weather.summary`. All visibility=Read, no required_capabilities. |
| Q4 | Mock data shape? | **Static canned data for one fixed location (Shanghai).** Same response every call (no time-of-day variation). Documented up-front as "canned, not a real service" in tool descriptions. |
| Q5 | Doc location? | **New `docs/integrations/cross-vendor-pattern.md`.** Different concern from the per-adopter case study; cross-links from `docs/integrations/overview.md` and the healthkit case study. |
| Q6 | Demo script shape? | **`scripts/cross-vendor-demo.sh`** boots both servers, runs `atd list` against each socket, prints expected output, then prints the MCP-bridge registration commands the user would run for Hermes / Claude Code. Hermes transcript is a follow-up the human captures. |
| Q7 | Hermes transcript in this SP? | **No.** Transcript needs an interactive LLM session; can't run from a code agent. Marked as a follow-up TODO in the doc. |
| Q8 | New crate version? | Workspace minor `0.3.0` already; `atd-mock-weather-server` ships at `0.3.0` from day one. No workspace bump. |
| Q9 | Conformance fixtures? | **No.** Mock weather is itself behaviorally trivial; the conformance suite already validates the wire surface against atd-ref-server. Adding fixtures here would just duplicate `tool_list_returns_known_reference_tools`. |

## 3. Touch points

One commit. Workspace addition + docs.

| # | File | Change |
|---|---|---|
| 1 | `Cargo.toml` (workspace root) | Add `crates/atd-mock-weather-server` to `members`. |
| 2 | `crates/atd-mock-weather-server/Cargo.toml` | New bin crate manifest. Path-deps on atd-protocol, atd-runtime, atd-server. |
| 3 | `crates/atd-mock-weather-server/src/main.rs` | Bin entry. Parses `--sock` flag. Builds `Registry` with 3 mock tools. Spins `atd_server::Server::new(...).run()`. ~80 lines. |
| 4 | `crates/atd-mock-weather-server/src/tools.rs` | Three `Tool` impls: `WeatherNowTool`, `WeatherForecastHourlyTool`, `WeatherSummaryTool`. Static canned responses. ~120 lines + 3 unit tests. |
| 5 | `scripts/cross-vendor-demo.sh` | Boot both servers, run discover against each, print MCP-bridge registration commands. ~80 lines. |
| 6 | `docs/integrations/cross-vendor-pattern.md` | Composition pattern doc. ~150 lines. |
| 7 | `docs/integrations/overview.md` | One-line cross-link to the new doc, mirroring the healthkit case-study link added in SP-healthkit-case-study. |
| 8 | `docs/architecture.md` | §10 status row. |

**Not touched:**

- `atd-protocol`, `atd-runtime`, `atd-sdk`, `atd-server`, `atd-cli`, `atd-conformance`, `atd-mcp-bridge` — pure additive: a new bin uses the existing public APIs.
- `atd-tools-*` — the mock weather tools are deliberately scoped to this bin (per Q2 flat layout).
- `healthkit_cli` — unchanged; the demo just runs both servers in parallel.

## 4. The three mock tools

All three accept `args: {}` (or trivial fields) and return canned data. Static responses prove the composition pattern without coupling the demo to API drift.

### 4.1 `mock:weather.now`

**Args:** `{}` (no fields)

**Returns:**

```json
{
  "location": "Shanghai",
  "temperature_c": 18,
  "condition": "partly_cloudy",
  "humidity_pct": 62,
  "wind_kph": 12,
  "observed_at": "2026-04-27T08:00:00+08:00",
  "note": "canned demo data — not a real weather service"
}
```

### 4.2 `mock:weather.forecast.hourly`

**Args:** `{"hours": Option<u32>}` — defaults to 6, max 24.

**Returns:** array of `{hour: i32, temperature_c: i32, condition: String, precipitation_pct: u32}`. Six fixed entries by default.

### 4.3 `mock:weather.summary`

**Args:** `{}` (no fields)

**Returns:** `{"summary": "Shanghai today: 17–22°C, partly cloudy, 60% humidity. Light wind. Best window for outdoor activity 9-11am."}`

The `summary` tool exists so an agent can compose with `huawei:hms.healthkit.heartrate` + `mock:weather.summary` in two calls instead of three. Demonstrates the cross-vendor compose path with minimal calls.

## 5. The bin (`main.rs`)

```rust
//! `atd-mock-weather-server` — small bin that registers 3 mock weather
//! tools onto an atd-server::Server. Used by the cross-vendor demo to
//! prove client-side composition: an agent connects to BOTH this server
//! AND healthkit_cli's server in one session, and the agent's tool
//! catalog spans both vendors' namespaces.
//!
//! Static canned data; not a real weather service. See
//! `docs/integrations/cross-vendor-pattern.md` and
//! SP-cross-vendor-mock-demo.

use std::path::PathBuf;
use std::sync::Arc;

use atd_runtime::registry::Registry;
use atd_server::{Server, ServerConfig};
use clap::Parser;

mod tools;

#[derive(Parser, Debug)]
#[command(name = "atd-mock-weather-server", about = "Mock weather ATD server (cross-vendor demo).")]
struct Args {
    /// Unix socket path to bind. Default: /tmp/atd-weather.sock
    #[arg(long, default_value = "/tmp/atd-weather.sock")]
    sock: PathBuf,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();

    // Best-effort cleanup so a stale socket from a crashed prior run
    // doesn't make `bind` fail.
    let _ = std::fs::remove_file(&args.sock);

    let mut reg = Registry::new();
    reg.register(Arc::new(tools::WeatherNowTool::new()));
    reg.register(Arc::new(tools::WeatherForecastHourlyTool::new()));
    reg.register(Arc::new(tools::WeatherSummaryTool::new()));

    let cfg = ServerConfig {
        socket_path: args.sock.clone(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        max_output_bytes: 65_536,
        default_call_timeout_ms: 1000,
        granted_capabilities: vec![],
        audit_sink: None,
        server_version: concat!("atd-mock-weather-server ", env!("CARGO_PKG_VERSION")).into(),
    };

    eprintln!(
        "atd-mock-weather-server: 3 tool(s) registered (mock:weather.*); listening on {}",
        args.sock.display()
    );

    match Server::new(reg, cfg).run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("server error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
```

## 6. The tools (`tools.rs`)

Three `Tool` impls following the atd-tools-echo pattern. Each `Tool::call` returns a `Box::pin(async { Ok(canned_value) })`. Tool definitions follow the existing convention (publisher `mock`, domain `weather`, visibility `Read`, no required_capabilities, tier `Hot`, trust `L0Unverified`).

Unit tests:
- `weather_now_definition_has_expected_id_and_no_caps`
- `weather_forecast_hourly_clamps_hours_to_24`
- `weather_summary_returns_string_under_200_chars`

## 7. Demo script (`scripts/cross-vendor-demo.sh`)

```bash
#!/usr/bin/env bash
# cross-vendor-demo.sh — boot healthkit + mock-weather servers, prove
# they're discoverable side-by-side from one client.
#
# Usage:
#   ./scripts/cross-vendor-demo.sh           # full demo
#   ./scripts/cross-vendor-demo.sh down      # tear down both servers
#
# Prereqs:
#   - atd-mvp built: cargo build --release -p atd-mock-weather-server -p atd-cli
#   - healthkit_cli built + auth'd: cd ~/proj/healthkit_cli && cargo build --release && \
#       ./target/release/healthkit auth login

set -euo pipefail

ATD_REPO="${ATD_REPO:-$HOME/proj/atd-mvp}"
HK_REPO="${HK_REPO:-$HOME/proj/healthkit_cli}"
HK_SOCK=/tmp/hk.sock
WX_SOCK=/tmp/atd-weather.sock
HK_PIDFILE=/tmp/hk-cross-demo.pid
WX_PIDFILE=/tmp/wx-cross-demo.pid

# (down logic, up logic — boots both servers, runs:
#   atd --sock $HK_SOCK list   # 27 huawei:hms.healthkit.* tools
#   atd --sock $WX_SOCK list   # 3  mock:weather.* tools
# Then prints `claude mcp add` / `hermes mcp add` lines for each socket
# so the user can wire the agent platform manually.)
```

The detail of up/down/status modes mirrors the existing `~/proj/healthkit_cli/scripts/atd-claude-setup.sh`. Keep PIDs in two separate files; tear down independently.

## 8. The doc (`docs/integrations/cross-vendor-pattern.md`)

Outline (~150 lines):

1. **What this proves** — client-side composition across two vendors. ATD = protocol, two ATD servers = two siblings on the discover surface.
2. **The setup** — healthkit_cli (Huawei health) + atd-mock-weather-server (canned weather) running on two separate Unix sockets.
3. **The recipe** — `scripts/cross-vendor-demo.sh up`; `atd list` against each; bridge registration commands.
4. **What an agent sees** — both `huawei:hms.healthkit.*` and `mock:weather.*` tool ids in one `discover()` call (when configured with both bridges).
5. **Why CLI can't compose** — separate binaries, separate stdios, separate auth flows, no shared catalog.
6. **Sample composition prompt** — "我跑 5km 应该穿什么？" — agent reasons about heart rate + sleep (healthkit) + temperature + condition (weather), composes recommendation.
7. **Hermes transcript: TODO** — explicit follow-up. Reproduction recipe documented; LLM-driven capture is a human step.
8. **Limits + caveats** — mock data is canned; real second-vendor adopter (option b/c in #5) is the next step.

## 9. Versioning

| Crate | Before | After | Reason |
|---|---|---|---|
| `atd-mock-weather-server` | (new) | `0.3.0` | New crate, ships at workspace version |
| Workspace | 0.3.0 | 0.3.0 | No bump (additive new bin) |

## 10. Validation

Exit gates:

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features` — passes (current 365 + 3 new unit tests = 368)
- [ ] `cargo build --release --workspace`
- [ ] `cargo run --release --bin atd-mock-weather-server -- --sock /tmp/test-wx.sock` — boots, binds socket, advertises 3 tools via `atd list`
- [ ] `bash scripts/cross-vendor-demo.sh` — completes without error against healthkit serve + mock weather, prints registration commands

## 11. Out of scope (deferred)

- **Real second-vendor adopter** (option (b) or (c) in [#5](https://github.com/downsea/atd-mvp/issues/5)) — weather wrapping a real API, or calendar integration. The mock proves the pattern; real adopters land later.
- **Live Hermes / Claude Code transcript capture** — needs LLM provider, interactive prompt, human-in-the-loop. Documented as TODO.
- **Multi-tenant token routing** — sister differentiator at [#4](https://github.com/downsea/atd-mvp/issues/4); separate SP.
- **Conformance fixtures for mock weather** — wire-level surface is identical to atd-ref-server's, already covered by the existing self-conformance suite.
- **`atd skills sync` for mock weather** — mock weather has no SKILL.md files; not an adopter expected to publish skills.

## 12. Architecture.md §10 row

Add after the SP-skills-discovery-convention row:

```
| Cross-vendor composition demo (`atd-mock-weather-server`) | Cross-cutting | ✅ | SP-cross-vendor-mock-demo | 2026-04-27 | Landed; new bin crate `atd-mock-weather-server` with 3 canned mock:weather.* tools; `scripts/cross-vendor-demo.sh` boots healthkit + mock-weather side-by-side; `docs/integrations/cross-vendor-pattern.md` documents the pattern. Closes #5 option (a). Real second-vendor adopter (#5 (b)/(c)) deferred; live LLM transcript capture is a human follow-up. |
```
