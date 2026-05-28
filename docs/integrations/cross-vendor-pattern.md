# Pattern — Cross-Vendor Composition

**Companion to:** [`docs/integrations/healthkit.md`](healthkit.md) (single-vendor adopter case study).

**Validated by:** `scripts/cross-vendor-demo.sh` (boots `healthkit_cli` v1.3.1+ on `/tmp/hk.sock` and `atd-mock-weather-server` v1.0.0 on `/tmp/atd-weather.sock`; `atd list` against each shows two distinct vendor namespaces).

**Status as of 2026-04-27:** mock weather server lands in this SP (SP-cross-vendor-mock-demo). Real second-vendor adopter (option (b)/(c) in [atd#5](https://github.com/downsea/atd/issues/5)) deferred. LLM-driven Hermes / Claude transcript capture is a human follow-up — see §6.

---

## 1. What this proves

The healthkit case study validated that ATD matches CLI on **per-vendor agent ergonomics**. The remaining differentiator is **client-side composition across multiple vendor servers in one agent session**:

> One agent session sees both `huawei:hms.healthkit.*` and `mock:weather.*` tool ids in a single `discover()` call. The agent doesn't need to know which server hosts which id — they're equally callable.

This is fundamentally a property of the **protocol**, not of any individual server. Two ATD servers are siblings on the agent's discover surface. CLI couldn't compose this without an ad-hoc multiplexing wrapper per agent platform.

---

## 2. The setup

Two independent ATD servers, each on its own Unix socket:

| Server | Crate | Socket | Tool namespace | Tool count |
|---|---|---|---|---|
| Huawei HMS HealthKit | [`healthkit_cli`](https://github.com/downsea/healthkit_cli) v1.3.1+ | `/tmp/hk.sock` | `huawei:hms.healthkit.*` | 27 (25 helpers + 2 skills meta) |
| Mock weather (canned demo) | `atd-mock-weather-server` v1.0.0 (in this repo) | `/tmp/atd-weather.sock` | `mock:weather.*` | 3 |

Each server is a separate process, separate audit log, separate capability gate, separate trust attestation. Neither knows the other exists.

---

## 3. The recipe

```bash
# Build all three binaries
cd ~/code/atd && cargo build --release \
  -p atd-mock-weather-server -p atd-cli -p atd-mcp-bridge
cd ~/code/healthkit_cli && cargo build --release

# OAuth (only needed for real healthkit calls — skills.list / list work without)
~/code/healthkit_cli/target/release/healthkit auth login

# Boot both servers + print bridge registration commands
~/code/atd/scripts/cross-vendor-demo.sh up
```

Expected `up` output (truncated):

```
✓ mock-weather up (pid …, sock /tmp/atd-weather.sock)
✓ healthkit up (pid …, sock /tmp/hk.sock)

→ ═══ tools published by mock-weather ═══
  ID                                       NAME                     DOMAIN     TIER   VIS
  mock:weather.now                         mock:weather.now         weather    hot    read
  mock:weather.forecast.hourly             …                        weather    hot    read
  mock:weather.summary                     mock:weather.summary     weather    hot    read
  3 tool(s) total

→ ═══ tools published by healthkit (first 6 + total) ═══
  …
  27 tool(s) total

→ ═══ to wire BOTH into Hermes (one agent session, both vendors) ═══
  hermes mcp add weather --command …/atd-mcp-bridge --env ATD_SOCK=/tmp/atd-weather.sock
  hermes mcp add healthkit --command …/atd-mcp-bridge --env ATD_SOCK=/tmp/hk.sock ATD_REQUEST_CAPS=…
```

Tear down: `~/code/atd/scripts/cross-vendor-demo.sh down`.

---

## 4. What an agent sees

Once both bridges are registered with the agent platform, a single `discover()` (or its MCP equivalent `tools/list`) returns the union — `huawei:hms.healthkit.*` and `mock:weather.*` are siblings. The agent's tool picker has no special knowledge of which server hosts which id; it routes the call to the right MCP bridge based on the tool's binding metadata.

```
agent ──┬─→ atd-mcp-bridge "weather" ──→ /tmp/atd-weather.sock ──→ atd-mock-weather-server
        │                                                              │
        │                                                              └─ 3 mock:weather.* tools
        │
        └─→ atd-mcp-bridge "healthkit" ──→ /tmp/hk.sock ──────────→ healthkit serve
                                                                       │
                                                                       └─ 27 huawei:hms.healthkit.* tools
```

Each bridge speaks MCP/stdio to the agent and the ATD wire format to the server. The agent platform sees one combined catalog.

---

## 5. Sample composition prompt

> 我跑 5 km 应该穿什么？

A reasoning agent should pick tools across **both** vendor namespaces:

| Step | Tool | Why |
|---|---|---|
| 1 | `mock:weather.summary` | One-line outdoor conditions (canned: 17–22°C, partly cloudy, afternoon rain risk) |
| 2 | `huawei:hms.healthkit.heartrate` | Recent HR — am I rested? |
| 3 | `huawei:hms.healthkit.sleep` | Last night's sleep — energy reserves |
| 4 | (compose) | Recommend layers based on temp + body state |

The agent never needs to think about *where* each tool runs. It picks by capability + semantic relevance; the MCP bridges + ATD socket layer route automatically.

---

## 6. Hermes transcript: TODO

The mechanical setup is complete and reproducible (§3). Capturing the actual LLM-driven session — DeepSeek/Kimi/Claude reasoning across both vendors and producing a recommendation — requires an interactive Hermes (or Claude Code) session with a configured LLM provider. **This is a human-in-the-loop step, not something a code agent can run end-to-end.**

When you do capture it, append the transcript at `docs/integrations/cross-vendor-pattern/hermes-transcript.md` (or similar) and link from this section. Include:
- The exact prompt
- The agent's tool-call sequence (audit log from both `/tmp/hk-audit.jsonl` if enabled and the bridge log)
- The final composed answer
- Total elapsed time + tool call count

---

## 7. Why CLI can't do this

- **Separate processes:** each CLI binary has its own argv, env, working directory. There's no way for `healthkit-cli` to `discover()` `weather-cli`'s tools.
- **Separate stdios:** an agent platform that wraps a single CLI as an MCP server (e.g., via a generic `cli-as-mcp` adapter) can wrap *one*. Wrapping two requires the platform to support multiple parallel adapters — and even then, each adapter has its own per-process authentication, no shared catalog, no shared audit.
- **Separate auth flows:** each CLI's `--login` is a separate OAuth dance. ATD lets each server own its auth without leaking it to the agent layer.
- **Separate audit logs:** ATD audit logs live next to each server (`/tmp/hk-audit.jsonl`, optionally a weather audit). CLI invocations leave breadcrumbs only in the agent platform's transcript log, with no per-server attestation.

ATD makes cross-vendor composition a *config* concern (which sockets to bridge), not a *coding* concern (write a multiplexer per agent platform).

---

## 8. Limits + caveats

- **Mock weather is canned.** Every call returns the same data for Shanghai. Not suitable for production agent recommendations. Replace with a real weather adopter (e.g., wrapping OpenWeatherMap) before shipping a public demo. See [atd#5](https://github.com/downsea/atd/issues/5) options (b) and (c).
- **No multi-tenant isolation.** Both bridges connect to their respective sockets with the same `caller_id` (or none at all). Multi-tenant token routing across bridges is the sister differentiator at [atd#4](https://github.com/downsea/atd/issues/4); deferred.
- **Schema collision is technically possible.** Two servers could publish the same tool id (e.g., both name a tool `vendor:foo.bar`). The agent platform will see one or the other depending on bridge ordering. Naming convention (`<publisher>:<service>.<tool>`) makes collisions unlikely in practice.
- **No automatic startup ordering.** The script boots mock-weather first, then healthkit. If healthkit fails to bind, mock-weather still runs and the agent will see only weather tools. Robust deployment uses a process manager (systemd, supervisor) per server.

---

## 9. Future expansions

- **Real weather adopter** — wrap OpenWeatherMap or AccuWeather; mirror the healthkit pattern (helper-tools + SKILL.md + skills meta-tools convention). [atd#5](https://github.com/downsea/atd/issues/5) option (b).
- **Calendar adopter** — Google Calendar or CalDAV; clearly complementary to healthkit + weather for a full day-planning use case. [atd#5](https://github.com/downsea/atd/issues/5) option (c).
- **Multi-tenant routing across both vendors** — agent A and agent B see the same union catalog but their calls route to different OAuth tokens per vendor. [atd#4](https://github.com/downsea/atd/issues/4).
- **Compositional skill** — a SKILL.md that explicitly references tools from both vendors (e.g., a "morning-briefing" skill that pulls health + weather + calendar). Once `atd skills sync` lands skills from both servers, a skill body can call across them naturally.

---

## 10. See also

- [`healthkit.md`](healthkit.md) — single-vendor adopter case study (the failure → fix arc this composition pattern builds on)
- [`overview.md`](overview.md) — the five integration paths
- [`../atd-architecture.md`](../atd-architecture.md) §1.1 — the "Any agent / Any framework" claim this demo cashes
- `crates/atd-mock-weather-server/` — the bin crate
- `scripts/cross-vendor-demo.sh` — the up/down/status helper
