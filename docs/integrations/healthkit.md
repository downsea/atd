# Adopter Case Study — Huawei HMS HealthKit

**Adopter:** [`healthkit_cli`](https://github.com/downsea/healthkit_cli) — agent-native CLI for the Huawei HMS HealthKit v2 REST API (4 resources, 8 endpoints, 25 helper commands).

**ATD versions exercised:** `atd-protocol` / `atd-sdk` / `atd-runtime` / `atd-server` v0.2.1 (path-deps from this repo).

**Validated against:** `sp-listener-extract` tag (2026-04-25) for the listener split, `healthkit_cli` v1.2.0 (2026-04-27) for the helper-tool surface, v1.2.1 (2026-04-27) for the HRV fix.

This document tells the full arc — failure → fix → win — of integrating a real REST API into ATD as a self-hosted server. It complements the per-framework integration guides under [`docs/integrations/`](.) by showing what an adopter who *publishes* tools (rather than consumes them) had to build, what didn't work, and what fixed it. If you're evaluating "should I expose my service via ATD?", read this before [the architecture doc](../architecture.md).

---

## 1. The setup

`healthkit_cli` is a Rust CLI that wraps the Huawei HMS HealthKit REST API. The schema (`schemas/healthkit-v1.json`) describes 4 resources (`sampleSet`, `dataCollector`, `activityRecord`, `healthRecord`) and 8 endpoints. On top of those endpoints the CLI ships **25 `+`-prefixed helper commands** — `+heartrate`, `+sleep`, `+steps`, `+stress`, `+spo2`, etc. — that hide HMS body construction and time-window math from the user. A SKILL.md file accompanies each helper (`skills/healthkit-<domain>/SKILL.md`) describing usage and domain context.

When the maintainer wanted to make this surface reachable from agents (Hermes, Claude Code, anything MCP-speaking), the obvious move was to host it as an ATD server. ATD's value over rolling a per-vendor MCP server bespoke:

- One process feeds many agent platforms simultaneously (Hermes + Claude Code share `/tmp/hk.sock` in this case study)
- Audit log unification across all callers
- Capability gate (`healthkit:read` / `healthkit:write`) enforced at the server, not per-client
- No bespoke wire protocol — the [atd-mvp listener](../architecture.md#84-current-crate-map) handles connect / discover / describe / call / dry-run for free

The integration path:

```
Hermes ─┐
         ├── atd-mcp-bridge (stdio MCP ↔ ATD wire) ── /tmp/hk.sock ── healthkit serve (atd-server + atd-runtime)
Claude ─┘                                                                   ↓
                                                                       HMS REST v2
```

`healthkit serve` was the new subcommand: it loaded the schema, registered tools into an `atd-runtime::Registry`, and ran an `atd-server::Server` against the socket.

---

## 2. The failure (v1.1.0)

The first cut was the literal one: walk `schemas/healthkit-v1.json` and emit one ATD `ToolDefinition` per HMS endpoint. Tool ids were `huawei:hms.healthkit.<resource>.<verb>`:

```
huawei:hms.healthkit.sampleSet.polymerize
huawei:hms.healthkit.sampleSet.create
huawei:hms.healthkit.activityRecord.read
huawei:hms.healthkit.activityRecord.create
huawei:hms.healthkit.healthRecord.read
huawei:hms.healthkit.healthRecord.create
huawei:hms.healthkit.dataCollector.list
huawei:hms.healthkit.dataCollector.update
```

8 raw tools, capabilities derived from HTTP verb (GET → `healthkit:read`, mutating → `healthkit:write`), `input_schema` permissive (`{"type":"object"}` — the HMS schema's request bodies are deeply nested and OneOfs, so the v1 translator didn't tighten beyond that). Each tool's `Tool::call` delegated straight to the existing CLI executor.

Same Hermes Agent + DeepSeek + 4 user queries the maintainer had already run against the human CLI:

| Q | Prompt (paraphrased) |
|---|---|
| Q1 | "Am I in shape to run 5 km tomorrow morning?" |
| Q2 | "Compare this week's heart rate / steps / RHR / kcal to last week." |
| Q3 | "Help me plan a step challenge for the rest of the week." |
| Q4 | "Give me today's full daily report — sleep, HR, SpO2, distance, calories, stress, active minutes, VO2max." |

**Result over the four prompts (audit log pulled from `/tmp/hk-audit.jsonl`):**

| Metric | Value |
|---|---|
| Total tool calls | 79 |
| Successful | 19 |
| `invalid_args` (HMS rejected the body shape) | 52 (66%) |
| Other failures | 8 |
| **Success rate** | **24%** |

The same model running against the plain CLI helpers — same prompts, same data — succeeded at roughly 95%. ATD made things *worse*.

**Why:** the LLM had the 8 raw endpoints and a permissive `{type:object}` schema. To call `huawei:hms.healthkit.sampleSet.polymerize` it had to hand-construct an HMS request body shaped like:

```json
{
  "polymerizeWith": [{
    "dataTypeName": "com.huawei.continuous.steps.delta",
    "groupByTime": {"groupPeriod": "groupByDay", "duration": 86400000, "timeZone": "+0800"}
  }],
  "startTime": 1714089600000,
  "endTime": 1714694400000
}
```

…with the right `dataTypeName` for the metric the user asked about (heart rate? `com.huawei.continuous.heart_rate`. Sleep? `com.huawei.continuous.sleep`. Calories? *which* of the four candidate calorie types?), the right grouping shape, the right timezone string format, the right millisecond-vs-nanosecond timestamp epoch (HMS uses both depending on endpoint), and the right outer envelope. DeepSeek guessed wrong nearly two-thirds of the time, retried, guessed wrong again, gave up, sometimes hallucinated a "data not available" answer and moved on. Q4 (the daily report) hit 17 calls and finished partial; Q1 finished without ever pulling sleep data because the body shape kept failing.

The signal was sharp: **a permissive `input_schema` over a non-trivial REST API forces the agent into trial-and-error.** The CLI helpers — which hard-code dataTypeName, body shape, and time semantics per metric — encode domain knowledge the agent no longer has when you strip them away and expose only the underlying schema. v1.1.0 stripped that knowledge.

---

## 3. The fix (v1.2.0)

The diagnosis pointed at a clear shape for the next surface: re-expose the *helper* layer, not the schema layer. SP-healthkit-helper-tools (spec at [`healthkit_cli/docs/superpowers/specs/2026-04-25-sp-healthkit-helper-tools-design.md`](https://github.com/downsea/healthkit_cli/blob/main/docs/superpowers/specs/2026-04-25-sp-healthkit-helper-tools-design.md)) replaced the 8 raw tools with **26 helper tools**, ids `huawei:hms.healthkit.<domain>`:

```
huawei:hms.healthkit.heartrate
huawei:hms.healthkit.steps
huawei:hms.healthkit.sleep
huawei:hms.healthkit.calories
huawei:hms.healthkit.stress
huawei:hms.healthkit.spo2
huawei:hms.healthkit.bloodpressure
…
huawei:hms.healthkit.daily              ← composite (steps + calories + distance)
huawei:hms.healthkit.healthkit-overview ← meta-tool listing all helpers
```

Each tool was auto-derived from two existing artifacts:

1. **`helpers/healthkit.rs`** — the body-builder functions the human CLI already uses (`build_polymerize_body`, `build_health_record_body`, `build_activity_body`). These hard-code dataTypeName + body shape per metric.
2. **`skills/<helper>/SKILL.md`** — the human-facing documentation. Parsed at compile time via `include_str!` so the binary ships with descriptions baked in regardless of cwd.

The result: the agent calls `huawei:hms.healthkit.heartrate {days: 7}` and the server constructs the polymerize body. The LLM no longer has to know what `com.huawei.continuous.heart_rate` is.

Three additional ergonomic moves landed in the same SP:

- **`intent_examples`** — three natural-language phrases per helper, synthesized from the SKILL.md examples block, surfaced in the [`Capability`](../protocol/wire-format.md) struct so LLMs can match user requests to the right tool.
- **`description`** with auto-extracted JSON args examples — the tool description embeds `Args examples: {"days": 7} | {"start": "2026-01-01", "end": "2026-01-31"}` so the LLM sees concrete shapes alongside the prose.
- **`--expose-raw-tools`** opt-in flag — the 8 raw schema tools are still reachable for debugging or queries the helpers don't cover, but off by default. Their descriptions prepend `[ADVANCED]` guidance toward helpers.

**Same Hermes + DeepSeek + 4 prompts:**

| Metric | v1.1.0 | v1.2.0 | Δ |
|---|---|---|---|
| Tool calls | 79 | 21 | **−73%** |
| Success rate | 24% | **95.2%** (20/21) | **+71pp** |
| `invalid_args` | 66% | 4.8% (1/21) | −61pp |

The single v1.2.0 failure was an HMS-side namespace quirk on HRV (`com.huawei.heart_rate_variability` rejected by `healthRecord.read`) — fixed in v1.2.1 by reclassifying the helper to dispatch via `sampleSet.polymerize` (where the CLI's own `+hrv` already routed it). Closes [healthkit_cli#1](https://github.com/downsea/healthkit_cli/issues/1).

The transcript-level qualitative shift was as striking as the numbers. v1.1.0 Q1 fumbled over body shapes and never pulled sleep data; v1.2.0 Q1 read HR / RHR / sleep / stress in five clean calls and produced a green-light recommendation citing concrete cautions ("sleep 6h40m, current HR 98 bpm"). v1.2.0 Q4 (the daily report) pulled 10 metrics in **parallel** — HR / RHR / steps / calories / sleep / SpO2 / distance / stress / active-min / VO2max — and produced a structured daily report with 7-day baselines. Audit log: [`audit.jsonl`](https://github.com/downsea/healthkit_cli/blob/main/docs/case-study-v1.2.0/audit.jsonl).

---

## 4. What ATD added that the CLI couldn't

Once the v1.2.0 surface matched the CLI on agent ergonomics, the operability features became the actual reason to deploy ATD:

- **One server, many agents.** The same `healthkit serve` process binds `/tmp/hk.sock`. Hermes connects via `atd-mcp-bridge`. Claude Code connects via `atd-mcp-bridge` registered through `claude mcp add`. The `atd` developer CLI inspects the same socket. All three see the same 26 tools, all three calls land in the same audit log. The CLI can't share state across agents; only ATD can.
- **Audit log unification.** Every call lands in [`/tmp/hk-audit.jsonl`](https://github.com/downsea/healthkit_cli/blob/main/docs/case-study-v1.2.0/audit.jsonl) with `caller_id`, `tool_id`, duration, outcome — regardless of which agent sent it. The audit format matches the [conformance suite's wire spec](../protocol/wire-format.md#audit-events).
- **Capability gate enforced server-side.** `healthkit serve --grant-capability healthkit:read --grant-capability healthkit:write` declares the allow-list. Clients negotiate down at the `Hello` handshake; `atd-mcp-bridge` carries this via `ATD_REQUEST_CAPS`. A misconfigured client cannot escalate.
- **Token reuse + auto-refresh in one place.** The HMS OAuth2 token lives at `~/.config/healthkit/token.json`. The server reads it on each call (env override → saved file → refresh on expiry). Multi-tenancy is deferred (see [atd-mvp#4](https://github.com/downsea/atd-mvp/issues/4)) but the single-tenant case ships today.

These features are exactly what's hardest to bolt onto a per-agent integration after the fact, and exactly what the listener-extract SP made cheap to bring up: `healthkit serve` is ~150 lines of glue around `atd-runtime::Registry` + `atd-server::Server::run`.

---

## 5. Architectural lessons for future adopters

Distilled from the v1.1.0 → v1.2.0 step, in priority order:

1. **Don't expose the schema layer. Expose the helper layer.** If your service has helper functions that hide construction quirks (body shapes, ID lookups, time-window math), turn each helper into one ATD tool. If it doesn't, write the helpers first — then expose them. Permissive `{type:object}` input schemas over rich REST APIs lose to retry loops.
2. **Pack `intent_examples`.** Three natural-language phrases per tool. Cheap to author, large effect on the LLM matching user intent to your tool id.
3. **Embed your skill docs at compile time.** `include_str!` of the SKILL.md content into the binary means the server works regardless of cwd or install location. The healthkit_cli implementation lives at [`src/atd_server/helper_tools.rs`](https://github.com/downsea/healthkit_cli/blob/main/src/atd_server/helper_tools.rs) — search for `embedded_skill_md`.
4. **Keep raw schema tools available, off by default.** Power users and integration tests still need `huawei:hms.healthkit.sampleSet.polymerize`. The `--expose-raw-tools` opt-in pattern (helpers visible to agents, raw visible to humans) is general-purpose.
5. **Audit log is the test fixture you didn't think you'd need.** The case-study numbers in this doc came straight out of `audit.jsonl`. Every adopter doing this kind of comparative work should turn `--audit-log` on.

---

## 6. Reproduction recipe

The full setup is one script in the adopter repo. From a fresh clone:

```bash
# 1. Build the bits
cd ~/proj/atd-mvp && cargo build --release -p atd-mcp-bridge
cd ~/proj/healthkit_cli && cargo build --release

# 2. One-time HMS OAuth (interactive)
./target/release/healthkit auth login

# 3. Start server + register Claude Code MCP entry
./scripts/atd-claude-setup.sh up

# 4. Talk to it
claude
# > How did I sleep last week?
```

`atd-hermes-setup.sh` is the parallel script for Hermes. The two scripts share the running `healthkit serve` process — running `up` on the second when the first is already up just adds the second client registration on top.

**Verify with the dev CLI:**

```bash
~/proj/atd-mvp/target/release/atd --sock /tmp/hk.sock list
# 26 tools, all huawei:hms.healthkit.*

~/proj/atd-mvp/target/release/atd --sock /tmp/hk.sock describe huawei:hms.healthkit.heartrate

~/proj/atd-mvp/target/release/atd --sock /tmp/hk.sock call \
  huawei:hms.healthkit.heartrate '{"days": 7}' --dry-run
```

**Watch live:**

```bash
tail -f /tmp/hk-audit.jsonl | jq
```

---

## 7. Appendix — pointers

**healthkit_cli repo (the adopter):**

- Setup scripts: [`scripts/atd-claude-setup.sh`](https://github.com/downsea/healthkit_cli/blob/main/scripts/atd-claude-setup.sh), [`scripts/atd-hermes-setup.sh`](https://github.com/downsea/healthkit_cli/blob/main/scripts/atd-hermes-setup.sh)
- Server entry: [`src/atd_server/server.rs`](https://github.com/downsea/healthkit_cli/blob/main/src/atd_server/server.rs)
- Helper-tool dispatch: [`src/atd_server/helper_tools.rs`](https://github.com/downsea/healthkit_cli/blob/main/src/atd_server/helper_tools.rs), [`src/atd_server/helper_class.rs`](https://github.com/downsea/healthkit_cli/blob/main/src/atd_server/helper_class.rs)
- Schema-to-tool fallback: [`src/atd_server/schema_to_tools.rs`](https://github.com/downsea/healthkit_cli/blob/main/src/atd_server/schema_to_tools.rs)
- Specs: [`docs/superpowers/specs/2026-04-25-sp-healthkit-atd-server-design.md`](https://github.com/downsea/healthkit_cli/blob/main/docs/superpowers/specs/2026-04-25-sp-healthkit-atd-server-design.md), [`...-helper-tools-design.md`](https://github.com/downsea/healthkit_cli/blob/main/docs/superpowers/specs/2026-04-25-sp-healthkit-helper-tools-design.md)
- Case study artifacts: [`docs/case-study-v1.2.0.md`](https://github.com/downsea/healthkit_cli/blob/main/docs/case-study-v1.2.0.md), [`docs/case-study-v1.2.0/`](https://github.com/downsea/healthkit_cli/tree/main/docs/case-study-v1.2.0) (q1–q4 transcripts + audit.jsonl)
- Changelog: [`CHANGELOG.md`](https://github.com/downsea/healthkit_cli/blob/main/CHANGELOG.md)

**This repo (atd-mvp):**

- Architecture doc: [`docs/architecture.md`](../architecture.md) — see §10 for the SP-listener-extract row this case triggered
- Integration overview: [`docs/integrations/overview.md`](overview.md)
- Wire spec: [`docs/protocol/wire-format.md`](../protocol/wire-format.md), [`docs/protocol/error-codes.md`](../protocol/error-codes.md)

**Open follow-ups (issues):**

- [atd-mvp#2](https://github.com/downsea/atd-mvp/issues/2) — skills layer convention + `atd-skills-sync`
- [atd-mvp#3](https://github.com/downsea/atd-mvp/issues/3) — `ToolVisibility::Hidden` (replaces `--expose-raw-tools` flag)
- [atd-mvp#4](https://github.com/downsea/atd-mvp/issues/4) — multi-tenant token broker
- [healthkit_cli#2](https://github.com/downsea/healthkit_cli/issues/2) — expose 26 SKILL.md via `skills.list/get`
