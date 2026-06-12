# Longbridge MCP — OpenAI Tool Justification Answers

Paste each line into the matching box of the OpenAI submission form's **Tool justification** section. Covers all **146** tools; each has the three required fields (Read Only / Open World / Destructive). The `(True)`/`(False)` in each label is the value your MCP server reports — match it to the value shown on the form card.

> Note: these justifications are author-supplied prose for the reviewer; the MCP server itself only supplies the boolean annotation values.

## `account_balance`

- **Read Only (True):** Calls `TradeContext::account_balance(currency)`, a pure getter that returns cash balance and asset summary. It accepts only an optional currency filter and writes nothing to the account.
- **Open World (True):** It reaches the external Longbridge brokerage backend over the network. Balances reflect live market valuations and account activity outside this server's control, so successive reads may legitimately return different values.
- **Destructive (False):** It performs no update of any kind; there is no write path, so no data can be lost or overwritten.

## `ah_premium`

- **Read Only (True):** Returns the A/H share premium historical K-line (OHLC of the premium percentage); read-only.
- **Open World (True):** A/H premium data is derived from live cross-market prices via the Longbridge backend over the network.
- **Destructive (False):** Retrieves historical premium data only; no modification.

## `ah_premium_intraday`

- **Read Only (True):** Returns the intraday A/H premium time-share series (timestamp, premium_rate); pure read.
- **Open World (True):** Intraday premium values reflect live market prices from the Longbridge backend over the network.
- **Destructive (False):** Reads intraday premium data only; no state change.

## `alert_add`

- **Read Only (False):** Registers a new price alert on the account.
- **Open World (True):** Writes to the Longbridge alert service.
- **Destructive (False):** Additive; it adds an alert and does not remove or overwrite any existing alert.

## `alert_delete`

- **Read Only (False):** Deletes a price alert.
- **Open World (True):** Mutates the Longbridge alert service.
- **Destructive (True):** Removes an existing alert by id — irreversible removal.

## `alert_disable`

- **Read Only (False):** Sets an alert's enabled flag to `false`.
- **Open World (True):** Mutates the Longbridge alert service.
- **Destructive (False):** Reversible toggle; the alert is preserved and can be re-enabled.

## `alert_enable`

- **Read Only (False):** Sets an alert's enabled flag to `true` on the server.
- **Open World (True):** Mutates the Longbridge alert service.
- **Destructive (False):** A reversible toggle; it changes a status flag and does not delete or overwrite alert data (it can be turned off again with `alert_disable`).

## `alert_list`

- **Read Only (True):** `alert::alert_list` calls `http_get_tool` against `/v1/notify/reminders` and returns configured price alerts (`lists[]{counter_id, indicators[]{id, condition, price, frequency, enabled, triggered_at}}`). It only **lists** alerts; adding/deleting/enabling/disabling are `alert_add`/`alert_delete`/`alert_enable`/`alert_disable` (excluded from this group).
- **Open World (True):** Alerts are account-scoped records on the external Longbridge backend; their `triggered_at`/`enabled` state updates server-side.
- **Destructive (False):** Listing alerts changes no alert.

## `anomaly`

- **Read Only (True):** Returns market anomaly alerts (unusual price/volume changes) for a market or symbol; a pure read of detected anomalies.
- **Open World (True):** Anomaly detection runs externally and results are served live from the Longbridge backend over the network.
- **Destructive (False):** Reading alerts changes no state.

## `authenticate`

- **Read Only (False):** The call exchanges a one-time authorization code for an access token and establishes an authenticated session; it mutates session/credential state and unlocks the full tool set, so it is not read-only.
- **Open World (True):** Performs an OAuth token exchange against the Longbridge authorization backend.
- **Destructive (False):** It only establishes a new session (additive). It does not delete or overwrite any existing user data; an already-authenticated session is left untouched (the handler returns `already_authenticated` without altering credentials).

## `bank_cards`

- **Read Only (True):** Calls `http_get_tool(client, "/v1/account/bank-cards", &[])`, an HTTP GET that _lists_ the linked withdrawal bank cards (masked). It is a query of existing linked cards; it does not add, remove, or modify any card, and it does not initiate any transfer.
- **Open World (True):** Card data is served by the remote Longbridge `/v1/account/bank-cards` endpoint, outside this server's control.
- **Destructive (False):** Pure GET listing; no card is created/deleted/changed.

## `broker_holding`

- **Read Only (True):** Reads top broker holdings for an HK stock (broker_name, holding_quantity, holding_change, holding_ratio) for a period (rct_1/5/20/60), sourced from HKEX CCASS disclosure. Read-only.
- **Open World (True):** Data comes from external HKEX CCASS participant disclosure that updates daily; not server-controlled.
- **Destructive (False):** Reading CCASS broker holdings modifies nothing.

## `broker_holding_daily`

- **Read Only (True):** Reads a specific broker's daily holding history in a symbol (date, holding_quantity, holding_change, holding_ratio) from HKEX CCASS. Read-only fetch.
- **Open World (True):** Daily CCASS records accumulate from external HKEX disclosure; not server-controlled.
- **Destructive (False):** Reading the daily history modifies nothing.

## `broker_holding_detail`

- **Read Only (True):** Reads the full broker holding list for an HK stock (broker_id, broker_name, holding_quantity, holding_ratio, holding_change, date) from HKEX CCASS. Query only.
- **Open World (True):** Sourced from external HKEX CCASS disclosure updated daily; outside server control.
- **Destructive (False):** Reading the detail list writes nothing.

## `brokers`

- **Read Only (True):** Returns the HK broker queue (bid/ask broker IDs by position) for a symbol; a pure read of broker-queue data.
- **Open World (True):** Broker-queue data originates from HKEX via the Longbridge backend over the network and changes with the market.
- **Destructive (False):** Only reads queue data; nothing is written or deleted.

## `business_segments`

- **Read Only (True):** Reads the current-period business segment revenue breakdown (name, percent, total, currency). Query only.
- **Open World (True):** Segment data is sourced from external issuer disclosures and updates each period; not server-controlled.
- **Destructive (False):** Reading the segment breakdown writes nothing.

## `business_segments_history`

- **Read Only (True):** Reads historical segment revenue trends by period and category (date, total, currency, business[], regionals[]). Read-only fetch.
- **Open World (True):** Historical segment data extends with each external disclosure; outside server control.
- **Destructive (False):** Reading the historical trend modifies nothing.

## `calc_indexes`

