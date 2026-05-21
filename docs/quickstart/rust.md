# Rust Quickstart — ATD Client SDK

**Environment:** Linux, Rust 1.85+ (edition 2024), Cargo.

---

## What this doc covers

By the end of this guide you will have:

- Added `atd-sdk` to a Cargo project (or run the in-repo example directly)
- Connected to an ATD server over a Unix socket
- Called `discover`, `describe`, and `call` against the `atd-ref-server` reference implementation
- Handled errors with `AtdError::is_retryable()` and `suggest_fix()`
- Exported tool definitions to OpenAI, Anthropic, and LangChain function-calling formats

This guide covers the Rust reference SDK only. For Python, see [`python.md`](python.md).
For the raw wire protocol, see [`../protocol/wire-format.md`](../protocol/wire-format.md).

---

## Install

`atd-sdk` is not yet published to crates.io. Add it from a local path. The
client SDK lives in the `atd-sdk` crate; the wire types it returns
(`ToolResult`, `ToolSummary`, `AtdError`, …) live in `atd-protocol`.

**In your `Cargo.toml`:**

```toml
[dependencies]
atd-sdk      = { path = "/path/to/atd/crates/atd-sdk" }
atd-protocol = { path = "/path/to/atd/crates/atd-protocol" }
serde_json   = "1"
tokio        = { version = "1", features = ["full"] }
```

Replace `/path/to/atd` with the absolute path where you cloned the repository.

**LLM adapter features (optional):**

```toml
# All three adapters:
atd-sdk = { path = "...", features = ["adapters"] }

# Individual adapters:
atd-sdk = { path = "...", features = ["openai"] }
atd-sdk = { path = "...", features = ["anthropic"] }
atd-sdk = { path = "...", features = ["langchain"] }
```

The `adapters` feature is shorthand for `["openai", "anthropic", "langchain"]`.
No extra runtime crates are pulled in — adapters emit plain `serde_json::Value`.

> **Future path:** Once `atd-sdk` is published, you will use `cargo add atd-sdk`
> or `atd-sdk = "1"` in `Cargo.toml`. The API surface will not change.

---

## The 30-second hello_atd

The following program connects to the ref-server, discovers available tools,
and calls `ref:echo.say`.

```rust
use atd_sdk::{AtdClient, CallOptions, DiscoverFilter, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to the ATD server over a Unix socket.
    // Replace <YOUR_SOCKET_PATH> with the actual socket path.
    let client = AtdClient::connect(Endpoint::unix("<YOUR_SOCKET_PATH>")).await?;

    // List all available tools.
    let tools = client.discover(None, DiscoverFilter::default()).await?;
    println!("connected — {} tools available", tools.len());

    // Call a tool.
    let result = client
        .call(
            "ref:echo.say",
            serde_json::json!({"text": "hello from ATD"}),
            CallOptions::default(),
        )
        .await?;

    match result {
        atd_protocol::ToolResult::Success { data, .. } => {
            println!("success: {}", serde_json::to_string(&data)?);
        }
        atd_protocol::ToolResult::Error { code, message, .. } => {
            eprintln!("tool error {code}: {message}");
        }
    }

    Ok(())
}
```

`AtdClient::connect` takes an `Endpoint` by value. The connection ping is
performed inside `connect` — if the server is unreachable you get an
`AtdError` before any of your code runs. `connect` also retries transient
failures with exponential backoff; for explicit control over retry policy use
`AtdClient::connect_with_options` with a `ConnectOptions`.

---

## Running against atd-ref-server

`atd-ref-server` is the in-repo neutral reference server. It registers the
built-in tools — `ref:echo.say`, the `ref:fs.*` family, `ref:shell.exec` /
`ref:shell.pwsh`, `ref:web.fetch`, and `ref:external.uname` on Unix (10 tools
total) — and speaks the ATD wire protocol over a Unix socket.

**Build the server:**

```bash
cd /path/to/atd
cargo build --release -p atd-ref-server
```

**Launch it with an explicit socket path:**

```bash
./target/release/atd-ref-server --sock /tmp/atd-demo.sock
```

The server is ready as soon as the socket file appears. The in-repo example
`examples/hello_atd.rs` auto-spawns and tears down the server for you:

```bash
cargo run --example hello_atd -p atd-examples
```

Expected output:

