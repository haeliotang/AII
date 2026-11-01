# 契约反例评审 A（工业自动化与过程控制视角）

本文件由 AI 子代理以『工业自动化工程师』视角生成，属 AI 预演评审，**不计入 M1.8 退出条件**（M1.8 要求真实的领域外读者）。

- 评审日期：2026-11-01
- 审读的契约版本：
  - `specifications/physical-ontology.zh-CN.md`（下称 PO）：0.3-draft / 2026-10-31
  - `specifications/state-action-protocol.zh-CN.md`（下称 SAP）：0.2-draft / 2026-10-31
  - `specifications/viability-expansion.zh-CN.md`（下称 VEC）：0.2-draft / 2026-10-31
- 评审标准：每条发现都必须能给出具体反例。评审对象是契约文字；代码（`crates/`）和场景（`scenarios/`）只用来确认反例能否触发。
- 独立性声明：评审过程中未阅读 `docs/status/` 下的其他文件，也未阅读 `docs/research/`。
- 验证用的临时文件放在仓库外的 `/tmp/poie-review/`，没有改动仓库里的其他文件。

严重度定义：**高**表示会把物理上不可行或不守恒的情况判为可行或通过，或者会造成重复执行、不可逆动作误执行；**中**表示会误判可行性、与现场做法冲突，或者条款之间互相矛盾，导致实现无所适从；**低**表示措辞问题，或影响面有限。

---

## A-01 质量守恒可以被一个"无质量"输入整体绕开（`unchecked` 既不阻止加载，也不阻止求解）

- 涉及条款：PO §5 第 4、6 条；§5 第 3 条（`mass_tolerance`）；§6.3、§6.4
- 严重度：**高**
- 反例：在 Base-01 中加入资源 `work-order`（单位 `item`，不声明 `mass_per_unit`）和一个批次（5 item，`accepted`），再加入工艺 `conjure-sleeve`：输入 1 item `work-order`，输出 1 item `sleeve-p1`（0.8 kg）和 500 kg `bronze-swarf`，`verification = "verified"`。工艺凭空产出了 500.8 kg 物质。按 §5 第 4 条，它只被标为 `unchecked`；§5 第 6 条只规定 `violated` 阻止加载，所以模型照常加载。随后把 `lot-sleeve-01` 设为 0，查询 `service(potable-water)`，`min_verification = verified`，得到的结论是 `candidate`，路线第一步就是 `conjure-sleeve`，最低验证等级报告为 `verified`。路线报告里也看不出这一步没有做过守恒校验。
- 为什么构成问题：
  1. 一个物质类输入缺少 `mass_per_unit`，整条工艺的守恒检查就失效了，其他已知质量的输入和输出完全没有被核对。
  2. 契约只要求把 `unchecked` 和"通过"区分开，但没有说 `unchecked` 的工艺能不能进入候选路线，也没有要求候选结果标出未校验的步骤。最终，一条"已验证"等级的路线里出现了一个产出无中生有的步骤。
  3. §5 第 3 条说 `mass_tolerance`"不得用来隐藏未建模的损耗"，但契约没有给出上限，所以这条要求无法检验。实现接受 [0, 1) 内的任意值，作者声明 0.9 就能让输入 100 kg、输出 15 kg 的工艺通过校验。
- 建议修改：(a) 对已知质量的部分仍做偏差校验，例如只在"缺失项可能弥补差额"的方向上放宽；(b) 求解器默认排除 `unchecked` 工艺，或者在 `candidate` 的每一步和汇总里强制标出 `mass_unchecked`；(c) 给 `mass_tolerance` 设硬上限（例如 0.05），超过上限时要求附带证据记录。
- 验证方式：在 `/tmp/poie-review/model.toml` 和 `cases.toml` 中构造上述用例，运行 `cargo run -p poie-ontology --bin poie-graph -- /tmp/poie-review/cases.toml`。输出为 `mass balance: 6 balanced, 1 unchecked ["conjure-sleeve"]`，用例 X1 判为 `candidate`，路线包含 `conjure-sleeve [verified]`。`crates/poie-ontology/src/model.rs` 中的 `check_mass` 遇到任一缺失项就直接返回 `Unchecked`，加载阶段只对 `Violated` 调用 `bail!`；`mass_tolerance` 的允许范围是 `(0.0..1.0)`。

