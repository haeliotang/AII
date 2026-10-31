# 阶段门槛证据包（2026-10-31）

当前一页式状态见 [status-snapshot](status-snapshot.zh-CN.md)；自主范围审计见 [autonomous-scope-audit](autonomous-scope-audit.zh-CN.md)。本文件汇总各阶段门槛的**已达成证据、复现方式与当前阻塞项**。所有数字来自本仓库提交，CI 逐项执行；`scripts/verify-all.sh` 可在本地一次重跑全部验证；干净克隆复现见 `scripts/check-fresh-clone.sh` 与 [fresh-clone-repro](fresh-clone-repro.zh-CN.md)。逐 crate/实验的测试与 CI 对应关系见 [测试与证据矩阵](test-matrix.zh-CN.md)。

## 0. 一键复现

```bash
scripts/verify-all.sh
```

CI 固定使用 Rust 1.94.0（与参考开发环境一致，避免 clippy 版本漂移；升级需单独 PR）。脚本依次运行：格式、clippy、全测试、**rustdoc（警告即错误）**、Markdown 相对链接检查、实验重放覆盖检查、测试矩阵计数校验、Base-01/Base-02 预登记用例、时间版用例与重放摘要、VEC 报告重放、H3 模拟重放、H4 报告重放、联邦分区模拟重放、本机 TCP 联邦演示、两站点影子集成、`cargo-deny`。

## 1. Foundation（M1–M5）

**状态：证据齐备，但阶段门槛被外部事项阻塞**（M1.8/M1.9/M1.10、M3.3）。

| 条件 | 证据 |
| --- | --- |
| 三份契约草案存在且可反驳 | [PO v0.3-draft](../../specifications/physical-ontology.zh-CN.md)、[SAP v0.2-draft](../../specifications/state-action-protocol.zh-CN.md)、[VEC v0.2-draft](../../specifications/viability-expansion.zh-CN.md) |
| 本体可执行 + 预登记用例 | Base-01 40/40（`scenarios/base-01/cases.toml`）；Base-02 8/8（**M12.5 新增**）；ontology 57 单元 + 16 集成 + 调度随机不变量（120 trials：前置满足、容量冲突完整性）+ 图求解随机不变量（120 trials：判定/阻断分类、候选步骤、验证等级与深度单调性）+ 模型/用例 TOML 随机变异不 panic + 单位换算/维度/解析随机不变量 + 候选路线枚举/外部依赖并集随机不变量 |
| 状态协议参考实现与故障注入 | `crates/poie-state`：53 单元 + 13 SAP 验收（T1–T11）；10⁵ 序列属性测试；逐写入点崩溃注入；120 次随机投影不变量测试（对照独立契约 oracle）；120 次记录验证/九类负载往返随机测试；追加日志随机不变量（重开保序/序号规则/崩溃尾部/中段损坏） |
| 时间维度与可重放 | `poie-sim` 13 测试 + 仿真随机不变量（80 trials：digest 确定、守恒分类、外部流/库存核算、SoC 边界）；Base-01 时间版 20/20，`digests.toml` CI 逐位校验 |
| H1 实验 | [H1 REPORT](../../experiments/h1-structural/REPORT.md)（弱支持、内部证据）与 [H1 temporal REPORT](../../experiments/h1-temporal/REPORT.md)，全量/开发集/留出集/时间版原始输出均 CI 重放 |
| M1.6–M1.7 | CI 全绿；DCO/CONTRIBUTING/SECURITY/issue 模板已合并 |
| M1.8/M1.9/M1.10 | **未完成**：外部评审、ADR-0001 由 Inceptless 确认、正式商标检索 |
| M3.3 | **未满足**：外部贡献用例配额（≥1/4） |

**结论**：实现与实验证据齐备；门槛未通过的唯一原因是外部事项。行动清单见 [external-actions](external-actions.zh-CN.md)。

## 2. Autonomy（M6–M8）

**状态：完成（M6–M8 均已完成）**；阶段门槛以 Foundation 通过为前提。

