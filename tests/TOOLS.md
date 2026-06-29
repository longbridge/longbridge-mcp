# Longbridge MCP — Tool Reference (英中对照)

108 tools. Grouped by OAuth scope (per `scopes.json`) with public / unscoped tools last.

## Scope `4` — Watchlist / 自选列表

5 tools managing the user's watchlist groups and security listings.

### `create_watchlist_group`

- **EN**: Create a new watchlist group with optional initial securities
- **中文**: 新建一个自选分组，可选地附带初始股票列表

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `name` | ✅ | string | Group name | 分组名称 |
| `securities` |  | array \| null | Securities to add, e.g. `["700.HK", "AAPL.US"]` | 要加入的证券代码，例如 `["700.HK", "AAPL.US"]` |

### `delete_watchlist_group`

- **EN**: Delete a watchlist group by id
- **中文**: 按分组 id 删除自选分组

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `id` | ✅ | integer | Watchlist group id | 自选分组 id |
| `purge` | ✅ | boolean | Whether to also remove the securities from other groups | 是否同时从其他分组中清除这些证券 |

### `security_list`

- **EN**: Get security list for a market. category must be "Overnight"; other values or omitting it will cause an error. Currently only market="US" is supported; other markets will also return an error
- **中文**: 获取指定市场的证券列表。`category` 当前必须为 `"Overnight"`，其他取值或留空都会报错；并且目前仅支持 `market="US"`，其他市场同样会返回错误

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `market` | ✅ | string | Market code: HK, US, CN, SG | 市场代码：HK、US、CN、SG |
| `category` |  | string \| null | Category filter. Currently only "Overnight" is supported; passing any other value or omitting this field will result in a param_error. Note: only "US" market is currently supported for the "Overnight" category; other markets will also return a param_error. | 类别筛选。当前仅支持 `"Overnight"`，传入其他值或不传都会触发 `param_error`；并且 `"Overnight"` 类别目前仅支持 `US` 市场，其他市场同样会返回 `param_error` |

### `update_watchlist_group`

- **EN**: Update a watchlist group (rename or add/remove/replace securities)
- **中文**: 更新自选分组（重命名，或对其中的证券进行新增 / 移除 / 替换）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `id` | ✅ | integer | Watchlist group id | 自选分组 id |
| `name` |  | string \| null | New group name (optional) | 新的分组名称（可选） |
| `securities` |  | array \| null | Securities list (optional) | 证券代码列表（可选） |
| `mode` |  | string \| null | Update mode for securities: "add", "remove", or "replace" (default: "replace") | 证券更新模式：`add`（新增）、`remove`（移除）或 `replace`（替换，默认） |

### `watchlist`

- **EN**: Get all watchlist groups and their securities
- **中文**: 获取所有自选分组及其包含的证券

_(no parameters / 无参数)_

## Scope `6` — Account & Positions / 账户资产与资金明细查询

9 tools for querying account balance, positions, margin and statements.

### `account_balance`

- **EN**: Get account cash balance and asset summary. Pass currency (e.g. "USD", "HKD") to filter; omit to return all currencies.
- **中文**: 查询账户现金余额与资产概览。可传入 `currency`（如 `"USD"`、`"HKD"`）按币种筛选，省略则返回全部币种

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `currency` |  | string \| null | Filter by currency code (e.g. "USD", "HKD"). Omit to return all currencies. | 按币种代码筛选（如 `"USD"`、`"HKD"`），省略则返回所有币种 |

### `cash_flow`

- **EN**: Get cash flow records (deposits, withdrawals, dividends)
- **中文**: 查询资金流水记录（出入金、派息等）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `start_at` | ✅ | string | Start time (RFC3339) | 起始时间（RFC3339 格式） |
| `end_at` | ✅ | string | End time (RFC3339) | 结束时间（RFC3339 格式） |

### `fund_positions`

- **EN**: Get current fund positions
- **中文**: 查询当前的基金持仓

_(no parameters / 无参数)_

### `margin_ratio`

- **EN**: Get margin ratio (initial/maintenance/forced liquidation)
- **中文**: 查询保证金比率（初始 / 维持 / 强平）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `profit_analysis`

- **EN**: Get portfolio profit and loss analysis summary. start/end: optional date range in yyyy-mm-dd format. Both must be provided together — passing only one returns empty results.
- **中文**: 获取组合盈亏分析汇总。`start` / `end` 为可选日期区间（`yyyy-mm-dd`），必须成对传入；只传其中之一会返回空结果

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `start` |  | string \| null | Start date (yyyy-mm-dd). Must be paired with `end`; passing only one returns empty results. | 起始日期（`yyyy-mm-dd`）。必须与 `end` 成对传入，只传其一会返回空结果 |
| `end` |  | string \| null | End date (yyyy-mm-dd). Must be paired with `start`; passing only one returns empty results. | 结束日期（`yyyy-mm-dd`）。必须与 `start` 成对传入，只传其一会返回空结果 |

### `profit_analysis_detail`

- **EN**: Get detailed profit and loss analysis for a specific symbol. start/end: optional date range in yyyy-mm-dd format. Both must be provided together — passing only one returns empty results.
- **中文**: 获取指定标的的详细盈亏分析。`start` / `end` 为可选日期区间（`yyyy-mm-dd`），必须成对传入；只传其中之一会返回空结果

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |
| `start` |  | string \| null | Start date (yyyy-mm-dd). Must be paired with `end`; passing only one returns empty results. | 起始日期（`yyyy-mm-dd`）。必须与 `end` 成对传入，只传其一会返回空结果 |
| `end` |  | string \| null | End date (yyyy-mm-dd). Must be paired with `start`; passing only one returns empty results. | 结束日期（`yyyy-mm-dd`）。必须与 `start` 成对传入，只传其一会返回空结果 |

### `statement_export`

- **EN**: Get a pre-signed download URL for a statement data file (obtained from statement_list). Returns {url}; fetch that URL to get the statement JSON.
- **中文**: 获取对账单数据文件的预签名下载 URL（`file_key` 来自 `statement_list`）。返回 `{url}`，访问该 URL 即可获取对账单 JSON

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `file_key` | ✅ | string | File key from statement_list, e.g. "/statement_data/data/.../20975338.json" | 来自 `statement_list` 的文件 key，例如 `"/statement_data/data/.../20975338.json"` |

