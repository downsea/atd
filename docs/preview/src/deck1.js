// Deck 1 — 第一部分 · 背景与必要性
const T = require("./theme");
const { C } = T;
const d = T.newDeck({ part: 1, partLabel: "一", partTitle: "背景与必要性" });

T.cover(d, {
  title: "背景与必要性",
  subtitle: "Agent 调用工具的现状为什么不够用 —— 对比 CLI / MCP / REST，\nATD 究竟解决了哪些现有技术解决不了的问题。",
  tagline: "工具碎片化，是 agent 可靠性的隐形天花板。",
});

T.agenda(d, {
  items: [
    { h: "断层在哪里", b: "Agent 与它要调用的工具之间，缺一层契约" },
    { h: "四种工具形态", b: "CLI / REST / MCP / 原生 SDK —— 互不兼容" },
    { h: "「任意」的四重诉求", b: "任意 agent、框架、平台，调任意工具" },
    { h: "ATD 的定位", b: "一层中立的 agent ↔ 工具调度协议" },
    { h: "实证：三轮 case study", b: "成功率 24% → 95% → 2-vs-8 次调用" },
    { h: "对比 CLI / MCP / REST", b: "逐项拆解它们解决不了的问题" },
    { h: "ATD 独有的痛点闭环", b: "协议层一次解决，而非每家重写" },
  ],
});

T.statement(d, {
  kicker: "问题",
  big: "LLM agent 的能力上限，往往不在模型本身，\n而在它能不能可靠地「把工具调对」。",
  sub: "模型推理逐年变强；但 agent 真正要调用的工具，今天仍是一堆形态各异、各说各话、" +
       "缺少统一契约的接口。调不对、调错路径、调用不可观测 —— 这些都不是模型问题。",
  attin: "实证见后文 healthkit_cli 三轮 case study（同模型、同 prompt、可复算）。",
});

T.cards(d, {
  title: "Agent 面对的，是四种互不兼容的工具形态",
  kicker: "现状 · 工具碎片化",
  intro: "同一个「调用一个工具」的动作，今天有四套完全不同的接口形态 —— agent 要么逐套适配，要么逐套踩坑。",
  cols: 2,
  items: [
    { n: "CLI", c: C.amber, h: "命令行工具", b: "靠 --help 文本描述自己；flag 增减无版本契约；" +
        "输出是非结构化 stdout。LLM 只能「猜命令、猜参数」。" },
    { n: "REST", c: C.amber, h: "HTTP / REST 端点", b: "每家鉴权、分页、错误体各不相同；" +
        "OpenAPI 是文档格式、不是 agent 调度协议 —— 每个 API 仍要手写一遍适配。" },
    { n: "MCP", c: C.blue, h: "Model Context Protocol", b: "解决了「客户端怎么连」，" +
        "但没有服务端治理：能力门禁、限流、多租户、审计规范一概空缺。" },
    { n: "SDK", c: C.blue, h: "厂商原生 SDK", b: "每个 agent 平台一套形状 —— Claude 的 tool " +
        "≠ OpenAI function ≠ App Intent。一个工具，N 份适配代码。" },
  ],
});

T.table(d, {
  title: "「任意」的四重诉求 —— 互操作性主张",
  kicker: "ATD 要成立的标准",
  intro: "ATD 的目标可以拆成四个「任意」。每一个，今天都被碎片化卡住。",
  head: ["维度", "今天的碎片化", "ATD 的答案"],
  colW: [2.5, 5.0, 4.59],
  rows: [
    [{ t: "任意工具", b: true, c: C.ink }, "CLI / REST / MCP / 原生 SDK —— 形状互不兼容",
      { t: "一份 ToolDefinition，映射到多种 binding", c: C.teal }],
    [{ t: "任意平台", b: true, c: C.ink }, "Linux / macOS / Windows / 移动端调用面各异",
      { t: "binding 选择在 dispatch 时由服务端决定", c: C.teal }],
    [{ t: "任意 agent", b: true, c: C.ink }, "Claude Code 无法直接消费 OpenAI function 形状",
      { t: "所有 agent 调同一 SDK，适配器渲染各家形状", c: C.teal }],
    [{ t: "任意框架", b: true, c: C.ink }, "LangChain tool ≠ MCP tool ≠ App Intent",
      { t: "一份定义，多种框架消费", c: C.teal }],
  ],
});

