# atd-mvp Phase 0 Week 1 Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bootstrap the atd-mvp Rust workspace and ship a working `atd-client` that connects to the ANOS daemon over Unix socket and exercises `discover` + `describe` + `call` against it, with zero runtime dependency on any `anos-*` crate.

**Architecture:** Two-crate Cargo workspace. `atd-types` holds protocol-level structs reimplemented cleanly from `/home/nan/proj/anos/crates/anos-types/src/tool.rs` (no `anos-*` deps). `atd-client` owns the async Unix-socket transport, wire codec (length-prefixed JSON, byte-compatible with `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs`), and the three-method client API. A `tests/integration/mock_server.rs` harness proves protocol independence by round-tripping the client against a mock that has no ANOS dependency at all.

**Tech Stack:** Rust 2024 edition · tokio (async I/O, full features for UnixStream) · serde + serde_json (JSON codec) · chrono (timestamps) · ulid (request IDs) · thiserror (error enum derivation) · tempfile (test socket paths) · GitHub Actions for CI.

**Scope boundary (do not exceed):**
- **In scope:** workspace skeleton, `atd-types`, `atd-client` (Unix socket only), `discover`/`describe`/`call`, ANOS-free integration harness, `hello_atd.rs` example, CI workflow, README install story.
- **Out of scope (defer to a later plan):** `atd-cli` binary, stdio transport, MCP-compat transport, session/cancel APIs, Python/TypeScript SDKs, LangChain demo, `atd-mcp-bridge`, capability tokens, `as_anthropic_tools` adapters.

**Prerequisites:**
- ANOS daemon is installed at `/home/nan/proj/anos/` and can be started with `cargo run -p anos-daemon` (verify before Task 15).
- User has `cargo` (stable toolchain ≥1.80) and `git` on PATH.
- The working directory `/home/nan/proj/atd-mvp/` exists with `README.md`, `CLAUDE.md`, and `docs/` populated but no code and no `.git/` yet.

---

## File Structure

The plan produces this tree under `/home/nan/proj/atd-mvp/`:

```
atd-mvp/
├── .git/                                          (new, Task 1)
├── .gitignore                                     (new, Task 1)
├── Cargo.toml                                     (new, Task 1 — workspace manifest)
├── LICENSE                                        (new, Task 1 — Apache-2.0)
├── README.md                                      (exists; rewritten in Task 18)
├── CLAUDE.md                                      (exists; unchanged)
├── rust-toolchain.toml                            (new, Task 1 — pin stable)
├── .github/
│   └── workflows/
│       └── ci.yml                                 (new, Task 17)
├── crates/
│   ├── atd-types/
│   │   ├── Cargo.toml                             (new, Task 2)
│   │   └── src/
│   │       ├── lib.rs                             (new, Task 2 — re-exports)
│   │       ├── enums.rs                           (new, Task 3 — simple enums)
│   │       ├── tool.rs                            (new, Task 4 — ToolDefinition family)
│   │       ├── summary.rs                         (new, Task 5 — ToolSummary)
│   │       ├── result.rs                          (new, Task 6 — ToolResult + metadata)
│   │       └── error.rs                           (new, Task 7 — AtdError)
│   └── atd-client/
│       ├── Cargo.toml                             (new, Task 8)
│       └── src/
│           ├── lib.rs                             (new, Task 8 — re-exports)
│           ├── endpoint.rs                        (new, Task 8 — Endpoint enum)
│           ├── wire.rs                            (new, Task 9 — frame codec)
│           ├── protocol.rs                        (new, Task 10 — request/response enums)
│           ├── client.rs                          (new, Tasks 11–14 — AtdClient)
│           └── options.rs                         (new, Task 14 — CallOptions, DiscoverFilter)
├── tests/
│   └── integration/
│       └── mock_server.rs                         (new, Task 15 — ANOS-free proof)
├── examples/
│   └── hello_atd.rs                               (new, Task 16)
└── docs/
    └── superpowers/plans/
        └── 2026-04-21-phase0-week1-bootstrap.md   (this file)
```

**File responsibility rationale:**
- `atd-types` is split by concern (enums, tool definition, summary, result, error) so each file stays <200 lines and ports map one-to-one to ANOS source sections.
- `atd-client` separates transport concerns: `endpoint` (connection config), `wire` (bytes↔frames), `protocol` (frames↔typed messages), `client` (request/response orchestration). This way an upcoming stdio/MCP transport only touches `endpoint.rs` + a new sibling module, not `client.rs`.
- Integration test lives under `tests/integration/` per Cargo convention; having a separate mock server is essential to enforce the "ANOS-free" CI guarantee (Task 17 greps the tree to prove no `anos-*` crate is a runtime dep).

---

## Task 1: Workspace Bootstrap

**Files:**
- Create: `/home/nan/proj/atd-mvp/.gitignore`
- Create: `/home/nan/proj/atd-mvp/Cargo.toml`
- Create: `/home/nan/proj/atd-mvp/LICENSE`
- Create: `/home/nan/proj/atd-mvp/rust-toolchain.toml`

- [ ] **Step 1.1: Initialize git repo**

```bash
cd /home/nan/proj/atd-mvp
git init
git config user.email "qiaonancn@gmail.com"
git config user.name "Nan Qiao"
```

Expected: `Initialized empty Git repository in /home/nan/proj/atd-mvp/.git/`.

- [ ] **Step 1.2: Write `.gitignore`**

Create `/home/nan/proj/atd-mvp/.gitignore`:

```
/target
/**/*.rs.bk
Cargo.lock.bak
.DS_Store
.idea/
.vscode/
*.swp
```

(Note: `Cargo.lock` is committed — this is a workspace with binary targets.)

- [ ] **Step 1.3: Write workspace `Cargo.toml`**

Create `/home/nan/proj/atd-mvp/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/atd-types", "crates/atd-client"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "Apache-2.0"
repository = "https://github.com/atd-protocol/atd-mvp"
authors = ["ATD Protocol Contributors"]
rust-version = "1.85"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["net", "io-util", "rt-multi-thread", "macros", "sync", "time"] }
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
ulid = { version = "1", features = ["serde"] }
tempfile = "3"
```

- [ ] **Step 1.4: Write `rust-toolchain.toml`**

Create `/home/nan/proj/atd-mvp/rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 1.5: Write `LICENSE` (Apache-2.0)**

Create `/home/nan/proj/atd-mvp/LICENSE` with the full Apache-2.0 text. Fetch with:

```bash
curl -sSL https://www.apache.org/licenses/LICENSE-2.0.txt \
  -o /home/nan/proj/atd-mvp/LICENSE
wc -l /home/nan/proj/atd-mvp/LICENSE
```

Expected: exactly 201 lines.

- [ ] **Step 1.6: Verify workspace parses**

Run:

```bash
cd /home/nan/proj/atd-mvp
cargo metadata --no-deps --format-version 1 >/dev/null
```

Expected: command succeeds with no output and exit code 0. (No members exist yet, which is fine — cargo will warn but not error.)

- [ ] **Step 1.7: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add .gitignore Cargo.toml LICENSE rust-toolchain.toml README.md CLAUDE.md docs/
git commit -m "chore: initialize workspace, license, and planning docs"
```

---

## Task 2: `atd-types` Crate Skeleton

**Files:**
- Create: `crates/atd-types/Cargo.toml`
- Create: `crates/atd-types/src/lib.rs`

- [ ] **Step 2.1: Write the failing test (smoke test for crate existence)**

Create `crates/atd-types/src/lib.rs`:

```rust
//! ATD protocol types — independent reimplementation.
//!
//! This crate must have zero runtime dependency on any `anos-*` crate.
//! Type shapes are compatible with `/home/nan/proj/anos/crates/anos-types/src/tool.rs`
//! but all derives and trait bounds are redefined here.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        // Intentionally empty — proves the crate builds.
    }
}
```

- [ ] **Step 2.2: Write crate `Cargo.toml`**

Create `crates/atd-types/Cargo.toml`:

