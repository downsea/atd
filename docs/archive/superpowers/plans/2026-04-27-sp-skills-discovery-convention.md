# SP-skills-discovery-convention Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a Skills meta-tool convention (`<publisher>:<service>.skills.list/get`) in atd-mvp's protocol docs, ship an `atd skills sync` subcommand on the existing atd-cli with hermes/claude-code/stdout targets, and adopt the convention in healthkit_cli (26 SKILL.md files exposed at `huawei:hms.healthkit.skills.list/get`).

**Architecture:** Convention is naming-only (no wire change). atd-cli grows one subcommand calling `atd_sdk::AtdClient` against any ATD server that publishes `*.skills.list`. healthkit_cli adds a 2-tool module reusing v1.2.0's `embedded_skill_md` helper. Both repos commit independently in this same SP.

**Tech Stack:** Rust 2021, Tokio, atd-sdk / atd-cli (atd-mvp); healthkit_cli's existing atd-server module.

**Spec:** [`../specs/2026-04-27-sp-skills-discovery-convention-design.md`](../specs/2026-04-27-sp-skills-discovery-convention-design.md)

---

## Task 1: Document the convention in atd-mvp protocol docs

**Files:**
- Modify: `docs/protocol/wire-format.md` (append a new section)
- Modify: `docs/architecture.md` (§7.3, §7.5, §10 — see spec §10)

- [ ] **Step 1: Find the right insert point in wire-format.md**

```bash
grep -n "^## " docs/protocol/wire-format.md | tail -10
```

