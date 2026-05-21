# atd-ref-server SP-6 Capstone Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut the ANOS dependency from the default demo path. Rewrite `examples/hello_atd.{rs,py}` to auto-spawn `atd-ref-server` and exercise 3 representative tools. Ship validation doc + updated README + `sp6-ref-server-capstone` tag.

**Architecture:** The Rust example now spawns `target/release/atd-ref-server --sock <tempdir>/demo.sock` as a `tokio::process::Child`, polls for socket readiness (100ms × 30 tries), connects via `atd-client`, exercises 3 tools (`ref:echo.say`, `ref:fs.glob`, `ref:shell.exec`), then kills the child and drops the tempdir on exit. Python parallel: `asyncio.create_subprocess_exec` + `tempfile.TemporaryDirectory`. Both honor `ATD_SOCK` env override (skip spawn, connect to the given path — enables demoing against ANOS or any other ATD server).

**Tech Stack:** No new crates. `examples/Cargo.toml` already has `atd-client`, `atd-types`, `tokio`, `serde_json`. Add `tempfile` as a workspace dep (already in the lockfile via other crates). Python: stdlib only (`asyncio`, `tempfile`, `pathlib`, `signal`, `subprocess`).

**Spec:** `docs/superpowers/specs/2026-04-23-atd-ref-server-sp6-capstone.md`

**Scope boundary:**
- **In:** rewrite of 2 example files; new validation doc; README + CLAUDE.md updates; tag.
- **Out:** demo video (human task); public release; conformance suite; cross-OS testing.

**Prerequisites:**
- `sp5-ref-server-web` tag, 243 workspace tests green.
- `cargo build --release -p atd-ref-server` has been run (the examples assume the binary exists at `target/release/atd-ref-server`).

**Exit criteria:**
1. `cargo build --release -p atd-ref-server && cargo run --example hello_atd` — runs end-to-end, prints 3 tool outputs, exits 0.
2. `cargo build --release -p atd-ref-server && uv run python examples/hello_atd.py` — same.
3. `ATD_SOCK=/tmp/x.sock cargo run --example hello_atd` against a pre-existing socket works — connects without spawning.
4. `cargo test --workspace --all-targets` stays at 243 tests, 0 failures.
5. `docs/validation/2026-04-23-sp6-capstone.md` committed with all sections filled.
6. README.md + CLAUDE.md updated — no remaining "demo depends on ANOS" language.
7. Tag `sp6-ref-server-capstone` created.
8. `grep -r -l 'anos\|ANOS' examples/hello_atd.rs python/examples/hello_atd.py` returns empty.

---

## File Structure

```
/
├── examples/
│   ├── Cargo.toml                         (MODIFY — add tempfile dev-dep)
│   └── hello_atd.rs                       (REWRITE — Task 1)
├── python/examples/
│   └── hello_atd.py                       (REWRITE — Task 2)
├── docs/validation/
│   └── 2026-04-23-sp6-capstone.md         (NEW — Task 3)
├── README.md                               (MODIFY — Task 4)
└── CLAUDE.md                               (MODIFY — Task 4)
```

---

## Task 1: Rewrite `examples/hello_atd.rs`

**Files:**
- Modify: `/home/nan/proj/atd-mvp/examples/Cargo.toml`
- Rewrite: `/home/nan/proj/atd-mvp/examples/hello_atd.rs`

- [ ] **Step 1.1: Add `tempfile` dep**

Edit `/home/nan/proj/atd-mvp/examples/Cargo.toml`. In `[dependencies]`, append:

```toml
tempfile = { workspace = true }
```

