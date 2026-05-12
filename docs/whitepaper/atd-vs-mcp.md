# ATD vs MCP — 定位与差异

> **核心论点：** raw MCP 能传 tool call，但 ATD 在协议层 ship 了 vendor 实现 server 时**都需要重新发明的那一堆运行时治理能力**（capability gate / multi-tenant token / audit / rate limit / tier / visibility）。两者不冲突——ATD 通过 `atd-mcp-bridge` 兼容现有 MCP 客户端 (Hermes / Claude Code / Cursor) 不改一行就能用。

**Date:** 2026-04-30
**Companion case study:** `~/proj/healthkit_cli/docs/case-study-v1.4.0/case-study.md` (v1.4.0 doctor-perspective HR analysis — ATD vs CLI head-to-head)
**Related:** [`atd-introduction.pptx`](atd-introduction.pptx) Slide 13 (vs raw alternatives 表格)

---

## 1. 简短答案

healthkit ATD case 里，**LLM 看到的"调 27 个工具"语义** raw MCP 也能做——`atd-mcp-bridge` 把 MCP `tools/call` 翻译成 ATD `RunTool`，对 LLM 而言两者无差别。

但 case 的**所有 server 侧保证**都是 ATD 独有：MCP 协议层根本没这些 surface。raw MCP 复现 healthkit serve 的全套行为有两类难度：

- **vendor 自己能补但成本高** — 每家 vendor 都要从零写一份 (capability gate / audit / rate limit / tier / safety / visibility)
- **MCP 架构层做不到** — stdio 1:1 框死了多租户、热轮换、单 socket 多 vendor 聚合

---

## 2. MCP 协议层缺失，vendor 能自补但成本高（"重新发明轮子"）

### 2.1 Capability gate

| | ATD | raw MCP |
|---|---|---|
| 握手 | `Hello { requested_capabilities }` → `HelloAck { granted_capabilities }` | 无握手；client 连进来直接 `tools/call` |
| 单调用 gate | `tool.required_capabilities ⊆ caps.granted()` 不通过 → `ERR_CAPABILITY_DENIED (1001, retryable: false)` | 每个 vendor 在 MCP server 里**自己写 wrapper**：每个 tool handler 进去先查 env / 配置 / hardcoded list |
| 跨 vendor 一致性 | 协议保证 | 每个 vendor 自己定义 capability 字符串语义 |

### 2.2 Standardized audit log（结构化 JSONL）

| | ATD | raw MCP |
|---|---|---|
| 自动 emit | `atd-runtime::JsonLinesAuditSink` 自动写 `{ts, call_id, tool_id, caller_id, granted_capabilities, duration_ms, outcome, tier, schema_version, secrets_resolved}` | 无 audit 概念；vendor 想要审计就自己 `eprintln!("[audit] ...")`，格式各自定 |
| 跨 vendor 聚合 | 同一 schema，可统一 `tail -f \| jq` | 不可聚合——agent 想"统一看 N 个 MCP server 的调用历史"做不到 |

### 2.3 Rate limit

- **ATD**：`tool.resources.max_concurrent` + `try_acquire_owned()` 自动 → `ERR_RATE_LIMITED (1002, retryable: true)` 快速失败不排队
- **MCP**：vendor 自己用 `tokio::sync::Semaphore` 包装每个 handler

### 2.4 Tier-based budgets（timeout + max_output_bytes）

- **ATD**：`tool.tier: Hot|Warm|Cold` 声明式 → runtime 派生 deadline + max_output 注入 `CallContext`，vendor 调 `ctx.remaining_time()` 包 `tokio::time::timeout`

  | Tier | timeout | max_output_bytes |
  |---|---|---|
  | Hot | 500 ms | 64 KiB |
  | Warm | 5 s | 1 MiB |
  | Cold | 60 s | 16 MiB |

- **MCP**：每个 vendor 在每个 handler 里 hardcode `Duration::from_secs(...)`；`max_output` 概念不存在

### 2.5 Tool visibility levels

- **ATD**：`ToolVisibility::{Hidden, Read, Write, Sensitive, Destructive}` —— 5 档；`Hidden` tool `discover()` 不出现但能 `RunTool` 直接调（用于 helper / meta-tool）
- **MCP**：tool 在 `tools/list` 里要么有要么没有——二元；无"暴露 schema 但不出现在催化目录"的能力

### 2.6 Safety classification

- **ATD**：`tool.safety.level: Read|Write|Financial|Privacy|Physical|Destructive` + `safety.dry_run: bool`
- **MCP**：tool 描述纯自然语言，agent / 用户没有结构化方式判断"这个 tool 会不会删数据"

### 2.7 Wire-level error semantics（1001/1002/1003 + retryable hint）

