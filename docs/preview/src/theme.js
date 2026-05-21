// theme.js — shared design system for the ATD 技术预览 deck series.
// Dark premium navy + teal. LAYOUT_WIDE (13.33 x 7.5).
const pptxgen = require("pptxgenjs");

const C = {
  bg:     "0B1322", bgAlt: "0F1C35", card: "17233D", cardHi: "1F3056",
  line:   "2C3D60", ink: "ECF1F9", mute: "98A8C4", faint: "6A7C9C",
  teal:   "2DD4BF", tealDk: "115E59", blue: "5B9DF0",
  amber:  "F2B44C", coral: "F47B72", green: "46D39A",
};
const F = { serif: "Noto Serif CJK SC", sans: "Noto Sans CJK SC", mono: "Noto Sans Mono CJK SC" };

const W = 13.33, H = 7.5, M = 0.62;
const CW = W - 2 * M;        // content width 12.09
const CB = 6.55;             // content bottom (footer line at 7.02 → 0.47 clearance)
const shadow = () => ({ type: "outer", color: "000000", blur: 9, offset: 3, angle: 135, opacity: 0.28 });

function newDeck({ part, partLabel, partTitle }) {
  const pres = new pptxgen();
  pres.layout = "LAYOUT_WIDE";
  pres.author = "ATD Protocol Contributors";
  pres.title = `ATD 技术预览 · 第${partLabel}部分 · ${partTitle}`;
  return { pres, part, partLabel, partTitle, page: 0 };
}
function save(d, file) { return d.pres.writeFile({ fileName: file }); }

function pageNum(d, slide, dark) {
  slide.addText(String(d.page).padStart(2, "0"),
    { x: W - M - 1.0, y: 7.06, w: 1.0, h: 0.34, fontFace: F.mono, fontSize: 9.5,
      color: C.teal, align: "right", margin: 0 });
}
function footer(d, slide) {
  slide.addShape("line", { x: M, y: 7.02, w: CW, h: 0, line: { color: C.line, width: 0.75 } });
  slide.addText(`ATD 技术预览   ·   第${d.partLabel}部分 · ${d.partTitle}`,
    { x: M, y: 7.06, w: 9, h: 0.34, fontFace: F.sans, fontSize: 9, color: C.faint, margin: 0 });
  pageNum(d, slide);
}
function header(d, slide, title, kicker) {
  slide.addShape("rect", { x: M, y: 0.56, w: 0.17, h: 0.17, fill: { color: C.teal } });
  slide.addText(kicker || `第${d.partLabel}部分 · ${d.partTitle}`,
    { x: M + 0.30, y: 0.5, w: CW - 0.3, h: 0.32, fontFace: F.sans, fontSize: 11,
      color: C.teal, bold: true, charSpacing: 2, margin: 0 });
  slide.addText(title, { x: M, y: 0.84, w: CW, h: 0.82, fontFace: F.serif, fontSize: 26,
    bold: true, color: C.ink, margin: 0 });
}
function base(d) {
  d.page += 1;
  const slide = d.pres.addSlide();
  slide.background = { color: C.bg };
  return slide;
}
// intro line under the header; returns the y where content should start.
function introLine(s, text) {
  if (!text) return 1.86;
  s.addText(text, { x: M, y: 1.82, w: CW, h: 0.5, fontFace: F.sans, fontSize: 12.5,
    color: C.mute, italic: true, margin: 0, lineSpacingMultiple: 1.2 });
  return 2.44;
}

