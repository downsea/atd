# ATD benchmark suite missing — 白皮书的所有性能声明无代码验证

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** MEDIUM — 影响白皮书诚信和采纳者的容量规划
**Component:** `crates/anos-tool-dispatch`
**Related:** ATD 白皮书 v2 §3.3 FIG 17 (所有性能指标); `docs/architecture/atd-overview.md §5 (tier 延迟), §7 (Circuit Breaker)`

## Symptom

ATD 白皮书 v2 §3.3 和 FIG 17 列出 6 项性能指标：

| 指标 | 白皮书值 |
|-----|---------|
| Dispatch 平均延迟 (Step 1-8) | < 5 ms |
| UCAN capability token 验证 | < 1 ms (cached) |
| Hot tier (20 tools) context 占用 | ~ 3K tokens |
| Warm tier HNSW 语义搜索 p99 | < 80 ms |
| MCP server 动态注册 | 秒级 |
| 跨 Read 工具并行执行 | 8 concurrent |

这些数字以 "生产级实测" 的口吻呈现。但：

1. `crates/anos-tool-dispatch/benches/` **不存在**（Rust 标准 benchmark 目录）
2. `Cargo.toml` 中没有 `[[bench]]` 配置
3. 没有 `criterion` 或类似 benchmark 框架依赖

这些数字多半是设计目标、理论估算、或其他项目的数据——没有代码证据。

## Root Cause

优先级排序：先实装、后验证。Benchmark 通常是项目后期才建设的，ATD 当前处于"实装和设计优先"阶段。

但**白皮书的性能声明以"实测"口吻写入后**，benchmark 缺失变成**诚信问题**而非单纯的技术债。

## Current state

- ❌ 无 benchmark 代码
- ❌ 无性能回归测试
- ❌ 无 flamegraph / profiling 产出
- ❌ FIG 17 的 6 项数字**全部**无法验证

## Fix

分层建设：

**Phase 1 - Core dispatch benchmark**（1-2 周）：

- 新建 `crates/anos-tool-dispatch/benches/dispatch.rs`
- 使用 `criterion`
- 场景：
  - 注册 20 / 100 / 500 工具后的 dispatch 延迟
  - Hot/Warm tier 查找延迟
  - Circuit breaker 状态转换性能
  - MCP binding 调用开销

**Phase 2 - Capability token benchmark**（依赖 `atd-ucan-capability-depth-unclear.md`）：

- Verify latency（cached vs uncached）
- Attenuation chain 验证延迟随深度变化

**Phase 3 - HNSW / 语义发现 benchmark**（依赖 `atd-semantic-discovery-not-connected.md`）：

- Embedding 生成延迟
- HNSW 查询 p50/p99 随索引大小变化

**Phase 4 - End-to-end latency**：

- "Agent 说话 → tool 调用 → 结果返回" 全链路
- 按 tier 分层测量

**Phase 5 - CI 集成**：

- 在 CI 跑 benchmark
- 设置 regression 告警（性能退化 >20% 时 fail build）

## Validation

- `cargo bench` 能跑出 FIG 17 各项指标
- 实测值与白皮书数字的偏差 <20%——否则更新白皮书为实测值
- CI 中 benchmark 作为 regression gate

## Priority

P1 — 白皮书一旦公开发布，外部读者第一件事就是看 benchmark 数据。若找不到，白皮书可信度受损。

## 短期对冲（在 benchmark 建成前）

白皮书 FIG 17 已按 "设计目标" 改写（commit aeeef41 之后的修订版），标注"设计目标，benchmark suite 建设中"。这个改写是诚实表达，直到真 benchmark 建成。

## Related

- `atd-semantic-discovery-not-connected.md` — HNSW 延迟 benchmark 需 HNSW 已实装
- `atd-ucan-capability-depth-unclear.md` — token 验证延迟 benchmark 需验证链路完整
- `atd-tier-management-incomplete.md` — tier 性能 benchmark 需 tier 管理自动化
