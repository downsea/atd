# Agent Tool Dispatch: A POSIX for the Autonomous Agent Era

# 通用工具调度协议：自主 Agent 时代的 POSIX

---

## Executive Summary

正如 POSIX 定义了程序与操作系统之间的契约，使得跨 Unix 变种的程序可移植成为可能，当前的自主 Agent 生态迫切需要一个定义 agent 与工具（现实世界能力）之间契约的协议标准。**Agent Tool Dispatch (ATD)** 是这样一个协议——它统一了 CLI、MCP、REST、AppFunction 四种异构协议，在三层容量模型下扩展到 10 万级工具，通过 UCAN 能力令牌提供形式化安全保证。

**三大贡献**：

1. **协议统一 (Interoperability)** — 首次将四种协议绑定到单一 schema 抽象下，消除 agent-tool 生态的协议碎片化
2. **规模扩展 (Scalability)** — 提出并证明 Hot/Warm/Cold 三层容量模型是 tool space 在上下文成本维度上的 Pareto 最优结构
3. **安全内建 (Security)** — 将能力授权作为协议层的一等公民，实现 "Visibility = Authorization" 的形式化模型

**关键理论结果**：

- **Tool Dispatch CAP 定理**：任何 agent 工具协议在 Scalability × Security × Interoperability 三者中最多完全满足两个；三者的协同需要分层架构
- **Hot/Warm/Cold 定理**：任何单层容量模型在 N→∞ 时必然退化；三层分级是上下文成本与发现延迟的 Pareto 最优
- **ATD ↔ POSIX 同构**：ToolDefinition ↔ syscall number；Capability Token ↔ file descriptor；Visibility Tier ↔ user/group/other

**读者导航**：

- 学者 → Part I（Formalization + 三个定理）+ Appendix
- 工程师 → Part II（完整协议规范）+ Appendix B/C/D
- 标准制定者 → Part III（治理路线图）+ Appendix A/E

**当前状态**：ATD v1.0 **协议设计**已完成；**参考实装**在 ANOS 项目中处于早期阶段——核心 dispatch、circuit breaker、MCP/REST binding 已可用，native CLI/AppFunction binding、HNSW 语义发现、pipe composition、dry-run dispatch 等高级特性仍在开发。ANOS 尚未公开发布，引用请以此文档为准。本文提议 v2.0 由多利益方通过 Agent Protocol Working Group 共同演化。

---

## 1. 引言：第四次范式跃迁

### 1.1 四次范式跃迁

```
2020s  Scaling Law           →  通用语言模型      (L0 → L1)
2025   Reasoning              →  通用推理器        (L1 → L2)
2026   Agentic Model          →  自主 Agent        (L2 → L3)
NEXT   Tool Dispatch Standard →  可互操作的 agent 生态 (L3 基础设施)
```

前三次是**能力跃迁**（模型本身变强），第四次是**基础设施跃迁**（协议层标准化）。没有这一步，每个 agent 系统都在重新发明工具接口，生态无法汇聚。这与 TCP/IP 之于互联网、POSIX 之于 Unix、SQL 之于数据库具有相同的历史地位——能力层需要基础设施层才能真正成为生态。

### 1.2 当前生态的三个断层

**断层 1：协议碎片化 (Protocol Fragmentation)**

- MCP 只绑定 JSON-RPC → 无法直接调用现有 REST API
- OpenAI Tools 只支持 HTTP → 无法直接调用 CLI 工具
- Apple App Intents 只在 iOS → 无法跨平台
- Android AppFunctions 只在 Android
- LangChain 只是 Python 库

**断层 2：规模危机 (Scale Crisis)**

- 当前所有协议假设 tool 数量 ≤ 100
- 实际生态正在突破 10K，目标 100K+
- 朴素方案：全部装入 context → 30M tokens，不可行
- 按需加载：每次发现延迟 → 无法响应

**断层 3：安全真空 (Security Vacuum)**

- MCP：无原生授权，每个 host 自己做
- OpenAI Tools：无权限模型
- LangChain：完全裸奔
- 结果：自主 agent 成为无管控的安全风险

### 1.3 三个核心挑战定义本书脉络

- **Interoperability** → Part I §3 + Part II §9 (Binding Layer)
- **Scalability** → Part I §4 + Part II §11 (Capacity Layer)
- **Security** → Part I §5 + Part II §10 (Security Layer)

### 1.4 ATD 的定位：Agent 时代的 POSIX

POSIX 不是操作系统，而是"什么是兼容 Unix 的程序"的定义。同理，ATD 不是一个 agent 系统，而是"什么是可互操作的 agent-tool 交互"的定义。

### 1.5 全书结构

- **Part I**: 形式化 + 三个定理（为什么需要 ATD 这样设计）
- **Part II**: 完整协议规范（ATD 是什么）
- **Part III**: 治理与路线图（ATD 如何演化）
- **Appendices**: 合规测试 / 完整 schema / 枚举值 / 错误码 / 迁移指南

---

# Part I — Foundations

## 2. 形式化 Agent Tool Dispatch 问题

### 2.1 为什么需要形式化

Agent tool dispatch 目前是一个**工程领域**——每个系统用不同的术语、不同的抽象、不同的成功标准。要让它成为一个**研究领域**，需要先定义问题本身。这类似 Lamport 为分布式系统提供形式化框架，或 Dijkstra 为并发提供同步原语理论。

### 2.2 基础对象定义

**定义 2.1 (Tool Space)**

```
T = { t₁, t₂, ..., tₙ } 是所有可用工具的集合
每个 tool tᵢ 是一个五元组：
  tᵢ = ⟨id, σ_in, σ_out, β, π⟩
  其中：
    id   : 唯一标识符（如 anos:fs.read）
    σ_in : 输入 schema（JSON Schema）
    σ_out: 输出 schema
    β    : binding 集合 {cli, mcp, rest, app_function, ...}
    π    : 安全策略（visibility, capability, side-effects）
```

**定义 2.2 (Agent Context)**

```
C = ⟨H, K, M⟩
  H : 当前装入 context 的 tool 集合，|H| ≤ C_max
  K : 当前 agent 持有的 capability token 集合
  M : memory（历史调用记录）
```

**定义 2.3 (Dispatch Function)**

```
dispatch : (intent, C, T) → (t ∈ T) × params × result

给定意图 intent 和当前 context C，在 tool space T 中找到合适的工具 t，
构造参数 params，调用并返回 result。
```

### 2.3 Dispatch 正确性的四个条件

一次 dispatch 是**正确的**，当且仅当同时满足：

- **(D1) 意图匹配性 (Intent Match)**：选中的 tool t 的语义覆盖 intent
- **(D2) 参数合法性 (Parameter Validity)**：params 满足 t.σ_in 的约束
- **(D3) 授权合法性 (Authorization Validity)**：存在 k ∈ K 使得 k 授权 t（包括 visibility、rate limit、budget）
- **(D4) 结果有效性 (Result Validity)**：result 满足 t.σ_out 的约束，且通过宪法守卫（secret scan 等）

### 2.4 Tool Space 的三个维度

```
规模维度 (Scale):     N = |T|                              (工具数量)
异构维度 (Heterogeneity): H = |unique(t.β for t ∈ T)|       (binding 种类)
敏感维度 (Sensitivity):   S = |{t ∈ T : t.π.visibility ≥ dangerous}| / N
```

这三个维度对应本书三条主线：
- Scale → §4 Hot/Warm/Cold 定理
- Heterogeneity → §3 CAP 定理
- Sensitivity → §5 POSIX 同构中的 capability 模型

### 2.5 问题形式化陈述

> **Agent Tool Dispatch Problem**
>
> 给定 tool space T with N tools, H bindings, 和 sensitivity ratio S，设计一个协议 P 使得：
>
> - 对任意 agent context C，存在多项式时间算法实现 dispatch
> - 满足 D1-D4 正确性
> - 上下文成本 cost(C, T) ≪ O(N)（规模不可行性避免）
> - 异构 binding 对上层透明（interoperability）
> - capability 授权不可伪造且可委托（security）

### 2.6 已知解的局限

| 系统 | N 上限 | H 支持 | S 处理 | 是否满足形式化要求 |
|------|--------|--------|--------|-------------------|
| MCP | ~100 | 1（JSON-RPC）| 委派给 host | 部分（缺 Scale + Security）|
| OpenAI Tools | ~128 | 1（HTTP）| 无 | 部分（缺 Scale + Heterogeneity + Security）|
| LangChain | ~N | N（Python 绑定）| 无 | 部分（缺 Security）|
| Apple App Intents | 平台内 N | 1（Swift binding）| 平台提供 | 部分（缺 Heterogeneity + cross-platform）|
| **ATD（本文）** | 10⁵+ | 4+ | UCAN + 4 级 visibility | **完整** |

这个表格成为后面三个定理论证的出发点。

---

## 3. Tool Dispatch 的 CAP 定理

### 3.1 定理陈述

> **定理 3.1 (Tool Dispatch CAP Theorem)**
>
> 对于任意 agent-tool dispatch 协议 P，下述三个性质不可能在**同一协议层**同时完全满足：
>
> - **S (Scalability)**：支持 tool space N → ∞ 且上下文成本 cost(C, T) = o(N)
> - **I (Interoperability)**：异构 binding H ≥ 2 且对 agent 透明
> - **C (Capability Security)**：capability 不可伪造、可委托、可形式化验证
>
> 任意协议 P 至多完全满足 S/I/C 三者中的两个。三者的协同需要**分层架构**。

### 3.2 直觉解释

```
S + I 但牺牲 C：  专注于"让很多异构工具被发现和调用"
                 代表：MCP + 它的 server 生态
                 问题：授权完全委派给 host，不可形式化验证

S + C 但牺牲 I：  专注于"让大量工具在统一安全模型下运行"
                 代表：Apple App Intents + iOS 内生态
                 问题：只能 bind 平台原生 API，无法跨协议

I + C 但牺牲 S：  专注于"让异构工具在强安全模型下被调用"
                 代表：精心设计的企业 gRPC + OAuth2 系统
                 问题：tool 数量爆炸时上下文崩溃
```

### 3.3 形式化证明草图

**引理 3.2 (I ⟹ schema 一致性代价)**

若 H 个 binding 对 agent 透明，则每个 tool 必须携带充分的元数据使 dispatch 层能选择 binding。单个 tool 的元数据大小 m ≥ Ω(H)。

**引理 3.3 (C ⟹ capability 元数据代价)**

若 capability 可形式化验证（不可伪造、可委托），则每个 tool 的访问必须携带不可压缩的能力描述，包括 resource pattern、attenuation proof chain、signature。单次 dispatch 的能力元数据大小 k ≥ Ω(log N + |proof_chain|)。

**引理 3.4 (S ⟹ 上下文不能承载全部元数据)**

当 N → ∞ 时，若所有工具的 m 和 k 必须同时装入 context，则 cost(C, T) ≥ N · (m + k) = Ω(N · H · log N)，违反 S 的 cost(C, T) = o(N) 要求。

**结论**：若协议在单一层次上同时提供 S/I/C，必然在某一性质上退化。

### 3.4 现有系统的 CAP 定位

```
                        S (Scale)
                           │
                     ╱     │     ╲
                    ╱      │      ╲
                   ╱       │       ╲
            MCP  ●        │        ● ATD
            (S+I)          │         (全部满足
                           │          通过分层)
                           │
       LangChain ●─────────┼─────────● Google Function Calling
       (I only,  │          (I+S,
        partial S)          │           no C)
                           │
               Apple ●────┼────● Enterprise gRPC+OAuth2
               App Intents │     (I+C, no S)
               (S+C, no I) │
                        C (Capability)
```

### 3.5 ATD 的分层突破

ATD 不是在单一层次同时满足 S/I/C——这被定理 3.1 排除。ATD 通过**将三者分配到不同层次**来协同解决：

```
        Schema Layer           ← 解决 I (所有 binding 映射到统一 schema)
            ↓
        Capacity Layer         ← 解决 S (Hot/Warm/Cold 三层)
            ↓
        Security Layer         ← 解决 C (UCAN + 4 级 visibility)
```

这是 ATD 作为**分层协议**而非平层协议的根本动机。定理 3.1 本身不是悲观结论——它是对协议架构的**设计约束**。

### 3.6 定理的意义

CAP 定理之于分布式系统的意义是"停止寻找 CP+AP 神话系统"。Tool Dispatch CAP 定理之于 agent 协议的意义是：

- 告诉 MCP 社区：想要增加安全模型，需要独立于 tool protocol 的 capability layer
- 告诉 OpenAI Tools：想要规模化，需要从协议层引入 capacity 分层
- 告诉 LangChain：想要互操作和规模，需要 binding 抽象化
- 告诉 ATD：分层不是可选项，而是唯一可行路径

---

## 4. Hot/Warm/Cold 容量定理

### 4.1 问题形式化

**定义 4.1 (Capacity Model)**

```
一个 capacity model 是 T 的一个分割 T = T₁ ∪ T₂ ∪ ... ∪ Tₖ
其中：
  每个 Tᵢ 对应一个分级 tier i
  每个 tier 有不同的访问成本 cost_context(i) 和发现延迟 latency(i)
```

**定义 4.2 (Two Costs)**

```
CC (Context Cost):     agent 上下文中被动装载的 tool 元数据 tokens 之和
DL (Discovery Latency): 首次调用 tool t 的期望额外延迟
```

任何 capacity model 都在这两个成本之间权衡。

### 4.2 单层模型的不可能性

**引理 4.3 (单层模型退化)**

```
令 capacity model 只有单层 T = T₁，则：

(a) 若 T₁ 全部装入 context：CC = Ω(N)，N → ∞ 时违反 cost = o(N)
(b) 若 T₁ 按需加载：DL = Ω(f(discovery))
    - 朴素遍历：f = O(N)
    - 索引查询：f = O(log N) + RTT

两种情况下，对于任意 N，必有一项成本爆炸。
```

### 4.3 Hot/Warm/Cold 三层模型

**定义 4.4 (三层 Capacity Model)**

```
Hot Tier   T_H ⊆ T，|T_H| ≤ H_max (典型 H_max = 20)
           CC(T_H) = |T_H| × compact_size ≈ 3000 tokens
           DL(T_H) = 0 (已在 context)

Warm Tier  T_W ⊆ T，|T_W| ≤ W_max (典型 W_max = 200)
           CC(T_W) ≈ 0 (只保留索引 header)
           DL(T_W) ≈ 50ms (本地 HNSW 搜索)

Cold Tier  T_C = T \ (T_H ∪ T_W)
           CC(T_C) = 0
           DL(T_C) ≈ 200ms (远程 registry 查询)
```

### 4.4 核心定理

