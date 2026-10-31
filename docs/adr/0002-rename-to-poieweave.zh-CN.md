# ADR-0002：项目由 AII 改名为 Poieweave

- 状态：**已接受**（仓库内改名与 GitHub 仓库改名已完成；商标初步检索已记录于 §7，正式检索与注册待办）
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
- [x] 在 USPTO、EUIPO、CNIPA、WIPO Global Brand Database 中检索软件与工业控制相关类别，把结果记入本 ADR（初步检索，2026-10-31，见 §7；官方库直接访问受限，类别专项检索需人工复核）
- [ ] 检索无冲突后，按 ADR-0001 §5 注册商标并发布 `TRADEMARKS.md`
- [ ] 视需要注册 poieweave.com 等域名

## 7. 商标初步检索（2026-10-31）

**性质**：自动化可及的公开数据检索，不构成法律意见，也不等于完整商标检索。官方库的直接界面在本机被反爬机制阻断，因此主要证据来自 **TMview**（聚合 USPTO、EUIPO、WIPO/Madrid、CNIPA 等约 40 个商标局）与 Trademarkia（美国镜像），并对被阻断的来源逐项标注。

| 来源 | 查询 | 命中 | 说明 |
| --- | --- | --- | --- |
| TMview（US/EM/WO/CN/GB/KR 等） | `poieweave`（包含、精确、模糊） | **0** | 三种模式均为 0；`poie weave`、`poieweav`、`poiwave` 也为 0。对照词 `tesla` 有数千条命中，说明接口可用 |
| Trademarkia（美国镜像） | `poieweave` | **0** | 无结果 |
| USPTO 直连（tmsearch / TSDR） | — | 不可用 | Angular SPA + AWS WAF；TSDR API 需要密钥 |
| EUIPO eSearch 直连 | — | 不可用 | TLS 握手失败 / WAF 403；EM 数据经 TMview 间接覆盖 |
| WIPO Global Brand Database | — | 不可用 | Altcha 工作量证明验证码；Madrid 数据经 TMview 间接覆盖（WO 0 条） |
| CNIPA | — | 不可用 | JS 反爬；CN 数据经 TMview 间接覆盖 |

**相近标记**（无完全相同的标记）：

| 标记 | 辖区 | 状态与类别 | 相关性 |
| --- | --- | --- | --- |
| POIE | US | 申请 50154930，2026-10-11，cl. 9（可下载图形） | 词根相同、类别相同，商品不同；**该申请号形制与 2026 年序列不符，需在 TSDR 核实** |
| 波意 POIE | CN | 已注册，cl. 9 | 拉丁要素 POIE，同类 |
| POIE | CN | 已注册，cl. 25（服装） | 类别不相关 |
| poie | GB | 2020 年失效，cl. 36 | 不相关 |
| POIESIS / POIEMA / POIETO / POIESIA / POIETIS | US、FR 等 | 已注册，主要涉及 cl. 35/41/42/44、FR cl. 7/9/42 | 真正需要关注的近似族，但不是完整标记 Poieweave |

包名侧：npm 与 PyPI 的 `poieweave` 均未被占用（404）；公开检索中唯一的 "poieweave" 使用是本仓库。英国存在公司 POIE LTD（#07351950），与软件商标无直接冲突。

**初步结论**：在可访问的数据库中未发现与 "Poieweave" 完全相同的标记；残余风险中等偏低，且主要来自 POIE 词根，而不是完整标记。**仍需人工/付费检索**：

1. USPTO TSDR 核实申请 50154930（序号真实性存疑），并做 cl. 9/35/42 的文字与图形检索；
2. EUIPO eSearch、WIPO Global Brand Database 直连检索；
3. CNIPA 按类别 9/35/42 的专项检索（TMview 的 CN 数据可能滞后）；
4. 普通法使用检索：GitHub、PyPI/npm、行业媒体、`poieweave.*` 域名；
5. 若进入硬件或服务业务，补充 cl. 7/37/40/41。

在上述项目完成前，不应宣称名称"已清权"；注册与 `TRADEMARKS.md` 仍按 ADR-0001 §5 待办。
