# 外部邀约模板（可直接发送）

本文件为 [外部行动清单](external-actions.zh-CN.md) 中每一项提供**可直接复制的邀约文本**，减少外部参与启动成本。发送前请替换 `[方括号]` 中的占位符；涉及预算、合同、联系人的内容由发起方决定，本仓库不代发。

使用建议：

1. 先读对应条目在 [external-actions](external-actions.zh-CN.md) 中的"需要谁/验收"；
2. 复制下面正文，替换占位符，附上所列链接；
3. 在 GitHub issue 中跟踪（可引用本文件条目），维护者逐条处置。

---

## A. M1.8 三份契约外部评审

**发送对象**：2–3 位领域外读者（工业自动化 / 过程安全 / 分布式系统或形式化方法背景）。

**正文（中文）**：

> 你好 [姓名]，
>
> 我们在做一个开放源码的工业能力图项目 Poieweave（Apache-2.0）。目前有三份草案契约，想请你按"**能否构造反例**"而不是文风来审读，帮我们在进入下一阶段前找出不可实现或不可判定的地方：
>
> - Physical Ontology v0.3-draft：<https://github.com/haeliotang/poieweave/blob/main/specifications/physical-ontology.zh-CN.md>
> - State and Action Protocol v0.2-draft：<https://github.com/haeliotang/poieweave/blob/main/specifications/state-action-protocol.zh-CN.md>
> - Viability and Expansion Contract v0.2-draft：<https://github.com/haeliotang/poieweave/blob/main/specifications/viability-expansion.zh-CN.md>
>
> 我们已经把已知的开放问题和"可构造反例的方向"整理成问题包，你可以直接从那里挑问题，也可以提出它没覆盖的问题：
> <https://github.com/haeliotang/poieweave/blob/main/docs/status/contract-self-audit.zh-CN.md>
>
> 每条意见请用这个模板登记（或直接回复我）：<https://github.com/haeliotang/poieweave/blob/main/.github/ISSUE_TEMPLATE/contract-review.yml>
>
> 预计需要 [2–3 小时]，范围可以只挑一份契约。我们会对每条意见给出"接受 / 拒绝（附理由）/ 推迟（附重开条件）"的处置。
>
> 时间上希望 [日期] 前收到初版意见；如不方便，告诉我可投入的时间和最关心的部分即可。
>
> 谢谢！
> [发起方/维护者]

**正文（English short version）**:

> Hi [Name],
>
> We are building Poieweave, an open-source (Apache-2.0) capability-graph project. We have three draft contracts and would value a review that tries to **construct counterexamples**, not a style review:
> - Physical Ontology v0.3-draft: <https://github.com/haeliotang/poieweave/blob/main/specifications/physical-ontology.zh-CN.md>
> - State and Action Protocol v0.2-draft: <https://github.com/haeliotang/poieweave/blob/main/specifications/state-action-protocol.zh-CN.md>
> - Viability and Expansion Contract v0.2-draft: <https://github.com/haeliotang/poieweave/blob/main/specifications/viability-expansion.zh-CN.md>
>
> We already listed the known open questions and counterexample directions here (please start there if useful): <https://github.com/haeliotang/poieweave/blob/main/docs/status/contract-self-audit.zh-CN.md>. Comments can be filed with this template: <https://github.com/haeliotang/poieweave/blob/main/.github/ISSUE_TEMPLATE/contract-review.yml>.
>
> About [2–3 hours] should be enough for one contract. We will record a disposition (accepted / rejected with reason / deferred with reopening condition) for every comment. A first round by [date] would help; even partial time or a single focus area is useful.
>
> Thanks,
> [name]

**附件/链接**：三份契约、[自审问题包](contract-self-audit.zh-CN.md)、评审模板、[证据矩阵](test-matrix.zh-CN.md)（可选背景）。

**验收**：意见已登记，并逐条标注接受/拒绝/推迟（DEVELOPMENT M1.8）。