> **定理 4.5 (Hot/Warm/Cold Pareto Optimality)**
>
> 给定调用频率分布 P(t) 和参数 (H_max, W_max)，三层模型对 (CC, DL) 空间的 Pareto 前沿是紧致的——即：
>
> 对任意单层或双层模型 M'，存在三层模型 M\* 使得：
>   CC(M\*) ≤ CC(M')  且  E[DL(M\*)] ≤ E[DL(M')]
>   至少一个不等式严格成立。
>
> 反之不成立：不存在单层或双层模型能在 (CC, DL) 的所有点上优于三层。

### 4.5 证明思路

```
观察 1：实际 tool 使用呈 Zipf 分布
  调用频率 P(t) ∝ 1/rank(t)^α，通常 α ∈ [0.8, 1.2]
  前 20 个工具占 ~70% 调用
  前 200 个工具占 ~95% 调用
  剩余工具占 <5% 调用

观察 2：不同频率区间的最优存储策略不同
  高频：装入 context（CC 换 DL=0）—— Hot
  中频：本地索引（小 CC 换 小 DL）—— Warm
  低频：远程查询（0 CC 换 大 DL）—— Cold

证明核心：若把中频 tool 强行升级到 Hot，context 爆炸；
         若把中频 tool 强行降级到 Cold，期望延迟爆炸；
         三层分级是唯一符合频率分布的 Pareto 前沿。
```

### 4.6 层级转换动力学

```
转换规则：
  Cold → Warm:  agent 首次成功调用
  Warm → Hot:   近 7 天调用 ≥ 5 次
  Hot → Warm:   14 天未调用（Hysteresis：必须连续 3 天不满足 Hot 条件才降级）
  Warm → Cold:  90 天未调用且不在已安装 skill 的依赖中

稳定性定理（非形式化陈述）：
  在 Zipf 分布且转换参数满足 7d window + 5 calls threshold + 3d hysteresis 下，
  稳态分布的转换率 < 5% per day，系统收敛于频率排序。
```

### 4.7 与分布式缓存的类比

```
ATD Capacity      ≈   CPU Cache Hierarchy
  Hot    (context) ≈    L1 Cache (registers)
  Warm   (HNSW)    ≈    L2/L3 Cache
  Cold   (registry)≈    Main Memory / Disk
```

相同设计原则：频率感知的分层（LFU-like）；访问成本与存储容量成反比；转换策略需要防抖。

### 4.8 实验验证（引用 AnyTool）

AnyTool (2024) 实验了分层发现对 tool space 规模化的影响：
- 16k APIs，单层全装入：上下文溢出，无法运行
- 单层按需检索：p99 延迟 > 3s，Pass Rate 下降 22%
- 三层分级（meta → category → tool agent）：p99 ≤ 800ms，Pass Rate +41%

AnyTool 的实验结果是定理 4.5 的经验支撑——分层不只是理论最优，在实际系统中也带来量级的改进。

### 4.9 定理的意义

Hot/Warm/Cold 不是 ATD 的工程选择，而是 tool space 规模化的**数学必然**。任何试图支撑 10⁴+ tool 的协议必须采用某种三层（或更多层）分级；否则在 N 足够大时必然崩溃。

---

## 5. ATD as POSIX for Agents

### 5.1 POSIX 的历史意义

POSIX (Portable Operating System Interface) 不是操作系统，而是"一个程序要能跨 Unix 变种运行需要满足什么"的形式化定义。它之所以重要，不是因为它发明了 syscall，而是因为它让：

- 程序作者不必绑定到某个具体的 OS 实现
- OS 实现可以独立演化，只要保持 POSIX 合规
- 第三方程序、OS、编译器形成可互操作的生态
- 创新可以发生在 POSIX 边界之外（BSD socket、epoll、io_uring）而不破坏既有程序

Agent-tool 生态当前正处于 pre-POSIX 阶段——每个系统有自己的 tool interface，每次迁移都是重写。

### 5.2 ATD 与 POSIX 的结构同构

这不是修辞类比，而是**结构同构** (structural isomorphism)：

| POSIX 概念 | ATD 对应 | 共同语义 |
|-----------|---------|---------|
| Syscall number | `ToolDefinition.id` | 跨实现稳定的操作标识符 |
| Syscall signature | `ToolDefinition.input/output schema` | 类型化参数契约 |
| `errno` | ATD 统一 error code | 跨实现统一的失败分类 |
| File descriptor | Capability Token | 不可伪造的资源引用 |
| user/group/other 权限位 | Read/Write/Dangerous/System visibility | 四级访问分类 |
| `setuid` / `sudo` | `/allow` / capability delegation | 特权提升机制 |
| `man` page | ATD `description` + `intent_examples` | 人类可读的语义描述 |
| `ld.so` 动态链接 | MCP/ATD plugin 运行时注册 | 延迟绑定与动态加载 |
| `PATH` 环境变量 | Tool Registry（Hot/Warm/Cold）| 工具查找机制 |
| `fork`+`exec` | Sub-agent spawn with attenuated capability | 进程/执行上下文派生 |
| `fcntl` | ATD resource constraints (timeout, rate limit) | 运行时约束控制 |
| `/proc` 文件系统 | `anos schema` CLI introspection | 自省与反射 |

### 5.3 深层同构：Capability 模型

POSIX 的 file descriptor 和 ATD 的 capability token 共享**相同的形式化语义**：

```
POSIX FD 性质                  ATD Capability Token 性质
─────────────────────────     ─────────────────────────
不可伪造：只有内核能创建        不可伪造：Ed25519 签名
可传递：fork/SCM_RIGHTS         可传递：sub-agent 继承
可撤销：close()                 可撤销：revocation list
有范围：read/write/exec         有范围：resource pattern + visibility
可委托降权：dup() with O_RDONLY 可委托降权：child ⊆ parent
```

两者都体现 Dennis & Van Horn (1966) 提出的 **capability machine** 模型。ATD 不是重新发明 capability，而是把 OS 级的 capability 机制提升到 agent-tool 层。

### 5.4 抽象边界的同构

POSIX 和 ATD 都是**抽象边界定义**——定义"上层看到什么"而不规定"下层如何实现"：

```
POSIX 不规定：               ATD 不规定：
  进程调度算法                 具体 LLM
  文件系统实现                 具体 dispatch 实现语言
  内存管理策略                 具体 capacity 策略 (H_max/W_max)
                              具体 binding 执行器
```

这种"定义边界而不定义实现"是 POSIX/ATD 得以形成生态的关键。

### 5.5 不同构的部分：Intent vs Syscall

```
POSIX 是 precisely addressed：程序明确调用 syscall number 5 (open)
ATD 是 intent-addressed：agent 表达"打开文件"，由 dispatch 层路由
```

这个差异来自 LLM 时代的特性——自然语言意图需要被映射到确定性工具调用。ATD 在 POSIX 之上增加了一层**意图路由**，由三层 capacity model 和语义发现实现。

### 5.6 历史类比：网络协议栈的四十年

```
1970s  异构物理层：以太网、令牌环、ATM 各自为政
1980s  TCP/IP 统一：L3 协议标准化
1990s  HTTP 应用层标准化：互联网应用生态爆发
2000s+ 创新在边界之外：HTTPS、HTTP/2、QUIC，但 L3 保持稳定

2020s  异构 agent 工具协议：MCP、OpenAI Tools、App Intents 各自为政
2026+  ATD 的机会：L? 协议统一化
2030s  预期：agent 应用生态爆发
2040s+ 预期：创新发生在 ATD 边界之外，但 ATD 保持稳定
```

POSIX 花了十年从 IEEE 1003.1 (1988) 到广泛采用。ATD 若作为 agent-POSIX，其历史时间尺度大概率是 5-10 年。

### 5.7 为什么是 ATD 而不是 MCP？

MCP 是 agent-tool 生态中最接近 POSIX 候选的协议，但它**缺少三个必要条件**：

```
条件 1：协议绑定的异构统一
  POSIX 允许任何 OS 实现，ATD 允许任何 binding（CLI/MCP/REST/...）
  MCP 只是这些 binding 中的一种，不是它们的上层抽象

条件 2：规模化的内建机制
  POSIX 从未面临 N → ∞ 问题，syscall 数量稳定在 ~400
  agent tool 必然 N → ∞，需要协议层的 capacity model
  MCP 无此机制

条件 3：capability 作为一等公民
  POSIX 的 FD 是协议的核心原语，不是可选项
  MCP 把授权委派给 host，不是协议的一部分
```

ATD 在设计上把 MCP 视为一个 binding，而不是竞争者。MCP 解决"如何让 LLM 与外部工具通信"，ATD 解决"如何定义可互操作、可扩展、可安全的 agent-tool 接口"——后者是前者的上位抽象。

### 5.8 Skills 层作为 stdlib：栈的上位补充，而非竞争

在讨论 ATD 作为 POSIX 的类比时，一个关键问题是：**Skills（Anthropic Agent Skills / agentskills.io 规范）在这个栈的什么位置？**

自 2025-12-18 agentskills.io 开放标准发布后，SKILL.md 已被 OpenAI、Microsoft、GitHub、Atlassian、Figma、Cursor、VS Code 等 26+ 平台采纳。任何讨论"agent 时代基础设施"的论述都必须回答：ATD 和 Skills 是什么关系？

**答案：两层正交，不竞争**。Skills 层在 ATD 层之**上**，类比 Unix 栈的 stdlib 与 POSIX：

```
应用程序（Django / Flask / agent）
    ↓
语言标准库（Python stdlib / Skills SKILL.md）
    ↓
系统调用层（POSIX / ATD）
    ↓
硬件 ABI（CPU ISA / tool binding：CLI/MCP/REST/AppFunction）
```

**职责分工**：

| 层 | 解决的问题 | 单位 | 例子 |
|----|-----------|------|------|
| Skills | agent 面对领域任务，**应该怎么做** | 可复用剧本（recipe / playbook） | PDF 报告生成、Git release、代码审查 |
| ATD | 当一个工具调用发生，**怎么跨 OS/vendor 调到** | 原子能力 | `fs.read`, `xiaomi:light.toggle`, `applehealth:sleep.query` |

**刻意不做的分界**（避免与 agentskills.io 竞争）：

ATD v1.0 规范**不涉及**以下 7 类问题，均由 Skills 层或其他已有规范承担：

1. Skill 的发现、注册、版本管理（agentskills.io / skills.sh / ClawHub）
2. Skill body 的自然语言格式、YAML frontmatter schema
3. Progressive disclosure 机制（只加载名称和描述到 system prompt）
4. Agent 人格 / identity 注入（SOUL.md / onlycrabs.ai）
5. Skill 的 LLM 执行循环
6. 领域专家知识库、参考资料、示例
7. Skill 之间的组合、版本依赖、冲突解决

ATD **只保证**一件事：当 skill body 说 "调 `xiaomi:light.toggle`" 时，该调用**在任何 OS、任何 agent、任何 framework 下可执行**。

**非 breaking 协作提案：`atd-tools` YAML 扩展**

允许 SKILL.md YAML frontmatter 可选声明依赖的 ATD tools，用于 install-time 校验和 H/W/C 预热：

```yaml
---
name: trip-prep
description: Prepare for tomorrow's trip
atd-tools:
  required: [calendar.get, weather.get]
  optional: [flight.status]
atd-capabilities: [calendar.read, net.http]
---
```

此字段是**可选的 superset**——不声明的 skill 照样能在 ATD 上跑，只是运行时才发现 tool 不可用。符合 POSIX 的向后兼容原则：新增能力不破坏旧客户端。

**历史类比验证**

1995-2010 年的栈演化证明了同样的分层模式：

- POSIX（1988）定义 `fopen` / `socket`，从未试图定义"如何写一个 web 服务器"
- Python stdlib（1994+）定义 `urllib` / `pathlib`，不限制你用什么 web framework
- Django / Flask（2005+）提供领域 framework，不改动底层 syscall 抽象

三层各司其职、互相依赖、没有竞争。ATD 在 agent 栈里扮演的就是**最底层 POSIX** 的角色；Skills 扮演的是 **Python stdlib** 的角色。agent framework（Claude Code / Cursor / OpenClaw / Hermes）扮演 **Django / Flask** 的角色。

这个分层一旦确立，ATD 就不再与 agentskills.io 在同一维度竞争——**两者的采纳是互相强化的**：每增加一个 SKILL.md-compatible 平台，ATD 下层的必要性就增加；每增加一个 ATD binding（华为 AppFunction、米家 SDK、企业 REST），Skills 上层的可移植性就增加。

### 5.9 本节的结论

ATD 的历史地位不取决于它的技术有多先进，而取决于它能否被足够多的实现者采纳为**共同的抽象边界**。POSIX 在 1988 年的技术不如某些竞争系统，但它成为了共同语言，因此赢了。

更关键的是：**ATD 的成功不需要 Skills 的失败**。Skills 正在蓬勃发展（26+ 平台采纳），这对 ATD 是利好——Skills 越多，其下方需要一个可互操作 tool 层的必要性就越强。ATD 与 Skills 的共同敌人不是彼此，而是**继续分裂的 tool 生态**。

---

## 6. [Bridge] From Theory to Specification

### 6.1 从定理到设计约束

```
定理 3.1 (CAP)        ⟹  协议必须分层
  → Part II 组织为 6 个协议层

定理 4.5 (H/W/C)      ⟹  Capacity 必须三层
  → Part II §11 明确 Hot/Warm/Cold 为协议级概念

POSIX 同构 (§5)        ⟹  协议必须定义抽象边界而非实现
  → Part II 严格区分 "MUST" / "SHOULD" / "MAY"
```

### 6.2 规范写作的四个原则

- **P1. Schema-First**：每个概念先给出完整 JSON Schema，再给出文字解释
- **P2. Normative Language**：使用 RFC 2119 关键字（MUST/MUST NOT/SHOULD/MAY/SHOULD NOT）
- **P3. Interop Over Elegance**：优先可互操作性，即使设计不够优雅
- **P4. Versioning Discipline**：所有 schema 字段声明其引入版本

### 6.3 六个协议层的角色分工

```
Layer 1: Schema Layer (§7)       — 定义 ToolDefinition 数据结构
Layer 2: Dispatch Layer (§8)     — 定义一次 tool call 的 8 步生命周期
Layer 3: Binding Layer (§9)      — 定义 4 种 binding 的参数/错误映射
Layer 4: Security Layer (§10)    — 定义 UCAN + 4 级 visibility + 宪法守卫
Layer 5: Capacity Layer (§11)    — 定义 Hot/Warm/Cold 语义与转换
Layer 6: Reliability Layer (§12) — 定义健康监控、circuit breaker、fallback
```

### 6.4 层间依赖关系

```
         Schema Layer (§7)
              ↑
              │ (所有上层依赖 schema 定义)
    ┌─────────┼─────────┬─────────┐
    │         │         │         │
Dispatch   Binding   Security  Capacity
 (§8)       (§9)      (§10)     (§11)
    │         │         │         │
    └─────────┼─────────┴─────────┘
              ↑
              │
        Reliability (§12)
```