Identify the last numbered section. The new section is **non-wire** (it's a convention) so it goes near the end, ideally before any "Errors" or "Appendix" section.

- [ ] **Step 2: Write the new section**

Append to `docs/protocol/wire-format.md`:

```markdown
## Skills meta-tool convention

This is a **convention**, not a wire-protocol message. ATD servers that wish to publish their skill files (e.g., SKILL.md content) to agent platforms expose two meta-tools at fixed ids.

### `<publisher>:<service>.skills.list`

**Args:** `{}` (no fields)

**Returns:** `Vec<SkillSummary>` where each entry is `{name: String, description: String, version: Option<String>}`. The `name` field is a slug (e.g., `"healthkit-heartrate"`), unique within the service, and is the lookup key for `skills.get`.

**Required capabilities:** none (skills are public-information by convention; vendors who want to gate can override per-tool).

### `<publisher>:<service>.skills.get`

**Args:** `{name: String}`

**Returns:** `{name: String, content_md: String}`. The `content_md` field is the full skill file content as a UTF-8 string. Format is markdown by convention.

**Errors:** Unknown name returns `ToolCallError::ExecutionFailed { code: "skill_not_found", message: ..., retryable: false }`.

### What this is NOT

- Not a wire-level `Request::SkillList` / `Request::SkillGet` message — pure tool-id naming. Adoption is opt-in.
- Not a SKILL.md parsing contract — ATD does not validate frontmatter or markdown syntax.
- Not version-aware in v0 — `version` field is reserved but not enforced.

### Future evolution

If 2+ vendor servers adopt this convention without divergence, a future SP can promote it to a wire-level `Request::SkillList` / `Request::SkillGet`. Until then, convention-only.

### See also

- `atd skills sync` subcommand — pulls skills via this convention into per-platform directories (hermes, claude-code, stdout).
- SP-skills-discovery-convention spec for the full design rationale.
```

- [ ] **Step 3: Update architecture.md §7.3**

Find the line `- ATD does not manage skill installation` in `docs/architecture.md` and replace it per spec §10. Use Edit tool with the exact spec text.

- [ ] **Step 4: Update architecture.md §7.5**

Find the existing §7.5 paragraph starting "A future SP (proposed) adds..." and replace with the spec §10 text pointing to this SP.

- [ ] **Step 5: Add the §10 status row**

Find the row `| Skills meta-tool convention + ...` (already in spec §10) and insert into the §10 status table, right after the SP-tool-visibility-hidden row.

- [ ] **Step 6: Verify markdown still renders**

```bash
grep -c "## Skills meta-tool convention" docs/protocol/wire-format.md
grep -c "atd skills sync" docs/architecture.md
```

Expected: both ≥ 1.

- [ ] **Step 7: Commit**

```bash
git add docs/protocol/wire-format.md docs/architecture.md
git commit -m "docs(protocol): skills meta-tool convention + architecture updates"
```

---

## Task 2: `atd skills sync` subcommand — module + dispatcher

**Files:**
- Create: `crates/atd-cli/src/skills.rs`
- Modify: `crates/atd-cli/src/lib.rs` or `src/main.rs` (whichever exposes the subcommand tree)
- Modify: clap argument parsing wherever existing subcommands like `list`, `describe`, `call` are registered

- [ ] **Step 1: Locate the existing subcommand tree**

```bash
grep -rn "Subcommand\|#\[command\|.subcommand(" crates/atd-cli/src/ | head -20
```

Note where existing subcommands (`list`, `describe`, `call`, etc.) are wired. The new `skills sync` subcommand should follow the same pattern. If the CLI uses a `clap::Subcommand` enum, add a `Skills(SkillsCmd)` variant. If a sub-subcommand pattern exists already, mirror it.

- [ ] **Step 2: Define the clap struct**

In `crates/atd-cli/src/skills.rs`:

```rust
//! `atd skills sync` — pull skill files from an ATD server via the skills
//! meta-tool convention and write them to per-platform install paths.
//!
//! See SP-skills-discovery-convention.

use std::path::PathBuf;
use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Args)]
pub struct SkillsCmd {
    #[command(subcommand)]
    pub action: SkillsAction,
}

#[derive(Debug, Subcommand)]
pub enum SkillsAction {
    /// Sync skills from the connected ATD server to a per-platform directory.
    Sync(SyncArgs),
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Where to write the synced skills.
    #[arg(long, value_enum)]
    pub target: SyncTarget,
    /// Override the target's default install directory.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,
    /// List what would be written without writing.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SyncTarget {
    Hermes,
    ClaudeCode,
    Stdout,
}

impl SyncTarget {
    pub fn default_out_dir(&self) -> Option<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        match self {
            SyncTarget::Hermes => Some(home.join(".hermes/skills")),
            SyncTarget::ClaudeCode => Some(home.join(".claude/skills")),
            SyncTarget::Stdout => None,
        }
    }
}
```

- [ ] **Step 3: Wire into the main subcommand tree**

In whatever file holds the top-level `Subcommand` enum, add:

```rust
#[derive(Debug, Subcommand)]
pub enum Cmd {
    // ... existing variants ...
    /// Sync skills from a connected ATD server to a per-platform directory.
    Skills(crate::skills::SkillsCmd),
}
```

And in the dispatch (e.g., `match cmd { ... }`):

```rust
Cmd::Skills(skills_cmd) => match skills_cmd.action {
    crate::skills::SkillsAction::Sync(args) => {
        crate::skills::cmd_skills_sync(&global_sock, args).await?;
    }
},
```

`global_sock` is whatever holds the `--sock` path (look at how existing commands like `list` access it).

- [ ] **Step 4: Implement `cmd_skills_sync`**

In `crates/atd-cli/src/skills.rs`:

```rust
use anyhow::{anyhow, Context};
use atd_sdk::{AtdClient, CallOptions, DiscoverFilter, Endpoint};
use serde_json::Value;
use std::path::PathBuf;

pub async fn cmd_skills_sync(sock: &PathBuf, args: SyncArgs) -> anyhow::Result<()> {
    let out_dir = args
        .out_dir
        .clone()
        .or_else(|| args.target.default_out_dir());

    if matches!(args.target, SyncTarget::Stdout) && out_dir.is_some() && !args.out_dir.is_none() {
        return Err(anyhow!("--out-dir cannot be combined with --target stdout; pipe instead"));
    }

    let client = AtdClient::connect(Endpoint::unix(sock.clone()))
        .await
        .context("connect to ATD server")?;

    let tools = client
        .discover(None, DiscoverFilter::default())
        .await
        .context("discover")?;

    // Find every <publisher>:<service>.skills.list tool.
    let list_tools: Vec<&str> = tools
        .iter()
        .map(|t| t.id.as_str())
        .filter(|id| id.ends_with(".skills.list"))
        .collect();

    if list_tools.is_empty() {
        eprintln!("no *.skills.list tool found on this server; nothing to sync");
        return Ok(());
    }

    let mut total_synced = 0;
    let mut publishers = 0;

    for list_id in &list_tools {
        publishers += 1;
        let prefix = list_id
            .strip_suffix(".skills.list")
            .ok_or_else(|| anyhow!("malformed list tool id: {list_id}"))?;
        let get_id = format!("{prefix}.skills.get");

        let list_result = client
            .call(list_id, serde_json::json!({}), CallOptions::default())
            .await
            .with_context(|| format!("call {list_id}"))?;
        let entries = list_result
            .data()
            .ok_or_else(|| anyhow!("{list_id} returned no data"))?
            .as_array()
            .ok_or_else(|| anyhow!("{list_id} returned non-array"))?
            .clone();

        // <publisher>:<service> → "<publisher>-<service>" prefix
        let dir_prefix = prefix.replace(':', "-").replace('.', "-");

        for entry in &entries {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("entry missing 'name': {entry}"))?;

            let get_result = client
                .call(
                    &get_id,
                    serde_json::json!({"name": name}),
                    CallOptions::default(),
                )
                .await
                .with_context(|| format!("call {get_id} {name}"))?;
            let content = get_result
                .data()
                .and_then(|d| d.get("content_md").cloned())
                .and_then(|c| c.as_str().map(String::from))
                .ok_or_else(|| anyhow!("{get_id}({name}) returned no content_md"))?;

            write_skill(&args.target, &out_dir, &dir_prefix, name, &content, args.dry_run)?;
            total_synced += 1;
        }
    }

    let dest = out_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "stdout".into());
    eprintln!("{total_synced} skill(s) synced from {publishers} publisher(s) to {dest}");
    Ok(())
}

fn write_skill(
    target: &SyncTarget,
    out_dir: &Option<PathBuf>,
    dir_prefix: &str,
    name: &str,
    content: &str,
    dry_run: bool,
) -> anyhow::Result<()> {
    let safe_name = sanitize_name(name);
    match target {
        SyncTarget::Stdout => {
            println!("--- {dir_prefix}-{safe_name} ---");
            println!("{content}");
            Ok(())
        }
        SyncTarget::Hermes | SyncTarget::ClaudeCode => {
            let base = out_dir
                .as_ref()
                .ok_or_else(|| anyhow!("no out_dir resolved for target"))?;
            let dir = base.join(format!("{dir_prefix}-{safe_name}"));
            let path = dir.join("SKILL.md");
            if dry_run {
                eprintln!("[would write] {} ({} bytes)", path.display(), content.len());
            } else {
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("create dir {}", dir.display()))?;
                std::fs::write(&path, content)
                    .with_context(|| format!("write {}", path.display()))?;
                eprintln!("[wrote] {}", path.display());
            }
            Ok(())
        }
    }
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
```

- [ ] **Step 5: Build the workspace**

```bash
cargo build -p atd-cli 2>&1 | tail -10
```

Expected: clean build. If errors mention missing imports, add them; if `AtdClient::connect` / `client.discover` / `client.call` signatures differ, adapt. Reference `crates/atd-cli/src/list.rs` and other existing subcommands for the established calling pattern.

- [ ] **Step 6: Commit**

```bash
git add crates/atd-cli/src/skills.rs crates/atd-cli/src/main.rs crates/atd-cli/src/lib.rs
git commit -m "feat(atd-cli): atd skills sync subcommand (3 targets)"
```

---

## Task 3: `atd skills sync` integration test

**Files:**
- Create: `crates/atd-cli/tests/skills_sync.rs`

- [ ] **Step 1: Look at existing atd-cli integration tests for boilerplate**

```bash
ls crates/atd-cli/tests/ 2>/dev/null
cat crates/atd-cli/tests/*.rs 2>/dev/null | head -80
```

If integration tests already exist, copy their boilerplate (server spawn, temp socket, client connect). If not, the closest reference is `crates/atd-server/src/connection.rs` test mod for in-process server bring-up, OR `crates/healthkit_cli/tests/atd_server_helper_tools_e2e.rs` for an end-to-end pattern using a real socket.

- [ ] **Step 2: Write the test**

`crates/atd-cli/tests/skills_sync.rs`:

```rust
//! Integration test for `atd skills sync` against a stub server.
//!
//! Spins up an in-process `atd-server` registry with two stub tools
//! mimicking the skills meta-tool convention (`stub:test.skills.list`
//! + `stub:test.skills.get`), then drives `cmd_skills_sync` with
//! `--target stdout` and asserts the printed content.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Registry, Tool};
use atd_server::{Server, ServerConfig};

// Stub `stub:test.skills.list`
struct StubListTool {
    def: ToolDefinition,
}
impl StubListTool {
    fn new() -> Self {
        Self {
            def: stub_def("stub:test.skills.list"),
        }
    }
}
impl Tool for StubListTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async {
            Ok(serde_json::json!([
                {"name": "alpha", "description": "alpha skill"},
                {"name": "beta",  "description": "beta skill"}
            ]))
        })
    }
}

// Stub `stub:test.skills.get`
struct StubGetTool {
    def: ToolDefinition,
}
impl StubGetTool {
    fn new() -> Self {
        Self {
            def: stub_def("stub:test.skills.get"),
        }
    }
}
impl Tool for StubGetTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }
    fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Box::pin(async move {
            if name == "alpha" || name == "beta" {
                Ok(serde_json::json!({
                    "name": name,
                    "content_md": format!("# {name}\n\ncontent for {name}\n")
                }))
            } else {
                Err(ToolCallError::ExecutionFailed {
                    code: "skill_not_found".into(),
                    message: format!("unknown skill: {name}"),
                    retryable: false,
                })
            }
        })
    }
}

fn stub_def(id: &str) -> ToolDefinition {
    ToolDefinition {
        id: id.into(),
        name: id.into(),
        description: "skills meta-tool stub".into(),
        version: "0.0.0".into(),
        capability: ToolCapability {
            domain: "test".into(),
            actions: vec!["op".into()],
            tags: vec![],
            intent_examples: vec![],
        },
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: serde_json::json!({}),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Read,
            dry_run: false,
            side_effects: vec![],
            data_sensitivity: None,
        },
        resources: ToolResources {
            timeout_ms: 1000,
            max_concurrent: 1,
            rate_limit_per_min: None,
            estimated_tokens: None,
        },
        trust: ToolTrust {
            publisher: "test".into(),
            trust_level: TrustLevel::L0Unverified,
            signature: None,
        },
        visibility: ToolVisibility::Read,
        required_capabilities: vec![],
        tier: None,
        errors: vec![],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn skills_sync_stdout_round_trips_two_stubs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = dir.path().join("test.sock");
    let mut reg = Registry::new();
    reg.register(Arc::new(StubListTool::new()));
    reg.register(Arc::new(StubGetTool::new()));
    let cfg = ServerConfig {
        socket_path: sock.clone(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        max_output_bytes: 1_048_576,
        default_call_timeout_ms: 60_000,
        granted_capabilities: vec![],
        audit_sink: None,
        server_version: "test 0.0.0".into(),
    };
    let server_task = tokio::spawn(async move { Server::new(reg, cfg).run().await });

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !sock.exists() {
        if std::time::Instant::now() > deadline {
            panic!("server didn't bind socket within 5s");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Drive the subcommand programmatically. (Don't shell-out — keeps the
    // test in-process and lets us catch errors with `?`.)
    use atd_cli::skills::{cmd_skills_sync, SyncArgs, SyncTarget};
    cmd_skills_sync(
        &sock,
        SyncArgs {
            target: SyncTarget::Stdout,
            out_dir: None,
            dry_run: false,
        },
    )
    .await
    .expect("sync stdout");

    // Sync to a temp dir for hermes layout.
    let hermes_dir = dir.path().join("hermes");
    cmd_skills_sync(
        &sock,
        SyncArgs {
            target: SyncTarget::Hermes,
            out_dir: Some(hermes_dir.clone()),
            dry_run: false,
        },
    )
    .await
    .expect("sync hermes");

    let alpha = hermes_dir.join("stub-test-alpha/SKILL.md");
    let beta = hermes_dir.join("stub-test-beta/SKILL.md");
    assert!(alpha.exists(), "alpha SKILL.md must be written: {alpha:?}");
    assert!(beta.exists(), "beta SKILL.md must be written: {beta:?}");
    let alpha_content = std::fs::read_to_string(&alpha).expect("read alpha");
    assert!(alpha_content.contains("content for alpha"));

    server_task.abort();
}
```

(`atd_cli::skills` module visibility may need `pub` on `cmd_skills_sync`, `SyncArgs`, `SyncTarget` — adjust.)

- [ ] **Step 3: Run the test**

```bash
cargo test -p atd-cli --test skills_sync 2>&1 | tail -20
```

Expected: PASS. If the test can't import `atd_cli::skills::*`, add `pub mod skills;` in `crates/atd-cli/src/lib.rs` (if there's no lib.rs, this signals atd-cli is bin-only; in that case, see the [bin-only fallback] below).

