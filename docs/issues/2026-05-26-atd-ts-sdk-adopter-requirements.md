# atd-ts SDK adopter requirements — oh-cli + HarmonyOS ArkTS ecosystem

**Layer:** adopter (oh-cli) + ecosystem (HarmonyOS ArkTS)
**Status:** ready-for-atd（新需求；论证 atd-ts SDK 该从 post-1.0 提前到 SP 序列）
**Effort:** P0-1 ~3-4 周 TS SDK · P0-2 ~2-3 周 ArkTS 适配 · P1-3 ~6-8 周 NAPI 路径 · P1-4 ~2 周/Kit OH-specific tool 库 · P2 ~ongoing
**Filed:** 2026-05-26
**Adopter repo:** `~/code/oh-cli` （参见 `oh-cli/docs/research/11-mobile-integration-plan.md` §7）
**Adopter contact:** oh-cli maintainer

---

## 1. Summary

oh-cli 是 HarmonyOS / OpenHarmony 设备的 agent-native CLI（已 Phase 11 收官、217 unit tests、HUAWEI Mate X5 真机 e2e 验过）。`oh serve` 启动 ATD vendor server（48 tools，Unix socket），ATD 1.0.0 已 path → crates.io。

`docs/research/11-mobile-integration-plan.md` 把"鸿蒙手机做 oh-cli 遥控前端 + 经 MCP 调远端 daemon"定为 Q2 主线方向（5 个高价值场景，含商业化最直接的 S2 QA 自动化、给华为讲故事最有力的 S3 多设备协同），并提出三条手机 native 路径：

- **路径 A — 经 MCP 调远端**（短期、Phase B 立即可做，依赖 atd-mcp-bridge）
- **路径 B — 等 atd-ts SDK ship 后用**（中期、当前不可控因 post-1.0 roadmap 项未实现）
- **路径 C — atd-runtime 经 NAPI 进 ArkTS App**（长期 6-8w，Phase D 候选）

调研后发现 ATD TypeScript / ArkTS SDK 当前状态是 **post-1.0 roadmap，未实现**（`atd-mvp/docs/quickstart/typescript.md` 明确标 "NOT SHIPPED in 1.0"，当前 TS workaround 是经 atd-mcp-bridge 走 MCP）。OH 调研进一步发现 atd-ts 在 HMOS 6.1 时代有几个独占优势位（详见 §6），是 oh-cli 之外 HarmonyOS ArkTS 生态也强需要的基础设施。

本 issue 从 **oh-cli adopter 视角 + HMOS ArkTS 生态视角** 双轴论证 atd-ts SDK 该提前到 SP 序列，并提出 4 阶段 API shape + scope 提案。

oh-cli 在 atd-ts ship 之前会用 Phase B PoC 的 minimal ArkTS MCP client 兜过去；ship 之后切换到上游、删 shim。

---

> ### 📝 订正（2026-05-26 当日，atd-mcp-bridge 源码核查后）
>
> **本 issue 把 oh-cli 标为 "P0 adopter" 有夸大**。源码核查 `crates/atd-mcp-bridge/src/bridge.rs:handle_tools_list`（暴露给 MCP 的 `Tool` 结构仅 `name / description / input_schema`）证实 atd-mcp-bridge 是 **lossy 降级映射**——ATD 的 `tier / safety.level / capability.granted / output_schema / dry_run / NDJSON streaming` 等特性投到 MCP 时**全部丢弃**。
>
> 但**对 oh-cli 实际定位无影响**：
> - oh-cli 全部场景（远端 daemon / Phase A `oh mcp serve` / Phase B 手机 PoC / Phase C 多设备 / Phase D §7.3 路径 C NAPI）**都不需要 atd-ts / atd-arkts**：手机端 MCP 客户端（小艺 Claw / Claude Code / DeepSeek V4 Pro）**本来就只懂 MCP**，给它们 ATD 全能力它们也用不上。
> - oh-cli 是 **atd-rs adopter（已 done，1.0.0 from crates.io）**，**不是 atd-ts adopter**。
>
> **本 issue 真正 driver 不是 oh-cli，是 ecosystem opportunity**：HarmonyOS 6.1 上"ArkTS in-process MCP server 没有官方 SDK"（详见 §4.2 + §6.2 论据 2）是个 OH 生态级空白，**等真正的 ArkTS agent runtime adopter 找上门**（类比 cbrain 之于 atd_server Python runtime；类比 cbrain P2-10 "wait for second adopter" 的纪律）再启动 SP 比抢跑更稳。
>
> **本 issue §3 / §5 / §6 的 P0-P2 排序保留作为 design reference**——SP 启动时直接复用，不必从头设计；但**优先级不该按当前 §3 P0-1 / P0-2 字面读**，建议视具名 ArkTS adopter 出现节奏调整。Status 暂不改（仍 `ready-for-atd`），但读者请把本订正作为入口。

