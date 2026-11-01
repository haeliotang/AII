# 外部依赖项 AI 预演任务进度（2026-11-01）

性质：以下产出全部是 **AI 预演**，用于给真实外部参与者准备起点；**不计入任何里程碑的退出条件**。分支 `status/ai-preview-external`。

| # | 事项 | 做法 | 产物 | 状态 |
| --- | --- | --- | --- | --- |
| 1 | M1.8 契约评审 | 3 个互不共享上下文的子代理，分别以工业自动化、分布式系统、运筹与实验统计视角找反例；不读作者自审 | `docs/status/ai-preview/contract-review-{a,b,c}.zh-CN.md` | 完成：36 条，去重 25 项，见汇总 |
| 2 | M3.3 外部用例 | 子代理不读已有用例，独立编写 10 例；预期先提交（815c2f5）再运行 | `scenarios/base-01/ai-preview-cases.toml`、`docs/status/ai-preview/base01-ai-cases-run.txt` | 完成：10/10 符合预期 |
| 3 | M1.10 商标检索补充 | 子代理补查普通法使用、包名、域名，核实 USPTO 50154930 | `docs/status/ai-preview/trademark-supplement.zh-CN.md` | 完成 |
| 4 | M9.2 真实适配器 | 子代理：本机 BaSyx AAS 容器 + 只读适配器 + 断连/重连 | `experiments/basyx-shadow/` | 完成：9/9 符合预期 |
| 5 | M12.6 真实多节点 | 两个 Docker 容器 + `docker network disconnect` 分区；PREREG 先提交（815c2f5） | `experiments/federation-docker/` | 完成：14/14 符合预期 |
| 6 | 汇总 | 更新状态文档、跑 `scripts/verify-all.sh`、提交 | 自主范围审计 §6 | 完成：verify-all 通过 |

不做的事项：M1.9（需发起方本人确认）、M9.0（需真实法务/合规）、M9.6/M10 物理部分与 M11.6（需实物试验台与建造资源）。

## 记录

- 2026-11-01：AP01–AP10 预期于 815c2f5 提交后首次运行，10/10 PASS。
- 2026-11-01：商标补充检索：普通法、crates.io/npm/PyPI/Docker Hub、6 个域名均无冲突；USPTO 50154930（POIE，cl. 9，表情/图像文件）经公开 statusview 页面核实为真实申请。
- 2026-11-01：评审 B（分布式系统）完成：12 条（高 4、中 7、低 1），B-01/03/04/05/06 用临时测试复现过；高：租约无 fencing、到期接管时结果未知动作不可见、去重窗口未绑定命令有效期、幂等键复用规则自相矛盾。
- 2026-11-01：评审 A（工业自动化）完成：12 条（高 5、中 7）。高：无质量输入绕开守恒（A-01，实跑复现）、仿真无全局质量核算与负库存检查（A-02，实跑复现）、幂等键复核与 outcome_unknown 不一致（A-03）、重复键回 NACK 语义错误（A-04）、VEC 把结构 blocked 当作全时域不可行（A-05）。A-03/A-04/A-05/A-11/A-12 与评审 B 独立重合。
- 2026-11-01：主线程独立核对 A-01/A-02 所指代码：sim Transfer 分支无非负检查；本体加载对 Unchecked 不阻断、mass_tolerance 允许 [0,1)。两条成立，触及研究纲领 §11 守恒暂停规则，需人工裁定。
- 2026-11-01：评审 C（运筹与统计）完成：12 条（高 5、中 7）。高：blocked 非不可行证明（C-01，与 A-05/B-11 三方独立重合，solve 与 enumerate 结论不一致已复现）、截尾分位数有偏、有限时域无终端条件、扩张不扣建造消耗、投运验证等级由提案者自定。C-11 与 A-01 的容差问题重合；C-12 与 A-03/B-04 重合。
- 2026-11-01：双容器分区预演首次运行 D0–D4 共 14/14 PASS（Docker 29.8.1，rust:1.94-slim-bookworm 构建）。未覆盖：时钟偏差、非对称分区、崩溃重启、认证加密；评审 B-01 的 fencing 问题也不在本实验范围内。
- 2026-11-01：BaSyx `2.0.0-milestone-15` 本机容器预演 9/9 PASS；局限：影子计划动作数为 0，不派发是平凡满足；预登记未先于运行单独提交。
- 2026-11-01：重放检查器新增 LIVE_RAW_EXPERIMENTS / LIVE_REPORTS 豁免；federation-docker 的判定仍在 CI 中从 raw 重算。`scripts/verify-all.sh` → ALL GATES PASSED。
