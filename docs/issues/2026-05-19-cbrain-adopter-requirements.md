# cbrain adopter requirements — embodied agent / robotics simulation

**Layer:** adopter (cbrain)
**Status:** ready-for-atd (新需求，请评估 SP 排期)
**Effort:** P0 ~3 周；P1 ~4–6 周；P2 ~继续迭代
**Filed:** 2026-05-19
**Adopter repo:** `/home/nan/code/cbrain` （参见 `cbrain/docs/research/14-仿真环境工程实现方案.md` 和 `15-cbrain-对atd的需求.md`）
**Adopter contact:** cbrain S2 team

---

## 1. Summary

cbrain 是一个具身机器人「大脑」（S2 自主决策层）项目，已经在 `docs/research/13-依赖与组件复用决策.md` 经过尽调决策**把 ATD 作为 cognitive plane 的统一工具调度协议**（与 ROS 2 在控制平面的混合架构）。现在进入工程实现阶段（`docs/research/14`），第一个里程碑 W1–W10 要把 MuJoCo 仿真 + Hermes Agent + cbrain-sim 通过 ATD 协议串起来。

工程过程中发现 ATD 当前实现（commit head, 2026-05）虽然 wire / protocol / type system 已基本就绪，但有若干 **gap 阻塞 cbrain 直接采用**。本 issue 把所有 gap 按优先级汇总，作为 cbrain adopter 的依赖清单，请 atd 团队评估提前排期。

cbrain 这边会同时维护一份 vendored shim（实现这些 gap 的最小子集，跑通 W1–W7）；一旦 atd 团队官方实现就绪，cbrain 立即切回上游，不希望长期维护 fork。

---

## 2. Current ATD state（cbrain 视角已确认）

✅ 已可用：
- Wire format（4-byte BE length + UTF-8 JSON，≤10 MiB）—— `crates/atd-protocol`, `python/src/atd_client/wire.py` (43 LOC)
- Message types: `ping`/`hello`/`tool_list`/`tool_schema`/`run_tool` + responses —— `python/src/atd_client/protocol.py` (64 LOC)
- Type surface: `ToolDefinition`/`ToolTier`/`ToolVisibility`/`ToolCapability`/`ToolResources`/`ToolErrorDef`/... —— `python/src/atd_client/types.py` (206 LOC)
- Python client: `AtdClient` + `AtdClientSync` —— `python/src/atd_client/client.py` (320 LOC)
- Reference server: `atd-ref-server` (Rust) —— 9 built-in tools
- MCP bridge: `atd-mcp-bridge` (Rust) —— Hermes 端验证过

❌ cbrain 阻塞项 见 §3。

---

## 3. Required gaps（按 cbrain 优先级）

### P0 · 阻塞 cbrain W1（必须有，否则 cbrain 没法基于 ATD 跑）

#### P0-1. **Python server-side runtime** —— 最关键

**Gap**：`atd_client` Python 包目前**只有 client side**。`transport.py` 21 行只有 `connect_unix()`，没有 `serve_unix()`。cbrain-sim 需要在 Python 进程里 expose ATD server（因为 MuJoCo 是 Python，且 simulator 是 stateful singleton 不能跨进程共享）。

**Required API**：

```python
# 提案：atd_server 或 atd_client.server
from atd_server import AtdServer, ToolHandler
from atd_client.types import ToolDefinition, ToolSuccess, ToolFailure

server = AtdServer(socket_path="/tmp/cbrain-sim.sock", server_id="cbrain-sim")

@server.register(definition=ToolDefinition(
    id="cbrain:perception.snapshot",
    name="Snapshot",
    tier=ToolTier.WARM,
    visibility=ToolVisibility.READ,
    # ...
))
async def snapshot(args: dict) -> ToolSuccess | ToolFailure:
    frame = my_simulator.render()
    return ToolSuccess(data={"rgb_b64": ..., "depth_b64": ...})

await server.serve()  # block until SIGTERM
```

**核心职责**：
1. Unix-socket listen + multi-client accept
2. Wire frame read/write 复用 `atd_client.wire`
3. 握手（接 `hello`，根据 server policy 颁发 `granted_capabilities`）
4. 分发：`tool_list` / `tool_schema` / `run_tool` → 注册的 handler
5. Tier-aware deadline 强制（基于 ToolDefinition.tier 自动 wrap handler 于 `asyncio.wait_for`）
6. Capability gate（hello 时的能力交集决定 handler 能否被调）
7. Dry-run（接 `run_tool {dry_run: true}` 时短路返回 args_preview）
8. 错误码统一封装（handler raise → server 转 ATD error envelope）

