# 时间版场景：Autonomous Base-01（M5.6）

版本：0.1 / 2026-10-31
依据：[Physical Ontology v0.2-draft](../../specifications/physical-ontology.zh-CN.md) §3.8–3.11 · [开发文档](../../docs/DEVELOPMENT.zh-CN.md) M5

## 1. 目的

在结构版 Base-01（[`../base-01`](../base-01/README.zh-CN.md)）之上叠加时间：泵的磨损、母线的预测失效、昼夜能量、补给中断。用例回答"从某个时刻开始，目标能否在截止时间前、用当时真正可用的资源完成"。

## 2. 文件

| 文件 | 内容 |
| --- | --- |
| [`scenario.toml`](scenario.toml) | 时间线：资产状态变化与预测失效、库存变化 |
| [`cases.toml`](cases.toml) | 20 个**预登记**时间版用例（T0–T19） |
| [`digests.toml`](digests.toml) | 每个用例的执行摘要（replay digest），由运行器生成并在 CI 中逐位校验 |

## 3. 结论类别

`feasible`（在截止前完成）/ `blocked`（结构上无路可走）/ `unknown` / `search_limit` / `too_late`（有路线但完成晚于截止）/ `insufficient_resource`（步骤开始时刻资源不足）/ `capacity_conflict`（资产被重叠占用）。

截止时间可以是显式时间，也可以是 `"prediction"`：取目标所依赖资产中最早的预测失效窗口起点（"在预测失效前修好"）。

## 4. 时间线（合成假设）

| 对象 | 时间线 |
| --- | --- |
| pump-p1 | 初始 degraded；6 h 磨损为 failed；预测失效窗口 4–8 h |
| bus-1 | 初始 available；16 h failed；预测失效窗口 5–9 h |
| lathe-1 | 初始 available；12 h failed |
| pv-1 | 无状态证据（unknown） |
| electric-energy | 初始 1 kWh；8 h 光伏补给 +40 kWh |
| bronze-bar | 初始 3 kg；10 h 补给中断 −3 kg |
| cutting-insert | 初始 4 item；10 h 离场 −4 item |

## 5. 运行与复现

```bash
cargo run -p poie-sim --bin poie-timed -- scenarios/base-01-timed/cases.toml
cargo run -p poie-sim --bin poie-timed -- scenarios/base-01-timed/cases.toml \
  --check-digests scenarios/base-01-timed/digests.toml
```

复用运行后，重新生成摘要（仅在输入或实现有意改变时）：

```bash
cargo run -p poie-sim --bin poie-timed -- scenarios/base-01-timed/cases.toml \
  --write-digests scenarios/base-01-timed/digests.toml
```

**复现清单**：模型版本 = 本仓库提交（结构模型 `../base-01/model.toml` + 本目录两个输入文件）；事件顺序 = 时间 + 调度顺序（确定性，无随机源，种子字段留待随机扰动引入）；浮点容差 = 资源检查 1e-6、时间比较 1e-6 秒、digest 以 3 位小数格式化。任意一次运行都应与 `digests.toml` 逐位一致；CI 会执行该校验（M5.5）。
