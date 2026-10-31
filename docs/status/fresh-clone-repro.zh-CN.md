# 全新克隆复现验证（2026-10-31）

本文件记录一次**从干净克隆运行完整验证套件**的结果，证明仓库中声称的证据不依赖本地未提交状态或构建缓存之外的隐藏状态。

## 命令

```bash
scripts/check-fresh-clone.sh
```

脚本在仓库内创建临时目录、`git clone --no-hardlinks` 当前仓库（仅已提交内容），然后在克隆内运行 `scripts/verify-all.sh`。临时目录在结束时删除。

## 结果

| 项 | 值 |
| --- | --- |
| 验证提交 | `a627cf5`（`main`；`a6413e8` 亦通过） |
| 工具链 | `rustc 1.94.0`，CI 同样固定 1.94.0 |
| 环境 | Apple M1 Pro / macOS；使用本机 cargo registry 缓存（`--offline`），不依赖网络 |
| 运行时长 | 约 62 秒（含全新 `target/` 构建与全部重放） |
| 结果 | `ALL GATES PASSED` + `FRESH CLONE REPRODUCTION PASSED` |

CI runner 上亦已复现：手动 dispatch scheduled workflow 的 [run 41245856574](https://github.com/haeliotang/poieweave/actions/runs/41245856574) 中 `fresh-clone` job 成功（Ubuntu runner、干净克隆内运行完整套件）。

覆盖内容与 `verify-all.sh` 相同：格式、clippy、全部 Rust/Python 测试、rustdoc（警告即错误）、Markdown 链接、实验重放覆盖、测试矩阵计数、Base-01/Base-02、时间版与摘要、VEC 报告、H1 结构层/时间版、H2 开发/留出、H3、H4、联邦分区、e2e、TCP、两站点、影子运行、`cargo-deny`。

## 边界（不要过度解读）

- 不是**完全封闭/无网络**的可复现构建：依赖来自本机 cargo registry 缓存；CI 的干净 runner 是另一层证据。
- 未验证跨平台/跨架构（仅 macOS arm64）。
- 工具链固定在 1.94.0；MSRV 仍未声明（见 CONTRIBUTING）。
- 该检查运行时间较长，不在每次 CI 中执行；需要时手动运行。

## 用途

- 外部评审者可先在干净副本上运行一次，再阅读报告；
- 每个里程碑合并后可按需重跑，作为"证据可复现"的独立确认。