### `statement_list`

- **EN**: List available account statements (daily/monthly)
- **中文**: 列出可获取的账户对账单（日 / 月对账单）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `statement_type` |  | string \| null | Statement type: "daily" (default) or "monthly". | 对账单类型：`"daily"`（默认，日报）或 `"monthly"`（月报） |
| `start_date` |  | string \| null | Start date (yyyy-mm-dd). Defaults to 30 days ago for "daily" or 12 months ago for "monthly". | 起始日期（`yyyy-mm-dd`）。`daily` 默认 30 天前，`monthly` 默认 12 个月前 |
| `limit` |  | integer \| null | Number of records to return. Defaults to 30 for "daily" or 12 for "monthly". | 返回条数。`daily` 默认 30，`monthly` 默认 12 |

### `stock_positions`

- **EN**: Get current stock positions across all channels
- **中文**: 查询所有渠道下的当前股票持仓

_(no parameters / 无参数)_

## Scope `10` — Trade Order Lookup / 交易查询

9 tools for querying orders, executions and DCA plan history.

### `dca_history`

- **EN**: Get execution history records for a DCA plan by plan_id
- **中文**: 按 `plan_id` 查询定投计划的扣款执行历史

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `plan_id` | ✅ | string | Plan ID | 定投计划 ID |
| `page` |  | integer \| null | Page number (default 1) | 页码（默认 1） |
| `limit` |  | integer \| null | Records per page (default 20) | 每页条数（默认 20） |

### `dca_list`

- **EN**: List DCA recurring investment plans. Filter by status (Active/Suspended/Finished) or symbol.
- **中文**: 列出定投计划，可按状态（`Active` 进行中 / `Suspended` 暂停 / `Finished` 已结束）或证券筛选

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `status` |  | string \| null | Filter by status: Active, Suspended, Finished. Omit to return all. | 按状态筛选：`Active`、`Suspended`、`Finished`，省略则返回全部 |
| `symbol` |  | string \| null | Filter by symbol, e.g. "AAPL.US". Omit to return all plans. | 按证券筛选，例如 `"AAPL.US"`，省略则返回全部计划 |
| `page` |  | integer \| null | Page number (default 1) | 页码（默认 1） |
| `limit` |  | integer \| null | Records per page (default 20) | 每页条数（默认 20） |

### `dca_stats`

- **EN**: Get DCA investment statistics summary. Optionally filter by symbol.
- **中文**: 获取定投投资统计汇总，可选地按证券筛选

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` |  | string \| null | Filter by symbol, e.g. "AAPL.US". Omit to return stats for all plans. | 按证券筛选，例如 `"AAPL.US"`，省略则返回所有计划的合计统计 |

### `estimate_max_purchase_quantity`

- **EN**: Estimate maximum buy/sell quantity for a symbol
- **中文**: 估算指定证券的最大可买 / 可卖数量

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string |  | 证券代码 |
| `side` | ✅ | string | Buy or Sell | 买卖方向：`Buy`（买入）或 `Sell`（卖出） |
| `order_type` | ✅ | string | Order type: LO (Limit Order) / ELO (Enhanced Limit Order) / MO (Market Order) / AO (At-auction) / ALO (At-auction Limit Order) | 委托类型：`LO`（限价单）/ `ELO`（增强限价单）/ `MO`（市价单）/ `AO`（竞价单）/ `ALO`（竞价限价单） |
| `price` |  | string \| null |  | 委托价格（限价类委托需要） |

### `history_executions`

- **EN**: Get historical trade executions between dates
- **中文**: 查询指定日期区间内的历史成交记录

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` |  | string \| null | Filter by symbol (optional) | 按证券筛选（可选） |
| `start_at` | ✅ | string | Start time (RFC3339) | 起始时间（RFC3339 格式） |
| `end_at` | ✅ | string | End time (RFC3339) | 结束时间（RFC3339 格式） |

### `history_orders`

- **EN**: Get historical orders between dates (excludes today)
- **中文**: 查询指定日期区间内的历史委托（不含当日）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` |  | string \| null | Filter by symbol (optional) | 按证券筛选（可选） |
| `start_at` | ✅ | string | Start time (RFC3339) | 起始时间（RFC3339 格式） |
| `end_at` | ✅ | string | End time (RFC3339) | 结束时间（RFC3339 格式） |

### `order_detail`

- **EN**: Get detailed information about a specific order
- **中文**: 查询单个委托的详细信息

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `order_id` | ✅ | string |  | 委托单号 |

### `today_executions`

- **EN**: Get today's trade executions (fills). Pass symbol or order_id to filter; omit both to return all.
- **中文**: 查询当日成交（filled）。可传入 `symbol` 或 `order_id` 筛选，两者都省略则返回全部

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` |  | string \| null | Filter by symbol, e.g. "700.HK". | 按证券筛选，例如 `"700.HK"` |
| `order_id` |  | string \| null | Filter by a specific order_id. | 按指定委托单号筛选 |

### `today_orders`