```
[atd] auto-spawning atd-ref-server → /tmp/.../demo.sock
[atd] connected
[atd] 10 tools registered

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"echoed":{"text":"hello from ATD"}}

[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → 5 paths: Cargo.toml, crates/atd-sdk/Cargo.toml, ... (+2 more)

[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout="Linux"

[atd] done.
```

**Override the server socket** (to point at a different ATD server):

```bash
ATD_SOCK=/path/to/other.sock cargo run --example hello_atd -p atd-examples
```

**Troubleshooting:**

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| `No such file or directory` on connect | Socket path wrong or server not started | Check `--sock` arg; wait for the file to appear |
| `Connection refused` | Socket file exists but server crashed | Check server stderr; restart |
| `expected pong, got ...` | Server speaks a different protocol version | Ensure client and server are from the same release |
| `atd-ref-server release binary not found` | Not built yet | `cargo build --release -p atd-ref-server` |

---

## Discover, describe, call in depth

### discover

```rust
pub async fn discover(
    &self,
    query: Option<&str>,
    filter: DiscoverFilter,
) -> Result<Vec<atd_protocol::ToolSummary>, AtdError>
```

Returns a list of `ToolSummary` values. The client applies `query` and `filter`
locally after fetching the full list from the server.

**`DiscoverFilter` fields:**

```rust
pub struct DiscoverFilter {
    pub tier:       Option<ToolTier>,       // Hot / Warm / Cold
    pub visibility: Option<ToolVisibility>, // Read / Write / Dangerous / System / Hidden
    pub domain:     Option<String>,         // e.g. "fs", "web"
    pub limit:      Option<usize>,          // cap result count
}
```

All fields are optional. `DiscoverFilter::default()` applies no filtering.

**Examples:**

```rust
use atd_sdk::DiscoverFilter;
use atd_protocol::ToolVisibility;

// No filter — get everything.
let all = client.discover(None, DiscoverFilter::default()).await?;

// Text search across id, name, description.
let fs_tools = client.discover(Some("fs"), DiscoverFilter::default()).await?;

// Domain filter.
let web_tools = client
    .discover(
        None,
        DiscoverFilter { domain: Some("web".into()), ..Default::default() },
    )
    .await?;

// Limit results.
let first_five = client
    .discover(
        None,
        DiscoverFilter { limit: Some(5), ..Default::default() },
    )
    .await?;
```

**`ToolSummary` key fields:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Canonical tool id: `<publisher>:<domain>.<action>` |
| `name` | `String` | Human-readable display name |
| `description` | `String` | One-line purpose |
| `domain` | `String` | Derived from id when the server omits it |
| `tier` | `ToolTier` | `Hot` / `Warm` / `Cold` |
| `visibility` | `ToolVisibility` | `Read` / `Write` / `Dangerous` / `System` / `Hidden` |
| `tags` | `Vec<String>` | Freeform labels |
| `input_schema` | `Option<serde_json::Value>` | JSON Schema when populated by the server |

The `id` format is `<publisher>:<domain>.<action>`, e.g. `ref:echo.say` or
`ref:fs.glob`. LLM adapters sanitize this to `ref_echo_say` (colons and dots
become underscores) because LLM APIs require alphanumeric-plus-underscore
function names.

### describe

```rust
pub async fn describe(
    &self,
    tool_id: &str,
) -> Result<atd_protocol::ToolDefinition, AtdError>
```

Fetches the full `ToolDefinition` for one tool. This includes the input/output
JSON Schemas, safety metadata, binding configuration, and trust level —
information that `ToolSummary` omits to keep the list response small.

```rust
let def = client.describe("ref:echo.say").await?;

println!("version:      {}", def.version);
println!("domain:       {}", def.capability.domain);
println!("safety:       {:?}", def.safety.level);
println!("input schema: {}", serde_json::to_string_pretty(&def.input_schema)?);
```

If the tool does not exist, you get `AtdError::ToolNotFound`. Call
`suggest_fix()` to surface a hint to the user:

```rust
use atd_protocol::AtdError;

match client.describe("ref:typo.tool").await {
    Ok(def) => { /* use def */ }
    Err(e @ AtdError::ToolNotFound { .. }) => {
        eprintln!("tool not found");
        if let Some(hint) = e.suggest_fix() {
            eprintln!("hint: {hint}");
        }
    }
    Err(e) => return Err(e.into()),
}
```

### call

```rust
pub async fn call(
    &self,
    tool_id: &str,
    args: serde_json::Value,
    opts: CallOptions,
) -> Result<atd_protocol::ToolResult, AtdError>
```

