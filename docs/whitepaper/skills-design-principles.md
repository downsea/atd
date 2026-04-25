# Twelve-Factor Skills

## SKILL.md 时代的 Agent Skills 设计原则

### Design Principles for Agent Skills in the SKILL.md Era

**White Paper v1.1 · 2026-04-22 · ATD v3 Integration Update**

> v1.1 相对 v1.0 的变化：保留 12 条原则不增不减，但在 §III / §V / §VI / §VII / §VIII / §X / §XI 注入 **ATD v3 协议上下文**（多设备 dispatch、distributed sessions、result middleware、ergonomic aliases），每条新增一段代码/案例，并在结语开放问题中更新 ATD 相关议题。v1.0 原始版本通过 git history 可追溯。前置阅读：[ATD v3 whitepaper](toward-agent-tool-dispatch-v3.md)。

---

## Foreword 导读

### SKILL.md 赢了，但好 Skill 是另一回事

2025 年 Anthropic 发起的 **SKILL.md** 已经在 agentskills.io 作为开放标准发布，**30+ 主流 agent 产品**采纳：Claude Code、Claude、Cursor、GitHub Copilot (VS Code + CLI + cloud agent)、Gemini CLI、OpenAI Codex、Goose、Kiro、JetBrains Junie、Databricks Genie、Letta、OpenHands、Factory、OpenCode、Roo Code、Amp、Firebender、Spring AI、Snowflake Cortex Code ……

这是 **LLM 时代第一个真正跨厂商互操作的 skill 格式**。格式的战争已经结束。

但格式的存在 ≠ 好 skill 的存在。

正如 POSIX 1988 年发布后，还需要 K&R、Effective C++、Linux Kernel Coding Style、Google C++ Style Guide 来持续提升代码质量——SKILL.md 之后同样需要**设计原则**来回答：

> 在 SKILL.md 之上，怎样写出在 100+ 规模下依然好用、可维护、可演化的 skill？

### 这份白皮书的角色

本文的蓝本是 **12-Factor App** (Adam Wiggins, 2011)。当年 12-Factor 不定义新协议、不取代 Heroku / Docker，它定义的是"怎样写能在云上长期运行的应用"——12 条简短规则，每条 1 页左右，既工程又文化。十年后 "12-Factor" 成了行业术语。

**本文就是 Skills 版本的 12-Factor**。

我们不做三件事：
1. 不定义新的 skill 格式（SKILL.md 已赢）
2. 不强推任何实现（原则应适用于所有 SKILL.md 兼容系统）
3. 不规避"设计品味"这件事的主观性（这是设计原则，不是协议规范）

我们做三件事：
1. **提炼原则**——从 30+ 产品的实战经验中提炼 12 条可验证的设计原则
2. **给出示例**——每条原则配一个正面示例与一个反面示例
3. **留出空间**——诚实说明每条原则不适用的边界

### 三部分结构

12 条原则按 Skill 的**生命周期三阶段**分组：

```
Part A — Shape         形态     Skill 是什么              §I – §IV
Part B — Behavior      行为     Skill 如何运作            §V – §VIII
Part C — Evolution     演化     Skill 如何变化            §IX – §XII
```

每条原则的结构：罗马数字编号 + 专有名词标题 + 一句口号式副标题 + 1 页展开（原理 + Good/Bad 示例 + 相关原则链接）。

适合从头读一遍建立体系，也适合作为 checklist 查阅。

---

## Part A — Shape 形态

*Skill 是什么——文件、命名、描述、内容的基本形态。*

---

### I. Scope

*Namespace before you publish.*

扁平命名空间在 10K+ skill 规模下必然崩溃。SKILL.md 标准没强制 namespace 语义——这是设计留白，不是建议留白。任何严肃 skill 作者**第一天就应该用带 scope 的名字**。

参考 npm 和 Rust crate 的经验教训：flat 命名空间的早期 npm 让 `request`、`express-handler` 这样的名字在几千个包之后互相冲突。带 `@scope/` 的包名（`@anthropic/skills`、`@vercel/skills-deploy`）解决了这个问题。

好的命名由三部分组成：**scope + category + action + 版本**。

```
@anthropic/doc.pdf@1.2         — 组织.领域.动作@版本
@vercel/deploy-to-preview@0.4
@community/code-review@1.0
```

Scope 的语义：
- `@anthropic`、`@vercel`、`@github` 等机构 scope 表明**发布者身份**
- `@community` 表明社区贡献、无组织背书
- `@user` 或 `@local` 表明本地未发布 skill
- 企业内部可用 `@acme-internal` 避免与公共 namespace 冲突

**✓ Good**

```yaml
---
name: "@anthropic/doc.pdf@1.2"
description: Read and manipulate PDF files ...
---
```

**✗ Bad**

```yaml
---
name: pdf-handler
description: Handles PDF stuff
---
```

没有 scope 的 `pdf-handler` 会在第一百个 PDF skill 发布时彻底失去辨识度。`@acme/pdf-handler` 不会。

**Consequence**：不带 scope 的 skill 终究要被 rename，rename 破坏所有已安装用户的引用。**第一天起 namespace**。

#### ATD v3 下的 scope 衔接

ATD v3 引入 tool namespace（`hms:health.*` / `vendor:huawei:*` / `anos:fs.*`），与 skill scope **正交不冲突**：
- Skill name scope 是**发布者身份**（`@anthropic/`）
- ATD tool namespace 是**能力来源**（`hms:`）

一个 skill 引用 ATD tool 时**两种 scope 同时出现**：

```yaml
---
name: "@acme/health-dashboard@1.0"    # skill 自己的 scope
atd-tools:
  required:
    - hms:health.heart_rate.get        # ATD scope
    - hms:health.sleep.get
    - vendor:xiaomi:light.toggle       # 跨 vendor 也 OK
---
```

`atd-tools` 字段（ATD v2 Appendix G / v3 protocol）声明 skill 依赖哪些 ATD tool。好处：install 时 registry 可以**静态校验**——目标环境是否有这些 tool；若缺失，用户**装 skill 前就知道**。这是"第一天起 namespace"在 ATD 生态的落地方式。

**Related**: III Body（scope 写在 frontmatter 的 name 字段）、VI Least Privilege（`allowed-tools` 引用 ATD pattern）、XII Evolution（版本与 scope 配套）

---

### II. Description

*The description is the API, not metadata.*

对 LLM 可见的 `description` 字段**不是元数据**，它是**接口**。模型读它决定是否激活这个 skill。改 description = 改 API 签名，必须 version-bump。

这是每个 SKILL.md 作者要接受的第一个反直觉事实：你写 description 时**面对的不是用户，是模型**。

三条具体规则：

**(1) Front-load 关键词。** LLM 匹配倾向注意前 20-30 个字符。把意图关键词放开头。

**(2) 声明触发条件。** 不要只说"这个 skill 做什么"，要说"什么时候应该激活它"。前者是产品描述，后者才是 API 契约。

