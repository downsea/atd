// Deck 5 — 第五部分 · 路线与生态发展建议
const T = require("./theme");
const { C } = T;
const d = T.newDeck({ part: 5, partLabel: "五", partTitle: "路线与生态" });

T.cover(d, {
  title: "路线与生态发展建议",
  subtitle: "1.0 冻结了什么、有意没做什么、2.0 会偿还什么 ——\n以及把一个参考实现推向一个中立标准，生态该怎么培育。",
  tagline: "1.0 不是功能终点，是稳定起点；生态需要被有意识地培育。",
});

T.agenda(d, {
  items: [
    { h: "1.0 意味着什么", b: "稳定承诺，而非功能里程碑" },
    { h: "稳定契约", b: "wire / schema / 扩展 trait 冻结在 1.x" },
    { h: "有意为之的「不做」", b: "六项 deferred feature 与加入门槛" },
    { h: "已设计未实现", b: "三个候选未来方向" },
    { h: "1.0 的已知限制 + 2.0 方向", b: "实现面的真实边缘，债务在一处偿还" },
    { h: "生态发展建议", b: "把参考实现推向中立标准的路径" },
  ],
});

T.statement(d, {
  kicker: "路线 · 1.0 的含义",
  big: "1.0 不是一个功能里程碑，而是一个稳定承诺。",
  sub: "1.0 相对 0.3.0 不引入新功能 —— 它把 0.3.0 的表面声明为稳定：wire 格式、JSON schema、" +
       "五个 pub 扩展 trait、AtdError 错误码，全部冻结在 1.x 线上。这正是 deferred 工作可以安心 defer 的原因 ——" +
       "它们都被设计成「可加性的」，adopter 能在 1.0 上放心建设，未来作为 minor 升级平滑获得。",
  attin: "稳定契约：docs/release-plan-v1.0.md · 演进范围：docs/roadmap.md",
});

T.cards(d, {
  title: "1.0 稳定契约 —— 冻结了什么",
  kicker: "稳定契约",
  intro: "下面四项构成 ATD 1.x 的稳定承诺。adopter 据此决定依赖的安全边界。",
  cols: 2,
  items: [
    { n: "①", c: C.teal, h: "wire 格式冻结", b: "1.x 线内，任何 1.0 客户端反序列化不了的改动 —— 删字段、改形状、删枚举变体、重定义错误码 —— 一律等到 2.0。" },
    { n: "②", c: C.teal, h: "schema 同约定冻结", b: "atd-protocol-schema.json 同样：从 1.0 schema 生成的代码能反序列化每一条 1.x 消息。加可选字段 = minor，删 / 改 = major。" },
    { n: "③", c: C.blue, h: "扩展 trait 稳定", b: "Tool / Binding / Middleware / TokenBroker / AuditSink 五个 pub trait 稳定 —— 针对 1.0 写的扩展，跨整个 1.x 线继续编译。" },
    { n: "④", c: C.blue, h: "版本与 MSRV", b: "workspace 锁步版本贯穿 1.x；MSRV 锁定 Rust 1.85；错误码 1000-1099 语义稳定。" },
  ],
});

T.table(d, {
  title: "有意为之的「不做」—— 六项 deferred feature",
  kicker: "演进范围 · 非目标",
  intro: "每一项都是「有意缺席」—— 既没 ship、也不是扩展点。加入的门槛统一是「具体的 adopter 需求」。",
  head: ["功能", "为什么 deferred / 加入门槛"],
  colW: [3.5, 8.59],
  rows: [
    [{ t: "多设备路由", b: true, c: C.ink }, "agent 框架的事；ATD 给每个设备一个干净端点就停。门槛：dispatch 真的无法表达成「一连接一端点」"],
    [{ t: "分布式 session", b: true, c: C.ink }, "迁移 / fork / 交接。设计面太宽，现在猜会定出没人要的 wire。门槛：有真实迁移需求的 adopter"],
    [{ t: "工具签名验证", b: true, c: C.ink }, "需要协议未规定的 PKI。门槛：有真实签名管线的 adopter —— wire 形状已预留，可无破坏补上"],
    [{ t: "REST / AppFunction binding", b: true, c: C.ink }, "Binding trait 能 host，参考实现不 bless 任何一种。门槛：照 trait 实现真实后端的 adopter"],
    [{ t: "per-tool dry-run 预览", b: true, c: C.ink }, "v1 的 dry-run 是服务端短路；per-tool 预览是更丰富契约。门槛：有有意义预览路径的工具"],
    [{ t: "per-tool 限流强制", b: true, c: C.ink }, "max_concurrent 已强制（信号量）；rate_limit_per_min 加 token-bucket 直接但未建"],
  ],
});

