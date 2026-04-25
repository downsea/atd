# Phase 0 Weeks 2-3 — `atd` CLI Binary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the `atd` command-line binary — a human-facing REPL over the Phase 0 three APIs (`discover`/`describe`/`call`) plus a `doctor` connectivity check, per design.md §6.2.

**Architecture:** New binary+lib crate `atd-cli` in the workspace. Subcommands live in one module each (`list`, `schema`, `call`, `doctor`), each exposing `pub async fn run(client: &AtdClient, opts: ..., out: &mut impl Write) -> Result<(), AtdError>` so tests can drive them with a mock server and capture output to a buffer. `main.rs` is a thin clap-derive wrapper that parses args, connects via `AtdClient::connect(Endpoint::...)`, dispatches to the right module, and maps `AtdError` to a formatted stderr message + non-zero exit using `AtdError::suggest_fix()`.

**Tech Stack:** Rust 2024 · `clap 4` with `derive` feature (most idiomatic CLI parser in Rust) · `tokio` (single-thread runtime, matches atd-mcp-bridge) · existing `atd-client` + `atd-types`. No `colored` / no TUI — plain text output, relying on default terminal.

**Scope:**
- **In scope:** `atd list`, `atd schema`, `atd call`, `atd doctor`, global `--sock PATH` override, per-subcommand `--json` flag for structured output, end-to-end integration test against a mock server, README CLI section.
- **Out of scope (explicitly deferred):**
  - `atd allow` — the design doc (§6.2) mentions this for capability-token grants, but Phase 0/1 does not enforce tokens (§3.6). Adding a no-op `allow` now is YAGNI. Track in `docs/issues/` once we get there.
  - Shell completions (`atd completions bash|zsh|fish`) — Phase 1 polish.
  - Colored output / pretty tables — Phase 1 DX work.
  - Config file / persistent endpoint selection — `--sock` flag is sufficient for Phase 0.

**Prerequisites:**
- Phase 0.5 is complete (`phase0.5-hermes` tag, 68 tests passing).
- ANOS daemon is running locally at `~/.anos/anos.sock` for manual smoke-testing (not required for automated tests — they use a mock server).
- Workspace has `atd-types` (20 tests), `atd-client` (23 tests counting `call_failure_preserves_raw_payload_in_reason`), `atd-mcp-bridge` (22 tests), `examples`.

**Exit criteria:**
1. `cargo test --workspace` passes all tests (new integration test included).
2. `cargo build --release -p atd-cli --bin atd` produces the binary.
3. With live ANOS running:
   - `atd list --limit 5` prints 5 rows plus a "108 tools total" line.
   - `atd schema anos:fs.read` pretty-prints the full `ToolDefinition` JSON.
   - `atd doctor` prints socket path, ping status, tool count.
   - `atd call anos:fs.read --args '{"path":"/etc/hostname"}'` exits non-zero with a clear error (ANOS `run_tool` stub — this is the expected path until the ANOS-side issue is fixed).
4. Error paths print a human-readable message on stderr AND the `suggest_fix()` hint when present.
5. `--json` on each command emits parseable JSON on stdout.

---

## File Structure

```
atd-mvp/
├── Cargo.toml                              (MODIFY — add atd-cli to workspace members)
├── crates/
│   └── atd-cli/                            (NEW crate, binary + lib)
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs                     (clap parser, dispatch, error formatting)
│       │   ├── lib.rs                      (re-exports subcommand modules)
│       │   ├── cli.rs                      (clap derive structs)
│       │   ├── connect.rs                  (endpoint resolution + connect helper)
│       │   ├── list.rs                     (`atd list` subcommand)
│       │   ├── schema.rs                   (`atd schema` subcommand)
│       │   ├── call.rs                     (`atd call` subcommand)
│       │   └── doctor.rs                   (`atd doctor` subcommand)
│       └── tests/
│           └── integration.rs              (end-to-end: binary <-> mock socket)
├── docs/
│   └── cli.md                              (NEW — short CLI reference, linked from README)
└── README.md                               (MODIFY — add "CLI quickstart" section)
```

**Responsibility rationale:**
- One file per subcommand matches design.md §4's layout (`main.rs, list.rs, call.rs, schema.rs, doctor.rs`). Each file stays under ~150 lines.
- `cli.rs` holds every clap `#[derive(Parser)]` struct in one place so the shape of the CLI surface is visible at a glance. Dispatch in `main.rs` is then a trivial match.
- `connect.rs` centralizes endpoint construction (default path vs `--sock`) so subcommands don't reinvent it.
- `lib.rs` re-exports subcommand modules so integration tests can call them directly with a captured `Write` sink, in addition to spawning the binary.

---

## Task 1: Crate Scaffold + Workspace Registration

**Files:**
- Create: `crates/atd-cli/Cargo.toml`
- Create: `crates/atd-cli/src/main.rs` (placeholder)
- Create: `crates/atd-cli/src/lib.rs` (empty re-export stub)
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1.1: Write the crate `Cargo.toml`**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/Cargo.toml`:

```toml
[package]
name = "atd-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Reference command-line client for the Agent Tool Dispatch (ATD) protocol."

[lib]
name = "atd_cli"
path = "src/lib.rs"

[[bin]]
name = "atd"
path = "src/main.rs"