## A-02 离散事件仿真只做"逐次转化"守恒：闭环会累积造出质量，库存透支也不报告

- 涉及条款：PO §3.10 第 3、4 条；§3.8 第 3 条；§5 第 3 条
- 严重度：**高**
- 反例：
  1. **闭环漂移**：设一个回收闭环，`bronze-bar` 经熔炼得到 `bronze-swarf`，再经重铸得到 `bronze-bar`。每次转化的输出都比输入多 0.99%，单次都在 1% 容差内。初始 3 kg，循环 200 次（400 次 transfer）后，`bronze-bar` 为 154.3 kg，守恒违规 0 条，外部流 0 条。
  2. **透支**：库存只有 3 kg `bronze-bar`，执行一次 transfer，输入 10 kg `bronze-bar`，输出 10 kg `bronze-swarf`。仿真终态为 `bronze-bar = -7 kg`、`bronze-swarf = 10 kg`，守恒违规 0 条。
- 为什么构成问题：§3.10 只要求"每次内部转化"在相对容差内平衡，没有要求对全仿真做总质量核算（期初库存 + 外部输入 − 外部输出 − 进入资产的质量 = 期末库存）。相对容差逐步累积后没有上界，而在循环水、回收料这类过程工业常见的闭环里，漂移是指数级的。§3.8 第 3 条"可用量不得为负"只约束"计划"，没有约束仿真。仿真把负库存当作正常数值继续计算，而且负库存参与 transfer 时，单次守恒检查仍然通过，所以透支等于凭空造出物质。现场的物料衡算（mass balance reconciliation）首先就要做全厂总平衡，单台设备平衡并不足够。
- 建议修改：§3.10 增加两条要求：(a) 仿真结束时做全局质量核算，偏差相对总通量计算，并与逐次容差分开报告；(b) 任何库存小于 0 时必须作为 `conservation` 或 `insufficient_resource` 类违规报告。
- 验证方式：在 `/tmp/poie-review/drift/` 中用 path 依赖调用 `poie_sim::Simulation`，载入 Base-01 模型，依次调度上述 transfer。输出为 `A: end bronze-bar = 154.3 kg, violations = 0, external flows = 0` 和 `B: bronze-bar = -7 kg, bronze-swarf = 10 kg, violations = 0`。`crates/poie-sim/src/simulation.rs` 的 `Transfer` 分支直接对库存做 `-=`，没有做非负检查。

## A-03 幂等键复核遗漏 `outcome_unknown`：按条文，不可逆动作可在结果查明前重派；而参考实现让任何重派都不可能