- **EN**: Get orders placed today. Pass symbol to filter; omit to return all.
- **中文**: 查询当日委托。可传入 `symbol` 筛选，省略则返回全部

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` |  | string \| null | Filter by symbol, e.g. "700.HK". Omit to return all today's orders. | 按证券筛选，例如 `"700.HK"`，省略则返回当日全部委托 |

## Scope `11` — Trade Execution / 交易下单

8 tools that place, modify or cancel orders and DCA plans.

### `cancel_order`

- **EN**: Cancel an open order by order_id
- **中文**: 按 `order_id` 撤销未成交的委托

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `order_id` | ✅ | string |  | 委托单号 |

### `dca_create`

- **EN**: Create a DCA recurring investment plan. frequency: Daily/Weekly/Monthly. day_of_week (Weekly): Mon/Tue/Wed/Thu/Fri. day_of_month (Monthly): 1-28.
- **中文**: 创建定投（DCA）计划。`frequency` 可选 `Daily` / `Weekly` / `Monthly`；周频对应 `day_of_week`（`Mon`–`Fri`），月频对应 `day_of_month`（1-28）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "AAPL.US" | 证券代码，例如 `"AAPL.US"` |
| `amount` | ✅ | string | Amount to invest per cycle, e.g. "100" | 每期投入金额，例如 `"100"` |
| `frequency` | ✅ | string | Investment frequency: Daily, Weekly, Monthly | 定投频率：`Daily`（每日）、`Weekly`（每周）、`Monthly`（每月） |
| `day_of_week` |  | string \| null | Day of week for Weekly frequency: Mon, Tue, Wed, Thu, Fri | 周频时指定的周几：`Mon`、`Tue`、`Wed`、`Thu`、`Fri` |
| `day_of_month` |  | integer \| null | Day of month for Monthly frequency (1-28) | 月频时指定的日期（1-28） |
| `allow_margin` |  | boolean \| null | Allow margin financing (default false) | 是否允许使用融资（默认 `false`） |

### `dca_pause`

- **EN**: Pause (suspend) a DCA recurring investment plan by plan_id
- **中文**: 按 `plan_id` 暂停定投计划

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `plan_id` | ✅ | string | Plan ID | 定投计划 ID |

### `dca_resume`

- **EN**: Resume a suspended DCA recurring investment plan by plan_id
- **中文**: 按 `plan_id` 恢复已暂停的定投计划

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `plan_id` | ✅ | string | Plan ID | 定投计划 ID |

### `dca_stop`

- **EN**: Permanently stop a DCA recurring investment plan by plan_id
- **中文**: 按 `plan_id` 永久终止定投计划

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `plan_id` | ✅ | string | Plan ID | 定投计划 ID |

### `dca_update`

- **EN**: Update an existing DCA recurring investment plan by plan_id
- **中文**: 按 `plan_id` 更新已存在的定投计划

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `plan_id` | ✅ | string | Plan ID to update | 待更新的定投计划 ID |
| `amount` |  | string \| null | New investment amount per cycle | 新的每期投入金额 |
| `frequency` |  | string \| null | New investment frequency: Daily, Weekly, Monthly | 新的定投频率：`Daily`、`Weekly`、`Monthly` |
| `day_of_week` |  | string \| null | Day of week for Weekly frequency: Mon, Tue, Wed, Thu, Fri | 周频时指定的周几：`Mon`、`Tue`、`Wed`、`Thu`、`Fri` |
| `day_of_month` |  | integer \| null | Day of month for Monthly frequency (1-28) | 月频时指定的日期（1-28） |
| `allow_margin` |  | boolean \| null | Allow margin financing | 是否允许使用融资 |

### `replace_order`

- **EN**: Replace/modify an existing order
- **中文**: 改单（替换 / 修改已存在的委托）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `order_id` | ✅ | string |  | 委托单号 |
| `quantity` | ✅ | string |  | 新的委托数量 |
| `price` |  | string \| null |  | 新的委托价格 |
| `trigger_price` |  | string \| null |  | 新的触发价 |
| `limit_offset` |  | string \| null |  | 新的限价偏移量 |
| `trailing_amount` |  | string \| null |  | 新的跟踪止损金额 |
| `trailing_percent` |  | string \| null |  | 新的跟踪止损百分比 |

### `submit_order`

- **EN**: Submit a buy/sell order. order_type: LO (Limit) / ELO (Enhanced Limit, HK) / MO (Market) / AO (At-auction, HK) / ALO (At-auction Limit, HK) / ODD (Odd Lots, HK) / LIT (Limit If Touched) / MIT (Market If Touched) / TSLPAMT (Trailing Limit by Amount) / TSLPPCT (Trailing Limit by Percent) / SLO (Special Limit, HK). side: Buy/Sell. time_in_force: Day/GTC/GTD
- **中文**: 提交买入 / 卖出委托。`order_type` 可选：`LO`（限价）/ `ELO`（港股增强限价）/ `MO`（市价）/ `AO`（港股竞价）/ `ALO`（港股竞价限价）/ `ODD`（港股碎股）/ `LIT`（触价限价）/ `MIT`（触价市价）/ `TSLPAMT`（按金额跟踪止损限价）/ `TSLPPCT`（按百分比跟踪止损限价）/ `SLO`（港股特别限价）；`side` 为 `Buy` / `Sell`；`time_in_force` 为 `Day` / `GTC` / `GTD`

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string |  | 证券代码 |
| `order_type` | ✅ | string | Order type (HK supports all; US supports LO/MO/LIT/MIT/TSLPAMT/TSLPPCT only):<br>- LO (Limit Order): requires submitted_price<br>- ELO (Enhanced Limit Order, HK only): requires submitted_price<br>- MO (Market Order): no price required<br>- AO (At-auction Order, HK only): executed at auction price, no price required<br>- ALO (At-auction Limit Order, HK only): requires submitted_price<br>- ODD (Odd Lots Order, HK only): requires submitted_price, for non-standard lot sizes<br>- LIT (Limit If Touched): requires submitted_price and trigger_price; activates when market price touches trigger_price<br>- MIT (Market If Touched): requires trigger_price only; executes at market when trigger_price is touched<br>- TSLPAMT (Trailing Limit If Touched by Amount): requires trailing_amount and limit_offset; trailing stop by fixed amount<br>- TSLPPCT (Trailing Limit If Touched by Percent): requires trailing_percent (0-1) and limit_offset; trailing stop by percentage<br>- SLO (Special Limit Order, HK only): requires submitted_price; cannot be replaced after submission | 委托类型（港股全部支持；美股仅支持 `LO` / `MO` / `LIT` / `MIT` / `TSLPAMT` / `TSLPPCT`）：<br>- `LO`（限价单）：需 `submitted_price`<br>- `ELO`（增强限价单，港股）：需 `submitted_price`<br>- `MO`（市价单）：无需价格<br>- `AO`（竞价单，港股）：以竞价价成交，无需价格<br>- `ALO`（竞价限价单，港股）：需 `submitted_price`<br>- `ODD`（碎股单，港股）：需 `submitted_price`，用于非标准手数<br>- `LIT`（触价限价）：需 `submitted_price` 与 `trigger_price`，行情触及触发价时激活<br>- `MIT`（触价市价）：仅需 `trigger_price`，触及后按市价成交<br>- `TSLPAMT`（按金额跟踪止损限价）：需 `trailing_amount` 与 `limit_offset`<br>- `TSLPPCT`（按百分比跟踪止损限价）：需 `trailing_percent`（0-1）与 `limit_offset`<br>- `SLO`（特别限价单，港股）：需 `submitted_price`，提交后不可改单 |
| `side` | ✅ | string | Buy or Sell | 买卖方向：`Buy`（买入）或 `Sell`（卖出） |
| `submitted_quantity` | ✅ | string |  | 委托数量 |
| `time_in_force` | ✅ | string | Order validity: "Day" (Day Order, expires end of session), "GTC" (Good Til Canceled), "GTD" (Good Til Date, requires expire_date) | 委托有效期：`Day`（当日有效）、`GTC`（撤单前有效）、`GTD`（指定日期前有效，需 `expire_date`） |
| `submitted_price` |  | string \| null | Limit price. Required for: LO, ELO, ALO, ODD, LIT, SLO | 委托限价。`LO` / `ELO` / `ALO` / `ODD` / `LIT` / `SLO` 必填 |
| `trigger_price` |  | string \| null | Trigger (activation) price. Required for: LIT, MIT, TSLPAMT, TSLPPCT | 触发价。`LIT` / `MIT` / `TSLPAMT` / `TSLPPCT` 必填 |
| `limit_offset` |  | string \| null | Limit offset from the trailing stop price. Required for: TSLPAMT, TSLPPCT | 相对跟踪止损价的限价偏移量。`TSLPAMT` / `TSLPPCT` 必填 |
| `trailing_amount` |  | string \| null | Trailing amount (absolute price distance). Required for TSLPAMT | 跟踪金额（绝对价格距离），`TSLPAMT` 必填 |
| `trailing_percent` |  | string \| null | Trailing percent as decimal (e.g. 0.05 = 5%). Required for TSLPPCT | 跟踪百分比（小数形式，例如 `0.05` 表示 5%），`TSLPPCT` 必填 |
| `expire_date` |  | string \| null | Expiry date (yyyy-mm-dd). Required when time_in_force is GTD | 到期日期（`yyyy-mm-dd`），`time_in_force=GTD` 时必填 |
| `outside_rth` |  | string \| null | Outside regular trading hours: "RTH_ONLY" (regular trading hours only), "ANY_TIME" (any time including pre/post market), "OVERNIGHT" (overnight session, US only) | 盘前盘后设置：`RTH_ONLY`（仅常规交易时段）、`ANY_TIME`（含盘前盘后任意时段）、`OVERNIGHT`（夜盘，仅美股） |
| `remark` |  | string \| null | Order remark (max 255 characters) | 委托备注（最多 255 字符） |

## Public / 行情与公开数据 (77 tools)

Tools that don't require a user-account scope — quotes, fundamentals, market data, calendars, alerts, sharelist, content, DCA queries, etc.

### `ah_premium`

- **EN**: Get A/H share premium historical K-line data
- **中文**: 获取 A/H 股溢价的历史 K 线数据

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |
| `period` |  | string \| null | K-line period: "1m", "5m", "15m", "30m", "60m", "day" (default), "week", "month", "year" | K 线周期：`1m`、`5m`、`15m`、`30m`、`60m`、`day`（默认）、`week`、`month`、`year` |
| `count` |  | integer \| null | Number of K-lines to return (default: 100) | 返回的 K 线数量（默认 100） |

### `ah_premium_intraday`

- **EN**: Get A/H share premium intraday time-share data
- **中文**: 获取 A/H 股溢价的当日分时数据

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `alert_add`

- **EN**: Add a price alert. condition: price_rise/price_fall/percent_rise/percent_fall
- **中文**: 新增价格预警。`condition` 可选 `price_rise`（涨破）/ `price_fall`（跌破）/ `percent_rise`（涨幅）/ `percent_fall`（跌幅）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |
| `condition` | ✅ | string | Alert condition: "price_rise", "price_fall", "percent_rise", "percent_fall" | 预警条件：`price_rise`、`price_fall`、`percent_rise`、`percent_fall` |
| `price` | ✅ | string | Threshold price or percentage value | 阈值价格或百分比数值 |
| `frequency` |  | string \| null | Alert frequency: "once" (trigger once then disable), "daily" (once per day), "every" (alert every time condition is met) | 预警频率：`once`（触发一次后停用）、`daily`（每日一次）、`every`（每次满足条件都触发） |

### `alert_delete`

- **EN**: Delete a price alert by alert_id
- **中文**: 按 `alert_id` 删除价格预警

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `alert_id` | ✅ | string | Alert indicator id | 预警指标 id |

### `alert_disable`

- **EN**: Disable a price alert by alert_id
- **中文**: 按 `alert_id` 停用价格预警

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `alert_id` | ✅ | string | Alert indicator id | 预警指标 id |

### `alert_enable`

- **EN**: Enable a price alert by alert_id
- **中文**: 按 `alert_id` 启用价格预警

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `alert_id` | ✅ | string | Alert indicator id | 预警指标 id |

### `alert_list`

- **EN**: Get all configured price alerts
- **中文**: 获取所有已配置的价格预警

_(no parameters / 无参数)_

### `anomaly`

- **EN**: Get market anomaly alerts (unusual price/volume changes)
- **中文**: 获取市场异动提醒（价量异常变动）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `market` | ✅ | string | Market code: HK, US, CN, SG | 市场代码：HK、US、CN、SG |

### `broker_holding`

- **EN**: Get top broker holding data for a symbol
- **中文**: 获取指定证券的主要券商持仓数据

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |
| `period` |  | string \| null | Period: "rct_1" (1 day, default), "rct_5" (5 days), "rct_20" (20 days), "rct_60" (60 days) | 时间窗口：`rct_1`（近 1 日，默认）、`rct_5`（近 5 日）、`rct_20`（近 20 日）、`rct_60`（近 60 日） |

### `broker_holding_daily`

- **EN**: Get daily holding history for a specific broker
- **中文**: 获取指定券商对某证券的逐日持仓历史

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |
| `broker_id` | ✅ | string | Broker participant number | 券商参与者编号 |

### `broker_holding_detail`

- **EN**: Get full broker holding detail list
- **中文**: 获取完整的券商持仓明细列表

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `brokers`

- **EN**: Get broker queue data
- **中文**: 获取经纪商买卖盘队列数据

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `calc_indexes`

- **EN**: Calculate financial indexes (PE, PB, dividend ratio, etc.) for symbols
- **中文**: 为证券计算财务 / 行情指标（PE、PB、股息率等）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbols` | ✅ | array | Security symbols, e.g. `["700.HK", "AAPL.US"]` | 证券代码，例如 `["700.HK", "AAPL.US"]` |
| `indexes` | ✅ | array | Calc indexes: LastDone, ChangeValue, ChangeRate, Volume, Turnover, YtdChangeRate, TurnoverRate, TotalMarketValue, CapitalFlow, Amplitude, VolumeRatio, PeTtmRatio, PbRatio, DividendRatioTtm, FiveDayChangeRate, TenDayChangeRate, HalfYearChangeRate, FiveMinutesChangeRate, ExpiryDate, StrikePrice, UpperStrikePrice, LowerStrikePrice, OutstandingQty, OutstandingRatio, Premium, ItmOtm, ImpliedVolatility, WarrantDelta, CallPrice, ToCallPrice, EffectiveLeverage, LeverageRatio, ConversionRatio, BalancePoint, OpenInterest, Delta, Gamma, Theta, Vega, Rho | 待计算指标：`LastDone`、`ChangeValue`、`ChangeRate`、`Volume`、`Turnover`、`YtdChangeRate`、`TurnoverRate`、`TotalMarketValue`、`CapitalFlow`、`Amplitude`、`VolumeRatio`、`PeTtmRatio`、`PbRatio`、`DividendRatioTtm`、`FiveDayChangeRate`、`TenDayChangeRate`、`HalfYearChangeRate`、`FiveMinutesChangeRate`、`ExpiryDate`、`StrikePrice`、`UpperStrikePrice`、`LowerStrikePrice`、`OutstandingQty`、`OutstandingRatio`、`Premium`、`ItmOtm`、`ImpliedVolatility`、`WarrantDelta`、`CallPrice`、`ToCallPrice`、`EffectiveLeverage`、`LeverageRatio`、`ConversionRatio`、`BalancePoint`、`OpenInterest`、`Delta`、`Gamma`、`Theta`、`Vega`、`Rho` |

