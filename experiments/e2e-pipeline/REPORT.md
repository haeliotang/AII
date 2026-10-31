# 端到端管线演示（simulated）

只读适配器 → 投影（证据过期）→ 联合规划 → SAP 校验 → 执行门（接管/降级）→ 模拟执行 → 独立验证 → 预测核对。

**本演示为模拟现场，不含真实设备；用于证明各 crate 可组合且不存在绕过授权/验证的路径。**

## 1. 观测与投影

- 观察数：9，断连适配器：0
- pump-p1 投影状态：Degraded

## 2. 规划

- 计划 1 个动作；完成 10800s

## 3. SAP 校验、派发与独立验证

- 派发结果：Dispatched { receipt: Receipt { action_id: "e2e-a1", accepted: true, executed_at_ms: 0, detail: "executed" } }
- SAP 状态：acknowledged；独立验证来源：pump-status-sensor

## 4. 人工接管演练

- 接管期间结果：RecordedOnly { reason: "human takeover is active" }；物理执行 0

## 5. 安全降级演练

- 降级期间结果：Fallback { reason: "runtime is degraded; site controls take over" }；物理执行 0

## 6. 预测核对

- pump-p1：窗口内 Some(true)，误差 Some(0.0)，截尾 false
- bus-1：窗口内 Some(false)，误差 Some(25200.0)，截尾 false
- lathe-1：窗口内 None，误差 None，截尾 true
- 汇总：观测 2，截尾 1，窗口内 1，MAE Some(12600.0) s

## 7. 断言

- projection applied：PASS
- plan produced an action：PASS
- dispatch succeeded：PASS
- independent verification：PASS
- takeover recorded only：PASS
- degradation fell back：PASS
- prediction inside window：PASS
- prediction outside window：PASS
- prediction censored：PASS
- 失败数：0