- [ ] **Step 4 (fallback if atd-cli has no `lib.rs`):**

If atd-cli is bin-only and the integration test can't import the module, either:
- (a) Add a `crates/atd-cli/src/lib.rs` re-exporting the public surface needed for tests (small refactor; one extra file).
- (b) Drop the in-process style and shell out to the built binary in the test (heavier; uses `Command::new(env!("CARGO_BIN_EXE_atd"))`).

(a) is cleaner. Pick (a) unless there's a strong reason against.

- [ ] **Step 5: Commit**

```bash
git add crates/atd-cli/tests/skills_sync.rs crates/atd-cli/src/lib.rs
git commit -m "test(atd-cli): integration test for skills sync (stdout + hermes targets)"
```

---

## Task 4: healthkit_cli — `SkillsListTool` + `SkillsGetTool`

**Repo:** healthkit_cli (`~/proj/healthkit_cli`)

**Files:**
- Create: `src/atd_server/skill_tools.rs`
- Modify: `src/atd_server/mod.rs` (or wherever submodules are declared) — add `pub mod skill_tools;`
- Modify: `src/atd_server/server.rs` — register the two new tools
- Modify: `tests/atd_server_helper_tools_e2e.rs` — bump 26 → 28
- Modify: `Cargo.toml` — version 1.2.1 → 1.3.0
- Modify: `CHANGELOG.md` — v1.3.0 entry

