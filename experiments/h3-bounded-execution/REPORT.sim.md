# H3 有界执行：模拟运行报告（simulated, not physical）

本报告由 `poie-h3-bounded-execution --write <dir>` 生成；预登记见 [PREREG.md](../PREREG.md)。

**限制**：这是模拟执行端（`crates/poie-exec::sim`），不是物理试验台；M10 退出条件要求的真实故障与安全演练未覆盖。

| 场景 | 动作 | 故障 | 结果 | 物理动作 | 已登记重复 | 未登记重复 | SAP 状态 | 独立验证 | 判定 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| h3-0-None | replace-sleeve-p1 | none | dispatched | 1 | 0 | 0 | acknowledged | pump-status-sensor | PASS |
| h3-1-None | turn-sleeve-p1 | none | dispatched | 1 | 0 | 0 | acknowledged | pump-status-sensor | PASS |
| h3-2-DropAck | replace-sleeve-p1 | ack_lost | dispatched | 1 | 0 | 0 | outcome_unknown | — | PASS |
| h3-3-DropAck | turn-sleeve-p1 | ack_lost | dispatched | 1 | 0 | 0 | outcome_unknown | — | PASS |
| h3-4-Crash | replace-sleeve-p1 | crash_after_execute | rejected | 1 | 0 | 0 | outcome_unknown | — | PASS |
| h3-5-Crash | turn-sleeve-p1 | crash_after_execute | rejected | 1 | 0 | 0 | outcome_unknown | — | PASS |
| h3-6-Duplicate | replace-sleeve-p1 | duplicate_delivery | dispatched | 2 | 1 | 0 | acknowledged | pump-status-sensor | PASS |
| h3-7-Duplicate | turn-sleeve-p1 | duplicate_delivery | dispatched | 2 | 1 | 0 | acknowledged | pump-status-sensor | PASS |
| h3-8-Network | replace-sleeve-p1 | network_loss | fallback | 0 | 0 | 0 | authorized | — | PASS |
| h3-9-Network | turn-sleeve-p1 | network_loss | fallback | 0 | 0 | 0 | authorized | — | PASS |
| h3-10-Takeover | replace-sleeve-p1 | human_takeover | recorded_only | 0 | 0 | 0 | authorized | — | PASS |
| h3-11-Takeover | turn-sleeve-p1 | human_takeover | recorded_only | 0 | 0 | 0 | authorized | — | PASS |
| h3-12-Degrade | replace-sleeve-p1 | runtime_degraded | fallback | 0 | 0 | 0 | authorized | — | PASS |
| h3-13-Degrade | turn-sleeve-p1 | runtime_degraded | fallback | 0 | 0 | 0 | authorized | — | PASS |

## 汇总

- 场景数：14
- 未登记重复物理动作：0（要求 0）
- 已登记重复物理动作：2（F3 重复投递的显式记录）
- 独立验证通过：4
- 未通过场景：0
- 结论：模拟 H3 被支持：无未登记物理重复，状态可对账，接管/降级不派发