T.statement(d, {
  kicker: "ATD 的定位",
  big: "ATD 是 agent 调用工具时的一层中立调度协议。",
  sub: "厂商把工具 host 成一个 ATD server（Unix socket 或 HTTP）；任意 agent 平台" +
       "（Hermes / Claude Code / Cursor / 自研）通过同一套 wire 格式 discover / describe / call。" +
       "中间层统一提供能力门禁、审计、多租户 token 路由、工具可见性、skill 同步 —— " +
       "这些都是 raw CLI 拉不出、raw MCP 没规范、每家自研都要重写一遍的东西。",
  attin: "Agent Tool Dispatch · 跨厂商中立 · Apache-2.0",
});

T.stats(d, {
  title: "实证：不是空谈，是同模型同 prompt 跑出来的",
  kicker: "证据 · healthkit_cli case study",
  intro: "三轮真实 LLM session（Hermes + DeepSeek），工具 surface 是唯一变量，audit log 全程落盘可复算。",
  items: [
    { big: "24%", c: C.coral, bigSize: 52, label: "raw endpoint 成功率", sub: "v1.1.0：8 个许可式 schema 的 HMS REST 端点，79 次调用、66% 参数非法" },
    { big: "95.2%", c: C.green, bigSize: 44, label: "helper-tool 成功率", sub: "v1.2.0：换成 26 个 ATD helper-tool，调用数 −73%，同模型同 prompt" },
    { big: "2 vs 8", c: C.teal, bigSize: 48, label: "ATD vs CLI 调用次数", sub: "v1.4.0：同 session 内 ATD 路径 2 次成功，CLI fallback 走 8 次、3 次错路径" },
    { big: "1.6s", c: C.teal, bigSize: 52, label: "首次拿到数据耗时", sub: "ATD 路径 call #1 即拿到；CLI fallback 到第 6 次（约 5s）才拿到" },
  ],
  note: "这是「协议层差异」，不是「工具能力差异」—— 工具底下是同一个 Huawei HMS HealthKit。",
});

T.table(d, {
  title: "v1.4.0 头对头：同一个 prompt，两条路径摆在 agent 面前",
  kicker: "证据 · ATD path vs CLI fallback",
  intro: "同一 Hermes session、同一 DeepSeek 模型、同一 prompt「从医生角度分析最近两个月心率」。",
  head: ["维度", "ATD 路径", "CLI fallback 路径"],
  colW: [3.4, 4.35, 4.34],
  rows: [
    [{ t: "调用次数", b: true, c: C.ink }, { t: "2", c: C.green, b: true }, { t: "8", c: C.coral }],
    [{ t: "总耗时", b: true, c: C.ink }, { t: "约 1.6 秒", c: C.green, b: true }, { t: "约 6 秒", c: C.coral }],
    [{ t: "走错路径次数", b: true, c: C.ink }, { t: "0", c: C.green, b: true }, { t: "3（错 wrapper、--offset 不存在 ×2）", c: C.coral }],
    [{ t: "审计可观测性", b: true, c: C.ink }, { t: "2 条 audit 记录，完整结构化", c: C.green, b: true }, { t: "仅 shell log", c: C.coral }],
    [{ t: "需 agent 自己知道 wrapper 命令", b: true, c: C.ink }, { t: "否", c: C.green, b: true }, { t: "是（双关键字 wrapper）", c: C.coral }],
    [{ t: "需 agent 自己知道后端 30 天上限", b: true, c: C.ink }, { t: "否", c: C.green, b: true }, { t: "是（撞错才知道）", c: C.coral }],
  ],
});

T.compare(d, {
  title: "对比 CLI / MCP / REST —— 它们各自缺什么",
  kicker: "横向对比",
  intro: "三种现有技术都能「让程序调到工具」，但都不是为「LLM agent 可靠调度」设计的。",
  columns: [
    { head: "对比 raw CLI", c: C.amber, tag: "命令行工具",
      items: [
        { k: "发现", v: "靠 --help 文本，混沌、无结构" },
        { k: "多 agent 共享", v: "N 进程 / N 配置 / N 套 OAuth" },
        { k: "审计", v: "只有 shell history" },
        { k: "升级安全", v: "flag 增减直接破坏 agent prompt" },
      ] },
    { head: "对比 raw MCP", c: C.blue, tag: "Model Context Protocol",
      items: [
        { k: "能力门禁", v: "无服务端 capability 概念" },
        { k: "限流 / 多租户", v: "无规范，假设单租户 stdio" },
        { k: "审计 / 可见性", v: "无标准审计；可见性仅二元" },
        { k: "安全 / 分级", v: "无 safety level、无 tier" },
      ] },
    { head: "对比 raw REST", c: C.coral, tag: "HTTP API",
      items: [
        { k: "面向", v: "面向程序员，不面向 LLM 发现" },
        { k: "契约", v: "每家鉴权 / 分页 / 错误体各异" },
        { k: "适配成本", v: "每个 API 都要手写一遍 agent 适配" },
        { k: "可恢复性", v: "HTTP 码粗粒度，无类型化恢复信号" },
      ] },
  ],
});