**(3) 控制长度。** SKILL.md 标准允许 description 最多 1024 字符。超过 300 字符基本都是在浪费——LLM 不会读完，用户也不会。

**✓ Good**

```yaml
---
description: |
  Review code changes for security vulnerabilities, focusing on 
  OWASP Top 10, SQL injection, XSS, and auth bypass patterns. 
  Activate when the user asks about security review, code audit, 
  or vulnerability assessment.
---
```

前置关键词（"Review code changes for security"）+ 具体范围（OWASP Top 10）+ 激活条件（"when the user asks about..."）。

**✗ Bad**

```yaml
---
description: A code review tool.
---
```

或：

```yaml
---
description: |
  This skill performs comprehensive code review operations
  including but not limited to style checks, bug detection,
  security analysis, performance profiling, and... [1000 字 novella]
---
```

前者信息量为零，LLM 无法判断何时激活；后者超长且什么都讲 = 什么都不突出。

**Consequence**：Description 写得好的 skill 被正确激活的概率是写得差的 skill 的 10×。这个数字来自 Anthropic Skills 团队的 A/B 数据（Dec 2025 blog）。

**Related**: V Activation（description 是 description-matched 激活的唯一输入）、VII Contracts（边界类型化、正文散文化）

---

### III. Body

*Prose for instructions, schema for contracts.*

SKILL.md 的正文是**写给 LLM 看的指令**。Frontmatter 是**写给 runtime 解析的契约**。两者不能混淆。

常见的错误：
- **契约写成散文**：在正文里用自然语言说"这个 skill 期望一个叫 `date` 的参数，是 YYYY-MM-DD 格式"——LLM 读得懂，但 runtime 无法解析、validator 无法检查
- **指令写成 schema**：用过度结构化的 bullet list / nested JSON 表达推理步骤——LLM 不擅长理解结构化的长指令，散文反而更清晰

Rule of thumb：

```
If a machine needs to read it    →  frontmatter (schema)
If an LLM needs to reason about it →  body (prose)
If a human needs to debug it     →  both should be clear
```

**✓ Good**

```yaml
---
name: "@acme/pdf-extract@1.0"
description: Extract text and tables from PDF files.
allowed-tools: [Read, shell.exec]
compatibility:
  models: ["claude-4.7", "gpt-5"]
---

# Extract PDF

## When to use this skill

When the user asks you to read, summarize, or extract content 
from a PDF file. Prefer this over generic file reading for any 
`.pdf` file larger than a few pages.

## How to extract

1. Use `shell.exec` to call `pdftotext`:
   `pdftotext -layout <input.pdf> -`
2. If tables are important, add `-table` flag.
3. For scanned PDFs, fall back to OCR via `tesseract`.
```

契约（name / description / allowed-tools / compatibility）在 frontmatter，指令在散文 body。

**✗ Bad**

```markdown
# PDF Extract

This skill takes a PDF file. The input parameter is called 
`file_path` and must be a string ending in `.pdf`. The output 
is a string containing the extracted text. It uses the tools 
Read and shell.exec. It is compatible with Claude 4.7 and GPT-5.

Run pdftotext on the file...
```

所有契约散落在散文里——runtime 无法验证、工具无法列出、LLM 要读整个 body 才能弄清楚。

**Consequence**：契约散文化的 skill **无法静态检查**，只能在运行时出错时才发现问题。

#### 案例：在 body 里调用 ATD v3 tool

Skill body 里的每一步**应该调 typed ATD tool 而不是 shell ad-hoc**。这让 dispatch 层的 v3 特性（device routing / capability token / result middleware）全部免费获得。

```yaml
---
name: "@acme/morning-briefing@1.0"
description: |
  Summarize last night sleep + today's calendar + weather.
  Activate when user says 'good morning' or asks for morning
  briefing at any time before noon.

atd-tools:
  required:
    - hms:health.sleep.get              # 从 phone 读 aggregated 睡眠数据
    - hms:location.current.get          # 当前位置
    - hms:site.nearby.search            # 附近 POI（备用）
  optional:
    - calendar.get                       # 日历（可选）
    - weather.get                        # 天气（可选）

allowed-tools: [Read, atd.call]          # 只能读 + 调 ATD
---

# Morning Briefing

## When to use

User says "good morning" / "早上好" / asks for "today's plan" before noon.

## Steps

1. Call `hms:health.sleep.get` with `date: yesterday`.
   ATD v3 routes this to the phone's HMS Health SDK (see device.preferred).
   Note: `source_device_id` may be redacted by result_middleware — don't
   rely on its presence.

2. If user has calendar permission, call `calendar.get` for today.
   If this tool is not available in the current ATD registry (check
   atd-tools.optional), proceed without it.

3. Call `hms:location.current.get` — returns lat/lon regardless of
   whether the user is on phone or car_hmi.

4. If weather.get is available, call it with the location.

5. Synthesize a 100-word briefing in markdown. Highlight anything
   unusual (sleep < 6 hours, big calendar changes, severe weather).
```

关键模式：
- **契约**（`atd-tools` / `allowed-tools`）在 frontmatter
- **指令**（"Call hms:health.sleep.get... Note: source_device_id may be redacted"）在散文
- **optional tool 的优雅降级**写在散文里（LLM 判断），不要写 if-else 硬编码（§IX）

**Related**: IV References（深度材料去 references/）、VI Least Privilege（`atd-tools` 显式声明）、VII Contracts（典型化的输入输出边界）

---

### IV. References

*Defer depth to on-demand files.*

SKILL.md 主文件应该 **≤500 行**。超过这个数，你应该开始把深度材料搬到 `references/` 目录。

三种典型的文件分工：

```
my-skill/
├── SKILL.md            ← 核心指令，LLM 激活时加载（≤500 行）
├── references/         ← 深度材料，按需加载
│   ├── deep-dive.md    ← 高级用法
│   ├── troubleshoot.md ← 问题排查
│   └── api-spec.yaml   ← 外部 API 详细规范
├── scripts/            ← 可执行脚本
│   ├── setup.sh
│   └── verify.py
└── assets/             ← 模板、样例
    ├── templates/
    └── examples/
```

这个结构不是建议，是**Progressive Disclosure 的具体落地**（参见 §V Activation 讨论的激活成本）。

关键纪律：**每个 references/ 文件的存在都必须在 SKILL.md 里被引用**，否则 LLM 不知道它存在。写成这样：

```markdown
## Advanced troubleshooting

For uncommon errors (Windows path quirks, encoding issues, 
broken PDFs), see `references/troubleshoot.md`.
```

这样 LLM 在遇到问题时会主动去读那个文件。

**✓ Good**

```
@anthropic/doc.pdf@1.2
├── SKILL.md                    180 lines
├── references/
│   ├── ocr-fallback.md         120 lines
│   ├── table-extraction.md     90 lines
│   └── windows-paths.md        40 lines
└── scripts/
    └── validate-pdf.sh
```