```toml
[package]
name = "atd-types"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Protocol-level types for the Agent Tool Dispatch (ATD) protocol."

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
ulid = { workspace = true }
thiserror = { workspace = true }
```

- [ ] **Step 2.3: Build and run the smoke test**

```bash
cd /home/nan/proj/atd-mvp
cargo test -p atd-types crate_compiles
```

Expected: `test tests::crate_compiles ... ok` with `1 passed; 0 failed`.

- [ ] **Step 2.4: Commit**

```bash
git add crates/atd-types/
git commit -m "feat(atd-types): initialize crate skeleton"
```

---

## Task 3: Port Simple Enums (ToolVisibility, ToolTier, BindingProtocol, SafetyLevel, TrustLevel)

**Files:**
- Create: `crates/atd-types/src/enums.rs`
- Modify: `crates/atd-types/src/lib.rs`

- [ ] **Step 3.1: Write the failing test**

Create `crates/atd-types/src/enums.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolVisibility {
    #[default]
    Read,
    Write,
    Dangerous,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTier {
    Hot,
    Warm,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum BindingProtocol {
    Cli,
    Mcp,
    AppFunction,
    Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SafetyLevel {
    Read = 0,
    Write = 1,
    Financial = 2,
    Privacy = 3,
    Physical = 4,
    Destructive = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TrustLevel {
    L0Unverified = 0,
    L1SchemaValid = 1,
    L2Tested = 2,
    L3Verified = 3,
    L4Certified = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ToolVisibility::Dangerous).unwrap(),
            "\"dangerous\""
        );
    }

    #[test]
    fn visibility_default_is_read() {
        assert_eq!(ToolVisibility::default(), ToolVisibility::Read);
    }

    #[test]
    fn tier_ordering_is_hot_warm_cold() {
        assert!(ToolTier::Hot < ToolTier::Warm);
        assert!(ToolTier::Warm < ToolTier::Cold);
    }

    #[test]
    fn binding_protocol_pascal_case() {
        assert_eq!(
            serde_json::to_string(&BindingProtocol::AppFunction).unwrap(),
            "\"AppFunction\""
        );
    }

    #[test]
    fn safety_level_ord() {
        assert!(SafetyLevel::Read < SafetyLevel::Destructive);
    }

    #[test]
    fn trust_level_ord() {
        assert!(TrustLevel::L0Unverified < TrustLevel::L4Certified);
    }
}
```

Update `crates/atd-types/src/lib.rs`:

```rust
//! ATD protocol types — independent reimplementation.
//!
//! This crate must have zero runtime dependency on any `anos-*` crate.

pub mod enums;

pub use enums::{BindingProtocol, SafetyLevel, ToolTier, ToolVisibility, TrustLevel};
```

- [ ] **Step 3.2: Run the tests to confirm they pass**

```bash
cargo test -p atd-types --lib enums
```

Expected: `6 passed; 0 failed`. (Tests are inline with the data — no implementation gap to fail first; these are pure data-type assertions.)

- [ ] **Step 3.3: Commit**

```bash
git add crates/atd-types/
git commit -m "feat(atd-types): add visibility, tier, binding, safety, trust enums"
```

---

## Task 4: Port ToolDefinition Family

**Files:**
- Create: `crates/atd-types/src/tool.rs`
- Modify: `crates/atd-types/src/lib.rs`

- [ ] **Step 4.1: Write the failing test**

Create `crates/atd-types/src/tool.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::enums::{BindingProtocol, SafetyLevel, ToolVisibility, TrustLevel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,

    pub capability: ToolCapability,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,

    pub bindings: Vec<ToolBinding>,
    pub safety: ToolSafety,
    pub resources: ToolResources,
    pub trust: ToolTrust,

    #[serde(default)]
    pub visibility: ToolVisibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCapability {
    pub domain: String,
    pub actions: Vec<String>,
    pub tags: Vec<String>,
    pub intent_examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolBinding {
    pub protocol: BindingProtocol,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSafety {
    pub level: SafetyLevel,
    pub dry_run: bool,
    pub side_effects: Vec<String>,
    pub data_sensitivity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResources {
    pub timeout_ms: u64,
    pub max_concurrent: u32,
    pub rate_limit_per_min: Option<u32>,
    pub estimated_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTrust {
    pub publisher: String,
    pub trust_level: TrustLevel,
    pub signature: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ToolDefinition {
        ToolDefinition {
            id: "anos:fs.read".into(),
            name: "Read File".into(),
            description: "Read a file from disk.".into(),
            version: "0.1.0".into(),
            capability: ToolCapability {
                domain: "fs".into(),
                actions: vec!["read".into()],
                tags: vec!["filesystem".into()],
                intent_examples: vec!["read config.toml".into()],
            },
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
            output_schema: serde_json::json!({"type": "string"}),
            bindings: vec![ToolBinding {
                protocol: BindingProtocol::Cli,
                config: serde_json::json!({"cmd": "cat"}),
            }],
            safety: ToolSafety {
                level: SafetyLevel::Read,
                dry_run: false,
                side_effects: vec![],
                data_sensitivity: None,
            },
            resources: ToolResources {
                timeout_ms: 5_000,
                max_concurrent: 8,
                rate_limit_per_min: None,
                estimated_tokens: Some(100),
            },
            trust: ToolTrust {
                publisher: "anos".into(),
                trust_level: TrustLevel::L3Verified,
                signature: None,
            },
            visibility: ToolVisibility::Read,
        }
    }

    #[test]
    fn tool_definition_roundtrip() {
        let t = sample();
        let json = serde_json::to_string(&t).unwrap();
        let back: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, t.id);
        assert_eq!(back.capability.domain, "fs");
        assert_eq!(back.safety.level, SafetyLevel::Read);
    }

    #[test]
    fn visibility_defaults_when_missing_in_json() {
        let mut v = serde_json::to_value(sample()).unwrap();
        v.as_object_mut().unwrap().remove("visibility");
        let back: ToolDefinition = serde_json::from_value(v).unwrap();
        assert_eq!(back.visibility, ToolVisibility::Read);
    }
}
```

Update `crates/atd-types/src/lib.rs` — append:

```rust
pub mod tool;

pub use tool::{
    ToolBinding, ToolCapability, ToolDefinition, ToolResources, ToolSafety, ToolTrust,
};
```

- [ ] **Step 4.2: Run the tests**

```bash
cargo test -p atd-types --lib tool
```

Expected: `2 passed; 0 failed`.

- [ ] **Step 4.3: Commit**

```bash
git add crates/atd-types/
git commit -m "feat(atd-types): port ToolDefinition family"
```

---

## Task 5: Define `ToolSummary`

**Files:**
- Create: `crates/atd-types/src/summary.rs`
- Modify: `crates/atd-types/src/lib.rs`

**Why this is new:** ANOS has `ToolDefinition` only. `discover()` in ATD returns summaries (not full schemas) so large registries stay cheap. `ToolSummary` is derived from `ToolDefinition` but drops schemas, bindings, and signatures.

- [ ] **Step 5.1: Write the failing test**

