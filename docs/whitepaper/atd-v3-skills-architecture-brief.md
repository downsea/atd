# Agent × Skill × ATD · Architecture Brief

**Date:** 2026-04-23
**Companion PPTX:** [`atd-v3-skills-architecture-brief.pptx`](atd-v3-skills-architecture-brief.pptx)
**Prerequisite reading:**
- [ATD v3 whitepaper](toward-agent-tool-dispatch-v3.md)
- [Twelve-Factor Skills v1.1](toward-skills-design-principles.md)

本 brief 三页回答两个具体问题：**① 完整架构包括 Skill 层之后长什么样？ ② Agent 和 Skill 分别怎么调 ATD？**

---

## Slide 1 — 完整架构：Agent × Skill × ATD × Bindings × Tools

> **Skill 是剧本层，ATD 是工具 dispatch 层；两层正交互补。**

### 栈结构（自上而下）

```
┌───────────────────────────────────────────────────────────────┐
│   用户 Intent  （语音 / 文本 / 触发器）                          │
└───────────────────────────┬───────────────────────────────────┘
                            │
┌───────────────────────────▼───────────────────────────────────┐
│   Agent Framework                                             │
│   Claude Code · Codex · Cursor · LangChain · OpenClaw · 自研  │
└────────────┬──────────────────────────────┬───────────────────┘
             │                              │
   激活 Skill │                              │ 直接 tool call
             ▼                              ▼
┌──────────────────────────────┐  ┌───────────────────────────┐
│  Skill Layer                 │  │  (无 Skill 中介)           │
│  SKILL.md + atd-tools        │  │  简单 / 一次性任务         │
│  剧本 · progressive disclosure│  │                           │
└──────────────┬───────────────┘  └──────────────┬────────────┘
               │                                 │
               └──────────────┬──────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│   ATD Client SDK   ·   discover / describe / call / session   │
└──────────────────────────────┬───────────────────────────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────┐
│   ATD Dispatch Layer                                          │
│   Device Routing · Capability Token · H/W/C tier · Middleware │
└──────────────────────────────┬───────────────────────────────┘
                               │
       ┌────┬─────┬──────┬─────┴─────┬──────────┐
       ▼    ▼     ▼      ▼           ▼          ▼
     ┌───┐┌────┐┌─────┐┌───────────┐┌──────────┐
     │CLI││MCP ││REST ││AppFunc[]  ││Distributed│
     └───┘└────┘└─────┘└───────────┘└──────────┘
                               │
                               ▼
┌──────────────────────────────────────────────────────────────┐
│   Tool Universe × 7 Device Classes                            │
│   (phone · watch · earbuds · tablet · pc · car_hmi · tv)      │
└──────────────────────────────────────────────────────────────┘
```

### 各层职责

| 层 | 职责 |
|---|---|
| **Agent** | 接收 intent · 推理 · 发起 tool call 或激活 Skill |
| **Skill** | 剧本 · 多步骤组合 · progressive disclosure · `atd-tools` 声明依赖 |
| **Client SDK** | 统一的 3 语言 API（Rust/Python/TS）· 名称 sanitize · capability token 封装 |
| **Dispatch** | device affinity 路由 · UCAN 验证 · result middleware · H/W/C tier 选择 |
| **Bindings** | CLI · MCP · REST · AppFunction (per device × vendor × platform) · Distributed |
| **Tools** | 102+ 内置 · host plugin · MCP server · AppIntent · Wear Engine · HMS kits · ... |

### 类比（三层独立演化，不竞争）

> **Skills（剧本）≈ Python stdlib  ·  ATD（原子能力）≈ POSIX  ·  Agent ≈ Django 应用**

---

## Slide 2 — 调用方式 A：Agent 直接调 ATD

> **适用：一次性 / 简单任务 / agent 已经知道确切要调的 tool**

### 执行流（7 步）

| # | 步骤 | 具体 |
|---|-----|------|
| 1 | Agent 收到 intent | `"我现在心率多少"` |
| 2 | LLM 推理 → 决定调 tool | `tool_id = hms:health.heart_rate.get` |
| 3 | `client.call(tool_id, args)` | SDK 打包请求 + capability token |
| 4 | Dispatch 路由 | `device.preferred=[watch]` → 选 Wear Engine |
| 5 | Binding 执行 | `ArkTS HealthDataProvider.getCurrentHeartRate` |
| 6 | Middleware 处理 | `pii_redact(source_device_id)` |
| 7 | 返回结果 → Agent context | `{bpm: 72, confidence: 0.95, ...}` |

### Agent 代码（Python）

```python
from atd_client import AtdClient, CallOptions

client = await AtdClient.connect(
    'unix:///home/me/.anos/anos.sock')

# 1. 发现（可选，若已知 id 可跳过）
tools = await client.discover(
    query='heart rate', limit=5)

# 2. 直接调用
result = await client.call(
    'hms:health.heart_rate.get',
    args={},
    options=CallOptions(
        session='lily-ambient',
        capability_token=tok))

# 3. 消费结果
print(f'BPM: {result.data["bpm"]}')
```

### 特征