主文件精简，深度材料按需加载。

**✗ Bad**

```
pdf-handler/
└── SKILL.md       1,200 lines
```

所有内容挤在一个文件——激活一次就是 1200 行进入 context，和 SKILL.md 标准的 O(1) 设计完全背道而驰。

**Consequence**：Monolithic SKILL.md 会**拖垮所有调用它的 agent 的 context**，哪怕 agent 只用其中 10% 的功能。

**Related**: V Activation（references 是 progressive disclosure 的物理实现）、III Body（主文件承担指令，references 承担深度）

---

## Part B — Behavior 行为

*Skill 如何运作——激活、权限、契约、组合的运行时语义。*

---

### V. Activation

*Pick activation mode by intent.*

激活模式是回答一个问题：**谁应该决定何时跑？** 四种模式，对应四种答案。

| 模式 | 谁决定 | 典型 |
|-----|-------|------|
| **Always-on** | 永远加载 | `AGENTS.md`、`CLAUDE.md`、项目编码规范 |
| **Path-scoped** | 文件路径匹配时 | Cursor `.mdc` with `globs: ["src/**/*.py"]` |
| **Description-matched** | LLM 读描述决定 | 标准 SKILL.md 激活 |
| **Explicit** | 用户 `/slash` 触发 | 特殊工具、危险操作 |

四种模式不是可替代的——它们服务不同场景。Skill 作者的责任是**为你的 skill 选对模式**：

- 项目级**永远适用**的规范 → Always-on
- 只在**特定文件类型**有意义 → Path-scoped
- **特定意图**才激活 → Description-matched（这是 SKILL.md 的默认）
- 有**副作用或敏感**的 → Explicit，让用户主动触发

**✓ Good**

```yaml
# A skill that only matters for Python type hints
---
name: "@acme/python-typing@1.0"
description: Improve Python type hints using modern syntax...
paths: ["**/*.py"]   # Path-scoped
---
```

```yaml
# A dangerous skill — explicit only
---
name: "@acme/db-migrate@1.0"
description: Run database schema migrations ...
activation: explicit
user-invocable: true
disable-model-invocation: true
---
```

每个 skill 显式声明它应该如何被激活。

**✗ Bad**

```yaml
# Tries to be everything
---
name: "@acme/mega-skill@1.0"
description: Does many things
paths: ["**/*"]
activation: [always, model, explicit]
---
```

这种 skill 会 always-on（污染每个 context）、匹配所有路径（触发时机失控）、同时又说是模型可激活——没人知道它什么时候跑。

**Consequence**：激活模式选错，就算你写了最好的 description 和 body，skill 也永远**在错的时机**出现——或者**永远不出现**。

#### ATD v3：Device-aware activation

v3 引入的 device affinity 把"何时激活"从**语义维度**扩展到**设备维度**。一个 skill 可能只在**特定设备类**有意义——手表上用不了的 skill 不该在手表 context 里被激活。

SKILL.md 的**扩展字段**（尚无官方规范，本文约定）：

```yaml
---
name: "@acme/driving-assistant@1.0"
description: |
  Help while driving: route planning, nearby POI, hands-free
  music control. Activate when the agent is running on a car HMI
  and the vehicle is in driving mode.

activation: description-matched
device:                              # v3 extension（与 ATD tool 的 device 字段同义）
  preferred: [car_hmi]               # 只在车机上激活
  fallback: []                       # 无 fallback — 不在其他设备激活
  requires:
    sensors: [gps]

atd-tools:
  required: [hms:site.nearby.search, car.navigation.route_to]
---
```

Agent runtime 启动时查询 `client.devices()`，若当前设备类不匹配，**skill 不进入 description match 池**——省下 tokens + 避免在错设备上误激活。

对**多设备 skill**（phone + car 都可用，但行为不同），用双 activation context：

```yaml
device:
  preferred: [phone, car_hmi]
  behavior_by_device:
    phone:
      prefer_display: [screen_medium]    # phone 出卡片
    car_hmi:
      prefer_display: [voice_summary]    # 车机用 TTS
      driving_constraint: safe_always
```

**Related**: II Description（description-matched 激活的唯一输入）、VI Least Privilege（激活的 skill 越多，权限面越大）、XI Compatibility（device 维度的兼容声明）

---

### VI. Least Privilege

*Declare the minimum tool surface.*

Skill 的 `allowed-tools` 列表**不是可选项**，是安全契约。

两个反模式：

**反模式 1：通配符**
```yaml
allowed-tools: ["*"]
```
这等于"给我所有工具"。skill 作者自己都不知道这个 skill 会用什么工具——这是未经设计的 skill。

**反模式 2：吐槽清单**
```yaml
allowed-tools: [Read, Write, Edit, MultiEdit, Glob, Grep, 
                LS, WebFetch, WebSearch, Bash, shell.exec, 
                Task, ... 30 more ...]
```
一个 skill 真的需要 30+ 工具吗？多半是"以防万一都加上"的思维。

**OpenAI 的实测数据**：skill 内工具并发超过 **20 个**，LLM 的工具选择准确率开始明显下降；超过 **30 个**，下降到不可接受。这不是理论——是 function calling 的已知 scaling limit。

**反过来的正模式**：

```yaml
allowed-tools: [Read, Grep]   # 一个代码审查 skill 只需要读和搜索
```

```yaml
allowed-tools: [Read, shell.exec]   # 一个 PDF 提取 skill 只需要读文件和调 pdftotext
```

声明 minimum set 的三个好处：
1. **安全**：agent runtime 可以拒绝 skill 调用未声明的工具
2. **选择准确率**：LLM 视野里工具数量少，选对的概率高
3. **可审计**：看 allowed-tools 就知道 skill 会做什么，不用读 body

**✓ Good**

```yaml
---
name: "@acme/code-review@1.0"
allowed-tools: [Read, Grep, Glob]
---
```

**✗ Bad**

```yaml
---
name: "@acme/code-review@1.0"
allowed-tools: ["*"]  # 或省略此字段
---
```

**Consequence**：不声明最小工具集的 skill 在多 skill 并存时**会互相污染**——LLM 的工具 context 被撑爆，选择开始失准。

#### ATD v3：Pattern-based declaration + capability token

ATD v3 让 `allowed-tools` 支持 **pattern**，使声明既精确又不冗长：

**反模式（pre-v3 习惯，继续）**：

```yaml
allowed-tools:
  - hms:health.heart_rate.get
  - hms:health.sleep.get
  - hms:health.steps.get
  - hms:health.spo2.get
  - hms:health.ecg.get
  # ... 12 more
```

**v3 推荐模式（pattern）**：

```yaml
allowed-tools:
  - "hms:health.*.get"                 # 全部 health read
  - "hms:health.*.set"                 # 需要时再加 write
  - "calendar.read"                    # 非 ATD 也可混列
```