### `candlesticks`

- **EN**: Get candlestick data (OHLCV). period: 1m/5m/15m/30m/60m/day/week/month/year. trade_sessions: intraday/all
- **中文**: 获取 K 线数据（OHLCV）。`period` 可选 `1m` / `5m` / `15m` / `30m` / `60m` / `day` / `week` / `month` / `year`；`trade_sessions` 可选 `intraday` / `all`

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string |  | 证券代码 |
| `period` | ✅ | string | Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year | K 线周期：`1m`、`5m`、`15m`、`30m`、`60m`、`day`、`week`、`month`、`year` |
| `count` | ✅ | integer | Number of candlesticks (max 1000) | K 线数量（最多 1000） |
| `forward_adjust` | ✅ | boolean | Whether to forward-adjust for splits/dividends | 是否对拆股 / 派息进行前复权 |
| `trade_sessions` | ✅ | string | Trade sessions: "intraday" (regular hours only) or "all" (include pre-market and post-market) | 交易时段：`intraday`（仅常规时段）或 `all`（含盘前盘后） |

### `capital_distribution`

- **EN**: Get capital distribution (large/medium/small holder flows)
- **中文**: 获取资金分布（大 / 中 / 小单的资金流向）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `capital_flow`

- **EN**: Get capital inflow/outflow time series
- **中文**: 获取资金流入 / 流出的时间序列

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `company`