Create `crates/atd-types/src/summary.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::enums::{ToolTier, ToolVisibility};
use crate::tool::ToolDefinition;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub domain: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub visibility: ToolVisibility,
    #[serde(default = "default_tier")]
    pub tier: ToolTier,
}

fn default_tier() -> ToolTier {
    ToolTier::Warm
}

impl From<&ToolDefinition> for ToolSummary {
    fn from(def: &ToolDefinition) -> Self {
        Self {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            domain: def.capability.domain.clone(),
            tags: def.capability.tags.clone(),
            visibility: def.visibility,
            tier: default_tier(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enums::{BindingProtocol, SafetyLevel, TrustLevel};
    use crate::tool::{ToolCapability, ToolResources, ToolSafety, ToolTrust};

    fn def() -> ToolDefinition {
        ToolDefinition {
            id: "anos:fs.read".into(),
            name: "Read File".into(),
            description: "desc".into(),
            version: "0.1.0".into(),
            capability: ToolCapability {
                domain: "fs".into(),
                actions: vec!["read".into()],
                tags: vec!["filesystem".into()],
                intent_examples: vec![],
            },
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            bindings: vec![crate::tool::ToolBinding {
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
                publisher: "anos".into(),
                trust_level: TrustLevel::L2Tested,
                signature: None,
            },
            visibility: ToolVisibility::Read,
        }
    }

    #[test]
    fn summary_is_derivable_from_definition() {
        let s = ToolSummary::from(&def());
        assert_eq!(s.id, "anos:fs.read");
        assert_eq!(s.domain, "fs");
        assert_eq!(s.tags, vec!["filesystem"]);
        assert_eq!(s.tier, ToolTier::Warm);
    }

    #[test]
    fn summary_roundtrip_json() {
        let s = ToolSummary::from(&def());
        let j = serde_json::to_string(&s).unwrap();
        let back: ToolSummary = serde_json::from_str(&j).unwrap();
        assert_eq!(back.id, s.id);
    }

    #[test]
    fn missing_tier_defaults_to_warm() {
        let j = r#"{"id":"a","name":"A","description":"d","domain":"x","tags":[]}"#;
        let s: ToolSummary = serde_json::from_str(j).unwrap();
        assert_eq!(s.tier, ToolTier::Warm);
        assert_eq!(s.visibility, ToolVisibility::Read);
    }
}
```

Update `lib.rs` — append:

```rust
pub mod summary;

pub use summary::ToolSummary;
```

- [ ] **Step 5.2: Run the tests**

```bash
cargo test -p atd-types --lib summary
```

Expected: `3 passed; 0 failed`.

- [ ] **Step 5.3: Commit**

```bash
git add crates/atd-types/
git commit -m "feat(atd-types): add ToolSummary for discover responses"
```

---

## Task 6: Port ToolResult + Metadata

**Files:**
- Create: `crates/atd-types/src/result.rs`
- Modify: `crates/atd-types/src/lib.rs`

- [ ] **Step 6.1: Write the failing test**

Create `crates/atd-types/src/result.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::enums::BindingProtocol;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolResult {
    Success {
        data: serde_json::Value,
        metadata: ToolResultMetadata,
    },
    Error {
        code: String,
        message: String,
        reason: Option<String>,
        retryable: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMetadata {
    pub tool_id: String,
    pub version: String,
    pub binding: BindingProtocol,
    pub latency_ms: u64,
    pub timestamp: DateTime<Utc>,
    pub request_id: ulid::Ulid,
}

impl ToolResult {
    pub fn is_success(&self) -> bool {
        matches!(self, ToolResult::Success { .. })
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, ToolResult::Error { retryable: true, .. })
    }

    pub fn data(&self) -> Option<&serde_json::Value> {
        match self {
            ToolResult::Success { data, .. } => Some(data),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn success() -> ToolResult {
        ToolResult::Success {
            data: serde_json::json!({"content": "hello"}),
            metadata: ToolResultMetadata {
                tool_id: "anos:fs.read".into(),
                version: "0.1.0".into(),
                binding: BindingProtocol::Cli,
                latency_ms: 3,
                timestamp: Utc::now(),
                request_id: ulid::Ulid::new(),
            },
        }
    }

    #[test]
    fn success_roundtrip() {
        let r = success();
        let j = serde_json::to_string(&r).unwrap();
        let back: ToolResult = serde_json::from_str(&j).unwrap();
        assert!(back.is_success());
        assert_eq!(back.data().unwrap()["content"], "hello");
    }

    #[test]
    fn error_retryable() {
        let r = ToolResult::Error {
            code: "TIMEOUT".into(),
            message: "timed out".into(),
            reason: None,
            retryable: true,
        };
        assert!(!r.is_success());
        assert!(r.is_retryable());
    }

    #[test]
    fn status_tag_uses_snake_case() {
        let j = serde_json::to_string(&success()).unwrap();
        assert!(j.contains("\"status\":\"success\""), "got: {j}");
    }
}
```

Update `lib.rs` — append:

```rust
pub mod result;

pub use result::{ToolResult, ToolResultMetadata};
```

- [ ] **Step 6.2: Run the tests**

```bash
cargo test -p atd-types --lib result
```

Expected: `3 passed; 0 failed`.

- [ ] **Step 6.3: Commit**

```bash
git add crates/atd-types/
git commit -m "feat(atd-types): add ToolResult with success/error variants"
```

---

## Task 7: Define `AtdError`

**Files:**
- Create: `crates/atd-types/src/error.rs`
- Modify: `crates/atd-types/src/lib.rs`

- [ ] **Step 7.1: Write the failing test**

Create `crates/atd-types/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtdError {
    #[error("tool not found: {tool_id}")]
    ToolNotFound {
        tool_id: String,
        suggestions: Vec<String>,
    },

    #[error("invalid arguments for {tool_id}: field `{field}` — {reason}")]
    InvalidArguments {
        tool_id: String,
        field: String,
        reason: String,
    },

    #[error("capability denied for {tool_id}: required={required:?} granted={granted:?}")]
    CapabilityDenied {
        tool_id: String,
        required: Vec<String>,
        granted: Vec<String>,
    },

    #[error("no binding available for {tool_id}: tried={tried:?} ({reason})")]
    BindingUnavailable {
        tool_id: String,
        tried: Vec<String>,
        reason: String,
    },

    #[error("tool execution failed: {tool_id}")]
    ToolExecutionFailed {
        tool_id: String,
        #[source]
        inner: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("timed out calling {tool_id} after {after_ms}ms")]
    Timeout { tool_id: String, after_ms: u64 },

    #[error("server unreachable: {0}")]
    ServerUnreachable(#[from] std::io::Error),

    #[error("not implemented: {feature}")]
    NotImplemented { feature: String },

    #[error("protocol error: expected {expected}, got {got}")]
    ProtocolError { expected: String, got: String },
}

impl AtdError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AtdError::Timeout { .. }
                | AtdError::ServerUnreachable(_)
                | AtdError::BindingUnavailable { .. }
        )
    }

    pub fn suggest_fix(&self) -> Option<String> {
        match self {
            AtdError::ToolNotFound { suggestions, .. } if !suggestions.is_empty() => {
                Some(format!("did you mean '{}'?", suggestions[0]))
            }
            AtdError::ToolNotFound { .. } => {
                Some("try `atd list --query <keyword>` to find available tools".into())
            }
            AtdError::CapabilityDenied { tool_id, .. } => Some(format!(
                "run `atd allow {tool_id}` to grant for this session"
            )),
            AtdError::ServerUnreachable(_) => {
                Some("is the ANOS daemon running? try `anos daemon status`".into())
            }
            AtdError::Timeout { tool_id, .. } => {
                Some(format!("increase timeout or retry; tool_id={tool_id}"))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_not_found_suggests_candidate() {
        let e = AtdError::ToolNotFound {
            tool_id: "fs.red".into(),
            suggestions: vec!["fs.read".into()],
        };
        assert_eq!(e.suggest_fix().unwrap(), "did you mean 'fs.read'?");
        assert!(!e.is_retryable());
    }

    #[test]
    fn tool_not_found_without_suggestions_hints_discovery() {
        let e = AtdError::ToolNotFound {
            tool_id: "xx".into(),
            suggestions: vec![],
        };
        assert!(e.suggest_fix().unwrap().contains("atd list"));
    }

    #[test]
    fn timeout_is_retryable() {
        let e = AtdError::Timeout {
            tool_id: "fs.read".into(),
            after_ms: 5000,
        };
        assert!(e.is_retryable());
    }

    #[test]
    fn io_error_converts_to_server_unreachable() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "no");
        let e: AtdError = io_err.into();
        assert!(matches!(e, AtdError::ServerUnreachable(_)));
        assert!(e.is_retryable());
    }

    #[test]
    fn display_includes_tool_id() {
        let e = AtdError::InvalidArguments {
            tool_id: "fs.read".into(),
            field: "path".into(),
            reason: "must be string".into(),
        };
        let s = format!("{e}");
        assert!(s.contains("fs.read"));
        assert!(s.contains("path"));
    }
}
```