Dispatch 层把 pattern 展开到具体 tool id 做 capability 检查。用户一眼看得清意图（read-only health），不用数清单。

#### 与 ATD capability token 的配合

v3 capability token 支持 **attenuation chain**——从 session token 派生出更窄的 skill token：

```
User session token (grants: "hms:*")
    ↓ skill 激活时，runtime 派生：
Skill token (grants: "hms:health.*.get", ttl: skill_session_only)
    ↓ skill 内调 ATD tool 时附带
ATD dispatch 验证 + 放行
```

Skill 自己**无法拿到超出 allowed-tools 声明范围的权限**——即使 body 指令想调 `hms:health.set`，token 里没这个 grant，dispatch 直接拒绝。

#### 完整示例：code review skill 的最小权限

```yaml
---
name: "@acme/code-security-review@1.3"
description: Review code for OWASP Top 10 vulnerabilities...
allowed-tools:
  - Read                               # 读文件
  - Grep                               # 搜索 pattern
  - atd.call                           # 可调 ATD tool
atd-tools:
  required: []                         # 不依赖特定 ATD tool
  optional:
    - "cve.lookup"                     # 如果环境有 CVE 查询 tool 就用
# NOTE: 没有 Write / Edit / shell.exec — review 是 read-only 操作
---
```

**关键**：`allowed-tools` 是**security contract**。Review skill 如果不小心声明了 `Edit`，用户信任被破坏——他们以为这个 skill 只读。

**Related**: V Activation（多个 always-on skill 叠加，工具面爆炸得最快）、IX Orchestration（工具少 → 路由简单 → 不用 LLM 编排）、VII Contracts（allowed-tools 是最粗粒度的契约）

---

### VII. Contracts

*Typed boundaries, prose interior.*

Skill 的 **输入 / 输出 / 错误** 是**类型化契约**。Skill 的 **内部推理步骤** 是**散文指令**。这两者的边界不能模糊。

SKILL.md 原生标准在这方面做得**不够好**——它只有 `name` / `description` / `allowed-tools` 等 frontmatter 字段，没有强制的 `input_schema` / `output_type`。这是一个已知缺陷。

**补偿办法**：即使 SKILL.md 标准不强制，好的 skill 作者也应该在 frontmatter 里**自愿添加结构化的输入输出描述**（Apple App Intents 和 Semantic Kernel 已经这样做了）：

```yaml
---
name: "@acme/pdf-extract@1.0"
description: Extract text from PDF files.

# Optional but strongly recommended:
input_schema:
  type: object
  properties:
    file_path:
      type: string
      pattern: "\\.pdf$"
    mode:
      type: string
      enum: [text, table, ocr]
      default: text
  required: [file_path]

output:
  type: object
  properties:
    content:
      type: string
      description: Extracted text content
    pages:
      type: integer
      description: Number of pages extracted

errors:
  - code: FILE_NOT_FOUND
    retryable: false
  - code: OCR_UNAVAILABLE
    retryable: false
    fallback: "Fall back to text extraction only"
---
```

**为什么坚持 typed boundaries**：

- 契约的**版本演化可追踪**（schema diff）
- 工具链（tests、docs generation、IDE tooling）可以生成
- **误用**（传错参数类型）在 skill 激活时立即失败，而不是运行时才崩溃

**为什么内部保持 prose**：

- LLM 不善于读过度结构化的长指令
- 推理步骤用自然语言表达更清晰
- 允许 skill 作者写"判断"和"权衡"这种难以结构化的内容

**✓ Good**：Typed frontmatter（schema）+ Markdown body（prose）

**✗ Bad**：两个极端
- "Pure prose" skill：所有契约在散文里（contracts drift 随模型升级）
- "Pure schema" skill：把推理步骤全变成 JSON 步骤树（LLM 读不懂）

**Consequence**：没有 typed contracts 的 skill 在模型升级（Claude 4.7 → 5.0）时**默默漂移**——今天"返回 JSON"的 skill，明天可能返回 markdown，且没有任何 test 能检测到。

#### ATD v3：Downstream contracts 的"中介变换"问题

Skill 调 ATD tool 时，tool 原始输出可能经过 **result middleware pipeline**（v3 §2.7）被改过再送给 skill。Skill 的心智模型必须认识这点。

**典型场景**：

```
Skill calls: hms:health.heart_rate.get
Tool returns: {bpm: 72, source_device_id: "did:device:huawei:watch:xxx", ...}
Middleware pipeline:
  1. pii_redact applied to [source_device_id]
  2. result returned to skill
Skill receives: {bpm: 72, source_device_id: "[REDACTED:device_id]", ...}
```

Skill 在 body 里**不能假设** `source_device_id` 是 MAC 地址——它可能已被 redact。**正确做法**：在 skill 契约里声明你期望的字段**和**默认 middleware 行为：

```yaml
---
name: "@acme/health-alerts@1.0"
atd-tools:
  required:
    - id: hms:health.heart_rate.get
      assume_middleware:                  # v3 声明
        - pii_redact: [source_device_id]  # 我知道这字段会被 redact，不依赖
      forbid_middleware:                  # 防御式声明
        - trim                            # 不允许 server 启用 trim，否则 BPM 数据可能丢
---

# Health Alerts

... call hms:health.heart_rate.get, check bpm field.
Note: source_device_id is redacted by default — use `device.id` 
from the session context instead if you need device identity.
```

**v3 skill 作者的新责任**：
- 知道 server-default middleware（§v3 Appendix K: `prompt_injection_scan` in warn 默认启用）
- 对关键字段声明 `forbid_middleware`（防止下游运维打开 middleware 破坏 skill）
- Skill body 的解释里明示"哪些字段可能 redacted"

#### 与 Ergonomic Aliases（v3 §2.8）的关系

如果 skill 想给下游消费者**再简化一层**，可以通过 tool-level `ergonomic_aliases`（不是 skill 的功能，而是 tool definition 的）。但 skill 本身作为**调用方**应该用 **raw tool id** 而非 alias——alias 的 transform DSL 失败时错误更难定位：

```markdown
# In a skill body

Use `hms:health.heart_rate.get` directly, not the `hms.heart_rate`
alias. The raw tool id is stable; aliases can be removed or changed
by the tool author without notice.
```

**Related**: III Body（边界与内部的分工）、X Testing（typed contracts 让测试可写）、XI Compatibility（中介变换的版本兼容）

---

### VIII. Composition

*Compose across context boundaries, not via text concatenation.*

Skill 调用另一个 skill 的**正确方式**是**派生一个新的 context**（subagent / fork），**不是**把两个 skill 的文本拼到一起。

反模式："Mega-skill"：

```markdown
# Mega Code Assistant

This skill combines code review, test generation, and documentation
generation. It first reviews the code (see steps 1-10 below),
then generates tests (see steps 11-25 below), then writes docs
(see steps 26-40 below)...

[2000 lines follow]
```

