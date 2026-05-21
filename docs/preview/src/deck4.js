// Deck 4 — 第四部分 · 实施案例与集成方法
const T = require("./theme");
const { C } = T;
const d = T.newDeck({ part: 4, partLabel: "四", partTitle: "实施案例与集成" });

T.cover(d, {
  title: "实施案例与集成方法",
  subtitle: "ATD 怎么接进主流 agent 工具 —— 五条集成路径、三家真实 adopter、\n五分钟上手，以及跨厂商组合。",
  tagline: "agent 平台不用改一行代码 —— 这是协议设计的目标，也是它的验收标准。",
});

T.agenda(d, {
  items: [
    { h: "五条集成路径", b: "按你的 agent 说什么协议，对号入座" },
    { h: "决策矩阵", b: "按处境选路径，附主文档指引" },
    { h: "框架兼容性", b: "谁已端到端验证、谁协议兼容" },
    { h: "三家 adopter", b: "healthkit / celia / cbrain —— 三种 transport" },
    { h: "publisher 视角", b: "从 healthkit 的失败→修复弧学到什么" },
    { h: "五分钟上手 + 跨厂商组合", b: "跑起来，再把两家拼到一个 agent" },
  ],
});

T.bullets(d, {
  title: "五条集成路径 —— 按 agent 说的协议对号入座",
  kicker: "集成路径",
  intro: "每个 agent 系统都落在五条路径之一，取决于它原生会说哪种协议面。",
  items: [
    { h: "路径 1 · 直连 SDK", c: C.teal, b: "import atd-sdk，自己写 agent loop，直接 discover / describe / call。适合自写 agent loop、Python 或 Rust。已发布。" },
    { h: "路径 2 · MCP bridge（通用）", c: C.teal, b: "把 atd-mcp-bridge 当 MCP 客户端的子进程跑，客户端不知道 ATD 存在。最高杠杆 —— 一个 binary 触达所有成熟 MCP 客户端。已发布、Hermes 实证。" },
    { h: "路径 3 · 裸 OpenAI / Anthropic API", c: C.blue, b: "as_openai_tools / as_anthropic_tools 产出 provider 期望的 dict 形状，喂进 SDK 的 tools= 参数。覆盖所有 OpenAI 兼容网关。已发布。" },
    { h: "路径 4 · 自研客户端（不支持的语言）", c: C.blue, b: "照 wire spec 写最小客户端（4 字节大端长度前缀 + UTF-8 JSON）。Rust / Python 客户端足够小，可整读作移植参考。" },
    { h: "路径 5 · SKILL.md 平台（规划中）", c: C.amber, b: "一个 atd-dispatch skill 发布一次即触达 26+ 平台。尚未发布 —— 当前走路径 2（MCP bridge）作为过渡。" },
  ],
});

T.table(d, {
  title: "决策矩阵 —— 按你的处境选路径",
  kicker: "决策矩阵",
  intro: "不必纠结架构 —— 看你在做什么，对号入座。",
  head: ["你的处境", "路径", "主文档"],
  colW: [5.4, 1.6, 5.09],
  rows: [
    [{ t: "写 Python agent，用 LangChain", c: C.ink }, { t: "1", c: C.teal, b: true }, "integrations/langchain.md"],
    [{ t: "写 Python agent，用裸 OpenAI / Anthropic SDK", c: C.ink }, { t: "3", c: C.teal, b: true }, "quickstart/python.md"],
    [{ t: "写 Rust agent", c: C.ink }, { t: "1", c: C.teal, b: true }, "quickstart/rust.md"],
    [{ t: "用 Claude Desktop / Claude Code / Cursor", c: C.ink }, { t: "2", c: C.teal, b: true }, "integrations/claude-code.md"],
    [{ t: "用 Hermes Agent", c: C.ink }, { t: "2", c: C.teal, b: true }, "integrations/hermes.md"],
    [{ t: "用其他 MCP 客户端", c: C.ink }, { t: "2", c: C.teal, b: true }, "claude-code.md（模式可迁移）"],
    [{ t: "写 TS / Go / Java agent", c: C.ink }, { t: "4 或 2", c: C.teal, b: true }, "protocol/wire-format.md"],
  ],
});

