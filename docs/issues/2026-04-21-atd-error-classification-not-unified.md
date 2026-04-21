# ATD error classification not unified — generic string errors, no ErrorClass enum

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** MEDIUM — 影响 Agent 重试决策质量
**Component:** `crates/anos-tool-dispatch`, `crates/anos-types`
**Related:** ATD 白皮书 v2 Appendix B (统一错误码表); `docs/architecture/atd-overview.md §4.5 (Error Code Mapping), §11.2 Gap 4`

## Symptom

ATD 白皮书 Appendix B 给出了完整的跨 binding 统一错误码表：

| ATD Code | CLI exit | MCP code | REST | 可重试 |
|---------|---------|---------|------|-------|
| PERMISSION_DENIED | 2 | -32600 | 403 | No |
| RATE_LIMITED | 1 | -32000 | 429 | Yes (delay) |
| TIMEOUT | 5 | -32000 | 504 | Yes |
| VALIDATION_ERROR | 3 | -32602 | 400 | No（需修正）|
| ... | ... | ... | ... | ... |

但实际 `ToolResult::Error` 定义（`anos-types/src/tool.rs`）目前是 generic string：

```rust
// Current:
Error {
    code: String,       // 未枚举化
    message: String,
    // 没有 ErrorClass
    // 没有 retryable 的结构化表达
}
```

架构文档 §11.2 Gap 4 自承：

> "LLM 无法区分：
> - 暂时错误（重试有意义）: rate limit, timeout, 503
> - 永久错误（修改参数）: 404, 400, permission denied
> - 环境错误（换工具）: binary not found, feature not available"

## Root Cause

ATD v1.0 schema 虽然在 `errors: Vec<ErrorDef>` 字段中允许工具声明自定义错误类型，但：

1. 大多数内置工具没有填写这个字段
2. `ToolResult::Error` 结构体没有 `class` 字段
3. MCP / REST binding 的错误 → ATD 错误码的映射表 "定义了但没落代码"

## Current state

- ✅ Appendix B 的错误码表已写入白皮书
- ✅ 部分工具（MCP binding）做了粗粒度错误映射
- ❌ `ErrorClass` enum 未定义
- ❌ `ToolResult::Error.error_class` 字段不存在
- ❌ 跨 binding 的统一映射代码不完整
- ❌ `retryable` / `retry_after_ms` 虽然在白皮书结构里描述，实际不一致

## Fix

**Part 1 - 增加 ErrorClass enum**（`anos-types/src/tool.rs`）：

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ErrorClass {
    Transient,     // 暂时，可重试（rate limit, timeout, network）
    Permanent,     // 永久，改 params 才可能成功（validation, permission）
    Environmental, // 环境问题，换工具或调配置（binary missing, feature unavailable）
}

pub struct ToolError {
    pub code: ATDErrorCode,          // 枚举，见 Part 2
    pub class: ErrorClass,
    pub message: String,
    pub retryable: bool,             // class == Transient 时自动为 true
    pub retry_after_ms: Option<u32>,
    pub binding_error: Option<Value>, // 原始协议错误，debug 用
}
```

**Part 2 - 统一 ATDErrorCode 枚举**：

```rust
pub enum ATDErrorCode {
    PermissionDenied,
    RateLimited,
    Timeout,
    ValidationError,
    ToolNotFound,
    PlatformUnsupported,
    BudgetExceeded,
    CircuitOpen,
    ConstitutionalViolation,
    InternalError,
    // ... 按白皮书 Appendix B 完整枚举
}
```

**Part 3 - 各 binding 的映射表落到代码**：

- `binding_mcp.rs`：JSON-RPC error.code → ATDErrorCode
- `binding_rest.rs`：HTTP status → ATDErrorCode
- 新增 `error_mapping.rs`：集中维护映射

## Validation

- 一个 rate-limited MCP 工具调用 → `ToolResult::Error { class: Transient, retry_after_ms: Some(N) }`
- 一个参数验证失败 → `ToolResult::Error { class: Permanent, retryable: false }`
- Agent 的重试决策可以基于 `error_class`（Transient 自动重试、Permanent 不重试、Environmental 问用户）

## Priority

P1 — 对 Agent 自主性和重试智能都是关键。没有结构化错误分类，每个 Agent 都要重新发明"这个错误该不该重试"的逻辑。