| Code | 含义 | retryable |
|---|---|---|
| `ERR_CAPABILITY_DENIED` (1001) | 无对应能力 | false |
| `ERR_RATE_LIMITED` (1002) | 并发上限 | true |
| `ERR_BROKER_FAILED` (1003) | TokenBroker 取 secret 失败 | true |

LLM/client 能 mechanical 决策"能不能重试"。MCP JSON-RPC error code 全是通用错（`-32xxx`），无领域语义、无 retryable hint。

### 2.8 Schema-first ToolDefinition（intent_examples / errors / required_capabilities / tier / safety）

- **ATD `ToolDefinition`** 13 个字段：`id` / `name` / `description` / `version` / `capability` / `input_schema` / `output_schema` / `bindings` / `safety` / `resources` / `trust` / `visibility` / `required_capabilities` / `tier` / `errors`
- **MCP `tools/list`** 返回 `{name, description, inputSchema}` —— 三个字段；其他全靠 vendor 塞进 description 里用自然语言说，agent 解释靠运气

### 2.9 Skills meta-tool convention + 一键 sync

- **ATD**：`<x>.skills.list/get` 公约 + `atd skills sync --target hermes/claude-code/stdout` —— vendor 把 SKILL.md 通过同样的工具调用机制发布；agent 端一行命令拉到本地 skills 目录
- **MCP**：无对应公约。SKILL.md 的分发是带外的（tarball / 官网 / 文档站）

### 2.10 开发者工具链

- **ATD**：`atd list/schema/call/doctor/skills sync` —— 命令行直接戳 server，不需要起 LLM
- **MCP**：无标准 CLI；要写测试客户端就自己实现 MCP JSON-RPC 客户端

---

## 3. MCP 架构层做不到的（不是"难做"，是"做不到"）

### 3.1 一个 server 进程 + 多 caller 多 OAuth 的多租户

**MCP 是 stdio 1:1**：每个 MCP server 是 Hermes 启动的子进程，stdin/stdout 一对一。要让 agent A 和 agent B 用不同的 OAuth token 访问同一个 healthkit 服务，**必须起两个 MCP server 进程**：

```
hermes mcp add healthkit-A --command healthkit-mcp --env TOKEN=tokA
hermes mcp add healthkit-B --command healthkit-mcp --env TOKEN=tokB
```

两份内存、两份 schema、两份 connection 状态。

**ATD 是 Unix socket 1:N**：单个 `healthkit serve` 监听 `/tmp/hk.sock`，N 个 client 各自 connect → 各自发 `Hello { client_id: "agent-A" }` —— **同一进程**，per-connection 缓存 `caller_id`。每次 RunTool 调 `TokenBroker.resolve("agent-A")` 查 `/tmp/hk-tokens/agent-A.json`，回 `SecretBundle { hms_oauth_token }` 给 tool 用。

> 实测见 `docs/case-study-v1.4.0/atd-audit.jsonl`（同 `atd-mcp-bridge` 名字下 N 个 connection 的 `caller_id` 区分）

### 3.2 OAuth token 热轮换 / 拒绝刚轮换

- **MCP**：token 在 server spawn 时通过 env 注入。token 过期想换：**杀进程重启**。期间 LLM 调用全部失败。
- **ATD**：`TokenBroker.resolve()` 是 **per-call**——每次 RunTool 进 dispatch → broker 现读文件 / 现刷 OAuth → 拿到当下有效 token。token 文件原地更新就生效，server 不重启。

### 3.3 跨 vendor 在单一 socket 下的统一目录 + 统一 audit

- **MCP**：N 个 vendor = N 个 MCP server 进程 = N 份 `tools/list`（agent 看到 N 个 namespace）= N 个独立 audit / 各自不同
- **ATD**：可以做"一个 atd-server 监听一个 socket，注册多 vendor 的 Tool 实现"——`Registry::register(Arc<dyn Tool>)` 注册 healthkit + weather-mock 在**同一进程**。`discover()` 返回合并 catalog（`huawei:hms.healthkit.* + mock:weather.*`），audit log 是**一份**。或者多个 vendor 各起 server + 一个 atd-router 聚合。MCP 这两条路都没法走（stdio 1:1 框死）。

### 3.4 Connection 级共享状态（per-conn `read_tracker`）

ATD `connection.rs:38-42` 的 dispatch 签名：

```rust
fn dispatch_request(
    state: &ConnectionState,
    tracker: &Arc<atd_runtime::ReadTracker>,   // ← 跨 RunTool 共享
    caps: &mut Arc<atd_runtime::CapabilitySet>,
    caller_id: &mut Option<String>,
    req: Request,
) -> Response
```

同一连接上多次 `RunTool` 共享一个 `read_tracker`，比如"agent 这次 session 已经读了哪些文件、读了多少字节"。fs 工具用它做"防止 LLM 在一次会话里把整个磁盘 cat 出来"。