- **EN**: Get company overview (name, CEO, employees, profile)
- **中文**: 获取公司概况（名称、CEO、员工数、简介等）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `consensus`

- **EN**: Get financial consensus estimates (revenue, EPS, net income)
- **中文**: 获取分析师一致预期（营收、EPS、净利润等）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `constituent`

- **EN**: Get constituent stocks of an index (e.g. HSI.HK)
- **中文**: 获取指数成分股（例如 `HSI.HK`）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Index symbol, e.g. "HSI.HK" | 指数代码，例如 `"HSI.HK"` |

### `corp_action`

- **EN**: Get corporate actions (splits, buybacks, name changes)
- **中文**: 获取公司行动信息（拆股、回购、更名等）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `dca_check`

- **EN**: Check whether given symbols support DCA recurring investment
- **中文**: 检查指定证券是否支持定投（DCA）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbols` | ✅ | array | Security symbols to check, e.g. `["AAPL.US", "TSLA.US"]` | 待检查的证券代码，例如 `["AAPL.US", "TSLA.US"]` |

### `depth`

- **EN**: Get order book depth (bid/ask levels)
- **中文**: 获取盘口深度数据（买卖各档位）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `dividend`

- **EN**: Get dividend history for a symbol
- **中文**: 获取证券的派息历史

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `dividend_detail`

- **EN**: Get detailed dividend distribution scheme
- **中文**: 获取详细的派息分红方案

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `exchange_rate`

- **EN**: Get exchange rates for all supported currencies
- **中文**: 获取全部支持币种的汇率

_(no parameters / 无参数)_

### `executive`

- **EN**: Get company executive and board member information
- **中文**: 获取公司高管及董事会成员信息

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `filings`

- **EN**: Get regulatory filings (8-K, 10-Q, 10-K, etc.)
- **中文**: 获取监管文件公告（8-K、10-Q、10-K 等）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `finance_calendar`

- **EN**: Get finance calendar events. category: financial/report/dividend/ipo/macrodata/closed
- **中文**: 获取财经日历事件。`category` 可选 `financial` / `report` / `dividend` / `ipo` / `macrodata` / `closed`

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `market` |  | string \| null | Market code: HK, US, CN, SG. Omit to query across all markets. | 市场代码：HK、US、CN、SG，省略则查询所有市场 |
| `start` | ✅ | string | Start date (yyyy-mm-dd) | 起始日期（`yyyy-mm-dd`） |
| `end` | ✅ | string | End date (yyyy-mm-dd) | 结束日期（`yyyy-mm-dd`） |
| `category` | ✅ | string | Event category:<br>- "financial": earnings/financial results announcements<br>- "report": scheduled financial report release dates<br>- "dividend": dividend ex-dates and payment dates<br>- "ipo": IPO listing dates<br>- "macrodata": macroeconomic data releases (GDP, CPI, etc.)<br>- "closed": market holiday / trading halt dates | 事件类别：<br>- `financial`：业绩 / 财报公告<br>- `report`：财报披露排期<br>- `dividend`：除息日与派发日<br>- `ipo`：IPO 上市日期<br>- `macrodata`：宏观数据发布（GDP、CPI 等）<br>- `closed`：市场休市 / 停牌日期 |

### `financial_report`

- **EN**: Get financial reports for a symbol. report_type: annual or quarterly
- **中文**: 获取证券的财务报告。`report_type` 用于区分年报或季报等

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "AAPL.US" | 证券代码，例如 `"AAPL.US"` |
| `kind` |  | string \| null | Statement kind: "IS" (income statement), "BS" (balance sheet), "CF" (cash flow), "ALL" (default) | 报表种类：`IS`（利润表）、`BS`（资产负债表）、`CF`（现金流量表）、`ALL`（默认全部） |
| `report_type` |  | string \| null | Report period: "af" (annual), "saf" (semi-annual), "q1"/"q2"/"q3" (quarterly), "qf" (quarterly full) | 报告期：`af`（年度）、`saf`（半年度）、`q1` / `q2` / `q3`（季度）、`qf`（季度全量） |

### `forecast_eps`

- **EN**: Get EPS forecast and analyst estimate history
- **中文**: 获取 EPS 预测及分析师预期历史

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `fund_holder`

- **EN**: Get funds and ETFs that hold a given symbol
- **中文**: 获取持有指定证券的基金及 ETF

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `history_candlesticks_by_date`

- **EN**: Get historical candlestick data by date range. period: 1m/5m/15m/30m/60m/day/week/month/year
- **中文**: 按日期区间获取历史 K 线数据。`period` 可选 `1m` / `5m` / `15m` / `30m` / `60m` / `day` / `week` / `month` / `year`

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string |  | 证券代码 |
| `period` | ✅ | string | Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year | K 线周期：`1m`、`5m`、`15m`、`30m`、`60m`、`day`、`week`、`month`、`year` |
| `forward_adjust` | ✅ | boolean | Whether to forward-adjust for splits/dividends | 是否对拆股 / 派息进行前复权 |
| `start` |  | string \| null | Start date (yyyy-mm-dd), optional | 起始日期（`yyyy-mm-dd`），可选 |
| `end` |  | string \| null | End date (yyyy-mm-dd), optional | 结束日期（`yyyy-mm-dd`），可选 |
| `trade_sessions` | ✅ | string | Trade sessions: "intraday" (regular hours only) or "all" (include pre-market and post-market) | 交易时段：`intraday`（仅常规时段）或 `all`（含盘前盘后） |

### `history_candlesticks_by_offset`

- **EN**: Get historical candlestick data by offset from a reference time. period: 1m/5m/15m/30m/60m/day/week/month/year
- **中文**: 以参考时间为锚点，按偏移量获取历史 K 线数据。`period` 可选 `1m` / `5m` / `15m` / `30m` / `60m` / `day` / `week` / `month` / `year`

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string |  | 证券代码 |
| `period` | ✅ | string | Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year | K 线周期：`1m`、`5m`、`15m`、`30m`、`60m`、`day`、`week`、`month`、`year` |
| `forward_adjust` | ✅ | boolean | Whether to forward-adjust for splits/dividends | 是否对拆股 / 派息进行前复权 |
| `forward` | ✅ | boolean | Whether to query forward in time (true) or backward (false) | 查询方向：`true` 向后（向未来），`false` 向前（向过去） |
| `time` |  | string \| null | Reference datetime (yyyy-mm-ddTHH:MM:SS), omit to start from latest | 参考时间（`yyyy-mm-ddTHH:MM:SS`），省略则以最新时间为起点 |
| `count` | ✅ | integer | Number of candlesticks (max 1000) | K 线数量（最多 1000） |
| `trade_sessions` | ✅ | string | Trade sessions: "intraday" (regular hours only) or "all" (include pre-market and post-market) | 交易时段：`intraday`（仅常规时段）或 `all`（含盘前盘后） |

### `history_market_temperature`

- **EN**: Get historical market temperature time series
- **中文**: 获取市场情绪温度的历史时间序列

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `market` | ✅ | string | Market code: HK, US, CN, SG | 市场代码：HK、US、CN、SG |
| `start` | ✅ | string | Start date (yyyy-mm-dd) | 起始日期（`yyyy-mm-dd`） |
| `end` | ✅ | string | End date (yyyy-mm-dd) | 结束日期（`yyyy-mm-dd`） |

### `industry_valuation`

- **EN**: Get industry valuation comparison for peers
- **中文**: 获取同行业可比公司的估值对比

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `industry_valuation_dist`

- **EN**: Get industry PE/PB/PS valuation distribution
- **中文**: 获取行业 PE / PB / PS 估值分布

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `institution_rating`

- **EN**: Get institution rating summary with analyst consensus and target price
- **中文**: 获取机构评级汇总，含一致评级与目标价

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `institution_rating_detail`

- **EN**: Get detailed historical institution ratings and target price history
- **中文**: 获取详细的机构评级与目标价历史

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `intraday`

- **EN**: Get intraday minute-by-minute price/volume data. trade_sessions: "intraday" (default, regular hours) or "all" (include pre-market and post-market)
- **中文**: 获取分时（逐分钟）价格 / 成交数据。`trade_sessions` 可选 `intraday`（默认，仅常规时段）或 `all`（含盘前盘后）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |
| `trade_sessions` |  | string \| null | Trade sessions to include: "intraday" (default, regular hours only) or "all" (include pre-market and post-market). | 包含的交易时段：`intraday`（默认，仅常规时段）或 `all`（含盘前盘后） |

### `invest_relation`

- **EN**: Get investor relations events and announcements
- **中文**: 获取投资者关系事件与公告

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `market_status`

- **EN**: Get current market trading status for all markets
- **中文**: 获取所有市场当前的交易状态

_(no parameters / 无参数)_

### `market_temperature`

- **EN**: Get current market sentiment temperature (0-100)
- **中文**: 获取当前市场情绪温度（0-100）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `market` | ✅ | string | Market code: HK, US, CN, SG | 市场代码：HK、US、CN、SG |

### `news`

- **EN**: Get latest news articles for a symbol
- **中文**: 获取证券相关的最新新闻

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `now`

- **EN**: Get current UTC time
- **中文**: 获取当前 UTC 时间

_(no parameters / 无参数)_

### `operating`

- **EN**: Get company operating metrics
- **中文**: 获取公司经营指标

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `option_chain_expiry_date_list`

- **EN**: Get option chain expiry dates for a symbol
- **中文**: 获取期权链可选的到期日列表

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `option_chain_info_by_date`

- **EN**: Get option chain strike prices and Greeks for an expiry date
- **中文**: 获取指定到期日的期权链行权价及希腊字母

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string |  | 证券代码 |
| `date` | ✅ | string | Date (yyyy-mm-dd) | 到期日（`yyyy-mm-dd`） |

### `option_quote`

- **EN**: Get option quotes (max 500 symbols)
- **中文**: 获取期权行情（最多 500 个标的）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbols` | ✅ | array | Security symbols, e.g. `["700.HK", "AAPL.US"]` | 证券代码，例如 `["700.HK", "AAPL.US"]` |

