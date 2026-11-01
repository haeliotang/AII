# H4 扩张核算报告：切屑回收熔炉

预登记：[PREREG.md](PREREG.md)。提案：[proposal.toml](../../scenarios/base-01-expansion/proposal.toml)。

**本报告按 VEC 2026-11-01 修订（issue #109、#110）重跑**：结构层在运营门槛 `demonstrated` 上、分三个时点评估，并扣除建造与 bootstrap 消耗；时域层的扩张分支从场景库存中支付建造消耗，并带入熔炉自身的故障机制。旧结论见 §7 变更记录。本报告最多给出 `modeled_gain`，不是实际建成证据。

## 1. 结构层（VEC §6.2.1）

- 运营门槛：`demonstrated`（人类策略给定，与 H2/H4 规划一致；不由提案给定）
- 投运等级：`demonstrated`（不低于运营门槛）；熔炉建成前不存在，建成未投运时以当前等级 `modeled` 参与
- 扣除的建造与 bootstrap 消耗（同一资源取两者较大值）：bronze-bar 1 kg、electric-energy 20 kWh；建造期间不可用的资产：lathe-1、robot-m1

| 时点 | 获得 | 失去 | 未定（search_limit） |
| --- | --- | --- | --- |
| 建造期间 | （无） | potable-water: candidate → blocked; restore:lathe-1: candidate → blocked | （无） |
| 建成未投运 | （无） | （无） | （无） |
| 投运后（假设达到投运等级） | obtain:sleeve-p1: blocked → candidate | （无） | （无） |

- 汇总：获得（投运后，以投运达标为前提）obtain:sleeve-p1: blocked → candidate；失去（任一时点）potable-water: candidate → blocked; restore:lathe-1: candidate → blocked；投运后保持 candidate 2、保持不可行 1
- 失去列表中的关键服务目标：potable-water（建造期间）

## 2. 时域层（VEC §6.2.2）

- 场景：16（H2 开发集；留出集未使用）
- 不扩张成功：16/16
- 扩张：本地库存无法支付建造消耗（不可建造）16/16；可建造且成功 0/16
- 不可建造的原因（示例）：electric-energy: needs 20 kWh, accepted stock short by 19 kWh
- 建造期（0–8 h）关键服务检查（只覆盖可建造的场景）：无可检查场景
- 不扩张 potable-water：KM 中位维持时长 > 86400 s，截尾 16/16
- 关键服务非劣效（δ = 0，未预登记，仅作描述）：不满足或无法比较（扩张分支在部分或全部场景中不可建造）
- 扩张分支的扰动集合 = 基线扰动 ∪ 熔炉故障（MTBF 500 h，synthetic；来源：synthetic assumption, review C-08 example; not calibrated）。共同外生源抽得的熔炉故障时刻：2 个场景在 24 h 时域内发生
- 时域层只比较供水服务；扩张的结构收益在准备目标（轴套 4）上，只在投运后出现。

## 3. 外部依赖

- 建造期外部输入：furnace-module 1 item（无本地供应，已列出；未闭合）
- 维护（upkeep）只声明了时域内的 5 kWh；按设施寿命或年化的维护核算（VEC §6.1 修订）在提案中尚未给出。

## 4. 采纳条件逐条核对（VEC §6.3）

1. 投运等级不低于运营门槛：满足（`demonstrated` ≥ `demonstrated`）；增益以投运达到该等级为前提。
2. 失去列表无关键服务恢复目标（或已被授权者接受）：**不满足**，且没有授权者接受该风险。
3. 时域层关键服务不劣于基线、且预登记的唯一主要长期目标超过阈值：**无法满足**（PREREG 未预登记主要长期目标与非劣效界；扩张分支在 16/16 个场景中不可建造）。
4. 未闭合外部依赖已列出：满足（furnace-module）。
5. 复现清单：见 §6。

## 5. 判定（PREREG §6；VEC §6.4）

- **判定：净增益为负**（失去列表含关键服务目标（建造占用唯一的维修机器人，供水恢复路线在建造期间失去）；时域层劣于基线：扩张分支在 16/16 个场景中无法从本地库存支付建造消耗）。
- PREREG §6 的「不确定」条件同样成立（未闭合外部依赖 furnace-module），PREREG 没有规定两类条件同时成立时的优先级。本报告判为负，理由是：闭合 furnace-module 不能消除上述失去与不可建造，因此不确定性只可能让结论持平或更差，不会让它转正。若维护者另定优先级，本节需按新规则重判。
- 声明等级：`proposed`。`modeled_gain` 不成立（§6.3 第 2、3 条不满足）；`demonstrated_gain` 需要实际建成（M11.6，外部条件）。

## 6. 每场景结果

| 场景 | 不扩张成功 | 不扩张完成 (h) | 不扩张期末储能 (kWh) | 扩张可建造 | 扩张成功 | 扩张完成 (h) | 熔炉故障 (h) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| h2-00 | true | 3.00 | 0.50 | false | - | - | 373.3 |
| h2-01 | true | 3.00 | 0.50 | false | - | - | 179.5 |
| h2-02 | true | 3.00 | 0.00 | false | - | - | 1379.3 |
| h2-03 | true | 3.00 | 0.50 | false | - | - | 152.9 |
| h2-04 | true | 3.00 | 0.50 | false | - | - | 189.9 |
| h2-05 | true | 3.00 | 0.00 | false | - | - | 898.0 |
| h2-06 | true | 3.00 | 0.00 | false | - | - | 8.0 |
| h2-07 | true | 3.00 | 1.50 | false | - | - | 321.9 |
| h2-08 | true | 3.00 | 1.50 | false | - | - | 153.7 |
| h2-09 | true | 3.00 | 1.50 | false | - | - | 355.2 |
| h2-10 | true | 3.00 | 0.00 | false | - | - | 961.5 |
| h2-11 | true | 3.00 | 1.50 | false | - | - | 254.2 |
| h2-12 | true | 3.00 | 0.50 | false | - | - | 1043.2 |
| h2-13 | true | 3.00 | 0.50 | false | - | - | 21.7 |
| h2-14 | true | 3.00 | 0.50 | false | - | - | 2560.2 |
| h2-15 | true | 3.00 | 0.50 | false | - | - | 460.7 |

## 7. 复现清单与变更记录

- 命令：`cargo run -p poie-h4-expansion -- --write experiments/h4-expansion`
- 种子：H2 开发集种子 `poieweave-h2-2026-10-31`（16 场景）；熔炉故障使用同一种子派生的独立流（`furnace1`）；确定性 runner。
- 原始输出：[`raw/h4.txt`](raw/h4.txt)。
- 2026-10-31 初版结论：**不确定**（理由：建造资源未在库存中扣除；外部依赖 furnace-module 未闭合）。当时结构层在 `modeled` 门槛上报告获得 obtain:sleeve-p1、失去（无），时域层 16/16 对 16/16。
- 2026-11-01 按 VEC 修订（issue #109、#110）重跑，结论改为本报告 §5。改变原因是契约修正，而不是数据或预期的调整：评估门槛改为运营门槛 `demonstrated`（提案原自设 `modeled`）；扣除建造与 bootstrap 消耗、建造期间占用资产不可用（暴露了建造期供水恢复路线的失去，以及 H2 场景储能 0.5–2 kWh 不足以支付建造所需 20 kWh）；扩张分支带入熔炉自身故障。
