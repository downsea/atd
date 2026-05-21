// Deck 2 — 第二部分 · 设计原则
const T = require("./theme");
const { C } = T;
const d = T.newDeck({ part: 2, partLabel: "二", partTitle: "设计原则" });

T.cover(d, {
  title: "设计原则",
  subtitle: "协议只是骨架。真正决定一个 ATD 工具服务能否跨平台、跨厂商、\n跨时间存活的，是它上面一层的设计约定。",
  tagline: "约定会悄悄退化 —— 写下来、用案例钉住，它才不退化。",
});

T.agenda(d, {
  items: [
    { h: "前提", b: "协议很小，约定很大 —— 有意思的决策在上面一层" },
    { h: "三类消费者", b: "一个 server，三条互不打架的管道" },
    { h: "七条原则 · 总览", b: "贯穿单一真相源 / 显式 / 可发现 / 可观测" },
    { h: "逐条拆解", b: "每条：规则 → 为什么 → 反模式" },
    { h: "落地", b: "adopter 自检清单，接入前必读" },
  ],
});

T.statement(d, {
  kicker: "前提",
  big: "ATD 协议本身很小 —— 一份 wire spec、一份 schema、四类核心消息。真正有意思的设计决策，都在它上面一层。",
  sub: "「上面一层」= adopter 怎么组织自己的工具服务，让结果能跨 agent 平台、跨厂商、跨时间存活。" +
       "这一层不由协议强制，却决定成败 —— 所以必须写成明确的原则，并用真实 adopter 案例钉住。",
  attin: "原则来源：docs/atd-design-philosophy.md（活文档，三家 adopter 实证）",
});

T.cards(d, {
  title: "一个 ATD 工具服务，同时服务三类消费者",
  kicker: "设计的出发点",
  intro: "每个设计决策都要同时对三种读法成立 —— 让 LLM 舒服却破坏桥接握手的选择，不是取舍，是 bug。",
  cols: 3,
  items: [
    { n: "①", h: "LLM Agent", c: C.teal, b: "需要：可发现的工具面、类型化错误信封、可预期的参数形状。\n通道：tool_list / tool_schema / run_tool wire 帧。" },
    { n: "②", h: "人类运维者", c: C.blue, b: "需要：审计轨迹、运维控制、结构化日志、门禁拒绝可见。\n通道：AuditSink 事件、server 日志、metrics 计数器。" },
    { n: "③", h: "Agent 平台桥接", c: C.amber, b: "Hermes / Claude Code / MCP。需要：稳定握手、能力协商、不出意外的 transport。\n通道：Hello/HelloAck + UCAN-lite。" },
  ],
});

T.table(d, {
  title: "七条原则 · 一页总览",
  kicker: "总览",
  intro: "下面逐条展开。它们不是七件无关的事，而是同一主线的七个切面。",
  head: ["#", "原则", "一句话"],
  colW: [0.7, 3.7, 7.69],
  rows: [
    [{ t: "1", c: C.teal, b: true }, { t: "ToolDefinition 是唯一真相源", b: true, c: C.ink }, "摘要、校验、skill、适配器、文档全部从一份定义派生"],
    [{ t: "2", c: C.teal, b: true }, { t: "Skills 跟工具走，不跟桥走", b: true, c: C.ink }, "用 skills.list/get meta-tool 暴露；平台目录只是缓存"],
    [{ t: "3", c: C.teal, b: true }, { t: "能力是协商出来的", b: true, c: C.ink }, "声明 required_capabilities，dispatch 门禁，handler 不查权限"],
    [{ t: "4", c: C.teal, b: true }, { t: "错误类型化、带命名空间", b: true, c: C.ink }, "协议 1000-1099、adopter 2000+；没有自由文本错误"],
    [{ t: "5", c: C.teal, b: true }, { t: "工具默认跨连接无状态", b: true, c: C.ink }, "共享世界状态必须显式声明 —— 默认无状态，偏离要响"],
    [{ t: "6", c: C.teal, b: true }, { t: "发现是唯一权威", b: true, c: C.ink }, "agent prompt 绝不写死工具 id；运行时 discover"],
    [{ t: "7", c: C.teal, b: true }, { t: "dispatch 有界且可观测", b: true, c: C.ink }, "tier deadline、middleware 可观测、不静默重试"],
  ],
});

