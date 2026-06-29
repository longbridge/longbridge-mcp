# Longbridge MCP Tool 验证报告

- 日期: 2026-04-20
- 版本: longbridge-mcp `main @ 0408f57`
- 测试方式: Claude Code 直连 MCP，逐 tool 调用并收集入参/出参
- 覆盖: **108 / 108 tool = 100%**（服务端实际暴露 108 个；README 的 "110" 计数不准）
- 2026-04-21 补测末尾 2 个 (`ah_premium_intraday`、`profit_analysis_detail`) 完成全覆盖
- 原始响应大文件备份: `tests/tool-results/20260420-163821/`

## 测试符号来源

- watchlist 返回的真实 symbol（700.HK、AAPL.US、981.HK、NVDA.US 等）
- 持仓：`981.HK` paper trading
- 主力测试 symbol：700.HK、AAPL.US、939.HK、NIO.SG、HSI.HK

## 核心发现

### ✅ P0 — counter_id 残留（已处理完）

| # | 字段 / 位置 | 原文 | 处理 |
|---|-------------|------|------|
| 1 | `topic_detail.tickers` 数组 | `["ST/US/TSLA","ST/US/AVGO",...]` | ✅ 已在上游接口修复 |
| 2 | `topic/topic_detail/news.description` 正文 BBCode | `[st]ST/US/AAPL#Apple.US[/st]` | ⏭ 不修复（保留富文本语义） |
| 3 | `valuation_history.stocks` object key | `s_t/_u_s/_a_a_p_l` | ✅ commit `96cee8d` |
| 4 | `financial_report.*.router` URL query | `lb://page/stock/pk?counter_id=ST/US/AAPL` | ⏭ 不修复（App 深链由客户端约定） |
| 5 | `executive.forward_url` / `invest_relation.forward_url` 路径段 | `...stocks/ST.US.AAPL#...`（点号形式） | ⏭ 不修复（Wiki 路径由站点约定） |
| 6 | `finance_calendar[].icon` 图标 URL 路径段 | `.../ticker/ST/US/AGNCM.png` | ⏭ 不修复（CDN 资源路径按 counter_id 组织，替换会 404） |

### 🟡 P0 — 日期/时间格式共 **9 种**混用（unix 字符串已修复）

| # | 格式示例 | 典型 tool |
|---|----------|-----------|
| 1 | `"2026-04-17T20:00:01Z"` RFC3339 | now / quote / trades / intraday / candlesticks / market_temperature / filings / news / order 相关 / topic_replies / topic_detail.created_at |
| 2 | `"2026-05-15"` ISO date | trading_days / option_chain_expiry_date_list / dividend_detail / profit_analysis.start_date + end_date + updated_date / shareholder.report_date |
| ~~3~~ | ~~`"1776671654"` unix seconds **字符串**~~ | ✅ 已统一转 RFC3339（详见下方"已修复：unix 字符串 → RFC3339"）。**未覆盖**：裸 `date` / `start` / `end` 字段（字段名过泛未纳入白名单）：`industry_valuation.history[].date` / `profit_analysis_detail.start` / `.end` / `finance_calendar.datetime` |
| 4 | `"2026-04-20 01:30:00.0 +00:00:00"` chrono Debug | capital_flow / capital_distribution |
| 5 | `"04:00:00.0"` 带多余 `.0` | trading_session.begin_time / end_time |
| 6 | `"2026.02.09"` 点分日期 | dividend / broker_holding.updated_at / broker_holding_detail.updated_at / broker_holding_daily.list[].date / fund_holder.report_date |
| 7 | `"2026 年 4 月 17 日"` 中文文本日期 | institution_rating.instratings.updated_at / valuation_history.overview.date |
| 8 | `"20260601"` 紧凑 YYYYMMDD | corp_action.date |
| 9 | `"06.01"` 部分日期 MM.DD / `"2026.04.01 (美东)"` 带中文时区后缀 | corp_action.date_str / finance_calendar.date |
| 10 | `"2021/04/30"` 斜杠分隔日期 | institution_rating_detail.evaluate.list[].date |