问题：
- **Context 污染**：三个 skill 的指令全部进入同一个 context，互相干扰
- **无法独立演化**：改 code review 的一行要动 2000 行文件
- **失去隔离**：测试生成阶段的错误可能通过共享 context 污染下一阶段

**正确模式**：独立的小 skill + subagent 组合：

```markdown
# Code Assistant Workflow

1. First, review the code using `@acme/code-review@1.0`.
   Use `context: fork` so review happens in isolation.
   Wait for summary, then proceed.

2. Based on review findings, generate tests using 
   `@acme/test-gen@1.0` in a new forked context.

3. Finally, generate documentation using 
   `@acme/doc-gen@1.0`, passing both the original code and 
   the review summary.
```

每个子 skill 在自己的 context 里运行，只返回 summary。主 skill 只负责编排。

Claude Code 的 `context: fork` 是这条原则的原生支持。其他 SKILL.md 实现可能通过 subagent API 或 MCP server 调用实现类似效果。

**✓ Good**：小 skill × subagent fork × summary-based integration

**✗ Bad**：Monolithic skill 把所有逻辑挤在一个文件

**Consequence**：违反这条原则的 skill 在规模化时**无法维护**——任何一个阶段的 bug 都会感染整个流程。

#### ATD v3：跨设备组合 — Compose across device boundaries

v3 的 distributed sessions（§2.6）把"context 边界"从 subagent fork 扩展到**设备边界**。一个 skill 可以**跨设备编排**，不只是跨 context。

**案例：Lily 健康异常闭环**（从 v3 §3.4 + §5.1 应用到 skill）：

```yaml
---
name: "@acme/health-anomaly-response@1.0"
description: |
  When health anomaly is detected on a wearable device, route to the
  user's phone for context analysis, then to the car for navigation
  if medical attention is needed. Activate via explicit trigger from
  the watch agent or manual user invocation.

activation: explicit

device:
  preferred: [phone]                     # 主编排在 phone
  fallback: []

atd-tools:
  required:
    - hms:health.heart_rate.get
    - hms:health.sleep.get
    - calendar.get
    - session.handoff                    # v3 distributed session primitive
  optional:
    - car.navigation.route_to            # 有车机才用

allowed-tools: [Read, atd.call, session.handoff]
---

# Health Anomaly Response

## When to use

Activated when the user's watch agent reports a sustained heart rate
anomaly (>20% above baseline for >5 min at rest).

## Steps

1. **Receive context from watch** (watch agent already handed off session
   to phone via `session.handoff(trigger=auto_on_event)`; this skill is
   running on phone with the handoff payload).

2. **Gather context on phone** (this device):
   - Call `hms:health.heart_rate.get` for current reading
   - Call `hms:health.sleep.get` for last night's sleep quality
   - Call `calendar.get` for today's schedule
   All run on phone — high compute, full data access.

3. **Make a recommendation** (LLM reasoning):
   - If sleep was < 5h AND heart rate is elevated → suggest rest
   - If resting heart rate > 100 AND no recent exertion → urgent,
     suggest medical attention
   - Otherwise → informational alert only

4. **If medical attention recommended**, handoff to car:
   - Call `session.handoff(target_device=car_hmi, reason=medical)`
   - This ends this skill's execution on phone
   - A sibling skill `@acme/urgent-medical-route@1.0` picks up on the
     car, calls `car.navigation.route_to` with nearest hospital

5. **Otherwise**, show user the analysis and possibly notify family
   via `hms:push.send`.

## Design notes

- This skill **composes via device handoff**, not text concatenation.
- Each device runs its own skill/context in isolation.
- Summary (anomaly cause + recommendation) travels with the handoff
  as structured session state, not raw dialogue history.
```

关键特征：
- **跨设备 composition** — watch → phone → car 三设备，三个独立 skill
- **Context 不混合** — handoff 传递**结构化 summary**，不是 raw dialogue
- **每个 skill 独立测试/演化** — phone 上的 response skill 变了不影响 car 上的 route skill
- **Capability token 按设备 attenuate**（§VI 描述）

#### 反模式：把跨设备塞进单 skill

```markdown
# Mega Health Skill

## Step 1: Read watch sensor
... [pretend we're on watch]
## Step 2: Analyze on phone
... [pretend we're on phone now]
## Step 3: Drive to hospital
... [pretend we're in the car]
```

这是 v1.0 原文批评的 "mega-skill" 反模式在跨设备场景的放大版——违反 §VIII **更严重**，因为 context 边界是物理的（不同设备内存），硬 concatenation 根本不工作。

**Related**: IV References（同一 skill 内的分层）、IX Orchestration（编排用代码，不用 LLM）、XI Compatibility（跨设备 skill 需声明 device 兼容矩阵）

---

## Part C — Evolution 演化

*Skill 如何变化——编排、测试、兼容性、版本的生命周期管理。*

---

### IX. Orchestration

*Code routes, LLM reasons.*

控制流（路由、重试、fan-out、聚合、条件分支）**用确定性代码**表达。把 LLM 留给**真正需要推理**的步骤。

这是所有多 skill 工作流都要面对的选择：**何时用代码，何时用 LLM？**

**用 LLM 的时机**：
- 需要**判断**：内容是否符合某个标准？这段代码安全吗？
- 需要**生成**：写一段文档、解释一个错误、重构一段代码
- 需要**理解非结构化输入**：解析用户意图、从 log 中提取异常

**用代码的时机**（这才是核心陷阱）：
- **路由**：if X then skill A else skill B
- **重试**：调用失败 → 退避 → 再调用
- **聚合**：多个 skill 的结果合并成一个
- **条件跳过**：某个 skill 失败了 → 跳过不做、走 fallback
- **状态管理**：上一个 skill 的输出作为下一个的输入
- **循环**：对 N 个文件各自跑一次 skill

研究已经反复验证（LangGraph、Dify、Self-Healing Router 的 paper）：**把路由从 LLM 搬到代码**，可以减少 **93%** 的 LLM 调用数量，且**准确率更高**。

SKILL.md 的内建 `!\`command\`` 和 `$ARGUMENTS` 就是为了这个——让 skill 作者在 skill 内混合**确定性 shell 命令**和 LLM 推理：

```markdown
# Deploy Pipeline

First, run the pre-deploy checks:
!`npm run test && npm run build`

If the above succeeded, $ARGUMENTS contains the target environment.
Now analyze the deploy target for any unique concerns...

[LLM reasoning continues here]
```

Shell 命令做确定性的事（测试、构建），LLM 做需要判断的事（分析部署环境）。

**✓ Good**：`!` shell injection 做 routing / side-effect，LLM 做 reasoning

**✗ Bad**：把 "如果 output 包含 error 就再试一次" 写进 skill body 让 LLM 循环执行——这不是 reasoning，这是控制流

**Consequence**：让 LLM 做编排 = token 成本 5-10 倍 + 延迟 5-10 倍 + 准确率下降。是双重输。

#### ATD v3：跨设备路由 — 代码 vs LLM