Executes a tool. `args` is a JSON object matching the tool's `input_schema`.
`CallOptions` carries two fields:

```rust
pub struct CallOptions {
    pub dry_run:           bool,                      // preview without execution
    pub preferred_binding: Option<BindingProtocol>,   // Cli / Mcp / Rest / AppFunction
}
```

`CallOptions::default()` sets `dry_run: false` and `preferred_binding: None`.

**`ToolResult` variants:**

```rust
pub enum ToolResult {
    Success {
        data:     serde_json::Value,
        metadata: ToolResultMetadata,
    },
    Error {
        code:      String,          // e.g. "EPERM", "TIMEOUT"
        message:   String,
        reason:    Option<String>,  // raw server payload (JSON string)
        retryable: bool,
    },
}
```

`ToolResult` is not an `Err` — a well-formed server response that reports
execution failure comes back as `Ok(ToolResult::Error { .. })`. You only get
`Err(AtdError::...)` when the transport or protocol layer fails.

**Pattern-match both arms:**

```rust
let r = client
    .call("ref:echo.say", serde_json::json!({"text": "hi"}), CallOptions::default())
    .await?;
match r {
    atd_protocol::ToolResult::Success { data, .. } => {
        println!("{}", data["echoed"]);
    }
    atd_protocol::ToolResult::Error { code, message, retryable, .. } => {
        eprintln!("tool error [{code}] {message} (retryable={retryable})");
    }
}
```

**Dry-run mode:**

```rust
let opts = CallOptions { dry_run: true, preferred_binding: None };
let preview = client
    .call("ref:shell.exec", serde_json::json!({"command": "rm -rf /"}), opts)
    .await?;
// Server validates args and returns what it *would* do without executing.
```

### Paginated results

A tool whose result can be large may opt into pagination. `call_page` fetches
one page (pass `cursor: None` on the first call, then the server's
`next_cursor` verbatim); `call_all` auto-loops and merges every page per a
`MergePolicy`. See [`../architecture.md`](../architecture.md) §5.6 for the
cursor contract.

```rust
use atd_sdk::CallAllOptions;

let all = client
    .call_all("vendor:list_things", serde_json::json!({}), CallAllOptions::default())
    .await?;
```

---

## Error handling

`AtdError` is the error type returned from all `AtdClient` methods. It is
defined in `atd-protocol` (re-exported under `atd_protocol::AtdError`) and
implements `std::error::Error`.

### AtdError variants

| Variant | When | `is_retryable()` |
|---------|------|-----------------|
| `ToolNotFound { tool_id, suggestions }` | Server does not know the tool id | `false` |
| `InvalidArguments { tool_id, field, reason }` | Args fail schema validation | `false` |
| `CapabilityDenied { tool_id, required, granted }` | Caller lacks a required capability | `false` |
| `BindingUnavailable { tool_id, tried, reason }` | No usable binding for the tool | `true` |
| `ToolExecutionFailed { tool_id, inner }` | Server attempted execution and it failed at the OS/network level | `false` |
| `Timeout { tool_id, after_ms }` | Server did not respond within the deadline | `true` |
| `ServerUnreachable(io::Error)` | Socket connect failed or connection dropped | `true` |
| `NotImplemented { feature }` | The server does not support a requested capability | `false` |
| `ProtocolError { expected, got }` | Response shape does not match the expected message type | `false` |
| `PaginationLimitExceeded { pages_fetched, bytes_fetched }` | `call_all` hit `max_pages` / `max_total_bytes` | `false` |
| `MergeFailed { reason }` | A `MergePolicy` could not combine pages | `false` |

### is_retryable and suggest_fix

```rust
use atd_protocol::AtdError;
use atd_sdk::{AtdClient, CallOptions};

async fn robust_call(
    client: &AtdClient,
    tool_id: &str,
    args: serde_json::Value,
) -> Result<atd_protocol::ToolResult, AtdError> {
    let mut attempts = 0u32;
    loop {
        match client.call(tool_id, args.clone(), CallOptions::default()).await {
            Ok(r) => return Ok(r),
            Err(e) if e.is_retryable() && attempts < 3 => {
                attempts += 1;
                let delay = std::time::Duration::from_millis(200 * (1 << attempts));
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                if let Some(hint) = e.suggest_fix() {
                    eprintln!("hint: {hint}");
                }
                return Err(e);
            }
        }
    }
}
```

