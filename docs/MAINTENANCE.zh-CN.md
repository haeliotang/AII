# 维护手册

面向维护者与外部评审者：如何验证、如何升级、如何新增实验、如何处理外部评审意见。日常命令以 `scripts/` 为准，文档若与脚本冲突，以脚本与 CI 为准。

## 1. 门禁（每次合并）

CI 在 `main` 每次推送/合并时运行：

1. `cargo fmt --all --check`
2. `cargo clippy --all-targets -- -D warnings`（工具链固定 1.94.0）
3. `cargo test --locked`
4. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`
5. `python3 -m unittest discover -s python/poie_models -p "test_*.py"`
6. Markdown 相对链接、实验重放覆盖、测试矩阵计数三项检查
7. 场景用例（Base-01 40、Base-02 8）、时间版用例与摘要
8. 全部实验输出重放（VEC、H1 结构层/时间版、H2 开发/留出、H3、H4、联邦分区、e2e、TCP、两站点、影子运行）
9. `cargo-deny`（advisories / licenses / bans / sources）

本地等价命令：

```bash
scripts/verify-all.sh          # 全部步骤
scripts/check-fresh-clone.sh   # 干净克隆内再跑一遍（约 1 分钟，里程碑后/评审前使用）
```

**合并规则**：PR 必须 CI 全绿再合并；合并后再确认 `main` 的最新 CI run 成功。若合并后才发现失败，立即开修复 PR（fix-forward），不等待。

**定期验证**：`.github/workflows/scheduled.yml` 每周一 04:17 UTC（也可手动 `workflow_dispatch`）运行完整的 `verify-all.sh`（跳过 deny，由独立 job 执行）与 `cargo-deny` 两个检查；advisories 只报告不阻塞（`continue-on-error`），许可/禁用/来源违规会失败。它用于捕获"新发布的公告/依赖来源变化"，不替代每次合并的 CI。

**手动触发额外 job**：`workflow_dispatch` 时还会运行 `fresh-clone` job——在 runner 上执行 `scripts/check-fresh-clone.sh`（干净克隆内重跑完整套件），用于在没有本地环境时验证证据可复现；定时触发不会运行该 job（成本较高）。

## 2. 工具链

- CI 固定 `dtolnay/rust-toolchain@1.94.0`；本地参考环境同为 1.94.0。
- MSRV **未固定**：代码使用 1.82 的 `Option::is_none_or` 等 API，更旧版本未验证（见 `CONTRIBUTING.md`）。
- 升级工具链的流程：
  1. 新建分支只改 `.github/workflows/ci.yml` 的版本号；
  2. 本地同版本跑 `scripts/verify-all.sh`（无该版本时至少跑 CI）；
  3. 检查 clippy/fmt 新告警并修复，不改行为；
  4. 合并后更新 `CONTRIBUTING.md` 与 `docs/DEVELOPMENT.zh-CN.md` 修订记录；
  5. 需要时刷新 `docs/status/fresh-clone-repro.zh-CN.md` 的验证提交与工具链。
- 历史教训：本地 1.94 与 CI 1.99 曾两次因新 lint 漂移导致"本地绿、CI 红"；固定版本后不再漂移。

## 3. 依赖升级

1. `cargo update -p <crate>`（或全量 `cargo update`），提交 `Cargo.lock`；
2. `cargo test --offline`、`cargo deny check licenses bans sources`、`scripts/verify-all.sh`；
3. 检查 `deny.toml` 的许可/禁用/来源策略是否需要调整（新增依赖不得放宽策略）；
4. 若涉及 `serde`/`toml` 等数据格式依赖，额外重跑所有实验重放；
5. 在 PR 描述中写清升级原因与验证命令。

## 4. 新增场景或实验（强制流程）

1. **先登记**：在 `experiments/<id>/PREREG.md` 写清问题、基线、指标、用例来源、留出集与决策规则；登记必须**先于任何运行**提交。
2. 运行产物必须**确定性**（固定顺序/种子/浮点容差）；把报告与原始输出提交。
3. 在 `.github/workflows/ci.yml` 与 `scripts/verify-all.sh` 加 diff 重放；若输出依赖日期/机器，改为固定输入或明确标注不可重放并加入检查器白名单。
4. 运行 `python3 scripts/check-experiment-replays.py`，确认新产物被覆盖。
5. 更新 `docs/status/test-matrix.zh-CN.md`（规模与重放状态）、`README.md`、`docs/status/stage-gates.zh-CN.md` 与 `docs/progress/foundation-tasks.md`。
6. 报告必须如实包含失败/超时/不可行/数值失败；负结果同等呈现。

## 5. 新增测试

1. 单元/集成测试放在 crate 内；随机性质测试用固定种子并写明 trials。
2. 新增测试后运行 `python3 scripts/check-test-matrix.py`，同步更新矩阵中的 crate 计数与合计。
3. 涉及契约行为的测试应引用契约条款；发现实现缺陷时先修实现，再保留回归测试。

## 6. 文档变更

```bash
python3 scripts/check-docs.py          # 相对链接必须可解析
python3 scripts/check-test-matrix.py   # 计数必须与 cargo test --list 一致
python3 scripts/check-experiment-replays.py
```

- 契约修订必须升版本、写修订记录，并同步 ADR 与 `docs/status/contract-self-audit.zh-CN.md`。
- 不要用"弱化预期"的方式让用例通过；修正预期必须记录原因（见 `docs/DEVELOPMENT.zh-CN.md` §1.4）。

## 7. 外部评审与外部行动

1. 评审意见使用 `.github/ISSUE_TEMPLATE/contract-review.yml` 登记；每条给出**接受 / 拒绝（附理由）/ 推迟（附重开条件）**。
2. 按其影响更新契约正文、版本、ADR 与自审问题包；被接受的条款需要新增或更新测试。
3. 外部行动清单与可直接发送文本：`docs/status/external-actions.zh-CN.md`、`docs/status/outreach-templates.zh-CN.md`。
4. 涉及法务/合规/商标/设备的事项由发起方决策，仓库只提供材料与验收标准。

## 8. 发布前检查清单

- [ ] `scripts/verify-all.sh` 全绿
- [ ] `scripts/check-fresh-clone.sh` 全绿
- [ ] `main` 最新 CI run 成功
- [ ] `docs/DEVELOPMENT.zh-CN.md` 修订记录已更新
- [ ] `docs/status/stage-gates.zh-CN.md`、`test-matrix.zh-CN.md` 与实际情况一致
- [ ] 所有新增实验均有重放，或已在检查器白名单说明原因
- [ ] 外部阻塞项清单未过期（`external-actions`）

## 9. 故障处理

- **CI 红**：先看是否为工具链/依赖漂移，再本地复跑最小命令；修复用独立 PR（fix-forward）。
- **实验重放不一致**：确认是否改了确定性输入/算法；若是预期变更，必须重新生成并记录原因；禁止只更新提交文件而不管生成器。
- **链接/计数检查失败**：修复路径或同步计数；不要关闭检查。
- **安全漏洞**：按 `SECURITY.md` 私下报告，不在公开 issue 披露细节。
