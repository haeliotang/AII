# 联邦：双容器网络分区预演（AI 预演 · 本机 Docker · 非真实多节点）

**两个站点各运行在一个 Docker 容器中，用 `docker network disconnect` 制造真实的网络层分区。两个容器共享宿主机内核与时钟，不是真实多节点；本结果不计入 M12.6 退出条件。** 预登记见 [PREREG.md](PREREG.md)。

| 场景 | 站点 | 观测 | 判定 |
| --- | --- | --- | --- |
| D0 联通基线 | site-a | 成功 2 / 失败 1；记录 0；预留 0；租约 0；冲突 0 条 [] | PASS |
| D0 联通基线 | site-b | 成功 3 / 失败 0；记录 0；预留 0；租约 0；冲突 0 条 [] | PASS |
| D1 分区检测 | site-a | 窗口内成功 +0，失败 +13 | PASS |
| D1 分区检测 | site-b | 窗口内成功 +0，失败 +12 | PASS |
| D2 分区期间写入 | site-a | 成功 2 / 失败 22；记录 1；预留 1；租约 1；冲突 0 条 [] | PASS |
| D2 分区期间写入 | site-b | 成功 3 / 失败 22；记录 1；预留 1；租约 1；冲突 0 条 [] | PASS |
| D3 重连对账 | site-a | 成功 4 / 失败 23；记录 2；预留 2；租约 2；冲突 3 条 [double_ownership, duplicate_idempotency_key, over_reservation] | PASS |
| D3 重连对账 | site-b | 成功 6 / 失败 22；记录 2；预留 2；租约 2；冲突 3 条 [double_ownership, duplicate_idempotency_key, over_reservation] | PASS |
| D4 反复分区 第 1 轮 | site-a | 成功 7 / 失败 31；记录 2；预留 2；租约 2；冲突 3 条 [double_ownership, duplicate_idempotency_key, over_reservation] | PASS |
| D4 反复分区 第 1 轮 | site-b | 成功 9 / 失败 30；记录 2；预留 2；租约 2；冲突 3 条 [double_ownership, duplicate_idempotency_key, over_reservation] | PASS |
| D4 反复分区 第 2 轮 | site-a | 成功 10 / 失败 39；记录 2；预留 2；租约 2；冲突 3 条 [double_ownership, duplicate_idempotency_key, over_reservation] | PASS |
| D4 反复分区 第 2 轮 | site-b | 成功 11 / 失败 39；记录 2；预留 2；租约 2；冲突 3 条 [double_ownership, duplicate_idempotency_key, over_reservation] | PASS |
| D4 反复分区 第 3 轮 | site-a | 成功 13 / 失败 47；记录 2；预留 2；租约 2；冲突 3 条 [double_ownership, duplicate_idempotency_key, over_reservation] | PASS |
| D4 反复分区 第 3 轮 | site-b | 成功 14 / 失败 47；记录 2；预留 2；租约 2；冲突 3 条 [double_ownership, duplicate_idempotency_key, over_reservation] | PASS |

## 判定

- 期望冲突类别：double_ownership, duplicate_idempotency_key, over_reservation
- 失败数：0
- 结论：双容器网络分区预演全部符合预登记期望

## 不覆盖

时钟漂移与偏差、非对称分区、丢包与乱序、崩溃重启与持久化、认证与加密、真实路由与广域网。见 PREREG §5。