- **Read Only (True):** Computes/returns requested financial indexes (e.g. PE, PB, LastDone, TurnoverRate) for the given symbols by reading market data; it performs no writes.
- **Open World (True):** Inputs are live market values fetched from the Longbridge backend over the network, so outputs change with the market.
- **Destructive (False):** The "calculation" is a derived read over market data; no environment state is modified.

## `cancel_order`

- **Read Only (False):** Cancels an open brokerage order.
- **Open World (True):** Mutates state on the Longbridge trade gateway.
- **Destructive (True):** Cancellation removes a pending order from the market — an irreversible state change to the order's lifecycle.

## `candlesticks`

- **Read Only (True):** Returns OHLCV candlestick data for a symbol/period; it only reads K-line data.
- **Open World (True):** Candlestick data comes from the Longbridge backend over the network and reflects external market activity.
- **Destructive (False):** Pure read of price history; no state is modified.

## `capital_distribution`

- **Read Only (True):** Returns capital-in/capital-out broken down by large/medium/small order size for a symbol; read-only.
- **Open World (True):** Distribution data is sourced from the Longbridge backend over the network and reflects live market flow.
- **Destructive (False):** Pure read of distribution data; no modification.

## `capital_flow`

- **Read Only (True):** Returns the same-day capital inflow/outflow/net-flow time series for a symbol; pure read.
- **Open World (True):** Capital-flow data is computed externally and served from the Longbridge backend over the network.
- **Destructive (False):** Reading capital-flow data writes nothing.

## `cash_flow`

- **Read Only (True):** Calls `TradeContext::cash_flow(opts)` over a date range, listing cash movement records (deposits, withdrawals, dividends). It reports past transactions; it does not initiate any cash movement.
- **Open World (True):** Records come from the remote Longbridge backend and reflect account activity outside this server's control.
- **Destructive (False):** Pure listing of existing records; no write or update.

## `company`

- **Read Only (True):** Reads the company profile (name, description, employees, CEO, founded_year, website, exchange, industry, market_cap, business summary). Read-only fetch.
- **Open World (True):** Company data is maintained externally on the Longbridge backend and updates with disclosures (e.g. market_cap moves daily); outside server control.
- **Destructive (False):** Reading the profile writes nothing.

## `consensus`

- **Read Only (True):** Reads consensus estimates for upcoming periods (period, revenue_estimate, eps_estimate, net_income_estimate, analyst_count, last_updated). Query only.
- **Open World (True):** Consensus figures aggregate external analyst estimates that revise over time on the Longbridge backend; not server-controlled.
- **Destructive (False):** Reading consensus estimates writes nothing.

## `constituent`

- **Read Only (True):** Returns index constituents or an ETF's asset allocation (holdings, regional, asset-class, industry breakdowns); pure lookup.
- **Open World (True):** Constituent and allocation data are maintained externally and fetched from the Longbridge backend over the network.
- **Destructive (False):** Reads constituent/allocation data only; nothing is modified.

## `corp_action`

- **Read Only (True):** Reads corporate actions (splits, buybacks, name changes) with action_type, effective_date, ratio, description. Query only.
- **Open World (True):** Corporate actions are external issuer disclosures that accrue over time; outside server control.
- **Destructive (False):** Reading the corporate-action log writes nothing.

## `create_watchlist_group`

- **Read Only (False):** Creates a new watchlist group (and optionally pre-populates securities) on the remote account.
- **Open World (True):** Writes to the Longbridge watchlist service.
- **Destructive (False):** Purely additive: it inserts a new group and never deletes or overwrites an existing one.

## `dca_check`

- **Read Only (True):** `dca::dca_check` POSTs `{counter_ids:[...]}` to `/v1/dailycoins/batch-check-support` and returns `items[]{symbol, support_dca, reason}`. Despite the POST, this is a stateless eligibility **check** (a lookup keyed by the symbol list); it creates no plan and stores nothing. The POST is only the transport for the batch of symbols.
- **Open World (True):** Support flags are determined by the external Longbridge backend (per-instrument eligibility rules).
- **Destructive (False):** Checking DCA support writes/deletes nothing.

## `dca_create`

- **Read Only (False):** Creates a recurring (DCA) investment plan on the account.
- **Open World (True):** Writes to the Longbridge DCA/trade service.
- **Destructive (False):** Additive; it creates a new plan and does not stop, overwrite, or delete any existing plan.

## `dca_history`

- **Read Only (True):** `dca::dca_history` calls `http_get_tool` against `/v1/dailycoins/query-records` to return a plan's execution history (`executions[]{date, quantity, amount, price, status, order_id}`). Read-only history query.
- **Open World (True):** Execution history is stored on the external Longbridge backend and grows as the plan executes.
- **Destructive (False):** Reading execution history changes nothing.

## `dca_list`

- **Read Only (True):** `dca::dca_list` calls `http_get_tool_unix` to list DCA (recurring investment) plans (`plans[]{plan_id, symbol, amount, frequency, status, next_execution_date}`). It only **lists** plans; creating/changing a plan is `dca_create`/`dca_update`/`dca_pause`/`dca_resume`/`dca_stop` (all excluded from this group with `read_only_hint = false`).
- **Open World (True):** DCA plans are account-scoped records on the external Longbridge backend, advanced server-side on schedule.
- **Destructive (False):** Listing plans creates/modifies/stops nothing.

## `dca_pause`

- **Read Only (False):** Suspends a DCA plan (stops execution until resumed).
- **Open World (True):** Mutates the Longbridge DCA service.
- **Destructive (False):** Reversible: the description explicitly pairs it with `dca_resume`; the plan is preserved, only its run state is suspended. (Contrast with the irreversible `dca_stop`, which is destructive.)

## `dca_resume`

- **Read Only (False):** Resumes a suspended DCA plan's scheduled execution.
- **Open World (True):** Mutates the Longbridge DCA service.
- **Destructive (False):** Reversible toggle (can be paused again); preserves the plan.

## `dca_stats`

- **Read Only (True):** `dca::dca_stats` calls `http_get_tool` against `/v1/dailycoins/statistic` and returns aggregate DCA statistics (`total_invested, total_value, total_return, ...`). Read-only aggregation.
- **Open World (True):** Statistics are computed server-side on the external Longbridge backend from live valuations.
- **Destructive (False):** Reading statistics modifies nothing.

## `dca_stop`

- **Read Only (False):** Permanently stops a DCA plan.
- **Open World (True):** Mutates the Longbridge DCA service.
- **Destructive (True):** The description states this "cannot be undone"; it terminates the plan irreversibly (distinct from the reversible `dca_pause`).

## `dca_update`

