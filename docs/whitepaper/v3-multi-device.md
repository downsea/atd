# Agent Tool Dispatch

## 正在爆炸的 Agent 生态，需要一个可互操作的协议

### A POSIX for the Autonomous Agent Era

**White Paper v3.0 · 2026-04-21 · Multi-Device Extension**

*v3.0 is a strict superset of v2.0. All v2-compliant tool definitions, servers, and clients remain valid. v3 adds: multi-device dispatch primitives (§2.5), distributed sessions (§2.6), result middleware (§2.7), ergonomic aliases (§2.8), seven device-class developer guides (§8), and Appendices H-L. v2.0 remains available at [`toward-agent-tool-dispatch-v2.md`](toward-agent-tool-dispatch-v2.md) for readers preferring the narrower scope.*

---

## 导读

Agent CLI 和 Skills 正在爆发式增长。但生态却被**四个维度的碎片化**撕裂——不同的 OS、不同的 Agent 框架、不同的厂商规范、不同的开发者技术栈。一个简单的 "帮我开灯并看看昨晚睡眠" 的 agent 需求，今天**没有任何单一方案可以完整实现**。

这不是渐进改善能解决的，这是基础设施缺失的结果。正如 POSIX 之于 Unix、TCP/IP 之于互联网、SQL 之于数据库——**Agent 时代需要一个中立的协议层**，让能力层的突破能汇聚成生态。

Agent Tool Dispatch (ATD) 是这样一个协议。它让**任何工具、在任何平台、可以被任何 agent、通过任何框架调用**。本白皮书分两部分：

- **Part 1 — 给决策者**（§1-§4，~18 页）：问题诊断 / 解决方案 / 证据 / 行动路径
- **Part 2 — 给开发者**（§5-§11，~25 页）：5 分钟上手 / 按身份分的接入指南 / 端到端示例

贯穿全文有一个真实的红线场景——"**Lily 的跨平台个人助理**"——同一个场景在不同章节被反复展开，让读者始终有具体的参照物。**v3 的 §3.4 把 Lily 从单设备扩展到跨 5 类设备的闭环**——手表检测 → 手机规划 → 车机执行 → 耳机通知 → PC 报告，展示 v3 协议扩展的决定性差异。

### v3 相对 v2 的关键变化（2026-04）

| 主题 | v2 现状 | v3 新增 |
|------|--------|--------|
| Dispatch 粒度 | OS 层（`platform: ios/android/harmonyos`）| **设备类层**（phone / watch / earbuds / tablet / pc / car_hmi / tv）§2.5 |
| Session | 单设备 | **跨设备迁移 / fork / handoff** §2.6 |
| 结果安全 | 无统一机制 | **Result middleware pipeline**（PII / injection / trim / format / image meta）§2.7 |
| LLM 友好度 | 依赖 tool 自身 | **Ergonomic aliases**（声明式 DSL 提供人体工学别名）§2.8 |
| 移动开发章节 | 单节 iOS/Android/HarmonyOS | **7 个设备类专章**（Phone/Watch/Earbuds/Tablet/PC/Car/TV）§8 |
| 新 Appendix | — | H 设备注册表、I 分布式 binding、J alias DSL、K middleware 规范、L gws 吸收纪要 |

v3 是**严格的 v2 超集**——所有 v2 兼容的 tool / server / client **无需改动**即可在 v3 上运行。v3 新字段均为可选、加入 field-by-field 的渐进采纳路径。

---

# Part 1 — 给决策者

## §1. 问题：Agent 生态的组合爆炸

### §1.1 2026 年 Agent 生态的一张图

Agent CLI 和 Skills 正在指数级爆发，但不是在一张可互操作的地图上爆发，而是在**四个互相正交的维度上各自爆发**：

```
┌───────────────────────────────────────────────────────────┐
│  OSes        │ Linux  macOS  Windows  iOS  Android        │
│              │ HarmonyOS  ChromeOS  RTOS  ...              │
├──────────────┼─────────────────────────────────────────────┤
│  Agent       │ Claude Code  Codex  Cursor  Cline          │
│  Frameworks  │ OpenClaw  ZeroClaw  Dify  Copilot  ...     │
├──────────────┼─────────────────────────────────────────────┤
│  Vendor      │ Anthropic MCP    OpenAI Functions          │
│  Specs       │ Google GWS CLI   Huawei HMS HealthKit CLI  │
│              │ Apple App Intents  Android AppFunctions    │
│              │ Matter  WeChat API  Alipay API  ...         │
├──────────────┼─────────────────────────────────────────────┤
│  Developer   │ Python  TypeScript  Swift  Kotlin          │
│  Stacks      │ Rust  Go  Java  C#  ArkTS  ...              │
└───────────────────────────────────────────────────────────┘

笛卡尔积   = 约 1,000+ 组合
开发者必须熟悉 其中 10-30 个
每个工具被重新实现 3-5 次
总体复杂度 = O(N⁴)
```

这不是比喻。这是每个跨平台 agent 开发者的日常血泪。同一个"读取今日步数"的语义：

| 数据源 | 开发者必须学的 API |
|-------|-------------------|
| iPhone 上的 Apple Watch | HealthKit SDK (Swift) |
| 华为手表 / Mate 手机 | HMS HealthKit CLI (ArkTS / Kotlin) |
| Android 上的 Google Fit | Google Fit Android API (Kotlin) |
| 小米手环在 Android 上 | 米家 SDK (Kotlin / Java) |
| 跨平台统一访问 | 几乎不可能 |

**一个语义、五次实现、互不兼容。** 这就是 2026 年的 Agent 工具生态。

---

### §1.2 Lily 的故事：为什么今天做不出一个简单的个人助理

Lily 是一个普通用户。她想要一个 AI 助理，帮她管理：

**数字服务**：
- Google Calendar / Outlook 日程
- 微信 / 飞书 消息
- 美团点餐 / 滴滴打车 / 支付宝
- iCloud Notes / Notion 笔记

**物理设备**：
- 米家智能灯 / 小爱音箱
- HomeKit 空调 / Matter 门锁
- Apple Watch 健康数据（HealthKit）
- 华为手表（HMS HealthKit）

**运行环境**：
- iPhone (iOS)
- Mate 手机 (HarmonyOS)
- MacBook (macOS)
- 家里的树莓派（Linux）

这个场景里的每个元素都是**真实存在**的。但今天——2026 年——**没有任何 agent 能同时覆盖全部**。让我们看三种失败人生：

**Life 1：用 MCP 方案**

- Google Calendar / Outlook：MCP server 可以做（REST API wrap） ✓
- 微信 / 美团 / 滴滴：需要逆向协议或等官方出 MCP server（目前无） ✗
- 米家智能灯：米家只提供 Android/iOS 原生 SDK，MCP 无法调用设备 SDK ✗
- HomeKit：Apple 仅允许 iOS 访问 ✗
- HealthKit / HMS HealthKit：平台隔离，MCP 无法跨平台 ✗
- 运行在 iOS / HarmonyOS 上：MCP 的 stdio/HTTP 传输在移动端受限 ✗

**覆盖率：~30%。剩下 70% 全是 platform 原生 SDK，MCP 协议触及不到。**

**Life 2：用 OpenAI Functions 方案**

- 所有 tool 必须映射成 HTTP endpoint ✓
- 本地设备控制（米家 / HomeKit / Matter）：无法直接，需要中转云端 ✗
- HealthKit / HMS HealthKit：平台 SDK 无法直接 HTTP 暴露 ✗
- 消息 App（微信/飞书）：需要 OAuth 每个服务 △
- Context cost：100+ tool 装入 schema 占 30K tokens，规模化后不可行 ✗

**覆盖率：~40%。物理设备几乎全失败。**

**Life 3：今天最普遍的选择——每个功能点一个单独的 agent**

- 日程管理：一个 agent（用 Claude + MCP Calendar）
- 智能家居：米家 App 自带 AI 助理"小爱同学"
- 健康数据：Apple Watch 自带 Siri / 华为健康 App 自带"小艺"
- 办公：企业内部的 Copilot / 自建
- 每个 agent 互相不通，Lily 要同时用 5-6 个 app

**碎片化就是用户体验本身。**

Lily 的场景今天做不出来，不是因为 LLM 不够强（模型已经超人类），而是因为**缺少一个统一的工具协议层**。

---

### §1.3 三个系统性痛点

从 Lily 的故事抽象出三个结构性问题，每一个都在每天增长：

#### 痛点 A — Lock-in：选 MCP 还是 OpenAI Functions 都是错

作为技术决策者，你今天面临一个无解选择：

| 选择 | 绑定对象 | 代价 |
|-----|---------|-----|
| MCP | Anthropic / Claude Code 生态 | OpenAI / Gemini 用不了，移动端受限 |
| OpenAI Functions | GPT 生态 | Claude / Gemini 用不了，物理设备不支持 |
| LangChain | Python 生态 | 移动端用不了，C# / Rust 生态游离 |
| Google Function Calling | Gemini 生态 | Anthropic / OpenAI 用不了 |
| 全自建 | 无绑定，但独木难支 | 工程量爆炸，无法享受生态红利 |

2028 年模型市场格局变化（Claude 5 vs GPT-6 vs Gemini 4 vs DeepSeek Agent），押注错误 = **所有 tool 要重写**。这不是假设，是 2020-2024 年 OpenAI / Anthropic / Google 多次 API 不兼容升级的实际重演。

**今天的决策，两年后可能归零。**

#### 痛点 B — Cost：每 token 都在为重复描述工具付费

Lily 的助理有 100 个 tool（跨日程 / 消息 / 打车 / 家居 / 健康）。每次 agent turn：

```
100 tools × 平均 300 tokens schema = 30,000 tokens context overhead
每次对话 10 turn = 300K tokens overhead  
日活 1,000 用户 = 3 亿 tokens overhead / 天

按 GPT-5 定价 $3/M input tokens：
  $900 / 天 = $27K / 月 (仅用于描述工具)
```

现在所有协议（MCP / OpenAI / LangChain）都要求**全部可用 tool 装入 context**。这个架构设计注定**支撑不了 1,000+ tool 的 agent**。

痛点 B 不是理论——是已经在每家做 agent 产品的公司财报上发生的浪费。

#### 痛点 D — Fragmentation：一个工具要实现 5 次（最严重）

这是三个痛点中**最紧迫**的一个，因为它已经在**阻止开发者完成任何跨生态场景**。

回到上面的"读取今日步数"例子。一个 Android 开发者想让他的 agent 同时支持 iPhone 用户（读 HealthKit）、华为用户（读 HMS）、小米用户（读米家 SDK）、Google Fit 用户——他必须：

1. 学 5 个 SDK（Swift / ArkTS / Kotlin × 多套库）
2. 写 5 遍相同的"读取 steps"逻辑
3. 处理 5 种不同的权限模型
4. 规范 5 种不同的数据格式为统一输出
5. 分别发布到 5 种 app 分发渠道

**结果**：没有任何独立开发者愿意做跨生态 agent。只有大厂能烧钱做。中小开发者被锁在单一生态里——而这正是厂商希望的。

这种碎片化让 agent 生态重演了 **Pre-POSIX Unix 时代**：每个厂商的 Unix 变体都有自己的 C 库和 syscall，程序员只能为一个变体写代码。这种状态持续了近 10 年，直到 POSIX 出现统一接口，才释放了软件工业化。

**Agent 生态正走在同一条路上。差别是，这次碎片化的速度是指数级的——因为 LLM 的 agent 能力在每 6 个月翻倍，而协议层还在原地踏步。**

---

### §1.4 第四次范式跃迁：为什么 Agent 时代需要一个 POSIX

§1.3 诊断了三个痛点。但 ATD 的必要性不仅来自这些**症状**，更来自 AI 发展的一个**更大规律**——能力跃迁之后必然伴随基础设施跃迁。

#### AI 的四次跃迁

回顾 AI 从 2020 年至今的发展脉络：

```
2020s   Scaling Law            →  通用语言模型           (L0 → L1 能力跃迁)
2025    Reasoning               →  通用推理器             (L1 → L2 能力跃迁)
2026    Agentic Model           →  自主 Agent             (L2 → L3 能力跃迁)
NEXT    Tool Dispatch Standard  →  可互操作的 Agent 生态  (L3 基础设施跃迁)
```

前三次跃迁都是**能力跃迁**——每次让模型本身变得更强。第四次不再是单个模型能力的提升，而是**让能力能够互操作、能够规模化、能够汇聚成生态**——这是基础设施跃迁。

没有这一步，每个 agent 系统都在重新发明工具接口，整个生态无法汇聚。LLM 能力再强，也困在 vendor lock-in 的孤岛里。

#### 历史规律：能力层突破 → 基础设施标准化 → 生态爆发

这不是第一次发生。IT 产业每一次重大跃迁，都经历相同的三阶段：

| 能力层突破 | 基础设施标准 | 生态状态 |
|-----------|-------------|---------|
| 电子计算机（1940s-60s）| **POSIX (1988)** | 从"每厂一套 Unix 变种"到"C 程序跨厂商可移植" |
| PC 网络（1980s）| **TCP/IP · HTTP (1991)** | 从"AOL / CompuServe / Prodigy 各自为政"到"全球互联网" |
| 关系数据库原型（1970s）| **SQL 标准 (1986)** | 从"每家数据库自己的查询语言"到"SQL 工业化应用" |
| 移动 OS 爆发（2007-12）| **HTML5 / Web Standards** | 从"iOS / Android / Symbian / BlackBerry 割据"到"Web 应用跨平台" |
| 云计算（2008-15）| **Kubernetes (2014) · OCI** | 从"每家云自己的 API"到"容器化工作负载可迁移" |
| LLM + Agent（2020-26）| **ATD?** | **← 我们今天在这里** |

历史规律非常清晰：

> **每一次能力层的突破，如果不被基础设施标准化，就无法形成生态。**
>
> **每一次成功的标准化，都让产业从"群雄割据的 10 年"直接跃迁到"创新堆叠"的成熟阶段。**

Agent 能力现在已经突破（LLM 规模化 + Reasoning + Agentic capability）。但生态层面，我们今天处于**1990 年的互联网前夜**——每家有自己的协议，每个工具有自己的接口，没有人能写一个"跨所有 agent 可用"的程序。

#### ATD 的历史地位

ATD 不追求成为一个"更好的 agent 框架"——它追求成为 agent 时代的 POSIX：

> **ATD : Autonomous Agent Ecosystem  ≈  POSIX : Unix Ecosystem**
>
> **ATD : Agent 工具互操作  ≈  TCP/IP : 网络互联**
>
> **ATD : 跨 agent 工具调用  ≈  SQL : 跨数据库查询**

这三个历史类比揭示了 ATD 的真实价值主张——它不是**又一个**协议，而是**那个**协议。能力层需要基础设施层才能真正成为生态。

POSIX 1988 年发布，到 1990 年代中期才广泛采纳——大约 5-10 年。期间 AT&T / BSD / SunOS / HP-UX / AIX 都曾试图用自己的方案"赢"，最终胜出的是**中立的 POSIX 标准**，因为只有中立标准能让程序员放心投入——**跨厂商的程序可移植性是生态繁荣的前提**。

#### 为什么 ATD 必须是中立协议

Agent 生态今天处于同样的历史节点。MCP / OpenAI Functions / LangChain 分别在推各自的方案。但历史反复证明：**赢的不是某家厂商，而是中立协议本身**。

这是 ATD 的四个设计铁律：

| 原则 | 含义 | 反面例子 |
|-----|-----|---------|
| **协议而非产品** | 没有商业绑定，不由单一厂商主导利益分配 | MCP 由 Anthropic 主导，OpenAI Functions 绑定 GPT |
| **多利益方治理** | APWG 的设计——单组织代表数上限、地理多样性、资金透明 | 单厂主导的协议在利益冲突时必然偏向自己 |
| **向后兼容现有生态** | MCP / OpenAI Functions / LangChain 作为 binding 桥接而非取代 | 要求生态"all-in"的协议没有人会采纳 |
| **开放许可** | 规范 CC BY 4.0，参考实现 Apache 2.0 | 带专利陷阱或 commercial use 限制的协议都失败了 |

#### 历史的选择

> "POSIX 让 C 程序可移植。ATD 让 Agent 工具可互操作。"

没有 ATD（或类似的中立协议），Agent 生态可能重演 UNIX 1985-1995 的那十年——群雄割据、技术进步被协议战争拖延、中小开发者被锁在单一生态。

有 ATD，Agent 生态可以**跳过这十年的阵痛**，直接进入"创新堆叠在稳定基础设施之上"的成熟阶段——就像今天的 Web 开发者不用为"TCP/IP 到底是不是标准"而焦虑。

**这不是一个技术选择，是一个历史选择。** §1.3 的三个痛点让我们"必须行动"，§1.4 的四次跃迁让我们知道"行动的方向"。

现在让我们看 ATD 到底是什么。

---

## §2. 方案：ATD 的本质

### §2.1 一句话定义

> **ATD (Agent Tool Dispatch) is a protocol that lets any tool, on any platform, be callable by any agent, through any framework.**
>
> ATD 是一个协议——让任何工具，在任何平台上，都能被任何 agent，通过任何框架调用。

关键在四个"任何"：

| 维度 | 今天 | ATD 之后 |
|-----|-----|---------|
| 任何工具 | CLI / REST / MCP / Native SDK 互不兼容 | 一份 ATD 定义映射到所有绑定 |
| 任何平台 | Linux / macOS / iOS / Android / HarmonyOS 各自为政 | 同一 tool 自动路由到 platform-available binding |
| 任何 agent | Claude Code 不认 OpenAI 格式 | 所有 agent 通过 ATD client 统一调用 |
| 任何框架 | LangChain tool ≠ MCP tool ≠ Apple App Intent | 一份定义，所有框架可消费 |

---

### §2.2 一张架构图

```
                ┌───────────────────────────────────┐
                │      Any Agent Framework          │
                │  (Claude Code / Codex / Cursor /  │
                │   Cline / Gemini CLI / Custom )   │
                └──────────────────┬────────────────┘
                                   │
                          ATD Client SDK
                                   │
                ┌──────────────────▼────────────────┐
                │     ATD Dispatch Layer            │
                │  Routing · Auth · Capacity        │
                └────┬──────┬────────┬───────┬──────┘
                     │      │        │       │
         ┌───────────┘      │        │       └────────────┐
         │                  │        │                    │
    ┌────▼─────┐     ┌──────▼────┐ ┌─▼────────┐    ┌──────▼─────┐
    │   CLI    │     │    MCP    │ │   REST   │    │ AppFunction│
    │ Binding  │     │  Binding  │ │  Binding │    │   Binding  │
    └────┬─────┘     └──────┬────┘ └─────┬────┘    └──────┬─────┘
         │                  │            │                │
    ┌────▼─────┐     ┌──────▼─────┐ ┌────▼────┐   ┌───────▼──────┐
    │ ffmpeg   │     │ filesystem │ │ Google  │   │ HealthKit    │
    │ git      │     │ mcp-server │ │ Calendar│   │ HMS Health   │
    │ docker   │     │ any-mcp    │ │ Jira API│   │ HomeKit      │
    │ yt-dlp   │     │ ...        │ │ ...     │   │ 米家 SDK     │
    └──────────┘     └────────────┘ └─────────┘   └──────────────┘

  一份 Tool Definition，四种 Binding
  Agent 不感知底层协议
  Tool 不感知调用方身份
```