#### ✅ 已修复：unix 字符串 → RFC3339（C.1 架构：pipeline + per-tool path）

**pipeline 层**（`src/serialize/`）只处理稳定惯例：

- `classify_field` 仅将 `*_at` 后缀认作 Timestamp（不维护其他字段名白名单，避免误分类）。
- `TimestampSerializer::serialize_str` 支持字符串 unix 秒：只在 [946684800, 4102444800]（约 2000–2100）内转换，`"0"` / `"-62135596800"` 等哨兵保留原样。
- `TimestampSerializer::serialize_seq → TimestampSeq` 让 `_at` 字段即使拿到数组也能逐元素转换。

**per-tool 层**（`src/tools/`）针对非 `_at` 字段显式声明 path：

- `serialize::convert_unix_paths(value, paths)` — path 语法 `a.b.c` / `*`，原地就地替换。
- `http_client::http_get_tool_unix(client, path, params, unix_paths)` — 流程为 `transform_json`（snake_case + `_at`）→ 解析 Value → `convert_unix_paths` → 序列化。

**涉及的 tool 与 path 声明**：

| Tool | path |
|------|------|
| `market_status` | `market_time.*.timestamp`, `market_time.*.delay_timestamp` |
| `ah_premium` / `ah_premium_intraday` | `klines.*.timestamp` |
| `trade_stats` | `statistics.timestamp`, `statistics.trade_date.*` |
| `option_volume_daily` | `stats.*.timestamp` |
| `short_positions` | `data.*.timestamp` |
| `industry_valuation` | `list.*.history.*.date` |
| `valuation` | `metrics.pe.list.*.timestamp` |
| `valuation_history` | `history.metrics.pe.list.*.timestamp` |
| `institution_rating` | `analyst.evaluate.{start,end}_date`, `analyst.target.{start,end}_date` |
| `institution_rating_detail` | `target.list.*.timestamp` |
| `forecast_eps` | `items.*.forecast_{start,end}_date` |
| `dca_list` | `plans.*.next_trd_date` |
| `profit_analysis` | `end_time`, `trade_update_time` |
| `profit_analysis_detail` | `start`, `end` |

仅 pipeline `_at` 就够的（无 path 声明）：`sharelist_popular.*.{created,edited}_at`、`corp_action.items.*.live.started_at`、`dca_list.plans.*.{created,updated}_at`、`profit_analysis.sublist.*`。

**空值反模式（保留语义，由客户端识别）**
- `"0"` 字符串表示"无"：`institution_rating.end_date` / `forecast_eps.forecast_end_date` — 范围外，保持原串，语义为 "no end"。
- `""` 空串表示"无"：`dividend_detail.symbol`、`company.listing_date`、`institution_rating.instratings.evaluate.date`、`sharelist_detail.stocks[].ipo_date`
- `"-62135596800"` Rust/Go zero-time（公元 1 年）：`sharelist_popular.edited_at` — 范围外，保持原串。
- `"1970-01-01T00:00:00Z"` Unix epoch 字符串：`topic_detail.updated_at`

特别严重（剩余）：
- `dividend` 用点分日期、`dividend_detail` 用 ISO date — **同一领域两 tool 格式不一致**；
- `profit_analysis_detail` 同一对象内 `start_date:"2026-04-21"` (ISO) 和 `start:"1776754804"` (unix，未修) 共存，字段命名过泛未纳入白名单；
- 十余处 `updated_at` 字段依然多种格式（中文文本、点分、ISO date），唯 unix 字符串形已统一转 RFC3339。

**建议**
1. 全部 timestamp 用 RFC3339（带 `Z` 或 `+08:00`）；✅ **unix 秒字符串已统一**
2. 纯日期用 ISO date `YYYY-MM-DD`；
3. 中文文本日期、点分/斜杠/紧凑日期应上游统一；
4. 时区用单独字段表示，不要在 date 后面拼接 `(美东)` 这种文本。
5. 空值全部用 `null`，消除 `""` / `"1970-01-01T00:00:00Z"` 这些占位（`"0"` / `"-62135596800"` 已由序列化层原样保留，保留语义）。