v3 让 "code routes, LLM reasons" 扩展到**跨设备路由**——同样的原则，但决策空间更大。

**用代码**（dispatch layer 自动处理）：
- Device affinity 路由：`device.preferred: [watch]` → dispatch 自己选手表 binding
- Binding fallback：手表离线 → 退回 phone REST
- Driving constraint 检查：`is_driving?` → 拒绝 `requires_parked` tool
- Session handoff：proximity 检测到新设备 → 自动迁移

这些**根本不该进 skill body**。Skill body 只需说 "调 `hms:health.heart_rate.get`"，dispatch 自己把它路由到对的设备、对的 binding。

**用 LLM**（skill body 里的推理）：
- 判断 "sleep < 5h 且心率偏高" 是否构成"紧急"
- 生成给家人的通知措辞
- 理解用户"我有点不舒服"的模糊意图

#### 典型反模式：在 skill body 里手写 device detection

```markdown
# BAD: Health Alert skill

1. Check if user has a watch:
   !`atd list --device-type watch --online`
2. If watch detected, call hms:health.heart_rate.get on it.
3. Otherwise, fall back to phone sync data.
```

这是把 dispatch 层的工作**重做一遍**——脆弱（shell 输出解析不稳）、冗余（dispatch 自己会做）、拖累（多跑两次工具）。

**正确**：

```markdown
# GOOD: Health Alert skill

1. Call `hms:health.heart_rate.get`.
   ATD dispatch handles device selection (watch preferred, phone fallback,
   rest as last resort). Don't second-guess the routing.
```

**原则保持不变**——v3 只是让 "code routes" 的 code **越来越少要你自己写**，dispatch 层承担更多。

**Related**: VIII Composition（skill 间组合）、VI Least Privilege（工具面小 → 路由简单）、XI Compatibility（dispatch 层的行为由协议版本决定）

---

### X. Testing

*Test behavior across models, not implementation.*

Skill 的测试不是 unit test——是**行为测试矩阵**。

**三种测试**，按重要性：

**1. 行为断言（必须）**：给定输入场景，skill 必须 / 不能触发什么工具、输出必须 / 不能包含什么信息。

**2. 跨模型矩阵（必须）**：同一个测试，在你声明的每个兼容模型上都跑。Claude Opus 4.7 → 5.0 很可能改变 skill 行为。

**3. Regression test（强烈推荐）**：把过去用户反馈的 bug case 转成测试。

示例测试文件：

```yaml
# test.yaml — next to SKILL.md
tests:
  - name: "security review of SQL-injection-like code"
    user_input: "Review this login function for security issues"
    fixture: "fixtures/vulnerable-login.py"
    expected_behavior:
      must_call_tools: [Read, Grep]
      must_not_call_tools: [Write, Edit]  # Review is read-only
      output_must_contain: ["SQL injection", "OWASP"]
      output_must_not_contain: ["I cannot review", "I'm unable"]
    model_matrix:
      - claude-opus-4-7
      - gpt-5
      - gemini-3.1-pro

  - name: "refuses to review non-existent file"
    user_input: "Review ./does-not-exist.py"
    expected_behavior:
      must_call_tools: [Read]
      output_must_contain: ["file", "not found"]
    model_matrix: [claude-opus-4-7]
```

SKILL.md 标准**没定义**测试格式——这是它的空白。上面是一种可能的约定，其他约定都可以。关键是**必须有测试**。

为什么不做 unit test：skill 的"正确性"不是 function 输入输出正确，而是**LLM 在看到这个 skill 后是否做出预期行为**。这是一种**行为契约**，不是代码契约。

**✓ Good**：每个 skill 配 `test.yaml`，断言工具调用 + 输出模式，跨多个模型跑

**✗ Bad**：没有测试。或者只测了"skill 能被 parse"这种浅层事情

**Consequence**：不测试的 skill **一定会在模型升级时悄悄失效**——这不是假设，这是实证。

#### ATD v3：Device matrix + driving_constraint 测试

v3 让 skill 可以跨 7 设备类运行——测试矩阵必须**同时**跨模型和跨设备。只测 model 不测 device，会漏掉设备类差异导致的失效（watch 上 binding 被拒、car 上 driving_constraint 命中等）。

升级后的 test.yaml：

```yaml
tests:
  - name: "health alert on watch → phone handoff"
    user_input: "my heart feels funny"
    fixture:
      current_device: watch
      user_fleet: [watch:huawei:watch4, phone:huawei:mate80]
      watch_sensor_reading: {heart_rate: 135, at_rest: true}
    expected_behavior:
      must_call_tools:
        - hms:health.heart_rate.get
        - session.handoff
      handoff_target: phone
      output_must_contain: ["elevated heart rate"]
    model_matrix:
      - claude-opus-4-7
      - gpt-5
    device_matrix:                         # v3 新增
      - watch:huawei:watch4
      - watch:apple:watch-ultra-2          # 跨 vendor 也测
      
  - name: "driving-safe navigation request"
    user_input: "take me to the nearest hospital"
    fixture:
      current_device: car_hmi
      vehicle_state: {is_driving: true, adas_level: 2}
    expected_behavior:
      must_call_tools:
        - car.navigation.route_to
      must_not_call_tools:
        - car.settings.change_layout      # driving_constraint: requires_parked
      driving_constraint_respected: true
    device_matrix:
      - car_hmi:huawei:aito-m9
      - car_hmi:apple:carplay              # CarPlay 应 graceful degrade
    
  - name: "degrades gracefully when watch offline"
    user_input: "what's my heart rate"
    fixture:
      current_device: phone
      user_fleet: [phone:huawei:mate80]    # watch 不在线
    expected_behavior:
      must_call_tools:
        - hms:health.heart_rate.get
      tool_binding_used: rest               # fallback 到云
      output_must_contain: ["last synced"]  # 明示是同步数据
```

**三类 v3-specific 断言**：
- `device_matrix: [...]` — 枚举要测的设备
- `handoff_target: ...` — 期望 session 迁到哪
- `tool_binding_used: rest|appfunction` — 哪种 binding 被选
- `driving_constraint_respected: true` — 驾驶安全约束触发
- `middleware_applied: [...]` — 哪些 result middleware 跑了

#### 避免 "device class cartesian explosion"

别真的测 model × device 全部组合（5 model × 7 device = 35 case/test）。实用策略：

- **Primary matrix**：当前用户最多的一个 model × 每个 device（N=7）
- **Secondary matrix**：每个 model × primary device（通常是 phone，N=5）
- **Spot check**：其余由 nightly CI 跑，不阻塞 PR

总 test 量约 12-15 个 case/test，而非 35。

**Related**: VII Contracts（typed contracts 让断言可写）、XI Compatibility（测试矩阵依赖声明的兼容性）

---

### XI. Compatibility

*Declare your assumptions. Skills age.*

