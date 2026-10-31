# Foundation 阶段任务进度

开始：2026-10-31 · 顺序：三契约 → 本体+能力图原型 → Base-01 场景规范

## 当前快照（2026-10-31）

M1 工作包 10 个，已完成 7 个（M1.1–M1.7），剩余 3 个（M1.8–M1.10）。
权威清单与退出条件见[开发文档 §2](../DEVELOPMENT.zh-CN.md#m1-契约草案与结构原型--基本完成)。

| 工作包 | 状态 | 证据 / 备注 |
| --- | --- | --- |
| M1.1–M1.3 三份契约 v0.1-draft | 完成 | `specifications/` |
| M1.4 能力图原型 | 完成 | `crates/poie-ontology`，38 个单元测试 |
| M1.5 Base-01 结构场景 | 完成 | 21 个预登记用例全部一致；14 个集成测试 |
| M1.6 持续集成 | 完成 | PR #2；`main` CI [run #2](https://github.com/haeliotang/poieweave/actions/runs/41222603966) 为绿 |
| M1.7 贡献指南 | 完成 | `CONTRIBUTING.md`（DCO、测试门、契约变更流程）、`DCO`、`SECURITY.md`、三类 issue 模板；GitHub 私有漏洞报告已启用 |
| M1.8 契约外部评审 | 未开始 | 需外部读者审读并登记 issue |
| M1.9 开源边界 | 部分 | 配套文件已齐（`NOTICE`、`DCO`、`SECURITY.md`、`cargo-deny`）；ADR-0001 仍为"提议"，待发起方确认 |
| M1.10 名称落地 | 部分 | 改名已完成；初步商标检索已记入 [ADR-0002 §7](../adr/0002-rename-to-poieweave.zh-CN.md)，官方库直连与类别专项检索待人工复核 |

M2 本体 v0.2：**M2.1–M2.5 完成**（OR 支撑、多来源补足、降级使用策略、保守合格率与返工、能力验证等级）——契约 §3.1 / §3.4 / §3.6 / §6.3 规则 9–13 / §6.4 加入 `requires_any`、`no_alternative_support`、组合补足、`degraded_restriction`、`yield` 与 `capability_below_verification`；实现、25 个新增单元测试、S11–S20 预登记用例；M2.6–M2.8 未开始。

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
