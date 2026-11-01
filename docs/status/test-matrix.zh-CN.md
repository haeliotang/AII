# 测试与证据矩阵（2026-11-01）

本表把"代码测试、随机性质测试、场景/实验重放、CI 步骤"与里程碑对应起来，便于外部评审快速核对。数字取自 `cargo test -- --list` 与 CI 配置；随机性质测试在列表中计为 1 个测试，但内部运行多次随机试验（trials）。

## 1. 库 crate

| crate | 列出测试数 | 其中随机性质测试 | 主要里程碑 |
| --- | ---: | --- | --- |
| `poie-ontology` | 83 | 图求解 120 trials；调度 120 trials；TOML 变异不 panic（120×2）；单位换算/解析 200×2；路线枚举 120+80 | M2、M4 |
| `poie-state` | 73 | 状态机 10⁵ 序列（单元内）；投影 120 trials；记录验证/往返 120 trials；日志重开/崩溃尾部随机测试 | M4、M5、M12 |
| `poie-sim` | 14 | 仿真 80 trials | M4、M6 |
| `poie-viability` | 38 | 指标 60 轨迹；报告 20 份 × 16 类变异 | M6 |
| `poie-plan` | 22 | 规划器 80 trials；重规划/基线 80+20+60 trials | M8 |
| `poie-process` | 7 | Rust 客户端 40 trials（故障/超时分类）；Python 侧 9 个 unittest | M7 |
| `poie-runtime` | 13 | 运行时/断连 80 trials | M9 |
| `poie-exec` | 13 | 执行门 120 trials | M10 |
| `poie-expansion` | 11 | 结构 40/时序 40 trials；提案 TOML 变异不 panic（120） | M11 |
| `poie-federation` | 10 | HLC 80 trials；分区/重连 120 trials | M12 |

**库 crate 测试合计：284 个已列出的测试**；另有其他包 15 个（`poie-h1-baseline` 5、`poie-vec-base01` 1、`poie-basyx-shadow` 9），总计 299。

## 2. 场景与实验重放

| 证据 | 规模 | CI 重放 |
| --- | --- | --- |
| Base-01 预登记结构用例 | 40 例 | ✅ |
| Base-02 共享仓库用例 | 8 例 | ✅ |
| Base-01 时间版 + 重放摘要 | 20 例 | ✅ |
| VEC v0.2 报告 | 20 runs / 78 metrics | ✅ |
| H1 结构 / 时间版报告 | 两实验 | 全量/开发集/留出集/时间版原始输出 CI 重放 |
| H2 联合规划（dev + holdout） | 负结果报告 | 开发集/留出集报告与原始输出 CI 重放（留出集重放仅复现，不用于开发） |
| H3 有界执行（模拟） | 14/14 | ✅ |
| H4 扩张核算 | 判定不确定 | ✅ |
| M12.6 分区/重连（模拟） | 6/6 场景 | ✅ |
| 端到端管线 | 9/9 断言 | ✅ |
| 本机 TCP 双进程联邦 | 2 站点 × 3 类冲突 | ✅ |
| 两站点影子集成 | 7/7 断言 | ✅ |
| M9.4 影子运行演示 | 7/7 断言（不派发、两类分歧） | ✅ |
| BaSyx AAS 只读适配器预演（M9.2，AI 预演） | 9/9 检查 | 非 CI 重放（live 容器；报告标注 AI 预演） |
| 双容器网络分区预演（M12.6，AI 预演） | 14/14 | 判定从 `raw/` 在 CI/`verify-all.sh` 重放 |
| AI 预演 Base-01 用例（M3.3，AI 预演） | 10 例 / 10 通过 | 运行输出提交，未纳入 CI；不占外部配额 |
| Python 水模型/桥 | 9 个 unittest | ✅ |

## 3. CI 步骤（`main` 每次合并）

1. `cargo fmt --all --check`
2. `cargo clippy --all-targets -- -D warnings`（工具链固定 1.94.0）
3. `cargo test --locked`
4. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`
5. Markdown 相对链接检查：`python3 scripts/check-docs.py`
6. 实验重放覆盖检查：`python3 scripts/check-experiment-replays.py`
7. 测试矩阵与状态快照计数校验：`python3 scripts/check-test-matrix.py`
8. Python：`python3 -m unittest discover -s python/poie_models`
9. Base-01 40 例、Base-02 8 例
10. M12 分区模拟重放、双容器分区判定重放、e2e 重放、TCP 重放、两站点重放、影子运行重放
11. Base-01 时间版用例与摘要校验
12. VEC 报告重放、H1 结构层/时间版重放、H2 开发集/留出集重放、H3 模拟重放、H4 报告重放
13. `cargo-deny`（advisories、licenses/bans/sources）

本地一键复现：`scripts/verify-all.sh`（与 CI 相同的步骤序列）；干净克隆复现：`scripts/check-fresh-clone.sh`。

AI 预演（2026-11-01）列在上表最后三行：它们为外部参与者提供起点，**不改变任何里程碑的退出条件**；限制见 [autonomous-scope-audit §6](autonomous-scope-audit.zh-CN.md)。

另：`.github/workflows/scheduled.yml` 每周一 04:17 UTC（或手动触发）运行 `verify-all.sh`（`SKIP_DENY=1`）、`cargo test --release` 与独立的 `cargo-deny` job；手动触发时还会运行 `fresh-clone` job（干净克隆内重跑完整套件）。

## 4. 对应关系

- **Foundation（M1–M5）**：契约文档 + Base-01/02 用例 + 状态机/SAP 验收 + 投影/调度/图求解性质测试；外部阻塞项见 [external-actions](external-actions.zh-CN.md)。
- **Autonomy（M6–M8）**：VEC 指标/方法/报告、H1/H2 报告、规划器性质测试、Python 过程桥。
- **Embodiment（M9–M12）**：运行时/执行门/扩张核算/联邦性质测试，H3/H4/分区模拟与端到端/两站点/TCP 集成演示；物理与真实多站点部分待外部条件。