### ✅ P0 — 响应格式不统一（部分已修复）

| Tool | 修复前 | 修复后 | 状态 |
|------|--------|--------|------|
| `alert_disable` | `"alert 491759 disabled"` | `{"alert_id":"491759","enabled":false}` | ✅ 已修复 |
| `alert_enable` | `"alert 491759 enabled"` | `{"alert_id":"491759","enabled":true}` | ✅ 已修复 |
| `create_watchlist_group` | 裸 int `4362235` | `{"id":4362235}` | ✅ 已修复 |
| `update_watchlist_group` | `"watchlist group updated"` | `{"id":4362235,"updated":true}` | ✅ 已修复 |
| `delete_watchlist_group` | `"watchlist group deleted"` | `{"id":4362235,"deleted":true}` | ✅ 已修复 |

**剩余同类问题**（未修）:
- `cancel_order` → `"order cancelled"` 纯文本
- `replace_order` → `"order replaced"` 纯文本
- `valuation.pe.desc` / `valuation_history.overview.metrics.pe.desc` 内嵌 HTML `<strong>` 标签，API 与渲染耦合。

### 🟡 P1 — 类型不一致

- 价格/金额字段几乎全部 string，而 volume/quantity 是 int；同一对象常混合（`price:"522.500"` vs `volume:100`）。
- `market_status`：`sub_status:20900` (int) + `timestamp` 现在是 RFC3339 字符串（已后补修复）。
- `exchange_rate.bid_rate:1.1465` 是浮点数字，但其他金额都是字符串。
- `account_balance.risk_level:"3"`、`dividend.total:"25"`、`dca_stats.active_count:"0"` — 计数/级别字段应为 int。
- **`warrant_issuers.issuer_id:1`** 是 int；**`alert_list.indicators[].id:"491196"`** 是 string — id 类型不统一。

### 🟡 P1 — 冗余 / 描述问题

- **`calc_indexes`** 返回所有 40+ 字段（未请求的全 null），冗余且难读。建议只返回请求字段。
- **`option_volume` 字段名过短**：`c` / `p`（推测 call / put），无文档说明。
- **`history_market_temperature.type:"Unknown"`** 无枚举定义。
- **`dividend_detail` 条目 `symbol:""`** 空串 — 应填实际 symbol。
- **`shareholder[].shareholder_id:"0"`** 所有条目都是 "0"，字段无用。
- **`invest_relation[].company_id:"0"`** 同上；**`invest_relation.total:0` 但 list 有 30 条** — 字段语义含糊。

### 🟡 P1 — MCP 参数序列化兼容性

Claude Code 作为 MCP 客户端时，以下参数类型**稳定地**序列化失败（服务器收到字符串形态），建议服务端容错接受 JSON 字符串 + 二次解析：

| 参数类型 | 影响 tool | 失败模式 |
|---------|-----------|---------|
| `items: string` 数组 | `option_quote.symbols` / `warrant_quote.symbols` / `dca_check.symbols` / `sharelist_add/sort/remove.symbols` | `expected a sequence` |
| `i64` 整数 | `update_watchlist_group.id` / `delete_watchlist_group.id` | `expected i64` |
| `usize` 整数（偶发） | `trades.count` / `option_volume_daily.count` / `sharelist_list.count` | `expected usize/u32` |

（同一会话内 `quote.symbols`、`calc_indexes.symbols` 有时成功、有时失败，完全不确定；`submit_order.submitted_quantity` 字符串却能正常工作。）

### ✅ 已做对

- 交易类（order/execution/cash_flow/submit/cancel）的 `submitted_at`/`updated_at`/`trade_done_at` 均为标准 RFC3339。
- 主要行情 tool 的 `timestamp` 已 RFC3339。
- `watchlist` / `static_info` / `quote` / `stock_positions` / `news` / `filings` / `alert_list` / `security_list` / `option_chain_info_by_date` / `shareholder` / `fund_holder` / `constituent` / `sharelist_detail` 等均已 symbol 化。
- OAuth + `Mcp-Session-Id` 会话保持正常。
- 订单、DCA、alert、sharelist、watchlist group、topic 生命周期 API 设计合理（create 返回 id、后续用 id 操作）。
- 大部分 enum 字段（market / trade_session / status / side）取值规范。