---

## 2. Current ATD state（oh-cli + ArkTS ecosystem 视角）

✅ 已就绪（atd-ts ecosystem 落地的基础）：
- atd-protocol Rust crate（wire / type system）—— `crates/atd-protocol`
- `/atd-protocol-schema.json` —— machine-readable schema，SP-protocol-schema 已 ship
- atd-mcp-bridge —— Rust → MCP server adapter，ship 1.0
- atd_server Python runtime —— P0-1 ship 2026-05-19（cbrain SP-server-py-v1）
- atd-runtime Rust crate —— oh-cli `oh serve` 已用其注册 48 tools 跑通端到端

✅ HarmonyOS 6.1 上下文（调研产出，详见 §6 + [VERIFY] 待核）：
- **Agent Framework Kit** 已 GA（`@kit.AgentFrameworkKit`，含 LLM / 工作流 / A2A / OpenClaw 4 种 agent 编排模式）
- **Intents Kit** 是声明式 app 自描述（`insight_intent.json`），是 OH 独有的"app 可被系统/小艺/外部 agent 标准发现"机制
- **MCP** 列为 HMOS 6.1 三类插件（端 / 云 / MCP）之一
- **ohos-rs**（napi-rs OH fork）2026-05-12 ship `ohrs@1.2.0`，Rust core 经 NAPI 进 ArkTS app 已是生产可用路径
- HTTP / WebSocket / SSE 在 ArkTS 是 day-1 stdlib（`@kit.RemoteCommunicationKit` / `@ohos.net.webSocket`）

❌ 缺（oh-cli + HMOS adopter 视角的阻塞项）：
- **atd-ts client SDK**（npm `@atd-protocol/client`）—— 完全未实现
- **atd-arkts 适配层** —— ArkTS 是 TS 严格子集，**不能直接 reuse Node TS SDK**；完全未实现
- **atd-arkts-native**（atd-runtime 经 ohos-rs NAPI 进 ArkTS app）—— 完全未实现
- **ArkTS in-process MCP server 的官方 SDK** —— HMOS 6.1 三类插件里 MCP 只支"小艺消费外部 MCP server"，"ArkTS 写 MCP server 上架"**没有 first-class SDK**。**atd-ts 是天然填补者**。

---

## 3. Required gaps（按 oh-cli adopter 优先级）

### P0 · 阻塞 oh-cli Phase B+ 主线路径

#### P0-1. **atd-ts client SDK**（npm `@atd-protocol/client`）

**Gap**：atd 当前只有 Rust + Python client SDK。任何 TypeScript / Node.js adopter（含 oh-cli 之外的潜在 adopter）都没官方包可用，必须用 `atd-mcp-bridge` workaround。

**Required API**（建议形态）：

```typescript
// npm: @atd-protocol/client
import { AtdClient, ToolDefinition, ToolSuccess, ToolFailure } from "@atd-protocol/client";

const client = await AtdClient.connect({
  transport: "wss://oh-cli-daemon.example.com",  // 也支 "stdio" / "unix"
  auth: { type: "bearer", token: "..." },
});

// 协议层接口（与 atd-protocol wire format 1:1）
const tools: ToolDefinition[] = await client.toolList();
const schema: object = await client.toolSchema("oh:device.list");
const result: ToolSuccess | ToolFailure = await client.runTool("oh:device.list", {});
const reply = await client.ping();

// session lifecycle（与 Rust / Python SDK 对齐）
const hello = await client.hello({ clientName: "my-app", grantedCapabilities: [...] });
await client.close();
```

**Scope**：
- Wire format codec（4-byte BE length + UTF-8 JSON ≤ 10 MiB，与 `crates/atd-protocol` byte-compat）
- 所有 protocol message types（ping / hello / tool_list / tool_schema / run_tool + responses）
- Transports：stdio（Node.js child_process）、unix-socket（Node.js `net`）、WSS（Node.js + browser native）
- Type generation：从 `/atd-protocol-schema.json` **自动生成** TS types（避免手工漂移；推荐 json-schema-to-typescript）
- 错误模型：与 Rust 错误码（1004-1005 / 1010-1013 等，详见 cbrain issue §9.5 spec corrections）对齐
- Tier-aware deadline / capability gate（client side honoring）
- Dry-run helper

