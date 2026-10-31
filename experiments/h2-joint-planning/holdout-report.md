# H2 留出集结果（自动生成）

本文件由 `poie-h2-joint-planning --holdout --write <dir>` 生成，是留出集的**首次**运行；开发集结果见 [DEV-NOTES.md](DEV-NOTES.md)。预登记见 [PREREG.md](PREREG.md)。

| 方法 | 成功 | 场景数 | 成功率 | 平均完成 (h) | 平均动作数 | SAP 通过 |
| --- | --- | --- | --- | --- | --- | --- |
| A_joint | 5 | 8 | 0.625 | 5.375 | 1.0 | 8/8 |
| B1_task_priority | 5 | 8 | 0.625 | 5.375 | 1.0 | 8/8 |
| B2_safety_stock | 5 | 8 | 0.625 | 5.375 | 1.0 | 8/8 |
| B3_short_term | 5 | 8 | 0.625 | 5.375 | 1.0 | 8/8 |

## 每场景结果

| 场景 | 方法 | 成功 | 完成 (h) | 动作 | 未达成 |
| --- | --- | --- | --- | --- | --- |
| h2-16 | A_joint | false | 8.00 | 1 | potable-water:deadline missed |
| h2-16 | B1_task_priority | false | 8.00 | 1 |  |
| h2-16 | B2_safety_stock | false | 8.00 | 1 |  |
| h2-16 | B3_short_term | false | 8.00 | 1 |  |
| h2-17 | A_joint | true | 3.00 | 1 |  |
| h2-17 | B1_task_priority | true | 3.00 | 1 |  |
| h2-17 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-17 | B3_short_term | true | 3.00 | 1 |  |
| h2-18 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-18 | B1_task_priority | true | 4.00 | 1 |  |
| h2-18 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-18 | B3_short_term | true | 4.00 | 1 |  |
| h2-19 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-19 | B1_task_priority | true | 4.00 | 1 |  |
| h2-19 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-19 | B3_short_term | true | 4.00 | 1 |  |
| h2-20 | A_joint | false | 8.00 | 1 | potable-water:deadline missed |
| h2-20 | B1_task_priority | false | 8.00 | 1 |  |
| h2-20 | B2_safety_stock | false | 8.00 | 1 |  |
| h2-20 | B3_short_term | false | 8.00 | 1 |  |
| h2-21 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-21 | B1_task_priority | true | 4.00 | 1 |  |
| h2-21 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-21 | B3_short_term | true | 4.00 | 1 |  |
| h2-22 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-22 | B1_task_priority | true | 4.00 | 1 |  |
| h2-22 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-22 | B3_short_term | true | 4.00 | 1 |  |
| h2-23 | A_joint | false | 8.00 | 1 | potable-water:deadline missed |
| h2-23 | B1_task_priority | false | 8.00 | 1 |  |
| h2-23 | B2_safety_stock | false | 8.00 | 1 |  |
| h2-23 | B3_short_term | false | 8.00 | 1 |  |

## 读法与局限

- 子集为 **留出集（8 场景）**，本次为首次运行；消融只在开发集运行（PREREG §6）。
- 成功 = 供水目标在 4 h 预测失效前完成、车床目标达成且无未达成目标；所有方法使用相同扰动轨迹、预算与 SAP 校验路径。
- 规划器是确定性贪心（ADR-0006）；合成场景、未校准；样本小，只作方向性比较。
