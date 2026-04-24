# ATD Error Codes Reference

**Protocol version:** 0.1.0
**Source:** `crates/atd-protocol/src/error.rs` + `crates/atd-tools-*/src/` at tag `sp-refactor-v1`

This document is the authoritative reference for all error conditions in the ATD
protocol. It covers both the client-side `AtdError` enum and the server-side
`ToolResult::Error` code strings emitted by the reference server's tool implementations.

---

## 1. Overview: Two Error Layers

ATD errors arise at two distinct layers. Understanding the distinction matters for
deciding how to handle and retry them.

### 1.1 Layer 1: Client-side `AtdError`

`AtdError` is the Rust enum in `crates/atd-protocol/src/error.rs`. It is returned by
`AtdClient` methods (`discover`, `describe`, `call`, `ping`) whenever something goes
wrong **before the tool produces a result** — transport failure, protocol violation,
argument validation, or capability denial.

`AtdError` values are **never serialized over the wire**. They are synthesized by the
client library and exist only in the calling process.

```rust
use atd_sdk::AtdClient;
use atd_protocol::AtdError;

let result: Result<ToolResult, AtdError> = client.call("ref:fs.read", args, opts).await;
match result {
    Ok(tool_result) => { /* tool ran; check ToolResult::Success vs Error */ }
    Err(atd_err) => { /* something failed before/during transport or invocation */ }
}
```

### 1.2 Layer 2: Server-side `ToolResult::Error`

When a tool is successfully invoked but fails during execution, the server returns a
`ToolResult` with `status: "error"`. This result travels over the wire and is
deserialized by the client into `ToolResult::Error { code, message, reason, retryable }`.

A `ToolResult::Error` is **not** an `AtdError`. It is a successful RPC call that
carried a failure payload. The caller receives it as `Ok(ToolResult::Error { .. })`,
not as `Err(AtdError::ToolExecutionFailed { .. })`.

```rust
match client.call("ref:fs.read", args, opts).await {
    Ok(ToolResult::Success { data, .. }) => { /* use data */ }
    Ok(ToolResult::Error { code, message, retryable, .. }) => {
        // tool was called, but execution failed on server
        eprintln!("tool error [{code}]: {message}");
        if retryable { /* retry */ }
    }
    Err(atd_err) => {
        // transport or client-side failure
        if atd_err.is_retryable() { /* retry */ }
    }
}
```

### 1.3 Relationship diagram

```
AtdClient::call()
    │
    ├── Err(AtdError::ServerUnreachable)   ← socket connect failed
    ├── Err(AtdError::ToolNotFound)        ← describe returned 404
    ├── Err(AtdError::ProtocolError)       ← response frame malformed
    ├── Err(AtdError::Timeout)             ← client-side deadline exceeded
    │
    └── Ok(ToolResult)
            ├── ToolResult::Success { data, metadata }     ← tool ran and succeeded
            └── ToolResult::Error { code, message, .. }    ← tool ran and returned error
```

---

## 2. Full `AtdError` Table

Source: `crates/atd-protocol/src/error.rs`

The enum is `#[non_exhaustive]` — third-party code must handle unknown variants with
a wildcard arm. All 9 variants known at v0.1.0 are listed below.

### 2.1 `ToolNotFound`

```rust
ToolNotFound {
    tool_id: String,
    suggestions: Vec<String>,
}
```

| attribute | value |
|---|---|
| **Trigger** | `describe(tool_id)` or `call(tool_id, ..)` when the server does not know the requested id |
| **is_retryable()** | `false` |
| **suggest_fix()** | `"did you mean '<suggestions[0]>'?"` if suggestions are non-empty; else `"try atd list --query <keyword> to find available tools"` |
| **Source line** | `error.rs:7–10` (variant), `error.rs:65–70` (suggest_fix impl) |

Recovery example:

```rust
match client.describe("ref:fr.read").await {
    Err(AtdError::ToolNotFound { tool_id, suggestions }) => {
        if let Some(fix) = AtdError::ToolNotFound {
            tool_id: tool_id.clone(), suggestions: suggestions.clone()
        }.suggest_fix() {
            eprintln!("Tool not found. {fix}");
        }
        // Re-discover and try suggestions[0] if non-empty
        if let Some(candidate) = suggestions.first() {
            client.describe(candidate).await?;
        }
    }
    Ok(def) => { /* use def */ }
    _ => {}
}
```