- 涉及条款：SAP §4 第 5 项；§3.2 补充规则 2；§11 第 2 条
- 严重度：**高**
- 反例：A1 = "打开泄放阀 XV-101"，`irreversible`，幂等键 `k1`。A1 派发后回执超时，状态转为 `outcome_unknown`。值班员按 §3.2 规则 2 新建 A2：新的 `action_id`，引用 A1，复用 `k1`。A2 校验、授权后进入派发前复核。§4 第 5 项只检查"`k1` 在本地有没有处于 dispatched / acknowledged / verified 的其他动作"。A1 处于 `outcome_unknown`，不在列表中，所以第 5 项通过；其余五项也与 A1 的结果无关。于是 A2 在 A1 的结果查明之前被派发了，直接违反 §3.2 规则 2"对 `irreversible` 动作，在检查结果出来之前不得重新派发"。`failed` 同样不在列表中，与 §11 第 2 条"`failed` 不释放幂等键"相互矛盾。
- 反过来看实现：`ActionLedger` 一旦某个键被派发，就永久记入 `dispatched_keys`，以后任何其他 `action_id` 用同一键派发都会返回 `DuplicateDispatch`。`redispatch_allowed()` 只有测试调用，没有接入派发路径。所以 §3.2 规则 2 允许的两种重派——结果检查后的不可逆重派、策略允许的幂等重派——在参考实现里都做不到。条文过松，实现过严，两者不一致。
- 为什么构成问题：对泄放阀、注药、点火这类不可逆动作，重复执行就是安全事件。六项派发前复核是唯一的强制关口，但它没有把规则 2 编码进去。
- 建议修改：把第 5 项改为"同一幂等键不存在处于 dispatched / acknowledged / verified / outcome_unknown / failed 的其他动作，除非满足 §3.2 规则 2 的重派条件（引用原动作，且结果已检查，或者原动作可幂等重派并有策略许可）"，并要求实现把 `redispatch_allowed` 接入复核。
- 验证方式：阅读 `crates/poie-state/src/review.rs`（第 5 项调用 `ledger.dispatched_by`）、`crates/poie-state/src/action.rs` 的 `apply` / `restore` / `redispatch_allowed`，以及 `crates/poie-state/tests/sap_acceptance.rs` 中的 `restart_preserves_the_dispatched_idempotency_key_guard`（该测试断言同键第二个动作必然被拒）。

## A-04 "重复幂等键被拒"按条文是 NACK，NACK 又被定义为"执行端声明未执行"，与物理事实正好相反

- 涉及条款：SAP §4 末段；§3.3 `dispatched → failed`；§3.2 补充规则 2；SAP-T2、T3
- 严重度：**高**
- 反例：命令"关闭进水阀 V3"（`idempotent`）A1 已被执行，阀门已关，但 ACK 丢失，A1 进入 `outcome_unknown`。策略允许幂等重派，于是 A2 复用键 `k1` 派发。执行端的去重表仍在，按 §4"以 NACK 回执说明原因"拒绝重复键。按 §3.3，收到 NACK 就转 `failed`，含义是"执行端声明未执行"。结果是：日志里 A2 = `failed`，表示阀门未关闭；实际上阀门已经关闭。规划层可能据此升级处理，例如派人或机器人去手动关阀，或者判定隔离失败而停掉整个系统。
- 为什么构成问题：PLC/SCADA 现场区分"命令被拒绝（未执行）"和"重复命令已被吸收（之前已执行）"，后者实际是成功信号。契约把两者都塞进 NACK，又给 NACK 规定了唯一语义"未执行"。按契约设计的幂等重派因此必然得出错误结论。参考执行端（`crates/poie-state/src/executor.rs`）其实已经区分了 `DuplicateRejected` 和 `Nack`，说明契约文字落后于实现。
- 建议修改：在 `receipt` 中增加第三种回执，例如 `DUPLICATE`，并附带原执行的标识或时间。新增转移 `dispatched → outcome_unknown`（或直接引用原动作结果），触发条件是收到 DUPLICATE，不得转 `failed`。

## A-05 VEC 把结构层 `blocked` 当作"任何时域内都无法完成"，与 PO 的库存/状态时间线矛盾，会误判不可行

- 涉及条款：VEC §3.3 第 1 点；§4.1 "恢复路径"；PO §6.4（`blocked` 的含义）、§3.8 第 3 条、§3.11 第 1 条
- 严重度：**高**
- 反例：t = 0 时备用断路器 `breaker-b1` 库存为 0，采购件按库存时间线在 t = 48 h 到货。`bus-1` 在 t = 1 h 失效。结构层在 t = 1 h 查询 `restore(bus-1)`，得到 `blocked`（`missing_resource: breaker-b1`）。VEC §3.3 说这类结果"在任何时域内都无法由本模型完成，可以直接作为可行性分析的反例依据"。但在 H = 72 h 的时域里，48 h 后到货就能修复，正确结论应当是"服务中断约 47 h 后恢复"，而不是不可行的反例。同理，因 `min_verification` 过滤得到的 `blocked`，或者某资产当前 `failed`、但按时间线在后续时刻恢复的情况，都不满足"任何时域"。
- 为什么构成问题：PO §6.4 明确说 `blocked` 只是"在给定模型、库存与验证等级下"的结论，而库存和资产状态在时间版场景里会随时间变化。VEC 把一个依赖快照的结论升格成了与时域无关的结论，会把可恢复的情况误报为不可行，正是 VEC §1 第 3 条要防止的那类错误的反方向。
- 建议修改：把 VEC §3.3 第 1 点改为"`blocked` 只说明在查询时刻的库存与状态快照下无路线"。只有当阻断根因全部是结构性的（`cycle`，或外部依赖在整个时域内都没有补给记录）时，才可以作为反例依据；否则要在后续时刻重查。