T.table(d, {
  title: "框架兼容性 —— 谁已验证、谁协议兼容",
  kicker: "框架兼容性",
  intro: "ATD 不要求框架改代码：会说 OpenAI function-calling 走路径 3，会说 MCP 走路径 2。",
  head: ["框架 / 客户端", "路径", "状态"],
  colW: [5.6, 2.2, 4.29],
  rows: [
    [{ t: "LangChain / LangGraph / crewAI", c: C.ink }, "1", { t: "已发布，单测 + 文档示例", c: C.green }],
    [{ t: "OpenAI / Anthropic API 直连", c: C.ink }, "3", { t: "已发布，单测覆盖", c: C.green }],
    [{ t: "OpenAI 兼容网关（OpenRouter / DeepSeek / Groq）", c: C.ink }, "3", { t: "已发布，DeepSeek 经 Hermes 实证", c: C.green }],
    [{ t: "Hermes Agent", c: C.ink }, "2", { t: "已发布，真 LLM 端到端验证", c: C.green }],
    [{ t: "Claude Desktop / Claude Code / Cursor", c: C.ink }, "2", { t: "已发布，配置文档化", c: C.green }],
    [{ t: "Continue.dev / Cline / Zed / Codex(MCP)", c: C.ink }, "2", { t: "协议兼容，未逐一专测", c: C.amber }],
    [{ t: "Go / Java / .NET / TypeScript agent", c: C.ink }, "4", { t: "暂无 SDK（TS SDK 规划中）", c: C.coral }],
  ],
});

T.cards(d, {
  title: "三家真实 adopter —— 三种 transport，三类领域",
  kicker: "实施案例",
  intro: "ATD 1.0 不是纸面协议。三家 adopter 分别压测了 Unix socket、HTTP、Python server 三条路径。",
  cols: 3,
  items: [
    { n: "①", c: C.teal, h: "healthkit_cli", b: "Unix socket / atd-server。首家厂商-server adopter。把华为 HMS HealthKit host 成 ATD server —— 25 个 helper-tool + 多租户 + skill 同步。三轮 case study 的主角。" },
    { n: "②", c: C.blue, h: "celia_phr", b: "HTTP / atd-server-http。首家云端托管 adopter。个人健康档案系统；用 FHIR R4 + HIPAA PHI 两个 middleware 做医疗 payload 的 egress 校验与脱敏。" },
    { n: "③", c: C.amber, h: "cbrain", b: "Python server runtime。首家 Python-host adopter —— 工具 host 必须与 MuJoCo 仿真同进程。具身 agent 的 S2 决策层，用 ATD 作 cognitive plane 的工具调度。" },
  ],
});

T.bullets(d, {
  title: "publisher 视角 —— 从 healthkit 的失败→修复弧学到什么",
  kicker: "实施案例 · 写一个好的 ATD 服务",
  intro: "healthkit_cli 既是首家 adopter，也是「怎么写一个好的 ATD 工具服务」的活教材。",
  items: [
    { h: "helper-tool 胜过 raw endpoint", c: C.green, b: "8 个许可式 schema 端点（24% 成功率）→ 26 个语义清晰的 helper-tool（95%）。工具面是为 LLM 设计的，不是为程序员。" },
    { h: "SKILL.md 走 meta-tool", c: C.teal, b: "26 个 SKILL.md 通过 skills.list / skills.get 暴露 + atd skills sync 安装，而不是手抄进平台目录。" },
    { h: "Hidden 可见性收编原始端点", c: C.teal, b: "容易让 LLM 困惑的原始端点标 Hidden —— 不进 discover 列表，但仍可按 id 调用，留给调试与运维。" },
    { h: "多租户只是加一行", c: C.blue, b: "FileTokenBroker 按 caller_id 路由 token；ServerConfig::token_broker 一行启用 —— 工具自己无需感知多租户。" },
    { h: "glue 很薄", c: C.blue, b: "healthkit serve 是约 150 行 glue（一半是命令行解析）。协议 + atd-runtime 把能力门禁 / 审计 / 限流 / token 的重活全做掉了。" },
  ],
});