Update `lib.rs` — append:

```rust
pub mod error;

pub use error::AtdError;
```

- [ ] **Step 7.2: Run the tests**

```bash
cargo test -p atd-types --lib error
```

Expected: `5 passed; 0 failed`.

- [ ] **Step 7.3: Full crate sanity check**

```bash
cargo test -p atd-types
cargo clippy -p atd-types -- -D warnings
```

Expected: all tests pass, clippy clean.

- [ ] **Step 7.4: Commit**

```bash
git add crates/atd-types/
git commit -m "feat(atd-types): add AtdError with suggest_fix helpers"
```

---

## Task 8: `atd-client` Crate Skeleton + Endpoint

**Files:**
- Create: `crates/atd-client/Cargo.toml`
- Create: `crates/atd-client/src/lib.rs`
- Create: `crates/atd-client/src/endpoint.rs`

- [ ] **Step 8.1: Write the failing test**

Create `crates/atd-client/src/endpoint.rs`:

```rust
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Endpoint {
    UnixSocket(PathBuf),
}

impl Endpoint {
    pub fn unix(path: impl Into<PathBuf>) -> Self {
        Endpoint::UnixSocket(path.into())
    }

    /// Default ANOS daemon socket: `$HOME/.anos/anos.sock`.
    pub fn default_anos() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Endpoint::UnixSocket(PathBuf::from(home).join(".anos").join("anos.sock"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_constructor_accepts_str_and_pathbuf() {
        let e1 = Endpoint::unix("/tmp/a.sock");
        let e2 = Endpoint::unix(PathBuf::from("/tmp/a.sock"));
        match (e1, e2) {
            (Endpoint::UnixSocket(a), Endpoint::UnixSocket(b)) => assert_eq!(a, b),
        }
    }

    #[test]
    fn default_anos_uses_dot_anos_anos_sock() {
        let e = Endpoint::default_anos();
        match e {
            Endpoint::UnixSocket(p) => {
                let s = p.to_string_lossy().to_string();
                assert!(s.ends_with(".anos/anos.sock"), "got: {s}");
            }
        }
    }
}
```

Create `crates/atd-client/src/lib.rs`:

```rust
//! ATD reference client SDK (Rust).
//!
//! Zero runtime dependency on any `anos-*` crate. Protocol-level types
//! live in the `atd-types` sibling crate.

pub mod endpoint;

pub use endpoint::Endpoint;
```

- [ ] **Step 8.2: Write `Cargo.toml`**

Create `crates/atd-client/Cargo.toml`:

```toml
[package]
name = "atd-client"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Reference Rust client SDK for the Agent Tool Dispatch (ATD) protocol."

[dependencies]
atd-types = { path = "../atd-types", version = "0.1.0" }
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
thiserror = { workspace = true }
ulid = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
tokio = { workspace = true, features = ["test-util"] }
```

- [ ] **Step 8.3: Run the tests**

```bash
cargo test -p atd-client --lib endpoint
```

Expected: `2 passed; 0 failed`.

- [ ] **Step 8.4: Commit**

```bash
git add crates/atd-client/
git commit -m "feat(atd-client): add crate skeleton and Endpoint type"
```

---

## Task 9: Wire Codec (Length-Prefixed JSON Frames)

**Files:**
- Create: `crates/atd-client/src/wire.rs`
- Modify: `crates/atd-client/src/lib.rs`

**Note:** Frame format is byte-compatible with `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs` — 4-byte big-endian `u32` length prefix, JSON body, 10 MiB max. Reimplemented here so atd-client does not depend on any `anos-*` crate.

- [ ] **Step 9.1: Write the failing test**

Create `crates/atd-client/src/wire.rs`:

```rust
use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAX_FRAME_BYTES: usize = 10 * 1024 * 1024;

pub async fn write_frame<W, T>(writer: &mut W, msg: &T) -> std::io::Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: Serialize,
{
    let body = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(body.len()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame too large: {} bytes", body.len()),
        )
    })?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_frame<R, T>(reader: &mut R) -> std::io::Result<T>
where
    R: AsyncReadExt + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame too large: {len} bytes"),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    serde_json::from_slice(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct M {
        kind: String,
        n: u32,
    }

    #[tokio::test]
    async fn write_then_read_roundtrip() {
        let msg = M {
            kind: "ping".into(),
            n: 7,
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();

        let mut cursor = std::io::Cursor::new(&buf);
        let back: M = read_frame(&mut cursor).await.unwrap();
        assert_eq!(back, msg);
    }

    #[tokio::test]
    async fn frame_uses_big_endian_u32_prefix() {
        let msg = M {
            kind: "x".into(),
            n: 1,
        };
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &msg).await.unwrap();
        let body_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        assert_eq!(body_len, buf.len() - 4);
    }

    #[tokio::test]
    async fn oversized_frame_errors() {
        // Craft a header claiming 20 MiB; reader should refuse before allocating.
        let mut header = Vec::new();
        let bogus_len: u32 = 20 * 1024 * 1024;
        header.extend_from_slice(&bogus_len.to_be_bytes());
        let mut cursor = std::io::Cursor::new(header);
        let err = read_frame::<_, M>(&mut cursor).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
```

Update `lib.rs`:

```rust
pub mod endpoint;
pub mod wire;

pub use endpoint::Endpoint;
```

- [ ] **Step 9.2: Run the tests**

```bash
cargo test -p atd-client --lib wire
```

Expected: `3 passed; 0 failed`.

- [ ] **Step 9.3: Commit**

```bash
git add crates/atd-client/
git commit -m "feat(atd-client): add length-prefixed JSON wire codec"
```

---

## Task 10: Protocol Message Enums

**Files:**
- Create: `crates/atd-client/src/protocol.rs`
- Modify: `crates/atd-client/src/lib.rs`

**Wire-tag compatibility:** The tag names below match ANOS `ClientMessage`/`DaemonMessage` in `/home/nan/proj/anos/crates/anos-runtime/src/ipc.rs` so the ANOS daemon can serve as the reference server with zero changes. The atd-client type names (`Request`/`Response`) are ours — only the `#[serde(rename = ...)]` tags interop.

- [ ] **Step 10.1: Write the failing test**

Create `crates/atd-client/src/protocol.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Request frames sent from client → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "hello")]
    Hello { version: String },

    #[serde(rename = "tool_list")]
    ToolList,

    #[serde(rename = "tool_schema")]
    ToolSchema { tool_id: String },

    #[serde(rename = "run_tool")]
    RunTool {
        tool_id: String,
        args: serde_json::Value,
        dry_run: bool,
    },
}

/// Response frames sent from server → client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    #[serde(rename = "pong")]
    Pong,

    #[serde(rename = "hello")]
    HelloResponse {
        version: String,
        capabilities: Vec<String>,
    },

    #[serde(rename = "tool_list")]
    ToolListResponse { tools: serde_json::Value },

    #[serde(rename = "tool_schema")]
    ToolSchemaResponse { schema: serde_json::Value },

    #[serde(rename = "tool_result")]
    ToolResultResponse {
        tool_id: String,
        result: serde_json::Value,
        success: bool,
        dry_run: bool,
    },

    #[serde(rename = "error")]
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retryable: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_serializes_with_type_tag() {
        let j = serde_json::to_string(&Request::Ping).unwrap();
        assert_eq!(j, r#"{"type":"ping"}"#);
    }

    #[test]
    fn run_tool_roundtrip() {
        let r = Request::RunTool {
            tool_id: "anos:fs.read".into(),
            args: serde_json::json!({"path": "/tmp/x"}),
            dry_run: false,
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: Request = serde_json::from_str(&j).unwrap();
        match back {
            Request::RunTool { tool_id, dry_run, .. } => {
                assert_eq!(tool_id, "anos:fs.read");
                assert!(!dry_run);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn tool_list_response_carries_array() {
        let r = Response::ToolListResponse {
            tools: serde_json::json!([{"id": "a"}, {"id": "b"}]),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"type\":\"tool_list\""));
        let back: Response = serde_json::from_str(&j).unwrap();
        match back {
            Response::ToolListResponse { tools } => {
                assert_eq!(tools.as_array().unwrap().len(), 2);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn error_deserializes_with_optional_fields_missing() {
        let j = r#"{"type":"error","message":"boom"}"#;
        let back: Response = serde_json::from_str(j).unwrap();
        match back {
            Response::Error { message, code, retryable, details } => {
                assert_eq!(message, "boom");
                assert!(code.is_none());
                assert!(retryable.is_none());
                assert!(details.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }
}
```