`is_retryable()` returns `true` for `Timeout`, `ServerUnreachable`, and
`BindingUnavailable`. `suggest_fix()` returns a human-readable string for
`ToolNotFound`, `CapabilityDenied`, `ServerUnreachable`, and `Timeout`; `None`
for others.

### ToolResult::Error vs AtdError

A `ToolResult::Error` means the server successfully processed your request but
the tool itself reported failure (wrong permissions, network error inside the
tool, etc.). An `AtdError` means the transport or protocol layer failed before
a valid result was produced. Do not conflate the two.

```rust
// AtdError — transport/protocol failure:
let result: Result<atd_protocol::ToolResult, AtdError> = client.call(...).await;

// ToolResult::Error — tool-reported failure inside Ok():
if let Ok(atd_protocol::ToolResult::Error { code, retryable, .. }) = result {
    // the server responded correctly but the tool failed
}
```

---

## LLM adapters

Adapters convert a `Vec<ToolSummary>` (from `discover`) into the JSON shape that
each LLM provider's SDK expects for function/tool calling. Enable them via Cargo
features — they add no runtime dependencies.

### OpenAI

```toml
atd-sdk = { path = "...", features = ["openai"] }
```

```rust
use atd_sdk::adapters::openai::as_openai_tools;

let summaries = client.discover(None, DiscoverFilter::default()).await?;
let tools_json = as_openai_tools(&summaries);
```

Each element has the shape:

```json
{
  "type": "function",
  "function": {
    "name": "ref_echo_say",
    "description": "Echo text back to the caller",
    "parameters": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"] }
  }
}
```

Tool names are sanitized: `ref:echo.say` → `ref_echo_say` (colons and dots
become underscores).

### Anthropic

```toml
atd-sdk = { path = "...", features = ["anthropic"] }
```

```rust
use atd_sdk::adapters::anthropic::as_anthropic_tools;

let summaries = client.discover(None, DiscoverFilter::default()).await?;
let tools_json = as_anthropic_tools(&summaries);
```

Anthropic's shape differs from OpenAI's: no `"type": "function"` wrapper, and
the schema field is `"input_schema"` instead of `"parameters"`:

```json
{
  "name": "ref_echo_say",
  "description": "Echo text back to the caller",
  "input_schema": { "type": "object", "properties": { "text": { "type": "string" } } }
}
```

### LangChain

```toml
atd-sdk = { path = "...", features = ["langchain"] }
```

```rust
use atd_sdk::adapters::langchain::as_langchain_tools;

let summaries = client.discover(None, DiscoverFilter::default()).await?;
let tools_json = as_langchain_tools(&summaries);
// tools_json is Vec<serde_json::Value> in OpenAI function-calling shape.
```

The LangChain Rust adapter emits the same shape as the OpenAI adapter
(OpenAI-compatible function-calling JSON). This is intentional: `langchain-rust`
is pre-1.0 and its Rust API changes frequently; emitting plain JSON keeps
`atd-sdk` stable regardless of `langchain-rust` version.

### Resolving sanitized names back to ATD ids

When an LLM returns a function call with the sanitized name (e.g.
`ref_echo_say`), resolve it back to the canonical ATD id before passing it to
`client.call`:

```rust
use atd_sdk::adapters::resolve_sanitized_id;

// `summaries` is the Vec<ToolSummary> you passed to the adapter.
let atd_id = resolve_sanitized_id("ref_echo_say", &summaries)
    .ok_or("unknown tool name")?;

let result = client.call(atd_id, args, CallOptions::default()).await?;
```

---

## Next steps

- **Framework integration:** [`../integrations/langchain.md`](../integrations/langchain.md) — full LangChain agent walk-through with `as_langchain_tools`.
- **Wire protocol:** [`../protocol/wire-format.md`](../protocol/wire-format.md) — length-prefixed JSON framing, all message types, extension points.
- **Error reference:** [`../protocol/error-codes.md`](../protocol/error-codes.md) — the full error taxonomy with trigger conditions and recovery strategies.
- **Python SDK:** [`python.md`](python.md) — the same APIs, idiomatic Python, sync wrapper, LangChain `StructuredTool` wiring.
- **In-repo example:** `examples/hello_atd.rs` is a self-contained demo that auto-spawns `atd-ref-server`. Read the source for a full lifecycle example including teardown.
