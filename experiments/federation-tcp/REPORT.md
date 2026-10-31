# 联邦：本机 TCP 双进程交换演示（localhost, two processes）

**本演示是两个本地进程通过真实 TCP socket 交换 JSON 消息；它不是真实多站点部署，只用于缩小 M12.6 的网络差距。**

两个站点各自产生：一条使用同一幂等键的 action、一条预留（4 kg + 3 kg，超过 5 kg）、一条重叠租约。

| 站点 | 合并记录数 | 对账冲突 | 判定 |
| --- | --- | --- | --- |
| site-a | 2 | double_ownership, duplicate_idempotency_key, over_reservation | PASS |
| site-b | 2 | double_ownership, duplicate_idempotency_key, over_reservation | PASS |

## 判定

- 期望冲突：double_ownership, duplicate_idempotency_key, over_reservation
- 失败数：0
- 结论：本机 TCP 交换与重连对账符合预期