### 2.2 `InvalidArguments`

```rust
InvalidArguments {
    tool_id: String,
    field: String,
    reason: String,
}
```

| attribute | value |
|---|---|
| **Trigger** | Client-side pre-validation detects a missing required field or wrong type before sending the request |
| **is_retryable()** | `false` |
| **suggest_fix()** | `None` — the caller must fix the arguments |
| **Source line** | `error.rs:12–17` |

Recovery example:

```rust
match client.call("ref:fs.read", bad_args, opts).await {
    Err(AtdError::InvalidArguments { tool_id, field, reason }) => {
        eprintln!("Bad argument for {tool_id}: field={field} reason={reason}");
        // Fix args and retry
    }
    _ => {}
}
```

### 2.3 `CapabilityDenied`

```rust
CapabilityDenied {
    tool_id: String,
    required: Vec<String>,
    granted: Vec<String>,
}
```

| attribute | value |
|---|---|
| **Trigger** | The server refuses the call because the client lacks required capabilities (SP-12: wire code `1001` / `ERR_CAPABILITY_DENIED`) |
| **Wire mapping** | `Response::Error { code: Some(1001), details: { required, granted, missing } }` |
| **How to grant** | Server operator: `atd-ref-server --grant-capability <name>`. Client: `AtdClient::hello(requested_capabilities)` declares what you want. |
| **is_retryable()** | `false` |
| **suggest_fix()** | `"run atd allow <tool_id> to grant for this session"` |
| **Source line** | `error.rs:19–24` (variant), `error.rs:71–73` (suggest_fix impl); mapping: `client.rs` where server `code == 1001` |

Recovery example:

```rust
match client.call("ref:fs.write", args, opts).await {
    Err(AtdError::CapabilityDenied { tool_id, required, granted }) => {
        eprintln!("Capability denied for {tool_id}.");
        eprintln!("  Required: {required:?}");
        eprintln!("  Granted:  {granted:?}");
        // Capability grants are session-level; prompt user to run `atd allow`
    }
    _ => {}
}
```

### 2.3a `RateLimited` (wire code `1002` / `ERR_RATE_LIMITED`)

| attribute | value |
|---|---|
| **Trigger** | Dispatch refuses the call because the tool's `max_concurrent` permits are exhausted (SP-operability-v1: per-tool `tokio::sync::Semaphore` in `Registry`). The tool is never invoked. |
| **Wire mapping** | `Response::Error { code: Some(1002), message: "rate limited: <tool_id>" }` — emitted by `atd-runtime` when `try_acquire_owned` on the per-tool semaphore returns `TryAcquireError::NoPermits`. |
| **Retryable?** | `true` — the client may retry after a backoff (no permits available *right now*, but permits free as in-flight calls complete). |
| **Source line** | `crates/atd-protocol/src/messages.rs` (constant `ERR_RATE_LIMITED`); `crates/atd-runtime/src/registry.rs` (enforcement); `crates/atd-runtime/src/error.rs` (`ToolCallError::RateLimited`). |
| **Since** | SP-operability-v1 |

Note: in v0.1.0 the SDK does not yet surface this as a dedicated `AtdError::RateLimited` variant; it arrives as a generic wire-error response with `code == 1002`. Client retry code should key off the numeric code.

### 2.4 `BindingUnavailable`

```rust
BindingUnavailable {
    tool_id: String,
    tried: Vec<String>,
    reason: String,
}
```

| attribute | value |
|---|---|
| **Trigger** | The server has a tool registered but none of its declared bindings are reachable (e.g., the CLI executable is missing, the downstream MCP server is down) |
| **is_retryable()** | `true` — the binding may become available after a transient failure |
| **suggest_fix()** | `None` |
| **Source line** | `error.rs:26–31` (variant), `error.rs:57` (is_retryable) |

Recovery example:

```rust
match client.call("ref:shell.exec", args, opts).await {
    Err(ref e @ AtdError::BindingUnavailable { ref tool_id, ref reason, .. }) => {
        eprintln!("Binding unavailable for {tool_id}: {reason}");
        if e.is_retryable() {
            tokio::time::sleep(Duration::from_secs(2)).await;
            // retry
        }
    }
    _ => {}
}
```

### 2.5 `ToolExecutionFailed`

```rust
ToolExecutionFailed {
    tool_id: String,
    inner: Box<dyn std::error::Error + Send + Sync>,
}
```