Update `lib.rs`:

```rust
pub mod endpoint;
pub mod protocol;
pub mod wire;

pub use endpoint::Endpoint;
```

- [ ] **Step 10.2: Run the tests**

```bash
cargo test -p atd-client --lib protocol
```

Expected: `4 passed; 0 failed`.

- [ ] **Step 10.3: Commit**

```bash
git add crates/atd-client/
git commit -m "feat(atd-client): define Request/Response wire message enums"
```

---

## Task 11: `AtdClient::connect` with Ping Handshake

**Files:**
- Create: `crates/atd-client/src/client.rs`
- Modify: `crates/atd-client/src/lib.rs`

- [ ] **Step 11.1: Write the failing test (uses an in-process mock over `tokio::io::duplex`)**

Create `crates/atd-client/src/client.rs`:

```rust
use atd_types::AtdError;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::endpoint::Endpoint;
use crate::protocol::{Request, Response};
use crate::wire::{read_frame, write_frame};

/// Async ATD client.
///
/// Each request/response pair is serialized under an internal mutex so the
/// client is safe to clone across tasks by wrapping in `Arc<AtdClient>`.
pub struct AtdClient {
    inner: Mutex<Pipe>,
}

enum Pipe {
    Unix {
        read: tokio::net::unix::OwnedReadHalf,
        write: tokio::net::unix::OwnedWriteHalf,
    },
    /// Used only by in-crate tests.
    #[cfg(test)]
    Duplex {
        read: Box<dyn AsyncRead + Send + Unpin>,
        write: Box<dyn AsyncWrite + Send + Unpin>,
    },
}

impl AtdClient {
    pub async fn connect(endpoint: Endpoint) -> Result<Self, AtdError> {
        match endpoint {
            Endpoint::UnixSocket(path) => {
                let stream = UnixStream::connect(&path).await?;
                let (read, write) = stream.into_split();
                let client = AtdClient {
                    inner: Mutex::new(Pipe::Unix { read, write }),
                };
                client.ping().await?;
                Ok(client)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn from_duplex<R, W>(read: R, write: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        AtdClient {
            inner: Mutex::new(Pipe::Duplex {
                read: Box::new(read),
                write: Box::new(write),
            }),
        }
    }

    pub async fn ping(&self) -> Result<(), AtdError> {
        match self.request(&Request::Ping).await? {
            Response::Pong => Ok(()),
            other => Err(AtdError::ProtocolError {
                expected: "pong".into(),
                got: format!("{other:?}"),
            }),
        }
    }

    pub(crate) async fn request(&self, req: &Request) -> Result<Response, AtdError> {
        let mut guard = self.inner.lock().await;
        match &mut *guard {
            Pipe::Unix { read, write } => {
                write_frame(write, req).await?;
                let resp: Response = read_frame(read).await?;
                Ok(resp)
            }
            #[cfg(test)]
            Pipe::Duplex { read, write } => {
                write_frame(write, req).await?;
                let resp: Response = read_frame(read).await?;
                Ok(resp)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    /// Spawn a task that acts as a one-shot server: reads exactly one request
    /// from the server-side of a duplex pipe, maps it to a scripted response.
    async fn spin_server<F>(server_end: tokio::io::DuplexStream, mut handler: F)
    where
        F: FnMut(Request) -> Response + Send + 'static,
    {
        let (mut read, mut write) = tokio::io::split(server_end);
        tokio::spawn(async move {
            while let Ok(req) = read_frame::<_, Request>(&mut read).await {
                let resp = handler(req);
                if write_frame(&mut write, &resp).await.is_err() {
                    break;
                }
            }
        });
    }

    #[tokio::test]
    async fn ping_returns_ok_when_server_sends_pong() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::Ping => Response::Pong,
            _ => Response::Error {
                message: "unexpected".into(),
                code: None,
                retryable: None,
                details: None,
            },
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        client.ping().await.unwrap();
    }

    #[tokio::test]
    async fn ping_errors_when_server_sends_wrong_response() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::HelloResponse {
            version: "x".into(),
            capabilities: vec![],
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let err = client.ping().await.unwrap_err();
        assert!(matches!(err, AtdError::ProtocolError { .. }));
    }
}
```

Update `lib.rs`:

```rust
pub mod client;
pub mod endpoint;
pub mod protocol;
pub mod wire;

pub use client::AtdClient;
pub use endpoint::Endpoint;
```

- [ ] **Step 11.2: Run the tests**

```bash
cargo test -p atd-client --lib client
```

Expected: `2 passed; 0 failed`.

- [ ] **Step 11.3: Commit**

```bash
git add crates/atd-client/
git commit -m "feat(atd-client): add AtdClient::connect with ping handshake"
```

---

## Task 12: `AtdClient::discover`

**Files:**
- Create: `crates/atd-client/src/options.rs`
- Modify: `crates/atd-client/src/client.rs`
- Modify: `crates/atd-client/src/lib.rs`

**Approach:** ANOS's `tool_list` has no filter args. We send `tool_list`, receive the full array, then apply query/filter/limit **client-side** so the server remains unchanged. The design doc permits this for Phase 0 (§3.6).

- [ ] **Step 12.1: Write the failing test**

Create `crates/atd-client/src/options.rs`:

```rust
use atd_types::{ToolTier, ToolVisibility};

#[derive(Debug, Clone, Default)]
pub struct DiscoverFilter {
    pub tier: Option<ToolTier>,
    pub visibility: Option<ToolVisibility>,
    pub domain: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    pub dry_run: bool,
    pub preferred_binding: Option<atd_types::BindingProtocol>,
}
```

Append to `crates/atd-client/src/client.rs` (inside `impl AtdClient { ... }`):

```rust
    pub async fn discover(
        &self,
        query: Option<&str>,
        filter: crate::options::DiscoverFilter,
    ) -> Result<Vec<atd_types::ToolSummary>, AtdError> {
        let resp = self.request(&Request::ToolList).await?;
        let raw = match resp {
            Response::ToolListResponse { tools } => tools,
            Response::Error { message, .. } => {
                return Err(AtdError::ProtocolError {
                    expected: "tool_list".into(),
                    got: format!("error: {message}"),
                });
            }
            other => {
                return Err(AtdError::ProtocolError {
                    expected: "tool_list".into(),
                    got: format!("{other:?}"),
                });
            }
        };

        let arr = raw.as_array().ok_or_else(|| AtdError::ProtocolError {
            expected: "array of tool summaries".into(),
            got: format!("{raw}"),
        })?;

        let mut out: Vec<atd_types::ToolSummary> = Vec::with_capacity(arr.len());
        for v in arr {
            match serde_json::from_value::<atd_types::ToolSummary>(v.clone()) {
                Ok(s) => out.push(s),
                Err(_) => {
                    // Tolerate entries that are full ToolDefinitions by projecting down.
                    if let Ok(def) = serde_json::from_value::<atd_types::ToolDefinition>(v.clone())
                    {
                        out.push(atd_types::ToolSummary::from(&def));
                    }
                }
            }
        }

        if let Some(q) = query {
            let q_lower = q.to_lowercase();
            out.retain(|s| {
                s.name.to_lowercase().contains(&q_lower)
                    || s.description.to_lowercase().contains(&q_lower)
                    || s.id.to_lowercase().contains(&q_lower)
            });
        }
        if let Some(d) = filter.domain.as_deref() {
            out.retain(|s| s.domain == d);
        }
        if let Some(v) = filter.visibility {
            out.retain(|s| s.visibility == v);
        }
        if let Some(t) = filter.tier {
            out.retain(|s| s.tier == t);
        }
        if let Some(n) = filter.limit {
            out.truncate(n);
        }

        Ok(out)
    }
```

