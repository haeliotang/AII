# 外部用例贡献模板（M3.3）

目标：让非项目作者为 Base-01 贡献**预登记**结构用例，使外部用例配额达到 ≥1/4。

## 1. 选择场景与标签

在 `scenarios/base-01/cases.toml` 末尾追加用例；`tags` 只能取现有分层之一：
`common-cause`、`cycle`、`stock-contention`、`quality-gate`、`verification`、`unknown`、`other`。

## 2. 编写用例（示例）

```toml
[[case]]
id = "S40"
title = "一句话说明你预期的阻断机理"
description = """
为什么这个用例重要；如果预期是 blocked，根因是什么。"""
goal = { service = "potable-water" }   # 或 restore = "..." / obtain = { resource = "...", quantity = "..." }
min_verification = "demonstrated"
tags = ["cycle"]
[case.patch]
asset_status = { "lathe-1" = "failed" }
[case.expect]
verdict = "blocked"
root_causes = [{ kind = "cycle", subject = "lathe-1" }]
```

## 3. 预登记规则

1. **先写预期，再运行**；预期一旦提交，运行后不得为了让结果通过而修改。
2. 若运行结果与预期不符：在 PR 中说明是**模型问题**还是**预期错误**；只有后者可以改预期，并必须记录原因。
3. 每个用例必须能说明"为什么这个预期是正确的"（可引用契约条款或既有用例）。

## 4. 提交前检查

```bash
cargo run -p poie-ontology --bin poie-graph -- scenarios/base-01/cases.toml
```

- 加载器通过（id 唯一、tags 合法、patch 字段合法）；
- 全部用例 PASS；
- 在 PR 描述中声明：**本人不是项目作者**、用例为独立编写、预期在运行前固定。

## 5. 验收

- ≥10 个外部贡献用例（40 例的 1/4）；
- 每个用例有预登记预期与通过记录；
- `docs/progress/foundation-tasks.md` 的外部配额更新为 "≥1/4 已满足"。