// ---- COVER -----------------------------------------------------------------
function cover(d, { title, subtitle, tagline }) {
  d.page += 1;
  const s = d.pres.addSlide();
  s.background = { color: C.bgAlt };
  s.addText(String(d.part).padStart(2, "0"),
    { x: 7.4, y: 0.2, w: 6.2, h: 7.0, fontFace: F.serif, fontSize: 380,
      color: C.cardHi, bold: true, align: "right", valign: "middle", margin: 0 });
  s.addShape("rect", { x: M, y: 2.18, w: 0.20, h: 3.05, fill: { color: C.teal } });
  s.addText("ATD 技术预览", { x: M + 0.42, y: 1.35, w: 8, h: 0.4, fontFace: F.sans,
    fontSize: 13, color: C.mute, charSpacing: 3, margin: 0 });
  s.addText(`第 ${d.partLabel} 部分`, { x: M + 0.42, y: 1.78, w: 8, h: 0.5, fontFace: F.sans,
    fontSize: 15, color: C.teal, bold: true, margin: 0 });
  s.addText(title, { x: M + 0.42, y: 2.3, w: 7.6, h: 2.2, fontFace: F.serif, fontSize: 44,
    bold: true, color: C.ink, lineSpacingMultiple: 1.08, margin: 0 });
  s.addText(subtitle, { x: M + 0.42, y: 4.5, w: 7.3, h: 1.0, fontFace: F.sans, fontSize: 15,
    color: C.mute, lineSpacingMultiple: 1.32, margin: 0 });
  if (tagline) {
    s.addShape("line", { x: M + 0.42, y: 5.66, w: 6.6, h: 0, line: { color: C.line, width: 1 } });
    s.addText(tagline, { x: M + 0.42, y: 5.8, w: 7.4, h: 0.8, fontFace: F.serif, fontSize: 14,
      italic: true, color: C.teal, lineSpacingMultiple: 1.3, margin: 0 });
  }
  s.addText("Agent Tool Dispatch  ·  中立 · 跨厂商 · 开源 (Apache-2.0)",
    { x: M + 0.42, y: 6.66, w: 9, h: 0.34, fontFace: F.mono, fontSize: 9.5, color: C.faint, margin: 0 });
}

// ---- AGENDA ----------------------------------------------------------------
function agenda(d, { title, items }) {
  const s = base(d);
  header(d, s, title || "本部分内容", "AGENDA · 本部分导览");
  const n = items.length;
  const rowH = Math.min(0.92, 4.55 / n);
  const top = 1.95;
  items.forEach((it, i) => {
    const y = top + i * rowH;
    s.addText(String(i + 1).padStart(2, "0"),
      { x: M, y, w: 1.0, h: rowH - 0.14, fontFace: F.serif, fontSize: 24, bold: true,
        color: C.teal, valign: "middle", margin: 0 });
    s.addText(it.h, { x: M + 1.15, y, w: 4.6, h: rowH - 0.14, fontFace: F.serif,
      fontSize: 16.5, bold: true, color: C.ink, valign: "middle", margin: 0 });
    s.addText(it.b || "", { x: M + 5.95, y, w: CW - 5.95, h: rowH - 0.14, fontFace: F.sans,
      fontSize: 11.5, color: C.mute, valign: "middle", margin: 0, lineSpacingMultiple: 1.15 });
    if (i < n - 1)
      s.addShape("line", { x: M, y: y + rowH - 0.07, w: CW, h: 0, line: { color: C.line, width: 0.75 } });
  });
  footer(d, s);
}

// ---- SECTION ---------------------------------------------------------------
function section(d, { no, kicker, title, sub }) {
  d.page += 1;
  const s = d.pres.addSlide();
  s.background = { color: C.bgAlt };
  s.addShape("rect", { x: 0, y: 0, w: 0.22, h: H, fill: { color: C.teal } });
  if (no) s.addText(no, { x: M + 0.2, y: 1.5, w: 3, h: 1.5, fontFace: F.serif, fontSize: 64,
    bold: true, color: C.cardHi, margin: 0 });
  s.addText(kicker || "", { x: M + 0.22, y: 3.05, w: 10, h: 0.4, fontFace: F.sans, fontSize: 12,
    color: C.teal, bold: true, charSpacing: 3, margin: 0 });
  s.addText(title, { x: M + 0.22, y: 3.45, w: 11.4, h: 1.5, fontFace: F.serif, fontSize: 38,
    bold: true, color: C.ink, lineSpacingMultiple: 1.1, margin: 0 });
  if (sub) s.addText(sub, { x: M + 0.22, y: 4.95, w: 10.6, h: 1.2, fontFace: F.sans, fontSize: 14,
    color: C.mute, lineSpacingMultiple: 1.35, margin: 0 });
}

