# Why ATD for HMS Core

**Date:** 2026-04-21
**Type:** Strategic briefing
**Audience:** HMS BU decision makers, HIMA consortium partners, HarmonyOS ecosystem leads
**Companion technical documents:**
- [ATD v3 whitepaper](toward-agent-tool-dispatch-v3.md) (protocol)
- [ATD for HMS Core — Applied Design](atd-for-hms-core-design.md) (concrete integration)

---

## Executive Summary

AI agent 生态在 2025-2026 爆发。但生态被**四个维度的碎片化**撕裂——不同的 OS、不同的 Agent 框架、不同的厂商规范、不同的开发者技术栈。对 HMS Core 而言，这种碎片化**放大为独特的战略挑战**：

HMS Core 是**业界唯一**覆盖完整 7 类终端设备（phone / watch / earbuds / tablet / pc / car / tv）的 kit 体系。这是华为的结构性优势——但也是独特压力：现有的 agent 协议（MCP / OpenAI Functions / LangChain）没有一个能表达 "同一能力在 watch 和 phone 上走不同 binding" 的场景。结果：

- **外部 agent 框架（OpenAI / Anthropic / Claude）用不了 HMS on-device kit**——只能调 HMS 云 REST，覆盖率 ~30%
- **HMS 开发者要为每设备类写独立集成**——手表 vs 手机 vs 车机互不认识
- **Lily 场景（"手表测异常 → 手机规划 → 车机执行 → 耳机通知"）今天写不出来**——没有协议层的 session handoff / device affinity

**ATD（Agent Tool Dispatch）是为此而生的中立协议**。v3 引入的 multi-device 原语——device affinity、distributed sessions、driving_constraint、result middleware——**精确对应 HMS Core 的结构性需求**。

本报告提出：**HMS Core 采纳 ATD 是 Huawei 生态在 agent 时代的战略关键步骤**，并非 HMS BU 独力而为，而是在 APWG（Agent Protocol Working Group）多方协作中**以 HMS 场景为 reference** 推进 agent 工具层的行业标准。

**具体结论**：
- ATD 能让 Huawei 的结构性优势（全 7 设备类）从"硬件事实"升级为"软件可编程能力"
- ATD 能让外部 agent 生态免费为 HMS 开发适配——不需要 Huawei 单独推动
- 时间窗口：HarmonyOS PC 2026-04 发布、HarmonySpace 6 / ADS 5.0 2026-04 发布、2026 年 Q3-Q4 是跨生态标准化的最后时机

本文档提供的不是"加入承诺"，是决策所需的**事实基础 + 风险评估**。

---

## §1. 华为生态的独特优势与独特压力

### 1.1 全 7 设备类覆盖是独特的战略资产

2026 年 Q2，华为（含 HIMA 联盟）是**全球唯一**同时出货下列 7 类终端的厂商：

| 设备类 | Huawei 代表产品 | 市场地位 |
|-------|---------------|---------|
| Phone | Mate 80 / Pura 80 | 中国市场 top 3 |
| Watch | Watch 4 / GT 系列 | 中国 smartwatch 出货量 #1 |
| Earbuds | FreeBuds Pro 5 | 中国 TWS 出货量 top 3 |
| Tablet | MatePad Pro | 中国平板 top 2 |
| PC | MateBook（HarmonyOS PC 2026-04 新发）| 中国 laptop 出货 top 3 |
| Car HMI | Aito M7/M9 / Luxeed / Seres（HIMA）| 中国新能源车高端市场迅速成长 |
| TV | Vision | 中国智慧屏 top 5 |

此外 HiLink / 全屋智能覆盖了 smart_home_hub 类别。

竞争对手里：
- **Apple** 缺 car（CarPlay 不算）
- **Google / Android** 设备由多 OEM 分散（没有任何单一 OEM 覆盖 7 类）
- **小米** 虽覆盖多类，但没有统一 OS
- **三星** 缺 car 和统一 OS

华为独有的**"一个 OS 跨 7 设备类"**地位（HarmonyOS）本应是**在 agent 时代的战略性软件资产**——agent 跨设备流动时，HarmonyOS 应该是最顺滑的生态。