**Effort 估算**：~500–800 LOC Python，可以与 `atd-ref-server` Rust 实现对齐。

**为什么不让 cbrain 自己写**：cbrain 自己写一份意味着两份代码漂移，cbrain 没有维护 ATD 协议的人力；且未来其他 Python adopter（如 healthkit_cli 后续可能转 Python）也会遇到。这是 ATD ecosystem 的基础设施，应该官方维护。

**cbrain 临时方案**：在 cbrain `sim/cbrain_sim/atd_shim/` 下 vendor 一份 minimal server，绑死在 cbrain repo 内，标注 `# TODO: upstream when atd ships official server runtime`，并跟踪本 issue。

---

#### P0-2. **`atd-mcp-bridge` release binary（无需 cargo build）**

**Gap**：`atd-mcp-bridge` 是 Hermes ↔ cbrain-sim 的关键桥（Hermes 视角的 MCP server，背后转 ATD wire）。当前需要 `cargo build --release -p atd-mcp-bridge`，新人 onboard 时是个坑。

**Required**：
- GitHub release 发布预编译二进制（Linux x86_64 / Linux aarch64 / macOS arm64）
- 或：pypi 发布一个 wrapper（`atd-mcp-bridge` Python 包通过 maturin 打包 Rust binary）
- 或：`uv tool install atd-mcp-bridge` 路径

**Effort**：~2–3 天 CI 工作。

**cbrain 临时方案**：cbrain 的 `scripts/setup.sh` 内会自动 `cd ../atd && cargo build --release -p atd-mcp-bridge`，但这强假设 cbrain 和 atd 同级目录。

---

### P1 · 阻塞 cbrain W7+（强烈需要，影响 demo 完整度）

#### P1-3. **Cancel / Abort 语义**

**Gap**：当前协议 request/response 是同步 1:1。机器人 atomic skill（pick / place / move）单次执行 1–10 秒，agent 可能想中途取消（用户改主意、检测到危险、超 budget）。`docs/protocol/wire-format.md` §"Non-goals for v0.1.0" 明确列出"无 streaming / 无 cancel"。

**Required**：
- 新消息类型：`cancel { request_id: str }` 客户端 → 服务端
- 响应：`cancelled { request_id, partial_result? }`
- Server side handler 收到 cancellation token（如 `asyncio.CancelledError`），优雅清理（停止 atomic skill 中途 motion，gripper 维持当前状态）
- error_codes.md 加 `1030 cancelled`

**为什么重要**：物理 agent 不能"不停止"——cbrain S2 的 L1–L5 自主级别（参见 cbrain 13/12 报告）有"高优先级 goal 抢占"语义，必须能 cancel 正在跑的低优先级 skill。

**Effort**：~1–2 周（协议 + Rust runtime + Python runtime + 测试）。

**cbrain 临时方案**：在 W7 之前 cbrain skill 不超过 5s budget，规避 cancel；W7 后如果 atd 未实现，cbrain 在 server 层加 hack（应用层 cancel token via shared state）。

---

#### P1-4. **Chunked / Streaming 响应**

**Gap**：当前帧上限 10 MiB；机器人视觉数据（640×480×3 RGB + depth = ~1 MiB，多相机 30 FPS）累积起来很容易爆。`scene.describe` 调 VLM 也是高延迟操作，分块返回中间结果更优。

**Required**：
- 新响应类型：`tool_result_chunk { request_id, seq, data, last: bool }`
- Client SDK：`call_streamed(tool_id, args) -> AsyncIterator[chunk]`
- Server SDK：handler 返回 `AsyncGenerator[chunk]` 的支持
- 帧大小上限可能仍是 10 MiB per chunk（不变），但允许多 chunk 拼装

**Effort**：~2 周。

**cbrain 临时方案**：W2–W6 内传图像走 base64 inline，单帧压制在 1 MiB 内（jpeg 编码降到 ~100 KB）；不做 streaming。视频回放走文件路径而非 ATD。

---

#### P1-5. **Binary payload 支持（避免 base64 开销）**

**Gap**：图像 / 点云 / 触觉数据 base64 编码 33% overhead，对 30 Hz 视觉流影响明显。当前 wire 是纯 JSON UTF-8。