// ---- BULLETS ---------------------------------------------------------------
function bullets(d, { title, kicker, intro, items }) {
  const s = base(d);
  header(d, s, title, kicker);
  const y = introLine(s, intro);
  const n = items.length, gap = 0.16;
  const rowH = Math.min(1.18, (CB - y - (n - 1) * gap) / n);
  const blockH = n * rowH + (n - 1) * gap;
  const y0 = y + ((CB - y) - blockH) / 2;
  items.forEach((it, i) => {
    const ry = y0 + i * (rowH + gap);
    s.addShape("rect", { x: M, y: ry, w: CW, h: rowH, fill: { color: C.card } });
    s.addShape("rect", { x: M, y: ry, w: 0.07, h: rowH, fill: { color: it.c || C.teal } });
    s.addText(it.h, { x: M + 0.32, y: ry, w: 4.3, h: rowH, fontFace: F.serif,
      fontSize: 14.5, bold: true, color: C.ink, valign: "middle", margin: 0, lineSpacingMultiple: 1.1 });
    s.addText(it.b, { x: M + 4.8, y: ry, w: CW - 5.05, h: rowH, fontFace: F.sans,
      fontSize: 11.5, color: C.mute, valign: "middle", margin: 0, lineSpacingMultiple: 1.24 });
  });
  footer(d, s);
}

// ---- CARDS -----------------------------------------------------------------
function cards(d, { title, kicker, intro, cols, items }) {
  const s = base(d);
  header(d, s, title, kicker);
  const top = introLine(s, intro);
  const rows = Math.ceil(items.length / cols);
  const gx = 0.26, gy = 0.26;
  const cwd = (CW - (cols - 1) * gx) / cols;
  const maxCht = rows === 1 ? 3.0 : 3.5;
  let cht = (CB - top - (rows - 1) * gy) / rows;
  cht = Math.min(cht, maxCht);
  const blockH = rows * cht + (rows - 1) * gy;
  const top0 = top + ((CB - top) - blockH) / 2;
  items.forEach((it, i) => {
    const r = Math.floor(i / cols), col = i % cols;
    const x = M + col * (cwd + gx), y = top0 + r * (cht + gy);
    s.addShape("rect", { x, y, w: cwd, h: cht, fill: { color: C.card }, shadow: shadow() });
    s.addShape("rect", { x, y, w: cwd, h: 0.06, fill: { color: it.c || C.teal } });
    if (it.n) s.addText(it.n, { x: x + 0.26, y: y + 0.22, w: cwd - 0.5, h: 0.46, fontFace: F.serif,
      fontSize: 19, bold: true, color: it.c || C.teal, margin: 0 });
    s.addText(it.h, { x: x + 0.26, y: y + (it.n ? 0.68 : 0.26), w: cwd - 0.5, h: 0.52,
      fontFace: F.serif, fontSize: 14, bold: true, color: C.ink, margin: 0, lineSpacingMultiple: 1.08 });
    s.addText(it.b, { x: x + 0.26, y: y + (it.n ? 1.16 : 0.78), w: cwd - 0.5,
      h: cht - (it.n ? 1.34 : 0.96), fontFace: F.sans, fontSize: 10.8, color: C.mute,
      margin: 0, lineSpacingMultiple: 1.26, valign: "top" });
  });
  footer(d, s);
}

