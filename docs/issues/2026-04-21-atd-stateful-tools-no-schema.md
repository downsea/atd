# ATD schema cannot express stateful tools — browser.* / terminal.* 的状态性依赖惯例而非协议

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** LOW — 功能上 HostInterface Agent 已处理，但协议层缺失
**Component:** `crates/anos-types`, ATD spec v1.1
**Related:** ATD 白皮书 v2 §2.2 (Layer 3: session-managed tools); `docs/architecture/atd-overview.md §11.2 Gap 5`

## Symptom

ATD v1.0 schema 假设工具是**无状态**的（JSON in → JSON out）。但 L3 层的 session-managed tools 是**有状态**的：

- `browser.navigate` 的"当前页面"
- `terminal.start` 的 PTY session
- `desktop.focus` 的当前 window

这些工具的 cross-turn 状态：

```
Turn 1: browser.navigate → https://example.com
Turn 2: browser.click(selector) → 点击的是 example.com 的按钮  ← 隐式依赖 Turn 1 的状态
```

ATD schema 目前**无法表达**：
- 这个工具有状态（model: stateless / session_scoped / global）
- 同一 session 的调用应路由到同一实例（session_affinity）
- 状态的生命周期（idle_timeout）

结果：HostInterface Agent 用 Rust 代码内部管理状态，但 ATD schema 层看不到这种复杂性。

## Root Cause

ATD v1.0 设计决策：**保持 Schema 简洁**。状态管理被下放到 L3 层（HostInterface Agent）处理。代价是：

1. Schema 不能自描述工具的状态性
2. 第三方实装者（非 ANOS）重现 ATD 协议时，不知道哪些工具需要状态管理
3. Conformance test 无法检查"stateful tool 的 session affinity"

架构 §11.2 Gap 5 建议 ATD v1.1 增加：

```yaml
state:
  model: stateless | session_scoped | global
  session_affinity: true
  idle_timeout_secs: 1800
```

## Current state

- ✅ L3 工具在 ANOS 实装中通过 HostInterface Agent + session manager 正常工作
- ❌ ATD schema 无 `state` 字段
- ❌ 其他实装者无法从 schema 推断工具的状态性
- ❌ ATD conformance test 未覆盖 stateful 语义

## Fix

这是 **ATD v1.1 spec 增量**，非 v1.0 bug。

**Part 1 - 扩展 ATD schema**：

```rust
pub enum StateModel {
    Stateless,       // 纯 IO，调用间无共享状态
    SessionScoped,   // session 内持续，跨 session 隔离
    Global,          // 跨 session 共享（谨慎使用）
}

pub struct StateSpec {
    pub model: StateModel,
    pub session_affinity: bool,
    pub idle_timeout_secs: Option<u32>,
    pub persistence: StatePersistence,  // in_memory | persistent_on_disk
}

// 在 ToolDefinition 中新增字段
pub struct ToolDefinition {
    // ... existing fields
    pub state: Option<StateSpec>,  // None = stateless（v1.0 兼容）
}
```

**Part 2 - Dispatch 层理解 state**：

- `session_affinity: true` → 路由到同一 executor 实例
- `idle_timeout_secs` → session manager 过期清理
- State 冲突检测（避免两个 global tool 用同一 resource）

**Part 3 - Conformance test**：

- 测 Stateful tool 在同一 session 内保持状态
- 测 Stateless tool 不泄漏状态到下次调用

## Validation

- 一份 ATD 定义声明 `state: { model: session_scoped, session_affinity: true }` 对 `browser.navigate`
- 同 session 内多次调用路由到同一 browser 实例
- 跨 session 调用不共享 cookie/history
- Idle timeout 后 session 自动清理

## Priority

P3 — 当前 ANOS 内部已有工作实装，协议层补齐是为了：
1. 第三方实装者的规范遵循
2. ATD v1.1 → v2.0 演进的完整性
3. Conformance test 的覆盖面

可延到 v1.1 spec 规划时一并处理。