- [ ] **Step 1: cd into the healthkit_cli repo**

```bash
cd ~/proj/healthkit_cli
```

- [ ] **Step 2: Look up `embedded_skill_md` and `parse_skill_md`**

```bash
grep -n "fn embedded_skill_md\|fn parse_skill_md\|pub fn parse_skill_md\|frontmatter\|description:" src/atd_server/helper_tools.rs src/atd_server/skill_md_parser.rs | head -20
```

Find: (a) where the SKILL.md content lookup lives (`embedded_skill_md(slug) -> Option<&'static str>`), and (b) how to extract the frontmatter `description:` field. If a parser already returns the description (T1 of SP-helper-tools), reuse it.

- [ ] **Step 3: Write the new module**

`src/atd_server/skill_tools.rs`:

```rust
//! Skills meta-tool implementations for the SP-skills-discovery-convention.
//!
//! Two tools registered alongside the 26 helper-tools:
//! - `huawei:hms.healthkit.skills.list` — returns the catalog
//! - `huawei:hms.healthkit.skills.get` — returns one skill's content
//!
//! Both reuse `embedded_skill_md` from `helper_tools.rs` (compile-time
//! `include_str!` of all 26 SKILL.md files).

use atd_protocol::{
    BindingProtocol, SafetyLevel, ToolBinding, ToolCapability, ToolDefinition, ToolResources,
    ToolSafety, ToolTrust, ToolVisibility, TrustLevel,
};
use atd_runtime::context::CallContext;
use atd_runtime::error::ToolCallError;
use atd_runtime::registry::{CallFuture, Tool};

use crate::atd_server::helper_class::HELPER_CONFIGS;
use crate::atd_server::helper_tools::embedded_skill_md;
use crate::atd_server::skill_md_parser::parse_skill_md;

fn skill_def(id: &str, description: &str) -> ToolDefinition {
    ToolDefinition {
        id: id.into(),
        name: id.into(),
        description: description.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        capability: ToolCapability {
            domain: "healthkit".into(),
            actions: vec![id.split('.').last().unwrap_or("op").into()],
            tags: vec!["meta".into(), "skills".into()],
            intent_examples: vec![],
        },
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: serde_json::json!({}),
        bindings: vec![ToolBinding {
            protocol: BindingProtocol::Cli,
            config: serde_json::json!({}),
        }],
        safety: ToolSafety {
            level: SafetyLevel::Read,
            dry_run: false,
            side_effects: vec![],
            data_sensitivity: None,
        },
        resources: ToolResources {
            timeout_ms: 1000,
            max_concurrent: 4,
            rate_limit_per_min: None,
            estimated_tokens: None,
        },
        trust: ToolTrust {
            publisher: "huawei".into(),
            trust_level: TrustLevel::L1SchemaValid,
            signature: None,
        },
        visibility: ToolVisibility::Read,
        required_capabilities: vec![],
        tier: None,
        errors: vec![],
    }
}

pub struct SkillsListTool {
    def: ToolDefinition,
}

impl SkillsListTool {
    pub fn new() -> Self {
        Self {
            def: skill_def(
                "huawei:hms.healthkit.skills.list",
                "List all SKILL.md files published by this server. Returns Vec<{name, description, version?}>.",
            ),
        }
    }
}

impl Default for SkillsListTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for SkillsListTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    fn call<'a>(&'a self, _args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        Box::pin(async {
            let entries: Vec<serde_json::Value> = HELPER_CONFIGS
                .iter()
                .map(|c| {
                    let description = embedded_skill_md(c.skill_md)
                        .and_then(|md| parse_skill_md(md))
                        .map(|p| p.description)
                        .unwrap_or_default();
                    serde_json::json!({
                        "name": c.skill_md,
                        "description": description,
                    })
                })
                .collect();
            Ok(serde_json::Value::Array(entries))
        })
    }
}

pub struct SkillsGetTool {
    def: ToolDefinition,
}

impl SkillsGetTool {
    pub fn new() -> Self {
        Self {
            def: skill_def(
                "huawei:hms.healthkit.skills.get",
                "Get one SKILL.md file by name. Args: {name: String}. Returns {name, content_md}.",
            ),
        }
    }
}

impl Default for SkillsGetTool {
    fn default() -> Self {
        Self::new()
    }
}

impl Tool for SkillsGetTool {
    fn definition(&self) -> &ToolDefinition {
        &self.def
    }

    fn call<'a>(&'a self, args: serde_json::Value, _ctx: &'a CallContext) -> CallFuture<'a> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Box::pin(async move {
            if name.is_empty() {
                return Err(ToolCallError::InvalidArgs(
                    "missing required arg 'name'".into(),
                ));
            }
            match embedded_skill_md(&name) {
                Some(content) => Ok(serde_json::json!({
                    "name": name,
                    "content_md": content,
                })),
                None => Err(ToolCallError::ExecutionFailed {
                    code: "skill_not_found".into(),
                    message: format!("unknown skill: {name}"),
                    retryable: false,
                }),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_list_def_id_matches_convention() {
        let t = SkillsListTool::new();
        assert_eq!(t.definition().id, "huawei:hms.healthkit.skills.list");
        assert!(t.definition().required_capabilities.is_empty());
    }

    #[test]
    fn skills_get_def_id_matches_convention() {
        let t = SkillsGetTool::new();
        assert_eq!(t.definition().id, "huawei:hms.healthkit.skills.get");
        assert!(t.definition().required_capabilities.is_empty());
    }
}
```