Verify `tempfile` is already in `[workspace.dependencies]` at the repo root. (It is — used by atd-ref-server's dev-deps.) If it's NOT, add it as `tempfile = "3"` at the workspace level first.

- [ ] **Step 1.2: Rewrite `hello_atd.rs`**

Replace `/home/nan/proj/atd-mvp/examples/hello_atd.rs` ENTIRELY with:

```rust
//! atd-mvp capstone demo. Auto-spawns `atd-ref-server` (the in-repo neutral
//! reference ATD server), connects via `atd-client`, exercises three
//! representative tools end-to-end.
//!
//! This demo has ZERO dependency on ANOS. It proves the ATD protocol is
//! vendor-neutral: the client speaks the wire format, the ref-server answers.
//!
//! Run:
//!   cargo build --release -p atd-ref-server
//!   cargo run --example hello_atd
//!
//! Override the server (e.g., to demo against ANOS):
//!   ATD_SOCK=~/.anos/anos.sock cargo run --example hello_atd

use std::path::PathBuf;
use std::time::Duration;

use atd_client::{AtdClient, CallOptions, DiscoverFilter, Endpoint};
use tokio::process::{Child, Command};

const SOCKET_WAIT_ATTEMPTS: u32 = 30;
const SOCKET_WAIT_INTERVAL_MS: u64 = 100;

/// Walk up from this example's manifest directory to find the workspace root.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples/ should have a parent")
        .to_path_buf()
}

async fn wait_for_socket(sock: &std::path::Path) -> bool {
    for _ in 0..SOCKET_WAIT_ATTEMPTS {
        if sock.exists() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(SOCKET_WAIT_INTERVAL_MS)).await;
    }
    false
}

/// Either return the externally-specified socket, or spawn ref-server with a
/// temp socket. Returns (child_process, tempdir_guard, socket_path). The
/// tempdir_guard keeps the temp directory alive — drop it and the socket
/// file is cleaned up.
async fn acquire_server() -> Result<
    (Option<Child>, Option<tempfile::TempDir>, PathBuf),
    Box<dyn std::error::Error>,
> {
    if let Ok(override_sock) = std::env::var("ATD_SOCK") {
        let sock = PathBuf::from(override_sock);
        println!("[atd] using ATD_SOCK override → {}", sock.display());
        return Ok((None, None, sock));
    }

    let binary = repo_root().join("target/release/atd-ref-server");
    if !binary.exists() {
        return Err(format!(
            "atd-ref-server release binary not found at {}.\n\
             build it first: cargo build --release -p atd-ref-server",
            binary.display()
        )
        .into());
    }

    let tmp = tempfile::tempdir()?;
    let sock = tmp.path().join("demo.sock");
    println!(
        "[atd] auto-spawning atd-ref-server → {}",
        sock.display()
    );
    let child = Command::new(&binary)
        .arg("--sock")
        .arg(&sock)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    if !wait_for_socket(&sock).await {
        return Err("ref-server didn't bind its socket within 3s".into());
    }

    Ok((Some(child), Some(tmp), sock))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut child, _tmpdir, sock) = acquire_server().await?;

    // Ensure we clean up on any early exit path.
    let run = async {
        let client = AtdClient::connect(Endpoint::unix(sock.clone())).await?;
        println!("[atd] connected");

        let all = client
            .discover(None, DiscoverFilter::default())
            .await?;
        println!("[atd] {} tools registered", all.len());

        // 1/3 — echo.say
        println!();
        println!("[1/3] ref:echo.say {{\"text\":\"hello from ATD\"}}");
        let r = client
            .call(
                "ref:echo.say",
                serde_json::json!({"text": "hello from ATD"}),
                CallOptions {
                    dry_run: false,
                    preferred_binding: None,
                },
            )
            .await?;
        print_result(r)?;

        // 2/3 — fs.glob (find Cargo manifests)
        println!();
        println!("[2/3] ref:fs.glob {{\"pattern\":\"**/*.toml\",\"path\":\".\"}}");
        let r = client
            .call(
                "ref:fs.glob",
                serde_json::json!({"pattern": "**/*.toml", "path": "."}),
                CallOptions {
                    dry_run: false,
                    preferred_binding: None,
                },
            )
            .await?;
        print_glob_result(r)?;

        // 3/3 — shell.exec (platform identity)
        println!();
        println!("[3/3] ref:shell.exec {{\"command\":\"uname -s\"}}");
        let r = client
            .call(
                "ref:shell.exec",
                serde_json::json!({"command": "uname -s"}),
                CallOptions {
                    dry_run: false,
                    preferred_binding: None,
                },
            )
            .await?;
        print_shell_result(r)?;

        println!();
        println!("[atd] done.");
        Ok::<_, Box<dyn std::error::Error>>(())
    };

    let outcome = run.await;

    // Teardown
    if let Some(c) = child.as_mut() {
        let _ = c.kill().await;
        let _ = c.wait().await;
    }

    outcome
}

fn print_result(r: atd_types::ToolResult) -> Result<(), Box<dyn std::error::Error>> {
    match r {
        atd_types::ToolResult::Success { data, .. } => {
            println!("      → {}", serde_json::to_string(&data)?);
        }
        atd_types::ToolResult::Error { code, message, .. } => {
            println!("      ✗ {code}: {message}");
        }
    }
    Ok(())
}

fn print_glob_result(r: atd_types::ToolResult) -> Result<(), Box<dyn std::error::Error>> {
    match r {
        atd_types::ToolResult::Success { data, .. } => {
            let paths = data["paths"].as_array().cloned().unwrap_or_default();
            let preview: Vec<String> = paths
                .iter()
                .take(3)
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
            let suffix = if paths.len() > 3 {
                format!(" (+{} more)", paths.len() - 3)
            } else {
                String::new()
            };
            println!("      → {} paths: {}{}", paths.len(), preview.join(", "), suffix);
        }
        atd_types::ToolResult::Error { code, message, .. } => {
            println!("      ✗ {code}: {message}");
        }
    }
    Ok(())
}

fn print_shell_result(r: atd_types::ToolResult) -> Result<(), Box<dyn std::error::Error>> {
    match r {
        atd_types::ToolResult::Success { data, .. } => {
            let exit = data["exit_code"].as_i64().unwrap_or(-1);
            let stdout = data["stdout"].as_str().unwrap_or("").trim();
            println!("      → exit {exit}, stdout={stdout:?}");
        }
        atd_types::ToolResult::Error { code, message, .. } => {
            println!("      ✗ {code}: {message}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_for_socket_returns_true_when_file_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("ready.sock");
        tokio::fs::File::create(&sock).await.unwrap();
        assert!(wait_for_socket(&sock).await);
    }

    #[tokio::test]
    async fn wait_for_socket_returns_false_on_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("never.sock");
        // Using the real constants would take 3s. Verify quickly by checking
        // the function honors nonexistence across at least a few cycles.
        // We test the real helper end-to-end in integration anyway.
        let start = std::time::Instant::now();
        let got = wait_for_socket(&sock).await;
        let elapsed = start.elapsed();
        assert!(!got);
        assert!(
            elapsed >= Duration::from_millis(SOCKET_WAIT_INTERVAL_MS * 2),
            "should have polled at least twice: {elapsed:?}"
        );
    }
}
```

**Notes:**
- `tempfile::tempdir()` lifetime is bound to the `TempDir` value in main's scope. Holding it in `_tmpdir` keeps the directory alive until `main` returns; its `Drop` removes the directory (and the socket inside).
- `Child::kill().await` sends SIGKILL. A brief `child.wait()` after reaps the zombie.
- `stdout/stderr: Stdio::null()` suppresses ref-server's own boot log — the example's own prints are the only thing on stdout.
- `discover(None, DiscoverFilter::default())` with no limit returns all 9 tools — the "9 tools registered" line comes from `all.len()`, not a hard-coded constant.

- [ ] **Step 1.3: Build check**

```bash
cd /home/nan/proj/atd-mvp
cargo build --release -p atd-ref-server
cargo build --example hello_atd -p atd-examples
```

Both must succeed with no warnings.

- [ ] **Step 1.4: Unit test**

```bash
cargo test --example hello_atd -p atd-examples
```

Expected: 2 tests pass (wait_for_socket_returns_true_when_file_exists + wait_for_socket_returns_false_on_timeout).

- [ ] **Step 1.5: End-to-end**

```bash
cd /home/nan/proj/atd-mvp
cargo run --example hello_atd -p atd-examples
```

Expected output (roughly):
```
[atd] auto-spawning atd-ref-server → /tmp/.../demo.sock
[atd] connected
[atd] 9 tools registered

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"call_id":"...","echoed":"hello from ATD"}

[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → N paths: Cargo.toml, crates/atd-cli/Cargo.toml, crates/atd-client/Cargo.toml (+N more)

[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout="Linux"

[atd] done.
```

Exit code must be 0.

- [ ] **Step 1.6: Workspace regression**

```bash
cargo test --workspace --all-targets
```

Expected: 243 tests, 0 failures (examples aren't in the workspace `--all-targets` default test scope).

- [ ] **Step 1.7: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add examples/Cargo.toml examples/hello_atd.rs
git commit -m "feat(examples): rewrite hello_atd.rs to auto-spawn atd-ref-server"
```

---

## Task 2: Rewrite `python/examples/hello_atd.py`

**Files:**
- Rewrite: `/home/nan/proj/atd-mvp/python/examples/hello_atd.py`

- [ ] **Step 2.1: Rewrite the file**

Replace `/home/nan/proj/atd-mvp/python/examples/hello_atd.py` ENTIRELY with:

```python
"""atd-mvp capstone demo (Python SDK).

Auto-spawns `atd-ref-server` (the in-repo neutral reference ATD server),
connects via the Python `atd_client` SDK, exercises three representative
tools end-to-end.

This demo has ZERO dependency on ANOS. It proves the ATD protocol is
vendor-neutral: the SDK speaks the wire format, the ref-server answers.

Run:
    cargo build --release -p atd-ref-server
    uv run python examples/hello_atd.py

Override the server (e.g., to demo against ANOS):
    ATD_SOCK=~/.anos/anos.sock uv run python examples/hello_atd.py
"""

from __future__ import annotations

import asyncio
import json
import os
import signal
import sys
import tempfile
from contextlib import asynccontextmanager
from pathlib import Path
from typing import AsyncIterator

from atd_client import AtdClient, ToolFailure, ToolSuccess

SOCKET_WAIT_ATTEMPTS = 30
SOCKET_WAIT_INTERVAL_S = 0.1


def repo_root() -> Path:
    # This file lives at <root>/python/examples/hello_atd.py
    return Path(__file__).resolve().parent.parent.parent


async def _wait_for_socket(sock: Path) -> bool:
    for _ in range(SOCKET_WAIT_ATTEMPTS):
        if sock.exists():
            return True
        await asyncio.sleep(SOCKET_WAIT_INTERVAL_S)
    return False


@asynccontextmanager
async def acquire_server() -> AsyncIterator[Path]:
    """Yield a Unix socket path pointing at a usable atd-ref-server.

    If ATD_SOCK is set, assume a server is already running there.
    Otherwise, spawn one (from target/release/atd-ref-server) into a
    tempdir and tear it down at exit.
    """
    override = os.environ.get("ATD_SOCK")
    if override:
        print(f"[atd] using ATD_SOCK override → {override}")
        yield Path(override)
        return

    binary = repo_root() / "target" / "release" / "atd-ref-server"
    if not binary.exists():
        raise RuntimeError(
            f"atd-ref-server release binary not found at {binary}.\n"
            "build it first: cargo build --release -p atd-ref-server"
        )

    with tempfile.TemporaryDirectory() as td:
        sock = Path(td) / "demo.sock"
        print(f"[atd] auto-spawning atd-ref-server → {sock}")
        proc = await asyncio.create_subprocess_exec(
            str(binary),
            "--sock",
            str(sock),
            stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.DEVNULL,
        )
        try:
            if not await _wait_for_socket(sock):
                raise RuntimeError("ref-server didn't bind its socket within 3s")
            yield sock
        finally:
            if proc.returncode is None:
                proc.send_signal(signal.SIGTERM)
                try:
                    await asyncio.wait_for(proc.wait(), timeout=2.0)
                except asyncio.TimeoutError:
                    proc.kill()
                    await proc.wait()


def _print_echo(result: ToolSuccess | ToolFailure) -> None:
    if isinstance(result, ToolSuccess):
        print(f"      → {json.dumps(result.data)}")
    else:
        print(f"      ✗ {result.code}: {result.message}")


def _print_glob(result: ToolSuccess | ToolFailure) -> None:
    if isinstance(result, ToolFailure):
        print(f"      ✗ {result.code}: {result.message}")
        return
    paths = result.data.get("paths", [])
    preview = paths[:3]
    suffix = f" (+{len(paths) - 3} more)" if len(paths) > 3 else ""
    print(f"      → {len(paths)} paths: {', '.join(preview)}{suffix}")


def _print_shell(result: ToolSuccess | ToolFailure) -> None:
    if isinstance(result, ToolFailure):
        print(f"      ✗ {result.code}: {result.message}")
        return
    exit_code = result.data.get("exit_code")
    stdout = result.data.get("stdout", "").rstrip()
    print(f"      → exit {exit_code}, stdout={stdout!r}")


async def main() -> int:
    async with acquire_server() as sock:
        async with await AtdClient.connect(sock) as client:
            print("[atd] connected")

            tools = await client.discover(limit=None)
            print(f"[atd] {len(tools)} tools registered")

            print()
            print('[1/3] ref:echo.say {"text":"hello from ATD"}')
            r = await client.call(
                "ref:echo.say",
                {"text": "hello from ATD"},
                dry_run=False,
            )
            _print_echo(r)

            print()
            print('[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}')
            r = await client.call(
                "ref:fs.glob",
                {"pattern": "**/*.toml", "path": "."},
                dry_run=False,
            )
            _print_glob(r)

            print()
            print('[3/3] ref:shell.exec {"command":"uname -s"}')
            r = await client.call(
                "ref:shell.exec",
                {"command": "uname -s"},
                dry_run=False,
            )
            _print_shell(r)

            print()
            print("[atd] done.")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(asyncio.run(main()))
    except KeyboardInterrupt:
        sys.exit(130)
```

**Notes:**
- `asyncio.create_subprocess_exec` with `stdout/stderr=DEVNULL` keeps the ref-server silent.
- `tempfile.TemporaryDirectory` is a context manager — its `__exit__` cleans up at block exit.
- `send_signal(SIGTERM)` is the grace path; `kill()` after a 2s timeout is the force path. This mirrors the SP-3 subprocess handler pattern.
- The outer `try/except KeyboardInterrupt` returns 130 (POSIX convention).

- [ ] **Step 2.2: Verify the client SDK supports `limit=None`**

Check `/home/nan/proj/atd-mvp/python/atd_client/` (or wherever the Python SDK lives). The `discover(limit=None)` call must return all tools, not error.

If `limit=None` isn't supported, either:
- Use `limit=100` (tool count is 9 today; won't hit the cap)
- Or update the SDK signature

Do NOT silently guess. If limit behavior differs, STOP and ask.

- [ ] **Step 2.3: Run the demo**

```bash
cd /home/nan/proj/atd-mvp
uv run python python/examples/hello_atd.py
```

Expected output parallel to the Rust version. Exit code 0.

If `uv run` fails because the environment isn't set up, check `python/pyproject.toml` or use `python -m pip install -e ./python` first.

- [ ] **Step 2.4: ATD_SOCK override test**

Quick sanity check: run the Rust version first to spawn a ref-server, Ctrl-C so the socket stays (won't — tempdir cleans up). Alternative: spawn ref-server manually, point both examples at it:

```bash
./target/release/atd-ref-server --sock /tmp/sp6-check.sock &
SRV=$!
sleep 1
ATD_SOCK=/tmp/sp6-check.sock cargo run --example hello_atd -p atd-examples
ATD_SOCK=/tmp/sp6-check.sock uv run python python/examples/hello_atd.py
kill $SRV
rm -f /tmp/sp6-check.sock
```

Both must produce the same 3-tool output using the shared socket.

- [ ] **Step 2.5: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add python/examples/hello_atd.py
git commit -m "feat(examples): rewrite hello_atd.py to auto-spawn atd-ref-server"
```

---

## Task 3: Validation doc

**Files:**
- Create: `/home/nan/proj/atd-mvp/docs/validation/2026-04-23-sp6-capstone.md`

- [ ] **Step 3.1: Capture transcripts**

Before writing the doc, capture fresh transcripts.

```bash
cd /home/nan/proj/atd-mvp
cargo build --release -p atd-ref-server
```

Rust transcript:
```bash
cargo run --example hello_atd -p atd-examples 2>&1 | tee /tmp/sp6-rust.log
```

Python transcript:
```bash
uv run python python/examples/hello_atd.py 2>&1 | tee /tmp/sp6-py.log
```

Dependency tree:
```bash
cargo tree -p atd-ref-server --prefix none 2>/dev/null | head -30 > /tmp/sp6-tree.log
```

License summary:
```bash
# If cargo-license is installed:
cargo license -p atd-ref-server 2>/dev/null | head -50 > /tmp/sp6-license.log
# Otherwise fall back to an inline list (document the fallback path in the doc)
```

Example diff:
```bash
git show sp5-ref-server-web -- examples/hello_atd.rs > /tmp/sp6-before.txt
```
Or equivalently: `git log -p -- examples/hello_atd.rs | head -200`.

- [ ] **Step 3.2: Write the validation doc**

Create `/home/nan/proj/atd-mvp/docs/validation/2026-04-23-sp6-capstone.md`:

````markdown
# SP-6 Capstone Validation

**Date:** 2026-04-23
**Tag:** `sp6-ref-server-capstone`
**Status:** Evidence-based claim — atd-mvp is the independent reference implementation of the ATD protocol with zero runtime dependency on ANOS.

---

## 1. Claim

`atd-mvp` positions itself (per `CLAUDE.md`) as *"the independent reference implementation of the ATD protocol and client SDK"*, intentionally separate from the ANOS project. Through SP-1 to SP-5 we built `atd-ref-server` — a clean-room, Apache-2.0-licensed, 9-tool reference server with 243 tests, full SSRF defense, and zero `anos-*` dependencies.

This document shows that claim working end-to-end. The same `hello_atd` example that previously required a running ANOS daemon now runs against our own reference server, in-repo, with a single `cargo run` command. The ATD protocol proves itself.

## 2. Evidence 1 — Rust end-to-end

Command:
```bash
cargo build --release -p atd-ref-server
cargo run --example hello_atd -p atd-examples
```

Captured output:

```
<paste contents of /tmp/sp6-rust.log>
```

Commentary:
- **Boot line.** `atd-ref-server` is spawned by the example into a tempdir socket. No user-managed daemon, no `ANOS_SOCK` env var, no global state touched.
- **`ref:echo.say`.** Deterministic call with a string argument. Proves request framing, JSON (de)serialization, call routing.
- **`ref:fs.glob`.** Real directory walk over the atd-mvp repo itself. Proves `ignore::Walk` + `globset` integration and `.gitignore` honoring — the returned list excludes `target/` entries automatically.
- **`ref:shell.exec`.** Real subprocess output from `uname -s`. Proves subprocess spawn, stdout capture, exit-code pass-through.

## 3. Evidence 2 — Python end-to-end

Command:
```bash
uv run python python/examples/hello_atd.py
```

Captured output:

```
<paste contents of /tmp/sp6-py.log>
```

Commentary: structurally identical to the Rust output. Different `call_id` values (ULID) across runs are expected. The Python SDK uses the same wire protocol; the ref-server answers the same way.

## 4. Evidence 3 — Dependency isolation

```bash
cargo tree -p atd-ref-server --prefix none | head -30
```

```
<paste contents of /tmp/sp6-tree.log>
```

None of the following appear anywhere in the tree:
- `anos-*` (any ANOS crate)
- `atd-client` (the client SDK — server doesn't depend on its own client)
- `atd-mcp-bridge`
- `atd-cli`

All direct deps are neutral infrastructure: tokio, serde, reqwest, rustls, hyper, ignore, grep-*, globset, html5ever, htmd. None protocol-coupling.

## 5. Evidence 4 — License audit

```
<paste output of cargo license or equivalent — license table>
```

All direct and transitive dependencies fall under MIT, Apache-2.0, BSD-*, ISC, or dual MIT/Apache-2.0. The GPL-3.0+ contamination from html2md (flagged and fixed in SP-5, commit `3ed261d`) is gone. `atd-ref-server` can be distributed as Apache-2.0.

## 6. Evidence 5 — Example diff

Before (pre-SP-6):

```rust
<paste relevant 20-30 lines from the old hello_atd.rs — the ANOS_SOCK +
Endpoint::default_anos() part>
```

After (current):

```rust
<paste the acquire_server helper + the 3-tool exercise — ~40 lines>
```

Net: `ANOS_SOCK` and `Endpoint::default_anos()` are gone. `ATD_SOCK` replaces them as a neutral override. The default path is `atd-ref-server` — a peer, not a dependency.

## 7. What remains for Phase 2+

- **Demo video** — a 90-second screen capture of `cargo run --example hello_atd` from a fresh clone, for the project README and the eventual public announcement.
- **Conformance suite** — a protocol-level test harness that validates third-party server implementations against the ATD wire protocol (tracked in `docs/design.md` §7).
- **Public release** — push `atd-mvp` to `github.com/atd-protocol/atd-mvp`, announce to partner stakeholders.
- **Cross-OS validation** — verify the capstone demo on macOS and Windows. Current SP only tested Linux.

These are downstream of the code. The code itself is capstone-complete.
````

(In the actual file, substitute `<paste ...>` blocks with the real captured content.)

- [ ] **Step 3.3: Fill in the placeholders**

Open the file you just wrote. Replace each `<paste ...>` block with the real captured content from Step 3.1. Be honest about imperfections — if `cargo license` isn't installed, note the fallback method. If a transcript line looks wrong, flag it in commentary.

- [ ] **Step 3.4: Commit**

```bash
cd /home/nan/proj/atd-mvp
git add docs/validation/2026-04-23-sp6-capstone.md
git commit -m "docs(validation): SP-6 capstone — atd-mvp independence proof"
```

---

## Task 4: README + CLAUDE.md + tag

**Files:**
- Modify: `/home/nan/proj/atd-mvp/README.md`
- Modify: `/home/nan/proj/atd-mvp/CLAUDE.md`

- [ ] **Step 4.1: Update root `README.md`**

Read `/home/nan/proj/atd-mvp/README.md` first. Scan for any text along the lines of "requires ANOS daemon running" or "set ANOS_SOCK=...". Replace with capstone-compatible copy. Specifically:

**(a)** If there's a "Quick start" section, its first example should be:
```bash
cargo build --release -p atd-ref-server
cargo run --example hello_atd
```

NOT:
```bash
ANOS_SOCK=... cargo run --example hello_atd
```

**(b)** Add a short subsection (or update an existing one) titled "Capstone demo — proving independence":

```markdown
### Capstone demo — proving independence

`atd-mvp` ships its own reference server (`atd-ref-server`) and uses it
for the `hello_atd` demos. The Rust example auto-spawns the ref-server
as a child process, exercises three real tools (`ref:echo.say`,
`ref:fs.glob`, `ref:shell.exec`), then cleans up. No ANOS daemon needed.

```bash
cargo build --release -p atd-ref-server
cargo run --example hello_atd           # Rust
uv run python python/examples/hello_atd.py   # Python
```

Want to demo against a different ATD server (ANOS or otherwise)? Set
`ATD_SOCK=/path/to/socket` — the demo skips the spawn and connects to
your chosen socket instead. Same client, same SDK, same output.

For full evidence of independence, see
[`docs/validation/2026-04-23-sp6-capstone.md`](docs/validation/2026-04-23-sp6-capstone.md).
```

If README has sections on "Project state" / "Phase 0 scope", skim them and adjust any language implying the reference server is hypothetical or in-flight. It's shipped.

- [ ] **Step 4.2: Update `CLAUDE.md`**

In `/home/nan/proj/atd-mvp/CLAUDE.md`, two adjustments:

**(a)** The "Project state" section opens with:
```
**Pre-implementation.** Design approved on 2026-04-21 through a brainstorming session in the ANOS project. No code has been written yet.
```

Replace with:

```
**SP-6 capstone complete.** atd-ref-server ships with 9 tools across 4 domains (echo, fs, shell, web), 243 workspace tests, and a `hello_atd.{rs,py}` demo that auto-spawns the ref-server — zero ANOS dependency in the default path. Tag: `sp6-ref-server-capstone`.
```

**(b)** In the "Relationship to ANOS" section, find:
```
**Reference server (Phase 0/1):** The ANOS daemon at `/home/nan/proj/anos/` implements the ATD dispatch pipeline and serves as the server-side reference during early development. No ANOS code changes are needed for atd-client to talk to it via Unix socket.
```

Replace with:

```
**Reference server:** `crates/atd-ref-server` is atd-mvp's own neutral reference ATD server, shipped via SP-1 through SP-5 (tags `sp1-ref-server-foundation` through `sp5-ref-server-web`). The `hello_atd` demos run against it by default. ANOS is still a valid server to speak to — set `ATD_SOCK=~/.anos/anos.sock` on any demo to demo against ANOS instead. Both backends speak the same wire protocol; that's the point.
```

**(c)** In "Phase 0 scope (hard boundary)", the bullet:
```
- 1 demo only: LangChain agent calling an ATD tool through the ANOS daemon
```

Update to:
```
- Phase 0 demo: capstone `hello_atd` exercising atd-ref-server — three tools, two language SDKs, zero ANOS dependency
```

- [ ] **Step 4.3: Final regression**

```bash
cd /home/nan/proj/atd-mvp
cargo build --release -p atd-ref-server
cargo test --workspace --all-targets
```

Expected: 243 tests pass. Binary builds clean.

Final grep check (exit criterion 8):
```bash
grep -l 'anos\|ANOS' examples/hello_atd.rs python/examples/hello_atd.py
# Expected: no output (empty)
```

- [ ] **Step 4.4: Commit + tag**

```bash
cd /home/nan/proj/atd-mvp
git add README.md CLAUDE.md
git commit -m "docs: update README + CLAUDE.md for SP-6 capstone"

git tag -a sp6-ref-server-capstone \
  -m "SP-6: atd-ref-server capstone — hello_atd demos auto-spawn the ref-server, zero ANOS dependency"
git log --oneline | head -10
git tag | grep sp6
```

---

## Post-Plan Verification Checklist

- [ ] `cargo build --release -p atd-ref-server && cargo run --example hello_atd` — runs clean, 3 tool outputs, exit 0
- [ ] `uv run python python/examples/hello_atd.py` — parallel
- [ ] `ATD_SOCK=...` override tested — connects without spawning
- [ ] `cargo test --workspace --all-targets` — 243 tests
- [ ] `docs/validation/2026-04-23-sp6-capstone.md` committed with filled-in evidence
- [ ] README.md + CLAUDE.md updated
- [ ] Tag `sp6-ref-server-capstone` created
- [ ] `grep anos\|ANOS examples/hello_atd.rs python/examples/hello_atd.py` empty
- [ ] The full arc of tags exists: sp1/sp2/sp3/sp4/sp5/sp6

## What comes after SP-6

The MVP development arc ends here. Downstream work is governance and ops:

- Public GitHub push (`github.com/atd-protocol/atd-mvp`)
- Announcement draft / partner outreach
- Phase 2 kickoff — conformance suite + web.post + streaming + cross-OS validation
- crates.io / PyPI packaging
- Demo video recording