**Required（两种方案二选一）**：

**方案 A**：扩展 wire 增加 binary frame type
```
[1-byte frame_kind: 0x00=JSON, 0x01=binary]
[4-byte BE length]
[payload]
```
JSON 中通过 reference（`{"image_ref": "frame:42"}`）引用 binary frame。**协议层 breaking change**。

**方案 B**：用 multipart-style：单条消息含多个 part，header part 是 JSON，data parts 是 binary blob，length-prefix 各自。**协议小改动**。

**为什么重要**：cbrain 30Hz 双相机 RGB-D（~30 MB/s）走 base64 ≈ 40 MB/s。对 sim 还好（本机 UDS），但未来真机走网络就成问题。

**Effort**：协议 ~3 天 + Rust impl ~1 周 + Python impl ~1 周。

**cbrain 临时方案**：W1–W10 仅用 base64 jpeg（≈100 KB/frame），可接受。

---

#### P1-6. **`error-codes.md` 扩展机器人语义**

**Gap**：当前 error_codes.md 以 ATD 协议层错误（1000–1099）和示例 tool 错误为主。cbrain 需要在 tool 自有错误码段（2000+）定义机器人/sim 特有错误，希望与 atd 社区约定命名空间避免冲突。

**Required**：
- error-codes.md 增加 "Adopter namespace allocation"（如 cbrain 用 2000–2099, healthkit 用 3000+）
- ToolErrorDef 校验：tool definition 声明的 errors 必须在 ToolDefinition.publisher 命名空间内

**Effort**：~2 天文档 + 简单校验。

**cbrain 临时方案**：先用 2000+ 自主分配，提 PR 给 atd 加入 README 的 namespace 表。

---

### P2 · 长期需要（不阻塞 PoC 但影响生产化）

#### P2-7. **Conformance suite Python runner**

**Gap**：`atd-conformance` 当前只能 Rust target 跑（36 fixture）。cbrain server 是 Python 实现，希望能 `pytest`-friendly 跑 conformance 验证自己合规。

**Required**：`atd-conformance-py`（pip-installable），调任意 ATD server endpoint 跑全套 fixture。

**Effort**：~1 周（既然 fixture 是声明式数据，调用是协议事件，Python 实现不难）。

---

#### P2-8. **Audit middleware hooks（pre / post / on_error）**

**Gap**：cbrain 需要在每个 tool call 前后插入 Merkle 链审计（参见 cbrain 12 报告"Trace Lake + Merkle"）。当前 server runtime 没有暴露 middleware extension point。

**Required**：
```python
@server.middleware(stage="post_call")
async def merkle_audit(request, response, next):
    entry = build_audit_entry(request, response)
    merkle_chain.append(entry)
    return response
```

**Effort**：~1 周。

---

#### P2-9. **Multi-client stateful session 语义**

**Gap**：当前文档说 "Stateless sessions ... server does not track session state between connections"。cbrain-sim 是 stateful（MuJoCo MjData 是 singleton），多个 client（Hermes + atd-cli debug + 评测 harness）同时连时，**所有 client 共享同一个仿真状态**。这与"无 session state"看起来矛盾。

**Required**：
- 文档化两种语义：「per-connection state」vs「shared world state」（server 选其一并声明）
- Server policy 字段：`hello_ack.session_model: "per_connection" | "shared_world"`

**Effort**：~3 天文档 + 1 周 Rust runtime 标注。

---

#### P2-10. **Cross-version tool schema migration**

**Gap**：ToolDefinition 当前无 version 字段。cbrain skill / agent 库依赖具体 tool 行为，未来 tool 升级（如 `manipulation.pick` v2 新增 `force` 参数）需要 graceful migration。

**Required**：
- ToolDefinition 加 `version: str` 字段
- 客户端调用支持 `tool_id@version` 寻址（默认 latest）
- Server 可同时注册多个版本

**Effort**：~2 周。

---

#### P2-11. **细化 Capability 命名空间约定**

**Gap**：`ToolCapability(domain, action)` 是开放的。机器人 capability 应有约定：
- `perception.read` / `perception.depth.read`
- `manipulation.write` / `manipulation.dangerous.write`
- `world.reset` / `world.set_state`
- `task.lifecycle.write`