**Effort**：~3-4 周（含 npm publish + jest tests + 与 Rust ref-server byte-compat 验证）

**为什么不让 oh-cli 自己写**：① 这是 ATD ecosystem 的基础设施；② 任何 TS adopter 都会遇到（Hermes 的某些 TS 模块、未来的 web 端 agent debug 控制台、未来的浏览器内 agent）。oh-cli 之外至少 1-2 个未来 adopter 会重复造，长期看维护多份漂移。

**oh-cli 临时方案**：Phase B PoC App 走 **路径 A** —— 依赖 atd-mcp-bridge 转 MCP；ArkTS App 内嵌 MCP client + 远端 daemon 跑 atd-mcp-bridge。如果 P0-1 在 Phase B 期间 ship，oh-cli 立即切换。

---

#### P0-2. **atd-arkts adapter**（OHPM `@atd/arkts-client`）

**Gap**：**ArkTS 不是 TypeScript 的超集**。ArkTS 强制 static typing strict mode，禁用 P0-1 大量依赖的 TS 模式：解构变量声明（destructuring）、import assertions、`Reflect` / `Object` 动态方法、运行时类型反射、原型操纵。**P0-1 编出的 npm 包不能直接在 ArkTS 项目里 import**。

**事实根据**：[官方 ArkTS 迁移指南](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides-V13/typescript-to-arkts-migration-guide-V13)；HarmonyOS NEXT 应用要求 `compatibleSdkVersion >= 10` 进 ArkTS strict mode，违规直接编译 error。

**Required**：在 P0-1 基础上做 ArkTS 子集兼容版本，**公开接口形态一致、内部 transport 走 OH Kit**：

```typescript
// OHPM: @atd/arkts-client
import { AtdClient } from "@atd/arkts-client";

// API shape 与 P0-1 一致
const client = await AtdClient.connect({
  transport: "wss://oh-cli-daemon.example.com",
  auth: { token: "..." },
});

// 内部 transport 实现：
// - WSS → @ohos.net.webSocket
// - HTTP/SSE → @kit.RemoteCommunicationKit (RCP)
// - stdio → 不支（ArkTS 沙箱不能 spawn）
```

**Scope**：
- 公开接口与 P0-1 一致（让 oh-cli Phase B PoC App 写代码时形态一致）
- 内部 transport 适配到 OH Kit（`@ohos.net.webSocket` / `@kit.RemoteCommunicationKit`）
- 严格遵 ArkTS 子集（无解构、无 Reflect、无 Object dynamic）
- OHPM publish + ohos-package 配置
- 不实现 P0-1 的 Node-specific 部分（stdio / unix-socket）—— ArkTS 沙箱不允许

**Effort**：~2-3 周（要求 ArkTS 开发环境真机/模拟器验证）

**为什么不让 oh-cli 自己写**：这是 ArkTS ecosystem 的基础设施 —— 任何 HMOS app 想接入 agent 工具调用（不限于 oh-cli adopter）都会遇到。OH 6.1 的 Agent Framework Kit 走小艺生态，**atd-arkts 是非小艺生态的 ArkTS app 接入 agent tool 调用的唯一开放路径**（小艺 plugin 必须实名注册，第三方非小艺 agent 没渠道）。

**oh-cli 临时方案**：Phase B PoC App 用手搓 ArkTS MCP client（`@ohos.net.webSocket` + JSON-RPC 2.0 手实现）。约 200-400 LOC ArkTS，可作为 P0-2 第一版起点贡献回上游。

---

### P1 · 中期重要

#### P1-3. **atd-arkts-native-rust**（OHPM `@atd/arkts-native-rust`，atd-runtime 经 NAPI 进 ArkTS app）

**Gap**：P0-2 是 ArkTS 纯 TS 实现 client。**没有 server side**。如果想让 ArkTS app 自己当 ATD server（把 app 能力暴露成 tool 给系统 agent / 远端 agent 调），需要 atd-runtime 经 NAPI 进 ArkTS app。

