# ATD Hot/Warm/Cold tier management incomplete — 升降级自动化未验证

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** MEDIUM — 白皮书核心卖点之一，影响规模化叙事
**Component:** `crates/anos-tool-dispatch`
**Related:** ATD 白皮书 v1 §11 Capacity Layer, v2 §2.2 + §5.1 Tier 概念, FIG 17; `docs/architecture/atd-overview.md §5`

## Symptom

ATD 白皮书以 **Hot/Warm/Cold 三层容量模型**为核心创新之一，声称：

- Hot tier：≤20 tools，~3K tokens 在 system prompt
- Warm tier：≤200 tools，本地索引（~50ms 发现延迟）
- Cold tier：∞，远程 registry（~200ms）
- 自动 promotion/demotion 规则：
  - Cold → Warm：首次成功调用
  - Warm → Hot：7 天内 ≥5 次调用
  - Hot → Warm：14 天未用
  - Warm → Cold：90 天未用

当前 `crates/anos-tool-dispatch/src/` 只有：
- `registry.rs` — 基础 ToolRegistry（flat 结构）
- `persistent.rs` — `PersistentToolRegistry`（SQLite-backed）

**没有看到 tier 字段 / 升降级逻辑 / frequency score 计算**的证据。具体缺失：

- `ToolEntry.tier: ToolTier` 字段存在但不明确是否被运行时使用
- 调用频率统计是否记录到 persistent store？未验证
- Promotion/demotion 是否由定时任务/事件触发？未看到对应模块

## Root Cause

Tier 系统的完整实装需要：

1. **数据层**：每个 tool 的 `(last_called, calls_7d, calls_30d, tier)` 持久化
2. **统计层**：每次 dispatch 后更新调用计数，计算 frequency score
3. **调度层**：周期性或事件驱动的 promotion/demotion
4. **注入层**：根据 Hot tier 向 system prompt 注入 compact ATD；Warm 通过 tool.search 暴露

设计文档齐备，但工程实装可能跳过了定时 promotion/demotion 环节，或实装了但未被 system prompt 构建器使用。

## Current state

- ✅ `PersistentToolRegistry`（SQLite）已有，可持久化 metadata
- ✅ Tier 的概念在 schema 和文档中清晰
- ⚠️ **需要审计**：调用计数是否实时更新？
- ⚠️ **需要审计**：promotion/demotion 是否被触发过？
- ⚠️ **需要审计**：System prompt 构建是否按 Hot tier 注入？
- ❌ 自动化 tier 管理的 CI/benchmark 未找到

## Fix

审计 + 补齐三层：

**Step 1 - Audit**：

- 读 `registry.rs` / `persistent.rs`，确认当前 tier 数据模型
- 读 engine.rs（或 context builder），确认 system prompt 如何挑选注入的 tool 描述
- 用 instrumented run（注册 50+ 工具，调用一部分）验证 tier 字段是否变化

**Step 2 - 补齐 missing pieces**：

- 若调用计数未实时更新：dispatch 层末尾增加 `registry.record_call(tool_id, outcome)`
- 若无 promotion 触发：在 daemon 主循环加 `tool_tier_manager` 定时任务（每小时 tick）
- 若 system prompt 未用 tier：engine 的 prompt builder 按 tier 过滤 + sort

**Step 3 - 数据验证**：

- 注册 200+ 工具，模拟 7 天的调用分布
- 验证：前 20 个高频工具自动进 Hot tier
- 验证：长期未用的工具自动降级

## Validation

- `anos schema --tier` CLI 显示每个工具当前 tier
- 连续运行 7 天后 Hot tier 的前 20 个工具是实际调用 Top 20
- System prompt 的 tool 描述 token 数 < 5000 即使注册工具数 > 200

## Priority

P1 — 这是白皮书规模化叙事（"1000+ tool 不爆 context"）成立的前提。若不实装，需从白皮书降调：tier 从"已实装"改为"设计完整、实装审计中"。

## Related issues

- `2026-04-21-atd-semantic-discovery-not-connected.md` — Warm tier 的发现延迟依赖 HNSW
- `2026-04-21-atd-benchmark-suite-missing.md` — tier 性能指标需要 benchmark
