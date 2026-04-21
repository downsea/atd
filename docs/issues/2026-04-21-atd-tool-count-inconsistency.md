# ATD tool count inconsistency — 20 / 94 / 102 三组数字在文档间漂移

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** LOW — 非代码 bug，是文档 / 叙事一致性问题
**Component:** docs
**Related:** `docs/modules/anos-tool-dispatch.md`, `docs/architecture/atd-overview.md`, `CLAUDE.md`, `docs/research/toward-agent-tool-dispatch.md` / `-v2.md`

## Symptom

ANOS 内置工具总数在不同文档中**不一致**：

| 文档 | 声称的工具数 |
|-----|-------------|
| `docs/modules/anos-tool-dispatch.md` § "Overview" | **20 built-in tools** |
| `docs/architecture/atd-overview.md` § header | **94 built-in + 10 host plugins + MCP bridge** |
| `CLAUDE.md` § "Built-in Tools" | **102 built-in tools + host:* plugins** |
| `docs/research/toward-agent-tool-dispatch.md` § "当前状态" (修订前) | **102 个内置工具** |
| `docs/research/toward-agent-tool-dispatch-v2.md` FIG 17（修订前） | **102 tools** |

差距最大 ~5x（20 vs 102）。读者无法判断真实规模。

## Root Cause

三组数字对应三个**不同计数口径**：

- **20**：`crates/anos-tool-dispatch/src/builtins.rs` 中真正的 `fn builtin_definitions()` 返回数量（严格的 L4 atomic tools）
- **94**：加上 `anos-runtime` 中的 session-managed tools（browser.*, terminal.*, desktop.*, agent.*, session.*）以及一些未在 `anos-tool-dispatch` crate 内但通过 ATD 分发的工具
- **102**：进一步包括 host:* 插件 + 其他 cross-crate 贡献的工具

三组数字都"技术上正确"，但**读者不知道口径差异**——白皮书简单写 "102 tools" 给人的印象是 `anos-tool-dispatch` crate 本身有 102 个实装，这是错的。

## Current state

- ❌ 文档间数字不统一
- ❌ 无明确的"工具计数口径"定义
- ❌ Module doc 的 20 数字已过时（可能是早期数字）

## Fix

**Part 1 - 定义计数口径**（10 分钟）：

在 `docs/architecture/atd-overview.md` 增加小节：

```markdown
### 工具计数口径

ANOS 的工具分三类，总数取决于口径：

| 口径 | 数量 | 说明 |
|-----|-----|-----|
| Core atomic tools | ~20 | `crates/anos-tool-dispatch/src/builtins.rs` 中的 L4 原子工具（fs/shell/web/git/docker 等）|
| Core + Session-managed | ~94 | 加上 browser.* / terminal.* / desktop.* / agent.* / session.* |
| Core + Session-managed + Host plugins | ~104 | 再加上 10 个 host:* bundled 插件 |

白皮书和对外材料默认使用**第三口径（~104）**作为"ANOS 可用工具总数"，因为它们都通过 ATD dispatch 对 agent 可见。
```

**Part 2 - 同步所有文档**（30 分钟）：

更新以下文档到统一数字：
- `docs/modules/anos-tool-dispatch.md`：将 "20 built-in tools" 改为引用新的计数口径表
- `CLAUDE.md`：确认 "102" 与最终口径一致（可能微调为 "104"）
- 白皮书 v1 / v2：修订版已改为诚实披露（不再用单个大数字），本 issue 关联已完成

**Part 3 - 建立 single source of truth**（1 小时）：

最优：在 `Cargo.toml` 或 build script 中自动计算并生成 `docs/generated/tool-count.md`，由所有文档引用。避免人工同步漂移。

## Validation

- 所有文档引用同一个计数口径表
- `anos schema --stats` 能实时显示当前计数
- CI 检查关键文档中的工具数字与实际 builtin_definitions() 返回一致

## Priority

P2 — 不阻塞任何功能，但影响文档一致性和白皮书诚信。建议和 `atd-benchmark-suite-missing.md` 一起作为"文档诚实性" workstream 处理。