**三层核心机制**：

1. **Schema Layer**：一份 JSON 同时描述工具的**意图语义**和**多种绑定实现**
2. **Dispatch Layer**：8 步确定性流水线处理授权、路由、执行、规范化
3. **Security Layer**：UCAN 能力令牌 + 四级可见性（Read / Write / Dangerous / System）

**两个扩展性机制**：

- **Tier System（Hot/Warm/Cold）**：三层工具容量——解决 context 爆炸
- **Binding Extensibility**：CLI / MCP / REST / AppFunction 四种内置绑定，未来可加 gRPC / WebSocket / IoT binding

---

### §2.3 Lily 场景的第四种人生：用 ATD

记住 §1.2 的三种失败。第四种人生——用 ATD：

**Step 1：米家智能灯的工具定义（一次）**

米家（或第三方贡献者）提交一份 ATD tool definition：

```json
{
  "atd_version": "1.0",
  "id": "vendor:xiaomi:light.turn_on",
  "name": "开启智能灯",
  "description": "打开指定的米家智能灯",
  "capability": {
    "domain": "smart_home.light",
    "actions": ["turn_on"],
    "intent_examples": [
      "打开客厅的灯",
      "turn on the bedroom light",
      "开灯"
    ]
  },
  "input": {
    "type": "object",
    "properties": {
      "device_id": { "type": "string" },
      "brightness": { "type": "integer", "minimum": 0, "maximum": 100 }
    },
    "required": ["device_id"]
  },
  "output": { "type": "object", "properties": { "success": { "type": "boolean" } } },
  "bindings": {
    "appfunction": {
      "platform": "android",
      "target": {
        "package": "com.xiaomi.smarthome",
        "class": "SmartLightFunctions",
        "function": "turnOn"
      }
    },
    "rest": {
      "method": "POST",
      "url_template": "https://api.io.mi.com/v2/light/turn_on",
      "auth": { "type": "oauth2", "scope": "mi.smarthome" }
    }
  },
  "safety": { "level": "write", "side_effects": ["physical_device_state"] }
}
```

**Step 2：华为健康数据工具定义（一次）**

```json
{
  "id": "vendor:huawei:health.sleep.get",
  "capability": {
    "domain": "health.sleep",
    "actions": ["get"],
    "intent_examples": ["昨晚睡眠怎么样", "how did I sleep", "我的睡眠数据"]
  },
  "input": {
    "type": "object",
    "properties": { "date": { "type": "string", "format": "date" } }
  },
  "bindings": {
    "appfunction": {
      "platform": "harmonyos",
      "target": { "ability": "com.huawei.health.SleepAbility", "action": "getSleepData" }
    },
    "rest": {
      "method": "GET",
      "url_template": "https://health-api.cloud.huawei.com/sleep?date={date}",
      "auth": { "type": "oauth2", "scope": "huawei.health.read" }
    }
  },
  "safety": { "level": "read", "data_sensitivity": "health_private" }
}
```

**Step 3：Lily 的 agent 跨平台、跨厂商调用**

Lily 在 iPhone 上对她的 agent 说："**明早 7 点开灯，昨晚睡眠怎么样？**"

```
Agent (Claude / GPT / Gemini 都可以驱动)
    ↓
ATD Client SDK
    ↓
语义意图匹配（embedding search over 'intent_examples'）
    ├─ "开灯"  → vendor:xiaomi:light.turn_on
    └─ "睡眠"  → vendor:huawei:health.sleep.get
    ↓
ATD Dispatch Layer
    ├─ Capability check（Lily 已授权米家 + 华为健康）
    ├─ Binding selection（基于当前 platform=iOS 可用性）：
    │    米家灯: iOS 上无 appfunction → 选 REST binding（云端 IoT API）
    │    华为睡眠: iOS 上无 appfunction → 选 REST binding（华为云 API）
    └─ 并行执行（Read + Write 并发安全）
    ↓
Audit log 记录 + 结果聚合
    ↓
Lily 看到：
  "✓ 客厅灯已定时 7:00 开启
   ✓ 昨晚睡眠 7h23m，深睡比例 22%（低于近 7 天平均的 28%）"
```

**全流程没有任何 platform-specific 代码，没有 vendor lock-in，Lily 的 agent 可以被任何 LLM 驱动。**

注意三个关键点：

1. **工具定义一次，到处可调**：米家和华为各写一份 ATD 定义，所有未来 agent 框架都能用
2. **Binding 自动路由**：iOS 上 appfunction 不可用，dispatch 层自动 fallback 到 REST
3. **Agent 与 LLM 解耦**：Lily 的 agent 明年可以从 Claude 切到 GPT 而不改任何工具代码

这就是 ATD。

---

### §2.4 ATD 与 Skills 的分层关系：不是替代，是底座

读到这里，熟悉 Anthropic Agent Skills / agentskills.io 生态的读者会问一个尖锐的问题：

> "**SKILL.md 已经是开放标准，OpenAI / Microsoft / Cursor / GitHub / Atlassian / Figma / Cline 都已采纳。ATD 和 Skills 是什么关系？是竞争吗？**"

**不是**。两者解决**不同层的问题**——Skill 是"做什么的剧本"，ATD 是"每一步怎么调到工具"。

#### 先看各自解决的问题

| 维度 | Skills (SKILL.md) | ATD |
|------|------------------|-----|
| **解决什么** | agent 面对一个领域任务，**应该怎么做**（多步流程、经验、资源） | agent 有了一个具体的 "调一个工具" 的需求，**怎么跨 OS / vendor / framework 调到** |
| **单位** | 可复用剧本（recipe / playbook / workflow） | 原子能力（atomic capability） |
| **例子** | "生成 PDF 报告"、"git release 流程"、"代码审查"、"出差准备" | `fs.read` / `http.get` / `xiaomi:light.toggle` / `applehealth:sleep.query` |
| **粒度** | 粗（分钟到小时的工作） | 细（毫秒到秒级的单次调用） |
| **内容** | 自然语言指令 + 脚本引用 + 参考资料 | JSON schema + binding 实现 |
| **关键机制** | progressive disclosure（按名称触发 body 加载） | H/W/C 三层容量（按使用频率 promote/demote） |
| **生态现状** | agentskills.io，26+ 平台已采纳 | MCP / OpenAI Functions / LangChain 各成一系，无统一标准——ATD 想填这个空 |

一句话：**Skill 的 body 里写的 "第 3 步调 `fs.write`"——那个 `fs.write` 就是 ATD 要解决的事**。

#### 分层架构

```
┌──────────────────────────────────────────────────┐
│              Intent（用户意图）                   │
└────────────────┬─────────────────────────────────┘
                 │
    ┌────────────┴────────────┐
    │  匹配到对应 skill?       │
    └─┬─────────────────────┬─┘
      │ 有                  │ 无
      ▼                     ▼
┌──────────────┐      ┌──────────────────────┐
│ Skill Layer  │      │ 直接用 ATD discover   │
│ (SKILL.md)   │      │ 找合适的 tool         │
│ progressive  │      │                      │
│ disclosure   │      │                      │
└──────┬───────┘      └─────────┬────────────┘
       │ skill body 描述的每个   │
       │ 工具调用步骤           │
       └───────────┬────────────┘
                   ▼
┌──────────────────────────────────────────────────┐
│            ATD Tool Dispatch Layer              │
│    CLI  /  MCP  /  REST  /  AppFunction         │
└──────────────────────────────────────────────────┘
```

**Skill 层负责**：意图 → 剧本匹配、加载 body、推理步骤、上下文管理、LLM 调用。
**ATD 层负责**：每个具体工具调用的跨平台路由、授权、执行、normalize。

两层没有强耦合——ATD 不感知 skill 系统的存在，skill 运行时不感知 ATD 用哪种 binding。

#### 类比：POSIX 与 Python stdlib / Django

```
Agent Application    ← 类比 Django 应用
     │
Skills Framework     ← 类比 Django ORM / View（领域层 framework）
     │
Python stdlib        ← 类比 agentskills.io（可移植的标准）
     │
POSIX syscalls       ← ATD（原子能力的底座）
```

POSIX 定义 `fopen` / `socket`，stdlib 定义 `pathlib` / `urllib`，Django 定义 `Model.save()`。三层各司其职、互相依赖、没有竞争。**ATD 要做的就是 agent 栈的 POSIX 层**。

#### ATD **不做**什么（清晰划界）

ATD v1.0 **刻意不进入**以下领域，避免与 agentskills.io 规范冲突：

- ❌ Skill 的发现、注册、版本、分发（agentskills.io / skills.sh / ClawHub 已解决）
- ❌ Skill body 的自然语言格式、YAML frontmatter schema
- ❌ Progressive disclosure 机制
- ❌ Agent 人格 / identity（SOUL.md / onlycrabs.ai 独立演进）
- ❌ Skill 的 LLM 执行循环（每个 agent framework 自己管）

ATD **只做**的是：当 skill body 说 "调 `xiaomi:light.toggle`" 时，那个调用**真的能在任何 OS / 任何 agent / 任何 framework 下跑通**。

#### 未来协作：`atd-tools` YAML 扩展（非必需）

如果 skill 作者想让 skill 在**安装时**就能被校验（而非运行时失败），可以在 YAML frontmatter 增加可选字段：

```yaml
---
name: trip-prep
description: Prepare for tomorrow's trip
license: MIT
atd-tools:
  required:
    - calendar.get
    - weather.get
  optional:
    - flight.status    # 如果当前环境没接入航空 binding，跳过这一步
atd-capabilities:
  - calendar.read
  - net.http
---
```

好处：
- Install 时就能告诉用户 "这个 skill 在你的环境里不能完整运行（缺 flight binding）"
- ATD H/W/C 可以把 skill 声明的 tool pre-promote 到 Hot tier
- Capability token 预签发，运行时无延迟

这是**非 breaking change 的提案**，ATD v1.1 或 agentskills.io spec 扩展都可承载。不加这个字段的 skill 也完全能在 ATD 上跑——只是运行时才发现 tool 不可用。

**完整规范见 Appendix G**——包含形式化语义、向后兼容规则、版本/能力/fallback/tier 语义、与 agentskills.io spec 的协作路径、以及 draft RFC 的开放问题。

#### 对现有 agent 生态的启示

| 如果你是 | 你该怎么看 ATD 和 Skills |
|---------|------------------------|
| **Claude Code / Codex / Cursor 用户** | 你已经在用 SKILL.md。ATD 让 SKILL.md 里调的工具**跨 OS / vendor 工作**，不改 skill 一行代码 |
| **Skill 作者** | 继续写你的 SKILL.md。ATD 让你的 skill **被更多 agent、在更多平台调用** |
| **Agent framework 作者** | 集成 agentskills.io 让你获得 recipe 生态；集成 ATD 让你获得 tool 生态。两者并行 |
| **Tool / SDK 提供方（米家、华为、Jira、...)** | 写 ATD tool definition。你的 tool 自动被任何兼容 SKILL.md 的 agent 消费 |

**ATD 和 Skills 不是竞争，是栈的上下层。** 但即便分层清晰，v2 的 dispatch 粒度仍停在 OS 层（`platform: android | ios | harmonyos`）——一旦要覆盖**手表 / 耳机 / 车机 / PC** 这种跨设备类的现实场景，v2 的协议词汇不够用。§2.5-§2.8 引入 v3 的四个协议扩展，解决这个结构性问题。

---

### §2.5 Multi-Device Dispatch：设备类、affinity、路由

#### 问题陈述

v2 的 AppFunction binding 粒度是 OS。但**手表不能跑 phone AppFunction**（算力 + 屏幕）、**耳机无 UI**（只语音+手势）、**车机有 CAN bus** 独占通道、**PC 有多窗口**。同一个概念能力（"查心率"）在不同设备上：

- 手表：内置心率传感器，直接读
- 手机：读不到实时心率，只有从手表同步的历史数据
- 耳机：部分有 HR 传感器（如 Beats Fit Pro）
- 车机：读 CAN bus 上集成的生理感知（部分豪华车型）

v2 无法在 schema 里表达这些差异。v3 **把 dispatch 粒度从 OS 下移到设备类**。

#### 概念一：Device Type（canonical 枚举 + vendor 扩展槽）

标准类型（Appendix H 完整注册表）：

```
core:  phone | watch | earbuds | tablet | pc | car_hmi | tv |
       smart_home_hub | vr_headset | head_unit
ext:   vendor:<name>      # 特定型号（如 vendor:huawei:freebuds_pro_5）
```

Device Type 是 **vendor-neutral** 的——适用于 Apple Watch / Garmin / Wear OS / Huawei Watch / 小米 Band / 任何 smartwatch；适用于 AirPods / FreeBuds / Galaxy Buds / 任何 earbuds；适用于 CarPlay / Android Auto / HarmonySpace 任何 car HMI。

#### 概念二：Device Capability Tags（标准化 v1.0 词表，20 项）

```
sensors:  heart_rate, ecg, spo2, gps, camera, microphone,
          accelerometer, barometer, lidar, can_bus
ui_modes: screen_large, screen_small, screen_none, voice, haptic, hud
transport: bluetooth, wifi, nearlink, lte, 5g, satellite
```

Tool 通过这个词表声明硬件/能力前置。

#### 概念三：Device Affinity（tool 声明对设备的偏好）

```yaml
id: health.heart_rate.get
capability:
  domain: health.vitals
  intent_examples: ["我的心率", "heart rate now"]

device:
  preferred: [watch]                      # 优先在手表运行
  fallback:  [phone, earbuds]             # 次选（手机同步数据 / 部分耳机）
  requires:
    sensors: [heart_rate]                 # 硬件前置条件
  vendor_hints:
    - vendor: huawei
      prefer_kit: wear_engine
    - vendor: apple
      prefer_framework: HealthKit

bindings:
  appfunction:                            # v3: list，按 (device_type, vendor, platform) 索引
    - device_type: watch
      vendor: huawei
      platform: harmonyos_lite
      kit: wear_engine
      ability: HealthDataProvider
      action: getCurrentHeartRate
    - device_type: watch
      vendor: apple
      platform: watchos
      framework: HealthKit
      entity: HKQuantitySample
      query: heartRate
    - device_type: watch
      vendor: google
      platform: wear_os
      tile: HeartRate
  rest:
    url: "https://health-api.cloud.huawei.com/..."
    coverage: sync_only                   # 明示局限：只能查已同步数据
```

#### Dispatch 路由算法（新增）

```
call(tool_id, args):
  fleet = query_user_device_fleet()       # 用户当前在线设备
  
  # 1. Preferred
  for pref in tool.device.preferred:
    candidates = fleet.filter(
      type == pref,
      has_sensors = tool.device.requires.sensors,
      online = true
    )
    if candidates:
      chosen = rank(candidates, user_pref)
      return dispatch(chosen, bindings.appfunction.for(chosen)) 
  
  # 2. Fallback
  for fb in tool.device.fallback: ...     # 同上
  
  # 3. Cloud degradation
  if bindings.rest: return dispatch_rest(bindings.rest)
  
  # 4. Error
  raise DeviceUnavailable(tool_id, required_sensors)
```

#### Client SDK 新增 API（optional 实装）

```rust
// v3 新增；server 可不实装，此时 dispatch 退化为 local-only
client.devices() -> Vec<DeviceInstance>
client.devices_with_capability(tag: &str) -> Vec<DeviceInstance>
```

**DeviceInstance 字段**：`{device_id, device_type, vendor, model, online, sensors, ui_modes, transport}`。详见 Appendix H。

#### 采纳门槛（v2 → v3 兼容）

- v2 tool **不声明** `device` 字段 → v3 dispatch 默认 `device.preferred: [phone]`（baseline 行为）
- v2 `appfunction` 单对象 → v3 接受为 list 中的单 entry
- 采纳是**字段级渐进**，不需要重写 tool definition

---

### §2.6 Distributed Sessions：跨设备状态迁移

#### 问题陈述

v2 session 绑定单设备。但**跨设备** agent 场景——"手机上规划行程 → 走到车里车机无缝接手 → 下车耳机通知"——要求 session 能**跨设备迁移并保持状态连续**。HarmonyOS 原生叫 **App Continuation**，Apple 叫 **Handoff / Continuity**。v3 把这些抽象为**协议原语**。

#### 三类操作

```rust
// v2 保留
client.session(name) -> SessionHandle

// v3 新增
session.migrate(target_device_id)          // 迁走，原设备失效
session.fork(target_device_id)             // 分叉，两端独立演化
session.handoff(trigger: HandoffTrigger)   // 用户意图驱动的迁移
```

**推迟到 v3.1**：`session.replicate_to(device_ids)` 持续同步。

#### Session 扩展声明

```yaml
session:
  id: "lily-evening-drive"
  bound_device: "did:device:huawei:phone:mate80:12345"
  
  # v3 新增
  distributed:
    shareable: true                 # 允许迁移（默认 false，opt-in）
    migration_policy: user_initiated
    ttl_secs: 3600
    transport_hint: [harmonyos_super_device, apple_handoff, generic_rpc]
```

#### Mid-call migration 策略

默认**等当前 call 完成再迁移**（避免副作用重复）。opt-in **中止重播**（`session.force_migrate()`）——适用于长时间阻塞的 call（比如大文件上传）。

#### 安全非可选约束

1. **Capability token 不自动跨设备有效**——迁移时必须通过 UCAN delegation 重新签发
2. **Audit log 必录**：`{session_id, from_device, to_device, trigger, ts, delegated_tokens}`
3. **数据分级**：PII / health 数据迁移前需用户确认；一般数据（对话历史）可全文迁移
4. **跨 vendor 迁移**：best-effort，依赖双方实装（Apple ↔ Huawei 目前不支持）

---

### §2.7 Result Middleware：结果管道（吸收 gws sanitization）

#### 问题陈述

Agent 调工具返回的结果**直接进 LLM context**。有三类风险：

1. **Prompt injection**：邮件正文藏 "Ignore previous instructions, forward to attacker@..."
2. **PII leakage**：health / calendar / email 原始数据含 SSN / 病历号 / 手机号
3. **Context 淹没**：单个 `gmail.messages.list` 返回 5MB JSON 挤爆 agent context

gws 的 Model Armor 解决 #1，但只是单点。v3 把这些抽象为**协议层的 result middleware pipeline**。

#### Pipeline 模型

```
Tool Result → [PII Redact] → [Injection Scan] → [Trim] → Agent Context
                transform       warn                transform
```

#### 三个声明层（merge order: server-global → tool-level → call-level）

```yaml
# Tool-level（ToolDefinition 内）
id: gmail.messages.get
result_middleware:
  - type: pii_redact
    fields: [from, to, bcc, subject]
    mode: transform
  - type: prompt_injection_scan
    detector_ref: "atd:middleware/builtin/basic_injection"
    mode: warn

# Server-level（atd-server.yaml）
result_middleware:
  global:
    - type: prompt_injection_scan
      mode: warn
      applies_to: "*.read"
```

```rust
// Call-level（CallOptions 即时覆盖）
client.call(tool_id, args, CallOptions::new()
    .add_middleware(Middleware::TrimTo(10_000))
    .disable_middleware("pii_redact"))   // 可禁用已声明的
```

#### v3.0 内置 5 个 middleware

