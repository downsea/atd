# ATD — ANOS Tool Definition v1.0

> 统一工具标准：4 种协议（CLI/MCP/REST/AppFunction）→ 1 个 Schema。
> Agent 不关心工具的实现协议，只关心工具能做什么。
> 工具是 Agent 的系统调用 —— Skill 编排意图，Tool 执行动作。

**Layer**: Runtime (cross-cutting)
**Crate**: `anos-tool-dispatch`, `anos-runtime`
**设计原则**: P2 (Intent-Driven), P4 (Schema-as-Truth)
**实现状态**: 🟢 核心已实现 (94 built-in + 10 host plugins + MCP bridge)
**设计原文**: `docs/design/anos-tool-standard.md`

---

## 1. Problem & Positioning

### 1.1 为什么需要统一工具标准

ANOS 生态中存在三大工具协议：

| 协议 | 来源 | 调用方式 | 优势 | 局限 |
|------|------|---------|------|------|
| **CLI** | Agent Native CLI | `mobile camera +photo --rear` | 结构化输出、dry-run、七原则 | 进程启动开销 |
| **MCP** | Model Context Protocol | JSON-RPC `tools/call` | 标准化、跨语言、流式 | 需服务端运行 |
| **AppFunction** | Android/HarmonyOS | IPC `executeAppFunction()` | OS 原生集成、强类型 | 平台锁定、粗粒度权限 |
| **REST** | HTTP API | `POST /api/v1/tools/...` | 通用、跨网络 | 延迟高、需认证 |

Agent 不应关心工具的实现协议——它只需要知道"拍照"这个工具存在、如何调用、返回什么。**ANOS 统一工具标准（ATD）** 在四个协议之上提供一层抽象：

```
Agent 视角:
  "我需要拍一张照片"
      │
      ▼
  ATD: anos:camera.capture.photo
      │
      ├── 绑定 1: CLI → mobile camera +photo --rear
      ├── 绑定 2: MCP → tools/call capture_photo
      ├── 绑定 3: AppFunction → CameraFunctions.takePhoto()
      └── 绑定 4: REST → POST /api/v1/tools/camera/capture
```

### 1.2 OpenClaw 工具爆炸问题

随着 MCP 生态爆发，单个 Agent 可访问的工具数量从几十个增长到数千个。直接暴露所有工具给 LLM 会导致：

- **Context 爆炸**: 每个工具 schema 约 150 token，1000 个工具 = 150K token（超出大多数模型上下文窗口）
- **选择困难**: LLM 在大量工具中选择正确工具的准确率急剧下降
- **安全失控**: 无法对工具进行分级管控

ATD 通过三级容量模型（Hot/Warm/Cold）和可见性分级（Read/Write/Dangerous/System）解决这些问题。

### 1.3 Skill vs Tool 区别

| 维度 | Skill（技能） | Tool（工具） |
|------|-------------|-------------|
| **OS 类比** | 程序/脚本 | 系统调用 |
| **本质** | 自然语言指令集（SKILL.md） | 可执行函数（typed schema） |
| **粒度** | 任务级（"拍一张照片并分析"） | 操作级（"调用相机 API"） |
| **面向** | Agent 理解和推理 | Agent Runtime 执行 |
| **组合方式** | DAG 编排（Recipe） | 管道组合（Pipe） |
| **发现机制** | 四层渐进式（Intent → Index → Schema → Load） | 三级容量模型（HOT → WARM → COLD） |
| **信任模型** | L0-L4 五级信任 | 能力令牌（Capability Token） |
| **定义格式** | SKILL.md（Markdown + YAML frontmatter） | ATD（YAML/JSON） |

**协作关系**:

```
用户意图: "拍一张白板照片并总结笔记"
     │
     ▼
Skills 层（编排）:
  1. 意图理解 → 需要两个 Skill
  2. 发现 @anos/camera-photo → 加载 SKILL.md
  3. 发现 @anos/image-analyze → 加载 SKILL.md
  4. 构建 DAG: camera-photo → image-analyze
     │
     ▼
Tools 层（执行）:
  Skill: @anos/camera-photo 调用:
    Tool: anos:camera.capture.photo (ATD)
      → 路由到 CLI 绑定: mobile camera +photo --rear
      → 返回: { file_path: "/tmp/photo.jpg", width: 4032, height: 3024 }

  Skill: @anos/image-analyze 调用:
    Tool: anos:ai.vision.analyze (ATD)
      → 路由到 MCP 绑定: tools/call analyze_image
      → 返回: { text: "白板上写着 Q3 规划...", confidence: 0.94 }
```

**关键设计边界**:

| 关注点 | Skills 系统负责 | Tool 标准负责 |
|--------|---------------|-------------|
| **发现** | 语义发现（自然语言意图 → HNSW 索引） | 能力发现（平台/协议/可用性过滤） |
| **编排** | DAG 编排（顺序/并行/条件/循环） | 管道组合（output type → input type） |
| **执行** | 委托给 Tool Dispatcher | 验证 → 路由 → 执行 → 规范化 |
| **安全** | 安全级别分类（read/write/financial...） | 能力令牌验证 + 速率限制 + 成本预算 |
| **质量** | 质量指标（成功率/延迟/评分） | 健康监控（熔断/降级/回退） |
| **版本** | Skill semver + 破坏性变更检测 | Tool semver + 绑定版本协调 |

---

## 2. ATD v1.0 Schema

### 2.1 Full Schema (Key Fields)

```rust
struct AtdDefinition {
    atd_version: String,               // "1.0"

    // Identity
    id: ToolId,                        // "anos:camera.capture.photo"
    version: SemVer,
    name: String,
    description: String,

    // Capability vector (for discovery)
    capability: CapabilityDescriptor {
        domain: String,                // "camera"
        actions: Vec<String>,          // ["capture", "photo"]
        tags: Vec<String>,
        intent_examples: Vec<String>,  // for embedding generation
    },

    // Schemas
    input: JsonSchema,                 // JSON Schema Draft 2020-12
    output: JsonSchema,
    errors: Vec<ErrorDef>,

    // Protocol bindings
    bindings: Bindings {
        cli: Option<CliBinding>,
        mcp: Option<McpBinding>,
        appfunction: Option<AppFunctionBinding>,
        rest: Option<RestBinding>,
    },

    // Safety classification
    safety: SafetyConfig {
        level: SafetyLevel,            // Read | Write | Financial | Privacy | Physical | Destructive
        requires_confirm: bool,
        supports_dry_run: bool,
        data_sensitivity: DataSensitivity,
        side_effects: Vec<SideEffect>,
    },

    // Resource constraints
    resources: ResourceConfig {
        timeout_ms: u32,
        max_concurrent: u8,
        rate_limit: RateLimit,
        estimated_tokens: u16,         // context cost of this tool's compact ATD
        estimated_latency_ms: u32,
    },

    // Trust & provenance
    trust: TrustConfig {
        publisher: String,
        trust_level: u8,               // L0–L4
        signature: Ed25519Signature,
    },

    // Compatibility
    compatibility: PlatformConfig {
        platforms: Vec<Platform>,
        requires_capabilities: Vec<CapabilityName>,
        requires_hardware: Vec<HardwareName>,
        offline_capable: bool,
    },

    // Fallback
    fallback: Option<FallbackConfig>,
}
```

