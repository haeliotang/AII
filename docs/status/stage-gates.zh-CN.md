# 阶段门槛证据包（2026-10-31）

本文件汇总各阶段门槛的**已达成证据、复现方式与当前阻塞项**。所有数字来自本仓库提交，CI 逐项执行；`scripts/verify-all.sh` 可在本地一次重跑全部验证。

## 0. 一键复现

```bash
scripts/verify-all.sh
```

脚本依次运行：格式、clippy、全测试、Base-01/Base-02 预登记用例、时间版用例与重放摘要、VEC 报告重放、H3 模拟重放、H4 报告重放、联邦分区模拟重放、`cargo-deny`。

## 1. Foundation（M1–M5）

**状态：证据齐备，但阶段门槛被外部事项阻塞**（M1.8/M1.9/M1.10、M3.3）。

| 条件 | 证据 |
| --- | --- |
| 三份契约草案存在且可反驳 | [PO v0.3-draft](../../specifications/physical-ontology.zh-CN.md)、[SAP v0.2-draft](../../specifications/state-action-protocol.zh-CN.md)、[VEC v0.2-draft](../../specifications/viability-expansion.zh-CN.md) |
| 本体可执行 + 预登记用例 | Base-01 40/40（`scenarios/base-01/cases.toml`）；Base-02 8/8（**M12.5 新增**）；ontology 57 单元 + 16 集成 |
| 状态协议参考实现与故障注入 | `crates/poie-state`：53 单元 + 13 SAP 验收（T1–T11）；10⁵ 序列属性测试；逐写入点崩溃注入 |
| 时间维度与可重放 | `poie-sim` 13 测试；Base-01 时间版 20/20，`digests.toml` CI 逐位校验 |
| H1 实验 | [H1 REPORT](../h1-structural/REPORT.md)（弱支持、内部证据）；[H1 temporal REPORT](../h1-temporal/REPORT.md) |
| M1.6–M1.7 | CI 全绿；DCO/CONTRIBUTING/SECURITY/issue 模板已合并 |
| M1.8/M1.9/M1.10 | **未完成**：外部评审、ADR-0001 由 Inceptless 确认、正式商标检索 |
| M3.3 | **未满足**：外部贡献用例配额（≥1/4） |

**结论**：实现与实验证据齐备；门槛未通过的唯一原因是外部事项。行动清单见 [external-actions](external-actions.zh-CN.md)。

## 2. Autonomy（M6–M8）

**状态：完成（M6–M8 均已完成）**；阶段门槛以 Foundation 通过为前提。

| 条件 | 证据 |
| --- | --- |
| VEC 参考实现 | [viability-expansion v0.2](../../specifications/viability-expansion.zh-CN.md)：查询/六类结论/抽样枚举/四组指标/报告；`poie-viability` 33 测试 |
| Base-01 v0.2 报告 | [VEC report](../vec-base01/report.md)（20 runs / 78 metrics，CI 重放） |
| 规划与 H2 | `poie-plan` 18 测试；[H2 REPORT](../h2-joint-planning/REPORT.md)：**H2 不被支持（负结果）**，留出集 5/8 持平 |
| 联合规划经 SAP 校验 | 每个动作走 proposed→validated→authorized + 六项检查，不派发、不绕过授权 |

## 3. Embodiment（M9–M12）

**状态：库级与模拟部分完成；物理/真实环境部分未完成**。

| 工作包 | 已完成证据 | 阻塞 |
| --- | --- | --- |
| M9 适配器与影子运行 | `poie-runtime` 12 测试（只读契约、离线运行时、影子规划、预测核对） | M9.0 合规复核、M9.2 真实适配器、M9.6 物理试验台 |
| M10 有界执行 | `poie-exec` 12 测试；[H3 模拟报告](../h3-bounded-execution/REPORT.sim.md) 14/14 | 物理试验台、真实故障、安全演练、执行端幂等验证 |
| M11 扩张核算 | `poie-expansion` 7 测试；[H4 报告](../h4-expansion/REPORT.md) 判定不确定 | M11.6 物理实施（`demonstrated_gain`） |
| M12 多站点联邦 | `poie-federation` 7 测试；[M12.6 模拟报告](../federation-partition/REPORT.sim.md) 6/6；Base-02 8/8；[本机 TCP 双进程交换](../federation-tcp/REPORT.md)（真实 socket，仍非真实多站点） | M12.6 真实多节点网络与长时间分区 |
| 端到端管线 | [e2e REPORT](../e2e-pipeline/REPORT.md)：适配器→投影→规划→SAP 校验→执行门→独立验证→预测核对，9/9 断言通过，CI 重放 | 真实适配器与物理执行（M9.2/M10） |

**结论**：不依赖物理/外部条件的实现与实验已完成并纳入 CI；上述四行阻塞项需要设备、场地、安全评审或真实网络环境。

## 4. 当前版本与链接

- 当前版本：`0.1.x`（见 [DEVELOPMENT 修订记录](../DEVELOPMENT.zh-CN.md#revision-history)）；`main` 每次合并后 CI 全绿。
- 外部行动：见 [external-actions.zh-CN.md](external-actions.zh-CN.md)；外部用例贡献模板见 [case-contribution-template.zh-CN.md](case-contribution-template.zh-CN.md)。