- **Read Only (False):** Updates an existing DCA plan by `plan_id` (amount/frequency/schedule).
- **Open World (True):** Mutates the Longbridge DCA service.
- **Destructive (True):** It overwrites the plan's existing configuration; prior values are replaced, so it can overwrite existing data.

## `delete_watchlist_group`

- **Read Only (False):** Deletes a watchlist group from the account.
- **Open World (True):** Mutates the Longbridge watchlist service.
- **Destructive (True):** It removes an existing group by id, and with `purge=true` also removes its securities from all other groups — irreversible data removal.

## `deposits`

- **Read Only (True):** Calls `http_get_tool(client, "/v1/account/deposits", ...)` with page/size/account_channel and optional states/currencies filters. This is an HTTP GET that _lists deposit history_; it does **not** initiate a deposit. Confirmed by the implementation: a GET against the history endpoint with filter/paging query params only.
- **Open World (True):** History is served by the remote Longbridge backend and reflects real deposit activity outside this server's control.
- **Destructive (False):** Listing past deposits changes nothing; no funds are moved.

## `depth`

- **Read Only (True):** Returns the order-book depth (bid/ask price levels with volume and order_num) for a symbol; it only reads the order book.
- **Open World (True):** Order-book depth is live exchange data delivered through the Longbridge backend over the network, controlled by the market.
- **Destructive (False):** Reading the book places no order and changes no state.

## `dividend`

- **Read Only (True):** Reads dividend history (ex_date, pay_date, record_date, dividend_type, amount, currency, status) via `GET /v1/quote/dividends`. Pure read.
- **Open World (True):** Dividend records are external issuer disclosures that extend as new distributions are announced; outside server control.
- **Destructive (False):** Reading the payout record changes nothing.

## `dividend_detail`

- **Read Only (True):** Reads the detailed distribution scheme (period, cash_dividend, stock_dividend, record_date, ex_date, pay_date, currency). Query only.
- **Open World (True):** Distribution details are sourced from external issuer disclosures and update as schemes are declared; not server-controlled.
- **Destructive (False):** Fetching distribution details writes nothing.

## `estimate_max_purchase_quantity`

- **Read Only (True):** Calls `TradeContext::estimate_max_purchase_quantity(opts)`. Despite taking order-like parameters (symbol, side, order_type, optional price), it only _estimates_ the maximum buy/sell quantity (returns `cash_max_qty`, `margin_max_qty`). No order is constructed or submitted — confirmed by the implementation, which builds `EstimateMaxPurchaseQuantityOptions` and calls the estimation getter, never `submit_order`. The parameters are inputs to a calculation, not an order placement.
- **Open World (True):** The estimate is computed by the remote Longbridge backend using live buying power and margin data, which vary over time outside this server.
- **Destructive (False):** It is a what-if calculation; no account state, position, or order is created or changed.

## `exchange_rate`

- **Read Only (True):** Returns current exchange rates for supported currency pairs; a pure read.
- **Open World (True):** FX rates are sourced externally and served live from the Longbridge backend over the network.
- **Destructive (False):** Reading FX rates writes nothing.

## `executive`

- **Read Only (True):** Reads executive and board information (name, title, appointed_date, age, biography, compensation). Query only.
- **Open World (True):** Executive data reflects external issuer disclosures and changes with appointments/departures; not server-controlled.
- **Destructive (False):** Reading executive records modifies nothing.

## `filings`

- **Read Only (True):** Returns regulatory filings (8-K, 10-Q, 10-K, etc.) metadata and URLs for a symbol; pure lookup.
- **Open World (True):** Filing data originates from regulators and is served from the Longbridge backend over the network.
- **Destructive (False):** Reads filing listings only; nothing is written or deleted.

## `finance_calendar`

- **Read Only (True):** Reads finance-calendar events (report/dividend/split/ipo/macrodata/closed) by category, market, and date range via `GET /v1/quote/finance_calendar`. Read-only.
- **Open World (True):** Calendar events (earnings dates, macro releases, holidays) are externally scheduled and revised as schedules change; outside server control.
- **Destructive (False):** Reading scheduled events writes nothing.

## `financial_report`

- **Read Only (True):** Performs `GET /v1/quote/financial-reports` to fetch income statement, balance sheet, and cash flow data (kind IS/BS/CF/ALL; report_type af/saf/q1-q3/qf). Pure read.
- **Open World (True):** Statement data is sourced from issuer filings on the external Longbridge backend and updates as new periods are disclosed; the server does not control it.
- **Destructive (False):** Retrieving published financial statements alters nothing.

## `financial_report_latest`

- **Read Only (True):** Reads the latest report summary (period, revenue, net_income, eps, roe, gross_margin, report_date). Query only.
- **Open World (True):** "Latest" reflects the most recent external disclosure on the Longbridge backend and advances as issuers report; not server-controlled.
- **Destructive (False):** Fetching a summary modifies no data.

## `financial_report_snapshot`

- **Read Only (True):** Reads a report snapshot: text summary (report*desc), actual-vs-forecast figures (fo_revenue/fo_ebit/fo_eps), and financial ratios (fr*\*: ROE, margins, assets, cash flow). Read-only fetch.
- **Open World (True):** Combines disclosed actuals and analyst forecasts from the external backend; both update with the market and disclosures.
- **Destructive (False):** No write occurs when reading the snapshot.

## `financial_statement`

- **Read Only (True):** Reads income statement, balance sheet, or cash flow (kind IS/BS/CF/ALL; report af/saf/qf/q1-q3) for a security via a backend GET. No state is changed.
- **Open World (True):** Backed by externally-disclosed issuer financials that change with each reporting period, outside server control.
- **Destructive (False):** Reading statement line items writes nothing.

## `forecast_eps`

- **Read Only (True):** Reads EPS forecast/estimate history (forecast_start_date, forecast_end_date, eps_estimate, eps_actual, surprise_pct, analyst_count). Read-only fetch.
- **Open World (True):** Estimates come from external analysts and actuals from issuer reports; both update on the backend with market events, outside server control.
- **Destructive (False):** Reading forecasts and actuals modifies nothing.

## `fund_holder`

- **Read Only (True):** Reads funds and ETFs holding a symbol (fund_name, fund_symbol, shares, ratio, change, reported_at). Read-only fetch.
- **Open World (True):** Fund-holding data derives from external filings and updates each reporting period; not server-controlled.
- **Destructive (False):** Reading fund holdings modifies nothing.

## `fund_positions`