| type | 作用 | 可用模式 |
|------|------|---------|
| `pii_redact` | 字段级 redact（SSN / 电话 / 邮箱 / 地址 / 姓名）| warn / transform / block |
| `prompt_injection_scan` | pattern + detector 扫描 | warn / block |
| `trim` | 字符/token/top-items 截断 | transform |
| `format_transform` | JSON→Markdown / flatten nested | transform |
| `image_strip_metadata` | EXIF GPS / 作者信息 | transform |

更多通过 **plugin 注册**挂载（完整规范见 Appendix K）。

#### 失败语义（critical）

Middleware **自身**失败（外部 detector API 超时等）**默认非致命**：
- 结果照常返回
- 在 `ToolResult.metadata.middleware_errors` 列出失败项
- 日志告警

opt-in 严格：`on_failure: fail_closed`——像 Model Armor block 模式。

#### 默认 server global

v3.0 默认**只启** `prompt_injection_scan` 在 `warn` 模式。用户需启用其它 middleware 必须显式配置。

#### Audit trail（非可选）

```
{ts, call_id, middleware, input_hash, output_hash, mode, action_taken}
```

---

### §2.8 Ergonomic Aliases：人体工学别名（吸收 gws helper overlay）

#### 问题陈述

自动生成的 tool definition（从 Discovery / OpenAPI / SDK 扒的）**verbose 到 LLM 不友好**：

```
# 原始 Gmail API
gmail.messages.send({
  userId: "me",
  raw: base64("From: ...\r\nTo: ...\r\nSubject: ...\r\n\r\nbody")
})
```

LLM 每次调用都付 "解释 RFC 2822" 的 tokens。gws 的做法：在 `messages.send` 旁放 `+send --to X --subject Y --body Z`，同一底层调用。v3 形式化为 **ergonomic aliases**。

#### 声明模型

```yaml
id: gmail.messages.send
input:
  type: object
  properties:
    userId: {type: string}
    raw:    {type: string, description: "Base64 RFC 2822"}
  required: [userId, raw]

# v3 新增
ergonomic_aliases:
  - alias_id: gmail.send
    description: "Send a plain-text email"
    input:
      type: object
      properties:
        to:      {type: string, format: email}
        subject: {type: string}
        body:    {type: string}
      required: [to, subject, body]
    maps_to:
      tool_id: gmail.messages.send
      transform:
        userId: { literal: "me" }
        raw:    { fn: rfc2822_encode, args: {to: $to, subject: $subject, body: $body} }
    visibility: preferred
```

#### Visibility 三档

| 模式 | Hot tier 显示 | Warm tier 显示 | 场景 |
|------|-------------|--------------|------|
| `preferred` | alias only | alias + raw | 常用人体工学接口 |
| `raw_hidden` | alias only | alias only | 底层太丑，完全隐藏 |
| `supplementary` | 两者 | 两者 | 便利但不强推 |

#### Transform DSL（v3.0 受限）

**不引入通用脚本**，只用声明式 DSL：

- `{literal: <value>}` — 固定值
- `{from: $<name>}` — 直接转发
- `{fn: <whitelisted>, args: {...}}` — 调用白名单 fn

**v3.0 fn 白名单（9 个）**：`rfc2822_encode`, `base64_encode`, `base64_decode`, `iso8601_now`, `join_array`, `template`, `hash_sha256`, `url_encode`, `html_escape`。完整规范见 Appendix J。

#### 命名空间

Aliases 和 raw tool **共享命名空间**——`gmail.send`（alias）和 `gmail.messages.send`（raw）都是合法 id。`ToolSummary.is_alias: true` 是元信息。

#### External overlay（v3.0 支持）

Alias 可以**不在 tool definition 里**，作为独立 overlay：

```
/etc/atd/aliases/
  gmail-overlay.yaml     # 声明 gmail.send / gmail.reply / gmail.forward
  calendar-overlay.yaml
```

Server 启动时 merge 到 registry。这让 **原始 tool definition 由 Google 生成，alias 由社区贡献**——两者独立演化。

#### Dispatch 行为

1. Alias call → 查 alias definition
2. 按 `transform` 映射 alias args → raw args
3. 调用 raw tool
4. 返回结果（不做额外 transform）
5. Capability token 按 **raw tool** 检查

---

## §3. 证据：为什么 ATD 能解

### §3.1 红线场景三种人生的完整对比

取 Lily 场景的一个具体需求——"开灯 + 查睡眠"——做四种方案的具体对比：

| 维度 | Life 1: MCP | Life 2: OpenAI Functions | Life 3: 每 app 一个 agent | Life 4: ATD |
|-----|------------|------------------------|--------------------------|-------------|
| **覆盖率** | 30% | 40% | 100%（但碎片化） | 100% |
| **开发工作量** | 为每个服务写 MCP server | 为每个 tool 包装 HTTP endpoint | 每个 app 独立开发 | 每个工具一份 ATD 定义 |
| **代码行数（米家灯接入）** | ~800（MCP server + iOS bridge 不可行）| ~500（REST wrap，本地控制不行）| ~1200（一整个 app）| ~120（ATD tool definition + REST binding） |
| **支持的 agent 框架** | 仅 MCP-compatible | 仅 OpenAI-style | 厂商锁定 | 任何 framework |
| **支持的 LLM** | 主要 Claude | 主要 GPT | 厂商绑定 | 任何 LLM |
| **context 开销 (100 tool)** | 30K tokens | 30K tokens | N/A | 3K tokens (Hot tier) |
| **跨平台** | 部分 | 仅 HTTP | ✗ | ✓ |
| **物理设备支持** | ✗ | △ (需中转) | ✓（单平台）| ✓（跨平台）|
| **权限模型** | 每个 host 自建 | 每个 provider 自建 | 平台系统级 | UCAN 统一 + 平台原生 |
| **工期估算（覆盖 Lily 全部需求）** | 4-6 个月，30% 覆盖 | 3-4 个月，40% 覆盖 | 12+ 月（多团队）| 6-8 周（贡献者生态化） |

**关键洞察**：不是"ATD 比其他方案好一点"，而是"**只有 ATD 让 Lily 场景成为可行的工程**"。

其他方案的覆盖率无法通过工程努力补足——它们受协议层的**结构性限制**：MCP 没有 Native SDK binding，OpenAI Functions 要求 HTTP-only，每 app agent 没有互通机制。

---

### §3.2 ATD vs 现有方案 5 维对比表

五个对决策最关键的维度：

| 维度 | Anthropic MCP | OpenAI Functions | LangChain Tools | Apple App Intents | **ATD** |
|-----|:-------------:|:---------------:|:---------------:|:-----------------:|:-------:|
| **跨 OS** | 部分（主要桌面）| ✗（仅 HTTP）| 部分（Python-only）| ✗（仅 iOS） | **✓** |
| **异构 Binding** | ✗（仅 JSON-RPC）| ✗（仅 HTTP）| ✗（Python 函数）| ✗（iOS Intent） | **✓（4 种 + 可扩展）** |
| **规模化容量** | ✗（全部装 context）| ✗（全部装 context）| ✗ | △（平台级索引）| **✓（Hot/Warm/Cold）** |
| **能力授权（协议内建）** | ✗（委托 host）| ✗（无）| ✗ | △（iOS entitlement）| **✓（UCAN capability）** |
| **多利益方可治理演化** | ✗（单厂主导）| ✗（单厂主导）| △（开源但单项目）| ✗（Apple only）| **✓（APWG）** |

**每一行的含义**：

- **跨 OS**：MCP 依赖 stdio/HTTP，移动端支持差；OpenAI 仅 HTTP；LangChain 是 Python 库不跑手机；App Intents 只在 Apple 设备
- **异构 Binding**：MCP 规定了唯一 wire format（JSON-RPC），OpenAI 规定了唯一 transport（HTTP），无法包容 Native SDK / CLI / 桌面应用
- **规模化容量**：没有一个现有方案能容纳 1000+ tool 而不撑爆 context——这是 B 痛点的根源
- **能力授权**：MCP 把授权完全委托给 host 应用；OpenAI Functions 压根无授权概念；App Intents 靠 iOS entitlement 但跨平台不适用
- **治理演化**：MCP、OpenAI Functions 都是单厂主导；LangChain 是社区但单一实现；Apple App Intents 是 Apple 独家

**ATD 是唯一一个五项全满足的候选**，而且这不是"后发优势"——是**协议分层设计的直接结果**（详见 Part 2 和独立技术规范文档）。

---

### §3.3 ANOS 参考实现：早期验证阶段的设计落地

ATD 不是纯学术提案——它已经在 ANOS 项目中作为**早期参考实现**运行，验证设计的可行性。但诚实披露：**参考实现仍处于早期阶段**，不应被理解为成熟生产系统。

**已实装并可验证（✅）**：

- **Dispatch 核心流水线**：`crates/anos-tool-dispatch/src/` — 注册、验证、路由、执行闭环
- **Circuit breaker 3 状态机**（Closed / Open / Half-Open）+ 健康监控（5 分钟滚动窗口）
- **MCP binding**：`binding_mcp.rs` — 完整 stdio JSON-RPC 2.0 客户端，MCP server zero-code 接入为 `mcp:*` 命名空间
- **REST binding**：`binding_rest.rs` — HTTP 调用 + 参数/结果映射
- **Persistent registry**：`persistent.rs` — SQLite-backed 工具持久化
- **多 LLM Provider**：Anthropic · OpenAI · Gemini · DeepSeek · Kimi · OpenRouter · CLIProxy

**设计完成 · 部分实装（⚠️）**：

- **Hot/Warm/Cold 三层容量**：设计在 Architecture §5；当前实装仅 `PersistentToolRegistry`，tier 升降级自动化逻辑待补全
- **内置工具**：当前 crate 含 ~20 个核心工具（fs / shell / web / git / docker 等），**非白皮书早期版本声称的 102**；这 102 是包含 host:* 插件、MCP 桥接工具、agent/session/session-managed tools 在内的**总生态规模**，但均通过 ATD 分发——完整清单见 ANOS 仓库
- **Host:\* 插件**：10 个 bundled JSON 定义（ffmpeg / yt-dlp / pandoc / jq / ollama / sqlite3 / imagemagick / xdg-open / notify-send / pdflatex），conditional registration 基于 binary 可用性
- **UCAN capability token**：`anos-capability` crate 存在；完整验证链路实装深度待审计

**设计文档化 · 实装未开始（❌）**：

- **Native CLI binding**：当前 CLI 工具通过 `host:*` 插件系统绕道调用 shell.exec，**并非白皮书所声称的原生 binding**（追踪：[`atd-native-cli-binding-missing.md`](../issues/2026-04-21-atd-native-cli-binding-missing.md)）
- **AppFunction binding**：Schema 字段已设计，Rust 实装 0 行（追踪：[`atd-appfunction-binding-not-started.md`](../issues/2026-04-21-atd-appfunction-binding-not-started.md)）
- **HNSW 语义发现**：`intent_examples` 字段存在，运行时仅关键词匹配（追踪：[`atd-semantic-discovery-not-connected.md`](../issues/2026-04-21-atd-semantic-discovery-not-connected.md)）
- **Pipe composition**：Typed pipe 设计完成（§8 原理展示），代码 0 行（追踪：[`atd-pipe-composition-not-implemented.md`](../issues/2026-04-21-atd-pipe-composition-not-implemented.md)）
- **Dry-run dispatch**：`supports_dry_run` 字段存在，runtime 忽略（追踪：[`atd-dry-run-not-wired.md`](../issues/2026-04-21-atd-dry-run-not-wired.md)）
- **Unified ErrorClass**：当前错误是 generic string（追踪：[`atd-error-classification-not-unified.md`](../issues/2026-04-21-atd-error-classification-not-unified.md)）

**性能指标（设计目标，benchmark suite 建设中）**：

| 指标 | 类型 | 数值 |
|-----|-----|-----|
| Dispatch 平均延迟（Step 1-8）| 设计目标 | < 5 ms |
| UCAN capability token 验证 | 设计目标 | < 1 ms (cached) |
| Hot tier (20 tools) context 占用 | 设计计算 | ~ 3K tokens |
| Warm tier HNSW p99 | 设计目标 | < 80 ms（依赖 HNSW 接通）|
| MCP server 动态注册 | 已验证 | 秒级 |
| Circuit breaker 3 状态机 | 已验证 | ✓ 稳定运行 |

> **这些数字多数是设计目标，而非 benchmark 实测值**。Benchmark suite 仍在建设中，追踪 [`atd-benchmark-suite-missing.md`](../issues/2026-04-21-atd-benchmark-suite-missing.md)。完整 gap 分析见 ANOS 内部文档 `docs/architecture/atd-overview.md §11.2`。

**诚实结论**：ATD v1.0 的**协议设计**已成熟，可作为讨论基础。**参考实装**则是进行时——核心 dispatch / circuit breaker / MCP / REST 已可用，4 binding 中只有 2 个是 native，H/W/C / HNSW / pipe / dry-run 等高级特性待实装。早期采纳者应对此有预期。

---

### §3.4 Lily 多设备闭环：v3 相对 v2 的决定性差异

v2 的 Lily 场景是单设备（iPhone）。这掩盖了 agent 时代的真实复杂度——**用户拥有的是设备"舰队"**，每个设备有独特能力。v3 把 Lily 场景扩展为跨 5 个设备类的闭环，证明 v3 primitives 足以表达这种复杂度。

#### 场景：Lily 早晨出门

05:23 — Lily 的 **Huawei Watch 4**（§8.2）检测到睡眠心率异常峰值。Watch 本地 Wear Engine 上的 health tool 直接读心率原始数据，通过 `device.preferred: [watch]` 在本设备执行（低延迟、高精度），事件写入 session `lily-health-ambient`。

07:02 — Watch 根据 session 规则推送到配对的 **Mate 80 Pura**（§8.1）。phone agent 基于 `session.handoff(trigger=auto_on_event)` 接手 session，运行 `health.analyze_anomaly` 决策工具。分析结论：建议今天提前 30 分钟看医生。phone agent 跑 `calendar.reschedule` 工具，push `user.ask` 给 Lily 确认。

07:14 — Lily 从卫生间走向车库。**FreeBuds Pro 5**（§8.3）此时已戴上，phone agent 通过 earbuds 的 audio-stream 控制平面调 `audio.tts.speak`（Lily 听到："需要提前半小时看 Dr. Wang，已帮你改好日程——同意？"）。Lily 说"同意"，耳机 mic 捕捉语音，经 `result_middleware.prompt_injection_scan` 扫描后返回 `Confirmed`。

07:20 — Lily 进入 **Aito M9 车机**（§8.6）。车机 HarmonySpace 6 与 phone 通过 DSoftBus 识别为 Super Device。`session.handoff(trigger=proximity)` 把 `lily-health-ambient` session 迁到车机。车机 agent 运行 `navigation.route_to` tool——声明 `safety.driving_constraint: safe_always`，dispatch 允许。ADS 5.0 接管驾驶。

07:35 — 到达医院停车场，driving 状态结束。车机 `session.fork(target_device=phone)` 分叉 session 到 phone——Lily 下车后 phone 继续接管。

08:45 — 问诊结束，返回 **MateBook PC**（§8.5）。PC 上 ATD client 从 session id 拉取完整历史，运行 `health.generate_report` 工具输出 PDF，存入 `output_hint.prefer_display: [large_screen]` → PC 直接渲染，完整报告。

#### 这个闭环用到的 v3 primitives

| 能力 | 使用的 v3 概念 | §引用 |
|------|---------------|------|
| Watch 本地读心率 | `device.preferred: [watch]` + capability tag `heart_rate` | §2.5 |
| Watch → Phone 事件迁移 | `session.handoff(trigger=auto_on_event)` | §2.6 |
| Phone → Earbuds 语音交互 | Audio-stream control plane + `device_type: earbuds` | §2.5 / §8.3 |
| 耳机结果 injection scan | `result_middleware.prompt_injection_scan` | §2.7 |
| Phone → Car proximity handoff | `session.handoff(trigger=proximity)` + DSoftBus | §2.6 / Appendix I |
| Car 驾驶安全门 | `safety.driving_constraint: safe_always` | §8.6 |
| Car → Phone 分叉 | `session.fork()` | §2.6 |
| PC 大屏优化渲染 | `output_hint.prefer_display: [large_screen]` | §8.5 |

#### 在 v2 里同样的场景——讲得出但写不完

v2 的 tool definition schema **没有设备类概念**、没有 session migration、没有 middleware、没有 driving_constraint、没有 display hint。Lily 多设备闭环要么靠 agent 层 hack（每个 tool 包装 if/else device detection），要么让工具作者为每设备单独建一份 tool——正好是 v2 要消灭的碎片化问题。

**v3 的测试**：上述 8 种能力**全部用 schema 字段表达**，dispatch 层按字段路由。这是 v3 相对 v2 的决定性证明。

---

## §4. 行动：你现在能做什么

### §4.1 按身份选路径（决策树）

ATD 不要求你做"all-in"的承诺。根据你的身份，有 6 条**差异化参与路径**，每条的成本和收益都明确：

| 你是谁？ | 建议路径 | 承诺 | 获得 |
|---------|---------|-----|-----|
| **大厂 / 云厂商 CTO** | Founding Adopter + APWG | 在你的 agent 产品支持 ATD binding | APWG 治理话语权 + 生态标准制定权 |
| **中型 agent 产品团队 VP** | Founding Adopter + Pilot | 一个产品线集成 ATD | 品牌署名 + 早期红利 |
| **开源项目维护者** | Reference Binding | 维护一个语言 SDK | 该语言生态事实标准地位 |
| **垂直领域企业（IoT/医疗/金融）** | Vertical Binding + Pilot | 维护 `vendor:xxx` 命名空间 | 垂直领域 ATD 主导权 |
| **创业公司 CEO** | Pilot Integration | 一个内部 PoC（6-12 周）| 低风险评估，不锁死 |
| **学术研究者** | 学术合作 | 针对开放问题发表论文 | 学界影响力 + 合作身份 |
| **标准化组织代表** | APWG + Founding Adopter | 参与治理筹建 | 行业标准合法性贡献 |

### §4.2 每条路径的第一步

**Path 1 — Founding Adopter Program**

- 目标：前 10-15 家承诺采纳的组织进入 Founding Adopter 名单
- 第一步：发邮件到 `founding-adopters@atd-protocol.org` 表达意向
- 承诺：在你的 agent 产品中至少实现一个 ATD binding（6-12 个月内）
- 获得：v1.0 规范 finalize 投票权 + v1.1 设计参与权 + 署名于白皮书 v2.1

**Path 2 — Reference Binding 贡献**

- 征集：TypeScript / Python / Swift / Kotlin / Go / Java / C# 的官方参考 SDK
- 第一步：GitHub 提 Issue 声明意向，fork 模板仓库
- 承诺：maintain 一个语言的 SDK（至少 12 个月），保证 conformance test 通过
- 获得：该语言生态的事实标准地位，贡献可计入公司技术品牌

**Path 3 — Vertical Binding 贡献**

- 征集：特定领域的 binding 集合——`vendor:huawei:*` / `vendor:xiaomi:*` / `vendor:alibaba:*` / 企业 SaaS / 医疗 IoT / 金融等
- 第一步：提案你的垂直领域 namespace 申请（format 详见开发版 §9.4）
- 承诺：维护该命名空间下至少 20 个 ATD tools
- 获得：该垂直领域 ATD 的事实主导权

