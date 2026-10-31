# H4 扩张核算报告：切屑回收熔炉

预登记：[PREREG.md](PREREG.md)。提案：[proposal.toml](../../scenarios/base-01-expansion/proposal.toml)。

**局限：建造资源与占用尚未在库存模型中扣除；本报告最多给出 `modeled_gain`，不是实际建成证据。**

## 1. 结构层（VEC §6.2.1）

- 获得：obtain:sleeve-p1: blocked → candidate
- 失去：（无）
- 保持 candidate：3；保持不可行：0

## 2. 时域层（VEC §6.2.2）

- 场景：16（H2 开发集；留出集未使用）
- 不扩张成功：16/16；扩张成功：16/16
- 建造期（0–8 h）关键服务检查：通过
- potable-water：min maintained None s，censored 16
- potable-water 维持时长变化：None s
- 时域层只比较供水服务；扩张的结构收益在准备目标（轴套 4、车床螺母 2）上，
  未反映到时域表中，因此时域层本身不构成正增益证据。

## 3. 外部依赖

- 建造期外部输入：furnace-module 1 item（无本地供应，已列出；未闭合）

## 4. 判定（VEC §6.4）

- **判定：不确定**（原因：建造资源未在库存中扣除；外部依赖 furnace-module 未闭合）。
- 声明等级：`modeled_gain` 不足以主张；`demonstrated_gain` 需要实际建成（M11.6，外部条件）。

## 5. 每场景结果

| 场景 | 不扩张成功 | 扩张成功 | 不扩张完成 (h) | 扩张完成 (h) |
| --- | --- | --- | --- | --- |
| h2-00 | true | true | 3.00 | 3.00 |
| h2-01 | true | true | 3.00 | 3.00 |
| h2-02 | true | true | 3.00 | 3.00 |
| h2-03 | true | true | 3.00 | 3.00 |
| h2-04 | true | true | 3.00 | 3.00 |
| h2-05 | true | true | 3.00 | 3.00 |
| h2-06 | true | true | 3.00 | 3.00 |
| h2-07 | true | true | 3.00 | 3.00 |
| h2-08 | true | true | 3.00 | 3.00 |
| h2-09 | true | true | 3.00 | 3.00 |
| h2-10 | true | true | 3.00 | 3.00 |
| h2-11 | true | true | 3.00 | 3.00 |
| h2-12 | true | true | 3.00 | 3.00 |
| h2-13 | true | true | 3.00 | 3.00 |
| h2-14 | true | true | 3.00 | 3.00 |
| h2-15 | true | true | 3.00 | 3.00 |

## 6. 复现清单

- 命令：`cargo run -p poie-h4-expansion -- --write experiments/h4-expansion`
- 种子：H2 开发集种子 `poieweave-h2-2026-10-31`（16 场景）；确定性 runner。
- 原始输出：[`raw/h4.txt`](raw/h4.txt)。