| 条件 | 证据 |
| --- | --- |
| VEC 参考实现 | [viability-expansion v0.2](../../specifications/viability-expansion.zh-CN.md)：查询/六类结论/抽样枚举/四组指标/报告；`poie-viability` 33 测试 + 随机不变量（60 条轨迹指标、汇总截尾、抽样/枚举方法可复现） |
| Base-01 v0.2 报告 | [VEC report](../../experiments/vec-base01/report.md)（20 runs / 78 metrics，CI 重放）；报告校验随机防作弊（合法 TOML 往返 + 16 类单字段变异全部拒绝） |
| 规划与 H2 | `poie-plan` 18 测试；[H2 REPORT](../../experiments/h2-joint-planning/REPORT.md)：**H2 不被支持（负结果）**，留出集 5/8 持平 |
| 联合规划经 SAP 校验 | 每个动作走 proposed→validated→authorized + 六项检查，不派发、不绕过授权；80 次随机化不变量测试（有序/不重叠/唯一 id/确定性/预算/所有计划可经 SAP 校验）；重规划/基线随机不变量（保留/取消划分、不早于 now、added 口径、确定性）；H2 开发集与留出集报告/原始输出 CI 重放（留出集仅复现，不用于开发） |
| 过程桥（M7） | 合成水模型 + JSON-line 桥 + Rust 客户端；Python 9 项测试 + Rust 客户端 40 次随机故障/超时测试纳入 CI；明确 `synthetic`，无标定 | 真实过程模型/标定（IDAES 候选） |

## 3. Embodiment（M9–M12）

**状态：库级与模拟部分完成；物理/真实环境部分未完成**。

| 工作包 | 已完成证据 | 阻塞 |
| --- | --- | --- |
| M9 适配器与影子运行 | `poie-runtime` 12 测试 + 80 次随机运行时/断连不变量测试；[影子运行端到端演示](../../experiments/shadow-run/REPORT.md)（投影→影子规划→不派发→操作者日志对比，7/7 断言） | M9.0 合规复核、M9.2 真实适配器、M9.6 物理试验台 |
| M10 有界执行 | `poie-exec` 12 测试 + 120 次随机执行门安全不变量测试；[H3 模拟报告](../../experiments/h3-bounded-execution/REPORT.sim.md) 14/14 | 物理试验台、真实故障、安全演练、执行端幂等验证 |
| M11 扩张核算 | `poie-expansion` 7 测试 + 随机不变量（40 trials 结构增益/损失划分与 expand/to_patch 一致、40 trials 时序增益与施工窗口检查）+ 提案 TOML 随机变异不 panic；[H4 报告](../../experiments/h4-expansion/REPORT.md) 判定不确定 | M11.6 物理实施（`demonstrated_gain`） |
| M12 多站点联邦 | `poie-federation` 7 测试 + HLC 随机不变量（80 trials）+ 120 次随机分区性质测试；[M12.6 模拟报告](../../experiments/federation-partition/REPORT.sim.md) 6/6；Base-02 8/8；[本机 TCP 双进程交换](../../experiments/federation-tcp/REPORT.md)（真实 socket，仍非真实多站点） | M12.6 真实多节点网络与长时间分区 |
| 端到端管线 | [e2e REPORT](../../experiments/e2e-pipeline/REPORT.md)：适配器→投影→规划→SAP 校验→执行门→独立验证→预测核对，9/9 断言通过，CI 重放 | 真实适配器与物理执行（M9.2/M10） |
| 两站点影子集成 | [two-site REPORT](../../experiments/two-site-shadow/REPORT.md)：站点 A 投影/规划，站点 B 共享库存/租约；超额预留与重叠租约阻断派发、干净对账放行，7/7 断言，CI 重放 | 真实多站点与网络（M9.2/M12.6） |

**结论**：不依赖物理/外部条件的实现与实验已完成并纳入 CI；上述四行阻塞项需要设备、场地、安全评审或真实网络环境。

## 4. 当前版本与链接

- 当前版本：`0.1.x`（见 [DEVELOPMENT 修订记录](../DEVELOPMENT.zh-CN.md#revision-history)）；`main` 每次合并后 CI 全绿。
- 外部行动：见 [external-actions.zh-CN.md](external-actions.zh-CN.md)；外部用例贡献模板见 [case-contribution-template.zh-CN.md](case-contribution-template.zh-CN.md)。