Schema Layer 是所有层的基石。Reliability 观察所有层但不被它们感知。

### 6.5 读者导航提示

- 协议实现者：§7 → §8 → §9 → §10 → §11 → §12
- 安全审计者：§7 → §10 → Appendix A
- 性能工程师：§7 → §8 → §11 → §12
- 标准化参与者：§7 → §6（本章）→ Part III

### 6.6 一致性承诺

- **C1**. ATD v1.0 规范是 self-contained
- **C2**. 所有规范可被 Rust、Python、TypeScript 任何语言实现
- **C3**. 所有示例是可执行的
- **C4**. 所有字段有明确的"为什么"

---

# Part II — Protocol Specification

## 7. Schema Layer

### 7.1 设计原则

- **Single Source of Truth**：一个 ToolDefinition 派生全部下游 artifacts (P4)
- **Protocol-Agnostic Affordance**：借鉴 W3C WoT 的 "affordance + forms" 分离
- **Forward-Compatible Evolution**：新字段默认 optional

### 7.2 ToolDefinition 核心结构

```
atd_version    : "1.0"                (MUST, 协议版本)
id             : ToolId                (MUST, 唯一标识)
version        : SemVer                (MUST, tool 版本)
name           : string                (MUST, 人类可读名称)
description    : string                (MUST, 英文 + 可选 localized)
capability     : CapabilityDescriptor  (MUST, 用于发现与路由)
input          : JsonSchema            (MUST, 参数 schema)
output         : JsonSchema            (MUST, 结果 schema)
errors         : ErrorDef[]            (SHOULD, 领域错误)
bindings       : BindingSet            (MUST, ≥1 个 binding)
safety         : SafetyClassification  (MUST, 安全分级)
resources      : ResourceConstraints   (SHOULD, 运行时约束)
trust          : TrustMetadata         (SHOULD, 发布者与签名)
compatibility  : CompatibilityInfo     (MAY, 平台与硬件要求)
fallback       : FallbackSpec          (MAY, 降级方案)
```

完整 JSON Schema 在 Appendix B。

### 7.3 ID 命名规范

```
格式: PREFIX:domain.resource.action[.variant]

PREFIX 枚举（规范约束）：
  anos:         核心内置（协议 reserved namespace）
  host:         宿主插件（本地二进制包装）
  mcp:          MCP server 桥接（运行时注册）
  vendor:<org>: 厂商 tool（签名验证）
  community:<pkg>: 社区发布
  custom:       用户自定义（未签名）

ID 不变性：一旦发布，不得修改 id。破坏变更通过 version 字段表达。
```

### 7.4 Capability Descriptor

```
CapabilityDescriptor {
  domain          : string        (如 "filesystem", "camera")
  actions         : string[]      (如 ["read", "write"])
  tags            : string[]      (语义标签)
  intent_examples : string[]      (自然语言意图示例，用于 embedding)
}
```

`intent_examples` 是 ATD 与 MCP 最大的语义层差异——它让工具可以被**意图级发现**而非仅字符串匹配。对应 §2 的 (D1) Intent Match。

### 7.5 三种表达形式

```
Full ATD     ~500 tokens  注册/API/Cold tier
Compact ATD  ~150 tokens  Hot tier（system prompt）
Deferred ATD  ~30 tokens  Warm tier（索引 header）
```

三者是**派生关系**（Full → Compact → Deferred），不是独立定义。投影规则在 §11 详述。

### 7.6 向后兼容规则

```
ADD field (optional)     : minor bump (v1.0 → v1.1)
ADD field (required)     : major bump (v1.0 → v2.0)
RENAME field             : major bump
REMOVE field             : 需要 deprecated_in + 至少一个 minor 的过渡期
TIGHTEN schema           : major bump
LOOSEN schema            : minor bump
```

---

## 8. Dispatch Layer

### 8.1 设计原则

- **Deterministic Pipeline**：dispatch 是确定性 8 步流水线，不依赖 LLM 推理
- **Fail-Fast**：每步失败立即返回错误，不进入下一步
- **Observable**：每步产生结构化 trace event
- **Idempotent on Failure**：失败的 dispatch 可安全重试（不产生副作用直到 Step 7）

### 8.2 八步流水线

```
Step 0 — ENGINE INTERCEPT
  特殊 tool（agent.*, session.*, code.delegate）路由到 Intent Bus
  MUST：这些 tool 不进入 Step 1-8

Step 1 — CAPABILITY CHECK
  验证 token 签名、有效期、resource pattern、usage/rate/budget
  失败 → PERMISSION_DENIED / RATE_LIMITED / BUDGET_EXCEEDED

Step 2 — RESOLVE TOOL
  lookup ToolDefinition + 选择候选 binding
  失败 → TOOL_NOT_FOUND / PLATFORM_UNSUPPORTED
  MUST：resolve 过程不得有副作用

Step 3 — VALIDATE PARAMS
  JSON Schema Draft 2020-12 验证
  失败 → VALIDATION_ERROR（包含 field-level 错误详情）

Step 4 — RATE LIMIT + CIRCUIT BREAKER
  rate_limiter.allow() + circuit_breaker.state
  失败 → RATE_LIMITED (retry_after) / TOOL_CIRCUIT_OPEN

Step 5 — ROUTE TO BINDING
  按 §9.9 定义的规范选择算法（filter + sort）选取 binding
  关键序：agent_pref > health_score > latency > default_priority
  MUST：binding 选择是确定性的

Step 6 — EXECUTE IN SANDBOX
  binding.execute(params, timeout=tool.resources.timeout_ms)
  sandbox：process isolation / capability filtering / resource limits
  失败 → TIMEOUT / EXECUTION_ERROR / SANDBOX_VIOLATION

Step 7 — NORMALIZE RESULT
  binding-specific 字段映射 → JSON Schema 验证 → secret 擦除
  失败 → RESULT_INVALID / SECRET_DETECTED

Step 8 — AUDIT + RETURN
  audit_log + meter + health tracker
  MUST：audit log append-only，tool 不可修改
```

### 8.3 并行执行规则

```
Read-Only Tools（visibility=Read）
  MAY 并行执行，上限由 agent 配置决定（典型 ≤ 8）

Write/Dangerous Tools
  MUST 串行执行
  MUST 在执行前重新检查 agent 的 abort signal

理由：并行写会破坏因果推理。Read 并行的安全性来自"读不改变状态"。
```

### 8.4 超时语义

```
tool.resources.timeout_ms 是 Step 6 的执行超时
  不包含 Step 1-5 dispatch overhead
  不包含 Step 7 normalize overhead

Dispatch 总超时 = timeout_ms + 500ms dispatch_budget
超过 → TIMEOUT 错误
```

### 8.5 错误处理与重试

```
错误分类（error_class 字段）：
  Transient      - 临时失败，SHOULD 重试（network, rate limit）
  Permanent      - 永久失败，MUST NOT 重试（validation, permission）
  Environmental  - 环境失败，SHOULD 检查状态后重试（binary missing, service down）

Retry 策略由 agent 决定，但 ATD spec 强制：
  retryable       : bool             MUST 正确设置
  retry_after_ms  : u32 (可选)        SHOULD 在 rate limit 时提供
  max_retry_hint  : u32 (可选)
```

### 8.6 统一 ToolResult envelope

```
ToolResult {
  status: "success" | "error"

  若 success:
    data     : <tool.output_schema 验证通过的值>
    metadata : {
      tool_id, tool_version, binding_used, latency_ms,
      timestamp, request_id, cache_hit (可选)
    }

  若 error:
    code          : ATDErrorCode      (统一枚举，见 Appendix D)
    message       : string            (人类可读)
    reason        : string            (机器可读的 sub-code)
    error_class   : Transient|Permanent|Environmental
    retryable     : bool
    retry_after_ms: u32?
    binding_error : Value?           (原始协议错误，调试用)
}
```

### 8.7 Dispatch 的 Invariants

```
I1. 对同一 (tool_id, params, token) 的两次 dispatch：
    若 tool side-effect-free：结果应等价
    若 tool 有 side effect：第二次可能失败或返回不同结果

I2. 任何 dispatch 失败不得泄露 secret
    Step 7 的 secret scanner 不可被绕过

I3. 任何 dispatch 成功必然经过 Step 1-8 的全部步骤
    不得跳过 capability check、schema validation、audit log

I4. Sub-agent 的 dispatch 能力 ⊆ parent 的 dispatch 能力
    capability 递减定理（UCAN 的直接应用）
```

---

## 9. Binding Layer

### 9.1 设计原则

- **Unified Envelope**：所有 binding 最终返回 ToolResult，agent 不感知 binding 类型
- **Explicit Mapping**：每个 binding 必须声明字段级映射规则
- **Error Normalization**：binding 层统一错误码，保留原始错误供调试
- **Platform Detection**：runtime MUST 在启动时检测 binding 可用性

### 9.2 四种核心 Binding

```
CLI Binding         - 包装本地命令行工具（ffmpeg, git, jq）
MCP Binding         - 桥接 MCP server（stdio / streamable-HTTP）
REST Binding        - 调用 HTTP API（内部或外部）
AppFunction Binding - 调用 OS 级应用功能（Android AppFunctions / Apple App Intents / HarmonyOS Intents）
```

同一个 ToolDefinition **可以声明多个 binding**，dispatch layer 按 §8 Step 5 的优先级选择。

### 9.3 CLI Binding 规范

```
CliBinding {
  binary            : string              MUST
  args_template     : string              MUST  (使用 {param_name} 占位)
  env_template      : map<string,string>  MAY
  working_dir       : string?             MAY
  stdin_template    : string?             MAY
  result_parser     : "json"|"text"|"exit_code"|"custom"  MUST
  result_path       : JsonPath?           MAY
  exit_code_mapping : map<int, ATDErrorCode>  MAY
}

参数映射：
  - 模板变量 {x} 替换为 params[x]
  - undefined 值在模板中触发 VALIDATION_ERROR
  - 数组值默认 space-join

结果解析：
  json       → stdout 解析为 JSON，按 result_path 提取
  text       → stdout 作为 string 返回
  exit_code  → 仅 0/non-zero 布尔结果
  custom     → 由 tool 自定义 parser

安全要求：
  MUST 使用 argv 数组传递参数（禁止 shell 展开）
  MUST 在 sandbox 中执行（seccomp / namespace / Landlock）
  MUST NOT 将 secret 写入 args（使用 env 或 stdin）
```

### 9.4 MCP Binding 规范

```
McpBinding {
  server_id         : string                  MUST
  mcp_tool_name     : string                  MUST
  param_mapping     : map<ATDName, McpName>   MAY
  result_mapping    : map<McpField, ATDField> MAY
  transport         : "stdio"|"streamable_http"  MUST
}

错误映射：
  JSON-RPC error -32600  → INVALID_REQUEST
  JSON-RPC error -32601  → TOOL_NOT_FOUND
  JSON-RPC error -32602  → VALIDATION_ERROR
  JSON-RPC error -32603  → INTERNAL_ERROR
  JSON-RPC error -32000 to -32099 → 按 server 语义映射

MCP Discovery 集成：
  /mcp add <command> 后，runtime MUST 自动：
    1. 启动 MCP server
    2. 调用 initialize + tools/list
    3. 为每个 MCP tool 生成 ToolDefinition (id = mcp:<server>.<tool>)
    4. 注册到 Tool Registry
```

### 9.5 REST Binding 规范

```
RestBinding {
  method            : "GET"|"POST"|"PUT"|"DELETE"|"PATCH"  MUST
  url_template      : string             MUST  (支持 {param} 占位)
  headers           : map<string,string> MAY
  body_mapping      : BodyMapping        MAY
  auth              : AuthSpec?          MAY
  result_path       : JsonPath?          MAY
  status_mapping    : map<int, ATDErrorCode>  MAY
}

状态码映射（默认）：
  2xx → Success
  400 → VALIDATION_ERROR
  401/403 → PERMISSION_DENIED
  404 → 消歧规则：
        若 endpoint 路径整体不可达（路由级 404）→ TOOL_NOT_FOUND
        若 endpoint 可达但请求的 resource 不存在（payload 级 404）→ RESOURCE_NOT_FOUND
  429 → RATE_LIMITED（retry_after 从 Retry-After header 解析）
  500-599 → INTERNAL_ERROR（retryable=true）

安全要求：
  MUST 支持 TLS，MUST 验证证书
  MUST NOT 在 URL 中暴露 secret（使用 header 或 body）
  SHOULD 支持 OAuth 2.1 / bearer token
```

### 9.6 AppFunction Binding 规范

```
AppFunctionBinding {
  platform          : "android"|"ios"|"harmonyos"  MUST
  target            : TargetSpec                    MUST
  param_mapping     : map<ATDName, PlatformName>   MAY
  result_mapping    : map<PlatformField, ATDField> MAY
}

TargetSpec（platform-specific）：
  android:    package, class, function
  ios:        bundle_id, intent_name
  harmonyos:  ability, action

安全要求：
  MUST 通过 platform 原生权限系统
  MUST NOT 绕过 platform 用户确认对话框

平台不可用时：binding MUST 在 Step 5 返回 PLATFORM_UNSUPPORTED 错误
```

### 9.7 统一错误码映射表

| ATD Error Code | CLI | MCP | REST | AppFunction |
|---------------|-----|-----|------|-------------|
| PERMISSION_DENIED | exit 126 | -32600 | 401/403 | SecurityException |
| TOOL_NOT_FOUND | exit 127 / binary missing | -32601 | 404 | FunctionNotFound |
| VALIDATION_ERROR | exit 2 | -32602 | 400 | IllegalArgument |
| RATE_LIMITED | exit 1 + stderr marker | -32000 custom | 429 | TooManyRequests |
| TIMEOUT | signal TERM/KILL | -32000 timeout | 504 | TimeoutException |
| INTERNAL_ERROR | 其他 | -32603 | 500-599 | RuntimeException |

完整错误码参见 Appendix D。

### 9.8 Binding 可扩展性

```
协议层保留未来扩展点：
  gRPC Binding       (预留 v1.1+)
  WebSocket Binding  (预留 v1.1+)
  Local Function Binding  (预留)

扩展 binding 的 MUST 要求：
  1. 返回 unified ToolResult
  2. 声明 error_class 映射
  3. 支持 timeout 强制
  4. 支持 capability token 传递
  5. 提供 conformance test
```

### 9.9 Binding 选择的决定论

```
给定同一个 (tool_id, platform, agent_preference)，binding 选择 MUST 是确定的。
选择算法：
  1. 过滤：binding 支持当前 platform
  2. 过滤：binding 所需的 external tool 可用
  3. 过滤：circuit_breaker.state != Open（或有 fallback）
  4. 排序：按 (agent_pref, health_score, latency, default_priority) 降序
  5. 选择：排序第一的 binding

default_priority（ATD 规范建议值）：
  AppFunction > REST (local) > CLI > MCP > REST (remote)
  理由：本地 > 远程，原生 API > 进程间 > 网络
```