- [ ] **Step 4: Add the module declaration**

In `src/atd_server/mod.rs` (or wherever the existing `pub mod helper_tools;` is), add:

```rust
pub mod skill_tools;
```

Verify `embedded_skill_md` is `pub` (or `pub(crate)`) — adjust if private.

Verify `parse_skill_md` returns a struct with a `description: String` field — adjust if shape differs.

- [ ] **Step 5: Register in `server.rs`**

In `src/atd_server/server.rs::serve`, after the helper-tool registration loop:

```rust
// Skills meta-tools (SP-skills-discovery-convention).
registry.register(Arc::new(crate::atd_server::skill_tools::SkillsListTool::new()));
registry.register(Arc::new(crate::atd_server::skill_tools::SkillsGetTool::new()));
tool_count += 2;
```

- [ ] **Step 6: Update startup log line**

The `eprintln!("healthkit-server: {tool_count} tool(s) registered ...")` already uses the bumped `tool_count`. Verify the log mentions skills meta-tools by adjusting the message:

```rust
eprintln!(
    "healthkit-server: {tool_count} tool(s) registered (huawei:hms.healthkit.* — 26 helpers + 2 skills meta-tools); starting"
);
```

(If `--expose-raw-tools` is also set, this line gets +8 raw — adjust the parenthetical accordingly to reflect actual content.)

