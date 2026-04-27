// Builds docs/whitepaper/atd-introduction.pptx
//
// 立场介绍 deck — 以 v1.4.0 healthkit case study 为锚点（实测 2 ATD calls
// vs 8 CLI fallback），讲清楚 ATD 是什么、ships 了什么、对比 raw CLI /
// raw MCP / per-vendor adapter 的优势在哪。
//
// Run:
//   NODE_PATH=$(npm root -g) node docs/whitepaper/build-introduction-pptx.js

const pptxgen = require("pptxgenjs");

// ─── palette (matches build-overview-pptx.js) ────────────────────────────
const C = {
  navy:     "0F1F3D",
  deepBlue: "065A82",
  teal:     "1C7293",
  midnight: "21295C",
  slate:    "475569",
  muted:    "64748B",
  faint:    "E2E8F0",
  paper:    "F8FAFC",
  card:     "FFFFFF",
  amber:    "D97706",
  green:    "059669",
  red:      "DC2626",
  purple:   "7C3AED",
};
const LAYER = {
  protocol: C.deepBlue,
  sdk:      C.teal,
  runtime:  C.amber,
  security: C.red,
  tools:    C.green,
  skills:   C.purple,
  neutral:  C.midnight,
};
const FONT_HEAD = "Cambria";
const FONT_BODY = "Calibri";
const FONT_MONO = "Courier New";

const pres = new pptxgen();
pres.layout  = "LAYOUT_16x9";
pres.author  = "atd-mvp maintainers";
pres.title   = "ATD — Agent Tool Dispatch Protocol";
pres.subject = "Introduction grounded in v1.4.0 healthkit case study";

const SLIDE_W = 10, SLIDE_H = 5.625;
const FOOT_Y  = SLIDE_H - 0.32;

// ─── helpers ──────────────────────────────────────────────────────────────
function addFooter(slide, pageNum, total, opts = {}) {
  const color = opts.light ? "8FA1B8" : C.muted;
  slide.addText("ATD Introduction   ·   atd-mvp v0.3.0   ·   Apache-2.0", {
    x: 0.45, y: FOOT_Y, w: 7.8, h: 0.28,
    fontSize: 9, fontFace: FONT_BODY, color, margin: 0,
  });
  slide.addText(`${pageNum} / ${total}`, {
    x: 8.4, y: FOOT_Y, w: 1.2, h: 0.28,
    fontSize: 9, fontFace: FONT_BODY, color, align: "right", margin: 0,
  });
}
function addRail(slide, color) {
  slide.addShape(pres.shapes.RECTANGLE, {
    x: 0, y: 0, w: 0.12, h: SLIDE_H,
    fill: { color }, line: { color, width: 0 },
  });
}
function addTitle(slide, text, { subtitle, y = 0.35 } = {}) {
  slide.addText(text, {
    x: 0.45, y, w: 9.1, h: 0.55,
    fontSize: 26, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  if (subtitle) {
    slide.addText(subtitle, {
      x: 0.45, y: y + 0.58, w: 9.1, h: 0.32,
      fontSize: 13, fontFace: FONT_BODY, color: C.slate, italic: true, margin: 0,
    });
  }
}
function contentSlide(layerColor) {
  const slide = pres.addSlide();
  slide.background = { color: C.paper };
  addRail(slide, layerColor);
  return slide;
}
function card(slide, { x, y, w, h, railColor, bg }) {
  slide.addShape(pres.shapes.RECTANGLE, {
    x, y, w, h,
    fill:  { color: bg || C.card },
    line:  { color: C.faint, width: 0.75 },
    shadow:{ type: "outer", color: "000000", blur: 6, offset: 1, angle: 90, opacity: 0.06 },
  });
  if (railColor) {
    slide.addShape(pres.shapes.RECTANGLE, {
      x, y, w: 0.08, h,
      fill: { color: railColor }, line: { color: railColor, width: 0 },
    });
  }
}
function chip(slide, { x, y, text, color, w = 0.9 }) {
  slide.addShape(pres.shapes.RECTANGLE, {
    x, y, w, h: 0.24,
    fill: { color }, line: { color, width: 0 },
  });
  slide.addText(text, {
    x, y, w, h: 0.24,
    fontSize: 9, fontFace: FONT_BODY, bold: true, color: "FFFFFF",
    align: "center", valign: "middle", margin: 0,
  });
}

const TOTAL = 14;

// ════════════════════════════════════════════════════════════════════════
// Slide 1 — Title
// ════════════════════════════════════════════════════════════════════════
{
  const s = pres.addSlide();
  s.background = { color: C.navy };
  s.addShape(pres.shapes.RECTANGLE, {
    x: 8.4, y: 0, w: 1.6, h: SLIDE_H,
    fill: { color: C.deepBlue }, line: { color: C.deepBlue, width: 0 },
  });
  s.addShape(pres.shapes.RECTANGLE, {
    x: 9.85, y: 0, w: 0.15, h: SLIDE_H,
    fill: { color: C.teal }, line: { color: C.teal, width: 0 },
  });

  s.addText("ATD", {
    x: 0.6, y: 0.85, w: 8.0, h: 1.2,
    fontSize: 88, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
    margin: 0, charSpacing: 4,
  });
  s.addText("Agent Tool Dispatch Protocol", {
    x: 0.6, y: 2.2, w: 7.5, h: 0.55,
    fontSize: 24, fontFace: FONT_HEAD, color: "CADCFC",
    margin: 0,
  });
  s.addText("跨 vendor 中立的 agent ↔ 工具调度协议", {
    x: 0.6, y: 2.78, w: 7.5, h: 0.45,
    fontSize: 16, fontFace: FONT_BODY, italic: true, color: "8FA1B8",
    margin: 0,
  });

  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.6, y: 3.55, w: 1.4, h: 0.04,
    fill: { color: C.teal }, line: { color: C.teal, width: 0 },
  });

  s.addText("v0.3.0  ·  Apache-2.0  ·  github.com/downsea/atd-mvp", {
    x: 0.6, y: 3.7, w: 8.5, h: 0.32,
    fontSize: 14, fontFace: FONT_BODY, color: "FFFFFF", margin: 0,
  });
  s.addText([
    { text: "13", options: { bold: true, color: "FFFFFF" } },
    { text: " crates   ·   ", options: { color: "CADCFC" } },
    { text: "378", options: { bold: true, color: "FFFFFF" } },
    { text: " tests   ·   ", options: { color: "CADCFC" } },
    { text: "35", options: { bold: true, color: "FFFFFF" } },
    { text: " conformance fixtures   ·   ", options: { color: "CADCFC" } },
    { text: "3", options: { bold: true, color: "FFFFFF" } },
    { text: " case studies", options: { color: "CADCFC" } },
  ], {
    x: 0.6, y: 4.08, w: 8.8, h: 0.32,
    fontSize: 13, fontFace: FONT_BODY, margin: 0,
  });
  s.addText(
    "实证驱动 · v1.1.0 (24%) → v1.2.0 (95.2%) → v1.4.0 (2 ATD calls vs 8 CLI fallback)",
    {
      x: 0.6, y: 4.85, w: 8.8, h: 0.3,
      fontSize: 11, fontFace: FONT_BODY, italic: true, color: "8FA1B8", margin: 0,
    });
}