Skill rot 是**真实的**。2024 年写给 Claude 3.5 的 skill，2026 年在 Claude 4.7 上行为可能已经微妙地不同。不要假装这不发生。

SKILL.md 标准把 `compatibility` 列为**可选**字段——但"可选"不意味着"不重要"。任何希望 >6 个月存活的 skill 都应该声明：

```yaml
---
name: "@acme/code-review@1.2"
compatibility:
  models:
    - claude-opus-4-7
    - claude-sonnet-4-6
    - gpt-5
    - gemini-3.1-pro
  min_context_window: 100000
  required_capabilities:
    - tool_use
    - extended_thinking
  tested_on:
    - {model: claude-opus-4-7, date: 2026-04-15}
    - {model: gpt-5, date: 2026-04-15}
  known_issues:
    - model: gpt-5-turbo
      issue: "Tends to skip OWASP checks; use full gpt-5 instead"
---
```

四个层面：
- **支持的模型**：明确列出，不要说"应该在所有模型上工作"
- **最小 context**：有些 skill 的 references/ 加起来需要 64K+，不能在 32K 模型上跑
- **需要的 capability**：`tool_use`、`extended_thinking`、`vision` 等
- **已知问题**：诚实列出在哪些模型上行为异常

**为什么这必须写**：
- 用户安装 skill 时**第一眼看到兼容性**——可以立即知道能不能在自己的 setup 上跑
- 模型升级时，有 `tested_on` 的 date，维护者知道"上次测试是半年前，现在该重测了"
- `known_issues` 给下一代维护者留下"考古记录"——你离开项目，接手的人从这里开始

**✓ Good**：明确列出支持与测试的模型 + 日期 + 已知问题

**✗ Bad**：完全省略 `compatibility` 字段，**默默假设**"最新模型应该可以跑"

**Consequence**：不声明兼容性的 skill **不会** magically 在新模型上继续工作——它们会在新模型上默默退化，用户只看到"skill 最近变笨了"，无从追溯。

#### ATD v3：Compatibility 的五个维度

v3 让 compatibility 必须声明五个维度，不只是 model：

```yaml
---
name: "@acme/driving-assistant@1.5"
compatibility:
  # 1. Model（v1.0 已有）
  models:
    - claude-opus-4-7
    - gpt-5
    - gemini-3.1-pro
  min_context_window: 100000
  required_capabilities: [tool_use, extended_thinking]
  
  # 2. Protocol version (v3 新增)
  min_atd_version: "3.0"               # 要 v3 的 driving_constraint 等
  
  # 3. Device classes (v3 新增)
  devices:
    supported: [car_hmi, phone]         # 这两类可用
    preferred: [car_hmi]                # 首选车机
    unsupported: [watch, earbuds, tv]   # 明示不支持
  
  # 4. Vendor bindings (v3 新增)
  vendor_bindings:
    required: []                        # 不依赖特定 vendor
    known_working:
      - {vendor: huawei, platform: harmonyspace_6}
      - {vendor: apple, platform: carplay, note: "degraded — no CAN bus"}
      - {vendor: google, platform: android_auto, note: "degraded — no ADS"}
    known_broken:
      - {vendor: tesla, platform: car_system, reason: "no public API"}
  
  # 5. Capabilities (ATD v3 feature flags, 新增)
  atd_capabilities:
    required:
      - device.preferred                # 依赖 device affinity
      - driving_constraint              # 依赖驾驶安全检查
    optional:
      - session.handoff                 # 有就用，没有也 OK
      - result_middleware               # 若启用，PII 会被 redact
  
  # 已有字段
  tested_on:
    - {model: claude-opus-4-7, device: huawei_aito_m9, date: 2026-04-22}
    - {model: gpt-5, device: carplay_simulator, date: 2026-04-20}
  known_issues:
    - device: carplay
      issue: "CAN bus read not available; falls back to GPS only"
    - device: android_auto
      issue: "ADS queries return null; manual nav only"
---
```

**五个维度的独立演化**：

- Model 升级（Claude 4.7 → 5.0）→ 更新 `models` + `tested_on`
- ATD 协议升级（v3.0 → v3.1）→ 更新 `min_atd_version`
- 新设备类上市（HarmonyOS PC 2026-04 新增）→ 更新 `devices.supported`
- Vendor 生态变化（HIMA 新增 OEM）→ 更新 `vendor_bindings`
- ATD feature 演化（replication 从 optional 到 required）→ 更新 `atd_capabilities`

#### Skill rot 的两倍加速

v1.0 时 skill rot = model 升级导致行为漂移。v3 时 skill rot 来源**翻倍**：

- Model 升级：同 v1.0
- **设备 OS 升级**：HarmonyOS 5 → 6 → 7 API 改动
- **Vendor binding 演化**：HMS Health 升级 API
- **ATD 协议扩展**：v3.1 新增 middleware，改变默认行为

对策：`tested_on` 里的 date 要足够新（半年以内）；CI 每季度自动跑 `tested_on` 的完整矩阵；`known_issues` 诚实列出。

**Related**: X Testing（兼容性声明需要测试验证）、XII Evolution（版本与兼容性一起演化）、V Activation（device-aware activation 呼应 compatibility.devices）

---

### XII. Evolution

*Version before you need to deprecate.*

**第一天就用 semver**——哪怕你的 skill 只有一个用户（你自己）。

Semver 对 skill 的语义：

- **MAJOR**：breaking change（改了 input schema、改了 output 结构、移除了某个 tool、重写了激活条件）
- **MINOR**：向后兼容的扩展（新增了 optional 参数、新增了 intent_examples、提升了准确性但不改契约）
- **PATCH**：bug 修复（拼写错误、描述清晰化、references/ 更新）

**Deprecation 纪律**：

Breaking change 不能**原地发生**。正确流程：

```
v1.0.0   首次发布
v1.1.0   minor 版本：新增 feature + 标记某字段为 deprecated_in: "2.0.0"
v1.2.0   另一个 minor：继续存活，给用户时间迁移
v2.0.0   major 版本：移除 deprecated 字段，文档说明迁移路径
```

**两个 minor 的过渡期**是最小单位——少于这个，用户来不及响应。

**Hot-patch 反模式**：

最常见的错误是"悄悄改一行 SKILL.md 内容，不升版本号"。想想看：

- 用户 A 的 agent 在 v1.0 下构建了心智模型（"这个 skill 会做 X 后做 Y"）
- 你改了 body，让它做 Y 后做 X
- 用户 A 的 agent 下次激活 skill 时**行为突变**，用户 A 不知道为什么

**永远 version-bump，即使改动 looks 微小**。Agent 的 context 里可能缓存了旧 skill 的理解——version change 是唯一的信号。

**✓ Good**

```yaml
# v1.2.0 → v2.0.0 planned migration
name: "@acme/deploy@1.2.0"
deprecated:
  - field: env
    deprecated_in: 1.2.0
    removed_in: 2.0.0
    migration: "Use environment instead. See references/migration-v2.md"
```