**Required**：在 `docs/archive/design.md` 或 `docs/protocol/` 增加 "Capability naming guidelines"，列出推荐 domain × action 矩阵。

**Effort**：~1 周文档 + 社区讨论。

---

## 4. cbrain 临时蛮力（与 atd 团队进度并行）

cbrain repo 在 `sim/cbrain_sim/atd_shim/` 下会维护一份 minimal Python ATD server side（~300 LOC），**仅满足 W1–W7 demo**。它实现：

- 完整 wire / protocol（与 atd 字节兼容）
- `ping` / `hello` / `tool_list` / `tool_schema` / `run_tool`
- 同步 1:1 request/response（无 cancel / 无 streaming）
- Tier-aware `asyncio.wait_for` deadline
- Handler-level capability check
- pre/post middleware hook（cbrain 自用 audit 必须）

**不实现**：Cancel / Streaming / Binary payload / Conformance runner —— 等 atd 上游。

**协议字节兼容**：cbrain shim 严格按 `docs/protocol/wire-format.md` v0.1.0 实现；atd 升级 spec 时 cbrain 跟进。

**切换计划**：atd 一旦正式发布 `atd-server` Python 包，cbrain 在 1 周内切换，删除 shim。本 issue 关闭。

---

## 5. 时间线建议（与 cbrain 路线图对齐）

cbrain 的 W1–W10 路线图（见 `cbrain/docs/research/14`）：

| atd 需求 | cbrain 需要它的时间 | atd 现状 | 建议 atd SP |
|---------|-------------------|----------|-----------|
| P0-1 server runtime | W1 (即刻) | 缺 | **SP-server-py-v1**，目标 2 周内 |
| P0-2 mcp-bridge binary | W6 (~2 月后) | 部分 | **SP-release-binaries**，目标 1 月内 |
| P1-3 cancel | W7 (~2 月后) | 缺 | **SP-cancel-v1**，目标 6 周内 |
| P1-4 streaming | W9 (~3 月后) | 缺 | SP Phase 2 |
| P1-5 binary payload | W10+ | 缺 | SP Phase 2 |
| P1-6 error namespace | W4 (~1 月后) | 部分 | 文档级 SP-3 天 |
| P2-7 conformance py | M4+ | 缺 | Phase 2 |
| P2-8 middleware hooks | W8 (~2 月后) | 缺 | 与 P0-1 同期实现可省力 |
| P2-9 session 文档 | M3+ | 模糊 | 文档级 SP-3 天 |
| P2-10 versioning | M6+ | 缺 | Phase 3 |
| P2-11 capability naming | M3+ | 缺 | 文档级 SP-1 周 |

**最关键**：P0-1（server runtime）—— cbrain W1 就需要。若 atd 团队 2 周内能产出 alpha 实现，cbrain 切上去后省 ~300 LOC shim 维护。否则 cbrain shim 跑 1–2 月没问题，但**两份实现长期漂移有风险**。

---

## 6. cbrain 愿意做的回馈

- **测试 case**：cbrain shim 完成后会把内部测试用例（基于 mock simulator）整理成 PR 提到 `atd-conformance` —— 物理 agent 视角的协议测试场景对 atd 社区有价值。
- **设计反馈**：cbrain 是第一个"stateful, physical, multi-client" adopter，能给 atd 团队提供 healthkit/celia 之外的 use case 验证。
- **文档**：本 issue 本身是 cbrain → atd 反馈的开始。cbrain 14 报告 §5.4 / §5.5 关于 ATD 集成的内容欢迎 atd 团队 review。

---

## 7. 关联文档

**cbrain 端**：
- `cbrain/docs/research/12-S2整合设计与仿真评测环境.md` —— 战略层
- `cbrain/docs/research/13-依赖与组件复用决策.md` —— ATD 作为 cognitive plane 协议的决策
- `cbrain/docs/research/14-仿真环境工程实现方案.md` —— 本期 PoC 工程方案
- `cbrain/docs/research/15-cbrain-对atd的需求.md` —— 本 issue 的 cbrain 视角镜像