Add tests to the `mod tests` block in `client.rs`:

```rust
    #[tokio::test]
    async fn discover_projects_tool_definitions_to_summaries() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |req| match req {
            Request::ToolList => Response::ToolListResponse {
                tools: serde_json::json!([
                    {
                        "id": "anos:fs.read",
                        "name": "Read",
                        "description": "read a file",
                        "version": "0.1.0",
                        "capability": {
                            "domain": "fs",
                            "actions": ["read"],
                            "tags": ["filesystem"],
                            "intent_examples": []
                        },
                        "input_schema": {},
                        "output_schema": {},
                        "bindings": [{"protocol": "Cli", "config": {}}],
                        "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
                        "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
                        "trust": {"publisher": "anos", "trust_level": "L2Tested", "signature": null},
                        "visibility": "read"
                    }
                ]),
            },
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let summaries = client
            .discover(None, crate::options::DiscoverFilter::default())
            .await
            .unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "anos:fs.read");
        assert_eq!(summaries[0].domain, "fs");
    }

    #[tokio::test]
    async fn discover_applies_query_and_limit_client_side() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |_| Response::ToolListResponse {
            tools: serde_json::json!([
                {"id": "anos:fs.read", "name": "Read", "description": "read a file", "domain": "fs", "tags": []},
                {"id": "anos:fs.write", "name": "Write", "description": "write a file", "domain": "fs", "tags": []},
                {"id": "anos:web.fetch", "name": "Fetch", "description": "download a url", "domain": "web", "tags": []}
            ]),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);

        let only_fs = client
            .discover(
                Some("fs"),
                crate::options::DiscoverFilter {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(only_fs.len(), 1);
        assert!(only_fs[0].id.starts_with("anos:fs"));
    }
```

Update `lib.rs`:

```rust
pub mod client;
pub mod endpoint;
pub mod options;
pub mod protocol;
pub mod wire;

pub use client::AtdClient;
pub use endpoint::Endpoint;
pub use options::{CallOptions, DiscoverFilter};
```

- [ ] **Step 12.2: Run the tests**

```bash
cargo test -p atd-client --lib client::tests::discover
```

Expected: `2 passed; 0 failed`.

- [ ] **Step 12.3: Commit**

```bash
git add crates/atd-client/
git commit -m "feat(atd-client): add discover with client-side query/filter/limit"
```

---

## Task 13: `AtdClient::describe`

**Files:**
- Modify: `crates/atd-client/src/client.rs`

- [ ] **Step 13.1: Write the failing test**

Add to the `impl AtdClient { ... }` block in `client.rs`:

```rust
    pub async fn describe(
        &self,
        tool_id: &str,
    ) -> Result<atd_types::ToolDefinition, AtdError> {
        let resp = self
            .request(&Request::ToolSchema {
                tool_id: tool_id.to_string(),
            })
            .await?;

        match resp {
            Response::ToolSchemaResponse { schema } => {
                serde_json::from_value(schema).map_err(|e| AtdError::ProtocolError {
                    expected: "ToolDefinition".into(),
                    got: format!("deserialize error: {e}"),
                })
            }
            Response::Error { message, .. } if message.to_lowercase().contains("not found") => {
                Err(AtdError::ToolNotFound {
                    tool_id: tool_id.to_string(),
                    suggestions: vec![],
                })
            }
            Response::Error { message, .. } => Err(AtdError::ProtocolError {
                expected: "tool_schema".into(),
                got: format!("error: {message}"),
            }),
            other => Err(AtdError::ProtocolError {
                expected: "tool_schema".into(),
                got: format!("{other:?}"),
            }),
        }
    }
```

Add to the `mod tests` block:

```rust
    fn tool_def_json() -> serde_json::Value {
        serde_json::json!({
            "id": "anos:fs.read",
            "name": "Read",
            "description": "read a file",
            "version": "0.1.0",
            "capability": {
                "domain": "fs", "actions": ["read"], "tags": [], "intent_examples": []
            },
            "input_schema": {"type": "object"},
            "output_schema": {"type": "string"},
            "bindings": [{"protocol": "Cli", "config": {}}],
            "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
            "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
            "trust": {"publisher": "anos", "trust_level": "L2Tested", "signature": null},
            "visibility": "read"
        })
    }

    #[tokio::test]
    async fn describe_returns_full_tool_definition() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |req| match req {
            Request::ToolSchema { tool_id } => {
                assert_eq!(tool_id, "anos:fs.read");
                Response::ToolSchemaResponse {
                    schema: tool_def_json(),
                }
            }
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let def = client.describe("anos:fs.read").await.unwrap();
        assert_eq!(def.id, "anos:fs.read");
        assert_eq!(def.capability.domain, "fs");
    }

    #[tokio::test]
    async fn describe_maps_not_found_error_to_tool_not_found() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::Error {
            message: "tool not found: anos:nope".into(),
            code: None,
            retryable: None,
            details: None,
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let err = client.describe("anos:nope").await.unwrap_err();
        assert!(matches!(err, AtdError::ToolNotFound { .. }));
    }
```

- [ ] **Step 13.2: Run the tests**

```bash
cargo test -p atd-client --lib client::tests::describe
```

Expected: `2 passed; 0 failed`.

- [ ] **Step 13.3: Commit**

```bash
git add crates/atd-client/
git commit -m "feat(atd-client): add describe returning ToolDefinition"
```

---

## Task 14: `AtdClient::call`

**Files:**
- Modify: `crates/atd-client/src/client.rs`

- [ ] **Step 14.1: Write the failing test**

Add to the `impl AtdClient { ... }` block:

```rust
    pub async fn call(
        &self,
        tool_id: &str,
        args: serde_json::Value,
        opts: crate::options::CallOptions,
    ) -> Result<atd_types::ToolResult, AtdError> {
        let resp = self
            .request(&Request::RunTool {
                tool_id: tool_id.to_string(),
                args,
                dry_run: opts.dry_run,
            })
            .await?;

        match resp {
            Response::ToolResultResponse {
                tool_id: resp_tool_id,
                result,
                success,
                dry_run: _,
            } => {
                if success {
                    // Server returned raw data JSON. Wrap in ToolResult::Success
                    // with synthetic metadata — the ANOS reference server does
                    // not yet populate atd-shaped metadata (tracked as an
                    // open gap in docs/issues/).
                    Ok(atd_types::ToolResult::Success {
                        data: result,
                        metadata: atd_types::ToolResultMetadata {
                            tool_id: resp_tool_id,
                            version: "0.0.0".into(),
                            binding: atd_types::BindingProtocol::Cli,
                            latency_ms: 0,
                            timestamp: chrono::Utc::now(),
                            request_id: ulid::Ulid::new(),
                        },
                    })
                } else {
                    let (code, message, retryable) = extract_error(&result);
                    Ok(atd_types::ToolResult::Error {
                        code,
                        message,
                        reason: None,
                        retryable,
                    })
                }
            }
            Response::Error { message, retryable, .. } => Err(AtdError::ToolExecutionFailed {
                tool_id: tool_id.to_string(),
                inner: Box::new(std::io::Error::other(format!(
                    "{message} (retryable={})",
                    retryable.unwrap_or(false)
                ))),
            }),
            other => Err(AtdError::ProtocolError {
                expected: "tool_result".into(),
                got: format!("{other:?}"),
            }),
        }
    }

fn extract_error(value: &serde_json::Value) -> (String, String, bool) {
    let code = value
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN")
        .to_string();
    let message = value
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("tool call failed")
        .to_string();
    let retryable = value
        .get("retryable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    (code, message, retryable)
}
```