---

## 10. Security Layer

### 10.1 设计原则

- **Capability-Native**：capability token 是协议的一等原语，不是外挂
- **Visibility = Authorization**：LLM 看不见的工具等于没有授权
- **Constitutional Never Bypassable**：宪法守卫不受任何 flag 影响
- **Attenuation-Only Delegation**：child agent 权限严格 ⊆ parent
- **Offline Verifiable**：不依赖中心化授权服务

### 10.2 UCAN 集成

ATD 采用 UCAN 1.0 作为 capability token 的形式化基础：

```
CapabilityToken {
  token_id   : string
  subject    : { agent_id, parent_agent_id? }
  issuer     : DID
  audience   : DID
  resource   : ResourcePattern    (tool:PREFIX:domain.resource.action)

  constraints: {
    methods             : string[]?
    excluded_methods    : string[]?
    rate_limit          : { max, window_secs }?
    cost_budget         : { max_cost, currency, period }?
    safety_max          : "read"|"write"|"dangerous"
    requires_dry_run_first : bool
    requires_human_confirm : bool
  }

  validity: {
    not_before : ISO8601
    expires_at : ISO8601
    max_uses   : u32
  }

  proof_chain: Signature[]
  signature  : Ed25519Signature
}
```

### 10.3 Resource Pattern 语义

```
精确匹配：tool:anos:camera.capture.photo
通配符：  tool:anos:camera.capture.*     (所有 capture action)
命名空间：tool:anos:camera.*             (所有 camera 工具)
前缀：    tool:anos:*                    (所有 anos 内置)
厂商：    tool:vendor:huawei:health.*
全通配：  tool:*                         (max permission)

匹配规则：
  - 精确 > 通配符 > 前缀
  - 同级重叠时，最具体的优先
  - 任意 match 成功即授权，但 constraints 必须全部通过
```

### 10.4 四级 Visibility

```
┌──────────────┐
│   System     │  kernel 守护进程专用，永不暴露给 LLM
│              │  例：constitutional.audit, evolution.trigger
├──────────────┤
│  Dangerous   │  需要显式 /allow 授权才能暴露给 LLM
│              │  例：shell.exec, docker.run, fs.delete
├──────────────┤
│    Write     │  始终可见，有副作用但通常安全
│              │  例：fs.write, git.commit, memory.store
├──────────────┤
│    Read      │  始终可见，只读操作
│              │  例：fs.read, web.fetch, system.status
└──────────────┘
```

### 10.5 Visibility = Authorization 定理

> **定理 10.1 (Visibility-Authorization Equivalence)**
>
> 对 Dangerous tool t：
>   agent 看见 t ⟺ agent 被授权调用 t
>
> 形式化：
>   t ∈ llm_visible_tools(agent) ⟺ ∃ token k ∈ K : authorizes(k, t)

```
实现约束：
  MUST：Dangerous tool 在 LLM 的 system prompt 中仅当 token 存在时注入
  MUST：Revoke token 后，下一轮 system prompt 重建时移除该 tool
  MUST：未授权的 Dangerous tool 即使 dispatch 到了也在 Step 1 被拒绝
```

这个等价性来自 POSIX 同构（§5.3）——就像 Unix 进程只能 `open()` 在 `open_files` 表中的 FD，agent 只能"看见"在能力表中的 tool。

### 10.6 宪法守卫 (Constitutional Guard)

宪法守卫是不可被任何 token、flag、配置绕过的强制检查：

```
CG1. Secret Scanning
     MUST 扫描所有 fs.write/fs.edit 的内容
     MUST 扫描所有 tool result 的 stdout/response
     匹配规则：API key、credential、private key 等 (pattern + entropy)
     触发 → SECRET_DETECTED，MUST 中止 dispatch

CG2. Forbidden Shell Patterns
     rm -rf / | fork bomb | :(){:|:&};: | dd to disk device
     git push --force origin main|master
     chmod 777 recursive
     触发 → CONSTITUTIONAL_VIOLATION

CG3. Capability Escalation Attempts
     任何 tool 试图调用 capability.grant / capability.delegate_up
     触发 → CONSTITUTIONAL_VIOLATION

MUST：即使 --dangerously-skip-permissions 也不得绕过宪法守卫
MUST：宪法守卫的触发 MUST 记录到 append-only audit log
MUST：守卫规则由 OS 级 daemon 持有，不在 tool 进程中检查
```

### 10.7 Token Attenuation

> **定理 10.2 (Attenuation Monotonicity)**
>
> 对任意 parent token K_p 和它派生的 child token K_c：
>   resource(K_c) ⊆ resource(K_p)
>   constraints(K_c) ≤ constraints(K_p)  (按每个约束分量，下方 "≤" 意为"不弱于")
>   validity(K_c) ⊆ validity(K_p)
>
> Attenuation 不可逆：无法从 K_c 派生出比 K_p 更宽的权限。
>
> **关于 "≤" 的方向**：对于数值上限字段（rate_limit.max, cost_budget.max, max_uses 等），
> "K_c ≤ K_p" 意为 K_c 的数值 MUST ≤ K_p 的数值（更小 = 更严格）。
> 对于枚举字段（safety_max），按枚举顺序 read < write < dangerous，K_c 的值 MUST ≤ K_p 的值。

实现规则：

```
签发 child token 时，runtime MUST 验证：
  1. K_c.resource 的 pattern 是 K_p.resource 的 specialization
  2. K_c.constraints.rate_limit ≤ K_p.constraints.rate_limit
  3. K_c.constraints.cost_budget ≤ K_p.constraints.cost_budget
  4. K_c.constraints.safety_max ≤ K_p.constraints.safety_max
  5. K_c.validity.expires_at ≤ K_p.validity.expires_at
  6. K_c.validity.max_uses ≤ K_p.validity.max_uses
  7. K_c.proof_chain 包含 K_p 的签名

任一检查失败 → ESCALATION_ATTEMPT 错误
```

### 10.8 Authorization 时间线

```
agent 请求 tool list
     ↓
system prompt 构建器：
  对每个 tool t：
    若 t.visibility == Read/Write → 始终注入
    若 t.visibility == Dangerous → 仅当存在 token 时注入
    若 t.visibility == System → 从不注入
     ↓
LLM 看到 tool schema，发起 tool call
     ↓
Dispatch Step 1 (CAPABILITY CHECK):
  1. 验证 token 签名
  2. 检查 not_before ≤ now ≤ expires_at
  3. 检查 current_uses < max_uses
  4. 检查 rate_limiter.allow(agent, tool)
  5. 检查 cost_meter.within_budget(agent)
  6. 检查 resource pattern 匹配
  7. 检查 tool.visibility ≤ token.safety_max
  任一失败 → PERMISSION_DENIED (具体 reason)
     ↓
Dispatch Step 6 执行前的最后检查：
  宪法守卫扫描 params（secret / forbidden pattern）
  触发 → CONSTITUTIONAL_VIOLATION（即使有 token 也拒绝）
     ↓
Execute
```

### 10.9 Revocation

```
M1. Expiry-based：
    token.expires_at 自然到期
    SHOULD：短生命周期 token（minutes-hours）优于长期 token

M2. Explicit Revocation List：
    runtime 维护 revoked_token_ids 集合
    实现 SHOULD：Bloom filter 加速检查
    SHOULD：revocation 通过 agent mesh 同步（最终一致）

Revocation 的即时性：
  MUST：下次 dispatch 之前检查 revocation list
  MAY：正在执行的 dispatch 不被中断（grace period）
```

### 10.10 Audit Log

```
每次 dispatch MUST 写入 append-only audit log：

AuditEntry {
  timestamp
  agent_id
  parent_agent_id?
  tool_id
  token_id           (不记录 signature)
  params_hash        (params 的 BLAKE3 哈希)
  result_status      (success | error)
  error_code?
  binding_used
  latency_ms
  cost_estimate
  constitutional_flags?  ("secret_detected", "forbidden_pattern", ...)
}

MUST：audit log 的持久化介质对 tool 和 agent 只写不可读
MUST：audit log 对 OS 级审计者 (root / constitutional daemon) 可读
MUST：log 被写入前 tool 不得知道自己的调用被记录
```

---

## 11. Capacity Layer

### 11.1 设计原则

- **Frequency-Driven**：层级分配由实际调用频率决定
- **Per-Agent State**：每个 agent 有独立的 Hot/Warm 集合
- **Lazy Promotion**：仅在调用后更新层级
- **Anti-Thrashing**：转换阈值包含 hysteresis

### 11.2 三层正式定义

```
HotTier {
  capacity_max   : u32    建议 20，MUST ≤ 50
  size_budget    : u32    建议 3000 tokens，MUST ≤ 8000
  expression     : Compact ATD（约 150 tokens/tool）
  location       : Agent system prompt
  discovery      : 0ms (已在 context)
  update_cadence : 每次 tool call 后重新评估
}

WarmTier {
  capacity_max   : u32    建议 200，MUST ≤ 1000
  expression     : Deferred ATD（约 30 tokens/tool）
  location       : 本地 HNSW 向量索引
  discovery      : < 100ms p99
  update_cadence : 每次 tool call 后更新
}

ColdTier {
  capacity_max   : ∞
  expression     : Full ATD（按需拉取）
  location       : Remote Tool Registry / MCP servers
  discovery      : < 500ms p99
  update_cadence : 每日批量同步 metadata
}
```

### 11.3 ATD 表达形式的投影规则

```
Full ATD → Compact ATD (Hot tier 投影)
  保留：id, name, short_description, input summary, output summary,
        safety.level, resources.estimated_tokens
  丢弃：errors, trust, compatibility, fallback, 完整 binding 细节
  投影规则 MUST 是确定性的（给定 Full，Compact 唯一确定）

Full ATD → Deferred ATD (Warm tier 投影)
  保留：id, name, domain, action list, safety.level
  丢弃：input/output schema, bindings, 大部分元数据
  投影规则 MUST 保留足够信息支持 HNSW 向量检索
```

投影是**单向压缩**。若 agent 需要完整信息，runtime 必须从 Full ATD 重建。

### 11.4 层级转换规则

```
Cold → Warm （首次成功调用）
  触发：agent 首次 dispatch tool t 成功
  动作：
    1. 拉取 Full ATD
    2. 生成 embedding（content = description + intent_examples）
    3. 插入本地 HNSW 索引
    4. 初始化调用计数器

Warm → Hot （频繁使用）
  触发：同时满足：
    - calls_7d(t) ≥ 5
    - 频率分 score(t) 进入 agent 的 top |HotTier.capacity_max|
  score 计算：score(t) = 0.7 × calls_7d + 0.3 × calls_30d
  动作：
    1. 从 HNSW 索引升级到 system prompt
    2. 若 Hot tier 已满，降级 score 最低的 Hot tool 到 Warm
    3. 下一轮 system prompt 重建生效

Hot → Warm （不再频繁）
  触发：last_called 早于 14 天前
  Hysteresis：必须连续 3 天不满足 Hot 条件才降级（防震荡）
  动作：从 system prompt 移除，保留 HNSW 索引

Warm → Cold （长期不使用）
  触发：同时满足：
    - last_called 早于 90 天前
    - 不在任何已安装 skill 的 required_tools 中
  动作：从 HNSW 索引移除，保留 tool_id 在 cold registry 中
```

### 11.5 Anti-Thrashing 设计

> **定理 11.1 (Transition Rate Bound)**
>
> 在 Zipf 调用频率分布、Hot threshold=5 calls/7d、Hysteresis=3d 条件下，稳态每日转换率 R 满足 R ≤ 0.05（每日转换 ≤ 5% 的工具）。

```
实践含义：
  1000 个 Warm tools，每天最多 50 个转换
  不会出现"早上 Hot，下午 Warm，晚上 Hot"的震荡
```

### 11.6 Agent Tool Profile 持久化

```
AgentToolProfile {
  agent_id
  hot_tools   : [{ tool_id, last_called, calls_7d, calls_30d, compact_atd_cache }]
  warm_tools  : [{ tool_id, last_called, calls_7d, calls_30d }]
  total_hot_tokens : u32
  last_updated     : timestamp
}

MUST：profile 跨 session 持久化
MUST：profile 对 agent 自身只读（由 runtime 维护，不被 LLM 修改）
SHOULD：profile 定期做一致性检查
```

### 11.7 System Prompt 注入布局

```
System Prompt (~8000 tokens):

  [1] Agent identity/role               ~500 tokens
  [2] Active skill instructions          ~1500 tokens
  [3] HOT tools (compact ATD)            ~3000 tokens  ★
      格式：per-tool YAML-like block
  [4] WARM tier discovery hint           ~200 tokens   ★
      "You have {N} additional tools available.
       Call tool.search(intent) to discover them."
  [5] Capability summary                 ~300 tokens   ★
      "Authorized: tool:anos:fs.*, tool:anos:web.*, ..."
  [6] Working memory snippet             ~1000 tokens
  [7] Current task context               剩余预算

★ 标记的三个部分由 Capacity Layer 管理。
```

### 11.8 Tool Search API (Warm Tier Discovery)

```
tool.search 是一个特殊的 tool（本身在 Hot tier）：

tool.search {
  input:
    intent       : string   (自然语言意图)
    max_results  : u32      (默认 5)
    domain_filter: string[] (可选)

  output:
    results: [{
      tool_id, name, short_description,
      match_score   : f32   (HNSW 余弦相似度)
      deferred_atd  : DeferredATD
    }]
}

实现：
  1. intent → embedding（与 deferred_atd 相同的模型）
  2. HNSW 最近邻搜索
  3. 按 match_score + capability 匹配度排序
  4. 返回 top max_results

MAY：runtime 自动 pre-load 高 score 的 Warm tool 到 Hot 一轮
```

### 11.9 Cold Tier Federation

```
RegistryFederation {
  registries: [
    { id: "anos-official", url, trust_level: 4 },
    { id: "vendor-huawei", url, trust_level: 3 },
    { id: "community",     url, trust_level: 2 },
  ]

  discovery: 按优先级顺序查询，首个命中返回
  caching: Cold tier 元数据缓存 24h，full ATD 缓存 7d
}

MUST：不同 registry 的 tool 必须有 namespace prefix 区分
MUST：registry 的 trust_level 影响默认 capability 上限
SHOULD：registry 的 CDN-style 分发降低 p99 延迟
```

---

## 12. Reliability Layer

### 12.1 设计原则

- **Fail-Fast on Known Degradation**：已知故障工具立即快失败
- **Graceful Fallback**：故障时自动切换到 fallback tool
- **Automatic Recovery**：不需要人工干预即可从故障恢复
- **Observable State**：所有 reliability 状态可被 agent、运维、审计者观察

