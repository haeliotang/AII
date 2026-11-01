# M9.2 BaSyx AAS 只读适配器兼容性预演登记（PREREG，AI 预演）

- 实验 ID：`basyx-shadow`
- 日期：2026-11-01
- 状态：**已运行**（2026-11-01；登记时为「已登记，尚未运行」，本文件先于任何适配器代码运行写定）；结果见 [REPORT.md](REPORT.md)
- 性质：**AI 预演；本机单容器，不是现场设备，不计入 M9.2 退出条件。**
- 依据：[外部行动清单 §F](../../docs/status/external-actions.zh-CN.md) · `crates/poie-runtime/src/adapter.rs`（`ReadOnlyAdapter`）· `crates/poie-runtime/src/runtime.rs`（`LocalRuntime`）

## 1. 问题

针对一个真实运行的 Eclipse BaSyx Java v2 AAS Environment 实例（本机 Docker 单容器），一个只经 HTTP GET 读取的 `ReadOnlyAdapter` 能否：正确读取 Submodel 属性值、映射单位、给出可辩护的时间戳语义，并在容器停止/重启时按声明的断连行为（`MarkUnknown`）降级、在数据恢复后重连，且整个只读影子运行（`LocalRuntime` 投影 + `shadow_plan`）不派发任何动作？

## 2. 被测系统（先于运行固定）

| 项 | 值 |
| --- | --- |
| 镜像 | `eclipsebasyx/aas-environment:2.0.0-milestone-15`（Docker Hub 上的正式 milestone 标签，非 SNAPSHOT） |
| 镜像 digest | `sha256:313da867059233e3c07e17a8ac21153a1f8fa0d02eada4e6b1b9f9e86afbd321`（本机拉取，arm64） |
| 容器端口 | 容器内 8081，映射到本机 `127.0.0.1` 上的随机空闲端口（由 `run.sh` 选择） |
| 后端 | 镜像默认配置（预期为 InMemory，不挂卷、不改配置） |
| 认证 | 无（镜像默认未启用认证；适配器不携带任何凭据） |
| 适配器访问 | 仅 HTTP/1.1 `GET`；数据注入由 `run.sh` 用 `curl POST` 完成，适配器本身不写 |
| 运行环境 | 本机 Docker 29.8.1，8 GB |

## 3. 数据集（先于运行固定）

Base-01（`scenarios/base-01/model.toml`）资产 `pump-p1` 的一个运行数据 Submodel（`experiments/basyx-shadow/data/`）：

| idShort | valueType | 值 | 单位来源 |
| --- | --- | --- | --- |
| `OperatingState` | `xs:string` | `degraded`（与 model.toml 中的状态一致） | — |
| `FlowRate` | `xs:double` | `12.5` | IEC 61360 数据规范 `unit = m3/h` |
| `FlowRateExport` | `xs:double` | `100` | IEC 61360 数据规范 `unit = bbl/d`（故意使用适配器单位表之外的单位） |
| `ObservedAt` | `xs:dateTime` | 注入时由 `run.sh` 写入的 UTC 时间（秒精度，`Z` 结尾） | — |

另注入一个引用该 Submodel 的 AAS（`globalAssetId` 指向 `pump-p1`）。

## 4. 兼容性项、期望与判定

适配器声明：`kind = ReadOnly`，`units = [asset.status → 1, pump-p1.flow → m3/s]`，`timestamp_semantics = EventTime`（来源为 `ObservedAt` 属性），`disconnect_behavior = MarkUnknown`。`LocalRuntime` 证据过期窗口 60 s。