Add to the `mod tests` block:

```rust
    #[tokio::test]
    async fn call_success_returns_tool_result_success() {
        let (client_end, server_end) = duplex(16_384);
        spin_server(server_end, |req| match req {
            Request::RunTool { tool_id, args, dry_run } => {
                assert_eq!(tool_id, "anos:fs.read");
                assert_eq!(args["path"], "/tmp/x");
                assert!(!dry_run);
                Response::ToolResultResponse {
                    tool_id,
                    result: serde_json::json!({"content": "ok"}),
                    success: true,
                    dry_run: false,
                }
            }
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let r = client
            .call(
                "anos:fs.read",
                serde_json::json!({"path": "/tmp/x"}),
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap();
        assert!(r.is_success());
        assert_eq!(r.data().unwrap()["content"], "ok");
    }

    #[tokio::test]
    async fn call_failure_returns_tool_result_error() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |_| Response::ToolResultResponse {
            tool_id: "anos:fs.read".into(),
            result: serde_json::json!({"code": "EPERM", "message": "no", "retryable": false}),
            success: false,
            dry_run: false,
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        let r = client
            .call(
                "anos:fs.read",
                serde_json::json!({}),
                crate::options::CallOptions::default(),
            )
            .await
            .unwrap();
        match r {
            atd_types::ToolResult::Error { code, .. } => assert_eq!(code, "EPERM"),
            _ => panic!("expected error variant"),
        }
    }

    #[tokio::test]
    async fn call_forwards_dry_run_flag() {
        let (client_end, server_end) = duplex(4096);
        spin_server(server_end, |req| match req {
            Request::RunTool { dry_run, .. } => {
                assert!(dry_run);
                Response::ToolResultResponse {
                    tool_id: "anos:fs.read".into(),
                    result: serde_json::json!({}),
                    success: true,
                    dry_run: true,
                }
            }
            _ => unreachable!(),
        })
        .await;

        let (cr, cw) = tokio::io::split(client_end);
        let client = AtdClient::from_duplex(cr, cw);
        client
            .call(
                "anos:fs.read",
                serde_json::json!({}),
                crate::options::CallOptions {
                    dry_run: true,
                    preferred_binding: None,
                },
            )
            .await
            .unwrap();
    }
```

- [ ] **Step 14.2: Run the tests**

```bash
cargo test -p atd-client --lib client::tests::call
cargo clippy -p atd-client -- -D warnings
```

Expected: `3 passed; 0 failed`, clippy clean.

- [ ] **Step 14.3: Commit**

```bash
git add crates/atd-client/
git commit -m "feat(atd-client): add call with success/error handling and dry_run"
```

---

## Task 15: ANOS-Free Integration Test Harness

**Files:**
- Create: `crates/atd-client/tests/mock_server.rs`

**Why under `crates/atd-client/tests/`:** Rust integration tests must live under a `tests/` dir inside the crate to link as separate compilation units. The design doc mentions `tests/integration/mock_server.rs` at the workspace root — we put it under the crate's `tests/` directory instead so `cargo test -p atd-client` picks it up automatically. The workspace-level `tests/` directory is not required and its mention in `design.md` §4 can be reconciled later.

- [ ] **Step 15.1: Write the failing test**

Create `crates/atd-client/tests/mock_server.rs`:

```rust
//! Integration test: prove atd-client can drive the full protocol against a
//! server that has zero `anos-*` crate dependencies. This is the load-bearing
//! check for the "independent reference implementation" claim in CLAUDE.md.

use atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

// Re-declare wire + protocol shapes here so the mock server has literally no
// path dependency into atd-client or atd-types crate internals.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerReq {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "tool_list")]
    ToolList,
    #[serde(rename = "tool_schema")]
    ToolSchema { tool_id: String },
    #[serde(rename = "run_tool")]
    RunTool {
        tool_id: String,
        args: serde_json::Value,
        dry_run: bool,
    },
    #[serde(rename = "hello")]
    Hello { version: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerResp {
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "tool_list")]
    ToolList { tools: serde_json::Value },
    #[serde(rename = "tool_schema")]
    ToolSchema { schema: serde_json::Value },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_id: String,
        result: serde_json::Value,
        success: bool,
        dry_run: bool,
    },
}

async fn read_frame<R: AsyncReadExt + Unpin>(r: &mut R) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let n = u32::from_be_bytes(len) as usize;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_frame<W: AsyncWriteExt + Unpin, T: Serialize>(
    w: &mut W,
    msg: &T,
) -> std::io::Result<()> {
    let b = serde_json::to_vec(msg).unwrap();
    let len = (b.len() as u32).to_be_bytes();
    w.write_all(&len).await?;
    w.write_all(&b).await?;
    w.flush().await
}

fn sample_tool() -> serde_json::Value {
    serde_json::json!({
        "id": "mock:echo.say",
        "name": "Echo",
        "description": "echo back the input",
        "version": "0.1.0",
        "capability": {
            "domain": "echo", "actions": ["say"], "tags": ["test"], "intent_examples": []
        },
        "input_schema": {"type": "object"},
        "output_schema": {"type": "object"},
        "bindings": [{"protocol": "Cli", "config": {}}],
        "safety": {"level": "Read", "dry_run": false, "side_effects": [], "data_sensitivity": null},
        "resources": {"timeout_ms": 1000, "max_concurrent": 1, "rate_limit_per_min": null, "estimated_tokens": null},
        "trust": {"publisher": "mock", "trust_level": "L2Tested", "signature": null},
        "visibility": "read"
    })
}

async fn spawn_mock_server() -> PathBuf {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mock.sock");
    let listener = UnixListener::bind(&path).unwrap();
    // Keep the tempdir alive for the duration of the test process.
    std::mem::forget(dir);

    let path_clone = path.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (mut read, mut write) = stream.into_split();
                loop {
                    let buf = match read_frame(&mut read).await {
                        Ok(b) => b,
                        Err(_) => return,
                    };
                    let req: ServerReq = serde_json::from_slice(&buf).unwrap();
                    let resp = match req {
                        ServerReq::Ping => ServerResp::Pong,
                        ServerReq::Hello { .. } => continue,
                        ServerReq::ToolList => ServerResp::ToolList {
                            tools: serde_json::json!([sample_tool()]),
                        },
                        ServerReq::ToolSchema { tool_id } => {
                            assert_eq!(tool_id, "mock:echo.say");
                            ServerResp::ToolSchema { schema: sample_tool() }
                        }
                        ServerReq::RunTool { tool_id, args, dry_run } => ServerResp::ToolResult {
                            tool_id,
                            result: serde_json::json!({"echo": args}),
                            success: true,
                            dry_run,
                        },
                    };
                    if write_frame(&mut write, &resp).await.is_err() {
                        return;
                    }
                }
            });
        }
    });

    // Give the listener a beat to be ready.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let _ = path_clone;
    path
}

#[tokio::test]
async fn end_to_end_against_anos_free_mock() {
    let sock = spawn_mock_server().await;
    let client = AtdClient::connect(Endpoint::unix(&sock)).await.unwrap();

    let summaries = client
        .discover(None, DiscoverFilter::default())
        .await
        .unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "mock:echo.say");

    let def = client.describe("mock:echo.say").await.unwrap();
    assert_eq!(def.capability.domain, "echo");

    let result = client
        .call(
            "mock:echo.say",
            serde_json::json!({"hello": "world"}),
            CallOptions::default(),
        )
        .await
        .unwrap();
    assert!(result.is_success());
    assert_eq!(
        result.data().unwrap()["echo"]["hello"],
        serde_json::json!("world")
    );
}
```

- [ ] **Step 15.2: Run the integration test**

```bash
cargo test -p atd-client --test mock_server
```

Expected: `1 passed; 0 failed`.

- [ ] **Step 15.3: Commit**

