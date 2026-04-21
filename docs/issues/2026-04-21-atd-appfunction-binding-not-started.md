# ATD AppFunction binding not implemented — 移动端 4-binding 叙事缺失关键支柱

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** MEDIUM — 非立即阻塞，但白皮书移动端示例（§8 "If you're a mobile developer"）实际无法执行
**Component:** `crates/anos-tool-dispatch`
**Related:** ATD 白皮书 v2 §2.2, §8 移动开发者章节; `docs/architecture/atd-overview.md §4`

## Symptom

ATD 白皮书 v2 §8 向移动应用开发者承诺：

> 一份 ATD 定义 → 三个平台原生实现
> iOS (App Intents) · Android (AppFunctions) · HarmonyOS (Intents Kit)

并展示了完整的 ATD tool definition 用 `bindings.appfunction` 声明三平台 target，说"ATD Dispatch 层根据 platform + 可用性自动选"。

但 `crates/anos-tool-dispatch/src/` 中 **`binding_appfunction.rs` 不存在**。ATD schema 中的 `bindings.appfunction` 字段在运行时**完全未处理**——如果某个 ATD definition 只声明了 AppFunction binding 没有 REST/CLI，该工具在 ANOS runtime 上**无法被调用**。

白皮书标注 "(Phase 2 设计完成)" 给读者的印象是"已有设计、即将实装"。实际情况：Phase 2 对应的 binding 代码 0 行。

## Root Cause

AppFunction binding 需要的实装工作量远大于其他 binding：

- 每个 platform（Android AppFunctions / iOS App Intents / HarmonyOS Intents Kit）都有独立的 IPC 机制（Binder / XPC / HiLink）
- 需要跨进程、跨语言（Rust → Kotlin/Swift/ArkTS）桥接
- 需要平台专属的权限模型映射（Android permissions / iOS entitlements / HarmonyOS Capabilities）
- ANOS 当前主要部署场景是服务器/桌面 Linux，移动端集成缺乏优先级驱动

## Current state

- ✅ ATD schema `bindings.appfunction` 字段已设计（在 `anos-types` 中定义）
- ✅ 白皮书和架构文档给出完整 target 结构（platform / package / class / function）
- ❌ Rust 实装 0 行
- ❌ Platform-side 桥接 Library 未启动（no Kotlin/Swift/ArkTS side crate）
- ❌ 端到端示例未验证

## Fix

阶段化实装（建议优先级从高到低）：

**Phase 2a - Android AppFunctions binding**

- 新建 `crates/anos-tool-dispatch-appfunction-android/`（或作为 `binding_appfunction.rs` 的 android feature）
- 通过 JNI / JNA 桥接 Android `AppFunctionService`
- 参考实现：启动 ANOS daemon 作为 Android service，接受 `AppSearch` 索引注册

**Phase 2b - iOS App Intents binding**

- 设计上 ANOS runtime 可能需要包装为 iOS App Extension
- 更复杂：iOS 进程模型限制大

**Phase 2c - HarmonyOS Intents Kit binding**

- 华为生态，如果有合作则优先

## Validation

- 至少 Phase 2a（Android）完成后：
  - 一份 ATD tool definition 只声明 appfunction binding (Android)
  - ANOS 在 Android 上能正确 dispatch 到 AppFunction
  - 白皮书 §8 的 Lily "米家开灯" 示例能真实端到端跑通

## Priority

P2 — 白皮书已明确标 "Phase 2"，读者对此有预期。但应避免在 marketing 语境下暗示"即将发布"。

## 替代措辞建议

短期（不做实装）：
- 白皮书 §8 章节头加警告："AppFunction binding 实装在 Phase 2；当前章节的 Android/iOS 示例为设计规范，未经端到端验证"
- 白皮书 §3.3 的 "4 binding 已实现" 改为 "2 binding native 实装 (MCP, REST) + 2 binding 设计完成 (CLI, AppFunction)"
