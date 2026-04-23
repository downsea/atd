# SP-6 Capstone Validation

**Date:** 2026-04-23
**Tag:** `sp6-ref-server-capstone`
**Status:** Evidence-based claim — atd-mvp is the independent reference implementation of the ATD protocol with zero runtime dependency on ANOS.

---

## 1. Claim

`atd-mvp` is positioned (per `CLAUDE.md`) as *"the independent reference implementation of the ATD protocol and client SDK"*, intentionally separate from the ANOS project. Through SP-1 to SP-5 we built `atd-ref-server` — a clean-room, Apache-2.0-licensed, 9-tool reference server with 243 tests, full SSRF defense, and zero `anos-*` dependencies.

This document shows that claim working end-to-end. The same `hello_atd` example that previously required a running ANOS daemon now runs against our own reference server — in-repo, with a single `cargo run` command after building the binary. The Python SDK demo is structurally identical. Both prove that the ATD wire protocol is vendor-neutral: the client speaks the format, any compliant server answers.

---

## 2. Evidence 1 — Rust end-to-end

Commands:
```bash
cargo build --release -p atd-ref-server
cargo run --example hello_atd -p atd-examples
```

Captured output:

```
[atd] auto-spawning atd-ref-server → /tmp/.tmpHOgM8X/demo.sock
[atd] connected
[atd] 9 tools registered

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"echoed":{"text":"hello from ATD"}}

[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → 9 paths: Cargo.toml, crates/atd-cli/Cargo.toml, crates/atd-client/Cargo.toml (+6 more)

[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout="Linux"

[atd] done.
```

Exit code: 0.

Commentary:

- **Boot line.** `atd-ref-server` is spawned by the example itself into a tempdir socket. No user-managed daemon, no `ANOS_SOCK` env var, no global state touched. The tempdir path (`/tmp/.tmpHOgM8X/`) is unique per run; the ULID in the socket path differs across runs — expected.
- **`ref:echo.say`.** Deterministic call with a string argument. Proves request framing, JSON (de)serialization, and call routing. The response wraps the input verbatim under `echoed`.
- **`ref:fs.glob`.** Real directory walk over the atd-mvp repo itself. Returns 9 TOML manifests. Proves `ignore::Walk` + `globset` integration and `.gitignore` honoring — the returned list excludes `target/` entries automatically.
- **`ref:shell.exec`.** Real subprocess output from `uname -s`. Proves subprocess spawn, stdout capture, and exit-code pass-through.

---

## 3. Evidence 2 — Python end-to-end

Command (run from repo root):
```bash
uv run --project python python python/examples/hello_atd.py
```

Captured output:

```
[atd] auto-spawning atd-ref-server → /tmp/tmpypu4hm6w/demo.sock
[atd] connected
[atd] 9 tools registered

[1/3] ref:echo.say {"text":"hello from ATD"}
      → {"echoed": {"text": "hello from ATD"}}

[2/3] ref:fs.glob {"pattern":"**/*.toml","path":"."}
      → 9 paths: Cargo.toml, crates/atd-cli/Cargo.toml, crates/atd-client/Cargo.toml (+6 more)

[3/3] ref:shell.exec {"command":"uname -s"}
      → exit 0, stdout='Linux'

[atd] done.
```

Exit code: 0.

Commentary: structurally identical to the Rust output. The tempdir socket path differs per run (expected — `tempfile.TemporaryDirectory` generates a fresh path each time). The `call_id` / ULID in the response is also unique per run and is not printed in the summary output, only in the raw wire response. One cosmetic difference: Python renders stdout with single quotes (`'Linux'`) vs Rust's double quotes (`"Linux"`) — this is Python's `repr()` vs Rust's `{:?}` formatting in the respective print helpers, not a protocol difference. The glob returns the same 9 paths as Rust because both examples run from the repo root with `path="."` relative to the server's cwd.

---

## 4. Evidence 3 — Dependency isolation

Command:
```bash
cargo tree -p atd-ref-server --prefix none | head -30
```

Output (first 30 lines of the dep tree):

```
atd-ref-server v0.1.0 (/home/nan/proj/atd-mvp/crates/atd-ref-server)
atd-types v0.1.0 (/home/nan/proj/atd-mvp/crates/atd-types)
serde v1.0.228
serde_core v1.0.228
serde_derive v1.0.228 (proc-macro)
proc-macro2 v1.0.106
unicode-ident v1.0.24
quote v1.0.45
proc-macro2 v1.0.106 (*)
syn v2.0.117
proc-macro2 v1.0.106 (*)
quote v1.0.45 (*)
unicode-ident v1.0.24
serde_json v1.0.149
itoa v1.0.18
memchr v2.8.0
serde_core v1.0.228
zmij v1.0.21
thiserror v2.0.18
thiserror-impl v2.0.18 (proc-macro)
proc-macro2 v1.0.106 (*)
quote v1.0.45 (*)
syn v2.0.117 (*)
clap v4.6.1
clap_builder v4.6.0
anstream v1.0.0
anstyle v1.0.14
anstyle-parse v1.0.0
utf8parse v0.2.2
anstyle-query v1.1.5
```

None of the following appear anywhere in the full dep tree:

- `anos-*` — absent (zero ANOS crates)
- `atd-client` — absent (server does not depend on its own client SDK)
- `atd-mcp-bridge` — absent
- `atd-cli` — absent

All direct deps are neutral infrastructure: `tokio`, `serde`, `serde_json`, `clap`, `thiserror`, `reqwest`, `rustls`, `hyper`, `ignore`, `grep-*`, `globset`, `html5ever`, `htmd`. None are protocol-coupling or vendor-specific.

---

