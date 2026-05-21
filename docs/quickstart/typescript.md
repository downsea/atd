# TypeScript Quickstart — ATD Client SDK

> **NOT SHIPPED in 1.0.** The TypeScript SDK does not exist yet. This document describes the planned API, the current workaround for TypeScript consumers, and where to watch for updates. Do not write code against the API shown here — it will change before it ships.

---

## Status

The ATD TypeScript SDK is a post-1.0 deliverable in the project roadmap (see [`../roadmap.md`](../roadmap.md)). It is **not implemented in the 1.0 release**.

There is no `typescript/` directory in this repository. The `npm` package `@atd-protocol/client` does not yet exist.

If you are a TypeScript developer who needs to call ATD tools today, see the [Interim workaround](#interim-workaround) section below.

---

## Planned API

The following interface represents the intended public API shape — **not yet implemented, subject to change before release**.

```typescript
// typescript/src/client.ts  (planned, does not exist yet)
export class AtdClient {
    static async connect(endpoint: string | Endpoint): Promise<AtdClient>;
    discover(query?: string, filter?: DiscoverFilter): Promise<ToolSummary[]>;
    describe(toolId: string): Promise<ToolDefinition>;
    call<T = unknown>(toolId: string, args: object, opts?: CallOptions): Promise<ToolResult<T>>;
}
```

**Notes on the planned design:**

- `endpoint` will accept either a string socket path or an `Endpoint` object, mirroring the Rust SDK's `Endpoint::unix(path)` constructor.
- `discover` will support the same `query` + `filter` parameters as the Rust and Python SDKs. Filtering will be applied client-side.
- `call` uses a generic `T` type parameter for the success data payload. The default is `unknown`, which requires callers to narrow the type themselves.
- `ToolResult<T>` will be a discriminated union: `{ success: true; data: T; metadata: ToolResultMetadata } | { success: false; code: string; message: string; retryable: boolean }`.
- Transport in Phase 1 will be Unix socket (same as Rust/Python). HTTP transport landed 2026-05-11 on the server side (`atd-server-http` crate, SP-streamable-http + SP-1.B); the TS SDK can target either Unix socket (matching Rust/Python ergonomics) or HTTP (matching browser/Node fetch ergonomics) once concrete TS adopter requirements surface.
- The package will be published as `@atd-protocol/client` on npm.

The full planned API including `DiscoverFilter`, `CallOptions`, `ToolSummary`, `ToolDefinition`, and the error types follows the same semantics as the Rust reference implementation. See `crates/atd-sdk/src/` for the authoritative behavior specification.

**Planned supporting types (preview — not yet implemented):**

```typescript
interface Endpoint {
    kind: "unix";
    path: string;
}

interface DiscoverFilter {
    domain?:     string;
    tier?:       "hot" | "warm" | "cold";
    visibility?: "read" | "write" | "dangerous" | "internal";
    limit?:      number;
}

interface CallOptions {
    dryRun?:           boolean;
    preferredBinding?: "Cli" | "Mcp" | "Rest" | "AppFunction";
}

interface ToolSummary {
    id:          string;       // e.g. "ref:echo.say"
    name:        string;
    description: string;
    domain:      string;       // e.g. "echo", "fs"
    tier:        "hot" | "warm" | "cold";
    visibility:  "read" | "write" | "dangerous" | "internal";
    tags:        string[];
    inputSchema: object | null;
}

type ToolResult<T = unknown> =
    | { success: true;  data: T;      metadata: { toolId: string } }
    | { success: false; code: string; message: string; retryable: boolean; reason?: string };
```

**Planned error types (preview):**

```typescript
class AtdError extends Error {}
class ToolNotFound        extends AtdError { toolId: string; suggestions: string[] }
class InvalidArguments    extends AtdError { toolId: string; field: string; reason: string }
class CapabilityDenied    extends AtdError { toolId: string; required: string[]; granted: string[] }
class BindingUnavailable  extends AtdError { toolId: string; tried: string[]; reason: string }
class ToolExecutionFailed extends AtdError { toolId: string }
class Timeout             extends AtdError { toolId: string; afterMs: number }
class ServerUnreachable   extends AtdError {}
class ProtocolError       extends AtdError { expected: string; got: string }
```

The `is_retryable` / `suggest_fix` behavior from the Rust SDK will be exposed as instance methods `isRetryable(): boolean` and `suggestFix(): string | undefined`.

**Planned usage sketch (will not compile until the SDK ships):**

```typescript
// This is illustrative only. The package does not exist yet.
import { AtdClient } from "@atd-protocol/client";

const client = await AtdClient.connect("/path/to/atd.sock");

const tools = await client.discover("echo");
console.log(tools.map(t => t.id));

const result = await client.call<{ echo: string }>(
    "ref:echo.say",
    { text: "hello from TypeScript" },
);

if (result.success) {
    console.log(result.data.echo);
} else {
    console.error(`[${result.code}] ${result.message}`);
}
```

---

## Interim workaround

TypeScript developers have two options today.

### Option 1: MCP bridge (recommended)

`atd-mcp-bridge` is a binary that speaks the ATD wire protocol toward an ATD server and presents an MCP (Model Context Protocol) interface outward. Any MCP-capable TypeScript client can connect to it.

Build the bridge:

```bash
cargo build --release -p atd-mcp-bridge
```

Configure your TypeScript MCP client to launch the bridge as a stdio server:

```json
{
  "mcpServers": {
    "atd": {
      "command": "/path/to/atd/target/release/atd-mcp-bridge",
      "env": {
        "ATD_SOCK": "<YOUR_SOCKET_PATH>"
      }
    }
  }
}
```

Replace `<YOUR_SOCKET_PATH>` with the path to your ATD server socket.

The bridge translates MCP `tools/list` → ATD `discover`, MCP `tools/call` → ATD `call`. Tool names are sanitized (`ref:echo.say` → `ref_echo_say`) to satisfy MCP's identifier constraints.

For detailed MCP client configuration examples (Claude Desktop, Claude Code, Cursor), see [`docs/integrations/claude-code.md`](../integrations/claude-code.md).

### Option 2: Implement the wire protocol directly

The ATD wire protocol is documented in [`docs/protocol/wire-format.md`](../protocol/wire-format.md). It is a simple length-prefixed JSON protocol over a Unix socket. The Rust implementation in `crates/atd-protocol/src/` is the reference.

The framing is:

1. Write a 4-byte big-endian `u32` containing the JSON body length in bytes.
2. Write the JSON body (UTF-8).
3. Read a 4-byte big-endian `u32` for the response length.
4. Read and parse the response JSON.

A minimal TypeScript implementation of this framing over a Unix socket is straightforward using Node.js's `net.createConnection`. The protocol message types are defined in `docs/protocol/wire-format.md`.

---

## When will the TypeScript SDK ship?

There is no committed timeline. The TypeScript SDK is tracked as a post-1.0 deliverable; the 1.0 release ships the Rust + Python reference implementation.

The sequencing rationale: the Rust SDK is the protocol reference. Python followed immediately because LangChain integration was an early validation goal. TypeScript follows because the ecosystem demand is real but no shipped integration test required it yet. The `atd-mcp-bridge` covers the immediate TypeScript use case (via any MCP-capable client) without requiring a native SDK.

To follow progress:

- Watch the repository for a `typescript/` directory to appear.
- Watch for the `@atd-protocol/client` package on npm.
- The [`../roadmap.md`](../roadmap.md) file tracks evolution scope and deferred features.
- Tracked gaps live as issues under [`../issues/`](../issues/).

---

## See also

- **Rust quickstart:** [`docs/quickstart/rust.md`](rust.md) — the reference SDK implementation. The TypeScript SDK will mirror this API.
- **Python quickstart:** [`docs/quickstart/python.md`](python.md) — async and sync clients, LangChain/OpenAI/Anthropic adapters.
- **Wire protocol:** [`docs/protocol/wire-format.md`](../protocol/wire-format.md) — full protocol specification for direct implementers.
- **MCP bridge integration:** [`docs/integrations/claude-code.md`](../integrations/claude-code.md) — using `atd-mcp-bridge` with MCP-capable clients.