```bash
git add crates/atd-client/tests/
git commit -m "test(atd-client): add ANOS-free end-to-end harness over Unix socket"
```

---

## Task 16: `hello_atd.rs` Example

**Files:**
- Create: `examples/hello_atd.rs`
- Modify: `Cargo.toml` (workspace manifest — register examples via a shim crate)
- Create: `examples/Cargo.toml`

**Approach:** Workspace-level `examples/` isn't automatically picked up by cargo. We make `examples` a tiny binary crate that's a workspace member.

- [ ] **Step 16.1: Write the failing test (build check)**

Create `examples/Cargo.toml`:

```toml
[package]
name = "atd-examples"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[[bin]]
name = "hello_atd"
path = "hello_atd.rs"

[dependencies]
atd-client = { path = "../crates/atd-client" }
atd-types = { path = "../crates/atd-types" }
tokio = { workspace = true }
serde_json = { workspace = true }
```

Create `examples/hello_atd.rs`:

```rust
//! Minimum working example: connect to the ANOS daemon (or any ATD server)
//! over a Unix socket, discover up to 3 tools, describe the first one, and
//! call it with `dry_run=true`. Prints structured output at each step.
//!
//! Run:
//!   ANOS_SOCK=~/.anos/anos.sock cargo run -p atd-examples --bin hello_atd

use atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sock = std::env::var("ANOS_SOCK").ok().map(std::path::PathBuf::from);
    let endpoint = match sock {
        Some(p) => Endpoint::unix(p),
        None => Endpoint::default_anos(),
    };

    println!("[atd] connecting to {endpoint:?}");
    let client = AtdClient::connect(endpoint).await?;
    println!("[atd] connected");

    let tools = client
        .discover(
            None,
            DiscoverFilter {
                limit: Some(3),
                ..Default::default()
            },
        )
        .await?;
    println!("[atd] {} tools discovered", tools.len());
    for t in &tools {
        println!("        - {} ({})", t.id, t.name);
    }

    let Some(first) = tools.first() else {
        println!("[atd] no tools to describe/call — done.");
        return Ok(());
    };

    let def = client.describe(&first.id).await?;
    println!(
        "[atd] describe({}) → domain={}, bindings={}",
        def.id,
        def.capability.domain,
        def.bindings.len()
    );

    let result = client
        .call(
            &first.id,
            serde_json::json!({}),
            CallOptions {
                dry_run: true,
                preferred_binding: None,
            },
        )
        .await?;

    match result {
        atd_types::ToolResult::Success { data, .. } => {
            println!("[atd] call ok: {}", serde_json::to_string(&data)?);
        }
        atd_types::ToolResult::Error { code, message, .. } => {
            println!("[atd] call error: {code} — {message}");
        }
    }

    Ok(())
}
```

Update workspace `Cargo.toml` — change `members`:

```toml
members = ["crates/atd-types", "crates/atd-client", "examples"]
```

- [ ] **Step 16.2: Verify the example builds**

```bash
cargo build -p atd-examples --bin hello_atd
```

Expected: compiles with no warnings.

- [ ] **Step 16.3: Run it against the mock server used in Task 15**

Write a throwaway verification: run the mock server manually in one shell, point the example at its socket. Since the mock is in a test binary we just re-run the integration test which covers the same wire calls:

```bash
cargo test -p atd-client --test mock_server -- --nocapture
```

Expected: the integration test already proves the three-API sequence works end-to-end. No separate manual run needed.

- [ ] **Step 16.4: Commit**

```bash
git add examples/ Cargo.toml
git commit -m "feat(examples): add hello_atd showing connect/discover/describe/call"
```

---

## Task 17: CI Workflow with ANOS-Free Guarantee

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 17.1: Write the workflow**

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Format check
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Build
        run: cargo build --workspace --all-targets
      - name: Test
        run: cargo test --workspace --all-targets
      - name: ANOS-free check (no anos-* crate in dependency tree)
        run: |
          if cargo tree --workspace --prefix none \
             | grep -E '^\s*anos-' ; then
            echo "::error::atd-mvp must not depend on any anos-* crate"
            exit 1
          fi
      - name: Manifest grep for anos-* deps
        run: |
          if grep -RInE '^\s*anos-[a-z-]+\s*=' \
             crates/ examples/ 2>/dev/null ; then
            echo "::error::found anos-* dependency in a Cargo.toml"
            exit 1
          fi
```

- [ ] **Step 17.2: Simulate the ANOS-free check locally**

```bash
cd /home/nan/proj/atd-mvp
cargo tree --workspace --prefix none | grep -E '^\s*anos-' && echo FAIL || echo OK
grep -RInE '^\s*anos-[a-z-]+\s*=' crates/ examples/ 2>/dev/null && echo FAIL || echo OK
```

Expected: both print `OK`.

- [ ] **Step 17.3: Format + clippy + test the whole workspace**

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Expected: fmt clean, clippy clean, all tests pass.

- [ ] **Step 17.4: Commit**

```bash
git add .github/
git commit -m "ci: add workflow with ANOS-free dependency check"
```

---

## Task 18: README 15-min Install Story

**Files:**
- Modify: `/home/nan/proj/atd-mvp/README.md`

- [ ] **Step 18.1: Rewrite the status + quickstart sections of `README.md`**

Open `/home/nan/proj/atd-mvp/README.md` and replace the `## Phase 0 Week 1` section with a concrete quickstart. Locate the existing section that starts with `## Phase 0 Week 1 (concrete first steps)` and replace it with:

```markdown
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

## Development

```bash
cargo test --workspace              # unit + integration tests
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

The ANOS-free integration test lives in `crates/atd-client/tests/mock_server.rs` and runs automatically in CI — it proves the client talks to a server that has zero ANOS crate dependencies.
```

- [ ] **Step 18.2: Sanity-check the README renders**

```bash
grep -n "15-minute quickstart" /home/nan/proj/atd-mvp/README.md
grep -n "Phase 0 Week 1 (concrete first steps)" /home/nan/proj/atd-mvp/README.md
```

Expected: first grep returns a line number; second returns nothing (the old section was replaced).

- [ ] **Step 18.3: Final full-workspace verification**

```bash
cd /home/nan/proj/atd-mvp
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace
```

Expected: all four commands succeed.

- [ ] **Step 18.4: Commit**

```bash
git add README.md
git commit -m "docs: rewrite README with 15-min install story"
```

- [ ] **Step 18.5: Tag the milestone**

```bash
git tag -a phase0-week1 -m "Phase 0 Week 1 bootstrap complete"
git log --oneline
```

Expected: `git log` shows 17 commits culminating in the README rewrite, tagged `phase0-week1`.

---

## Post-Plan Verification Checklist

After all tasks complete, run this sequence and confirm each passes before declaring Phase 0 Week 1 done:

- [ ] `cargo build --workspace` — clean build
- [ ] `cargo test --workspace --all-targets` — all tests pass (unit + integration)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] `cargo fmt --all -- --check` — formatting clean
- [ ] `cargo tree --workspace --prefix none | grep -E '^\s*anos-'` — must print nothing (exit status 1)
- [ ] `cargo run -p atd-examples --bin hello_atd` against a live ANOS daemon — prints the three-step trace
- [ ] First-call latency <100ms on local Unix socket (design exit criterion §7.1) — verify by running `time cargo run -p atd-examples --bin hello_atd` and checking total wall time; the first call (after connect) should be sub-100ms

## What's Out of Scope for This Plan

Deferred to their own plans:

1. **Phase 0 Week 2–3 deliverables** not in §11: `atd-cli` binary (`atd list|schema|call|doctor|allow`), LangChain demo, demo video.
2. **Phase 1 deliverables** (§7.2): Python SDK, TypeScript SDK, stdio transport, MCP-compat transport, `atd-langchain`, `atd-mcp-bridge`, `atd-dispatch` skill on skills.sh.
3. **Phase 2 deliverables** (§7.3): HTTP transport, AppFunction binding, conformance test suite.
