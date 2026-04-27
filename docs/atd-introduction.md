# ATD 协议介绍 — 功能与优势

> Agent Tool Dispatch (ATD) — 跨 vendor 中立的 agent ↔ 工具调度协议。
>
> 本文从一个真实跑过的 LLM session（[v1.4.0 doctor-perspective HR analysis](https://github.com/downsea/healthkit_cli/blob/main/docs/case-study-v1.4.0/case-study.md)）切入，自下而上讲清楚 ATD 是什么、ships 了什么、解决了哪些 raw CLI / raw MCP / 自研 vendor adapter 解决不了的问题。
>
> 深度架构参考见 [`architecture.md`](architecture.md)；本文是入门 + 立场总览。

---

## 1. 一句话定位

> **ATD 是 agent 调用工具时的一层中立调度协议**。Vendor 把工具 host 成一个 ATD server（Unix socket），任意 agent 平台（Hermes / Claude Code / Cursor / 自研）通过同样的 wire 格式 discover / describe / call / dry-run。中间层提供 capability gate、audit log、多租户 token 路由、tool 可见性控制、skill 同步 — 这些都是 raw CLI 拉不出来、raw MCP 没有规范、per-vendor 自研每个都要写一遍的东西。

---

## 2. 经验证据（不是空谈）

### 2.1 Healthkit_cli 三轮 case study

| 版本 | 实验 | 工具 surface | LLM 表现 |
|---|---|---|---|
| **v1.1.0** | Hermes + DeepSeek + 4 prompt | 8 个 raw HMS REST endpoint（permissive `{type:object}` schema） | **24% 成功率，79 次调用，66% `Invalid param`** |
| **v1.2.0** | 同上 4 prompt | 26 个 helper-tool（auto-derived 自 CLI helpers + SKILL.md） | **95.2% 成功率，21 次调用（-73%），1 次失败（HMS-side HRV quirk）** |
| **v1.4.0** | 单 prompt：「从医生角度分析最近两个月心率」 | 27 工具（25 helper + 2 skill meta），多租户 mode | **2 ATD 调用（1.6s，零错试）vs 8 CLI fallback（含 3 次走错路径）** |

完整 transcript / audit.jsonl / agent reply 在：
- [`healthkit_cli/docs/case-study-v1.2.0/`](https://github.com/downsea/healthkit_cli/tree/main/docs/case-study-v1.2.0)（4 prompt × log）
- [`healthkit_cli/docs/case-study-v1.4.0/`](https://github.com/downsea/healthkit_cli/tree/main/docs/case-study-v1.4.0)（最新一轮，本介绍主参考）

### 2.2 v1.4.0 这次跑出的关键数据点

同一个 Hermes session、同一个 DeepSeek-chat 模型、同一个 prompt、ATD bridge + CLI fallback 都摆在 agent 面前：

| 维度 | ATD path | CLI fallback path |
|---|---|---|
| 调用次数 | **2** | 8 |
| 总耗时 | **~1.6s** | ~6s |
| 走错路径次数 | **0** | 3（错 wrapper、`--offset` 不存在 ×2） |
| 第一次拿到数据 | call #1（1.2s） | call #6（5s） |
| Audit 可观测性 | **2 entries 完整** | shell log only |
| Agent 需自己知道 wrapper 命令 | 否 | 是（`healthkit healthkit +x` 双关键字） |
| Agent 需自己知道 HMS 30 天上限 | 否 | 是（撞错才知道） |

ATD 路径**严格优于** CLI fallback，并且 audit log 自带可观测性 — 这两点 raw CLI 做不到、raw MCP 没规范保证。

---

## 3. ATD 协议的核心抽象

### 3.1 Wire 层（5 个消息）

```text
Hello { client_id, requested_capabilities }    →  HelloAck { granted, server_version, supported_tiers }
Ping                                           →  Pong
ToolList                                       →  ToolListResponse { tools: [ToolSummary] }
ToolSchema { tool_id }                         →  ToolSchemaResponse { schema: ToolDefinition }
RunTool { tool_id, args, dry_run }             →  ToolResultResponse { result, success } | Error
```

Unix socket、length-prefixed JSON 帧、零 schema 协商 — 简洁到 pre-SP 的 client 都能反序列化。详细 wire 格式见 [`docs/protocol/wire-format.md`](protocol/wire-format.md)。

### 3.2 Tool 层（每个工具的 declarative metadata）

每个 ATD tool 有完整 `ToolDefinition`：

| 字段 | 用途 |
|---|---|
| `id` | `<publisher>:<service>.<x>.<y>` 命名空间 |
| `description` | LLM 看到的自然语言描述 |
| `capability.intent_examples` | 3 个自然语言短语，帮 LLM 匹配用户意图 |
| `input_schema` / `output_schema` | JSON Schema |
| `safety.level` / `safety.dry_run` | Read / Write / Financial / Privacy / Physical / Destructive；是否支持 dry-run |
| `visibility` | Read / Write / Dangerous / System / **Hidden**（v0.3.0） |
| `required_capabilities` | server-side 强制门禁 |
| `tier` | Hot / Warm / Cold（决定 deadline + max_output 预算） |
| `resources.max_concurrent` | per-tool semaphore 限并发 |
| `bindings` | Cli / Mcp / AppFunction / Rest |
| `trust` | publisher + L0-L4 信任等级 |

LLM-driven agent 平台（Hermes、Claude）拿到 `discover()` 的 `ToolSummary` 列表后，按 `description` + `intent_examples` 自动匹配，按 `safety.level` 决定是否需要用户二次确认。

### 3.3 Per-call 上下文

服务器在每个 `RunTool` dispatch 时给 `Tool::call(args, ctx)` 注入 `CallContext`：

```rust
pub struct CallContext {
    cwd, max_output_bytes, call_id,
    deadline,                         // 由 tier 推导
    read_tracker,                     // 跨 connection 共享
    capabilities: Arc<CapabilitySet>, // Hello 协商出的 granted 子集
    tier: ToolTier,
    caller_id: Option<String>,        // 来自 Hello.client_id
    secrets: Option<Arc<SecretBundle>>,  // 来自 TokenBroker（v0.3.0）
}
```

工具实现读 `ctx` 决定 deadline、读 `secrets()` 取 OAuth token、读 `capabilities` 做细粒度 gate。所有这些都不用工具自己实现 — 框架统一发。

---

## 4. ATD 提供的能力（自下而上）

### 4.1 Capability gate（SP-12）

`Hello` 时 client 声明想要的 capabilities，server 用 allow-list（`--grant-capability`）求交集。每个 tool 在 `required_capabilities` 里声明自己需要哪些；dispatch 在 `Tool::call` 之前做 subset 检查。不满足 → `Response::Error { code: 1001 / ERR_CAPABILITY_DENIED }`，工具根本不被调用。

**raw MCP 没这个东西**：MCP 没有 server 侧 capability 概念，gate 由各 client 自己实现，不一致也没规范。

### 4.2 Per-tool rate limit（SP-operability-v1）

每个 tool 在 `resources.max_concurrent` 声明并发上限；server 用 `tokio::sync::Semaphore` 在 dispatch 层 fail-fast 拒绝（`try_acquire_owned`）。saturated → `Response::Error { code: 1002 / ERR_RATE_LIMITED, retryable: true }`，工具不被调用。

### 4.3 Audit log（SP-operability-v1 C1）

每次 `RunTool` 落一条 JSON Lines 记录：

```json
{"ts":"2026-04-27T15:42:30+0000","call_id":"01J...","tool_id":"huawei:hms.healthkit.heartrate",
 "caller_id":"agent-A","granted_capabilities":["healthkit:read"],"duration_ms":1169,
 "outcome":{"kind":"success"},"tier":"warm","dry_run":false,
 "schema_version":1,"secrets_resolved":true}
```

cover：成功、参数错、执行失败、cap_denied、rate_limited、tool_not_found。case study 里的 audit log 直接拿来当性能 benchmark 用：

```bash
jq -c '{caller_id, tool_id, duration_ms, outcome: .outcome.kind, secrets_resolved}' /tmp/hk-audit.jsonl
```

**raw CLI 做不到这个**：你只能开 shell history、grep stdout — 没有结构化、没有跨 caller 聚合。

### 4.4 Multi-tenant TokenBroker（SP-token-broker-phase1/2）

`ServerConfig::token_broker: Option<Arc<dyn TokenBroker>>` 是 v0.3.0 引入的扩展点：

```rust
pub trait TokenBroker: Send + Sync {
    fn resolve<'a>(&'a self, caller_id: Option<&'a str>) -> ResolveFuture<'a>;
    // 返回 Ok(Some(SecretBundle)) | Ok(None) | Err(BrokerError)
}
```

`SecretBundle = HashMap<String, RedactedString>` — `RedactedString` 在 `Debug`/`Display` 下自动渲染成 `<redacted>`，audit log 只记 `secrets_resolved: bool`，**不会**泄漏 key 名或值。

dispatch 在 capability + rate-limit 通过后、`Tool::call` 之前调 `broker.resolve(caller_id).await` → 把 bundle 挂到 `ctx.secrets`。工具读 `ctx.secrets().get("oauth_token")` 即可，工具自己**不用知道**多租户的存在。

healthkit_cli v1.4.0 ships `FileBackedTokenBroker`：每个 caller 一个 `<dir>/<caller_id>.json`，schema 跟单租户的 `~/.config/healthkit/token.json` 一样。运维拷文件就完事。

**raw CLI 做不到**：每个 CLI 进程一个 token 来源（env 或文件），N 个 user 要 N 个进程 + N 个 token 文件 + N 套 OAuth 刷新逻辑。

**实测证据**：v1.4.0 dispatch test 用 audit.jsonl 实证：

| caller_id | secrets_resolved |
|---|---|
| agent-A | `true` |
| agent-B | `true` |
| ghost | `false`（未注册，落到 env fallback） |

### 4.5 Tool visibility（SP-tool-visibility-hidden）

`ToolVisibility::Hidden` 是 v0.3.0 加的变体：tool 被 server `Request::ToolList` 过滤掉、不出现在 agent discover 结果里，但仍能 `ToolSchema` 和 `RunTool` by id 调到。用例：

- vendor 的 8 个 raw schema endpoint（容易让 LLM 困惑）— 标 Hidden；agent 只看 26 个 helper
- 集成测试 / debug 工具
- 操作员后门

healthkit v1.2.0 之前用 `--expose-raw-tools` 这种「per-binary 开关」凑合；v0.3.0 起用 `Hidden` 是协议级方案。

### 4.6 Skills meta-tool 公约（SP-skills-discovery-convention）

约定每个 ATD server 自愿暴露两个 meta-tool：

- `<publisher>:<service>.skills.list`  →  `Vec<{name, description, version?}>`
- `<publisher>:<service>.skills.get { name }`  →  `{name, content_md}`

加 `atd skills sync --target {hermes|claude-code|stdout}` 子命令把 SKILL.md 一键拉到 agent 平台的 skill 目录。healthkit_cli v1.3.0 是首个 adopter，把 26 个 SKILL.md 通过这个公约暴露出去；用 `atd skills sync --target hermes` 实测拉下来 26 个文件，diff 与源完全一致。

**这不是 wire 协议改动**，是一个 tool-id 命名 + 响应 shape 的「social convention」 — 任何 server 加入只需要写两个 tool。

### 4.7 Cross-vendor 组合（SP-cross-vendor-mock-demo）

ATD 是协议；同一个 agent 可以连 N 个 ATD server，每个自己一个 socket、自己一个 audit、自己一个 token store，agent 看到的是合并 catalog。`scripts/cross-vendor-demo.sh` 把 healthkit + `atd-mock-weather-server` 同时启动，证明 `atd list` 对两个 socket 各自看到 27 + 3 个 tool。

CLI 做不到的关键点：你不能在一个 CLI 进程里同时 `+heartrate` 和 `+weather.now`。要做就得自己写 multiplexer。ATD 把这个折叠成「桥接两个 socket」的配置项。

### 4.8 dry-run（自始至终）

每个 `RunTool { dry_run: true }` 在 dispatch 层短路返回 `{dry_run: true, tool_id, args_preview}`，工具根本不执行。Agent 拿到不执行的 preview 决定要不要走真的。`safety.dry_run: true` 的 tool 标记自己支持。

---

## 5. 与 raw 选项对比

### 5.1 vs raw CLI

| 关注点 | CLI | ATD |
|---|---|---|
| 第一次调用成功率（v1.4.0 实测） | 需先猜命令 / flag | 1 次成功，0 retry |
| 多个 agent 共享 | N 进程、N 配置、N OAuth | 1 server、N caller_id、broker 路由 |
| Audit | shell history | 结构化 JSON Lines |
| 跨 vendor 组合 | 自己写 multiplexer | 桥接多 socket |
| LLM matching ergonomics | 看 `--help` 文本（混沌） | `description` + `intent_examples`（结构化） |
| 升级安全 | flag 加减破坏 agent prompt | tool def 是 schema，rev 跟踪 |

case study v1.1.0 (24%) → v1.2.0 (95%) → v1.4.0 (2 calls vs 8) 全部用 audit log + transcript 证过。

### 5.2 vs raw MCP

MCP 是 client-server 协议，没有：

- server 侧 capability gate（每个 client 自己 gate）
- server 侧 rate limit（同上）
- multi-tenant token routing（MCP 假设单租户 stdio）
- audit log 标准格式
- tool visibility 多档（只有 hidden / visible 二元）
- safety levels（Read/Write/Financial/Privacy/Physical/Destructive）
- tier 概念（Hot/Warm/Cold + 推导 deadline）

ATD 在协议层 ship 这些，并通过 [`atd-mcp-bridge`](../crates/atd-mcp-bridge/) 兼容现有 MCP 客户端 — Hermes、Claude Code、Cursor 不用改一行代码就能接 ATD server。

### 5.3 vs per-vendor 自研 adapter

每个 vendor 自己写一套 server？写过的人都知道：每写一次都要重新设计 capability、audit、rate limit、token 管理、stop logic。ATD `atd-runtime` + `atd-server` 是 ~2000 行 Rust，vendor 写自己的 server 只需要：

- Implement `Tool` trait（`fn definition() + fn call()`）
- 把工具注册进 `Registry`
- 调用 `atd_server::Server::new(registry, config).run().await`

healthkit_cli 的 `healthkit serve` 是 ~150 行 glue（一半是命令行参数解析）。multi-tenant 通过 `ServerConfig::token_broker` 添一行就启用。

---

## 6. 架构层次（5 层）

```
┌──────────────────────────────────────────────────────────────┐
│ Skills Layer (adjacent)                                      │  ← SKILL.md 文件、progressive disclosure
│  - atd skills sync 把 SKILL.md 推到 agent 平台               │
├──────────────────────────────────────────────────────────────┤
│ Agent Framework (Hermes / LangChain / Claude / OpenClaw)     │
├──────────────────────────────────────────────────────────────┤
│ ATD SDK (atd-sdk Rust / atd_client Python)                   │  ← discover / describe / call
│  ├─ atd-cli (命令行入口)                                      │
│  └─ atd-mcp-bridge (MCP/stdio ↔ ATD wire)                    │
├──────────────────────────────────────────────────────────────┤
│ ATD Wire Protocol (5 messages, length-prefixed JSON)         │  ← 跨语言中立
├──────────────────────────────────────────────────────────────┤
│ ATD Server Runtime (atd-runtime + atd-server)                │  ← Tool trait, dispatch
│  ├─ Capability gate                                          │
│  ├─ Per-tool rate limit (Semaphore)                          │
│  ├─ Audit sink                                               │
│  ├─ TokenBroker                                              │
│  └─ Tool visibility filter                                   │
├──────────────────────────────────────────────────────────────┤
│ Vendor Tools (healthkit_cli, atd-mock-weather-server, ...)   │
├──────────────────────────────────────────────────────────────┤
│ Underlying Service (Huawei HMS REST, OpenWeatherMap, ...)    │
└──────────────────────────────────────────────────────────────┘
```

---

## 7. Workspace 实现（atd-mvp v0.3.0）

13 个 crate，378 测试 passing，Apache-2.0：

| crate | 职责 |
|---|---|
| `atd-protocol` | wire 格式 + 类型 + sanitize |
| `atd-sdk` | Rust 客户端 SDK |
| `atd-runtime` | server runtime（registry、dispatch、audit、rate limit、TokenBroker） |
| `atd-server` | Unix socket listener + connection 任务 |
| `atd-tools-{echo,fs,shell,web}` | 内置工具示例 |
| `atd-ref-server` | 参考 server binary（合所有内置工具） |
| `atd-mcp-bridge` | MCP/stdio ↔ ATD wire bridge（v0.3.0+ 支持 `ATD_CLIENT_ID` 多租户） |
| `atd-cli` | `atd` 开发者 CLI（list / schema / call / doctor / **skills sync**） |
| `atd-conformance` | 35 个跨实现 conformance fixture（wire / behavior / sanitize 三类） |
| `atd-mock-weather-server` | 跨 vendor 组合 demo bin |

---

## 8. 何时用 ATD / 何时不

### 用 ATD 当：

- 你的工具 surface 要被 ≥1 个 LLM agent 平台用
- 你预期多 user / 多 caller_id（多租户）
- 你需要审计、可观测性、capability 门禁
- 你想把同一个工具同时给 Hermes + Claude Code + Cursor 用，不重复实现 N 套 adapter
- 你的工具底下是真后端服务（REST / DB / cloud API），不是 sandbox 内简单计算
- 你有多个 vendor 想一起 host 给一个 agent

### 不用 ATD 当：

- 单进程脚本 + 单 user + 单工具（杀鸡用牛刀）
- 工具是 sandbox 内纯计算 / 无 side effect / 无外部依赖（直接函数调用就行）
- 你的工具在 agent 进程内（in-process Tool trait + Registry 不需要跨 socket）
- 你只想要 MCP，且不需要多租户 / 审计 / 跨 vendor 组合（直接写 MCP server）

---

## 9. 上手 5 分钟

### 9.1 跑参考 server

```bash
git clone https://github.com/downsea/atd-mvp.git
cd atd-mvp
cargo build --release -p atd-ref-server -p atd-cli -p atd-mcp-bridge

# 启动参考 server（包含 echo / fs / shell / web / uname 共 10 个内置工具）
./target/release/atd-ref-server --sock /tmp/atd.sock &

# 看
./target/release/atd --sock /tmp/atd.sock list
./target/release/atd --sock /tmp/atd.sock schema ref:fs.read
./target/release/atd --sock /tmp/atd.sock call ref:echo.say --args '{"text":"hello"}'
```

### 9.2 接 Hermes（或 Claude Code）

```bash
# Hermes
hermes mcp add atd-ref \
  --command ./target/release/atd-mcp-bridge \
  --env ATD_SOCK=/tmp/atd.sock

# Claude Code
claude mcp add -s user --env=ATD_SOCK=/tmp/atd.sock \
  atd-ref ./target/release/atd-mcp-bridge
```

agent 平台立刻看到所有 ATD 工具，按 `description` + `intent_examples` 自动选用。

### 9.3 写自己的 vendor server

最小骨架：

```rust
use std::sync::Arc;
use atd_runtime::registry::Registry;
use atd_server::{Server, ServerConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut reg = Registry::new();
    reg.register(Arc::new(MyTool::new()));  // 自己实现 Tool trait
    let cfg = ServerConfig::default();      // 改 socket_path 等
    Server::new(reg, cfg).run().await?;
    Ok(())
}
```

参考 [`crates/atd-mock-weather-server/src/main.rs`](../crates/atd-mock-weather-server/src/main.rs)（80 行）或 [`healthkit_cli/src/atd_server/server.rs`](https://github.com/downsea/healthkit_cli/blob/main/src/atd_server/server.rs)（生产级，含 OAuth、多租户、25 helper）。

---

## 10. 关键引用

- 架构深度参考：[`docs/architecture.md`](architecture.md)
- Wire 协议：[`docs/protocol/wire-format.md`](protocol/wire-format.md)
- 错误码：[`docs/protocol/error-codes.md`](protocol/error-codes.md)
- 集成路径总览：[`docs/integrations/overview.md`](integrations/overview.md)
- per-platform 集成指南：[`hermes.md`](integrations/hermes.md) / [`claude-code.md`](integrations/claude-code.md) / [`langchain.md`](integrations/langchain.md) / [`openclaw.md`](integrations/openclaw.md)
- Adopter case study：[`docs/integrations/healthkit.md`](integrations/healthkit.md)
- 跨 vendor 组合：[`docs/integrations/cross-vendor-pattern.md`](integrations/cross-vendor-pattern.md)
- 实证 transcript：
  - [healthkit_cli/docs/case-study-v1.2.0/](https://github.com/downsea/healthkit_cli/tree/main/docs/case-study-v1.2.0)（4 个 prompt × log，95.2% 验证）
  - [healthkit_cli/docs/case-study-v1.4.0/](https://github.com/downsea/healthkit_cli/tree/main/docs/case-study-v1.4.0)（医生视角心率分析，ATD vs CLI 头对头）
- SP（spec + plan）历史：[`docs/superpowers/`](superpowers/)

---

## 11. 一句话回顾

> **ATD = Unix-socket 上一份精心调校过的 5-message 协议，加一套 server runtime（capability gate / audit / rate limit / TokenBroker / visibility），加一组桥接（MCP-bridge / SDK / CLI），让 vendor 写一份 server 就能被任意 agent 平台用，并自带审计 / 多租户 / 跨 vendor 组合 — raw CLI 拉不出来、raw MCP 没规范、自研 adapter 每次重写的东西，全在这里 ship 了。**

实证：v1.4.0 case study 一个 prompt、2 ATD 调用 vs 8 CLI fallback、完整 audit log 落盘 — 这是协议层差异，不是工具能力差异。
