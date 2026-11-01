# Poieweave 商标/名称补充检索（普通法使用、域名、USPTO 核实）

> AI 子代理补充检索，2026-11-01；不构成法律意见，**不计入 M1.10 退出条件**（需知识产权代理/法务）。

范围：只补做 ADR-0002 §7 所列「仍需人工/付费检索」中无需付费、无需登录、无需过验证码的部分（第 1 项中的 TSDR 单号核实、第 4 项普通法使用与域名）。未绕过任何验证码/反爬，未注册账号，未提交表单。访问时间均为 2026-11-01（本机时区 UTC+5:45，约 14:30–14:40）。

## 1. 普通法使用检索

| 来源 | 查询 | 结果 | 访问时间 | 是否可用 |
| --- | --- | --- | --- | --- |
| 网页搜索（WebSearch） | `"poieweave"` | 命中几乎全是本仓库 `haeliotang/poieweave` 的 PR 页面；无第三方网站、公司页、媒体报道。其余命中（Poee 面包、Pweave）无关 | 11-01 | 可用（搜索引擎覆盖有限） |
| 网页搜索 | `"poie weave" OR "poie-weave"` + github/pypi/npm/crates | 无同名项目；仅有无关的 `weave`、`weave-core`、`pi-weave`、`weave-python` 等「weave」类项目 | 11-01 | 可用 |
| 网页搜索 | `Poieweave autonomous infrastructure intelligence` | 仅本仓库 PR；无官网、无独立报道 | 11-01 | 可用 |
| GitHub API | `search/repositories?q=poieweave` | 1 条：本仓库 `haeliotang/poieweave` | 11-01 | 可用 |
| GitHub API | `search/users?q=poieweave`；`users/poieweave` | 0 条；用户名 `poieweave` 返回 404（未被占用） | 11-01 | 可用 |
| GitHub API | `search/code?q=poieweave` | 401（需认证） | 11-01 | **不可用**（未登录） |
| npm registry | `registry.npmjs.org/poieweave` | 404，未占用 | 11-01 | 可用 |
| PyPI | `pypi.org/pypi/poieweave/json` | 404，未占用 | 11-01 | 可用 |
| crates.io API（带 User-Agent） | `/api/v1/crates/poieweave` | 404，未占用 | 11-01 | 可用 |
| crates.io API | `/api/v1/crates/<name>`，name = `poie`、`poie-weave`、`poie-core`、`poie-cli`、`poie-sdk`、`poie-types` 以及本仓库实际使用的 `poie-exec`、`poie-plan`、`poie-state`、`poie-expansion`、`poie-process`、`poie-federation`、`poie-ontology`、`poie-runtime`、`poie-sim`、`poie-viability` | 全部 404，均未被占用 | 11-01 | 可用 |
| crates.io API | `/api/v1/crates?q=poie` | 总数 0：crates.io 上没有名称含 `poie` 的 crate | 11-01 | 可用 |
| Docker Hub API | `search/repositories?query=poieweave`；`repositories/poieweave/` | 均 0 条，命名空间 `poieweave` 下无仓库（不能据此断定该用户/组织名未被占用） | 11-01 | 可用 |
| 行业媒体 | 见上述网页搜索 | 未发现任何第三方媒体使用该名称 | 11-01 | 部分可用（无法穷尽媒体库） |

小结：除本项目自身外，未发现 "poieweave" 的第三方使用。局限：搜索引擎对新词收录有限；GitHub 代码搜索不可用；未检索 Hugging Face、Maven、Go、社交媒体、应用商店。

## 2. 域名

方法：本机 `whois`（`whois.verisign-grs.com`、`whois.pir.org`、`whois.nic.io`、`whois.nic.ai`、`whois.cnnic.cn`）；`.dev` 无可达的 43 端口 whois，改用 Google Registry 公开 RDAP（`pubapi.registry.google/rdap/domain/`）；另用 `dig` 核对 DNS。对照（`google.com`、`google.io`、`baidu.cn`、`google.dev`）均能返回注册信息，说明查询通道有效。注意：直接 `whois poieweave.<tld>` 对 .com/.cn 只返回 IANA 根区信息，必须指定注册局服务器。

