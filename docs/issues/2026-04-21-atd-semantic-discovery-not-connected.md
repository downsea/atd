# ATD semantic discovery not wired — intent_examples 存在但 runtime 仅关键词搜索

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** MEDIUM — 影响 tool discovery 准确率，触及白皮书性能声明
**Component:** `crates/anos-tool-dispatch`, `crates/anos-embedding`
**Related:** ATD 白皮书 v2 §2.1 (intent match 语义), §3.3 (HNSW p99 < 80ms 声明); `docs/architecture/atd-overview.md §11.2 Gap 1`

## Symptom

ATD 白皮书在多处承诺语义化 tool discovery：

- §2.1 D1 Intent Match: "基于 intent_examples 的语义匹配"
- §3.3 FIG 17: "Warm tier HNSW 语义搜索 p99 < 80 ms"
- §11 Capacity Layer: "tool.search(intent) 使用 HNSW 最近邻检索"

但当前 ANOS runtime 的 tool discovery 路径**只用关键词搜索**。`capability.intent_examples` 字段存在于 schema、存在于注册的每个 tool 的 `ToolDefinition` 中，**但没有任何代码把它们变成 embedding 索引**。

架构文档 §11.2 Gap 1 自承：

> "ATD 有 capability.intent_examples 字段，设计上支持 embedding 相似度发现，但运行时**只用关键词搜索**。Agent 说'我要处理图片' → 无法自动匹配到 host:media.image_convert。"

## Root Cause

HNSW 索引需要的完整依赖链：

```
Tool registration
  → intent_examples 列表
  → 调用 EmbeddingProvider.embed(text) （本地 fastembed 或远程 API）
  → 向量化结果
  → 插入 HNSW 索引（anos-embedding crate 已有）
  → 查询时：intent → embed → HNSW nearest neighbor → tool candidates
```

当前缺失：
1. Tool registration 时没有触发 embedding 生成
2. `anos-embedding` crate 存在但没被 `anos-tool-dispatch` 消费
3. `tool.search` 入口（如存在）仍然走关键词分支

## Current state

- ✅ ATD schema 的 `capability.intent_examples: Vec<String>` 已定义
- ✅ `anos-embedding` crate 提供 HNSW 实装
- ✅ 多数内置工具和 host 插件已填写了 `intent_examples`
- ❌ Embedding 生成在 tool registration 时不触发
- ❌ HNSW 索引没有 tool 向量
- ❌ Tool discovery 退化为关键词搜索

## Fix

约 200-300 行改动（架构文档估算约 200 行）：

1. **在 `ToolRegistry::register` 中**，若配置了 EmbeddingProvider：
   ```rust
   if let Some(provider) = &self.embedding_provider {
       let text = tool.capability.intent_examples.join(" ");
       let vec = provider.embed(&text).await?;
       self.hnsw_index.insert(tool.id.clone(), vec);
   }
   ```

2. **新增 `tool.search(intent)` 入口**（或扩展现有发现入口）：
   ```rust
   async fn search_by_intent(&self, intent: &str) -> Vec<(ToolId, f32)> {
       let query_vec = self.embedding_provider.embed(intent).await?;
       self.hnsw_index.search(&query_vec, top_k=10)
   }
   ```

3. **degradation path**：如果没有配置 EmbeddingProvider，保留当前关键词搜索作为 fallback。

4. **性能验证**：添加 benchmark 确认 p99 < 80ms 声明。

## Validation

- 注册 100 个含 intent_examples 的工具后
- 查询 "process image" 能返回 `host:media.image_convert` 在 top 5
- p99 延迟测得 < 80ms（或更新白皮书为实测值）
- 无 EmbeddingProvider 时 degradation 到关键词搜索不崩溃

## Priority

P1 — 是白皮书声称的性能数字成立的前提。若不实装，白皮书 §3.3 的 "HNSW p99 < 80ms" 必须从"实测"改为"设计目标"。