T.compare(d, {
  title: "原则一 · ToolDefinition 是唯一真相源",
  kicker: "原则 1 / 7",
  intro: "工具的每个事实 —— 名字、参数、能力、可见性、deadline、安全分级 —— 只活在一个地方。",
  columns: [
    { head: "反模式 · 两处维护", bad: true, tag: "它们一定会漂移",
      items: [
        { k: "", v: "input_schema 声明一份参数形状" },
        { k: "", v: "docstring / SKILL.md 又写一份" },
        { k: "", v: "schema 加字段时，只有一份被更新" },
        { k: "", v: "错的那份会「意外」变权威" },
      ] },
    { head: "正确 · 单一来源派生", good: true, tag: "没有第二份拷贝",
      items: [
        { k: "", v: "一份 ToolDefinition 字面量 / 工具" },
        { k: "", v: "ToolSummary 由 registry 派生" },
        { k: "", v: "skills meta-tool 直接服务该定义" },
        { k: "", v: "OpenAI / Anthropic 适配器从它生成" },
      ] },
  ],
});

T.bullets(d, {
  title: "原则二 · Skills 跟工具走，不跟桥走",
  kicker: "原则 2 / 7",
  intro: "一个工具的 SKILL.md 内容是「工具的一部分」，不是「agent 平台的一部分」。",
  items: [
    { h: "规则", c: C.teal, b: "SKILL.md 活在工具服务的仓库里，通过 <publisher>:<service>.skills.list / .get meta-tool 暴露，由 atd skills sync 按平台安装。平台的 skill 目录是缓存，不是源。" },
    { h: "反模式", c: C.coral, b: "把 SKILL.md 手抄进 ~/.hermes/skills/ 或 ~/.claude/skills/ —— 升级工具不刷新、加第二个平台要复制、换平台直接丢在地上。" },
    { h: "收益", c: C.green, b: "同一套机制（meta-tool 发布 + sync 安装）既做工具发现，也做 skill 分发；一次性修掉上述三种失效模式。" },
    { h: "实证", c: C.blue, b: "healthkit_cli 把 26 个 SKILL.md 通过该公约暴露，atd skills sync --target hermes 实测拉下 26 文件、与源逐字一致。" },
  ],
});

T.steps(d, {
  title: "原则三 · 能力是协商出来的，不是写死的",
  kicker: "原则 3 / 7",
  intro: "工具声明需要什么能力；连接在握手时协商出有什么能力；dispatch 负责把关。Handler 自己不查权限。",
  items: [
    { h: "工具声明", b: "ToolDefinition.required_capabilities: list[str] —— 不透明的能力字符串。" },
    { h: "Hello 协商", b: "client 请求的 capabilities 与 server 的 allow-list 求交集 → granted_capabilities。" },
    { h: "dispatch 门禁", b: "调用前算 missing = required − granted；非空即拒，返回 ERR_CAPABILITY_DENIED (1001)。" },
    { h: "handler 干净", b: "handler 里没有 if-not-has-cap；dispatcher 已经判过，工具只管干活。" },
  ],
  note: "好处：LLM 在 tool_schema 里就看见门禁要求；检查统一；审计在任何 handler 跑之前就看见拒绝。",
});

T.bullets(d, {
  title: "原则四 · 错误是类型化、带命名空间的",
  kicker: "原则 4 / 7",
  intro: "每个失败都带一个数字 code，而不是自由文本。数字码能跨翻译存活，让 agent 可判定地恢复。",
  items: [
    { h: "协议层 1000-1099", c: C.teal, b: "TOOL_NOT_FOUND / CAPABILITY_DENIED / RATE_LIMITED / DEADLINE_EXCEEDED / INVALID_ARGS / UCAN(1010-1013) / cursor(1020-1021) / INTERNAL —— 定义在 atd-protocol。" },
    { h: "adopter 层 2000+", c: C.blue, b: "每个 adopter 认领自己的号段（cbrain 2000-2099 / healthkit 3000-3099 / celia 4000-4099），跨厂商不撞号。" },
    { h: "ToolDefinition.errors 预告", c: C.green, b: "工具在定义里预先声明可能抛的错误码；retryable 位诚实标注 —— 只有 client 能安全重调时才为 true。" },
    { h: "反模式", c: C.coral, b: "return ToolFailure(code=\"ERR\", message=\"出错了，待会再试\") —— LLM 必须读英文散文才能恢复，且各家措辞不一。" },
  ],
});