| attribute | value |
|---|---|
| **Trigger** | The tool panicked or the server returned an unrecognized error response that could not be mapped to `ToolResult::Error` |
| **is_retryable()** | `false` |
| **suggest_fix()** | `None` |
| **Source line** | `error.rs:33–39` |

Note: this variant is rarely produced by the reference server in normal operation.
Well-formed execution failures are returned as `ToolResult::Error` (Layer 2). This
variant fires for unexpected panics or deserialization failures on the client side.

Recovery example:

```rust
match client.call("ref:fs.read", args, opts).await {
    Err(AtdError::ToolExecutionFailed { tool_id, inner }) => {
        eprintln!("Unexpected failure for {tool_id}: {inner}");
        // Log and alert; do not retry automatically
    }
    _ => {}
}
```

### 2.6 `Timeout`

```rust
Timeout { tool_id: String, after_ms: u64 }
```

| attribute | value |
|---|---|
| **Trigger** | The client-side deadline (set via `CallOptions::timeout_ms`) was exceeded before the server returned a result |
| **is_retryable()** | `true` |
| **suggest_fix()** | `"increase timeout or retry; tool_id=<tool_id>"` |
| **Source line** | `error.rs:41` (variant), `error.rs:57–58` (is_retryable), `error.rs:77–79` (suggest_fix) |

Recovery example:

```rust
match client.call("ref:web.fetch", args, CallOptions { timeout_ms: Some(5000), ..Default::default() }).await {
    Err(AtdError::Timeout { tool_id, after_ms }) => {
        eprintln!("Timed out calling {tool_id} after {after_ms}ms");
        // Retry with a longer timeout or back off
    }
    _ => {}
}
```

### 2.7 `ServerUnreachable`

```rust
ServerUnreachable(#[from] std::io::Error)
```

| attribute | value |
|---|---|
| **Trigger** | `connect()` or any subsequent frame read/write fails with an `std::io::Error` (connection refused, socket not found, broken pipe, etc.) |
| **is_retryable()** | `true` |
| **suggest_fix()** | `"is the ANOS daemon running? try anos daemon status"` |
| **Source line** | `error.rs:43–44` (variant with `#[from]`), `error.rs:57` (is_retryable), `error.rs:74–76` (suggest_fix) |

This variant is produced automatically by the Rust `From<std::io::Error>` impl, so any
`?` propagation of an `io::Error` in the client library becomes `ServerUnreachable`.

Recovery example:

```rust
loop {
    match AtdClient::connect(&opts).await {
        Err(AtdError::ServerUnreachable(e)) => {
            eprintln!("Server not reachable: {e}. Retrying in 2s...");
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        Ok(client) => break client,
        Err(other) => return Err(other),
    }
}
```

### 2.8 `NotImplemented`

```rust
NotImplemented { feature: String }
```

| attribute | value |
|---|---|
| **Trigger** | The caller requested a feature or message type that the server explicitly does not support in this version (e.g., `session.start` in v0.1.0) |
| **is_retryable()** | `false` |
| **suggest_fix()** | `None` |
| **Source line** | `error.rs:46–48` |

Recovery example:

```rust
match client.call("ref:fs.read", args, CallOptions { dry_run: true, ..Default::default() }).await {
    Err(AtdError::NotImplemented { feature }) => {
        eprintln!("Feature not supported in this server version: {feature}");
        // Fall back to non-dry-run path or skip
    }
    _ => {}
}
```

### 2.9 `ProtocolError`

```rust
ProtocolError { expected: String, got: String }
```

| attribute | value |
|---|---|
| **Trigger** | The server returned a response frame that could not be deserialized, or the response type did not match what the client sent (e.g., sent `run_tool`, got `tool_list` response) |
| **is_retryable()** | `false` |
| **suggest_fix()** | `None` |
| **Source line** | `error.rs:49–51` |

Recovery example:

```rust
match client.ping().await {
    Err(AtdError::ProtocolError { expected, got }) => {
        eprintln!("Protocol mismatch: expected {expected}, server returned {got}");
        // Likely a version mismatch; check server version
    }
    Ok(()) => { /* liveness confirmed */ }
    _ => {}
}
```

---

## 3. Server-Side Error Codes (`ToolResult::Error.code`)

These are the `code` strings emitted inside `ToolResult::Error` by the reference
server's tool implementations. Enumerated by grepping
`crates/atd-tools-*/src/` for `ExecutionFailed { code:`.