### 12.2 健康监控：5 分钟滚动窗口

```
HealthState {
  window_start      : timestamp           (5 分钟滚动窗口起点)
  success_count     : u32
  failure_count     : u32
  timeout_count     : u32

  latencies_p50     : u32 ms              (近 5 分钟 p50)
  latencies_p99     : u32 ms

  last_success      : timestamp?
  last_failure      : timestamp?
  last_failure_code : ATDErrorCode?
}

计算方式：
  metrics MUST 使用滚动窗口（sliding window），不使用定期重置
  实现 SHOULD：token bucket / exponentially weighted moving average
```

### 12.3 健康分级

```
Healthy:
  success_rate ≥ 95%
  p50_latency ≤ tool.resources.expected_latency_ms × 2
  p99_latency ≤ tool.resources.expected_latency_ms × 5

Degraded:
  80% ≤ success_rate < 95%
  或 p50_latency 超 2-5 倍预期
  或 p99 超 5-10 倍预期

Unhealthy:
  success_rate < 80%
  或 p50/p99 超 degraded 上限

Unknown:
  window 内调用次数 < 5（样本不足）
  默认按 Healthy 处理
```

### 12.4 Circuit Breaker 状态机

```
Closed (正常)
  所有请求透传
  记录 success/failure
  状态转换：近 5min error_rate > 50% AND calls ≥ 10 → Open

Open (熔断)
  所有请求立即返回 TOOL_CIRCUIT_OPEN
  MAY 透明切换到 fallback（若声明且 fallback 非 Open）
  状态转换：cooldown 过期 → Half-Open

Half-Open (探测)
  允许 3 个 probe 请求通过
  状态转换：
    3 个 probe 全部成功 → Closed
    任一 probe 失败 → Open (cooldown × 2)
```

```
Cooldown exponential backoff：
  初始 cooldown     = 30 seconds
  连续进入 Open 时   cooldown × 2 (max 300 seconds)
  成功回到 Closed 时 cooldown 重置为 30s

时间线示例：
  T=0    Open (cooldown=30s)
  T=30   Half-Open → 失败 → Open (cooldown=60s)
  T=90   Half-Open → 失败 → Open (cooldown=120s)
  T=210  Half-Open → 成功 → Closed, cooldown reset to 30s
```

### 12.5 Fallback 机制

```
FallbackSpec {
  fallback_tool_id    : ToolId
  fallback_condition  : "circuit_open" | "timeout" | "rate_limited" | "always_on_error"
  degraded_params     : map<string, JsonValue>?
  max_chain_length    : u8 (默认 3)
}

Fallback 触发规则：
  Step 4 检测到 circuit_breaker.state == Open
    → 查询 tool.fallback
    → 若存在且 fallback.state == Closed → 切换到 fallback，记录 metadata.used_fallback=true
    → 若不存在或 fallback 也 Open → 返回 TOOL_CIRCUIT_OPEN 错误

Fallback 链长度限制：
  MUST：fallback 最多链式 3 跳
  MUST：fallback chain 不得成环
```

### 12.6 Retry 策略由 agent 决定

```
tool.resources.retry_policy {
  safe_to_retry     : bool        (幂等性声明)
  recommended_retry : u8          (建议最大次数，默认 3)
  backoff_ms        : [u32]       (建议退避时间，默认 [1000, 2000, 4000])
}

Agent 决策：
  错误 retryable=true AND retry_count < recommended_retry → retry
  错误 error_class == Permanent → 不 retry
  错误 error_class == Environmental → 按 recommended_retry 策略 retry
  错误 error_class == Transient → retry_after_ms 等待后 retry

MUST：tool 本身不得在内部 retry
      理由：内部 retry 会绕过 circuit breaker，放大故障
```

### 12.7 Observability

```
Runtime MUST 提供：
  GET /health/{tool_id}    返回当前 HealthState + HealthStatus + CircuitBreaker.state
  GET /health/summary      返回所有 tool 的健康概览
  GET /circuit/{tool_id}   返回 circuit breaker 完整状态
  GET /audit/{request_id}  返回特定 dispatch 的完整 trace

CLI：
  anos health             列出所有不健康的 tool
  anos circuit list       列出所有 non-Closed circuit
  anos circuit reset X    强制重置 tool X 的 circuit
```

### 12.8 协议层 vs 实现层的边界

```
规范层 MUST 定义：
  - 健康分级阈值
  - Circuit breaker 状态机
  - Fallback 触发规则
  - Retry 语义
  - Introspection API 形状

规范层不规定：
  - Health metrics 的存储方式
  - HNSW 索引的具体实现
  - Circuit breaker 状态的持久化策略
  - 跨 agent 的 health 共享机制
```

### 12.9 与 Hystrix/Polly/Resilience4j 的关系

ATD Reliability Layer 不是重新发明，而是把 industry-proven 模式（Hystrix 2011, Polly 2015, Resilience4j 2017）提升到 agent-tool 协议层：

```
借鉴的模式：
  - Circuit Breaker state machine
  - Bulkhead isolation (per-tool concurrency limit)
  - Time Limiter (per-dispatch timeout)
  - Fallback policy composition
  - Metrics first, policy second

ATD 的新贡献：
  - 这些模式作为协议字段可声明（不仅是 host 代码逻辑）
  - Health state 是可 introspect 的协议对象
  - Fallback chain 跨 binding（不只是进程内）
```

---

## 13. [Bridge] From Specification to Ecosystem

### 13.1 从规范到采纳的落差

规范写完不等于生态形成。POSIX 1988 年发布，真正广泛采纳是 1990s 中期。RFC 2119 的关键字 MUST/SHOULD 保证了规范的精确性，但不能保证有人实现它。

ATD 面临的采纳挑战有三个：

```
挑战 1：Incumbent Lock-in
  MCP 已有成熟 SDK 和生态（Anthropic/OpenAI/开源社区）
  OpenAI Tools 已是 de-facto JSON Schema 标准
  LangChain 已是 Python 事实标准
  "Why switch to ATD?" 需要清晰的迁移收益

挑战 2：Reference Implementation Quality
  规范再好，若没有高质量参考实现，采纳者面临工程风险
  ATD 当前参考实现在 ANOS（Rust）—— 单语言单项目，生态单薄

挑战 3：Governance Legitimacy
  由单一项目（ANOS）主导的规范缺乏行业认可
  需要多利益方共识才能成为真正的标准
```

Part III 依次回应这三个挑战：§14（生态对齐 + 迁移路径）、§15（治理结构）、§16（版本演化）、§17（开放问题）。

### 13.2 规范已定义的接口 vs 需要生态协作的领域

```
规范层面已定义（Part II）：
  ✓ Tool 数据结构
  ✓ Dispatch 流程
  ✓ 4 种 binding 的语义
  ✓ Security/Capacity/Reliability 的协议约束

需要生态协作（Part III 讨论）：
  - Tool registry 的托管（中心化？联邦？去中心化？）
  - Tool 命名空间的分配（vendor: prefix 如何分配？）
  - Capability token 的 DID 根锚点（谁是 issuer 信任根？）
  - Conformance test 的权威来源（谁负责认证？）
  - 版本演化的决策流程（谁决定 v1.1 加哪些字段？）
  - 与 MCP/OpenAI Tools 的互操作承诺（是否保证向后兼容？）
```

### 13.3 可能的对齐策略

```
策略 A：取代 (Replacement)
  ATD 试图取代 MCP/OpenAI Tools/LangChain
  风险：战略冲突，行业不会站队单一项目

策略 B：并列 (Coexistence)
  ATD 作为"又一个"协议存在，与 MCP 等平行
  风险：增加碎片化，违背 ATD 的初衷

策略 C：上位 (Super-Abstraction) ← ATD 采纳
  ATD 作为 MCP/OpenAI Tools/REST/App Intents 的上位抽象
  MCP 等作为 ATD 的 binding 之一
  生态可以选择性实施 ATD 的一部分（e.g. 仅采纳 Capacity Layer）
  "ATD compatible" 不意味着放弃已有技术
```

策略 C 让采纳成为**加法**而非**替换**——这是 POSIX 成功的关键模式。

### 13.4 从 Part II 规范到 Part III 治理的逻辑流

```
Schema Layer (§7)
  → §14.1 如何与 MCP/OpenAI Tools schema 互操作？
  → §15 命名空间分配的治理

Dispatch Layer (§8)
  → §14.2 不同实现如何保证 dispatch 行为一致？
  → Conformance test（Appendix A）谁维护？

Binding Layer (§9)
  → §14.3 gRPC/WebSocket binding 何时加入？
  → 多利益方决策流程

Security Layer (§10)
  → §14.4 DID 的信任根如何建立？
  → Capability federation 的跨注册表验证

Capacity Layer (§11)
  → §14.5 Cold tier federation 的治理
  → 不同 registry 的 trust_level 谁定？

Reliability Layer (§12)
  → §14.6 Health 跨 agent 共享的隐私边界
  → 集体 circuit breaker 的信号传播
```

### 13.5 读者身份切换提示

```
Part II 假设读者是"实现 ATD 的工程师"
Part III 假设读者是"决定是否 / 如何采纳 ATD 的决策者"

包括：
  - 开源项目维护者
  - 企业架构师
  - 云厂商
  - 标准化组织
  - 研究者
```

Part III 的论证风格会从"技术精确"转向"战略权衡"，但保持同样的严谨性。

---

# Part III — Governance & Roadmap

## 14. Related Work & Ecosystem Alignment

### 14.1 相关工作的全景图

按"与 ATD 的关系"组织相关工作：

```
┌─────────────────────────────────────────────────────────────┐
│  作为 ATD Binding 的候选（可被吸收）                          │
│  ─────────────────────────────────────────────────────────  │
│  MCP (Anthropic)      → mcp: binding                         │
│  OpenAPI 3.1          → rest: binding                        │
│  gRPC service         → grpc: binding (预留 v1.1)           │
│  CloudEvents          → event: binding (预留 v1.2)          │
│  W3C WoT TD           → thing: binding (预留 v1.2)          │
│  Android AppFunctions → appfunction: binding (android)      │
│  Apple App Intents    → appfunction: binding (ios)          │
│  HarmonyOS Intents Kit→ appfunction: binding (harmonyos)   │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  作为 ATD 协议层原语（已引用）                                │
│  ─────────────────────────────────────────────────────────  │
│  UCAN 1.0             → capability token 的形式化基础        │
│  JSON Schema 2020-12  → input/output schema 的验证标准       │
│  JSON-RPC 2.0         → mcp: binding 的 wire protocol        │
│  DID (W3C)            → agent identity 的根锚点              │
│  Ed25519              → token signature 算法                │
│  BLAKE3               → params hash、audit log integrity     │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  作为 ATD 理论基础（已引用）                                  │
│  ─────────────────────────────────────────────────────────  │
│  POSIX (IEEE 1003.1)      → 抽象边界定义的范式参考            │
│  Capability Machine        → Dennis & Van Horn 1966          │
│  Hystrix/Polly/Resilience4j → 可靠性模式的工业先例          │
│  AnyTool, ToolBench        → 规模化发现的学术证据            │
│  Zipf Distribution         → 频率分布假设的经验基础         │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│  作为 ATD 上层消费者（ATD 服务的对象）                        │
│  ─────────────────────────────────────────────────────────  │
│  LangChain Tools       → 可通过 langchain: binding 桥接     │
│  Semantic Kernel        → 可通过 sk: binding 桥接            │
│  CrewAI / AutoGen       → Python SDK 调用 ATD                │
│  OpenAI Agents SDK     → 可将 ATD 工具导出为 OpenAI Tools    │
│  Claude Code / Codex / Gemini CLI  → 通过 ACP 暴露 ATD 工具  │
└─────────────────────────────────────────────────────────────┘
```

### 14.2 与 MCP 的具体互操作承诺

MCP 是 agent-tool 生态中最活跃的协议，ATD 与 MCP 的关系对生态采纳至关重要：

```
互操作承诺（v1.0 MUST 满足）：

IO-MCP-1：任何 MCP server 可 zero-code 接入 ATD
  MCP server 通过 /mcp add 注册 → 自动生成 mcp:<server>.<tool> 的 ATD ToolDefinition
  MCP 侧不需要任何修改

IO-MCP-2：ATD tool 可导出为 MCP server
  ATD runtime MAY 暴露 MCP-compatible endpoint
  外部 MCP client 可发现并调用 ATD tools

IO-MCP-3：错误码双向映射无损
  ATD ↔ MCP 错误码映射是双射（§9.4 已定义）
  往返转换不丢失语义

IO-MCP-4：Schema 双向映射
  ATD input/output JSON Schema ↔ MCP tool inputSchema
  snake_case ↔ camelCase 在 param_mapping 中显式声明

IO-MCP-5：Transport 透明
  MCP 的 stdio / streamable-HTTP 作为 ATD mcp: binding 的 transport 子类型
  ATD 不限制 MCP server 的 transport 选择
```

### 14.3 与 OpenAI Tools 的对齐

```
ATD 不取代 OpenAI Tools 的 wire shape：

IO-OAI-1：Compact ATD → OpenAI Tools 投影是规范化的
  { tool_id → name, description → description, input_schema → parameters }
  投影规则在 Appendix E 完整定义

IO-OAI-2：OpenAI Tools 的 parallel tool calls 被 ATD 支持
  §8.3 的 Read 并行规则 map 到 OpenAI 的 parallel_tool_calls

IO-OAI-3：OpenAI strict mode 兼容
  若 tool.input_schema 符合 OpenAI strict mode 子集，
  ATD runtime SHOULD 自动启用 strict mode
```

### 14.4 与 LangChain/Semantic Kernel 的对齐

```
这些框架是上层消费者，不是对等协议：

Python：
  langchain_tool(atd_tool_def) → langchain.BaseTool

C#/.NET：
  SemanticKernel.KernelFunction.FromATD(atd_tool_def)

承诺：
  ATD 提供 SDK-level 的适配层，框架不需要感知 ATD 协议层
```

### 14.5 与 Claude Code Agent SDK / ACP 的对齐

```
ACP (Agent Communication Protocol) 是 agent-orchestration 协议，
处理 "外部系统 ↔ Claude Code / Codex / Gemini CLI" 的通信。

ATD 与 ACP 是互补的：
  ACP：处理 agent 生命周期、session 管理、prompt 交互、approval events
  ATD：处理 agent 内部如何调用 tool

集成点：
  ACP 的 "tool approval" event → 对应 ATD Dangerous tool 的 /allow
  ACP 的 session 概念 → 对应 ATD Agent Tool Profile
  ACP 的 capability 声明 → 对应 ATD capability token 的简化子集

承诺：
  ATD runtime SHOULD 同时支持 ACP endpoint（被 Claude Code 等调用）和
  内部 tool dispatch
```

### 14.6 与 W3C WoT Thing Description 的对齐

