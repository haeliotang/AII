# 契约 issue 修复进度（#106–#111，2026-11-01）

来源：AI 预演评审的裁定（[评审汇总](../status/ai-preview/contract-review-summary.zh-CN.md)）。#105 已由 PR #114 修复；#112 推迟。

做法：4 个子代理各在独立的 worktree 中修复，主线程审查后依次集成。共享文件由主线程在集成时统一更新：测试矩阵计数、状态快照计数、DEVELOPMENT 修订记录、foundation-tasks。

| 组 | Issue | 范围 | 分支 | 状态 |
| --- | --- | --- | --- | --- |
| A | #106 | VEC §3.3：`blocked` 的语义；solve 与 enumerate 不一致；vec-base01 报告 | `fix/106-blocked-semantics` | 已审查并集成 |
| B | #107、#108 | SAP 幂等键、回执、去重窗口；租约 fencing、接管余量、移交 | `fix/107-108-sap-execution-safety` | 已审查并集成 |
| C | #109、#110 | VEC 扩张核算；统计与报告规则；H2/H4 重跑 | `fix/109-110-vec-accounting-stats` | 已审查并集成 |
| D | #111 | 状态与时间语义：证据过期、投影时间、序号复用、预留释放 | `fix/111-state-time-semantics` | 已审查并集成 |
| — | 集成 | 依次合并、更新计数与修订记录、`verify-all`、开 PR | — | 待办 |

## 记录

- 2026-11-01：派出 A–D 四个子代理。
- 2026-11-01：A（#106）完成并审查：3 个提交，预登记测试（91ee896）先于修复。PO §6.3 新增规则 17：solve 失败后回退到与 enumerate 相同的 64 次轮转尝试；VEC §3.2/§3.3 规定 `blocked` 不能单独作为 `infeasible_in_model` 的证书。vec-base01 中 T6/T11/T13/T18 从 `infeasible_in_model` 改为 `counterexample`（`structural_route`），变更记录见实验 README；其他实验重放无变化。测试：本体 87→89、可行性 38→39、vec-base01 1→2。集成时需同步：DEVELOPMENT M6 节与 foundation-tasks 中「4 `infeasible_in_model`」的历史表述。
- 2026-11-01：D（#111）完成并审查：预登记 35feff5 先于修复 deab40f。PO §3.1/§3.8/§4：证据过期才是 unknown，预测只产生警告；SAP §2.3 投影显式 `as_of`，§2.1/§5 序号高水位防止回滚复用，§6.2 预留生命周期（含 in_doubt），§6.4 `sequence_collision`；ADR-0003 补充。时间版场景 schema 升到 0.2（`evidence_validity = 24 h`），20 个用例结论不变；vec-base01 只有两个输入哈希变化。测试：本体 +4、state +6、sim +2、federation +2。集成注意：与 #106 都改了 vec-base01/report.toml（需重新生成）；SAP §6.2「NACK 释放预留」需与 #107 的回执语义对齐。
- 2026-11-01：B（#107+#108）完成并审查：预登记 6e66f1e 先于修复。三种回执（ACK/NACK/DUPLICATE）、统一键占用规则（SAP §3.4）、派发前复核增至七项、租约纪元 fencing、接管等待 `until+B`、到期接管前对账；ADR-0007 补充。所有实验产物不变。核对：模拟适配器 `executed_keys` 只增不删，`dedup_window_ms = u64::MAX` 属实。测试：state +18、exec +6、federation +1。
- 2026-11-01：开始集成 A、B、D 到 `integrate/contract-fixes`。
- 2026-11-01：A、B、D 已合入 `integrate/contract-fixes`。冲突：federation.rs（两边都保留：#111 的序号碰撞为第 0 步，#107 的跨场址键检查为第 1 步）、SAP 头部与 §6.4 第 3 条（合并两边条文）、PO 头部版本行；语义冲突：#106 测试补 `TimedOutcome.warnings`。SAP §6.2 与 #107 的 NACK 新语义一致，无需改。verify-all：只剩测试计数不匹配（库 333、总 349）。等待 C。
- 2026-11-01：C（#109+#110）完成并集成：预登记 5ce5a9a 先于修复。H4 结论由「不确定」改为「净增益为负」（建造占用唯一维修机器人导致建造期失去供水恢复路线；16/16 场景储能付不起 20 kWh 建造），旧结论保留。主线程补充：H4 报告写明 PREREG §6「不确定」条件同样成立、PREREG 未定优先级、为何仍判负。与 #106 的冲突：采用 #110 的方法×结论兼容表（`set_based` 可在写明保守方向时给出 `infeasible_in_model`），保留 #106 的「`blocked` 不能单独作为证书」；`report.rs` 统一用 `Method::allows`。vec-base01 报告重新生成，结论与 C 分支一致。共享文档已同步（计数 384、H4 结论、DEVELOPMENT 0.1.100）。