T.compare(d, {
  title: "原则五 · 工具默认跨连接无状态",
  kicker: "原则 5 / 7",
  intro: "每个连接拿到全新 ConnectionContext；共享世界状态是「opt-in 且声明」的 —— 默认无状态，偏离要大声。",
  columns: [
    { head: "反模式 · 模块全局冒充连接态", bad: true,
      items: [
        { k: "", v: "_LAST_CONFIG 是模块全局变量" },
        { k: "", v: "意图却是「每连接一份」" },
        { k: "", v: "第二个连接覆盖第一个的视图" },
        { k: "", v: "于是给出错误答案，且难复现" },
      ] },
    { head: "正确 · 显式二选一", good: true,
      items: [
        { k: "", v: "要么放进 ctx.connection（每连接）" },
        { k: "", v: "要么承认「共享世界」并在 description 里大声说明" },
        { k: "", v: "cbrain 的 MuJoCo 单例属后者 —— 正确，但要写明" },
        { k: "", v: "大多数 agent 平台会自由重连，假设连接亲和性必出错" },
      ] },
  ],
});

T.bullets(d, {
  title: "原则六 · 发现是唯一权威 —— prompt 不写死工具 id",
  kicker: "原则 6 / 7",
  intro: "agent 在运行时通过 tool_list → tool_schema 发现工具。系统 prompt 里绝不含硬编码的工具 id 列表。",
  items: [
    { h: "规则", c: C.teal, b: "ToolSummary.id 是唯一稳定句柄；name / description 是给人看的散文，可以随时改而不破坏 agent。新工具自动出现，重命名不破坏流程。" },
    { h: "反模式", c: C.coral, b: "系统 prompt 写「你可以调用 x:a、x:b、x:c，永远先调 x:a」—— 加了新工具它不知道；某个工具改名，所有 agent 同时崩。" },
    { h: "收益", c: C.green, b: "不写死的 prompt 反而更简单：「看你手上有哪些工具，挑匹配这个任务的那个」一行话；写死的版本是维护负债。" },
  ],
});

T.cards(d, {
  title: "原则七 · dispatch 有界且可观测",
  kicker: "原则 7 / 7",
  intro: "每个工具调用都裹在三层契约里 —— 没有调用能逃出这三条。",
  cols: 3,
  items: [
    { n: "有界", c: C.teal, h: "Bounded", b: "deadline 由 resources.timeout_ms 推导（未设默认 30s）。超时返回 1004 DEADLINE_EXCEEDED。没有工具能无限期跑下去 —— 否则整条 agent 栈一起死锁。" },
    { n: "可观测", c: C.blue, h: "Observable", b: "middleware（pre_call / post_call / on_error）看见每一次 dispatch。审计、追踪、限流、metrics 都作为 middleware 实现。dispatch 路径自己不静默吞错。" },
    { n: "不重试", c: C.amber, h: "No silent retry", b: "server 永不内部重试一个工具调用。瞬时失败就回 retryable=true，让 client 决定。静默重试会瞒过审计、对有副作用的操作重复计费。" },
  ],
});

T.closing(d, {
  title: "第二部分小结",
  points: [
    "协议小；决定成败的是它上面一层的 adopter 约定 —— 所以要写成原则、用案例钉住。",
    "一个 server 同时服务 LLM、人类运维、平台桥接三类消费者；每个设计要对三种读法都成立。",
    "七条原则一条主线：单一真相源、显式优于隐式、可发现、可观测、可恢复。",
    "约定会悄悄退化；它靠「被写下来、用 adopter 案例钉住、每次接入前重读」才不退化。",
  ],
  tagline: "好协议给的是地基；好原则保证盖上去的楼不会慢慢长歪。",
  next: "下一部分 · 第三部分：ATD 的架构与设计",
});

T.save(d, process.argv[2] || "deck2.pptx").then(() => console.log("deck2 ok"));