- **Read Only (True):** Calls `TradeContext::fund_positions(None)`, a getter for current fund holdings. It is parameterless apart from the implicit filter and never writes.
- **Open World (True):** Backed by the external Longbridge backend; NAV and holding units update over time independently of this server.
- **Destructive (False):** No mutation; fund positions are only read.

## `history_candlesticks_by_date`

- **Read Only (True):** Reads historical candlesticks for a given date range; pure read.
- **Open World (True):** Historical data is retrieved from the Longbridge backend over the network, controlled by the exchange/Longbridge.
- **Destructive (False):** Retrieves historical price data only; no modification of state.

## `history_candlesticks_by_offset`

- **Read Only (True):** Reads historical candlesticks anchored by an offset from a reference time; lookup only.
- **Open World (True):** Historical candles are fetched from the Longbridge backend over the network; the underlying data is externally maintained.
- **Destructive (False):** Only retrieves historical K-line data; nothing is written.

## `history_executions`

- **Read Only (True):** Calls `TradeContext::history_executions` and `history_orders` (joined, `trade.rs:270`) to list historical fills and annotate side. Both are getters; nothing is mutated.
- **Open World (True):** Backed by the external Longbridge backend serving real historical trade records.
- **Destructive (False):** Only in-memory enrichment of read data; no upstream write.

## `history_market_temperature`

- **Read Only (True):** Returns the historical market-temperature time series for a market and date range; pure read.
- **Open World (True):** Historical sentiment data is fetched from the Longbridge backend over the network.
- **Destructive (False):** Retrieves historical sentiment data only; no state change.

## `history_orders`

- **Read Only (True):** Calls `TradeContext::history_orders(opts)` with a date range and optional symbol. It is a historical read; no order is touched.
- **Open World (True):** Historical order data resides on the remote Longbridge backend and is not controlled by this server.
- **Destructive (False):** No write/update/delete; orders are only listed.

## `industry_peers`

- **Read Only (True):** Reads the hierarchical sub-sector peer tree for an industry group (chain{name, counter_id, stock_num, chg, ytd_chg, next[]}, top{name, market}). Query only.
- **Open World (True):** Peer-group structure and its change figures track external market data; not server-controlled.
- **Destructive (False):** Reading the peer tree writes nothing.

## `industry_rank`

- **Read Only (True):** Reads an industry ranking list by market and indicator (leaders, trend, heat, market cap, revenue, profit, growth) returning counter_id, name, chg, lists[]. Read-only.
- **Open World (True):** Rankings recompute from external market data and shift daily; outside server control.
- **Destructive (False):** Reading the ranking modifies nothing.

## `industry_valuation`

- **Read Only (True):** Reads industry peer valuation (symbol, name, pe, pb, ps, dividend_yield, history) via a backend GET. Read-only.
- **Open World (True):** Peer valuations follow external market data and change daily; outside server control.
- **Destructive (False):** Reading peer multiples writes nothing.

## `industry_valuation_dist`

- **Read Only (True):** Reads the industry PE/PB/PS distribution (min, p25, median, p75, max, current_percentile). Query only.
- **Open World (True):** Distribution recomputes from external sector data as the market moves; not server-controlled.
- **Destructive (False):** Reading the distribution modifies nothing.

## `institution_rating`

- **Read Only (True):** Reads analyst rating summary (buy/outperform/hold/underperform/sell counts, target_price, consensus_rating) plus the instratings list via backend GETs. No mutation.
- **Open World (True):** Analyst ratings originate from external research firms and are updated on the Longbridge backend as firms publish; not server-controlled.
- **Destructive (False):** Reading the rating consensus changes nothing.

## `institution_rating_detail`

- **Read Only (True):** Reads per-institution historical ratings and target-price history (analyst, firm, rating, target_price, timestamp). Query only.
- **Open World (True):** Rating history is external research data that grows as firms issue new ratings; outside server control.
- **Destructive (False):** Retrieving historical ratings writes nothing.

## `institution_rating_history`

- **Read Only (True):** Reads target-price change history (firm, analyst, old_target, new_target, date) and rating-change history (firm, old_rating, new_rating, date). Read-only.
- **Open World (True):** Change events come from external analyst actions surfaced on the backend; they accrue with market activity, not server control.
- **Destructive (False):** No state is altered by reading the change log.

## `institution_rating_industry_rank`

- **Read Only (True):** Reads peers ranked by analyst ratings within the same industry (symbol, name, buy_count, sell_count, consensus_rating, target_price). Paginated read.
- **Open World (True):** Rankings derive from external analyst ratings across the industry and shift as ratings update; not server-controlled.
- **Destructive (False):** Ranking peers reads only; nothing is written.

## `institutional_views`

- **Read Only (True):** Reads the monthly institutional rating distribution timeline (date, buy, outperform, hold, underperform, sell, total). Query only.
- **Open World (True):** The timeline aggregates external analyst ratings that change monthly; not server-controlled.
- **Destructive (False):** Reading the rating timeline writes nothing.

## `intraday`

- **Read Only (True):** Returns intraday minute-by-minute price/volume series for a symbol; pure read.
- **Open World (True):** Intraday data is sourced live from the Longbridge backend over the network and updates with the trading session.
- **Destructive (False):** Reading the intraday line writes nothing and removes nothing.

## `invest_relation`

- **Read Only (True):** Reads investor relations events/announcements (title, event_type, event_date, url, description). Read-only fetch.
- **Open World (True):** IR events are externally published and grow as the company announces; not server-controlled.
- **Destructive (False):** Reading IR events modifies nothing.

## `ipo_calendar`

- **Read Only (True):** `ipo::ipo_calendar` calls `http_get_tool_unix` against `/v1/ipo/calendar` and returns scheduled/recent IPOs. Pure calendar read.
- **Open World (True):** The IPO calendar is served by the external Longbridge backend and updated as schedules change.
- **Destructive (False):** Reading the calendar modifies nothing.

## `ipo_detail`

- **Read Only (True):** `ipo::ipo_detail` issues `http_get_tool` reads for `/v1/ipo/profile`, `/v1/ipo/timeline`, and `/v1/ipo/eligibility` and assembles the detail view. It only reads IPO information; checking eligibility is informational and submits no order.
- **Open World (True):** IPO profile/timeline/eligibility data is hosted on the external Longbridge backend.
- **Destructive (False):** Reading detail/eligibility writes nothing.

## `ipo_listed`

- **Read Only (True):** `ipo::ipo_listed` issues two `http_get_tool` reads (`/v1/ipo/listed`, `/v1/ipo/us/listed`) for recently listed IPOs. Read-only.
- **Open World (True):** Listing data and first-day returns come from the external Longbridge backend.
- **Destructive (False):** Listing recently listed IPOs changes nothing.

