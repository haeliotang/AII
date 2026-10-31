# 两站点影子集成演示（simulated, in-process federation）

站点 A：现场观测 → 投影 → 规划；站点 B：共享库存与租约。派发以联邦对账结果为门禁。

**本演示为模拟/进程内联邦，不含真实多站点网络；真实多节点仍待 M12.6 外部环境。**

## 1. 站点 A：观测与投影

- 观测 9 条；pump-p1 = Degraded

## 2. 规划

- 动作数 1，完成 10800s

## 3. 联邦对账：超额预留

- 冲突：["over_reservation"]
- 是否阻断派发：是（over_reservation）

## 4. 联邦对账：干净预留与顺序租约

- 冲突：[]；允许派发：true

## 5. 联邦对账：重叠租约

- 冲突：["double_ownership"]；阻断派发：true

## 6. 派发与独立验证

- 派发：true；SAP 状态：acknowledged；验证来源：site-a-pump-status

## 7. 断言

- site A projection applied：PASS
- site A planned an action：PASS
- over-reservation blocks dispatch：PASS
- clean federation allows dispatch：PASS
- overlapping lease blocks dispatch：PASS
- dispatch succeeded：PASS
- independent verification：PASS
- 失败数：0
