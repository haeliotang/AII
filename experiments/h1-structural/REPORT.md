# H1 结构层实验：报告（REPORT）

- 实验：`h1-structural` · 预登记：[PREREG.md](PREREG.md) · 留出集：[HOLDOUT.md](HOLDOUT.md)
- 报告日期：2026-10-31
- 结论：按预登记的决策规则，H1 在留出集上被支持（弱形式，见 §7）；但 M3.3 的"外部贡献用例 ≥1/4"配额未满足，本结论只能作为内部证据，且真值由同一作者用同一模型写定（见 §8）。

## 1. 方法回顾

- **方法 A（Poieweave）**：`poie-ontology` v0.2-draft 能力图求解器（单账本、递归可用性、循环检测、M2 表达力）。
- **方法 B（基线）**：逐设备独立检查——直接状态、直接输入（accepted 库存求和）、直接占用、能力提供者的直接状态；不递归、不共享账本、不检测循环、不建模生产、不看能力等级。精确定义见 PREREG §2 与 `baseline/`。
- **真值**：40 个用例在首次运行前写定的预登记预期（verdict + root_causes）。
- **留出集**：10/40（25%），种子 `poieweave-h1-2026-10-31`，先于最终评估提交。

## 2. 复现清单

| 项 | 值 |
| --- | --- |
| 代码提交 | `87957a5`（本报告与原始输出基于该提交；报告随后的文档提交不改变实验代码） |
| 工具链 | `rustc 1.94.0 (4a4ef493e 2026-03-02)`，`cargo` 1.94.0 |
| 机器 | Apple M1 Pro（arm64），macOS |
| 构建 | `cargo build --release -p poie-h1-baseline` |
| 开发集运行 | `./target/release/poie-h1-baseline scenarios/base-01/cases.toml <开发集 30 个 ID>` → `raw/dev-2026-10-31.txt` |
| 全量运行 | `./target/release/poie-h1-baseline scenarios/base-01/cases.toml` → `raw/final-all-2026-10-31.txt` |
| 留出集运行 | `./target/release/poie-h1-baseline scenarios/base-01/cases.toml <留出集 10 个 ID>` → `raw/final-holdout-2026-10-31.txt` |
| 留出集名单 | [HOLDOUT.md](HOLDOUT.md)（种子、哈希方法与名单） |

逐用例原始输出（含每个用例的 tags、两种方法结论、阻断 subject）保存在 `raw/`。下列指标均可由这些文件重新计算。

## 3. 总体结果

| 数据集 | n | 一致 | 漏判 (blocked/unknown → 基线 feasible) | 误判 (candidate → 基线 blocked/unknown) | 根因 subject 匹配（基线） | 根因 subject 匹配（Poieweave） |
| --- | --- | --- | --- | --- | --- | --- |
| 开发集 | 30 | 15 | 4/17 (0.235) | 11/13 (0.846) | 1/19 (0.053) | 19/19 (1.000) |
| 留出集 | 10 | 7 | **2/7 (0.286)** | 1/3 (0.333) | 2/8 (0.250) | 8/8 (1.000) |
| 全部 | 40 | 22 | 6/24 (0.250) | 12/16 (0.750) | 3/27 (0.111) | 27/27 (1.000) |

**注意**：Poieweave 的"根因匹配 100%"不是独立证据——真值由同一作者依据同一模型的输出写定（见 §8）。可比较的是基线与真值之间的差异。

## 4. 按扰动类型（全部 40 例）

| 类型 | n | 一致 | 漏判 | 误判 | 根因匹配（基线） | 根因匹配（Poieweave） |
| --- | --- | --- | --- | --- | --- | --- |
| common-cause | 5 | 1 | **4/4 (1.000)** | 0/1 | 0/6 | 6/6 |
| cycle | 2 | 2 | 0/2 | 0/0 | 0/2 | 2/2 |
| stock-contention | 14 | 7 | 0/7 | 7/7 (1.000) | 0/8 | 8/8 |
| quality-gate | 4 | 2 | 0/2 | 2/2 (1.000) | 0/2 | 2/2 |
| verification | 5 | 4 | 1/4 (0.250) | 0/1 | 0/4 | 4/4 |
| unknown | 3 | 2 | 1/3 (0.333) | 0/0 | 2/3 | 3/3 |
| other | 7 | 4 | 0/2 | 3/5 (0.600) | 1/2 | 2/2 |

（"漏判"的分母是该类型中真值为 blocked/unknown 的用例数；"误判"的分母是真值为 candidate 的用例数。）

## 5. 基线的漏判（H1 关注的现象）

6 个用例中，基线把真实阻断判成可行：