Each code is a `String` in the wire JSON. The `retryable` boolean travels alongside
it. The set of codes is part of the stability surface: changing existing codes is a
breaking change (see `docs/protocol/wire-format.md` §8.2).

### 3.1 Code table

| code | emitting tool(s) | retryable | description |
|---|---|---|---|
| `IO` | `ref:fs.read`, `ref:fs.write`, `ref:fs.edit`, `ref:fs.glob`, `ref:fs.grep`, `ref:shell.exec`, `ref:shell.pwsh`, `ref:web.fetch` | no | Low-level I/O error: OS read/write failure, broken pipe, etc. |
| `IS_DIR` | `ref:fs.read` | no | The path argument resolves to a directory, not a file |
| `ENCODING` | `ref:fs.read`, `ref:fs.edit` | no | File bytes are not valid UTF-8 |
| `NOT_A_DIRECTORY` | `ref:fs.glob`, `ref:fs.grep` | no | The `path` argument is not a directory (or does not exist) |
| `NO_PARENT` | `ref:fs.write` | no | The destination file's parent directory does not exist |
| `NOT_READ` | `ref:fs.edit` | no | `edit` requires a prior `read` of the file (for mtime tracking); the file has not been read yet |
| `FILE_MODIFIED` | `ref:fs.edit` | no | The file was modified externally since the last `read`; edit is rejected to prevent clobbering |
| `NOT_AVAILABLE` | `ref:shell.exec`, `ref:shell.pwsh` | no | The requested shell executable (`sh`, `pwsh`) is not installed or not on PATH |
| `TIMEOUT` | `ref:shell.exec`, `ref:shell.pwsh`, `ref:web.fetch` | yes | The tool's internal execution timeout was exceeded (distinct from the client-side `AtdError::Timeout`) |
| `PRIVATE_ADDRESS_BLOCKED` | `ref:web.fetch` | no | The resolved IP address is in a private/loopback range (SSRF protection) |
| `DNS_FAILED` | `ref:web.fetch` | no | DNS resolution for the given hostname failed |
| `TLS_FAILED` | `ref:web.fetch` | no | TLS handshake or certificate validation failed |
| `TOO_MANY_REDIRECTS` | `ref:web.fetch` | no | The HTTP response chain exceeded the redirect limit |

### 3.2 Code details

#### `IO`

The most common code. Covers all `std::io::Error` conditions not given a more specific
code: file not found, permission denied, disk full, broken pipe, network I/O failure.

Check `message` for the OS error string. Examples:

- `"No such file or directory (os error 2)"`
- `"Permission denied (os error 13)"`
- `"read: connection reset by peer"`

Retryable: `false` for filesystem tools (the file state is unlikely to self-correct);
`false` for web fetch (distinct from `TIMEOUT`).

#### `IS_DIR`

`ref:fs.read` returns this when the given `path` is a directory. Use `ref:fs.glob`
to list directory contents.

```json
{"status": "error", "code": "IS_DIR", "message": "/home/user/docs is a directory", "retryable": false}
```

#### `ENCODING`

The file exists and was read, but its bytes are not valid UTF-8. ATD text tools
(`fs.read`, `fs.edit`) only handle text files.

#### `NOT_A_DIRECTORY`

Both `ref:fs.glob` and `ref:fs.grep` require their `path` argument to be an existing
directory. This code fires when `path` does not exist or resolves to a file.

#### `NO_PARENT`

`ref:fs.write` will not create intermediate directories. The parent directory of the
destination path must already exist.

#### `NOT_READ` and `FILE_MODIFIED`

`ref:fs.edit` implements an optimistic concurrency check. The caller must first call
`ref:fs.read` on the file (which captures the mtime). If the file is subsequently
modified externally before the edit call, `FILE_MODIFIED` is returned. If the caller
skips the read step entirely, `NOT_READ` is returned.

This two-phase pattern prevents silent data loss on concurrent edits.

#### `NOT_AVAILABLE`

`ref:shell.exec` runs via `/bin/sh`; `ref:shell.pwsh` requires PowerShell Core
(`pwsh`). If the required binary is absent from PATH, this code is returned. It is
not retryable because the absence is structural, not transient.

#### `TIMEOUT`

The tool's internal deadline (configured per-tool in the server, typically via
`ToolResources.timeout_ms`) was exceeded during execution. This is distinct from
`AtdError::Timeout`, which is a client-side deadline. Both can fire for the same
request if both deadlines are set.