### 2.2 ATD ID Naming Convention

```
Format: anos:<domain>.<resource>.<action>[.<variant>]

Examples:
  anos:camera.capture.photo           // take photo
  anos:camera.capture.photo.burst     // burst mode variant
  anos:health.heartrate.measure       // measure heart rate
  anos:ai.vision.analyze              // image analysis
  anos:system.settings.wifi.toggle    // toggle WiFi

Third-party:
  vendor:huawei:health.spo2.measure
  community:smart-home:light.control

Host plugins:
  host:media.convert                  // ffmpeg
  host:data.json_query                // jq

MCP external:
  mcp:server-filesystem.read_file     // MCP server tool
```

### 2.3 Compact ATD (~150 tokens, for Hot tier)

```yaml
- id: "anos:camera.capture.photo"
  name: "Take Photo"
  desc: "Capture a photo using device camera"
  input: { camera_id: "rear|front|wide|macro", resolution: "low|medium|high|max" }
  output: { file_path: str, mime_type: str, width: int, height: int }
  safety: "write"
  cost: "free"
```

### 2.4 Deferred ATD (~30 tokens, for Warm tier index)

```yaml
- id: "anos:camera.capture.photo"
  name: "Take Photo"
  domain: "camera"
  safety: "write"
```

### 2.5 Complete Working Example: `fs.read`

```yaml
atd_version: "1.0"
tool:
  id: "anos:fs.read"
  version: "1.0.0"
  name: "Read File"
  description: "Read a file from the filesystem. Returns file content as text. For binary files, returns base64-encoded content."

  capability:
    domain: "fs"
    actions: ["read"]
    tags: ["filesystem", "io", "read-only"]
    intent_examples:
      - "read a file"
      - "show me the contents of"
      - "cat file"
      - "读取文件内容"

  input:
    type: object
    properties:
      path:
        type: string
        description: "Absolute path to the file to read"
      offset:
        type: integer
        description: "Line number to start reading from (0-based)"
      limit:
        type: integer
        description: "Maximum number of lines to read"
    required: ["path"]

  output:
    type: object
    properties:
      content:
        type: string
        description: "File content as text"
      lines:
        type: integer
        description: "Total number of lines in the file"
      truncated:
        type: boolean
        description: "Whether the output was truncated"

  safety:
    level: read
    requires_confirm: false
    supports_dry_run: false

  resources:
    timeout_ms: 10000
    max_concurrent: 10
    rate_limit: { max: 120, window_secs: 60 }
    estimated_tokens: 120

  trust:
    publisher: "anos"
    trust_level: 4
```

### 2.6 Unified Result Format

```rust
enum ToolResult {
    Success {
        data: Value,                   // validated against ATD output schema
        metadata: ResultMeta {
            tool_id: ToolId,
            tool_version: SemVer,
            binding_used: Protocol,    // Cli | Mcp | AppFunction | Rest
            latency_ms: u32,
            timestamp: DateTime<Utc>,
            request_id: RequestId,
        },
    },
    Error {
        code: String,                  // ATD-defined error code
        message: String,
        reason: String,                // machine-readable
        retryable: bool,
        retry_after_ms: Option<u32>,
        binding_error: Option<Value>,  // raw protocol error for debugging
    },
}
```

---

## 3. Five-Layer Tool Architecture

```
┌────────────────────────────────────────────────────────────────┐
│  L5  Skills (workflow orchestration, SKILL.md)                 │
│       编排意图，DAG 工作流，自然语言定义                           │
│       可声明 requires_tools: [host:media.convert]              │
├────────────────────────────────────────────────────────────────┤
│  L4  Structured Tools (anos:*, host:*, mcp:* — JSON I/O)      │
│       无状态原子操作，ATD Schema 定义                             │
│       anos:* 编译时内置 (85)  |  host:* JSON 插件 (10)          │
│       mcp:*  外部扩展 (0-N)                                    │
├────────────────────────────────────────────────────────────────┤
│  L3  Session-Managed (browser.*, terminal.*, desktop.*)        │
│       有状态、跨 turn、session-scoped                           │
│       由 HostInterface Agent 管理生命周期                        │
├────────────────────────────────────────────────────────────────┤
│  L2  shell.exec (unstructured escape hatch, raw text)          │
│       非结构化逃生口，返回 stdout/stderr/exit_code              │
│       SAPVA 检测高频模式 → 提议升级为 host:* 插件               │
├────────────────────────────────────────────────────────────────┤
│  L1  OS Kernel (processes, filesystem, network)                │
│       底层操作系统接口                                           │
└────────────────────────────────────────────────────────────────┘
```

**Agent 优先级**: L5 → L4 → L3 → L2

Agent 应优先使用高层抽象：先查找 Skill（L5），再查找结构化工具（L4），再考虑有状态工具（L3），最后才回退到 `shell.exec`（L2）。

**三层工具注册**:

| 层 | 定义 | 注册时机 | 创建者 | 扩展方式 |
|---|---|---|---|---|
| `anos:*` | Rust 代码 | 编译时 | 开发者 | 改代码重编译 |
| `host:*` | JSON 定义文件 | 运行时按环境 | 开发者/用户/Agent | 添加 JSON 文件 |
| `mcp:*` | JSON-RPC 进程 | 运行时注册 | 外部 | `/mcp add` |

---

## 4. Protocol Bridging

### 4.1 Cross-Protocol Bridge

A single ATD definition maps to multiple protocol bindings. The bridge selects the best available binding at dispatch time:

```
Agent: "take a photo"
  → ATD: anos:camera.capture.photo
    ├── CLI:         mobile camera +photo --rear
    ├── MCP:         tools/call capture_photo (JSON-RPC)
    ├── AppFunction: CameraFunctions.takePhoto() (IPC)
    └── REST:        POST /api/v1/tools/camera/capture
```

