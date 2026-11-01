# 自主范围完成审计（2026-10-31）

本文件记录一次**自主范围审计**：目标的限定是"先做不需要发起方/外部参与的部分"。审计结论是：**所有不依赖外部参与的里程碑工作已完成、已复现、并由 CI 持续校验；剩余里程碑项全部需要外部参与。**

审计依据：`docs/DEVELOPMENT.zh-CN.md` 的里程碑表、[阶段门槛证据](stage-gates.zh-CN.md)、[测试与证据矩阵](test-matrix.zh-CN.md)、CI 与干净克隆复现。

## 1. 审计方法

1. 逐条检查 DEVELOPMENT 中 M1–M12 的每个里程碑项，判断其完成是否需要以下任一外部条件：外部读者评审、发起方书面确认、法务/商标/合规、外部贡献用例、真实适配器、物理试验台、真实多节点网络。
2. 对"不需要外部条件"的项，核对：实现/文档是否存在、是否有预登记证据、是否有 CI 重放或测试、是否被检查器覆盖。
3. 对"需要外部条件"的项，记录所需条件、现有替代证据与交接材料。

## 2. 已完成（不需要外部参与）

| 范围 | 完成情况 | 证据 |
| --- | --- | --- |
| M1.1–M1.7 | 三份契约草案、能力图原型、Base-01、CI、贡献指南 | `specifications/`、`crates/poie-ontology`、`scenarios/base-01`、`.github/workflows/ci.yml`、`CONTRIBUTING.md` |
| M1.8/M1.9/M1.10 的**准备材料**（非确认本身） | 自审问题包、评审模板、邀约文本、ADR 草案、商标初步检索 | [contract-self-audit](contract-self-audit.zh-CN.md)、[outreach-templates](outreach-templates.zh-CN.md)、ADR-0001/0002 |
| M2.1–M2.8 | OR 支撑、多来源、降级、合格率、能力等级、多路线、外部依赖、S11–S24 | `crates/poie-ontology`、`scenarios/base-01/cases.toml` |
| M3.1–M3.2、M3.4–M3.5 | 预登记、逐设备基线、留出集、H1 报告 | `experiments/h1-structural/` |
| M3.3 内部部分 | 40 例、分层标签、加载器校验（外部配额除外） | `scenarios/base-01/cases.toml` |
| M4.1–M4.9 | 记录/日志/投影/状态机/复核/执行端/恢复/隔离/本体桥 | `crates/poie-state`、ADR-0003 |
| M5.1–M5.7 | 时间线、调度、能量、仿真、重放、时间版场景、H1 时间版 | `crates/poie-sim`、`scenarios/base-01-timed/`、`experiments/h1-temporal/` |
| M6.1–M6.6 | VEC 查询/方法/指标/报告/桥接/集合方法 ADR | `crates/poie-viability`、ADR-0004 |
| M7.1–M7.5 | 进程桥、水模型、验证、失败处理、校准数据计划 | `crates/poie-process`、`python/poie_models/`、ADR-0005 |
| M8.1–M8.6 | 规划器、重规划、基线、H2 预登记/消融/负结果报告 | `crates/poie-plan`、`experiments/h2-joint-planning/`、ADR-0006 |
| M9.1、M9.3–M9.5 | 适配器契约、本地运行时、影子运行、预测核对（库级/模拟） | `crates/poie-runtime`、`experiments/shadow-run/` |
| M10.1–M10.3、M10.5 | 可写适配器契约、授权/接管、独立验证、安全降级（库级） | `crates/poie-exec`、ADR-0007 |
| M10.4 模拟部分 | H3 14/14 模拟场景 | `experiments/h3-bounded-execution/` |
| M11.1–M11.5 | 提案、结构/时序增益、Base-01 扩张、H4 模拟与不确定判定 | `crates/poie-expansion`、`experiments/h4-expansion/` |
| M12.1–M12.5、M12.6 模拟部分 | HLC/租约/预留/对账、Base-02、6/6 分区模拟、本机 TCP | `crates/poie-federation`、`scenarios/base-02/`、`experiments/federation-partition/`、`experiments/federation-tcp/` |
| 证据与验证基础设施 | 单元/性质测试、场景/实验重放、链接/矩阵/重放覆盖检查、`verify-all`、干净克隆、scheduled workflow、维护手册 | `scripts/`、`.github/workflows/`、[test-matrix](test-matrix.zh-CN.md)、[MAINTENANCE](../MAINTENANCE.zh-CN.md) |

## 3. 剩余项：全部需要外部参与