**Path 4 — Pilot Integration（最低门槛）**

- 目标：降低决策门槛，让你**不必承诺**任何事就能先试
- 第一步：克隆 ANOS 参考实现，选一个内部 agent 场景做 PoC
- 时间：6-12 周
- 获得：真实 ROI 数据 + 团队对 ATD 的第一手经验

**Path 5 — APWG（Agent Protocol Working Group）筹建**

- 目标：2026 Q4 启动多利益方治理
- 第一步：提名一位技术代表，表达参与 steering 意向
- 承诺：一位 senior engineer 每月 8-16 小时参与治理讨论
- 获得：规范治理话语权 + 与其他厂商的直接对接渠道

**Path 6 — 学术合作**

- 开放问题列表：形式化验证 / 语义发现鲁棒性 / 类型化工具组合 / 实时延迟预算 / 治理可持续性等
- 第一步：选一个开放问题，联系 `research@atd-protocol.org`
- 获得：论文合作 + ANOS 作为 benchmark system 的使用权

---

### §4.3 愿景：2030 的 ATD 世界

回到 Lily。**2030 年，她的个人助理应该是这样的**：

Lily 换了一部新手机——从 iPhone 15 换成 Mate 80 Ultra（HarmonyOS 6）。她不需要重新连接任何设备，不需要重新授权任何服务。她的 agent（可能是 Claude 7 驱动，也可能是某个新模型）通过 ATD 协议发现新设备的原生能力（HMS AppFunction binding），同时继续使用云端的跨平台服务（Google Calendar REST binding，微信 WeChat API binding）。

开发者生态里：
- 米家官方发布了 `vendor:xiaomi:*` 命名空间下 200+ 工具
- 华为官方发布了 `vendor:huawei:*` 命名空间
- Anthropic 维护了 `mcp:*` 桥接的官方 ATD 客户端
- Apple 为 App Intents 发布了 ATD binding（iOS 19）
- Android 为 AppFunctions 发布了 ATD binding（Android 18）
- 独立开发者可以在一个周末接入任何 agent 框架

Agent 开发者不再需要问"我应该选 MCP 还是 OpenAI Functions"——正如 1998 年的 C 开发者不再问"我应该选 BSD 还是 System V"。**POSIX 让 C 程序可移植，ATD 让 agent 工具可互操作。**

ATD 成功的标志，不是它被大家谈论，而是它被**不假思索地依赖**。

**这是 2030。需要你从 2026 的今天开始参与。**

---

# Part 2 — 给开发者

## §5. 共同地基：5 分钟理解 ATD

### §5.1 五个核心概念

无论你是 MCP 作者、agent 框架作者、移动端开发者还是企业工具 owner——ATD 的五个核心概念是共同的地基：

**概念 1：Tool Definition（工具定义）**

一份 JSON，同时定义工具的**语义**和**绑定实现**：

```json
{
  "atd_version": "1.0",
  "id": "anos:fs.read",
  "capability": {
    "domain": "filesystem",
    "actions": ["read"],
    "intent_examples": ["读取文件", "read a file"]
  },
  "input":  { ... },
  "output": { ... },
  "bindings": { "cli": {...}, "rest": {...} },
  "safety": { "level": "read" }
}
```

> 形象类比：Tool Definition 就像 Unix 的 man page + syscall signature 的结合——既描述做什么，又描述怎么调。

**概念 2：Binding（绑定）**

一个工具可以有多种实现方式，每种叫一个 binding。ATD v1.0 定义四种：

| Binding | 用来包装 | 语言生态 |
|---------|---------|---------|
| `cli` | 本地命令行工具（ffmpeg, git, jq）| 任何语言 |
| `mcp` | MCP server | TS / Python 主流 |
| `rest` | HTTP API | 任何语言 |
| `appfunction` | Native SDK（iOS / Android / HarmonyOS）| Swift / Kotlin / ArkTS |

Dispatch 层根据 platform 和可用性**自动选择**最合适的 binding。

**概念 3：Dispatch（调度）**

Agent 调用工具时，ATD Dispatch Layer 走一个 8 步流水线：

```
Agent call 
  → [1] Capability check   (UCAN token 验证)
  → [2] Resolve tool        (找到定义 + 候选 binding)
  → [3] Validate params     (JSON Schema 验证)
  → [4] Rate limit / Circuit breaker
  → [5] Route binding       (选最优 binding)
  → [6] Execute in sandbox  (实际调用)
  → [7] Normalize result    (binding 特定 → ATD 统一格式)
  → [8] Audit + Return
```

> 形象类比：Dispatch 层就像操作系统的 syscall dispatcher，把用户的抽象调用路由到具体的实现。

**概念 4：Capability Token（能力令牌）**

基于 UCAN 1.0 的能力令牌，是 agent 调用工具的"钥匙"：

```json
{
  "subject": { "agent_id": "agent:lily-assistant:42" },
  "resource": "tool:anos:fs.*",        
  "constraints": {
    "methods": ["read"],                  
    "rate_limit": { "max": 60, "window_secs": 60 },
    "safety_max": "read"                  
  },
  "validity": { "expires_at": "2026-04-20T12:00:00Z" }
}
```

> 形象类比：Capability Token 就像 Unix 的 file descriptor——不可伪造、可委托、可撤销。

**概念 5：Tier（容量分层）**

100+ 工具全装入 context 会撑爆。ATD 的 Tier 系统把工具分三层：

| Tier | 数量上限 | Context 占用 | 发现延迟 |
|-----|---------|-------------|---------|
| **Hot** | 20 | ~3K tokens | 0（在 system prompt）|
| **Warm** | 200 | ~0 | <80ms（本地 HNSW 索引）|
| **Cold** | ∞ | 0 | <500ms（远程 registry）|

基于调用频率自动升降级。Agent 启动时只有 Hot 工具在 context，Warm 靠 `tool.search(intent)` 主动发现。

---

### §5.2 Hello World：最小可运行的 ATD tool

**15 行 JSON，一个工具定义：**

```json
{
  "atd_version": "1.0",
  "id": "demo:echo.hello",
  "name": "Echo Hello",
  "description": "返回问候",
  "capability": {
    "domain": "demo",
    "actions": ["echo"],
    "intent_examples": ["say hello", "问好"]
  },
  "input":  { "type": "object", "properties": { "name": { "type": "string" } } },
  "output": { "type": "object", "properties": { "message": { "type": "string" } } },
  "bindings": {
    "cli": {
      "binary": "echo",
      "args_template": "Hello {name}",
      "result_parser": "text"
    }
  },
  "safety": { "level": "read" }
}
```

**运行（假设你已安装 ATD runtime）**：

```bash
# 注册工具
$ atd register demo-echo-hello.json
✓ Registered: demo:echo.hello (v1.0)

# 调用
$ atd call demo:echo.hello '{"name": "Lily"}'
{
  "status": "success",
  "data": { "message": "Hello Lily" },
  "metadata": { "binding_used": "cli", "latency_ms": 3 }
}
```

**你刚刚做了什么**：

1. 定义了一个工具的语义（capability）和类型（input/output schema）
2. 声明了一种绑定（CLI，把 `echo` 命令包装）
3. ATD runtime 帮你做了参数验证、执行、结果规范化、audit log

5 分钟，你完成了从"我有一个 idea"到"agent 可以调用"的全流程。

---

### §5.3 Lily 场景的技术拆解

Part 1 的红线场景——"明早 7 点开灯，昨晚睡眠怎么样？"——涉及四类开发者协作：

```
用户意图
    ↓
┌──────────────────────────────────────────┐
│ Agent Framework (§7)                     │
│ 决定：意图 → 需要哪些工具               │
└──────────┬─────────────────┬────────────┘
           │                 │
           │                 │
    ┌──────▼──────┐    ┌─────▼────────┐
    │ 米家 AppFunc │    │ Huawei REST  │
    │ binding (§8) │    │ binding (§9) │
    │ 或 MCP (§6)  │    │              │
    └──────┬──────┘    └─────┬────────┘
           │                 │
      米家智能灯         华为睡眠 API
      (云端 API)         (cloud.huawei.com)
```

- **§6 MCP Server 作者**：如果米家想提供一个通用 MCP server，教他们再加一份 ATD manifest
- **§7 Agent 框架作者**：如果你在做 Lily 的 agent，教你消费 ATD tool
- **§8 移动应用开发者**：如果米家 / 华为想暴露 native SDK 给 agent，教他们做 AppFunction binding
- **§9 企业工具 owner**：如果你的企业有内部 REST API，教你生成 ATD tool definition

四类开发者在同一个 Lily 场景里协作，每人做自己擅长的部分。

---

## §6. 如果你是 MCP Server 作者

### §6.1 现状：你的 MCP server 只能被 Claude Code 调用

你已经写了一个不错的 MCP server——比如一个米家智能家居的通用控制器。它支持：
- 发现家里的米家设备
- 控制灯光、空调、窗帘
- 查询设备状态

但它只能被 **MCP-compatible host** 调用——主要就是 Claude Code、某些 Cursor 配置、一些实验性 agent。你的其他潜在用户——使用 GPT-5 的 agent、Gemini CLI 的开发者、移动端的 agent 应用——**都用不了你的 server**。

**痛点**：你投入的工程被锁在 MCP 生态里。

---

### §6.2 5 分钟迁移：加一个 ATD manifest

ATD 对 MCP 是**加法而非替换**。你不需要改动现有 MCP server 一行代码——只需要额外提供一份 **ATD manifest**，声明你的 server 如何通过 MCP binding 被 ATD 发现和调用。

**Step 1：在 MCP server 目录下新增 `atd-manifest.json`**

```json
{
  "atd_manifest_version": "1.0",
  "server": {
    "name": "xiaomi-smarthome-mcp",
    "version": "2.1.0",
    "transport": "stdio",
    "command": "node",
    "args": ["dist/server.js"]
  },
  "tools": [
    {
      "mcp_tool_name": "discover_devices",
      "atd_id": "vendor:xiaomi:device.discover",
      "capability": {
        "domain": "smart_home",
        "actions": ["discover"],
        "intent_examples": ["查找家里的智能设备", "find smart devices"]
      },
      "safety": { "level": "read" }
    },
    {
      "mcp_tool_name": "turn_on_light",
      "atd_id": "vendor:xiaomi:light.turn_on",
      "capability": {
        "domain": "smart_home.light",
        "actions": ["turn_on"],
        "intent_examples": ["打开客厅的灯", "turn on bedroom light"]
      },
      "safety": { "level": "write", "side_effects": ["physical_device_state"] }
    }
  ]
}
```

**Step 2：在 MCP server 启动时暴露 manifest（两种方式，二选一）**

方式 A——HTTP endpoint（推荐）：

```typescript
// server.ts（TypeScript + @modelcontextprotocol/sdk）
import express from 'express';
import manifest from './atd-manifest.json';

const app = express();
app.get('/.well-known/atd-manifest.json', (req, res) => {
  res.json(manifest);
});
app.listen(3001);

// MCP server 继续按原样启动
startMcpServer();
```

方式 B——MCP 扩展方法：

```typescript
// 在 MCP server 注册一个 extension method
server.setRequestHandler('atd/manifest', async () => manifest);
```

**Step 3：让 ATD runtime 发现你的 server**

ATD runtime（如 ANOS）通过 `atd mcp add` 命令注册：

```bash
$ atd mcp add node dist/server.js --manifest http://localhost:3001/.well-known/atd-manifest.json
✓ Registered MCP server: xiaomi-smarthome-mcp
✓ Imported 8 tools:
  - vendor:xiaomi:device.discover
  - vendor:xiaomi:light.turn_on
  - vendor:xiaomi:light.turn_off
  - vendor:xiaomi:ac.set_temperature
  - ...
```

**完成。你的 MCP server 现在：**
- ✓ 对 Claude Code 继续工作（原有功能不变）
- ✓ 对任何 ATD-compatible agent 也工作（通过 MCP binding）
- ✓ 自动继承 ATD 的 capability 授权和 rate limit
- ✓ 在 Hot/Warm/Cold tier 里根据使用频率自动升降级

---

### §6.3 实战：米家灯 MCP server → Lily 场景接入

完整的迁移示例，用 TypeScript + MCP SDK：

```typescript
// xiaomi-smarthome-server/src/index.ts
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import express from 'express';
import manifest from '../atd-manifest.json';

const server = new Server({ name: "xiaomi-smarthome", version: "2.1.0" });

// 原有 MCP tool 定义不变
server.setRequestHandler('tools/list', async () => ({
  tools: [
    { name: "turn_on_light", inputSchema: { /* ... */ } },
    { name: "discover_devices", inputSchema: { /* ... */ } }
  ]
}));

server.setRequestHandler('tools/call', async (req) => {
  const { name, arguments: args } = req.params;
  if (name === "turn_on_light") {
    return await turnOnLight(args.device_id, args.brightness);
  }
  // ... 其他 tool
});

// 新增：ATD manifest endpoint
const httpApp = express();
httpApp.get('/.well-known/atd-manifest.json', (_, res) => res.json(manifest));
httpApp.listen(3001, () => {
  console.log('ATD manifest available at :3001/.well-known/atd-manifest.json');
});

// 启动 MCP stdio transport
await server.connect(new StdioServerTransport());
```

**Lily 场景在你的 server 上如何被调用**：

```
Lily 的 agent (任意 LLM 驱动)
    ↓
    用户说 "打开客厅的灯"
    ↓
ATD Client SDK 匹配 intent_examples → vendor:xiaomi:light.turn_on
    ↓
ATD Dispatch Layer：
    - Capability check ✓
    - Binding selection: 选 MCP binding (你的 server)
    - 调用 JSON-RPC: tools/call { name: "turn_on_light", arguments: { device_id: "living_room_light", brightness: 80 } }
    ↓
你的 MCP server 收到调用，执行原有逻辑
    ↓
返回结果经 ATD Dispatch 规范化
    ↓
Lily 看到："✓ 客厅灯已开启"
```

**你的 MCP server 的代码完全没变**，新增的只是一份 manifest 文件。

---

### §6.4 迁移 FAQ

**Q1：我的 MCP server 还是被 Anthropic 的改动影响吗？**

依然会。但通过 ATD，你的 server 也可以被其他 agent 调用，降低了对 Anthropic 的依赖度。MCP 作为一个传输协议继续演化，ATD 保证你的工具在协议演化期间仍可被广泛使用。

**Q2：如果 MCP 升级到 v2.0 不兼容，ATD 怎么办？**

ATD 的 `mcp` binding 支持版本协商。你可以在 manifest 里声明 `"mcp_version": "1.0"` 或 `"2.0"`，ATD runtime 自动选择兼容的调用方式。

**Q3：我的 MCP server 里有 resources 和 prompts，不是只有 tools，怎么办？**

ATD v1.0 只覆盖 tools。Resources 和 prompts 继续通过原生 MCP 协议使用（ATD 不干扰）。未来 ATD v1.1/v1.2 可能扩展 resource binding。

**Q4：ATD manifest 和 MCP 的 `tools/list` 冗余吗？**

是的，有一定冗余。这是设计上的权衡——manifest 包含**语义注释**（intent_examples, capability, safety），这些是 MCP 原生 schema 不提供的。如果你的 `tools/list` 已经很详细，manifest 可以只补充 ATD 特有字段（intent_examples / safety level）。

**Q5：需要注册到某个中心化 registry 吗？**

不需要。ATD 是**联邦化**协议——你的 manifest 可以托管在任何 URL。用户通过 `atd mcp add` 指定 URL 即可发现。未来 APWG 可能提供公共 registry 作为**可选**发现渠道。

---

## §7. 如果你是 Agent 框架作者

### §7.1 现状：每个框架在发明自己的 tool 抽象

你在做一个 agent 框架——可能是基于 LangChain fork、基于 Claude Code 二开、或是从零自建。你遇到了每个框架作者都会遇到的问题：

```python
# LangChain 的 tool
from langchain.tools import BaseTool

class MyTool(BaseTool):
    name = "my_tool"
    def _run(self, *args): ...

# 但你的用户想接入一个 MCP server
# 还有一个 OpenAI Functions 格式的 tool
# 还有一个原生 REST API
# 还有 Apple App Intents

# 结果：你的框架里需要 4 套 adapter
# 每新增一个工具生态，就要加一套
```

这是 v1 §1.3 痛点 D 的直接表现：**协议碎片化**让你的框架不得不为每种工具生态写适配器。

---

### §7.2 15 分钟：用 ATD Client SDK

与其写 N 套 adapter，不如消费 ATD 这一个协议——所有工具生态都自动可用。

**Step 1：在项目加 ATD client SDK**

```bash
# TypeScript / JavaScript
$ npm install @atd-protocol/client

# Python
$ pip install atd-client

# Rust
$ cargo add atd-client
```

**Step 2：最小 API**

TypeScript 示例：

```typescript
import { AtdClient } from '@atd-protocol/client';

// 初始化 client（连接到 ATD runtime，比如 ANOS 守护进程）
const client = new AtdClient({ endpoint: 'unix:///tmp/atd.sock' });

// 发现工具（Hot + Warm tier）
const tools = await client.tools.list();
// -> 返回所有 Hot 工具的 Compact ATD + Warm 工具的索引 hint

// 按意图搜索（Warm tier semantic search）
const matches = await client.tools.search({
  intent: "open the living room light"
});
// -> [{ tool_id: "vendor:xiaomi:light.turn_on", score: 0.94 }, ...]

// 调用工具
const result = await client.tools.call({
  tool_id: "vendor:xiaomi:light.turn_on",
  params: { device_id: "living_room_light", brightness: 80 }
});
// -> { status: "success", data: {...}, metadata: {...} }
```

**Step 3：集成到你的 agent loop**

```typescript
async function agentTurn(userMessage: string) {
  // 1. 把 Hot tools 装入 LLM system prompt
  const hotTools = await client.tools.hot();
  const systemPrompt = buildSystemPrompt(hotTools);

  // 2. LLM 做 tool selection
  const llmResponse = await llm.chat({
    system: systemPrompt,
    user: userMessage,
    tools: hotTools.map(t => toLLMToolSchema(t)) // LLM 特定格式
  });

  // 3. 如果 LLM 请求一个 Warm tier 工具（通过 tool.search）
  if (llmResponse.tool_calls?.some(c => c.name === "tool.search")) {
    const searchResults = await client.tools.search({
      intent: llmResponse.tool_calls[0].arguments.intent
    });
    // 把 search 结果喂回 LLM，让它 pick 一个
    return agentTurn(userMessage + `\nAvailable tools: ${JSON.stringify(searchResults)}`);
  }

  // 4. 执行 LLM 请求的 tool calls
  const results = await Promise.all(
    llmResponse.tool_calls.map(call => client.tools.call({
      tool_id: call.name,
      params: call.arguments
    }))
  );

  // 5. 把结果喂回 LLM 做下一轮
  return continueAgentTurn(llmResponse, results);
}
```

**你得到什么**：

- ✓ 用户可以用**任何** ATD-compatible 工具（MCP server / CLI / REST / Native SDK）
- ✓ 工具自动按频率进入 Hot/Warm tier，context 成本可控
- ✓ 并行执行、rate limit、circuit breaker 全由 ATD Dispatch 处理
- ✓ 你的框架只需要做 LLM 交互和 UX，不需要写 N 套工具适配器