## `ipo_order_detail`

- **Read Only (True):** `ipo::ipo_order_detail` does a single `http_get_tool` read for one IPO order by id (`{order_id, symbol, allotted_quantity, status, ...}`). Pure read of an existing order record.
- **Open World (True):** The order record lives on the external Longbridge backend and its status may update server-side.
- **Destructive (False):** Reading order detail changes nothing.

## `ipo_orders`

- **Read Only (True):** `ipo::ipo_orders` issues `http_get_tool` reads against `/v1/ipo/orders` and `/v1/ipo/orders/history` to list existing IPO orders. It **queries** the user's IPO application records; it does not place or amend an order.
- **Open World (True):** IPO orders are account-scoped records on the external Longbridge backend; their status changes server-side (e.g. allotment).
- **Destructive (False):** Listing orders modifies no order.

## `ipo_profit_loss`

- **Read Only (True):** `ipo::ipo_profit_loss` issues `http_get_tool` reads for `/v1/ipo/profit-loss` and `/v1/ipo/profit-loss/items` and returns a P/L summary and per-stock breakdown. Read-only reporting.
- **Open World (True):** P/L is derived server-side on the external Longbridge backend from live valuations.
- **Destructive (False):** Reading P/L figures modifies nothing.

## `ipo_subscriptions`

- **Read Only (True):** `ipo::ipo_subscriptions` issues two `http_get_tool` reads (`/v1/ipo/subscriptions` and `/v1/ipo/us/subscriptions`) and merges them. It lists IPOs in the subscription stage; it does **not** submit any subscription/application.
- **Open World (True):** The subscription pipeline is maintained on the external Longbridge backend and changes as IPOs open/close.
- **Destructive (False):** Listing subscription-stage IPOs writes nothing.

## `margin_ratio`

- **Read Only (True):** Calls `TradeContext::margin_ratio(symbol)`, returning initial/maintenance/forced-liquidation margin factors for a symbol. It is a lookup; nothing is changed.
- **Open World (True):** Margin factors are served by the remote Longbridge backend and may change per upstream risk policy, outside this server's control.
- **Destructive (False):** No write or update; the factors are read-only reference values.

## `market_status`

- **Read Only (True):** Returns the current trading status (Trading, Closed, Lunch Break, etc.) per market; pure read.
- **Open World (True):** Market status reflects live exchange state delivered by the Longbridge backend over the network.
- **Destructive (False):** Reading market status changes no state.

## `market_temperature`

- **Read Only (True):** Returns the current market sentiment temperature (temperature, valuation, sentiment, description, timestamp) for a market; read-only.
- **Open World (True):** The temperature is computed externally and served live from the Longbridge backend over the network.
- **Destructive (False):** Reads a computed sentiment metric; nothing is written.

## `news`

- **Read Only (True):** `content::news` issues a GET-style fetch for a symbol's latest news articles and returns `items[]{id, title, source, publish_time, summary, url, related_symbols}`. It only reads a news feed.
- **Open World (True):** News content comes from the external Longbridge backend and updates continuously as new articles are published.
- **Destructive (False):** Fetching articles writes or deletes nothing on the backend.

## `news_search`

- **Read Only (True):** `search::news_search` calls `http_get_tool` to run a keyword search and returns `news_list[]{id, title, description, source_name, publish_at, score}`. It is a search/read over the news corpus.
- **Open World (True):** Results are served by the external Longbridge search backend and shift as news is indexed.
- **Destructive (False):** A search query persists nothing and removes nothing.

## `now`

- **Read Only (True):** Returns the current UTC time as an RFC3339 string; it reads the system clock and writes nothing.
- **Open World (True):** The value reflects real wall-clock time outside this server's control, advancing independently of the server.
- **Destructive (False):** Reading the clock changes no state, so nothing can be overwritten or deleted.

## `operating`

- **Read Only (True):** Reads company operating metrics (HK stocks only) such as passenger traffic, cargo volumes, or store counts (period, metric_name, value, unit). Query only.
- **Open World (True):** Operating data is sourced from external issuer disclosures and updates each period; outside server control.
- **Destructive (False):** Reading operating metrics writes nothing.

## `option_chain_expiry_date_list`

- **Read Only (True):** Returns the list of option-chain expiry dates for a symbol; pure lookup.
- **Open World (True):** Expiry data is fetched from the Longbridge backend over the network and reflects externally listed contracts.
- **Destructive (False):** Only reads available expiries; nothing is written or deleted.

## `option_chain_info_by_date`

- **Read Only (True):** Returns the option chain (strikes with call/put quotes and Greeks) for an expiry date; read-only.
- **Open World (True):** Chain quotes and Greeks are live data from the Longbridge backend over the network and change with the market.
- **Destructive (False):** Retrieves chain data only; no state is modified.

## `option_quote`

- **Read Only (True):** Reads option quote data including Greeks (delta, gamma, theta, vega, rho), IV, and open_interest for up to 500 symbols; lookup only.
- **Open World (True):** Option quotes and Greeks are sourced live from the Longbridge backend over the network and vary with the market.
- **Destructive (False):** Pure read of option market data; no state is altered.

## `option_volume`

- **Read Only (True):** Returns real-time call/put volume stats (call/put volume, put_call_ratio, open interest, top contracts) for a US stock; pure read.
- **Open World (True):** Option-volume stats are live data from the Longbridge backend over the network and change with the market.
- **Destructive (False):** Reads option-volume stats only; nothing is written.

## `option_volume_daily`

- **Read Only (True):** Returns daily historical option volume/open-interest stats for a US stock; read-only.
- **Open World (True):** Daily option stats are fetched from the Longbridge backend over the network and reflect external market data.
- **Destructive (False):** Retrieves historical option stats only; no modification.

## `order_detail`

- **Read Only (True):** Calls `TradeContext::order_detail(order_id)`, fetching the details of one order by id. It only reads.
- **Open World (True):** The order record lives on the remote brokerage backend and reflects real-time execution state outside this server.
- **Destructive (False):** No mutation of the order; status/quantities are reported, not changed.

## `participants`

- **Read Only (True):** Returns the HK market participant directory (broker_ids mapped to names); a static reference lookup.
- **Open World (True):** The participant directory is maintained externally and retrieved from the Longbridge backend over the network.
- **Destructive (False):** Read-only reference fetch; no modification of any state.

## `profit_analysis`