- [ ] **Step 7: Run the new module's unit tests**

```bash
cargo test --lib atd_server::skill_tools 2>&1 | tail -10
```

Expected: 2 passing.

- [ ] **Step 8: Update the e2e test**

In `tests/atd_server_helper_tools_e2e.rs`, find:

```rust
assert_eq!(
    tools.len(),
    26,
    "expected 26 helper tools, got {}",
    tools.len()
);
```

Change to:

```rust
assert_eq!(
    tools.len(),
    28,
    "expected 26 helpers + 2 skill meta-tools, got {}",
    tools.len()
);
```

After the existing `ids.contains(&"huawei:hms.healthkit.healthkit-overview")` assertion, add:

```rust
assert!(ids.contains(&"huawei:hms.healthkit.skills.list"));
assert!(ids.contains(&"huawei:hms.healthkit.skills.get"));
```

After the existing dry-run check, append a round-trip:

```rust
// (4) skills.list returns 26 entries.
let list_result = client
    .call("huawei:hms.healthkit.skills.list", serde_json::json!({}), CallOptions::default())
    .await
    .expect("skills.list");
let entries = list_result
    .data()
    .expect("list data")
    .as_array()
    .expect("array")
    .clone();
assert_eq!(entries.len(), 26);

// (5) skills.get for healthkit-heartrate returns content_md containing "heartrate".
let get_result = client
    .call(
        "huawei:hms.healthkit.skills.get",
        serde_json::json!({"name": "healthkit-heartrate"}),
        CallOptions::default(),
    )
    .await
    .expect("skills.get");
let content = get_result.data().expect("get data");
assert_eq!(content["name"], "healthkit-heartrate");
assert!(
    content["content_md"]
        .as_str()
        .unwrap_or("")
        .contains("heartrate"),
    "content_md should contain 'heartrate'"
);
```

