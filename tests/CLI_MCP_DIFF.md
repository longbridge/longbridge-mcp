# Longbridge Terminal CLI vs MCP 对照

- 日期: 2026-04-21
- CLI 仓库: `/Users/hogan/work/longbridge/developers/longbridge-terminal`
- MCP 仓库: `/Users/hogan/work/longbridge/longbridge-mcp`（当前分支 `feat/change_description_compatible_input_params`）

## 一句话结论

- CLI 有 **120** 个 `cmd_*` 函数（含 4 个 dispatcher），MCP 有 **107** 个 tool 函数；两者覆盖度接近。
- CLI 走 SDK 和 Longbridge 私有 HTTP 几乎各半，额外用外部 HTTP 抓 SEC / 站点内容。
- MCP 定位是 Longbridge API 代理，不抓外部站点，因此 CLI 里依赖 SEC EDGAR / 富文本站点的命令（`insider_trades`、`investors_*`、`news_detail`、`filing_detail`）在 MCP 侧没有直接对应。
- 真正**业务上缺口**的 MCP tool 可控制在 10 来个（见末尾"补齐建议"）。

## CLI `cmd_*` 按传输方式分类

| 分类 | 数量 | 占比 | 代表命令 |
|------|------|------|----------|
| **SDK**（`longbridge::XxxContext`） | 49 | 41% | `cmd_quote` / `cmd_submit_order` / `cmd_positions` / `cmd_kline` / `cmd_watchlist_list` |
| **Longbridge 私有 HTTP**（`crate::openapi::http_client()` REST） | 56 | 47% | `cmd_financial_report` / `cmd_dividend` / `cmd_sharelist_list` / `cmd_dca_list` / `cmd_alert_list` |
| **SDK + LB HTTP 混合** | 2 | 2% | `cmd_filing_detail`（SEC + SDK）、`cmd_statement_export`（SDK get URL → HTTP 下载） |
| **外部 HTTP**（SEC EDGAR / 站点抓取） | 6 | 5% | `cmd_insider_trades` / `cmd_investors_list` / `cmd_investor_changes` / `cmd_investor_holdings_by_cik` / `cmd_news_detail` / `cmd_topic_detail`（外链正文） |
| **Dispatcher**（仅转发到子命令） | 4 | 3% | `cmd_dca` / `cmd_sharelist` / `cmd_watchlist` / `cmd_statement` |
| **本地只读**（token / 连通检查） | 3 | 2% | `cmd_auth_status` / `cmd_check` |

SDK 和 LB HTTP 合计 **91%**（105 个），是 CLI 主要的能力来源。

## MCP 按传输方式分类

| 分类 | 数量 |
|------|------|
| SDK | 51 |
| LB HTTP | 54 |
| 其他（`tool_json` 直接构造，如 `now`） | 2 |

与 CLI 极度接近，说明 MCP 基本是 CLI SDK+HTTP 能力的 1:1 映射。

## CLI 有但 MCP 无（差集）

> 去除"同义/复数差异"（如 `fund_holders` vs `fund_holder`）和"CLI subcommand 被 MCP 拆成多个独立 tool"（如 `option quote` + `option chain`）。

### 业务数据类（建议补齐）

| CLI 命令 | 说明 | 传输 | 为什么 MCP 没有 |
|----------|------|------|-----------------|
| `buyback` | 回购历史 | LB HTTP | 未实现 |
| `rating_history` | 评级历史 | LB HTTP | 未实现 |
| `valuation_detail` | 估值详情（含 peers 对比） | LB HTTP | MCP 用 `valuation_history`，数据结构不完全等价 |
| `history_intraday` | 指定日期的历史分时 | LB HTTP | MCP 的 `intraday` 只返当日 |
| `profit_analysis_by_market` | 按市场拆分的盈亏 | LB HTTP | 未实现 |
| `assets` | 总资产概览（信息密度 > `account_balance`） | SDK（helper 聚合） | MCP 仅有 `account_balance` |
| `portfolio` | 综合风险 + 持仓 + 资产视图 | SDK（helper 聚合） | 同上 |

### 依赖外部源（MCP 定位不适合）

| CLI 命令 | 说明 | 传输 |
|----------|------|------|
| `insider_trades` | 美股内部人交易 | SEC EDGAR XML |
| `investors_list` | 13F 机构列表 | SEC EDGAR |
| `investor_changes` | 13F 持仓变动 | SEC EDGAR |
| `investor_holdings_by_cik` | 按 CIK 查持仓 | SEC EDGAR |
| `news_detail` | 新闻正文 | 长桥 CMS/外链 |
| `filing_detail` | 文件正文 | SEC 原始文档 + SDK 获取 URL |

这些 MCP 暂不考虑，AI agent 可用 WebFetch 等工具自行抓取。

### CLI 便利/聚合命令

| CLI 命令 | 说明 |
|----------|------|
| `topics_mine` | 我发的主题（CLI 专用过滤） |
| `watchlist pin` | 置顶股票（MCP `update_watchlist_group` 可能间接覆盖） |
| `dca calc_date` | 下次扣款日计算 |
| `dca set_reminder` | 设置扣款前提醒 |
| `subscriptions` | 当前 WebSocket 订阅（MCP 无常驻 WS，无对应概念） |
| `check` | token + API 连通性自检（CLI 运维用） |
| `auth_status` | 显示当前登录账号（CLI 运维用） |

