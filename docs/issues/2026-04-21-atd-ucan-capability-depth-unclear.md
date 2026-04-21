# ATD UCAN capability token implementation depth unclear — 需审计验证链路完整性

**Date:** 2026-04-21
**Status:** OPEN
**Severity:** MEDIUM-HIGH — 涉及 P3 (Capability-as-Security) 原则落地
**Component:** `crates/anos-capability`, `crates/anos-tool-dispatch`
**Related:** ATD 白皮书 v2 §10 Security Layer; `docs/architecture/atd-overview.md §6.2-6.4`, `docs/issues/2026-03-28-p3-capability-tokens-not-wired.md`

## Symptom

ATD 白皮书 §10 Security Layer 完整描述了 UCAN-based capability token 验证流程：

1. Parse & 签名验证
2. Expiry 检查
3. Usage count / rate / budget 检查
4. Resource pattern 匹配
5. Safety level 匹配
6. Attenuation chain 验证

但：
- **已存在的 issue `2026-03-28-p3-capability-tokens-not-wired.md`** 早已标注 "capability tokens not wired"——说明 3 月 28 日时验证链路是断的
- 4 月 21 日的当前状态：未审计确认是否已修复
- 白皮书 v2 FIG 17 大字声称 "UCAN capability token 验证 < 1 ms (cached)"——有"实测"含义

如果 2026-03-28 的 issue 仍是 OPEN，白皮书的这个性能数字是虚构的（因为验证链路根本不工作）。

## Root Cause

UCAN 实装需要：

1. **Token 格式**：JWT-like + Ed25519 签名，符合 UCAN 1.0 spec
2. **签名验证**：DID → public key lookup → 验签
3. **Attenuation chain**：proof_chain 验证（child ⊆ parent）
4. **Revocation**：Bloom filter 或 revocation list
5. **集成点**：dispatch 流水线 Step 1 入口拦截

任一环节缺失都会让整个安全叙事崩塌。

## Current state (需审计)

- ✅ `anos-capability` crate 存在（dependency 证据）
- ⚠️ **已知 issue 2026-03-28**：tokens not wired
- ❓ 4 月 21 日当前状态未审计
- ❓ Dispatch 是否真的在 Step 1 调用 token validator
- ❓ Attenuation chain 验证是否完整
- ❓ Revocation 机制是否存在

## Fix

三步：

**Step 1 - 审计当前实装深度**：

读 `crates/anos-capability/src/`，回答：

1. UCAN token 的数据结构完整吗？
2. `verify(token, resource, action) → bool` 函数存在且正确吗？
3. Dispatch 流水线哪一步调用 verify？
4. Attenuation chain 的 proof 验证有吗？
5. Revocation 机制在哪里？

产出：审计报告 `docs/reports/2026-04-*-ucan-audit.md`

**Step 2 - 补齐缺失环节**：

根据审计报告，优先补：
1. Dispatch 集成（让 verify 真的被调用）
2. Attenuation chain 验证（child ≤ parent 的严格检查）
3. Revocation（至少先有基于 expiry 的机制）

**Step 3 - Benchmark**：

- 测 verify 的延迟（cached / uncached）
- 更新或确认白皮书的 "< 1ms (cached)" 数字

## Validation

- 审计报告说明当前实装覆盖 UCAN spec 的 N%
- Dispatch 的 Step 1 测试：未授权 token → PERMISSION_DENIED；有效 token → pass
- Attenuation 测试：child token 超出 parent 范围时验证失败
- 白皮书性能数字 "< 1ms (cached)" 能实测验证或调整为实测值

## Priority

P0 — Capability-as-Security (P3) 是 ANOS 的核心设计原则之一。若链路不完整，所有"安全 agent"的声明都是空的。

## Related

- Precedes: `docs/issues/2026-03-28-p3-capability-tokens-not-wired.md`（此 issue 是对早期 issue 的 follow-up 审计）