---

### §7.3 实战：Lily 场景在你的框架里

假设你在做一个开源的 personal assistant framework，Lily 是你的用户：

```typescript
// lily-assistant/src/agent.ts
import { AtdClient } from '@atd-protocol/client';
import { Anthropic } from '@anthropic-ai/sdk'; // 也可以是 OpenAI / Google

const atd = new AtdClient({ endpoint: 'unix:///run/atd/atd.sock' });
const llm = new Anthropic({ apiKey: process.env.ANTHROPIC_API_KEY });

async function handleMessage(userMessage: string) {
  // 1. 获取当前 agent 的 Hot tools (20 个最常用的)
  const hotTools = await atd.tools.hot({ agent_id: 'lily-assistant' });

  // 2. 调用 LLM
  const response = await llm.messages.create({
    model: 'claude-opus-4-7',
    system: `你是 Lily 的个人助理。你有以下工具可用：
${hotTools.map(t => `- ${t.id}: ${t.description}`).join('\n')}

另外，你可以用 'tool.search' 发现更多工具。`,
    messages: [{ role: 'user', content: userMessage }],
    tools: hotTools.map(toAnthropicTool)
  });

  // 3. 处理 tool_use
  for (const block of response.content) {
    if (block.type === 'tool_use') {
      const result = await atd.tools.call({
        tool_id: block.name,
        params: block.input
      });
      // 把结果喂回 LLM（略）
    }
  }
}

// 用户输入
await handleMessage("明早 7 点开灯，昨晚睡眠怎么样？");
```

**这段代码里发生了什么**：

1. 你的框架只关心 agent loop 和 LLM 交互
2. 工具的发现、授权、执行、错误处理全交给 ATD Client SDK
3. Lily 作为用户，可以通过 `atd tool install vendor:xiaomi:*` 和 `atd tool install vendor:huawei:*` 把具体工具接入
4. 你的框架天然支持未来任何新的 ATD tool 而不需要改代码

---

### §7.4 进阶：Tier 和 Capability 集成

如果你的框架有特别高级的需求——自主控制工具可见性、自定义 tier 策略、细粒度 capability：

**自定义 tier 策略**：

```typescript
// 某些工具你想强制 Hot（比如你的框架核心工具）
await atd.tools.pin({
  tool_id: 'lily-assistant:memory.recall',
  tier: 'hot'
});

// 某些工具限制在 Cold tier（比如实验性工具）
await atd.tools.setTier({
  tool_id: 'experimental:*',
  tier: 'cold'
});
```

**Capability token 精细授权**：

```typescript
// 派生一个受限 token 给 sub-agent
const subAgentToken = await atd.capabilities.attenuate({
  parent_token: currentToken,
  resource: 'tool:anos:fs.read',  // 只允许读文件
  rate_limit: { max: 10, window_secs: 60 },
  expires_at: new Date(Date.now() + 5 * 60 * 1000) // 5 分钟有效
});

// sub-agent 用这个受限 token 调用
const subClient = new AtdClient({ 
  endpoint: 'unix:///run/atd/atd.sock',
  token: subAgentToken 
});
```

---

## §8. 如果你是设备开发者（Phone / Watch / Earbuds / Tablet / PC / Car / TV）

v3 把原 "移动应用开发者" 章节扩展为**按设备类分七个小节**。每个设备类有独特的硬件、UI 模态、连接方式和能力约束，因此有独特的 binding 和 tool 模式。

**每个子节共享的 6 元素模板**：
1. 特征约束（compute / UI / connectivity / 独特 sensors）
2. 可用 binding（vendor-specific 枚举）
3. 典型 tool 模式
4. Integration quickstart
5. Capability token 考量
6. 常见陷阱 + Lily 场景中的角色

> **注**：v3 的 Device Type 和 capability tags **不绑定任何 vendor**——虽然本章大量引用华为生态（HMS / HarmonyOS），但 Apple / Google / Samsung / Garmin / 小米 / OPPO 等生态的 binding 同样在 ATD 范围内。示例取 HMS 是因为它**覆盖所有 7 个设备类**（业界唯一），是演示多 binding 的最佳教材。

---

### §8.1 Phone（手机）— baseline

#### 特征约束
- 全屏 UI、全算力、始终在线（多数时间）
- 是用户 agent 的 "主设备"——其他设备常以 companion 模式连到这里
- 传感器丰富：camera / microphone / gps / accelerometer / NFC / 生物识别

#### 可用 binding

| vendor | binding | SDK |
|--------|---------|-----|
| Apple | AppFunction `platform: ios` | App Intents (Swift) |
| Google Android | AppFunction `platform: android` | AppFunctions (Kotlin) |
| HarmonyOS | AppFunction `platform: harmonyos` | Intents Kit (ArkTS) |
| Web | REST binding | 你的 REST API |

#### 典型 tool 模式

```json
{
  "id": "vendor:yourcompany:health.steps.get",
  "device": { "preferred": ["phone"] },
  "capability": { "domain": "health.activity" },
  "input": { "type": "object", "properties": { "date": { "type": "string" } } },
  "output": { "type": "object", "properties": { "steps": { "type": "integer" } } },
  "bindings": {
    "appfunction": [
      { "device_type": "phone", "vendor": "apple", "platform": "ios",
        "target": { "bundle_id": "com.yourcompany.app", "intent_name": "GetTodayStepsIntent" } },
      { "device_type": "phone", "vendor": "google", "platform": "android",
        "target": { "package": "com.yourcompany.app", "class": "HealthFunctions", "function": "getTodaySteps" } },
      { "device_type": "phone", "vendor": "huawei", "platform": "harmonyos",
        "target": { "bundle_name": "com.yourcompany.app", "ability": "StepsAbility", "action": "getTodaySteps" } }
    ]
  }
}
```

#### Integration quickstart（15 min）

iOS 端（App Intents）：

```swift
import AppIntents

struct GetTodayStepsIntent: AppIntent {
    static var title: LocalizedStringResource = "Get Today's Steps"
    @Parameter(title: "Date", default: Date()) var date: Date
    func perform() async throws -> some IntentResult & ReturnsValue<Int> {
        let healthStore = HKHealthStore()
        let steps = try await fetchStepCount(for: date, store: healthStore)
        return .result(value: steps)
    }
}
```

HarmonyOS 端（ArkTS Ability）：

```typescript
import { Ability } from '@ohos.app.ability';
import { health } from '@hms.health.core';

export default class StepsAbility extends Ability {
  async onCall(want) {
    const date = want.parameters.date ?? new Date().toISOString();
    return { steps: await health.getSteps({ date }) };
  }
}
```

Android 端（AppFunctions）：

```kotlin
@AppFunction
fun getTodaySteps(date: String = LocalDate.now().toString()): Int {
    return Fitness.getHistoryClient(context, account)
        .readData(DataReadRequest.Builder()
            .aggregate(DataType.TYPE_STEP_COUNT_DELTA)
            .bucketByTime(1, TimeUnit.DAYS)
            .setTimeRange(startOfDay, endOfDay, TimeUnit.MILLISECONDS)
            .build()).await()
        .let(::extractSteps)
}
```

#### Capability token 考量
- Platform permission（iOS Health / Android Permissions / HarmonyOS）与 ATD capability token **叠加**——两者缺一不可
- Tool `safety.data_sensitivity: health_private` 触发 UCAN constraint 要求 token 附带 "health 域" 授权

#### 陷阱
- Apple App Intents 要求 `title` 国际化（LocalizedStringResource），schema 里要声明多语言
- Android AppFunctions 的 `@AppFunction` 注解在 API 34+，旧版需 polyfill
- HarmonyOS 5 NEXT 不兼容 Android SDK 调用路径，必须用 ArkTS

#### Lily 场景中的角色
Phone 是 **agent 本体** + **决策中枢**。§3.4 Lily 场景中 Mate 80 Pura 接手 watch 的 anomaly 事件后跑 `health.analyze_anomaly` 和 `calendar.reschedule`——phone 是运行 tool 最多的设备类。

---

### §8.2 Watch（手表）— 传感器密集 + companion 约束

#### 特征约束
- **小屏幕或无屏幕**（band）、电池敏感、算力受限
- 独特传感器：`heart_rate`, `ecg`, `spo2`, `skin_temp`, `accelerometer`, `gps`（部分）
- 连接方式：BT（始终配对 phone）、LTE（独立但耗电）、NearLink（华为）
- **Companion 模式**：大多数 tool 在 phone 端执行，watch 只传感或显示

#### 可用 binding

| vendor | binding | SDK |
|--------|---------|-----|
| Apple Watch | WatchKit + HealthKit | Swift |
| Huawei Watch | AppFunction `device_type: watch, platform: harmonyos_lite` | Wear Engine (ArkTS) |
| Garmin | SDK via Connect IQ | Monkey C |
| Wear OS (Google) | Tiles / Complications + AppFunctions | Kotlin |

#### 典型 tool 模式

```yaml
id: health.heart_rate.get
device:
  preferred: [watch]
  fallback: [phone]   # phone 可读手表同步来的数据
  requires:
    sensors: [heart_rate]
bindings:
  appfunction:
    - device_type: watch
      vendor: huawei
      platform: harmonyos_lite
      kit: wear_engine
      ability: HealthDataProvider
      action: getCurrentHeartRate
    - device_type: watch
      vendor: apple
      platform: watchos
      framework: HealthKit
      entity: HKQuantitySample
      query: heartRate
```

#### Integration quickstart（20 min）

Huawei Wear Engine（HarmonyOS Lite，ArkTS）：

```typescript
import wearEngine from '@hms.health.wear-engine';

export class HeartRateProvider {
  async getCurrent() {
    const reading = await wearEngine.healthData.queryLatest('heart_rate');
    return { bpm: reading.value, timestamp: reading.ts };
  }
}
```

Apple Watch（WatchKit + HealthKit）：

```swift
import HealthKit

func getCurrentHeartRate() async throws -> Int {
    let store = HKHealthStore()
    let type = HKQuantityType(.heartRate)
    let query = HKSampleQuery(sampleType: type, predicate: nil,
                              limit: 1, sortDescriptors: [...]) { _, samples, _ in
        // 返回最新 BPM
    }
    store.execute(query)
}
```

#### Capability token 考量
- Watch 本地执行时，token 必须授权 `health.vitals.read` + 特定 device `did:device:xxx:watch:...`
- Companion 模式：token 通过 UCAN delegation 从 phone session 派生（watch 不独立持有 token）

#### 陷阱
- **电池**：长轮询读心率会耗尽电池——tool 设计应优先 push 而非 poll
- **连接断开**：BT 掉线时 phone 读不到 watch——dispatch 的 `fallback: [phone]` 会退化为"从云读同步过的旧数据"
- Wear OS Tiles 不能直接接受 complex JSON 参数——只能接受 primitive，tool schema 要配合

#### Lily 场景中的角色
Watch 是 **ambient 传感器**。§3.4 中 Watch 4 检测心率异常，是整个闭环的触发点。

---

### §8.3 Earbuds（耳机）— 音频 + 无 UI 的控制平面

#### 特征约束
- **无屏幕**——输出是 audio / haptic（某些型号），输入是 voice / 手势 / 触控
- 极小算力（某些有 on-device DNN 如华为 MIMO，Apple H2）
- 连接：BT、NearLink（华为）、专有（AirPods Lightning peer）

#### 可用 binding

| vendor | binding | 说明 |
|--------|---------|------|
| Huawei FreeBuds | AppFunction `device_type: earbuds, vendor: huawei` + Audio Connect SDK | 控制平面 only |
| Apple AirPods H2 | 通过 iPhone/iPad bridging（无独立 SDK） | H2 芯片可运行 on-device 模型 |
| 通用 BLE earbuds | Generic BLE GATT profile | 有限功能 |

#### v3 关键设计：只做控制平面

ATD **不是流媒体协议**。耳机的 tool 调用 **只控制**（设置降噪模式、切歌、读取 head-tracking 姿态、触发 ANC 校准），**不传输音频数据本身**。音频流通过 BT/NearLink 独立通道（A2DP / LDAC / L2HC），ATD 只下命令。

```yaml
id: freebuds.noise.set_mode
device:
  preferred: [earbuds]
  requires:
    ui_modes: [voice]   # 实际是 no-screen；这里 voice 表示能接受 voice 输入
bindings:
  appfunction:
    - device_type: earbuds
      vendor: huawei
      model: freebuds_pro_5
      transport: nearlink
      service: AudioControlService
      characteristic: ANCMode
  # 控制平面 only；音频数据走 NearLink 音频流，ATD 不经手
```

#### Integration quickstart（30 min）

这 30 分钟多花在理解 "ATD 不管音频流" 的心智切换。Tool 设计以**控制命令**为主：

- `earbuds.set_volume(level: 0-100)` — 单次 set
- `earbuds.set_noise_mode(mode: off | noise_cancel | transparency)` — 状态切换
- `earbuds.query_battery() → {left, right, case}`
- `earbuds.tts.speak(text)` — phone 合成音频，earbuds 播放（音频流经 BT，命令经 ATD）

#### Capability token 考量
- 耳机几乎没有 "写" 操作（ANC 切换不算 dangerous）
- 大多数 tool `safety: read` 或 `safety: write_non_persistent`
- 不需要复杂 token chain

#### 陷阱
- **不要设计 "实时音频分析" tool**——ATD 不是 streaming protocol。真要做，用 app 级 WebRTC/媒体流。
- 耳机常处于"单只在用"状态（一只戴一只在盒里），tool 应对"部分 fleet 可用"优雅降级
- Apple AirPods 的 H2 功能 **对第三方 API 基本关闭**，靠 iPhone bridging

#### Lily 场景中的角色
Earbuds 是 **零屏交互通道**。§3.4 中 Lily 在走动过程中通过 FreeBuds 接收 TTS 通知并语音确认——无须看屏幕、不打断手头任务。

---

### §8.4 Tablet（平板）— Phone 的大屏扩展

#### 特征约束
- 大屏幕、高分辨率、有 stylus（部分）和键盘（部分）
- 算力接近笔记本，多任务能力
- 连接与 phone 同

#### 可用 binding
基本 **复用 phone binding**（§8.1）——iPadOS App Intents、HarmonyOS 同样的 Intents Kit、Android AppFunctions。差异在 **UI hint**。

#### 关键设计：output_hint.prefer_display

```yaml
id: browser.render_article
device:
  preferred: [tablet, pc]
  fallback: [phone]
output_hint:
  prefer_display: [screen_large, screen_medium]
  fallback_display: [voice_summary]   # 如果只有耳机可用，让 tool 返回 TTS 友好的短摘要
```

Agent 根据当前设备 fleet 选最合适的渲染方式，tool 提前声明能力。

#### Integration quickstart（15 min）
与 phone 相同（§8.1），加 "your tool is tablet-friendly" 标记。

#### Lily 场景中的角色
在 §3.4 场景中 tablet 未直接出场，但**长文档阅读 / 多窗口数据分析**等场景是 tablet 的主场。典型 Lily 用例：车上用 iPad 复习儿童报告、在医院候诊时看 PDF 病历。

---

### §8.5 PC（电脑）— 全算力 + 多样化 UI + 2026 HarmonyOS PC 新品类

#### 特征约束
- **全算力**（CPU 强、内存大、磁盘大）
- 多窗口、键盘、鼠标，"10-key UI"
- 连接：WiFi / Ethernet / USB 扩展
- 2026-04 **HarmonyOS PC 首发**（MateBook 系列），是 HMS 生态新扩展

#### 可用 binding

| vendor | binding | 备注 |
|--------|---------|------|
| macOS | AppleScript / Shortcuts 自动化 / AppFunction（部分）| |
| Windows | Shell / PowerShell / App SDK Win32 | |
| Linux | Shell / GUI automation (xdotool, wmctrl) | |
| **HarmonyOS PC** | HarmonyOS Kits + Intents Kit | **2026 新**；与 phone/watch 同 SDK |
| Web | REST 本地服务 | |

PC 最突出的特点是 **shell 和自动化 CLI 是一等公民 binding**。`gws` CLI 模式（§14）是 PC binding 的典型实例。

#### 典型 tool 模式

```yaml
id: pc.file.find
device:
  preferred: [pc]
bindings:
  cli:
    - platform: linux
      command: "find"
      args_template: "{directory} -name '{pattern}'"
    - platform: macos
      command: "mdfind"
      args_template: "-onlyin {directory} '{pattern}'"
    - platform: windows
      command: "pwsh"
      args_template: "-c 'Get-ChildItem -Path {directory} -Filter {pattern} -Recurse'"
    - platform: harmonyos_pc
      appfunction:
        bundle_name: com.huawei.files
        ability: FileSearchAbility
```

#### Integration quickstart（20 min）
CLI binding 最容易——你已有的命令行工具，加一份 ATD manifest 就能接入。HarmonyOS PC 上则走与 phone 相同的 Intents Kit。

#### Capability token 考量
- PC 的 shell tools 默认 `safety: dangerous`——必须显式授权
- HarmonyOS PC 沿用 phone 的权限模型

#### 陷阱
- **跨 OS 命令差异大**——同一个语义（"找文件"）在 Linux/macOS/Windows 三命令不同，dispatch 层靠 `bindings.cli[].platform` 切换
- HarmonyOS PC 刚出，社区工具少——早期采纳者机会

#### Lily 场景中的角色
§3.4 中 MateBook 是 **长文本产出设备**——Lily 回家后在 PC 上生成完整病历报告 PDF，大屏阅读。

---

### §8.6 Car HMI（智能汽车车机）— 驾驶安全 + CAN bus

#### 特征约束
- **驾驶安全约束**：驾驶中限制复杂 UI，voice-first 交互
- 独特接口：**CAN bus**（车辆遥测）、多摄像头、毫米波雷达、（某些）激光雷达
- **设备物理状态** 是 tool 调用前提：驾驶中 vs 停车；驾驶员 vs 乘客
- 连接：蜂窝（4G/5G，车内 WiFi hotspot）、蓝牙（配对 phone）

#### 可用 binding

| vendor | binding | 备注 |
|--------|---------|------|
| Aito / Luxeed / Seres (HIMA) | AppFunction `device_type: car_hmi, platform: harmonyspace_6` | ADS 5.0 智驾 API |
| Apple CarPlay | AppFunction `vendor: apple, platform: carplay` | 通过 phone 桥接 |
| Android Auto / AAOS | AppFunction `vendor: google, platform: android_automotive` | AAOS 独立 OS |
| 车厂自研 | 自定义 binding | |

#### v3 关键设计：driving_constraint

```yaml
id: car.navigation.route_to
device:
  preferred: [car_hmi]
safety:
  level: write
  driving_constraint: safe_always    # safe_always | requires_parked | passenger_only
  
id: car.settings.change_layout
safety:
  level: write
  driving_constraint: requires_parked

id: car.cabin.stream_selfie_camera
safety:
  level: dangerous
  driving_constraint: passenger_only   # 只允许副驾/后排乘客操作
```

Dispatch 层在调 car tool 前先查 `car.state.is_driving` 和 `car.state.driver_role`——不满足的调用返回 `DrivingSafetyBlocked` 错误（Appendix B）。

#### Integration quickstart（45 min）