- [ ] **Step 9: Run the e2e test**

```bash
cargo test --test atd_server_helper_tools_e2e 2>&1 | tail -15
```

Expected: PASS.

- [ ] **Step 10: Bump version + write CHANGELOG**

In `Cargo.toml`: `version = "1.2.1"` → `version = "1.3.0"`.

In `CHANGELOG.md`, prepend a new entry (after the `# Changelog` heading and before `## [1.2.1]`):

```markdown
## [1.3.0] — 2026-04-27

Skills meta-tool surface. The ATD server now publishes 28 tools by default (was 26 in v1.2.0): the 26 helpers plus two new meta-tools `huawei:hms.healthkit.skills.list` and `huawei:hms.healthkit.skills.get` implementing the [SP-skills-discovery-convention](https://github.com/downsea/atd-mvp/blob/master/docs/superpowers/specs/2026-04-27-sp-skills-discovery-convention-design.md). Agent platforms that support the convention can now pull the 26 SKILL.md files from a running healthkit server via `atd skills sync --target hermes` (or `--target claude-code`).

### Features

- **`huawei:hms.healthkit.skills.list`** — returns `Vec<{name, description}>` for the 26 helpers, derived from `HELPER_CONFIGS` + frontmatter parse of each `embedded_skill_md`.
- **`huawei:hms.healthkit.skills.get`** — returns `{name, content_md}` for one skill by slug. Unknown name returns `ToolCallError::ExecutionFailed { code: "skill_not_found" }`.
- Both tools are visibility=Read, no required_capabilities.

### Validation

- `cargo test --all-targets`: passing (was 209 in v1.2.1; +N tests for the new module + e2e additions)
- `tests/atd_server_helper_tools_e2e.rs`: tool count assertion bumped 26 → 28; new round-trip checks for `skills.list` returning 26 entries and `skills.get` for `healthkit-heartrate` returning expected content prefix
- Verified end-to-end: `atd skills sync --target stdout --sock /tmp/hk.sock` round-trips 26 SKILL.md blocks

### Out of scope (deferred)

- Cursor target for `atd skills sync` — different format (MDX with YAML frontmatter)
- MCP-bridge auto-install at handshake
- Wire-level `Request::SkillList/SkillGet` (promote convention if 2+ vendors adopt)
```

- [ ] **Step 11: Run full validation**

```bash
cd ~/proj/healthkit_cli
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --all-targets
```

All three must pass.

- [ ] **Step 12: Commit**

```bash
git add src/atd_server/skill_tools.rs src/atd_server/mod.rs src/atd_server/server.rs tests/atd_server_helper_tools_e2e.rs Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "$(cat <<'EOF'
feat(atd-server): skills meta-tools (v1.3.0)

Implements the SP-skills-discovery-convention by exposing 26 SKILL.md
files via two new meta-tools at the conventional ids:
- huawei:hms.healthkit.skills.list — Vec<{name, description}>
- huawei:hms.healthkit.skills.get  — {name, content_md}

Both reuse v1.2.0's embedded_skill_md helper (commit bc70c42); no new
data files. Total tools registered by default goes 26 → 28.

agent platforms can now pull the SKILL.md content via
`atd skills sync --target {hermes|claude-code}` from the atd-mvp CLI.

Closes #2.
EOF
)"
git tag v1.3.0
```

---

## Task 5: atd-mvp final validation + push + close issues

**Files:**
- (read-only validation, no further edits expected)

- [ ] **Step 1: cd back to atd-mvp**

```bash
cd ~/proj/atd-mvp
```

- [ ] **Step 2: Run all four gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace
```

All four must pass.

- [ ] **Step 3: Manual integration test (server + sync)**

```bash
# Build healthkit_cli@v1.3.0
cd ~/proj/healthkit_cli && cargo build --release && cd -