| 用例 | tags | 真值 | Poieweave | 基线 | 分类 | 预期根因 | 基线报告根因 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| S3 | common-cause | blocked | blocked | candidate | missed_blocking | bus-1 | — |
| S12 | common-cause | blocked | blocked | candidate | missed_blocking | bus-1,emergency-bus | — |
| S19 | verification | blocked | blocked | candidate | missed_blocking | sleeve-replacement | — |
| S32 | common-cause | blocked | blocked | candidate | missed_blocking | bus-1,breaker-b1 | — |
| S34 | unknown | unknown | unknown | candidate | missed_blocking | bus-1 | — |
| S39 | common-cause | blocked | blocked | candidate | missed_blocking | breaker-b1 | — |


共性：漏判集中在**联合依赖**——共因故障（S3/S12/S32/S39：机器人/充电桩/母线同源故障）、能力等级不足（S19）、未知状态沿依赖传播（S34）。基线逐设备看直接状态时，这些阻断不可见。

## 6. 基线的误判

12 个用例中，基线把可行判成阻断：

| 用例 | tags | 真值 | Poieweave | 基线 | 分类 | 预期根因 | 基线报告根因 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| S1 | stock-contention | candidate | candidate | blocked | false_block | — | sleeve-p1 |
| S8 | stock-contention | candidate | candidate | blocked | false_block | — | sleeve-p1 |
| S13 | stock-contention | candidate | candidate | blocked | false_block | — | sleeve-p1 |
| S15 | other | candidate | candidate | blocked | false_block | — | lathe-nut |
| S18 | quality-gate | candidate | candidate | blocked | false_block | — | sleeve-p1 |
| S21 | stock-contention | candidate | candidate | blocked | false_block | — | sleeve-p1 |
| S23 | stock-contention | candidate | candidate | blocked | false_block | — | sleeve-p1 |
| S25 | stock-contention | candidate | candidate | blocked | false_block | — | sleeve-p1 |
| S27 | quality-gate | candidate | candidate | blocked | false_block | — | sleeve-p1 |
| S35 | other | candidate | candidate | blocked | false_block | — | lathe-nut |
| S36 | other | candidate | candidate | blocked | false_block | — | lathe-nut |
| S38 | stock-contention | candidate | candidate | blocked | false_block | — | sleeve-p1 |


共性：全部是"需要生产/多来源/降级策略"的路线——基线按定义不建模生产（obtain 只查库存）、不组合多来源、不处理降级策略，因此这些误判是其定义带来的，而不是随机错误。这说明基线的**漏判**不能简单用"基线更保守"解释：两类错误出现在不同用例类型上。

## 7. 结论（按预登记决策规则）

- 留出集上：基线漏判 **2/7（0.286）**，Poieweave 漏判 0/7；基线误判 1/3。共性共因类型在留出集为 **2/2 漏判**（S3、S32）。
- 按 PREREG §7 的规则："在留出集上，基线的漏判率高于 Poieweave 的漏判率，且该差异不能由基线误判率更高来解释" → **满足**。因此 **H1 在本实验的留出集上被支持（弱形式）**。
- "弱形式"指：支持的是"联合依赖模型能识别逐设备基线遗漏的阻断条件"这一结构性断言。它**不能**证明 Poieweave 的结论在物理上正确。
- 没有发现需要修正模型的证据；未出现 Poieweave 与最终预登记预期不一致的用例（40/40 通过）。

## 8. 局限与本实验自身的错误

1. **循环论证风险（最重要）**：真值由作者依据同一模型写定，Poieweave 的"漏判 0"因此不能作为其正确性的独立证据；本实验真正可外推的只有"基线与真值的差异"。
2. **外部用例配额未满足**：PREREG §4 要求 ≥1/4 用例来自作者以外；当前为 0。M3.3 的该退出条件未满足，H1 结论只能标注为内部证据。
3. **留出集弱隔离**：CI 与场景测试会执行全部用例；harness 在 `HOLDOUT.md` 提交前也在全部 40 例上做过一次冒烟运行（PREREG §5 已披露）。本次未据此修改实现、预期或基线。
4. **预期错误（已披露）**：M3.3 首次运行发现 3 处预期写错（S31/S35 的验证门槛、S37 的请求量），已按 §1.4 记录理由并修正，发生在留出集选定之前。
5. **基线实现偏差**：基线由同一团队实现，且定义上故意不含生产/多来源/降级策略；其高误判率部分是定义使然。若换成更接近真实调度的基线，漏判率可能变化。
6. **规模与层次**：40 个合成用例、结构层、无时间维度；指标只作方向性判断，不构成统计显著性结论。

## 9. 与 M3 退出条件

| 退出条件 | 状态 |
| --- | --- |
| 报告合并到 `experiments/h1-structural/REPORT.md`，附复现清单 | 是（本文件 §2） |
| 无论结果是否支持 H1，写明结论与局限 | 是（§7–§8） |
| 不支持时登记需修正的模型或方法 | 不适用（未发现需要修正模型的证据） |
| M3.3 外部贡献用例 ≥1/4 | **未满足**（依赖外部贡献，M3 因此标记为"基本完成"） |

## 10. 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-10-31 | 初版：含开发集、留出集与全量结果；外部配额与循环论证局限已披露 |
