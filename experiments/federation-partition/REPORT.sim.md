# M12.6 分区与重连实验：模拟报告（simulated, not multi-node）

预登记：[PREREG.md](PREREG.md)。**本报告来自进程内模拟，不覆盖真实网络/认证/时钟漂移；真实多节点复测仍待外部条件。**

| 场景 | 期望 | 实际冲突 | 合并记录 | 预留 | 租约 | 判定 |
| --- | --- | --- | --- | --- | --- | --- |
| P0 | no conflicts, ordered merge | （无） | 2 | 0 | 0 | PASS |
| P1 | over_reservation, both retained | over_reservation | 0 | 2 | 0 | PASS |
| P2 | double_ownership, leases retained | double_ownership | 0 | 0 | 2 | PASS |
| P3 | duplicate_idempotency_key | duplicate_idempotency_key | 2 | 0 | 0 | PASS |
| P4 | double_dispatch | double_dispatch | 2 | 0 | 0 | PASS |
| P5 | no duplicate records, stable conflicts | （无） | 2 | 0 | 0 | PASS |

## 汇总

- 场景：6；未通过：0
- 冲突均被报告（无静默双重控制/超额预留）；预留与租约未被丢弃
- 结论：模拟分区/重连部分被支持