**事实根据**：调研发现 OH 6.1 的"端/云/MCP 三类插件"中 MCP 只支"小艺消费外部 MCP server"，**ArkTS in-process MCP server 没有官方 SDK**（详见 §6.2）。atd-arkts-native-rust + atd-mcp-bridge 路径可填这个空白。

**Required**：
- atd-runtime Rust crate 通过 [ohos-rs](https://github.com/ohos-rs/ohos-rs)（napi-rs OH fork，`ohrs@1.2.0` 2026-05-12 ship）包成 ArkTS native module
- ArkTS API（生成的 .d.ts）：
  ```typescript
  // OHPM: @atd/arkts-native-rust
  import { AtdServer, ToolDefinition } from "libatd.so";

  const server = new AtdServer({ serverId: "my-app" });
  server.register({
    definition: { id: "myapp:status.get", /* ... */ } as ToolDefinition,
    handler: async (args: object): Promise<object> => { /* ... */ }
  });
  await server.listen("unix:///data/storage/el2/base/myapp.sock");
  ```
- 编译产物：`libs/arm64-v8a/libatd.so` 等 ABI 全套（参考 [stuartZhang/Arkts-NAPI-Rust-Demo](https://github.com/stuartZhang/Arkts-NAPI-Rust-Demo) 目录结构）
- 跨 ABI 数据交换性能验证（图像 / 大 payload）

**Effort**：~6-8 周（NAPI binding + ABI + ArkTS bridge + 真机测试）

**Value**（关联 oh-cli `docs/research/11` §7.3 列的 6 开发者新能力）：
1. App self-introspect（app 把自身健康/配置暴露给运维 agent 远程查）
2. 华为账号能力代理（agent 经 app 调 Health Kit / Wallet 等用户态权限）
3. Distributed Soft Bus 跨设备桥（app 把 SoftBus 发现的设备注册成 ATD target）
4. 端侧推理 tool（MindSpore Lite 经 NAPI tool 暴露）
5. oncall 反向调用（agent 远端 push tool call 让手机端处理）
6. app 内 UI 自动化 tool（受沙箱约束，仅同 app，[VERIFY] 是否真开放）

**oh-cli 临时方案**：不做这条 —— oh-cli daemon 跑远端机，不需要 in-process server。但 oh-cli 关联场景（如把 oh-cli 的某个 resource 反向暴露给手机 agent 用的场景）可能用到，列为 Phase D 候选。

---

#### P1-4. **HarmonyOS-specific tool integrations**（独立 ATD adopter SP）

**Gap**：OH 系统能力面（Intents Kit / Background Tasks Kit / Distributed Soft Bus / Sensor / MindSpore Lite / RCP）都是天然的 ATD tool 包装目标 —— 但目前没有"OH ATD tool 标准库"。每个 adopter 各包一次会有大量重复。

**Required**：
- atd-arkts-oh-kit-tools（OHPM 包）：标准 wrap 7-9 个核心 Kit 成 ATD tool
  - `oh:intents.discover` / `oh:intents.invoke`（Intents Kit）
  - `oh:bgtask.schedule_reminder` / `oh:bgtask.continuous_start`（Background Tasks Kit）
  - `oh:dsoftbus.discover_devices` / `oh:dsoftbus.start_remote_ability`（Distributed Soft Bus）
  - `oh:sensor.subscribe` / `oh:camera.snapshot`
  - `oh:mindspore.infer`（端侧推理）
- **Intents Kit 自动转译**（OH 独有杠杆）：把 app 的 `insight_intent.json` **声明式自动转译**成 ATD tool schema。开发者声明一次同时给小艺、外部 agent、CI 测试驱动用。**这是 atd-ts 在 OH 上比其他生态更有价值的差异化点**（详见 §6.3）。

**Effort**：~2 周 per 核心 Kit × 5-6 个核心 Kit = ~10 周（可分批，单 Kit 也能独立交付）

**oh-cli 视角**：oh-cli 本身不直接受益（oh-cli 走 hdc 路线，不是 ArkTS app 内嵌路径）。但作为 atd ecosystem 演进，这是吸引第二、第三 OH adopter 的关键。

---

### P2 · 长期需要

#### P2-5. **Browser transport / web 端 ATD client**

**Gap**：P0-1 的 npm 包目标主要是 Node.js；浏览器内 agent debug 控制台 / web 端运维 dashboard 缺 SDK。

**Required**：P0-1 的 WSS transport 兼容浏览器原生 WebSocket（应当天然，但需要 build target 验证 + 浏览器 unit tests）。

**Effort**：~1 周（如果 P0-1 design 就考虑到）。

#### P2-6. **Conformance suite for TS / ArkTS**

**Gap**：参 cbrain P2-7（Python conformance runner）—— TS / ArkTS SDK ship 后也需要类似 conformance 自验工具。

**Required**：`atd-conformance-ts`（npm-installable），调任意 ATD server endpoint 跑全套 fixture。

**Effort**：~1 周（复用 atd-conformance fixtures）。

---

## 4. oh-cli 临时蛮力（与 atd 团队进度并行）

oh-cli `docs/research/11` Phase B PoC App（2-3 周）期间会做以下 minimal 工程，作为后续 atd-ts SDK 的起点 / conformance fixture 输入：

- **Phase B.3** 写一个 minimal ArkTS MCP client（手搓 `@ohos.net.webSocket` + JSON-RPC 2.0）—— 不是 ATD client，是 MCP client 跑通到远端 atd-mcp-bridge。
- **Phase B.5** 端到端验证 S1 demo（手机说"装贪吃蛇到 Mate X5"），录视频。
- 一旦 P0-1 + P0-2 ship，oh-cli **切换**：B.3 的 MCP client 替换为 atd-arkts client，B.4 的 DeepSeek V4 Pro tool calling 中 tool source 从 MCP server URL 改成 ATD server URL，省一跳 bridge。

**协议字节兼容**：手搓 MCP client 严格遵 [MCP spec](https://modelcontextprotocol.io/specification)；atd-arkts ship 后切换到 atd wire（4-byte BE length frame）。

---

## 5. 时间线建议（与 oh-cli + 生态路线对齐）

| atd 需求 | oh-cli 需要它的时间 | atd 现状 | 建议 atd SP |
|---|---|---|---|
| P0-1 atd-ts client SDK | Phase B+ (PoC 后开始切换；硬需 ~5-8 周后) | 无 | **`SP-ts-client-v1`** ~3-4 周 |
| P0-2 atd-arkts adapter | Phase B+ (硬需同 P0-1) | 无 | **`SP-arkts-client-v1`** ~2-3 周（依赖 P0-1） |
| P1-3 atd-arkts-native-rust | Phase D（仅 §7 路径 C 启动时） | 无 | `SP-arkts-native-v1` ~6-8 周（远期）|
| P1-4 OH Kit tool 标准库 | 不阻塞 oh-cli（关注 ecosystem 演进）| 无 | 分批，每 Kit ~2 周 |
| P2-5 Browser transport | 远期 | 无 | 与 P0-1 同 SP（如果 design 就考虑）|
| P2-6 Conformance TS | P0-1 ship 后 | 无 | ~1 周 |

**最关键**：P0-1 + P0-2 应作为 **互依的 SP pair** 一起设计。P0-1 ship 后 P0-2 是 2-3 周补充工作；如果先 ship P0-1 不考虑 ArkTS 子集，会被反复改。

---

## 6. Value proposition — OH 系统能力面（atd-ts 的独占优势位）

### 6.1 OH 调研结论摘要

调研子代理（2026-05-26）梳理 HMOS 6.1 ArkTS 可调系统能力如下：

| OH 系统能力 | ArkTS 接入面 | atd-ts wrap 成本 | 评分 |
|---|---|---|---|
| HTTP / WebSocket / Socket | `@kit.RemoteCommunicationKit` / `@ohos.net.webSocket` | 极低（wire-side 直接可用） | 🟢 |
| **Intents Kit**（`insight_intent.json`） | `@kit.AbilityKit` / `@ohos.app.ability.insightIntent` | 低（**声明 → 自动转译为 ATD tool schema**）| 🟢 **OH 独有杠杆** |
| Background Tasks Kit（含 Agent Reminders） | `@kit.BackgroundTasksKit` | 低 | 🟢 |
| Sensor / Camera / Multimedia | `@kit.SensorServiceKit` 等 | 低 | 🟢 |
| **Distributed Soft Bus** | `@ohos.distributedHardware.deviceManager` | 中（鸿蒙独有的跨设备 tool）| 🟢 **OH 独有杠杆** |
| MindSpore Lite Kit（端侧推理） | `@kit.MindSporeLiteKit` | 低 | 🟢 |
| Agent Framework Kit | `@kit.AgentFrameworkKit` | 中（需小艺平台 agentId） | 🟡 |
| Account / Health / Wallet Kit | `@kit.AccountKit` 等 | 高（OAuth/权限弹窗）| 🟡 |
| Push Kit / 系统文件系统 | 系统 Kit 表面 | 不可（沙箱外）| 🔴 |

### 6.2 关键论据（5 条）

1. **OH 生态在 2026 已确定 agent-first 路线**：Agent Framework Kit + Intents Kit + 三类插件分类（端/云/MCP）都进了 HMOS 6.x，atd-ts 与之天然对齐。
2. **MCP 在 OH 的"上行链路"清晰，"下行链路"是空白**：小艺/OpenClaw 已能消费外部 MCP server，但 **ArkTS in-process MCP server 没有官方 first-class SDK**。**atd-ts SDK 是填这个空白的最佳候选**——尤其加上 atd-mcp-bridge，ArkTS app 跑 atd server 也即跑 MCP server。
3. **wire transport 全部 day-1 可用**：HTTP / WebSocket / SSE 在 ArkTS 是 stdlib 能力 —— atd wire 在 ArkTS 上不需要 polyfill。
4. **Native fast-path 已通**：ohos-rs（napi-rs fork）`ohrs@1.2.0` 2026-05-12 ship，Rust core 可无缝复用（P1-3 路径）。
5. **不能 reuse Node SDK**：ArkTS 是 TS 严格子集 + 没有 Node 标准库 —— **atd-ts on OH 必须是单独的 ArkTS 包**（P0-2 必需，不是 nice-to-have）。

### 6.3 Intents Kit 自动转译（最 OH 独有的卖点）

OH 的 `insight_intent.json` 已经是开发者"声明 app 能力给系统/小艺/搜索/卡片"的标准格式。atd-ts P1-4 可设计成：

```
[ArkTS App: 已有 insight_intent.json]
    ↓ (atd-ts P1-4 工具)
[运行时自动转译为 ATD ToolDefinition[]]
    ↓ (atd server 注册)
[外部 agent（含 oh-cli 等任意 ATD client / MCP via bridge）可发现并调用]
```

→ 开发者**声明一次**，同时供小艺、外部 agent、CI 测试驱动等场景使用。**这是 ATD 在 OH 上比其他平台更有意义的差异化点**——其他平台（iOS App Intents / Android App Actions）也有类似声明式机制，但 OH 是第一个让"app 自描述能力"能直接喂给非自家生态 agent 的（通过 ATD 中立协议）。

### 6.4 [VERIFY] 待 atd 团队/oh-cli 团队核验

调研子代理标注未直接核验的事实：
- `@kit.MCPClient`-style 官方包名是否存在（仅在公开文档曲面外搜不到）
- HarmonyOS 6.1 NAPI 协议级 changelog 详细
- 跨 HAP `dlopen` 沙箱白名单精确规则
- HMS MLKit / HiAI 在 6.x 是否完全被 AI Kit 吸收
- ArkTS 原生 WebRTC 入口（目前只确认 WebView 内可用）

建议 atd 团队接入华为开发者计划核验上述事实，再启动 P0-1 + P0-2 设计 spec。

---

## 7. oh-cli 愿意做的回馈

- **测试用例**：oh-cli 48 ATD tools + 217 unit tests + Mate X5 真机 e2e 可作为 atd-ts SDK conformance fixture base（catalog-driven 测试场景对 atd 社区有价值）。
- **设计反馈**：oh-cli 是第一个 "Rust 写的 multi-tool catalog ATD vendor server" adopter，能给 ts SDK 设计提供 stress test。
- **第一版 P0-2 ArkTS adapter PoC**：Phase B.3 写的 minimal ArkTS MCP client 代码可贡献为 P0-2 第一版起点（约 200-400 LOC ArkTS）。
- **Intents Kit auto-translation 设计反馈**：oh-cli Phase D 如果做 §7.3 路径 C，会真实践 Intents Kit ↔ ATD tool schema 转译，可贡献设计经验。
- **文档**：本 issue 本身是 oh-cli → atd 反馈的开始。oh-cli `docs/research/11` 全文欢迎 atd 团队 review。

---

## 8. 关联文档

**oh-cli 端**：
- `oh-cli/docs/research/10-harmony-host-feasibility.md` —— HarmonyOS 宿主适配可行性（含 §5 与华为 agent 生态对比、§5.7 hdc-mcp 源码级核查）
- `oh-cli/docs/research/11-mobile-integration-plan.md` —— 手机集成开发计划（5 场景 / 端云架构 / Phase A-D / §7 手机 native ATD binding 三路径）
- **`oh-cli/docs/research/12-arkts-napi-capabilities-survey.md` —— OpenHarmony ArkTS / NAPI 系统能力面调研**（本 issue §6 论据的事实基础；含 5 类核心 Kit / NAPI 现状 / ArkTS vs TS 兼容性 / HMOS 6.1 MCP 三类插件 / atd-ts 三形态权衡 / 5 个 [VERIFY] 待核）
- `oh-cli/src/atd_server/` —— oh-cli 现有 ATD vendor server 实现（48 tools 投影自 catalog/oh-catalog.json）
- `oh-cli/Cargo.toml` —— `atd-protocol/runtime/server = "1.0"` 依赖

**atd 端**：
- `atd-mvp/docs/quickstart/typescript.md` —— TS SDK NOT SHIPPED 现状说明
- `atd-mvp/docs/atd-architecture.md` —— ATD 整体架构（含 TypeScript / ArkTS SDK 在 cross-language parity 表中的位置）
- `atd-mvp/crates/atd-protocol/` —— wire format ground truth
- `atd-mvp/crates/atd-mcp-bridge/` —— MCP bridge 现有实现（oh-cli Phase A 计划包成 `oh mcp serve`）
- `atd-mvp/docs/issues/2026-05-19-cbrain-adopter-requirements.md` —— cbrain Python server runtime 提案（本 issue 的体例参照）

**OH 调研依据**：
- ohos-rs：[github.com/ohos-rs/ohos-rs](https://github.com/ohos-rs/ohos-rs)（`ohrs@1.2.0` 2026-05-12 ship）
- ArkTS migration guide：[developer.huawei.com/.../typescript-to-arkts-migration-guide](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides-V13/typescript-to-arkts-migration-guide-V13)
- Agent Framework Kit 实战：[CSDN 小雨青年](https://harmonyosdev.csdn.net/697715ac7c1d88441d8fa817.html)
- 三类插件分类（端/云/MCP）：[CSDN TUNGHU78](https://harmonyosdev.csdn.net/69c3ac5154b52172bc642f95.html)
- HarmonyOS-mcp-server（Python 外控参考）：[github.com/XixianLiang/HarmonyOS-mcp-server](https://github.com/XixianLiang/HarmonyOS-mcp-server)
- napi-rs ArkTS 样板：[stuartZhang/Arkts-NAPI-Rust-Demo](https://github.com/stuartZhang/Arkts-NAPI-Rust-Demo)
- OpenClaw MCP transport（互操作目标）：[docs.openclaw.ai/cli/mcp](https://docs.openclaw.ai/cli/mcp)

---

## 9. Recommended next step（给 atd 团队）

1. atd 团队 ACK 本 issue 并标注每项的 SP 排期（accept / defer / 需求澄清）；
2. **优先 P0-1 + P0-2 作为 SP pair 设计**：先出 `SP-ts-client-v1` design + `SP-arkts-client-v1` design 两份 spec，确保 P0-1 不偏离 ArkTS 子集兼容；
3. **争取 oh-cli Phase A ship 后立即启动 P0-1**：Phase A 把 `oh mcp serve` 包好后（依赖 atd-mcp-bridge，~1 周），oh-cli 是 P0-1 第一个真 adopter；oh-cli Phase B PoC 期间走 atd-mcp-bridge workaround，B 完成后立即切 P0-1 + P0-2；
4. **P1-3 NAPI 路径等 oh-cli Phase D 触发条件**：仅当出现明确"app 自身做 agent tool provider"商业场景才启动；
5. **P1-4 OH Kit 标准库**：可作为吸引第二个 OH adopter 的关键 carrot —— 一旦 P0-1+P0-2 ship，启动 `SP-oh-intents-bridge-v1`（Intents Kit auto-translation，OH 独有杠杆）会比其他 Kit 优先；
6. close criteria：P0-1 + P0-2 都 ship 后整体 close；P1/P2 独立追踪不绑定本 umbrella。

如需 oh-cli 团队提供更多 OH 调研细节或同步会议，请 ping `oh-cli/docs/research/11` 或在本 issue 评论。

---

**Filed by:** oh-cli maintainer, 2026-05-26