- **Read Only (True):** Reads a portfolio profit-and-loss analysis summary over an optional date range. Query only; it computes/reads results without changing account state.
- **Open World (True):** The analysis is computed on the external Longbridge backend from account and market data that update over time; not server-controlled.
- **Destructive (False):** Reading P&L analysis writes nothing to the account.

## `profit_analysis_detail`

- **Read Only (True):** Reads detailed per-symbol profit-and-loss analysis over an optional date range. Read-only fetch.
- **Open World (True):** Computed on the external backend from account and market data that change over time; outside server control.
- **Destructive (False):** Reading per-symbol P&L modifies no account state.

## `quant_run`

- **Read Only (True):** `quant::run_script` fetches historical K-line data for the symbol/period/date-range and runs the supplied indicator **script** server-side, returning the computed indicator/plot values as JSON. Although it issues a `POST` to `/v1/quant/run_script`, the request body is just the input (counter_id, time range, line_type, the script source, and `input_json`); the call is a stateless computation that reads K-line data and returns derived values. It persists nothing and modifies no account, watchlist, plan, or market state. The script runs against historical data only.
- **Open World (True):** The computation depends on live/historical K-line data served by the external Longbridge backend, which is not under this server's control.
- **Destructive (False):** Running the indicator script writes/deletes nothing on the backend; it produces a computed result and discards all working state.

## `quote`

- **Read Only (True):** Returns the latest price quote fields (last_done, open/high/low, volume, turnover, change, trade_status, timestamp) per symbol; it only reads live quotes.
- **Open World (True):** Quote values come from the live Longbridge quote feed over the network and change continuously with the market, outside server control.
- **Destructive (False):** Reading quotes does not write, overwrite, or delete anything.

## `rank_categories`

- **Read Only (True):** `market::rank_categories` calls `http_get_tool` against `/v1/quote/market/rank/categories` and returns the leaderboard tab configuration (`first_tags[]{key, name, second_tags[]}`). Read-only configuration fetch.
- **Open World (True):** Leaderboard category config is served by the external Longbridge backend.
- **Destructive (False):** Reading category config modifies nothing.

## `rank_list`

- **Read Only (True):** `market::rank_list` calls `http_get_tool` to fetch the ranked stock list for a leaderboard tab key (`lists[]{symbol, name, last_done, chg, ...}`). It only reads the leaderboard.
- **Open World (True):** Rankings are computed live on the external Longbridge backend and change with market activity (`updated_at` is returned).
- **Destructive (False):** Reading rankings changes no state.

## `replace_order`

- **Read Only (False):** Modifies an existing open order's quantity / price / trigger / trailing parameters.
- **Open World (True):** Mutates state on the Longbridge trade gateway.
- **Destructive (True):** It overwrites the existing order's terms in place; the prior order parameters are replaced and not recoverable from this call.

## `screener_indicators`

- **Read Only (True):** `screener::screener_indicators` calls `http_get_tool` against `/v1/quote/ai/screener/indicators` to return indicator metadata (`groups[]{group_name, indicators[]{id, key, name, unit, default_range, tech_values}}`). Pure metadata read.
- **Open World (True):** Indicator metadata is supplied by the external Longbridge backend.
- **Destructive (False):** Reading the indicator catalog changes nothing.

## `screener_recommend_strategies`

- **Read Only (True):** `screener::screener_recommend_strategies` calls `http_get_tool` to list platform-preset screener strategies (`strategys[]{id, name, description, market, three_months_chg, risk}`). It only reads strategy metadata.
- **Open World (True):** Strategy presets and their `three_months_chg` figures are served by the external Longbridge backend and updated server-side.
- **Destructive (False):** Listing presets changes no state.

## `screener_search`

- **Read Only (True):** `screener::screener_search` runs a screener query and returns `{total, items[]{symbol, name, indicators[]}}`. In Mode A it first does a `GET` to load a strategy, then submits the filter set; the actual screen is a `POST` to `/v1/quote/ai/screener/search`. The POST is purely the transport for the filter conditions of a stateless search computation — it stores no strategy and mutates no account or universe data.
- **Open World (True):** The screen runs against the live Longbridge universe and indicator values on the external backend, which change with the market.
- **Destructive (False):** Executing a screen creates/deletes nothing on the backend.

## `screener_strategy`

- **Read Only (True):** `screener::screener_strategy` calls `http_get_tool` to fetch a single strategy's filter conditions (`market`, `filter{filters[]{key, min, max, tech_values}}`). It inspects a strategy; it does not run or change it.
- **Open World (True):** Strategy definitions are stored on the external Longbridge backend.
- **Destructive (False):** Inspecting filter conditions writes nothing.

## `screener_user_strategies`

- **Read Only (True):** `screener::screener_user_strategies` calls `http_get_tool` to list the current user's saved screener strategies. It reads saved-strategy metadata only; it does not create or modify strategies.
- **Open World (True):** The saved list is account-scoped data held on the external Longbridge backend.
- **Destructive (False):** Listing the user's saved strategies modifies nothing.

## `security_list`

- **Read Only (True):** Returns a paginated security list for a market/category; a directory lookup.
- **Open World (True):** The security universe is maintained externally and served from the Longbridge backend over the network.
- **Destructive (False):** Reads the listing only; nothing is written or removed.

## `shareholder`

- **Read Only (True):** Reads institutional shareholders (institution, shares, ratio, change, change_type, reported_at). Read-only fetch.
- **Open World (True):** Holdings come from external regulatory filings (e.g. 13F) and update each reporting period; outside server control.
- **Destructive (False):** Reading holdings writes nothing.

## `shareholder_detail`

- **Read Only (True):** Reads a single holder's holding and trade history by object_id (name, owner_source, tradings with accum_buy/accum_sell/net_buy and trading_details, holding/trading summaries). Query only.
- **Open World (True):** Holder activity comes from external filings (13F / Form 4) and grows with new disclosures; outside server control.
- **Destructive (False):** Reading a holder's history writes nothing.

## `shareholder_top`

- **Read Only (True):** Reads the Top 20 major shareholders across reporting periods (period, object_id, name, title, shares_held, percent_shares_held, shares_changed, filing_date). Read-only.
- **Open World (True):** Top-holder data is sourced from external filings and revises each reporting period; not server-controlled.
- **Destructive (False):** Reading the top-holder list modifies nothing.

## `sharelist_add`

- **Read Only (False):** Adds securities to an existing community sharelist.
- **Open World (True):** Writes to the Longbridge community sharelist service.
- **Destructive (False):** Additive; it appends symbols and does not remove or reorder existing constituents.

