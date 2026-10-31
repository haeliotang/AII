# 当前状态快照（2026-10-31 UTC）

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
| Rust 列表测试 | **290**（库 crate 284 + 实验 runner 6） |
| Python 桥测试 | 9（CI 执行） |
| 随机性质测试 | 图求解 120、记录 120、投影 120、调度 120、仿真 80、VEC 60 轨迹 + 报告 20×16 变异、规划器 80、重规划/基线 80+20+60、HLC 80、联邦分区 120、执行门 120、运行时 80、扩张 40+40、桥 40 trials |
| 预登记场景用例 | Base-01 40、Base-02 8、时间版 20 |
| 生成型实验产物 | **26** 个全部在 CI 中重放（另有 4 份手工分析报告显式白名单） |
| 检查器 | 链接、测试矩阵计数、实验重放覆盖（均 CI 执行） |

## 3. 复现与验证运行

- 每次合并：CI（fmt/clippy/test/rustdoc/Python/链接/矩阵/重放覆盖/全部场景与实验/`cargo-deny`）全绿后合并，并确认 `main` CI run 成功。
- 本地一键：`scripts/verify-all.sh`。
- 干净克隆：`scripts/check-fresh-clone.sh` —— 最近在 `aa62a83` 上通过（约 1 分钟）。
- GitHub runner：手动 dispatch scheduled workflow 的 [run 41246880552](https://github.com/haeliotang/poieweave/actions/runs/41246880552) 五个 job 全绿（verify-all、release tests、fresh-clone、advisories、licenses/bans/sources）。
- 定期：每周一 04:17 UTC scheduled workflow（`verify-all` + `cargo test --release` + `cargo-deny`；手动触发时额外 fresh-clone）。

## 4. 外部阻塞项（需要发起方/外部参与）

| 优先级 | 事项 | 材料/下一步 |
| --- | --- | --- |
| 1 | **M1.8 三契约外部评审** | [自审问题包](contract-self-audit.zh-CN.md)、[contract-review 模板](../../.github/ISSUE_TEMPLATE/contract-review.yml)、[邀约文本](outreach-templates.zh-CN.md#a-m18-三份契约外部评审) |
| 1 | **M1.9 ADR-0001 由 Inceptless 确认** | [ADR-0001](../adr/0001-open-source-boundary.zh-CN.md)、[邀约文本](outreach-templates.zh-CN.md#b-m19-adr-0001-开源边界确认inceptless) |
| 2 | **M3.3 外部贡献用例 ≥1/4** | [用例模板](case-contribution-template.zh-CN.md) |
| 3 | **M1.10 正式商标检索** | [ADR-0002 §7](../adr/0002-rename-to-poieweave.zh-CN.md) |
| 4 | M9.0 合规复核；M9.2 真实适配器；M9.6/M10 试验台与安全演练；M11.6 物理实施；M12.6 真实多节点 | 见 [external-actions](external-actions.zh-CN.md) 与 [邀约文本](outreach-templates.zh-CN.md) |

## 5. 建议的下一步（按解锁价值）

1. 指定 M1.8 评审者（2–3 位）并发起审读；同日请发起方确认 ADR-0001（M1.9）；
2. 邀请独立贡献者提交 ≥10 个预登记用例（M3.3）；
3. 并行推进商标检索（M1.10）；
4. 按可获得的设备/网络安排 M9.2/M9.6/M10/M11.6/M12.6。

> 本快照不替代各证据文档；如与提交内容不一致，以提交与 CI 为准。更新方式：在每个里程碑合并或外部事项状态变化后刷新本页，并同步 DEVELOPMENT 修订记录。