// ---- COMPARE ---------------------------------------------------------------
function compare(d, { title, kicker, intro, columns }) {
  const s = base(d);
  header(d, s, title, kicker);
  const top = introLine(s, intro);
  const n = columns.length, gx = 0.28;
  const cwd = (CW - (n - 1) * gx) / n;
  const colH = CB - top;
  columns.forEach((col, i) => {
    const x = M + i * (cwd + gx);
    const accent = col.c || (col.good ? C.green : col.bad ? C.coral : C.blue);
    s.addShape("rect", { x, y: top, w: cwd, h: colH, fill: { color: C.card } });
    s.addShape("rect", { x, y: top, w: cwd, h: 0.6, fill: { color: accent } });
    s.addText(col.head, { x: x + 0.24, y: top, w: cwd - 0.48, h: 0.6, fontFace: F.serif,
      fontSize: 15, bold: true, color: C.bg, valign: "middle", margin: 0 });
    if (col.tag) s.addText(col.tag, { x: x + 0.24, y: top + 0.68, w: cwd - 0.48, h: 0.3,
      fontFace: F.mono, fontSize: 10, color: accent, bold: true, margin: 0 });
    const items = col.items;
    const iy = top + (col.tag ? 1.06 : 0.78);
    const ih = (top + colH - 0.24 - iy) / items.length;
    items.forEach((it, j) => {
      const yy = iy + j * ih;
      s.addText([
        { text: (it.k ? it.k + "  " : ""), options: { bold: true, color: C.ink, fontFace: F.sans } },
        { text: it.v || it, options: { color: C.mute, fontFace: F.sans } },
      ], { x: x + 0.24, y: yy, w: cwd - 0.48, h: ih, fontSize: 10.8, valign: "middle",
           margin: 0, lineSpacingMultiple: 1.18 });
      if (j < items.length - 1)
        s.addShape("line", { x: x + 0.24, y: yy + ih, w: cwd - 0.48, h: 0, line: { color: C.line, width: 0.5 } });
    });
  });
  footer(d, s);
}

// ---- STATS -----------------------------------------------------------------
function stats(d, { title, kicker, intro, items, note }) {
  const s = base(d);
  header(d, s, title, kicker);
  const top0 = introLine(s, intro);
  const n = items.length, gx = 0.28;
  const cwd = (CW - (n - 1) * gx) / n;
  const cht = note ? 3.05 : 3.5;
  const top = top0 + 0.1;
  items.forEach((it, i) => {
    const x = M + i * (cwd + gx);
    s.addShape("rect", { x, y: top, w: cwd, h: cht, fill: { color: C.card }, shadow: shadow() });
    s.addShape("rect", { x, y: top, w: cwd, h: 0.06, fill: { color: it.c || C.teal } });
    s.addText(String(it.big), { x: x + 0.16, y: top + 0.4, w: cwd - 0.32, h: 1.35, fontFace: F.serif,
      fontSize: it.bigSize || 54, bold: true, color: it.c || C.teal, align: "center",
      valign: "middle", margin: 0 });
    s.addText(it.label, { x: x + 0.2, y: top + 1.82, w: cwd - 0.4, h: 0.44, fontFace: F.sans,
      fontSize: 12.5, bold: true, color: C.ink, align: "center", margin: 0 });
    s.addText(it.sub || "", { x: x + 0.24, y: top + 2.28, w: cwd - 0.48, h: cht - 2.4,
      fontFace: F.sans, fontSize: 10.2, color: C.mute, align: "center", margin: 0,
      lineSpacingMultiple: 1.24, valign: "top" });
  });
  if (note) s.addText(note, { x: M, y: top + cht + 0.26, w: CW, h: 0.6, fontFace: F.sans,
    fontSize: 11.5, color: C.mute, italic: true, align: "center", margin: 0, lineSpacingMultiple: 1.3 });
  footer(d, s);
}