### 1.2 但 agent 时代没有现成的表达方式

问题在协议层。2026 现有的 agent 工具协议（MCP、OpenAI Functions、LangChain tools、Anthropic Agent Skills）都是**单设备语义**——它们假设 tool 调用发生在 "同一台机器"。

示例：一个"查心率"的 tool——

- MCP 定义：JSON-RPC over stdio，要求 tool server 和 agent 在同一主机；**不能跨设备**
- OpenAI Functions 定义：function schema + HTTP endpoint；**只支持 REST**
- LangChain Tool：Python 类；**只在 Python agent 里有用**
- Anthropic SKILL.md：内容是 markdown + YAML frontmatter；**不规范 tool dispatch**

结果：HMS HealthKit 的丰富能力——**watch 原生读心率 + phone 从 watch 同步 + 云端同步备份——这三套实装 agent 协议层全部无法区分表达**。

### 1.3 今天 HMS 的 agent 接入现状

我们调研 2026 Q2 的实际状态：

- **OpenAI / Anthropic / Claude Code / Cursor** 的 agent 只能通过 HMS 云 REST 访问 HMS（如 Push、Site、Health Cloud），覆盖 ~30% HMS Core 能力
- 要用 watch 的心率传感器，开发者必须写独立 Android/HarmonyOS 原生 SDK 集成，**agent 看不到**
- HarmonyOS 5 NEXT 的 Super Device / DSoftBus 能力**对外部 agent 生态完全封闭**——它是华为内部特性，外部 agent 框架不识别
- HIMA 联盟车机的 ADS 5.0 API、HarmonySpace 6、Aito/Luxeed/Seres 的各自能力——**在 Lily 场景中 agent 看不见**

**这不是 HMS 的错，也不是 agent 框架的错——是生态层面缺一层协议**。

---

## §2. 不用 ATD 的五种失败场景

以 Lily 的"心率异常 → 医院路线规划"场景为例（详见 [应用设计文档 §5.1-§5.3](atd-for-hms-core-design.md)）。今天五种方案都失败：

### 失败 1：纯云 REST

所有 HMS kit 通过云 REST 暴露给 agent。

**结果**：~30% 覆盖。watch 的实时心率、phone 的 fused location、车机 CAN bus、earbuds 的 head tracking——**全部读不到**。而且云 REST 的延迟（100-500ms）对驾驶场景不可接受。

### 失败 2：只做 Android / HarmonyOS SDK

放弃跨平台，让每个 agent 厂商自己写 Android / HarmonyOS 客户端。

**结果**：OpenAI、Anthropic、Google 都不会为 HMS 单独做。HMS 的用户用这些 agent 时**HMS 能力相当于不存在**。

### 失败 3：Huawei 自家 agent（"Celia" / AI Agent）独占 HMS

华为做一个自家 agent，绑定 HMS，忽略外部 agent 生态。

**结果**：市场隔离。用户如果用 Claude 或 GPT，就不能用 HMS 工具；用 HMS 工具就要离开熟悉的 agent。**用户用脚投票离开任一边**。过去 App Store vs AppGallery 的困境在 agent 时代放大。

### 失败 4：为每个 agent 厂商做 adapter

Huawei 出动工程师，为 OpenAI 做 OpenAI Functions adapter，为 Anthropic 做 MCP server，为 LangChain 做 Python 包——N × M 成本。

**结果**：工程不可持续。2026 年已有 10+ 主要 agent 框架，2027 还会出现更多。每出现一个 Huawei 就要做一次适配。**成本无限增长**。

### 失败 5：今天的 Skill 生态（SKILL.md / agentskills.io）

2025-12 Anthropic 开源 SKILL.md 标准，26+ 平台采纳。是不是让 HMS 写 Skill 就够了？

**结果**：Skill 是**剧本**层，ATD 是**工具层**（见 [ATD v3 §2.4](toward-agent-tool-dispatch-v3.md)）。Skill 里写 "第 3 步调 xiaomi:light.toggle" 那个调用**还是要 ATD 层执行**。SKILL.md 规范本身无意填工具层的空。

