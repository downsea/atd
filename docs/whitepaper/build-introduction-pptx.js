// Builds docs/whitepaper/atd-introduction.pptx
//
// 立场介绍 deck — 以 5 个真跑过的 Hermes case study 为锚点：
//   v1.2.0 q1 (5K readiness)
//   v1.2.0 q2 (weekly compare)
//   v1.2.0 q3 (step challenge)
//   v1.2.0 q4 (daily report)
//   v1.4.0 doctor-perspective HR analysis
//
// 每个 case 一张 slide，带 prompt / 工具调用序列 / agent 实际回复摘录 /
// 数据指标。后半段是 ATD 协议层能力 + 三方对比 + 架构 + 上手。
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
pres.subject = "Introduction grounded in 5 Hermes case studies";

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
function addTitle(slide, text, { subtitle, y = 0.32 } = {}) {
  slide.addText(text, {
    x: 0.45, y, w: 9.1, h: 0.5,
    fontSize: 24, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  if (subtitle) {
    slide.addText(subtitle, {
      x: 0.45, y: y + 0.52, w: 9.1, h: 0.3,
      fontSize: 12, fontFace: FONT_BODY, color: C.slate, italic: true, margin: 0,
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

// ─── Case slide builder ──────────────────────────────────────────────────
function caseSlide(opts) {
  const { caseNum, total, color, title, subtitle, prompt, calls, callsLabel,
          metrics, excerpt, excerptLabel } = opts;
  const s = contentSlide(color);
  addTitle(s, title, { subtitle });

  // 1. Prompt callout (top)
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 1.05, w: 9.0, h: 0.55,
    fill: { color: C.midnight }, line: { color: C.midnight, width: 0 },
  });
  s.addText("用户 Prompt", {
    x: 0.65, y: 1.05, w: 1.4, h: 0.55,
    fontSize: 9, fontFace: FONT_HEAD, bold: true, color: "8FA1B8",
    valign: "middle", margin: 0,
  });
  s.addText(prompt, {
    x: 1.95, y: 1.05, w: 7.4, h: 0.55,
    fontSize: 12, fontFace: FONT_BODY, color: "FFFFFF", italic: true,
    valign: "middle", margin: 0,
  });

  // 2. Tool sequence (left card)
  card(s, { x: 0.5, y: 1.75, w: 4.4, h: 2.85, railColor: C.teal });
  s.addText(callsLabel || "工具调用序列", {
    x: 0.7, y: 1.85, w: 4.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  calls.forEach((c, i) => {
    s.addText(c, {
      x: 0.7, y: 2.18 + i * 0.27, w: 4.05, h: 0.25,
      fontSize: 9, fontFace: FONT_MONO,
      color: c.startsWith("✗") ? C.red : (c.startsWith("✓") ? C.green : C.midnight),
      margin: 0,
    });
  });

  // 3. Excerpt (right card)
  card(s, { x: 5.1, y: 1.75, w: 4.4, h: 2.85, railColor: color });
  s.addText(excerptLabel || "Agent 实际回复（摘录）", {
    x: 5.3, y: 1.85, w: 4.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  excerpt.forEach((line, i) => {
    s.addText(line.text, {
      x: 5.3, y: 2.18 + i * 0.27, w: 4.05, h: 0.25,
      fontSize: line.fontSize || 9, fontFace: FONT_BODY,
      color: line.color || C.slate, bold: line.bold,
      margin: 0,
    });
  });

  // 4. Metrics row (bottom)
  const mY = 4.75, mH = 0.5;
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: mY, w: 9.0, h: mH,
    fill: { color: C.faint }, line: { color: C.faint, width: 0 },
  });
  const colW = 9.0 / metrics.length;
  metrics.forEach((m, i) => {
    s.addText(m.label, {
      x: 0.5 + i * colW, y: mY, w: colW, h: mH * 0.45,
      fontSize: 8, fontFace: FONT_BODY, color: C.muted,
      align: "center", valign: "bottom", margin: 0,
    });
    s.addText(m.value, {
      x: 0.5 + i * colW, y: mY + mH * 0.4, w: colW, h: mH * 0.6,
      fontSize: m.fontSize || 13, fontFace: FONT_HEAD, bold: true,
      color: m.color || C.midnight,
      align: "center", valign: "top", margin: 0,
    });
  });

  addFooter(s, caseNum + 3, total);  // case 1 = slide 4 (after title/problem/overview)
  return s;
}

const TOTAL = 19;

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
    x: 0.6, y: 0.7, w: 8.0, h: 1.3,
    fontSize: 96, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
    margin: 0, charSpacing: 4,
  });
  s.addText("Agent Tool Dispatch Protocol", {
    x: 0.6, y: 2.2, w: 7.5, h: 0.55,
    fontSize: 24, fontFace: FONT_HEAD, color: "CADCFC",
    margin: 0,
  });
  s.addText("跨 vendor 中立的 agent ↔ 工具调度协议", {
    x: 0.6, y: 2.8, w: 7.5, h: 0.45,
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

  s.addText("5 个真跑过的 Hermes case study  ·  实证驱动 · 不是 marketing talk", {
    x: 0.6, y: 4.15, w: 8.8, h: 0.32,
    fontSize: 12, fontFace: FONT_BODY, italic: true, color: C.teal, margin: 0,
  });

  s.addText([
    { text: "v1.2.0 q1: 5K体能 ✓  ·  ", options: { color: "CADCFC" } },
    { text: "q2: 周对比 ✓  ·  ",        options: { color: "CADCFC" } },
    { text: "q3: 步数挑战 ✓  ·  ",       options: { color: "CADCFC" } },
    { text: "q4: 健康日报 ✓  ·  ",       options: { color: "CADCFC" } },
    { text: "v1.4.0 医生视角 ✓",          options: { bold: true, color: "FFFFFF" } },
  ], {
    x: 0.6, y: 4.6, w: 8.8, h: 0.32,
    fontSize: 11, fontFace: FONT_BODY, margin: 0,
  });
}

// ════════════════════════════════════════════════════════════════════════
// Slide 2 — The Problem + What is ATD (合并版)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.neutral);
  addTitle(s, "问题 + ATD 是什么", {
    subtitle: "Agent 调用工具的现状碎片化；ATD 在协议层 ship 了缺的能力",
  });

  // 左：问题
  card(s, { x: 0.5, y: 1.0, w: 4.4, h: 3.85, railColor: C.red });
  s.addText("问题", {
    x: 0.7, y: 1.1, w: 4.0, h: 0.3,
    fontSize: 13, fontFace: FONT_HEAD, bold: true, color: C.red, margin: 0,
  });
  const problems = [
    { tag: "Raw CLI",         desc: "N user 要 N 进程; agent 要靠 --help 猜命令; 无审计" },
    { tag: "Raw MCP",         desc: "无 server 侧 capability gate / rate limit / multi-tenant / audit 标准" },
    { tag: "Per-vendor 自研", desc: "每个 vendor 重写 capability/audit/rate limit/token 管理" },
    { tag: "Skills 散落",     desc: "SKILL.md 在各 agent 平台目录, vendor 没有标准方式推" },
  ];
  problems.forEach((p, i) => {
    const y = 1.5 + i * 0.78;
    s.addText(p.tag, {
      x: 0.7, y, w: 4.05, h: 0.3,
      fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.red, margin: 0,
    });
    s.addText(p.desc, {
      x: 0.7, y: y + 0.3, w: 4.05, h: 0.45,
      fontSize: 9.5, fontFace: FONT_BODY, color: C.slate, margin: 0,
    });
  });

  // 右：ATD 解法
  card(s, { x: 5.1, y: 1.0, w: 4.4, h: 3.85, railColor: C.green });
  s.addText("ATD 怎么解", {
    x: 5.3, y: 1.1, w: 4.0, h: 0.3,
    fontSize: 13, fontFace: FONT_HEAD, bold: true, color: C.green, margin: 0,
  });
  const sols = [
    { tag: "5-msg wire 协议",   desc: "Hello / Ping / ToolList / ToolSchema / RunTool — 跨语言中立" },
    { tag: "Server runtime",   desc: "capability gate · rate limit · audit · TokenBroker · visibility" },
    { tag: "Bridge 兼容现有 MCP", desc: "atd-mcp-bridge: Hermes/Claude/Cursor 不改一行就能用" },
    { tag: "Skills 公约",       desc: "<x>.skills.list/get + atd skills sync 推到 agent 平台" },
  ];
  sols.forEach((p, i) => {
    const y = 1.5 + i * 0.78;
    s.addText(p.tag, {
      x: 5.3, y, w: 4.05, h: 0.3,
      fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.green, margin: 0,
    });
    s.addText(p.desc, {
      x: 5.3, y: y + 0.3, w: 4.05, h: 0.45,
      fontSize: 9.5, fontFace: FONT_BODY, color: C.slate, margin: 0,
    });
  });

  addFooter(s, 2, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 3 — 5 cases overview
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.tools);
  addTitle(s, "5 Case Study 总览", {
    subtitle: "全部真跑过 Hermes + DeepSeek-chat — transcript / audit log 公开",
  });

  // header row
  const cols = [
    { x: 0.5,  w: 0.65, label: "#" },
    { x: 1.2,  w: 1.3,  label: "版本" },
    { x: 2.55, w: 3.6,  label: "Prompt 概要" },
    { x: 6.2,  w: 1.6,  label: "工具调用" },
    { x: 7.85, w: 1.65, label: "outcome" },
  ];
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 1.05, w: 9.0, h: 0.4,
    fill: { color: C.midnight }, line: { color: C.midnight, width: 0 },
  });
  cols.forEach((c) => {
    s.addText(c.label, {
      x: c.x, y: 1.05, w: c.w, h: 0.4,
      fontSize: 11, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
      valign: "middle", margin: 0,
    });
  });

  const rows = [
    { num: "1", ver: "v1.2.0 q1", color: C.green,
      prompt: "我想跑个 5 公里, 帮我评估今天身体状态适不适合",
      tools: "5 ATD\n(1 HRV ✗)", outcome: "✓ 绿灯放行" },
    { num: "2", ver: "v1.2.0 q2", color: C.green,
      prompt: "对比本周和上周心率/步数/卡路里",
      tools: "4 ATD\n+ 3 Python", outcome: "✓ 完整周对比表" },
    { num: "3", ver: "v1.2.0 q3", color: C.green,
      prompt: "帮我创建一个本周步数挑战",
      tools: "2 ATD + 1 cron\n+ Python", outcome: "✓ 33,424/70K + 调整建议" },
    { num: "4", ver: "v1.2.0 q4", color: C.green,
      prompt: "生成今天的健康日报",
      tools: "10 ATD parallel\n+ 1 skill_view", outcome: "✓ 10 指标日报" },
    { num: "5", ver: "v1.4.0", color: C.deepBlue,
      prompt: "从医生角度详细分析最近两个月的心率数据",
      tools: "2 ATD + 8 CLI\nfallback", outcome: "✓ 医生视角报告" },
  ];
  const rowY = 1.5, rowH = 0.62;
  rows.forEach((r, i) => {
    const y = rowY + i * rowH;
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 9.0, h: rowH - 0.04,
      fill: { color: i % 2 === 0 ? C.card : "F1F5F9" },
      line: { color: C.faint, width: 0.5 },
    });
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 0.06, h: rowH - 0.04,
      fill: { color: r.color }, line: { color: r.color, width: 0 },
    });
    s.addText(r.num, {
      x: 0.62, y, w: 0.5, h: rowH - 0.04,
      fontSize: 16, fontFace: FONT_HEAD, bold: true, color: r.color,
      align: "center", valign: "middle", margin: 0,
    });
    s.addText(r.ver, {
      x: 1.2, y, w: 1.3, h: rowH - 0.04,
      fontSize: 11, fontFace: FONT_MONO, bold: true, color: C.midnight,
      valign: "middle", margin: 0,
    });
    s.addText(r.prompt, {
      x: 2.55, y, w: 3.55, h: rowH - 0.04,
      fontSize: 10, fontFace: FONT_BODY, italic: true, color: C.slate,
      valign: "middle", margin: 0,
    });
    s.addText(r.tools, {
      x: 6.2, y, w: 1.6, h: rowH - 0.04,
      fontSize: 9.5, fontFace: FONT_MONO, color: C.deepBlue,
      valign: "middle", margin: 0,
    });
    s.addText(r.outcome, {
      x: 7.85, y, w: 1.6, h: rowH - 0.04,
      fontSize: 9.5, fontFace: FONT_BODY, bold: true, color: C.green,
      valign: "middle", margin: 0,
    });
  });

  addFooter(s, 3, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 4 — Case 1 (v1.2.0 q1: 5K readiness)
// ════════════════════════════════════════════════════════════════════════
caseSlide({
  caseNum: 1, total: TOTAL, color: LAYER.runtime,
  title: "Case 1 · v1.2.0 q1 — 5公里跑步可行性评估",
  subtitle: "Hermes + DeepSeek; agent 自动选 ATD 工具串联多个数据源",
  prompt: "我想跑个 5 公里, 帮我评估一下今天身体状态适不适合.",
  calls: [
    "✓ huawei:hms.healthkit.heartrate",
    "✓ huawei:hms.healthkit.restinghr",
    "✓ huawei:hms.healthkit.sleep",
    "✓ huawei:hms.healthkit.stress",
    "✗ huawei:hms.healthkit.hrv  (HMS 拒)",
    "",
    "→ 5 调用并行 (单次 round-trip)",
    "→ 4 success + 1 invalid_args",
    "→ HRV 失败不影响结论",
  ],
  excerpt: [
    { text: "▲ 身体状况评估 — 适合跑 5K 吗?", bold: true, color: C.midnight, fontSize: 11 },
    { text: "" },
    { text: "✓ 静息心率 61 bpm — 正常健康", color: C.green, bold: true },
    { text: "✓ 压力值 26 — 身体很放松", color: C.green, bold: true },
    { text: "✓ 睡眠 6h40m — 昨晚无中途醒来", color: C.green, bold: true },
    { text: "✓ 当前 HR 98 bpm — 状态稳定", color: C.green, bold: true },
    { text: "" },
    { text: "结论: 绿灯放行, 注意补水 + 控制配速", color: C.midnight, bold: true, fontSize: 10 },
    { text: "(失败的 HRV 路径不影响总结论)", color: C.muted, fontSize: 9 },
  ],
  metrics: [
    { label: "ATD 调用", value: "5" },
    { label: "成功率", value: "80%", color: C.amber },
    { label: "拿到数据", value: "1 round-trip" },
    { label: "agent 决策", value: "GO ✓", color: C.green },
  ],
});

// ════════════════════════════════════════════════════════════════════════
// Slide 5 — Case 2 (v1.2.0 q2: weekly compare)
// ════════════════════════════════════════════════════════════════════════
caseSlide({
  caseNum: 2, total: TOTAL, color: LAYER.runtime,
  title: "Case 2 · v1.2.0 q2 — 本周 vs 上周健康数据对比",
  subtitle: "Agent 串联多源数据 + execute_code 做 statistics",
  prompt: "对比本周和上周的心率、静息心率、步数、卡路里",
  calls: [
    "✓ huawei:hms.healthkit.heartrate",
    "✓ huawei:hms.healthkit.restinghr",
    "✓ huawei:hms.healthkit.steps",
    "✓ huawei:hms.healthkit.calories",
    "↓",
    "🐍 execute_code (Python)  ×3",
    "    日期解析 / 周分组 / 统计聚合",
    "",
    "→ 4 ATD success + 0 retry",
  ],
  excerpt: [
    { text: "▲ 给你完整的周对比分析", bold: true, color: C.midnight, fontSize: 11 },
    { text: "" },
    { text: "周日均: HR 79 / RHR 64 / 5,214 步 / 318 kcal", fontSize: 9 },
    { text: "周四 HR 峰值 153 (强度最大)", color: C.amber, fontSize: 9 },
    { text: "周三 RHR 最低 63 bpm (心肺最佳)", color: C.green, fontSize: 9 },
    { text: "周六步数最高 6,825 (户外活动)", fontSize: 9 },
    { text: "" },
    { text: "本周 vs 上周: RHR 下降 -6 bpm,", color: C.green, bold: true, fontSize: 10 },
    { text: "  步数 −7%, 卡路里 −9%", color: C.midnight, fontSize: 9 },
  ],
  metrics: [
    { label: "ATD 调用", value: "4" },
    { label: "成功率", value: "100%", color: C.green },
    { label: "Python 分析", value: "3" },
    { label: "数据维度", value: "4 metric × 2 周", fontSize: 11 },
  ],
});

// ════════════════════════════════════════════════════════════════════════
// Slide 6 — Case 3 (v1.2.0 q3: step challenge)
// ════════════════════════════════════════════════════════════════════════
caseSlide({
  caseNum: 3, total: TOTAL, color: LAYER.runtime,
  title: "Case 3 · v1.2.0 q3 — 本周步数挑战 + cron 推送",
  subtitle: "ATD 拿数据 + cronjob 设定时推送 + Python 算进度",
  prompt: "帮我创建一个本周步数挑战",
  calls: [
    "📚 skill_view huawei-healthkit-analysis",
    "✓ huawei:hms.healthkit.healthkit-overview",
    "✓ huawei:hms.healthkit.steps",
    "🐍 execute_code (Python 算进度)",
    "⏰ cronjob create 本周步数挑战",
    "",
    "→ 2 ATD + 1 cron + 1 Python",
    "→ 共 0 retry (skill 引导一次成型)",
  ],
  excerpt: [
    { text: "🏃 本周步数挑战", bold: true, color: C.midnight, fontSize: 11 },
    { text: "" },
    { text: "目标: 70,000 步 / 周", color: C.midnight, fontSize: 10 },
    { text: "已完成: 33,424 / 70,000 = 47.7%", color: C.amber, bold: true, fontSize: 10 },
    { text: "" },
    { text: "⏰ 每天早 8:00 自动推送进度", color: C.purple, fontSize: 9 },
    { text: "💡 建议下调到 40K/周(日均~5.7K)", color: C.green, fontSize: 9 },
    { text: "  先突破日均 5K, 再挑战 10K", color: C.muted, fontSize: 9 },
  ],
  metrics: [
    { label: "ATD 调用", value: "2" },
    { label: "辅助工具", value: "skill + cron + py", fontSize: 11 },
    { label: "Side effect", value: "✓ cron 已创建", color: C.purple },
    { label: "Agent 提议", value: "70K → 40K", color: C.amber, fontSize: 11 },
  ],
});

// ════════════════════════════════════════════════════════════════════════
// Slide 7 — Case 4 (v1.2.0 q4: daily report)
// ════════════════════════════════════════════════════════════════════════
caseSlide({
  caseNum: 4, total: TOTAL, color: LAYER.runtime,
  title: "Case 4 · v1.2.0 q4 — 完整健康日报 (10 指标并行)",
  subtitle: "Agent 一次拉 10 个 ATD 工具, 生成结构化日报",
  prompt: "生成今天的健康日报",
  calls: [
    "📚 skill_view huawei-healthkit-analysis",
    "✓ huawei:hms.healthkit.healthkit-overview",
    "✓ heartrate    ✓ restinghr",
    "✓ steps        ✓ calories",
    "✓ sleep        ✓ spo2",
    "✓ distance     ✓ stress",
    "✓ activeminutes ✓ vo2max",
    "🐍 execute_code (聚合 + 7 日基线)",
    "→ 10 ATD parallel + 1 skill + py",
  ],
  excerpt: [
    { text: "▲ 今日健康概览 (vs 7日基线)", bold: true, color: C.midnight, fontSize: 11 },
    { text: "" },
    { text: "1. RHR 61 — 良好(基线 65)", color: C.green, fontSize: 9 },
    { text: "2. 步数 2,140 — 走低(基线 4,775)", color: C.amber, fontSize: 9 },
    { text: "3. 睡眠 6.7h — 略低(推荐 7-8h)", color: C.amber, fontSize: 9 },
    { text: "4. SpO2 98% — 稳定优秀", color: C.green, fontSize: 9 },
    { text: "5. 压力 42 — 正常可控", color: C.green, fontSize: 9 },
    { text: "" },
    { text: "建议: 今日步数偏低, 出门走动一下", color: C.midnight, bold: true, fontSize: 10 },
  ],
  metrics: [
    { label: "ATD 调用", value: "10", color: C.green },
    { label: "成功率", value: "100%", color: C.green },
    { label: "并行执行", value: "single round-trip", fontSize: 10 },
    { label: "指标维度", value: "10", color: C.midnight },
  ],
});

// ════════════════════════════════════════════════════════════════════════
// Slide 8 — Case 5 (v1.4.0 doctor analysis)
// ════════════════════════════════════════════════════════════════════════
caseSlide({
  caseNum: 5, total: TOTAL, color: LAYER.protocol,
  title: "Case 5 · v1.4.0 — 医生视角心率分析 (ATD vs CLI 头对头)",
  subtitle: "同一 Hermes session, 两个 surface 都摆好 → ATD 严格胜出",
  prompt: "从医生角度详细分析一下我最近两个月的心率数据, 给出具体的建议.",
  callsLabel: "ATD path  vs  CLI fallback",
  calls: [
    "✓ ATD heartrate {days:60}  1169ms",
    "✓ ATD restinghr {days:60}   438ms",
    "─────────── 然后 fall through ──",
    "✗ terminal +heartrate (错 wrapper)",
    "✓ healthkit --help",
    "✗ +heartrate --days 60 (HMS 拒)",
    "✓ +heartrate --days 7",
    "✗ --offset (flag 不存在 ×2)",
    "✓ for-loop 8 周 --start/--end",
    "🐍 execute_code (Python 分析)",
  ],
  excerpt: [
    { text: "▲ 心率数据分析 — 医生视角", bold: true, color: C.midnight, fontSize: 11 },
    { text: "" },
    { text: "RHR 平均 65.9 bpm — 良好(60-70)", color: C.green, fontSize: 9 },
    { text: "日均 HR 84 bpm — 正常偏高", color: C.amber, fontSize: 9 },
    { text: "▲ 第3天峰值 153 (运动区)", color: C.amber, fontSize: 9 },
    { text: "▲ 第7天最低 41 (低于 50, 监测)", color: C.red, fontSize: 9 },
    { text: "" },
    { text: "建议: 7-8h 睡眠 / 中等有氧 3-5×/周", fontSize: 9 },
    { text: "若 RHR 持续爬至 75+ 建议就医", color: C.red, fontSize: 9 },
    { text: "心悸 / 头晕 → 24h Holter", color: C.red, bold: true, fontSize: 9 },
  ],
  metrics: [
    { label: "ATD 调用", value: "2", color: C.green },
    { label: "CLI fallback", value: "8", color: C.amber },
    { label: "ATD 总耗时", value: "1.6s", color: C.green },
    { label: "CLI 走错路径", value: "3 次", color: C.red, fontSize: 11 },
  ],
});

// ════════════════════════════════════════════════════════════════════════
// Slide 9 — ATD core abstractions (compact)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.protocol);
  addTitle(s, "ATD 协议核心抽象", {
    subtitle: "5 wire messages + ToolDefinition + per-call CallContext",
  });

  // 5 messages
  card(s, { x: 0.5, y: 1.0, w: 9.0, h: 1.5, railColor: C.deepBlue });
  s.addText("5 Wire Messages (length-prefixed JSON over Unix socket)", {
    x: 0.7, y: 1.1, w: 8.7, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const msgs = [
    "Hello { client_id, requested_capabilities }   →  HelloAck { granted, server_version, supported_tiers }",
    "Ping                                          →  Pong",
    "ToolList                                      →  ToolListResponse { tools: [ToolSummary] }",
    "ToolSchema { tool_id }                        →  ToolSchemaResponse { schema: ToolDefinition }",
    "RunTool { tool_id, args, dry_run }            →  ToolResultResponse { result, success } | Error",
  ];
  msgs.forEach((m, i) => {
    s.addText(m, {
      x: 0.7, y: 1.42 + i * 0.2, w: 8.7, h: 0.18,
      fontSize: 8.5, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });
  });

  // ToolDefinition compact
  card(s, { x: 0.5, y: 2.65, w: 4.4, h: 2.2, railColor: C.teal });
  s.addText("ToolDefinition (declarative metadata)", {
    x: 0.7, y: 2.75, w: 4.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const td = [
    "id              — <publisher>:<service>.<x>",
    "description     — LLM-facing 自然语言",
    "intent_examples — 3 短语帮匹配意图",
    "input_schema    — JSON Schema",
    "safety.level    — Read..Destructive",
    "visibility      — Hidden / Read / Write / ...",
    "required_capabilities — server gate",
    "tier            — Hot / Warm / Cold",
  ];
  td.forEach((l, i) => {
    s.addText(l, {
      x: 0.7, y: 3.1 + i * 0.21, w: 4.05, h: 0.2,
      fontSize: 9, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });
  });

  // CallContext compact
  card(s, { x: 5.1, y: 2.65, w: 4.4, h: 2.2, railColor: C.amber });
  s.addText("CallContext (per-call ctx)", {
    x: 5.3, y: 2.75, w: 4.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const cc = [
    "cwd, max_output_bytes, call_id",
    "deadline             — 由 tier 推导",
    "capabilities         — Hello 协商的 granted",
    "tier: ToolTier",
    "caller_id            — 来自 Hello.client_id",
    "secrets              — 来自 TokenBroker (v0.3.0)",
    "                       Option<Arc<SecretBundle>>",
    "read_tracker         — 跨 connection 共享",
  ];
  cc.forEach((l, i) => {
    s.addText(l, {
      x: 5.3, y: 3.1 + i * 0.21, w: 4.05, h: 0.2,
      fontSize: 9, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });
  });

  addFooter(s, 9, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 10 — ATD 模块架构 (11-crate dependency graph)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.protocol);
  addTitle(s, "ATD 模块架构", {
    subtitle: "11 crates · 共享 atd-protocol · 客户端 / 服务端两侧分别落地",
  });

  // 三柱：客户端 / 共享 wire / 服务端
  const colW = 2.95, colY = 1.05, colH = 3.0;

  // ── 左：客户端 ────────────────────────────────────────────────────────
  card(s, { x: 0.5, y: colY, w: colW, h: colH, railColor: C.teal });
  s.addText("客户端 / 集成层", {
    x: 0.7, y: colY + 0.08, w: colW - 0.4, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const lhs = [
    { name: "atd-mcp-bridge", desc: "binary · MCP/stdio ↔ ATD wire", bold: true },
    { name: "atd-cli",        desc: "binary · `atd list/schema/call`", bold: true },
    { name: "atd-sdk",        desc: "Rust crate · AtdClient + adapters", bold: true },
    { name: "  · adapters",   desc: "openai / anthropic / langchain helpers", bold: false },
  ];
  lhs.forEach((c, i) => {
    const y = colY + 0.5 + i * 0.6;
    s.addText(c.name, {
      x: 0.7, y, w: colW - 0.4, h: 0.28,
      fontSize: 10.5, fontFace: FONT_MONO, bold: c.bold, color: C.deepBlue, margin: 0,
    });
    s.addText(c.desc, {
      x: 0.7, y: y + 0.28, w: colW - 0.4, h: 0.28,
      fontSize: 9, fontFace: FONT_BODY, color: C.slate, margin: 0,
    });
  });

  // ── 中：共享 wire ─────────────────────────────────────────────────────
  card(s, { x: 0.5 + colW + 0.15, y: colY, w: colW, h: colH, railColor: C.midnight });
  s.addText("共享 wire 层", {
    x: 0.7 + colW + 0.15, y: colY + 0.08, w: colW - 0.4, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  s.addText("atd-protocol", {
    x: 0.7 + colW + 0.15, y: colY + 0.5, w: colW - 0.4, h: 0.3,
    fontSize: 13, fontFace: FONT_MONO, bold: true, color: C.midnight, margin: 0,
  });
  const proto = [
    "5 wire messages",
    "ToolDefinition / Summary",
    "CallContext / SecretBundle",
    "ToolResult / errors",
    "wire framing (length-prefix)",
    "sanitize (id 非法字符)",
  ];
  proto.forEach((l, i) => {
    s.addText("· " + l, {
      x: 0.7 + colW + 0.15, y: colY + 0.85 + i * 0.31, w: colW - 0.4, h: 0.28,
      fontSize: 9, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });
  });

  // ── 右：服务端 ────────────────────────────────────────────────────────
  card(s, { x: 0.5 + 2 * (colW + 0.15), y: colY, w: colW, h: colH, railColor: C.amber });
  s.addText("服务端 / 工具层", {
    x: 0.7 + 2 * (colW + 0.15), y: colY + 0.08, w: colW - 0.4, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const rhs = [
    { name: "atd-runtime",       desc: "dispatcher · capability · rate limit · audit · TokenBroker", bold: true },
    { name: "atd-server",        desc: "Unix socket listener (uses runtime)", bold: true },
    { name: "atd-tools-{echo,fs,shell,web}", desc: "built-in 工具 (4 crates)", bold: false },
    { name: "atd-ref-server",    desc: "binary · 自带的 demo server (uses tools)", bold: true },
  ];
  rhs.forEach((c, i) => {
    const y = colY + 0.5 + i * 0.6;
    s.addText(c.name, {
      x: 0.7 + 2 * (colW + 0.15), y, w: colW - 0.4, h: 0.28,
      fontSize: 10.5, fontFace: FONT_MONO, bold: c.bold, color: C.amber, margin: 0,
    });
    s.addText(c.desc, {
      x: 0.7 + 2 * (colW + 0.15), y: y + 0.28, w: colW - 0.4, h: 0.32,
      fontSize: 9, fontFace: FONT_BODY, color: C.slate, margin: 0,
    });
  });

  // ── 底栏：跨场景 + 测试 ─────────────────────────────────────────────────
  const botY = 4.18;
  card(s, { x: 0.5, y: botY, w: 9.0, h: 0.62, bg: "F1F5F9" });
  s.addText("跨 vendor demo / 测试 / 第三方 adopter", {
    x: 0.7, y: botY + 0.05, w: 4.5, h: 0.22,
    fontSize: 9, fontFace: FONT_HEAD, bold: true, color: C.muted, margin: 0,
  });
  s.addText([
    { text: "atd-mock-weather-server", options: { bold: true, color: C.purple, fontFace: FONT_MONO } },
    { text: "  跨 vendor 组合 demo  ·  ", options: { color: C.slate } },
    { text: "atd-conformance",          options: { bold: true, color: C.purple, fontFace: FONT_MONO } },
    { text: "  35 fixtures  ·  ",        options: { color: C.slate } },
    { text: "healthkit_cli",            options: { bold: true, color: C.green, fontFace: FONT_MONO } },
    { text: "  外部 adopter (使用 atd-server + atd-runtime)", options: { color: C.slate } },
  ], {
    x: 0.7, y: botY + 0.27, w: 8.7, h: 0.32,
    fontSize: 9.5, fontFace: FONT_BODY, margin: 0,
  });

  addFooter(s, 10, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 11 — Deployment topology: healthkit ATD + Hermes
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.runtime);
  addTitle(s, "实测部署: healthkit ATD + Hermes", {
    subtitle: "两进程 · 一 Unix socket · 一 OAuth · 一 audit log",
  });

  // ── 顶部：进程拓扑 ────────────────────────────────────────────────────
  const topY = 1.0, topH = 1.55;

  // Hermes process box
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: topY + 0.3, w: 1.7, h: 0.95,
    fill: { color: C.deepBlue }, line: { color: C.deepBlue, width: 0 },
  });
  s.addText("Hermes Agent", {
    x: 0.5, y: topY + 0.32, w: 1.7, h: 0.32,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
    align: "center", margin: 0,
  });
  s.addText("DeepSeek-chat\nLLM driver", {
    x: 0.5, y: topY + 0.62, w: 1.7, h: 0.55,
    fontSize: 9, fontFace: FONT_BODY, color: "CADCFC",
    align: "center", margin: 0,
  });

  // arrow 1: stdio (Hermes ↔ bridge)
  s.addShape(pres.shapes.RIGHT_ARROW, {
    x: 2.25, y: topY + 0.65, w: 0.55, h: 0.3,
    fill: { color: C.muted }, line: { color: C.muted, width: 0 },
  });
  s.addText("stdio\nMCP", {
    x: 2.25, y: topY + 0.32, w: 0.55, h: 0.3,
    fontSize: 7.5, fontFace: FONT_MONO, color: C.muted,
    align: "center", margin: 0,
  });

  // bridge process box
  s.addShape(pres.shapes.RECTANGLE, {
    x: 2.85, y: topY + 0.3, w: 1.95, h: 0.95,
    fill: { color: C.teal }, line: { color: C.teal, width: 0 },
  });
  s.addText("atd-mcp-bridge", {
    x: 2.85, y: topY + 0.32, w: 1.95, h: 0.32,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
    align: "center", margin: 0,
  });
  s.addText("MCP↔wire 翻译器\nHermes 子进程 (spawn)", {
    x: 2.85, y: topY + 0.62, w: 1.95, h: 0.55,
    fontSize: 9, fontFace: FONT_BODY, color: "CADCFC",
    align: "center", margin: 0,
  });

  // arrow 2: Unix socket
  s.addShape(pres.shapes.RIGHT_ARROW, {
    x: 4.85, y: topY + 0.65, w: 0.55, h: 0.3,
    fill: { color: C.muted }, line: { color: C.muted, width: 0 },
  });
  s.addText("Unix socket\n/tmp/hk.sock", {
    x: 4.85, y: topY + 0.28, w: 1.05, h: 0.35,
    fontSize: 7.5, fontFace: FONT_MONO, color: C.muted,
    align: "center", margin: 0,
  });

  // healthkit serve box
  s.addShape(pres.shapes.RECTANGLE, {
    x: 5.45, y: topY + 0.3, w: 2.0, h: 0.95,
    fill: { color: C.green }, line: { color: C.green, width: 0 },
  });
  s.addText("healthkit serve", {
    x: 5.45, y: topY + 0.32, w: 2.0, h: 0.32,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
    align: "center", margin: 0,
  });
  s.addText("ATD server (vendor)\n27 tools 注册", {
    x: 5.45, y: topY + 0.62, w: 2.0, h: 0.55,
    fontSize: 9, fontFace: FONT_BODY, color: "E0F2EE",
    align: "center", margin: 0,
  });

  // arrow 3: HTTPS
  s.addShape(pres.shapes.RIGHT_ARROW, {
    x: 7.5, y: topY + 0.65, w: 0.55, h: 0.3,
    fill: { color: C.muted }, line: { color: C.muted, width: 0 },
  });
  s.addText("HTTPS\nOAuth", {
    x: 7.5, y: topY + 0.32, w: 0.55, h: 0.3,
    fontSize: 7.5, fontFace: FONT_MONO, color: C.muted,
    align: "center", margin: 0,
  });

  // HMS box
  s.addShape(pres.shapes.RECTANGLE, {
    x: 8.1, y: topY + 0.3, w: 1.4, h: 0.95,
    fill: { color: C.slate }, line: { color: C.slate, width: 0 },
  });
  s.addText("HMS REST", {
    x: 8.1, y: topY + 0.32, w: 1.4, h: 0.32,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
    align: "center", margin: 0,
  });
  s.addText("Huawei Cloud\n(external)", {
    x: 8.1, y: topY + 0.62, w: 1.4, h: 0.55,
    fontSize: 9, fontFace: FONT_BODY, color: "E2E8F0",
    align: "center", margin: 0,
  });

  // ── 中部：4 张外挂 (token / audit / capability / register) ──────────────
  const midY = 2.7;
  const ext = [
    {
      x: 0.5, color: C.purple, title: "OAuth token",
      lines: [
        "~/.config/healthkit/", "  token.json",
        "(fallback)",
        "+ /tmp/hk-tokens/",
        "  agent-A.json",
        "  agent-B.json",
        "(multi-tenant)",
      ],
    },
    {
      x: 2.85, color: C.amber, title: "Capability grant",
      lines: [
        "healthkit serve \\",
        "  --grant-capability \\",
        "    healthkit:read \\",
        "  --grant-capability \\",
        "    healthkit:write",
        "",
        "(server allow-list)",
      ],
    },
    {
      x: 5.2, color: C.deepBlue, title: "Audit log (JSONL)",
      lines: [
        "/tmp/hk-audit.jsonl",
        "{ ts, call_id,",
        "  tool_id, caller_id,",
        "  granted_capabilities,",
        "  outcome, duration_ms,",
        "  secrets_resolved }",
        "→ tail -f | jq",
      ],
    },
    {
      x: 7.55, color: C.teal, title: "Hermes 注册",
      lines: [
        "hermes mcp add \\",
        "  healthkit \\",
        "  --command \\",
        "    atd-mcp-bridge \\",
        "  --env \\",
        "    ATD_SOCK=...sock \\",
        "    ATD_REQUEST_CAPS=...",
      ],
    },
  ];
  ext.forEach((c) => {
    card(s, { x: c.x, y: midY, w: 2.25, h: 2.05, railColor: c.color });
    s.addText(c.title, {
      x: c.x + 0.12, y: midY + 0.06, w: 2.0, h: 0.26,
      fontSize: 10, fontFace: FONT_HEAD, bold: true, color: c.color, margin: 0,
    });
    c.lines.forEach((l, i) => {
      s.addText(l, {
        x: c.x + 0.12, y: midY + 0.34 + i * 0.22, w: 2.05, h: 0.2,
        fontSize: 8.5, fontFace: FONT_MONO, color: C.midnight, margin: 0,
      });
    });
  });

  addFooter(s, 11, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 12 — Case 5 完整交互时序 (sequence diagram)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.protocol);
  addTitle(s, "完整交互时序 — Case v1.4.0 心率分析", {
    subtitle: "从用户 prompt 到 audit log: ATD 在每一步做了什么",
  });

  // 4 lane headers
  const lanes = [
    { x: 0.5, w: 2.0, label: "User / LLM",       color: C.deepBlue },
    { x: 2.55, w: 2.0, label: "atd-mcp-bridge",  color: C.teal },
    { x: 4.6, w: 2.5, label: "healthkit serve\n(atd-runtime)",  color: C.amber },
    { x: 7.15, w: 2.35, label: "外部 / 副效果", color: C.slate },
  ];
  lanes.forEach((L) => {
    s.addShape(pres.shapes.RECTANGLE, {
      x: L.x, y: 1.0, w: L.w, h: 0.4,
      fill: { color: L.color }, line: { color: L.color, width: 0 },
    });
    s.addText(L.label, {
      x: L.x, y: 1.0, w: L.w, h: 0.4,
      fontSize: 10, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
      align: "center", valign: "middle", margin: 0,
    });
  });

  // sequence rows
  const rows = [
    { n: "1", lane: 0, text: '用户 prompt: "从医生角度…最近两个月心率"', color: C.midnight },
    { n: "2", lane: 0, text: "DeepSeek 决定: 调 mcp_healthkit_*.heartrate {days:60}", color: C.deepBlue },
    { n: "3", lane: 1, text: "Hermes → bridge: MCP tools/call (stdio)", color: C.teal },
    { n: "4", lane: 1, text: "bridge → server: RunTool 帧 (length-prefixed JSON)", color: C.teal },
    { n: "5", lane: 2, text: "Capability gate: granted ⊇ healthkit:read ✓", color: C.green },
    { n: "6", lane: 2, text: "Rate limit: try_acquire_owned() ✓", color: C.green },
    { n: "7", lane: 2, text: "TokenBroker.resolve(\"atd-mcp-bridge\") → SecretBundle", color: C.purple },
    { n: "8", lane: 3, text: "→ HMS REST GET /heartrate (HTTPS, OAuth bearer)", color: C.slate },
    { n: "9", lane: 2, text: "Tool::call() → 解析 HMS JSON → ToolResult", color: C.amber },
    { n: "10", lane: 3, text: "→ /tmp/hk-audit.jsonl: append 1 row", color: C.deepBlue },
    { n: "11", lane: 1, text: "server → bridge: ToolResultResponse (~24 KB)", color: C.teal },
    { n: "12", lane: 0, text: "bridge → Hermes: MCP tool response (1169 ms 总耗时)", color: C.deepBlue },
    { n: "13", lane: 0, text: "(LLM 同样调 .restinghr; 然后产出医生报告)", color: C.muted },
  ];

  const rowY = 1.5, rowH = 0.24;
  rows.forEach((r, i) => {
    const y = rowY + i * rowH;
    if (i % 2 === 0) {
      s.addShape(pres.shapes.RECTANGLE, {
        x: 0.5, y, w: 9.0, h: rowH,
        fill: { color: "F8FAFC" }, line: { color: "F8FAFC", width: 0 },
      });
    }
    // step number
    s.addText(r.n, {
      x: 0.5, y, w: 0.25, h: rowH,
      fontSize: 8.5, fontFace: FONT_HEAD, bold: true, color: C.muted,
      align: "right", valign: "middle", margin: 0,
    });
    // active lane indicator
    const L = lanes[r.lane];
    s.addShape(pres.shapes.RECTANGLE, {
      x: L.x, y: y + 0.04, w: 0.06, h: rowH - 0.08,
      fill: { color: r.color }, line: { color: r.color, width: 0 },
    });
    // text spans the active lane and beyond
    s.addText(r.text, {
      x: L.x + 0.12, y, w: 9.5 - (L.x + 0.12), h: rowH,
      fontSize: 9, fontFace: FONT_MONO, color: r.color,
      valign: "middle", margin: 0,
    });
  });

  // Audit log call-out at bottom
  const auY = 4.65;
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: auY, w: 9.0, h: 0.32,
    fill: { color: C.midnight }, line: { color: C.midnight, width: 0 },
  });
  s.addText('audit row (实测): {"ts":"…","tool_id":"huawei:hms.healthkit.heartrate","caller_id":"atd-mcp-bridge","outcome":"success","duration_ms":1169}', {
    x: 0.65, y: auY, w: 8.7, h: 0.32,
    fontSize: 8, fontFace: FONT_MONO, color: "CADCFC",
    valign: "middle", margin: 0,
  });

  addFooter(s, 12, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 13 — Capability + Rate limit + Audit
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.security);
  addTitle(s, "运行时门禁: Capability + Rate Limit + Audit", {
    subtitle: "三件 raw MCP 没规范、raw CLI 做不到的 — server 强制, 工具不用自己写",
  });

  const cards3 = [
    {
      x: 0.5, color: C.red, title: "Capability Gate (SP-12)",
      lines: [
        "1. Hello { requested_capabilities }",
        "2. server allow-list ∩ requested",
        "   → granted",
        "3. tool.required ⊂ granted ?",
        "   no → ERR_CAPABILITY_DENIED",
        "        (1001, retryable: false)",
        "   yes → Tool::call(...)",
      ],
    },
    {
      x: 3.5, color: C.amber, title: "Rate Limit (SP-op-v1)",
      lines: [
        "1. tool.resources.max_concurrent",
        "2. try_acquire_owned() per call",
        "3. NoPermits →",
        "   ERR_RATE_LIMITED",
        "   (1002, retryable: true)",
        "4. permit drops auto",
        "   → fail-fast, 不排队",
      ],
    },
    {
      x: 6.5, color: C.deepBlue, title: "Audit Log (JSON Lines)",
      lines: [
        '{ ts, call_id, tool_id,',
        '  caller_id,',
        '  granted_capabilities,',
        '  duration_ms, outcome,',
        '  tier, dry_run,',
        '  schema_version,',
        '  secrets_resolved }',
      ],
    },
  ];
  cards3.forEach((c) => {
    card(s, { x: c.x, y: 1.0, w: 3.0, h: 3.85, railColor: c.color });
    s.addText(c.title, {
      x: c.x + 0.15, y: 1.1, w: 2.7, h: 0.32,
      fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
    });
    c.lines.forEach((l, i) => {
      s.addText(l, {
        x: c.x + 0.15, y: 1.5 + i * 0.4, w: 2.85, h: 0.35,
        fontSize: 9.5,
        fontFace: l.includes("→") || l.startsWith("{") || l.includes("ERR_") || l.includes("→") ? FONT_MONO : FONT_BODY,
        color: l.includes("ERR_") ? c.color : C.midnight,
        margin: 0,
      });
    });
  });

  addFooter(s, 13, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 14 — TokenBroker / multi-tenant (with audit-log proof)
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.security);
  addTitle(s, "TokenBroker 多租户路由", {
    subtitle: "v0.3.0 起 — 一个 server, N caller, N OAuth (raw CLI 做不到)",
  });

  // trait + impl box
  card(s, { x: 0.5, y: 1.0, w: 9.0, h: 1.55, railColor: C.purple });
  s.addText("Extension point in atd-runtime", {
    x: 0.7, y: 1.08, w: 8.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  s.addText(
    "trait TokenBroker {\n" +
    "    fn resolve(caller_id: Option<&str>) -> ResolveFuture<'_>;\n" +
    "}\n" +
    "// → Ok(Some(SecretBundle)) | Ok(None) | Err(BrokerError)\n" +
    "// SecretBundle = HashMap<String, RedactedString>  (Debug → \"<redacted>\", 永不泄漏值)",
    {
      x: 0.7, y: 1.4, w: 8.6, h: 1.1,
      fontSize: 10, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });

  // Live audit-log proof
  card(s, { x: 0.5, y: 2.7, w: 9.0, h: 2.2, railColor: C.green });
  s.addText("v1.4.0 实测: /tmp/hk-audit.jsonl  (3 caller_id 跑同一个工具)", {
    x: 0.7, y: 2.78, w: 8.6, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });

  // Header
  const auditCols = [
    { x: 0.7, w: 1.8, label: "caller_id" },
    { x: 2.6, w: 1.6, label: "secrets_resolved" },
    { x: 4.3, w: 1.4, label: "outcome" },
    { x: 5.8, w: 1.4, label: "duration" },
    { x: 7.3, w: 2.1, label: "解析" },
  ];
  auditCols.forEach((c) => {
    s.addText(c.label, {
      x: c.x, y: 3.15, w: c.w, h: 0.28,
      fontSize: 9, fontFace: FONT_HEAD, bold: true, color: C.muted, margin: 0,
    });
  });

  const auditRows = [
    { caller: "agent-A", resolved: "true",  outcome: "success", dur: "1169 ms", note: "broker → agent-A.json" },
    { caller: "agent-B", resolved: "true",  outcome: "success", dur: "438 ms",  note: "broker → agent-B.json" },
    { caller: "ghost",   resolved: "false", outcome: "success", dur: "—",       note: "未注册 → env/saved fallback" },
  ];
  auditRows.forEach((r, i) => {
    const y = 3.5 + i * 0.4;
    s.addText(r.caller, {
      x: 0.7, y, w: 1.8, h: 0.32,
      fontSize: 10, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });
    s.addText(r.resolved, {
      x: 2.6, y, w: 1.6, h: 0.32,
      fontSize: 10, fontFace: FONT_MONO, bold: true,
      color: r.resolved === "true" ? C.green : C.amber, margin: 0,
    });
    s.addText(r.outcome, {
      x: 4.3, y, w: 1.4, h: 0.32,
      fontSize: 10, fontFace: FONT_MONO, color: C.green, margin: 0,
    });
    s.addText(r.dur, {
      x: 5.8, y, w: 1.4, h: 0.32,
      fontSize: 10, fontFace: FONT_MONO, color: C.slate, margin: 0,
    });
    s.addText(r.note, {
      x: 7.3, y, w: 2.1, h: 0.32,
      fontSize: 9, fontFace: FONT_BODY, color: C.muted, margin: 0,
    });
  });

  addFooter(s, 14, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 15 — Cross-vendor + Skills convention
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.tools);
  addTitle(s, "跨 Vendor 组合 + Skills 公约", {
    subtitle: "一个 agent session, N vendor server (CLI 做不到); SKILL.md 一键同步",
  });

  // 左: 跨vendor 拓扑
  card(s, { x: 0.5, y: 1.0, w: 4.4, h: 3.85, bg: C.card });
  s.addText("Cross-vendor (SP-cross-vendor-mock-demo)", {
    x: 0.7, y: 1.08, w: 4.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });

  // Agent box
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.7, y: 1.7, w: 1.0, h: 0.6,
    fill: { color: C.deepBlue }, line: { color: C.deepBlue, width: 0 },
  });
  s.addText("Hermes", {
    x: 0.7, y: 1.7, w: 1.0, h: 0.6,
    fontSize: 10, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
    align: "center", valign: "middle", margin: 0,
  });

  // Bridge boxes
  ["bridge", "bridge"].forEach((b, i) => {
    const y = 1.55 + i * 0.85;
    s.addShape(pres.shapes.RECTANGLE, {
      x: 1.95, y, w: 0.95, h: 0.5,
      fill: { color: C.teal }, line: { color: C.teal, width: 0 },
    });
    s.addText(b, {
      x: 1.95, y, w: 0.95, h: 0.5,
      fontSize: 8, fontFace: FONT_MONO, color: "FFFFFF",
      align: "center", valign: "middle", margin: 0,
    });
  });

  // server boxes
  const ssr = [
    { name: "healthkit", sub: "27 tools", color: C.green, y: 1.55 },
    { name: "weather-mock", sub: "3 tools", color: C.amber, y: 2.4 },
  ];
  ssr.forEach((srv) => {
    s.addShape(pres.shapes.RECTANGLE, {
      x: 3.2, y: srv.y, w: 1.6, h: 0.5,
      fill: { color: srv.color }, line: { color: srv.color, width: 0 },
    });
    s.addText(srv.name + "\n" + srv.sub, {
      x: 3.2, y: srv.y, w: 1.6, h: 0.5,
      fontSize: 8, fontFace: FONT_HEAD, bold: true, color: "FFFFFF",
      align: "center", valign: "middle", margin: 0,
    });
  });

  s.addText(
    "Agent discover() 看到合并 catalog\n" +
    "huawei:hms.healthkit.* + mock:weather.*\n" +
    "工具按 description 匹配, 不用知道哪个 socket",
    {
      x: 0.7, y: 3.4, w: 4.0, h: 1.4,
      fontSize: 9, fontFace: FONT_BODY, italic: true, color: C.muted,
      margin: 0,
    });

  // 右: Skills convention
  card(s, { x: 5.1, y: 1.0, w: 4.4, h: 3.85, railColor: C.purple });
  s.addText("Skills Meta-tool Convention", {
    x: 5.3, y: 1.08, w: 4.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const sk = [
    "<publisher>:<service>.skills.list",
    "  →  Vec<{name, description}>",
    "",
    "<publisher>:<service>.skills.get",
    "  args: { name }",
    "  →  { name, content_md }",
    "",
    "+ atd skills sync",
    "    --target { hermes | claude-code | stdout }",
    "",
    "★ healthkit_cli v1.3.0 实测",
    "   26 SKILL.md 同步, diff = 0",
    "   ATD 协议改动: 0 (纯命名公约)",
  ];
  sk.forEach((l, i) => {
    s.addText(l, {
      x: 5.3, y: 1.45 + i * 0.26, w: 4.0, h: 0.24,
      fontSize: 10,
      fontFace: l.startsWith("★") || l.startsWith(" ") && l.includes("ATD") || l.includes("diff = 0") ? FONT_BODY : FONT_MONO,
      italic: l.startsWith("★") || l.startsWith("   "),
      color: l.startsWith("★") ? C.purple : (l.includes("→") ? C.deepBlue : C.midnight),
      bold: l.startsWith("★"),
      margin: 0,
    });
  });

  addFooter(s, 15, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 16 — vs raw alternatives
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.neutral);
  addTitle(s, "三方比较: ATD vs raw CLI vs raw MCP", {
    subtitle: "ATD 在协议层 ship 了 raw 选项缺的能力 — 不是 marketing, 是协议 surface",
  });

  const cols = [
    { x: 0.5, w: 2.4, label: "维度" },
    { x: 2.95, w: 2.05, label: "Raw CLI", color: C.amber },
    { x: 5.05, w: 2.05, label: "Raw MCP", color: C.deepBlue },
    { x: 7.15, w: 2.35, label: "ATD ★", color: C.green },
  ];
  s.addShape(pres.shapes.RECTANGLE, {
    x: 0.5, y: 1.05, w: 9.0, h: 0.4,
    fill: { color: C.midnight }, line: { color: C.midnight, width: 0 },
  });
  cols.forEach((c) => {
    s.addText(c.label, {
      x: c.x, y: 1.05, w: c.w, h: 0.4,
      fontSize: 11, fontFace: FONT_HEAD, bold: true, color: c.color || "FFFFFF",
      align: "center", valign: "middle", margin: 0,
    });
  });

  const rows = [
    ["Capability gate",       "无", "client 自己", "server 强制 ✓"],
    ["Rate limit",            "无", "无",          "per-tool semaphore ✓"],
    ["Audit log",             "shell history", "无规范", "JSON Lines ✓"],
    ["Multi-tenant token",    "N 进程 / N token", "stdio 单租户", "TokenBroker ✓"],
    ["Tool visibility",       "无", "二元 hidden", "5 档 (含 Hidden) ✓"],
    ["Safety levels",         "无", "无", "Read..Destructive ✓"],
    ["跨 vendor 组合",        "自己写 mux", "需自己 mux", "桥接多 socket ✓"],
    ["LLM matching",          "--help 文本", "tool desc only", "desc + intent_examples ✓"],
    ["Case study v1.4 实证",  "8 calls / 3 错试", "—", "2 calls / 0 错试 ★"],
  ];
  const rowY = 1.5, rowH = 0.34;
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
        fontSize: 9.5, fontFace: FONT_BODY,
        color: c.color, bold: c.bold,
        valign: "middle", margin: 0,
      });
    });
  });

  s.addText(
    "★ 通过 atd-mcp-bridge 兼容现有 MCP 客户端 — Hermes / Claude Code / Cursor 不改一行代码",
    {
      x: 0.5, y: 4.65, w: 9.0, h: 0.3,
      fontSize: 10, fontFace: FONT_BODY, italic: true, color: C.green, align: "center", margin: 0,
    });

  addFooter(s, 16, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 17 — 5-layer architecture
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.neutral);
  addTitle(s, "5-Layer 架构", {
    subtitle: "自上而下: skills → agent → SDK → wire → runtime → tools → service",
  });

  const layers = [
    { name: "Skills Layer (adjacent)",     sub: "SKILL.md, atd skills sync", color: C.purple },
    { name: "Agent Framework",             sub: "Hermes / LangChain / Claude / OpenClaw", color: C.deepBlue },
    { name: "ATD SDK + CLI + MCP Bridge",  sub: "atd-sdk, atd-cli, atd-mcp-bridge", color: C.teal },
    { name: "ATD Wire Protocol",           sub: "5 messages, length-prefixed JSON, Unix socket", color: C.midnight },
    { name: "ATD Server Runtime",          sub: "atd-runtime + atd-server: capability / rate limit / audit / TokenBroker", color: C.amber },
    { name: "Vendor Tools",                sub: "healthkit_cli, atd-mock-weather-server, ...", color: C.green },
    { name: "Underlying Service",          sub: "Huawei HMS REST, OpenWeatherMap, ...", color: C.slate },
  ];
  const rowY = 1.05, rowH = 0.55;
  layers.forEach((L, i) => {
    const y = rowY + i * rowH;
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 9.0, h: rowH - 0.08,
      fill: { color: C.card }, line: { color: C.faint, width: 0.75 },
    });
    s.addShape(pres.shapes.RECTANGLE, {
      x: 0.5, y, w: 0.18, h: rowH - 0.08,
      fill: { color: L.color }, line: { color: L.color, width: 0 },
    });
    s.addText(L.name, {
      x: 0.85, y, w: 4.0, h: rowH - 0.08,
      fontSize: 12, fontFace: FONT_HEAD, bold: true, color: C.midnight,
      valign: "middle", margin: 0,
    });
    s.addText(L.sub, {
      x: 4.95, y, w: 4.55, h: rowH - 0.08,
      fontSize: 10, fontFace: FONT_BODY, color: C.slate,
      valign: "middle", margin: 0,
    });
  });

  addFooter(s, 17, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 18 — Workspace + 5 min start
// ════════════════════════════════════════════════════════════════════════
{
  const s = contentSlide(LAYER.tools);
  addTitle(s, "Workspace + 上手 5 分钟", {
    subtitle: "13 crates · 378 tests · Apache-2.0 · 5 行代码写自己的 server",
  });

  // 左: crates
  card(s, { x: 0.5, y: 1.0, w: 4.4, h: 3.85, railColor: C.green });
  s.addText("Crates", {
    x: 0.7, y: 1.08, w: 4.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
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
    "atd-conformance   35 fixture",
    "atd-mock-weather-server",
  ];
  crates.forEach((c, i) => {
    s.addText(c, {
      x: 0.7, y: 1.45 + i * 0.32, w: 4.0, h: 0.3,
      fontSize: 9.5, fontFace: FONT_MONO, color: C.midnight, margin: 0,
    });
  });

  // 右: 5 分钟跑通
  card(s, { x: 5.1, y: 1.0, w: 4.4, h: 3.85, railColor: C.deepBlue });
  s.addText("5 分钟跑通", {
    x: 5.3, y: 1.08, w: 4.0, h: 0.3,
    fontSize: 11, fontFace: FONT_HEAD, bold: true, color: C.midnight, margin: 0,
  });
  const cmds = [
    "$ cargo build --release \\",
    "    -p atd-ref-server \\",
    "    -p atd-cli \\",
    "    -p atd-mcp-bridge",
    "",
    "$ ./target/release/atd-ref-server &",
    "",
    "$ atd list",
    "$ atd schema ref:fs.read",
    "$ atd call ref:echo.say \\",
    "    --args '{\"text\":\"hi\"}'",
    "",
    "# 接 Hermes:",
    "$ hermes mcp add atd-ref \\",
    "    --command ./atd-mcp-bridge \\",
    "    --env ATD_SOCK=/tmp/atd.sock",
  ];
  cmds.forEach((l, i) => {
    s.addText(l, {
      x: 5.3, y: 1.45 + i * 0.22, w: 4.0, h: 0.2,
      fontSize: 9, fontFace: FONT_MONO,
      color: l.startsWith("$") ? C.green : (l.startsWith("#") ? C.muted : C.slate),
      italic: l.startsWith("#"),
      margin: 0,
    });
  });

  addFooter(s, 18, TOTAL);
}

// ════════════════════════════════════════════════════════════════════════
// Slide 19 — Closing
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
    { text: "实证: ",                                                  options: { color: "8FA1B8" } },
    { text: "5 个 case study", options: { bold: true, color: "FFFFFF" } },
    { text: ", 全部真跑过 Hermes + DeepSeek-chat:\n", options: { color: "CADCFC" } },
    { text: "  v1.2.0 q1 5K体能 · q2 周对比 · q3 步数挑战 · q4 健康日报", options: { color: C.teal } },
    { text: "  ·  v1.4.0 医生视角心率分析 (2 ATD vs 8 CLI)", options: { bold: true, color: "FFFFFF" } },
  ], {
    x: 0.6, y: 3.85, w: 8.8, h: 0.95,
    fontSize: 12, fontFace: FONT_BODY, margin: 0,
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