// ════════════════════════════════════════════════════════════════════════
// Slide 2 — The Problem
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.neutral);
  addTitle(s, "问题", { subtitle: "Agent 调用工具的现状：碎片化、重复造轮、缺少标准" });

  const rows = [
    { tag: "Raw CLI", color: C.amber,
      desc: "每个 vendor 一个进程一个 token；多 user 要 N 套；agent 要靠 --help 文本猜命令；无审计" },
    { tag: "Raw MCP", color: C.deepBlue,
      desc: "无 server 侧 capability gate / 无 rate limit / 无 multi-tenant / 无 audit 标准" },
    { tag: "Per-vendor 自研", color: C.purple,
      desc: "每个 vendor 自己实现 capability、audit、rate limit、token 管理 — 都重写一次" },
    { tag: "Skills 层独立", color: C.teal,
      desc: "SKILL.md 散落在各 agent 平台目录里；vendor 没有标准方式把 skills 推给 agent" },
  ];
  const startY = 1.45, rowH = 0.55;
  rows.forEach((r, i) => {
    const y = startY + i * rowH;
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 9.0, h: rowH - 0.12,
      fill: { color: C.card }, line: { color: C.faint, width: 0.75 },
    });
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 0.06, h: rowH - 0.12,
      fill: { color: r.color }, line: { color: r.color, width: 0 },
    });
    s.addText(r.tag, {
      x: 0.72, y, w: 1.9, h: rowH - 0.12,
      fontSize: 14, fontFace: FONT_HEAD, bold: true, color: C.midnight,
      valign: "middle", margin: 0,
    });
    s.addText(r.desc, {
      x: 2.6, y, w: 6.8, h: rowH - 0.12,
      fontSize: 11, fontFace: FONT_BODY, color: C.slate,
      valign: "middle", margin: 0,
    });
  });

  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 4.05, w: 9.0, h: 0.7,
    fill: { color: C.deepBlue }, line: { color: C.deepBlue, width: 0 },
  });
  s.addText([
    { text: "缺一层中立协议  ⇒  ", options: { color: "CADCFC" } },
    { text: "audit / 多租户 / 跨 vendor 组合每次重新发明", options: { bold: true, color: "FFFFFF" } },
  ], {
    x: 0.5, y: 4.05, w: 9.0, h: 0.7,
    fontSize: 16, fontFace: FONT_BODY, align: "center", valign: "middle", margin: 0,
  });

  addFooter(s, 2, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 3 — What is ATD (一句话定位 + 5 messages)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.protocol);
  addTitle(s, "ATD 是什么", { subtitle: "Unix-socket 上的 5-message 协议 + 一套 server runtime" });

  // pitch box
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 1.4, w: 9.0, h: 0.95,
    fill: { color: C.midnight }, line: { color: C.midnight, width: 0 },
  });
  s.addText([
    { text: "Vendor host 工具成 ATD server  ·  ", options: { color: "CADCFC" } },
    { text: "任意 agent 平台用同一份 wire 协议 discover / describe / call / dry-run.  ", options: { bold: true, color: "FFFFFF" } },
    { text: "中间层", options: { color: "CADCFC" } },
    { text: " ship", options: { bold: true, color: "FFFFFF" } },
    { text: " 了 capability gate / audit / 多租户 / 可见性 / skills 同步 — ", options: { color: "CADCFC" } },
    { text: "raw CLI 拉不出来、raw MCP 没规范、自研 adapter 每次重写的东西.", options: { bold: true, color: "FFFFFF" } },
  ], {
    x: 0.65, y: 1.45, w: 8.7, h: 0.85,
    fontSize: 12, fontFace: FONT_BODY, valign: "middle", margin: 0,
  });

  // 5 messages
  s.addText("5 Wire Messages", {
    x: 0.5, y: 2.55, w: 9.0, h: 0.3,
    fontSize: 14, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });

  const msgs = [
    "Hello { client_id, requested_capabilities }       →  HelloAck { granted, server_version, supported_tiers }",
    "Ping                                              →  Pong",
    "ToolList                                          →  ToolListResponse { tools: [ToolSummary] }",
    "ToolSchema { tool_id }                            →  ToolSchemaResponse { schema: ToolDefinition }",
    "RunTool { tool_id, args, dry_run }                →  ToolResultResponse { result, success } | Error",
  ];
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 2.95, w: 9.0, h: 1.55,
    fill: { color: C.card }, line: { color: C.faint, width: 0.75 },
  });
  msgs.forEach((m, i) => {
    s.addText(m, {
      x: 0.7, y: 3.0 + i * 0.28, w: 8.6, h: 0.26,
      fontSize: 10, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });
  });

  // Length-prefixed JSON note
  s.addText("· Length-prefixed JSON frames  ·  Unix domain socket  ·  零 schema 协商  ·  跨语言中立", {
    x: 0.5, y: 4.7, w: 9.0, h: 0.3,
    fontSize: 11, fontFace: FONT_BODY, italic: true, color: C.muted, margin: 0,
  });

  addFooter(s, 3, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 4 — Empirical evidence (3 case studies)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.tools);
  addTitle(s, "实证证据", { subtitle: "三轮 healthkit_cli case study — 不是 marketing talk" });

  // header
  const colX = [0.5, 1.5, 4.0, 6.4, 8.5];
  const headers = ["版本", "实验设置", "工具 surface", "LLM 表现"];
  const headW   = [1.0, 2.4, 2.5, 2.95];
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 1.4, w: 9.0, h: 0.4,
    fill: { color: C.midnight }, line: { color: C.midnight, width: 0 },
  });
  headers.forEach((h, i) => {
    s.addText(h, {
      x: colX[i], y: 1.4, w: headW[i], h: 0.4,
      fontSize: 11, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
      valign: "middle", margin: 0,
    });
  });

  const rows = [
    {
      ver: "v1.1.0", setup: "Hermes + DeepSeek\n4 prompt",
      surface: "8 raw HMS endpoints\n{type:object} schema",
      result: "24% 成功率\n79 调用 / 66% invalid_args",
      color: C.red,
    },
    {
      ver: "v1.2.0", setup: "同 4 prompt",
      surface: "26 helper-tools\nauto-derived 自 CLI + SKILL.md",
      result: "95.2% 成功率\n21 调用 (-73%)",
      color: C.green,
    },
    {
      ver: "v1.4.0", setup: "1 prompt: 医生视角\n心率分析 (本介绍主参考)",
      surface: "27 工具 (25 helper + 2 skills meta)\n多租户 mode + 修过的 skill",
      result: "2 ATD calls vs 8 CLI fallback\n0 错试 / 完整 audit log",
      color: C.deepBlue,
    },
  ];
  const rowY = 1.85, rowH = 0.85;
  rows.forEach((r, i) => {
    const y = rowY + i * rowH;
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 9.0, h: rowH - 0.05,
      fill: { color: C.card }, line: { color: C.faint, width: 0.5 },
    });
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 0.06, h: rowH - 0.05,
      fill: { color: r.color }, line: { color: r.color, width: 0 },
    });
    s.addText(r.ver, {
      x: 0.65, y, w: 0.95, h: rowH - 0.05,
      fontSize: 13, fontFace: FONT_HEAD, bold: true, color: r.color,
      valign: "middle", margin: 0,
    });
    s.addText(r.setup, {
      x: 1.65, y: y + 0.05, w: 2.3, h: rowH - 0.15,
      fontSize: 10, fontFace: FONT_BODY, color: C.slate, valign: "middle", margin: 0,
    });
    s.addText(r.surface, {
      x: 4.0, y: y + 0.05, w: 2.4, h: rowH - 0.15,
      fontSize: 10, fontFace: FONT_BODY, color: C.slate, valign: "middle", margin: 0,
    });
    s.addText(r.result, {
      x: 6.45, y: y + 0.05, w: 2.95, h: rowH - 0.15,
      fontSize: 10, fontFace: FONT_BODY, bold: true, color: C.midnight,
      valign: "middle", margin: 0,
    });
  });

  // bottom note
  s.addText(
    "全部 transcript / audit.jsonl / agent reply 在 healthkit_cli/docs/case-study-v{1.2,1.4}.0/",
    {
      x: 0.5, y: 4.65, w: 9.0, h: 0.35,
      fontSize: 10, fontFace: FONT_BODY, italic: true, color: C.muted, align: "center", margin: 0,
    });

  addFooter(s, 4, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 5 — v1.4.0 head-to-head
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.runtime);
  addTitle(s, "v1.4.0 头对头实测", {
    subtitle: '"从医生角度详细分析最近两个月的心率数据" — 同一 Hermes session, 两个 surface 都摆好',
  });

  // table header
  const cols = [
    { x: 0.5,  w: 3.6, label: "维度" },
    { x: 4.15, w: 2.6, label: "ATD path" },
    { x: 6.85, w: 2.6, label: "CLI fallback path" },
  ];
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 1.5, w: 9.0, h: 0.38,
    fill: { color: C.midnight }, line: { color: C.midnight, width: 0 },
  });
  cols.forEach((c) => {
    s.addText(c.label, {
      x: c.x, y: 1.5, w: c.w, h: 0.38,
      fontSize: 12, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
      valign: "middle", margin: 0,
    });
  });

  const rows = [
    ["调用次数",        "2",            "8 (含 3 硬错)"],
    ["总耗时",          "~1.6s",        "~6s"],
    ["走错路径次数",    "0",            "3 (错 wrapper / --offset ×2)"],
    ["第一次拿到数据",  "call #1 (1.2s)", "call #6 (5s)"],
    ["Audit 可观测性",  "2 entries 完整",  "shell log only"],
    ["要 agent 知 wrapper", "否",        "是 (healthkit healthkit +x)"],
    ["要 agent 知 HMS 上限", "否",        "是 (撞错才知)"],
  ];
  const rowY = 1.93, rowH = 0.32;
  rows.forEach((r, i) => {
    const y = rowY + i * rowH;
    if (i % 2 === 0) {
      s.addShape(pres.shapes.RECTANGLE, {
        x: 0.5, y, w: 9.0, h: rowH,
        fill: { color: C.faint }, line: { color: C.faint, width: 0 },
      });
    }
    s.addText(r[0], {
      x: 0.6, y, w: 3.5, h: rowH, fontSize: 11, fontFace: FONT_BODY,
      color: C.midnight, valign: "middle", margin: 0,
    });
    s.addText(r[1], {
      x: 4.15, y, w: 2.6, h: rowH, fontSize: 11, fontFace: FONT_BODY,
      bold: true, color: C.green, valign: "middle", margin: 0,
    });
    s.addText(r[2], {
      x: 6.85, y, w: 2.6, h: rowH, fontSize: 11, fontFace: FONT_BODY,
      color: C.amber, valign: "middle", margin: 0,
    });
  });

  // conclusion
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 4.3, w: 9.0, h: 0.5,
    fill: { color: C.green }, line: { color: C.green, width: 0 },
  });
  s.addText("ATD 严格胜出 — 同样的数据、同样的报告质量；区别在 operational ergonomics + 可观测性", {
    x: 0.5, y: 4.3, w: 9.0, h: 0.5,
    fontSize: 13, fontFace: FONT_BODY, bold: true, color: "FFFFFF",
    align: "center", valign: "middle", margin: 0,
  });

  addFooter(s, 5, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 6 — ToolDefinition (declarative metadata)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.protocol);
  addTitle(s, "ToolDefinition", { subtitle: "每个工具携带的完整 declarative metadata" });

  const left = [
    { f: "id",                  v: "<publisher>:<service>.<x>.<y>" },
    { f: "description",         v: "LLM 看到的自然语言描述" },
    { f: "intent_examples",     v: "3 短语帮 LLM 匹配用户意图" },
    { f: "input_schema /",      v: "JSON Schema" },
    { f: "  output_schema",     v: "" },
    { f: "safety.level",        v: "Read / Write / Financial / Privacy / Physical / Destructive" },
    { f: "safety.dry_run",      v: "是否支持 preview-only 调用" },
  ];
  const right = [
    { f: "visibility",            v: "Read / Write / Dangerous / System / Hidden ★" },
    { f: "required_capabilities", v: "server-side 强制门禁" },
    { f: "tier",                  v: "Hot / Warm / Cold (推导 deadline + max_output)" },
    { f: "resources.max_concurrent", v: "per-tool semaphore 限并发" },
    { f: "bindings",              v: "Cli / Mcp / AppFunction / Rest" },
    { f: "trust.publisher",       v: "签发方" },
    { f: "trust.trust_level",     v: "L0 unverified  →  L4 certified" },
  ];

  function colTable(items, x0) {
    s.addShape(pres.shapes.RECTANGLE, {
      x: x0, y: 1.4, w: 4.4, h: 3.55,
      fill: { color: C.card }, line: { color: C.faint, width: 0.75 },
    });
    items.forEach((it, i) => {
      const y = 1.5 + i * 0.45;
      s.addText(it.f, {
        x: x0 + 0.15, y, w: 1.6, h: 0.4,
        fontSize: 10, fontFace: FONT_MONO, bold: true, color: C.deepBlue,
        valign: "middle", margin: 0,
      });
      s.addText(it.v, {
        x: x0 + 1.78, y, w: 2.55, h: 0.4,
        fontSize: 9, fontFace: FONT_BODY, color: C.slate,
        valign: "middle", margin: 0,
      });
    });
  }
  colTable(left, 0.5);
  colTable(right, 5.1);

  s.addText(
    "★ Hidden (v0.3.0): 不出现在 discover, 但仍可 describe + call by id — 替代 v1.2.0 的 --expose-raw-tools 开关",
    {
      x: 0.5, y: 5.05, w: 9.0, h: 0.3,
      fontSize: 10, fontFace: FONT_BODY, italic: true, color: C.purple, margin: 0,
    });

  addFooter(s, 6, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 7 — Capability Gate + Rate Limit
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.security);
  addTitle(s, "Capability Gate + Rate Limit", {
    subtitle: "Server 层强制；工具实现不用自己写 — raw MCP 没有这个",
  });

  // Capability flow
  card(s, { x: 0.5, y: 1.4, w: 4.4, h: 3.4, railColor: C.red });
  s.addText("Capability Gate (SP-12)", {
    x: 0.7, y: 1.5, w: 4.0, h: 0.3,
    fontSize: 14, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const capLines = [
    "1. Hello { requested_capabilities }",
    "2. server allow-list ∩ requested → granted",
    "3. tool.required_capabilities ⊂ granted ?",
    "    → if no: ERR_CAPABILITY_DENIED (1001)",
    "    → if yes: Tool::call(...)",
    "",
    "raw MCP: 无 server 侧 cap 概念,",
    "每个 client 自己 gate,",
    "不一致也没规范.",
  ];
  capLines.forEach((l, i) => {
    s.addText(l, {
      x: 0.7, y: 1.85 + i * 0.32, w: 4.0, h: 0.3,
      fontSize: 10, fontFace: l.startsWith("raw MCP") || i > 5 ? FONT_BODY : FONT_MONO,
      color: i > 5 ? C.muted : (l.includes("→") ? C.red : C.midnight),
      italic: i > 5,
      margin: 0,
    });
  });

  // Rate limit flow
  card(s, { x: 5.1, y: 1.4, w: 4.4, h: 3.4, railColor: C.amber });
  s.addText("Rate Limit (SP-operability-v1)", {
    x: 5.3, y: 1.5, w: 4.0, h: 0.3,
    fontSize: 14, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const rlLines = [
    "1. tool.resources.max_concurrent: u32",
    "2. dispatch: try_acquire_owned() → permit",
    "    → if NoPermits:",
    "       ERR_RATE_LIMITED (1002, retryable: true)",
    "3. permit drop = call done (success/error/panic)",
    "",
    "fail-fast: 不排队, latency 可预测.",
    "audit log 记 rate_limited outcome.",
    "",
  ];
  rlLines.forEach((l, i) => {
    s.addText(l, {
      x: 5.3, y: 1.85 + i * 0.32, w: 4.0, h: 0.3,
      fontSize: 10, fontFace: i >= 6 && i <= 7 ? FONT_BODY : FONT_MONO,
      color: i >= 6 && i <= 7 ? C.muted : (l.includes("→") || l.includes("ERR") ? C.amber : C.midnight),
      italic: i >= 6 && i <= 7,
      margin: 0,
    });
  });

  addFooter(s, 7, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 8 — TokenBroker (multi-tenant)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.security);
  addTitle(s, "TokenBroker / 多租户", {
    subtitle: "v0.3.0 起：一个 server, N caller, N OAuth — raw CLI 做不到的",
  });

  // Trait box
  card(s, { x: 0.5, y: 1.4, w: 9.0, h: 1.45, railColor: C.purple });
  s.addText("Extension point in atd-runtime", {
    x: 0.7, y: 1.5, w: 8.0, h: 0.3,
    fontSize: 13, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  s.addText(
    "trait TokenBroker {\n" +
    "    fn resolve(caller_id: Option<&str>) -> ResolveFuture<'_>;\n" +
    "}\n" +
    "// Result: Ok(Some(SecretBundle)) | Ok(None) | Err(BrokerError)\n" +
    "// SecretBundle = HashMap<String, RedactedString>  (Debug → \"<redacted>\")",
    {
      x: 0.7, y: 1.83, w: 8.6, h: 1.0,
      fontSize: 10, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });

  // Audit log proof
  card(s, { x: 0.5, y: 3.0, w: 9.0, h: 1.85, railColor: C.green });
  s.addText("v1.4.0 实测: /tmp/hk-audit.jsonl 跑 3 个 caller", {
    x: 0.7, y: 3.1, w: 8.0, h: 0.3,
    fontSize: 12, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });

  const auditCol = [
    { caller: "agent-A",  resolved: "true",  via: "broker (file: agent-A.json)" },
    { caller: "agent-B",  resolved: "true",  via: "broker (file: agent-B.json)" },
    { caller: "ghost",    resolved: "false", via: "fallback → env / saved" },
  ];
  // header
  const auditX = [0.7, 2.9, 4.8];
  ["caller_id", "secrets_resolved", "来源"].forEach((h, i) => {
    s.addText(h, {
      x: auditX[i], y: 3.5, w: 4.0, h: 0.3,
      fontSize: 10, fontFace: FONT_HEAD, bold: true, color: C.muted, margin: 0,
    });
  });
  auditCol.forEach((r, i) => {
    const y = 3.85 + i * 0.32;
    s.addText(r.caller, {
      x: 0.7, y, w: 2.0, h: 0.3, fontSize: 10, fontFace: FONT_MONO,
      color: C.midnight, margin: 0,
    });
    s.addText(r.resolved, {
      x: 2.9, y, w: 1.6, h: 0.3, fontSize: 10, fontFace: FONT_MONO,
      bold: true, color: r.resolved === "true" ? C.green : C.amber, margin: 0,
    });
    s.addText(r.via, {
      x: 4.8, y, w: 4.5, h: 0.3, fontSize: 10, fontFace: FONT_BODY,
      color: C.slate, margin: 0,
    });
  });

  addFooter(s, 8, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 9 — Audit log + Skills convention
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.tools);
  addTitle(s, "Audit Log + Skills 公约", {
    subtitle: "每次调用结构化落盘 ; SKILL.md 一键同步到 agent 平台",
  });

  // Audit
  card(s, { x: 0.5, y: 1.4, w: 4.4, h: 3.4, railColor: C.deepBlue });
  s.addText("Audit Event (JSON Lines)", {
    x: 0.7, y: 1.5, w: 4.0, h: 0.3,
    fontSize: 13, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  s.addText(
    '{\n' +
    '  "ts": "2026-04-27T15:42:30+08",\n' +
    '  "call_id": "01J...",\n' +
    '  "tool_id": "huawei:hms.\n' +
    '    healthkit.heartrate",\n' +
    '  "caller_id": "agent-A",\n' +
    '  "granted_capabilities":\n' +
    '    ["healthkit:read"],\n' +
    '  "duration_ms": 1169,\n' +
    '  "outcome": {"kind":"success"},\n' +
    '  "tier": "warm",\n' +
    '  "secrets_resolved": true\n' +
    '}',
    {
      x: 0.7, y: 1.85, w: 4.0, h: 2.85,
      fontSize: 9, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });

  // Skills convention
  card(s, { x: 5.1, y: 1.4, w: 4.4, h: 3.4, railColor: C.purple });
  s.addText("Skills Meta-tool Convention", {
    x: 5.3, y: 1.5, w: 4.0, h: 0.3,
    fontSize: 13, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const sk = [
    "<publisher>:<service>.skills.list",
    "  →  Vec<{name, description}>",
    "",
    "<publisher>:<service>.skills.get",
    "  args: { name }",
    "  →  { name, content_md }",
    "",
    "+  atd skills sync",
    "    --target { hermes | claude-code | stdout }",
    "",
    "healthkit_cli v1.3.0: 26 SKILL.md",
    "实测同步, diff 与源完全一致.",
  ];
  sk.forEach((l, i) => {
    s.addText(l, {
      x: 5.3, y: 1.85 + i * 0.24, w: 4.0, h: 0.22,
      fontSize: 10, fontFace: l.startsWith("healthkit") || i === 10 || i === 11 ? FONT_BODY : FONT_MONO,
      italic: i >= 10,
      color: i >= 10 ? C.purple : (l.includes("→") ? C.deepBlue : C.midnight),
      margin: 0,
    });
  });

  addFooter(s, 9, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 10 — Cross-vendor composition
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.tools);
  addTitle(s, "跨 Vendor 组合", {
    subtitle: "一个 agent session, N 个 vendor server — CLI 做不到的能力",
  });

  // diagram
  card(s, { x: 0.5, y: 1.4, w: 9.0, h: 2.55, bg: C.card });

  // Agent box (left)
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.85, y: 2.2, w: 1.6, h: 1.0,
    fill: { color: C.deepBlue }, line: { color: C.deepBlue, width: 0 },
  });
  s.addText("Hermes\n(or Claude /\nCursor)", {
    x: 0.85, y: 2.2, w: 1.6, h: 1.0,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
    align: "center", valign: "middle", margin: 0,
  });

  // Bridge boxes (middle)
  ["atd-mcp-bridge", "atd-mcp-bridge"].forEach((b, i) => {
    const y = 1.7 + i * 1.05;
    s.addShape(pres.shapes.RECTANGLE, {
      x: 3.25, y, w: 1.7, h: 0.8,
      fill: { color: C.teal }, line: { color: C.teal, width: 0 },
    });
    s.addText(b, {
      x: 3.25, y, w: 1.7, h: 0.8,
      fontSize: 10, fontFace: FONT_MONO, color: "FFFFFF",
      align: "center", valign: "middle", margin: 0,
    });
    // arrow from agent
    s.addShape(pres.shapes.LINE, {
      x: 2.45, y: 2.7, w: 0.8, h: i === 0 ? -0.6 : 0.5,
      line: { color: C.muted, width: 1.2, endArrowType: "triangle" },
    });
  });

  // Server boxes (right)
  const servers = [
    { name: "healthkit serve",   sub: "/tmp/hk.sock\n27 tools",  color: C.green, y: 1.7 },
    { name: "atd-mock-weather", sub: "/tmp/atd-weather.sock\n3 tools", color: C.amber, y: 2.75 },
  ];
  servers.forEach((srv, i) => {
    s.addShape(pres.shapes.RECTANGLE, {
      x: 5.8, y: srv.y, w: 2.5, h: 0.95,
      fill: { color: srv.color }, line: { color: srv.color, width: 0 },
    });
    s.addText(srv.name + "\n" + srv.sub, {
      x: 5.8, y: srv.y, w: 2.5, h: 0.95,
      fontSize: 10, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
      align: "center", valign: "middle", margin: 0,
    });
    // arrow from bridge to server
    s.addShape(pres.shapes.LINE, {
      x: 4.95, y: srv.y + 0.4, w: 0.85, h: 0,
      line: { color: C.muted, width: 1.2, endArrowType: "triangle" },
    });
  });

  // Catalog box
  s.addText(
    "Agent discover() 看到合并 catalog: huawei:hms.healthkit.* + mock:weather.*\n" +
    "工具按 description 匹配, 不需要知道 哪个 socket host 哪个 tool",
    {
      x: 0.85, y: 4.05, w: 8.4, h: 0.7,
      fontSize: 11, fontFace: FONT_BODY, italic: true, color: C.slate,
      align: "center", valign: "middle", margin: 0,
    });

  addFooter(s, 10, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 11 — vs raw CLI / raw MCP / per-vendor adapter
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.neutral);
  addTitle(s, "三方比较", { subtitle: "ATD 在协议层 ship 了 raw 选项缺的能力" });

  // Header
  const cols = [
    { x: 0.5, w: 2.4, label: "维度" },
    { x: 2.95, w: 2.05, label: "Raw CLI", color: C.amber },
    { x: 5.05, w: 2.05, label: "Raw MCP", color: C.deepBlue },
    { x: 7.15, w: 2.35, label: "ATD ★", color: C.green },
  ];
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 1.4, w: 9.0, h: 0.4,
    fill: { color: C.midnight }, line: { color: C.midnight, width: 0 },
  });
  cols.forEach((c) => {
    s.addText(c.label, {
      x: c.x, y: 1.4, w: c.w, h: 0.4,
      fontSize: 11, fontFace: FONT_HEAD, bold: true, color: c.color || "FFFFFF",
      align: "center", valign: "middle", margin: 0,
    });
  });

  const rows = [
    ["Capability gate",     "无", "client 自己", "server 强制 ✓"],
    ["Rate limit",          "无", "无", "per-tool semaphore ✓"],
    ["Audit log",           "shell history", "无规范", "JSON Lines ✓"],
    ["Multi-tenant token",  "N 进程 / N token", "stdio 单租户", "TokenBroker ✓"],
    ["Tool visibility",     "无", "二元 hidden", "5 档 (含 Hidden) ✓"],
    ["Safety levels",       "无", "无", "Read..Destructive ✓"],
    ["跨 vendor 组合",      "自己写 mux", "需自己 mux", "桥接多 socket ✓"],
    ["LLM matching",        "--help 文本", "tool desc only", "desc + intent_examples ✓"],
  ];
  const rowY = 1.85, rowH = 0.34;
  rows.forEach((r, i) => {
    const y = rowY + i * rowH;
    if (i % 2 === 0) {
      s.addShape(pres.shapes.RECTANGLE, {
        x: 0.5, y, w: 9.0, h: rowH,
        fill: { color: C.faint }, line: { color: C.faint, width: 0 },
      });
    }
    [
      { x: 0.6, w: 2.3, color: C.midnight, bold: true,  text: r[0] },
      { x: 2.95, w: 2.05, color: C.amber,    bold: false, text: r[1] },
      { x: 5.05, w: 2.05, color: C.deepBlue, bold: false, text: r[2] },
      { x: 7.15, w: 2.35, color: C.green,    bold: true,  text: r[3] },
    ].forEach((c) => {
      s.addText(c.text, {
        x: c.x, y, w: c.w, h: rowH,
        fontSize: 10, fontFace: FONT_BODY,
        color: c.color, bold: c.bold,
        valign: "middle", margin: 0,
      });
    });
  });

  // bottom
  s.addText(
    "★ 通过 atd-mcp-bridge 兼容现有 MCP 客户端 — Hermes / Claude Code / Cursor 不改一行代码",
    {
      x: 0.5, y: 4.7, w: 9.0, h: 0.3,
      fontSize: 10, fontFace: FONT_BODY, italic: true, color: C.green, align: "center", margin: 0,
    });

  addFooter(s, 11, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 12 — Architecture stack
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.neutral);
  addTitle(s, "5-Layer 架构", { subtitle: "自上而下: skills → agent → SDK → wire → runtime → tools → service" });

  const layers = [
    { name: "Skills Layer (adjacent)",        sub: "SKILL.md, atd skills sync", color: C.purple },
    { name: "Agent Framework",                sub: "Hermes / LangChain / Claude / OpenClaw", color: C.deepBlue },
    { name: "ATD SDK + CLI + MCP Bridge",     sub: "atd-sdk, atd-cli, atd-mcp-bridge", color: C.teal },
    { name: "ATD Wire Protocol",              sub: "5 messages, length-prefixed JSON, Unix socket", color: C.midnight },
    { name: "ATD Server Runtime",             sub: "atd-runtime + atd-server: capability / rate limit / audit / TokenBroker", color: C.amber },
    { name: "Vendor Tools",                   sub: "healthkit_cli, atd-mock-weather-server, ...", color: C.green },
    { name: "Underlying Service",             sub: "Huawei HMS REST, OpenWeatherMap, ...", color: C.slate },
  ];
  const rowY = 1.4, rowH = 0.5;
  layers.forEach((L, i) => {
    const y = rowY + i * rowH;
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 9.0, h: rowH - 0.07,
      fill: { color: C.card }, line: { color: C.faint, width: 0.75 },
    });
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 0.18, h: rowH - 0.07,
      fill: { color: L.color }, line: { color: L.color, width: 0 },
    });
    s.addText(L.name, {
      x: 0.85, y, w: 4.0, h: rowH - 0.07,
      fontSize: 12, fontFace: FONT_HEAD, bold: true, color: C.midnight,
      valign: "middle", margin: 0,
    });
    s.addText(L.sub, {
      x: 4.95, y, w: 4.55, h: rowH - 0.07,
      fontSize: 10, fontFace: FONT_BODY, color: C.slate,
      valign: "middle", margin: 0,
    });
  });

  addFooter(s, 12, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 13 — Workspace + 上手 5 分钟
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.tools);
  addTitle(s, "Workspace + 上手 5 分钟", {
    subtitle: "13 crates, 378 tests, Apache-2.0 — 5 分钟跑通 + 5 行写自己的 server",
  });

  // Workspace
  card(s, { x: 0.5, y: 1.4, w: 4.4, h: 3.55, railColor: C.green });
  s.addText("Crates", {
    x: 0.7, y: 1.5, w: 4.0, h: 0.3,
    fontSize: 13, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const crates = [
    "atd-protocol      wire 格式 + 类型",
    "atd-sdk           Rust 客户端",
    "atd-runtime       server runtime",
    "atd-server        Unix socket listener",
    "atd-tools-{echo,fs,shell,web}",
    "atd-ref-server    参考 server bin",
    "atd-mcp-bridge    MCP/stdio ↔ wire",
    "atd-cli           atd 开发 CLI",
    "atd-conformance   35 跨实现 fixture",
    "atd-mock-weather-server  组合 demo",
  ];
  crates.forEach((c, i) => {
    s.addText(c, {
      x: 0.7, y: 1.85 + i * 0.3, w: 4.0, h: 0.28,
      fontSize: 10, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });
  });

  // 5 min start
  card(s, { x: 5.1, y: 1.4, w: 4.4, h: 3.55, railColor: C.deepBlue });
  s.addText("5 分钟跑通", {
    x: 5.3, y: 1.5, w: 4.0, h: 0.3,
    fontSize: 13, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const cmds = [
    "$ cargo build --release",
    "    -p atd-ref-server",
    "    -p atd-cli",
    "    -p atd-mcp-bridge",
    "",
    "$ ./target/release/atd-ref-server &",
    "",
    "$ atd list",
    "$ atd schema ref:fs.read",
    "$ atd call ref:echo.say",
    "    --args '{\"text\":\"hi\"}'",
  ];
  cmds.forEach((l, i) => {
    s.addText(l, {
      x: 5.3, y: 1.85 + i * 0.27, w: 4.0, h: 0.25,
      fontSize: 10, fontFace: FONT_MONO,
      color: l.startsWith("$") ? C.green : C.slate,
      margin: 0,
    });
  });

  addFooter(s, 13, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 14 — Closing
// ════════════════════════════════════════════════════════════════════════
{
  const s = pres.addSlide();
  s.background = { color: C.navy };
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0, y: 0, w: 0.18, h: SLIDE_H,
    fill: { color: C.teal }, line: { color: C.teal, width: 0 },
  });

  s.addText("一句话回顾", {
    x: 0.6, y: 0.55, w: 8.5, h: 0.45,
    fontSize: 22, fontFace: FONT_HEAD, bold: true, color: "FFFFFF", margin: 0,
  });

  // big quote
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.6, y: 1.25, w: 8.8, h: 2.4,
    fill: { color: "1A2D54" }, line: { color: "1A2D54", width: 0 },
  });
  s.addText([
    { text: "ATD = ",                                                            options: { color: "8FA1B8" } },
    { text: "5-message Unix-socket 协议",                                          options: { bold: true, color: "FFFFFF" } },
    { text: "  +  ",                                                             options: { color: "8FA1B8" } },
    { text: "一套 server runtime",                                                options: { bold: true, color: "FFFFFF" } },
    { text: "  +  ",                                                             options: { color: "8FA1B8" } },
    { text: "桥接到任意 agent 平台",                                                options: { bold: true, color: "FFFFFF" } },
    { text: "\n\nVendor 写一份 server 就被任意 agent 平台用,\n",                  options: { color: "CADCFC" } },
    { text: "并自带审计 / 多租户 / 跨 vendor 组合",                                  options: { bold: true, color: C.teal } },
    { text: " — ",                                                               options: { color: "CADCFC" } },
    { text: "raw CLI 拉不出来、raw MCP 没规范、自研 adapter 每次重写的东西.",          options: { color: "CADCFC" } },
  ], {
    x: 0.85, y: 1.4, w: 8.3, h: 2.1,
    fontSize: 16, fontFace: FONT_BODY, valign: "top", margin: 0,
    paraSpaceAfter: 8,
  });

  // Evidence call-out
  s.addText([
    { text: "实证: ",                                                              options: { color: "8FA1B8" } },
    { text: "v1.4.0 case study",                                                    options: { bold: true, color: "FFFFFF" } },
    { text: " — 1 个 prompt, ",                                                    options: { color: "CADCFC" } },
    { text: "2 ATD calls vs 8 CLI fallback",                                        options: { bold: true, color: C.teal } },
    { text: ", 完整 audit log 落盘.\n",                                            options: { color: "CADCFC" } },
    { text: "协议层差异, 不是工具能力差异.",                                          options: { italic: true, color: "8FA1B8" } },
  ], {
    x: 0.6, y: 3.85, w: 8.8, h: 0.85,
    fontSize: 13, fontFace: FONT_BODY, margin: 0,
  });

  // Footer
  s.addText("github.com/downsea/atd-mvp   ·   v0.3.0   ·   Apache-2.0", {
    x: 0.6, y: 5.0, w: 8.8, h: 0.3,
    fontSize: 12, fontFace: FONT_BODY, color: "8FA1B8", margin: 0,
  });
}

// ─── write ────────────────────────────────────────────────────────────────
pres.writeFile({ fileName: "docs/whitepaper/atd-introduction.pptx" })
    .then((f) => console.log("[ok] wrote", f))
    .catch((e) => { console.error(e); process.exit(1); });
