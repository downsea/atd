# ATD native CLI binding not implemented — CLI 工具实际通过 host:* 绕道 shell.exec

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** MEDIUM — 违背 ATD 白皮书核心叙事（"4 种协议统一"），削弱协议差异化
**Component:** `crates/anos-tool-dispatch`
**Related:** ATD 白皮书 v1/v2 §2.2 (四 binding 架构图), §3.3 (实装声明); `docs/architecture/atd-overview.md §4.1`

## Symptom

ATD 白皮书反复强调 "4 种 protocol binding 统一在单一 schema 下"：

```
Agent: "take a photo"
  → ATD: anos:camera.capture.photo
    ├── CLI:         mobile camera +photo --rear    ← 声称 native binding
    ├── MCP:         tools/call capture_photo
    ├── AppFunction: CameraFunctions.takePhoto()
    └── REST:        POST /api/v1/tools/camera/capture
```

但 `crates/anos-tool-dispatch/src/` 实际只有 `binding_mcp.rs` 和 `binding_rest.rs` 两个 binding 模块——**`binding_cli.rs` 不存在**。

当前 CLI 工具的调用路径实际是：

```
Agent → host:* 插件定义（JSON 模板）→ 渲染为 shell.exec 命令 → 进程执行
```

这是"模板化的 shell.exec 绕道"，不是白皮书描述的 native CLI binding。差别：

- Native CLI binding 应该有 argv 数组构造、结构化 stdout 解析、exit_code → ATD error 映射的专属逻辑
- 当前 host:* 路径把所有 CLI 工具都走 shell.exec，失去了 CLI binding 的结构化优势

## Root Cause

ATD 协议设计时将 CLI binding 定位为 "Rust-native 对接 Agent Native CLI (ANC)" ——对标 `mobile camera`、`anos schema` 等结构化 CLI。但在实装中：

1. Agent Native CLI 生态尚不成熟，没有多少可对接的 ANC 格式工具
2. 实际需求集中在包装通用 Linux 命令（ffmpeg / yt-dlp / jq 等），这些走 host:* JSON 模板更自然
3. 两条路径没有统一——导致 "CLI binding" 的语义悬空

## Current state

- ✅ `host:*` 插件系统工作良好（10 个 bundled 插件）
- ❌ `binding_cli.rs` 不存在
- ❌ ATD schema 中的 `bindings.cli` 字段在运行时没有专属 handler

## Fix

两种方案：

**方案 A（推荐）：实装 native CLI binding**

- 新建 `crates/anos-tool-dispatch/src/binding_cli.rs`
- 处理结构化 CLI（ANC）格式：JSON 输入 → argv → 进程 exec → JSON 输出解析
- host:* 保持独立模块，作为"非结构化 CLI 模板包装器"（是 shell.exec 的扩展，不是 CLI binding）
- ATD schema 的 `bindings.cli` 和 `bindings.host_plugin` 明确分开

**方案 B：重新定义 CLI binding 语义**

- 白皮书和架构文档更新：将 host:* 插件正式定义为 CLI binding 的实现
- ATD schema 保留 `bindings.cli`，内部实现为 host:* 路径
- 风险：弱化"4 协议统一"叙事——CLI binding 只是 shell.exec 的美化

## Validation

- `crates/anos-tool-dispatch/src/binding_cli.rs` 存在
- 至少 1 个示例工具使用 native CLI binding（而非 host:*）通过端到端测试
- 文档 `docs/architecture/atd-overview.md` 明确区分 CLI binding vs host:* plugin

## Priority

P1 — 对白皮书叙事诚信度影响大。如果暂时不实装 native binding，至少应同步更新文档以消除"已实装 4 binding"的暗示。
