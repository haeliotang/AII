# H2 开发集结果（自动生成）

本文件由 `poie-h2-joint-planning --dev --write <dir>` 生成；留出集未运行。预登记见 [PREREG.md](PREREG.md)。

| 方法 | 成功 | 场景数 | 成功率 | 平均完成 (h) | 平均动作数 | SAP 通过 |
| --- | --- | --- | --- | --- | --- | --- |
| A_joint | 14 | 16 | 0.875 | 3.938 | 1.0 | 16/16 |
| B1_task_priority | 14 | 16 | 0.875 | 3.938 | 1.0 | 16/16 |
| B2_safety_stock | 14 | 16 | 0.875 | 3.938 | 1.0 | 16/16 |
| B3_short_term | 14 | 16 | 0.875 | 3.938 | 1.0 | 16/16 |
| abl_no_replanning | 14 | 16 | 0.875 | 3.625 | 1.0 | 16/16 |
| abl_short_horizon | 4 | 16 | 0.250 | 6.000 | 0.0 | 16/16 |
| abl_no_deadline | 14 | 16 | 0.875 | 3.938 | 1.0 | 16/16 |
| abl_capability_blind | 14 | 16 | 0.875 | 3.938 | 1.0 | 16/16 |

## 每场景结果

| 场景 | 方法 | 成功 | 完成 (h) | 动作 | 未达成 |
| --- | --- | --- | --- | --- | --- |
| h2-00 | A_joint | true | 3.00 | 1 |  |
| h2-00 | B1_task_priority | true | 3.00 | 1 |  |
| h2-00 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-00 | B3_short_term | true | 3.00 | 1 |  |
| h2-00 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-00 | abl_short_horizon | true | 0.00 | 0 | potable-water:horizon exceeded |
| h2-00 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-00 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-01 | A_joint | true | 3.00 | 1 |  |
| h2-01 | B1_task_priority | true | 3.00 | 1 |  |
| h2-01 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-01 | B3_short_term | true | 3.00 | 1 |  |
| h2-01 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-01 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-01 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-01 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-02 | A_joint | true | 3.00 | 1 |  |
| h2-02 | B1_task_priority | true | 3.00 | 1 |  |
| h2-02 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-02 | B3_short_term | true | 3.00 | 1 |  |
| h2-02 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-02 | abl_short_horizon | true | 0.00 | 0 | potable-water:horizon exceeded |
| h2-02 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-02 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-03 | A_joint | true | 3.00 | 1 |  |
| h2-03 | B1_task_priority | true | 3.00 | 1 |  |
| h2-03 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-03 | B3_short_term | true | 3.00 | 1 |  |
| h2-03 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-03 | abl_short_horizon | true | 0.00 | 0 | potable-water:horizon exceeded |
| h2-03 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-03 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-04 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-04 | B1_task_priority | true | 4.00 | 1 |  |
| h2-04 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-04 | B3_short_term | true | 4.00 | 1 |  |
| h2-04 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-04 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-04 | abl_no_deadline | true | 4.00 | 1 |  |
| h2-04 | abl_capability_blind | true | 4.00 | 1 |  |
| h2-05 | A_joint | true | 3.00 | 1 |  |
| h2-05 | B1_task_priority | true | 3.00 | 1 |  |
| h2-05 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-05 | B3_short_term | true | 3.00 | 1 |  |
| h2-05 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-05 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-05 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-05 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-06 | A_joint | false | 8.00 | 1 | potable-water:deadline missed |
| h2-06 | B1_task_priority | false | 8.00 | 1 |  |
| h2-06 | B2_safety_stock | false | 8.00 | 1 |  |
| h2-06 | B3_short_term | false | 8.00 | 1 |  |
| h2-06 | abl_no_replanning | false | 8.00 | 1 |  |
| h2-06 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-06 | abl_no_deadline | false | 8.00 | 1 |  |
| h2-06 | abl_capability_blind | false | 8.00 | 1 |  |
| h2-07 | A_joint | true | 3.00 | 1 |  |
| h2-07 | B1_task_priority | true | 3.00 | 1 |  |
| h2-07 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-07 | B3_short_term | true | 3.00 | 1 |  |
| h2-07 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-07 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-07 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-07 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-08 | A_joint | true | 3.00 | 1 |  |
| h2-08 | B1_task_priority | true | 3.00 | 1 |  |
| h2-08 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-08 | B3_short_term | true | 3.00 | 1 |  |
| h2-08 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-08 | abl_short_horizon | true | 0.00 | 0 | potable-water:horizon exceeded |
| h2-08 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-08 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-09 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-09 | B1_task_priority | true | 4.00 | 1 |  |
| h2-09 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-09 | B3_short_term | true | 4.00 | 1 |  |
| h2-09 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-09 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-09 | abl_no_deadline | true | 4.00 | 1 |  |
| h2-09 | abl_capability_blind | true | 4.00 | 1 |  |
| h2-10 | A_joint | false | 8.00 | 1 | potable-water:deadline missed |
| h2-10 | B1_task_priority | false | 8.00 | 1 |  |
| h2-10 | B2_safety_stock | false | 8.00 | 1 |  |
| h2-10 | B3_short_term | false | 8.00 | 1 |  |
| h2-10 | abl_no_replanning | false | 8.00 | 1 |  |
| h2-10 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-10 | abl_no_deadline | false | 8.00 | 1 |  |
| h2-10 | abl_capability_blind | false | 8.00 | 1 |  |
| h2-11 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-11 | B1_task_priority | true | 4.00 | 1 |  |
| h2-11 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-11 | B3_short_term | true | 4.00 | 1 |  |
| h2-11 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-11 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-11 | abl_no_deadline | true | 4.00 | 1 |  |
| h2-11 | abl_capability_blind | true | 4.00 | 1 |  |
| h2-12 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-12 | B1_task_priority | true | 4.00 | 1 |  |
| h2-12 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-12 | B3_short_term | true | 4.00 | 1 |  |
| h2-12 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-12 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-12 | abl_no_deadline | true | 4.00 | 1 |  |
| h2-12 | abl_capability_blind | true | 4.00 | 1 |  |
| h2-13 | A_joint | true | 3.00 | 1 |  |
| h2-13 | B1_task_priority | true | 3.00 | 1 |  |
| h2-13 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-13 | B3_short_term | true | 3.00 | 1 |  |
| h2-13 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-13 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-13 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-13 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-14 | A_joint | true | 3.00 | 1 |  |
| h2-14 | B1_task_priority | true | 3.00 | 1 |  |
| h2-14 | B2_safety_stock | true | 3.00 | 1 |  |
| h2-14 | B3_short_term | true | 3.00 | 1 |  |
| h2-14 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-14 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-14 | abl_no_deadline | true | 3.00 | 1 |  |
| h2-14 | abl_capability_blind | true | 3.00 | 1 |  |
| h2-15 | A_joint | true | 4.00 | 1 | potable-water:deadline missed |
| h2-15 | B1_task_priority | true | 4.00 | 1 |  |
| h2-15 | B2_safety_stock | true | 4.00 | 1 |  |
| h2-15 | B3_short_term | true | 4.00 | 1 |  |
| h2-15 | abl_no_replanning | true | 3.00 | 1 |  |
| h2-15 | abl_short_horizon | false | 8.00 | 0 | potable-water:horizon exceeded |
| h2-15 | abl_no_deadline | true | 4.00 | 1 |  |
| h2-15 | abl_capability_blind | true | 4.00 | 1 |  |

## 读法与局限

- 子集为 **开发集（16 场景）**；留出集（8 场景）未运行、未查看。
- 成功 = 供水目标在 4 h 预测失效前完成、车床目标达成且无未达成目标；所有方法使用相同扰动轨迹、预算与 SAP 校验路径。
- 规划器是确定性贪心（ADR-0006）；合成场景、未校准；样本小，只作方向性比较。