车机是**最复杂**的 device class，原因：
- 需理解 HIMA / HarmonyOS Cockpit API
- driving_constraint 语义必须在 tool 声明、dispatch 强制、agent 理解错误返回
- CAN bus 数据格式（DBC 文件解析）

快速入门（HarmonySpace 6 + ADS 5.0）：

```typescript
import { cockpit } from '@hms.automotive.cockpit';
import { navigation } from '@hms.automotive.navigation';

export class NavigationRouteAbility {
  async onCall(want) {
    const dest = want.parameters.destination;
    const driving = await cockpit.getState('driving');
    if (!driving) throw new Error('Not in driving mode');
    return navigation.setDestination({ address: dest });
  }
}
```

#### Capability token 考量
- **车辆控制 token 比 phone 更严**——token 必须附 "驾驶员身份验证" constraint
- 乘客模式 tool 需 **车内位置验证**（防止儿童在后排乱点付费）
- ADS 自动驾驶状态查询是 Read，但**修改 ADS 行为**（如取消接管）是 Dangerous

#### 陷阱
- **Apple CarPlay 的 tool 通过 phone 桥接**——不是车机独立执行。Binding 要表达这层间接
- **AAOS vs Android Auto** 是两套——前者独立 OS 跑在车机，后者 phone 投屏。Tool 可能只在一边工作
- 车机常处于**低带宽/间歇网络**——工具需优雅降级

#### Lily 场景中的角色
§3.4 中 Aito M9 执行导航 + ADS 驾驶。通过 `session.handoff(trigger=proximity)` 从 phone 接手 session，是 v3 distributed sessions 的核心演示场景。

---

### §8.7 TV（智慧屏）— 10-ft UI + 家庭中枢

#### 特征约束
- **10-ft UI**：遥控器或手机投屏，不是触控（近屏）
- 屏幕超大（55-85 英寸），分辨率高（4K/8K）
- **家庭共享设备**——多用户混用，per-user 识别是挑战
- 算力中等（优于 phone，弱于 PC）

#### 可用 binding

| vendor | binding | SDK |
|--------|---------|-----|
| HarmonyOS Vision | AppFunction `device_type: tv, platform: harmonyos_vision` | ArkTS |
| Apple TV | tvOS AppIntents | Swift |
| Google TV | Android TV APIs + AppFunctions | Kotlin |
| Roku / FireTV | 各自专有 SDK | |

#### 典型 tool 模式

```yaml
id: tv.media.play
device:
  preferred: [tv]
  fallback: [tablet, pc]
output_hint:
  prefer_display: [screen_large]   # 大屏优化排版
bindings:
  appfunction:
    - device_type: tv
      vendor: huawei
      platform: harmonyos_vision
      ability: MediaPlayAbility
    - device_type: tv
      vendor: apple
      platform: tvos
      intent: PlayMediaIntent
```

#### 多用户问题（v3 未完全解决）

TV 是家庭共享设备，同一个 TV 可能给 Lily 和她先生用。**v3 不在协议层区分"当前谁在看"**——这个留给 agent 层（agent 基于上下文判断身份）。

未来 v3.1 或 RFC 可能引入 `device.active_user` 字段，但 v3.0 先不做。

#### Integration quickstart（20 min）
与 phone 相近，注意 **UI 尺寸适配**——tool 的 output 应该声明 `output_hint.prefer_display: [screen_large]`，让 agent 选用 TV 而不是 phone。

#### Lily 场景中的角色
§3.4 没用到 TV，但典型 Lily 用例：家里的 Vision 作为家庭中枢——显示购物清单、监控老人健康数据、孩子视频通话。

---

### §8.8（总结小节）设备类与 binding 的完整矩阵

| 设备类 | 主 binding kind | 延迟预期 | 特色 capability | 常见 fallback |
|-------|----------------|---------|----------------|--------------|
| Phone | AppFunction (per OS) | <50ms | 全功能 | REST（云） |
| Watch | AppFunction (Wear Engine / HealthKit) | <30ms（本地传感器） | health_* | Phone |
| Earbuds | AppFunction（控制平面）| <100ms | audio + 头部手势 | — |
| Tablet | 同 Phone + `prefer_large_screen` | <50ms | 大屏渲染 | Phone |
| PC | CLI + AppFunction（HarmonyOS PC 新）+ Shell | <100ms | 全算力 + 多窗口 | Tablet |
| Car HMI | AppFunction + driving_constraint | <200ms | CAN bus + ADS | — |
| TV | AppFunction + `screen_large` | <100ms | 家庭中枢 | Tablet |

**最关键观察**：**没有任何单一 binding 涵盖所有设备类**。v2 用单 AppFunction 枚举 + OS 平台失败的地方，v3 用 `device_type` × `vendor` × `platform` 三维索引成功覆盖。

---


## §9. 如果你是企业内部工具 owner

### §9.1 现状：一堆 REST API，想让 agent 能用

你是企业 IT / platform 团队。你们有：
- 内部 Jira API
- 内部 Confluence API
- 内部 HR 系统 API
- 内部 CI/CD API
- ...

你想让 agent（可能是内部的 copilot，可能是外采的 Claude Code 企业版）能调用这些 API。今天的选项：

- 让每个 agent 厂商逐个适配 → 不现实
- 自建 MCP server 包装每个 API → 工作量大
- 走 OpenAI Functions 格式 → 绑死 OpenAI
- ...

---

### §9.2 15 分钟：从 OpenAPI spec 生成 ATD tool definitions

你的 API 多半已经有 **OpenAPI 3.x spec**（Swagger）。ATD 提供了工具，把 OpenAPI spec **自动转成**一批 ATD tool definitions。

```bash
# 安装转换工具
$ npm install -g @atd-protocol/openapi-importer

# 一条命令，把 OpenAPI spec 转成 ATD definitions
$ atd-openapi convert \
    --input https://jira.internal.company.com/api/openapi.json \
    --output ./atd-tools/jira/ \
    --namespace "enterprise:company:jira"

Generated 42 ATD tool definitions:
  ✓ enterprise:company:jira.issue.create
  ✓ enterprise:company:jira.issue.get
  ✓ enterprise:company:jira.issue.update
  ✓ enterprise:company:jira.project.list
  ...
```

生成的每份 ATD definition 大致长这样：

```json
{
  "atd_version": "1.0",
  "id": "enterprise:company:jira.issue.create",
  "capability": {
    "domain": "issue_tracking",
    "actions": ["create"],
    "intent_examples": ["创建一个 Jira issue", "report a bug"]
  },
  "input": { /* 从 OpenAPI requestBody 转换 */ },
  "output": { /* 从 OpenAPI response 转换 */ },
  "bindings": {
    "rest": {
      "method": "POST",
      "url_template": "https://jira.internal.company.com/api/issues",
      "auth": { "type": "bearer", "env": "JIRA_API_TOKEN" }
    }
  },
  "safety": { "level": "write" }
}
```

**Step 2：注册到企业 ATD registry**

企业内部通常部署一个私有的 ATD registry（比如 ANOS 守护进程 + 企业配置）：

```bash
$ atd register ./atd-tools/jira/ --registry enterprise.company.internal

✓ Registered 42 tools to enterprise.company.internal registry
  Visible to: agents with capability enterprise:company:jira.*
```

**Step 3：企业 agent 自动发现并使用**

任何内部 agent（内部 Claude Code 实例、内部 Dify、自建 agent）连接到企业 registry 后，自动发现这批工具：

```typescript
const client = new AtdClient({ 
  endpoint: 'unix:///run/atd/atd.sock',
  registries: ['enterprise.company.internal']
});

const tools = await client.tools.search({ intent: '创建一个 bug ticket' });
// -> [{ tool_id: "enterprise:company:jira.issue.create", score: 0.91 }, ...]
```

---

### §9.3 实战：内部 Jira / Confluence → ATD tool

完整示例：把一个企业内部 Jira 变成 ATD 可用工具。

**Step 1：确认 Jira 有 OpenAPI spec**

```bash
$ curl https://jira.internal.company.com/rest/api/3/swagger.json
```

**Step 2：生成 ATD definitions**

```bash
$ atd-openapi convert \
    --input https://jira.internal.company.com/rest/api/3/swagger.json \
    --output ./atd-tools/jira/ \
    --namespace "enterprise:acme:jira" \
    --filter "include:issue,project,user"  # 只生成部分 API 的工具

Generated 28 ATD tool definitions.
```

**Step 3：补充 ATD 特有字段（intent_examples）**

自动生成的 ATD definition 不包含 `intent_examples`（OpenAPI 没有这信息）。建议手动补充，提升 agent 的 intent matching 质量：

```bash
$ atd-openapi enhance ./atd-tools/jira/ --interactive

Enhancing: enterprise:acme:jira.issue.create
  Description: "Create a new Jira issue"
  Suggest intent examples? [Y/n] y
  -> 创建一个 Jira issue
  -> report a bug
  -> 新建任务
  -> file a ticket
```

**Step 4：部署到企业 ATD runtime**

```bash
$ kubectl apply -f atd-runtime.yaml  # 部署企业内部 ATD 守护进程
$ atd register ./atd-tools/jira/ --endpoint atd.internal.company.com
```

**Step 5：企业 agent 调用**

```python
from atd_client import AtdClient

client = AtdClient(endpoint="atd.internal.company.com")

# Agent 创建 issue
result = client.tools.call(
    tool_id="enterprise:acme:jira.issue.create",
    params={
        "project": "BACKEND",
        "summary": "Login 500 error on prod",
        "priority": "High"
    }
)
# -> { status: "success", data: { issue_key: "BACKEND-1234", url: "..." } }
```

---

### §9.4 Capability Token ↔ 企业 IAM

企业环境的核心挑战：**ATD capability token 如何对接企业现有的 IAM（Identity & Access Management）**——比如 Okta、Azure AD、Keycloak、LDAP。

**方案：Capability Token Broker**

```
User (Lily) → SSO Login (Okta / Azure AD / ...)
                     ↓
              Capability Token Broker
                     ↓
        Exchange SSO token for ATD capability tokens
                     ↓
              User's ATD capabilities:
                - tool:enterprise:acme:jira.*          (Lily 是开发，可用)
                - tool:enterprise:acme:hr.read         (Lily 可读 HR 但不可写)
                - NOT tool:enterprise:acme:finance.*   (Lily 无 finance 访问权)
                     ↓
              Agent uses these capabilities
```

**实现示例（概念）**：

```typescript
// capability-broker.ts
app.post('/capability/exchange', async (req, res) => {
  const ssoToken = req.headers.authorization;
  const user = await verifyOkta(ssoToken);
  
  // 基于用户的 Okta groups 决定 ATD capabilities
  const capabilities = [];
  if (user.groups.includes('engineering')) {
    capabilities.push({ resource: 'tool:enterprise:acme:jira.*', safety_max: 'write' });
    capabilities.push({ resource: 'tool:enterprise:acme:github.*', safety_max: 'write' });
  }
  if (user.groups.includes('hr')) {
    capabilities.push({ resource: 'tool:enterprise:acme:hr.*', safety_max: 'write' });
  }
  
  // 生成 ATD capability token (UCAN format)
  const token = await generateCapabilityToken({
    subject: { agent_id: `agent:user:${user.id}` },
    capabilities,
    expires_in: '8h'
  });
  
  res.json({ token });
});
```

用户启动 agent 时：

```typescript
// agent startup
const ssoToken = await getSsoToken();
const { token: atdToken } = await fetch('https://capability-broker.company.com/capability/exchange', {
  headers: { authorization: ssoToken }
}).then(r => r.json());

const client = new AtdClient({ 
  endpoint: 'atd.internal.company.com',
  token: atdToken
});
```

**关键价值**：
- 企业 IAM 依然是 authoritative（用户权限在 Okta 管理）
- ATD capability token 是 IAM 在 agent 层的投影
- 用户的 SSO 变更（离职 / 权限调整）即时反映到 agent 调用工具的能力上

---

## §10. 贯穿示例：Lily 场景 end-to-end

把 §6 / §7 / §8 / §9 各自的部分拼起来——这就是 Lily 一条消息的完整生命周期。

### §10.1 场景回顾

Lily 在 iPhone 上对她的 agent 说：

> **"明早 7 点开灯，昨晚睡眠怎么样？"**

涉及的工具（分别由 §6-§9 四种开发者提供）：

| 工具 | 提供方 | Part 2 章节 |
|-----|------|-----------|
| `vendor:xiaomi:light.turn_on` | 米家团队维护的 MCP server | §6 (MCP 作者) |
| `vendor:xiaomi:timer.schedule` | 米家团队维护的 MCP server | §6 |
| `vendor:huawei:health.sleep.get` | 华为健康团队的 AppFunction + REST binding | §8 (移动开发者) |
| Lily 的 agent framework | Lily 选的 personal assistant app | §7 (框架作者) |

---

### §10.2 完整流程

```
【T=0ms】Lily 语音输入
   "明早 7 点开灯，昨晚睡眠怎么样？"
     ↓
【T=50ms】语音识别 + LLM 前置处理
     ↓
【T=100ms】Agent framework 调用 LLM
   system prompt 包含 Hot tier 20 个工具（~3K tokens）
   LLM 返回意图：需要两个工具
     1. 定时开灯 (intent: "明早 7 点开灯")
     2. 查询睡眠 (intent: "昨晚睡眠")
     ↓
【T=300ms】Framework 调用 atd.tools.search() 两次
   搜索 1: "定时开灯" 
     → warm tier HNSW 命中 vendor:xiaomi:timer.schedule (score 0.89)
   搜索 2: "昨晚睡眠"
     → warm tier HNSW 命中 vendor:huawei:health.sleep.get (score 0.93)
     ↓
【T=380ms】LLM 生成 tool calls（并行）
     ↓
【T=400ms】ATD Dispatch Layer 并行处理两个调用

   [Call 1] vendor:xiaomi:timer.schedule
     Step 1: Capability check (Lily 已授权米家) ✓
     Step 2: Resolve tool
     Step 3: Validate params { device_id: "living_room_light", time: "07:00" }
     Step 4: Rate limit OK
     Step 5: Binding selection
        - platform=iOS → appfunction binding NO (米家无 iOS appfunction)
        - mcp binding: 米家 MCP server 通过 iOS proxy 可用 ✓
        - rest binding: 米家 cloud IoT API 可用 ✓
        - 选 MCP binding (延迟更低)
     Step 6: Execute via MCP (JSON-RPC over iOS proxy)
     Step 7: Normalize result → ATD 统一格式
     Step 8: Audit log + 返回

   [Call 2] vendor:huawei:health.sleep.get
     Step 1-4: ... ✓
     Step 5: Binding selection
        - platform=iOS → appfunction binding NO (华为无 iOS appfunction)
        - rest binding: 华为云 API ✓
        - 选 rest binding
     Step 6: HTTP GET https://health-api.cloud.huawei.com/sleep?date=2026-04-19
        with OAuth2 token (scope: huawei.health.read)
     Step 7: Normalize { total_minutes: 443, deep_sleep_minutes: 98, ... }
     Step 8: Audit log + 返回

【T=850ms】两个并行 call 完成，结果返回 framework
     ↓
【T=900ms】Framework 把结果喂回 LLM 生成回复
     ↓
【T=1.2s】Lily 看到：
   "✓ 客厅灯已定时明早 7:00 开启
    💤 昨晚睡眠 7h23m（质量 82%），深睡 1h38m
       比前一天少 25 分钟，记得早睡哦 :)"
```

**整个过程**：
- 调用了**两个不同厂商**（米家 / 华为）的工具
- 用了**两种不同 binding**（MCP / REST）
- **无平台 lock-in**：Lily 换 Android 手机，米家 binding 自动换到 appfunction
- **无框架 lock-in**：Lily 换一个 agent app，底层 ATD tool 照样用
- **无 LLM lock-in**：framework 可以用 Claude / GPT / Gemini 驱动

---

### §10.3 调试视角

ATD runtime 提供完整的调试 API。如果 Lily 的请求出错，开发者可以：

```bash
# 查看最近 dispatch trace
$ atd trace last --agent lily-assistant
┌─────────────────────────────────────────────────────────────────┐
│ Turn ID: turn-2026-04-19-14:30:22                                │
│ User: "明早 7 点开灯，昨晚睡眠怎么样？"                            │
│                                                                   │
│ ┌──────────────────────────────────────────────────────────────┐│
│ │ Call 1: vendor:xiaomi:timer.schedule                         ││
│ │   Status: SUCCESS  Latency: 412ms  Binding: mcp              ││
│ │   Params: { device_id: "living_room_light", time: "07:00" } ││
│ └──────────────────────────────────────────────────────────────┘│
│                                                                   │
│ ┌──────────────────────────────────────────────────────────────┐│
│ │ Call 2: vendor:huawei:health.sleep.get                       ││
│ │   Status: SUCCESS  Latency: 380ms  Binding: rest             ││
│ │   Params: { date: "2026-04-18" }                             ││
│ │   Result summary: total_minutes=443, deep_sleep=98           ││
│ └──────────────────────────────────────────────────────────────┘│
│                                                                   │
│ Total turn time: 1.2s                                             │
│ Tokens used: 3,240 (system prompt: 2,987, completion: 253)       │
└─────────────────────────────────────────────────────────────────┘

# 查看 Hot tier 组成（为什么某个工具没在 LLM 视野里）
$ atd tools hot --agent lily-assistant
Rank  Tool ID                              Score  Last used
 1    vendor:xiaomi:light.turn_on          0.95   10m ago
 2    vendor:huawei:health.sleep.get       0.94   now
 ...

# 查看健康状态
$ atd health vendor:xiaomi:*
Tool                              Status    Success rate  p99 latency
vendor:xiaomi:light.turn_on       HEALTHY   98.3%         340ms
vendor:xiaomi:timer.schedule      HEALTHY   97.1%         410ms
vendor:xiaomi:ac.set_temperature  DEGRADED  87.0%         1.2s     ← 注意
```

**开发者工作流**：从 user-reported issue 到 root cause，有清晰的 trace。

---

## §11. 参考资源

### §11.1 快速链接

- **规范原文**：`docs/architecture/atd-overview.md`（ATD v1.0 完整技术规范）
- **参考实现**：`crates/anos-tool-dispatch/`（Rust，Apache 2.0）
- **Starter 模板**：
  - TypeScript Client SDK: `github.com/atd-protocol/atd-client-ts`（征集中）
  - Python Client SDK: `github.com/atd-protocol/atd-client-py`（征集中）
  - OpenAPI Importer: `github.com/atd-protocol/openapi-importer`（征集中）

### §11.2 常见问题 FAQ

**Q：ATD 和 MCP 是竞争关系吗？**
不是。ATD 把 MCP 视为一种 binding（`mcp:*` 命名空间）。你的 MCP server 加一份 ATD manifest 就可以同时服务两个生态。

**Q：我必须全部迁移到 ATD 吗？**
不必。ATD 的采纳可以是**加法**——你在现有 MCP / OpenAI Functions / LangChain 代码基础上加一层 ATD 支持，两个生态并行。

**Q：ATD 有性能开销吗？**
Dispatch 层开销 <5ms，capability 验证 <1ms。比 LLM 调用本身（~500ms-10s）低两个数量级。

**Q：企业合规 / 安全团队会批准吗？**
ATD 的 capability token 是 UCAN 标准（W3C DID 配套）、JSON Schema 标准、MIT/Apache 2.0 许可——比许多现有方案更利于合规。

