# 影子运行演示（M9.4，simulated field）

投影现场状态 → 影子规划（不派发）→ 与模拟操作者日志对比。

**本演示为模拟现场与模拟操作者日志，不含真实设备；用于验证 M9.4 的库级路径。**

## 1. 观测与投影

- 观察 9 条；pump-p1 = Degraded，lathe-1 = Failed

## 2. 影子规划与对比

- 计划动作 1 个；`dispatched` = false（必须为 false）
  - replace-sleeve-p1 → site @ 0s
  - 分歧 would_do_not_observed: planned=Some(("replace-sleeve-p1", "site")) operator=None
  - 分歧 observed_not_planned: planned=None operator=Some(("manual-inspection", "pump-p1"))

## 3. 断言

- projection applied：PASS
- shadow plan records actions：PASS
- shadow never dispatched：PASS
- would_do_not_observed reported：PASS
- observed_not_planned reported：PASS
- dispatch guard rejects shadow dispatch：PASS
- recorded-only run has no plan actions：PASS
- 失败数：0
