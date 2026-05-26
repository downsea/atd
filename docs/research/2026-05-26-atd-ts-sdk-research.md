# atd-ts SDK — Pre-design Deep Research

**Status:** research-input (not a spec; informs whatever SP design eventually starts)
**Author:** atd-mvp maintainer
**Date:** 2026-05-26
**Triggered by:** `docs/issues/2026-05-26-atd-ts-sdk-adopter-requirements.md` — and explicitly governed by the **订正 callout** added to that issue's §1 end (load-bearing; read it first)
**Out of scope:** any non-TS deliverable; the SP design specs themselves; any execution commitment (see §1.3 + §2 below)

---

## 0. How to read this document

This is **research and option-shaping, not a launch document**. The issue has a same-day correction block that this document is built on top of — go read `docs/issues/2026-05-26-atd-ts-sdk-adopter-requirements.md` §1 末尾的 "📝 订正" segment first. Its three load-bearing points:

1. `atd-mcp-bridge` is **lossy** by construction (tier / safety / capability / output_schema / dry_run / NDJSON streaming all collapse to MCP's narrower Tool surface). Confirmed by source-check of `crates/atd-mcp-bridge/src/bridge.rs:handle_tools_list`.
2. **oh-cli is an atd-rs adopter, not an atd-ts adopter.** Its mobile scenarios talk to MCP-only consumers (小艺 Claw / Claude Code / DeepSeek V4 Pro) for which the lossy mapping is fine — they cannot consume ATD's full surface anyway. There is no oh-cli-blocking timeline pressure for atd-ts.
3. The real driver is an **ecosystem opportunity** (HMOS 6.1 has no first-class in-process ArkTS MCP server SDK; future browser dashboards; future TS adopters). Discipline: **wait for a named ArkTS agent-runtime adopter** — same shape as cbrain → `atd_server` Python runtime, or cbrain P2-10's "wait for second adopter" rule.

Therefore: this document **does not advocate starting work now**. It consolidates the design space, fact-checks the issue's claims against the open web, and produces a defensible default architecture so that *whenever* a named adopter materializes, the SP author starts from a researched position rather than a blank page. Each section ends with one of:

- **Decision input** — recommendation the SP should carry forward unless the adopter's constraints overrule it.
- **[VERIFY]** — claim we couldn't confirm from open web sources; needs Huawei dev-portal lookup, real-device test, or upstream maintainer ping before it becomes load-bearing.
- **Open question** — fork in the design space the SP author must commit to.

Treat "Decision input" as the working default, not a decision.

---

## 1. Executive summary

The TypeScript SDK, if/when built, is two published packages and one auxiliary — not a single deliverable. The Node/browser/Bun/Deno target and the ArkTS target **share a wire format and a JSON schema but zero source code**: ArkTS rejects too many idioms a good general TS SDK relies on (nominal-only typing, no destructuring, no `Reflect`, no index access, no function expressions, no `Symbol` outside iterator). Trying to make one source compile to both is strictly worse than maintaining two implementations against a shared protocol fixture set.

### 1.1 Defensible defaults to carry into SP design

| Concern | Default | Justified in |
|---|---|---|
| Repo layout | pnpm monorepo at `typescript/`, sibling to `crates/` and `python/` | §5 |
| Published packages | `@atd-protocol/client`, `@atd-protocol/arkts-client`, `@atd-protocol/conformance`, `@atd-protocol/adapters` | §5 |
| Build tool | **tsdown** (Rolldown-based, tsup successor); ESM-first dual publish + `.d.cts` | §6 |
| Runtime targets | Node ≥20.19, modern evergreen browsers, Bun, Deno, Cloudflare Workers (`workerd`). ArkTS is a separate package. | §4, §6 |
| Transport abstraction | 2-method async interface: `request(req) → resp` + `close()`. Connection-level events as optional callbacks. Subpath exports gate Node-only transports. | §7 |
| Concrete transports v1 | `HttpTransport` (root, runtime-neutral), `UnixSocketTransport` (`./node/unix`, Node only), `StdioTransport` (`./node/stdio`, Node only). WebSocket deferred until `atd-server` adds it. | §7 |
| Type generation | Hand-written TS types as source of truth; `json-schema-to-typescript` in CI as drift gate against `/atd-protocol-schema.json`. No Zod on outbound. | §8 |
| Runtime validation | **None on happy path**, opt-in `parseStrict()` helper backed by Valibot for adopters who want it. | §8 |
| UCAN-lite | Hand-rolled verify in `core/ucan.ts` (~250 LOC) on top of `@noble/ed25519` + `@scure/base`. No off-the-shelf UCAN lib. | §10 |
| Pagination cursors | Opaque `string` roundtrip only; **client SDK does not mint, never parses**. | §10 |
| Error model | Two channels (MCP pattern): connection/protocol/transport errors **throw** (`AtdError` taxonomy); tool execution errors **return** as `ToolResult.Error`. Parity with Rust + Python SDKs. | §9 |
| ArkTS strategy | Standalone implementation in `@atd-protocol/arkts-client`, shares only `atd-protocol-schema.json` + recorded wire fixtures. HTTP via `@kit.RemoteCommunicationKit`; WebSocket via `@ohos.net.webSocket`. Crypto via `@kit.CryptoArchitectureKit`. | §11 |
| Conformance | Recorded wire fixtures committed to `tests/fixtures/`, replayed by `@atd-protocol/conformance` (TS) and `atd-conformance` (Rust). Single source of truth. | §13 |
| Publishing | npm Trusted Publishing via OIDC, provenance on, `sideEffects: false`. | §6 |

### 1.2 The single most important framing

ATD already has a stable wire (1.0), a machine-readable schema, and two reference SDK implementations (Rust + Python). A TS SDK's job is **not** to invent — it is to mirror the existing surface idiomatically while handling three runtime-target tradeoffs that the original issue collapsed into one (web/Node, ArkTS-pure, ArkTS-native via NAPI).

### 1.3 Execution gating (per issue §1 订正)

This research is published; engineering is not started. Gating conditions:

- **P0-1 (`@atd-protocol/client`)** — start when a non-oh-cli TS adopter appears with concrete blocking work, OR when a separate strategic decision is made that ecosystem-funded work is worth the maintenance load. Estimated effort below stays at ~3–4 weeks once started.
- **P0-2 (`@atd-protocol/arkts-client`)** — start when a named ArkTS-agent-runtime adopter materializes (same shape as cbrain → Python `atd_server`). The HMOS gap is real (§12.1 confirms by absence of `@kit.MCPClient`) but "real" ≠ "now".

The §16 phasing exists so that "we know what 3–4 weeks looks like" — it is sizing, not scheduling.

---

## 2. Problem framing — what is actually being asked

Re-reading the issue after its §1 订正:

| Claim | Status |
|---|---|
| atd-ts SDK is **post-1.0 not-shipped**; bridge is the TS workaround | ✅ |
| `atd-mcp-bridge` is **lossy** (drops tier/safety/capability/output_schema/dry_run/NDJSON) | ✅ source-confirmed |
| oh-cli **needs** atd-ts to function on mobile | ❌ **refuted by 订正** — oh-cli's mobile consumers are MCP-only; lossy bridge is acceptable. oh-cli is an atd-rs adopter. |
| HMOS ArkTS lacks first-class in-process MCP server SDK | ✅ confirmed by absence (§12) |
| oh-cli timing creates SP pressure | ❌ **refuted by 订正** — no blocking timeline |
| Ecosystem demand justifies design ahead of adopter | ✅ — but **design**, not **scaffolding** (per 订正 discipline) |

The issue's §3/§5/§6 P0-P2 ranking remains useful as **design reference for whenever the SP starts** — same shape, same order — but the priority numbers no longer mean "start P0-1 next sprint". Read them as "this is the order of dependency, not the order of urgency."

What this research adds beyond the issue:

1. **Architectural defaults** (§5–§14) that the SP author shouldn't have to re-derive from scratch.
2. **Fact-check of HMOS ecosystem claims** (§12) — several of the issue's §6 supporting facts overshoot.
3. **Open questions list** (§15) for the SP forks that this research deliberately stops short of.
4. **Sizing estimate** (§16) for a 3-4w + 2-3w execution if/when adopter materializes — not a plan.

---

## 3. Protocol baseline — what a TS SDK must implement

Source of truth: `docs/protocol/wire-format.md` (1122 lines, derived from `crates/atd-protocol/src/`) + `/atd-protocol-schema.json` (CI-gated against Rust types).

### 3.1 Wire framing (UDS / TCP transport)

```
┌──────────────┬─────────────────────────────┐
│ 4-byte BE u32│   UTF-8 JSON body (≤10 MiB) │
└──────────────┴─────────────────────────────┘
```

- No HTTP headers, no JSON-RPC envelope, no version negotiation byte.
- 10 MiB body cap enforced **before** allocation (security against `0xFFFFFFFF` prefix attack).
- Length prefix does **not** include the 4 prefix bytes themselves.

### 3.2 Wire framing (HTTP transport)

Important finding: **`atd-server-http` is plain `POST /mcp` JSON-RPC** (one envelope per HTTP request, `Content-Length`-bounded), **not** length-prefixed stream-over-HTTP. There is **no WebSocket** in `atd-server-http` today. SSE exists only as adopter-layered streaming for specific tools, not as a wire transport.

**Implication for any TS SDK**: the browser / ArkTS path **only** speaks HTTP JSON-RPC. The two transports are not parameterized variants of one codec; they are two distinct message formats:

- UDS/TCP: 4-byte BE length + JSON ATD `Request` envelope (`{"type": "run_tool", …}`)
- HTTP: standard JSON-RPC 2.0 envelope (`{"jsonrpc":"2.0","method":"tools/call",…}`) translated server-side to/from ATD

A clean split — the browser SDK never deals with binary framing.

### 3.3 Message types (request side)

| Variant | Wire `"type"` | Notes |
|---|---|---|
| `Ping` | `"ping"` | liveness |
| `ToolList` | `"tool_list"` | discover |
| `ToolSchema` | `"tool_schema"` | describe |
| `RunTool` | `"run_tool"` | with `dry_run: bool` |
| `RunToolContinue` | `"run_tool_continue"` | SP-pagination-v1; opaque cursor |
| `Hello` | `"hello"` | optional capability handshake; `ucan_tokens?` (SP-capability-v2) |

### 3.4 Message types (response side)

| Variant | Wire `"type"` | Carries |
|---|---|---|
| `Pong` | `"pong"` | — |
| `ToolListResponse` | `"tool_list"` | `tools: Array<ToolSummary>` |
| `ToolSchemaResponse` | `"tool_schema"` | `schema: ToolDefinition` |
| `ToolResultResponse` | `"tool_result"` | `result: ToolResult`, `success: bool`, `dry_run: bool`, `next_cursor?: string` |
| `HelloAck` | `"hello_ack"` | `granted_capabilities`, `server_version`, `supported_tiers` |
| `Error` | `"error"` | `message`, `code?: u16`, `retryable?: bool`, `details?: object` |

### 3.5 Error codes a TS SDK must understand

From `docs/protocol/error-codes.md` + SP-capability-v2 + SP-pagination-v1:

- `1001` — `ERR_CAPABILITY_DENIED` (SP-12) → typed `CapabilityDenied` with `required` + `granted` arrays
- `1010` — `ERR_UCAN_INVALID` (parse / sig / alg / DID / attenuation widening)
- `1011` — `ERR_UCAN_EXPIRED`
- `1012` — `ERR_DELEGATION_TOO_DEEP` (chain > `max_ucan_chain_depth`, default 5)
- `1013` — `ERR_AUDIENCE_MISMATCH` (deepest UCAN `aud` ≠ connection `client_id`)
- `1020` — `ERR_CURSOR_EXPIRED` (default TTL 5min)
- `1021` — `ERR_CURSOR_INVALID` (forged / tampered / wrong-tool)
- Tool-level codes (string, not numeric): `"IO"`, `"TIMEOUT"`, `"NOT_FOUND"`, `"INVALID_ARGS"`, `"EPERM"`, etc.

### 3.6 Type surface — count

Counting from `crates/atd-protocol/src/`: ~12 structs + 6 enums = **18 named types**. Hand-writable surface. (Compare: `@modelcontextprotocol/sdk` has ~50 message variants and uses Zod schemas as source of truth.)

**Decision input:**
- Two transport wire formats: binary length-prefix (UDS/TCP) + JSON-RPC over HTTP. Two codecs in the SDK, not one parameterized.
- Hand-written TS types covering 18 named types is feasible.
- Browser/ArkTS path is HTTP-only by physics.

---

## 4. Target runtime matrix

| Target | Bundle entry | Available transports | Crypto | Constraints |
|---|---|---|---|---|
| **Node ≥20.19** | `./dist/index.mjs` / `.cjs` | All: UDS, TCP, HTTP, Stdio | `subtle` Ed25519 + `@noble/ed25519` | — |
| **Modern browsers** (Chrome 137+, FF 129+, Safari 17+) | `./dist/browser.mjs` via `browser` condition | HTTP only | `subtle` Ed25519 OR `@noble/ed25519` | Bundle size matters |
| **Cloudflare Workers** | `./dist/browser.mjs` via `workerd` | HTTP only | `subtle` Ed25519 (Workers compat-date ≥ 2024-11) | CSP — no `new Function()` (rules out Ajv default validator) |
| **Bun** | `./dist/index.mjs` | All (Bun supports `net.Socket`) | `subtle` + `@noble` | — |
| **Deno** | `./dist/index.mjs` via `deno` condition | All (`Deno.connect`) | `subtle` + `@noble` | `--allow-net=unix:…` permission |
| **ArkTS (HMOS 6.1)** | Separate package | HTTP via `@ohos.net.http` (RCP); WebSocket via `@ohos.net.webSocket`; raw TCP via `@ohos.net.socket` ([VERIFY]) | `@kit.CryptoArchitectureKit` (`cryptoFramework.AsyKeyGenerator("Ed25519")`) | ArkVM bytecode; no JS bundle |

**Cross-runtime gotchas:**
- `@ohos.net.socket` UDS-like raw socket support: thin docs. **[VERIFY]** before relying on it.
- WebCrypto Ed25519 on Safari ≥17 / Chrome ≥137 confirmed (Igalia post Aug 2025) — covers 2026 evergreens.
- Workers cannot use Ajv (`new Function`) — irrelevant for client SDK (no runtime validation), relevant for any future TS server.

**Decision input:**
- Top-level browser entry is HTTP-only. Stdio/Unix/TCP at subpath imports (`./node/stdio`, `./node/unix`) that pull `node:net` / `node:child_process` lazily.
- ArkTS = separate published package; no attempt to share emitted code.

---

## 5. Package & repo layout

**Location**: add `typescript/` at the repo root, sibling to `crates/`, `python/`, `examples/`. Mirrors the existing project shape — pythonistas look in `python/`; tsifolks will look in `typescript/`.

**Workspace**: pnpm monorepo. Catalog feature for cross-package version pinning; lower disk usage than npm/yarn; matches MCP SDK precedent.

```
typescript/
├── package.json                # private:true root, scripts only
├── pnpm-workspace.yaml         # workspace + catalogs
├── tsconfig.base.json
├── vitest.workspace.ts
├── packages/
│   ├── core/                   # @atd-protocol/core (private barrel)
│   │   ├── package.json
│   │   ├── src/
│   │   │   ├── wire.ts         # 4-byte BE length codec
│   │   │   ├── jsonrpc.ts      # HTTP JSON-RPC envelope codec
│   │   │   ├── types.ts        # 18 named types, hand-written
│   │   │   ├── errors.ts       # AtdError taxonomy
│   │   │   ├── sanitize.ts     # MCP name sanitization (port of Rust)
│   │   │   ├── ucan.ts         # UCAN-lite parse + verify (~250 LOC)
│   │   │   └── transport.ts    # Transport interface
│   │   └── public/             # curated re-exports (published surface only)
│   ├── client/                 # @atd-protocol/client (public)
│   │   ├── package.json
│   │   ├── tsdown.config.ts
│   │   └── src/
│   │       ├── index.ts        # runtime-neutral root (HttpTransport, AtdClient)
│   │       ├── browser.ts      # browser/workerd entry — HTTP only
│   │       ├── node/
│   │       │   ├── unix.ts     # subpath ./node/unix — UnixSocketTransport
│   │       │   ├── tcp.ts      # subpath ./node/tcp
│   │       │   └── stdio.ts    # subpath ./node/stdio
│   │       └── client.ts       # AtdClient core
│   ├── adapters/               # @atd-protocol/adapters
│   │   └── src/
│   │       ├── openai.ts       # as_openai_tools(ToolSummary[]) → OpenAI tools
│   │       ├── anthropic.ts    # as_anthropic_tools(...)
│   │       └── langchain.ts    # makeAtdToolkit() — parity with python/src/atd_client/adapters.py
│   ├── conformance/            # @atd-protocol/conformance
│   │   └── src/
│   │       └── runner.ts       # TS twin of cargo run -p atd-conformance
│   └── arkts-client/           # @atd-protocol/arkts-client — published to OHPM, not npm
│       ├── oh-package.json5
│       └── src/                # ArkTS source, hand-written, no shared emit
└── tests/
    └── fixtures/               # canonical wire fixtures — shared with Rust atd-conformance
        ├── ping-pong.bin
        ├── tool-list.json
        └── …
```

**Naming**: issue mixes `@atd-protocol/client` and `@atd/arkts-client`. Standardize on `@atd-protocol/*` for npm and OHPM — one namespace, reads as "same family" to consumers.

**Why monorepo, not separate repos:**
- Type changes in `core` ripple to `client` + `adapters` — single PR per change.
- Conformance package depends on the client SDK at exact source version.
- Pre-1.0 versioning: workspace moves together; one `pnpm publish -r` ships a coherent set.

**Why a private `core`, not just internal modules:**
- Hard ESM barrier — refactor `core/src/wire.ts` internals without breaking public API.
- `core/public/` is the only re-export surface; `core` itself is `"private": true`. MCP SDK CLAUDE.md treats this as load-bearing.

**Decision input:**
- `typescript/` at repo root, pnpm workspace, `@atd-protocol/*` namespace.
- Four published packages + one private barrel.
- Tests reference shared fixtures under `tests/fixtures/`; same files consumed by Rust `atd-conformance`.

---

## 6. Build / distribution / publishing

**Build tool: tsdown** (Rolldown-based; tsup's successor).

| Option | Verdict |
|---|---|
| **tsdown** | **Pick.** 3–5× faster than tsup, same `defineConfig`, built-in `attw` + `publint` hooks, active. |
| tsup | Safe fallback; maintenance slowed. |
| tshy | Zero-deps tsc-only; loses esbuild minification + browser-shim conditional. |
| unbuild | Only if we adopt UnJS/Nuxt ecosystem (we're not). |
| Plain `tsc` | Use as a `dts`-only generator inside tsdown. |

`packages/client/tsdown.config.ts`:

```ts
import { defineConfig } from "tsdown"
export default defineConfig({
  entry: {
    index:   "src/index.ts",
    browser: "src/browser.ts",
    "node/unix":  "src/node/unix.ts",
    "node/tcp":   "src/node/tcp.ts",
    "node/stdio": "src/node/stdio.ts",
  },
  format: ["esm", "cjs"],
  dts: true, sourcemap: true, clean: true,
  treeshake: true, target: "es2022",
  attw: true, publint: true,
})
```

**Module strategy: dual ESM/CJS, ESM-first.**

- Node ≥20.19 stabilized `require(esm)` (Joyee Cheung 2025), but Node 18.x + webpack 5 in CJS mode still cause real trouble in 2026.
- openai-node, anthropic-sdk-typescript, stripe-node all still dual-ship in 2026.
- Vercel AI SDK is ESM-only at Node ≥22 — viable for greenfield, but ATD's slow adopter ramp argues for dual.

`packages/client/package.json` (load-bearing parts):

```json
{
  "name": "@atd-protocol/client",
  "type": "module",
  "main":   "./dist/index.cjs",
  "module": "./dist/index.js",
  "types":  "./dist/index.d.ts",
  "sideEffects": false,
  "exports": {
    ".": {
      "types":   { "import": "./dist/index.d.ts", "require": "./dist/index.d.cts" },
      "browser": "./dist/browser.js",
      "workerd": "./dist/browser.js",
      "deno":    "./dist/index.js",
      "import":  "./dist/index.js",
      "require": "./dist/index.cjs"
    },
    "./node/unix":  { "types": "./dist/node/unix.d.ts",  "import": "./dist/node/unix.js" },
    "./node/tcp":   { "types": "./dist/node/tcp.d.ts",   "import": "./dist/node/tcp.js" },
    "./node/stdio": { "types": "./dist/node/stdio.d.ts", "import": "./dist/node/stdio.js" },
    "./package.json": "./package.json"
  },
  "engines": { "node": ">=20.19" },
  "files": ["dist", "README.md", "LICENSE"],
  "publishConfig": { "access": "public", "provenance": true }
}
```

**`attw` lint** (`@arethetypeswrong/cli`) must pass pre-publish — catches `.d.cts` separate-types trap and "ESM dependency in CJS module graph" footgun. tsdown runs it pre-build with `attw: true`.

**Publishing: npm Trusted Publishing (OIDC)** — GA July 2025. Provenance auto-generated; no tokens to rotate. Requires npm CLI ≥11.5.1 + GitHub Actions with `id-token: write`.

**Engines pin**: `"node": ">=20.19"` — `require(esm)` stable + WebCrypto Ed25519 reliable.

**`barrelClean` test** (port from MCP SDK): tests verify root entry doesn't pull `node:net`/`node:child_process`/`node:fs` into the module graph. Implementation: build root entry with esbuild in browser target, assert no Node-only imports remain.

**Decision input:**
- tsdown + ESM-first dual + subpath splits for Node-only entries.
- `attw` + `publint` + `barrelClean` are pre-publish gates.
- npm Trusted Publishing on GitHub Actions.

---

## 7. Transport abstraction

MCP uses a 5-callback Transport (`start`/`send`/`close`/`onmessage`/`onerror`/`onclose`) because MCP carries server→client notifications, progress events, and resumable long-running requests. **ATD does not** — every request gets exactly one response, no multiplexing, no push. Our Transport is simpler:

```ts
// core/src/transport.ts
export interface Transport {
  /**
   * Send one ATD request frame, await one response frame.
   * Stream transports (UDS/TCP/Stdio) MUST serialize concurrent calls
   * via an internal mutex; HTTP transports may execute concurrently.
   */
  request(req: AtdRequest, opts?: RequestOptions): Promise<AtdResponse>

  /** Best-effort graceful shutdown. Idempotent. */
  close(): Promise<void>

  /** Optional connection-lifecycle hooks. */
  onClose?: () => void
  onError?: (err: Error) => void
}

export interface RequestOptions {
  /** Per-call deadline; throws `Timeout` on expiry. */
  timeoutMs?: number
  /** Abort via standard AbortSignal. */
  signal?: AbortSignal
}
```

**Concrete transports v1:**

| Transport | Subpath | Runtimes | Notes |
|---|---|---|---|
| `HttpTransport` | root entry | Node, browser, Bun, Deno, Workers, ArkTS-via-`@ohos.net.http` | speaks `POST /mcp` JSON-RPC; carries `Bearer` for UCAN |
| `UnixSocketTransport` | `./node/unix` | Node, Bun | `net.createConnection({path})` + length-prefix codec |
| `TcpTransport` | `./node/tcp` | Node, Bun, Deno | length-prefix codec over `net.Socket` / `Deno.connect` |
| `StdioTransport` | `./node/stdio` | Node, Bun | spawns child via `cross-spawn`, length-prefix on stdin/stdout |

**Why no WebSocket transport in v1**: `atd-server-http` doesn't implement WebSocket today. Once it does (its own SP), add `WsTransport` at root entry. The `Transport` interface already covers it.

**Concurrent-request semantics**: ATD wire is "one in-flight per connection". For `UnixSocketTransport`/`TcpTransport`/`StdioTransport`, `request()` serializes with internal mutex (mirrors Rust `Mutex<Pipe>`). For `HttpTransport`, each call is its own POST; concurrency bounded only by `http.Agent` pooling.

**Connection retry**: SP-concurrency-baseline §5.3 in Rust does exponential backoff + jitter. Mirror in `UnixSocketTransport`/`TcpTransport`:
- max attempts default 3, configurable
- exponential 50ms → 100ms → 200ms (cap 1s)
- ±20% jitter
- fatal short-circuit on `ENOENT` / `EACCES`

**HTTP transport sketch:**

```ts
class HttpTransport implements Transport {
  constructor(opts: { url: string; bearer?: string; fetch?: typeof fetch }) {}
  async request(req: AtdRequest, opts?: RequestOptions): Promise<AtdResponse> {
    const jsonRpc = atdRequestToJsonRpc(req)
    const resp = await this.fetch(this.url, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(this.bearer ? { authorization: `Bearer ${this.bearer}` } : {}),
      },
      body: JSON.stringify(jsonRpc),
      signal: opts?.signal,
    })
    if (!resp.ok) throw atdErrorFromHttpStatus(resp.status, await resp.text())
    return jsonRpcToAtdResponse(await resp.json())
  }
  async close() { /* http pool cleanup */ }
}
```

`fetch` is injected for testability and for ArkTS (which passes `@ohos.net.http`-backed fetch shim).

**Decision input:**
- 2-method Transport interface.
- 4 concrete transports v1; HTTP at root, others at `./node/*` subpaths.
- Per-connection mutex on stream transports; HTTP concurrency-free.
- Retry+jitter semantics match Rust SDK.

---

## 8. Type generation & runtime validation

### 8.1 Types

18 named types (§3.6). Hand-write in `core/src/types.ts`. CI verifies drift against `/atd-protocol-schema.json`:

```bash
# scripts/check-schema-drift.ts (CI gate)
npx json-schema-to-typescript ../atd-protocol-schema.json > .generated.d.ts
npx tsc --noEmit --strict packages/core/src/types.ts .generated.d.ts
# Together-compile fails if hand-written and generated types diverge structurally.
```

Why not codegen the public surface:
- `json-schema-to-typescript` output is generated-looking — union names like `ToolSummary | ToolSummary1`, awkward `$ref`, no docstrings from `description`.
- Discriminated union syntax (`{ status: "success", data: T } | { status: "error", code: string }`) reads better hand-written.
- 18 types fits in <200 LOC.

**Discriminated unions** — `ToolResult` example:

```ts
export type ToolResult<T = unknown> =
  | { status: "success"; data: T;       metadata: ToolResultMetadata }
  | { status: "error";   code: string;  message: string; reason: string | null; retryable: boolean }
```

Matches Rust `#[serde(tag="status", rename_all="snake_case")]` 1:1.

### 8.2 Runtime validation

**Default: none on happy path.** Wire is server-validated; if server emits garbage, that's a server bug. Defensive deserialization (per-field optional, tolerate unknown enum variants, ignore extra fields) handles realistic failure modes.

Opt-in `parseStrict()` helper backed by Valibot at separate subpath:

```ts
import { parseStrict } from "@atd-protocol/client/strict"
const result = parseStrict.toolResult(rawJson)   // throws ProtocolError if invalid
```

Why Valibot, not Zod:
- 1.37 KB vs Zod v4's ~15 KB.
- API is Zod-shaped, switch cost low.
- Tree-shakable: importing only one schema's parser keeps bundle minimal.

Why opt-in, not always-on:
- 90% of users don't need it. 15 KB Zod in every browser bundle is a real cost.
- Defensive deserialization suffices for normal operation.
- Adopters writing ATD servers (cbrain, healthkit, celia) want strict validation in tests — `parseStrict` is for them.

### 8.3 Tool input/output schema

`ToolDefinition` carries `input_schema: object` and `output_schema: object` — JSON Schema draft-07. Client does **not** validate args before send (server authoritative). Adopters wanting pre-flight validation plug Ajv themselves.

For an MCP-style "BYO validator" experience (post-1.0 maybe):

```ts
import { AtdClient } from "@atd-protocol/client"
import { AjvValidator } from "@atd-protocol/client/validators/ajv"
const client = await AtdClient.connect(transport, { argsValidator: AjvValidator })
```

**Decision input:**
- Hand-written types; CI drift gate via `json-schema-to-typescript`.
- No outbound validation. Defensive inbound deserialization.
- Valibot-backed `parseStrict()` at subpath for opt-in.
- BYO arg validator deferred to post-1.0.

---

## 9. Error model & public API

### 9.1 Error taxonomy (parity with Rust + Python SDKs)

```ts
export class AtdError extends Error {}
export class ServerUnreachable      extends AtdError {}
export class ProtocolError          extends AtdError { constructor(public expected: string, public got: string) {} }
export class Timeout                extends AtdError { constructor(public toolId: string | null, public afterMs: number) {} }
export class ToolNotFound           extends AtdError { constructor(public toolId: string, public suggestions: string[]) {} }
export class InvalidArguments       extends AtdError { constructor(public toolId: string, public field: string, public reason: string) {} }
export class CapabilityDenied       extends AtdError { constructor(public toolId: string, public required: string[], public granted: string[]) {} }
export class BindingUnavailable     extends AtdError { constructor(public toolId: string, public tried: string[], public reason: string) {} }
export class ToolExecutionFailed    extends AtdError { constructor(public toolId: string, public inner: unknown) {} }
export class CursorExpired          extends AtdError {}
export class CursorInvalid          extends AtdError {}
export class UcanInvalid            extends AtdError {}
export class UcanExpired            extends AtdError {}
export class DelegationTooDeep      extends AtdError {}
export class AudienceMismatch       extends AtdError {}
export class PaginationLimitExceeded extends AtdError { constructor(public pagesFetched: number, public bytesFetched: number) {} }
export class MergeFailed            extends AtdError { constructor(public reason: string) {} }
```

Methods on `AtdError`:

```ts
isRetryable(): boolean         // mirrors AtdError::is_retryable in Rust
suggestFix(): string | null    // mirrors AtdError::suggest_fix
```

### 9.2 Two-channel error semantics (adopted from MCP)

- **Throw**: any failure where the server didn't compute a result — connection failures, protocol violations, capability denial, cursor invalid/expired, UCAN errors, timeouts.
- **Return as `ToolResult.Error`**: the tool computed a failure and the server delivered it intact — `IO`, `TIMEOUT`, `EPERM`, `NOT_FOUND` (tool-internal).

Matches both Rust SDK (`AtdError` enum vs `ToolResult::Error` variant) and MCP's `isError` pattern.

### 9.3 Public client API

```ts
import { AtdClient, HttpTransport } from "@atd-protocol/client"

// Construction
const client = await AtdClient.connect(
  new HttpTransport({ url: "https://atd.example.com/mcp", bearer: "..." }),
  { clientName: "my-app", clientVersion: "1.0.0" }
)

// Capability handshake (optional)
const granted: string[] = await client.hello({
  requestedCapabilities: ["fs:read", "shell:exec"],
  ucanTokens: ["<JWT>"]   // optional; carries delegation
})

// Discover
const tools: ToolSummary[] = await client.discover({ query: "fs", filter: { tier: "warm" }, limit: 20 })
const def:   ToolDefinition = await client.describe("ref:fs.read")

// Call (single shot)
const result: ToolResult<{content: string}> = await client.call(
  "ref:fs.read",
  { path: "/etc/hostname" },
  { dryRun: false, signal: ac.signal, timeoutMs: 5000 }
)
if (result.status === "success") console.log(result.data.content)
else                              console.error(result.code, result.message)

// Pagination (low-level)
const page = await client.callPage("celia:fhir.list_obs", { patient: "p1" }, undefined)
const next = await client.callPage("celia:fhir.list_obs", null, page.nextCursor!)

// Pagination (auto-loop)
const all = await client.callAll("celia:fhir.list_obs", { patient: "p1" }, {
  mergePolicy: { kind: "concatField", field: "obs" },
  maxPages: 100,
  maxTotalBytes: 8 * 1024 * 1024,
})

await client.ping()
await client.close()
```

### 9.4 Parity matrix with Rust + Python SDKs

| Capability | Rust `atd-sdk` | Python `atd_client` | TS `@atd-protocol/client` |
|---|---|---|---|
| `connect` | ✅ | ✅ | ✅ |
| `ping` | ✅ | ✅ | ✅ |
| `hello` | ✅ | ✅ | ✅ |
| `hello_with_ucan_tokens` | ✅ | ✅ | ✅ (one method; `ucanTokens` optional) |
| `discover` | ✅ | ✅ | ✅ |
| `describe` | ✅ | ✅ | ✅ |
| `call` | ✅ | ✅ | ✅ |
| `call_page` / `call_all` | ✅ | ✅ | ✅ |
| Sync wrapper | n/a | `AtdClientSync` | n/a (TS is async-native) |
| Capability denial as typed error | ✅ | ✅ | ✅ |
| UCAN verifier errors surfaced | ✅ | ✅ | ✅ |
| Connect retry + jitter | ✅ | ✅ | ✅ |
| `as_openai_tools` / `as_anthropic_tools` | adapters module | adapters module | `@atd-protocol/adapters` |
| Sanitize / desanitize names | ✅ | ✅ | ✅ |

**Decision input:**
- Error taxonomy 1:1 with Rust + Python; same class names in PascalCase.
- Throw vs return-as-`ToolResult.Error` follows existing SDKs.
- API surface ~10 methods; Python snake_case → TS camelCase (`tool_id` → `toolId`).

---

## 10. Crypto primitives — UCAN-lite + cursors + framing

### 10.1 UCAN-lite verification

Protocol uses constrained UCAN: Ed25519 only, `did:key` only, `cmd="atd-cap"`, `args.caps: string[]`. Off-the-shelf libs (`@ucans/ucans`, `@ipld/dag-ucan`, `@ucanto/*`) target broader UCAN 1.0 (heavier; includes IPLD/CAR) or are unmaintained.

**Hand-roll in `core/src/ucan.ts`** (~250 LOC):

```ts
import { ed25519 } from "@noble/curves/ed25519"
import { base58, base64url } from "@scure/base"

export async function verifyUcanChain(
  tokens: string[],
  audience: string,
  maxDepth: number,
): Promise<{ caps: string[] }> {
  // 1. Split each token on "."
  // 2. base64url-decode header + payload
  // 3. Check header.alg === "EdDSA", typ === "ucan/1.0+jwt"
  // 4. Parse iss/aud (both did:key Ed25519 → 32-byte pubkey)
  // 5. ed25519.verify(sig, signedPayload, pubkey)
  // 6. Walk delegation chain: each next iss === prev aud
  // 7. Attenuation: child caps ⊆ parent caps
  // 8. Enforce maxDepth, audience pinning, exp
  // 9. Return union of caps
}
```

Cross-target Ed25519:
- **Node / browser / Workers / Bun / Deno**: `@noble/ed25519` (5 KB), pure JS, zero deps. ✅
- **ArkTS**: `@noble/*` uses BigInt + tagged template literals (legal in ArkTS), but vendor through DevEco's ArkTS lint first. **Better path**: thin abstraction; ArkTS impl via `@kit.CryptoArchitectureKit` (native NAPI).

```ts
// core/src/crypto.ts
export interface Ed25519Verifier {
  verify(sig: Uint8Array, msg: Uint8Array, pubkey: Uint8Array): Promise<boolean>
}

// client default
import { ed25519 } from "@noble/curves/ed25519"
export const defaultEd25519Verifier: Ed25519Verifier = {
  async verify(sig, msg, pubkey) { return ed25519.verify(sig, msg, pubkey) },
}
```

Why not WebCrypto `subtle.sign("Ed25519")` universally:
- Browser support landed 2025 (Chrome 137, FF 129, Safari 17) — fine in 2026 — **but not in ArkTS**.
- Async-only, requires `subtle.importKey` boilerplate.
- 5 KB `@noble/ed25519` is cheaper than the conditional logic.

### 10.2 Pagination cursors

**Confirmed by research: the client SDK should never mint cursors.** Cursors are opaque per protocol contract (server-issued, server-validated, HMAC-signed CBOR payload binding `(tool_id, caller_id, args_fingerprint, page_index, issued_at, server_session)`).

**Implication**: zero CBOR / HMAC code in `@atd-protocol/client`. Pagination API roundtrips `string | undefined` through `next_cursor` and that's the entire contract.

If we ever ship `atd-server-ts` (TS-native reference server), CBOR + HMAC live there. Recommended stack for that future package:
- **`cborg`** (strict deterministic mode) — same input → same bytes is load-bearing for HMAC determinism.
- **`@noble/hashes/hmac` + `@noble/hashes/sha2`** — sync, 4 KB, portable.
- Avoid `cbor-x` (V8-specific tricks, ArkTS portability uncertain) and `cbor2` (decoder-focused).

### 10.3 Wire framing (UDS/TCP)

```ts
let buf = Buffer.alloc(0)
socket.on("data", chunk => {
  buf = Buffer.concat([buf, chunk])
  while (buf.length >= 4) {
    const len = buf.readUInt32BE(0)
    if (len > 10 * 1024 * 1024) {
      socket.destroy(new Error("frame too large"))
      return
    }
    if (buf.length < 4 + len) break        // partial body, wait
    const body = buf.subarray(4, 4 + len)
    buf = buf.subarray(4 + len)            // O(1) view
    emit(JSON.parse(body.toString("utf8")))
  }
})
```

Pitfalls to encode:
1. Validate `len` **before** allocating — `0xFFFFFFFF` prefix attack.
2. For high throughput, use `Buffer[]` queue with virtual cursor (avoids per-chunk concat).
3. `socket.setNoDelay(true)` — without it, small frames Nagle to 40ms latency.
4. Respect `socket.write() === false` backpressure; await `drain` before next write.
5. ArkTS: `@ohos.net.socket` UDS support **[VERIFY]**.

**Decision input:**
- UCAN-lite verify hand-rolled in `core/src/ucan.ts` over `@noble/ed25519` + `@scure/base`. ~250 LOC.
- `Ed25519Verifier` abstraction; arkts-client swaps to native crypto.
- Client SDK never mints cursors; CBOR/HMAC code stays out.
- Frame parsing follows Rust invariants 1:1 (length check before alloc).

---

## 11. ArkTS deep dive

### 11.1 Restrictions confirmed by research

Verified from [awesome-harmonyos ArkTS migration rules](https://github.com/HarmonyOS-Next/awesome-harmonyos/blob/main/Adaptation_rules_from_TypeScript_to_ArkTS.md) mirroring the official cookbook:

| Restriction | Rule label | Impact on generic TS SDK |
|---|---|---|
| No `any` / `unknown` | `arkts-no-any-unknown` | Wire frames typed `unknown` from `JSON.parse` need explicit narrowing |
| Nominal typing only | `arkts-no-structural-typing` | Discriminated unions on plain shapes fail; need `class` with explicit `implements` |
| No destructuring declarations | `arkts-no-destruct-decls` | `const {data, metadata} = result` doesn't compile |
| No destructuring assignment | `arkts-no-destruct-assignment` | Same for `({a,b} = obj)` |
| No `delete` | `arkts-no-delete` | Rarely needed |
| No `in` operator | `arkts-no-in` | Use `Object.hasOwn` or property check on declared class |
| No `Symbol` except `Symbol.iterator` | `arkts-no-symbol` | — |
| No function expressions | `arkts-no-func-expressions` | Arrow only |
| No generic arrow functions | — | `<T>(x: T) => x` doesn't parse — use named generic function |
| No regex literals | — | Use `new RegExp("...")` |
| No class expressions | — | — |
| No index signatures | `arkts-no-indexed-signatures` | `{ [key: string]: T }` types fail |
| No property-access-by-index | `arkts-no-props-by-index` | `obj["key"]` fails on declared classes |
| No dynamic object layout | — | Cannot add/remove properties at runtime |

**Not yet [VERIFY]'d but widely reported**: `Reflect.*`, `Object.keys`/`entries`, prototype manipulation, computed property names, dynamic `import()`, `eval`/`Function`. Cookbook V14 is auth-gated; list above is best-effort from third-party mirrors.

### 11.2 Why shared source is the wrong call

Even with careful TS, the *types* in `core/src/types.ts` use patterns ArkTS rejects:

- `ToolResult<T>` discriminated union on `status` — ArkTS rejects structural unions; needs `class ToolResultSuccess<T> implements IToolResult` + `class ToolResultError implements IToolResult` with manual `kind()` method.
- `tools.find(t => t.id === id)` — fine, but `[...tools]` spreading uses `Symbol.iterator` indirectly; some array methods have ArkTS-friendly equivalents but patterns diverge.
- Anywhere `unknown` appears (JSON parse output, opaque cursor) needs reshaping.

Per [ISSTA 2025 ArkAdapter paper](https://dl.acm.org/doi/10.1145/3728941), automated TS→ArkTS porting succeeds on 88.6% of attempted libraries — meaning ~11% need hand intervention even with the tool. Doing this on every release is wrong-shaped work.

**Better**: hand-write `@atd-protocol/arkts-client` against the same wire fixtures. Protocol surface is small (~18 types, ~10 client methods); duplication is one-time + minor protocol-evolution touch-up.

### 11.3 ArkTS package architecture

```
typescript/packages/arkts-client/
├── oh-package.json5
├── src/
│   ├── main/ets/
│   │   ├── types.ets        # class-based, nominal-typed mirror of core/src/types.ts
│   │   ├── errors.ets       # error class hierarchy
│   │   ├── client.ets       # AtdClient — same method names as TS client
│   │   ├── http.ets         # HttpTransport using @ohos.net.http (RCP)
│   │   ├── ws.ets           # WsTransport using @ohos.net.webSocket
│   │   ├── ucan.ets         # UCAN-lite verify via @kit.CryptoArchitectureKit
│   │   └── sanitize.ets     # name sanitization
│   └── test/
└── README.md
```

**Distribution**: OHPM, not npm. Package name `@atd-protocol/arkts-client`. OHPM supports scoped names; native `.so` can ride in HAR `libs/<abi>/` (relevant only for P1-3 ohos-rs path).

**HTTP transport via RCP** sketch:

```ts
import { rcp } from '@kit.RemoteCommunicationKit'

class HttpTransport implements Transport {
  private session: rcp.Session
  async request(req: AtdRequest): Promise<AtdResponse> {
    const jsonRpc = atdRequestToJsonRpc(req)
    const httpReq = new rcp.Request(this.url, "POST", {
      "content-type": "application/json",
      "authorization": `Bearer ${this.bearer}`,
    }, JSON.stringify(jsonRpc))
    const httpResp = await this.session.fetch(httpReq)
    return jsonRpcToAtdResponse(JSON.parse(httpResp.body.toString()))
  }
}
```

**WebSocket** (future): `@ohos.net.webSocket` is first-class in HMOS 6.1. Callback-based `connect/send/close` + event listeners. Wraps cleanly into Transport interface.

**[VERIFY] RCP SSE support**: research found no documentation of streaming-response support in RCP. If we ever add SSE streaming to ATD HTTP transport, ArkTS may not be able to consume it.

### 11.4 Source-of-truth strategy

The two TS packages and the Rust SDK consume the same protocol fixtures (`tests/fixtures/`). Drift surfaces in CI:

```
   Rust crates/atd-conformance ──┐
                                  ├──→ tests/fixtures/*.bin  (golden frames)
   TS @atd-protocol/conformance ─┘
                                  ├──→ Same fixtures, replayed
   ArkTS @atd-protocol/arkts-client tests ─┘
```

CI runs all three test suites against the same fixtures on every PR. If anyone's deserialization diverges, fixture replay fails.

**Decision input:**
- `@atd-protocol/arkts-client` is standalone, not generated from / sharing source with `@atd-protocol/client`.
- Native crypto via `@kit.CryptoArchitectureKit`.
- HTTP via `@kit.RemoteCommunicationKit`; WebSocket via `@ohos.net.webSocket`.
- Shared test fixtures are the only cross-language coupling.
- **[VERIFY]** before designing: RCP SSE/streaming, `@ohos.net.socket` UDS support, ArkTS-banned-list precision against Cookbook V14.

---

## 12. HMOS 6.1 fact-check against the issue

The issue (§6) makes several claims about HMOS ecosystem. Research findings:

| Issue claim | Status | Notes |
|---|---|---|
| ohos-rs `ohrs@1.2.0` shipped 2026-05-12 | ✅ **Confirmed** | GitHub releases verified; fork of napi-rs; arm64/arm/x86_64; MSRV 1.88. |
| Agent Framework Kit GA, 4 modes (LLM / 工作流 / A2A / OpenClaw) | ⚠ **Partially refuted** | Kit GA with HMOS 6.0 (Nov 2025). Surface: `FunctionComponent`, `FunctionController`. OpenClaw + A2A references found; **4-mode taxonomy not in any official source** — community summary. |
| Intents Kit `insight_intent.json` declarative app capability | ✅ **Confirmed** | Path: `entry/src/main/resources/base/profile/insight_intent.json`. Fields: `intentName`, `domain`, `intentVersion`, `srcEntry`, `uiAbility.ability`, `executeMode`. |
| Intents Kit can auto-translate to ATD tool schema | ⚠ **Aspirational** | Schema is declarative — mappable. But **runtime enumeration API** is undocumented; **third-party agent invocation** (non-小艺) is also undocumented. The 自动转译 path is plausible but un-validated. |
| Three plugin classes (端 / 云 / MCP) | ⚠ **Refuted as official** | 小艺开放平台 official taxonomy: **6 categories** (智能体 / 知识库 / 工作流 / 资源 / 插件 / 卡片). "MCP/端/云" is community summary, marketing-tier backing. |
| MCP only "consume-direction" supported; ArkTS in-process MCP server is a gap | ✅ **Confirmed by absence** | No `@kit.MCPClient`/`@kit.MCPServer` package found anywhere. Only HMOS↔MCP artifacts are external dev tools (`HarmonyOS-mcp-server`, `harmonyos-dev-helper-mcp`). The gap atd-ts can fill is real. |
| HTTP / WebSocket / SSE are day-1 stdlib | ⚠ **Partially confirmed** | HTTP via `@kit.RemoteCommunicationKit` (RCP); WebSocket via `@ohos.net.webSocket`. **SSE: no evidence found**. |
| `@kit.MCPClient`-style package name | 🔴 **Refuted** | No such package. |
| ArkTS strict subset, can't reuse Node TS SDK | ✅ **Confirmed** | Migration rules + ISSTA paper independently confirm. |
| HMS MLKit / HiAI fully absorbed into AI Kit | ⚠ **Refuted as "fully"** | HiAI Foundation Kit, MindSpore Lite Kit, NN Runtime Kit, Computer Vision Kit still exist as separate kits in HMOS 6.x alongside Harmony Intelligence umbrella. |
| Native WebRTC entry | ⚠ **Unverifiable** | Search surfaced only WebView-embedded WebRTC + NAPI patterns. No `@ohos.webrtc`-style first-class module. |

### 12.1 Adjustments to the issue's positioning

The issue argues atd-ts has three OH-specific value props (§6.2 + §6.3):

1. **OH 2026 has agent-first roadmap** — true.
2. **MCP downlink is empty** — true (no in-process ArkTS MCP server SDK). atd-ts is a real fit.
3. **Intents Kit auto-translation is the killer differentiator** — **partially aspirational**. Declarative schema is real; runtime enumeration + third-party invocation paths are undocumented. A first pass would have to register translations at build time, not runtime. Full vision requires validating two HMOS APIs without public docs.

**Recommendation**: when SP starts, carry forward (1) and (2) as decision drivers; treat (3) as "Phase 2 stretch goal pending HMOS API validation" rather than v1 scope.

### 12.2 Verification path

Before any `SP-arkts-client-v1` ships, validate:
1. Sign up for a Huawei developer account → fetch Cookbook V14 official restrictions list.
2. Test on HMOS 6.1 emulator or real device:
   - `@ohos.net.socket` UDS support (does it expose `AF_UNIX`?)
   - RCP SSE / streaming behavior
   - `@kit.CryptoArchitectureKit` Ed25519 sign/verify performance
   - Intents Kit runtime enumeration (is there an API?)
3. Survey: does any HMOS app in production use `ohos-rs` ≥ 1.0? File issue on `ohos-rs/ohos-rs` asking for adopter list.

These verifications shape the spec, not the research.

---

## 13. Testing & conformance

### 13.1 TS test stack

- **Vitest** — workspace config + per-package overrides. Browser-mode for browser entry.
- **MSW** for wire-mocked tests — intercepts at network layer, same mocks in Node + browser builds.
- **`supertest` or `undici.MockAgent`** for HTTP transport unit tests.

### 13.2 Cross-language conformance

Rust `atd-conformance` exists already. Mirror in TS:

```
typescript/packages/conformance/src/
├── runner.ts         # atd-conformance-ts <transport-url> [--scenarios=…]
├── scenarios/        # one .ts per scenario, mirrors Rust crate's fixtures
└── fixtures/         # symlink/copy from ../../tests/fixtures
```

**Source of truth: `tests/fixtures/`** — pre-recorded wire frames committed once, replayed by:
- Rust `atd-conformance`
- TS `@atd-protocol/conformance`
- ArkTS `arkts-client` tests

When protocol evolves, fixture is regenerated in one place (run against Rust ref-server), all three suites re-run.

### 13.3 CI matrix

```yaml
# .github/workflows/ts.yml — sketch
jobs:
  test:
    strategy:
      matrix:
        node: ["20.19", "22"]
        runtime: ["node", "browser"]
    steps:
      - run: pnpm install --frozen-lockfile
      - run: pnpm -r test
      - run: pnpm -F @atd-protocol/conformance run conformance:ref-server
      - run: pnpm exec attw --pack packages/client
      - run: pnpm exec publint packages/client
      - run: pnpm exec barrelClean packages/client
```

### 13.4 Recorded fixture format

```
tests/fixtures/scenarios/discover-and-call-fs-read/
├── description.md       # human prose
├── frames.jsonl         # canonical wire frames, one per line
├── http-rpc.jsonl       # equivalent HTTP JSON-RPC envelopes
└── expected-result.json # client-side observation
```

Both binary (length-prefixed) and HTTP-JSON-RPC representations committed so each transport's tests use the same scenario.

**Decision input:**
- Vitest + MSW.
- `@atd-protocol/conformance` mirrors Rust `atd-conformance`, both consume `tests/fixtures/`.
- `attw` + `publint` + `barrelClean` are pre-publish gates.
- Test matrix covers Node 20.19 + 22, browser entry under Vitest browser-mode.

---

## 14. MCP TS SDK borrow list

The MCP SDK is the nearest neighbor and is mature (12.5k stars, v2 in flight).

### ✅ Adopt

1. **Monorepo with private `core` barrel + curated `/public`** — refactor internals freely.
2. **Subpath exports + conditional `browser`/`workerd`/`deno`** for runtime-specific entries.
3. **`barrelClean` test** — guard root-entry runtime-neutrality with code, not docs.
4. **Hand-written wire types + spec-fetch CI drift check** — bidirectional drift visibility.
5. **Two-channel error model**: throw protocol errors, return tool errors.
6. **Pluggable JSON Schema validator** — only relevant if/when we add input validation.
7. **Catalog-pinned monorepo deps + N-day quarantine on new dep versions** — supply-chain hygiene (MCP uses 7 days).
8. **`REVIEW.md` with stable principles + auto-grown "Recurring Catches"** — brilliant low-effort knowledge management.
9. **Spec-parity tests across language SDKs** — formalize between Rust + TS via shared fixtures.
10. **`tsdown` over `tsup`** — successor; same config shape; 3–5× faster.

### ❌ Avoid (footgun catalog from MCP issues)

1. **Don't ship dual ESM/CJS sloppily** — separate `.d.cts`, run `attw` in CI. (MCP #2011)
2. **Don't import Node builtins from root entry** — even tree-shaken, some bundlers parse-fail. (MCP #2077)
3. **Don't conflate HTTP 400 (missing session) with 404 (unknown session)** — distinct semantics for retry.
4. **Don't broadcast `-32700/-32602` from broad catches** — caller-fault codes; server-internal failures map to `-32603`.
5. **Don't trust JSON for U+2028/U+2029** — sanitize tool text output. (MCP #2155)
6. **Don't ship a builder/registry/middleware engine in the SDK.** Userland.
7. **Don't `await` user callbacks without `try/finally` in shutdown paths.** A throw leaks the connection half-open.
8. **Don't put Zod in both `dependencies` AND `peerDependencies`.** (MCP #2011)
9. **Don't use class decorators / class-as-DSL** for tool/handler registration. Object-literal + function is more portable (ArkTS-compatible).

---

## 15. Open questions for SP design

The research nailed down defaults but left these as fork-in-the-road decisions:

### For an eventual `SP-ts-client-v1`

1. **Transport interface scope** — does v1 ship `WsTransport` even though `atd-server-http` doesn't have WebSocket yet? **Lean toward: wait**, ship 3 transports; WS lands when server adds it.
2. **Sync API surface** — Python ships `AtdClientSync`. TS no equivalent need (async-native). **Lean toward: document async pattern in README; no API surface.**
3. **Browser bundle size budget** — concrete number? **Lean toward: ≤25 KB minified gzipped for browser entry** (`HttpTransport` + `AtdClient` + types + UCAN-lite).
4. **Telemetry hooks** — `on('beforeRequest', …)` / `on('afterResponse', …)` for OpenTelemetry adopters? **Lean toward: yes, two callbacks at construct-time.** Retrofitting post-1.0 is painful.
5. **AbortSignal vs custom cancel** — standard `AbortSignal` only. (Decided.)
6. **UCAN minting API** — does client mint or only carry? **Lean toward: no minting in v1**; surfaces as separate `@atd-protocol/ucan` package later.
7. **Bring-your-own-fetch** — `HttpTransport` accepts a `fetch` option? **Lean toward: yes.** Vercel AI / Cloudflare adopters need this.

### For an eventual `SP-arkts-client-v1`

1. **OHPM publishing identity** — organization-level OHPM publisher account; **[VERIFY]** cost / requirements.
2. **HMOS minimum API level** — API 14 (HMOS 6.0) or API 22 (6.1)? **Lean toward: HMOS 6.1 / API 22** for RCP.
3. **Native crypto fallback** — if `@kit.CryptoArchitectureKit` unavailable, fall back to ported `@noble/ed25519`? **Lean toward: no fallback;** require native kit.
4. **DevEco compilation target** — ship `.ets` source or pre-compiled `.har`? **Lean toward: `.ets` source** so DevEco's strict-checker flags issues downstream.
5. **Test infrastructure** — ArkTS tests run only on emulator/device. **Open**: CI runs them how? Likely manual on-device validation gate at release.

---

## 16. Sizing estimate (NOT a schedule)

This section answers "if/when an SP starts, what does the engineering effort look like?" — sized once so future planners don't re-estimate from scratch. **Execution gating per §1.3 + §2 still applies.** A real adopter's constraints can collapse or extend any phase.

### SP-ts-client-v1 — sizing ~3-4 weeks

**Phase A — scaffolding (~3 days)**
- Set up `typescript/` monorepo, pnpm workspace, tsconfig base, vitest workspace
- `@atd-protocol/core` skeleton: types.ts (hand-written), errors.ts, sanitize.ts
- CI: type-check + lint + drift check vs `/atd-protocol-schema.json`

**Phase B — wire codec + Transport interface (~3 days)**
- `core/src/wire.ts` — 4-byte BE length codec
- `core/src/jsonrpc.ts` — HTTP JSON-RPC envelope
- `core/src/transport.ts` — Transport interface
- `core/src/sanitize.ts` — MCP name sanitization (port of Rust)
- Unit tests against committed fixtures

**Phase C — transports (~5 days)**
- `client/src/http.ts` — `HttpTransport`, BYO `fetch`
- `client/src/node/unix.ts` — `UnixSocketTransport` (retry + jitter)
- `client/src/node/tcp.ts` — `TcpTransport`
- `client/src/node/stdio.ts` — `StdioTransport` (`cross-spawn`)
- Tests with in-process mock servers

**Phase D — AtdClient (~5 days)**
- `client/src/client.ts` — public API surface
- ping / hello / discover / describe / call / callPage / callAll
- AtdError taxonomy fully wired
- Defensive deserialization

**Phase E — UCAN + cursors (~3 days)**
- `core/src/ucan.ts` — verify chain over `@noble/ed25519` + `@scure/base`
- `Ed25519Verifier` abstraction
- Cursor opaque-roundtrip in `callPage`/`callAll`
- Tests against known-good UCAN fixtures (generate from Rust)

**Phase F — adapters + conformance (~3 days)**
- `@atd-protocol/adapters` — openai/anthropic/langchain tool-schema converters
- `@atd-protocol/conformance` — TS conformance runner consuming `tests/fixtures/`
- Wire up `atd-mvp` GH Actions to run TS suite alongside Rust

**Phase G — publish (~2 days)**
- `attw` + `publint` + `barrelClean` green
- npm Trusted Publishing configured
- README, CHANGELOG, examples/
- First `0.1.0` publish

### SP-arkts-client-v1 — sizing ~2-3 weeks (after P0-1)

**Phase A — env + scaffolding (~3 days)**
- DevEco Studio install on dev machine; HMOS 6.1 emulator running
- `typescript/packages/arkts-client/` skeleton, `oh-package.json5`
- ArkTS lint in CI (best-effort; full strict checker is DevEco-only)

**Phase B — types + errors (~3 days)**
- `types.ets` — class-based nominal mirrors of `core/src/types.ts`
- `errors.ets` — error class hierarchy

**Phase C — transports (~5 days)**
- `http.ets` — HTTP via `@kit.RemoteCommunicationKit`
- `ws.ets` — WebSocket via `@ohos.net.webSocket`
- Mock-server tests on emulator

**Phase D — client (~3 days)**
- `client.ets` — public API matching `@atd-protocol/client`
- UCAN-lite verify via `@kit.CryptoArchitectureKit`
- ping, hello, discover, describe, call, callPage/callAll

**Phase E — conformance + publish (~3 days)**
- Run shared fixtures on emulator
- OHPM publish (requires HMOS dev account — [VERIFY] requirement)

---

## 17. References

### Authoritative (this repo)
- `docs/architecture.md` — ATD architecture overview
- `docs/protocol/wire-format.md` — wire-level spec
- `docs/protocol/error-codes.md` — error taxonomy
- `docs/quickstart/typescript.md` — planned API stub (pre-research)
- `docs/issues/2026-05-26-atd-ts-sdk-adopter-requirements.md` — the issue this research consolidates (read its §1 订正 callout first)
- `crates/atd-sdk/src/client.rs` — Rust client public surface (parity reference)
- `python/src/atd_client/` — Python client public surface (parity reference)
- `/atd-protocol-schema.json` — machine-readable wire schema

### MCP SDK (nearest neighbor)
- [@modelcontextprotocol/sdk on npm](https://www.npmjs.com/package/@modelcontextprotocol/sdk)
- [modelcontextprotocol/typescript-sdk on GitHub](https://github.com/modelcontextprotocol/typescript-sdk)
- MCP REVIEW.md (in repo) — design principles + recurring footgun catalog
- [MCP spec](https://modelcontextprotocol.io/specification)

### Modern TS SDK tooling (2026)
- [PkgPulse 2026 — tsup vs tsdown vs unbuild](https://www.pkgpulse.com/guides/tsup-vs-tsdown-vs-unbuild-typescript-library-bundling-2026)
- [PkgPulse 2026 — State of TypeScript Tooling](https://www.pkgpulse.com/guides/state-of-typescript-tooling-2026)
- [Liran Tal — TypeScript in 2025 with ESM and CJS](https://lirantal.com/blog/typescript-in-2025-with-esm-and-cjs-npm-publishing)
- [Joyee Cheung — require(esm) stable (2025)](https://joyeecheung.github.io/blog/2025/12/30/require-esm-in-node-js-from-experiment-to-stability/)
- [@arethetypeswrong/cli](https://github.com/arethetypeswrong/arethetypeswrong.github.io)
- [npm Trusted Publishing GA (Jul 2025)](https://github.blog/changelog/2025-07-31-npm-trusted-publishing-with-oidc-is-generally-available/)
- [Cloudflare Workers bundling — workerd condition](https://developers.cloudflare.com/workers/wrangler/bundling/)

### Sample protocol SDKs to study
- [openai-node](https://github.com/openai/openai-node) — dual ESM/CJS exports
- [anthropic-sdk-typescript](https://github.com/anthropics/anthropic-sdk-typescript) — Stainless-generated, parallel structure
- [stripe-node](https://github.com/stripe/stripe-node) — hand-written exports
- [vercel/ai](https://github.com/vercel/ai) — ESM-only Node ≥22 (greenfield extreme)

### Crypto + serialization
- [@noble/curves on npm](https://www.npmjs.com/package/@noble/curves) (Ed25519)
- [@noble/ed25519 on npm](https://www.npmjs.com/package/@noble/ed25519) (5KB, audited)
- [@noble/hashes on npm](https://www.npmjs.com/package/@noble/hashes)
- [@scure/base on npm](https://www.npmjs.com/package/@scure/base) (base58/base64url)
- [cborg on GitHub](https://github.com/rvagg/cborg) — strict deterministic CBOR
- [Igalia Aug 2025 — Ed25519 lands in Chrome](https://blogs.igalia.com/jfernandez/2025/08/25/ed25519-support-lands-in-chrome-what-it-means-for-developers-and-the-web/)
- [How to Handle Binary Protocols Over TCP in Node.js (OneUptime, Jan 2026)](https://oneuptime.com/blog/post/2026-01-25-binary-protocols-tcp-nodejs/view)
- [frame-stream](https://github.com/davedoesdev/frame-stream)

### Runtime validation
- [Zod vs Valibot vs ArkType 2026 — Pockit](https://pockit.tools/blog/zod-valibot-arktype-comparison-2026/)
- [json-schema-to-typescript](https://github.com/bcherny/json-schema-to-typescript)

### Testing
- [Vitest comparisons](https://vitest.dev/guide/comparisons.html)
- [MSW](https://mswjs.io/)

### HarmonyOS / ArkTS
- [TS→ArkTS Migration Cookbook V14](https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V14/typescript-to-arkts-migration-guide-V14) (auth-gated)
- [awesome-harmonyos ArkTS rules mirror](https://github.com/HarmonyOS-Next/awesome-harmonyos/blob/main/Adaptation_rules_from_TypeScript_to_ArkTS.md)
- [ISSTA 2025 — Porting Libraries to OpenHarmony (ArkAdapter)](https://dl.acm.org/doi/10.1145/3728941)
- [ohos-rs](https://github.com/ohos-rs/ohos-rs) (`ohrs@1.2.0`, 2026-05-12)
- [Agent Framework Kit guide (auth-gated)](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/harmony-agent-framework-kit-guide)
- [Agent Framework Kit hands-on (CSDN)](https://harmonyosdev.csdn.net/697715ac7c1d88441d8fa817.html)
- [Intents Kit reference](https://blog.csdn.net/weixin_69135651/article/details/143602146)
- [@ohos.net.webSocket reference](https://developer.huawei.com/consumer/en/doc/harmonyos-references/js-apis-websocket)
- [@ohos.net.socket reference](https://developer.huawei.com/consumer/en/doc/harmonyos-references/js-apis-socket)
- [Remote Communication Kit (RCP) migration](https://kitemetric.com/blogs/mastering-harmonyos-network-requests-transitioning-to-the-rcp-based-approach)
- [HarmonyOS Crypto Architecture Kit overview](https://dev.to/harmonyos/what-is-the-crypto-architecture-kit-47e)
- [ArkTS Crypto APIs reference](https://developer.huawei.com/consumer/en/doc/harmonyos-references/crypto-architecture-arkts)
- [queueit/harmony-sdk](https://github.com/queueit/harmony-sdk) — closest public ArkTS port template
- [HMOS 6 launch coverage — Gizmochina](https://www.gizmochina.com/2025/06/21/huawei-unveils-harmonyos-6-with-ai-agent-support/)
- [napi-rs ArkTS demo](https://github.com/stuartZhang/Arkts-NAPI-Rust-Demo)

---

*End of research. This document does not commit to execution; see §1.3 + §2 for gating rationale. The §16 phasing is a sizing artifact, not a schedule.*
