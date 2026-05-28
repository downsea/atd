# SP-skills-discovery-convention — Skills meta-tool convention + `atd skills sync` + healthkit_cli adoption

**Date:** 2026-04-27
**Status:** Approved — ready for implementation plan
**Parent:** Closes [atd-mvp#2](https://github.com/downsea/atd-mvp/issues/2) and [healthkit_cli#2](https://github.com/downsea/healthkit_cli/issues/2). Continues the v1.2.0 healthkit case study (see [`docs/integrations/healthkit.md`](../../integrations/healthkit.md)) by closing the loop from "ATD ships rich tool ergonomics" to "agent platforms get the SKILL.md content automatically."
**Anchor:** Architecture §7 (Skills Layer adjacent) — softens §7.3 to permit a thin opt-in sync helper without taking on full skill installation responsibility.

## 1. Context

ATD's v1.2.0 healthkit case study closed the loop on **tool ergonomics** (24% → 95.2% LLM success). The remaining gap is **skill content**: each healthkit helper has a SKILL.md file with usage examples, when-to-use guidance, and progressive-disclosure detail — but those files only land on agent platforms (Hermes, Claude Code, Cursor) when a human manually copies them into the platform's skills directory. ATD-hosted vendor servers ship the content but agent platforms can't pull it.

This SP standardizes the wire-level convention for *publishing* skills and ships a thin sync utility that *installs* them. The protocol contract is unchanged (no new wire messages); the convention is purely a tool-id naming rule plus expected-shape contracts on `args` / response.

## 2. Decisions locked in during brainstorming

| # | Question | Answer |
|---|---|---|
| Q1 | Architectural tension: §7.3 says "ATD does not manage skill installation". Where does the sync helper live? | **(c) Hybrid.** Convention (tool-id naming rule + response shapes) is core ATD; ships in `docs/protocol/wire-format.md` and `docs/atd-architecture.md` §7. The sync helper is a subcommand on the existing `atd` CLI binary (`atd skills sync ...`). No new crate. §7.3 softens by one line: "ATD does not own per-platform install paths; those are convention-driven and easily overridable via `--out-dir`." Keeps protocol crates pure. |
| Q2 | Scoping — single SP across both repos, or two paired SPs? | **One SP, both repos.** Convention shape, `atd skills sync`, and healthkit_cli adoption are tightly coupled — they need to ship together to be self-validating. Each repo still gets independent commits. |
| Q3 | Tool-id shape — `<publisher>:<service>.skills.list/get` (dot segment) or `<publisher>:<service>.skills/list` (slash)? | **Dot.** Matches existing v1.2.0 convention (`huawei:hms.healthkit.heartrate`). No new id-syntax surface. |
| Q4 | `skills.list` response shape? | `Vec<{name: String, description: String, version: Option<String>}>`. `name` is the slug (e.g., `"healthkit-heartrate"`); `description` is the SKILL.md frontmatter `description` field; `version` is reserved for future semver — Optional in v0. |
| Q5 | `skills.get` response shape? | `{name: String, content_md: String}`. **No `format_hint`** — always markdown in v0. Future SP can add if MDX / YAML adoption emerges. |
| Q6 | Required capability for `skills.list/get`? | **None.** Skills are public-information by convention; vendors who want to gate can override per-tool. Empty `required_capabilities`. |
| Q7 | `atd skills sync` per-target install paths? | **Prefix with `<publisher>-<service>-`** to prevent collisions across multiple synced servers: <br>• Hermes: `~/.hermes/skills/<publisher>-<service>-<name>/SKILL.md` <br>• Claude Code: `~/.claude/skills/<publisher>-<service>-<name>/SKILL.md` <br>• `--out-dir` flag overrides for testing or non-standard layouts. |
| Q8 | MCP-bridge auto-install at handshake? | **Out of scope.** Defer until a real adopter wants implicit install; the explicit `atd skills sync` flow is the safer default (prompt-injection-resistant). |
| Q9 | What targets ship in v1? | **Two: `hermes` and `claude-code`.** Cursor uses `.cursor/rules/<name>.mdc` (different format); add when needed. `--target stdout` for piping is trivial — include as a third trivial target. |
| Q10 | Does atd-ref-server need demo skills for self-testing? | **No.** healthkit_cli (in this same SP) is the integration test target. `atd-cli` unit tests use stubbed `AtdClient` against a temp socket. |
| Q11 | Should `skills.list` / `skills.get` count as Hidden? | **No, Read.** They're meta-tools; agents that explicitly look for skills should find them in `discover`. Visible by default. |

## 3. Touch points

**Repo 1: atd-mvp** (one commit, same SP)

| # | File | Change |
|---|---|---|
| 1 | `docs/protocol/wire-format.md` | New section "§N — Skills meta-tool convention". Tool-id naming rule + response shapes (`SkillSummary`, `SkillContent`). Marked as a convention, not a wire-protocol message. |
| 2 | `docs/atd-architecture.md` | §7.3 — soften the "ATD does not manage skill installation" line per Q1. §7.5 — replace "future SP" stub with concrete pointer to this SP. §10 — new status row. |
| 3 | `crates/atd-cli/src/skills.rs` | New module. `pub async fn cmd_skills_sync(...)` + helper functions. ~150-200 lines. Three target adapters: `hermes`, `claude-code`, `stdout`. |
| 4 | `crates/atd-cli/src/main.rs` (or `src/cli.rs`) | Wire `skills sync` subcommand into the clap tree. Args: `--target {hermes,claude-code,stdout}` (required), `--out-dir <path>` (optional, overrides target default), `--dry-run` (lists what would be written without writing). Inherits `--sock` from the global `atd` arg. |
| 5 | `crates/atd-cli/tests/skills_sync.rs` (new integration test file) | Spin up an in-process AtdServer with two stub tools (`stub:test.skills.list` + `stub:test.skills.get`); call `cmd_skills_sync` with `--target stdout`; assert content. ~120 lines. |
| 6 | (none — no new crate) | — |

**Repo 2: healthkit_cli** (one commit, same SP)

| # | File | Change |
|---|---|---|
| H1 | `src/atd_server/skill_tools.rs` (new module) | Two tool impls: `SkillsListTool` (id `huawei:hms.healthkit.skills.list`) and `SkillsGetTool` (id `huawei:hms.healthkit.skills.get`). Reuses `embedded_skill_md(skill_md)` from v1.2.0 (`src/atd_server/helper_tools.rs`). |
| H2 | `src/atd_server/server.rs` | Register the two new tools alongside the 26 helpers — total goes 26 → 28. Update startup log line `"{tool_count} tool(s) registered"`. |
| H3 | `tests/atd_server_helper_tools_e2e.rs` | Bump `assert_eq!(tools.len(), 26)` → `28`; add assertions that `skills.list` returns 26 entries and `skills.get` for one known name returns expected content prefix. |
| H4 | `CHANGELOG.md` | New `## [1.3.0]` entry. |
| H5 | `Cargo.toml` | Bump `version = "1.2.1"` → `"1.3.0"`. |

**Not touched:**

- `atd-protocol`, `atd-runtime`, `atd-server`, `atd-conformance`, `atd-mcp-bridge`, `atd-tools-*`, `atd-ref-server` — convention is *naming-only*; no wire/runtime/server changes.
- No new crate (Q1).
- healthkit_cli's `helper_tools.rs` / `helper_class.rs` — the new tools are a separate module to avoid bloating the existing dispatcher; `embedded_skill_md` is the only shared piece.

## 4. The skills meta-tool convention (the protocol contract)

Two tool ids, fixed shape:

### 4.1 `<publisher>:<service>.skills.list`

**Args:** `{}` (no fields)

**Returns (success path):**

```json
[
  {"name": "healthkit-heartrate", "description": "Query heart rate data from HealthKit"},
  {"name": "healthkit-sleep",     "description": "Query sleep data from HealthKit"},
  …
]
```

Each entry:
- `name: String` — slug, must be unique within a service. Consumed by `skills.get` as the lookup key.
- `description: String` — one-line summary; matches the SKILL.md frontmatter `description` field if present.
- `version: Option<String>` — reserved for future per-skill semver; servers can omit.

**Capabilities:** none required.

### 4.2 `<publisher>:<service>.skills.get`

**Args:** `{"name": "<slug>"}`

**Returns (success path):**

```json
{
  "name": "healthkit-heartrate",
  "content_md": "---\nname: healthkit-heartrate\ndescription: …\n---\n\n# healthkit hk +heartrate\n…"
}
```

- `name` echoes the input.
- `content_md` is the full SKILL.md content as a UTF-8 string.

**Errors:** unknown name → `ToolCallError::ExecutionFailed { code: "skill_not_found", message: …, retryable: false }`.

**Capabilities:** none required.

### 4.3 What this is NOT

- Not a new `Request::SkillList` / `Request::SkillRead` wire message — pure tool-id convention. Adoption is opt-in per server.
- Not a parsing contract — ATD does not validate the SKILL.md content, frontmatter shape, or markdown syntax.
- Not an authentication scheme — gating is at the tool level via `required_capabilities` (none in v0).
- Not version-aware — `version` is reserved but not enforced.

### 4.4 Future evolution

If 2+ vendor servers adopt the convention without divergence, a future SP can promote it to a wire-level `Request::SkillList` / `Request::SkillGet` for ergonomic SDK access. Until then, this is convention-only.

## 5. `atd skills sync` subcommand

### 5.1 CLI shape

```
atd --sock /tmp/hk.sock skills sync --target {hermes|claude-code|stdout} [--out-dir <path>] [--dry-run]
```

- Inherits `--sock` from the global `atd` argument.
- `--target` (required): one of `hermes`, `claude-code`, `stdout`. Drives the per-target default install path.
- `--out-dir` (optional): overrides the target default. For `--target stdout` this is rejected (use shell redirection instead).
- `--dry-run`: lists what would be written without writing.

### 5.2 Target install paths (Q7)

| Target | Default `<out-dir>` | Per-skill path |
|---|---|---|
| `hermes` | `$HOME/.hermes/skills` | `<out-dir>/<publisher>-<service>-<name>/SKILL.md` |
| `claude-code` | `$HOME/.claude/skills` | `<out-dir>/<publisher>-<service>-<name>/SKILL.md` |
| `stdout` | (n/a) | one `--- name ---` divider per skill, then content |

The publisher / service prefix prevents collisions when an agent platform syncs from multiple ATD servers (e.g., one healthkit + one fitness-vendor).

### 5.3 Algorithm

```
1. Connect to --sock via atd_sdk::AtdClient.
2. discover() with no filter; collect all tool ids matching `*.skills.list`.
   For each match (allow multiple in case of multi-publisher servers):
3.   Call <id>(args={}) — parse response as Vec<SkillSummary>.
4.   Parse <id> into <publisher>:<service> prefix; remember for path construction.
5.   For each (name, _) in the list:
6.     Call <publisher>:<service>.skills.get(args={name}) — parse response as SkillContent.
7.     Construct target path per §5.2.
8.     If --dry-run: print "[would write] <path> (<bytes> bytes)".
9.     Else: mkdir -p parent; write content_md to <path>; print "[wrote] <path>".
10. Print summary: "<n> skill(s) synced from <m> publisher(s) to <out-dir>".
```

### 5.4 Edge cases

- **No `*.skills.list` tool found** — print one-line warning to stderr, exit 0 (server doesn't publish skills; not an error).
- **`skills.get` returns name not in earlier `skills.list`** — accept; some servers may publish skills not in the catalog (unusual but valid).
- **Path collision (file exists at target)** — overwrite without backup. Keeps the binary simple; users can use `--out-dir <fresh-dir>` for safety.
- **Invalid characters in `name`** — sanitize before path construction (replace anything outside `[a-zA-Z0-9._-]` with `_`).

## 6. healthkit_cli adoption

### 6.1 `SkillsListTool` (`src/atd_server/skill_tools.rs`)

```rust
pub struct SkillsListTool {
    def: ToolDefinition,
}

impl SkillsListTool {
    pub fn new() -> Self { /* ... build ToolDefinition with id "huawei:hms.healthkit.skills.list", capability domain "healthkit", visibility Read, no required_capabilities ... */ }
}

impl Tool for SkillsListTool {
    fn definition(&self) -> &ToolDefinition { &self.def }
    fn call<'a>(&'a self, _args: Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async {
            let entries: Vec<Value> = HELPER_CONFIGS
                .iter()
                .map(|c| serde_json::json!({
                    "name": c.skill_md,
                    "description": skill_description(c.skill_md).unwrap_or_default(),
                }))
                .collect();
            Ok(serde_json::Value::Array(entries))
        })
    }
}
```

`skill_description` is a small helper that pulls the frontmatter `description:` field from `embedded_skill_md(c.skill_md)`. If `skill_md_parser::parse_skill_md` already extracts this (T1 of SP-healthkit-helper-tools), reuse it.

The `name` field is the skill slug (`healthkit-heartrate`), which matches `HelperConfig::skill_md` and is the lookup key used by `embedded_skill_md`.

### 6.2 `SkillsGetTool`

Same shape; on call, parses `name` from args, returns `{name, content_md: embedded_skill_md(name).unwrap_or("")}`. Returns `ToolCallError::ExecutionFailed { code: "skill_not_found", … }` if `embedded_skill_md` returns None.

### 6.3 Server registration

In `src/atd_server/server.rs`, after the helper-tool registration loop:

```rust
// Skills meta-tools (SP-skills-discovery-convention).
registry.register(Arc::new(skill_tools::SkillsListTool::new()));
registry.register(Arc::new(skill_tools::SkillsGetTool::new()));
tool_count += 2;
```

Total tools: 26 helpers + 2 skill meta-tools = **28** (default; with `--expose-raw-tools` the 8 raw tools land too → 36 total).

### 6.4 e2e test update

`tests/atd_server_helper_tools_e2e.rs`:

```rust
assert_eq!(tools.len(), 28, "expected 26 helpers + 2 skill meta-tools, got {}", tools.len());
let ids: Vec<&str> = tools.iter().map(|t| t.id.as_str()).collect();
assert!(ids.contains(&"huawei:hms.healthkit.skills.list"));
assert!(ids.contains(&"huawei:hms.healthkit.skills.get"));

// Round-trip skills.list and pick one to skills.get
let list_result = client.call("huawei:hms.healthkit.skills.list", json!({}), CallOptions::default()).await.expect("list");
let entries = list_result.data().expect("list data").as_array().expect("array");
assert_eq!(entries.len(), 26);

let get_result = client.call("huawei:hms.healthkit.skills.get", json!({"name": "healthkit-heartrate"}), CallOptions::default()).await.expect("get");
let content = get_result.data().expect("get data");
assert!(content["content_md"].as_str().unwrap_or("").contains("heartrate"));
```

## 7. Versioning

| Repo | Crate / Package | Before | After | Reason |
|---|---|---|---|---|
| atd-mvp | (workspace) | 0.3.0 | 0.3.0 | No protocol/SDK/runtime change; convention is doc-only + a CLI subcommand. **No bump.** |
| atd-mvp | `atd-cli` | 0.3.0 | 0.3.0 | New subcommand is additive; CLI consumers don't pin against a feature contract. |
| healthkit_cli | `healthkit_cli` | 1.2.1 | 1.3.0 | Two new public tool ids registered by default; agent-visible surface change. Minor bump. |

## 8. Validation

Exit gates (must all be ✅ before commit):

**atd-mvp side:**
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features` — passes (current 358 + ~3 new in atd-cli)
- [ ] `cargo build --release --workspace`

**healthkit_cli side:**
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo test --all-targets` — passes (current 209 + ~3 new for skill_tools)

**Integration (manual, against running healthkit serve):**
- [ ] Build healthkit_cli@v1.3.0; start `healthkit serve --sock /tmp/hk.sock`
- [ ] Run `atd --sock /tmp/hk.sock skills sync --target stdout` — emits 26 SKILL.md blocks
- [ ] Run `atd --sock /tmp/hk.sock skills sync --target hermes --out-dir /tmp/sync-test/hermes` — creates 26 dirs with SKILL.md
- [ ] Run `atd --sock /tmp/hk.sock skills sync --target claude-code --out-dir /tmp/sync-test/claude` — creates 26 dirs with SKILL.md
- [ ] Diff `/tmp/sync-test/hermes/huawei-hms-healthkit-heartrate/SKILL.md` against `~/proj/healthkit_cli/skills/healthkit-heartrate/SKILL.md` — content matches modulo the prefix in dir name

## 9. Out of scope (deferred)

- **MCP-bridge auto-install at handshake** (Q8) — adds an implicit FS-write side-effect to a stdio bridge; defer until a real adopter need surfaces.
- **Cursor target** (`.cursor/rules/<name>.mdc`) — different format (MDX with YAML frontmatter); add when a real Cursor user requests it.
- **Wire-level `Request::SkillList/SkillGet`** — promote convention to first-class messages only when 2+ vendors adopt without divergence.
- **Skill versioning enforcement** — `version` field is reserved but not consumed.
- **`atd-skills-sync` as a separate crate** — folded into `atd-cli` per Q1; promote to a separate binary if users start asking for `cargo install atd-skills-sync` independently.
- **`format_hint` in `skills.get` response** — always markdown in v0; revisit if MDX / YAML / other adoption emerges.
- **healthkit_cli `healthkit-shared` / 27th skill** — the `HELPER_CONFIGS` table has 26 entries; the `healthkit-shared/SKILL.md` is referenced *by* helpers but not itself a helper. Skip for v1; add only if `atd skills sync` users ask for it.

## 10. Architecture.md edits

### §7.3 (line 605, "ATD's non-commitments")

Change:

```
- ATD does not manage skill installation
```

to:

```
- ATD does not own per-platform skill install paths or progressive-disclosure runtime; those are convention-driven (see §7.5) and easily overridable. ATD ships an optional `atd skills sync` helper that writes per-target paths derived from the [skills meta-tool convention](protocol/wire-format.md#skills-meta-tool-convention).
```

### §7.5 (currently "Future: SKILL.md generation from ATD tools")

Replace the "future SP (proposed)" line with:

```
The [skills meta-tool convention](protocol/wire-format.md#skills-meta-tool-convention) (SP-skills-discovery-convention, 2026-04-27) standardizes how ATD servers publish their skills via two meta-tool ids — `<publisher>:<service>.skills.list` and `<publisher>:<service>.skills.get`. The `atd skills sync` subcommand pulls those skills into per-platform directories (hermes, claude-code, stdout). Reverse direction (generating SKILL.md stubs FROM tool definitions) remains a future SP candidate.
```

### §10 status table (new row, after SP-tool-visibility-hidden)

```
| Skills meta-tool convention + `atd skills sync` | Skills (adjacent) | ✅ | SP-skills-discovery-convention | 2026-04-27 | Landed; convention defined in `docs/protocol/wire-format.md` (§N); `atd skills sync` subcommand ships in atd-cli with hermes / claude-code / stdout targets; healthkit_cli v1.3.0 first adopter, exposing 26 SKILL.md files via `huawei:hms.healthkit.skills.list/get`. §7.3 softened: ATD ships an optional sync helper but does not own per-platform install paths. Promotion to wire-level `Request::SkillList/Get` deferred until 2+ vendors adopt without divergence. |
```
