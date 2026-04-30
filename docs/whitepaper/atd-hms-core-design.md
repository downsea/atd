# ATD for HMS Core — Applied Design

**Date:** 2026-04-21
**Status:** Design proposal. Non-binding; seeking HMS team feedback.
**Companion documents:**
- Blueprint: [ATD v3 whitepaper](toward-agent-tool-dispatch-v3.md) (the vocabulary this design uses)
- Cycle 2 spec: [2026-04-21-atd-for-hms-core-applied-design.md](../superpowers/specs/2026-04-21-atd-for-hms-core-applied-design.md)
- Strategic counterpart (editorial): `why-atd-for-hms-core.md` (pending, Task #26)

---

## §1. Scope 与 Flagship Matrix 总览

HMS Core 是 2026 年**结构最复杂的 agent tool 生态测试对象**：

- **20+ kit**，覆盖 health / location / messaging / ML / payment / advertising / vehicle / ...
- **4 个主要 binding 层**：REST（cloud API）、on-device SDK（Android）、on-device kit（HarmonyOS / ArkTS）、HarmonyOS NEXT 分布式（DSoftBus）
- **7 个设备类**，Huawei 在每类都有出货产品（这是业界唯一做到的厂商）：phone / watch / earbuds / tablet / pc（2026-04 新）/ car_hmi（HIMA 联盟 Aito/Luxeed/Seres）/ tv（Vision）

本文档不试图穷举全部 20+ × 7 = 140+ cells，而是**以 Flagship Matrix 证明 v3 schema 足以表达 HMS Core 的全部结构复杂度**。

### Flagship Matrix（5 × 3 = 15 cells）

| | **Phone** (Mate 80 Pura) | **Watch** (Watch 4) | **Car HMI** (Aito M9 · HarmonySpace 6) |
|---|---|---|---|
| **HealthKit** | REST + HMS Health SDK（ArkTS）| Wear Engine：原始心率/ECG/SpO2 | 从 phone DSoftBus 同步到车机仪表 |
| **Location Kit** | REST geocoding + on-device fused location（GPS+WiFi+BT）| GPS + 本地 geofence | 车机 GNSS + CAN-bus 车速融合 |
| **Push Kit** | 纯 REST（服务端 send）+ 客户端接收 | 通过 phone bridge 转发 | 驾驶中静音策略，停车再通知 |
| **ML Kit** | On-device inference + Cloud 备份 | 轻量端推理（人脸、语音唤醒）| 车内语音识别 + 视觉感知 |
| **Site Kit** | REST（POI 搜索）| 通过 phone 代理 | **旗舰场景**：导航目的地搜索 |

标记 **★** 的 5 个 cell（本表对角线 + Site×Car）在 §4 全展开；其余 10 cell 在 Appendix A skeleton。

### 贯穿场景（§5）

1. **健康异常闭环**：Watch（HealthKit） → Phone → 提醒 → 日程改动
2. **智能驾车**：Phone（Site） → Car HMI（Site + 导航） → Earbuds（语音）
3. **家庭健康周报**：Watch（HealthKit） → Cloud 聚合 → PC/TV 渲染

### 非 goal

- 不是商业案例分析（见 Editorial "Why ATD for HMS Core"）
- 不是 Huawei 内部产品需求
- 不是 HIMA governance 提案
- 不定义新的 ATD schema 字段（完全使用 v3 v3 vocabulary）

---

## §2. Kit Inventory

HMS Core 2026 年公开的 **20+ kit** 完整清单。5 个 flagship 加粗 + ★。

| Kit | 简述 | Binding 形态 | 设备类 | v3 relevance |
|-----|------|------------|-------|-------------|
| ★ **HealthKit** | 健康数据（心率/睡眠/步数/ECG/SpO2）| Cloud REST + Android SDK + HarmonyOS Kit + Wear Engine | phone / watch / band | device.preferred + requires.sensors |
| ★ **Location Kit** | 定位服务（fused location / geofence / activity recognition / geocoding）| Cloud REST + Android SDK + HarmonyOS Kit | phone / car | 典型双 binding：REST cloud vs on-device |
| ★ **Push Kit** | 消息推送（server → device）| 服务端纯 REST + 客户端 SDK 接收 | all devices | gws-style REST binding 范本 |
| ★ **ML Kit** | 端侧/云侧机器学习（人脸/文本识别/翻译）| Cloud REST + Android NNAPI + HarmonyOS ML Kit | phone / watch（轻量）| compute-affinity via device.preferred |
| ★ **Site Kit** | POI 搜索（place search / place detail）| Cloud REST | phone / car（导航）| 纯 REST，与 Push 对照 |
| Account Kit | Huawei ID 登录/授权 | Client SDK + REST（OAuth）| all | IAM 的基础，§7 映射到 ATD capability |
| Map Kit | 地图渲染 + 导航 | Client SDK + REST | phone / car | 与 Location/Site 复用 |
| Scan Kit | 条码/二维码扫描 | on-device only（需相机）| phone / watch | camera sensor 前置 |
| Wallet Kit | 钱包 / 卡券 / NFC 支付 | Client SDK + REST | phone / watch | safety: dangerous |
| Safety Detect | 设备/应用安全检查 | Mixed（端 + 云 attestation）| phone | 主要用于 agent 自身信任验证 |
| Awareness Kit | 上下文感知（天气/时间/位置/耳机连接等状态集合）| on-device only | phone | 组合多个 sensor/service |
| Ads Kit | 广告 SDK | on-device only | phone / tv | safety: read（广告显示）/ write（互动）|
| IAP | 应用内购买 | Client SDK + REST | phone | safety: dangerous（支付）|
| Camera Kit | 相机控制 API | on-device only | phone | streaming 数据不经 ATD |
| Analytics Kit | 事件上报 + 分析 | Client SDK + REST | all | safety: write |
| Network Kit | 网络能力（低延迟 / 多链路聚合）| on-device | phone | 通常作为其他 binding 的 transport |
| Search Kit | 应用内搜索 | SDK + REST | phone | |
| Game Service | 游戏中心 / 成就 / 排行榜 | Client SDK + REST | phone | |
| Drive Kit | 华为云盘 | REST | phone / pc | 纯 cloud REST 案例 |
| Video Kit / Audio Kit | 多媒体 SDK | on-device | phone / tv | streaming 不经 ATD |
| FIDO | 生物识别 / passkey | on-device only | phone | safety: dangerous |
| Dynamic Ability Engine | HarmonyOS 原子服务动态装配 | HarmonyOS only | phone / tablet | 跨 kit orchestration |

### Kit 按 binding 形态分三组

**Pure Cloud REST**（纯云，跨平台）：Push Kit (send), Site Kit, Drive Kit, 部分 Account Kit endpoint

**Hybrid（REST + 端侧双实装）**：HealthKit, Location Kit, Map Kit, ML Kit, Analytics Kit

**On-device Only**（端侧专有）：Scan Kit, Camera Kit, Awareness Kit, Wallet Kit, Ads Kit, IAP, Safety Detect (attestation 部分), Dynamic Ability Engine, Network Kit, FIDO

**v3 相关观察**：三组比例大约 3:5:10。说明 **HMS 生态超过半数能力是端侧独占**——gws 风格的"单个 CLI 包 REST"方案，对 HMS Core 只能覆盖 ~30% 场景。必须用 ATD 的多 binding 才能完整覆盖。

---

## §3. Device Inventory

### 3.1 HMS 设备矩阵（2026-04 现状）

| Device class | HMS 代表产品 | OS | 典型传感器 / 能力 |
|-------------|------------|-----|----------------|
| **phone** | Mate 80 Pura / Pura 80 | HarmonyOS 5/6 | camera, microphone, gps, accel, bio-id |
| **watch** | Watch 4 / GT 系列 | HarmonyOS Lite | heart_rate, ecg, spo2, skin_temp, accel, gps (部分) |
| **earbuds** | FreeBuds Pro 5 | HarmonyOS Audio (DNN) | microphone, head-tracking, NearLink |
| **tablet** | MatePad Pro | HarmonyOS 5/6 | 大屏 + stylus + 与 phone 同 SDK |
| **pc** | MateBook（HarmonyOS PC 2026-04 新）| HarmonyOS PC | 键盘鼠标 + 完整算力 + shell 接入 |
| **car_hmi** | Aito M7/M9 / Luxeed / Seres（HIMA 联盟）| HarmonySpace 6 + ADS 5.0 | can_bus, gps, 多摄像头, 激光雷达（部分）|
| **tv** | Vision | HarmonyOS | 10-ft UI, 家庭共享 |
| **smart_home_hub** | HiLink 中枢 | HiLink OS | 家居控制协议 |

### 3.2 Kit × Device 能力矩阵（重要）

`✓✓` = flagship cell（深展开）
`✓` = 可用（Appendix A skeleton）
`—` = 不适用

| Kit \ Device | phone | watch | earbuds | tablet | pc | car_hmi | tv |
|-----|---|---|---|---|---|---|---|
| HealthKit       | ✓✓ | ✓✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Location Kit    | ✓✓ | ✓ | — | ✓ | ✓ | ✓ | — |
| Push Kit        | ✓✓ | ✓ | — | ✓ | ✓ | ✓ | ✓ |
| ML Kit          | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Site Kit        | ✓ | — | — | ✓ | ✓ | ✓✓ | — |
| Account Kit     | ✓ | ✓ | — | ✓ | ✓ | ✓ | ✓ |
| Scan Kit        | ✓ | ✓ | — | ✓ | — | ✓ | — |
| Wallet Kit      | ✓ | ✓ | — | — | — | ✓ | — |
| Camera Kit      | ✓ | — | — | ✓ | — | ✓ | — |
| Awareness Kit   | ✓ | ✓ | ✓ | ✓ | — | ✓ | — |
| Ads Kit         | ✓ | — | — | ✓ | — | — | ✓ |
| Safety Detect   | ✓ | ✓ | — | ✓ | ✓ | ✓ | — |

### 3.3 关键洞察

1. **HealthKit 和 Push Kit 几乎全设备可用**——但每个设备 binding 形态不同
2. **Site Kit / Map Kit 在 car HMI 是刚需**——导航场景驱动
3. **Scan / Camera / Wallet 依赖特定 sensor**——device.requires.sensors 过滤关键
4. **Earbuds / TV 覆盖度低**——这两类场景偏窄（audio control / media display）

---

## §4. Flagship Cells — 完整 Tool Definition 展开

### §4.1 Cell: HealthKit × Watch

#### 4.1.1 Capability summary

让 agent 从用户的 Huawei Watch 本地读取**实时生理数据**（心率 / ECG / SpO2），数据延迟 <30ms，不经手机/云端。

典型 intent：*"我现在心率多少？"* / *"past hour heart rate trend"*

#### 4.1.2 Complete tool definition

```yaml
atd_version: "1.0"
id: hms:health.heart_rate.get
version: "1.0.0"
name: "获取当前心率"
description: "从佩戴的 HMS HealthKit 兼容手表读取当前心率（BPM）及时间戳"

capability:
  domain: health.vitals
  actions: [get_current]
  intent_examples:
    - "我现在心率多少"
    - "heart rate now"
    - "what's my current BPM"

input:
  type: object
  properties: {}   # 无参数，默认读当前
  required: []

output:
  type: object
  properties:
    bpm: {type: integer, minimum: 30, maximum: 240}
    confidence: {type: number, minimum: 0, maximum: 1}
    timestamp: {type: string, format: date-time}
    source_device_id: {type: string}
  required: [bpm, timestamp]

# v3 device affinity
device:
  preferred: [watch]
  fallback: [phone]              # phone 可读从 watch 同步的最近值
  requires:
    sensors: [heart_rate]

  vendor_hints:
    - vendor: huawei
      prefer_kit: wear_engine
    - vendor: apple
      prefer_framework: HealthKit
    - vendor: google
      prefer_framework: wear_os_health_services

bindings:
  appfunction:
    # Watch 本地：Huawei Wear Engine
    - device_type: watch
      vendor: huawei
      platform: harmonyos_lite
      kit: wear_engine
      ability: com.huawei.health.HealthDataProvider
      action: getCurrentHeartRate
      permissions: [HEALTHKIT_HEARTRATE_READ]
    
    # Watch 本地：Apple WatchKit + HealthKit
    - device_type: watch
      vendor: apple
      platform: watchos
      framework: HealthKit
      entity: HKQuantityTypeIdentifier.heartRate
      query: HKSampleQuery
      permissions: [NSHealthShareUsageDescription]
    
    # Phone fallback：HMS Health SDK (HarmonyOS/Android) 读已同步心率
    - device_type: phone
      vendor: huawei
      platform: harmonyos
      kit: hms_health
      ability: com.huawei.health.HealthKitApi
      action: getLatestHeartRate
      coverage: "从 watch 同步的最近值；延迟取决于同步间隔（默认 15min）"
      permissions: [HEALTH_READ_HEART_RATE]

  rest:
    method: GET
    url_template: "https://health-api.cloud.huawei.com/v2/heartrate/latest"
    auth: {type: oauth2, scope: "huawei.health.heart_rate.read"}
    coverage: sync_only           # 云端只能读已同步数据，非实时

safety:
  level: read
  data_sensitivity: health_private
  side_effects: []

result_middleware:
  - type: pii_redact
    fields: [source_device_id]    # device_id 可能泄漏 MAC，默认 redact
    mode: transform

ergonomic_aliases:
  - alias_id: hms.heart_rate
    description: "Shortcut for hms:health.heart_rate.get"
    input:
      type: object
      properties: {}
    maps_to:
      tool_id: hms:health.heart_rate.get
      transform: {}
    visibility: preferred

compatibility:
  requires_capabilities: [health_read_permission, hms_health_app_installed]
```

#### 4.1.3 Binding implementation (Huawei Wear Engine)

Watch 端（ArkTS，HarmonyOS Lite）：

```typescript
// src/main/ets/abilities/HealthDataProvider.ets
import { Ability } from '@ohos.app.ability';
import wearEngine from '@hms.health.wear-engine';

@AppFunction   // HarmonyOS 5+ 装饰器，暴露为 ATD AppFunction
export default class HealthDataProvider extends Ability {
  async getCurrentHeartRate(): Promise<HeartRateResult> {
    // WearEngine 1.0+ API
    const reading = await wearEngine.healthData.queryLatest({
      dataType: wearEngine.DataType.HEART_RATE,
      maxAge: 10000,                  // 10s 内数据，否则触发新测量
    });
    
    return {
      bpm: reading.value,
      confidence: reading.accuracy === 'HIGH' ? 0.95 : 0.7,
      timestamp: new Date(reading.timestamp).toISOString(),
      source_device_id: await this.getDeviceId(),
    };
  }
}
```

Apple Watch 等价实现（Swift + HealthKit）：

```swift
import HealthKit

struct HeartRateIntent: AppIntent {
    static var title: LocalizedStringResource = "Get Current Heart Rate"
    
    func perform() async throws -> some IntentResult & ReturnsValue<HeartRateResult> {
        let store = HKHealthStore()
        let type = HKQuantityType(.heartRate)
        try await store.requestAuthorization(toShare: [], read: [type])
        
        let query = HKSampleQuery(
            sampleType: type,
            predicate: nil,
            limit: 1,
            sortDescriptors: [NSSortDescriptor(key: HKSampleSortIdentifierEndDate, ascending: false)]
        )
        let samples = try await store.samples(for: query)
        let bpm = samples.first!.quantity.doubleValue(for: HKUnit(from: "count/min"))
        
        return .result(value: HeartRateResult(bpm: Int(bpm), ...))
    }
}
```

#### 4.1.4 Pitfalls & gotchas

- **Watch 未佩戴**：Wear Engine 返回空结果。Tool 应该在 output 里有 `confidence < 0.5` 表达，不要返回假数据
- **电池敏感**：心率传感器持续开启耗电快。不要设计 "每秒拉取" tool——应设计 event-driven push tool 替代
- **手表离线**：BT 断开时 phone 读不到 watch；dispatch 会 fallback 到 rest binding，但延迟/准确性下降
- **HarmonyOS 5 NEXT 和 Android 不兼容**：Huawei watch 4 上 Wear Engine 只在 ArkTS 可用，Android SDK 调用链不兼容（这是 2026 HarmonyOS NEXT 的主要变化）
- **权限对话框**：首次调用时 OS 弹窗要用户确认；ATD `capability_token` 授权 + OS permission 是**叠加**关系，缺一不可
- **Apple 和 Huawei 数据互不相通**：同一个 tool 定义跨 vendor 可执行，但**数据不跨云同步**（除非用户同时授权两边）

#### 4.1.5 Cross-references

- §5.1 Health-anomaly closed loop 使用此 tool 作为触发点
- v3 §2.5 Multi-Device Dispatch — device.preferred 与 requires.sensors 路由
- v3 §2.7 Result Middleware — pii_redact 默认启用
- v3 Appendix H Device Type Registry — `heart_rate` capability tag 定义

---

### §4.2 Cell: HealthKit × Phone

#### 4.2.1 Capability summary

让 agent 从 HMS Health 手机应用读取**聚合健康数据**（每日/每周/每月报告），这些数据由手表同步汇总，延迟几秒到几分钟。

典型 intent：*"昨晚睡眠怎么样"* / *"this week's exercise summary"*

#### 4.2.2 Tool definition（核心字段，相对 §4.1 精简）

```yaml
id: hms:health.sleep.get
capability:
  domain: health.sleep
  actions: [get]
  intent_examples: ["昨晚睡眠怎么样", "how did I sleep", "my sleep last night"]

input:
  type: object
  properties:
    date: {type: string, format: date}
  required: [date]

output:
  type: object
  properties:
    total_minutes: {type: integer}
    deep_sleep_minutes: {type: integer}
    rem_minutes: {type: integer}
    awake_minutes: {type: integer}
    efficiency: {type: number, minimum: 0, maximum: 1}
    source_device_id: {type: string}

device:
  preferred: [phone]              # 聚合视图在 phone 最合适
  fallback: []
  requires: {}

bindings:
  appfunction:
    - device_type: phone
      vendor: huawei
      platform: harmonyos
      kit: hms_health
      ability: com.huawei.health.SleepDataAbility
      action: getSleepData
      permissions: [HEALTH_READ_SLEEP]
    - device_type: phone
      vendor: huawei
      platform: android
      sdk: com.huawei.hms.health-sleep
      method: HealthClient.getSleepData
      permissions: [com.huawei.health.READ_SLEEP_DATA]
    - device_type: phone
      vendor: apple
      platform: ios
      framework: HealthKit
      entity: HKCategoryValueSleepAnalysis

  rest:
    method: GET
    url_template: "https://health-api.cloud.huawei.com/v2/sleep?date={date}"
    auth: {type: oauth2, scope: "huawei.health.sleep.read"}

safety:
  level: read
  data_sensitivity: health_private

result_middleware:
  - type: pii_redact
    fields: [source_device_id]
    mode: transform
```

#### 4.2.3 Binding details

HMS Health Android SDK 调用：

```kotlin
val healthClient = HuaweiHiHealth.getDataController(context)
val request = ReadOptions.Builder()
    .read(DataType.DT_CONTINUOUS_SLEEP)
    .setTimeRange(startOfDay.time, endOfDay.time, TimeUnit.MILLISECONDS)
    .build()

val sleepData = healthClient.read(request).await()
return SleepResult(
    totalMinutes = sleepData.totalDuration.toMinutes(),
    deepSleepMinutes = sleepData.deepSleepDuration.toMinutes(),
    // ...
)
```

HarmonyOS NEXT（ArkTS）：

```typescript
import { health } from '@hms.health.core';

const sleepData = await health.querySleepData({
  startTime: dayStart,
  endTime: dayEnd,
});
return {
  total_minutes: sleepData.totalMinutes,
  deep_sleep_minutes: sleepData.deepMinutes,
  rem_minutes: sleepData.remMinutes,
  efficiency: sleepData.efficiency,
};
```

#### 4.2.4 Pitfalls

- **数据同步延迟**：手表到手机同步间隔 15 分钟（默认），早晨刚醒 tool 可能拿不到昨晚完整数据
- **多手表场景**：用户有多只手表时，phone 聚合所有来源——`source_device_id` 可能混杂。Tool 应在 output 明示
- **HarmonyOS 5 vs 5 NEXT（HarmonyOS 5.0+ 纯鸿蒙）**：API 命名空间不同，binding 需区分 platform 字段
- **iOS binding**：Apple HealthKit 的 sleep 数据模型与 Huawei 不一致（Apple 分 deep/core/rem/awake 四档，Huawei 加 "light sleep"）。Tool output 要 normalize 到公共子集

#### 4.2.5 Cross-references

- §5.1 与 §4.1 联动：watch 推送心率异常 → phone 用本 cell 查睡眠质量
- §5.3 家庭健康周报直接调用本 tool

---

### §4.3 Cell: Location Kit × Phone

#### 4.3.1 Capability summary

获取用户当前位置，自动选择**on-device fused location**（高精度，室内也可用）或 **cloud geocoding**（IP 基础，精度低但 cross-platform）。

典型 intent：*"我在哪儿"* / *"nearest coffee shop from here"*

#### 4.3.2 Tool definition

```yaml
id: hms:location.current.get
capability:
  domain: location.current
  actions: [get]
  intent_examples: ["我在哪儿", "where am I", "current location"]

input:
  type: object
  properties:
    accuracy: {type: string, enum: [high, medium, low]}
  required: []

output:
  type: object
  properties:
    latitude: {type: number, minimum: -90, maximum: 90}
    longitude: {type: number, minimum: -180, maximum: 180}
    accuracy_meters: {type: number}
    altitude_meters: {type: number}
    source: {type: string, enum: [gps, wifi, cell, ip, fused]}
    timestamp: {type: string, format: date-time}

device:
  preferred: [phone, car_hmi]
  fallback: [tablet, pc]
  requires:
    sensors: [gps]                # 至少要有某种定位能力
  vendor_hints:
    - vendor: huawei
      prefer_kit: location_kit
    - vendor: apple
      prefer_framework: CoreLocation

bindings:
  appfunction:
    - device_type: phone
      vendor: huawei
      platform: harmonyos
      kit: location_kit
      ability: com.huawei.hms.location.FusedLocationAbility
      action: getCurrentLocation
      permissions: [ohos.permission.LOCATION, ohos.permission.APPROXIMATELY_LOCATION]
    - device_type: phone
      vendor: apple
      platform: ios
      framework: CoreLocation
      entity: CLLocation
      permissions: [NSLocationWhenInUseUsageDescription]
    - device_type: phone
      vendor: google
      platform: android
      sdk: com.google.android.gms.location.FusedLocationProviderClient
      method: getLastLocation
  
  rest:
    method: POST
    url_template: "https://location-api.cloud.huawei.com/v2/locate"
    auth: {type: oauth2, scope: "huawei.location.read"}
    coverage: "IP-based，精度通常 1-5 km，室内失效"

safety:
  level: read
  data_sensitivity: location_private

result_middleware:
  - type: pii_redact
    fields: []               # 不 redact 坐标（tool 本身就是查位置）
  - type: prompt_injection_scan
    mode: warn               # 反射式结果理论上不含注入，但保险

ergonomic_aliases:
  - alias_id: hms.where
    description: "Shortcut"
    input: {type: object, properties: {}}
    maps_to:
      tool_id: hms:location.current.get
      transform:
        accuracy: {literal: "high"}
    visibility: preferred
```

#### 4.3.3 Binding details

HarmonyOS NEXT Location Kit：

```typescript
import { location } from '@hms.location.core';

const current = await location.getCurrentLocation({
  priority: location.Priority.HIGH_ACCURACY,
  timeoutMs: 3000,
});
return {
  latitude: current.latitude,
  longitude: current.longitude,
  accuracy_meters: current.accuracy,
  source: current.source,       // 'gps' | 'wifi' | 'fused' ...
  timestamp: new Date().toISOString(),
};
```

#### 4.3.4 Pitfalls

- **室内 GPS 不准**：iOS/Android/HarmonyOS 都有 WiFi/BT 定位补偿，但精度从 5m 降到 50-100m。Tool output 的 `accuracy_meters` 必须如实填
- **权限门**：iOS 的 "Always / When In Use" vs "Once" 区分；Android 的 FINE/COARSE；HarmonyOS 类似。用户授权粒度影响 tool 能返回的精度
- **隐私 Mode**：用户可能启用 "精确位置 Off"；tool 必须优雅降级（source 返回 "ip"）
- **飞行模式 / 无服务**：REST binding 不可用时，fallback 到 on-device

#### 4.3.5 Cross-references

- §5.2 智能驾车：此 tool 为起点（agent 查用户所在地 → Site Kit 查附近 POI → 导航）
- v3 §2.5 fallback 链的典型案例

---

### §4.4 Cell: Push Kit × Phone（cloud-only）

#### 4.4.1 Capability summary

让 agent **发送 push 通知**给用户（或其它用户）的 HMS Push-enabled 设备。纯 cloud REST（由 HMS server 处理 token 路由），客户端只接收。这是 HMS 生态最简单的 REST-only binding 案例。

典型 intent：*"提醒张三下午 3 点开会"* / *"send push reminder"*

#### 4.4.2 Tool definition（strikingly simple — nothing on-device）

```yaml
id: hms:push.send
capability:
  domain: messaging.push
  actions: [send]
  intent_examples: ["推送通知", "send push notification"]

input:
  type: object
  properties:
    device_token: {type: string, description: "Target device HMS Push token"}
    title: {type: string, maxLength: 50}
    body: {type: string, maxLength: 500}
    data: {type: object, description: "Optional custom payload"}
  required: [device_token, title, body]

output:
  type: object
  properties:
    message_id: {type: string}
    sent_at: {type: string, format: date-time}

device:
  preferred: [phone, pc]         # 需要能发 HTTP 请求的设备
  fallback: [tablet]
  requires:
    transport: [wifi, lte, 5g]   # 需要网络
    # 无 sensor 需求

bindings:
  rest:
    method: POST
    url_template: "https://push-api.cloud.huawei.com/v2/{app_id}/messages:send"
    auth: {type: oauth2, scope: "huawei.push.send"}
    headers:
      Content-Type: application/json
    body_template: |
      {
        "message": {
          "notification": {"title": "{{title}}", "body": "{{body}}"},
          "data": {{data | json}},
          "token": ["{{device_token}}"]
        }
      }

safety:
  level: write
  side_effects: [user_notification]
  data_sensitivity: messaging_metadata

result_middleware:
  - type: trim
    limit: 1024
    strategy: chars
    # push 结果小，一般不需要但保险

ergonomic_aliases:
  - alias_id: hms.push.simple
    description: "Simplified push to token holder"
    input:
      type: object
      properties:
        to: {type: string, description: "Target HMS Push token"}
        message: {type: string}
      required: [to, message]
    maps_to:
      tool_id: hms:push.send
      transform:
        device_token: {from: $to}
        title: {literal: "提醒"}
        body: {from: $message}
    visibility: preferred
```

#### 4.4.3 Binding details（服务端 REST）

任何设备（phone/pc/cloud function）都可以发起：

```python
import requests

resp = requests.post(
    f"https://push-api.cloud.huawei.com/v2/{app_id}/messages:send",
    headers={"Authorization": f"Bearer {oauth_token}"},
    json={
        "message": {
            "notification": {"title": "提醒", "body": "会议 15:00"},
            "token": [target_device_token],
        }
    },
)
```

#### 4.4.4 Pitfalls

- **device_token 从哪来**：Push Kit 的 token 是客户端 SDK 首次启动注册得到，agent 调 `hms:push.send` 前需拿到 token。生态里常见做法是用户授权后 agent 缓存 token
- **Rate limit**：HMS Push 单应用每秒有限额，tool 层必须走 v3 `resources.rate_limit`
- **跨地区**：国内 HMS Push 和 海外 HMS Push 是两套 endpoint，tool 可能需要两个 binding 按用户注册地路由
- **与其他 OS 的兼容**：Push Kit 只推送给 HMS Push 客户端（HarmonyOS / Android with HMS）。推送给 iOS 要改用 APNs——跨 platform scheduling 的 tool 应该封装为 `vendor:multichannel:push` 调 HMS + APNs + FCM 三个 binding

#### 4.4.5 Cross-references

- 是 gws-style "pure REST tool" 在 HMS 场景的范本
- §5.1 场景用此 tool 做跨设备通知

---

### §4.5 Cell: Site Kit × Car HMI（旗舰场景）

#### 4.5.1 Capability summary

让 agent 查询用户目的地附近的 POI（餐厅 / 加油站 / 医院 / ...），用于车机导航选目的地。**这是 v3 driving_constraint 和 HarmonySpace 6 集成的最强展示**。

典型 intent：*"找最近的加油站"* / *"nearby hospital"*

#### 4.5.2 Tool definition

```yaml
id: hms:site.nearby.search
capability:
  domain: location.poi
  actions: [search_nearby]
  intent_examples: ["附近的加油站", "nearby hospital", "find nearest pharmacy"]

input:
  type: object
  properties:
    query: {type: string, minLength: 1, maxLength: 100}
    latitude: {type: number}
    longitude: {type: number}
    radius_meters: {type: integer, minimum: 100, maximum: 50000}
  required: [query, latitude, longitude]

output:
  type: object
  properties:
    results:
      type: array
      maxItems: 10
      items:
        type: object
        properties:
          name: {type: string}
          address: {type: string}
          latitude: {type: number}
          longitude: {type: number}
          distance_meters: {type: number}
          phone: {type: string}
          category: {type: string}

device:
  preferred: [car_hmi, phone]      # 车机是首选（典型驾驶场景）
  fallback: [tablet]
  requires:
    transport: [lte, 5g, wifi]

bindings:
  rest:
    method: POST
    url_template: "https://siteapi.cloud.huawei.com/mapApi/v2/siteService/nearbySearch"
    auth: {type: oauth2, scope: "huawei.site.read"}
    body_template: |
      {
        "location": {"lat": {{latitude}}, "lng": {{longitude}}},
        "query": "{{query}}",
        "radius": {{radius_meters}},
        "language": "zh"
      }
  
  appfunction:
    - device_type: car_hmi
      vendor: huawei
      platform: harmonyspace_6
      kit: site_kit
      ability: com.huawei.automotive.NearbySiteProvider
      action: searchNearby
      # 车机 binding 本地缓存 + REST 合并，延迟更低

safety:
  level: read
  data_sensitivity: none
  driving_constraint: safe_always  # 搜索本身安全；但返回结果 UI 渲染要符合驾驶安全

result_middleware:
  - type: trim
    strategy: top_items_by_path
    path: $.results
    limit: 5                     # 车机驾驶中只展示 5 条
  - type: format_transform
    target: markdown             # 车机 voice TTS 友好

output_hint:
  prefer_display: [screen_medium, voice_summary]   # 车机屏或 TTS
  fallback_display: [voice_summary]

ergonomic_aliases:
  - alias_id: car.find_nearby
    description: "Nearby POI (car-optimized)"
    input:
      type: object
      properties:
        query: {type: string}
      required: [query]
    maps_to:
      tool_id: hms:site.nearby.search
      transform:
        query: {from: $query}
        latitude: {fn: template, args: {template_str: "{{car.gps.lat}}"}}
        longitude: {fn: template, args: {template_str: "{{car.gps.lng}}"}}
        radius_meters: {literal: 5000}
    visibility: preferred
```

#### 4.5.3 Binding details

HarmonySpace 6 车机（ArkTS）：

```typescript
import { site } from '@hms.automotive.site';
import { cockpit } from '@hms.automotive.cockpit';

export class NearbySiteProvider {
  async searchNearby(query: string) {
    // 先拿车机当前位置
    const loc = await cockpit.getGps();
    
    // Site Kit 查询（本地 cache 命中 fast path）
    const results = await site.nearbySearch({
      location: loc,
      keyword: query,
      radius: 5000,
      pageSize: 5,          // 驾驶中最多 5 条
    });
    
    // 结果回传给 agent，agent 再决定是否调 navigation.route_to
    return { results };
  }
}
```

#### 4.5.4 Pitfalls

- **驾驶时 UI 限制**：即使 `driving_constraint: safe_always`，**渲染 10+ POI 列表不安全**——`result_middleware.trim top 5` 强制限制
- **本地缓存 vs cloud fallback**：车机本地 Site Kit 有离线 POI 数据库，REST 是补充（精度高、数据新）。Dispatch 应优先本地以保证无网络区（隧道、偏远路段）可用
- **跨 OEM**：HIMA 联盟车（Aito / Luxeed / Seres）都用 HarmonySpace 6 + Site Kit，但非 HIMA OEM（吉利、比亚迪等）用自己的系统，Site Kit binding 不适用。这些 OEM 要么接 ATD 生态（提供自己的 binding），要么在车机上只能走 REST fallback
- **ADS 5.0 互动**：如果车在 ADS 自动驾驶模式下，"附近的加油站"可能触发自动变道 + 路径重规划。这超出本 tool 范围，属于 `car.navigation.route_to` 工具的职责

#### 4.5.5 Cross-references

- §5.2 智能驾车场景：整个流程的中枢 tool
- v3 §8.6 Car HMI developer guide
- v3 Appendix H `driving_constraint` + `output_hint`

---

## §5. Integration Scenarios — 跨 Cell 协作

### §5.1 Scenario A：健康异常闭环

**Trigger**：Watch 检测到心率异常（睡眠中 90+ bpm 或清醒静息 120+ bpm）。

**Steps**：

1. Watch 本地运行 `hms:health.anomaly.detect`（周期性）→ 触发 event
2. Watch push event → Phone（通过 DSoftBus，§v3 Appendix I）
3. Phone agent 接手 session（`session.handoff(trigger=auto_on_event)`）
4. Agent 调 `hms:health.heart_rate.get`（§4.1）拿当前数据
5. Agent 调 `hms:health.sleep.get`（§4.2）拿昨晚睡眠 context
6. Agent 调 `calendar.get`（非 HMS，但 tool 同格式）查今天日程
7. Agent 决策：建议提前看医生 → 调 `calendar.reschedule` 改今天下午
8. Agent 调 `hms:push.send`（§4.4）通知家人 + `audio.tts.speak`（earbuds）告知 Lily

**v3 原语使用**：

| 步骤 | v3 原语 |
|------|---------|
| 1 | `device.preferred: [watch]` + `device.requires.sensors: [heart_rate]` |
| 2 | HarmonyOS Super Device via DSoftBus（Appendix I）|
| 3 | `session.handoff(trigger=auto_on_event)` |
| 4-6 | 普通 tool call，无 v3 特殊字段 |
| 7 | dispatch 的 capability check + audit log |
| 8 | `result_middleware` 在返回 push 结果前 trim |

**关键验证**：整个闭环**未使用任何 v2 中没有的协议扩展**外。v3 的三个新原语（device affinity / session handoff / result middleware）共同组成闭环。v2 做不到。

---

### §5.2 Scenario B：智能驾车（Phone → Car → Earbuds）

**Trigger**：Lily 对 phone 说 "去最近的医院"。

**Steps**：

1. Phone agent 调 `hms:location.current.get`（§4.3）拿 GPS
2. Phone agent 调 `hms:site.nearby.search`（§4.5）找附近医院，radius=10km
3. Agent 决策目的地 → 调 `car.navigation.route_to`（v3 §8.6 definition）设置导航
4. Lily 进车 → `session.handoff(trigger=proximity)` 把 session 迁移到 Car HMI
5. Car HMI 展示路线（driving_constraint: safe_always OK）
6. ADS 5.0 接管驾驶（Lily 手放开方向盘）
7. 导航过程中 earbuds 语音通知转弯 → `audio.tts.speak` via earbuds control plane
8. 到达目的地 → `session.fork(target_device=phone)` 分叉回 phone（Lily 下车后 phone 继续用）

**v3 原语使用**：

| 步骤 | v3 原语 |
|------|---------|
| 1-3 | `device.preferred` 链 phone→car |
| 4 | `session.handoff(trigger=proximity)` via DSoftBus |
| 5 | `safety.driving_constraint: safe_always` allowed |
| 6 | ADS state 不通过 ATD，但车机 state machine 影响 dispatch 决策 |
| 7 | earbuds control plane（§v3 §8.3），audio stream out-of-band |
| 8 | `session.fork()` |

---

### §5.3 Scenario C：家庭健康周报

**Trigger**：周日晚 8 点 cron 触发 agent。

**Steps**：

1. Agent 对家庭成员每人调 `hms:health.sleep.get`（§4.2）7 次（一周每天）
2. 对每人调 `hms:health.exercise.get` 拿运动数据
3. 云端聚合 → 生成 markdown report
4. Agent 调 `hms:push.send`（§4.4）发送摘要给每个家庭成员 phone
5. Agent 把详细 PDF 存到 Drive（用 Drive Kit REST）
6. TV 或 PC 上点开链接 → `output_hint.prefer_display: [screen_large]` 触发大屏渲染

**v3 原语使用**：

| 步骤 | v3 原语 |
|------|---------|
| 1-2 | 批量 tool call（§5.2 的重复 pattern）|
| 3 | agent 自己聚合，无 ATD 参与 |
| 4 | `hms:push.send` 多 device_token 批量发（或 rate-limited 循环）|
| 5 | Drive Kit REST binding |
| 6 | `output_hint.prefer_display` 路由到 TV/PC |

---

## §6. 分阶段实装路线

与 HMS BU 合作建议分三阶段，每阶段独立可验证：

### Phase 0（Q2 2026）— Cloud REST 覆盖

**目标**：让 non-Huawei agent（如 OpenAI / Anthropic / LangChain）能直接调 HMS 云 API。

**交付**：
- HMS REST binding 参考实装：Push / Site / Drive / Health Cloud / ML Cloud（5 kit）
- OAuth 2.0 token 自动 refresh（§7 IAM）
- Publishing：`atd-hms-rest` crate / pypi package
- 覆盖率：~30% HMS Core（所有 pure cloud kit）

**关键指标**：
- 3 家独立 agent 框架（LangChain / Hermes / OpenClaw）通过 ATD 调 HMS Push 成功
- 延迟：Beijing data center 调用 <80ms p99

### Phase 1（Q3 2026）— Phone On-device Bindings

**目标**：在 Huawei phone 上，agent 调用 HMS Health / Location / ML 等 on-device kit。

**交付**：
- HarmonyOS 5 / NEXT AppFunction 绑定 SDK（ArkTS）
- Android (HMS Mobile Services) AppFunction 绑定 SDK（Kotlin）
- Phone 优先级 dispatch（on-device 优先，REST fallback）
- 覆盖率：~60% HMS Core（phone 端全 kit 可用）

**关键指标**：
- 一个 real-world agent（比如 HMS 官方 Dify Huawei）在 Mate 80 上完整跑 Lily §5.1 场景
- 首次使用权限对话框 + UCAN delegation 实装

### Phase 2（Q4 2026）— Watch / Car / Earbuds + Distributed

**目标**：完整 Lily §5.1-§5.3 场景跨 5 设备类闭环。

**交付**：
- Wear Engine binding（Watch 4 + GT 系列）
- HarmonySpace 6 binding（Aito / Luxeed / Seres）
- FreeBuds Pro 5 audio control plane binding
- DSoftBus distributed session transport 参考实装（§v3 §2.6 + Appendix I）
- 覆盖率：~95% HMS Core + 7 设备类 ×§4 flagship 完整

**关键指标**：
- §5 三场景端到端跑通（Lily demo video）
- APWG 接纳 HMS device binding 作为参考实装

### Phase 3（2027+）— TV / PC / 长尾 kit

剩余 5% 覆盖 + HIMA 扩展 OEM 对接。此时 ATD 已进入 standards body（Appendix D），HMS 作为 reference 提供者。

---

## §7. IAM Mapping：Huawei ID ↔ HMS OAuth ↔ ATD UCAN

### 7.1 现有链路

```
Lily 登录 Huawei ID
    ↓
AppGallery / 华为开发者平台 颁发 OAuth 2.0 access token
    (scope: huawei.health.read / huawei.push.send / ...)
    ↓
HMS Kit 服务端验证 token + 执行 API
```

这是 HMS 现有的 IAM。

### 7.2 ATD 层的补充

ATD 在**之上**加一层 UCAN capability token，不替代 HMS OAuth：

```
Lily 的 agent 要调用 hms:health.heart_rate.get
    ↓
ATD Dispatch Step 1: Verify UCAN capability token
    (token 声明 agent 被授权调此 tool pattern)
    ↓
ATD Dispatch Step 5-6: Route to binding
    ├─ appfunction binding: 使用设备的 HMS SDK，内部 HMS OAuth
    └─ rest binding: attach HMS OAuth access token in HTTP header
    ↓
HMS kit 执行（OAuth 授权逻辑不变）
```

**双层验证**的意义：

1. **OAuth** 是 *Huawei 授权 agent 的应用访问 HMS 数据*
2. **UCAN** 是 *Lily 授权 agent 实例调用此 tool pattern*

OAuth 保证 agent 的 app 有权限；UCAN 保证 agent 的**这次调用**符合 Lily 的意愿。两者叠加使 cross-vendor agent（不属 Huawei 开发）也能在 Lily 授权下调 HMS tool。

### 7.3 跨设备 UCAN delegation

v3 §2.6 session.migrate 要求目标设备**重新获得** capability token。HMS 场景：

```
Phone 的 agent 持有 UCAN token A:
  issuer: Lily (did:huawei:lily)
  audience: agent_on_phone (did:device:mate80:abc)
  capability: "hms:health.*"
  ttl: 3600

session.migrate(target=car_hmi):
  Phone 调 UCAN delegation:
    issuer: agent_on_phone (did:device:mate80:abc)
    audience: agent_on_car (did:device:m9:xyz)
    capability: "hms:health.read" (attenuated!)   ← 降级：只读，不可写
    ttl: 1800                                     ← 降级：更短 TTL
    proof: [token A]                              ← 持 A 的证明
```

Car HMI 的 agent 收到 new token B → 向 HMS Kit 请求 new OAuth token（或重用 phone 的 session OAuth 如果 HMS 支持 device-to-device delegation，这是 HMS 待定功能）。

### 7.4 待和 Huawei IAM 团队讨论的点

1. **OAuth token 是否支持跨设备复用**：当前 HMS OAuth 不做 device binding，理论上一个 token 可以跨设备用，但安全审计可能不通过
2. **UCAN DID method**：Lily 的 DID（`did:huawei:<huawei_id>`）是否写入 DID registry，或用已有 Huawei ID 作为 DID string
3. **Revocation**：Huawei ID 失效时如何 cascade revoke UCAN（bloom filter 同步还是 online check）
4. **Compliance**：中国个人信息保护法（PIPL）对 agent 跨设备数据流动的要求

---

## Appendix A. Full 5×3 Matrix — Non-Flagship Cells Skeleton

10 个非 flagship cell 的 10-20 行 skeleton。完整实装留给生态贡献。

### A.1 HealthKit × Car HMI

```yaml
id: hms:health.vitals.summary_for_driver
device: {preferred: [car_hmi], fallback: [phone]}
bindings:
  appfunction:
    - device_type: car_hmi
      vendor: huawei
      platform: harmonyspace_6
      source: driver_profile_sync_from_phone
safety: {level: read, driving_constraint: safe_always}
# 疲劳驾驶告警、心率异常提示
```

### A.2 Location Kit × Watch

```yaml
id: hms:location.watch.current
device: {preferred: [watch], fallback: [phone]}
bindings:
  appfunction:
    - device_type: watch
      vendor: huawei
      kit: wear_engine
      ability: LocationAbility
# 跑步/户外场景独立 GPS 读取
```

### A.3 Location Kit × Car HMI

```yaml
id: hms:location.car.current
device: {preferred: [car_hmi]}
bindings:
  appfunction:
    - device_type: car_hmi
      kit: harmonyspace_location
      sources: [gnss, can_bus_speed, imu]
# 车机高精度融合定位（含车速/加速度）
```

### A.4 Push Kit × Watch

```yaml
id: hms:push.watch.vibrate
device: {preferred: [watch]}
bindings:
  appfunction:
    - device_type: watch
      vendor: huawei
      kit: wear_engine
      ability: NotificationAbility
# 手表震动通知（触觉反馈）
```

### A.5 Push Kit × Car HMI

```yaml
id: hms:push.car.hud
device: {preferred: [car_hmi]}
bindings:
  appfunction:
    - device_type: car_hmi
      kit: harmonyspace_notification
safety: {driving_constraint: safe_always}
# 车机通知（HUD 显示或音频），驾驶安全约束
```

### A.6 ML Kit × Phone

```yaml
id: hms:ml.translate.text
device: {preferred: [phone], fallback: []}
bindings:
  appfunction:
    - device_type: phone
      kit: hms_ml
      method: translateText
      compute: on_device    # 不需联网
  rest:
    url_template: "https://ml-api.cloud.huawei.com/..."
    compute: cloud          # 大模型质量更高
# compute-affinity 决策
```

### A.7 ML Kit × Watch

```yaml
id: hms:ml.watch.wakeword
device: {preferred: [watch]}
bindings:
  appfunction:
    - device_type: watch
      kit: hms_ml_lite
      ability: WakewordAbility
# 端侧 "Hey Siri/Celia" 等效，<30ms
```

### A.8 ML Kit × Car HMI

```yaml
id: hms:ml.car.driver_attention
device: {preferred: [car_hmi]}
bindings:
  appfunction:
    - device_type: car_hmi
      kit: harmonyspace_ads_ml
      camera: cabin_camera
safety: {level: dangerous, driving_constraint: safe_always, data_sensitivity: biometric}
# 驾驶员注意力监测（瞳孔/头部），Privacy-sensitive
```

### A.9 Site Kit × Phone

```yaml
id: hms:site.text_search
device: {preferred: [phone]}
bindings:
  rest: {url_template: "https://siteapi.cloud.huawei.com/.../textSearch"}
# 纯 REST，与 §4.5 车机版是同 tool 不同 device 默认
```

### A.10 Site Kit × Watch

```yaml
id: hms:site.watch.nearby
device: {preferred: [watch], fallback: [phone]}
bindings:
  appfunction:
    - device_type: watch
      kit: wear_engine
      proxy_to: phone       # 手表不独立查 Site，代理 phone 执行
      display: small_list   # 最多 3 条结果
output_hint: {prefer_display: [screen_small]}
# Apple Watch / Wear OS 同理
```

### A.11-A.15 延伸（earbuds / tablet / pc / tv / smart_home_hub 各 kit）

略。模式类似，未来由 HMS 团队或社区贡献者填充。

---

## Appendix B. External References

### B.1 Huawei 开发者文档入口
- HMS Core: `https://developer.huawei.com/consumer/en/hms/`
- HarmonyOS NEXT: `https://developer.huawei.com/consumer/cn/harmonyosnext/`
- HIMA 联盟: `https://en.wikipedia.org/wiki/Harmony_Intelligent_Mobility_Alliance`
- AppGallery Connect: `https://developer.huawei.com/consumer/en/service/josp/agc/`

### B.2 具体 kit 文档
- HealthKit REST API: `https://developer.huawei.com/consumer/en/doc/HMSCore-References/api-summary-desc-0000001226730563`
- Wear Engine: `https://developer.huawei.com/consumer/en/hms/huawei-wearengine`
- Push Kit: `https://developer.huawei.com/consumer/en/hms/huawei-pushkit/`
- Location Kit: `https://developer.huawei.com/consumer/en/doc/HMSCore-References/location-description-0000001088559417`
- Site Kit / ML Kit / Account Kit: `developer.huawei.com/consumer/en/hms/`

### B.3 HarmonyOS NEXT / Super Device
- DSoftBus 分布式软总线: `https://www.harmony-developers.com/p/harmonyos-dawn-the-philosophy-and`
- App Continuation: HarmonyOS 开发者文档
- HarmonyOS PC（2026-04 发布）: `https://windowsforum.com/threads/huawei-launches-harmonyos-pc...`

### B.4 Aito / HarmonySpace 6 / ADS 5.0
- HarmonySpace 6 / ADS 5.0 launch 参考（2026-04）
- Aito M7 / M9 产品页: AITO 官网
- HIMA 联盟成员（Aito / Luxeed / Seres / Maextro）

### B.5 ATD 相关
- ATD v3 whitepaper: `/home/nan/proj/anos/docs/research/toward-agent-tool-dispatch-v3.md`
- v3 Spec: `/home/nan/proj/anos/docs/superpowers/specs/2026-04-21-atd-v3-protocol-design.md`
- 本文档 spec: `/home/nan/proj/anos/docs/superpowers/specs/2026-04-21-atd-for-hms-core-applied-design.md`

### B.6 合规
- 中国 PIPL（个人信息保护法）: 决定 agent 跨设备数据流动的合规框架
- 欧盟 GDPR: HMS Global 场景相关
- HMS 应用市场审核要求: 集成 ATD 的 app 需考虑

---

**文档版本**：v0.1 · 2026-04-21 · Applied Design Proposal
**状态**：非约束性 design proposal，征求 HMS 团队反馈
**许可**：CC BY 4.0
**反馈**：`feedback@atd-protocol.org`（筹建中）· GitHub Issues（待启）

**前置阅读**：
- [ATD v3 whitepaper](toward-agent-tool-dispatch-v3.md)
- [ATD v2 whitepaper](toward-agent-tool-dispatch-v2.md)

**后续文档**：
- `why-atd-for-hms-core.md`（Editorial，Task #26，待写）
