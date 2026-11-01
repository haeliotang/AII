# 当前状态快照（2026-11-01 UTC）

本页是给发起方/外部评审者的一页式状态；测试计数随代码更新（由 `check-test-matrix.py` 校验），最近一次干净克隆验证提交见 §3。逐项证据与索引见 [stage-gates](stage-gates.zh-CN.md)、[test-matrix](test-matrix.zh-CN.md) 与 [external-actions](external-actions.zh-CN.md)。

## 1. 里程碑

| 阶段 | 里程碑 | 状态 |
| --- | --- | --- |
| Foundation | M0–M5 | **完成** |
| Autonomy | M6–M8 | **完成**（H2 为负结果，已如实报告） |
| Embodiment | M9–M10 | 库级/模拟完成；**物理试验台、真实故障与安全演练待外部** |
| Expansion | M11 | 库级与 H4 模拟完成（判定不确定）；**物理实施待外部** |
| Federation | M12 | 库级/模拟 + 本机 TCP 完成；**真实多节点待外部** |

阶段门槛：Foundation 与后续门槛仍被 M1.8/M1.9/M1.10 与 M3.3 外部事项阻塞，因此**不宣称通过阶段门槛**（证据见 [stage-gates](stage-gates.zh-CN.md)）。

## 2. 测试与证据

| 项 | 当前值 |
| --- | --- |
| Rust 列表测试 | **299**（库 crate 284 + 其他包 15） |
| Python 桥测试 | 9（CI 执行） |
| 随机性质测试 | 图求解 120、记录 120、投影 120、调度 120、仿真 80、VEC 60 轨迹 + 报告 20×16 变异、规划器 80、重规划/基线 80+20+60、HLC 80、联邦分区 120、执行门 120、运行时 80、扩张 40+40、桥 40 trials |
| 预登记场景用例 | Base-01 40、Base-02 8、时间版 20 |
| 生成型实验产物 | **28** 个：26 个在 CI 中重放，live 容器预演另按规则豁免/从 raw 判定（见 §2.1） |
| 检查器 | 链接、测试矩阵计数、实验重放覆盖（均 CI 执行） |

### 2.1 外部事项 AI 预演（2026-11-01，不计入里程碑）

由 AI 子代理或主线程为真实外部参与者准备的起点；**对应里程碑的退出条件不变**。逐项产物与限制见 [autonomous-scope-audit §6](autonomous-scope-audit.zh-CN.md)、[ai-preview-tasks](../progress/ai-preview-tasks.md)。

| 事项 | 预演结果 | 产物 |
| --- | --- | --- |
| M1.8 契约评审 | 3 个互不共享上下文的评审视角，36 条、去重 25 项，其中 13 项为高严重度 | [评审汇总](ai-preview/contract-review-summary.zh-CN.md) |
| M3.3 外部用例 | 子代理独立编写 10 例，预登记后运行 10/10 通过 | [ai-preview-cases](../../scenarios/base-01/ai-preview-cases.toml)、[运行输出](ai-preview/base01-ai-cases-run.txt) |
| M1.10 商标检索 | 普通法、包名、域名补充检索；USPTO 50154930 已核实为真实申请 | [补充检索](ai-preview/trademark-supplement.zh-CN.md) |
| M9.2 适配器 | BaSyx AAS 本机容器只读适配器 9/9（单容器、回环、无认证；影子计划动作数为 0） | [basyx-shadow](../../experiments/basyx-shadow/REPORT.md) |
| M12.6 多节点 | 两个 Docker 容器 + 网络层分区，D0–D4 共 14/14（共享内核与时钟，非真实多节点） | [federation-docker](../../experiments/federation-docker/REPORT.md) |

> **待裁定**：13 项高严重度评审问题尚未处置。其中 P4（质量守恒可被绕开）与 P5（仿真不查负库存）已由主线程核对代码确认成立；是否触发研究纲领 §11 的"守恒暂停"由维护者决定。AI 预演不替代 M1.8 的真人评审。

## 3. 复现与验证运行

- 每次合并：CI（fmt/clippy/test/rustdoc/Python/链接/矩阵/重放覆盖/全部场景与实验/`cargo-deny`）全绿后合并，并确认 `main` CI run 成功。
- 本地一键：`scripts/verify-all.sh`。
- 干净克隆：`scripts/check-fresh-clone.sh` —— 最近在 `9a31a15` 上通过（约 1 分钟）。
- GitHub runner：手动 dispatch scheduled workflow 的 [run 41246880552](https://github.com/haeliotang/poieweave/actions/runs/41246880552) 五个 job 全绿（verify-all、release tests、fresh-clone、advisories、licenses/bans/sources）。
- 定期：每周一 04:17 UTC scheduled workflow（`verify-all` + `cargo test --release` + `cargo-deny`；手动触发时额外 fresh-clone）。

## 4. 外部阻塞项（需要发起方/外部参与）

| 优先级 | 事项 | 材料/下一步 |
| --- | --- | --- |
| 1 | **M1.8 三契约外部评审** | [自审问题包](contract-self-audit.zh-CN.md)、[contract-review 模板](../../.github/ISSUE_TEMPLATE/contract-review.yml)、[邀约文本](outreach-templates.zh-CN.md#a-m18-三份契约外部评审)；[AI 三方评审汇总](ai-preview/contract-review-summary.zh-CN.md) 可作对照起点 |
| 1 | **M1.9 ADR-0001 由 Inceptless 确认** | [ADR-0001](../adr/0001-open-source-boundary.zh-CN.md)、[邀约文本](outreach-templates.zh-CN.md#b-m19-adr-0001-开源边界确认inceptless) |
| 2 | **M3.3 外部贡献用例 ≥1/4** | [用例模板](case-contribution-template.zh-CN.md)；[AI 独立用例 10 例](../../scenarios/base-01/ai-preview-cases.toml) 仅供参考、不占配额 |
| 3 | **M1.10 正式商标检索** | [ADR-0002 §7](../adr/0002-rename-to-poieweave.zh-CN.md)；[AI 补充检索](ai-preview/trademark-supplement.zh-CN.md) |
| 4 | M9.0 合规复核；M9.2 真实适配器；M9.6/M10 试验台与安全演练；M11.6 物理实施；M12.6 真实多节点 | 见 [external-actions](external-actions.zh-CN.md) 与 [邀约文本](outreach-templates.zh-CN.md)；已有 [BaSyx 预演](../../experiments/basyx-shadow/REPORT.md) 与 [双容器预演](../../experiments/federation-docker/REPORT.md) 作为接入起点 |

完整审计见 [autonomous-scope-audit](autonomous-scope-audit.zh-CN.md)：所有不需要外部参与的里程碑工作已完成；剩余项全部需要外部参与。

## 5. 建议的下一步（按解锁价值）

0. **先裁定 AI 预演发现的 13 项高严重度问题**（P4/P5 已确认），决定是否触发守恒暂停；再邀请真人评审；
1. 指定 M1.8 评审者（2–3 位）并发起审读；同日请发起方确认 ADR-0001（M1.9）；
2. 邀请独立贡献者提交 ≥10 个预登记用例（M3.3）；
3. 并行推进商标检索（M1.10）；
4. 按可获得的设备/网络安排 M9.2/M9.6/M10/M11.6/M12.6。

> 本快照不替代各证据文档；如与提交内容不一致，以提交与 CI 为准。更新方式：在每个里程碑合并或外部事项状态变化后刷新本页，并同步 DEVELOPMENT 修订记录。
