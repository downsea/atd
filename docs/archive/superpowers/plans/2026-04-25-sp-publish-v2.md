# SP-publish-v2 — crates.io publish prep for 11 crates

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every workspace crate ready to publish to crates.io at `0.2.0`. **No actual `cargo publish` upload happens in this SP** — that's deferred to a future user-driven step. This SP stops at dry-run verification, tags `v0.2.0` + `sp-publish-v2`, and pushes to GitHub.

**Spec:** `docs/superpowers/specs/2026-04-25-sp-publish-v2-design.md`

**Baseline tag (set before starting):** `pre-sp-publish-v2`

**Exit criteria:** see spec §7.

---

## Task 0: Rename `atd-ref-server-bin` → `atd-ref-server`

Drop the awkward `-bin` suffix while we have the chance. Post-rename, the package and binary names match (both `atd-ref-server`).

- [ ] **0.1: Pre-flight grep**

```bash
grep -rln 'atd-ref-server-bin\|atd_ref_server_bin' --include='*.toml' --include='*.rs' .
grep -rln 'atd-ref-server-bin\|atd_ref_server_bin' --include='*.md' . \
  | grep -v '^docs/superpowers/' | grep -v '^docs/whitepaper/'
```

The first list is the **code/manifest** changes (must do). The second is the **live docs** changes (must do, excluding historical SP archive). Historical specs/plans under `docs/superpowers/` are read-only per project rule.

- [ ] **0.2: Rename the directory**

```bash
git mv crates/atd-ref-server-bin crates/atd-ref-server
```

- [ ] **0.3: Update workspace `Cargo.toml`**

Change member entry `"crates/atd-ref-server-bin"` → `"crates/atd-ref-server"`.

- [ ] **0.4: Update `crates/atd-ref-server/Cargo.toml`**

- `name = "atd-ref-server-bin"` → `name = "atd-ref-server"`
- `[lib].name = "atd_ref_server_bin"` → `name = "atd_ref_server"`
- `[[bin]].name` already `"atd-ref-server"`; verify unchanged
- Drop the `[[bin]]` doc-comment about the `-bin` suffix (no longer needed)

- [ ] **0.5: Update `crates/atd-ref-server/src/main.rs`**

- `use atd_ref_server_bin::builtin::builtin_registry;` → `use atd_ref_server::builtin::builtin_registry;`
- `use atd_ref_server_bin::server::{Server, ServerConfig};` → `use atd_ref_server::server::{Server, ServerConfig};`

- [ ] **0.6: Update `crates/atd-ref-server/tests/*.rs`**

7 test files contain `atd_ref_server_bin::...`. Replace with `atd_ref_server::...`:

```bash
sed -i 's/atd_ref_server_bin/atd_ref_server/g' crates/atd-ref-server/tests/*.rs
```

- [ ] **0.7: Update `crates/atd-conformance/Cargo.toml`**

- Path dep: `atd-ref-server-bin = { path = "../atd-ref-server-bin", version = "0.1.0" }` → `atd-ref-server = { path = "../atd-ref-server", version = "0.1.0" }`

- [ ] **0.8: Update `crates/atd-conformance/tests/atd_mvp_self_conformance.rs`**

4 references; sed in place:

```bash
sed -i 's/atd_ref_server_bin/atd_ref_server/g; s/atd-ref-server-bin/atd-ref-server/g' \
  crates/atd-conformance/tests/atd_mvp_self_conformance.rs
```

- [ ] **0.9: Update `crates/atd-mcp-bridge/tests/integration_e2e.rs` + `examples/hello_atd.rs`**

These contain `cargo build --release -p atd-ref-server-bin` instructions in module doc-comments. Replace with the new package name.

```bash
sed -i 's/atd-ref-server-bin/atd-ref-server/g' \
  crates/atd-mcp-bridge/tests/integration_e2e.rs \
  examples/hello_atd.rs
```

- [ ] **0.10: Update `crates/atd-runtime/src/error.rs`**

One doc-comment reference; replace `atd-ref-server-bin` → `atd-ref-server`.

- [ ] **0.11: Update live docs (NOT historical SP archive)**

Live docs that mention the old name:
- `README.md`
- `docs/atd-architecture.md`
- `docs/design.md`
- `docs/protocol/error-codes.md`
- `docs/integrations/{overview,hermes,langchain,claude-code}.md`
- `crates/atd-mcp-bridge/README.md`