```
WoT TD 是 ATD 在 IoT 领域的最接近的先例：

相似点：
  WoT property/action/event ↔ ATD read/write/stream tool 语义
  WoT forms (protocol bindings) ↔ ATD bindings
  WoT security schemes ↔ ATD capability constraints

差异点：
  WoT 面向 IoT 设备，强调 "Thing" 作为资源单位
  ATD 面向 agent，强调 "Tool" 作为能力单位
  ATD 有 agent-specific 的概念（intent, capacity tier, LLM visibility）
  WoT 使用 JSON-LD，ATD 使用 plain JSON Schema

承诺：
  ATD v1.1 将提供 ATD ↔ WoT TD 的参考转换器
  一个 WoT Thing 可被暴露为 ATD tools（thing: binding）
```

### 14.7 与 AnyTool / ToolBench 的关系

```
这些学术工作证明了 "hierarchical tool retrieval is necessary at scale"：

引用关系：
  AnyTool (2024) 的实验数据作为 §4 Hot/Warm/Cold 定理的经验支撑
  ToolBench (2023) 的 16k API 数据集作为 ATD 规模测试的 benchmark 候选
  Gorilla (2023-24) 的 APIBench 作为 capability descriptor 质量测试

ATD 的贡献：
  把学术工作中的 "hierarchical retrieval" 工程化为协议层的 Capacity Layer
  提供 reference implementation 验证学术结果在生产环境的可重现性
```

### 14.8 本节小结

ATD 与现有生态的关系是**上位抽象 + 互操作 + 借鉴**的组合：

- 上位抽象：对 MCP/OpenAPI/App Intents 等 binding 级协议
- 互操作：与 LangChain/OpenAI Tools/ACP 等消费者层 SDK
- 借鉴：从 POSIX/UCAN/Hystrix/AnyTool 等理论和工业工作吸取设计

这种定位让 ATD 不需要"打败"任何现有系统，而是成为让它们**可以共存**的协议基础。

---

## 15. Multi-Stakeholder Governance Proposal

### 15.1 治理合法性的三个原则

回应 §13 的"挑战 3：Governance Legitimacy"：

```
原则 G1：Technical Correctness Is Not Enough
  ATD v1.0 的技术设计再正确，若由单一项目（ANOS）主导，
  无法承担"行业标准"的治理责任
  参照：Rust 从 Mozilla 项目演化到 Rust Foundation 的经验

原则 G2：Multi-Stakeholder Inclusion
  工具生态跨越多个领域（云厂商、LLM 提供商、开源社区、学术界、
  应用开发者、标准化组织），治理结构必须容纳这些利益方

原则 G3：Phased Legitimacy Building
  治理合法性不是一次性建立的，而是通过阶段性演化：
  单项目 → 开源基金会 → 行业联盟 → 正式标准化组织
```

### 15.2 三阶段治理演化

```
═══════════════════════════════════════════════════════════════
Phase 1: Reference Implementation (现在 — 2026 Q3)
  主导：ANOS 项目
  形式：开源规范 + 参考实现（Rust）
  决策：维护者 consensus + GitHub RFC 流程
  产出：
    - ATD v1.0 规范（本白皮书 Part II）
    - 参考实现（crates/anos-tool-dispatch）
    - Conformance test suite (Appendix A)
    - 第一批 binding（CLI/MCP/REST/AppFunction）

  关键里程碑：
    M1.1 [完成] ATD v1.0 规范发布（本白皮书）
    M1.2 [3 个月] 至少 1 个独立第三方实现（非 Rust）
    M1.3 [6 个月] 至少 3 个 tool registry 跨项目互操作验证
═══════════════════════════════════════════════════════════════

Phase 2: Working Group Formation (2026 Q4 — 2027 Q4)
  主导：Agent Protocol Working Group (APWG)
  形式：在中立基金会下（候选：Linux Foundation AI & Data、
       CNCF、新成立的 Agent Protocol Foundation）
  成员构成（建议）：
    - 2-3 个 LLM 提供商（Anthropic / OpenAI / Google / 其他）
    - 2-3 个云厂商（AWS / Azure / GCP / Alibaba / Huawei）
    - 2-3 个开源项目代表（ANOS / LangChain / SK / Claude Code）
    - 1-2 个学术代表
    - 1-2 个应用开发者代表（社区选举）
  决策机制：Rough Consensus + Working Draft
  产出：
    - ATD v1.1 规范（包含 gRPC binding 等扩展）
    - Official conformance program
    - 多实现（Rust/TypeScript/Python/Go 参考实现）
    - Tool registry federation protocol

  关键里程碑：
    M2.1 [12 个月] APWG 正式成立，至少 5 个创始成员
    M2.2 [18 个月] ATD v1.1 正式发布
    M2.3 [24 个月] 至少 3 个云厂商原生支持 ATD
═══════════════════════════════════════════════════════════════

Phase 3: Formal Standardization (2028 Q1+)
  主导：正式标准化组织
  候选路径：
    A. W3C：作为 "Agent Tool Dispatch" WG
    B. IETF：作为 RFC
    C. ISO/IEC JTC 1/SC 42（AI 子委员会）
  形式：标准化组织的正式 spec + 维护机制
  产出：
    - ATD v2.0 作为国际标准
    - 官方测试套件与认证流程
    - 治理的长期机构化

  关键里程碑：
    M3.1 [30 个月] 提交到标准化组织
    M3.2 [48 个月] 至少一个标准化组织接受为候选推荐
    M3.3 [60 个月] 首个正式标准发布
═══════════════════════════════════════════════════════════════
```

### 15.3 APWG 的具体结构建议

```
Steering Committee (5-7 人)
  由创始成员代表构成
  决策：高层方向、版本发布、成员接纳
  任期：2 年，可连任

Technical Committee (7-11 人)
  主要的技术决策机构
  按协议层次设立 subgroup（Schema/Dispatch/Binding/Security/Capacity/Reliability）
  成员通过技术贡献 review 加入

Interop Committee
  负责 conformance test 维护
  负责跨实现互操作性验证
  认证机构的运营

Ecosystem Committee
  处理 namespace 分配（vendor: prefix）
  处理 registry federation 申请
  处理与现有生态的 liaison

独立审计者 (Independent Auditor)
  每年发布 ATD 实现的生态报告
  处理 conformance 争议
  可以是学术机构或独立非盈利组织
```

### 15.4 决策机制细节

```
日常技术决策（Technical Committee）：
  RFC 流程（参考 Rust RFC）：
    1. 提案者提交 RFC PR
    2. 公开 review 不少于 14 天
    3. Technical Committee 两周内给出 disposition
    4. 接受：merge 到规范；拒绝：记录 rationale；需要修改：迭代

协议 breaking change（全体共识）：
  需要 Steering Committee 2/3 多数 + Technical Committee 简单多数
  需要至少 6 个月的 deprecation period

版本发布决策：
  minor version (v1.1, v1.2)：Technical Committee 简单多数
  major version (v2.0)：Steering Committee 2/3 多数

争议解决：
  技术争议：Technical Committee 投票，必要时升级到 Steering Committee
  成员冲突：Independent Auditor 仲裁
  紧急安全问题：Security Subgroup 可发布紧急补丁（事后 review）
```

### 15.5 防止 capture 的机制

```
限制 1：单一组织代表数上限
  任何单一组织在 Steering Committee 中不得超过 1 人
  任何单一组织在 Technical Committee 中不得超过 2 人

限制 2：地理多样性要求
  Steering Committee 必须代表至少 3 个地区（北美/欧洲/亚洲）

限制 3：资金透明度
  APWG 运营资金公开披露
  单一赞助者不得超过总资金的 40%

限制 4：开源许可保证
  所有 WG 产出 MUST 以 Apache 2.0 或同等无限制许可发布

限制 5：弃权权保留
  任何 Committee 成员对其雇主有直接商业利益冲突的决策必须弃权
  冲突声明强制公开
```

### 15.6 知识产权策略

```
S1. 规范本身
    以 Creative Commons Attribution 4.0 发布
    允许 fork、衍生、商业使用

S2. 参考实现
    以 Apache 2.0 发布（包含专利授权）

S3. 商标（"ATD", "ATD Compatible"）
    APWG 或其继承组织持有
    Conformance 认证通过后可使用 "ATD Compatible" 商标

S4. Patent Non-Assertion
    APWG 成员承诺对"必要实施 ATD 规范"的技术不行使相关专利
    参考 W3C Patent Policy
```

### 15.7 与现有基金会 / 组织的关系

```
候选托管基金会分析：

Linux Foundation AI & Data
  + 已有 AI 治理经验（LF AI Foundation, KubeFlow, Trusted AI）
  + 中立性强，有成熟治理流程
  + 与 CNCF 兼容（云原生生态）
  - 偏重工程，学术参与度较低

CNCF (Cloud Native Computing Foundation)
  + 技术治理成熟
  + 与 Kubernetes / service mesh 生态良好互补
  - 重点在云原生，agent 并非核心议题

Apache Software Foundation
  + 治理流程经典，"Apache Way"
  - 决策慢，可能不适合快速演化的 agent 生态

新成立的 Agent Protocol Foundation
  + 专注于 agent 生态
  + 治理结构可定制
  - 启动成本高，合法性建立慢

建议：Phase 2 初期选择 LF AI & Data 作为托管，保留 Phase 3 独立的可能
```

### 15.8 开放问题：治理尚未回答的难题

```
OP-GOV-1：如何处理与 MCP 规范的协调？
  MCP 由 Anthropic 主导，决策权在单一组织
  APWG 如何与 MCP 演化保持兼容？
  可能路径：邀请 MCP 维护者加入 APWG Binding Subgroup

OP-GOV-2：Conformance 认证的商业模式
  认证是免费还是收费？
  免费可能导致质量下降，收费可能限制小厂采纳
  参照 Java Compatibility Kit 的演化历程

OP-GOV-3：中国 / 欧盟 / 美国的法规差异
  欧盟 AI Act、中国生成式 AI 管理、美国 executive orders
  ATD 规范如何在不同法规下保持技术中立？

OP-GOV-4：Post-quantum cryptography 迁移
  Ed25519 签名面临长期量子威胁
  何时、如何引入 PQC？由谁决策？
```

---

## 16. Versioning & Migration

### 16.1 版本语义

```
ATD 使用 Semantic Versioning 2.0.0：

MAJOR.MINOR.PATCH

MAJOR：不兼容的协议变更（MUST 通过 Phase 2+ 的治理流程）
  示例：添加 required 字段、重命名字段、删除 binding 类型

MINOR：向后兼容的新特性
  示例：添加 optional 字段、新 binding 类型、新 visibility tier

PATCH：规范澄清、勘误、不影响实现的文字修订
  示例：修正 typo、澄清歧义、补充 example
```

### 16.2 兼容性承诺矩阵

```
                    v1.0 client  v1.1 client  v2.0 client
v1.0 server         ✓            ✓            ✗ (降级模式)
v1.1 server         ✓ (降级)     ✓            ✗
v2.0 server         ✗            ✗            ✓

规则：
  - 同 major 版本内：server 必须接受较老 client（向后兼容）
  - 同 major 版本内：client SHOULD 支持协商到 server 版本
  - 跨 major 版本：需要显式的 migration layer（§16.5）
```

### 16.3 版本协商机制

```
Request {
  atd_version: "1.0"      // client 支持的最高版本
  compat_range: ["1.0"]   // client 支持的所有版本
  ...
}

Response {
  atd_version: "1.0"      // server 选择的实际版本
  ...
}

协商规则：
  1. server 从 client.compat_range 中选择自己支持的最高版本
  2. 若无交集 → 返回 VERSION_INCOMPATIBLE 错误
  3. 协商结果 MUST 在 response 中明示
```

### 16.4 Deprecation Policy

```
ATD 字段 / 特性的生命周期：

[Introduced]    字段被引入，标注 introduced_in: "1.0"
  ↓
[Stable]        字段稳定，可被依赖
  ↓
[Deprecated]    字段标注 deprecated_in: "1.2"
                规范说明推荐替代方案
                deprecation 必须持续 ≥ 2 个 minor 版本
  ↓
[Removed]       字段标注 removed_in: "2.0"
                只能在 major version bump 时移除

MUST：deprecated 字段在规范中保留完整定义，直到 removed
MUST：runtime 在处理 deprecated 字段时 SHOULD 输出警告
```

### 16.5 Migration Layer

```
v1.x → v2.0 Migration:

M1. Tool Definition Migration
    工具：atd-migrate v1 → v2
    输入：v1.x ToolDefinition JSON
    输出：v2.0 ToolDefinition JSON
    自动化：所有无歧义转换
    人工审查：涉及语义变更的字段

M2. Runtime Compatibility Mode
    v2.0 runtime SHOULD 内置 v1.x compatibility mode
    通过 atd.compat.v1 模块加载 v1.x 定义
    运行时动态升级到 v2.0 schema
    性能代价：~5% dispatch overhead（可接受）

M3. Dual-Version Registry
    迁移期（建议 12 个月）：registry 同时存储 v1 和 v2 表达
    按 client 版本动态选择返回版本
```

### 16.6 Extension Mechanism

```
不通过版本升级也可扩展 ATD：

E1. Experimental Fields
    字段名以 x_ 前缀（如 x_custom_metadata）
    不在规范中定义，实现可自由添加
    其他实现 MUST 忽略未知的 x_ 字段

E2. Namespace Extensions
    ID prefix 扩展：vendor: / community: / custom: 已预留
    新的 top-level prefix 需要 APWG 批准

E3. Binding Extensions
    新 binding 类型：
    - Phase 1（实验）：单一实现声明 x_binding_grpc
    - Phase 2（候选）：多实现支持，提案进入 RFC
    - Phase 3（标准）：MINOR version 纳入规范

E4. Capability Extensions
    UCAN constraints 新字段可 opt-in 扩展
    未知 constraint 字段 MUST 被保守解释（默认 deny）
```

### 16.7 已规划的版本路线

```
v1.0 (本白皮书)                                    - 核心规范
  - 4 binding (CLI, MCP, REST, AppFunction)
  - UCAN security
  - Hot/Warm/Cold capacity
  - Circuit breaker reliability

v1.1 (APWG 首个版本, 预计 2027 Q2)
  - gRPC binding
  - WebSocket binding (for streaming)
  - Dry-run execution 字段启用
  - Error class 标准化

v1.2 (预计 2027 Q4)
  - W3C WoT TD binding
  - CloudEvents event-driven binding
  - Registry federation protocol v1

v2.0 (预计 2028+)
  - Post-quantum crypto 过渡
  - Stateful tool protocol（session 绑定的 tool）
  - Agent-to-agent tool delegation
  - Formal semantics for semantic search
```

### 16.8 版本演化的开放问题