---

## B. M1.9 ADR-0001 开源边界确认（Inceptless）

**发送对象**：发起方指定的确认人 / 法务联系人。

**正文**：

> 你好 [姓名]，
>
> 按 M1.9 要求，需要发起方对 [ADR-0001 开源边界](../adr/0001-open-source-boundary.zh-CN.md) 给出**书面确认**。请特别确认：
>
> 1. 项目采用 Apache-2.0；
> 2. 六条不变式（安全与授权语义保持开放、可审计、可重放等）；
> 3. "开放核心 + 可闭源适配器 / 客户模型 / 托管服务"的边界；
> 4. 配套文件 `NOTICE`、DCO、`SECURITY.md` 与 `cargo-deny` 策略可接受。
>
> 材料：[ADR-0001](../adr/0001-open-source-boundary.zh-CN.md)、[外部行动清单](external-actions.zh-CN.md#b-m19-adr-0001-开源边界确认)。
>
> 如确认，请回复"已接受"并给出确认人与日期，我们会更新 ADR 状态；如需法务复核，请告知意见渠道与预计时间。

**验收**：ADR 状态改为"已接受"，记录确认人与日期。

---

## C. M1.10 正式商标检索

**发送对象**：知识产权代理 / 发起方法务。

**正文**：

> 你好 [姓名]，
>
> 请协助对 "Poieweave" 做正式商标检索与注册建议。背景：[ADR-0002 §7](../adr/0002-rename-to-poieweave.zh-CN.md) 记录了初步检索（TMview/Trademarkia 未见完全同名；"POIE" 词根存在残余风险；官方库直连在欧洲/美国被阻断，类别专项检索未完成）。
>
> 建议范围：类别 7（机械/泵）、9（软件/控制）、42（工程/技术服务）；辖区 US / EU / CN / WIPO。
>
> 交付物：各库检索结果、风险判断、是否申请及类别建议、预算与时间估计。预算范围由发起方确定（[预算范围]）。
>
> 验收：检索结果记入 ADR-0002，并给出申请建议。

---

## D. M3.3 外部贡献用例（配额 ≥ 1/4）

**发送对象**：独立工程师 / 研究者 / 课程合作方。

**正文**：

> 你好 [姓名]，
>
> 我们在为工业能力图项目收集**独立编写、预登记**的测试用例（不要求写代码，用例是 TOML）。目标：至少 10 个非项目作者贡献的用例（40 例的 1/4），覆盖扰动类型：共因、循环、库存竞争、质量门、验证等级、未知状态。
>
> 模板与示例：<https://github.com/haeliotang/poieweave/blob/main/docs/status/case-contribution-template.zh-CN.md>
> 现有用例：<https://github.com/haeliotang/poieweave/blob/main/scenarios/base-01/cases.toml>
>
> 规则很简单：**先写预期，再运行**；如果结果与预期不符，我们会一起判断是模型问题还是预期错误。提交时请声明你不是项目作者、用例独立编写。
>
> 你愿意贡献 [N] 个用例吗？预计每个 [30–60 分钟]。我可以先给你讲一遍加载器与标签规则。
>
> [发起方/维护者]

**验收**：≥10 个外部用例，均有预登记预期与通过记录；`docs/progress/foundation-tasks.md` 更新配额。

---

## E. M9.0 合规与出口管制复核

**发送对象**：发起方法务/合规。

**正文**：

> 你好 [姓名]，
>
> 请对适配器归属与出口管制做一次复核：Eclipse Ditto、BaSyx AAS、ROS 2 适配器计划采用"开放核心 + 适配器可在客户侧实现"的结构（[ADR-0001](../adr/0001-open-source-boundary.zh-CN.md) §4.3）。需要确认：
>
> - 每个适配器属于"可开源 / 客户侧 / 需许可"中的哪一类；
> - 是否有出口管制、行业监管或数据驻留限制；
> - 需要在 README/ADR 中写明的限制条款。
>
> 材料：[external-actions](external-actions.zh-CN.md#e-m90-合规与出口管制复核)、[ADR-0001](../adr/0001-open-source-boundary.zh-CN.md)。
>
> 验收：每个适配器登记结论与限制条款。

---

## F. M9.2 真实适配器接入（Ditto / BaSyx AAS / ROS 2）

**发送对象**：能提供测试实例的团队。

**正文**：

> 你好 [姓名]，
>
> 我们想接一个**只读影子适配器**做验证，不改动你们的系统。需要的信息：
>
> - 版本、端点（sandbox/staging）、认证方式（只读凭据）；
> - 时间戳语义（event time / ingest time）、断连行为；
> - 是否支持命令幂等键（后续 M10 写入前必须）；
> - 可测试的时间窗口与联系人。
>
> 我们会遵守：只读、限速、不写状态；适配器契约见 <https://github.com/haeliotang/poieweave/blob/main/crates/poie-runtime/src/adapter.rs>，计划见 [external-actions](external-actions.zh-CN.md#f-m92-真实适配器eclipse-ditto--basyx-aas--ros-2)。
>
> 验收：每个适配器有兼容性记录，只读影子运行跑通。

---

## G. M9.6 / M10 物理试验台与安全演练

**发送对象**：实验室/试验场负责人。

**正文**：

> 你好 [姓名]，
>
> 我们计划做一次小规模物理验证：小型水泵回路 + 可编程执行器，验证"有界执行"（授权、幂等、结果未知不重试）与接管/降级。**急停与保护必须独立于我们的软件**；我们需要：
>
> - 设备清单与可编程接口（PLC/OPC UA/ROS 2 均可）；
> - 安全评审与操作规程；
> - 可用的试验时段与操作人员；
> - 断电、断网、重启演练的许可。
>
> 计划与验收标准见 [external-actions](external-actions.zh-CN.md#g-m96--m10-物理试验台与安全演练)；已有的模拟结果见 [H3 报告](../../experiments/h3-bounded-execution/REPORT.sim.md)（明确标注 simulated）。
>
> 验收：物理 H3 报告、无重复物理动作或已登记修复、接管/降级演练记录。

---

## H. M11.6 扩张的物理实施

**发送对象**：可建造/改造试验设施的合作方。

**正文**：

> 你好 [姓名]，
>
> 我们有一个"切屑回收熔炉"扩张提案（[proposal.toml](../../scenarios/base-01-expansion/proposal.toml)），模型评估显示可让原本 blocked 的目标变为 candidate，但目前只有 `modeled_gain`。要升级到 `demonstrated_gain`，需要实际建造/改造并留下运行记录。
>
> 想了解：可用的场地/设备、建造与检验资源、安全与环保要求、预算与周期。验收标准见 [external-actions](external-actions.zh-CN.md#h-m116-扩张的物理实施)。

---

## I. M12.6 真实多节点联邦测试

**发送对象**：拥有两台可隔离网络节点/VM 的团队。

**正文**：

> 你好 [姓名]，
>
> 我们想复测多站点联邦：两台节点、可控制网络分区与时钟偏移。测试内容：SAP-T5–T8 复测、长时间分区、反复分区/重连、重连对账、无双重控制/超额预留。
>
> 现有证据：进程内模拟与本机 TCP 双进程演示（明确标注非真实多站点）。生产传输验收要求见 [ADR-0008](../adr/0008-federation-sync.zh-CN.md)。
>
> 需要：节点访问方式、可用的分区/时钟控制手段、测试窗口。验收见 [external-actions](external-actions.zh-CN.md#i-m126-真实多节点联邦测试)。

---

> 本文件不包含真实联系人信息；请发起方指定负责人并决定发送渠道。所有外部行动的状态以 GitHub issue 为准。