---

## §3. ATD v3 为 HMS 提供了什么

[ATD v3 whitepaper](toward-agent-tool-dispatch-v3.md) §2.5-§2.8 的四个新原语，**每一个都解决一个 HMS-specific 问题**：

### 3.1 Device Affinity 解决 "同一能力不同 binding"

```yaml
id: hms:health.heart_rate.get
device:
  preferred: [watch]         # 手表原生读，精度高
  fallback: [phone]          # 手机读同步数据
bindings:
  appfunction:
    - device_type: watch, vendor: huawei, kit: wear_engine, ...    # Huawei 独占
    - device_type: watch, vendor: apple, framework: HealthKit, ... # 允许跨 vendor
  rest:
    url: "https://health-api.cloud.huawei.com/..."  # Cloud fallback
```

Agent dispatch 自动按运行 platform 选 binding。Huawei 开发者**维护一份 tool definition**，跨 Apple/Google 的 fallback 也自动工作——ATD 的 neutrality 在此实现。

### 3.2 Distributed Sessions 解决 "跨设备 agent 流动"

HarmonyOS 5 NEXT 的 DSoftBus 原本是华为内部特性，外部 agent 看不见。v3 把 DSoftBus 封装为 transport_hint：

```yaml
distributed:
  shareable: true
  transport_hint: [harmonyos_super_device, apple_handoff, generic_rpc]
```

外部 agent 框架（LangChain、Claude Code、OpenAI Assistants）统一用 `session.migrate(target_device)` —— Dispatch 层根据设备能力选具体 transport。Huawei 设备之间自动走 DSoftBus（最快），跨 vendor 走 generic_rpc 兜底。

**华为的分布式能力从"自家封闭优势"升级为"外部 agent 可消费的能力"**——这是 ATD 对 Huawei 最有战略价值的部分。

### 3.3 driving_constraint 解决 "汽车驾驶安全 + agent"

```yaml
id: car.navigation.route_to
safety:
  level: write
  driving_constraint: safe_always     # 或 requires_parked / passenger_only
```

Dispatch 层在调用前查车辆 state，不符合约束的调用**直接拒绝**（返回 DrivingSafetyBlocked）。这对 HIMA 联盟车——Aito / Luxeed / Seres / 未来 OEM——是**核心卖点**：

- 第三方 agent 接 HarmonySpace 6 有安全护栏，不用 HMS 团队逐个 audit
- ADS 5.0 的"驾驶状态"天然对接 driving_constraint，无需额外接口
- HIMA 联盟 OEM 可以独立扩展自己的 driving_constraint 语义，不破坏协议

### 3.4 Result Middleware 解决 "agent 调 HMS 结果的隐私/安全"

HMS 的 health / location 数据高度敏感。ATD v3 的 middleware 管道：

```yaml
result_middleware:
  - type: pii_redact
    fields: [source_device_id, user_phone, ssn]
    mode: transform
  - type: prompt_injection_scan
    mode: warn
```

**每次 agent 调 HMS tool，结果进 LLM 上下文前都经过 PII 清洗和 prompt injection 扫描**。这对 HMS 的合规意义：

- **PIPL（个人信息保护法）**：敏感字段默认 redact，符合最小必要原则
- **GDPR（HMS Global 场景）**：审计日志记录所有 PII 访问，符合 right-to-know
- **企业 agent 场景**：HMS 企业客户可以信任 agent 不会泄漏数据

**没有任何其他 agent 协议有这个**。MCP / OpenAI Functions / LangChain 都不在协议层处理敏感数据——全靠 agent 开发者自觉。ATD v3 把它变成**协议级保证**。

---

## §4. 与 HIMA / HarmonyOS Cockpit 的契合

### 4.1 HIMA 的战略契机

**2026-04** 是关键节点：HarmonySpace 6 + ADS 5.0 发布，Aito M9 出货，HIMA 联盟扩展。这是华为智能汽车战略的关键里程碑。