### `option_volume`

- **EN**: Get real-time option call/put volume and put/call ratio for a US stock
- **中文**: 获取美股的实时期权认购 / 认沽成交量及 PCR

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Underlying symbol (US market only), e.g. "AAPL.US" | 标的代码（仅美股），例如 `"AAPL.US"` |

### `option_volume_daily`

- **EN**: Get daily historical option call/put volume, open interest, and put/call ratios for a US stock
- **中文**: 获取美股的逐日期权认购 / 认沽成交量、未平仓量及 PCR 历史

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Underlying symbol (US market only), e.g. "AAPL.US" | 标的代码（仅美股），例如 `"AAPL.US"` |
| `count` |  | integer \| null | Number of trading days to return (default 20) | 返回的交易日数量（默认 20） |

### `participants`

- **EN**: Get market participant broker information
- **中文**: 获取市场参与者（券商）信息

_(no parameters / 无参数)_

### `quote`

- **EN**: Get latest price quotes (last_done, open, high, low, volume, turnover)
- **中文**: 获取最新行情快照（最新价、开盘、最高、最低、成交量、成交额）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbols` | ✅ | array | Security symbols, e.g. `["700.HK", "AAPL.US"]` | 证券代码，例如 `["700.HK", "AAPL.US"]` |

### `shareholder`

- **EN**: Get institutional shareholders for a symbol
- **中文**: 获取证券的机构股东信息

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `sharelist_add`

- **EN**: Add securities to a community sharelist
- **中文**: 向社区分享股单中添加证券

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `id` | ✅ | string | Sharelist ID | 分享股单 ID |
| `symbols` | ✅ | array | Security symbols, e.g. `["AAPL.US", "700.HK"]` | 证券代码，例如 `["AAPL.US", "700.HK"]` |

### `sharelist_create`

- **EN**: Create a new community sharelist
- **中文**: 新建一个社区分享股单

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `name` | ✅ | string | List name (also used as description if `description` is omitted). | 股单名称（若未传 `description`，名称同时作为描述） |
| `description` |  | string \| null | List description. Defaults to `name` when omitted. | 股单描述，省略时默认与 `name` 相同 |

### `sharelist_delete`

- **EN**: Delete a community sharelist by id
- **中文**: 按 id 删除社区分享股单

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `id` | ✅ | string | Sharelist ID | 分享股单 ID |

### `sharelist_detail`

- **EN**: Get community sharelist detail including constituent stocks and quotes by id
- **中文**: 按 id 获取社区分享股单详情，含成分股及行情

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `id` | ✅ | string | Sharelist ID | 分享股单 ID |

### `sharelist_list`

- **EN**: List user's own and subscribed community sharelists
- **中文**: 列出用户自建以及已订阅的社区分享股单

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `count` |  | integer \| null | Number of lists to return (default 20) | 返回数量（默认 20） |

### `sharelist_popular`

- **EN**: Get popular/trending community sharelists
- **中文**: 获取热门 / 正在流行的社区分享股单

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `count` |  | integer \| null | Number of lists to return (default 20) | 返回数量（默认 20） |

### `sharelist_remove`

- **EN**: Remove securities from a community sharelist
- **中文**: 从社区分享股单中移除证券

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `id` | ✅ | string | Sharelist ID | 分享股单 ID |
| `symbols` | ✅ | array | Security symbols, e.g. `["AAPL.US", "700.HK"]` | 证券代码，例如 `["AAPL.US", "700.HK"]` |

### `sharelist_sort`

- **EN**: Reorder securities in a community sharelist (provide symbols in desired order)
- **中文**: 调整社区分享股单中证券的顺序（按目标顺序传入 `symbols`）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `id` | ✅ | string | Sharelist ID | 分享股单 ID |
| `symbols` | ✅ | array | Security symbols, e.g. `["AAPL.US", "700.HK"]` | 证券代码，例如 `["AAPL.US", "700.HK"]` |

### `short_positions`

- **EN**: Get short interest data for a US stock (short ratio, short shares, days to cover). Only US market is supported.
- **中文**: 获取美股的做空数据（空头比率、空头股数、回补天数等），仅支持美股市场

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol (US market only), e.g. "AAPL.US" | 证券代码（仅美股），例如 `"AAPL.US"` |
| `count` |  | integer \| null | Number of records to return (1-100, default 20) | 返回条数（1-100，默认 20） |

### `static_info`

- **EN**: Get basic information of securities (name, exchange, type, lot_size)
- **中文**: 获取证券的基础信息（名称、交易所、类型、每手股数等）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbols` | ✅ | array | Security symbols, e.g. `["700.HK", "AAPL.US"]` | 证券代码，例如 `["700.HK", "AAPL.US"]` |

