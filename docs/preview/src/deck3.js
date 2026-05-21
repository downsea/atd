// Deck 3 — 第三部分 · 架构与设计
const T = require("./theme");
const { C } = T;
const d = T.newDeck({ part: 3, partLabel: "三", partTitle: "架构与设计" });

T.cover(d, {
  title: "架构与设计",
  subtitle: "从统一 schema 到分层模型、确定性 dispatch 管线、安全模型、\n中间件与扩展点 —— ATD 参考实现的系统全貌。",
  tagline: "一份 schema 管所有 wire 形状；一条管线管所有调用。",
});

T.agenda(d, {
  items: [
    { h: "统一 schema", b: "每条消息都序列化成同一份机器可读 schema" },
    { h: "分层模型", b: "从用户意图到工具宇宙的全栈" },
    { h: "wire 与类型", b: "消息集、ToolDefinition、错误两层、游标分页" },
    { h: "dispatch 管线", b: "确定性的七步调度，transport 无关" },
    { h: "安全模型", b: "分类 / 能力门禁 / 运行时控制 / 审计" },
    { h: "中间件与扩展点", b: "egress 管线 + 六个不 fork 的 pub trait" },
    { h: "crate 地图", b: "16 个 crate 的分层与职责" },
  ],
});

T.statement(d, {
  kicker: "核心抽象",
  big: "ATD 最原子的承诺：每条消息、每个方向、每种 transport，都序列化成同一份机器可读 schema 定义的形状。",
  sub: "atd-protocol-schema.json —— 从 atd-protocol 的 Rust 类型经 schemars 生成，按 JSON Schema " +
       "2020-12 元 schema 校验，CI 守护 Rust 源与 JSON 之间的漂移。UDS 与 HTTP 两个 listener " +
       "反序列化成同一套 Rust 类型，没有 per-transport 分叉 —— 跨语言 SDK 因此能与 Rust 实现自动类型兼容。",
  attin: "权威架构文档：docs/architecture.md",
});

T.layers(d, {
  title: "分层模型 —— 从用户意图到工具宇宙",
  kicker: "分层模型",
  intro: "三个核心机制（schema / dispatch / 安全）+ 两个扩展机制（binding / middleware）。",
  items: [
    { h: "用户意图", b: "语音 · 文本 · 触发器", c: C.faint },
    { h: "Agent 框架", b: "Claude Code · Cursor · Hermes · LangChain · 自研", c: C.blue },
    { h: "Skills 层（相邻）", b: "SKILL.md · atd-tools 依赖声明 · 渐进披露 —— ATD 的上游消费者", c: C.blue },
    { h: "客户端 SDK", b: "discover · describe · call · call_page · call_all · hello（Rust / Python）", c: C.teal },
    { h: "Dispatch（核心）", b: "能力门禁 · tier deadline · binding 选择 · cursor · middleware 管线", c: C.teal, hi: true },
    { h: "两个 transport", b: "Unix socket（atd-server） / HTTP · MCP JSON-RPC（atd-server-http）", c: C.teal },
    { h: "工具宇宙", b: "NativeBinding + 扩展点 —— ref:echo · ref:fs.* · ref:shell.* · ref:web.fetch · 厂商工具", c: C.amber },
  ],
  note: "两条调用路径（agent 直连 / Skill body）走的是同一条 ATD dispatch —— Skills 层只是上面的编排器。",
});

T.bullets(d, {
  title: "wire 与类型 —— 一份 schema 覆盖全部词汇",
  kicker: "wire 与类型",
  intro: "wire 是 length-prefixed JSON，跑在双工字节流上；两个 listener 把 transport 帧翻译成同一套类型。",
  items: [
    { h: "请求联合 ClientMessage", c: C.teal, b: "Hello（握手 + 能力 + 可选 UCAN）· Ping · ToolList（发现）· ToolSchema（深描）· RunTool（调用）· RunToolContinue（分页续取）。" },
    { h: "响应镜像 ServerMessage", c: C.teal, b: "HelloAck · Pong · ToolListResponse · ToolSchemaResponse · ToolResultResponse（数据或错误 + 可选 next_cursor）。" },
    { h: "错误是两层", c: C.amber, b: "客户端 AtdError 是 Rust 枚举、本身不带数字码；wire 数字码是 ERR_* 常量（1001-1021），走 Response::Error.code。" },
    { h: "游标分页", c: C.blue, b: "大结果带 HMAC-SHA256 签名的 next_cursor，绑定 (tool_id, caller_id, args 指纹, page, ...)；无状态校验、512B 上限、默认 5 分钟 TTL。" },
    { h: "id 净化", c: C.blue, b: "工具 id 含 : 和 . 会破坏 LLM 函数名槽位；atd-sdk::sanitize 做规范双向变换（ref:fs.read <-> ref_fs_read）。" },
  ],
});

