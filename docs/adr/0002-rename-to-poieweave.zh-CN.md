# ADR-0002：项目由 AII 改名为 Poieweave

- 状态：**已接受**（仓库内改名与 GitHub 仓库改名已完成；商标检索待办）
- 日期：2026-10-31
- 决策者：Inceptless
- 相关：[ADR-0001 开源边界](0001-open-source-boundary.zh-CN.md) §5 商标 · [开发文档](../DEVELOPMENT.zh-CN.md) M1.10

## 1. 背景

项目原名 AII（Autonomous Infrastructure Intelligence）。这个名字有三个实际问题：

1. **难以辨认**：在多数无衬线字体中，`AII`、`All`、`AIl` 几乎无法区分。
2. **难以检索**：三个字母的缩写在搜索引擎和包仓库中很难被准确找到，也容易与其他机构的缩写混淆。
3. **含义偏窄**："Infrastructure Intelligence"容易被理解为 IT 设施运维或智慧城市产品，没有表达出"自我维持、修复与生产"这一核心问题。

改名时仓库刚公开 3 天：没有 star、fork 和外部引用，也没有发布任何包，改名成本最低。

## 2. 命名规则

沿用 Inceptless 模块统一的 **`<希腊或拉丁词根> + weave`** 规则：词根说明模块的本质职能，weave 表示把多方面的记录与依赖编织成一个整体。

## 3. 候选与筛选

2026-10-31 在 crates.io、PyPI、npm、GitHub（仓库、用户名和组织名）和 .com 域名上做了查询：

| 候选 | 词根 | 软件平台 | .com | 结论 |
| --- | --- | --- | --- | --- |
| **Poieweave** | poiesis（制造）；autopoiesis（自我生产与维持） | 均未占用 | 未注册 | **采用** |
| Ergoweave | ergon（工作、功能） | 均未占用 | 未注册 | 备选：好读，但含义不够具体 |
| Physiweave | physis（自然、物理） | 均未占用 | 未注册 | 只表达"物理"，不表达"生产" |
| Autarkweave | autarkeia（自给自足） | 均未占用 | 未注册 | 有经济封闭的含义；与"完全自给是未解问题"的立场冲突，有夸大之嫌 |
| Homeoweave | homeostasis（稳态） | 均未占用 | 未注册 | 只表达"维持"，不表达"扩张" |
| Techneweave / Dynamiweave / Poiesiweave | techne / dynamis / poiesis | 均未占用 | 未注册 | 拼读更难，或词根已被大量商标使用 |
| Fabriweave | faber（工匠） | 均未占用 | **已注册**（2026-01） | 排除；也容易被读成纺织品牌 |

以上查询只能说明这些名字当时未被占用，不构成商标检索。

## 4. 决定

1. 项目名为 **Poieweave**：首字母大写，其余小写。原全称 "Autonomous Infrastructure Intelligence" 保留为副标题。
2. **前缀统一为 `poie`**，沿用 Inceptless 其他模块用简短词根作 crate 前缀的做法：

| 类别 | 原名 | 新名 |
| --- | --- | --- |
| Rust crate | `aii-ontology` | `poie-ontology` |
| 规划中的 crate | `aii-state`、`aii-sim`、`aii-viability`、`aii-runtime` | `poie-state`、`poie-sim`、`poie-viability`、`poie-runtime` |
| 命令行工具 | `aii-graph` | `poie-graph` |
| Python 包（规划） | `aii_models` | `poie_models` |
| 模式字符串 | `aii.physical-ontology/0.1`、`aii.scenario-cases/0.1` | `poie.physical-ontology/0.1`、`poie.scenario-cases/0.1` |

3. 模式字符串**不升版本号**：旧前缀从未在仓库外使用过，也没有需要兼容的旧数据。加载器只接受新前缀。

## 5. 后果

- 名称可读、可检索，含义与项目的核心问题一致；
- 与 Inceptless 其他模块形成统一的命名体系；
- 拼读不直观（大致念作 POY-ee-weave），需要在 README 中说明来源；
- 历史提交与 2026-10-28 版研究纲领中仍出现 AII，README 与研究纲领开头已注明曾用名。

## 6. 后续动作

- [x] 仓库内改名：文档、crate、命令行工具、模式字符串
- [x] 将 GitHub 仓库 `haeliotang/AII` 改名为 `haeliotang/poieweave`（2026-10-31；GitHub 自动重定向旧地址），并更新 `Cargo.toml` 与 README 中的仓库链接
- [ ] 在 USPTO、EUIPO、CNIPA、WIPO Global Brand Database 中检索软件与工业控制相关类别，把结果记入本 ADR
- [ ] 检索无冲突后，按 ADR-0001 §5 注册商标并发布 `TRADEMARKS.md`
- [ ] 视需要注册 poieweave.com 等域名