Each bridge handles:
- **Parameter mapping**: ATD param names → protocol-specific names
- **Invocation**: Process exec (CLI), JSON-RPC (MCP), Binder IPC (AppFunction), HTTP (REST)
- **Result mapping**: Protocol result fields → ATD output fields
- **Error mapping**: Protocol errors → unified ATD error codes

### 4.2 Protocol Comparison Table

| 维度 | CLI | MCP | REST | AppFunction |
|------|-----|-----|------|-------------|
| **传输** | 进程 exec (stdio) | JSON-RPC 2.0 (stdio/SSE) | HTTP/HTTPS | IPC (Binder/HiLink) |
| **延迟** | 中 (进程启动) | 低 (持久连接) | 中-高 (网络) | 极低 (进程内/IPC) |
| **Schema 描述** | ATD JSON | MCP tool schema | OpenAPI 3.1 | IDL/AIDL |
| **流式** | 有限 (stdout pipe) | 支持 (SSE) | 支持 (SSE/WebSocket) | 部分 |
| **平台** | 跨平台 | 跨平台 | 跨平台 | 平台锁定 |
| **安全** | OS 进程隔离 | 进程隔离 | TLS + Auth | OS 权限 |
| **ANOS 实现** | host:* 插件 | mcp:* 桥接 | D8 (计划中) | 未实现 |

### 4.3 Binding Selection Strategy

When multiple bindings are available, selection follows this priority:

1. **Agent/user preference** (`prefer=cli`)
2. **Platform availability** (current platform supports this binding?)
3. **Health status** (prefer `healthy` over `degraded`)
4. **Latency** (prefer lower latency)
5. **Capability match** (prefer bindings that don't need extra capabilities)

### 4.4 MCP → ATD Conversion

When an MCP server is registered via `/mcp add`, ANOS performs real-time conversion of MCP tool schemas into ATD `ToolDefinition`:

```
/mcp add npx -y @modelcontextprotocol/server-filesystem /home/user
    │
    ├── 1. Spawn MCP server process (stdio JSON-RPC)
    ├── 2. MCP initialize handshake
    ├── 3. tools/list → enumerate available tools
    ├── 4. Per-tool conversion:
    │       McpToolInfo {
    │         name: "read_file",
    │         description: "Read file contents",
    │         inputSchema: { ... }
    │       }
    │           │
    │           ▼  mcp_tool_to_definition()
    │       ToolDefinition {
    │         id: "mcp:server-filesystem.read_file",
    │         domain: "mcp",
    │         safety: Write,  // default for MCP
    │         trust_level: L2,
    │         tags: ["mcp", "server-filesystem"],
    │         ...
    │       }
    └── 5. Register into ToolRegistry (mcp:* namespace)
```

**MCP Tool Default Properties**:

| Property | Value |
|----------|-------|
| Safety level | Write |
| Trust level | L2 (Authenticated) |
| Publisher | `mcp:<server-name>` |
| Timeout | 60s |
| Max concurrent | 5 |
| Rate limit | 30/min |

**MCP Tool ID Format**: `mcp:<server-name>.<tool-name>` — server name derived from the launch command (e.g., `npx -y @modelcontextprotocol/server-filesystem` → `server-filesystem`).

**Capability Requirement**: MCP tools require UCAN capability scoped to `anos:tool:mcp.write`.

**Reverse direction**: When ANOS exposes tools to MCP clients, `ToolDefinition` → MCP tool schema conversion uses the same mapping in reverse.

**Implementation**: `crates/anos-tool-dispatch/src/binding_mcp.rs` — stdio JSON-RPC 2.0, `initialize`/`tools/list`/`tools/call` methods, schema conversion, async concurrent request handling.

### 4.5 Error Code Mapping

Unified ATD error codes map from each protocol:

| ATD Code | CLI exit | MCP code | AppFunction exception | HTTP |
|----------|----------|----------|-----------------------|------|
| PERMISSION_DENIED | 2 | -32600 | SecurityException | 403 |
| VALIDATION_ERROR | 3 | -32602 | IllegalArgumentException | 400 |
| TOOL_NOT_FOUND | 4 | -32601 | FunctionNotFoundException | 404 |
| TIMEOUT | 5 | -32000 | TimeoutException | 504 |
| RATE_LIMITED | 1 | -32000 | TooManyRequestsException | 429 |
| INTERNAL_ERROR | 5 | -32603 | RuntimeException | 500 |
| RESOURCE_UNAVAILABLE | 1 | -32000 | IllegalStateException | 503 |

---

## 5. Three-Tier Discovery (Hot / Warm / Cold)

### 5.1 Capacity Model

| Tier | Count | Context cost | Discovery latency | Loading |
|------|-------|--------------|--------------------|---------|
| **Hot** | ≤20 tools | ~3,000 tokens (compact ATD) | 0 ms | Pre-loaded in system prompt |
| **Warm** | ≤200 tools | 0 tokens | ~50 ms | Local HNSW search + ATD load |
| **Cold** | Unlimited | 0 tokens | ~200 ms | Network fetch from registry |

### 5.2 Promotion / Demotion Rules

| Transition | Trigger | Action |
|------------|---------|--------|
| Cold → Warm | First successful call | Download full ATD → local index → generate deferred ATD |
| Warm → Hot | ≥ 5 calls in past 7 days | Generate compact ATD → inject into system prompt |
| Hot → Warm | 14 days unused | Remove from system prompt → keep local index |
| Warm → Cold | 90 days unused + not in any installed skill's deps | Remove from local index |

**Frequency score**: `0.7 × recent_7d_calls + 0.3 × total_30d_calls`
Top 20 → Hot; top 200 → Warm; rest → Cold.

### 5.3 Agent Tool Profile

Each agent maintains a personalised tool configuration tracking Hot/Warm sets, usage statistics, and promotion candidates:

```rust
struct AgentToolProfile {
    agent_id: AgentId,
    hot_tools: Vec<HotToolEntry>,      // max 20, each with compact ATD
    warm_tools: Vec<WarmToolEntry>,     // max 200, each with deferred ATD
    total_hot_tokens: u16,             // sum of compact ATD token costs
    promotion_candidates: Vec<(ToolId, f32)>,  // (id, frequency_score)
}
```

### 5.4 Context Injection Layout

```
System Prompt:
  [1] Agent identity + role              (~200 tokens)
  [2] Active skill instructions          (~1,500 tokens)
  [3] HOT tool definitions (compact ATD) (~3,000 tokens)  ★
  [4] WARM index hint                    (~100 tokens)    ★
      "187 additional tools available.
       Use tool_search(intent) to discover more."
  [5] Capability token summary           (~200 tokens)    ★

  Total tool-related context: ~3,300 tokens
```

---

## 6. Visibility Classification

### 6.1 Four-Tier Visibility Pyramid

```
            ┌──────────┐
            │  System  │  Kernel daemons only, never exposed to LLM
            │  (7)     │
         ┌──┴──────────┴──┐
         │   Dangerous    │  Requires explicit /allow authorization
         │   (shell.exec, │  Authorization = Visibility
         │    docker.run)  │
      ┌──┴────────────────┴──┐
      │      Write           │  Always visible, has side effects
      │  (fs.write, git.commit) │
   ┌──┴──────────────────────┴──┐
   │         Read                │  Always visible, no side effects
   │  (fs.read, web.fetch,      │  Parallel execution (up to 8)
   │   memory.recall)           │
   └─────────────────────────────┘
```

### 6.2 Authorization = Visibility

| Visibility | LLM sees it? | Authorization needed? |
|------------|-------------|----------------------|
| **Read** | Always | No |
| **Write** | Always | No |
| **Dangerous** | Only after grant | `/allow` or `--dangerously-skip-permissions` |
| **System** | Never | Kernel daemons only |

**Key design**: Granting a Dangerous tool makes it appear in the LLM's tool schema on the next turn. Revoking re-hides it. Constitutional guards (secret scan, forbidden capabilities) are **never** bypassed — even `--dangerously-skip-permissions` does not skip them.

### 6.3 Authorization Commands

| Command | Effect | Persistent? |
|---------|--------|------------|
| `/allow shell.exec` | Allow for this session | No (lost on daemon restart) |
| `/allow-always shell.exec` | Allow permanently | Yes (`~/.anos/extension_permissions.json`) |
| `/deny shell.exec` | Revoke permission | Yes |
| `--dangerously-skip-permissions` | Bypass all checks | CLI flag only |

### 6.4 Three-Level Gate (Dispatch)

```
Tool call request
  │
  ├─ Check 1: --dangerously-skip-permissions flag?
  │   └─ Yes → All Dangerous tools visible to LLM + allowed
  │
  ├─ Check 2: PermissionStore has grant for this tool?
  │   └─ Yes → This tool visible to LLM + allowed
  │
  └─ Check 3: Neither
      └─ Tool hidden from LLM + blocked at dispatch
```

---

## 7. Health Management & Circuit Breaker

### 7.1 Health Metrics (5-min rolling window)

| Metric | Healthy | Degraded | Unhealthy |
|--------|---------|----------|-----------|
| Success rate | ≥ 95% | ≥ 80% | < 80% |
| p50 latency | ≤ 2s | ≤ 5s | > 5s |
| p99 latency | ≤ 10s | ≤ 30s | > 30s |

### 7.2 Circuit Breaker (3-State Machine)

```
                 error_rate > 50% for 5 min
    ┌──────────┐ ──────────────────────────→ ┌──────────┐
    │  Closed  │                              │   Open   │
    │ (normal) │ ←────────────────────────── │ (reject) │
    └──────────┘   probes succeed             └──────────┘
         ↑                                        │
         │                                   cooldown expires
         │          ┌───────────┐                  │
         └───────── │ Half-Open │ ←────────────────┘
        probes OK   │  (probe)  │
                    └───────────┘
                         │
                    probes fail → back to Open
```

| State | Behaviour | Transition |
|-------|-----------|------------|
| Closed | All requests pass | → Open when error_rate > 50% for 5 min |
| Open | All requests rejected (fallback tool used) | → Half-Open after cooldown |
| Half-Open | Probe requests allowed | → Closed if probes succeed; → Open if fail |

### 7.3 Tool Lifecycle State Machine

```
              publish        activate
 registered ──────→ active ←────────── degraded
                     │                    ↑
                disable                recover
                     │                    │
                     ↓                    │
                  disabled ───────────────┘
                     │
               deprecate
                     │
                     ↓
                deprecated
                     │
                  remove
                     ↓
                  removed
```

| State | Discoverable | Callable | Trigger |
|-------|--------------|----------|---------|
| registered | No | No | ATD submitted + validated |
| active | Yes | Yes | Published/activated |
| degraded | Yes (flagged) | Yes (may limit params) | error_rate > 20% or p99 > threshold |
| disabled | No | No | error_rate > 50% for 5 min (auto circuit-break) |
| deprecated | Yes (warning) | Yes (deprecation notice injected) | Publisher manual action |
| removed | No | No | 90 days after deprecation or manual removal |

---

## 8. Schema Derivation Pipeline (P4)

> ATD Schema 是唯一输入，所有下游产物是派生输出。
> 下游与 ATD 不一致 = bug，不是 feature。

P4 states: **一切从 Schema 派生。Schema 是唯一的真相源。**

Architecture implication: `ToolDefinition` (ATD v1.0) is the single source of truth for every tool in ANOS. Nine downstream artifacts derive from it. None of them may be independently maintained.

**Invariant**: If any downstream artifact diverges from the ATD Schema, that is a bug. There are no exceptions — not for "convenience", not for "performance", not for "we'll sync it later".

### 8.1 Pipeline Diagram

```
                         ToolDefinition (ATD v1.0)
                                  │
                  ┌───────────────┼───────────────┐
                  │               │               │
            Build-time        Runtime          On-demand
                  │               │               │
          ┌───┬───┴───┐   ┌──┬───┼───┬──┐    ┌───┴───┐
          │   │       │   │  │   │   │  │    │       │
         D1  D8      D9  D2 D3  D5  D7 │   D4      D6
         Docs REST   Type LLM Val Sub MCP  CLI    Skill
         gen  spec   bind desc      Agent       schema  assoc
```

### 8.2 Nine Downstream Artifacts (D1–D9)

| # | Artifact | Derivation | Status | Priority |
|---|----------|-----------|--------|----------|
| D1 | Tool documentation (`docs/tools/*.md`) | Build-time | ❌ Manual → auto-generate | P0 |
| D2 | LLM tool descriptions (system prompt) | Runtime | ✅ Implemented | — |
| D3 | JSON Schema validation (dispatch entry) | Runtime | ✅ Implemented | — |
| D4 | `anos schema` CLI introspection | On-demand | ✅ Implemented | — |
| D5 | Sub-agent capability-scoped tool visibility | Runtime | ⚠️ Uses parent perms → fix to UCAN | P0 |
| D6 | Skill metadata association | On-demand | ⚠️ Independent → associate at load | P1 |
| D7 | MCP protocol mapping | Runtime | ✅ Implemented | — |
| D8 | OpenAPI/REST spec generation | Build-time | ❌ Not implemented | P1 |
| D9 | Multi-language type bindings (TS/Python) | Build-time | ❌ Not implemented | P2 |

### 8.3 Three Derivation Classes

| Class | When | Consistency guarantee | Artifacts |
|-------|------|----------------------|-----------|
| **Build-time** | `cargo build` or CI | CI check: generated output diff = 0 against checked-in files | D1, D8, D9 |
| **Runtime** | Daemon start / tool registration | Single `ToolDefinition` object — drift impossible | D2, D3, D5, D7 |
| **On-demand** | User request / skill matching | Live read from `ToolRegistry` | D4, D6 |

### 8.4 Build-Time Derivation

#### D1 — Tool Documentation Generation

**Problem**: `docs/tools/*.md` (11 files) is manually maintained. Schema changes in `builtin_definitions()` don't propagate to docs.

**Design**:

```
builtin_definitions()  →  60 ToolDefinition
        │
   gen-tool-docs (Rust binary)
        │
        ├── docs/tools/web.md       (grouped by domain)
        ├── docs/tools/fs.md
        ├── docs/tools/shell.md
        └── ...
```

**Generator**: `crates/anos-tool-dispatch/src/bin/gen-tool-docs.rs`

**Per-tool output format**:

```markdown
## anos:web.fetch

Fetch a URL via HTTP GET or POST.

| Field | Value |
|-------|-------|
| Visibility | Write |
| Safety | write |
| Timeout | 30s |
| Rate limit | 60/min |
| Max concurrent | 10 |

### Input

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| url | string | yes | URL to fetch |
| method | string | no | GET (default) or POST |
| ... | ... | ... | ... |

### Output

| Field | Type | Description |
|-------|------|-------------|
| status | integer | HTTP status code |
| body | string | Response body |
| ... | ... | ... |
```

**Generated file header**:

```markdown
<!-- AUTO-GENERATED from ATD Schema. Do not edit manually. -->
<!-- Regenerate: cargo run --bin gen-tool-docs -->
```

#### D8 — OpenAPI Spec Generation

**Problem**: No machine-readable REST API description exists for ANOS tools.

**Design**:

```
builtin_definitions()  →  60 ToolDefinition
        │
   gen-openapi (Rust binary)
        │
        └── docs/api/openapi.yaml
```

**Generator**: `crates/anos-tool-dispatch/src/bin/gen-openapi.rs`

**Mapping rules**:

| ATD field | OpenAPI field |
|-----------|--------------|
| `id` (e.g., `anos:web.fetch`) | path: `/api/v1/tools/web/fetch` |
| `description` | operation summary |
| `input_schema` | requestBody (application/json) |
| `output_schema` | 200 response schema |
| `safety.level` | `x-anos-safety` extension |
| `visibility` | `x-anos-visibility` extension |
| `resources.rate_limit` | `x-ratelimit-*` headers |

**Error responses**: Unified from ATD error codes:

```yaml
responses:
  '400': { $ref: '#/components/responses/ValidationError' }
  '403': { $ref: '#/components/responses/PermissionDenied' }
  '429': { $ref: '#/components/responses/RateLimited' }
  '504': { $ref: '#/components/responses/Timeout' }
```

**Scope**: Only tools with REST binding generate paths. Tools without REST binding are omitted.

#### D9 — Multi-Language Type Bindings

**Problem**: Third-party developers using ANOS tools via SDK have no type safety.

**Design**:

```
builtin_definitions()  →  60 ToolDefinition
        │
   gen-type-bindings (Rust binary)
        │
        ├── sdk/typescript/src/tools.generated.ts
        ├── sdk/python/anos/tools_generated.py
        └── (sdk/rust/src/tool_params.rs — optional)
```

**Generator**: `crates/anos-tool-dispatch/src/bin/gen-type-bindings.rs`

**Type mapping** (JSON Schema → language types):

| JSON Schema | TypeScript | Python |
|-------------|-----------|--------|
| `string` | `string` | `str` |
| `number` | `number` | `float` |
| `integer` | `number` | `int` |
| `boolean` | `boolean` | `bool` |
| `object` | `interface` | `@dataclass` |
| `array<T>` | `T[]` | `list[T]` |
| `string` + `enum` | union literal | `Literal[...]` |
| `oneOf` | discriminated union | `Union[...]` |

**Per-tool output** (TypeScript example):

```typescript
/** anos:web.fetch — Fetch a URL via HTTP GET or POST */
export interface WebFetchParams {
  /** URL to fetch */
  url: string;
  /** HTTP method (default: GET) */
  method?: 'GET' | 'POST';
  /** Request headers */
  headers?: Record<string, string>;
  /** Request body (for POST) */
  body?: string;
}

export interface WebFetchResult {
  status: number;
  body: string;
  headers: Record<string, string>;
  content_type: string;
}
```

**Priority**: TypeScript first (largest ecosystem), Python second.

#### CI Consistency Check

All build-time generators share a CI pattern:

```yaml
# .github/workflows/schema-consistency.yml
- name: Verify schema-derived artifacts
  run: |
    cargo run --bin gen-tool-docs
    cargo run --bin gen-openapi
    cargo run --bin gen-type-bindings
    git diff --exit-code docs/tools/ docs/api/ sdk/
```

If any generated file differs from what's checked in, CI fails. This makes P4 violations impossible to merge.

### 8.5 Runtime Derivation

#### D2 — LLM Tool Descriptions (Implemented)

**Derivation path**: `ToolDefinition` → `tool_to_schema()` → LLM system prompt

**Code**: `crates/anos-runtime/src/engine.rs`

```rust
fn tool_to_schema(tool: &ToolDefinition) -> ToolSchema {
    ToolSchema {
        id: tool.id.clone(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        input_schema: tool.input_schema.clone(),
    }
}
```

**Guarantee**: Single `ToolDefinition` object — no copy, no drift.

#### D3 — JSON Schema Validation (Implemented)

**Derivation path**: `ToolDefinition.input_schema` → `jsonschema::validator_for()` → validate before dispatch

**Code**: `crates/anos-tool-dispatch/src/registry.rs`

```rust
fn validate_tool_args(tool_id: &str, args: &Value, input_schema: &Value) -> Result<(), String> {
    let validator = jsonschema::validator_for(input_schema)?;
    // ... validate and return field-level errors
}
```

**Flow**: Called at dispatch entry before any tool handler executes. Schema is read from the same `ToolDefinition` used for registration.

#### D5 — Sub-Agent Capability-Scoped Tool Visibility (To Fix)

**Problem**: `engine.rs` filters tools by parent agent's permissions, not by sub-agent's own UCAN capability token. Sub-agents cannot self-describe their authorized tool set.

**Current flow**:

```
agent.spawn(type: "research", allowed_tools: ["fs.read", "web.fetch"])
    │
    └── build_tool_schemas() filters by parent's permission grants
        └── Sub-agent sees tools based on parent's session state ← WRONG
```

**Target flow**:

```
agent.spawn(type: "research", allowed_tools: ["fs.read", "web.fetch"])
    │
    ├── 1. Attenuate parent UCAN → child UCAN
    │      child_ucan.tools = allowed_tools ∩ parent_ucan.tools  (P3)
    │
    ├── 2. build_tool_schemas(child_ucan) filters by child's own token
    │      for tool in registry.list_tools():
    │          if child_ucan.authorizes(tool.id) → include in schema
    │
    └── 3. Sub-agent system prompt declares its capability boundary
           "你被授权使用以下 N 个工具: [list derived from child_ucan]"
```

**Key changes**:
- `AllowedTools` list → UCAN attenuated token (aligns with P3)
- `build_tool_schemas()` accepts a capability token parameter, not a parent reference
- Sub-agent can introspect its own tools via `anos schema` equivalent (constitutional A1: right to know)

#### D7 — MCP Protocol Mapping (Implemented)

**Derivation path**: `McpToolInfo` → `mcp_tool_to_definition()` → unified `ToolDefinition`

External MCP tools are converted INTO the ATD format on registration. The MCP binding is just one protocol — internally everything is a `ToolDefinition`.

**Reverse direction**: When ANOS exposes tools to MCP clients, `ToolDefinition` → MCP tool schema conversion uses the same mapping in reverse.

### 8.6 On-Demand Derivation

#### D4 — `anos schema` CLI Introspection (Implemented)

**Derivation path**: IPC → `ToolRegistry.list_tools()` / `ToolRegistry.get_tool()` → formatted output

**Code**: `crates/anos-cli/src/schema.rs`

```bash
anos schema                          # List all tools by category
anos schema anos:web.fetch           # Full tool definition with schemas
anos schema --json                   # Machine-readable output
anos schema --category web           # Filter by domain
anos schema --all                    # Include Dangerous/System tools
```

**Guarantee**: Reads live from `ToolRegistry` at request time — always reflects current state.

#### D6 — Skill Metadata Association (To Fix, Approach B)

**Problem**: `SkillManifest` maintains independent `description`, `category`, `tags`. Its `required_tools` references tool IDs but doesn't derive schema information from them.

**Design (Approach B)**: Skill keeps its own workflow-level description (a skill is a composition, not an atomic tool). But when a skill is loaded, its `required_tools` are resolved against the ATD Registry — tool schemas are injected, not duplicated.

**Current flow**:

```
SkillManifest {
    description: "Code review workflow",       // independent
    required_tools: ["fs.read", "shell.exec"], // ID-only reference
    input_schema: { ... },                     // independently defined
}
```

**Target flow**:

```
SkillManifest {
    description: "Code review workflow",       // skill-level (kept independent)
    required_tools: ["fs.read", "shell.exec"], // ID-only reference
    // input_schema: REMOVED — derived at load time
}

// At skill load time:
fn resolve_skill(manifest: &SkillManifest, registry: &ToolRegistry) -> ResolvedSkill {
    let tool_schemas: Vec<ToolDefinition> = manifest.required_tools
        .iter()
        .filter_map(|id| registry.get(id))
        .collect();

    ResolvedSkill {
        manifest,
        tool_schemas,  // injected from ATD, not duplicated
        // Skill's effective input = union of required tools' inputs
    }
}
```

**Key changes**:
- `SkillManifest` no longer contains `input_schema` for referenced tools
- Tool schemas resolved at load time from `ToolRegistry`
- If a referenced tool doesn't exist in registry → skill load warns (degraded, not fatal)
- Skill's own `description` and `category` remain independent (workflow-level semantics)

### 8.7 Implementation Priority

| Phase | Artifact | Effort | Rationale |
|-------|----------|--------|-----------|
| **P0** | D1 Tool docs generation | S | Most direct P4 violation, lowest cost to fix |
| **P0** | D5 Sub-agent UCAN visibility | M | Security-relevant (P3 cross-concern), blocks correct sub-agent behavior |
| **P1** | D6 Skill-schema association | S | Affects skill loading correctness |
| **P1** | D8 OpenAPI spec generation | S | Enables open ecosystem, low effort (similar to D1) |
| **P2** | D9 Type bindings generation | M | Developer experience, not blocking |

Effort: S = days, M = week

---

## 9. ATD Plugin System (host:*)

> ATD 的可扩展插件形态 — 运行时可发现、可加载、可扩展的工具定义。

### 9.1 Plugin Definition Format

存储路径: `~/.anos/host-tools/<id>.json`

```json
{
  "id": "host:media.convert",
  "name": "Media Convert",
  "description": "Convert media files between formats (audio/video/image). Powered by ffmpeg.",
  "binary": "ffmpeg",
  "visibility": "dangerous",
  "category": "media",
  "input_schema": {
    "type": "object",
    "properties": {
      "input": { "type": "string", "description": "Input file path (absolute)" },
      "output": { "type": "string", "description": "Output file path (absolute)" },
      "extract_audio": { "type": "boolean", "default": false }
    },
    "required": ["input", "output"]
  },
  "command_template": "ffmpeg -y -i {{input}} {{#if extract_audio}}-vn {{/if}}{{output}}",
  "param_transforms": {
    "quality_flag": {
      "source": "quality",
      "map": { "low": "-crf 28", "high": "-crf 18" }
    }
  },
  "timeout_secs": 300,
  "created_by": "bundled",
  "version": "1.0.0"
}
```

**命令模板语法** (简化 Handlebars):
- `{{param}}` — 插入参数值
- `{{#if param}}...{{/if}}` — 条件块
- `{{transform_key}}` — 插入 param_transforms 映射值

### 9.2 Conditional Registration (Startup)

```
daemon boot
  ├─ detect_environment() → EnvSnapshot (宿主 binary 扫描)
  ├─ scan ~/.anos/host-tools/*.json → 加载插件定义
  ├─ 对每个插件:
  │   ├─ 检查 binary 在 EnvSnapshot 中是否存在
  │   ├─ 存在 → 注册到 ToolRegistry (host:* namespace)
  │   └─ 不存在 → 跳过 (tracing::info)
  └─ Agent 通过 ATD Schema 发现已注册的 host:* 工具
```

### 9.3 Runtime Invocation

```
Agent 调用 host:media.convert(input="/tmp/a.mp4", output="/tmp/a.mp3")
  ├─ ToolDispatch 查找 host:media.convert → HostToolPlugin
  ├─ 验证 input 参数 against input_schema
  ├─ 应用 param_transforms
  ├─ 渲染 command_template → "ffmpeg -y -i /tmp/a.mp4 /tmp/a.mp3"
  ├─ 通过 shell.exec 执行（继承沙箱/权限）
  └─ 返回 { stdout, stderr, exit_code }
```

### 9.4 Three Extension Paths

| 路径 | 触发 | 审批 |
|------|------|------|
| **内置** | `anos --init` 安装 bundled 定义 | 无需 |
| **用户** | `/host-tool add <binary>` 或 agent 生成 | 用户确认 |
| **自演进** | SAPVA 检测 shell.exec 高频模式 → 提议插件 | L3 人类审批 |

### 9.5 Visibility Rules (P3)

所有 `host:*` 插件遵循 P3 (Capability-as-Security):

| Visibility | 条件 | 例子 |
|------------|------|------|
| `dangerous` | 执行命令、网络、文件修改 | ffmpeg, docker, ssh, ollama |
| `write` | 低风险通知或打开 | xdg-open, notify-send |

Agent 需要 `/allow host:media.convert` 后才能看到该工具（授权 = 可见性）。

### 9.6 Bundled Plugins (10)

| Phase | Plugin ID | Binary | 说明 | Visibility |
|-------|-----------|--------|------|------------|
| 4a | `host:media.convert` | ffmpeg | 音视频格式转换 | Dangerous |
| 4a | `host:media.download` | yt-dlp | 视频下载 | Dangerous |
| 4a | `host:doc.convert` | pandoc | 文档格式转换 | Dangerous |
| 4a | `host:data.json_query` | jq | JSON 查询 | Dangerous |
| 4a | `host:ai.chat_local` | ollama | 本地 LLM | Dangerous |
| 4b | `host:media.image_convert` | convert | 图片转换 | Dangerous |
| 4b | `host:data.sqlite_query` | sqlite3 | SQLite 查询 | Dangerous |
| 4b | `host:doc.pdf_export` | pdflatex | PDF 导出 | Dangerous |
| 4b | `host:desktop.open` | xdg-open | 打开文件/URL | Write |
| 4b | `host:desktop.notify` | notify-send | 桌面通知 | Write |

### 9.7 Custom Plugin Guide

To add a custom host plugin:

1. Create a JSON file at `~/.anos/host-tools/<plugin-id>.json`
2. Follow the format in §9.1 above
3. Ensure the `binary` is installed on the system
4. Restart daemon or run `/host-tool reload`
5. The plugin auto-registers if the binary is found

Example — adding a custom `host:text.translate` plugin:

```json
{
  "id": "host:text.translate",
  "name": "Text Translate",
  "description": "Translate text between languages using translate-shell.",
  "binary": "trans",
  "visibility": "write",
  "category": "text",
  "input_schema": {
    "type": "object",
    "properties": {
      "text": { "type": "string", "description": "Text to translate" },
      "target_lang": { "type": "string", "description": "Target language code (e.g., en, zh, ja)" },
      "source_lang": { "type": "string", "description": "Source language code (auto-detect if omitted)" }
    },
    "required": ["text", "target_lang"]
  },
  "command_template": "trans {{#if source_lang}}{{source_lang}}:{{/if}}{{target_lang}} '{{text}}'",
  "timeout_secs": 30,
  "created_by": "user",
  "version": "1.0.0"
}
```

### 9.8 SAPVA Auto-Propose Integration (Phase 4c)

```
SENSE:  扫描 session logs, 检测 shell.exec 高频模式
        shell.exec("jq '.data[]' file.json") × 15 次

ANALYZE: 识别可模板化的参数模式
         固定部分: jq
         变化部分: filter, input_file

PROPOSE: 生成 ATD Plugin JSON 定义
         host:data.jq_query { filter, input }

VALIDATE: 用历史参数在沙箱测试

APPLY:  需 L3 人类审批 → 保存到 ~/.anos/host-tools/
```

This closes the L2 → L4 promotion loop: high-frequency `shell.exec` patterns are automatically proposed as structured `host:*` plugins, elevating unstructured commands to typed, schema-validated tools.

### 9.9 Skill Dependency Declaration

```yaml
# SKILL.md
requires_tools:
  - host:media.convert
  - host:media.download
```

Skill 激活时检查: 依赖工具已注册 → 正常激活; 缺失 → 提示用户安装对应 binary。

### 9.10 Key Files

| 文件 | 职责 |
|------|------|
| `crates/anos-runtime/src/host_tools.rs` | HostToolPlugin struct, load/save, 模板渲染, 条件注册 |
| `crates/anos-runtime/src/host_tools_bundled.rs` | 10 个 bundled 插件定义 |
| `~/.anos/host-tools/*.json` | 插件定义文件 (真相源) |
| `crates/anos-tool-dispatch/src/builtins.rs` | host:* 分发 → 模板渲染 → shell.exec |
| `crates/anos-cli/src/daemon.rs` | 启动时加载 + 注册 |
| `crates/anos-cli/src/commands.rs` | /host-tool slash 命令 |

---

## 10. MCP Integration

### 10.1 MCP → ATD Conversion

ANOS supports the **Model Context Protocol (MCP)** for integrating external tool servers. MCP tools appear alongside built-in tools and follow the same ATD dispatch pipeline, including capability checks and rate limiting.

**Registration flow**:

```
/mcp add npx -y @modelcontextprotocol/server-filesystem /home/user
    │
    ├─ 1. Connect: Spawn server process, MCP initialize handshake (stdio)
    ├─ 2. Discover: tools/list → enumerate available tools
    ├─ 3. Register: Each MCP tool → mcp_tool_to_definition() → ToolDefinition
    │       Tool ID: mcp:<server-name>.<tool-name>
    │       Domain: mcp
    │       Safety: Write (default)
    │       Tags: ["mcp", "<server-name>"]
    └─ 4. Dispatch: tools/call → ToolResult (standard envelope)
```

### 10.2 MCP Tool Properties

| Property | Value |
|----------|-------|
| Safety level | Write |
| Trust level | L2 (Authenticated) |
| Publisher | `mcp:<server-name>` |
| Timeout | 60s |
| Max concurrent | 5 |
| Rate limit | 30/min |

### 10.3 Capability Requirements

MCP tools require a UCAN capability token scoped to:

```
anos:tool:mcp.write
```

Since MCP tools have external side effects, they are classified at the Write safety level by default.

### 10.4 Error Handling

If an MCP tool call fails:

```json
{
  "code": "MCP_TOOL_ERROR",
  "message": "Description of the error",
  "retryable": false
}
```

If the MCP server process crashes or disconnects, subsequent tool calls return an error until the server is reconnected.

### 10.5 Implementation

The MCP client implementation lives in `crates/anos-tool-dispatch/src/binding_mcp.rs`. It handles:

- Stdio-based JSON-RPC 2.0 communication
- `initialize` / `tools/list` / `tools/call` protocol methods
- Conversion of MCP tool schemas to ATD `ToolDefinition` structs
- Concurrent request handling with async channels

For the full MCP user guide, see `docs/guides/mcp-integration.md`.

---

## 11. Implementation Status & Gaps

### 11.1 Implemented

- ✅ **统一 Schema** — id, input/output schema, safety, resources, trust, visibility
- ✅ **Hot/Warm/Cold 三层发现** — SQLite 持久化，自动升降级
- ✅ **host:\* 插件系统** — JSON 模板 → shell.exec，10 个 bundled 插件
- ✅ **MCP 桥接** — MCP tool → ATD ToolDefinition 实时转换
- ✅ **电路断路器** — 3-state: Closed/Open/Half-Open
- ✅ **可见性分级** — Read/Write/Dangerous/System，授权 = 可见性

### 11.2 Gap Analysis

#### Gap 1: 语义发现未接通 (intent_examples → embedding)

ATD 有 `capability.intent_examples` 字段，设计上支持 embedding 相似度发现，但运行时只用关键词搜索。

**影响**: Agent 说"我要处理图片" → 无法自动匹配到 `host:media.image_convert`。工具发现退化为 LLM 从 schema 列表硬选。

**修复**: 为 `intent_examples` 生成 embedding，接入 `anos-embedding` 的 HNSW 索引。约 200 行改动。

#### Gap 2: 工具组合（Pipe composition）未实现

ATD 设计了 typed pipe composition（`tool_a | tool_b | tool_c`），但完全没有实现。

**影响**:
- 每个工具调用都是独立的 LLM → tool → LLM 往返（每次 ~500ms + token 消耗）
- 不能做 `fs.read | json.parse | data.filter` 这样的零 LLM 开销管道
- 与 Unix pipe 哲学脱节

**评估**: 这是 ATD 最大的未兑现承诺。但优先级中等 — LLM 循环虽然低效，但功能上可以覆盖管道能做的事。管道的价值在于**性能优化**和**token 节省**，不是功能性缺失。

#### Gap 3: Dry-run 从未执行

Agent-Native CLI Principle 4 强调 dry-run 是安全网。ATD 有 `supports_dry_run` 字段。但运行时没有 dry-run 模式。

**影响**: 对 Dangerous 工具（shell.exec, fs.delete, docker.run）是安全缺失。Agent 无法预览操作效果再决定是否执行。

**修复**: dispatch 层检查 `dry_run: true` 参数，tool handler 返回"将要执行什么"而非实际执行。需要每个 Dangerous tool 实现 dry-run 路径。

#### Gap 4: 错误分类不统一 (generic strings, no ErrorClass)

ATD 设计了 `ErrorDef` 允许工具声明自己的错误类型，但实际所有工具返回 generic string error。

**影响**: LLM 无法区分：
- 暂时错误（重试有意义）: rate limit, timeout, 503
- 永久错误（修改参数）: 404, 400, permission denied
- 环境错误（换工具）: binary not found, feature not available

**修复**: 在 ToolResult 中增加 `error_code: Option<ErrorClass>` 枚举（Transient/Permanent/Environmental），dispatch 层统一映射。

#### Gap 5: 有状态工具无 Schema 表达 (browser/terminal have state, ATD can't express)

ATD 假设工具是无状态的（JSON in → JSON out）。但 browser.*/terminal.* 是有状态的（Session-Managed）。

**影响**: ATD Schema 无法表达"这个工具有状态"、"同一 session 的调用应路由到同一实例"、"有 idle timeout"。

**修复建议**: ATD v1.1 增加 `state` 字段:

```yaml
state:
  model: stateless | session_scoped | global
  session_affinity: true
  idle_timeout_secs: 1800
```

#### Gap 6: 跨 Agent 工具视图

ATD 工具作用域是全局的（所有 Agent 共享同一个 ToolRegistry）。缺少：
- Per-Agent 工具视图（Agent A 看到的工具集和 Agent B 不同）
- 工具版本隔离
- 动态工具注入（运行中为特定 Agent 添加工具）

当前用 `AllowedTools` 白名单模拟了 per-Agent 过滤，但这是视图过滤，不是真正的隔离。

### 11.3 ATD 通用性评估

| 维度 | 通用性 | 评价 |
|------|--------|------|
| **协议覆盖** | 高 | CLI/MCP/REST/AppFunction 覆盖了主流协议 |
| **安全分级** | 高 | Read/Write/Dangerous/System 四级覆盖了常见需求 |
| **发现机制** | 中 | Hot/Warm/Cold 静态分层好用，语义发现未通 |
| **组合能力** | 低 | Pipe composition 未实现，每次调用都需 LLM |
| **状态表达** | 低 | 无法表达有状态工具 |
| **错误语义** | 低 | 无统一错误分类 |
| **跨平台** | 低 | PlatformConfig 字段存在但未实现 |

**总评**: ATD 作为统一工具 Schema 的核心设计是正确的且已验证。但"工具定义标准"和"工具运行时"之间存在 gap — ATD 定义了工具"是什么"，但对工具"怎么被组合"、"怎么表达状态"、"怎么分类错误"覆盖不足。

---

## 12. Related Documents

| Document | Relationship |
|----------|-------------|
| [runtime-tool-dispatch.md](./runtime-tool-dispatch.md) | Dispatch pipeline internals (8-step pipeline, binding selection, pipe composition design) |
| schema-derivation-pipeline.md (merged into §8 above) | P4 derivation architecture |
| atd-plugin-system.md (merged into §9 above) | host:* plugin architecture |
| [docs/guides/mcp-integration.md](../guides/mcp-integration.md) | MCP user guide (setup, commands, troubleshooting) |
| [docs/design/anos-tool-standard.md](../design/anos-tool-standard.md) | Original ATD design (full Chinese specification) |
| [host-interface-agent.md](./host-interface-agent.md) | Layer 3 stateful tools (browser.*, terminal.*) |
| [docs/design/anos-design-philosophy.md](../design/anos-design-philosophy.md) | P2 Intent-Driven, P4 Schema-as-Truth principles |
| [application-skills-system.md](./application-skills-system.md) | Skill discovery, loading, and DAG orchestration |
| [docs/reports/2026-03-28-anos-systematic-review.md](../reports/2026-03-28-anos-systematic-review.md) | §七 ATD completeness assessment |