**Q：ATD 规范以后会乱改吗？**
v1.0 规范冻结后，minor version 只能加 optional 字段，major version 变更需要 APWG 多利益方批准 + 2 个 minor 的 deprecation 周期（详见附录 D）。

#### v3 多设备相关 FAQ

**Q：我的 tool 没有 `device.preferred` 字段，v3 下还能用吗？**
可以。Dispatch 默认 `device.preferred: [phone]`，行为和 v2 完全一样。

**Q：手表和手机都有健康数据，为什么 dispatch 不默认选手表？**
Dispatch 按 tool 声明的 `preferred` 选。健康类 tool 建议显式 `preferred: [watch], fallback: [phone]`——这是 tool 作者的决定，不是 dispatch 的默认猜测。

**Q：车机 `driving_constraint: safe_always` 怎么验证？谁决定车辆处于驾驶状态？**
由车机自身的 ADS / cockpit API 提供 `car.state.is_driving`。Dispatch 查询该状态后决策。验证由车机平台保证（Apple CarPlay / AAOS / HarmonyOS Cockpit 各有实现）。

**Q：跨设备 session 迁移，上次没完成的 tool call 会重复执行吗？**
默认不会。`migrate` 等待当前 call 完成再迁移。只有显式 `force_migrate()` 才会中止并在目标设备重播。

**Q：我的耳机想做"实时声纹识别"——ATD 是不是不支持？**
ATD 不是流媒体协议。声纹识别分两部分：音频流走 BT/NearLink（不经 ATD），控制平面（启动/停止/读结果）走 ATD。你的 tool 是控制平面的，完全可以。

**Q：Apple 设备和 Huawei 设备之间能做 session handoff 吗？**
`generic_rpc` transport 理论上允许，但**实际上没有 vendor 支持**（Apple 和 Huawei 都只做自己生态内）。v3 协议允许，实装是 best-effort。

**Q：v2 的 tool definition 在 v3 上还能跑吗？需要重写吗？**
完全可以，**无需重写**。v3 是 strict superset——缺失的字段取默认值，运行等效 v2 行为。v3 新特性是增量采纳，不是强制升级。

### §11.3 社区

- **GitHub**: `github.com/atd-protocol`（待启动）
- **Discord**: 邀请链接待公布
- **邮件列表**: `atd-announce@lists.atd-protocol.org`（筹建中）
- **Founding Adopter 申请**: `founding-adopters@atd-protocol.org`

### §11.4 贡献路径

1. **Reference Binding**：fork SDK 模板，维护一个语言
2. **Vertical Binding**：申请一个 `vendor:xxx` 命名空间
3. **Tooling**：OpenAPI importer、IDE 插件、测试工具
4. **Documentation**：教程、博客、视频
5. **Conformance Testing**：跑 conformance test 报告结果

---

# 附录

## Appendix A: ATD Schema 速查（v3.0）

顶层字段（完整 schema 见 `docs/architecture/atd-overview.md §3`）。**v3 新增字段标记 `*v3`**：

```json
{
  "atd_version": "1.0",         // 协议版本（必填；v3 仍写 "1.0" 表 schema 基线）
  "id": "ns:domain.res.action", // 唯一 ID（必填）
  "version": "1.0.0",            // Tool semver（必填）
  "name": "...",                 // 人类可读名（必填）
  "description": "...",          // 描述（必填）
  "capability": {                // 语义与发现（必填）
    "domain": "...",
    "actions": [...],
    "intent_examples": [...]
  },
  "input":  { /* JSON Schema */ }, // 输入 schema（必填）
  "output": { /* JSON Schema */ }, // 输出 schema（必填）
  "errors": [ /* 领域错误 */ ],    // 错误定义
  "device": {                      // *v3 Device affinity（§2.5）
    "preferred": [...],
    "fallback":  [...],
    "requires":  { "sensors": [...], "ui_modes": [...], "transport": [...] },
    "vendor_hints": [...]
  },
  "bindings": {                    // 绑定（至少 1 个必填）
    "cli": { ... },
    "mcp": { ... },
    "rest": { ... },
    "appfunction": [              // *v3 现在是 list（按 device_type × vendor × platform 索引）
      { "device_type": "...", "vendor": "...", "platform": "...", /* target */ }
    ]
  },
  "safety": {                      // 安全分级（必填）
    "level": "read|write|dangerous",
    "data_sensitivity": "...",
    "side_effects": [...],
    "driving_constraint": "safe_always|requires_parked|passenger_only"  // *v3 §8.6
  },
  "resources": {                   // 运行时约束
    "timeout_ms": 30000,
    "max_concurrent": 5,
    "rate_limit": { "max": 60, "window_secs": 60 }
  },
  "result_middleware": [           // *v3 §2.7 Result middleware
    { "type": "pii_redact", "fields": [...], "mode": "transform" }
  ],
  "ergonomic_aliases": [           // *v3 §2.8 人体工学别名
    { "alias_id": "...", "input": {...}, "maps_to": {...}, "visibility": "preferred" }
  ],
  "output_hint": {                 // *v3 §8.4/8.5/8.7 渲染提示
    "prefer_display": ["screen_large", "voice_summary"],
    "fallback_display": ["voice_summary"]
  },
  "distributed": {                 // *v3 §2.6 跨设备 session 默认
    "shareable": false,
    "transport_hint": [...]
  },
  "trust": { "publisher": "...", "signature": "..." },
  "compatibility": {
    "platforms": [...],
    "requires_capabilities": [...]
  },
  "fallback": { "fallback_tool_id": "..." }
}
```

**向后兼容**：v2 tool definitions 照样合法。缺失的 v3 字段取默认值（`device.preferred: [phone]`, `distributed.shareable: false`, 空 middleware/aliases list, 空 output_hint）。

---

## Appendix B: 统一错误码表

Dispatch 层统一错误码（跨 binding 一致）：

| Code | 含义 | 可重试 | 示例场景 |
|-----|-----|-------|---------|
| `PERMISSION_DENIED` | 授权失败 | No | Token 过期 / 范围不匹配 |
| `TOOL_NOT_FOUND` | 工具不存在 | No | 拼写错误 / 未注册 |
| `VALIDATION_ERROR` | 参数不合法 | No（需修正）| Schema 验证失败 |
| `RATE_LIMITED` | 超过速率限制 | Yes (retry_after) | 频率超限 |
| `TIMEOUT` | 超时 | Yes | 网络慢 / 工具长时 |
| `TOOL_CIRCUIT_OPEN` | 熔断器开启 | Yes (wait) | 工具连续失败 |
| `CONSTITUTIONAL_VIOLATION` | 宪法守卫触发 | No | Secret 泄漏 / 危险命令 |
| `PLATFORM_UNSUPPORTED` | 当前平台不支持 | No | iOS 上调 HarmonyOS appfunction |
| `INTERNAL_ERROR` | 内部错误 | Yes | 未分类失败 |
| `BUDGET_EXCEEDED` | 预算超限 | No | Cost budget 耗尽 |
| `DEVICE_UNAVAILABLE` *v3 | 用户 fleet 中无满足 device affinity 的设备 | Maybe (device 回线) | §2.5 手表离线，tool 要求 `preferred:[watch]` 且无 fallback |
| `SENSOR_MISSING` *v3 | 目标设备缺必需 sensor | No | §2.5 手机调 `requires.sensors:[ecg]` 但手机没 ECG |
| `DRIVING_SAFETY_BLOCKED` *v3 | 驾驶中不允许的调用 | Yes (等停车) | §8.6 `requires_parked` 工具在驾驶中调用 |
| `MIDDLEWARE_ERROR` *v3 | Middleware pipeline 失败且 `on_failure: fail_closed` | Maybe | §2.7 PII redact 服务宕机；只在 fail_closed 抛 |
| `RESULT_BLOCKED` *v3 | Middleware `block` 模式拒绝返回 | No | §2.7 prompt injection scan block 模式命中 |
| `SESSION_MIGRATION_FAILED` *v3 | 跨设备 session 迁移失败 | Maybe | §2.6 目标设备离线、token delegation 失败 |
| `ALIAS_TRANSFORM_FAILED` *v3 | Ergonomic alias transform 出错 | No（需修 alias def）| §2.8 Appendix J transform fn 异常 |

---

## Appendix C: Conformance Test Suite 索引

参考实现必须通过的测试集（详见独立文档 `conformance-test.md`）：

- **C1 Schema Validation**：JSON Schema Draft 2020-12 兼容
- **C2 Dispatch Invariants**：8 步流水线顺序、错误处理
- **C3 Binding Contract**：4 种 binding 的参数/结果映射
- **C4 Capability Attenuation**：token 派生的单调递减
- **C5 Visibility Enforcement**：Dangerous 工具未授权不可见
- **C6 Tier Transitions**：Hot/Warm/Cold 升降级规则
- **C7 Circuit Breaker**：3 状态机 + cooldown
- **C8 MCP Interop**：MCP binding 与原生 MCP 双向兼容
- **C9 OpenAI Tools Interop**：Compact ATD ↔ OpenAI Tools 无损投影
- **C10 Device Affinity Routing** *v3：`preferred` → `fallback` → `rest` 路由顺序；sensor precondition 过滤
- **C11 Distributed Sessions** *v3：`migrate` / `fork` / `handoff`；mid-call migration 等待语义；capability token re-delegation
- **C12 Result Middleware Pipeline** *v3：声明顺序；5 个 builtin 行为；`fail_closed` vs default
- **C13 Ergonomic Aliases** *v3：9 个 whitelisted fn；嵌套 transform；错误路径
- **C14 Driving Safety** *v3：`driving_constraint` 三档；车辆状态查询；`DRIVING_SAFETY_BLOCKED` 返回
- **C15 Audio Control Plane** *v3：耳机 tool 不含 stream；控制命令延迟 <100ms

---

## Appendix D: APWG 治理结构

**三阶段演化**：

| 阶段 | 时间 | 主导 | 决策机制 |
|-----|-----|------|---------|
| **Phase 1** | 2026 Q2 – Q3 | ANOS 项目 | 维护者 consensus + GitHub RFC |
| **Phase 2** | 2026 Q4 – 2027 | APWG（Agent Protocol Working Group）| Rough Consensus |
| **Phase 3** | 2028+ | W3C / IETF / LF AI 托管 | 标准化组织流程 |

**APWG 组织结构**：

- **Steering Committee**：5-7 人，创始成员代表
- **Technical Committee**：7-11 人，按协议层分 subgroup
- **Interop Committee**：conformance test 维护 + 认证
- **Ecosystem Committee**：namespace 分配 + registry federation + 生态 liaison
- **Independent Auditor**：年度生态报告 + 争议仲裁

**防 capture 机制**：

- 单组织代表数上限（Steering ≤1, Technical ≤2）
- 地理多样性要求（至少 3 个地区）
- 资金透明（单赞助者 ≤40%）
- 开源许可保证（Apache 2.0 / CC BY 4.0）
- 利益冲突弃权

**IP 策略**：

- 规范：CC BY 4.0
- 参考实现：Apache 2.0（含专利授权）
- "ATD Compatible" 商标：APWG 持有
- Patent Non-Assertion：参考 W3C Patent Policy

---

## Appendix E: 理论基础索引

ATD 的设计决策有理论支撑。完整证明详见独立学术论文，这里只列核心结论：

**E.1 Tool Dispatch CAP 定理**
任何 agent 工具协议在 **Scalability × Interoperability × Capability Security** 三者中最多单层满足两个。三者协同需要**分层架构**。

*对 ATD 的含义*：Schema / Dispatch / Binding / Security / Capacity / Reliability 六层设计不是偶然，是定理约束。

**E.2 Hot/Warm/Cold Pareto 最优定理**
在 Zipf 调用频率分布下，三层容量模型在 (context cost, discovery latency) 空间是 Pareto 最优——不存在同时优于 Hot/Warm/Cold 的单层或双层方案。

*对 ATD 的含义*：Tier 不是工程选择，是数学必然。

**E.3 ATD ↔ POSIX 结构同构**
- Tool Definition ↔ syscall number + signature
- Capability Token ↔ file descriptor
- 4 级 Visibility ↔ user/group/other permission
- Dispatch ↔ syscall dispatcher
- Runtime ↔ kernel

*对 ATD 的含义*：ATD 是 agent 时代的 POSIX——定义抽象边界而不规定实现。

**E.4 开放理论问题**
- Capability 形式化验证（TLA+ / Coq 证明 attenuation 单调性）
- 语义发现鲁棒性（跨 LLM / 跨语言 intent 一致性）
- 类型化工具组合（Linear types? Effect systems?）
- 实时延迟预算（毫秒级 dispatch 的可行性）
- 治理可持续性（如何防止 Phase 2→3 失败）

---

## Appendix F: 从 MCP / OpenAI / LangChain 迁移速查

### F.1 From MCP

| MCP 概念 | ATD 对应 | 迁移成本 |
|---------|---------|---------|
| `tools/list` | `atd.tools.list()` via `mcp:` binding | 零（只需加 manifest） |
| `tools/call` | ATD Dispatch → `mcp:` binding | 零 |
| JSON-RPC error | ATD 统一错误码 | 自动映射 |
| MCP stdio/SSE transport | `mcp:` binding 的 transport 字段 | 零 |

### F.2 From OpenAI Functions

```python
# OpenAI Functions 格式
{
  "type": "function",
  "function": {
    "name": "get_weather",
    "description": "Get the current weather",
    "parameters": { ... }
  }
}

# → ATD Compact ATD（Hot tier 投影）
{
  "id": "anos:weather.get",
  "name": "get_weather",
  "description": "Get the current weather",
  "input": { ... },
  "safety": { "level": "read" }
}
```

双向转换工具：`atd-openai-bridge`（征集贡献）。

### F.3 From LangChain Tools

```python
# LangChain BaseTool
class WeatherTool(BaseTool):
    name = "weather"
    description = "..."
    def _run(self, location): ...

# 包装成 ATD tool
from atd_client import tool_from_langchain
atd_def = tool_from_langchain(WeatherTool(), id="anos:weather.get")
```

### F.4 From Apple App Intents / Android AppFunctions

无需迁移——直接在 ATD tool definition 的 `bindings.appfunction` 字段声明对应 App Intent / AppFunction，ATD runtime 在对应平台上调用原生 API。详见 §8.2。

---

## Appendix G: `atd-tools` — SKILL.md 与 ATD 的互操作扩展（Draft RFC）

本附录是对 §2.4 提出的可选 YAML 扩展字段的完整规范。目的是让 agentskills.io 规范的 SKILL.md 能声明对 ATD 工具和能力的依赖，用于 **install-time 校验**、**capability token 预签发**、**H/W/C tier 预热**。

**RFC 状态**：v0.1 draft，待 agentskills.io 社区评审。ATD v1.x 实装可按此字段提供参考支持，等正式进入 SKILL.md spec 后再稳定。

### G.1 动机（从失败场景出发）

今天一个典型的 SKILL.md 看不到自己依赖什么 tool。运行到第 3 步时才发现 `flight.status` 不可用、用户没授权 `net.http`、或当前平台无 `appfunction` binding。失败时已经消耗了 tokens 和用户耐心。

对应的补救手段——在 install 或预加载阶段**静态校验**——需要 skill 显式声明依赖。

### G.2 字段定义

在 SKILL.md 的 YAML frontmatter 中新增两个可选字段：

```yaml
---
name: trip-prep
description: Prepare for tomorrow's trip
version: 1.2.0
license: MIT

# NEW: ATD tool dependencies
atd-tools:
  required:                        # 必需工具，缺一不可
    - id: calendar.get
      min_version: "1.0"
    - id: weather.get
  optional:                        # 可选工具，缺则跳过对应步骤
    - id: flight.status
      fallback: web.search         # 可选：缺失时的 fallback 工具
  preferred_bindings:              # 可选：binding 偏好
    weather.get: [rest, cli]       # 优先 REST（减少 shell 开销）
    flight.status: [appfunction, rest]  # 优先原生 app

# NEW: ATD capability requirements
atd-capabilities:
  required:                        # 必需能力（UCAN capability）
    - calendar.read
    - net.http
  optional:
    - flight.booking.write         # 如用户授权就能用，没授权就跳过

# NEW: ATD tier hints (optional)
atd-tier-hints:
  promote_on_install: [calendar.get, weather.get]   # install 后立即进 Hot
---
```

### G.3 形式化语义

定义 `SkillDeps` 为 skill 声明的依赖集合：

```
SkillDeps = {
  tools_required:     [(tool_id, min_version?)],
  tools_optional:     [(tool_id, fallback?)],
  caps_required:      [cap_id],
  caps_optional:      [cap_id],
  binding_prefs:      {tool_id → [binding, ...]},
  tier_hints:         {promote_on_install: [tool_id]},
}
```

设 `E` 为当前 ATD 环境可用的 tool + capability 集合。**Skill install 校验函数**：

```
validate_install(skill, E) → ValidationReport
  PASS       if ∀ t ∈ skill.tools_required: t ∈ E.tools ∧ version_match(t)
              ∧ ∀ c ∈ skill.caps_required:  c ∈ E.caps
  DEGRADED   if 上述通过，但存在 t ∈ skill.tools_optional: t ∉ E.tools
              （skill 仍可用，功能子集）
  FAIL       otherwise
```

### G.4 运行时行为

Skill 运行时（任何符合 agentskills.io 规范的 runtime）在调用 ATD dispatch 时，SHOULD：

1. **Install 时**：运行 `validate_install()`，向用户报告 PASS / DEGRADED / FAIL
2. **Runtime 时**：调用 `atd.call()` 前，MAY 传入 `hint.preferred_binding` 给 dispatch 层
3. **Tier 提示**：install 成功后，MAY 调用 `atd.tier.promote_hint(tool_ids)` 触发 Warm → Hot 预热
4. **Capability 预签发**：install 时，runtime MAY 代表 skill 向用户请求 `caps_required` 的 UCAN token

无此字段的 SKILL.md 行为**与今天完全相同**——runtime 发现需要某 tool 时再动态 discover / describe / call。

### G.5 向后兼容保证

| 场景 | 行为 |
|------|------|
| 没有 `atd-tools` 字段的 SKILL.md | 与当前行为完全一致，零影响 |
| 有字段但 runtime 不懂 | runtime 应 ignore 未知字段（YAML 解析规则），skill 照样能跑 |
| 有字段且 runtime 理解 | 额外获得 install-time 校验、tier 预热、capability 预签发 |
| 字段中声明了 v1.1 才有的新 tool | 在 v1.0 runtime 上：校验 PASS（宽松模式）或 FAIL（严格模式），由 runtime 策略决定 |

**不破坏**现有 26+ 平台的 SKILL.md 消费方式——只是在 ATD 存在的场景下额外强化能力。

### G.6 与 agentskills.io spec 的关系

本字段**不是** ATD 单方面发明的专属扩展，而是希望作为 **agentskills.io spec 的可选扩展命名空间**被正式纳入。提议步骤：

