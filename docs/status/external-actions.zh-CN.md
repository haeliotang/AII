# 外部行动清单（需要发起方/评审者/现场参与）

本清单列出**只有外部参与才能推进**的事项、所需材料与验收方式。仓库内证据见 [stage-gates](stage-gates.zh-CN.md)。

## A. M1.8 三份契约的外部评审（最高优先级）

- **需要谁**：2–3 位领域外读者（工业自动化、过程安全、分布式系统/形式化方法背景各一为佳）。
- **审读材料**：[Physical Ontology v0.3-draft](../../specifications/physical-ontology.zh-CN.md)、[State & Action Protocol v0.2-draft](../../specifications/state-action-protocol.zh-CN.md)、[Viability & Expansion Contract v0.2-draft](../../specifications/viability-expansion.zh-CN.md)。
- **问题包**：[契约自审与外部评审问题包](contract-self-audit.zh-CN.md)（逐条开放问题、反例方向、建议评审顺序）；评论模板 [contract-review](../../.github/ISSUE_TEMPLATE/contract-review.yml)。
- **审读方式**：按"能否构造反例"审读，而非文风；每条意见登记为 GitHub issue（模板 `contract-review`）。
- **验收**：评审意见全部登记，并逐条标注**接受 / 拒绝（附理由）/ 推迟**（DEVELOPMENT M1.8）。
- **请求**：指定评审者与期望完成日期；仓库维护者负责响应与修订。

## B. M1.9 ADR-0001 开源边界确认

- **需要谁**：Inceptless（发起方）书面确认。
- **材料**：[ADR-0001](../adr/0001-open-source-boundary.zh-CN.md)、`NOTICE`、DCO、`SECURITY.md`、`cargo-deny` 配置。
- **确认内容**：六条不变式与"开放核心 + 可闭源适配器/客户模型/托管服务"的边界。
- **验收**：ADR 状态改为"已接受"，记录确认人与日期。
- **请求**：由发起方指定确认人；如需法务复核，请提供意见渠道。

## C. M1.10 正式商标检索

- **需要谁**：知识产权代理或发起方法务；预算与类别决策。
- **现状**：[ADR-0002 §7](../adr/0002-rename-to-poieweave.zh-CN.md) 记录初步检索（TMview/Trademarkia 未见完全同名，POIE 词根有残余风险；官方库直连被阻断）。
- **建议检索**：类别 7（机械/泵）、9（软件/控制）、42（工程/技术服务）；辖区 US/EU/CN/WIPO。
- **验收**：USPTO/EUIPO/CNIPA/WIPO 结果记入 ADR-0002，并给出是否申请及类别建议。

## D. M3.3 外部贡献用例（配额 ≥ 1/4）

- **需要谁**：至少 1–2 位独立贡献者（非项目作者）。
- **材料**：模板 [case-contribution-template.zh-CN.md](case-contribution-template.zh-CN.md)；现有 40 例见 [Base-01 cases](../../scenarios/base-01/cases.toml)。
- **要求**：用例在**运行前**写入预期；覆盖扰动分层标签；通过加载器与 CI；贡献者需在 PR 中声明独立性。
- **验收**：≥10 个外部用例（40 的 1/4），全部有预登记预期并通过 CI；`docs/progress/foundation-tasks.md` 更新配额。

## E. M9.0 合规与出口管制复核

- **需要谁**：发起方法务/合规。
- **材料**：适配器归属判定（ADR-0001 §4.3）、各适配器数据来源与部署位置。
- **验收**：每个适配器登记"可开源 / 客户侧 / 需许可"结论；限制条款写入 ADR/README。

## F. M9.2 真实适配器（Eclipse Ditto / BaSyx AAS / ROS 2）

- **需要谁**：能提供测试实例与网络访问的环境。
- **需要的信息**：版本、端点、认证方式、只读凭据、时间戳语义、断连行为、是否支持命令幂等（写入 M10 使用前必须）。
- **验收**：每个适配器有兼容性测试记录（ADR），只读影子运行可跑通。

## G. M9.6 / M10 物理试验台与安全演练

- **需要谁**：场地、设备与独立安全评审。
- **最低要求**：小型水泵回路 + 可编程执行器；**急停与保护独立于 Poieweave**；操作者接管演练；断电/断网/重启演练记录。
- **验收**：M10 退出条件（物理 H3 报告、无重复物理动作或已登记修复、接管/降级演练记录）。

## H. M11.6 扩张的物理实施

- **需要谁**：可建造/改造的试验设施与检验资源。
- **建议最小项**：切屑回收熔炉原型或等效的小型回收单元；按 VEC §6.4 升级到 `demonstrated_gain`。
- **验收**：建造、检验、运行记录与增益声明；`modeled_gain` 之外的证据。

## I. M12.6 真实多节点联邦测试

- **需要谁**：两台可隔离网络的节点（物理或 VM）与时钟/分区控制能力。
- **测试**：SAP-T5–T8 复测、长时间分区、反复分区、重连对账、无双重控制/超额预留。
- **验收**：[ADR-0008](../adr/0008-federation-sync.zh-CN.md) 的生产传输验收要求全部满足。

## 优先级建议

1. M1.8 外部评审（解锁 Foundation 门槛）与 M1.9 确认（同日可完成）；
2. M3.3 外部用例（解锁 Foundation 门槛）；
3. M1.10 商标（可与 1–2 并行）；
4. M9.2/M9.6/M10/M11.6/M12.6（按可获得的设备与场地排序）。

> 本文件不包含真实联系人信息；请发起方指定负责人后，在 GitHub issue 中跟踪每项行动。