| 编号 | 兼容性项 | 期望 | 判定规则 |
| --- | --- | --- | --- |
| C1 | 读取 Submodel 属性值 | 一次 GET 取回 Submodel；`OperatingState` 映射为 `pump-p1 = Degraded`；`FlowRate` 原值 12.5 | 两者都满足 → PASS |
| C2a | 单位映射（已知单位） | 从 IEC 61360 `unit` 读到 `m3/h`，换算为 SI：12.5 m3/h = 0.003472222… m3/s | 单位字符串完全相同且相对误差 < 1e-9 → PASS |
| C2b | 单位映射（未知单位） | `bbl/d` 被拒绝：不产生 SI 值、记录原因、轮询本身不失败 | 三者都满足 → PASS |
| C3a | 时间戳语义（事件时间） | `observed_at_ms` 等于注入的 `ObservedAt`（毫秒）；`ingested_at_ms ≥ observed_at_ms` | 都满足 → PASS |
| C3b | 时间戳语义（BaSyx 原生） | BaSyx 返回的 Property JSON 中**没有**服务端生成的时间戳字段（AAS v3 Property 无时间戳）；因此事件时间只能来自显式属性 | 返回的 Property 键集合中无 `timestamp`/`time`/`updated` 类字段 → PASS（结论：未提供 `ObservedAt` 的 Submodel 不能声明 `EventTime`） |
| C4 | 只读影子运行 | `LocalRuntime` 投影后 `pump-p1 = Degraded`；`shadow_plan`（目标：服务 `potable-water`）`dispatched = false` | 都满足 → PASS |
| C5 | 容器停止后的断连 | `docker stop` 后轮询返回 `AdapterError::Disconnected`（连接拒绝/重置/无响应均归入此类，而非伪造数据）；`pump-p1` 投影为 `Unknown`；`healthy = false`；接受观测数 0 | 四者都满足 → PASS |
| C6 | 容器重启后（尚未重注入） | 默认 InMemory 后端在重启后丢失数据：HTTP 可达，但 Submodel GET 返回 404；适配器返回 `Protocol` 错误（不是成功、也不伪造状态）；`pump-p1` 仍为 `Unknown`；`healthy = false` | 都满足 → PASS |
| C7 | 重注入后重连 | 轮询成功；`pump-p1 = Degraded`；`healthy = true`；`observed_at_ms` 等于新的 `ObservedAt` | 都满足 → PASS |

总判定：C1–C7 全部 PASS → 本机预演支持「该适配器与 BaSyx `2.0.0-milestone-15` 的只读兼容性」；任一 FAIL → 预演不通过，如实保留并逐项分析（实现错 / 模型错 / 预期错），**不得只改期望使结果变绿**。

## 5. 运行与记录

- 由 `run.sh` 编排：`docker run`（固定标签）→ 等待就绪 → 注入 → runner 第 1 阶段（C1–C4）→ `docker stop` → runner（C5）→ `docker start` → 等待就绪 → runner（C6）→ 重注入 → runner（C7）→ 清理容器。
- runner 将原始 HTTP 响应、轮询结果与适配器日志写入 `raw/`，并生成 `REPORT.md`（表头注明「AI 预演 · 本机容器 · 非现场设备」）。
- runner 的单元测试不依赖 Docker；需要 Docker 的部分只在 `run.sh` 中。

## 6. 局限

- 单容器、本机回环网络、无认证、InMemory 后端；不覆盖生产部署、TLS、权限、持久化后端（MongoDB）、多 Submodel 规模、订阅/事件（MQTT）。
- 数据为 AI 编写的合成值，不是现场设备读数。
- 只测 BaSyx；Eclipse Ditto 与 ROS 2 未测。M9.2 退出条件仍需现场/测试实例与兼容性 ADR。

## 7. 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-11-01 | 初版，先于运行写定 |
| 2026-11-01 | 运行 1：C1–C7（9 项）全部 PASS，期望未改动。运行后仅为报告生成器增加「局限与注记」与 C4 注记（计划动作为 0 时 `dispatched = false` 为平凡满足），未改判定规则 |
| 2026-11-01 | 运行 2（报告生成器更新后重跑，当前 REPORT.md）：结果与运行 1 相同，9/9 PASS |