## 按 tool 分类汇总

### ✅ 正常（格式 & 转换均符合预期，60 个）

**Quote**: `now`, `static_info`, `quote`, `depth`, `brokers`, `participants`, `trades`, `intraday`, `candlesticks`, `history_candlesticks_by_date`, `history_candlesticks_by_offset`, `trading_days`, `option_chain_expiry_date_list`, `option_chain_info_by_date`, `market_temperature`, `history_market_temperature`, `watchlist`, `filings`, `warrant_issuers`, `warrant_list`, `security_list`

**Trade**: `margin_ratio`, `today_orders`, `order_detail`, `today_executions`, `history_orders`, `history_executions`, `cash_flow`, `estimate_max_purchase_quantity`, `submit_order`, `cancel_order`

**Portfolio**: `stock_positions`, `fund_positions`, `account_balance`, `exchange_rate`

**Fundamental**: `company`, `consensus`, `shareholder` (除 shareholder_id)

**Market**: `constituent`, `anomaly`, `broker_holding_daily` (除 date 格式)

**Content**: `news`, `topic_replies`

**Alert**: `alert_list`, `alert_add`, `alert_delete`

**DCA**: `dca_list`, `dca_stats`, `dca_create`, `dca_pause`, `dca_resume`, `dca_update`, `dca_history`, `dca_stop`

**Sharelist**: `sharelist_list`, `sharelist_popular` (除 zero-time), `sharelist_detail` (除 ipo_date 空串), `sharelist_create`, `sharelist_delete`

**Option Volume**: `option_volume_daily` (除 timestamp)

**Short**: `short_positions` (除 timestamp)

**Statement**: `statement_list`

### ⚠️ 含问题（25 个）

| Tool | 问题 |
|------|------|
| ~~`market_status`~~ | ~~timestamp/delay_timestamp 用 unix 字符串~~ ✅ 已转 RFC3339（后补修复）|
| `capital_flow` | timestamp 用 chrono Debug 格式 |
| `capital_distribution` | 同上 |
| `trading_session` | begin_time/end_time 带多余 `.0` 尾缀 |
| `calc_indexes` | 返回所有字段（含未请求的全 null），冗余 |
| `financial_report` | router URL 残留 `counter_id=ST/US/AAPL` |
| `dividend` | 日期点分格式 `"2026.02.09"` |
| `dividend_detail` | symbol 字段空串 |
| `institution_rating` | start_date 已转 RFC3339（后补修复）；`end_date:"0"` 哨兵保留；instratings.updated_at 中文文本；evaluate.date 空串 |
| `institution_rating_detail` | evaluate.list[].date 斜杠分隔；target.list[].timestamp 已转 RFC3339（后补修复）；target.updated_at 中文文本 |
| `forecast_eps` | forecast_start_date / forecast_end_date 已转 RFC3339（后补修复）；末条 `end_date:"0"` 哨兵保留（"no end" 语义）|
| `valuation` | list[].timestamp 已转 RFC3339（后补修复）；desc 含 HTML |
| `valuation_history` | stocks object key `"s_t/_u_s/_a_a_p_l"` counter_id 转换失败（`96cee8d` 已修）；list[].timestamp 已转 RFC3339（后补修复）；desc 含 HTML；overview.date 中文文本 |
| `executive` | forward_url 带 `ST.US.AAPL` 路径（非标准 symbol） |
| `corp_action` | date=YYYYMMDD；date_str=MM.DD；live.started_at 已转 RFC3339（后补修复）；date_zone=中文 |
| ~~`ah_premium` / `ah_premium_intraday`~~ | ~~klines[].timestamp unix 字符串（intraday 为分钟级）~~ ✅ 已转 RFC3339（后补修复）|
| ~~`trade_stats`~~ | ~~statistics.timestamp + trade_date[] unix 字符串~~ ✅ 已转 RFC3339（后补修复，数组元素逐个转换）|
| `broker_holding` / `broker_holding_detail` | updated_at 点分日期 |
| `broker_holding_daily` | list[].date 点分日期 |
| `profit_analysis` / `profit_analysis_detail` | updated_at/end_time/trade_update_time 已转 RFC3339（后补修复）；`profit_analysis_detail.start`/`.end` 裸字段仍为 unix（字段名过泛未纳入白名单，同对象 start_date/end_date 为 ISO date）|
| ~~`sharelist_popular`~~ | ~~created_at/edited_at unix 字符串~~ ✅ 有效值已转 RFC3339（后补修复）；`edited_at:"-62135596800"` zero-time 哨兵保留 |
| `finance_calendar` | date="2026.04.01 (美东)"；datetime unix 字符串（`datetime` 未在白名单内，保留）；icon URL 带 ST/US/xxx 形式 |
| `fund_holder` | report_date 点分日期 |
| `industry_valuation` | history[].date unix 字符串（裸 `date` 字段过泛，未纳入白名单；同名字段其他 tool 中为 `YYYY/MM/DD` / `YYYYMMDD`）|
| `invest_relation` | forward_url 带 `ST.HK.700` 形式；company_id 全 0；total 字段与 list 长度不一致 |
| `topic` / `topic_detail` | description 内嵌 `[st]ST/US/...[/st]`；topic_detail.tickers 全 counter_id；topic_detail.updated_at="1970-01-01T00:00:00Z"；topic_replies.created_at 正常 |
| `option_volume` | 字段名 `c`/`p` 缺文档 |
| ~~`alert_disable` / `alert_enable`~~ | ~~返回纯文本而非 JSON~~ ✅ 已修复 |
| ~~`create_watchlist_group`~~ | ~~返回裸 int 而非对象~~ ✅ 已修复 |
| ~~`update_watchlist_group` / `delete_watchlist_group`~~ | ~~返回纯文本~~ ✅ 已修复 |