但 HIMA 面临一个结构性挑战：**每个 HIMA 成员 OEM（Aito / Luxeed / Seres / Maextro）都想自己做 agent 差异化**。没有统一协议，这会导致 HIMA 内部出现 N 套 agent tool schema，分裂度接近 Android 碎片化。

ATD 可以作为 **HIMA 统一 agent tool 协议**：

- 每家 OEM 提供自己的 ATD tool definition（car-specific feature）
- 共享的是 binding 接口（HarmonySpace 6 + ADS 5.0）
- Agent 不感知具体 OEM，但 OEM 可以在 tool 层面差异化

这是"**合作 on 协议，竞争 on 实装**"——对 HIMA 长期健康最友好。

### 4.2 HarmonyOS Cockpit API 已开放但无聚合

Huawei 已经对外开放了 HarmonyOS Cockpit API（[huaweicentral.com/hms-core/](https://www.huaweicentral.com/hms-core/)），让 OEM 和 ecosystem partners 可以接入。但今天的开放是**点到点**——每个 partner 自己 integrate。

ATD 作为聚合协议，可以让 **Cockpit API 的每个能力自动成为 agent 可调 tool**。Huawei 的开放投入不变，但 ROI 放大数倍——因为每个 agent framework 都能调。

### 4.3 车-家联动场景的协议支撑

2026 年 HarmonyOS 生态正在推的"车-家联动"——"回家路上车让家里灯/空调启动" / "出门车预热座椅并推送堵车提醒"——这些场景**横跨 Car HMI + smart_home_hub 两设备类**。

v3 的 distributed sessions + device affinity **正好支持**：

```yaml
session.migrate(from_device=car, to_device=home_hub)
# 车机到家门时自动迁移 agent session 到家里的 HiLink 中枢
```

没有 v3 这种场景必须靠 HMS 内部定制。有了 v3，外部 agent（任何用户选的 agent）都能 orchestrate 这个场景。

---

## §5. 采纳路径

核心原则：**Huawei 不用独立承担全部工程**。ATD 是 APWG 协作标准，Huawei 的角色是"**HMS 场景的 reference 实装贡献者**"，而不是"protocol owner"。

### 5.1 Phase 0 — 2026 Q2/Q3（现在）

**投入**：1-2 个工程师 quarter

**交付**：
- HMS 云 REST binding 参考实装（5 flagship kit：Push / Site / Drive / Health Cloud / ML Cloud）
- 参加 APWG 第一次 working group call（Q3 2026 计划）
- 在 GitHub atd-protocol 组织里贡献 HMS binding demo

**收益**：
- 外部 agent（OpenAI / Anthropic / LangChain）立即可调 HMS 云 API
- HMS Cloud API 的调用量增加（跨生态的 agent 都能接，不再局限华为 agent）
- 建立 HMS 在 agent 协议标准化中的 credible voice

**风险**：极低。REST binding 是成熟技术，HMS 云 API 已有，只是包装。

### 5.2 Phase 1 — 2026 Q4

**投入**：3-5 个工程师 quarter

**交付**：
- HarmonyOS 5 NEXT AppFunction binding SDK（覆盖 Health / Location / ML / Awareness 等端侧 kit）
- Android binding SDK（HMS on Android）
- Mate 80 / Pura 80 上的完整 Lily §5.1 场景 demo

**收益**：
- HMS on-device 能力对外部 agent 可见（covered ~60% kit）
- Huawei phone 用户在任意 agent 下都能用全部 HMS 能力
- 建立"HarmonyOS = agent-friendly OS" 品牌

**风险**：中等。需要和 HarmonyOS 开发者关系 + AppGallery 审核团队协作，审核标准可能需要更新。

### 5.3 Phase 2 — 2027 Q1-Q2

**投入**：5-10 个工程师 quarter

**交付**：
- Wear Engine binding（Watch 4+）
- HarmonySpace 6 binding（Aito M9 / Luxeed 等 HIMA 全员）
- FreeBuds Pro 5 audio control plane binding
- DSoftBus distributed session transport 的 ATD 实装（v3 §2.6）
- 完整 Lily §5.1-§5.3 跨 5 设备 demo

**收益**：
- Huawei 生态在 agent 时代的结构性优势**落实为软件能力**
- HIMA 联盟 OEM 获得统一 agent 协议支持，加速新 OEM 加入
- 成为 APWG reference implementation，影响协议演化方向

**风险**：中高。需多部门协作（HMS BU、HarmonyOS BU、智能汽车解决方案 BU、HIMA 管理办公室）。建议 Phase 2 启动前做 Phase 0/1 成果 review，根据实际 adoption 再确认投入规模。

### 5.4 Phase 3+ — 2027 下半年至之后

- TV / PC / smart_home_hub binding
- 贡献到 W3C / IETF / LF AI 的标准化（APWG 协议升格为国际标准）
- HMS 作为 ATD 最深度 reference impl，参与协议 v4 设计

---

## §6. 为什么"现在"是关键时间窗口

### 6.1 协议标准化有明确窗口期

历史规律：基础设施协议的标准化窗口通常 **3-5 年**。过了就固化：
- TCP/IP 1981 RFC，1990 年代前标准化完成；后续者只能在之上叠加
- POSIX 1988，1990 年前后主要 OS 都 comply；后来者若偏离付出兼容成本
- SQL 1986 ANSI，1990 年代稳定；今天所有 DB 都 SQL-compatible
- MCP 2024-11 发布，1 年内 Claude / Cursor / GitHub / Atlassian 等采纳——但 MCP 是**单设备协议**，无法成为 agent tool 的 POSIX

Agent tool 协议的标准化窗口正在打开：2024-11 到 2027 年前后决定谁是主导者。**2026 年是决定性年份**。

### 6.2 Huawei 的具体时间窗口

HarmonyOS 5 / HarmonyOS NEXT 于 2024 发布，2026 进入成熟期。2026 的三个关键事件让此时成为**Huawei 影响 agent 协议走向的独特窗口**：

- **2026-04 HarmonyOS PC 发布**：新增 pc 设备类；ATD v3 §8.5 PC developer guide 首次把 HarmonyOS PC 作为 reference
- **2026-04 HarmonySpace 6 + ADS 5.0 发布**：为 v3 §8.6 driving_constraint 提供现实的 state-query API；HIMA 扩展到更多 OEM
- **2026 Q3-Q4 APWG 形成**：ATD 协议治理机构成立；此时加入可获得治理席位，之后是加入已有组织

**如果 2026 Q3 前 Huawei 没对 agent 工具协议层表态**，MCP + agentskills.io 可能在非 Huawei 生态里形成事实标准（即使它们本质不能表达 HMS 多设备场景），后续 Huawei 接入成本会显著升高。

### 6.3 战略不对称性

ATD 的采纳对 HMS 有**不对称收益**：

| 维度 | 采纳 ATD | 不采纳 |
|------|---------|-------|
| 外部 agent 调 HMS 云 kit | 所有 agent 都能调 | 每个 agent 单独对接 |
| 外部 agent 调 HMS 端 kit | Phase 1/2 后可用 | 永远做不到（协议层不存在） |
| Lily 多设备场景 | v3 协议直接支持 | HMS 内部定制，外部看不见 |
| HIMA 联盟 OEM 加入 | 统一协议低门槛 | 每家 OEM 单独 integrate |
| 车-家联动 | 协议层支撑 | HMS 内部定制 |
| HarmonyOS PC 生态冷启动 | 立即有 agent 工具生态 | 从零教育开发者 |

---

## §7. 风险与反对意见

### 7.1 "ATD 是个新协议，凭什么信它"

有效反对。缓解：
- v2 / v3 whitepaper 公开草案，治理走 APWG 多方 rough consensus
- Phase 0 投入低（2 工程师 1 Q），试水 cost 有限
- 即使 ATD 最终不成主流，Phase 0 的 HMS REST binding 本身也是有价值的产物（就是 OpenAPI wrapper）

### 7.2 "我们已有 Celia / HMS 自家 agent"

不冲突。Celia 是 agent 产品，ATD 是工具协议。Celia 同样可以 ATD compliant，**不妨碍** Celia 的差异化（体验、人格、垂直能力）。ATD 是底座不是天花板。

### 7.3 "中美技术脱钩，ATD 会不会是美国协议"

v2 whitepaper 明确声明 neutral protocol。治理机构 APWG 的成员组成直接反映。Huawei 采纳不是跟随美国，是参与全球标准定义——Huawei 是 APWG 潜在创始成员之一（中美技术标准竞合中的 Huawei 参与有独立价值）。

### 7.4 "HarmonyOS NEXT 开放 DSoftBus 给外部 agent，会不会损失独占优势"

不会。DSoftBus 作为 transport 被外部 agent 用，Huawei 依然是 DSoftBus 的实装方。这像 "TCP/IP 公开后思科依然卖最好的路由器"——**协议开放反而放大实装者的市场**。封闭的 DSoftBus 只服务 Huawei 生态；开放的 DSoftBus 服务全生态，Huawei 硬件受益（因为只有 Huawei 设备完整支持）。

### 7.5 "工程投入风险"

合理顾虑。缓解：
- Phase 0/1/2 是逐级 commit，每阶段有独立 go/no-go
- Phase 0 投入极低（2 quarter × 2 工程师）
- 每阶段产出独立有价值，不是"all or nothing"
- Phase 0 成果（REST binding）即使 ATD 失败也可作为 OpenAPI gateway 复用

---

## §8. 具体请求

### 8.1 Phase 0 试点（2026 Q2-Q3）

请求 HMS BU 指定 1-2 名工程师（合适人选：熟悉 HMS Cloud API + Rust 或 Go），和 ATD 项目方（ANOS / atd-protocol 筹备组）合作：

1. 选取 1 个 flagship kit（推荐 Push Kit，最简单）做 ATD REST binding 参考实装
2. 发布到 GitHub atd-protocol 组织作为 reference
3. 参加 2026 Q3 的 APWG formation call（Huawei 作为潜在创始成员）
4. Phase 0 完成后 evaluate Phase 1

**投入上限**：2 FTE × 2 quarter = 4 工程师季度

### 8.2 工程协作接口

ATD 项目方提供：
- v3 whitepaper 完整规范
- 应用设计文档（本文档的 technical counterpart）[atd-for-hms-core-design.md](atd-for-hms-core-design.md)
- atd-mvp 独立 repo 作为参考 SDK（/home/nan/proj/atd-mvp/ 初始设计已完成）
- 每周同步会议（时区友好）

### 8.3 决策时间表

- **2026-05 底**：Phase 0 启动与否
- **2026-08 底**：Phase 0 成果 review；决定是否进 Phase 1
- **2027-01**：Phase 1 review；决定是否进 Phase 2

---

## §9. 结语

> Huawei 在 2026 年拥有一个**独特的事实**：全 7 设备类 + 统一 OS + 新兴 AI agent 潮流。
>
> Huawei 在 2026 年面临一个**结构性缺失**：没有一个工具协议能让这些优势跨生态释放。
>
> ATD v3 不是 Huawei 的专有方案——它**恰好是为 HMS Core 这种场景设计的中立协议**。
>
> 采纳 ATD 不是对外部开放的"让步"，而是**把硬件优势转化为跨生态软件影响力的杠杆**。
>
> 时间窗口 2026 Q3 前，之后成本陡增。

---

**文档版本**：v0.1 · 2026-04-21 · 战略决策 briefing
**状态**：非约束性提议，征求 HMS BU / HIMA 相关方反馈
**许可**：CC BY 4.0
**反馈**：`feedback@atd-protocol.org`（筹建中）

**关联文档**：
- 技术基础：[toward-agent-tool-dispatch-v3.md](toward-agent-tool-dispatch-v3.md)
- 应用设计：[atd-for-hms-core-design.md](atd-for-hms-core-design.md)
- v2 whitepaper：[toward-agent-tool-dispatch-v2.md](toward-agent-tool-dispatch-v2.md)