Use a guarded sed (excluding `docs/superpowers/` and `docs/whitepaper/`):

```bash
git ls-files -z 'README.md' 'docs/**/*.md' 'crates/**/README.md' \
  | grep -zv '^docs/superpowers/' \
  | grep -zv '^docs/whitepaper/' \
  | xargs -0 sed -i 's/atd-ref-server-bin/atd-ref-server/g'
```

(Or do it file by file if the find-replace might touch quoted strings that should stay literal — but `atd-ref-server-bin` is a unique enough token that blanket replacement is safe.)

- [ ] **0.12: Build + test**

```bash
cargo build --workspace
cargo test --workspace --all-targets
```

Expected: 334 tests pass; no `unresolved import` errors.

- [ ] **0.13: fmt + clippy**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
```

- [ ] **0.14: Commit**

```bash
git add -A
git commit -m "refactor(crates): rename atd-ref-server-bin → atd-ref-server

Drop the -bin suffix now that the rename is no longer breaking — the
installed binary was already named atd-ref-server, so package + binary
names finally match. Last chance before crates.io publication.

Live docs updated; historical SP archive (docs/superpowers/) left
read-only per project rule."
```

---

## Task 1: Version bump to 0.2.0

**Files:**
- Modify: `Cargo.toml` (workspace)
- Modify: every `crates/*/Cargo.toml` containing `version = "0.1.0"` path-dep literals

- [ ] **1.1: Workspace version**

In `Cargo.toml`, change `[workspace.package].version = "0.1.0"` → `"0.2.0"`.

- [ ] **1.2: Path-dep version literals**

Every line of the form `atd-<x> = { path = "../atd-<x>", version = "0.1.0" }` in any `crates/*/Cargo.toml` becomes `version = "0.2.0"`.

```bash
grep -rn 'version = "0.1.0"' crates/*/Cargo.toml   # enumerate first
sed -i 's/version = "0.1.0"/version = "0.2.0"/g' crates/*/Cargo.toml
```

The `[package].version.workspace = true` lines are unaffected (no literal `"0.1.0"`).

- [ ] **1.3: Build + test**

```bash
cargo build --workspace
cargo test --workspace --all-targets
```

Expected: 334 tests pass; no version-mismatch errors.

- [ ] **1.4: Commit**

```bash
git add Cargo.toml crates/*/Cargo.toml
git commit -m "chore(release): bump workspace 0.1.0 → 0.2.0 for SP-publish-v2"
```

---

## Task 2: Cargo.toml metadata fill-in (7 crates)

The 4 already-polished crates (`atd-protocol`, `atd-sdk`, `atd-mcp-bridge`, `atd-conformance`) are skipped. The 7 remaining need `readme = "README.md"`, `keywords`, `categories`.

After each Cargo.toml edit, the `[package]` block should contain the new lines just below the existing `description = ...` line.

- [ ] **2.1: `atd-runtime/Cargo.toml`**

```toml
readme = "README.md"
keywords = ["atd", "agent", "tool-dispatch", "runtime", "server"]
categories = ["api-bindings", "asynchronous"]
```

- [ ] **2.2: `atd-tools-echo/Cargo.toml`**

```toml
readme = "README.md"
keywords = ["atd", "tool", "echo", "agent"]
categories = ["api-bindings", "development-tools"]
```

- [ ] **2.3: `atd-tools-fs/Cargo.toml`**

```toml
readme = "README.md"
keywords = ["atd", "tool", "filesystem", "agent"]
categories = ["api-bindings", "filesystem"]
```

- [ ] **2.4: `atd-tools-shell/Cargo.toml`**

```toml
readme = "README.md"
keywords = ["atd", "tool", "shell", "agent"]
categories = ["api-bindings", "command-line-utilities"]
```

- [ ] **2.5: `atd-tools-web/Cargo.toml`**

```toml
readme = "README.md"
keywords = ["atd", "tool", "web", "fetch", "agent"]
categories = ["api-bindings", "web-programming::http-client"]
```

- [ ] **2.6: `atd-cli/Cargo.toml`**

```toml
readme = "README.md"
keywords = ["atd", "cli", "agent", "tool-dispatch"]
categories = ["command-line-utilities", "development-tools"]
```

- [ ] **2.7: `atd-ref-server/Cargo.toml`**

```toml
readme = "README.md"
keywords = ["atd", "agent", "server", "tool-dispatch", "reference"]
categories = ["command-line-utilities", "api-bindings"]
```

- [ ] **2.8: Build check + commit**

```bash
cargo build --workspace
git add crates/*/Cargo.toml
git commit -m "chore(release): fill in readme/keywords/categories for 7 crates"
```

---

## Task 3: README.md authoring (7 crates)

Each README is short — name + one-paragraph purpose + 1 quick example or usage line + cross-links + license. ~30-60 lines.

- [ ] **3.1: `crates/atd-runtime/README.md`**

```markdown
# atd-runtime