T.bullets(d, {
  title: "为什么必须在「协议层」解决，而不是每家自研",
  kicker: "横向对比 · vs 自研 adapter",
  intro: "每个厂商自己写一套 server？写过的人都知道 —— 每写一次，都要把同样五件事重新设计一遍。",
  items: [
    { h: "能力门禁会各写各的", c: C.coral, b: "有的查环境变量、有的查 header、有的查 client_id —— 没有统一语义，agent 无法预期。" },
    { h: "审计格式无法跨厂商聚合", c: C.coral, b: "每家一套日志 schema，运维拿不到「跨 caller、跨工具」的统一可观测性。" },
    { h: "多租户 token 路由反复重造", c: C.amber, b: "N 个用户要 N 个进程、N 份 token、N 套 OAuth 刷新逻辑 —— 每家都踩同一遍坑。" },
    { h: "限流、可见性、dry-run 全要重写", c: C.amber, b: "并发上限、隐藏工具、副作用预演 —— 协议不提供，就成了每家的可选项与不一致项。" },
    { h: "协议层做一次，所有 adopter 复用", c: C.teal, b: "atd-runtime + atd-server 约 2000 行 Rust；厂商写 server 只需实现 Tool trait、注册、跑起来。" },
  ],
});

T.cards(d, {
  title: "ATD 在协议层一次性闭环的痛点",
  kicker: "ATD 独有解决的问题",
  intro: "下面六项，raw CLI 拉不出、raw MCP 没规范、自研 adapter 每次重写 —— ATD 全部 ship 在协议与 runtime 里。",
  cols: 3,
  items: [
    { n: "01", h: "能力门禁", b: "Hello 协商 + 工具声明 required_capabilities，dispatch 前置 subset 检查；不满足直接拒，工具根本不执行。" },
    { n: "02", h: "结构化审计", b: "每次调用落一条 JSON Lines：caller、工具、耗时、结果、是否取密 —— 可 jq、可跨 caller 聚合。" },
    { n: "03", h: "多租户 token 路由", b: "TokenBroker 按 caller_id 注入密钥；一个 server 服务 N 个用户，工具自己无需感知多租户。" },
    { n: "04", h: "工具可见性", b: "Hidden 可见性：原始端点 / 调试工具不进 discover 列表，但仍可按 id 调用 —— 协议级，非 per-binary 开关。" },
    { n: "05", h: "跨厂商组合", b: "一个 agent 连 N 个 ATD server，各自一个 socket / 审计 / token store，agent 看到的是合并 catalog。" },
    { n: "06", h: "类型化错误 + dry-run", b: "数字错误码（1000-1099）让 agent 可判定恢复；dry-run 在 dispatch 层短路，副作用工具不执行。" },
  ],
});

T.closing(d, {
  title: "第一部分小结",
  points: [
    "Agent 的可靠性瓶颈，常常不在模型，而在工具调度这一层缺契约。",
    "CLI / MCP / REST 都能「调到工具」，但都不是为 LLM agent 的发现、门禁、审计、可恢复而设计。",
    "实证：同模型同 prompt 下，工具 surface 从 raw endpoint 换成 ATD，成功率 24% → 95%。",
    "能力门禁、审计、多租户、可见性、跨厂商组合 —— 必须在协议层做一次，而非每家重写。",
  ],
  tagline: "ATD 把「每家都要重写一遍」的中间层，折叠成一份协议加一套 runtime。",
  next: "下一部分 · 第二部分：ATD 的设计原则",
});

T.save(d, process.argv[2] || "deck1.pptx").then(() => console.log("deck1 ok"));