### Dispatcher（CLI 特有）

- `cmd_dca` / `cmd_sharelist` / `cmd_watchlist` / `cmd_statement` 都是"无子命令默认调 list"的语法糖，MCP 不需要 dispatcher（每个 action 一个 tool）。

## MCP 有但 CLI 无直接命令

数量不多，且多为"CLI 把多个 action 合并到一个 subcommand 层级下，MCP 拆成独立 tool"：

| MCP tool | CLI 对应 |
|----------|----------|
| `option_chain_expiry_date_list` | CLI `option chain --list-dates`（参数形式） |
| `option_chain_info_by_date` | CLI `option chain --date` |
| `history_candlesticks_by_offset` | CLI `kline` 内部逻辑 |
| `history_candlesticks_by_date` | CLI `kline history` |
| `statement_download_url` | 新增，CLI 里对应 `statement export` 的 step 1 |

基本都是 API 层面的细分，不算功能缺失。

## 已知的 CLI 命令完整清单（120 个）

```
asset                : cmd_exchange_rate cmd_profit_analysis cmd_profit_analysis_by_market
                       cmd_profit_analysis_detail
auth                 : cmd_auth_status
check                : cmd_check
dca                  : cmd_dca cmd_list cmd_create cmd_update cmd_toggle cmd_records cmd_stats
                       cmd_calc_date cmd_check cmd_set_reminder
fundamental          : cmd_financial_report cmd_institution_rating cmd_institution_rating_detail
                       cmd_dividend cmd_dividend_detail cmd_forecast_eps cmd_consensus cmd_valuation
                       cmd_valuation_detail cmd_shareholders cmd_fund_holders cmd_finance_calendar
                       cmd_company cmd_executive cmd_buyback cmd_industry_valuation
                       cmd_industry_valuation_dist cmd_operating cmd_rating_history cmd_corp_action
                       cmd_invest_relation
insider_trades       : cmd_insider_trades
investors            : cmd_investors_list cmd_investor_holdings_by_cik cmd_investor_changes
news                 : cmd_news cmd_filings cmd_filing_detail cmd_topics cmd_topic_detail
                       cmd_news_detail
quote                : cmd_quote cmd_depth cmd_brokers cmd_trades cmd_intraday cmd_history_intraday
                       cmd_kline cmd_kline_history cmd_static cmd_calc_index cmd_capital_flow
                       cmd_capital_dist cmd_market_temp cmd_market_status cmd_security_list
                       cmd_participants cmd_subscriptions cmd_option_quote cmd_option_chain
                       cmd_warrant_quote cmd_warrant_list cmd_warrant_issuers cmd_trading_session
                       cmd_trading_days cmd_anomaly cmd_constituent cmd_ah_premium_kline
                       cmd_ah_premium_intraday cmd_broker_holding_top cmd_broker_holding_detail
                       cmd_broker_holding_daily cmd_trade_stats
sharelist            : cmd_sharelist cmd_list cmd_detail cmd_create cmd_delete cmd_add cmd_remove
                       cmd_sort cmd_popular
statement            : cmd_statement cmd_list cmd_export
topic                : cmd_topic_detail_api cmd_topic_replies cmd_topics_mine cmd_create_topic
                       cmd_create_reply
trade                : cmd_portfolio cmd_assets cmd_positions cmd_fund_positions cmd_margin_ratio
                       cmd_max_qty cmd_cash_flow cmd_orders cmd_executions cmd_order_detail
                       cmd_submit_order cmd_replace_order cmd_cancel_order cmd_alert_list
                       cmd_alert_add cmd_alert_set_enabled cmd_alert_delete
watchlist            : cmd_watchlist cmd_list cmd_show cmd_create cmd_delete cmd_update cmd_pin
```

## 补齐 MCP 的建议

**P0**（AI agent 明显受益）

- `assets` / `portfolio`（聚合视图，比零散 account_balance + stock_positions + margin_ratio 更适合 LLM 一次性拿到上下文）
- `insider_trades`（if MCP 愿意扩展到外部数据源）

**P1**（数据补全）

- `buyback`（回购历史）
- `rating_history`（评级历史）
- `valuation_detail`（估值详情含 peers）
- `history_intraday`（指定历史日期的分时）
- `profit_analysis_by_market`（按市场拆盈亏）

**P2**（CLI 便利，AI 场景不是刚需）

- `topics_mine`、`watchlist_pin`、`dca_calc_date`、`dca_set_reminder`
- `check` / `auth_status`（MCP 有 `/metrics` 和 OAuth，不强需要）
- 外部抓取类（`news_detail` / `filing_detail` / `investors_*`）不做，由 agent 用 WebFetch 覆盖

## 附录：统计脚本

分析代码位于本次会话，基于以下模式识别：

- **SDK**: 正则匹配 `crate::openapi::(quote|trade|statement|content|quote_limited|trade_limited)\(\)` 或 `crate::openapi::helpers::`
- **LB HTTP**: 文件导入了 `super::api::http_get/post/put/delete` 或本地定义 `fn http_get(...)` 且调用 `crate::openapi::http_client()`
- **External**: 匹配 `reqwest::Client::(new|builder)` 或 `sec_client()`
- **Dispatcher**: 函数体为 `match ... => cmd_xxx(...)`

脚本产出：CLI 共 120 个 `cmd_*` 函数，MCP 共 107 个 tool 函数。