### ✅ 被 MCP 序列化 bug 阻塞（9 个，已修复）

commit `02dbaaa fix(tools): tolerant deserializers for array/int/bool params`：

| 参数类型 | 影响的 tool | 修复前错误 |
|----------|-------------|-----------|
| `Vec<String>` | `option_quote`、`warrant_quote`、`dca_check`、`sharelist_add`、`sharelist_sort`、`sharelist_remove` | `invalid type: string "[\"X\"]", expected a sequence` |
| `i64` | `update_watchlist_group`、`delete_watchlist_group` | `invalid type: string "4360498", expected i64` |
| `bool` | `delete_watchlist_group.purge`（回归测试发现） | `invalid type: string "true", expected a boolean` |

**修复方式**: 在 `src/tools/tolerant.rs` 新增 `deserialize_with` helpers，遇到 JSON-encoded 字符串自动二次 parse。Schema 输出不变。已实际调用验证：
- `warrant_quote(symbols=["64187.HK"])` → 返回完整 quote
- `dca_check(symbols=["AAPL.US","700.HK"])` → 返回 support flag
- `sharelist_add/sort/remove` 生命周期全通
- `update_watchlist_group` / `delete_watchlist_group` → 正常 JSON 响应

### 🫥 返回空数据的 tool

**A. 账号 / 数据状态自然为空**（非 bug，仅记录）

| Tool | 返回 | 原因 |
|------|------|------|
| `fund_positions` | `{"list":[]}` | 该账号未持有基金 |
| `brokers(700.HK)` | `{"ask_brokers":[],"bid_brokers":[]}` | 港股盘前/盘后或无经纪队列数据 |
| `anomaly(market=HK)` | `{"changes":[],"all_off":false}` | 当时市场无异动 |
| `operating(AAPL.US)` | `{"list":[]}` | 该 tool 后端似乎无数据（所有 symbol 都可能为空，建议再挑其他 symbol 验证） |
| `history_orders` / `history_executions` / `cash_flow`（2026-04-01→04-15） | `[]` | paper trading 账号该期间无订单 / 成交 / 现金流 |
| `dca_list` | `{"plans":[]}` | 无 DCA 计划（创建后再查会有） |
| `dca_history(plan_id=X)` | `{"records":[],"has_more":false}` | 新建计划尚未到扣款日 |
| `sharelist_list` | `{"sharelists":[],"subscribed_sharelists":[],"tail_mark":""}` | 该账号未创建/订阅股单（`sharelist_popular` 有公开股单数据） |
| `statement_list`（初测 2026-04-20） | `{"list":[]}` | paper trading 当日无对账单；**次日 2026-04-21 再测已有数据** |