Retryable: `true` — a retry with the same arguments may succeed (e.g., a temporarily
slow network or a cold-start shell).

#### `PRIVATE_ADDRESS_BLOCKED`

`ref:web.fetch` blocks requests to RFC-1918 addresses (`10.x`, `172.16–31.x`,
`192.168.x`), loopback (`127.x`), and link-local (`169.254.x`) as an SSRF mitigation.
This block applies to the resolved IP address, not the hostname — `localhost` is
blocked even if the caller avoids the word "localhost" in the URL.

Not retryable. Calling the same URL again will produce the same result.

#### `DNS_FAILED`

The hostname in the URL could not be resolved to an IP address. Possible causes:
incorrect hostname, DNS server unreachable, or network partition.

Not retryable in the error code itself; the caller may choose to retry after a delay.

#### `TLS_FAILED`

The TLS handshake failed — certificate validation error, expired cert, or cipher
negotiation failure. The reference server uses the OS TLS stack; the exact error
message comes from the underlying TLS library.

Source: `crates/atd-tools-web/src/fetch.rs`. The check is
heuristic: if the I/O error message contains `"tls"` or `"certificate"`, the code is
`TLS_FAILED`; otherwise `IO`.

#### `TOO_MANY_REDIRECTS`

The HTTP client followed more than the configured redirect limit (default: 10).
Circular redirects or excessively deep redirect chains trigger this.

---

## 4. Retry Decision Tree

Use the following logic to decide whether and how to retry after an error.

### 4.1 Pseudocode

```
function maybe_retry(err, attempt, max_attempts=3):

    if attempt >= max_attempts:
        FAIL "max retries exceeded"

    # Layer 1: AtdError
    if err is AtdError:
        if err.is_retryable() == false:
            FAIL immediately with err

        # Retryable AtdErrors: Timeout, ServerUnreachable, BindingUnavailable
        base_delay_ms = 500
        cap_ms        = 30_000
        jitter_ms     = random(0, 500)

        delay_ms = min(base_delay_ms * 2^attempt + jitter_ms, cap_ms)
        sleep(delay_ms)
        RETRY

    # Layer 2: ToolResult::Error
    if err is ToolResult::Error:
        if err.retryable == false:
            FAIL immediately with err

        # Only TIMEOUT has retryable=true in the reference server
        delay_ms = min(1000 * 2^attempt + random(0, 500), 30_000)
        sleep(delay_ms)
        RETRY
```

### 4.2 Per-error retry strategy

| error | retry? | recommended strategy |
|---|---|---|
| `AtdError::ServerUnreachable` | yes | Exponential backoff, up to 30s cap. Check server health. |
| `AtdError::Timeout` | yes | Increase `timeout_ms` per-attempt, or backoff and retry same timeout. |
| `AtdError::BindingUnavailable` | yes | Short backoff (2–5s); may resolve if transient. |
| `AtdError::ToolNotFound` | no | Check `suggestions` field; call `discover()` to refresh list. |
| `AtdError::InvalidArguments` | no | Fix the arguments. |
| `AtdError::CapabilityDenied` | no | Grant the capability or use a different tool. |
| `AtdError::ToolExecutionFailed` | no | Log and alert; likely a server-side bug. |
| `AtdError::NotImplemented` | no | Remove the unsupported call. |
| `AtdError::ProtocolError` | no | Check client/server version compatibility. |
| `ToolResult::Error { code: "TIMEOUT" }` | yes | Backoff; server-side timeout is transient. |
| `ToolResult::Error { code: "IO" }` | no | OS I/O errors are typically not transient. |
| `ToolResult::Error { code: "DNS_FAILED" }` | maybe | Retry after delay; DNS may recover. |
| All other `ToolResult::Error` codes | no | Structural; fix the call arguments or server state. |

### 4.3 Rust retry wrapper example