**atd 端**：
- `docs/archive/design.md` —— 协议设计文档
- `docs/protocol/wire-format.md` —— v0.1.0 wire spec
- `docs/protocol/error-codes.md` —— 错误码表（cbrain 申请 2000–2099 命名空间；P1-6 will formalize）
- `crates/atd-protocol` / `python/src/atd_client/` —— client / protocol crates
- `python/src/atd_server/` —— **NEW** Python server runtime (P0-1 ship, 2026-05-19)
- `docs/archive/superpowers/specs/2026-05-19-sp-server-py-v1-design.md` —— SP-server-py-v1 design
- `docs/archive/superpowers/plans/2026-05-19-sp-server-py-v1.md` —— SP-server-py-v1 phasing plan
- `docs/integrations/python-server.md` —— **NEW** cbrain-style hello-world + adopter guide

---

## 8. Recommended next step（给 atd 团队）

> **2026-05-19 update**: steps 1-2 done same session, ahead of expectation. Steps 3-4 revised below.

**Original cbrain ask (kept for history):**

1. ~~atd 团队 ACK 本 issue 并标注每项的状态（accept / defer / 需求澄清）；~~ ✅ done, see §9.1
2. ~~**先动 P0-1 Python server runtime** —— 这是当前唯一硬阻塞；~~ ✅ done, **shipped same session** (SP-server-py-v1 Phase A-H)
3. cbrain 维护方在 cbrain repo 用 vendored shim 开干 W1，不阻塞； ⚠ obsoleted — cbrain can consume upstream `atd_server` directly; shim no longer needed
4. atd 团队 ship 后 cbrain 切换 + 关闭本 issue。 → now reads: **cbrain 切换 + close P0-1 row + remaining SPs (P0-2 / P1-3+4 / P1-6) follow their own timelines**

**Post-ship next step (revised):**

1. cbrain team: merge `worktree-cbrain-triage-and-sp-server-py-v1` to master (or cherry-pick the 8 atd-side commits), pull `atd_server` via path-dep, follow `docs/integrations/python-server.md`. Estimated ~2h.
2. atd team: pick up the smaller queued SPs in priority order — **P1-6 (error namespace, ~2 days)** + **P2-9 (session model docs, ~3 days)** are the lowest-hanging that unblock cross-adopter coordination. **P0-2 (release binaries)** removes cbrain's onboarding paper cut at W6.
3. atd team: file `SP-cancel-streaming-v1` design before cbrain W7 — the Phase-2 seam in `atd_server` is ready, but the wire-level `request_id` decision (added to `Request::RunTool`?) needs cross-adopter agreement first.
4. close criteria: this issue closes when P0-2 + P1-3+4 + P1-6 ship. P2-* SPs proceed independently and are tracked outside this umbrella.

如需 cbrain 团队提供更多细节或同步会议，请 ping `cbrain/docs/research/` 或在本 issue 评论。

---

**Filed by:** cbrain S2 team, 2026-05-19

---

## 9. ATD team triage response (2026-05-19)

ACK 已收到。本节是 ATD 团队对 §3 全部 11 项的 per-item 裁决与 SP 排期。**P0-1 已立刻进入设计阶段**（见 §9.2）；其余项给出明确归类与时间线，避免长期悬而未决。

### 9.1 Per-item verdict

