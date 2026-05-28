# ATD 技术综述（中文深度版）

> **Agent Tool Dispatch** —— 让任意 LLM agent、任意框架，调任意工具，跑在任意平台上。
>
> 本文档是 ATD 的一份**面向工程师与决策者的综合介绍**，覆盖：
>
> 1. [定位](#1-定位postioning) — ATD 是什么 / 不是什么 / 为什么不直接用 raw MCP / raw CLI
> 2. [设计哲学](#2-设计哲学design-philosophy) — 7 条原则 + 反模式 + adopter checklist
> 3. [架构](#3-架构architecture) — 5 层结构 / dispatch pipeline / 17 crate 拓扑 / 安全三轴
> 4. [高价值应用场景](#4-高价值应用场景) — 医疗 / agentic CLI / 跨厂商 / embodied agent / federation
> 5. [深度案例：celia_phr](#5-深度案例celia_phr--最复杂的-atd-adopter) — 用真实落地的最复杂 adopter 完整说明 ATD 的价值
>
> 关联文档（三份"宪法"）：
> - [`docs/atd-positioning.md`](../atd-positioning.md) — 范围 / 身份
> - [`docs/atd-design-philosophy.md`](../atd-design-philosophy.md) — 设计冲突 / 取舍
> - [`docs/atd-architecture.md`](../atd-architecture.md) — 代码导航 / 结构变更
>
> 本文档是上述三份文档面向人类（工程师 + 决策者 + PPT 听众）的综合再表述，附带 celia_phr 端到端案例分析；不替代上述任一文档作为权威源。

---

## 0. 一页执行摘要

| 维度 | 不用 ATD | 用 ATD |
|---|---|---|
| **工具被多个 agent 平台调用** | 每个平台一套适配代码 | 写一份 server，所有平台用 |
| **多用户 / 多租户** | N 进程 × N 配置 × N OAuth 状态 | 一进程 + caller_id 路由 + 一份 token broker |
| **审计 / 可观测性** | shell 历史 + grep stdout | 结构化 JSON Lines（call_id / caller / duration / capability / outcome / secrets_resolved） |
| **能力门禁** | 各工具自检（漂移） | dispatch 层一致 gate，工具不被调用即拒 |
| **限流 / 超时** | 各工具自实现 | tier-aware deadline + per-tool semaphore |
| **跨厂商组合**（healthkit + weather + …） | 自写 multiplexer | 桥接多 socket，agent 看到合并 catalog |
| **LLM 一次成功率** | 看 `--help` 文本，撞了才知道（v1.1.0: 24%） | `description + intent_examples` 结构化（v1.2.0: 95.2%） |

实证：healthkit_cli case study v1.4.0 一个 prompt 下 — **ATD 路径 2 次调用 ~1.6s 零错试**，CLI fallback 路径 8 次调用 ~6s 含 3 次走错。两条路径同一 agent / 同一 LLM / 同一 prompt 头对头跑。

---

## 1. 定位（Positioning）

### 1.1 一句话

> **ATD 是 agent 调用工具时的一层中立调度协议**。Vendor 把工具 host 成一个 ATD server（Unix socket 或 HTTP），任意 agent 平台（Hermes / Claude Code / Cursor / 自研）通过同样的 wire 格式 discover / describe / call / dry-run。中间层提供 capability gate / audit log / 多租户 token 路由 / tool 可见性控制 / skill 同步 / cursor 分页 —— 这些都是 raw CLI 拉不出来、raw MCP 没规范、per-vendor 自研每个都要重写的东西。

### 1.2 "四个任意"

ATD 把工具世界的四种分裂折叠到一个统一面：

| 维度 | 现状的分裂 | ATD 的答案 |
|---|---|---|
| 任意工具 | CLI / REST / MCP / native SDK 各自一套 shape | 一份 `ToolDefinition` 映射多个 binding |
| 任意平台 | Linux / macOS / Windows / iOS / Android / HarmonyOS 各自一组调用面 | binding 选择在 server 侧 dispatch 时决定 |
| 任意 agent | Claude Code 吃不下 OpenAI function-calling 的 shape | 所有 agent 调同一份 SDK；adapter 渲染 per-provider dict |
| 任意 framework | LangChain tool ≠ MCP tool ≠ Apple App Intent | 一份定义，多种 framework consumer |

### 1.3 经验证据 —— v1.4.0 case study 实测

Hermes + DeepSeek 在同一个 prompt "从医生角度分析最近两个月心率" 下：

| 维度 | ATD 路径 | CLI fallback 路径 |
|---|---|---|
| 调用次数 | **2** | 8 |
| 总耗时 | **~1.6s** | ~6s |
| 走错路径次数 | **0** | 3（错 wrapper、`--offset` 不存在 × 2） |
| 首次拿到数据 | call #1（1.2s） | call #6（5s） |
| Audit 可观测性 | **2 entries 完整** | shell log only |
| Agent 需自己知道 wrapper 命令 | 否 | 是（`healthkit healthkit +x` 双关键字） |
| Agent 需自己知道 HMS 30 天上限 | 否 | 是（撞错才知道） |

ATD 路径**严格优于** CLI fallback，**两点是协议层差异，不是工具能力差异**：

1. `description + intent_examples` 给了 LLM 结构化的"我能做什么"
2. audit log 给了人类结构化的"实际发生了什么"

健康记录三轮 case study 进展：

| 版本 | 工具 surface | LLM 表现 |
|---|---|---|
| v1.1.0 | 8 个 raw HMS REST endpoint（permissive `{type:object}` schema） | **24% 成功率**，79 次调用，66% Invalid param |
| v1.2.0 | 26 个 helper-tool（auto-derived 自 CLI helpers + SKILL.md） | **95.2% 成功率**，21 次调用（-73%） |
| v1.4.0 | 27 工具 + 多租户 mode | **2 ATD 调用 vs 8 CLI fallback**，0 错试 |

### 1.4 用 ATD / 不用 ATD 的边界

**用 ATD 当：**

- 你的工具 surface 要被 ≥1 个 LLM agent 平台用
- 你预期多 user / 多 caller_id（多租户）
- 你需要审计、可观测性、capability 门禁
- 你想把同一个工具同时给 Hermes + Claude Code + Cursor 用，不重复 N 套 adapter
- 工具底下是真后端服务（REST / DB / cloud API），不是 sandbox 内简单计算
- 你有多个 vendor 想一起 host 给同一个 agent

**不用 ATD 当：**

- 单进程脚本 + 单 user + 单工具（杀鸡用牛刀）
- 工具是 sandbox 内纯计算 / 无 side effect / 无外部依赖（直接函数调用就行）
- 工具在 agent 进程内（in-process Tool trait + Registry 不需跨 socket）
- 你只要 MCP，且不需要多租户 / 审计 / 跨 vendor 组合（直接写 MCP server）

### 1.5 与 raw 选项对比

#### 1.5.1 vs raw CLI

| 关注点 | CLI | ATD |
|---|---|---|
| 第一次调用成功率 | 需先猜命令 / flag | 1 次成功，0 retry |
| 多 agent 共享 | N 进程、N 配置、N OAuth | 1 server、N caller_id、broker 路由 |
| Audit | shell history | 结构化 JSON Lines |
| 跨 vendor 组合 | 自己写 multiplexer | 桥接多 socket |
| LLM matching | 看 `--help` 文本（混沌） | `description + intent_examples`（结构化） |
| 升级安全 | flag 加减破坏 agent prompt | tool def 是 schema，rev 跟踪 |

#### 1.5.2 vs raw MCP

MCP 是 client-server 协议，**缺**：

- server 侧 capability gate（每个 client 自己 gate）
- server 侧 rate limit
- multi-tenant token routing（MCP 假设单租户 stdio）
- audit log 标准格式
- tool visibility 多档（只有 hidden / visible 二元）
- safety levels（Read / Write / Financial / Privacy / Physical / Destructive）
- tier 概念（Hot / Warm / Cold + 推导 deadline）

ATD 在协议层 ship 这些，并通过 `atd-mcp-bridge` 兼容现有 MCP 客户端 —— Hermes、Claude Code、Cursor 不改一行代码就能接 ATD server。

#### 1.5.3 vs per-vendor 自研 adapter

每个 vendor 自己写一套 server？写过的人都知道：每写一次都重新设计 capability、audit、rate limit、token 管理、stop logic。`atd-runtime` + `atd-server` 是 ~2000 行 Rust，vendor 写自己的 server 只需要：

- Implement `Tool` trait（`fn definition() + fn call()`）
- 把工具注册进 `Registry`
- 调用 `atd_server::Server::new(registry, config).run().await`

healthkit_cli 的 `healthkit serve` 是 ~150 行 glue（一半是命令行参数解析）；多租户通过 `ServerConfig::token_broker` 添一行就启用。

### 1.6 1.0 已发布的稳定面

**v1.1.0** 是当前最新 SemVer tag（2026-05-27）。1.0.0 + 1.1.0 已 publish 到 crates.io，wire 格式冻结为 1.x 稳定面。发版策略自 2026-05-27 起切到 **per-crate independent SemVer**（[ADR 0004](../adr/0004-per-crate-versioning.md)，废弃早期 workspace-lockstep）：`atd-protocol` 的版本即 ATD wire/协议版本（tag `v<atd-protocol-version>` 锚此），其余 crate 各自按自身 source 变更独立 bump。

17 个 workspace crate（含 demo bin），Apache-2.0：

| crate | 职责 |
|---|---|
| `atd-protocol` | wire 格式 + 类型 + sanitize |
| `atd-sdk` | Rust 客户端 SDK（discover / describe / call / call_page / call_all / hello） |
| `atd-runtime` | server runtime（registry / dispatch / audit / rate limit / TokenBroker / UCAN 校验 / CursorIssuer / MetricsCounters） |
| `atd-server` | Unix socket listener + 连接任务 |
| `atd-server-http` | HTTP listener + MCP JSON-RPC translator + bearer auth + SSE refresh |
| `atd-middleware-fhir` | FHIR R4 egress validation |
| `atd-middleware-pii-redact-medical` | HIPAA PHI redaction |
| `atd-tools-{echo,fs,shell,web}` | 4 个内置工具示例 |
| `atd-ref-server` | 参考 server binary |
| `atd-mcp-bridge` | MCP/stdio ↔ ATD wire bridge |
| `atd-cli` | `atd` 开发者 CLI |
| `atd-conformance` | 跨实现 conformance fixture |
| `atd-mock-weather-server` | 跨 vendor demo bin（publish = false） |

外加 Python runtime `python/src/atd_server/` —— cbrain 那类 in-process Python embodied agent 用的 server-side 镜像。

---

## 2. 设计哲学（Design Philosophy）

### 2.1 三个消费者

任何 ATD tool server 同时面对三个消费者，且**互不冲突**只要分得清谁是谁：

| 消费者 | 需求 | 通道 |
|---|---|---|
| **LLM Agent** | 可发现的工具面 / 类型化错误信封 / 可预测的 arg shape | `tool_list` / `tool_schema` / `run_tool` over wire |
| **人类运维** | 审计轨迹 / 运维控制 / 结构化日志 / capability 拒绝可见性 | `AuditSink` 事件 / server log / metrics |
| **Agent 平台桥接**（Hermes / Claude Code / MCP） | 稳定握手 / capability 协商 / 不出意外的传输 | `Hello`/`HelloAck` + UCAN-lite，length-prefixed JSON over UDS/HTTP/stdio |

**Wire frame 是给 LLM 看的，audit sink 是给人看的，handshake 是给桥接用的**。同一个 server，三条管子，没有 flag、没有 mode。每个设计决策都要同时通过三个读者的检验 —— 让 LLM 爽但让桥接握手崩 = bug，不是 trade-off。

### 2.2 七条原则

| # | 原则 | 一句话 |
|---|---|---|
| 1 | ToolDefinition 是唯一真实源 | 从一份 `ToolDefinition` 生成 summaries / args 校验 / skills / adapter / 文档 —— 不要并行手维护 |
| 2 | Skill 跟着工具走，不跟着桥接走 | 暴露 `skills.list` / `skills.get` meta-tool；`atd skills sync` 按平台安装 —— 不手抄 SKILL.md 到 agent 平台 config |
| 3 | Capability 协商而非硬编码 | 声明 `required_capabilities`，与 `Hello.granted_capabilities` 求交集，dispatch 层 gate；handler 不做 auth 检查 |
| 4 | Error 类型化、namespace 化 | 协议占 1000-1099；adopter 占 2000+；不允许自由文本作为主信号 |
| 5 | 工具默认跨连接无状态 | 每连接一份 `ConnectionContext`；shared world state 必须显式声明 |
| 6 | Discovery 是 canonical —— 不要在 agent prompt 硬编码 tool id | agent 启动时 `discover`；新增工具自动出现；改名不破坏流程 |
| 7 | Dispatch 是 bounded + observable | tier deadline / 中间件 / 不在 server 内静默重试 / 失败可观测 |

下面逐条展开 + adopter 实证。

### 2.3 原则 1 —— ToolDefinition 是唯一真实源

**规则**：每个工具的每一个事实 —— 名字、args shape、需要的 capability、可见性、tier deadline、safety 分类 —— 只活在一个地方：`ToolDefinition` 结构体。Summary 是从它投影出来的，JSON Schema validation 读它，Skill meta-tool 服务它，LLM-adapter shape 从它生成。**没有第二份拷贝**。

**为什么**：当 args 同时存在 `ToolDefinition.input_schema` 和手写 `SKILL.md` 示例里时，它们会**静默漂移**。LLM 看 SKILL，server 按 schema 校验，`1005 invalid_arguments` 失败让人和 agent 都困惑。更糟：哪份成为"权威"是个意外（谁恰好下次更新谁就是）。

**反模式**（args 描述手维护）：

```python
# ❌ args shape 活在两处，必将漂移：
@server.register(definition=ToolDefinition(
    input_schema={"type": "object", "properties": {"path": {"type": "string"}},
                  "required": ["path"]},
    description="Read a file from `path` (required) and return its contents.",  # ← 会烂
))
```

新增 optional `encoding` 字段时只有 schema 更新，description 还说"from `path`" only。LLM 读 description 凭上下文猜编码。

**Adopter 检验**：

- ✅ **healthkit_cli** 从 Huawei HealthKit OpenAPI spec 生成 SKILL.md + CLI command + OpenAPI schema；同一份 spec 喂 `ToolDefinition`
- ✅ **celia_phr** 把 FHIR R4 schema 当 source of truth；`atd-middleware-fhir` 与 tool 声明的 shape 同源校验
- 🟡 **cbrain** 风险位：如果计划把 `hermes-config/skills/` 当 SKILL 家，SKILL 内容会成为第二个 source —— 建议改用 `cbrain:sim.skills.list/get` meta-tool

### 2.4 原则 2 —— Skill 跟着工具走，不跟着桥接走

**规则**：SKILL.md 是工具的**一部分**，不是 agent 平台的一部分。它住在 tool server 的 repo 里（与 `ToolDefinition` 同源），通过 `<publisher>:<service>.skills.list / .skills.get` meta-tool 暴露，由 `atd skills sync --target {hermes,claude-code,...}` 安装到各 agent 平台。**agent 平台的 skill 目录是 cache，不是 source**。

**为什么**：手抄到 `~/.hermes/skills/` 的 SKILL 跟着"配 agent 那个人"走，不跟着实际被调用的"工具版本"走。升级 server 不刷新 SKILL，加第二个 agent 平台要重复一份，换平台直接掉到地上。Agent 拿陈旧指引，维护者看不到。

**实现**：

```python
SKILL_ROOT = Path(__file__).parent / "skills"

@server.register(definition=ToolDefinition(
    id="cbrain:sim.skills.list",
    visibility=ToolVisibility.READ,
    required_capabilities=[],   # 公开 meta-tool
))
async def list_skills(args, ctx) -> dict:
    return [{"name": p.parent.name, "description": _read_description(p)}
            for p in sorted(SKILL_ROOT.glob("*/SKILL.md"))]
```

agent 主机上：

```bash
atd skills sync --target hermes      # → ~/.hermes/skills/cbrain-sim-<name>/SKILL.md
atd skills sync --target claude-code # → ~/.claude/skills/cbrain-sim-<name>/SKILL.md
```

### 2.5 原则 3 —— Capability 协商而非硬编码

**规则**：工具在 `ToolDefinition.required_capabilities: list[str]` 声明不透明 capability 字符串。连接在 `Hello` 时通过 `ServerPolicy` 回调协商 `granted_capabilities`。Dispatch 在每个 `run_tool` 算 `missing = required - granted`；非空则 `ERR_CAPABILITY_DENIED` (1001) 带 `details = {required, granted, missing}`。**Handler 自身不查 capability** —— dispatcher 已经查过。

**为什么硬编码"this tool requires admin" 在 handler 里有 4 个问题**：

1. 检查对 LLM 不可见 —— `tool_list` 反映不出
2. 不同 handler 漂移（有的查 env、有的查 header、有的查 client_id）
3. Audit / observability 看到失败太晚 —— handler 已经开始跑
4. 未来想 pre-fetch capability（如 UI 权限弹窗）的桥接没东西可查

外化到 `required_capabilities` + dispatcher gate 解决全部四个。LLM 在 `tool_schema` 看到要求；检查统一；audit sink 在 handler 跑之前就看到 denied；桥接可以预取 schema。

### 2.6 原则 4 —— Error 类型化、namespace 化

**规则**：每个失败带数字 `code`，落在两个区段之一：

- **1000-1099** —— 协议级，定义在 `crates/atd-protocol/src/messages.rs`：
  - `1000` ToolNotFound · `1001` CapabilityDenied · `1002` RateLimited · `1003` BrokerFailed · `1004` DeadlineExceeded · `1005` InvalidArgs
  - `1010-1013` UCAN（invalid / expired / delegation-too-deep / audience-mismatch）
  - `1020-1021` Cursor（expired / invalid）
  - `1099` Internal
- **2000+** —— adopter 区段：cbrain 2000-2099 / healthkit 3000-3099 / celia 4000-4099（per `SP-error-namespace-v1`）

**禁用自由文本错误字符串**。`ToolError(code=2042, message="cbrain skill aborted", partial_data={...})` 是对的。`raise Exception("something broke")` 落到 `1099 INTERNAL`，按"需维护者处理"事件记录。

**为什么**：数字 code 跨越翻译生存。LLM 看到 `code: 1001` 可以恢复（请求缺的 capability，或退避）。自由文本 `"forbidden"` 需 LLM 读散文，不同 adopter 措辞不同，恢复不可靠。Namespace 防冲突 —— 没它的话两个 adopter 都挑 `code: 42` 表示"事情坏了"，桥接分不清。

### 2.7 原则 5 —— 工具默认跨连接无状态

**规则**：每个连接拿一份新 `ConnectionContext`，只带 `Hello` 协商出的状态（`client_id`、`granted_capabilities`、`ucan_tokens`）。同一个 agent 进程的两个连接拿两份独立 context。Server 侧共享状态（singleton 模拟器、session pool、in-memory KV）**必须 opt-in 且显式声明** —— `SP-session-model-doc`（队列中）会给 `HelloAck.session_model` 加字段。

**例外**（shared world state）真实且有用 —— cbrain-sim 是原型：`MjData` 是 singleton，所有 client 看同一份物理。**让它"显式声明而非隐式"**防止 adopter 被"号称无状态"的工具突然 mutate 共享 buffer 吓到。

### 2.8 原则 6 —— Discovery 是 canonical

**规则**：Agent 在运行时通过 `tool_list → tool_schema` 发现工具。Agent prompt **禁止**硬编码 tool id list。新工具自动出现；改名不破坏流程（client 每个 session 重新 discover）。`ToolSummary` 的 `id` 是唯一稳定 handle；其他都是人类面的散文，可改不破坏 agent。

**反模式**：

```
❌ System prompt:
   "You may call cbrain:perception.snapshot, cbrain:manipulation.pick,
    cbrain:world.reset. Always start by calling perception.snapshot."
```

当 cbrain-sim 加 `cbrain:perception.depth_snapshot` 时 prompt 不知道；当 `manipulation.pick` 改名 `manipulation.grasp` 时所有 agent 一起断。两种情况在 discovery-driven prompt 下都不是问题。

### 2.9 原则 7 —— Dispatch 是 bounded + observable

每个工具调用包在三个 contract 里：

- **Bounded**：deadline 由 `definition.resources.timeout_ms` 推导（unset 默认 30s）。超时返回 `1004 DEADLINE_EXCEEDED`。**没有工具永远跑**。
- **Observable**：middleware（`pre_call` / `post_call` / `on_error`）看到每个 dispatch。需要 audit（cbrain 的 Merkle trace）、tracing（OpenTelemetry）、rate limit、metric 的 adopter 都通过中间件实现。Dispatch 路径自身不静默吞。
- **No silent retries**：server 从不在内部重试工具调用。Tool 短暂失败时返回 `retryable=True`，让 client 决定。**Server 侧静默重试隐藏失败、对 side-effect 重复扣费**。

### 2.10 反模式速查

- ❌ 手抄 SKILL.md 到 agent 平台 config 目录（原则 2）
- ❌ 手写 args description 重复 `input_schema`（原则 1）
- ❌ Per-handler 硬编码 auth / capability 检查（原则 3）
- ❌ 返回自由文本错误字符串不带数字 code（原则 4）
- ❌ 用 module-global state 模拟 per-connection（原则 5）
- ❌ Tool id 烧进 agent system prompt（原则 6）
- ❌ Handler 内部隐式 retry loop（原则 7）
- ❌ `raise Exception("...")` 作为主失败路径（原则 4 + 7）
- ❌ 捕获 `asyncio.CancelledError` 后继续（原则 7）
- ❌ 给 wire frame 加 per-platform shim（破坏跨实现 byte-compat）

### 2.11 Adopter Checklist（精简版）

设计或审计一个 ATD tool server，逐项检验：

- [ ] 每个工具事实只在一份 `ToolDefinition` 里
- [ ] `ToolSummary` *派生*自 `ToolDefinition`，不手维护
- [ ] SKILL.md `description` frontmatter 与 `ToolDefinition.description` 一致
- [ ] 工具暴露 `<publisher>:<service>.skills.list` + `.skills.get`
- [ ] `~/.hermes/skills/` / `~/.claude/skills/` 内不手抄 SKILL.md
- [ ] Agent prompt 不硬编码 tool id
- [ ] 每个需要 cap 的工具声明 `required_capabilities`
- [ ] `ServerPolicy` 用 allow-list 求交集（不"grant 一切"）
- [ ] Handler 内无 `if not has_cap(...)`
- [ ] 每个 `ToolError` 带数字 `code`，落对 namespace
- [ ] `retryable` 诚实标记
- [ ] 主失败路径无 `raise Exception(...)`
- [ ] 每个工具状态模型显式：stateless / per-connection / shared-world
- [ ] Shared-world 工具在 description 说清楚
- [ ] 每个工具设 `resources.timeout_ms`（或自觉接受 30s 默认）
- [ ] 中间件实现 audit / tracing / rate；dispatch 可观测
- [ ] Handler 内无静默 retry loop
- [ ] `asyncio.CancelledError` 总是 re-raise
- [ ] 无 platform-specific shim 包 wire frame
- [ ] 跨语言实现先过 `atd-conformance` fixture

---

## 3. 架构（Architecture）

### 3.1 统一 schema

ATD 最核心的一个 claim —— 也是最值得先理解的：**每个 wire 上的消息，每个方向，每个 transport（UDS 或 HTTP），都序列化为一份机器可读 schema 定义的 shape**：`/atd-protocol-schema.json`。

Schema 从 `atd-protocol` 的 Rust type 通过 `schemars` 生成，与 [JSON Schema 2020-12 meta-schema](https://json-schema.org/draft/2020-12/schema) 校验，CI 拒 PR 让 Rust source 与已发布 JSON 漂移。

Schema 覆盖：

| 层 | Schema 覆盖的类型 |
|---|---|
| Envelope | `ClientMessage` (=`Request`), `ServerMessage` (=`Response`) |
| Handshake | `Hello`, `HelloAck`, `Ping`, `Pong` |
| Discovery | `ToolList` request/response · `ToolSchema` request/response · `ToolSummary` · `DiscoverFilter` |
| Invocation | `RunTool` · `RunToolContinue` · `ToolResultResponse` · `CallOptions` |
| Tool 描述 | `ToolDefinition` · `ToolCapability` · `ToolBinding` · `ToolSafety` · `ToolResources` · `ToolTrust` · `ToolErrorDef` |
| 枚举 | `SafetyLevel`, `ToolVisibility`, `TrustLevel`, `ToolTier`, `BindingProtocol` |
| 错误 | `AtdError` taxonomy + wire codes |
| 分页 | `CursorPayload`, `next_cursor` |
| Capability 协商 | `CapabilitySet` · `Hello.requested_capabilities` · `HelloAck.granted_capabilities` · `Hello.ucan_tokens` |

**一份 schema 的价值**：

- **跨语言 SDK parity**：TS / Go / Swift SDK 从 schema 生成即自动与 Rust SDK + Rust server 类型兼容
- **跨 transport parity**：UDS 与 HTTP listener 共用 `atd-runtime::dispatch::dispatch_request` 入口和类型；加第三种 transport（如 WebSocket）只写新 listener
- **审计 / 分析**：audit log 出现的任何字段名、agent 看到的任何错误 code、任何工具 metadata —— 都可回溯到 schema，没有"只能读 Rust 源码"的隐藏字段
- **Conformance 可测试**：`atd-conformance` ship test scenario 验证任何 ATD-speaking server 对 schema 契约行为的遵守

**1.0 schema 已冻结为 1.x 稳定面**：additive 改（新 optional 字段、新 enum variant）是 minor；移除字段或改 shape 是 major。1.0 时生成的代码持续反序列化每个 1.x 消息。

### 3.2 5 层模型 / 3 核心 + 2 扩展

```
┌────────────────────────────────────────────────────────────────┐
│  User intent (voice · text · trigger)                          │
└────────────────────────────┬───────────────────────────────────┘
                             │
┌────────────────────────────▼───────────────────────────────────┐
│  Agent framework                                               │
│  (Claude Code · Cursor · Hermes · LangChain · custom)          │
└────────────┬──────────────────────────────┬────────────────────┘
             │                              │
   via Skill │                              │ direct tool call
             ▼                              ▼
┌──────────────────────────────┐  ┌───────────────────────────┐
│  Skills layer (adjacent)     │  │  (no Skill intermediary)  │
└──────────────┬───────────────┘  └──────────────┬────────────┘
               │                                 │
               └──────────────┬──────────────────┘
                              ▼
┌────────────────────────────────────────────────────────────────┐
│  Client SDK                                                    │
│  discover · describe · call · call_page · call_all             │
└────────────────────────────┬───────────────────────────────────┘
                             ▼
┌────────────────────────────────────────────────────────────────┐
│  Dispatch (deterministic pipeline)                             │
│  capability gate · tier · binding · cursor · middleware        │
└────────────────────────────┬───────────────────────────────────┘
              ┌──────────────┴───────────────┐
              ▼                              ▼
   ┌─────────────────────┐         ┌─────────────────────┐
   │  Unix socket        │         │  HTTP / MCP JSON-RPC│
   │  (atd-server)       │         │  (atd-server-http)  │
   └─────────────────────┘         └─────────────────────┘
                             ▼
┌────────────────────────────────────────────────────────────────┐
│  Tool universe (bindings + extension points)                   │
└────────────────────────────────────────────────────────────────┘
```

**3 个核心机制**：

1. **Schema**（§3.1）—— 统一、机器可读、每个 wire shape 的唯一真实源
2. **Dispatch**（§3.4）—— 确定 pipeline：capability gate → tier-aware deadline → binding 选择 → tool 调用 → cursor / middleware
3. **Security**（§3.5）—— 分类 + per-tool runtime 控制 + capability allow-listing + UCAN-lite + 多租户 secret routing + audit

**2 个扩展机制**：

- **Bindings**（§3.4.4）—— 可插拔调用后端。参考实现 ship `NativeBinding` 与 `CliBinding`；trait 开放
- **Middleware**（§3.6）—— egress validation / redaction pipeline。参考实现 ship path-redaction、FHIR validation、HIPAA PHI redaction；trait 开放

### 3.3 Wire 与类型

Wire 是**length-prefixed JSON** over duplex byte stream。两个 listener（`atd-server` over UDS、`atd-server-http` over HTTP+SSE）把 transport 层 framing 翻译成同样的 in-memory `ClientMessage` / `ServerMessage`。

**6 个 request 变体**：

| 变体 | 用途 |
|---|---|
| `Hello` | 连接握手。带 `client_id` / `requested_capabilities` / 可选 `ucan_tokens`。Server 回 `HelloAck` 带交集后的 `granted_capabilities` |
| `Ping` | 心跳 |
| `ToolList` | Discovery。返回 `Vec<ToolSummary>`，按 `DiscoverFilter` 过滤（visibility / capability / tier） |
| `ToolSchema` | 单工具深 describe。返回完整 `ToolDefinition` 含 JSON input/output schema 和 intent example |
| `RunTool` | 调用。带 `tool_id` / `args` / `CallOptions`。返回 `ToolResultResponse`（success-data 或 error-code） |
| `RunToolContinue` | 分页续传。带上一个 response 返回的 opaque `cursor` |

**`ToolDefinition` 完整结构**：

```rust
pub struct ToolDefinition {
    pub id: String,                       // "ref:fs.read" 等
    pub name: String,
    pub description: String,
    pub version: String,
    pub capability: ToolCapability,       // domain · actions · tags · intent_examples
    pub input_schema: serde_json::Value,  // JSON Schema 2020-12
    pub output_schema: serde_json::Value,
    pub bindings: Vec<ToolBinding>,
    pub safety: ToolSafety,               // level · dry_run · side_effects
    pub resources: ToolResources,         // timeout_ms · max_concurrent · ...
    pub trust: ToolTrust,                 // publisher · trust_level · signature
    pub visibility: ToolVisibility,       // Read / Write / Dangerous / System / Hidden
    pub required_capabilities: Vec<String>,
    pub tier: Option<ToolTier>,           // Hot / Warm / Cold
    pub errors: Vec<ToolErrorDef>,
}
```

**错误 taxonomy**：两层隔开 ——

- **`AtdError`** —— client 侧 Rust enum（`ToolNotFound` / `InvalidArguments` / `CapabilityDenied` / `BindingUnavailable` / 执行失败 / `PaginationLimitExceeded` / `MergeFailed` / ...）
- **数字 wire code** —— `ERR_*` u16 常量在 `atd_protocol::messages`，落在 `Response::Error.code`

完整表见 [`docs/protocol/error-codes.md`](../protocol/error-codes.md)。

**Cursor 分页**：超过 1MB 输出预算的工具，wire 携带 opaque、HMAC-签名的 `next_cursor` 字符串。Client 通过 `RunToolContinue { tool_id, cursor }` 续传。Cursor 绑定到 `(tool_id, caller_id, args_fingerprint, page_index, issued_at_unix, server_session)` —— 不能跨 caller 重放、不能针对篡改的 args 重放；验证 stateless。默认 TTL 5 分钟，wire cap 512 字节。

**Sanitization**：tool id 含 `:` 和 `.`，会破 LLM / MCP 函数名 slot。`ref:fs.read` ↔ `ref_fs_read` 由 `atd-sdk::sanitize` 做规范化双向映射。Wire 上走 canonical（`ref:fs.read`），LLM tool-calling shape 内走 sanitised（`ref_fs_read`）。

### 3.4 Dispatch pipeline

每次调用走确定 pipeline：

```
accept connection
  → Hello handshake (capability gate, optional UCAN verify)
  → receive RunTool / RunToolContinue
  → registry.get(tool_id)
  → capability check (refuse if required ⊄ granted)
  → tier-aware deadline + max_output_bytes 解析
  → TokenBroker::resolve(caller_id) → CallContext::secrets
  → binding.invoke(args, &ctx)         // 或 call_paginated 当 cursor 存在
  → middleware pipeline (RedactPaths, FHIR, PII, ...)
  → serialize ToolResultResponse + 可选 next_cursor
```

Dispatch **transport-agnostic**：UDS 和 HTTP 都 call into 同一个 `atd_runtime::dispatch::dispatch_request`。

#### 3.4.1 核心 SDK API

| API | 用途 | SDK 形式 |
|---|---|---|
| `discover` | 列出可见工具 | `AtdClient::discover(filter) -> Vec<ToolSummary>` |
| `describe` | 取完整 `ToolDefinition` | `AtdClient::describe(tool_id) -> ToolDefinition` |
| `call` | 调用返回单 result | `AtdClient::call(tool_id, args, CallOptions) -> ToolResult` |
| `call_page` | 单页分页 | `AtdClient::call_page(tool_id, args, Option<&cursor>, opts)` |
| `call_all` | 自动走 cursor chain | `AtdClient::call_all(tool_id, args, CallAllOptions)` |
| `ping` | 心跳 | `AtdClient::ping()` |
| `hello` | Capability 协商 | `AtdClient::hello(Some(client_id), requested_caps) -> Vec<String>` |

Python SDK 在 `python/src/atd_client/` 镜像同样 API（含 sync + async）。Python **server runtime** 在 `python/src/atd_server/`（SP-server-py-v1，2026-05-19）—— 通过 22/24 conformance fixture，给 cbrain 那类 in-process Python embodied agent 用。

#### 3.4.2 Capability gate —— 两机制组合

**1. 操作员 allow-list（字符串）**：server 启动时声明它 offer 的 capability string（`--grant-capability healthkit:read`）。Client `Hello.requested_capabilities` 与 offer 求交集；任何 requested 但 server 未 offer 的被静默丢弃。

交集 strict 双向：

- Requested but not offered → not granted
- Offered but not requested → not granted
- Requested ∧ offered → granted

**2. UCAN-lite bearer token**：当 client 发送一或多 JWT-shape `Hello.ucan_tokens`，server UCAN verifier 走 attenuation chain（`prf[]` 链接子到父），通过 `did:key` audience 钉验 Ed25519 签名，发出 leaf token 实际带的 capability subset。**Dispatch 级 `granted_capabilities` = `strings ∪ ucan_capabilities`**。每个 chain link 通过 `UcanRevocationStore` trait 查 revocation。Bounded chain depth（默认 5）。

UCAN-lite **additive**：不发送 token 的 client 只走 string allow-list 路径。两条路径产出同样 `granted_capabilities` shape；工具不知道 caller 走哪条。

#### 3.4.3 Tier-aware deadline

每个工具声明 `ToolTier`（`Hot` / `Warm` / `Cold`）。Dispatch 从 tier 解析每次调用的 deadline + max-output-bytes 预算，可通过 `CallOptions::deadline_ms` 和 server 的 `--tier-override` 调整。**Tier 是 latency/cost class 信号 —— 不是生命周期策略**。

| Tier | 默认 deadline | 典型用途 |
|---|---|---|
| `Hot` | sub-second | 同步无 side-effect 查询（time / env） |
| `Warm` | seconds | 大多数工具 —— 文件 IO / shell / web fetch |
| `Cold` | minutes | 慢导入 / 大导出 / 模型推理 |

Cursor-paginated 工具的 tier deadline **per page** 算 —— Cold 工具可以在长 wall-time 上流式输出而不违反 page 级 SLO。

#### 3.4.4 Bindings

Binding 是 dispatch 把 `(args, CallContext)` 变 `Result<Value, ToolCallError>` 的抽象方式。`Binding` trait 开放；2 个参考实现：

| Binding | 行为 |
|---|---|
| `NativeBinding` | 委托给同 Rust 进程的 `Tool` impl。每个内置工具默认 |
| `CliBinding` | 派生子进程，JSON args → argv，捕获 stdout/stderr，honor `ctx.deadline` 配 SIGTERM-then-SIGKILL grace。Demo: `ref:external.uname` |

Trait 开放。`GrpcBinding` / `WasmBinding` / 假想 `McpBinding` 实现同 `Binding::invoke` 签名，dispatcher 通过 `ToolBinding::protocol` 选。v1 总路由到工具声明的第一个 binding；多 binding 选择是 dispatcher 小升级。

#### 3.4.5 Secret routing —— TokenBroker

多租户 adopter（一个 server 进程通过一个 socket 服务多个 OAuth user）需要 per-caller secret 不互相看到 token：

```rust
pub trait TokenBroker: Send + Sync {
    fn resolve(&self, caller_id: Option<&str>) -> ResolveFuture;
    fn resolve_bearer(&self, bearer: &str) -> ResolveBearerFuture;
    fn accepted_token_formats(&self) -> &'static [&'static str];
}
```

参考实现：

- `InMemoryTokenBroker` —— 单元测试 / 单进程；UCAN-JWT 分支通过 `register_ucan_audience()` dispatch
- `FileTokenBroker` —— 磁盘后端；每 bearer subdir `${root}/${bearer_id}/{access_token,refresh_token,expires_at}.json` 配 Unix 0700/0600；per-bearer refresh mutex 防 OAuth 双 round-trip；`is_near_expiry()` 是 no-IO 谓词

生产部署在 vault 或 secrets-manager 上自实现 `impl TokenBroker`。Trait `pub` stable。

**HTTP bearer auth** 是同 trait 的 `resolve_bearer` 臂。HTTP listener parse `Authorization: Bearer ...`，call `broker.resolve_bearer(token)`，回类型化 `BearerOutcome`（11 个变体：Ok / OkShrunk / Expired / Revoked / Unknown / Internal / Lookup / ...），每个映射到具体 HTTP status + `WWW-Authenticate` + 可选 `Retry-After`。SSE bearer-refresh helper 做 60s 心跳 re-resolution，emit `RefreshEvent::{Refreshed, AuthLost}`。

`CallContext::secrets: Option<Arc<SecretBundle>>` 由 dispatcher 在 `Tool::call` 前填充。工具读 `ctx.secrets().get("access_token")` 即可；不读的工具忽略。`SecretBundle` 把 value 包在 `RedactedString` —— `Debug`/`Display` 拒打印，意外 log 行不漏 credential。Audit event 只含 `secrets_resolved: bool`。

### 3.5 Security

#### 3.5.1 三轴分类

每个工具声明三个正交分类作为 `ToolDefinition` 的一部分。这些是**描述 metadata** —— 让 caller 和 operator 推理风险用；不是单独的执行机制（capability gate + per-tool runtime control 才是实际执行）：

| 分类 | 值 | 字段 |
|---|---|---|
| Safety | `Read` / `Write` / `Financial` / `Privacy` / `Physical` / `Destructive` | `ToolSafety::level` |
| Visibility | `Read` / `Write` / `Dangerous` / `System` / `Hidden` | `ToolVisibility` |
| Trust | `L0Unverified` / `L1SchemaValid` / `L2Tested` / `L3Verified` / `L4Certified` | `ToolTrust::trust_level` |

`Visibility::Hidden` 把工具从 `ToolList` discovery 排除但保留 `ToolSchema` 和 `RunTool` 可达 —— 用于 raw vendor endpoint、debug helper、集成测试工具。

#### 3.5.2 Per-tool runtime 控制

防御活在工具内部，不在 dispatch 层 —— 每个守一个该工具类暴露的攻击面：

| 控制 | 适用 | 位置 |
|---|---|---|
| **SSRF guard** | `ref:web.fetch` | `crates/atd-tools-web/src/fetch.rs::check_ssrf` |
| **Header allow-list** | `ref:web.fetch` | 同上，`build_headers` |
| **Must-read-before-edit** | `ref:fs.edit` | `tracker.rs` + `atd-tools-fs/src/edit.rs` |
| **SIGTERM → grace → SIGKILL subprocess timeout** | `ref:shell.exec` / `pwsh` | `atd-tools-shell/src/shared.rs` |
| **Per-tool semaphore** | 全部工具 | `atd-runtime/src/registry.rs` |
| **Request-arg schema validation** | 全部工具 | per-tool `call` impl + serde |

SSRF guard 覆盖：loopback + RFC1918 + link-local + CGN + TEST-NET + 0.0.0.0/8 + IPv4-mapped-private —— **重定向每跳重新检查**。Header allow-list 只接受 Accept / Accept-Language / Referer / User-Agent；Authorization + Cookie 用 `InvalidArgs` 拒。

#### 3.5.3 Audit

每个 dispatch 调用 emit 一条结构化 `CallEvent` 到配置 `AuditSink`：

```rust
pub struct CallEvent {
    pub ts: String,                  // RFC3339
    pub call_id: String,
    pub tool_id: String,
    pub caller_id: Option<String>,
    pub granted_capabilities: Vec<String>,
    pub duration_ms: u64,
    pub outcome: Outcome,            // Success / ExecutionFailed / InvalidArgs / ...
    pub tier: String,
    pub dry_run: bool,
    pub schema_version: u32,         // 当前 2
    pub secrets_resolved: bool,      // 永不含 key 名或 value
    pub cursor_page: Option<u32>,    // 1-based 页 index
}
```

参考 sink `JsonLinesAuditSink` 写 JSONL 到配置路径；读通过 bounded `tokio::sync::mpsc` channel + 专用 writer task，所以 `on_call` non-blocking —— 慢盘 back up writer queue 而不是 dispatch loop。`drops` counter 通过 `Server::metrics_snapshot()` 暴露。

Adopter 需要不同 sink（Kafka / OpenTelemetry / ...）实现 `AuditSink` 接自己 pipeline。Trait `pub` stable。

#### 3.5.4 限流与并发

| 机制 | 行为 |
|---|---|
| `ToolResources::max_concurrent` per-tool semaphore | 在 Registry 强制；permit 耗尽时拒 `RateLimited` (1002, retryable) |
| Multi-thread tokio runtime | Ref binary 默认 `multi_thread` 配 `min(cpus, 4)` worker（`atd_runtime::default_worker_threads()`）。Accept loop 不再被单个 in-flight call 饿死 |
| Per-state frame deadline on UDS | 5s 握手 / 30s 活跃；通过 `Server::set_frame_deadlines` 配置 |
| SDK connect retry 指数退避 + ±20% jitter | `AtdClient::connect_with_options` 通过 `ConnectOptions` / `ATD_CONNECT_RETRIES` env 配 |
| Server 侧 rate-limiter（governor token bucket） | v1 不在。`ToolResources::rate_limit_per_min` 当前 declarative-only |

50-client `concurrent_handshake_storm` conformance scenario 验证 SLO：4-core dev 机上 p99 < 200 ms（实测 125 ms），0 错，0 audit drop。

#### 3.5.5 Dry-run

`CallOptions::dry_run: bool` 是 wire 字段。Server 侧 dispatcher 在 `dry_run: true` 时短路返回合成 `tool_result` 不调用工具 —— 所以 `ref:shell.exec("rm -rf /", dry_run=true)` 不会真跑。`ToolSafety::dry_run: true` 在工具 metadata 标记工具自己有有意义的 dry-run preview 路径；路由到那条路径是 follow-up。v1 是纯 server-side short-circuit。

### 3.6 Middleware

Middleware pipeline 是工具成功返回与 wire reply 之间的 egress hook。Trait `atd_runtime::Middleware` 取 `(tool_id, &ToolDefinition, &mut serde_json::Value)`；impl 可以重写 value、剥敏感子树、或 mutate 成错误信封拒绝。错误绕过原样流过。

Pipeline 在 `Server::new` 时组装：

```rust
let mut server = Server::new(registry, cfg);
server.set_middleware(vec![
    Arc::new(FhirMiddleware::default()),
    Arc::new(PiiRedactMiddleware::default()),
    Arc::new(RedactPathsMiddleware::default()),
]);
```

**3 个内置中间件**：

| Middleware | Crate | 作用 |
|---|---|---|
| `RedactPathsMiddleware` | `atd-runtime` | 剥或 mask JSON-Pointer 路径（如剥 shell output 的 `$HOME`） |
| `FhirMiddleware` | `atd-middleware-fhir` | FHIR R4 egress validation。确认 `resourceType` 在 12-resource 已知集；coding system URI 对 75-URI `ALLOWED_SYSTEMS_DEFAULT` 校（与 celia `whitelists.toml` 通过 I1 drift-guard 保 set-equal）；required field 存在 per resource。3 个 `MismatchPolicy`：`AnnotateAndPass` 默认 / `ReplaceWithError` fail-closed / `StripOffending` |
| `PiiRedactMiddleware` | `atd-middleware-pii-redact-medical` | HIPAA Safe Harbor PHI redaction。18 identifier 类 × 13 JSON-Pointer × 7 `RedactionStrategy` + 5 catch-all 正则（SSN / driver's license / IP / URL / email） |

两个医疗中间件在独立 crate —— 不 ship FHIR / PHI payload 的 adopter 不拉依赖。

**Whitelist 不变量 I1**：`atd_middleware_fhir::ALLOWED_SYSTEMS_DEFAULT` 是 FHIR egress 准入 CodeSystem URI 的 canonical 集。它**保 set-equal** 于 celia 的 source-of-truth `crates/celia-types/data/whitelists.toml`（vendored 到 `crates/atd-middleware-fhir/vendor/`）。单元测试在 `systems.rs` 通过 `include_str!` 解析 vendored toml 并在每次 `cargo test` 断 set 相等；漂移则失败打印差集。**反方向**对称：celia 通过 `use atd_middleware_fhir::ALLOWED_SYSTEMS_DEFAULT` 跑同样 set-equal 断言。任一仓单独更新 set 就在两 CI gate 之一失败。

### 3.7 Skills 层（adjacent）

Skills 层（SKILL.md + `atd-tools:` 依赖声明 + 渐进披露 skill 体）在 layer model 中位于 ATD 之上。从协议角度，Skills 是 ATD 的**上游消费者**，不是 ATD 自身的一部分。

| 关注 | 拥有者 |
|---|---|
| SKILL.md 创作 / 校验 / 安装 | Skills runtime（Anthropic Skills / OpenClaw ClawHub / 第三方） |
| 渐进披露到 agent context | Skills runtime |
| `atd-tools:` 依赖声明 | SKILL.md 格式；ATD 的贡献是稳定 tool id |
| 从 skill body 调 ATD 工具 | Skills runtime 像任何 agent 一样调 ATD SDK |
| `discover` / `describe` / `call` API | ATD（本仓） |

ATD 给 Skills 的承诺：稳定 `discover` / `describe` / `call` 语义、稳定 `AtdError` taxonomy、稳定 tool-id 约定、稳定 skill discovery meta-tool 命名。

### 3.8 Crate 与扩展点

完整 17-crate 拓扑见 [`docs/atd-architecture.md`](../atd-architecture.md) §9.1。

**扩展点**（第三方代码不 fork 参考 server 即可挂接）：

| 想做… | 表面 | 需 fork? |
|---|---|---|
| 加新工具 | `Tool` trait impl + `Registry::register` | 否 |
| 加新 binding | `Binding` trait impl | 否 |
| 加新 middleware | `Middleware` trait impl + `Server::set_middleware` | 否 |
| 加新 auth scheme | `TokenBroker` trait impl + `ServerConfig::token_broker` | 否 |
| 加新 audit sink | `AuditSink` trait impl + `ServerConfig::audit_sink` | 否 |
| 加新 transport | 新 listener crate call `atd_runtime::dispatch::dispatch_request` | 否 |
| 改 wire 格式 | — | 是（不是扩展点） |

### 3.9 Non-goals（v1 故意不做）

- **Multi-device routing** —— ATD 每连接 dispatch 到一个 socket，不路由"用户当前在用的设备"
- **Distributed session（迁移 / fork / handoff）** —— Session scope 到一个连接
- **Tool signature verification** —— `ToolTrust::signature` declarative；签名 scheme 需 PKI 基础设施协议未规范
- **REST / AppFunction / 分布式 binding** —— Binding trait 可承载，参考实现只 ship `NativeBinding` 和 `CliBinding`
- **Native Skills-layer 支持** —— ATD 故意与 Skills runtime 分离
- **Per-tool dry-run preview semantics** —— v1 是 server-side short-circuit
- **Per-tool rate-limiter 强制** —— `rate_limit_per_min` declarative-only

每个 non-goal 都有 rationale；adopter 信号可推到 roadmap，门槛是**具体需求而非愿景**。

---

## 4. 高价值应用场景

ATD 的协议级抽象 —— capability / audit / multi-tenant / cursor / cross-vendor —— 在以下场景产出非线性价值。这些场景的共通点是：**raw CLI / raw MCP / 自研 adapter 都能"勉强能跑"，但要做到生产质量都得重新实现 ATD ship 的中间层**。

### 4.1 个人健康记录（PHR） / 医疗 vertical

**典型**：[celia_phr](#5-深度案例celia_phr--最复杂的-atd-adopter)（本文档下一章详述）、[healthkit_cli](https://github.com/downsea/healthkit_cli)

**用 ATD 解的问题**：

- **FHIR R4 + HIPAA PHI 合规** —— `atd-middleware-fhir` + `atd-middleware-pii-redact-medical` 即装即用；coding system whitelist 通过 drift-guard 跟 celia source-of-truth set-equal
- **多用户、多 OAuth token** —— `TokenBroker` + `caller_id` 路由；audit 只记 `secrets_resolved: bool`
- **多 agent 子委托** —— UCAN-lite Hello.ucan_tokens 走 attenuation chain（Ed25519 + did:key），Agent A 把"读 Patient X"3 个月只读 delegate 给 sub-agent B
- **跨厂商健康数据**（华为 HMS / Apple HealthKit / Garmin / Fitbit）—— 每 vendor 一个 ATD server，agent 看合并 catalog
- **端到端加密不破** —— DEK 留在 Tauri 进程内存，ATD 仅 dispatch 解密后 JSON；middleware 在 egress 截 PHI，不接触加密层

### 4.2 Agent-Native CLI

**典型**：[healthkit_cli](https://github.com/downsea/healthkit_cli)、`agentic-native-cli (mycli)`、`oh-cli`

CLI 既是给人用的命令行工具，也是给 agent 调的 ATD server。`--atd-serve` flag（`agentic-native-cli` 在 1.1.0 同日加）让一个 binary 四个出口：

1. 人类 CLI 子命令
2. `--atd-serve` 启动 ATD Unix server
3. 走 `atd-mcp-bridge` 接到 MCP 客户端
4. 走 `atd-server-http` 接到 HTTP / MCP-over-HTTP

**ATD 价值**：

- 给 LLM 暴露的不是 `--help` 散文而是结构化 `description + intent_examples + input_schema`
- 同一个工具既给人也给 agent，工具行为零分叉（"agent mode" 不是另一份代码）
- skills.list / skills.get 让 SKILL.md 跟着 CLI 版本走，agent 平台拿到的永远是当前 CLI 的指引

### 4.3 跨厂商工具组合（cross-vendor）

**典型**：`cross-vendor-demo.sh`（healthkit + atd-mock-weather-server）

`atd` 是协议；同一个 agent 可以连 N 个 ATD server，每个自己一个 socket / audit / token store，agent 看到合并 catalog。**CLI 做不到的关键点**：不能在一个 CLI 进程里同时 `+heartrate` 和 `+weather.now`。

**ATD 价值**：

- 桥接多 socket / 多 HTTP endpoint
- 每 vendor 自治（自己的 audit、自己的 broker、自己的 capability allow-list）
- Agent 只见合并 catalog，不感知 vendor 边界

### 4.4 Embodied agent / 具身 LLM 仿真

**典型**：[cbrain](https://github.com/downsea/cbrain) —— MuJoCo 物理仿真 + LLM agent

**为什么 ATD Python server 是对的**：

- 工具与共享物理状态（`MjData` singleton）必须 in-process —— Rust server 跨 socket 太慢
- 多 agent 看同一份物理（shared world state），通过 `description` 显式声明
- Merkle audit 走 `Middleware::post_call`，每次 `pick / place / reset` 都进 chain
- `resources.timeout_ms` 把仿真步限制在 2s —— LLM 不会被卡住

**ATD 价值（cbrain 触发了 SP-server-py-v1）**：

- Python 进程的 `atd_server` runtime 跟 Rust runtime byte-compat（通过 22/24 conformance fixture）
- 同一份 wire 协议，物理仿真也能被任何 ATD client 调（Hermes / Claude / 未来的 TS SDK）

### 4.5 跨设备 federation（healthkit 远程数据）

**典型**：[celia-connectors Phase L](#56-phase-l-federation--healthkit_cli-作为远程-atd-endpoint)

celia_phr 把 healthkit_cli 当远程 ATD endpoint 接进自己，做"健康数据联邦"：

- `AtdUpstreamIngest` 通过 atd-sdk 走 cursor 分页拉远程 FHIR
- 每条记录加 `meta.source = atd://<endpoint>/<tool>` provenance
- `CursorStore`（持久为 FHIR `Basic` 资源）+ `SyncOrchestrator`（tick scheduler + ±20% jitter + 指数退避 1m→5m→30m→2h + 5-failure → Degraded + per-task tokio::spawn 失败隔离）
- 远程 ATD server 重启返回 `1020 ERR_CURSOR_EXPIRED` → `CursorStore::invalidate` 标 tombstone 重启拉

**ATD 价值**：

- Cursor 是 stateless HMAC 签名，跨 server 重启可恢复
- AuditSink 在两边都记联邦同步事件
- Vendor 中性 —— `celia-connectors` 不知道 healthkit；任何 ATD-speaking server 都能被接入

### 4.6 Agentic IDE / 代码助手

**典型潜在**：Cursor / Claude Code / 自研 IDE agent

**ATD 价值**：

- Tool surface 不重复实现 —— 同一份 `ref:fs.*` / `ref:shell.*` / `ref:web.*` 喂多个 IDE
- Audit 让团队 review agent 改动（"agent 在哪里跑了什么 shell 命令"）
- Capability 让 agent 默认只读，需要时升权（避免误删）
- Dry-run 让"建议 vs 执行"分开

### 4.7 多 agent 编排（orchestrator + N children）

**典型**：Hermes "Manager + Specialised Children" workflow（驱动 UCAN-lite 的核心场景）

| 痛点 | ATD 答案 |
|---|---|
| Parent agent 把"读 patient X 3 个月" 委托给 child 不给 child 全部权限 | UCAN-lite delegation chain，attenuation 自动收缩 |
| Child agent 失败不让 parent 的 audit 混乱 | `caller_id` 隔离 audit |
| Child agent 共享 parent 的 OAuth token | TokenBroker 按 caller_id 路由 |
| 不同 child 走不同 vendor | Cross-vendor 合并 catalog |

---

## 5. 深度案例：celia_phr —— 最复杂的 ATD adopter

celia_phr 是 ATD 第一个**生产级、HTTP transport、多 binding、cross-repo federation** 的 adopter。它把 ATD 几乎所有扩展点都用到了 —— UCAN delegation、TokenBroker 多租户、FHIR + PHI middleware、HTTP + UDS dual transport、cursor 分页 federation、AuditSink 自定义。读懂 celia 就读懂了"ATD 在最复杂场景下究竟解决什么问题"。

### 5.1 celia 是什么

> **Celia PHR** —— 一个本地优先、零知识、专利级 Personal Health Record 应用。

- **业务核心**：Rust 工作区 ~41k LoC，8 crate（auth / FHIR / crypto / vc / crdt / sub / RBAC）
- **Agent 表面**：`celia` binary（J 阶段）—— `celia` CLI 子命令 + `celia serve --atd` ATD Unix server + `atd-mcp-bridge` 接 MCP client（Hermes / Claude Desktop / Cursor）+ `atd-server-http` 接 MCP-over-HTTP
- **三 shell 运行时**：Tauri 2.x 桌面 + Capacitor 6 移动 + PWA WebAssembly（同一份 Rust core）
- **存储**：SQLite（rusqlite native；PWA 走 sqlite-wasm-rs / IndexedDB）
- **专利**：§13.1 device-local volatile DEK + §13.4 多 agent 隔离 + §13.5 multi-binding equivalence（ATD-class 中立协议）

### 5.2 隐私不变量（专利级，每个 PR review 抓）

| 不变量 | 强制位置 | 验证 |
|---|---|---|
| **§13.1** DEK 只在易失内存；永不在盘 / 日志 / 线 | `crates/celia-core/src/auth/key_cache.rs` —— `Mutex<HashMap<UserId, Box<Zeroizing<[u8;32]>>>>` | `scripts/e2e/dek-eviction-check.sh`（gcore 双 dump） |
| **§13.1 multi-process** Pattern A —— Tauri 父与 `celia serve` 子之间 DEK 只过 Unix socket | `apps/desktop/src-tauri/src/agent_bootstrap.rs` + `crates/celia-cli/src/parent_ipc.rs` | `serve-pattern-a-test.sh` PASS |
| **§3** AES-256-GCM + SHA-256 双查每次读 | `celia-core/src/db/fhir_store.rs::decrypt_and_verify` | 171 celia-core cargo 测试 |
| **§4** 版本化 append + 软删除（无行更新） | `FhirStore::create / update / soft_delete` | 单元测试 |
| **Coding 白名单** —— 只 LOINC / SNOMED CT / RxNorm / ICD-10 / UCUM / HL7 | `celia-core/src/fhir/systems.rs`（75-entry）| 写入时 validation gate |
| **Agent gateway per-user, in-process or per-spawn child** | `celia-tools::dispatch_for_caller` 与 DEK 同地址空间 | 3-dim RBAC + per-call audit |
| **Multi-agent isolation** —— agent A 的 grant 对 B 不可见 | `consent.grantee` + `dispatch_for_caller(CallerKind::External {agent_id})` | 9 cargo 测试在 `celia-core::auth::rbac` |
| **Multi-binding equivalence** —— CLI / MCP-via-bridge / ATD-native / MCP-over-HTTP 全部保 §13.1 | `celia-tools` transport-agnostic；每 binding 都 route 通过 `dispatch_for_caller`（UDS/stdio）或 `atd_runtime::dispatch::run_tool`（HTTP via atd-server-http）| Phase J parity test 6/6 = 100% + atd-server-http UDS↔HTTP byte-identical parity test 2/2 green |

最后一条 —— **multi-binding equivalence** —— 是 celia 选 ATD 的根本原因：四条 binding 路径同一份业务代码，§13.1 不变量在每条路径都可验证。

### 5.3 三 shell 单核心架构

```
                    apps/web (React, single codebase)
                                  │
                       services/celia-runtime.ts
                       ┌──────────┴──────────┐
                       │  isInTauri()        │
                       │  isInCapacitor()    │
                       │  isInBrowser()      │
                       └──────────┬──────────┘
       ┌──────────────────────────┼──────────────────────────┐
       ▼                          ▼                          ▼
┌───────────────┐         ┌─────────────────┐          ┌──────────────┐
│ Desktop       │         │ Mobile          │          │ PWA / Web    │
│ Tauri 2.x     │         │ Capacitor 6     │          │ Browser      │
└──────┬────────┘         └────────┬────────┘          └──────┬───────┘
       │ invoke()                  │ Capacitor.invoke()       │ wasm-bindgen
       │                           │  → custom plugin         │
       │                           │  → UniFFI Swift/Kotlin   │
       ▼                           ▼                          ▼
┌──────────────┐          ┌──────────────────┐         ┌──────────────┐
│ src-tauri/   │          │ capacitor-celia- │         │ celia-core-  │
│ 8 #[tauri::  │          │ core plugin      │         │ wasm (1.4MB) │
│  command]    │          │ Swift + Kotlin   │         │ subset only  │
└──────┬───────┘          └────────┬─────────┘         └──────┬───────┘
       │                           │                          │
       └─────────┬─────────────────┴──────────────────────────┘
                 ▼
╔══════════════════════════════════════════════════════════════╗
║   crates/celia-core (Rust, 40k LoC, single source of truth) ║
║   crypto + fhir + db + auth + vc + crdt + sub + mcp         ║
╚══════════════════════════════════════════════════════════════╝
```

**关键 invariant**：三 shell 一份 Rust core；DEK 在三种宿主中都只活在 Rust `KeyCache`。Tauri 与 `celia serve` 子进程通过 Pattern A（kernel-mediated UDS）传 DEK，永不落盘 / 落网。

### 5.4 21 个工具 + 多绑定 dispatch

`crates/celia-tools` 是 transport-agnostic 的 21-tool 目录：

- 健康记录 CRUD（FHIR Patient / Observation / Condition / MedicationStatement / ...）
- VC（Verifiable Credentials）签发与验证
- CRDT 同步
- Subscription / event
- Bulk export
- Skills meta-tool（`celia:phr.skills.list` / `.skills.get`）

每个工具一份 `ToolDefinition`，配 21 份 bilingual `intent_examples`，配 19 份 embedded SKILL.md。

**四条 binding 路径同一份业务代码**：

```
LLM agent
  │
  ├── (1) Tauri 命令 in-process   →  celia-tools::dispatch_for_caller
  ├── (2) ATD UDS                  →  atd_runtime::dispatch::run_tool
  ├── (3) MCP-stdio + atd-mcp-bridge →  ATD UDS                     →  同上
  └── (4) MCP-over-HTTP + atd-server-http → atd_runtime::dispatch::run_tool

每条路径都通过相同的 RBAC + capability gate + audit sink + middleware pipeline
→ §13.1 / §13.4 / §13.5 在每条路径都自动 hold
```

**Parity 测试**：`tests/e2e_parity.rs` 验证 UDS 和 HTTP 两条 transport 对同一份工具调用产出 byte-identical 结果。

### 5.5 celia 触发的 4 个 ATD SP

celia 是 ATD post-1.0 期最重要的 adopter trigger —— 4 个 SP 直接因 celia 需求 ship：

#### 5.5.1 SP-streamable-http（1.B —— HTTP transport）

**Trigger**：celia 是云端可托管的 PHR，需要 HTTP 而非 Unix socket（Unix socket 跨 host 走不通）

**Ship**：`atd-server-http` crate —— axum + hyper + MCP JSON-RPC translator + bearer auth + origin gate + SSE refresh helper。**复用同一个 `atd_runtime::dispatch::dispatch_request`** —— UDS 和 HTTP listener 共享 dispatch 逻辑。

#### 5.5.2 SP-token-broker-phase2

**Trigger**：HTTP 走 bearer token，celia 需要 token broker 处理多种 token 形式

**Ship**：`TokenBroker::resolve_bearer` 臂 + 类型化 `BearerOutcome`（11 变体）+ `BearerIdentity.granted_capabilities` + 60s SSE 心跳 re-resolution

#### 5.5.3 SP-capability-v2（UCAN-lite）

**Trigger**：celia 的 Hermes "orchestrator + N specialised children" workflow 需要 sub-agent delegation —— Agent A 把"读 patient X 3 个月" 委托给 child B 不给 B 全部权限。celia 的 flat RBAC 强迫用户重新 pair 每个 child。

**Ship**：

- `Hello.ucan_tokens: Vec<String>` additive 字段（pre-SP server 透明降级）
- UCAN-lite v1.0 profile —— JWT-shape compact、Ed25519、did:key 唯一
- `cmd = "atd-cap"` 把 ATD 字符串 capability tunnel 进 UCAN payload
- `args.with = [{patient: "Patient/abc"}]` resource-binding 列表（与 celia `consent.patient_filter` 1:1）
- `UcanRevocationStore` trait —— 与 SP-token-broker-phase2 §4.8 三层 revocation 组合
- `max_ucan_chain_depth: u8`（默认 5，可配）防 verifier DoS
- 27 unit + 12 integration test green

**关键设计**：UCAN-lite **additive**，不取代 SP-12 字符串 allow-list。当 client 发 `ucan_tokens`，granted set = `granted_strings ∪ granted_ucan`（联合，不交集 —— UCAN 已经 attenuated；交集是双重惩罚）。

**当前状态（2026-05-28）**：**shipped-dormant**。代码端到端 ship，27+12 测试绿；生产流量仍走 `ce_<hex>` bearer 因为没有 adopter 真在 mint chain。Keystone scenario 锁定："分享我近 3 个月心率给王医生，7 天后失效"。等产品化触发即激活。

#### 5.5.4 SP-medical-middleware

**Trigger**：celia 已经实现的 FHIR validation + HIPAA PHI redact 在错误层 —— 应该是 ATD 中间件，让其他医疗 vendor 复用

**Ship**：两个独立 crate

| Crate | 内容 |
|---|---|
| `atd-middleware-fhir` | 75-URI coding system 白名单（与 celia source-of-truth set-equal，drift-guard 双 CI gate）+ 12-resource Celia-subset required-field 校 + 3 个 `MismatchPolicy` |
| `atd-middleware-pii-redact-medical` | 18 HIPAA Safe Harbor identifier × 13 JSON-Pointer × 7 `RedactionStrategy` + 5 catch-all 正则 + `fhir_aware` opt-in |

**关键设计**：`CallEvent` schema **不变** —— PHI 永不出现在 audit。Middleware 只 mutate flow back to wire 的 result；audit sink 看到 `tool_id / outcome / duration` 等元数据，永远看不到 result body。"audit log should see redacted event" 是 v1 下的 non-problem。

#### 5.5.5 SP-concurrency-baseline（perf-v1 axis 1）

**Trigger**：celia 10-concurrent benchmark 60% session-init failure（`Connection lost`）

**Ship**（5 轴干预，全 back-compat）：

| 轴 | 改 |
|---|---|
| Server runtime | `current_thread` → `multi_thread` tokio；`ATD_WORKER_THREADS` env |
| Wire deadline | `WireError::Timeout` + `read_frame_with_deadline` / `write_frame_with_deadline`；5s 握手 / 30s 活跃 |
| SDK retry | `AtdClient::connect` 指数退避（5× / 50→800ms / ±20% jitter）；fatal 短路 |
| Audit sink | `JsonLinesAuditSink` 重写为 bounded `tokio::sync::mpsc` + 专用 drain；`on_call` non-blocking |
| Metrics | `MetricsCounters` + `Server::metrics_snapshot()`：accepted_connections / dispatched_requests / dispatch_errors_by_code / audit_events_total / audit_drops_total |

**结果**：

- ATD ref-server `concurrent_handshake_storm` n=50 wall=127ms p50=116ms p99=125ms 错=0 audit_drops=0
- celia iter-4 SHARP baseline 120Q 0 rate-limit 0 connection 失败（vs iter-3 6/10）
- celia issue **closed-verified**

### 5.6 Phase L federation —— healthkit_cli 作为远程 ATD endpoint

celia 把 healthkit_cli 当远程 ATD endpoint 接进自己，做"跨 vendor 健康联邦"。

```
celia-connectors (Phase L.1 + L.2 + L.4)
  ├── AtdUpstreamIngest    — atd-sdk + cursor 分页 + Provenance (meta.source = atd://<ep>/<tool>)
  ├── CursorStore trait    — InMemory + FhirBasicCursorStore (CRDT max-by-advanced_at + Lamport)
  └── SyncOrchestrator     — tick + ±20% jitter + 指数退避 1m→5m→30m→2h
                            + 5-failure → Degraded
                            + per-task tokio::spawn 失败隔离
                            + audit events via atd-runtime::AuditSink
```

**关键问题**：远程 ATD server 重启时本地持久 cursor 失效。SP-pagination-v1 设计的 stateless HMAC cursor 让 server 重启返回 `1020 ERR_CURSOR_EXPIRED` —— `CursorStore::invalidate` 标 tombstone，下次 sync 从头拉。

**Vendor-中性**：`celia-connectors` 不知道 healthkit，任何 ATD-speaking server 都能被接入。Huawei-specific adapter 在 `healthkit_cli` repo 自己 host。

### 5.7 celia 端到端数据流（一个 LLM 调用经历的全程）

以"给我看 patient X 最近 3 个月血压" 为例：

```
1. 用户在 Tauri 桌面 app 输入对话
   ↓
2. apps/web/services/agent-api.ts
   detect runtimeKind() === 'tauri'
   → invoke('celia_chat_stream', {user_id, message, history, active_patient_id, on_event})
   ↓
3. apps/desktop/src-tauri/src/commands.rs
   spawn Hermes orchestrator
   ↓
4. Hermes orchestrator (LLM) sees 21 ATD tools + 19 SKILL.md
   pick "celia:phr.observation.search"
   args = {patient_id: "Patient/X", code: "85354-9 (BP)", date_range: "2026-02..2026-05"}
   ↓
5. Hermes 发 RunTool over ATD UDS (Pattern A child socket)
   ↓
6. atd_runtime::dispatch::dispatch_request
   ├── Hello capability gate: granted = ["records:read", "patient:X"] (UCAN-lite chain attenuated)
   ├── required = ["records:read"]  ✓
   ├── tier = Warm, deadline = 5s
   ├── TokenBroker::resolve(caller_id="hermes-orch") → SecretBundle{user_id, dek_ref}
   └── NativeBinding::invoke → celia_tools::dispatch_for_caller
        ↓
7. celia-core RBAC check
   consent.grantee = "agent:hermes-orch"
   consent.patient_filter = "Patient/X"
   consent.scope ⊇ ["records:read"]
   ✓ allow
   ↓
8. celia-core fhir_store
   SELECT ... WHERE user_id=? AND resource_type='Observation'
                   AND patient_id='Patient/X' AND date_bucket BETWEEN ...
   for each row:
     decrypt_and_verify(encrypted_data, dek, hash)
     → JSON FHIR Observation
   累成 array
   ↓
9. Middleware pipeline (post-dispatch, on Value)
   ├── FhirMiddleware: 每个 Observation
   │     - resourceType ∈ {Patient, Observation, ...} ✓
   │     - coding[].system ∈ ALLOWED_SYSTEMS_DEFAULT(75) ✓
   │     - required fields present ✓
   ├── PiiRedactMiddleware:
   │     - /name → Token("NAME")
   │     - /telecom → Token("PHONE")
   │     - /address postalCode → ZipPrefix3
   │     - /birthDate → YearOnly
   └── (RedactPathsMiddleware: $HOME 路径 mask — 本例无)
   ↓
10. AuditSink::on_call
    CallEvent {
      ts, call_id, tool_id: "celia:phr.observation.search",
      caller_id: "hermes-orch",
      granted_capabilities: ["records:read", "patient:X"],
      duration_ms: 47,
      outcome: Success,
      tier: "warm", dry_run: false,
      schema_version: 2,
      secrets_resolved: true,
      cursor_page: None
    }
    → JSONL via bounded mpsc → 不阻 dispatch
    ↓
11. ToolResultResponse 序列化回 wire
    ↓
12. Hermes 收到 redacted FHIR JSON
    生成自然语言总结返回 Tauri front-end
    ↓
13. 用户看到 "Patient X 过去 3 个月血压均值 128/82，3 次高于 140/90，建议..."
```

**全程 5 个 ATD 协议特性参与**：
- `Hello.ucan_tokens` capability chain（5.5.3）
- TokenBroker 按 caller_id 取 secret（3.4.5）
- Tier-aware deadline（3.4.3）
- Middleware pipeline FHIR + PHI（5.5.4）
- AuditSink mpsc non-blocking（5.5.5）

### 5.8 celia 为什么必须用 ATD —— 反事实分析

如果不用 ATD，celia 要自己实现：

| 自实现成本 | ATD 已 ship |
|---|---|
| MCP server / HTTP server / Unix server 三套 transport 逻辑 | `atd-server` + `atd-server-http` + `atd-mcp-bridge` 共用 dispatch |
| Capability gate（且要保跨 transport 一致） | `Hello.granted_capabilities` + dispatch 层 gate |
| Audit log schema + 写盘（且非阻塞）+ rotate | `JsonLinesAuditSink` mpsc bounded |
| Multi-tenant token routing | `TokenBroker::resolve` + `caller_id` 路由 |
| OAuth bearer + SSE 心跳 re-validation | `resolve_bearer` + `sse_refresh` |
| UCAN-lite delegation（27 测 + 12 e2e） | `Hello.ucan_tokens` + `atd_runtime::ucan::*` |
| FHIR R4 validation + 75-URI 白名单 | `atd-middleware-fhir` |
| HIPAA PHI redaction（18 类 × 13 路径） | `atd-middleware-pii-redact-medical` |
| Cursor 分页 + HMAC 签名 + cross-tool 重放防御 | `CursorIssuer` + `Tool::supports_pagination` |
| 多 connection 并发 + p99 < 200ms SLO | `multi_thread` tokio + frame deadline + SDK retry |
| 跨 transport byte-parity 测试 | `atd-conformance` |

保守估计：**~8000-15000 行 Rust** —— 而且每条 SP 的设计要从头来一遍。celia 通过 `path =` 依赖 ATD（post-1.0 切换到 crates.io `= "1"`）拿到这些 batteries-included。

### 5.9 celia 没有的（ATD 故意 non-goal）

| celia 自己实现 | 为何不在 ATD |
|---|---|
| §13.1 DEK 加密 / KeyCache | adopter-specific 加密策略；ATD operate on already-decrypted Value |
| Pattern A IPC（Tauri 父子 socket bootstrap） | 桌面 app shape；ATD layer 不管 process model |
| FHIR Bundle / CSV / JSON ingestion | Ingestion 是产品 surface，不是协议 |
| CRDT 同步 + Lamport tiebreaker | celia 业务模型 |
| WebCrypto PWA subset | 浏览器特定 |
| GDPR Article 17 erasure（hard delete） | celia 业务模型 + 合规 |
| Tauri commands + UniFFI Swift/Kotlin | 桥接产品壳，非协议 |

ATD 给的是**协议 + runtime + 中间件原语**；celia 用这些原语搭出符合自己专利、合规、跨 shell 需求的产品。

### 5.10 一句话总结 celia 案例

> **celia_phr 证明：一份协议级中立调度面（ATD）+ 一组可装配的扩展点（Binding / Middleware / TokenBroker / AuditSink / UCAN verifier），能让一个本地优先、零知识、专利级的 PHR 应用在 3 个 shell、4 条 binding 路径、跨 vendor federation、多 agent delegation 场景下保持 single source of truth 的业务代码 + 可逐路径验证的隐私不变量。**
>
> raw MCP 没有 capability / multi-tenant / audit；raw HTTP server 没有 cross-binding parity；自研 adapter 每写一次都要重新设计 6-8 个机制。ATD 把这些一次性 ship 完。

---

## 6. 关键引用

- 三份宪法文档：[positioning](../atd-positioning.md) · [design philosophy](../atd-design-philosophy.md) · [architecture](../atd-architecture.md)
- Wire 协议：[`docs/protocol/wire-format.md`](../protocol/wire-format.md)
- 错误码：[`docs/protocol/error-codes.md`](../protocol/error-codes.md)
- 集成路径总览：[`docs/integrations/overview.md`](../integrations/overview.md)
- Per-platform 集成指南：[hermes](../integrations/hermes.md) · [claude-code](../integrations/claude-code.md) · [langchain](../integrations/langchain.md) · [openclaw](../integrations/openclaw.md)
- Adopter case：[healthkit](../integrations/healthkit.md)
- 跨 vendor 组合：[`docs/integrations/cross-vendor-pattern.md`](../integrations/cross-vendor-pattern.md)
- 实证 transcript：
  - [healthkit_cli/docs/case-study-v1.2.0/](https://github.com/downsea/healthkit_cli/tree/main/docs/case-study-v1.2.0)（4 prompt × log，95.2% 验证）
  - [healthkit_cli/docs/case-study-v1.4.0/](https://github.com/downsea/healthkit_cli/tree/main/docs/case-study-v1.4.0)（医生视角心率分析，ATD vs CLI 头对头）
- SP 归档：[`docs/archive/superpowers/`](../archive/superpowers/)
- ADR：[`docs/adr/`](../adr/)
- Issues：[`docs/issues/`](../issues/)

---

## 7. 一句话回顾

> **ATD = 一份冻结的 5-message 中立协议 + 一套可装配的 server runtime（capability / audit / rate limit / TokenBroker / UCAN-lite / Cursor / middleware）+ 一组桥接（MCP-bridge / SDK / CLI），让 vendor 写一份 server 就能被任意 agent 平台用，并自带 audit / 多租户 / 跨 vendor 组合 / 子委托 —— raw CLI 拉不出、raw MCP 没规范、自研 adapter 每次重写的东西，全在这里 ship 了。**
>
> celia_phr 是当前最复杂的 adopter —— 3 shell × 4 binding × cross-vendor federation × multi-agent delegation —— 它的存在证明这层抽象在生产规模下站得住。
