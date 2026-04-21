# ATD dry-run dispatch not wired — supports_dry_run 字段存在但 runtime 忽略

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** HIGH — 对 Dangerous 级工具是显著安全缺口
**Component:** `crates/anos-tool-dispatch`
**Related:** ATD 白皮书 v2 Appendix A (schema 字段), `docs/architecture/atd-overview.md §11.2 Gap 3`; Agent-Native CLI Principle 4

## Symptom

ATD schema 定义了 `safety.supports_dry_run: bool` 字段，架构文档明确说"Agent-Native CLI Principle 4 强调 dry-run 是安全网"。

但当前 runtime **完全不处理 dry_run 参数**：

```rust
// Current (problematic):
registry.dispatch(tool_id, params);  // 直接执行，无 dry-run 分支

// Expected:
registry.dispatch_with_mode(tool_id, params, DispatchMode::DryRun);
```

具体后果：

1. Agent 请求 `rm -rf ./build` 的 dry-run 预览 → runtime 直接执行，**数据可能真的被删除**
2. 白皮书 §5 VI "Least Privilege" 和 Appendix A 展示 `supports_dry_run` 字段给读者预期，但用户实际调用时得不到对应行为
3. 违反 P8（Human-on-the-Loop）——"允许人在不可逆操作前预览"是该原则的基础

## Root Cause

设计时认为 dry-run 是每个工具内部实装的责任——但：

1. 没有统一的 dispatch-层 dry-run 模式通路
2. 每个工具都要自己知道"我被用 dry_run=true 调用时应该不执行副作用"——实际上没人实装

架构 §11.2 Gap 3 明确说：

> "dispatch 层检查 dry_run: true 参数，tool handler 返回'将要执行什么'而非实际执行。需要每个 Dangerous tool 实现 dry-run 路径。"

## Current state

- ✅ ATD schema 有 `safety.supports_dry_run: bool`
- ✅ 部分工具（如 `fs.delete`、`shell.exec`）在设计上**能**dry-run
- ❌ Dispatch 层没有 dry-run 模式分支
- ❌ 没有标准的 dry-run 返回格式（应该返回"将要执行什么"）
- ❌ 没有工具实装 dry-run 路径

## Fix

三部分改动：

**Part 1 - Dispatch 层支持**：

```rust
pub enum DispatchMode {
    Execute,      // 当前默认
    DryRun,       // 新增：返回"将要做什么"但不做
}

pub async fn dispatch(
    tool_id: &str,
    params: Value,
    mode: DispatchMode,
) -> ToolResult { ... }
```

**Part 2 - 标准 dry-run 返回格式**：

```rust
pub struct DryRunResult {
    tool_id: ToolId,
    resolved_binding: BindingKind,
    would_execute: Value,        // 具体命令/API 调用
    estimated_side_effects: Vec<SideEffect>,
    would_produce: Option<Value>, // 预测的输出（若能静态推导）
    reversibility: Reversibility,
}
```

**Part 3 - Dangerous 工具实装 dry-run handler**：

优先工具（按危险级排序）：
1. `fs.delete` / `fs.move` — 只返回"将要操作的文件列表"
2. `shell.exec` — 返回 argv + cwd + env diff，不实际 spawn
3. `docker.run` — 返回 docker CLI 命令，不实际 run
4. `git.push --force` — 返回本地 ref 和 remote 的 diff
5. Dangerous 级 host:* 插件（ffmpeg 转换、yt-dlp 下载等）

## Validation

- 一个 Dangerous 工具在 dry_run 模式下不产生实际副作用
- Agent 能使用 dry-run 结果向用户展示"即将发生什么"然后请求确认
- 白皮书 Appendix A 的 `supports_dry_run` 字段有实际含义

## Priority

P0 — 这是对用户安全承诺的兑现。在 Dangerous 工具被使用的任何场景（自主 agent 执行任务、/dangerously-skip-permissions 模式等），dry-run 是**减少不可逆损失**的唯一协议级机制。
