# anos-tool-dispatch

> ATD v1.0 tool registry with 20 built-in tools, MCP client, and REST bindings.

**Layer:** Runtime
**Dependencies:** `anos-types`, `anos-capability`, `anos-identity`, `anos-resource-metering`
**Design doc:** [runtime-tool-dispatch.md](../architecture/runtime-tool-dispatch.md)

## Overview

Implements the Agentic Tool Dispatch (ATD) v1.0 protocol. Manages tool lifecycle (registration, health monitoring, circuit breaking), dispatches tool calls with capability verification, and supports three binding types: built-in Rust tools, MCP (Model Context Protocol) remote tools, and REST API tools.

## Modules

| Module | Purpose |
|--------|---------|
| `binding_mcp` | `McpClient` — MCP protocol client for remote tools |
| `binding_rest` | `execute_rest`, `RestBindingConfig` — REST API tool binding |
| `builtins` | 20 built-in tool definitions and execution |
| `circuit_breaker` | Per-tool circuit breaker (Closed → Open → HalfOpen) |
| `health` | `HealthTracker`, `HealthMetrics` — tool health monitoring |
| `persistent` | `PersistentToolRegistry` — SQLite-backed tool persistence |
| `registry` | `ToolRegistry`, `ToolLifecycle` — tool registration and lifecycle |

## Key Types

- **`ToolRegistry`** — Central registry for all tools (built-in + external)
- **`PersistentToolRegistry`** — SQLite-backed persistent registry
- **`McpClient`** — Connects to MCP servers for remote tool access
- **`CircuitBreaker`** — Per-tool circuit breaker with configurable thresholds
- **`HealthTracker`** — Monitors tool response times and error rates

## 20 Built-in Tools

Registered via `builtin_definitions()` and executed via `execute_builtin()`.

## Usage

```rust
use anos_tool_dispatch::{ToolRegistry, builtin_definitions, execute_builtin};

let mut registry = ToolRegistry::new();
for def in builtin_definitions() {
    registry.register(def)?;
}
```