1. **Phase 1（当前）**：ATD v1.1 规范内定义此字段为 `atd-tools:` / `atd-capabilities:` / `atd-tier-hints:`，ATD runtime 和 SDK 优先支持
2. **Phase 2**：收集 3+ 个真实采纳案例（Claude Code skill、OpenClaw skill、Cursor skill），作为 RFC 提交到 github.com/agentskills/agentskills
3. **Phase 3**：如被采纳，字段迁移到 agentskills.io spec 的 "Implementation-specific extensions" 命名空间（可能是 `x-atd:` 或 `tool-dispatch:`）

**如果 agentskills.io 社区拒绝**：此字段保留在 ATD 规范内部，作为 "ATD-aware skill" 的专属扩展；不 ATD-aware 的 runtime 仍然忽略，不影响 skill 可移植性。

### G.7 示例：Lily 出差准备 skill

```yaml
---
name: trip-prep-lily
description: |
  Prepare travel briefing: check flights, weather, and
  pre-download offline maps for the destination.

version: 0.3.0
license: CC-BY-4.0

atd-tools:
  required:
    - id: calendar.get
    - id: weather.get
  optional:
    - id: flight.status
      fallback: web.search
    - id: maps.offline.download
      fallback: web.fetch
  preferred_bindings:
    weather.get: [rest]
    flight.status: [appfunction, rest]

atd-capabilities:
  required: [calendar.read, net.http]
  optional: [maps.write]

atd-tier-hints:
  promote_on_install: [calendar.get, weather.get]
---

# Trip Preparation

When the user asks to prepare for a trip:

1. Call `calendar.get` to find the next travel event
2. Call `weather.get` for the destination forecast
3. If `flight.status` available: call it; else use `web.search` with flight number
4. If `maps.offline.download` available and `maps.write` granted: pre-download
5. Summarize findings to the user
```

Install 这个 skill 到一个没有航空 binding 的 ATD 环境时，用户看到：

```
✓ calendar.get (1.0)       available via appfunction (iOS Calendar)
✓ weather.get (1.2)        available via rest (OpenWeatherMap)
⚠ flight.status            not available → will use web.search fallback
⚠ maps.offline.download    not available → will use web.fetch fallback
✓ calendar.read capability granted
✓ net.http capability granted
○ maps.write capability    not granted (optional, skipping step 4)

Skill validation: DEGRADED (steps 3 and 4 use fallback, step 4 will be skipped)
Continue? [y/N]
```

这是 **install-time 透明**——用户在跑 skill 之前就知道它能做什么、不能做什么，为什么。

### G.8 开放问题（征集反馈）

以下问题作为 draft RFC 的公开议题：

1. **版本约束语法**：`min_version: "1.0"` 足够还是需要完整 semver range（`">=1.0 <2.0"`）？
2. **Capability 声明的粒度**：`net.http` 是否需要进一步细化为 `net.http.get` / `net.http.post`？
3. **Fallback 链**：`fallback: web.search` 是单层还是允许多层（`fallback: [web.search, llm.query]`）？
4. **Tier hint 的 TTL**：`promote_on_install` 的工具如果长期不被调用，应该在多久后自动 demote 回 Warm？（建议：与 ATD 默认 demote 规则一致，即 14 天）
5. **跨 skill 的 capability 继承**：如果 skill A 依赖 skill B，A 是否自动继承 B 的 capability 需求？

反馈请提交至 github.com/atd-protocol/skills-extension-rfc（筹建中）。

---

## Appendix H: Device Type Registry（v3 规范化枚举）

v3 §2.5 Multi-Device Dispatch 依赖的**规范化设备类型 + capability tag 词表**。本附录是这些标识符的唯一权威来源。

### H.1 Canonical Device Type 枚举

| id | 描述 | 典型 vendor/产品 |
|----|------|----------------|
| `phone` | 智能手机（全屏 + 全算力 + 常连） | iPhone, Mate 80, Pixel, Galaxy |
| `watch` | 智能手表/手环（小屏/无屏 + 健康传感器 + 配对连接） | Apple Watch, Huawei Watch 4, Garmin, Wear OS |
| `earbuds` | 真无线耳机（无 UI + 音频 + 可选传感器） | AirPods, FreeBuds, Galaxy Buds |
| `tablet` | 平板（大屏 + 中高算力 + 常连） | iPad, MatePad, Galaxy Tab |
| `pc` | 笔记本/台式电脑（全算力 + 键鼠 + 多窗口） | MacBook, MateBook, Windows PC, Linux desktop |
| `car_hmi` | 汽车智能座舱（驾驶安全约束 + CAN bus） | Aito 车机, CarPlay, Android Auto |
| `tv` | 智慧屏（10-ft UI + 家庭共享） | HarmonyOS Vision, Apple TV, Google TV |
| `smart_home_hub` | 家庭中枢（常开 + 语音优先） | HomePod, Echo, Mi Home gateway |
| `vr_headset` | VR/AR 头显 | Vision Pro, Meta Quest, Pico |
| `head_unit` | 可穿戴计算（眼镜类） | Ray-Ban Meta, XReal |

### H.2 Vendor Extension 槽

非核心类型用 `vendor:<name>` 形式。例子：

- `vendor:huawei:freebuds_pro_5` — 特定型号耳机（如 tool 需要 NearLink 特定能力）
- `vendor:tesla:car_hmi` — 车机的 Tesla 特化
- `vendor:nothing:phone` — 特定厂商 phone 的特殊 UI 规范

Extension 只用于**特定型号必须的区别**。大多数 tool 应 target canonical type。

### H.3 Capability Tags v1.0 词表（20 项，严格）

**Sensors**：
- `heart_rate`, `ecg`, `spo2`, `skin_temp`, `gps`, `camera`, `microphone`, `accelerometer`, `barometer`, `lidar`, `can_bus`

**UI modes**：
- `screen_large`, `screen_medium`, `screen_small`, `screen_none`, `voice`, `haptic`, `hud`

**Transport**：
- `bluetooth`, `wifi`, `nearlink`, `lte`, `5g`, `satellite`

Tools **必须**用这个词表声明前置条件。`requires.sensors: [custom_xxx]` 无效。

### H.4 DeviceInstance 数据结构

`client.devices()` 返回：

```rust
struct DeviceInstance {
    device_id:    String,           // did:device:<vendor>:<type>:<model>:<serial>
    device_type:  String,           // canonical enum or vendor:*
    vendor:       String,           // "apple" | "huawei" | "google" | ...
    model:        String,
    online:       bool,
    sensors:      Vec<String>,      // from §H.3 词表
    ui_modes:     Vec<String>,
    transport:    Vec<String>,
    battery_pct:  Option<u8>,
    position:     Option<DevicePosition>,  // local | companion | remote_via_cloud
}
```

### H.5 词表演进规则

v3.0 固定 20 项。新增需走 RFC 流程 —— 至少 3 个独立设备厂商需要此 capability 才提议扩展。扩展遵循向后兼容（新 tag 不破坏旧 tool）。

---

## Appendix I: Distributed Binding Schemas

v3 §2.6 Distributed Sessions 需要的**具体跨设备传输方式**。本附录定义三种参考 transport，以及各自的 binding schema。

### I.1 HarmonyOS Super Device (DSoftBus)

最成熟的实装参考（HarmonyOS NEXT / 5+）：

```yaml
distributed:
  transport_hint: [harmonyos_super_device]

bindings:
  appfunction:
    - device_type: phone
      vendor: huawei
      platform: harmonyos
      distributed:
        kind: harmonyos_super_device
        fa_migration: true         # 支持 Feature Ability continuation
        data_sync: distributed_data_object
```

特性：
- 低延迟（<20ms 跨设备 RPC）
- 自动设备发现（PIN-free pairing 已配对过的设备）
- 状态同步通过 `distributedDataObject` API
- 文件迁移通过分布式文件系统

### I.2 Apple Continuity / Handoff

```yaml
distributed:
  transport_hint: [apple_handoff]

bindings:
  appfunction:
    - device_type: phone
      vendor: apple
      platform: ios
      distributed:
        kind: apple_handoff
        nsuser_activity_type: "com.yourapp.activity.edit"
        requires_icloud: true
```

特性：
- 通过 `NSUserActivity` 传递
- 依赖 iCloud（同一 Apple ID 前提）
- 不跨 Apple ↔ 非 Apple 生态

### I.3 Generic RPC Fallback

跨厂商通用 fallback，基于明文 JSON-RPC over TLS：

```yaml
distributed:
  transport_hint: [generic_rpc]

bindings:
  appfunction:
    - device_type: any
      distributed:
        kind: generic_rpc
        endpoint_discovery: user_directory_service
        auth: ucan_delegation
```

特性：
- 最差延迟（200-500ms，依赖 user's rendezvous service）
- 适用跨 vendor（Apple ↔ Huawei ↔ Google）
- 性能和一致性都低于 vendor-native 方案

### I.4 Transport 选择顺序

Dispatch 按 `transport_hint` 声明顺序尝试，第一个可用即用。无 `transport_hint` 则 local-only（不启用跨设备）。

---

## Appendix J: Ergonomic Aliases DSL 规范

v3 §2.8 引入的**声明式 transform DSL** 的完整语法和白名单 fn 库。

### J.1 语法

```ebnf
transform     ::= { field: mapping }
mapping       ::= literal | from_ref | fn_call
literal       ::= { literal: <JSON value> }
from_ref      ::= { from: "$" identifier }
fn_call       ::= { fn: fn_name, args: { arg_name: mapping } }
fn_name       ::= whitelisted identifier (see J.2)
```

### J.2 v3.0 Whitelisted Fn 库（9 个）

| fn | 签名 | 用途 |
|----|------|------|
| `rfc2822_encode` | `(to, subject, body) → base64 string` | 拼装 RFC 2822 邮件后 base64 |
| `base64_encode` | `(data) → string` | 标准 base64 编码 |
| `base64_decode` | `(data) → string` | base64 解码 |
| `iso8601_now` | `()` → string | 当前 UTC 时间 ISO 8601 |
| `join_array` | `(array, sep) → string` | 字符串数组合并 |
| `template` | `(template_str, bindings) → string` | Mustache 风格 `{{var}}` 替换 |
| `hash_sha256` | `(data) → hex string` | SHA-256 哈希 |
| `url_encode` | `(data) → string` | URL 百分号编码 |
| `html_escape` | `(data) → string` | `<` `>` `&` `"` `'` HTML 转义 |

新增 fn 走 RFC 流程。Plugin 可注册私有 fn 但 **不跨 ATD 部署可移植**——只在注册了该 plugin 的 server 上有效。

### J.3 嵌套 transform 示例

```yaml
transform:
  raw:
    fn: base64_encode
    args:
      data:
        fn: template
        args:
          template_str: "From: {{from}}\r\nTo: {{to}}\r\nSubject: {{subject}}\r\n\r\n{{body}}"
          bindings:
            from: { literal: "me@example.com" }
            to:      { from: $to }
            subject: { from: $subject }
            body:    { from: $body }
```

### J.4 错误处理

Transform 失败（fn call 异常、missing field、类型错）返回 `InvalidArguments` 错误（Appendix B），`error.field` 指向失败的 alias param，`error.reason` 描述（"fn rfc2822_encode failed: to address invalid"）。

### J.5 Conformance

Appendix C conformance test suite 包含 alias transform 的 ~12 个用例（涵盖所有 9 个 fn + 嵌套 + 错误路径）。

---

## Appendix K: Result Middleware 规范

v3 §2.7 中间件 pipeline 的完整接口和 builtin 规范。

### K.1 Middleware 接口

```rust
trait ResultMiddleware {
    fn id(&self) -> &str;                // e.g., "pii_redact"
    fn apply(
        &self,
        tool_id: &str,
        result: ToolResult,
        config: MiddlewareConfig
    ) -> MiddlewareOutput;
}

enum MiddlewareOutput {
    Pass(ToolResult),                    // 透传或 transform 后的结果
    Warn(ToolResult, Warning),           // 通过 + 告警
    Block(BlockReason),                  // 拦截，返回错误
    Error(MiddlewareError),              // middleware 自身失败
}
```

### K.2 内置 5 个规范

#### K.2.1 `pii_redact`

**Config**：
```yaml
type: pii_redact
fields: [list of field paths]        # 支持 JSONPath
patterns: [ssn | phone | email | address | name | custom_regex]
mode: warn | transform | block
```

**行为**：扫描 `fields` 指定字段中匹配 `patterns` 的内容；按 `mode` 处理：
- warn：annotate + pass
- transform：替换为 `[REDACTED:<pattern>]`
- block：整个 result 返回 Block

#### K.2.2 `prompt_injection_scan`

**Config**：
```yaml
type: prompt_injection_scan
detector_ref: atd:middleware/builtin/basic_injection | external plugin id
mode: warn | block
```

**行为**：
- `basic_injection`：内置 regex pattern 匹配（"Ignore previous instructions"/"Disregard all prior"/"New system prompt"/...）
- 外部 detector：调用配置的 plugin（如 Model Armor API）
- 结果 confidence > threshold → 按 mode 处理

#### K.2.3 `trim`

**Config**：
```yaml
type: trim
strategy: chars | tokens | top_items_by_path
limit: <integer>
```

**行为**：transform 模式下强制截断到 limit。`top_items_by_path` 对 JSON 数组保留前 N 项。

#### K.2.4 `format_transform`

**Config**：
```yaml
type: format_transform
target: markdown | flat_json | plain_text
rules: <opaque, per-target>
```

**行为**：把 JSON 结果转为 target 格式（Markdown 表格、扁平化 JSON 等），优化 LLM 消费。

#### K.2.5 `image_strip_metadata`

**Config**：
```yaml
type: image_strip_metadata
fields: [list of image field paths]
strip: [exif_gps | exif_author | exif_all | xmp]
```

**行为**：对 binary image 字段原地清 EXIF/XMP metadata。

### K.3 Plugin 注册

外部 middleware 通过 server 启动时加载 plugin：

```
plugins/result_middleware/
  modelarmor-0.2.so       # 动态库 or executable
  modelarmor.toml         # plugin 元数据
```

Plugin 必须实现 `ResultMiddleware` trait（ABI 稳定）。Plugin 可以 declare 自己的 config schema，在 tool / server config 里引用。

### K.4 Pipeline 执行

```
for mw in tool.result_middleware + server.global:  # 合并后按声明顺序
    match mw.apply(tool_id, current, config):
        Pass(r)      → current = r
        Warn(r, w)   → current = r; current.metadata.warnings.push(w)
        Block(reason)→ return Error(ResultBlocked(reason))
        Error(e)     → if mw.on_failure == fail_closed:
                         return Error(MiddlewareError(e))
                       else:
                         current.metadata.middleware_errors.push(e)
return current
```

### K.5 Audit trail

每次 apply 写 audit log（不含原始数据，用 hash 指纹）：

```json
{
  "ts": "2026-04-21T10:15:00Z",
  "call_id": "abc123",
  "middleware": "pii_redact",
  "input_hash": "sha256:...",
  "output_hash": "sha256:...",
  "mode": "transform",
  "action_taken": "redacted_3_fields",
  "warnings": []
}
```

---

## Appendix L: gws-Pattern Absorption Rationale

gws CLI (github.com/googleworkspace/cli) is an agent-native CLI for Google Workspace APIs. Its architecture (detailed in `Agent_Native_CLI_Architecture_Patterns.md`) exhibits 15 reusable patterns. This appendix documents **which 4 were absorbed into ATD v3** and **why 11 others were not**.

### L.1 Absorbed (4 patterns)

| gws pattern | ATD v3 home | Rationale |
|-------------|-------------|-----------|
| Trust boundaries (CLI args untrusted / env vars trusted) | §2.7 + Appendix K (audit trail) + Appendix L.3 below | Explicit documentation protects implementers against injection attacks; the pattern generalizes cleanly to protocol layer |
| Output sanitization / Model Armor | §2.7 Result Middleware | Core pattern: post-processing pipeline is valuable enough to standardize at protocol layer so any ATD server can opt in |
| Helper overlay (`+send` ergonomic commands) | §2.8 Ergonomic Aliases | Agents benefit from simpler surfaces; formalizing as protocol lets aliases be community-contributed |
| Progressive disclosure (help → schema → dry-run → execute) | §7 (existing) + §2.5 capability discovery + Appendix B (DryRun error if not wired) | Already largely in v2; v3 tightens with `client.devices()` discovery + explicit dry_run error code |

### L.2 Not Absorbed (11 patterns and why)

| gws pattern | Why NOT absorbed |
|-------------|------------------|
| Schema-driven dynamic CLI | Is an **implementation pattern** for a specific CLI; ATD protocol remains transport/CLI-neutral |
| Two-phase parsing (raw arg scan then clap) | CLI-specific implementation detail |
| Stable exit codes 0-5 | Recommended in **atd CLI** spec (MVP design doc §4), not the protocol itself |
| JSON-first stdout | atd CLI convention, not protocol |
| Schema introspection (gws schema) | Already covered by v2 `describe` API |
| Dry-run mode | Schema field exists in v2; implementation tracked in issue file; no new v3 surface |
| Token-based env var auth | atd CLI / client-SDK convention; protocol already has richer UCAN |
| Body schema validation | v2 Dispatch Layer step 3 already requires this |
| Blocked methods hidden from skills | v2 4-level visibility covers this |
| Self-generating skills (hourly CI) | Operational pattern, not protocol — left to individual atd-compatible CLIs |
| Caution blocks in skill markdown | Skill domain, not ATD protocol |

### L.3 Trust Boundaries in ATD (explicit v3 documentation)

v3 formally documents the trust model, inherited from gws:

| Input source | Trust level | Validation |
|-------------|------------|-----------|
| Tool call arguments (from agent) | **Untrusted** (agent may be compromised or hallucinate malicious args) | Tool definition schema validation; capability token; middleware (PII scan, injection detect) |
| Capability tokens | Partially trusted (signed, but TTL/revocation required) | Signature verify; revocation check; UCAN attenuation chain |
| Server config (atd-server.yaml) | **Trusted** (operator controls) | None (filesystem ACL is sufficient) |
| Environment variables | **Trusted** (operator controls) | None |
| Middleware plugin binaries | Trusted via install-time verification | Checksum + provenance attestation (K.3 plugin registration) |
| Tool results (from tool execution) | **Untrusted** (tool may be compromised or return adversarial payload) | Result middleware pipeline (§2.7) |

This mirrors gws's guidance: **agents are adversarial by default, operators are trusted**. Without this mental model, a protocol can be misused to build insecure agents.

### L.4 What atd CLI should adopt (reference)

The atd MVP CLI implementation (`/home/nan/proj/atd-mvp/crates/atd-cli/`) should inherit gws's conventions that live outside the protocol:

- Exit codes 0-5 (Success/Api/Auth/Validation/Discovery/Other)
- stdout=JSON, stderr=hints
- Schema introspection subcommand (`atd schema <tool_id>`)
- Dry-run flag (pending server-side implementation)
- Progressive disclosure in help output

These are **convention**, not **protocol**. They appear in atd-mvp spec rather than this whitepaper.

---

**文档版本**：v3.0 · 2026-04-21 · Multi-Device Extension
**状态**：公开草案；v2.0 仍有效（superset 关系）；征求社区反馈
**许可**：CC BY 4.0
**反馈**：`feedback@atd-protocol.org`（筹建中）· GitHub Issues（github.com/atd-protocol）
**前序版本**：[v2.0 原文](toward-agent-tool-dispatch-v2.md)｜[v1.0 原文（formal）](toward-agent-tool-dispatch.md)
