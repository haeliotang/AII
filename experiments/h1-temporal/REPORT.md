# H1 时间版复测：报告（REPORT）

- 实验：`h1-temporal` · 预登记：[PREREG.md](PREREG.md) · 用例：[Base-01 时间版](../../scenarios/base-01-timed/README.zh-CN.md)
- 报告日期：2026-10-31
- 结论：按预登记规则，**H1 时间版子集在本实验中被支持（弱形式）**：时间盲基线漏掉 3/7 个时间类阻断；时间版方法自身错误 0/20。但样本量 20、作者相同、无留出集，只能作为内部证据（§6）。

## 1. 方法

- **Poieweave 时间版**：结构求解 → 调度 → 截止检查 → 逐步资源检查（PO §3.11）。
- **时间盲基线**：M3 的逐设备检查基线，结构 candidate 即视为可行；忽略时长、截止、预测与资源时间线。
- **真值**：20 个预登记时间版用例 T0–T19（先于运行写定）。

## 2. 复现清单

| 项 | 值 |
| --- | --- |
| 代码提交 | `6f92f70`（runner 与预登记所在提交；运行原始输出基于同一提交） |
| 工具链 | `rustc 1.94.0 (4a4ef493e 2026-03-02)` |
| 机器 | Apple M1 Pro（arm64），macOS |
| 命令 | `cargo run --release -p poie-h1-temporal -- scenarios/base-01-timed/cases.toml` |
| 原始输出 | [`raw/dev-2026-11-01.txt`](raw/dev-2026-11-01.txt) |

## 3. 结果

- 用例 20：一致 12，时间漏判 3/7，联合依赖漏判 2/5，误判 3/8，**Poieweave 时间版与真值不一致 0/20**。

| 类别 | 计数 | 说明 |
| --- | --- | --- |
| 时间漏判 | 3/7 | T1、T9、T16：基线只看结构，因此"晚启动 / 超截止"不可见 |
| 联合依赖漏判 | 2/5 | T11、T18：母线故障后的机器人—充电桩循环，逐设备检查看不到 |
| 误判 | 3/8 | T3、T17、T19：需要加工/多来源的可行路线，基线因"不建模生产"判为阻断 |
| 一致 | 12/20 | 包括同为不可行但类别不同的 T2/T4/T10/T15（基线因库存不足判 blocked） |

### 逐用例结果

| 用例 | 真值 | Poieweave | 时间盲基线 | 分类 |
| --- | --- | --- | --- | --- |
| T0 | feasible | feasible | feasible | agree |
| T1 | too_late | too_late | feasible | temporal_miss |
| T2 | insufficient_resource | insufficient_resource | blocked | agree |
| T3 | feasible | feasible | blocked | false_block |
| T4 | too_late | too_late | blocked | agree |
| T5 | feasible | feasible | feasible | agree |
| T6 | blocked | blocked | blocked | agree |
| T7 | unknown | unknown | unknown | agree |
| T8 | feasible | feasible | feasible | agree |
| T9 | too_late | too_late | feasible | temporal_miss |
| T10 | insufficient_resource | insufficient_resource | blocked | agree |
| T11 | blocked | blocked | feasible | joint_miss |
| T12 | feasible | feasible | feasible | agree |
| T13 | blocked | blocked | blocked | agree |
| T14 | feasible | feasible | feasible | agree |
| T15 | insufficient_resource | insufficient_resource | blocked | agree |
| T16 | too_late | too_late | feasible | temporal_miss |
| T17 | feasible | feasible | blocked | false_block |
| T18 | blocked | blocked | feasible | joint_miss |
| T19 | feasible | feasible | blocked | false_block |

## 4. 结论

按 PREREG §5 的决策规则：时间漏判 3 > 0，Poieweave 时间版错误 0，且误判集中在"需要生产/多来源"的用例（T3/T17/T19），与漏判的失效模式不同——因此 **H1 时间版子集在本实验中被支持（弱形式）**。

支持的具体内容是：时间盲的逐设备调度会漏掉两类阻断——
1. 时间类：完成晚于预测失效/截止（T1/T9/T16）；
2. 随时间变化的资源：夜间能量不足、补给中断（T2/T10/T15 由时间版识别；基线虽判 blocked，但原因不同）。

## 5. 方法自身的错误

- Poieweave 时间版与真值不一致：**0/20**。
- 未发现时间版实现的缺陷；运行器在首次运行即与全部预登记预期一致。

## 6. 局限

1. **无留出集**：这是对 M3 方法的内部时间版复测，20 个用例、作者相同；结论不构成独立验证（与 M3、M5.6 的局限一致）。
2. **基线定义偏弱**：时间盲基线也不建模生产，因此误判率（3/8）较高；漏判应理解为"结构可行但时间/资源不可行"这一特定失效模式。
3. **规模小**：n=20，未给统计显著性；只作方向性判断。
4. **外部配额仍未满足**：M3.3 的外部用例配额（≥1/4）依然为 0，M5.7 沿用该限制。

## 7. 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-10-31 | 初版：首次（也是最终）运行；含时间漏判/联合依赖漏判/误判与方法自身错误 |