| 事项 | 需要的条件 | 现有替代证据/交接材料 |
| --- | --- | --- |
| M1.8 契约外部评审 | 2–3 位领域外读者 | [自审问题包](contract-self-audit.zh-CN.md)、[评审模板](../../.github/ISSUE_TEMPLATE/contract-review.yml)、[邀约文本](outreach-templates.zh-CN.md) |
| M1.9 ADR-0001 确认 | 发起方书面确认 | ADR-0001、[邀约文本](outreach-templates.zh-CN.md) |
| M1.10 商标检索 | 知识产权代理/法务 | ADR-0002 §7、[邀约文本](outreach-templates.zh-CN.md) |
| M3.3 外部用例配额 | 独立贡献者 | [用例模板](case-contribution-template.zh-CN.md) |
| M9.0 合规复核 | 发起方法务/合规 | [外部行动清单](external-actions.zh-CN.md) |
| M9.2 真实适配器 | Ditto/BaSyx/ROS 2 测试实例 | 库级适配器契约、[外部行动清单](external-actions.zh-CN.md) |
| M9.6/M10 物理部分 | 试验台、安全评审、操作人员 | H3 模拟报告、[外部行动清单](external-actions.zh-CN.md) |
| M11.6 物理实施 | 建造/检验资源 | H4 模拟报告（`modeled_gain`） |
| M12.6 真实多节点 | 两台可隔离节点与网络控制 | 6/6 模拟、本机 TCP 演示、ADR-0008 |

## 4. 审计时的验证状态

- `main` = `4caddb6`（审计时），合并后 CI 为 `success`；
- `scripts/verify-all.sh` → `ALL GATES PASSED`；
- 干净克隆复现最近在 `aa62a83` 上通过；
- 链接、测试矩阵（含状态快照计数）、实验重放覆盖三项检查器通过。

## 5. 结论

**自主范围已完成。** 继续开发剩余里程碑需要外部输入；在此之前，仓库中的实现、实验、证据与验证处于可持续维护状态。外部事项请按 [status-snapshot §4/§5](status-snapshot.zh-CN.md) 与 [external-actions](external-actions.zh-CN.md) 推进。

## 6. 外部依赖项的 AI 预演（2026-11-01）

下面这些产出由 AI 子代理或主线程完成，用来给真实的外部参与者准备起点。**它们都不计入对应里程碑的退出条件**，第 3 节的剩余项和第 5 节的结论保持不变。任务进度见 [ai-preview-tasks](../progress/ai-preview-tasks.md)。

| 事项 | 预演做法 | 结果 | 产物 |
| --- | --- | --- | --- |
| M1.8 契约评审 | 3 个互不共享上下文的子代理，分别从工业自动化、分布式系统、运筹与统计视角找反例，都没有读作者的自审 | 36 条，去重后 25 项；其中 13 项为高严重度。P4（守恒可被绕开）和 P5（仿真不查负库存）主线程已核对代码，确认成立 | [评审汇总](ai-preview/contract-review-summary.zh-CN.md) |
| M3.3 外部用例 | 子代理不读已有用例，独立编写 10 例；预期先提交（815c2f5），之后才运行 | 10/10 符合预期 | `scenarios/base-01/ai-preview-cases.toml`、[运行输出](ai-preview/base01-ai-cases-run.txt) |
| M1.10 商标检索 | 补查普通法使用、包名和域名，并核实 USPTO 申请 50154930 | 没有发现冲突；50154930 确有其申请（POIE，cl. 9，表情和图像文件）。仍需法务检索 | [补充检索](ai-preview/trademark-supplement.zh-CN.md) |
| M9.2 真实适配器 | 在本机容器跑 BaSyx AAS `2.0.0-milestone-15`，接入只读适配器，测试断连和重连 | 9/9 PASS。影子计划中动作数为 0，所以「不派发」只是自然满足；预登记没有先于运行单独提交。Ditto 和 ROS 2 未测 | [basyx-shadow](../../experiments/basyx-shadow/REPORT.md) |
| M12.6 真实多节点 | 两个 Docker 容器，用 `docker network disconnect` 制造分区；预登记先提交（815c2f5） | D0–D4 共 14/14 PASS。未覆盖：时钟偏差、非对称分区、崩溃重启、认证加密 | [federation-docker](../../experiments/federation-docker/REPORT.md) |

没有做预演的事项及原因：M1.9 需要发起方本人确认；M9.0 需要真实的法务或合规意见；M9.6/M10 的物理部分和 M11.6 需要实物试验台和建造资源。