## A-06 `status_evidence` 只是"应当"，但"没有有效证据即 `unknown`"是"必须"，参考场景本身就违反后者

- 涉及条款：PO §3.1 字段表（`status_evidence` 为"应当"）、§3.1 状态语义中的 `unknown`、§4 第 3、4 条；SAP §2.3 第 3 条
- 严重度：中
- 反例：`scenarios/base-01/model.toml` 中，`pv-1`、`bat-1`、`bus-1`、`filter-1`、`lathe-1`、`gauge-1`、`charger-1`、`robot-m1` 都声明为 `status = "available"`，但都没有 `status_evidence`。按 §4 第 3 条（"没有有效证据时为 `unknown`"），这些资产都应当是 `unknown`，S0 的结论应当是 `unknown`，而不是 `candidate`。参考实现照单全收，把它们当作 `available`。同一个 `pv-1` 在 `scenarios/base-01-timed/scenario.toml` 中又被设成了 `initial = "unknown"`，注释写着"no status evidence"。同一事实在两个场景里得到两种处理。
- 为什么构成问题：读者无法判断，声明为 `available` 但没有证据的资产到底算可用还是未知。这直接决定 `candidate` 和 `unknown` 两个结论之间的边界。在 SIS 的 proof test 实践中，没有检验记录的设备一律视为状态未知。
- 建议修改：二选一。(a) `status_evidence` 改为"必须"，缺失时投影为 `unknown`；(b) 明确"场景声明的初态视为一条合成观测"，并要求报告中标出 `synthetic` 证据。

## A-07 服务只有 AND 结构，无法表达 N+1 冗余；绕道"虚拟资产"又与资产定义和证据规则冲突

- 涉及条款：PO §3.7（`requires` 要求全部可用）、§3.1（资产是"有身份的物理对象"；`requires_any` 只存在于资产）、§6.2 `service(s)`
- 严重度：中
- 反例：供水服务由两台 100% 容量的泵 P1、P2（一用一备）提供，任一台运行即满足。
  - 写成 `requires = ["p1", "p2"]`：P1 失效时服务判为不成立，P2 明明可以接替。这是漏判可行。
  - 写成 `requires = ["p1"]`：忽略了备用泵，P1 失效时结论同样错误。
  - 唯一的绕法是建一个虚拟资产 `pump-group`，令 `requires_any = ["p1", "p2"]`，服务依赖它。但 `pump-group` 不是物理对象，没有任何传感器能为它提供 `status_evidence`。如果按 A-06 的严格读法，它永远是 `unknown`；如果随便声明为 `available`，又会掩盖"泵组本身"根本不存在的事实。
- 为什么构成问题：N+1 / 2oo3 冗余是过程工业关键服务的标准配置（泵、风机、电源、仪表）。契约无法正确表达冗余，`service` 结论就会系统性地偏向悲观；用户为了绕过而建虚拟资产，又会污染证据链。
- 建议修改：给 Service 增加 `requires_any`（或通用的 k-out-of-n）字段，求解语义复用 §6.3 规则 9。

## A-08 储能功率上限在时间版结论中不起作用：能量够、功率不够的任务会被判为可行