## `sharelist_create`

- **Read Only (False):** Creates a new community sharelist (name + optional description).
- **Open World (True):** Writes to the Longbridge community sharelist service.
- **Destructive (False):** Additive; it creates a new list and does not delete or overwrite existing lists.

## `sharelist_delete`

- **Read Only (False):** Deletes a community sharelist (own lists only).
- **Open World (True):** Mutates the Longbridge community sharelist service.
- **Destructive (True):** Removes an existing list — irreversible removal of the list and its membership.

## `sharelist_detail`

- **Read Only (True):** `sharelist::sharelist_detail` calls `http_get_tool` to fetch one sharelist by id, including its constituents and quote data. Pure read.
- **Open World (True):** Sharelist content (constituents, live quotes, subscription status) is served by the external Longbridge backend.
- **Destructive (False):** Reading a sharelist's detail changes nothing.

## `sharelist_list`

- **Read Only (True):** `sharelist::sharelist_list` calls `http_get_tool` against `/v1/sharelists` to list the user's own and subscribed community sharelists. Read-only listing (create/delete/add/remove/sort are excluded from this group).
- **Open World (True):** Sharelists are community data on the external Longbridge backend, changed by their owners/followers.
- **Destructive (False):** Listing sharelists modifies none of them.

## `sharelist_popular`

- **Read Only (True):** `sharelist::sharelist_popular` calls `http_get_tool` against `/v1/sharelists/popular` and returns trending sharelists sorted by popularity. Read-only.
- **Open World (True):** Popularity rankings are computed on the external Longbridge backend and shift with community activity.
- **Destructive (False):** Reading popular lists modifies nothing.

## `sharelist_remove`

- **Read Only (False):** Removes securities from a community sharelist.
- **Open World (True):** Mutates the Longbridge community sharelist service.
- **Destructive (True):** Removes constituents from an existing list — a removal/deletion operation on existing data.

## `sharelist_sort`

- **Read Only (False):** Reorders securities within a community sharelist.
- **Open World (True):** Mutates the Longbridge community sharelist service.
- **Destructive (True):** It overwrites the list's existing ordering with the supplied order — the previous arrangement is replaced.

## `short_margin`

- **Read Only (True):** Reads short margin deposit details for the current account (margin_amount, margin_rate, interest_rate, symbol, quantity per position) via `GET /v1/asset/cash/short-margin`. Read-only.
- **Open World (True):** Margin figures are maintained on the external Longbridge backend and update with positions, rates, and market data; outside server control.
- **Destructive (False):** Reading short-margin details modifies no account or position state.

## `short_positions`

- **Read Only (True):** Returns short-interest history (open short positions, ratio, days_to_cover, etc.) for HK/US stocks; read-only.
- **Open World (True):** Short-interest data originates from FINRA (US) and HKEX (HK) and is served from the Longbridge backend over the network.
- **Destructive (False):** Reads short-interest data only; no state change.

## `short_trades`

- **Read Only (True):** Reads daily short-sale volume history for HK or US stocks (timestamp, short_vol, rate, close; US-only nasdaq_vol/nyse_vol; HK-only balance/market_vol). Read-only.
- **Open World (True):** Data is sourced externally (US: FINRA/NASDAQ daily; HK: HKEX daily) and extends each trading day; not server-controlled.
- **Destructive (False):** Reading short-sale history writes nothing.

## `statement_export`

- **Read Only (True):** Calls `AssetContext::statement_download_url(options)`. It only retrieves a pre-signed download URL for an existing statement file (keyed by `file_key` from `statement_list`); it returns `{url}` and does nothing else. It does not generate, alter, or move the statement, and it does not even download the file itself.
- **Open World (True):** The URL is minted by the remote Longbridge backend / object store, outside this server's control; the link is time-limited and externally managed.
- **Destructive (False):** Producing a download URL is non-destructive: the underlying statement data is unchanged, and no account state is touched.

## `statement_list`

- **Read Only (True):** Calls `AssetContext::statements(options)` to list available daily/monthly statements (id, type, date, status). It only enumerates existing statements.
- **Open World (True):** Statement metadata is served by the remote Longbridge backend, where new statements appear over time independently of this server.
- **Destructive (False):** No statement is generated, deleted, or modified; the list is read-only.

## `static_info`

- **Read Only (True):** Fetches static security metadata (symbol, names, exchange, type, lot_size, listed_date, delisted) for the given symbols; pure lookup, no writes.
- **Open World (True):** Reads listing/reference data from the Longbridge backend over the network; the data is maintained externally by exchanges/Longbridge.
- **Destructive (False):** Only retrieves reference data; nothing is modified or removed.

## `stock_positions`

- **Read Only (True):** Calls `TradeContext::stock_positions(None)`, a getter returning current stock holdings. No parameters that mutate state; no write occurs.
- **Open World (True):** Data comes from the remote Longbridge account backend and changes as fills/corporate actions occur outside this server.
- **Destructive (False):** No update or deletion is performed; positions are only read.

## `submit_order`

- **Read Only (False):** Submits a live buy/sell order to the brokerage.
- **Open World (True):** Submits to the Longbridge trade gateway.
- **Destructive (True):** Placing an order has a real, irreversible market side effect (it can execute and move cash/positions); it is not a reversible or purely-additive bookkeeping change, so the destructive hint is appropriate for an action this consequential.

## `today_executions`

- **Read Only (True):** Calls `TradeContext::today_executions` and `today_orders` (joined via `tokio::try_join!`, `trade.rs:211`) to list today's fills and annotate each with its order side. Both are getters; no execution or order is created or modified.
- **Open World (True):** Execution data is fetched live from the Longbridge backend and grows as trades fill during the day.
- **Destructive (False):** The side annotation is computed in-memory on the returned data only; nothing upstream is written.

## `today_orders`

- **Read Only (True):** Calls `TradeContext::today_orders(opts)` with an optional symbol filter; it lists orders placed today and does not create, modify, or cancel any order.
- **Open World (True):** Reads live order state from the Longbridge backend, which changes as orders are submitted/filled elsewhere.
- **Destructive (False):** Pure listing; no order is altered or removed.

## `top_movers`

- **Read Only (True):** Returns stocks whose price move exceeds the 20-trading-day standard deviation, with correlated news; a pure read of computed events.
- **Open World (True):** Top-mover events are computed externally from live market data and served from the Longbridge backend over the network.
- **Destructive (False):** Reading the movers list changes no state.

## `topic`