T.cards(d, {
  title: "已设计、未实现 —— 三个候选未来方向",
  kicker: "演进范围 · designed but unimplemented",
  intro: "三份 SP 设计写到了完整深度但未实现；归档在 docs/archive/superpowers/。它们仍是候选方向，需 adopter 信号触发。",
  cols: 3,
  items: [
    { n: "①", c: C.blue, h: "Agent identity", b: "did:agent —— 在今天自由文本、不可验证的 Hello.client_id 之上加一层跨厂商身份。让受监管的工具服务能按「哪个厂商签的、哪个 build」门禁。ATD 只贡献一个 DidResolver trait。无 wire 改动。" },
    { n: "②", c: C.blue, h: "Secret bootstrap", b: "启动时的密钥传输，与 TokenBroker 正交 —— 父进程怎么把 bootstrap 密钥交给刚 spawn 的子 server，不经 argv / env / 磁盘。把一个 adopter 已在用的 0600-socket 握手泛化成 runtime 模块。" },
    { n: "③", c: C.blue, h: "Streamable HTTP 尾巴", b: "核心（atd-server-http、MCP 翻译、bearer）已在 0.3.0 ship；未实现的是 spec 尾巴：Mcp-Session-Id 粘性会话、Last-Event-ID 可恢复、TLS 终止、OAuth 2.1 签发。URL 空间已预留。" },
  ],
});

T.table(d, {
  title: "1.0 的已知限制 —— 实现面的真实边缘",
  kicker: "演进范围 · 已知限制",
  intro: "几个表面比类型签名暗示的窄。它们不是 bug —— 是 1.0 有意画下的 stop-line，逐项有 issue 跟踪。",
  head: ["限制", "细节"],
  colW: [3.4, 8.69],
  rows: [
    [{ t: "rate_limit_per_min 仅声明式", b: true, c: C.ink }, "schema 里有、每个工具都声明，但无代码路径强制；max_concurrent 经信号量是强制的"],
    [{ t: "单 binding 路由", b: true, c: C.ink }, "ToolDefinition 带 Vec<ToolBinding>、wire 上有 preferred_binding，但 dispatch 永远路由到第一个声明的 binding"],
    [{ t: "工具签名仅声明式", b: true, c: C.ink }, "ToolTrust::signature 与 TrustLevel 是描述性元数据；runtime 不验签 —— trust 等级是 honor-system"],
    [{ t: "dry-run 仅服务端短路", b: true, c: C.ink }, "dispatcher 兑现（合成 tool_result、工具不执行）；不路由进工具自己的预览路径"],
    [{ t: "无 session / cancel", b: true, c: C.ink }, "SDK 没有 session() / cancel()，服务端无 session 状态机 —— 一个连接就是一个 session"],
    [{ t: "Python 类型手抄", b: true, c: C.ink }, "python 的 types.py 是手写、非从 schema 生成；漂移只由集成测试兜底，切换为生成式是 1.0 后工作"],
  ],
});