## 5. Evidence 4 — License audit

`cargo-license` is not installed in this environment. The fallback used `cargo metadata --format-version=1` piped through a Python script that groups all workspace packages by declared SPDX license expression. Output:

```
(Apache-2.0 OR MIT) AND BSD-3-Clause: 1 crates — encoding_rs
(MIT OR Apache-2.0) AND Unicode-3.0: 1 crates — unicode-ident
0BSD OR MIT OR Apache-2.0: 1 crates — adler2
Apache-2.0: 8 crates — atd-cli, atd-client, atd-examples, atd-mcp-bridge, atd-ref-server ...
Apache-2.0 AND ISC: 1 crates — ring
Apache-2.0 OR BSL-1.0: 1 crates — ryu
Apache-2.0 OR ISC OR MIT: 2 crates — hyper-rustls, rustls
Apache-2.0 OR MIT: 8 crates — atomic-waker, fastrand, idna_adapter, pin-project-lite, rustc-hash ...
Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT: 5 crates — linux-raw-sys, rustix, wasi, wasip2, wit-bindgen
BSD-2-Clause OR Apache-2.0 OR MIT: 2 crates — zerocopy, zerocopy-derive
BSD-3-Clause: 3 crates — alloc-no-stdlib, alloc-stdlib, subtle
BSD-3-Clause AND MIT: 1 crates — brotli
BSD-3-Clause/MIT: 1 crates — brotli-decompressor
CDLA-Permissive-2.0: 1 crates — webpki-roots
ISC: 2 crates — rustls-webpki, untrusted
MIT: 32 crates — bytes, cfg_aliases, http-body, http-body-util, hyper ...
MIT OR Apache-2.0: 110 crates — anstream, anstyle, anstyle-parse, anstyle-query, anstyle-wincon ...
MIT OR Apache-2.0 OR LGPL-2.1-or-later: 1 crates — r-efi
MIT OR Apache-2.0 OR Zlib: 2 crates — lru-slab, tinyvec_macros
MIT OR Zlib OR Apache-2.0: 1 crates — miniz_oxide
MIT/Apache-2.0: 2 crates — serde_urlencoded, siphasher
Unicode-3.0: 18 crates — icu_collections, icu_locale_core, icu_normalizer, icu_normalizer_data, icu_properties ...
Unlicense OR MIT: 8 crates — aho-corasick, globset, grep-matcher, grep-regex, grep-searcher ...
Unlicense/MIT: 2 crates — same-file, walkdir
Zlib OR Apache-2.0 OR MIT: 1 crates — tinyvec
```

**Note on `r-efi` (`MIT OR Apache-2.0 OR LGPL-2.1-or-later`):** This crate appears in the metadata because it is in the workspace lockfile (pulled in transitively by platform toolchain dependencies). Since LGPL is offered as one of three alternatives and Apache-2.0 is also offered, linking against it under Apache-2.0 is permissible. No obligation to distribute sources arises when the permissive alternative is selected.

**GPL / AGPL / SSPL scan:** No `GPL*`, `AGPL*`, or `SSPL*` entries appear. The GPL-3.0+ contamination from `html2md` (flagged in SP-5 issue tracking) was removed in commit `3ed261d` by replacing `html2md` with `htmd` (Apache-2.0). The audit confirms that fix is in effect. `atd-ref-server` can be distributed as Apache-2.0.

---

## 6. Evidence 5 — Example diff

The `hello_atd.rs` example changed fundamentally between SP-5 and SP-6. Representative slices:

**Before (pre-SP-6, at `sp5-ref-server-web` tag):**

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
        .discover(None, DiscoverFilter { limit: Some(3), ..Default::default() })
        .await?;
    println!("[atd] {} tools discovered", tools.len());
```

**After (SP-6, current):**

```rust
//! atd-mvp capstone demo. Auto-spawns `atd-ref-server` (the in-repo neutral
//! reference ATD server), connects via `atd-client`, exercises three
//! representative tools end-to-end.
//!
//! This demo has ZERO dependency on ANOS. It proves the ATD protocol is
//! vendor-neutral: the client speaks the wire format, the ref-server answers.
//!
//! Override the server (e.g., to demo against ANOS):
//!   ATD_SOCK=~/.anos/anos.sock cargo run --example hello_atd

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
    let tmp = tempfile::tempdir()?;
    let sock = tmp.path().join("demo.sock");
    let child = Command::new(&binary)
        .arg("--sock").arg(&sock)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    if !wait_for_socket(&sock).await {
        return Err("ref-server didn't bind its socket within 3s".into());
    }
    Ok((Some(child), Some(tmp), sock))
}
```

Net change: `ANOS_SOCK` and `Endpoint::default_anos()` are gone. `ATD_SOCK` replaces them as a neutral override (works with ANOS, `atd-ref-server`, or any ATD-compliant server). The default path is `acquire_server()` — which spawns `atd-ref-server` from the repo's own `target/release/`, a peer implementation, not a dependency. The example now exercises three specific tools with real arguments instead of a generic `dry_run=true` probe.

---

## 7. What remains for Phase 2+

- **Demo video** — a 90-second screen capture of `cargo run --example hello_atd` from a fresh clone, for the project README and the eventual public announcement.
- **Conformance suite** — a protocol-level test harness that validates third-party server implementations against the ATD wire protocol (tracked in `docs/design.md` §7).
- **Public release** — push `atd-mvp` to `github.com/atd-protocol/atd-mvp`, announce to partner stakeholders.
- **Cross-OS validation** — verify the capstone demo on macOS and Windows. This SP was tested on Linux only (kernel 6.19.10, Fedora 43).

These are downstream of the code. The code itself is capstone-complete.