**B. 写入类成功后返回 `{}`**（约定，通过 HTTP 200 区分成功/失败）

- `alert_delete`、`dca_pause/resume/stop`、`sharelist_add/sort/remove/delete` 均返回 `{}`
- 创建类则返回含 id 的对象：`sharelist_create → {sharelist_id}`、`dca_create → {plan_id}`、`submit_order → {order_id}`、`create_watchlist_group → {id}`（commit `e4a8522` 后）、`topic_create → {id}`

**C. ⚠️ 可疑空返回（建议跟进）**

| Tool | 返回 | 说明 |
|------|------|------|
| `alert_add(symbol=..., condition=..., price=...)` | `{}` | 与其他创建类不一致 — 按照约定应返回 `{alert_id: "..."}`，目前用户只能通过 `alert_list` 反查刚创建的 id。建议改为返回新建 alert id |

### ⏭️ 未测 / 重新验证后的定性

| Tool | 原因 | 是否 bug |
|------|------|---------|
| ~~`statement_export`~~ | 已移除 — 该命令是 CLI 独有（做了下载+分 section 渲染），不适合作为 MCP tool。替换为 `statement_download_url` | ✅ 已解决 |
| `statement_download_url` | 新增。调用 SDK `AssetContext::statement_download_url`，返回 `{url}` 预签名 S3 地址（1 小时有效） | ✅ 实测通过 |
| `replace_order` | 2026-04-21 用 LO + ELO 两种订单类型重测，都返回 `602012: Order amendment is not supported for this order type`。paper trading 账号似乎完全不支持 amendment | ⚠️ 业务限制（paper trading） |

### 🧹 残留测试资源

- **需你手动清理**：
  - `watchlist_group id=4360498` 名为 `mcp_test_20260420`（delete 调用因序列化 bug 失败）
  - `topic id=40036798` + `reply 40643840`（你说会手动删）
- **已清理**：测试 sharelist、alert、dca plan、submit_order（被拒）

## 建议的修复优先级

1. **🔴 P0 日期/时间统一**: 所有 timestamp → RFC3339；纯日期 → ISO date；时区字段独立；空值 → null。
2. ~~🔴 P0 counter_id URL/BBCode 残留~~ ✅ **已处理完**：
   - `topic_detail.tickers` ✅ 上游接口修复；
   - `valuation_history.stocks` key ✅ commit `96cee8d`；
   - `description` BBCode / `router` / `forward_url` / `icon` ⏭ 保留原形态（App 深链 / Wiki 路径 / CDN 资源路径均由客户端或站点约定）。
3. ~~🟡 P1 `alert_enable` / `alert_disable` 改返回 JSON~~ ✅ **已修复**
4. ~~🟡 P1 `create_watchlist_group` 返回对象~~ ✅ **已修复**（同 commit 也修了 `update/delete_watchlist_group`）
5. **🟡 P1 `calc_indexes` 只返请求字段**；`option_volume.c/p` 改成 `call_volume/put_volume`。
6. ~~🟡 P1 服务端容错 MCP 字符串数组/整数~~ ✅ **已处理完**：commit `02dbaaa`。

## 附录

- 详细逐 tool 入参/出参在会话记录中已打印。
- 溢出大响应备份在 `tests/tool-results/20260420-163821/`：`financial_report`(144KB)、`participants`(65KB)、`security_list`(1017KB)、`warrant_list`(426KB)。
- 配套自动化 Python 测试脚本：`tests/test_tools_full.py`。