- **● 控制精细**：agent 决定每个 tool id / 参数 / session
- **● Token 成本低**：无 skill body 加载，仅 tool schema 进 context
- **● 适合 one-shot 场景**：查询、测试、主 agent 熟知的核心能力
- **△ 不适合多步工作流**：重复代码、无复用、无 install-time 依赖校验

---

## Slide 3 — 调用方式 B：Skill 配置调 ATD

> **适用：多步骤剧本 / 可复用任务 / 需要 install-time 依赖校验**

### SKILL.md 声明 ATD 依赖（完整示例）

```yaml
---
name: "@acme/morning-briefing@1.0"
description: |
  Summarize last night sleep + today's calendar + weather.
  Activate on "good morning" before noon.

device:
  preferred: [phone, tablet]

atd-tools:
  required:
    - hms:health.sleep.get           # 必需：查睡眠
    - hms:location.current.get       # 必需：查位置
  optional:
    - calendar.get                    # 有就用，没有跳过
    - weather.get                     # 有就用，没有跳过

allowed-tools: [Read, atd.call]       # 最小权限：只读 + ATD 调用
---

# Morning Briefing

## When to use

User says "good morning" / "早上好" / asks for "today's plan"
before noon.

## Steps

1. Call `hms:health.sleep.get(date: yesterday)`.
   ATD v3 routes to phone HMS Health SDK. Note: `source_device_id`
   may be redacted by result_middleware — don't rely on its presence.

2. If `calendar.get` is available (check atd-tools.optional),
   call it for today's events.

3. Call `hms:location.current.get`; pass lat/lon to `weather.get`
   if available.

4. Synthesize a 100-word briefing in markdown. Highlight anything
   unusual (sleep < 6h, big calendar changes, severe weather).
```

### 执行流（7 步）

| # | 步骤 |
|---|-----|
| 1 | Agent 收到 intent `"早上好"` |
| 2 | Description-match → 激活 `@acme/morning-briefing` |
| 3 | Runtime 用 `atd-tools` 校验 → 派生 attenuated capability token |
| 4 | Skill body 进入 agent context（progressive disclosure） |
| 5 | LLM 按 body 步骤**依次**调 ATD tool（A → B → C） |
| 6 | 每次 ATD call 走完整 dispatch（device 路由 / binding 选择 / middleware） |
| 7 | Skill 内部综合输出（100-word markdown 摘要） |

### 特征

- **● Install 时 `atd-tools` 校验**：环境缺必需 tool → skill 不可安装
- **● Capability token 按 skill 粒度 attenuate**（§VI 最小权限）
- **● 可复用 / 可版本化 / 可跨 agent 移植**（SKILL.md 标准加持）
- **● 跨设备组合可行**：skill body 说 `session.handoff()`，dispatch 自动处理
- **△ Token 成本高于直接调用**：body 进 context

---

## 核心 Takeaway

> **Skill = 剧本层**（何时、按什么顺序调）
> **ATD = 原子能力层**（怎么真正调到工具）

两条调用路径并存。写一次性查询用 **A**，写可复用多步任务用 **B**。

## 决策树：A vs B 怎么选？

```
是多步工作流（>2 步）吗？
├── 是 → 需要跨 session / 跨用户复用吗？
│   ├── 是 → 用 Skill（方式 B）
│   └── 否 → 看下一条
│
├── 否（单次调用）→ 用 Agent 直接调（方式 A）
│
└── 不确定 / 需要 install-time 校验依赖 → 用 Skill（方式 B）
```

**额外判据**：
- 需要 atd-tools 依赖校验（装 skill 之前知道能不能跑）→ 用 B
- 预算敏感 / Hot tier 常驻工具 → 用 A
- 跨设备 handoff 编排 → 用 B（skill body 表达更清晰）
- 临时探索 / 调试 → 用 A

## 交叉引用

| 主题 | 位置 |
|------|------|
| Multi-device dispatch 原语 | [ATD v3 §2.5](toward-agent-tool-dispatch-v3.md) |
| Distributed sessions (migrate / fork / handoff) | [ATD v3 §2.6](toward-agent-tool-dispatch-v3.md) |
| Result middleware (pii_redact 等 5 个 builtin) | [ATD v3 §2.7 + Appendix K](toward-agent-tool-dispatch-v3.md) |
| Ergonomic aliases (DSL) | [ATD v3 §2.8 + Appendix J](toward-agent-tool-dispatch-v3.md) |
| `atd-tools` YAML spec | [ATD v2 Appendix G（v3 仍有效）](toward-agent-tool-dispatch-v2.md) |
| Skill ↔ ATD layering（§2.4）| [ATD v3 §2.4](toward-agent-tool-dispatch-v3.md) |
| §III Body · 调用 ATD tool | [Skills v1.1 §III](toward-skills-design-principles.md) |
| §VI Least Privilege · allowed-tools pattern | [Skills v1.1 §VI](toward-skills-design-principles.md) |
| §VIII Composition · 跨设备 | [Skills v1.1 §VIII](toward-skills-design-principles.md) |

---

**文档版本**：v1.0 · 2026-04-23
**状态**：brief — 非规范性概览
**许可**：CC BY 4.0
**关联 PPTX**：[atd-v3-skills-architecture-brief.pptx](atd-v3-skills-architecture-brief.pptx)（3 slides，16:9 widescreen）