T.table(d, {
  title: "ToolDefinition —— agent 用来决定「是否 / 如何调用」的契约",
  kicker: "wire 与类型 · 工具描述",
  intro: "ToolSchema 返回完整 ToolDefinition；每个字段都是已发布 schema 的一部分。",
  head: ["字段", "作用"],
  colW: [3.4, 8.69],
  rows: [
    [{ t: "id", b: true, c: C.teal }, "<publisher>:<service>.<x>.<y> 命名空间，唯一稳定句柄"],
    [{ t: "description / capability", b: true, c: C.teal }, "LLM 看到的自然语言；capability.intent_examples 给 3 条意图短语辅助匹配"],
    [{ t: "input_schema / output_schema", b: true, c: C.teal }, "JSON Schema 2020-12，描述参数与返回值"],
    [{ t: "safety", b: true, c: C.teal }, "level（Read/Write/Financial/Privacy/Physical/Destructive）+ 是否支持 dry-run"],
    [{ t: "visibility", b: true, c: C.teal }, "Read / Write / Dangerous / System / Hidden（Hidden 不进 discover）"],
    [{ t: "required_capabilities", b: true, c: C.teal }, "服务端在 dispatch 前强制的能力门禁"],
    [{ t: "tier", b: true, c: C.teal }, "Hot / Warm / Cold —— 推导 per-call deadline 与输出预算"],
    [{ t: "resources / bindings / trust", b: true, c: C.teal }, "并发上限；binding 协议；publisher + L0-L4 信任等级 + 签名（声明式）"],
  ],
});

T.steps(d, {
  title: "dispatch —— 一条确定性的调度管线",
  kicker: "dispatch",
  intro: "每个调用都走同一条管线；atd-server 与 atd-server-http 都汇入同一个 dispatch_request 入口。",
  items: [
    { h: "握手", b: "接受连接 → Hello 能力门禁，可选 UCAN token 校验，得出 granted_capabilities。" },
    { h: "查表", b: "收到 RunTool / RunToolContinue → registry.get(tool_id)。" },
    { h: "能力检查", b: "required_capabilities 不是 granted 的子集 → 拒，ERR_CAPABILITY_DENIED (1001)。" },
    { h: "tier 预算", b: "由 ToolTier 推导 per-call deadline + max_output_bytes，可按调用 / 按 server 覆盖。" },
    { h: "解析密钥", b: "TokenBroker::resolve(caller_id) → 把 SecretBundle 挂到 CallContext.secrets。" },
    { h: "binding 调用", b: "binding.invoke(args, &ctx) 执行工具；游标场景走 call_paginated。" },
    { h: "middleware + 回复", b: "egress middleware 管线（脱敏 / FHIR / PII）→ 序列化 ToolResultResponse + 可选 next_cursor。" },
  ],
});

T.cards(d, {
  title: "安全模型 —— 分类、门禁、运行时控制、审计",
  kicker: "安全模型",
  intro: "分类是「描述性元数据」供判断风险；门禁与运行时控制才是真正的强制机制。",
  cols: 2,
  items: [
    { n: "①", h: "三轴分类", c: C.blue, b: "每个工具声明 Safety（6 档）/ Visibility（5 档）/ Trust（L0-L4）。LLM 适配器把 Safety 与 Visibility 透传给 agent 的工具选择器。" },
    { n: "②", h: "能力 allow-list + UCAN-lite", c: C.teal, b: "运营声明的能力字符串 + 客户端可带 JWT 形 UCAN token（Ed25519 签名、did:key 受众、衰减链、撤销）。dispatch 的 granted = 字符串并 UCAN。" },
    { n: "③", h: "per-tool 运行时控制", c: C.amber, b: "活在具体工具里：web.fetch 的 SSRF 防护与 header allow-list、fs.edit 的必读后写、shell 的 SIGTERM→SIGKILL 超时、所有工具的 per-tool 信号量。" },
    { n: "④", h: "结构化审计", c: C.green, b: "每次调用发一条 CallEvent 给 AuditSink。参考 sink JsonLinesAuditSink 经有界 mpsc + 专用写任务落 JSONL；on_call 非阻塞；只记 secrets_resolved 布尔，绝不记密钥。" },
  ],
});