```rust
use atd_protocol::{AtdError, ToolResult};
use atd_sdk::{AtdClient, CallOptions};
use std::time::Duration;
use tokio::time::sleep;

async fn call_with_retry(
    client: &AtdClient,
    tool_id: &str,
    args: serde_json::Value,
    max_attempts: u32,
) -> Result<ToolResult, AtdError> {
    let mut attempt = 0u32;
    loop {
        match client.call(tool_id, args.clone(), CallOptions::default()).await {
            Err(ref e) if e.is_retryable() && attempt < max_attempts => {
                let base = 500u64 * 2u64.pow(attempt);
                let jitter = rand::random::<u64>() % 500;
                let delay = Duration::from_millis((base + jitter).min(30_000));
                eprintln!("Retryable error (attempt {}): {e}. Retrying in {delay:?}", attempt + 1);
                sleep(delay).await;
                attempt += 1;
            }
            Ok(ToolResult::Error { retryable: true, ref code, .. }) if attempt < max_attempts => {
                let base = 1000u64 * 2u64.pow(attempt);
                let jitter = rand::random::<u64>() % 500;
                let delay = Duration::from_millis((base + jitter).min(30_000));
                eprintln!("Retryable tool error [{code}] (attempt {}). Retrying in {delay:?}", attempt + 1);
                sleep(delay).await;
                attempt += 1;
            }
            result => return result,
        }
    }
}
```

---

## 5. Debugging Errors

### 5.1 Enable wire-level logging

The reference client uses the `tracing` crate. Set `RUST_LOG` before running your
binary to see request/response frames:

```bash
RUST_LOG=atd_sdk=debug cargo run --example hello_atd -p atd-examples
```

For full frame content (including JSON bodies), use `trace` level:

```bash
RUST_LOG=atd_sdk=trace cargo run --example hello_atd -p atd-examples
```

### 5.2 Server-side logs

The reference server logs to stderr. If you launched it via `AtdClient::connect()`
(which auto-spawns `atd-ref-server`), its stderr is attached to your process's stderr.

For a manually launched server:

```bash
RUST_LOG=atd_ref_server_bin=debug ./target/release/atd-ref-server --socket /tmp/atd.sock
```

Relevant log fields:

| field | meaning |
|---|---|
| `tool_id` | Which tool was called |
| `elapsed_ms` | Server-side execution time |
| `code` | The `ToolResult::Error.code` if the tool failed |
| `retryable` | Whether the server marked the failure retryable |

### 5.3 Correlating client error with server log

In v0.1.0 there is no `request_id` in the request frame (it is only in
`ToolResultMetadata` when the server populates it). To correlate:

1. Find the `tool_id` in the client error.
2. Search server logs for the same `tool_id` around the same timestamp.
3. Check `elapsed_ms` — if it matches `AtdError::Timeout.after_ms`, the server did
   receive the request but was slow.

### 5.4 Common gotchas

**`AtdError::ServerUnreachable(Connection refused)`**

The socket file does not exist or the server has not started yet. Check:

```bash
ls -la /tmp/atd.sock          # Does the socket exist?
ps aux | grep atd-ref-server  # Is the server process running?
```

**`AtdError::ProtocolError { expected: "pong", got: "..." }`**

The server returned an unexpected response to `ping`. Likely a version mismatch —
check that `atd-sdk` and `atd-ref-server` are built from the same commit.

**`ToolResult::Error { code: "IS_DIR" }` from `ref:fs.read`**

You passed a directory path to `ref:fs.read`. Use `ref:fs.glob` to list directory
contents and `ref:fs.read` for individual files.

**`ToolResult::Error { code: "NOT_READ" }` from `ref:fs.edit`**

`ref:fs.edit` requires a prior `ref:fs.read` call on the same path. Read the file
first to establish the baseline mtime, then call edit.

**Sanitized name mismatch in LLM traces**

LLM agent frameworks (LangChain, MCP) show the sanitized tool name (`ref_fs_read`),
not the ATD id (`ref:fs.read`). When a tool call fails, grep the agent trace for
the sanitized name and map it back using the `desanitize_tool_name` function or the
`_atd.tool_id` field in the MCP tool description.

**`ToolResult::Error { code: "PRIVATE_ADDRESS_BLOCKED" }` from `ref:web.fetch`**

You tried to fetch a private or loopback address. This is intentional SSRF
protection. If you need to test with a local server, start `atd-ref-server` with
SSRF protection disabled (a compile-time feature flag, not yet exposed in v0.1.0).

**`AtdError::Timeout` but the server log shows no `elapsed_ms` entry**

The client-side deadline fired before the server even received the request (e.g., the
server was under load and the socket accept queue was full). Increase the client
timeout or reduce request rate.

---

*End of error codes reference. See `docs/protocol/wire-format.md` for the full protocol framing and type definitions.*