T.steps(d, {
  title: "五分钟上手 —— 跑起来，再接进 agent",
  kicker: "集成方法",
  intro: "从 clone 到 agent 看见工具，四步。agent 平台不用改一行代码。",
  items: [
    { h: "跑参考 server", b: "cargo build --release -p atd-ref-server -p atd-cli -p atd-mcp-bridge；atd-ref-server --sock /tmp/atd.sock（自带 10 个内置工具）。" },
    { h: "看一眼", b: "atd --sock /tmp/atd.sock list / schema ref:fs.read / call ref:echo.say --args '{...}'。" },
    { h: "接 Hermes / Claude Code", b: "hermes mcp add 或 claude mcp add，命令指向 atd-mcp-bridge、环境给 ATD_SOCK。agent 立刻看到全部工具，按 description + intent_examples 自动选用。" },
    { h: "写自己的 vendor server", b: "实现 Tool trait（definition + call）→ 注册进 Registry → Server::new(reg, cfg).run()。参考 atd-mock-weather-server，约 80 行。" },
  ],
  note: "桥接靠 atd-mcp-bridge、自写 agent 靠 atd-sdk —— 两条路都不要求改 agent 平台。",
});

T.bullets(d, {
  title: "跨厂商组合 —— 一个 agent，多家工具",
  kicker: "集成方法 · cross-vendor",
  intro: "ATD 是协议 —— 同一个 agent 可以同时连 N 个 ATD server，看到的是合并后的工具目录。",
  items: [
    { h: "各自独立", c: C.teal, b: "每个 ATD server 自己一个 socket、自己一份审计、自己一个 token store —— 互不耦合。" },
    { h: "合并视图", c: C.teal, b: "agent（或桥）对每个 socket 各跑一次 discover，工具目录自然合并成一个 catalog。" },
    { h: "可运行 demo", c: C.blue, b: "scripts/cross-vendor-demo.sh 同时启动 healthkit + atd-mock-weather-server，证明 atd list 对两个 socket 各看到自己的工具。" },
    { h: "CLI 做不到的关键点", c: C.amber, b: "你无法在一个 CLI 进程里同时 +heartrate 和 +weather.now；ATD 把它折叠成「桥接多个 socket」的一行配置。" },
    { h: "典型用例", c: C.green, b: "「我跑 5km 该穿什么」—— agent 同时查天气 server 与健康 server，把跨厂商的数据合成一条建议。" },
  ],
});

T.closing(d, {
  title: "第四部分小结",
  points: [
    "五条集成路径覆盖全部情形：直连 SDK、MCP bridge、裸 provider API、自研客户端、SKILL.md 平台。",
    "MCP bridge 是最高杠杆路径 —— 一个 binary 触达 Hermes / Claude / Cursor 等所有 MCP 客户端。",
    "三家 adopter 在生产中压测了三种 transport：healthkit（UDS）、celia（HTTP）、cbrain（Python server）。",
    "写一个 ATD server 的 glue 很薄 —— 重活在协议与 runtime 里；上手只需五分钟。",
  ],
  tagline: "ATD 的集成验收标准很硬：agent 平台不用改一行代码。",
  next: "下一部分 · 第五部分：路线与生态发展建议",
});

T.save(d, process.argv[2] || "deck4.pptx").then(() => console.log("deck4 ok"));