### `topic`

- **EN**: Get discussion topics for a symbol
- **中文**: 获取与某证券相关的讨论

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `topic_create`

- **EN**: Create a new discussion topic. topic_type="post" (default) is plain text; "article" requires a non-empty title and accepts Markdown body.
- **中文**: 发布讨论。`topic_type="post"`（默认）为纯文本；`"article"` 需要非空 `title`，正文可使用 Markdown

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `title` | ✅ | string | Topic title. Required when topic_type is "article", optional for "post". | 讨论标题。`topic_type="article"` 时必填，`"post"` 时可选 |
| `body` | ✅ | string | Topic body. "post" type is plain text only; "article" type accepts Markdown. | 讨论正文。`post` 仅支持纯文本，`article` 支持 Markdown |
| `symbols` |  | array \| null | Related security symbols, e.g. `["700.HK", "TSLA.US"]` (max 10). | 相关证券代码，例如 `["700.HK", "TSLA.US"]`（最多 10 个） |
| `topic_type` |  | string \| null | Topic type: "post" (default, plain text) or "article" (Markdown, title required). | 讨论类型：`post`（默认，纯文本）或 `article`（Markdown，需 `title`） |

### `topic_create_reply`

- **EN**: Create a reply to a discussion topic. Pass reply_to_id to nest under another reply; omit for a top-level reply.
- **中文**: 对讨论发表回复。传入 `reply_to_id` 表示对某条回复的子级回复，省略则为顶层回复

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `topic_id` | ✅ | string | Topic ID to reply to. | 待回复的讨论 ID |
| `body` | ✅ | string | Reply body (plain text only). | 回复正文（仅支持纯文本） |
| `reply_to_id` |  | string \| null | Optional parent reply ID for nested replies. Get IDs from `topic_replies`. Omit for a top-level reply. | 可选的父回复 ID，用于楼中楼回复，从 `topic_replies` 获取；省略则为顶层回复 |

