# vec-base01：Base-01 v0.2 时间版场景的 VEC 报告

对 [时间版 Base-01](../../scenarios/base-01-timed/cases.toml) 的 20 个预登记用例，按 [VEC](../../specifications/viability-expansion.zh-CN.md) §7 生成机读报告 [report.toml](report.toml) 与人读摘要 [report.md](report.md)。方法类型为 `enumerated`（有限固定轨迹集合，无随机抽样）。CI 用 `--check` 重放机读报告。

重新生成：

```
cargo run -p poie-vec-base01 -- scenarios/base-01-timed/cases.toml --write experiments/vec-base01/report.toml experiments/vec-base01/report.md
```

## 结论变更记录

### 2026-11-01：T6、T11、T13、T18 由 `infeasible_in_model` 改为 `counterexample`（issue #106）

| 用例 | 旧结论（2026-10-31） | 新结论（2026-11-01） |
| --- | --- | --- |
| T6 | `infeasible_in_model`，证书 "capability-graph blocked: below_verification turn-lathe-nut" | `counterexample`，约束 `structural_route`，46800 s |
| T11 | `infeasible_in_model`，证书 "capability-graph blocked: cycle bus-1" | `counterexample`，约束 `structural_route`，61200 s |
| T13 | `infeasible_in_model`，证书 "capability-graph blocked: missing_resource bronze-bar" | `counterexample`，约束 `structural_route`，28800 s |
| T18 | `infeasible_in_model`，证书 "capability-graph blocked: cycle bus-1" | `counterexample`，约束 `structural_route`，61200 s |

结论计数由 "counterexample 7、held_on_tested 8、infeasible_in_model 4、unknown_input 1" 变为 "counterexample 11、held_on_tested 8、unknown_input 1"。指标（生存、韧性、独立性、过程）没有变化。

**原因是契约修正，不是为了让测试通过。** AI 预演评审（A-05、B-11、C-01）指出，旧报告把能力图的结构层 `blocked` 当作 `infeasible_in_model` 的证书，这一用法不成立：

1. `blocked` 只相对**查询时刻的快照**（资产状态与 `accepted` 库存），不考虑时域内库存时间线上的到货与补给（PO §6.4）；
2. PO 参考求解器**不完备**：多来源补足是贪心的（PO §6.3 规则 10），搜索有界（规则 14、17）。C-01 复现过同一查询 `solve` 判 `blocked`、`enumerate` 却找到候选的情况；
3. 本报告的方法是 `enumerated`，按 VEC §3.2 最强只能支持"在有限 W 上成立 / 找到反例"，本来就不能给出"在模型内不可行"。

修订后的 VEC §3.3 规定：`blocked` 只有在求解完备、时域乐观松弛后仍阻断、阻断原因确定、且方法为 `certificate` 时，才能升级为 `infeasible_in_model`；否则在 `enumerated` 报告中只能作为**被测策略**的 `counterexample`（约束 `structural_route`，首次违反时刻取用例起点），并注明"不是模型内不可行的证明"。报告校验（`poie-viability` 的 `MachineReport::validate`）现在拒绝非 `certificate` 方法下的 `infeasible_in_model`。

旧结论在实质上的状况：这四个用例的时间线在时域内没有能解除阻断的补给（铜棒只在 10 h 减少，母线与车床在时域内没有计划修复），所以"被测策略在这些场景里无法恢复目标"仍然成立；不成立的只是"在模型内不可行"这一更强的表述及其证书。
