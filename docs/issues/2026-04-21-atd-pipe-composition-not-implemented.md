# ATD pipe composition not implemented — 最大的未兑现设计承诺

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** MEDIUM — 功能上可用 LLM 循环替代，但是白皮书的重要性能卖点
**Component:** `crates/anos-tool-dispatch`
**Related:** ATD 白皮书 v2 §8 VIII Composition, v1 §14.4 (DAG 组合); `docs/architecture/atd-overview.md §11.2 Gap 2 — "ATD 最大的未兑现承诺"`

## Symptom

ATD 白皮书和设计文档多次提到 typed pipe composition 作为 ATD 的关键特性：

- v1 §2.3 Dispatch Function: "dispatch → (tool ∈ T) × params × result"（隐含链式组合）
- v2 §8 VIII Composition: "Skill 调用另一个 skill 的正确方式是派生一个新 context"
- 架构 §11.2 自承："ATD 设计了 typed pipe composition（tool_a | tool_b | tool_c），但完全没有实现"

实际情况：
```
当前：
  Agent → LLM → tool_a call → result
       → LLM → tool_b call（以 tool_a 结果为输入） → result
       → LLM → tool_c call → result
  （每次调用都是独立的 LLM round-trip，~500ms × 3 = 1.5s 延迟）

承诺：
  Agent → pipe(tool_a, tool_b, tool_c) → result
  （单次 dispatch 完成管道，零 LLM 介入，可降到 <100ms）
```

架构文档的评估：

> "这是 ATD 最大的未兑现承诺。但优先级中等——LLM 循环虽然低效，但功能上可以覆盖管道能做的事。管道的价值在于性能优化和 token 节省，不是功能性缺失。"

## Root Cause

Typed pipe composition 需要：

1. **类型系统**：Tool A 的 output schema 必须能对接 Tool B 的 input schema
   - 简单情况：exact match（A.output.file_path → B.input.path）
   - 复杂情况：字段重命名、类型强转、缺省值填充
2. **组合语义**：
   - Sequential: `a | b | c`
   - Parallel: `a & b`（两个独立工具并发）
   - Conditional: `a ? b : c`（根据 a 的结果选择 b 或 c）
3. **错误传播**：某步失败时短路、跳过、或 fallback
4. **Dispatch 流水线扩展**：8 步流水线 → 管道步骤聚合

这些都需要从 `ToolResult` envelope 出发设计 pipe 的执行模型。设计上成熟，但需要跨 5-7 个文件的实装。

## Current state

- ✅ 设计文档完备（v1 §14.4 + 架构 §11.2）
- ❌ 无 `pipe.rs` 或 composition 入口
- ❌ Tool 的 output 到下一个 tool 的 input 的类型检查逻辑不存在
- ❌ Parallel / conditional composition 语义未落地

## Fix

分阶段：

**Stage 1 - Sequential pipe（最小可用）**：

```rust
pub struct PipelineSpec {
    steps: Vec<ToolCallSpec>,
    // step[i].input 从 step[i-1].output 投影
    projections: Vec<FieldProjection>,
}

pub async fn execute_pipeline(spec: PipelineSpec) -> ToolResult {
    let mut state = serde_json::Value::Null;
    for (step, proj) in spec.steps.iter().zip(&spec.projections) {
        let inputs = apply_projection(&state, proj, &step.params)?;
        state = dispatch_tool(&step.tool_id, inputs).await?;
    }
    state
}
```

**Stage 2 - Parallel / conditional**（如果 Stage 1 有用户真实需求）

## Validation

- 能执行 `fs.read | json.parse | data.filter` 在单次 dispatch 内完成
- 实测延迟对比：LLM 循环 vs pipe
- 类型检查能在 pipe spec 构造时提前发现 mismatch

## Priority

P2 — 非功能性阻塞，纯性能/token 优化。但若不实装，ATD 白皮书需更诚实地将"管道组合"标为 future work，而非让读者误以为是当前特性。

## Related

- 依赖 issue: 无
- Blocks: ATD v1.1 spec（typed composition 应作为 v1.1 强制特性）