```
OP-VER-1：MCP 的独立演化
  Anthropic 可能以自己的节奏演化 MCP
  若 MCP 引入不兼容变更，ATD 的 mcp: binding 如何处理？
  可能方案：ATD 支持同时映射 MCP v1 和 v2，通过 transport 协商

OP-VER-2：OpenAI Tools schema 的演化
  OpenAI Responses API 持续演化
  ATD 的 Compact ATD → OpenAI Tools 投影需要同步更新
  建议：APWG Ecosystem Committee 维护 upstream tracking

OP-VER-3：实验性 binding 的孵化
  如何让生态实验新 binding 而不污染规范？
  建议：APWG 维护 "incubating binding" 文档
       满足采纳标准后才进入 MINOR version
```

---

## 17. Open Problems & Research Agenda

### 17.1 八个开放问题

**OP-ATD-1：Capability 语义的形式化验证 (HIGH)**

§10.7 的 Attenuation 定理是非形式化陈述。当 UCAN proof chain 跨越 10+ 层委托时，如何用形式化工具（Coq / Lean / TLA+）证明权限单调递减？

```
研究方向：
  - 用 TLA+ 形式化 capability delegation state machine
  - 证明任意 proof chain 的 attenuation invariant
  - 证明宪法守卫不可被任何 token 绕过
优先级：HIGH（安全模型的基础，若有漏洞后果严重）
```

**OP-ATD-2：语义发现的鲁棒性 (HIGH)**

§11 的 Warm tier 依赖 HNSW 向量检索。但 LLM 的"意图表达"是高度上下文相关的。

```
研究方向：
  - Intent embedding 的 robustness benchmark
  - Adversarial intent（被恶意引导的意图）检测
  - 跨模型（Claude / GPT / Gemini）的 intent 一致性
  - 冷启动问题（新 agent 没有调用历史时如何 bootstrap Hot tier）
优先级：HIGH（直接影响 agent 的 dispatch 质量）
```

**OP-ATD-3：Tool Composition 的类型系统 (HIGH)**

ATD v1.0 有 tool composition 的草案但未落地。typed pipes 需要一个类型系统支持 structural / coercible / failure 三种组合语义。

```
研究方向：
  - Tool composition 的类型论基础（linear types? effect systems?）
  - 组合链的 static cost estimation
  - Composition 的失败处理（partial composition 如何回滚）
  - 与 LLM tool selection 的协同
优先级：HIGH（是 ATD 从"工具调度"进化到"能力编排"的门控）
```

**OP-ATD-4：Registry Federation 的信任模型 (MEDIUM)**

§11.9 和 §14 提到 federated registry，但跨 registry 的信任传递没有完整定义。

```
研究方向：
  - Trust transitivity 的边界（web of trust vs hierarchical）
  - Registry 跨节点的 capability token 验证
  - 恶意 registry 的隔离机制
  - 与 PKI / DID / Verifiable Credential 生态的集成
优先级：MEDIUM（对 multi-registry 生态的规模化关键，但 single-registry 可用）
```

**OP-ATD-5：Cross-Agent Health 共享 (MEDIUM)**

§12 的 circuit breaker 是 per-agent state。跨 agent 共享 health 可降低系统总体故障率，但涉及隐私边界。

```
研究方向：
  - Differential privacy 下的 health 聚合
  - Agent mesh 的 gossip protocol for health
  - 恶意 agent 上报虚假 health 的防御
  - 跨 agent health 的时效性（stale information 问题）
优先级：MEDIUM（优化问题，不影响正确性）
```

**OP-ATD-6：实时工具调用的延迟预算 (MEDIUM)**

§12 假设 dispatch 延迟在秒级可接受。但 agent 进入物理世界（机器人、IoT）时，延迟预算缩到毫秒级。

```
研究方向：
  - Dispatch 开销的 profiling（每步的 overhead）
  - Capability check 的 caching 策略（O(1) 授权验证）
  - 针对实时场景的"fast path" binding
  - 与 GAA 白皮书 OP5（实时性冲突）的协同
优先级：MEDIUM（影响 ATD 在物理世界的适用性）
```

**OP-ATD-7：LLM 误用工具的防御 (MEDIUM)**

§10 的宪法守卫防御已知模式。但 LLM 可能通过创造性组合产生意料外的危害。

```
研究方向：
  - Flow-sensitive 的宪法守卫（跨 tool call 的状态追踪）
  - Red-team automation（LLM 自动探索 ATD 的攻击面）
  - Capability leak detection（权限的隐式转移）
  - 与 Agent alignment 框架的协同
优先级：MEDIUM（已知问题有防御，未知组合是 open）
```

**OP-ATD-8：规范治理本身的可持续性 (LOW)**

§15 的 APWG 结构是理论设计，能否真的运转取决于多个主体的协作意愿。

```
研究方向：
  - 治理成功的早期指标（leading indicators）
  - 治理失败的回退机制（fallback plan）
  - 与现有标准化组织的长期合作模式
  - 发展中国家 / 非主流地区的参与门槛降低
优先级：LOW（但深远）
```

### 17.2 研究优先级排序

```
HIGH (blocks critical path):
  OP-ATD-1  Capability 形式化验证       (安全模型基础)
  OP-ATD-2  语义发现鲁棒性              (dispatch 质量)
  OP-ATD-3  Tool Composition 类型系统   (能力编排演化)

MEDIUM (important but non-blocking):
  OP-ATD-4  Registry Federation 信任    (规模化)
  OP-ATD-5  Cross-Agent Health 共享     (性能优化)
  OP-ATD-6  实时延迟预算                (物理世界适用性)
  OP-ATD-7  LLM 误用防御                (安全长尾)

LOW (deep but not urgent):
  OP-ATD-8  治理可持续性                (生态长期问题)
```

### 17.3 与 GAA 白皮书的研究议程呼应

ATD 的 OP 与 GAA 的 OP 形成研究议程的闭环：

```
ATD OP-ATD-1 (Capability 形式化)     ↔  GAA OP4 (Cross-Interface Alignment)
ATD OP-ATD-3 (Tool Composition)      ↔  GAA OP1 (Primitive Completeness)
ATD OP-ATD-6 (实时延迟预算)          ↔  GAA OP5 (Real-time Cognitive Conflict)
ATD OP-ATD-7 (LLM 误用防御)          ↔  GAA OP2 (Norm Reasoning Reliability)
ATD OP-ATD-8 (治理可持续性)          ↔  GAA OP8 (Governance Legitimacy)
```

这种呼应不是巧合——ATD 解决"agent 与世界的接口"，GAA 解决"agent 自身的通用性"，两者的开放问题在能力边界、安全性、治理层面必然交织。

### 17.4 对 Agent 研究社区的邀请

本白皮书的八个开放问题不是 ANOS 项目的待办事项，而是 agent-tool 生态的公共研究议程。

我们特别欢迎：
- 学术研究者将 ATD 作为 benchmark 系统
- 工业实验室针对 OP-ATD-X 发表论文
- 开源社区 fork / 挑战 ATD 规范以暴露盲点
- 不同语言、不同文化的 agent 生态提供多样化视角

长期目标：让 ATD 的每一个开放问题在 3-5 年内有至少一个可引用的学术成果，形成 tool dispatch 作为独立研究领域的学术基础。

---

## 18. Conclusion

### 18.1 本文的贡献回顾

```
学术贡献（Part I）：
  C1. 首次形式化定义 Agent Tool Dispatch 问题
  C2. Tool Dispatch CAP 定理：S/I/C 在单层协议中不可三全
  C3. Hot/Warm/Cold 容量定理：三层分级是 Pareto 最优
  C4. ATD ↔ POSIX 结构同构

工程贡献（Part II）：
  C5. 6 层协议规范（Schema/Dispatch/Binding/Security/Capacity/Reliability）
  C6. 4 种 binding 的统一抽象（CLI/MCP/REST/AppFunction）
  C7. Visibility = Authorization 定理及实现约束

治理贡献（Part III）：
  C8. 三阶段治理演化框架（ANOS → APWG → 标准化组织）
  C9. 与现有生态的对齐承诺（IO-MCP-1 至 IO-MCP-5 等）
```

### 18.2 本文的局限

```
L1. 规范层面
    - Part II 是 v1.0 规范，已知 8 个开放问题待解决
    - Tool Composition 只有草案
    - Cross-platform binding 需要更多实现验证

L2. 实现层面
    - 参考实现仅在 ANOS（Rust 单项目）
    - 尚无非 Rust 的独立第三方实现
    - Conformance test suite 仍在完善中

L3. 治理层面
    - APWG 尚未成立
    - 多利益方共识尚未形成
    - Phase 2/3 的时间表是理想预期，可能滑动

L4. 理论层面
    - CAP 定理和 H/W/C 定理的证明是草图级别
    - 完整形式化证明需要后续论文
    - OP-ATD-1 的 Capability 形式化验证尚未完成
```

这些局限不是放弃的理由，而是研究议程的起点。

### 18.3 对三类读者的呼吁

```
对研究者（Part I 读者）：
  - ATD 提供了一个新的研究领域的问题边界
  - 8 个开放问题都是值得发论文的方向
  - 形式化证明、实证 benchmark、替代定理都欢迎

对工程师（Part II 读者）：
  - ATD v1.0 规范足以被独立实现
  - 鼓励用 TypeScript / Python / Go 等语言实现参考版本
  - Conformance test suite 接受社区贡献

对标准化参与者（Part III 读者）：
  - 邀请加入 APWG 的筹建
  - 欢迎来自任何地区、任何组织的代表
  - Phase 1 → Phase 2 的过渡需要 5-10 个创始成员的承诺
```

### 18.4 长期愿景

```
2030 年的预期图景（若本路线图按计划推进）：

  - ATD 成为 agent-tool 生态的事实标准
  - 每个主要 LLM 提供商（Anthropic/OpenAI/Google/...）支持 ATD binding
  - 全球 tool registry 形成联邦网络，10⁵+ 工具可被发现
  - Capability token 成为 agent 授权的通用语言
  - ATD 规范进入标准化组织的正式流程（W3C / IETF / ISO）

  最终结果：agent 开发者像今天的 web 开发者一样工作——
  假设底层协议（HTTP / TCP / TLS）稳定，专注于上层价值创造。
  这是 ATD 真正的历史意义——把 agent 生态从 "pre-POSIX" 阶段
  推入 "post-POSIX" 阶段，让创新可以堆叠在稳定的基础设施之上。
```

### 18.5 呼应 GAA 白皮书

```
GAA（General Autonomous Agent）回答：
  "什么是通用自主 Agent？"
  通过三个充要条件 + 10 能力原语 + 三层对齐定义

ATD（Agent Tool Dispatch）回答：
  "通用自主 Agent 如何与世界交互？"
  通过统一协议 + 规模化容量 + capability 安全定义

两份白皮书互为支撑：
  GAA 需要 ATD 作为"接口泛化"（条件三）的具体实现路径
  ATD 需要 GAA 作为"为什么要设计这样一个协议"的动机

它们合在一起构成一个完整的 agent 系统的理论框架——
  上层：GAA 定义 agent 自身
  下层：ATD 定义 agent 与世界
```

### 18.6 结语

POSIX 的诞生让 Unix 变体走向可互操作。40 年后，这种可互操作性已成为软件工程不可见但关键的基础设施——任何 C 程序员写代码时都不假思索地依赖 POSIX，但正是这种"不假思索"证明了它的成功。

ATD 的野心是成为 agent 时代的 POSIX——不是被看见的明星技术，而是被依赖的隐形基石。若 2035 年的 agent 开发者写代码时不假思索地假设 ATD 存在，那就是 ATD 成功的最佳证据。

本白皮书只是起点。真正的工作在于让 ATD 从 ANOS 项目的内部规范演化为 agent 生态的公共基础设施。这需要研究社区、工程社区、标准化社区的共同努力。欢迎加入。

---

# Appendices

## Appendix A: Conformance Test Suite

ATD 实现的合规测试覆盖六个协议层。每层的合规性检查通过黑盒测试验证——只观察 input/output 行为，不检查内部实现。

### A.1 Schema Layer Conformance

```
A.1.1 Tool Definition Validation
  - 给定 valid Full ATD JSON → 实现 MUST 接受
  - 给定缺少 MUST 字段的 JSON → 实现 MUST 拒绝并报告 MISSING_FIELD
  - 给定 unknown x_ 字段 → 实现 MUST 接受并保留
  - 给定 unknown 非 x_ 字段 → 实现 MUST 拒绝（前向兼容性约束）

A.1.2 ID Format Compliance
  - PREFIX:domain.resource.action 格式严格匹配
  - 大小写敏感
  - 长度上限 256 chars

A.1.3 Three Form Projection
  - Full → Compact 投影是确定性的
  - Full → Deferred 投影保留 HNSW 检索所需信息
  - Compact / Deferred 不能反向重建 Full
```

### A.2 Dispatch Layer Conformance

```
A.2.1 Eight-Step Pipeline
  - 任意失败必须在对应 Step 立即返回
  - Step 跳过 → 测试失败
  - Step 顺序错误 → 测试失败

A.2.2 Parallel Execution Rules
  - Read tools 可被同时调用（实现可选支持，但若声称支持必须正确）
  - Write/Dangerous tools 必须串行（任何顺序违反 → 测试失败）

A.2.3 Timeout Enforcement
  - tool.resources.timeout_ms = T
  - dispatch 必须在 T + 500ms 内返回
  - 超过 → TIMEOUT 错误

A.2.4 Idempotency on Failure
  - Step 1-5 的失败必须不产生副作用
  - 重试 dispatch 必须能重新执行
```

### A.3 Binding Layer Conformance

```
A.3.1 CLI Binding
  - 模板替换正确性
  - Sandbox 执行（无文件系统逃逸）
  - Secret 不出现在 process args

A.3.2 MCP Binding
  - JSON-RPC 2.0 协议合规
  - param_mapping 双向正确
  - 错误码映射符合 §9.4 表

A.3.3 REST Binding
  - HTTP method 正确
  - TLS 验证不被禁用
  - 状态码映射符合 §9.5 表

A.3.4 AppFunction Binding
  - Platform-specific 调用语义
  - 用户对话框不被绕过
```

### A.4 Security Layer Conformance

```
A.4.1 Capability Token Verification
  - 签名验证正确
  - 过期 token 必须被拒绝
  - Resource pattern 匹配规则正确

A.4.2 Visibility = Authorization
  - 无 token 时 Dangerous tool 不在 LLM tool list 中
  - 有 token 时 Dangerous tool 进入 LLM tool list
  - Revoke token 后下一轮 LLM tool list 不含该 tool

A.4.3 Constitutional Guard
  - --dangerously-skip-permissions 不绕过 CG1-CG3
  - Secret 检测到必须中止 dispatch
  - Forbidden pattern 检测到必须中止 dispatch

A.4.4 Token Attenuation
  - Child token 严格 ⊆ parent token
  - 任何扩权尝试 → ESCALATION_ATTEMPT
```

