# BaSyx AAS 只读适配器兼容性预演报告（M9.2）

> **AI 预演 · 本机容器 · 非现场设备** —— 不计入 M9.2 退出条件。

- 预登记：[PREREG.md](PREREG.md)
- 镜像：`eclipsebasyx/aas-environment:2.0.0-milestone-15`
- 端点：`http://127.0.0.1:58386`；读取路径：`GET /submodels/dXJuOnBvaWV3ZWF2ZTpiYXNlLTAxOnB1bXAtcDE6b3BlcmF0aW9uYWwtZGF0YQ`
- 适配器：`basyx-aas-readonly`（std `TcpStream` 手写 HTTP/1.1 GET；只读；`MarkUnknown`）
- 证据过期窗口：60000 ms
- 原始输出：`raw/`（每阶段 `phase-*.txt`、`adapter-log.jsonl`、`checks.tsv`、`run.log`）

## 兼容性项

| 编号 | 兼容性项 | 期望 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| C1 | 读取 Submodel 属性值 | HTTP 200；OperatingState → pump-p1 = Degraded；FlowRate 原值 12.5 | HTTP Some(200)；错误 = none；状态 = Some(Degraded)；FlowRate 原值 = Some("12.5") | PASS |
| C2a | 单位映射（已知单位） | unit = m3/h；SI = 0.003472222… m3/s（相对误差 < 1e-9） | unit = Some("m3/h")；SI = Some(0.003472222222222222) | PASS |
| C2b | 单位映射（未知单位） | bbl/d 被拒绝：无 SI 值、有原因、轮询不失败 | unit = Some("bbl/d")；结果 = Some(Err("unit \"bbl/d\" is not in the adapter's unit table"))；轮询错误 = none | PASS |
| C3a | 时间戳语义（事件时间） | observed_at_ms = 注入的 ObservedAt；ingested_at_ms ≥ observed_at_ms | ObservedAt = Some("2026-11-01T08:58:39Z")；observed_at_ms = Some(1793523519000)（期望 Some(1793523519000)）；ingested_at_ms = Some(1793523519682)；quality = Some(Good) | PASS |
| C3b | 时间戳语义（BaSyx 原生） | Property JSON 无服务端时间戳字段 | Property 键集合 = {"embeddedDataSpecifications", "idShort", "modelType", "value", "valueType"}；时间戳类键 = [] | PASS |
| C4 | 只读影子运行 | 投影 pump-p1 = Degraded；dispatched = false | 投影 pump-p1 = Degraded；计划动作 0 个；dispatched = false | PASS |
| C5 | 容器停止后的断连 | Disconnected；pump-p1 = Unknown；healthy = false；接受观测 0 | 错误 = Disconnected(127.0.0.1:58386: Connection refused (os error 61) (ConnectionRefused))；pump-p1 = Some(Unknown)；healthy = false；接受观测 0 | PASS |
| C6 | 容器重启后（尚未重注入） | HTTP 404；Protocol 错误；pump-p1 = Unknown；healthy = false | HTTP Some(404)；错误 = Protocol(HTTP 404 for GET /submodels/dXJuOnBvaWV3ZWF2ZTpiYXNlLTAxOnB1bXAtcDE6b3BlcmF0aW9uYWwtZGF0YQ)；pump-p1 = Some(Unknown)；healthy = false | PASS |
| C7 | 重注入后重连 | 轮询成功；pump-p1 = Degraded；healthy = true；observed_at = 新 ObservedAt | 错误 = none；pump-p1 = Some(Degraded)；healthy = true；observed_at_ms = Some(1793523529000)（期望 Some(1793523529000)） | PASS |

失败数：0 / 9

## 影子计划（仅记录，未派发）

- （无计划动作）

注：计划动作为 0 时，C4 的 `dispatched = false` 是平凡满足的；本次只证明了「真实适配器观测 → 投影 → 影子规划」路径可运行，没有证明非空计划在影子模式下不派发（该性质由 `experiments/shadow-run` 的模拟演示覆盖）。

## 结论

C1–C7 全部 PASS：本机预演支持该只读适配器与此 BaSyx 版本的兼容性。这仍只是 AI 在本机单容器上的预演，M9.2 需要现场/测试实例与兼容性 ADR。

## 局限与注记

- 单容器、本机回环网络、无认证、镜像默认（InMemory）后端；不覆盖 TLS、权限、MongoDB 持久化后端、订阅/事件（MQTT）与规模。
- 数据为 AI 编写的合成值，不是现场设备读数；`ObservedAt` 由注入脚本写入，不是设备时钟。
- BaSyx 的 AAS v3 Property 不带服务端时间戳（C3b）：只有显式提供 `ObservedAt` 一类属性的 Submodel 才能声明 `EventTime`；否则适配器把观测降为 `uncertain` 并以接入时间计。
- C6 表明重启后数据丢失（InMemory）：HTTP 可达不等于数据可用；适配器把 404 视为错误而不是「无变化」，因此不会伪造状态。
- 断连与重连之间的间隔短于证据过期窗口，证据过期路径未在本次真实运行中被触发（由 `poie-runtime` 单元测试覆盖）。
- 仅测 BaSyx；Eclipse Ditto 与 ROS 2 未测。M9.2 退出条件仍需现场/测试实例与兼容性 ADR。