// ---- TABLE -----------------------------------------------------------------
function table(d, { title, kicker, intro, head, rows, colW }) {
  const s = base(d);
  header(d, s, title, kicker);
  const top = introLine(s, intro);
  const headRow = head.map((h) => ({
    text: h, options: { fill: { color: C.cardHi }, color: C.teal, bold: true,
      fontFace: F.sans, fontSize: 11.5, align: "left", valign: "middle" } }));
  const body = rows.map((r, ri) => r.map((cell) => {
    const isObj = cell && typeof cell === "object";
    return {
      text: isObj ? cell.t : String(cell),
      options: { fill: { color: ri % 2 ? C.bg : C.card }, fontFace: F.sans, fontSize: 10.6,
        color: isObj && cell.c ? cell.c : C.mute, bold: !!(isObj && cell.b),
        align: "left", valign: "middle" } };
  }));
  s.addTable([headRow, ...body], {
    x: M, y: top, w: CW, colW,
    rowH: (CB - top) / (rows.length + 1),
    border: { type: "solid", color: C.line, pt: 0.75 },
    margin: [3, 8, 3, 8],
  });
  footer(d, s);
}

// ---- LAYERS ----------------------------------------------------------------
function layers(d, { title, kicker, intro, items, note }) {
  const s = base(d);
  header(d, s, title, kicker);
  const top = introLine(s, intro);
  const n = items.length, gap = 0.13;
  const bottom = note ? CB - 0.42 : CB;
  const bh = (bottom - top - (n - 1) * gap) / n;
  items.forEach((it, i) => {
    const y = top + i * (bh + gap);
    const accent = it.c || C.teal;
    s.addShape("rect", { x: M, y, w: CW, h: bh, fill: { color: it.hi ? C.cardHi : C.card } });
    s.addShape("rect", { x: M, y, w: 0.10, h: bh, fill: { color: accent } });
    s.addText(it.h, { x: M + 0.36, y, w: 4.5, h: bh, fontFace: F.serif, fontSize: 13.5,
      bold: true, color: C.ink, valign: "middle", margin: 0, lineSpacingMultiple: 1.05 });
    s.addText(it.b, { x: M + 5.0, y, w: CW - 5.3, h: bh, fontFace: F.sans, fontSize: 10.6,
      color: C.mute, valign: "middle", margin: 0, lineSpacingMultiple: 1.2 });
  });
  if (note) s.addText(note, { x: M, y: bottom + 0.14, w: CW, h: 0.42, fontFace: F.sans,
    fontSize: 10.5, color: C.faint, italic: true, align: "center", margin: 0 });
  footer(d, s);
}

// ---- STEPS -----------------------------------------------------------------
function steps(d, { title, kicker, intro, items, note }) {
  const s = base(d);
  header(d, s, title, kicker);
  const top = introLine(s, intro);
  const n = items.length;
  const avail = (note ? CB - 0.5 : CB) - top;
  const rowH = Math.min(1.1, avail / n);
  const blockH = rowH * n;
  const y0 = top + (avail - blockH) / 2;
  // one continuous connector behind the circles
  if (n > 1)
    s.addShape("line", { x: M + 0.27, y: y0 + rowH / 2, w: 0, h: (n - 1) * rowH,
      line: { color: C.line, width: 1.4 } });
  items.forEach((it, i) => {
    const y = y0 + i * rowH;
    const cy = y + rowH / 2 - 0.27;
    s.addShape("oval", { x: M, y: cy, w: 0.54, h: 0.54,
      fill: { color: C.card }, line: { color: it.c || C.teal, width: 1.5 } });
    s.addText(String(i + 1), { x: M, y: cy, w: 0.54, h: 0.54, fontFace: F.serif,
      fontSize: 16, bold: true, color: it.c || C.teal, align: "center", valign: "middle", margin: 0 });
    s.addText(it.h, { x: M + 0.86, y, w: 4.4, h: rowH, fontFace: F.serif,
      fontSize: 13.5, bold: true, color: C.ink, valign: "middle", margin: 0, lineSpacingMultiple: 1.1 });
    s.addText(it.b, { x: M + 5.4, y, w: CW - 5.4, h: rowH, fontFace: F.sans,
      fontSize: 11, color: C.mute, valign: "middle", margin: 0, lineSpacingMultiple: 1.22 });
  });
  if (note) s.addText(note, { x: M + 0.86, y: y0 + blockH + 0.16, w: CW - 0.86, h: 0.42,
    fontFace: F.sans, fontSize: 10.5, color: C.faint, italic: true, margin: 0 });
  footer(d, s);
}