- 涉及条款：PO §3.3 末段（储能按批次建模）、§3.9 第 1、2 条、§3.11 第 4 条（"能量与物质资源使用同一规则"）
- 严重度：中
- 反例：电池组 `bat-1` 存有 40 kWh，逆变器最大放电功率 5 kW。工艺"焊接修复"消耗 6 kWh，时长 30 min，平均功率 12 kW。按 §3.11 第 4 条，时间版资源检查只比较"步骤开始时刻的库存 ≥ 累计消耗"：40 kWh ≥ 6 kWh，所以判为 `feasible`。按 §3.9，实际只能放出 5 kW × 0.5 h = 2.5 kWh，任务在物理上无法完成。§3.9 的裁剪规则只在 DES 的 `EnergyRequest` 中生效，时间版场景和结论类别（§3.11）里没有引用它。
- 为什么构成问题：孤岛微网、备用电源场景中，功率约束往往比电量约束更早成为瓶颈，例如电机启动电流、焊机、加热器。契约里同时存在两套电池模型（批次和储能设备），却没有规定时间版结论用哪一套，结果是能量够、功率不够的情况被判为可行。
- 建议修改：§3.11 第 4 条增加一条要求：消耗能量的步骤还必须满足"消耗量 ÷ 时长 ≤ 所用储能设备的最大放电功率"，不满足时报告 `insufficient_power`（或并入 `insufficient_resource` 并注明原因）。
- 验证方式：`crates/poie-sim/src/timed.rs` 中没有任何对 `Battery` 或功率的引用；`scenarios/base-01-timed/scenario.toml` 只把电能写成 `stock_timeline`。

## A-09 预测失效窗口结束后"降为 `unknown`"等于让预测覆盖了仍然有效的观测；实现中两处语义也不一致

- 涉及条款：PO §3.8 第 2 条；§4 第 1 条（观测、预测"不得互相覆盖"）、§4 第 4 条（有效期由策略规定）；§3.11 第 2 条
- 严重度：中
- 反例：
  1. t = 0 h 时振动观测显示泵 P1 正常，策略规定有效期 24 h。模型预测 P1 在 [2 h, 4 h] 内失效。到 t = 5 h，观测仍在有效期内，按 §4 第 4 条 P1 应为 `available`；按 §3.8 第 2 条（窗口结束仍无新证据）P1 却是 `unknown`。预测把仍然有效的观测"覆盖"了，与 §4 第 1 条以及同一条里的"预测不得覆盖证据"矛盾。
  2. 窗口内的"预测失效"不在 `available / degraded / failed / unknown` 四种状态里，契约没有说此时 P1 能不能被占用。
- 实现层面：`AssetTimeline::status_at` 在观测时间 ≤ 窗口起点时让预测生效，所以 t = 1.9 h 的新鲜观测也会在窗口结束后被降为 `unknown`。而时间版求解（`timed.rs` 中的 `patched_model_at`）使用的是 `explicit_status_at`，完全忽略预测。窗口内和窗口后的资产在时间版结论里都按 `available` 处理。同一份契约在两处得出相反的语义。
- 建议修改：明确"预测失效"对占用的影响，例如"可以占用，但报告必须标注"；窗口结束后的状态改为按 §4 第 4 条的有效期判断，而不是一律降为 `unknown`。

## A-10 "质量状态即资源类型"与"批次 `quality` 字段"两套机制冲突，无法表达"隔离品经检验后放行"

- 涉及条款：PO §3.2 末段（质量状态必须是不同资源类型，只能通过检验工艺转换）、§3.3（只有 `accepted` 批次可以分配给工艺）
- 严重度：中
- 反例：Base-01 的 `lot-sleeve-02` 资源类型是 `sleeve-p1`（已检验合格），`quality = quarantined`（"Supplier certificate missing"）。现场标准做法是对该批次做来料检验，合格后放行。在契约里这无法表达：
  1. 检验工艺 `inspect-sleeve-p1` 的输入是 `sleeve-p1-raw`，不是 `sleeve-p1`。
  2. 即使写一个以 `sleeve-p1` 为输入的复检工艺，§3.3 也禁止把 `quarantined` 批次分配给任何工艺，包括检验工艺。
  反过来，要让未检验的毛坯 `sleeve-p1-raw` 能进入检验工艺，它的批次又必须标为 `accepted`，也就是说"未检验品"必须被标成"已接受"，这与质量管理中待检品必须处于 hold / 隔离状态的做法正好相反。