**✗ Bad**：直接修改 skill 内容，保持版本号不变，"就当没事发生"。

**Consequence**：Hot-patch 不 version-bump = 对所有下游 agent 的**静默 breaking change**。等你发现用户抱怨时，已经没人知道到底改了什么。

**Related**: I Scope（scope + version 一起定义身份）、XI Compatibility（版本升级时同步更新 compatibility）

---

## Afterword 结语

### 这 12 条不是什么

- **不是 SKILL.md 的竞争规范**。SKILL.md 赢了，我们在它之上建楼。
- **不是强制标准**。违反这些原则的 skill 仍然可以工作——但不会**长期**工作、**规模化**工作、**优雅地**工作。
- **不是静态的**。这是 v1.0。五年后 skill 生态演化出新的 pattern，这份文档也会演化。
- **不是"必须全部满足才算好 skill"**。单独任何一条都能让你的 skill 变好。全部 12 条是 production-grade 的参考。

### 这 12 条是什么

**一份设计品味的基准**。当你下次写 SKILL.md 时，这 12 条可以作为心里的 checklist。当你 review 别人的 skill 时，它们是共同语言。当你解释"为什么这个 skill 质量更高"时，它们是**可引用的理由**。

### 开放问题

这份白皮书刻意没有回答的问题（因为生态还太早，答案不成熟）：

**OP-1：Testing 的格式标准**
Skill 测试的具体格式（test.yaml? testing.md? 独立的 framework?）尚无共识。§X 给出的是一种可能的形式。v1.1 加入了 device_matrix，但**整体 test 格式仍待收敛**。

**OP-2：Trust / Signing**
谁给 skill 签名？签名验证在 agent runtime 里如何执行？SKILL.md 标准没规定。这是一个需要 ecosystem-level 协议的问题，不是单个 skill 作者能解决。ATD 的 capability token（UCAN）**部分解决了运行时授权**，但 **skill 静态源码的完整性签名** 仍未标准化。

**OP-3：Federated Discovery**
100 个 registry 并存时，agent 如何决定去哪里搜 skill？skills.sh 是一个答案，但不是唯一的。

**OP-4：Intent Catalog**
Apple App Intents / Android App Actions 证明了**策划的 intent 词汇表**在消费端 AI 有巨大价值。Skills 生态应该有类似的平台策划 BII 吗？谁来策划？ATD v3 的 `intent_examples` 字段是起点，但**规范化的 intent 词汇表**谁来维护？

**OP-5：Multi-lingual Skills**
`intent_examples` 应该覆盖多种语言吗？中文用户"开灯"和英文用户"turn on the light"是否该同一个 skill？这既是技术问题也是 i18n 的工程问题。

**OP-6：Cross-device skill state**（v1.1 更新）
ATD v3 引入了 `session.migrate` / `session.fork` 原语（§VIII 详述），但**skill 层跨设备状态 schema** 仍是 open。当一个 skill 在 watch 上记了"baseline heart rate = 68"，handoff 到 phone 后这个 fact 怎么传递？是走 session state、走外部 memory store、还是走 skill 间的显式 context hand-over？

**OP-7：Skill 作为 Agent 的 First-class Capability vs Tool 的区别**
Skill 和 Tool 的边界到底在哪？当一个 skill 只有 `!shell.exec` 一行时，它是 skill 还是 tool？这是未解决的**类型学**问题。ATD v3 §2.8 的 ergonomic_aliases **把 skill-like 简化接口下沉到 tool 层**——某种程度上，"tool + alias" 可以 cover 某些"简单 skill"的场景。边界更模糊了。

**OP-8：Skills Economics**
付费 skill、企业私有 skill registry、skill 质量保障的商业模式——这些会如何形成？这不仅是技术问题，更是生态治理问题。

**OP-9：ATD 与 Skills 的分层演化（v1.1 新增）**
ATD v3 的 `atd-tools` 字段（白皮书 Appendix G）是 skill YAML 的扩展，但**这个字段应该由 agentskills.io spec 正式吸纳，还是保留在 ATD 生态的命名空间**（`x-atd:`）？若采纳，以何种速率演化？

**OP-10：Device catalog 的策划（v1.1 新增）**
v3 §2.5 规范化了 10 个 device type + 20 个 capability tag，但**未来扩展**——折叠手机算 phone 还是 tablet？AR 眼镜算 head_unit 还是 vr_headset？XR 设备？——谁决策词表加新值？APWG 走 RFC，但**谁维护 skill 侧对这些新值的最佳实践**？

**OP-11：Multi-device skill 的行为一致性（v1.1 新增）**
同一个 skill（如 `@acme/morning-briefing`）在 phone 和 car_hmi 上应该产出**不同 output shape**（phone 卡片 vs 车机 TTS 语音）。目前靠 skill body 的 LLM 推理 + `output_hint.prefer_display`——但**如何测试行为一致**（两个设备上给出的核心信息集相同）？是否需要"device-invariant assertion"这样的新测试概念？

### 下一步

这份白皮书是 v1.0。欢迎来自任何使用 SKILL.md 的团队、任何实现 SKILL.md 兼容 agent 的产品、任何独立开发者的反馈。

本文献给那些花了周末写 SKILL.md 又被模型升级打脸的开发者。你们的挫折感是真实的、合理的，也是这份文档存在的理由。

**如果这 12 条中有哪怕一条让你写下一个 skill 时少踩一个坑——这份白皮书就值得。**

---

**文档版本**：v1.1 · 2026-04-22 · ATD v3 Integration Update
**前序版本**：v1.0 · 2026-04（git history 可追溯）
**状态**：公开草案，征求反馈
**许可**：CC BY 4.0
**反馈**：GitHub Issues（待启动）· 或直接引用到任何采用 SKILL.md 的生态讨论

**v1.1 更新摘要**：
- §I / §III / §V / §VI / §VII / §VIII / §IX / §X / §XI 注入 ATD v3 协议上下文
- 新增跨设备 skill 案例（Lily 健康异常闭环）
- 新增 `atd-tools` / device affinity / result middleware / ergonomic aliases 与 skill 设计的交互
- Compatibility 从 model 单维度扩展到 5 维度（model × protocol × device × vendor × capability）
- Test matrix 加入 device_matrix
- Afterword 新增 OP-9/10/11 三个 v3 时代的开放问题
- 12 条原则**不加不减**——保 12-Factor 品牌单一性

**致谢**：本文受以下工作启发
- **SKILL.md 开放标准** (Anthropic, 2025) · agentskills.io
- **ATD v3 whitepaper** (2026-04, 本项目): [toward-agent-tool-dispatch-v3.md](toward-agent-tool-dispatch-v3.md)
- **12-Factor App** (Adam Wiggins, 2011)
- **Unix Philosophy** (Mike Gancarz, 1994)
- **The Zen of Python** (Tim Peters, 1999)
- **Effective C++** (Scott Meyers, 2005)