// ---- STATEMENT -------------------------------------------------------------
function statement(d, { kicker, big, sub, attin }) {
  d.page += 1;
  const s = d.pres.addSlide();
  s.background = { color: C.bgAlt };
  s.addText(kicker || "", { x: M + 0.4, y: 1.5, w: CW, h: 0.4, fontFace: F.sans, fontSize: 12,
    color: C.teal, bold: true, charSpacing: 3, margin: 0 });
  s.addShape("rect", { x: M + 0.4, y: 2.0, w: 0.7, h: 0.12, fill: { color: C.teal } });
  s.addText(big, { x: M + 0.4, y: 2.32, w: CW - 0.8, h: 2.6, fontFace: F.serif, fontSize: 30,
    bold: true, color: C.ink, lineSpacingMultiple: 1.22, margin: 0, valign: "top" });
  if (sub) s.addText(sub, { x: M + 0.4, y: 5.15, w: CW - 1.4, h: 1.45, fontFace: F.sans,
    fontSize: 13.5, color: C.mute, lineSpacingMultiple: 1.38, margin: 0 });
  if (attin) s.addText(attin, { x: M + 0.4, y: 6.7, w: CW - 2, h: 0.4, fontFace: F.mono,
    fontSize: 9.5, color: C.faint, margin: 0 });
  pageNum(d, s);
}

// ---- CLOSING ---------------------------------------------------------------
function closing(d, { title, points, tagline, next }) {
  d.page += 1;
  const s = d.pres.addSlide();
  s.background = { color: C.bgAlt };
  s.addShape("rect", { x: M, y: 1.3, w: 0.20, h: 1.15, fill: { color: C.teal } });
  s.addText("小结", { x: M + 0.42, y: 1.26, w: 8, h: 0.36, fontFace: F.sans, fontSize: 12,
    color: C.teal, bold: true, charSpacing: 3, margin: 0 });
  s.addText(title, { x: M + 0.42, y: 1.6, w: CW - 0.5, h: 0.9, fontFace: F.serif, fontSize: 29,
    bold: true, color: C.ink, margin: 0, lineSpacingMultiple: 1.1 });
  const pts = points || [];
  const top = 2.78, rowH = Math.min(0.78, 2.7 / pts.length);
  pts.forEach((p, i) => {
    const y = top + i * rowH;
    s.addShape("rect", { x: M + 0.42, y: y + 0.08, w: 0.15, h: 0.15, fill: { color: C.teal } });
    s.addText(p, { x: M + 0.76, y, w: CW - 1.0, h: rowH, fontFace: F.sans,
      fontSize: 12.5, color: C.mute, valign: "middle", margin: 0, lineSpacingMultiple: 1.2 });
  });
  if (tagline) {
    const ty = top + pts.length * rowH + 0.22;
    s.addShape("rect", { x: M + 0.42, y: ty, w: CW - 0.84, h: 0.92, fill: { color: C.card } });
    s.addShape("rect", { x: M + 0.42, y: ty, w: 0.07, h: 0.92, fill: { color: C.teal } });
    s.addText(tagline, { x: M + 0.72, y: ty, w: CW - 1.5, h: 0.92, fontFace: F.serif,
      fontSize: 13, italic: true, color: C.teal, valign: "middle", margin: 0, lineSpacingMultiple: 1.25 });
  }
  if (next) s.addText(next, { x: M + 0.42, y: 7.06, w: 10, h: 0.34, fontFace: F.mono,
    fontSize: 9.5, color: C.faint, margin: 0 });
  pageNum(d, s);
}

module.exports = { C, F, newDeck, save, cover, agenda, section, bullets, cards,
  compare, stats, table, layers, steps, statement, closing };