# (server should still be running from earlier session if not restart)
~/proj/healthkit_cli/scripts/atd-claude-setup.sh restart || true

# Stdout target — quick visual confirmation 26 skills round-trip
target/release/atd --sock /tmp/hk.sock skills sync --target stdout 2>&1 | head -30

# Hermes target — ephemeral out-dir
mkdir -p /tmp/sync-test && \
  target/release/atd --sock /tmp/hk.sock skills sync \
    --target hermes --out-dir /tmp/sync-test/hermes
ls /tmp/sync-test/hermes/ | head -10
diff /tmp/sync-test/hermes/huawei-hms-healthkit-heartrate/SKILL.md \
     ~/proj/healthkit_cli/skills/healthkit-heartrate/SKILL.md
# diff should be empty (content matches modulo dir prefix)

# Claude Code target
target/release/atd --sock /tmp/hk.sock skills sync \
  --target claude-code --out-dir /tmp/sync-test/claude
ls /tmp/sync-test/claude/ | head -5

# Cleanup
rm -rf /tmp/sync-test
```

If `healthkit auth login` is required again (token expired), document in the close comment that the manual sync was completed once auth refreshed.

- [ ] **Step 4: Tag SP**

```bash
cd ~/proj/atd-mvp && git tag sp-skills-discovery-convention
```

- [ ] **Step 5: Push both repos**

```bash
cd ~/proj/atd-mvp && git push origin master --tags
cd ~/proj/healthkit_cli && git push origin main --tags
```

- [ ] **Step 6: Close both issues**

```bash
cd ~/proj/atd-mvp && gh issue close 2 --comment "Fixed in $(git rev-parse --short HEAD) (tag sp-skills-discovery-convention).

**Spec:** docs/superpowers/specs/2026-04-27-sp-skills-discovery-convention-design.md
**Plan:** docs/superpowers/plans/2026-04-27-sp-skills-discovery-convention.md

**Landed:**
- Convention documented in docs/protocol/wire-format.md (skills meta-tool §)
- architecture.md §7.3 softened, §7.5 updated, §10 row added
- atd-cli: \`atd skills sync\` subcommand with hermes / claude-code / stdout targets
- Integration test against in-process stub server
- healthkit_cli v1.3.0 first adopter (separate repo, see linked PR)

Healthkit_cli#2 closed in lockstep."

cd ~/proj/healthkit_cli && gh issue close 2 --comment "Fixed in v1.3.0 (commit $(git rev-parse --short HEAD), tag v1.3.0).

Implements the atd-mvp SP-skills-discovery-convention. 26 SKILL.md files
now exposed via huawei:hms.healthkit.skills.list/get; total tools registered
by default goes 26 → 28. Verified end-to-end via \`atd skills sync\` from
the atd-mvp CLI."
```

---

## Task 6: Cross-check + retro

- [ ] **Step 1: Confirm spec exit gates**

Re-read `docs/superpowers/specs/2026-04-27-sp-skills-discovery-convention-design.md` §8 and tick each box. If the manual integration in Task 5 Step 3 hit an unexpected snag (e.g., expired auth, path-resolution issue), document in the close comment.

- [ ] **Step 2: Confirm the §10 architecture entry**

```bash
grep "Skills meta-tool convention" docs/architecture.md
```

Expected: matches the spec §10 wording.

- [ ] **Step 3: Confirm both issues are CLOSED with the right SHAs**

```bash
cd ~/proj/atd-mvp && gh issue view 2 --json state,closedAt
cd ~/proj/healthkit_cli && gh issue view 2 --json state,closedAt
```

---

## Self-Review

Spec coverage:
- §3 (atd-mvp) row 1 → Task 1 ✓
- §3 (atd-mvp) row 2 → Task 1 ✓
- §3 (atd-mvp) row 3 → Task 2 ✓
- §3 (atd-mvp) row 4 → Task 2 ✓
- §3 (atd-mvp) row 5 → Task 3 ✓
- §3 (healthkit_cli) all rows → Task 4 ✓
- §8 (Validation) → Task 5 ✓
- §10 (Arch edits) → Task 1 ✓

Type consistency:
- `SyncTarget::Hermes` / `ClaudeCode` / `Stdout` consistent across module + test
- `SkillsListTool` / `SkillsGetTool` consistent across module + server.rs registration + e2e
- Tool ids: `huawei:hms.healthkit.skills.list/get` — consistent everywhere

No placeholders.
