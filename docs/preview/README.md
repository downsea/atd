# ATD 技术预览 · Technical Preview Decks

A five-part Chinese-language slide series introducing ATD (Agent Tool Dispatch)
to a technical audience. Each part is a self-contained `.pptx` deck and can be
presented on its own.

The decks are **generated from the docs in this repository** — they are a
point-in-time snapshot of the 1.0 documentation, not an independent source of
truth. Where a deck and the docs disagree, the docs win
([`../index.md`](../index.md) defines the authority hierarchy).

## The five parts

| # | Deck | 内容 | Sourced from |
|---|---|---|---|
| 1 | [`atd-preview-1-background.pptx`](atd-preview-1-background.pptx) | 背景与必要性 —— 对比 CLI / MCP / REST，ATD 解决了什么 | `atd-architecture.md` · `atd-positioning.md` · `README.md` |
| 2 | [`atd-preview-2-design-principles.pptx`](atd-preview-2-design-principles.pptx) | 设计原则 —— 三类消费者 + 七条原则 | `atd-design-philosophy.md` |
| 3 | [`atd-preview-3-architecture.pptx`](atd-preview-3-architecture.pptx) | 架构与设计 —— schema · dispatch · 安全 · 扩展点 · crate 地图 | `atd-architecture.md` · `extending/` |
| 4 | [`atd-preview-4-adoption.pptx`](atd-preview-4-adoption.pptx) | 实施案例与集成 —— 五条路径 · 三家 adopter | `integrations/` · `atd-positioning.md` |
| 5 | [`atd-preview-5-roadmap.pptx`](atd-preview-5-roadmap.pptx) | 路线与生态发展建议 | `roadmap.md` · `release-plan-v1.0.md` |

## Regenerating the decks

The decks are built with [pptxgenjs](https://gitbrent.github.io/PptxGenJS/).
Sources are in [`src/`](src/) — one `theme.js` (shared design system) plus one
`deckN.js` per part.

```bash
cd docs/preview/src
npm install
./build.sh          # regenerates all five .pptx into docs/preview/
```

Chinese text uses the Noto CJK font family (`Noto Serif CJK SC` /
`Noto Sans CJK SC`); install it if your system lacks CJK fonts before
presenting or re-rendering.