| # | cbrain ask | 裁决 | SP slot | 备注 |
|---|---|---|---|---|
| **P0-1** | Python server runtime | ✅ **shipped 2026-05-19** | **`SP-server-py-v1`** Phase A-H | spec + plan + all 8 phases landed same session (8 commits `c79317d`→`aeab2f5`): skeleton / handshake / registry / dispatch / middleware (P2-8 bundled) / conformance subset (22/24 fixtures, 96% coverage) / docs. cbrain swap = `path = "../atd/python"` + `from atd_server import AtdServer` + delete `cbrain/sim/cbrain_sim/atd_shim/` (~2h if §9.3 guidance followed). |
| **P0-2** | `atd-mcp-bridge` 预编译二进制 | ⏳ **queued — not yet started** | `SP-release-binaries-v1` | next action: record the decision as an ADR in `docs/adr/`. Scope: GitHub Actions matrix (linux x86_64/aarch64 + macOS arm64) + tag-triggered GH release; pypi wrapper via maturin optional. Effort ~2-3 days CI work. Target: 1 month. **Not blocked by anything**; can be picked up by anyone with GitHub Actions familiarity. |
| **P1-3** | Cancel / abort | ⏳ **queued · merged with P1-4** | **`SP-cancel-streaming-v1`** | next action: file the umbrella design spec; uses per-`request_id` router on connection. **SP-server-py-v1 left the Phase-2 seam** (see `python/src/atd_server/server.py:163-194` — `_serve_one_connection` is strictly serial; v2 just replaces the inner `await self._dispatch(msg, ctx)` with `asyncio.create_task(...)` + request_id router without changing the registry/handler API). Supersedes legacy `2026-04-24-dispatch-session-cancel-not-implemented.md`. Target 6 weeks. |
| **P1-4** | Chunked / streaming 响应 | ⏳ **queued · merged with P1-3** | `SP-cancel-streaming-v1` | 见上 |
| **P1-5** | Binary payload | 🟡 **deferred — design SP only** | `SP-binary-frames-v1` (design only) | 协议 breaking change. cbrain W1-W10 base64 jpeg ≤100KB unchanged — no immediate pressure. Recommendation: don't open the SP until either (a) cbrain hits real-robot stage and measures the base64 overhead as a real bottleneck, or (b) healthkit / celia presents an independent need. **Risk if opened prematurely**: design churn against a single use case. |
| **P1-6** | Adopter error 命名空间 | ⏳ **queued — small doc SP** | `SP-error-namespace-v1` (~2 天) | next action: PR `docs/protocol/error-codes.md` with allocation table (cbrain 2000-2099 / healthkit 3000-3099 / celia 4000-4099) + add a `ToolErrorDef` validator pass in `crates/atd-protocol/src/sanitize.rs`. `atd_server.errors` already uses int codes consistent with this scheme (Phase E commit `dd9116d` allocated 1004=`ERR_DEADLINE_EXCEEDED` and 1005=`ERR_INVALID_ARGS` in the protocol range; the adopter range starts at 2000 cleanly). Target 1 month. |
| **P2-7** | Python conformance runner | 🟢 **NOW UNBLOCKED — was waiting on P0-1** | `SP-conformance-py-v1` | next action: file the spec. Scope shrunk: `python/tests/test_server_conformance.py` (this SP) already does the hard work of fixture-parsing + partial-match + reference-server setup; the future SP just extracts that into an `atd-conformance-py` CLI binary that points at arbitrary external servers. Effort revised **down** from ~1 week to ~3 days given the Phase-G work. Target: P0-1 + 1 month (i.e. by 2026-06-19). |
| **P2-8** | Middleware hooks | ✅ **shipped 2026-05-19** (bundled in P0-1) | `SP-server-py-v1` Phase F (`91bf1b5`) | `@server.middleware(stage="pre_call" \| "post_call" \| "on_error")` 三阶段 + call_next 链；8 tests including the LIFO order proof. **Rust runtime parity** is its own sibling SP (not opened yet, low priority — cbrain only needs the Python side). |
| **P2-9** | Session model 文档 | ⏳ **queued — small doc SP** | `SP-session-model-doc` (~3 天) | next action: edit `docs/protocol/wire-format.md` to add "Session models" section; document `per_connection` (current Rust + Python behavior) vs `shared_world` (cbrain-sim's mode); add optional `session_model: "per_connection" \| "shared_world"` field to `HelloAck` (backward-compat: absent = `per_connection`). `atd_server.AtdServer` would advertise `"shared_world"` when registered tools share mutable state (cbrain's case) — for v1 this is just documented convention, no code change. Target 1 month. |
| **P2-10** | 工具版本化 | 🟡 **deferred — wait for second adopter** | n/a yet | Open SP when a second adopter (besides cbrain's M6+) hits the same need. Single-adopter SPs ossify around one use case; waiting for two avoids that. |
| **P2-11** | Capability 命名规范 | ⏳ **queued — doc SP + community review** | `SP-capability-naming-v1` (~1 周 + 2 wk review) | next action: draft `docs/protocol/capability-naming.md` with the recommended `domain:action` matrix. Coordinate with healthkit_cli (uses `records:read`-style), celia (similar), and cbrain (proposed `perception.read`-style with `.`). Phase G already proved the wire is convention-neutral (`required_capabilities` is `list[str]` compared directly); this SP is about **community alignment**, not protocol change. Target 1.5 months. |

### 9.2 P0-1 立即动作

- **设计**：`docs/archive/superpowers/specs/2026-05-19-sp-server-py-v1-design.md`（本 commit）。
- **计划**：`docs/archive/superpowers/plans/2026-05-19-sp-server-py-v1.md`（本 commit）。
- **API 兼容承诺**：Python server 复用 `atd_client.wire` / `atd_client.protocol` 常量，与 `crates/atd-protocol` byte-for-byte 兼容。cbrain shim 按 `docs/protocol/wire-format.md` v0.1.0 实现的话，**切换零协议改动**，只是 import 换源 + 删 shim。
- **Phase B 落地节奏**（详见 plan）：B 骨架 → C 握手 → D 注册/列表/schema → E 派发+dry_run+tier deadline → F middleware → G 测试+conformance 子集 → H 文档+integrations 页。每阶段独立可 commit，**B-E 出来后 cbrain 就能开始切换**（不必等 F-H 全部 ship）。

### 9.3 cbrain 临时 shim 的策略建议

> ⚠ **post-ship update (2026-05-19)**: P0-1 shipped same session — cbrain doesn't actually need to vendor a shim. If they already wrote some W1 code against a shim, treat the items below as **switch-over checklist** (each item is also enforced by `atd_server` itself, so divergence will fail tests). If they haven't started, just consume `atd_server` directly.

cbrain §4 计划 vendor ~300 LOC shim 跑 W1。ATD 团队建议：

1. **byte-compat 严格按 `docs/protocol/wire-format.md` v0.1.0** —— shim 任何"私有便利字段"都会阻塞切换。✅ `atd_server` 使用 `atd_client.wire.{read_frame, write_frame}` 直接保证字节兼容。
2. **handler 签名**：`async def handler(args: dict, ctx: CallContext) -> ToolSuccess | ToolFailure`. `CallContext` 字段为 `request_id` / `tool_id` / `granted_capabilities` / `connection`. ⚠ **correction**: 早先版本说 `ctx` 应有 `dry_run` —— 实际上 SP-server-py-v1 §G5 + Phase E 实现让 dispatcher **在调用 handler 之前** 短路 dry_run，所以 handler 永远看不到 `ctx.dry_run=True`。`CallContext` 不带 `dry_run` 字段。adopter 若想 handler 控制 dry-run 行为，是 future SP 的事。
3. **middleware 用 stage-based wrapper**（`pre_call` / `post_call` / `on_error`），见 `docs/integrations/python-server.md` 的 cbrain-style Merkle audit 范例 + `SP-server-py-v1` §5.6.
4. **不要 vendor 任何 cancel / streaming 简化版** —— 这两个语义留给 `SP-cancel-streaming-v1` 一次性出，cbrain 在 W7 之前用 5s budget 兜过去就行。`SP-server-py-v1` Phase B 在 `_serve_one_connection` 留了 Phase-2 seam，v2 切换不破坏 handler 签名。
5. **capability 字符串约定**：tool 的 `required_capabilities: list[str]` 与 `Hello.granted_capabilities` 直接比对（不再走 `domain:action` 自动展开）。cbrain 选 `.`(point) 还是 `:`(colon) 分隔符是 adopter 自由，但 ServerPolicy 必须 grant 完全匹配的字符串。详见 `docs/integrations/python-server.md` §"Capability gate"。

### 9.4 Issue 状态

本 issue 整体 status timeline：

- 2026-05-19 ACK：`ready-for-atd` → `triaged-2026-05-19, P0-1 in flight`
- 2026-05-19 ship：**P0-1 + P2-8 (bundled) shipped same session**
- 2026-05-19 wrap-up (this commit)：full status table at §9.6 + spec corrections recorded at §9.5
- current：`triaged-2026-05-19, P0-1 + P2-8 done; 6 SPs queued; 2 deferred`
- close criteria：P0-2 + P1-3/4 + P1-6 都 ship 之后整体 close；其余子项独立追踪不再绑定本 umbrella

**cbrain 下一步**：merge worktree branch `worktree-cbrain-triage-and-sp-server-py-v1` (8 commits `c79317d` → `aeab2f5`) 到 master，然后在 cbrain 端 `path = "../atd/python"` 拉 `atd_server` + 删 shim。切换工作量 ~2h（详见 `docs/integrations/python-server.md` cbrain-style hello-world 范例）。

**ATD team contact:** atd maintainers
**Triage by:** ATD team, 2026-05-19
**P0-1 ship:** ATD team, 2026-05-19 (same session as triage; SP-server-py-v1 Phase A-H)

### 9.5 Spec corrections + drift fixes applied during P0-1 impl

These were caught while landing SP-server-py-v1. Each was fixed in the originating commit and recorded in the SP plan. Future SPs (especially Rust-side work) should propagate the relevant ones.

| # | Issue | Severity | Found in | Fixed in | Should propagate to Rust? |
|---|---|---|---|---|---|
| 1 | `_drain_and_close` log counter underflow (`done_callback.discard` runs synchronously during `asyncio.wait`, so `len(self._connection_tasks) - len(pending)` could go negative) | functional bug (log-only, no behavior break) | impl review | `afa7982` (Phase D) | Rust impl is different shape; not applicable. |
| 2 | `atd_client.types.ToolVisibility` missing `HIDDEN` variant — Rust emits `"hidden"` (per `crates/atd-protocol/src/enums.rs:86-95`) and Python Pydantic would reject | adopter-blocking (any Rust → Python flow involving hidden tools) | Phase D conformance fixture inspection | `afa7982` | No — Rust already correct; this was Python-only drift. |
| 3 | `atd_client.types.ToolDefinition` missing `required_capabilities: list[str]` (Rust has it at `crates/atd-protocol/src/tool.rs:31`) — without it the Python dispatcher had to invent a `domain:action` convention that didn't match Rust ref-server's flat opaque-string convention | adopter-blocking + conformance failure | Phase G fixture inspection | `20b798c` (Phase G) | No — Rust already correct; Python-only drift. |
| 4 | Spec §5.7 error code allocation conflicted with Rust: `1002 invalid_arguments` collides with `ERR_RATE_LIMITED`; `1003 deadline_exceeded` collides with `ERR_BROKER_FAILED` | spec doc bug (would have caused silent miscategorization in shipped Python code) | Phase E impl | `dd9116d` (Phase E) — reallocated to `1004 ERR_DEADLINE_EXCEEDED` and `1005 ERR_INVALID_ARGS` | **Yes**: SP-error-namespace-v1 should propose these constants for the Rust `messages.rs` allocation too, so cross-impl error code semantics align. |
| 5 | Spec §Phase C plan said pre-Hello frames should return `1005 not_handshaken` — but Rust `atd-ref-server` does no such enforcement (no state machine). Would have been Python-only divergence. | spec plan bug | Phase C impl | `74a42b2` (Phase C) — relaxed to "Hello optional, may arrive any time, replaces prior conn ctx" | No — matches Rust today; just need to keep both in sync if a future SP adds session enforcement. |
| 6 | Spec §4 cbrain example showed `if ctx.dry_run: ...` inside handler — but §G5 says dispatcher short-circuits dry_run before handler runs; field was dead | spec doc bug + ambiguous handler contract | Phase E impl | `dd9116d` (Phase E) — `CallContext` does NOT carry `dry_run`; spec comment added. cbrain shim guidance updated at §9.3 above. | No — Python-specific design choice. |

### 9.6 Post-ship roadmap snapshot (for handoff)

| SP | Priority | Effort | Blocked? | Driver | Whoever picks it up |
|---|---|---|---|---|---|
| `SP-release-binaries-v1` (P0-2) | medium | 2-3 days CI | no | cbrain W6 onboarding ergonomics | Anyone w/ GH Actions familiarity |
| `SP-error-namespace-v1` (P1-6) | medium | ~2 days docs | no | cross-adopter alignment | Doc writer; coordinate w/ cbrain + healthkit + celia |
| `SP-session-model-doc` (P2-9) | medium | ~3 days docs | no | cbrain-sim's shared-world model needs naming | Doc writer |
| `SP-conformance-py-v1` (P2-7) | low-medium | ~3 days (revised down) | no (P0-1 unblocked it) | Future Python adopters | Python dev; can reuse Phase G code |
| `SP-cancel-streaming-v1` (P1-3+4) | medium-high | ~6 weeks | no (Phase 2 seam ready) | cbrain W7 priority preemption | Protocol contributor + dual-impl (Rust+Python) |
| `SP-capability-naming-v1` (P2-11) | low | ~1 wk + 2 wk review | no | community alignment | Doc writer + adopter consensus |
| `SP-binary-frames-v1` (P1-5, design only) | low | ~3 days design | no | Wait for demonstrated need | Defer until cbrain real-robot stage |
| Versioning SP (P2-10) | low | n/a | yes — waiting for 2nd adopter | None active | Defer |

**Closure tracking**: this issue closes when P0-2 + P1-3/4 + P1-6 all ship. The other SPs proceed on their own timelines without blocking the umbrella close.