### A.5 Capacity Layer Conformance

```
A.5.1 Tier Transitions
  - Cold → Warm 在首次成功调用后发生
  - Warm → Hot 在 5 calls / 7 days 后发生
  - Hot → Warm 在 14 days 未调用后发生
  - Hysteresis 规则被遵守

A.5.2 Hot Tier Capacity Limits
  - |HotTier| ≤ HotTier.capacity_max
  - Hot tier total tokens ≤ size_budget

A.5.3 tool.search 接口
  - 接受 intent string
  - 返回 top max_results 排序结果
  - HNSW 检索 p99 < 100ms
```

### A.6 Reliability Layer Conformance

```
A.6.1 Health Monitoring
  - 5-min sliding window 实现
  - Health classification 阈值正确

A.6.2 Circuit Breaker State Machine
  - Closed → Open 在 error_rate > 50% AND calls ≥ 10 时
  - Open → Half-Open 在 cooldown 后
  - Half-Open → Closed 在 3 successful probes 后
  - Half-Open → Open 在任一 probe 失败时
  - Cooldown exponential backoff 正确

A.6.3 Fallback Chain
  - Fallback 在 Open 时触发（若声明）
  - 链长度 ≤ 3
  - 不成环
```

### A.7 互操作合规

```
A.7.1 MCP Compatibility (IO-MCP-1 至 IO-MCP-5)
A.7.2 OpenAI Tools Compatibility (IO-OAI-1 至 IO-OAI-3)
A.7.3 Cross-Implementation Interoperability
  - 同一 tool 在两个 ATD 实现中行为等价
  - Capability token 跨实现可验证
```

---

## Appendix B: Complete JSON Schema (Excerpt)

完整 Schema 见 ANOS reference implementation `crates/anos-tool-dispatch/schemas/`。本附录仅展示核心结构。

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://atd.spec/v1.0/tool-definition.json",
  "title": "ATD ToolDefinition v1.0",
  "type": "object",
  "required": [
    "atd_version", "id", "version", "name", "description",
    "capability", "input", "output", "bindings", "safety"
  ],
  "properties": {
    "atd_version": {
      "type": "string",
      "const": "1.0"
    },
    "id": {
      "type": "string",
      "pattern": "^(anos|host|mcp|vendor:[a-z0-9-]+|community:[a-z0-9-]+|custom):[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*)+$",
      "maxLength": 256
    },
    "version": {
      "type": "string",
      "pattern": "^\\d+\\.\\d+\\.\\d+(-[a-z0-9-]+)?$"
    },
    "name": { "type": "string", "minLength": 1, "maxLength": 100 },
    "description": { "type": "string", "minLength": 1, "maxLength": 2000 },
    "capability": { "$ref": "#/$defs/CapabilityDescriptor" },
    "input": { "$ref": "https://json-schema.org/draft/2020-12/schema" },
    "output": { "$ref": "https://json-schema.org/draft/2020-12/schema" },
    "errors": {
      "type": "array",
      "items": { "$ref": "#/$defs/ErrorDef" }
    },
    "bindings": { "$ref": "#/$defs/BindingSet" },
    "safety": { "$ref": "#/$defs/SafetyClassification" },
    "resources": { "$ref": "#/$defs/ResourceConstraints" },
    "trust": { "$ref": "#/$defs/TrustMetadata" },
    "compatibility": { "$ref": "#/$defs/CompatibilityInfo" },
    "fallback": { "$ref": "#/$defs/FallbackSpec" }
  },
  "$defs": {
    "CapabilityDescriptor": {
      "type": "object",
      "required": ["domain", "actions"],
      "properties": {
        "domain": { "type": "string" },
        "actions": { "type": "array", "items": { "type": "string" } },
        "tags": { "type": "array", "items": { "type": "string" } },
        "intent_examples": {
          "type": "array",
          "items": { "type": "string" },
          "minItems": 1,
          "maxItems": 20
        }
      }
    },
    "BindingSet": {
      "type": "object",
      "minProperties": 1,
      "properties": {
        "cli": { "$ref": "#/$defs/CliBinding" },
        "mcp": { "$ref": "#/$defs/McpBinding" },
        "rest": { "$ref": "#/$defs/RestBinding" },
        "appfunction": { "$ref": "#/$defs/AppFunctionBinding" }
      }
    },
    "SafetyClassification": {
      "type": "object",
      "required": ["level"],
      "properties": {
        "level": {
          "type": "string",
          "enum": ["read", "write", "dangerous", "system"]
        },
        "requires_confirm": { "type": "boolean", "default": false },
        "supports_dry_run": { "type": "boolean", "default": false },
        "data_sensitivity": {
          "type": "string",
          "enum": ["public", "internal", "confidential", "restricted"]
        },
        "side_effects": {
          "type": "array",
          "items": {
            "type": "string",
            "enum": ["none", "filesystem", "network", "process", "device", "external_state"]
          }
        }
      }
    },
    "ResourceConstraints": {
      "type": "object",
      "properties": {
        "timeout_ms": { "type": "integer", "minimum": 100, "maximum": 600000 },
        "max_concurrent": { "type": "integer", "minimum": 1, "maximum": 256 },
        "rate_limit": {
          "type": "object",
          "properties": {
            "max": { "type": "integer" },
            "window_secs": { "type": "integer" }
          }
        },
        "estimated_tokens": { "type": "integer" },
        "estimated_latency_ms": { "type": "integer" },
        "max_result_size_bytes": { "type": "integer" },
        "retry_policy": {
          "type": "object",
          "properties": {
            "safe_to_retry": { "type": "boolean" },
            "recommended_retry": { "type": "integer" },
            "backoff_ms": { "type": "array", "items": { "type": "integer" } }
          }
        }
      }
    }
  }
}
```

---

## Appendix C: Field Enumerations

### C.1 Visibility Levels

```
read       - 只读，无副作用
write      - 写入，有副作用但通常安全
dangerous  - 危险，需要 /allow 授权
system     - 系统级，永不暴露给 LLM
```

### C.2 Side Effects

```
none           - 无副作用
filesystem     - 文件系统读写
network        - 网络通信
process        - 进程创建/终止
device         - 设备访问（camera/mic/sensor）
external_state - 外部系统状态变更（API 调用）
```

### C.3 Data Sensitivity

```
public        - 公开数据
internal      - 组织内部数据
confidential  - 机密数据
restricted    - 高度受限数据（PII, 财务, 医疗）
```

### C.4 Error Class

```
Transient      - 临时失败，应该重试
Permanent      - 永久失败，不应重试
Environmental  - 环境失败，应检查后重试
```

### C.5 Trust Level

```
0  Unverified    - 未验证发布者
1  Self-Signed   - 自签名
2  Authenticated - 经身份验证（MCP server, vendor 注册）
3  Reviewed      - 经人工审核（community registry）
4  Certified     - 经 APWG 认证
```

### C.6 Binding Protocol

```
cli            - 本地命令行
mcp            - Model Context Protocol
rest           - HTTP REST API
appfunction    - OS 级应用功能
grpc           - gRPC（v1.1 预留）
websocket      - WebSocket（v1.1 预留）
event          - CloudEvents（v1.2 预留）
thing          - W3C WoT TD（v1.2 预留）
```

### C.7 Tier

```
hot   - 在 system prompt 中
warm  - 在本地 HNSW 索引中
cold  - 在 remote registry 中
```

### C.8 Circuit Breaker State

```
closed     - 正常运行
open       - 熔断
half_open  - 探测恢复
```

---

## Appendix D: Complete Error Code Reference

### D.1 Capability Errors (1xx)

```
PERMISSION_DENIED      101  无 token 或 token 不授权
TOKEN_EXPIRED          102  token 过期
TOKEN_INVALID          103  签名无效
ESCALATION_ATTEMPT     104  尝试扩权
RATE_LIMITED           105  超过 rate limit（用于 token 级和 tool 级两种场景，
                            通过 reason 字段区分："token_rate_limit" / "tool_rate_limit"）
BUDGET_EXCEEDED        106  超过 cost budget
USAGE_EXCEEDED         107  超过 max_uses
```

### D.2 Resolution Errors (2xx)

```
TOOL_NOT_FOUND         201  tool_id 不存在（或 endpoint 路由级 404）
TOOL_DEPRECATED        202  tool 已 deprecated
PLATFORM_UNSUPPORTED   203  当前 platform 不支持
BINDING_UNAVAILABLE    204  binding 所需的 external tool 缺失
VERSION_INCOMPATIBLE   205  ATD 版本不兼容
RESOURCE_NOT_FOUND     206  endpoint 可达但请求的具体 resource 不存在（payload 级 404）
```

### D.3 Validation Errors (3xx)

```
VALIDATION_ERROR       301  参数 schema 验证失败（顶层错误）
INVALID_REQUEST        302  请求格式错误
RESULT_INVALID         303  结果不符合 output schema
SCHEMA_DRIFT           304  binding 返回的 schema 与声明不一致
MISSING_FIELD          305  ToolDefinition / 请求中缺少 MUST 字段
                            （VALIDATION_ERROR 的特化子类，便于实现层针对性诊断）
```

### D.4 Execution Errors (4xx)

```
TIMEOUT                401  执行超时
EXECUTION_ERROR        402  binding 执行失败
SANDBOX_VIOLATION      403  违反 sandbox 约束
RESOURCE_EXHAUSTED     404  资源耗尽
TOOL_CIRCUIT_OPEN      405  circuit breaker 处于 Open 状态
```

### D.5 Constitutional Errors (5xx)

```
SECRET_DETECTED            501  Secret 被检测到
FORBIDDEN_PATTERN          502  匹配 forbidden pattern
CONSTITUTIONAL_VIOLATION   503  其他宪法守卫触发
```

### D.6 Internal Errors (9xx)

```
INTERNAL_ERROR         901  内部错误
NETWORK_ERROR          902  网络错误
STORAGE_ERROR          903  持久化失败
UNKNOWN_ERROR          999  未分类错误
```

### D.7 错误属性表

```
Error Code              error_class    retryable  retry_after
─────────────────────────────────────────────────────────────
PERMISSION_DENIED       Permanent      false      -
TOKEN_EXPIRED           Permanent      false      -  (refresh first)
RATE_LIMITED            Transient      true       Yes
BUDGET_EXCEEDED         Permanent      false      -
TOOL_NOT_FOUND          Permanent      false      -
PLATFORM_UNSUPPORTED    Permanent      false      -
BINDING_UNAVAILABLE     Environmental  true       30s
VALIDATION_ERROR        Permanent      false      -
MISSING_FIELD           Permanent      false      -
RESOURCE_NOT_FOUND      Permanent      false      -
TIMEOUT                 Transient      true       backoff
EXECUTION_ERROR         Transient      true       backoff
SANDBOX_VIOLATION       Permanent      false      -
TOOL_CIRCUIT_OPEN       Transient      true       circuit cooldown
SECRET_DETECTED         Permanent      false      -
CONSTITUTIONAL_VIOLATION Permanent     false      -
INTERNAL_ERROR          Transient      true       backoff
NETWORK_ERROR           Transient      true       backoff
```

---

## Appendix E: Migration from MCP / OpenAI Tools / LangChain

### E.1 From MCP

```
现有 MCP server → ATD 接入

Step 1: 启动 MCP server
  无需任何修改

Step 2: 在 ATD runtime 注册
  /mcp add <command>

Step 3: ATD 自动生成 ToolDefinition
  id           = mcp:<server>.<tool>
  bindings.mcp = { server_id, mcp_tool_name, transport }
  其他字段从 MCP 元数据派生

Step 4: 应用 ATD security/capacity/reliability 层
  默认 visibility = "write"
  默认 capacity tier = "warm"（首次调用后升 hot）
  默认 circuit breaker = enabled

Step 5: 使用
  agent 通过 ATD dispatch 调用，行为对 MCP server 透明
```

### E.2 From OpenAI Tools (Function Calling)

```
现有 OpenAI Tools schema → ATD ToolDefinition

OpenAI:
{
  "type": "function",
  "function": {
    "name": "get_weather",
    "description": "Get weather for location",
    "parameters": { ... }
  }
}

ATD:
{
  "atd_version": "1.0",
  "id": "custom:weather.get",
  "version": "1.0.0",
  "name": "get_weather",
  "description": "Get weather for location",
  "capability": {
    "domain": "weather",
    "actions": ["get"],
    "intent_examples": ["check weather in Boston"]
  },
  "input": { ... 与 OpenAI parameters 相同 ... },
  "output": { "type": "object", ... },
  "bindings": {
    "rest": { ... 用户提供 ... }
  },
  "safety": { "level": "read" }
}
```

### E.3 From LangChain Tools

```
Python：
from atd_sdk import wrap_langchain_tool
from langchain.tools import StructuredTool

lc_tool = StructuredTool.from_function(my_func, ...)
atd_def = wrap_langchain_tool(lc_tool, id="custom:my.tool")
atd_runtime.register(atd_def)

ATD 自动：
  - 从 LangChain tool.args_schema 派生 input schema
  - 从 LangChain tool 创建 in-process binding
  - 默认 visibility = "write"（保守）
  - 默认 capacity tier = "warm"
```

### E.4 From Apple App Intents / Android AppFunctions

```
现有 platform-native intent → ATD

iOS App Intent:
@AppIntent(title: "Schedule Meeting")
struct ScheduleMeetingIntent: AppIntent {
  @Parameter var participants: [Person]
  @Parameter var time: Date
  func perform() async throws -> some IntentResult { ... }
}

ATD:
{
  "atd_version": "1.0",
  "id": "custom:calendar.schedule_meeting",
  "version": "1.0.0",
  "name": "Schedule Meeting",
  ...
  "bindings": {
    "appfunction": {
      "platform": "ios",
      "target": {
        "bundle_id": "com.example.calendar",
        "intent_name": "ScheduleMeetingIntent"
      }
    }
  }
}
```

### E.5 双向桥接：ATD ↔ External Protocol

```
ATD → MCP（暴露 ATD 工具为 MCP server）：
  ATD runtime 提供 atd-mcp-server adapter
  外部 MCP client 可调用 ATD 工具
  限制：仅 mcp 兼容子集（不暴露 capacity/reliability 元数据）

ATD → OpenAI Tools（导出为 OpenAI function calling）：
  Compact ATD → OpenAI Tools schema 投影
  用于：在不能直接接入 ATD 的 LLM provider 上使用 ATD 工具

ATD → LangChain（导出为 LangChain BaseTool）：
  Python SDK 提供 atd_def_to_langchain_tool()
  用于：在 LangChain 应用中使用 ATD 工具
```

---

_2026-04 — ATD v1.0 Whitepaper_

_本白皮书与 [Toward General Autonomous Agent](./toward-general-autonomous-agent.md) 互为支撑：GAA 定义 agent 自身，ATD 定义 agent 与世界的接口。_