T.bullets(d, {
  title: "中间件 + 扩展点 —— 不 fork 参考服务器就能挂载",
  kicker: "中间件与扩展点",
  intro: "中间件是 egress 钩子；ATD 的每个扩展点都是 atd-runtime 里的 pub trait，各配一篇 docs/extending/ how-to。",
  items: [
    { h: "Middleware", c: C.teal, b: "on_result(tool_id, &def, &mut Value) —— 可改写 / 剥子树 / 改成错误信封。内置 RedactPaths、FHIR R4 校验、HIPAA PII 脱敏；Server::set_middleware 组合，自上而下跑。" },
    { h: "Tool / Binding", c: C.blue, b: "实现 Tool trait 加一个内置工具（参考 atd-tools-echo）；实现 Binding trait 加一种调用后端（已有 NativeBinding / CliBinding）。" },
    { h: "TokenBroker / AuditSink", c: C.blue, b: "实现 TokenBroker 接入 vault / 密钥管理（已有 InMemory / FileTokenBroker）；实现 AuditSink 把审计接到 Kafka / OpenTelemetry。" },
    { h: "新 transport", c: C.amber, b: "写一个 listener crate，把 transport 帧翻成 ClientMessage 并调 dispatch_request —— 无需改 dispatch、无需 fork。改 wire 格式则不是扩展点，是协议变更。" },
  ],
});

T.table(d, {
  title: "crate 地图 —— 16 个 crate 的分层与职责",
  kicker: "crate 地图",
  intro: "Apache-2.0 单一 workspace；所有可发布 crate 共享一个版本号。",
  head: ["crate", "职责"],
  colW: [3.9, 8.19],
  rows: [
    [{ t: "atd-protocol", b: true, c: C.teal }, "wire 类型 + codec + sanitize —— schema 的 Rust 源"],
    [{ t: "atd-sdk", b: true, c: C.teal }, "Rust 客户端 SDK：discover / describe / call / call_page / call_all / hello"],
    [{ t: "atd-runtime", b: true, c: C.teal }, "服务端核心：Tool / Registry / dispatch / Binding / Middleware / TokenBroker / 审计 / UCAN（transport 无关）"],
    [{ t: "atd-server / atd-server-http", b: true, c: C.teal }, "两个 transport：Unix socket listener / HTTP + MCP JSON-RPC + bearer auth"],
    [{ t: "atd-middleware-fhir / -pii-redact-medical", b: true, c: C.teal }, "FHIR R4 egress 校验 / HIPAA Safe Harbor PHI 脱敏"],
    [{ t: "atd-tools-{echo,fs,shell,web}", b: true, c: C.teal }, "4 个内置参考工具 crate；atd-tools-echo 是新工具的范本"],
    [{ t: "atd-cli / atd-ref-server / atd-mcp-bridge", b: true, c: C.teal }, "atd 开发者 CLI / 参考 server binary / MCP-over-stdio 网关"],
    [{ t: "atd-conformance / atd-mock-weather-server", b: true, c: C.teal }, "跨实现 conformance fixture 套件 / 跨厂商组合 demo（publish=false）"],
  ],
});

T.closing(d, {
  title: "第三部分小结",
  points: [
    "统一 schema 是地基：一份机器可读 schema 覆盖全部 wire 词汇，CI 守护漂移。",
    "dispatch 是一条确定性管线 —— 握手 → 能力 → tier → 密钥 → binding → middleware，两个 transport 共用入口。",
    "安全是分层的：描述性分类 + 能力门禁 / UCAN + per-tool 运行时控制 + 结构化审计。",
    "扩展点全是 pub trait —— 加工具 / binding / middleware / 鉴权 / 审计 / transport 都无需 fork。",
  ],
  tagline: "架构的目标只有一个：让协议的承诺，在每一层都被同一套类型钉死。",
  next: "下一部分 · 第四部分：实施案例与集成方法",
});

T.save(d, process.argv[2] || "deck3.pptx").then(() => console.log("deck3 ok"));