### `topic_detail`

- **EN**: Get discussion topic detail by topic_id
- **中文**: 按 `topic_id` 获取讨论详情

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `topic_id` | ✅ | string | Topic ID | 讨论 ID |

### `topic_replies`

- **EN**: Get replies to a discussion topic, paginated (page default 1, size default 20, range 1-50)
- **中文**: 分页获取讨论的回复（`page` 默认 1，`size` 默认 20，范围 1-50）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `topic_id` | ✅ | string | Topic ID. | 讨论 ID |
| `page` |  | integer \| null | Page number, 1-based (default: 1). | 页码（从 1 开始，默认 1） |
| `size` |  | integer \| null | Records per page, 1-50 (default: 20). | 每页条数（1-50，默认 20） |

### `trade_stats`

- **EN**: Get trade statistics (buy/sell/neutral volume distribution)
- **中文**: 获取成交统计（主动买 / 主动卖 / 中性盘的成交量分布）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `trades`

- **EN**: Get recent trades (max 1000)
- **中文**: 获取最近的逐笔成交（最多 1000 条）

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string |  | 证券代码 |
| `count` | ✅ | integer | Maximum number of results (max 1000) | 返回的最大条数（最多 1000） |

### `trading_days`

- **EN**: Get trading days for a market between dates
- **中文**: 获取指定市场在某日期区间内的交易日

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `market` | ✅ | string | Market code: HK, US, CN, SG | 市场代码：HK、US、CN、SG |
| `start` | ✅ | string | Start date (yyyy-mm-dd) | 起始日期（`yyyy-mm-dd`） |
| `end` | ✅ | string | End date (yyyy-mm-dd) | 结束日期（`yyyy-mm-dd`） |

### `trading_session`

- **EN**: Get trading session schedule for all markets
- **中文**: 获取所有市场的交易时段安排

_(no parameters / 无参数)_

### `valuation`

- **EN**: Get valuation overview with peer comparison
- **中文**: 获取估值概览及同业对比

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `valuation_history`

- **EN**: Get detailed valuation history time series
- **中文**: 获取详细的估值历史时间序列

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Security symbol, e.g. "700.HK" | 证券代码，例如 `"700.HK"` |

### `warrant_issuers`

- **EN**: Get warrant issuer information
- **中文**: 获取窝轮 / 牛熊证发行商信息

_(no parameters / 无参数)_

### `warrant_list`

- **EN**: Get filtered warrant list for an underlying symbol
- **中文**: 按筛选条件获取指定标的的窝轮 / 牛熊证列表

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbol` | ✅ | string | Underlying symbol, e.g. "700.HK" | 标的代码，例如 `"700.HK"` |
| `sort_by` | ✅ | string | Sort field: LastDone, ChangeRate, ChangeValue, Volume, Turnover, ExpiryDate, StrikePrice, UpperStrikePrice, LowerStrikePrice, OutstandingQuantity, OutstandingRatio, Premium, ItmOtm, ImpliedVolatility, Delta | 排序字段：`LastDone`、`ChangeRate`、`ChangeValue`、`Volume`、`Turnover`、`ExpiryDate`、`StrikePrice`、`UpperStrikePrice`、`LowerStrikePrice`、`OutstandingQuantity`、`OutstandingRatio`、`Premium`、`ItmOtm`、`ImpliedVolatility`、`Delta` |
| `sort_order` | ✅ | string | Sort order: Ascending or Descending | 排序方向：`Ascending`（升序）或 `Descending`（降序） |
| `warrant_type` |  | array \| null | Filter by warrant type (optional): "Call", "Put", "Bull", "Bear", "Inline" | 按品种筛选（可选）：`Call`（认购证）、`Put`（认沽证）、`Bull`（牛证）、`Bear`（熊证）、`Inline`（界内证） |
| `issuer` |  | array \| null | Filter by issuer ID (optional), use issuer_id from warrant_issuers tool | 按发行商 ID 筛选（可选），`issuer_id` 来自 `warrant_issuers` |
| `expiry_date` |  | array \| null | Filter by expiry date range (optional): "LT_3" (<3 months), "Between_3_6" (3-6 months), "Between_6_12" (6-12 months), "GT_12" (>12 months) | 按到期时段筛选（可选）：`LT_3`（<3 个月）、`Between_3_6`（3-6 个月）、`Between_6_12`（6-12 个月）、`GT_12`（>12 个月） |
| `price_type` |  | array \| null | Filter by in/out of bounds (optional): "In" (in bounds), "Out" (out of bounds). Only for Inline warrants. | 按界内 / 界外筛选（可选）：`In`（界内）、`Out`（界外），仅适用于界内证 |
| `status` |  | array \| null | Filter by status (optional): "Suspend" (suspended), "PrepareList" (pending listing), "Normal" (normal trading) | 按状态筛选（可选）：`Suspend`（停牌）、`PrepareList`（待上市）、`Normal`（正常交易） |

### `warrant_quote`

- **EN**: Get warrant quotes
- **中文**: 获取窝轮 / 牛熊证行情

| Param | Req | Type | English | 中文 |
|-------|-----|------|---------|------|
| `symbols` | ✅ | array | Security symbols, e.g. `["700.HK", "AAPL.US"]` | 证券代码，例如 `["700.HK", "AAPL.US"]` |