T.bullets(d, {
  title: "2.0 会带来什么 —— 债务在一处偿还",
  kicker: "演进范围 · post-1.0",
  intro: "1.x 冻结 wire；任何 wire-breaking 改动都等到 2.0 —— 这让 §1 / §2 的 deferred 工作可以安心 defer。当前没有 2.0 计划，以下只固定「原则」。",
  items: [
    { h: "可加 = minor，破坏 = major", c: C.teal, b: "新可选字段 / 新枚举变体 / 新错误码 / 新工具 / 新 pub trait 走 1.x minor；删除 / 重塑 / 语义重定义等到 2.0。" },
    { h: "多 binding dispatch 成一等契约", c: C.blue, b: "让 preferred_binding 真正生效，可能重塑 ToolBinding 选择 —— 不再是 1.x 的「第一个 binding 胜出」。" },
    { h: "枚举扩展批量进 major", c: C.blue, b: "新增 BindingProtocol / ToolTier 变体对严格反序列化器是 wire-breaking，因此批量进一个 major。" },
    { h: "per-crate 独立版本", c: C.amber, b: "1.x 锁步版本；2.0 是重新考虑「稳定 crate 能否独立版本化」的自然节点。" },
  ],
});

T.statement(d, {
  kicker: "生态发展建议",
  big: "协议成立只是起点。ATD 要成为标准，靠的是生态 ——\n而生态需要被有意识地培育。",
  sub: "下面六条建议，是把「一个能跑、文档完备的参考实现」推向「一个被多方采纳的中立标准」的路径。" +
       "它们的共同前提只有一个：ATD 必须始终保持中立。",
  attin: "ATD · Agent Tool Dispatch · Apache-2.0 · 零厂商耦合",
});

T.cards(d, {
  title: "六条生态发展建议",
  kicker: "生态发展建议",
  intro: "从参考实现到中立标准，按杠杆从高到低。",
  cols: 3,
  items: [
    { n: "01", c: C.teal, h: "多语言 SDK，优先 TypeScript", b: "目前只有 Rust + Python。schema 机器可读 → SDK 可从 atd-protocol-schema.json 生成。优先 TypeScript（覆盖最大 agent 生态），再 Go / Java / Swift。" },
    { n: "02", c: C.teal, h: "conformance 作为互操作硬门槛", b: "atd-conformance 是跨实现 fixture 语料。把「过 conformance」立为「声称兼容 ATD」的准入标准 —— 这是生态健康的地基。" },
    { n: "03", c: C.teal, h: "落地 SKILL.md 平台路径", b: "一个 atd-dispatch skill 发布一次即触达 26+ 平台（集成路径 5）。目前未发布 —— 这是生态扩张最高杠杆的近期项。" },
    { n: "04", c: C.blue, h: "成立跨厂商工作组", b: "agent identity（did:agent）、能力命名等公约，roadmap 已明确定位为「跨厂商工作组交付物」。生态成熟需要一个中立治理体来推进。" },
    { n: "05", c: C.blue, h: "真正发布到 crates.io", b: "1.0 后按 release-plan-v1.0 的 checklist publish；让 adopter 从 path-dep 转向版本化依赖，把接入门槛降到 cargo add。" },
    { n: "06", c: C.blue, h: "沉淀 adopter 案例与反模式", b: "设计哲学是活文档。鼓励 adopter 公开案例（如 healthkit case study）、把新反模式 PR 回去 —— 让踩过的坑变成生态的复利。" },
  ],
});

T.closing(d, {
  title: "结语 · 五个部分，一条主线",
  points: [
    "必要性 —— 工具碎片化是 agent 可靠性的隐形天花板；CLI / MCP / REST 都不为「LLM 可靠调度」而设计。",
    "设计原则 —— 七条原则一条主线：单一真相源、显式、可发现、可观测、可恢复。",
    "架构 —— 一份 schema、一条确定性 dispatch 管线、分层安全、全 pub-trait 扩展点。",
    "集成 —— 五条路径、三家生产 adopter、agent 平台零改动。",
    "路线 —— 1.0 冻结稳定面；演进靠 adopter 信号；生态靠有意识培育。",
  ],
  tagline: "ATD 的生态价值，前提是它保持中立 —— 不绑死任一 agent 或工具厂商。这是它能成为标准的唯一条件。",
  next: "ATD · Agent Tool Dispatch · github.com/downsea/atd · Apache-2.0",
});

T.save(d, process.argv[2] || "deck5.pptx").then(() => console.log("deck5 ok"));