[dependencies]
atd-client = { path = "../atd-client", version = "0.1.0" }
atd-types = { path = "../atd-types", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
clap = { version = "4", features = ["derive"] }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 1.2: Write placeholder `main.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/src/main.rs`:

```rust
//! `atd` — reference command-line client for the ATD protocol.
//!
//! Real dispatch lands in Tasks 2-6. This placeholder exits with a clear message
//! so the crate compiles.

fn main() {
    eprintln!("atd: scaffold — subcommands land in Tasks 2-6");
    std::process::exit(1);
}
```

- [ ] **Step 1.3: Write empty `lib.rs`**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/src/lib.rs`:

```rust
//! `atd-cli` library side — exposes subcommand modules so integration tests can
//! drive them with captured output buffers. Tasks 2-6 fill in the modules.
```

- [ ] **Step 1.4: Register the crate in the workspace**

Edit `/home/nan/proj/atd-mvp/Cargo.toml` — change the `members` line:

From:
```toml
members = ["crates/atd-types", "crates/atd-client", "crates/atd-mcp-bridge", "examples"]
```
To:
```toml
members = ["crates/atd-types", "crates/atd-client", "crates/atd-cli", "crates/atd-mcp-bridge", "examples"]
```

- [ ] **Step 1.5: Build and smoke test the binary**

```bash
cd /home/nan/proj/atd-mvp
cargo build -p atd-cli --bin atd
./target/debug/atd
echo "exit=$?"
```

Expected:
- Build completes with no warnings.
- Binary prints `atd: scaffold — subcommands land in Tasks 2-6` to stderr and exits 1.
- `exit=1`.

- [ ] **Step 1.6: Full workspace regression**

```bash
cargo test --workspace --all-targets
```

Expected: 68 tests still pass (68 unchanged; atd-cli has no tests yet).

- [ ] **Step 1.7: Commit**

```bash
git add crates/atd-cli/ Cargo.toml Cargo.lock
git commit -m "feat(atd-cli): scaffold crate with bin + lib targets"
```

---

## Task 2: Shared `connect.rs` Helper

**Files:**
- Create: `crates/atd-cli/src/connect.rs`
- Modify: `crates/atd-cli/src/lib.rs`

Every subcommand needs "given an optional `--sock` path, return an `AtdClient` or error with a suggestion." Centralizing this avoids copy-paste across four modules and makes the error story consistent.

- [ ] **Step 2.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/src/connect.rs`:

```rust
//! Endpoint resolution + AtdClient connect helper.

use atd_client::{AtdClient, Endpoint};
use atd_types::AtdError;
use std::path::PathBuf;

/// Resolve the endpoint from an optional explicit `--sock PATH` override.
/// Falls back to `Endpoint::default_anos()` which reads `$HOME/.anos/anos.sock`.
pub fn resolve_endpoint(sock: Option<PathBuf>) -> Endpoint {
    match sock {
        Some(p) => Endpoint::unix(p),
        None => Endpoint::default_anos(),
    }
}

/// Connect to the configured ATD server. On failure returns an `AtdError`
/// suitable for the top-level error formatter; the caller owns printing.
pub async fn connect(sock: Option<PathBuf>) -> Result<AtdClient, AtdError> {
    let endpoint = resolve_endpoint(sock);
    AtdClient::connect(endpoint).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_endpoint_uses_override_when_provided() {
        let e = resolve_endpoint(Some(PathBuf::from("/tmp/custom.sock")));
        match e {
            Endpoint::UnixSocket(p) => assert_eq!(p, PathBuf::from("/tmp/custom.sock")),
        }
    }

    #[test]
    fn resolve_endpoint_falls_back_to_default_anos() {
        let e = resolve_endpoint(None);
        match e {
            Endpoint::UnixSocket(p) => assert!(
                p.to_string_lossy().ends_with(".anos/anos.sock"),
                "default should point at ~/.anos/anos.sock, got {p:?}"
            ),
        }
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-cli/src/lib.rs` to:

```rust
//! `atd-cli` library side — exposes subcommand modules so integration tests can
//! drive them with captured output buffers.

pub mod connect;
```

- [ ] **Step 2.2: Run the tests**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-cli --lib connect
```

Expected: `2 passed; 0 failed`.

- [ ] **Step 2.3: Commit**

```bash
git add crates/atd-cli/
git commit -m "feat(atd-cli): add endpoint resolution and connect helper"
```

---

## Task 3: `cli.rs` — Clap Derive Structs + Dispatch Skeleton

**Files:**
- Create: `crates/atd-cli/src/cli.rs`
- Modify: `crates/atd-cli/src/lib.rs`
- Modify: `crates/atd-cli/src/main.rs`

Defines every subcommand + arg shape. Main.rs routes to placeholder functions for now; Tasks 4-7 replace the placeholders with real logic.

- [ ] **Step 3.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/src/cli.rs`:

```rust
//! CLI surface: every clap-derive struct in one place.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "atd",
    version,
    about = "Reference client for the Agent Tool Dispatch (ATD) protocol."
)]
pub struct Cli {
    /// Override the Unix socket path. Default: $HOME/.anos/anos.sock
    #[arg(long, global = true)]
    pub sock: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List available tools (wraps the ATD `discover` API).
    List(ListArgs),
    /// Show a tool's full schema (wraps `describe`).
    Schema(SchemaArgs),
    /// Invoke a tool (wraps `call`).
    Call(CallArgs),
    /// Check connectivity to the ATD server.
    Doctor(DoctorArgs),
}

#[derive(Debug, clap::Args)]
pub struct ListArgs {
    /// Substring match against id/name/description.
    #[arg(short, long)]
    pub query: Option<String>,
    /// Filter by domain (e.g. "fs", "web").
    #[arg(short, long)]
    pub domain: Option<String>,
    /// Filter by tier.
    #[arg(long, value_parser = ["hot", "warm", "cold"])]
    pub tier: Option<String>,
    /// Filter by visibility.
    #[arg(long, value_parser = ["read", "write", "dangerous", "system"])]
    pub visibility: Option<String>,
    /// Cap the number of results.
    #[arg(short, long)]
    pub limit: Option<usize>,
    /// Emit structured JSON instead of human output.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct SchemaArgs {
    /// Tool id, e.g. "anos:fs.read".
    pub tool_id: String,
    /// Emit raw JSON instead of pretty-printed JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct CallArgs {
    /// Tool id, e.g. "anos:fs.read".
    pub tool_id: String,
    /// JSON arguments object, e.g. '{"path":"/tmp/x"}'. Defaults to `{}`.
    #[arg(long, default_value = "{}")]
    pub args: String,
    /// Run in dry-run mode (server may return a preview).
    #[arg(long)]
    pub dry_run: bool,
    /// Emit the result as raw JSON instead of pretty-printed.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct DoctorArgs {
    /// Emit structured JSON instead of human output.
    #[arg(long)]
    pub json: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_parses_list_with_flags() {
        let cli = Cli::try_parse_from([
            "atd", "list", "--query", "fs", "--limit", "5", "--json",
        ])
        .unwrap();
        match cli.command {
            Command::List(args) => {
                assert_eq!(args.query.as_deref(), Some("fs"));
                assert_eq!(args.limit, Some(5));
                assert!(args.json);
            }
            _ => panic!("expected List variant"),
        }
    }

    #[test]
    fn cli_parses_schema_with_positional_tool_id() {
        let cli = Cli::try_parse_from(["atd", "schema", "anos:fs.read"]).unwrap();
        match cli.command {
            Command::Schema(args) => assert_eq!(args.tool_id, "anos:fs.read"),
            _ => panic!("expected Schema variant"),
        }
    }

    #[test]
    fn cli_parses_call_with_args_and_dry_run() {
        let cli = Cli::try_parse_from([
            "atd", "call", "anos:fs.read",
            "--args", r#"{"path":"/tmp/x"}"#,
            "--dry-run",
        ])
        .unwrap();
        match cli.command {
            Command::Call(args) => {
                assert_eq!(args.tool_id, "anos:fs.read");
                assert_eq!(args.args, r#"{"path":"/tmp/x"}"#);
                assert!(args.dry_run);
            }
            _ => panic!("expected Call variant"),
        }
    }

    #[test]
    fn sock_flag_is_global_and_parses_before_subcommand() {
        let cli = Cli::try_parse_from([
            "atd", "--sock", "/tmp/x.sock", "list",
        ])
        .unwrap();
        assert_eq!(cli.sock.as_deref().map(|p| p.to_string_lossy().into_owned()),
                   Some("/tmp/x.sock".to_string()));
    }

    #[test]
    fn invalid_tier_value_is_rejected() {
        let err = Cli::try_parse_from(["atd", "list", "--tier", "lukewarm"]).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("lukewarm"), "error should mention bad value, got: {s}");
    }

    #[test]
    fn cli_is_wellformed() {
        // Fails at compile / factory time if the derive macros produce an
        // invalid command tree (e.g. overlapping short flags).
        Cli::command().debug_assert();
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-cli/src/lib.rs`:

```rust
//! `atd-cli` library side — exposes subcommand modules so integration tests can
//! drive them with captured output buffers.

pub mod cli;
pub mod connect;
```

Replace `/home/nan/proj/atd-mvp/crates/atd-cli/src/main.rs` with:

```rust
//! `atd` — reference command-line client for the ATD protocol.

use atd_cli::cli::{Cli, Command};
use clap::Parser;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::List(_) => {
            eprintln!("atd list: not yet implemented (Task 4)");
            std::process::ExitCode::from(2)
        }
        Command::Schema(_) => {
            eprintln!("atd schema: not yet implemented (Task 5)");
            std::process::ExitCode::from(2)
        }
        Command::Call(_) => {
            eprintln!("atd call: not yet implemented (Task 6)");
            std::process::ExitCode::from(2)
        }
        Command::Doctor(_) => {
            eprintln!("atd doctor: not yet implemented (Task 7)");
            std::process::ExitCode::from(2)
        }
    }
}
```

- [ ] **Step 3.2: Run the tests**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-cli --lib cli
```

Expected: `6 passed; 0 failed`.

- [ ] **Step 3.3: Smoke-test the binary**

```bash
cargo build -p atd-cli --bin atd
./target/debug/atd --help 2>&1 | head -20
./target/debug/atd list --help 2>&1 | head -20
```

Expected: help output shows top-level usage with `list`, `schema`, `call`, `doctor` subcommands and a `--sock` global option; `list --help` shows `--query`, `--domain`, `--tier`, `--visibility`, `--limit`, `--json`.

- [ ] **Step 3.4: Commit**

```bash
git add crates/atd-cli/ Cargo.lock
git commit -m "feat(atd-cli): define clap CLI surface with four subcommands"
```

---

## Task 4: `atd list` Subcommand

**Files:**
- Create: `crates/atd-cli/src/list.rs`
- Modify: `crates/atd-cli/src/lib.rs`
- Modify: `crates/atd-cli/src/main.rs`

- [ ] **Step 4.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/src/list.rs`:

```rust
//! `atd list` — discover tools and print them, filtered by flags.

use atd_client::{AtdClient, DiscoverFilter};
use atd_types::{AtdError, ToolTier, ToolVisibility};
use std::io::Write;

use crate::cli::ListArgs;

pub async fn run(
    client: &AtdClient,
    args: ListArgs,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let filter = DiscoverFilter {
        tier: args.tier.as_deref().and_then(parse_tier),
        visibility: args.visibility.as_deref().and_then(parse_visibility),
        domain: args.domain,
        limit: args.limit,
    };

    let summaries = client.discover(args.query.as_deref(), filter).await?;

    if args.json {
        serde_json::to_writer(&mut *out, &summaries)
            .map_err(|e| AtdError::ProtocolError {
                expected: "serializable ToolSummary list".into(),
                got: format!("serde error: {e}"),
            })?;
        writeln!(out).ok();
        return Ok(());
    }

    if summaries.is_empty() {
        writeln!(out, "no tools matched").ok();
        return Ok(());
    }

    writeln!(
        out,
        "{:<40} {:<24} {:<10} {:<6} {:<10}",
        "ID", "NAME", "DOMAIN", "TIER", "VIS"
    )
    .ok();
    for s in &summaries {
        writeln!(
            out,
            "{:<40} {:<24} {:<10} {:<6} {:<10}",
            truncate(&s.id, 40),
            truncate(&s.name, 24),
            truncate(&s.domain, 10),
            tier_str(s.tier),
            visibility_str(s.visibility)
        )
        .ok();
    }
    writeln!(out, "{} tool(s) total", summaries.len()).ok();
    Ok(())
}

fn parse_tier(s: &str) -> Option<ToolTier> {
    match s {
        "hot" => Some(ToolTier::Hot),
        "warm" => Some(ToolTier::Warm),
        "cold" => Some(ToolTier::Cold),
        _ => None,
    }
}

fn parse_visibility(s: &str) -> Option<ToolVisibility> {
    match s {
        "read" => Some(ToolVisibility::Read),
        "write" => Some(ToolVisibility::Write),
        "dangerous" => Some(ToolVisibility::Dangerous),
        "system" => Some(ToolVisibility::System),
        _ => None,
    }
}

fn tier_str(t: ToolTier) -> &'static str {
    match t {
        ToolTier::Hot => "hot",
        ToolTier::Warm => "warm",
        ToolTier::Cold => "cold",
    }
}

fn visibility_str(v: ToolVisibility) -> &'static str {
    match v {
        ToolVisibility::Read => "read",
        ToolVisibility::Write => "write",
        ToolVisibility::Dangerous => "dangerous",
        ToolVisibility::System => "system",
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_client::Endpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    async fn spawn_fake_server() -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::mem::forget(dir);

        let ret = path.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = stream.into_split();
                    loop {
                        let mut lb = [0u8; 4];
                        if r.read_exact(&mut lb).await.is_err() { return; }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() { return; }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let reply: serde_json::Value = match req["type"].as_str() {
                            Some("ping") => serde_json::json!({"type":"pong"}),
                            Some("tool_list") => serde_json::json!({
                                "type":"tool_list",
                                "tools":[
                                    {"id":"anos:fs.read","description":"Read a file","tier":"hot","visibility":"read"},
                                    {"id":"anos:fs.write","description":"Write a file","tier":"hot","visibility":"write"}
                                ]
                            }),
                            _ => serde_json::json!({"type":"error","message":"no"}),
                        };
                        let body = serde_json::to_vec(&reply).unwrap();
                        if w.write_all(&(body.len() as u32).to_be_bytes()).await.is_err() { return; }
                        if w.write_all(&body).await.is_err() { return; }
                        let _ = w.flush().await;
                    }
                });
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ret
    }

    #[tokio::test]
    async fn list_prints_table_with_totals() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            ListArgs { query: None, domain: None, tier: None, visibility: None, limit: None, json: false },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("ID") && s.contains("NAME") && s.contains("DOMAIN"));
        assert!(s.contains("anos:fs.read"));
        assert!(s.contains("anos:fs.write"));
        assert!(s.contains("2 tool(s) total"));
    }

    #[tokio::test]
    async fn list_json_flag_emits_array() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            ListArgs { query: None, domain: None, tier: None, visibility: None, limit: None, json: true },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert!(v.is_array());
        assert_eq!(v.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn list_limit_truncates_output() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            ListArgs { query: None, domain: None, tier: None, visibility: None, limit: Some(1), json: false },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("1 tool(s) total"));
        assert!(s.contains("anos:fs.read"));
        assert!(!s.contains("anos:fs.write"));
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-cli/src/lib.rs`:

```rust
//! `atd-cli` library side — exposes subcommand modules so integration tests can
//! drive them with captured output buffers.

pub mod cli;
pub mod connect;
pub mod list;
```

Update `/home/nan/proj/atd-mvp/crates/atd-cli/src/main.rs` — replace the `Command::List(_)` arm:

```rust
        Command::List(args) => {
            let client = match atd_cli::connect::connect(cli.sock).await {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            let mut out = std::io::stdout().lock();
            match atd_cli::list::run(&client, args, &mut out).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
```

And add this helper at module scope in `main.rs` (below `async fn main`):

```rust
fn fail(e: atd_types::AtdError) -> std::process::ExitCode {
    eprintln!("atd: {e}");
    if let Some(hint) = e.suggest_fix() {
        eprintln!("hint: {hint}");
    }
    std::process::ExitCode::from(1)
}
```

- [ ] **Step 4.2: Run the tests**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-cli --lib list
```

Expected: `3 passed; 0 failed`.

- [ ] **Step 4.3: Regression**

```bash
cargo test --workspace --all-targets
```

Expected: previous 68 + 2 connect + 6 cli + 3 list = 79 tests passing.

- [ ] **Step 4.4: Commit**

```bash
git add crates/atd-cli/
git commit -m "feat(atd-cli): implement list subcommand with table + JSON output"
```

---

## Task 5: `atd schema` Subcommand

**Files:**
- Create: `crates/atd-cli/src/schema.rs`
- Modify: `crates/atd-cli/src/lib.rs`
- Modify: `crates/atd-cli/src/main.rs`

- [ ] **Step 5.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/src/schema.rs`:

```rust
//! `atd schema` — describe a tool and pretty-print its ToolDefinition.

use atd_client::AtdClient;
use atd_types::AtdError;
use std::io::Write;

use crate::cli::SchemaArgs;

pub async fn run(
    client: &AtdClient,
    args: SchemaArgs,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let def = client.describe(&args.tool_id).await?;

    let json = if args.json {
        serde_json::to_string(&def)
    } else {
        serde_json::to_string_pretty(&def)
    }
    .map_err(|e| AtdError::ProtocolError {
        expected: "serializable ToolDefinition".into(),
        got: format!("serde error: {e}"),
    })?;
    writeln!(out, "{json}").ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_client::Endpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    fn sample_tool_def() -> serde_json::Value {
        serde_json::json!({
            "id": "anos:fs.read",
            "name": "Read File",
            "description": "Read a file from disk.",
            "version": "0.1.0",
            "capability": {"domain": "fs", "actions": ["read"], "tags": [], "intent_examples": []},
            "input_schema": {"type": "object"},
            "output_schema": {"type": "string"},
            "bindings": [{"protocol": "Cli", "config": {}}],
            "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
            "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
            "trust": {"publisher": "anos", "trust_level": "L2Tested", "signature": null},
            "visibility": "read"
        })
    }

    async fn spawn_fake_server() -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::mem::forget(dir);

        let ret = path.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = stream.into_split();
                    loop {
                        let mut lb = [0u8; 4];
                        if r.read_exact(&mut lb).await.is_err() { return; }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() { return; }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let reply: serde_json::Value = match req["type"].as_str() {
                            Some("ping") => serde_json::json!({"type":"pong"}),
                            Some("tool_schema") => serde_json::json!({
                                "type":"tool_schema","schema": sample_tool_def(),
                            }),
                            _ => serde_json::json!({"type":"error","message":"no"}),
                        };
                        let body = serde_json::to_vec(&reply).unwrap();
                        if w.write_all(&(body.len() as u32).to_be_bytes()).await.is_err() { return; }
                        if w.write_all(&body).await.is_err() { return; }
                        let _ = w.flush().await;
                    }
                });
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ret
    }

    #[tokio::test]
    async fn schema_pretty_by_default_has_newlines_and_indent() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            SchemaArgs { tool_id: "anos:fs.read".into(), json: false },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("\n"));
        assert!(s.contains("  \"id\""), "pretty output should have indented keys");
        assert!(s.contains("anos:fs.read"));
    }

    #[tokio::test]
    async fn schema_json_flag_emits_compact_single_line() {
        let sock = spawn_fake_server().await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            SchemaArgs { tool_id: "anos:fs.read".into(), json: true },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        // Exactly one trailing newline; no indentation newlines in the body.
        let trimmed = s.trim_end_matches('\n');
        assert!(!trimmed.contains('\n'), "json output should be one line, got: {s}");
        let v: serde_json::Value = serde_json::from_str(trimmed).unwrap();
        assert_eq!(v["id"], "anos:fs.read");
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-cli/src/lib.rs`:

```rust
pub mod cli;
pub mod connect;
pub mod list;
pub mod schema;
```

Replace the `Command::Schema(_)` arm in `main.rs`:

```rust
        Command::Schema(args) => {
            let client = match atd_cli::connect::connect(cli.sock).await {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            let mut out = std::io::stdout().lock();
            match atd_cli::schema::run(&client, args, &mut out).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
```

- [ ] **Step 5.2: Run the tests**

```bash
cargo test -p atd-cli --lib schema
```

Expected: `2 passed; 0 failed`.

- [ ] **Step 5.3: Commit**

```bash
git add crates/atd-cli/
git commit -m "feat(atd-cli): implement schema subcommand (pretty / compact JSON)"
```

---

## Task 6: `atd call` Subcommand

**Files:**
- Create: `crates/atd-cli/src/call.rs`
- Modify: `crates/atd-cli/src/lib.rs`
- Modify: `crates/atd-cli/src/main.rs`

- [ ] **Step 6.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/src/call.rs`:

```rust
//! `atd call` — invoke a tool with JSON args and print the result.

use atd_client::{AtdClient, CallOptions};
use atd_types::{AtdError, ToolResult};
use std::io::Write;

use crate::cli::CallArgs;

pub async fn run(
    client: &AtdClient,
    args: CallArgs,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let call_args: serde_json::Value =
        serde_json::from_str(&args.args).map_err(|e| AtdError::InvalidArguments {
            tool_id: args.tool_id.clone(),
            field: "--args".into(),
            reason: format!("not valid JSON: {e}"),
        })?;

    let result = client
        .call(
            &args.tool_id,
            call_args,
            CallOptions {
                dry_run: args.dry_run,
                preferred_binding: None,
            },
        )
        .await?;

    if args.json {
        let v = serde_json::to_string(&result)
            .map_err(|e| AtdError::ProtocolError {
                expected: "serializable ToolResult".into(),
                got: format!("serde error: {e}"),
            })?;
        writeln!(out, "{v}").ok();
        return Ok(());
    }

    match result {
        ToolResult::Success { data, .. } => {
            let pretty = serde_json::to_string_pretty(&data).unwrap_or_else(|_| "{}".into());
            writeln!(out, "ok:").ok();
            writeln!(out, "{pretty}").ok();
            Ok(())
        }
        ToolResult::Error { code, message, reason, retryable } => {
            Err(AtdError::ToolExecutionFailed {
                tool_id: args.tool_id.clone(),
                inner: Box::new(std::io::Error::other(format!(
                    "[{code}] {message}{}{}",
                    if retryable { " (retryable)" } else { "" },
                    reason.as_deref().map(|r| format!(" — raw: {r}")).unwrap_or_default()
                ))),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atd_client::Endpoint;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    async fn spawn_fake_server(
        handler: fn(serde_json::Value) -> serde_json::Value,
    ) -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::mem::forget(dir);

        let ret = path.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = stream.into_split();
                    loop {
                        let mut lb = [0u8; 4];
                        if r.read_exact(&mut lb).await.is_err() { return; }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() { return; }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let reply = match req["type"].as_str() {
                            Some("ping") => serde_json::json!({"type":"pong"}),
                            _ => handler(req),
                        };
                        let body = serde_json::to_vec(&reply).unwrap();
                        if w.write_all(&(body.len() as u32).to_be_bytes()).await.is_err() { return; }
                        if w.write_all(&body).await.is_err() { return; }
                        let _ = w.flush().await;
                    }
                });
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ret
    }

    #[tokio::test]
    async fn call_prints_ok_and_data_on_success() {
        let sock = spawn_fake_server(|req| match req["type"].as_str() {
            Some("run_tool") => serde_json::json!({
                "type":"tool_result",
                "tool_id": req["tool_id"],
                "result": {"content":"hello"},
                "success": true,
                "dry_run": false
            }),
            _ => serde_json::json!({"type":"error","message":"no"}),
        })
        .await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            CallArgs {
                tool_id: "anos:fs.read".into(),
                args: r#"{"path":"/tmp/x"}"#.into(),
                dry_run: false,
                json: false,
            },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("ok:\n"));
        assert!(s.contains("\"content\": \"hello\""));
    }

    #[tokio::test]
    async fn call_errors_on_invalid_json_args() {
        let sock = spawn_fake_server(|_| serde_json::json!({"type":"error","message":"no"})).await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        let err = run(
            &client,
            CallArgs {
                tool_id: "anos:fs.read".into(),
                args: "not json".into(),
                dry_run: false,
                json: false,
            },
            &mut out,
        )
        .await
        .unwrap_err();
        match err {
            AtdError::InvalidArguments { field, .. } => assert_eq!(field, "--args"),
            _ => panic!("expected InvalidArguments variant"),
        }
    }

    #[tokio::test]
    async fn call_json_flag_emits_full_tool_result_envelope() {
        let sock = spawn_fake_server(|req| match req["type"].as_str() {
            Some("run_tool") => serde_json::json!({
                "type":"tool_result",
                "tool_id": req["tool_id"],
                "result": {"k":"v"},
                "success": true,
                "dry_run": false
            }),
            _ => serde_json::json!({"type":"error","message":"no"}),
        })
        .await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        run(
            &client,
            CallArgs {
                tool_id: "anos:fs.read".into(),
                args: "{}".into(),
                dry_run: false,
                json: true,
            },
            &mut out,
        )
        .await
        .unwrap();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["status"], "success");
        assert_eq!(v["data"]["k"], "v");
    }

    #[tokio::test]
    async fn call_surfaces_server_reported_failure_as_error() {
        let sock = spawn_fake_server(|req| match req["type"].as_str() {
            Some("run_tool") => serde_json::json!({
                "type":"tool_result",
                "tool_id": req["tool_id"],
                "result": {"code":"EPERM","message":"denied","retryable":false},
                "success": false,
                "dry_run": false
            }),
            _ => serde_json::json!({"type":"error","message":"no"}),
        })
        .await;
        let client = AtdClient::connect(Endpoint::unix(sock)).await.unwrap();
        let mut out: Vec<u8> = Vec::new();
        let err = run(
            &client,
            CallArgs {
                tool_id: "anos:fs.read".into(),
                args: "{}".into(),
                dry_run: false,
                json: false,
            },
            &mut out,
        )
        .await
        .unwrap_err();
        let s = format!("{err:?}");
        assert!(s.contains("ToolExecutionFailed"), "got: {s}");
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-cli/src/lib.rs`:

```rust
pub mod call;
pub mod cli;
pub mod connect;
pub mod list;
pub mod schema;
```

Replace the `Command::Call(_)` arm in `main.rs`:

```rust
        Command::Call(args) => {
            let client = match atd_cli::connect::connect(cli.sock).await {
                Ok(c) => c,
                Err(e) => return fail(e),
            };
            let mut out = std::io::stdout().lock();
            match atd_cli::call::run(&client, args, &mut out).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
```

- [ ] **Step 6.2: Run the tests**

```bash
cargo test -p atd-cli --lib call
```

Expected: `4 passed; 0 failed`.

- [ ] **Step 6.3: Commit**

```bash
git add crates/atd-cli/
git commit -m "feat(atd-cli): implement call subcommand with dry-run + JSON output"
```

---

## Task 7: `atd doctor` Subcommand

**Files:**
- Create: `crates/atd-cli/src/doctor.rs`
- Modify: `crates/atd-cli/src/lib.rs`
- Modify: `crates/atd-cli/src/main.rs`

- [ ] **Step 7.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/src/doctor.rs`:

```rust
//! `atd doctor` — connectivity sanity check: socket exists, ping succeeds,
//! how many tools does `discover` return.

use atd_client::{AtdClient, DiscoverFilter};
use atd_types::AtdError;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;

use crate::cli::DoctorArgs;

#[derive(Serialize)]
pub struct DoctorReport {
    pub socket_path: String,
    pub socket_exists: bool,
    pub ping_ok: bool,
    pub tool_count: Option<usize>,
    pub error: Option<String>,
}

/// `sock` is the resolved endpoint path — we need it separately from the
/// connected client to report socket existence when connect fails.
pub async fn run(
    sock: PathBuf,
    args: DoctorArgs,
    out: &mut impl Write,
) -> Result<(), AtdError> {
    let socket_exists = sock.exists();
    let socket_path = sock.to_string_lossy().into_owned();

    let (ping_ok, tool_count, error) = match AtdClient::connect(atd_client::Endpoint::unix(&sock)).await {
        Ok(client) => match client.discover(None, DiscoverFilter::default()).await {
            Ok(v) => (true, Some(v.len()), None),
            Err(e) => (true, None, Some(format!("discover failed: {e}"))),
        },
        Err(e) => (false, None, Some(format!("connect failed: {e}"))),
    };

    let report = DoctorReport {
        socket_path,
        socket_exists,
        ping_ok,
        tool_count,
        error,
    };

    if args.json {
        serde_json::to_writer(&mut *out, &report)
            .map_err(|e| AtdError::ProtocolError {
                expected: "serializable DoctorReport".into(),
                got: format!("serde error: {e}"),
            })?;
        writeln!(out).ok();
    } else {
        writeln!(out, "socket path:   {}", report.socket_path).ok();
        writeln!(out, "socket exists: {}", report.socket_exists).ok();
        writeln!(out, "ping:          {}", if report.ping_ok { "ok" } else { "FAIL" }).ok();
        match report.tool_count {
            Some(n) => writeln!(out, "tool count:    {n}").ok(),
            None => writeln!(out, "tool count:    unavailable").ok(),
        };
        if let Some(e) = &report.error {
            writeln!(out, "error:         {e}").ok();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    async fn spawn_server_with_3_tools() -> std::path::PathBuf {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        std::mem::forget(dir);

        let ret = path.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (mut r, mut w) = stream.into_split();
                    loop {
                        let mut lb = [0u8; 4];
                        if r.read_exact(&mut lb).await.is_err() { return; }
                        let n = u32::from_be_bytes(lb) as usize;
                        let mut buf = vec![0u8; n];
                        if r.read_exact(&mut buf).await.is_err() { return; }
                        let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                        let reply = match req["type"].as_str() {
                            Some("ping") => serde_json::json!({"type":"pong"}),
                            Some("tool_list") => serde_json::json!({
                                "type":"tool_list",
                                "tools":[
                                    {"id":"anos:fs.read","description":"r","tier":"hot","visibility":"read"},
                                    {"id":"anos:fs.write","description":"w","tier":"hot","visibility":"write"},
                                    {"id":"anos:web.search","description":"s","tier":"hot","visibility":"read"}
                                ]
                            }),
                            _ => serde_json::json!({"type":"error","message":"no"}),
                        };
                        let body = serde_json::to_vec(&reply).unwrap();
                        if w.write_all(&(body.len() as u32).to_be_bytes()).await.is_err() { return; }
                        if w.write_all(&body).await.is_err() { return; }
                        let _ = w.flush().await;
                    }
                });
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ret
    }

    #[tokio::test]
    async fn doctor_reports_ok_against_reachable_server() {
        let sock = spawn_server_with_3_tools().await;
        let mut out: Vec<u8> = Vec::new();
        run(sock.clone(), DoctorArgs { json: false }, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("socket exists: true"));
        assert!(s.contains("ping:          ok"));
        assert!(s.contains("tool count:    3"));
    }

    #[tokio::test]
    async fn doctor_json_flag_emits_structured_report() {
        let sock = spawn_server_with_3_tools().await;
        let mut out: Vec<u8> = Vec::new();
        run(sock.clone(), DoctorArgs { json: true }, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();
        let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
        assert_eq!(v["socket_exists"], true);
        assert_eq!(v["ping_ok"], true);
        assert_eq!(v["tool_count"], 3);
        assert!(v["error"].is_null());
    }

    #[tokio::test]
    async fn doctor_reports_unreachable_when_socket_missing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist.sock");
        let mut out: Vec<u8> = Vec::new();
        run(missing, DoctorArgs { json: false }, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("socket exists: false"));
        assert!(s.contains("ping:          FAIL"));
        assert!(s.contains("error:"));
    }
}
```

Update `/home/nan/proj/atd-mvp/crates/atd-cli/src/lib.rs`:

```rust
pub mod call;
pub mod cli;
pub mod connect;
pub mod doctor;
pub mod list;
pub mod schema;
```

Replace the `Command::Doctor(_)` arm in `main.rs`:

```rust
        Command::Doctor(args) => {
            let endpoint = atd_cli::connect::resolve_endpoint(cli.sock);
            let sock = match &endpoint {
                atd_client::Endpoint::UnixSocket(p) => p.clone(),
            };
            let mut out = std::io::stdout().lock();
            match atd_cli::doctor::run(sock, args, &mut out).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
```

- [ ] **Step 7.2: Run the tests**

```bash
cargo test -p atd-cli --lib doctor
```

Expected: `3 passed; 0 failed`.

- [ ] **Step 7.3: Regression**

```bash
cargo test --workspace --all-targets
```

Expected: 68 prior + 2 connect + 6 cli + 3 list + 2 schema + 4 call + 3 doctor = 88 tests passing.

- [ ] **Step 7.4: Commit**

```bash
git add crates/atd-cli/
git commit -m "feat(atd-cli): implement doctor subcommand with socket/ping/count report"
```

---

## Task 8: End-to-End Integration Test (Binary Spawn)

**Files:**
- Create: `crates/atd-cli/tests/integration.rs`

Unit tests cover each subcommand's `run()` function. This integration test spawns the real compiled binary against a mock server to catch argv-parsing or wire-up regressions at the process boundary.

- [ ] **Step 8.1: Write the failing test**

Create `/home/nan/proj/atd-mvp/crates/atd-cli/tests/integration.rs`:

```rust
//! Integration test: spawn the `atd` binary against a mock Unix server and
//! assert on stdout / exit status.

use std::path::PathBuf;
use std::process::Command;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

/// Locate the compiled `atd` binary. Cargo sets `CARGO_BIN_EXE_atd` when
/// building integration tests for a crate with `[[bin]]`.
fn atd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atd"))
}

async fn spawn_3_tool_mock() -> PathBuf {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    std::mem::forget(dir);

    let ret = path.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut r, mut w) = stream.into_split();
                loop {
                    let mut lb = [0u8; 4];
                    if r.read_exact(&mut lb).await.is_err() { return; }
                    let n = u32::from_be_bytes(lb) as usize;
                    let mut buf = vec![0u8; n];
                    if r.read_exact(&mut buf).await.is_err() { return; }
                    let req: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                    let reply: serde_json::Value = match req["type"].as_str() {
                        Some("ping") => serde_json::json!({"type":"pong"}),
                        Some("tool_list") => serde_json::json!({
                            "type":"tool_list",
                            "tools":[
                                {"id":"anos:fs.read","description":"Read a file","tier":"hot","visibility":"read"},
                                {"id":"anos:fs.write","description":"Write a file","tier":"hot","visibility":"write"},
                                {"id":"anos:web.search","description":"Search the web","tier":"hot","visibility":"read"}
                            ]
                        }),
                        _ => serde_json::json!({"type":"error","message":"unexpected"}),
                    };
                    let body = serde_json::to_vec(&reply).unwrap();
                    if w.write_all(&(body.len() as u32).to_be_bytes()).await.is_err() { return; }
                    if w.write_all(&body).await.is_err() { return; }
                    let _ = w.flush().await;
                }
            });
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    ret
}

#[tokio::test]
async fn atd_list_against_mock_prints_table() {
    let sock = spawn_3_tool_mock().await;
    let output = Command::new(atd_bin())
        .args(["--sock", sock.to_str().unwrap(), "list"])
        .output()
        .expect("atd binary should run");
    assert!(output.status.success(), "non-zero exit, stderr={}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("anos:fs.read"));
    assert!(stdout.contains("anos:web.search"));
    assert!(stdout.contains("3 tool(s) total"));
}

#[tokio::test]
async fn atd_list_json_flag_produces_parseable_array() {
    let sock = spawn_3_tool_mock().await;
    let output = Command::new(atd_bin())
        .args(["--sock", sock.to_str().unwrap(), "list", "--json"])
        .output()
        .expect("atd binary should run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn atd_doctor_prints_3_tools_for_reachable_mock() {
    let sock = spawn_3_tool_mock().await;
    let output = Command::new(atd_bin())
        .args(["--sock", sock.to_str().unwrap(), "doctor"])
        .output()
        .expect("atd binary should run");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("tool count:    3"));
    assert!(stdout.contains("ping:          ok"));
}

#[tokio::test]
async fn atd_exits_nonzero_when_sock_missing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.sock");
    let output = Command::new(atd_bin())
        .args(["--sock", missing.to_str().unwrap(), "list"])
        .output()
        .expect("atd binary should run");
    assert!(!output.status.success(), "expected non-zero exit when socket missing");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("atd:"), "stderr should start with 'atd:' prefix, got: {stderr}");
}
```

- [ ] **Step 8.2: Run the tests**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-cli --test integration
```

Expected: `4 passed; 0 failed`.

- [ ] **Step 8.3: Commit**

```bash
git add crates/atd-cli/tests/
git commit -m "test(atd-cli): add end-to-end integration test spawning atd binary"
```

---

## Task 9: README + CLI Docs

**Files:**
- Create: `docs/cli.md`
- Modify: `README.md` (add "CLI quickstart" section after the Rust quickstart)

- [ ] **Step 9.1: Write `docs/cli.md`**

Create `/home/nan/proj/atd-mvp/docs/cli.md`:

````markdown
# `atd` — command-line reference

The `atd` binary is a thin convenience layer over the three Phase 0 ATD APIs (`discover`, `describe`, `call`) plus a `doctor` connectivity check. Install with:

```bash
cargo install --path crates/atd-cli --bin atd
```

Every command accepts `--sock PATH` to override the default endpoint (`$HOME/.anos/anos.sock`).

## `atd list` — discover tools

```
atd list [--query STR] [--domain STR] [--tier hot|warm|cold]
         [--visibility read|write|dangerous|system]
         [--limit N] [--json]
```

Default output is a table: `ID NAME DOMAIN TIER VIS` followed by a total count. With `--json`, emits a single JSON array of tool summaries.

Example:

```bash
$ atd list --query fs --limit 3
ID                                       NAME                     DOMAIN     TIER   VIS
anos:fs.read                             Read a file              fs         hot    read
anos:fs.write                            Write a file             fs         hot    write
anos:fs.list                             Directory List           fs         hot    read
3 tool(s) total
```

## `atd schema TOOL_ID` — inspect a tool

```
atd schema TOOL_ID [--json]
```

Without `--json`, pretty-prints the full `ToolDefinition` with 2-space indent. With `--json`, compact single-line output for piping into `jq`.

## `atd call TOOL_ID --args JSON` — invoke a tool

```
atd call TOOL_ID [--args JSON] [--dry-run] [--json]
```

`--args` takes a JSON object, defaulting to `{}`. `--dry-run` asks the server to describe what would happen without side effects.

On server-reported failure (`success:false`), `atd` exits non-zero and prints the error message on stderr.

**Known Phase 0 limitation:** the ANOS reference server's `run_tool` IPC is stubbed; expect `direct tool execution via IPC not yet supported` errors until that is wired up. See `docs/issues/2026-04-21-atd-run-tool-stub.md`.

## `atd doctor` — connectivity check

```
atd doctor [--json]
```

Reports:
- Resolved socket path
- Whether the socket file exists
- Whether `ping` succeeds
- How many tools `discover` returns

Useful for debugging setup issues — run it first when something feels wrong.
````

- [ ] **Step 9.2: Update `README.md`**

Find the section `## 15-minute quickstart (Rust, Phase 0)` in `/home/nan/proj/atd-mvp/README.md`. Immediately after that section (before the `## Development` section), insert:

````markdown
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
````

- [ ] **Step 9.3: Smoke-check**

```bash
grep -n "CLI quickstart" /home/nan/proj/atd-mvp/README.md
grep -n "# \`atd\`" /home/nan/proj/atd-mvp/docs/cli.md
```

Expected: both produce a non-empty line.

- [ ] **Step 9.4: Commit**

```bash
git add README.md docs/cli.md
git commit -m "docs: add CLI reference (docs/cli.md) and README quickstart section"
```

---

## Task 10: Live Smoke + Milestone Tag

**Files:** none (verification + tag)

- [ ] **Step 10.1: Live smoke against ANOS (skip if daemon not running)**

With ANOS daemon at `~/.anos/anos.sock`:

```bash
cd /home/nan/proj/atd-mvp
cargo build --release -p atd-cli --bin atd

./target/release/atd doctor
./target/release/atd list --limit 3
./target/release/atd schema anos:fs.read | head -30
./target/release/atd call anos:system.time --args '{}' ; echo "exit=$?"
```

Expected:
- `doctor`: prints socket path, `socket exists: true`, `ping: ok`, `tool count: 108` (or local count).
- `list --limit 3`: shows 3 tool rows and a total line.
- `schema anos:fs.read`: dumps pretty JSON starting with `"id": "anos:fs.read"`.
- `call`: exits non-zero with an error about `direct tool execution via IPC not yet supported` — this is the ANOS-side stub. Capture the exact message; it should come out on stderr with the `atd:` prefix and a `hint:` line from `suggest_fix()`.

If ANOS is not running, the `doctor` step will report `socket exists: false` and `ping: FAIL` — note this in the report and skip the remaining steps. Integration tests cover the logic regardless.

- [ ] **Step 10.2: Full test regression**

```bash
cargo test --workspace --all-targets
```

Expected count (starting from 68 before this plan):
- 68 prior
- +2 connect
- +6 cli
- +3 list
- +2 schema
- +4 call
- +3 doctor
- +4 integration

= 92 tests passing.

- [ ] **Step 10.3: ANOS-free tree check**

```bash
cargo tree --workspace --prefix none | grep -E '^\s*anos-' && echo FAIL || echo "OK: no anos- deps"
```

Expected: `OK: no anos- deps`.

- [ ] **Step 10.4: Tag the milestone**

```bash
git tag -a phase0-weeks2-3 -m "Phase 0 weeks 2-3: atd CLI with list/schema/call/doctor"
git log --oneline | head -15
```

Expected: tag exists, log shows the ~10 commits of this plan plus prior history.

---

## Post-Plan Verification Checklist

- [ ] `cargo test --workspace` passes (92 tests)
- [ ] `cargo build --release -p atd-cli --bin atd` clean
- [ ] `atd --help` shows 4 subcommands + `--sock` global flag
- [ ] `atd list --limit 3` against a live daemon prints 3 rows
- [ ] `atd schema anos:fs.read` pretty-prints a parseable JSON
- [ ] `atd doctor` prints a 4-line human report OR structured JSON with `--json`
- [ ] `atd call TOOL --args '{}'` surfaces server errors with exit 1
- [ ] Each `--json` flag produces valid parseable JSON (tested via `jq`)
- [ ] `docs/cli.md` has sections for all four subcommands
- [ ] README has a CLI quickstart section
- [ ] No new deps in `cargo tree` outside `clap` + its transitives

## What's Out of Scope (later plans)

- `atd allow TOOL_ID` — needs capability-token machinery (Phase 2 per design §3.6)
- `atd completions {bash,zsh,fish}` — Phase 1 polish
- Colored / pretty-table output — Phase 1 DX
- Config file (`~/.config/atd/config.toml`) — Phase 1+
- Phase 1 proper: Python SDK, TypeScript SDK, stdio transport, `atd-langchain`