- 为什么构成问题：同一个质量含义用两套互不相通的机制表达，导致"隔离品复检放行"这个最常见的质量流程无法建模，求解器会对 S1 一类情况给出过于悲观的结论（明明可以复检放行，却被判为只能重新加工），也会迫使建模者给毛坯贴上错误的 `accepted` 标签。
- 建议修改：允许 `quarantined` 批次分配给声明为检验或处置类的工艺，例如在工艺上加 `accepts_quarantined = true`；或者干脆取消批次的 `quality` 字段，统一用资源类型表达质量状态。

## A-11 租约到期的时钟保守性只约束持有方，接管方没有对称的余量，两个场址可能同时派发

- 涉及条款：SAP §6.1 第 1、3 条；§11 第 4 条；SAP-T7、T8
- 严重度：中
- 反例：偏差上界 `skew_bound` = 2000 ms，租约名义到期时刻为 T。场址 A（持有方）的时钟比参考时钟慢 1500 ms，它在本地时间 T − 2000 ms 停止派发，对应参考时间 T − 500 ms。场址 B（接管方）的时钟比参考时钟快 1500 ms，在本地时间到达 T 后获得授予，对应参考时间 T − 1500 ms。两边的偏差都在上界之内，但在参考时间 [T − 1500, T − 500] 这 1 s 里，A 和 B 都认为自己可以派发，违反 §6.1 第 1 条"任一时刻最多一个持有者"。
- 为什么构成问题：§6.1 第 3 条只笼统地说"时钟偏差界必须计入到期判断"，§11 第 4 条只给出了持有方的规则；单边余量在两侧时钟偏差相加时不足以保证互斥。参考实现 `OwnershipRegistry::grant` 只要 `now_ms > until_ms` 就允许他方接管，没有加余量。
- 建议修改：规定接管方必须等到本地时间 ≥ `valid_until + skew_bound` 才能接收；或者把 `skew_bound` 明确定义为"任意两场址之间的相对偏差上界"，并相应调整持有方的提前量。

## A-12 扩张核算的"失去"只统计 `candidate → blocked`，变成 `unknown` 或 `search_limit` 的关键目标会从两张表里都消失

- 涉及条款：VEC §6.2 第 1 点、§6.3 第 2 条
- 严重度：中
- 反例：扩张提案"新增一台注塑机"。建造时消耗了唯一一批有效盘点证据的备件 A，剩下的同类备件批次证据已过期。另外新增工艺使搜索树变深。扩张前，关键服务恢复目标 G1 是 `candidate`。扩张后：
  - G1 变为 `unknown`（剩余备件证据过期）；
  - G2 变为 `search_limit`（搜索深度超限）。
  按条文，"失去"只统计 `candidate → blocked`，"获得"只统计 `blocked / unknown → candidate`，G1、G2 两个列表都不进。§6.3 第 2 条（"失去列表中没有关键服务的恢复目标"）于是被满足，提案可以宣称"带来净能力增益"。
- 为什么构成问题：结论退化成 `unknown` 或 `search_limit`，对关键服务来说同样意味着失去了已确认的恢复路线。条文给扩张方留下了"把失去伪装成未定"的空间。参考实现（`crates/poie-expansion/src/structural.rs`）实际上把所有 `candidate → 非 candidate` 都计入 `lost`，比条文严格；但它又把 `search_limit → candidate` 计入 `gained`，条文没有允许这一点。契约与实现在两个方向上都不一致。
- 建议修改：把"失去"定义为 `candidate → 任何非 candidate`，"获得"定义为 `blocked / unknown → candidate`，并单独列出 `search_limit` 参与的变化，不计入任何一方。