- **Read Only (True):** `content::topic` lists community discussion topics for a symbol, returning `items[]{id, title, author, created_at, like_count, comment_count, content_summary}`. Read-only listing (creation is the separate `topic_create`, not in this group).
- **Open World (True):** Community topics live on the external Longbridge backend and are updated by other users in real time.
- **Destructive (False):** Listing topics modifies no community data.

## `topic_create`

- **Read Only (False):** Publishes a new community discussion topic (post or article).
- **Open World (True):** Writes to the Longbridge community backend.
- **Destructive (False):** Additive; it creates a new topic and does not modify or delete existing community content.

## `topic_create_reply`

- **Read Only (False):** Posts a reply under an existing topic (optionally nested under another reply).
- **Open World (True):** Writes to the Longbridge community backend.
- **Destructive (False):** Additive; it appends a reply and does not alter or remove the parent topic or other replies.

## `topic_detail`

- **Read Only (True):** `content::topic_detail` fetches one topic by `topic_id`, returning its full content and metadata. Pure read.
- **Open World (True):** Topic data is hosted on the external Longbridge backend and may change as the post is edited or gains likes/comments.
- **Destructive (False):** Reading a topic does not edit or delete it.

## `topic_replies`

- **Read Only (True):** `content::topic_replies` reads the paginated reply list under a topic. It only retrieves replies (posting a reply is `topic_create_reply`, excluded from this group).
- **Open World (True):** Replies are stored on the external Longbridge backend and grow as users respond.
- **Destructive (False):** Reading replies creates/removes nothing.

## `topic_search`

- **Read Only (True):** `search::topic_search` uses `http_get_tool` to search community topics by keyword and returns id/author/time/excerpt. Read-only search.
- **Open World (True):** Topic search results come from the external Longbridge backend and vary as content is indexed.
- **Destructive (False):** A search persists nothing.

## `trade_stats`

- **Read Only (True):** Returns the buy/sell/neutral volume distribution (price-volume profile) for a symbol; read-only.
- **Open World (True):** Trade statistics are computed from live market data and served from the Longbridge backend over the network.
- **Destructive (False):** Reads statistics only; nothing is written.

## `trades`

- **Read Only (True):** Returns recent trade ticks (price, volume, timestamp, trade_type, direction; up to 1000) for a symbol; read-only.
- **Open World (True):** Trade ticks are live exchange data served by the Longbridge backend over the network.
- **Destructive (False):** Reads historical/recent ticks only; no state is altered.

## `trading_days`

- **Read Only (True):** Returns trading and half-trading days for a market between two dates; a calendar lookup.
- **Open World (True):** The trading calendar is maintained externally and served from the Longbridge backend over the network.
- **Destructive (False):** Reading the trading calendar changes no state.

## `trading_session`

- **Read Only (True):** Returns the trading-session schedule (session windows and types) for all markets; a schedule lookup.
- **Open World (True):** Session schedules are defined by the exchanges and retrieved from the Longbridge backend over the network.
- **Destructive (False):** Reading the schedule changes no state.

## `update_watchlist_group`

- **Read Only (False):** Updates a watchlist group by id (rename, or modify securities).
- **Open World (True):** Mutates the Longbridge watchlist service.
- **Destructive (True):** The `replace` mode overwrites the group's existing securities, and a rename overwrites the prior name — both can destroy/overwrite existing data, so the destructive hint is warranted.

## `valuation`

- **Read Only (True):** Reads a valuation overview (PE/PB/PS/dividend_yield with current, industry_avg, 5yr_avg, percentile) plus peer comparison via `GET /v1/quote/valuation`. Read-only.
- **Open World (True):** Multiples are derived from external market prices and financials and move every trading day; outside server control.
- **Destructive (False):** Reading valuation multiples changes nothing.

## `valuation_comparison`

- **Read Only (True):** Reads cross-stock valuation comparison (market_value, price_close, pe, pb, ps, history) for a symbol against auto-selected or explicitly listed industry peers. Query only.
- **Open World (True):** Comparison data tracks external prices and fundamentals that update with the market; not server-controlled.
- **Destructive (False):** Comparing valuations reads only; nothing is written.

## `valuation_history`

- **Read Only (True):** Reads a valuation time series (PE/PB/PS/dividend_yield {timestamp, value}) for long-term percentile analysis. Query only.
- **Open World (True):** The series extends as new market data accumulates on the external backend; not server-controlled.
- **Destructive (False):** Reading the historical series writes nothing.

## `valuation_rank`

- **Read Only (True):** Reads the daily valuation rank (PE/PB/PS/dividend-yield industry percentile) over a date range. Read-only fetch.
- **Open World (True):** Ranks recompute daily from external market and peer data; outside server control.
- **Destructive (False):** Reading percentiles modifies nothing.

## `warrant_issuers`

- **Read Only (True):** Returns the HK warrant issuer directory (id and names); a static reference lookup.
- **Open World (True):** The issuer directory is maintained externally and fetched from the Longbridge backend over the network.
- **Destructive (False):** Read-only reference fetch; no modification.

## `warrant_list`

- **Read Only (True):** Returns a filtered list of warrants for an underlying symbol with their quote/term fields; read-only.
- **Open World (True):** Warrant listings and quotes are live data from the Longbridge backend over the network.
- **Destructive (False):** Only reads the warrant list; no state is altered.

## `warrant_quote`

- **Read Only (True):** Reads warrant quote fields (price, volume, IV, delta, leverage_ratio, effective_leverage) per symbol; no writes.
- **Open World (True):** Warrant quotes are fetched live from the Longbridge backend over the network and change with the market.
- **Destructive (False):** Retrieves warrant market data only; nothing is modified.

## `watchlist`

- **Read Only (True):** Returns the user's existing watchlist groups and their securities; it only reads the watchlist and never modifies it. (Mutation is handled by the separate create/update/delete_watchlist_group tools, which are not in this group.)
- **Open World (True):** The watchlist is stored in the user's Longbridge account and retrieved from the backend over the network.
- **Destructive (False):** Reading the watchlist neither adds, removes, nor reorders any group or security.

## `withdrawals`

- **Read Only (True):** Calls `http_get_tool(client, "/v1/account/withdrawals", ...)` with page/size/account_channel params. This is an HTTP GET that _lists withdrawal history_; it does **not** initiate a withdrawal. Confirmed by the implementation: it only issues a GET against the history endpoint with paging parameters.
- **Open World (True):** History is served by the remote Longbridge backend and grows as real withdrawals occur outside this server.
- **Destructive (False):** Listing past withdrawals changes nothing; no funds are moved.