| 域名 | 结果 | 访问时间 | 是否可用 |
| --- | --- | --- | --- |
| poieweave.com | `No match for domain`（Verisign）；DNS NXDOMAIN | 11-01 | 可用：看起来未注册 |
| poieweave.org | `Domain not found`（PIR） | 11-01 | 可用：看起来未注册 |
| poieweave.io | `Domain not found`（nic.io） | 11-01 | 可用：看起来未注册 |
| poieweave.dev | RDAP 404 `not found`；DNS NXDOMAIN | 11-01 | 可用：看起来未注册 |
| poieweave.ai | `Domain not found`（nic.ai） | 11-01 | 可用：看起来未注册 |
| poieweave.cn | `No matching record`（CNNIC） | 11-01 | 可用：看起来未注册 |

注意：whois「未找到」只表示当前未注册，不等于可购买（可能是保留、溢价或注册局限制；.cn 另有实名/资质要求）。未执行任何购买或注册。

## 3. USPTO 申请 50154930 核实

| 来源 | 查询 | 结果 | 访问时间 | 是否可用 |
| --- | --- | --- | --- | --- |
| TSDR 单页链接 `tsdr.uspto.gov/#caseNumber=50154930&caseType=SERIAL_NO` | WebFetch | 仅得到空的 SPA 外壳，无数据 | 11-01 | 不可用 |
| TSDR 静态状态页 `tsdr.uspto.gov/statusview/sn50154930`（公开、无需登录/验证码，普通 HTTP GET） | curl | **可用，记录真实存在**（页面生成时间 2026-11-01 03:47 EST）：标记 POIE，标准字符；美国序列号 50154930；申请日 2026-10-11；Principal 注册簿；状态 LIVE/APPLICATION/Awaiting Examination（尚未分配审查员）；国际类 009；商品：可下载表情符号、可下载手机图形、含卡通角色美术作品的可下载图像文件、含卡通角色视频的可下载多媒体文件；申请人为美国个人（公开记录，姓名此处不转录）；无代理律师 | 11-01 | 可用 |
| TSDR API `tsdrapi.uspto.gov/ts/cd/casestatus/sn50154930/info.json` | curl | 401，需 API key | 11-01 | 不可用 |
| tmsearch.uspto.gov | curl | 仅 SPA 外壳（125 KB HTML），无检索结果 | 11-01 | 不可用 |
| Trademarkia | curl | 301 跳转，未继续 | 11-01 | 未使用 |
| 网页搜索 `"POIE" trademark 50154930` | WebSearch | 无该记录（新申请，未被收录） | 11-01 | 可用但无结果 |

结论：ADR 中「序列号形制存疑」的担忧可以解除：50154930 是真实存在的 USPTO 申请（TSDR 官方页面返回完整记录；号段为何不同于以往的 97/98/99，本次未能查明）。该申请的商品是卡通表情/图像/视频文件，与 Poieweave 的基础设施规划软件（类 9 的其他软件、类 42 等）商品不同，但类别同为 cl. 9，且词根相同，属于「近似但商品不同」的低到中等关联，仍需代理判断。申请仍在待审查阶段，状态可能变化，建议代理复查。

## 对 ADR-0002 §7 初步结论的影响

- **总体：基本不变，略有加强。** 普通法使用与域名检索均未发现第三方使用 "poieweave"，crates.io 上 `poieweave` 与全部 `poie-*` 现用名称均未被占用，六个主要域名当前看起来均未注册，这加强了「无完全相同标记、残余风险主要来自 POIE 词根」的判断。
- **修正一项：** 50154930 的真实性存疑已消除（真实存在，待审查）；§7 相关表格行可在复核时改为「已核实」，并补注商品为卡通表情/图像文件。其作为 POIE 词根在先申请的风险评估不变。
- **未改变：** 知识产权代理/法务检索（EUIPO、WIPO、CNIPA 直连，cl. 9/35/42 图文检索，cl. 7/37/40/41 扩展检索）仍然必需，仍不应宣称「已清权」；本补充不计入 M1.10 退出条件。
- **建议（非法律意见）：** 尽早在 crates.io 预留关键名称、注册域名（需用户自行决定与操作，本次未执行），并让代理复查 50154930 的审查进展。
