# Foundation 阶段任务进度

开始：2026-10-31 · 顺序：三契约 → 本体+能力图原型 → Base-01 场景规范

## 当前快照（2026-10-31）

M1 工作包 10 个，已完成 7 个（M1.1–M1.7），剩余 3 个（M1.8–M1.10）。
权威清单与退出条件见[开发文档 §2](../DEVELOPMENT.zh-CN.md#m1-契约草案与结构原型--基本完成)。

| 工作包 | 状态 | 证据 / 备注 |
| --- | --- | --- |
| M1.1–M1.3 三份契约 v0.1-draft | 完成 | `specifications/`；PO 已在 M2 升为 v0.2-draft |
| M1.4 能力图原型 | 完成 | `crates/poie-ontology`，44 个单元测试 |
| M1.5 Base-01 结构场景 | 完成 | 40 个预登记用例全部一致；16 个集成测试 |
| M1.6 持续集成 | 完成 | PR #2；`main` CI [run #2](https://github.com/haeliotang/poieweave/actions/runs/41222603966) 为绿 |
| M1.7 贡献指南 | 完成 | `CONTRIBUTING.md`（DCO、测试门、契约变更流程）、`DCO`、`SECURITY.md`、三类 issue 模板；GitHub 私有漏洞报告已启用 |
| M1.8 契约外部评审 | 未开始 | 需外部读者审读并登记 issue |
| M1.9 开源边界 | 部分 | 配套文件已齐（`NOTICE`、`DCO`、`SECURITY.md`、`cargo-deny`）；ADR-0001 仍为"提议"，待发起方确认 |
| M1.10 名称落地 | 部分 | 改名已完成；初步商标检索已记入 [ADR-0002 §7](../adr/0002-rename-to-poieweave.zh-CN.md)，官方库直连与类别专项检索待人工复核 |

M2 本体 v0.2：**已完成**（M2.1–M2.8）。PO 升为 v0.2-draft（schema `poie.physical-ontology/0.2`，场景用例 schema 升为 0.2）：OR 支撑、多来源补足、降级占用、保守合格率与返工、能力验证等级、路线枚举、外部依赖上溯全部并入正文；S11–S24 共 14 个预登记用例逐条覆盖 M2.1–M2.7；S0–S10 的预期与首跑结果未改；运行时间基线见 [m2-baseline.md](m2-baseline.md)。

M3 H1 结构层实验：**基本完成**——实验已登记（[PREREG.md](../../experiments/h1-structural/PREREG.md)）、基线已实现、40 例分层用例、留出集已选定（[HOLDOUT.md](../../experiments/h1-structural/HOLDOUT.md)）、[报告已发布](../../experiments/h1-structural/REPORT.md)。留出集上基线漏判 2/7、误判 1/3；Poieweave 漏判 0/7。按预登记规则 H1 以弱形式被支持，但**外部用例配额（≥1/4）未满足**，结论仅为内部证据。

M4 状态与事件内核：**已完成**（M4.1–M4.9）。SAP 升为 **v0.2-draft**（记录模式 `poie.record/0.2`，实现决策见 SAP §11）；`crates/poie-state` 53 个单元测试 + 13 个 SAP 验收测试，T1–T11 全绿；退出条件逐项满足（属性测试 10⁵ 序列、逐写入点崩溃注入、契约升级）。跨场址交换与对账按计划留到 M12。

M5 时间维度与 Base-01 v0.2：**进行中（仅剩 M5.7）**——M5.1 时间线/预测、M5.2 调度、M5.3 能量/功率、M5.4 `poie-sim`、M5.5 逐位重放、M5.6 Base-01 v0.2 已完成（PO §3.8–3.11 草案；`scenarios/base-01-timed/` 20 个预登记时间版用例 T0–T19 全绿，digests.toml 在 CI 逐位校验）；M5.7 H1 时间版复测待做。

## 初期工作清单（2026-10-31 完成）

| # | 任务 | 产物 | 状态 |
| --- | --- | --- | --- |
| 1 | Physical Ontology 契约 v0.1-draft | `specifications/physical-ontology.zh-CN.md` | 已完成草案 |
| 2 | State & Action Protocol 契约 v0.1-draft | `specifications/state-action-protocol.zh-CN.md` | 已完成草案 |
| 3 | Viability & Expansion 契约 v0.1-draft | `specifications/viability-expansion.zh-CN.md` | 已完成草案 |
| 4 | 本体+能力图 Rust 原型 | `crates/poie-ontology/` | 已完成 |
| 5 | Base-01 场景规范与机读数据 | `scenarios/base-01/` | 已完成 |
| 6 | 场景用例测试、README 更新 | `cargo test` 全绿 | 已完成 |
| 7 | 按里程碑的完整开发文档（用户 2026-10-31 追加） | `docs/DEVELOPMENT.zh-CN.md` | 已完成 |

## 记录

- 2026-10-31：Rust 1.94 可用；crates.io 可访问，原型依赖限定为 serde + toml。
- 2026-10-31：原型完成；21 个测试通过（13 单元 + 8 集成），clippy 无警告。
- 2026-10-31：Base-01 的 11 个用例在运行前写定预期，首次运行全部一致；之后只改进了报告文字。
- 2026-10-31：S3 暴露单母线 + 单机器人的结构弱点，已写入 PO §8 与 Base-01 §6。
- 2026-10-31：开发文档完成，M1 剩余项为 CI、贡献指南、契约外部评审。
- 2026-10-31：项目改名 Poieweave（原 AII），crate 前缀 `poie-`，模式字符串前缀 `poie.`；见 ADR-0002。
- 2026-10-31：GitHub 仓库改名为 haeliotang/poieweave，本地 origin 已更新。
- 2026-10-31：M1.6 CI 已配置（GitHub Actions + cargo-deny），本地按 CI 步骤全部通过；分支 ci/m1.6-github-actions。
- 2026-10-31：M1.6 完成：PR #2 合并，`main` 上 CI 为绿。M1 剩余 M1.7–M1.10。
- 2026-10-31：进度对齐复核：`main` 上 CI [run #2](https://github.com/haeliotang/poieweave/actions/runs/41222603966) 为 `success`；本地 `cargo fmt --check`、`cargo clippy --all-targets -D warnings`、`cargo test`（21 项）、`cargo deny check licenses bans sources` 全部通过，与 CI 一致。
- 2026-10-31：与计划文档逐项对照的差异：M1.9 的 `NOTICE`（ADR-0001 §5 标为"立即"）尚未添加；M1.7 的 `CONTRIBUTING.md`、`SECURITY.md`、DCO 与三类 issue 模板尚未创建；M1.8 外部评审、M1.10 商标检索尚未开始；GitHub 侧尚无 issue/里程碑，进度目前只记在本文与开发文档中。
- 2026-10-31：注意：目录由 `Documents/AII` 改名后，旧 `target/` 缓存会让集成测试按旧路径找场景文件而失败；`cargo clean -p poie-ontology` 即可。
- 2026-10-31：M1.7 完成：新增 `CONTRIBUTING.md`（含 DCO 说明与测试门要求）、`DCO`（DCO 1.1 全文）、`SECURITY.md`；`.github/ISSUE_TEMPLATE/` 下三类 issue 表单（反例 / 契约修改 / 实验提案）与 `config.yml`；创建 `kind:counterexample`、`kind:spec`、`kind:experiment` 标签；GitHub 私有漏洞报告已启用。README 的 Participate 一节已链接贡献指南与安全政策。
- 2026-10-31：M1.9 配套文件齐备：`NOTICE` 已按 Apache-2.0 §4(d) 添加；`DCO`、`SECURITY.md` 随 M1.7 落地；`cargo-deny` 随 M1.6 落地。M1.9 仅剩 ADR-0001 的"已接受"状态确认（需 Inceptless）。
- 2026-10-31：M1.10 商标初步检索记入 ADR-0002 §7：TMview（聚合 USPTO/EUIPO/WIPO/CNIPA 等）与 Trademarkia 对 "poieweave" 均为 0 命中；USPTO/EUIPO/WIPO/CNIPA 官方界面被反爬阻断，检索经聚合库间接覆盖并逐项标注；相近标记集中在 POIE 词根（含一条序列号存疑的美国 cl. 9 申请）。仍需人工/付费复核，不得宣称"已清权"。
- 2026-10-31：M2.1（OR 支撑）完成：PO §3.1 新增 `requires_any`、§6.3 规则 9、§6.4 新增阻断原因 `no_alternative_support`；实现支持替代支撑的递归求解与账本回滚，补丁新增 `asset_requires` / `asset_requires_any` / `add_asset`；单元测试 13→21、集成测试 8→10；Base-01 新增 S11（应急馈线，candidate）与 S12（两个替代都不可用，blocked）两个预登记用例，首次运行即与预期一致，S0–S10 的预期未改动。
- 2026-10-31：M2.2（多来源补足）完成：PO §6.3 规则 10；求解器先尝试单工艺补足整个缺口，失败后按声明顺序逐批组合多条工艺，组合遵守单账本与回溯；补丁新增 `add_resource` / `add_lot` / `add_process`；单元测试 21→24、集成测试 10→11；Base-01 新增 S13（两来源各补一部分，candidate）与 S14（总产能不足，blocked），预登记后首次运行即一致，S0–S12 不受影响。
- 2026-10-31：M2.3（降级使用策略）完成：PO §3.1 新增 `degraded` 策略（`occupiable` / `allowed_processes` / `max_occupations`）、§6.3 规则 11、§6.4 新增 `degraded_restriction`；实现区分占用与恢复路径，降级占用不写入可用集合、按路线计数并在步骤与报告中标注；补丁新增 `asset_degraded`；单元测试 24→30、集成测试 11→12；Base-01 新增 S15（允许占用，candidate）与 S16（超出每路线上限，blocked），预登记后首次运行即一致，S0–S14 不受影响。
- 2026-10-31：M2.4（保守合格率与返工）完成：PO §3.4 新增 `yield = { output, fraction, reject }`、§6.3 规则 12；求解器按有效产出率（单次产出 × fraction）计算运行次数与多来源补足，废品以 `reject` 资源显式记账并出现在步骤产出中，返工路线由普通工艺表达；模型校验 fraction 范围、output 归属、reject 单位与单位质量一致且不得重复声明；单元测试 30→34、集成测试 12→13；Base-01 新增 S17（50% 合格率下青铜不足，blocked）与 S18（废品返工 2 次 → 检验 2 次，candidate），预登记后首次运行即一致，S0–S16 不受影响。
- 2026-10-31：M2.5（能力验证等级）完成：PO §3.6 定义能力自身等级、§6.3 规则 13、§6.4 新增 `capability_below_verification`；资产可声明 `capability_verification`（未声明按 `declared`），求解器按查询门槛过滤提供者，并把能力等级计入路线最低验证等级；基础模型为 robot-m1 的 5 项能力声明等级（现有 S0–S18 结论不变）；补丁新增 `asset_capability_verification`；单元测试 34→38、集成测试 13→14；Base-01 新增 S19（能力仅 declared，blocked）与 S20（能力 modeled、门槛 modeled，candidate），预登记后首次运行即一致。
- 2026-10-31：M2.6（多候选路线）完成：PO §6.3 规则 14（有界、去重、逐条汇总、不声称穷尽）；求解器支持按备选顺序轮换的枚举（64 次上限），每条路线给出消耗、外部依赖与最低验证等级；CLI 新增 `--routes N`；用例新增 `min_routes` / `max_routes` 预登记断言；单元测试 38→40、集成测试 14→15；Base-01 新增 S21（两条不同路线，min_routes=2）与 S22（基础场景仅一条路线，max_routes=1），预登记后首次运行即一致。
- 2026-10-31：M2.7（外部依赖上溯）完成：PO §6.3 规则 15、§6.5 与 VEC §4.2 明确"全部候选路线取并集、无路线时回退阻断根因"；实现 `external_dependency_report`（每条依赖标注出现的路线与单条路线最大消耗量）与 CLI `--external-deps N`；用例新增 `external_dependencies` 预登记断言；单元测试 40→42、集成测试 15→16；Base-01 新增 S23（双来源供水，并集 5 项外部依赖）与 S24（blocked 时回退根因报告 cutting-insert），预登记后首次运行即一致。
- 2026-10-31：M2.8 完成、M2 退出复核通过：PO 版本升为 0.2-draft（§8 只保留未解决条目；schema 字符串升至 `poie.physical-ontology/0.2`，用例 schema 升至 `poie.scenario-cases/0.2`）；S11–S24 覆盖 M2.1–M2.7 各至少 2 例（S11 为 S3 的应急供电改进版）；用脚本比对 `7b163ad` 与当前 `cases.toml`，确认 S0–S10 的预登记预期与首跑结果未改；`--timings` 记录 25 个用例的 release 运行时间（合计 8.437 ms，中位数 0.095 ms，见 docs/progress/m2-baseline.md）。M2 退出条件逐项满足。
- 2026-10-31：M3.1/M3.2 完成：H1 结构层实验登记 `experiments/h1-structural/PREREG.md` 在任何运行前提交（问题、基线精确定义、指标、分层、留出集程序与决策规则）；新增 `poie-h1-baseline` 实验 crate（逐设备检查、不递归、不共享账本），含 5 个单元测试，其中一个回归测试确认基线看不到共因循环。尚无实验结果；按登记，需先扩充到 ≥40 用例并选定留出集后再运行。
- 2026-10-31：M3.3 内部分层扩充完成：Base-01 从 25 例扩充到 40 例（新增 S25–S39），每例带 `tags`（common-cause / cycle / stock-contention / quality-gate / verification / unknown / other），场景加载器拒绝缺少或未知标签；分层计数记入场景规范 §4.2。首次运行发现 3 处预期错误并修正（S31/S35 的 `turn-lathe-nut` 实为 modeled、门槛应为 modeled；S37 的青铜请求量小于库存），理由记录于场景规范 §4.1，符合"先判断是模型、实现还是预期错了"的纪律。留出集尚未选定；PREREG 补记了 CI 全量执行导致的留出集弱隔离偏差。
- 2026-10-31：M3.4 完成：以种子 `poieweave-h1-2026-10-31`、按 `SHA-256("<种子>:<用例>")` 字典序抽取 10/40 留出集（S0、S2、S3、S4、S10、S20、S23、S24、S32、S33），名单在任何最终评估运行前提交到 HOLDOUT.md；harness 增加分层统计与两种方法的根因 subject 匹配。开发集（30 例）首次运行结果存档于 `experiments/h1-structural/raw/dev-2026-10-31.txt`：基线一致 15/30、漏判 4/17、误判 11/13；Poieweave 根因匹配 19/19，基线 1/19。该结果仅记录，结论留待含留出集的最终评估。PREREG 同时记录了 harness 冒烟运行早于留出集提交的偏差。
- 2026-10-31：M3.5 报告完成并合并：全量 40 例（开发集 30 + 留出集 10）分别运行，原始输出存档；留出集基线漏判 2/7（S3、S32，共因）、误判 1/3（S23，双来源），Poieweave 漏判 0/7；按 PREREG §7 规则 H1 以弱形式被支持。报告同时披露：真值由同作者用同一模型写定（循环论证风险）、外部用例配额为 0、留出集弱隔离、3 处预期错误的修正，以及基线定义导致的高误判率。M3 标记为"基本完成"，M3.3 外部配额仍未满足。
- 2026-10-31：开始 M4：新增 `crates/poie-state`。M4.1 实现九类记录、公共信封（含 event/recorded time、HLC、causes、namespace、schema_version）与配对校验；M4.4 精确实现 SAP §3.3 的 18 条动作转移，终态不可再转移，账本拒绝表外转移与同一幂等键的重复派发；属性测试用确定性 PRNG 跑 10⁵ 条事件序列，断言无表外转移、无重复派发，并覆盖非法转移与重复键的拒绝。SAP 契约状态行同步注明"C 参考实现（记录+状态机）"；持久化、投影、派发复核、崩溃恢复尚未实现。
- 2026-10-31：M4.3 与 M4.5 完成：`projection` 实现版本化投影（`poie.projection/0.1`）、每场址状态版本向量、证据过期降为 unknown、更正记录生效、仿真命名空间隔离与确定性；`review` 实现 SAP §4 六项派发前复核（时效、当前状态版本与谓词、命令所有权、预留、幂等键、授权），一次返回全部拒绝项并带稳定 reason code 与 detail。poie-state 依赖 poie-ontology 以复用 `AssetStatus`；测试 13→27。
- 2026-10-31：ADR-0003 与 M4.2 完成：经 ADR 选择"单文件 JSON Lines + fsync"作为本地只追加日志（SQLite/嵌入式 KV 推迟，记录推翻条件），新增 serde/serde_json 运行依赖（许可符合 ADR-0001 §5，cargo-deny 通过）。`log` 模块实现逐条写入后 `sync_data`、场址序号严格递增、按场址读取、启动时只截断尾部不完整行（中间损坏报错），并提供崩溃残行、乱序、重开不重写、JSON 往返等 6 个测试；poie-state 测试 27→33。SAP §5 的只追加持久化与尾部恢复已实现，重启语义与逐写入点崩溃注入留待 M4.7。
- 2026-10-31：M4.6 与 M4.7（部分）完成：`executor` 模拟执行端支持去重窗口、过期拒绝、ACK 丢失、NACK、重启丢失去重表（延迟由调用时刻表示）；`recovery` 实现转移日志重放与 SAP §5 重启决策；`action` 补齐重派策略（irreversible 未检查结果不得重派），`record` 增加"自报验证不算独立验证"的判定。新增 `tests/sap_acceptance.rs`：SAP-T1（派发后崩溃→outcome_unknown 不重发）、T2（ACK 丢失不重复执行，验证收尾）、T3（重复送达只执行一次）、T4（前置变化→stale_precondition 拒绝）、T9（丢失去重表为已知风险 + irreversible 不重派）、T10（self_reported 不算独立验证）。state 测试 33→42，另加 6 个验收测试；T5–T8、T11 与逐写入点崩溃注入仍待做。
- 2026-10-31：M4.7 补全与 M4.8 完成：崩溃注入覆盖派发流的全部持久写入点（action 记录、validated、authorized、dispatched、acknowledged）并断言重启后状态/决策一致；`ActionLedger::restore` 从重放状态重建账本并重新登记已派发幂等键，重启后同键派发仍被拒绝；`isolation::ensure_live_dispatch` 拒绝仿真分支派发现场动作（SAP-T11）。SAP 验收测试 6→9，state 单元测试 42→45。
- 2026-10-31：M4.9 与 §6 完成：`bridge` 把投影状态转成资产状态补丁（无证据→unknown），用 Base-01 验证投影→能力图链路（bus-1 投影故障→blocked；泵无证据→unknown）。`ownership` 实现命令所有权/租约与时钟偏差检测（偏差超界即报错、不计租约有效）；`reservations` 实现实物所在地的库存预留、远程请求在重连后才生效、预留不超额。SAP-T5–T8 验收测试补齐；SAP 验收测试 9→13、state 单元测试 45→53，SAP-T1–T11 全部自动化。
- 2026-10-31：M4 完成：SAP 升为 v0.2-draft（记录模式 `poie.record/0.2`；新增 §11 记录 8 条实现决策：failed 的幂等键保守占用、重启语义、租约时钟保守性、预留场址性、日志格式、T9 已知风险、未实现范围）；M4 退出条件逐项核对通过（SAP-T1–T11 全绿、10⁵ 序列属性测试、逐写入点崩溃注入、契约升级）。Foundation 阶段剩余 M5（时间维度与 Base-01 v0.2）。
- 2026-10-31：M5.1/M5.2 完成：PO 增加 §3.8 时间草案（状态/库存时间线、预测失效是警告而非事实、过期转 unknown、证据优先、工艺实例、容量冲突与最早开始调度）；`time` 模块实现时间线/预测/库存最小值（5 测试），`schedule` 模块实现最早开始调度、前置环检测、容量冲突与"结构路线→时间计划"（7 测试，含 Base-01 S0/S1 与缺时长拒绝）。ontology 单元测试 44→56。
- 2026-10-31：M5.3/M5.4 完成：PO 增加 §3.9（能量/功率：电池容量/SoC/功率上限与裁剪原因、发电轨迹）与 §3.10（离散事件仿真：确定性 (时间,序列) 排序、固定/状态相关扰动、每 transfer 质量守恒、external 流、日志与终态）；新增 `crates/poie-sim`（battery + simulation），测试覆盖守恒、规则触发、电池裁剪、外部流与分块/一次运行 digest 一致。工作区测试新增 10 个；M5.5 的逐位重放与复现清单列为下一步。
- 2026-10-31：M5.5/M5.6 完成：PO 增加 §3.11（时间版场景、截止时间可为预测失效起点、结论类别 too_late/insufficient_resource/capacity_conflict、资源按步骤开始时刻检查、digest 逐位重放）；新增 `scenarios/base-01-timed/`（资产/库存时间线、20 个预登记用例 T0–T19、README、digests.toml），新增 `poie-timed` 运行器（生成/校验摘要）与 CI 步骤。首次运行 20/20 与预登记预期一致；`poie-sim` 测试 10→13，工作区 CI 增加时间版用例与重放校验。