MCP stdio 是流，没有"connection 概念"，也没有 server 端的 per-session 跨工具共享状态。每次 `tools/call` 是孤立的；vendor 想跨工具共享就自己搞全局状态——但全局对**所有 client 共享**，不是 per-session。

### 3.5 HelloAck 把 server 真实能力告诉 client

- **ATD**：client `Hello → granted_capabilities, server_version, supported_tiers` —— client 提前知道"我拿到了什么权限、对方是哪个版本、支持哪些 tier"。可以 fail-fast 拒绝过老 server。
- **MCP**：`initialize` 方法返回 `serverInfo {name, version}` 和 `capabilities`，但 `capabilities` 是一组**协议特性 flag**（`tools`、`resources`、`prompts`、`logging`），**不是 vendor-domain 的能力**——没办法表达"我这个 server 提供 healthkit:read 但不提供 healthkit:write"这种业务能力维度。

---

## 4. healthkit v1.4.0 case 里实际依赖的 ATD 独占特性

把 v1.4.0 case study 跑通**必须用到的**，对应 raw MCP 做不到 / 必须自己重写的：

| 用到的 ATD 特性 | raw MCP 怎么办 |
|---|---|
| `--grant-capability healthkit:read --grant-capability healthkit:write` 启动时声明 + Hello 协商 | 写在 vendor 自己的 env / config，无统一格式 |
| audit log `/tmp/hk-audit.jsonl` 可被 `tail -f \| jq` 直接消费 | vendor 自定义日志，无标准 |
| `caller_id` 在 audit 里区分 `atd-mcp-bridge` 默认与 agent-A/B 多租户 | MCP 1:1 stdio 没有 caller 概念，要做就开多进程 |
| `--token-broker-dir /tmp/hk-tokens/` per-caller 文件路由 | 多进程 + 各自 env，无单进程方案 |
| `huawei:hms.healthkit.skills.list/get` 暴露 26 个 SKILL.md，`atd skills sync` 一键拉到 `~/.hermes/skills/` | SKILL.md 用户手动拷或自定义脚本 |
| `ToolVisibility::Hidden` —— skills meta-tool 在 discover 里不冒头但 client 能直接调 | 没法做（要么 list 里有要么没有） |
| 27 tool defs 含 `safety.level / required_capabilities / tier / errors` | tools/list 三个字段，其他自然语言塞 description |

---

## 5. 三方比较表（与 PPT Slide 13 一致）

| 维度 | Raw CLI | Raw MCP | ATD |
|---|---|---|---|
| Capability gate | 无 | client 自己 | server 强制 ✓ |
| Rate limit | 无 | 无 | per-tool semaphore ✓ |
| Audit log | shell history | 无规范 | JSON Lines ✓ |
| Multi-tenant token | N 进程 / N token | stdio 单租户 | TokenBroker ✓ |
| Tool visibility | 无 | 二元 hidden | 5 档 (含 Hidden) ✓ |
| Safety levels | 无 | 无 | Read..Destructive ✓ |
| 跨 vendor 组合 | 自己写 mux | 需自己 mux | 桥接多 socket ✓ |
| LLM matching | --help 文本 | tool desc only | desc + intent_examples ✓ |
| Case study v1.4 实证 | 8 calls / 3 错试 | — | 2 calls / 0 错试 ★ |

★ 通过 `atd-mcp-bridge` 兼容现有 MCP 客户端 — Hermes / Claude Code / Cursor 不改一行代码。

---

## 6. 架构定位的一句话

**MCP 是"消息总线"——它把 tool call 从 LLM 送到 server。ATD 是"协议+运行时"——它把 LLM-tool 通信背后 vendor 都需要的那一坨 server 侧治理能力（capability / rate / audit / tier / multi-tenant token）做成协议默认，让 vendor 实现 server 时不用每家从零写一遍。**

类比：

- MCP 之于 ATD ≈ HTTP 之于 OAuth + RBAC + rate limit + APM
- 两者**不冲突**——ATD 通过 `atd-mcp-bridge` 桥接进现有 MCP 生态，让 Hermes / Claude / Cursor 这些"只会说 MCP"的 client **直接受益于 ATD 的 runtime 保证**而无需重写 client

---

## 7. 交叉引用

- [`atd-introduction.pptx`](atd-introduction.pptx) — Slide 11 部署架构 / Slide 12 完整交互时序 / Slide 13 三方比较表
- [`atd-v3-multi-device.md`](atd-v3-multi-device.md) — v3 whitepaper（multi-device 视角下 ATD 协议形式化）
- [`../architecture.md`](../architecture.md) — ATD 参考实现的架构文档
- [`../protocol/wire-format.md`](../protocol/wire-format.md) — wire-level 协议规范
- [`~/proj/healthkit_cli/docs/case-study-v1.4.0/case-study.md`](../../../healthkit_cli/docs/case-study-v1.4.0/case-study.md) — v1.4.0 doctor-perspective HR 分析（实证 baseline）