Server-side runtime for the [Agent Tool Dispatch (ATD) protocol](https://github.com/downsea/atd-mvp).

This crate provides the building blocks for hosting ATD tools:

- `Tool` trait — implement once per tool
- `Registry` — register tools and dispatch incoming calls
- `Binding` (`NativeBinding`, `CliBinding`, future `McpBinding`) — adapter between a `Tool` and the runtime
- `Middleware` — pre/post-call interceptors (audit, redact, rate-limit are built-in)
- `CapabilityGate` — checks `required_capabilities` against the caller's grants

If you want to **build** an ATD-speaking server, use this crate.
If you want to **call** an ATD server, use [`atd-sdk`](https://crates.io/crates/atd-sdk).

See `docs/atd-architecture.md` §4 (Dispatch Layer) for the conceptual model.

## License

Apache-2.0.
```

- [ ] **3.2: `crates/atd-tools-echo/README.md`**

```markdown
# atd-tools-echo

Built-in `ref:echo.say` tool implementation for the ATD reference runtime.

This crate is a `cargo add atd-tools-echo` ingredient — pair it with
[`atd-runtime`](https://crates.io/crates/atd-runtime) to register the echo tool
in your own server. The reference server [`atd-ref-server`](https://crates.io/crates/atd-ref-server)
already wires this in.

## License

Apache-2.0.
```

- [ ] **3.3: `crates/atd-tools-fs/README.md`**

```markdown
# atd-tools-fs

Built-in filesystem tools (`ref:fs.read`, `ref:fs.write`, `ref:fs.edit`,
`ref:fs.glob`, `ref:fs.grep`) for the ATD reference runtime.

Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime) in your own
server, or use [`atd-ref-server`](https://crates.io/crates/atd-ref-server)
which has these tools registered out of the box.

Path safety is enforced by the runtime's capability gate; see
[`docs/protocol/wire-format.md`](https://github.com/downsea/atd-mvp/blob/master/docs/protocol/wire-format.md).

## License

Apache-2.0.
```

- [ ] **3.4: `crates/atd-tools-shell/README.md`**

```markdown
# atd-tools-shell

Built-in shell tools (`ref:shell.exec`, `ref:shell.pwsh`) for the ATD reference
runtime.

Both tools enforce a timeout, a hard byte cap on stdout/stderr, and the runtime's
capability gate. PowerShell is invoked via `pwsh -NoProfile -Command` when
available; `exec` uses `/bin/sh -c` (Unix) or `cmd /C` (Windows).

Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime), or get them
preregistered via [`atd-ref-server`](https://crates.io/crates/atd-ref-server).

## License

Apache-2.0.
```

- [ ] **3.5: `crates/atd-tools-web/README.md`**

```markdown
# atd-tools-web

Built-in `ref:web.fetch` tool for the ATD reference runtime — HTTP/HTTPS GET with
SSRF guards (private IPs blocked by default), per-call timeouts, byte caps, and
HTML-to-Markdown conversion via `htmd`.

Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime), or use
[`atd-ref-server`](https://crates.io/crates/atd-ref-server) which has this tool
registered.

## License

Apache-2.0.
```

- [ ] **3.6: `crates/atd-cli/README.md`**

````markdown
# atd-cli

Reference command-line client for the [ATD protocol](https://github.com/downsea/atd-mvp).

## Install

```bash
cargo install atd-cli
```

This installs the `atd` binary.

## Usage

```bash
atd discover --sock /tmp/atd.sock
atd describe ref:echo.say --sock /tmp/atd.sock
atd call ref:echo.say --sock /tmp/atd.sock --args '{"text":"hello"}'
```

For the full surface, run `atd --help` and `atd <subcommand> --help`.

## See also

- [`atd-sdk`](https://crates.io/crates/atd-sdk) — the underlying Rust SDK
- [`atd-ref-server`](https://crates.io/crates/atd-ref-server) — a server the
  CLI can talk to out of the box

## License

Apache-2.0.
````

- [ ] **3.7: `crates/atd-ref-server/README.md`**

````markdown
# atd-ref-server

Reference server binary for the [ATD protocol](https://github.com/downsea/atd-mvp) —
serves 9 built-in tools (echo + 5 fs + 2 shell + 1 web) over a Unix socket.

## Install

```bash
cargo install atd-ref-server
```

This installs the `atd-ref-server` binary.

## Usage

```bash
atd-ref-server --sock /tmp/atd.sock
```

Useful flags:

- `--sock <path>` — Unix socket to bind
- `--audit-log <path>` — JSON-Lines structured audit log of every call
- `--allow-private-ips` — relax `ref:web.fetch` SSRF guard (off by default)
- `--enable-conformance-tool` — register conformance gated tools (1001/1002 fixtures)

Run `atd-ref-server --help` for the full surface.

## What's inside

This binary is a thin wiring of [`atd-runtime`](https://crates.io/crates/atd-runtime)
plus the four tool crates: [`atd-tools-echo`](https://crates.io/crates/atd-tools-echo),
[`atd-tools-fs`](https://crates.io/crates/atd-tools-fs),
[`atd-tools-shell`](https://crates.io/crates/atd-tools-shell),
[`atd-tools-web`](https://crates.io/crates/atd-tools-web).

To build your own server, use `atd-runtime` directly and pick whichever tool
crates you want.

## License

Apache-2.0.
````

- [ ] **3.8: Commit**

```bash
git add crates/atd-runtime/README.md crates/atd-tools-*/README.md crates/atd-cli/README.md crates/atd-ref-server/README.md
git commit -m "docs: add README.md for 7 crates missing one (publish prep)"
```

---

## Task 4: Cross-link audit on existing READMEs

The 4 already-existing READMEs (`atd-protocol`, `atd-sdk`, `atd-mcp-bridge`, `atd-conformance`) were last touched at v0.1.0. They were written for the post-refactor library names but predate the T0 rename. Cross-links to siblings need verification.

- [ ] **4.1: Re-read each existing README**

```bash
for f in crates/atd-protocol/README.md crates/atd-sdk/README.md crates/atd-mcp-bridge/README.md crates/atd-conformance/README.md; do
  echo "=== $f ==="
  cat "$f"
done
```

Look for:
- References to `atd-types` or `atd-client` (old pre-refactor names) — replace with `atd-protocol` / `atd-sdk`
- References to `atd-ref-server-bin` (old pre-T0 name) — replace with `atd-ref-server`
- Stale "future `atd-protocol`" caveat language
- Crates.io links pointing to old names
- `<YOUR_USERNAME>` placeholders that survived

- [ ] **4.2: Patch any issues found**

Fix in place; if all 4 READMEs are clean, skip.

- [ ] **4.3: Commit (only if patches were made)**

```bash
git add crates/*/README.md
git commit -m "docs: refresh existing crate READMEs for post-refactor + post-rename names"
```

If no patches, skip the commit; record "Task 4 no-op" in the executing-plans log.

---

## Task 5: Dry-run + tag + push

**Note: this SP does NOT include `cargo publish` upload.** Dry-run is a verification step only. The user explicitly chose to defer publication.

- [ ] **5.1: Crates.io name availability pre-flight**

```bash
for c in atd-protocol atd-sdk atd-runtime atd-tools-echo atd-tools-fs atd-tools-shell atd-tools-web atd-cli atd-conformance atd-mcp-bridge atd-ref-server; do
  printf '%-22s ' "$c"
  curl -s -o /dev/null -w '%{http_code}\n' "https://crates.io/api/v1/crates/$c"
done
```

Expected: all 11 → `404`. If any returns `200`, the name is taken — surface to the user (publication isn't happening now anyway, but the user should know).

- [ ] **5.2: Lint + format gate**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-features -- -D warnings
```

- [ ] **5.3: Test gate**

```bash
cargo test --workspace --all-targets
```

Expected: 334 tests pass.

- [ ] **5.4: Dry-run publish for all 11 crates (in dep order)**

```bash
# Layer 1
cargo publish -p atd-protocol --dry-run 2>&1 | tail -10

# Layer 2
cargo publish -p atd-runtime --dry-run 2>&1 | tail -10
cargo publish -p atd-sdk     --dry-run 2>&1 | tail -10

# Layer 3
cargo publish -p atd-tools-echo  --dry-run 2>&1 | tail -10
cargo publish -p atd-tools-fs    --dry-run 2>&1 | tail -10
cargo publish -p atd-tools-shell --dry-run 2>&1 | tail -10
cargo publish -p atd-tools-web   --dry-run 2>&1 | tail -10

# Layer 4
cargo publish -p atd-cli         --dry-run 2>&1 | tail -10
cargo publish -p atd-conformance --dry-run 2>&1 | tail -10
cargo publish -p atd-mcp-bridge  --dry-run 2>&1 | tail -10

# Layer 5
cargo publish -p atd-ref-server --dry-run 2>&1 | tail -10
```

Expected per crate:
- "Packaging" + "Verifying" + "Compiling" + "Finished" + "Packaged"
- **No** warnings of the form `manifest has no description` / `... no license` / `... no documentation` / `... no homepage` / `... no repository` (warnings about missing `homepage` and `documentation` are acceptable — those are nice-to-have, not gating).

If any **gating** warning fires (description/license/repository/readme), go back to the corresponding Cargo.toml and fix.

- [ ] **5.5: Tag**

```bash
git log --oneline | head -10

git tag -a v0.2.0 -m "v0.2.0 — atd-mvp post-refactor crates.io readiness

11-crate workspace ready for publish (dry-runs clean):
- atd-protocol, atd-sdk, atd-runtime (libraries)
- atd-tools-{echo,fs,shell,web} (built-in tools)
- atd-conformance (test runner)
- atd-cli, atd-mcp-bridge, atd-ref-server (binaries)

Notable: atd-ref-server-bin renamed to atd-ref-server in this release.
334 tests pass; cargo publish not yet invoked.
"

git tag -a sp-publish-v2 -m "SP-publish-v2 implementation complete (publish-prep, no upload)"
```

- [ ] **5.6: Push to GitHub**

```bash
git push origin master
git push origin v0.2.0
git push origin sp-publish-v2
```

- [ ] **5.7: Final summary to user**

Print:

```
SP-publish-v2 complete.

- Tags v0.2.0 and sp-publish-v2 created and pushed to origin.
- All 11 crates dry-run clean at version 0.2.0.
- atd-ref-server-bin renamed to atd-ref-server.
- 334 tests passing; fmt + clippy gates green.

Crates.io publication intentionally deferred — when ready:
  cargo publish -p atd-protocol  && sleep 60
  cargo publish -p atd-runtime   && sleep 60
  cargo publish -p atd-sdk       && sleep 60
  cargo publish -p atd-tools-echo  && sleep 60
  cargo publish -p atd-tools-fs    && sleep 60
  cargo publish -p atd-tools-shell && sleep 60
  cargo publish -p atd-tools-web   && sleep 60
  cargo publish -p atd-cli         && sleep 60
  cargo publish -p atd-conformance && sleep 60
  cargo publish -p atd-mcp-bridge  && sleep 60
  cargo publish -p atd-ref-server
```

---

## Post-plan verification checklist

- [ ] Workspace `version = "0.2.0"`; no `"0.1.0"` literals remain
- [ ] Package `atd-ref-server-bin` no longer exists; `atd-ref-server` does
- [ ] All 11 `crates/*/Cargo.toml` have `description`, `readme`, `keywords`, `categories`
- [ ] All 11 `crates/*/README.md` files exist
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy --workspace --all-features -- -D warnings` clean
- [ ] `cargo test --workspace --all-targets` — 334 tests pass
- [ ] All 11 `cargo publish -p <crate> --dry-run` succeed without gating warnings
- [ ] Tag `v0.2.0` exists and is pushed to origin
- [ ] Tag `sp-publish-v2` exists and is pushed to origin
- [ ] `master` is pushed to origin
