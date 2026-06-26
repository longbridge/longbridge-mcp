# Group C — Fundamentals & Research Data Tools: Annotation Accuracy Justification

This document supports the OpenAI MCP submission review. It justifies, per tool, why each
explicitly declared annotation value is accurate and does not misrepresent the tool's actual
behavior.

All tools in this group are **read-only fundamental/research data queries**. Each is implemented
as a single HTTP `GET` request against the Longbridge fundamental/research backend (e.g.
`GET /v1/quote/...`, `GET /v1/asset/cash/short-margin`) via the shared `http_get_tool` /
`http_get_tool_unix` helpers. None of them issue any write, mutation, or side-effecting request.

Shared annotation semantics for this group:

- **readOnlyHint = true** — the tool only reads fundamental/research data; it never modifies any
  state on the server or in the user's account.
- **destructiveHint = false** — the tool performs no write or destructive update of any kind.
- **idempotentHint = true** — calling with the same parameters repeatedly produces no additional
  side effects (each call is just another read).
- **openWorldHint = true** — the tool reaches an external Longbridge fundamental/research backend
  over the network; the returned data tracks the market and issuer disclosures and is not under
  this server's control.

---

### financial_report
- **Read Only = true** — Performs `GET /v1/quote/financial-reports` to fetch income statement,
  balance sheet, and cash flow data (kind IS/BS/CF/ALL; report_type af/saf/q1-q3/qf). Pure read.
- **Destructive = false** — Retrieving published financial statements alters nothing.
- **Idempotent = true** — Repeating the same symbol/kind/report_type returns the same statements
  with no cumulative effect.
- **Open World = true** — Statement data is sourced from issuer filings on the external Longbridge
  backend and updates as new periods are disclosed; the server does not control it.

### financial_statement
- **Read Only = true** — Reads income statement, balance sheet, or cash flow (kind IS/BS/CF/ALL;
  report af/saf/qf/q1-q3) for a security via a backend GET. No state is changed.
- **Destructive = false** — Reading statement line items writes nothing.
- **Idempotent = true** — Identical kind/report parameters yield the same statement data each call.
- **Open World = true** — Backed by externally-disclosed issuer financials that change with each
  reporting period, outside server control.

### financial_report_latest
- **Read Only = true** — Reads the latest report summary (period, revenue, net_income, eps, roe,
  gross_margin, report_date). Query only.
- **Destructive = false** — Fetching a summary modifies no data.
- **Idempotent = true** — The same symbol returns the same latest-period summary until a new report
  is disclosed; calling repeatedly has no side effect.
- **Open World = true** — "Latest" reflects the most recent external disclosure on the Longbridge
  backend and advances as issuers report; not server-controlled.

### financial_report_snapshot
- **Read Only = true** — Reads a report snapshot: text summary (report_desc), actual-vs-forecast
  figures (fo_revenue/fo_ebit/fo_eps), and financial ratios (fr_*: ROE, margins, assets, cash
  flow). Read-only fetch.
- **Destructive = false** — No write occurs when reading the snapshot.
- **Idempotent = true** — Same symbol/report parameter returns the same snapshot with no
  accumulation.
- **Open World = true** — Combines disclosed actuals and analyst forecasts from the external
  backend; both update with the market and disclosures.

### institution_rating
- **Read Only = true** — Reads analyst rating summary (buy/outperform/hold/underperform/sell
  counts, target_price, consensus_rating) plus the instratings list via backend GETs. No mutation.
- **Destructive = false** — Reading the rating consensus changes nothing.
- **Idempotent = true** — The same symbol returns the same consensus snapshot per call.
- **Open World = true** — Analyst ratings originate from external research firms and are updated on
  the Longbridge backend as firms publish; not server-controlled.

### institution_rating_detail
- **Read Only = true** — Reads per-institution historical ratings and target-price history
  (analyst, firm, rating, target_price, timestamp). Query only.
- **Destructive = false** — Retrieving historical ratings writes nothing.
- **Idempotent = true** — Identical symbol parameter returns the same history each call.
- **Open World = true** — Rating history is external research data that grows as firms issue new
  ratings; outside server control.

### institution_rating_history
- **Read Only = true** — Reads target-price change history (firm, analyst, old_target, new_target,
  date) and rating-change history (firm, old_rating, new_rating, date). Read-only.
- **Destructive = false** — No state is altered by reading the change log.
- **Idempotent = true** — Same symbol yields the same change history per call.
- **Open World = true** — Change events come from external analyst actions surfaced on the backend;
  they accrue with market activity, not server control.

### institution_rating_industry_rank
- **Read Only = true** — Reads peers ranked by analyst ratings within the same industry (symbol,
  name, buy_count, sell_count, consensus_rating, target_price). Paginated read.
- **Destructive = false** — Ranking peers reads only; nothing is written.
- **Idempotent = true** — The same query/page returns the same ranked list with no side effect.
- **Open World = true** — Rankings derive from external analyst ratings across the industry and
  shift as ratings update; not server-controlled.

### dividend
- **Read Only = true** — Reads dividend history (ex_date, pay_date, record_date, dividend_type,
  amount, currency, status) via `GET /v1/quote/dividends`. Pure read.
- **Destructive = false** — Reading the payout record changes nothing.
- **Idempotent = true** — The same symbol returns the same dividend history per call.
- **Open World = true** — Dividend records are external issuer disclosures that extend as new
  distributions are announced; outside server control.

### dividend_detail
- **Read Only = true** — Reads the detailed distribution scheme (period, cash_dividend,
  stock_dividend, record_date, ex_date, pay_date, currency). Query only.
- **Destructive = false** — Fetching distribution details writes nothing.
- **Idempotent = true** — Same symbol returns the same scheme details per call.
- **Open World = true** — Distribution details are sourced from external issuer disclosures and
  update as schemes are declared; not server-controlled.

### forecast_eps
- **Read Only = true** — Reads EPS forecast/estimate history (forecast_start_date,
  forecast_end_date, eps_estimate, eps_actual, surprise_pct, analyst_count). Read-only fetch.
- **Destructive = false** — Reading forecasts and actuals modifies nothing.
- **Idempotent = true** — The same symbol returns the same forecast series per call.
- **Open World = true** — Estimates come from external analysts and actuals from issuer reports;
  both update on the backend with market events, outside server control.

### consensus
- **Read Only = true** — Reads consensus estimates for upcoming periods (period,
  revenue_estimate, eps_estimate, net_income_estimate, analyst_count, last_updated). Query only.
- **Destructive = false** — Reading consensus estimates writes nothing.
- **Idempotent = true** — Same symbol returns the same consensus snapshot per call.
- **Open World = true** — Consensus figures aggregate external analyst estimates that revise over
  time on the Longbridge backend; not server-controlled.

### valuation
- **Read Only = true** — Reads a valuation overview (PE/PB/PS/dividend_yield with current,
  industry_avg, 5yr_avg, percentile) plus peer comparison via `GET /v1/quote/valuation`. Read-only.
- **Destructive = false** — Reading valuation multiples changes nothing.
- **Idempotent = true** — The same symbol returns the same valuation snapshot per call.
- **Open World = true** — Multiples are derived from external market prices and financials and move
  every trading day; outside server control.

### valuation_history
- **Read Only = true** — Reads a valuation time series (PE/PB/PS/dividend_yield {timestamp, value})
  for long-term percentile analysis. Query only.
- **Destructive = false** — Reading the historical series writes nothing.
- **Idempotent = true** — Same symbol returns the same series per call.
- **Open World = true** — The series extends as new market data accumulates on the external
  backend; not server-controlled.

### valuation_rank
- **Read Only = true** — Reads the daily valuation rank (PE/PB/PS/dividend-yield industry
  percentile) over a date range. Read-only fetch.
- **Destructive = false** — Reading percentiles modifies nothing.
- **Idempotent = true** — The same symbol/date-range returns the same ranks per call.
- **Open World = true** — Ranks recompute daily from external market and peer data; outside server
  control.

### valuation_comparison
- **Read Only = true** — Reads cross-stock valuation comparison (market_value, price_close, pe, pb,
  ps, history) for a symbol against auto-selected or explicitly listed industry peers. Query only.
- **Destructive = false** — Comparing valuations reads only; nothing is written.
- **Idempotent = true** — The same symbol/comparison_symbols/currency returns the same comparison
  per call.
- **Open World = true** — Comparison data tracks external prices and fundamentals that update with
  the market; not server-controlled.

### industry_valuation
- **Read Only = true** — Reads industry peer valuation (symbol, name, pe, pb, ps, dividend_yield,
  history) via a backend GET. Read-only.
- **Destructive = false** — Reading peer multiples writes nothing.
- **Idempotent = true** — Same symbol returns the same peer valuation set per call.
- **Open World = true** — Peer valuations follow external market data and change daily; outside
  server control.

### industry_valuation_dist
- **Read Only = true** — Reads the industry PE/PB/PS distribution (min, p25, median, p75, max,
  current_percentile). Query only.
- **Destructive = false** — Reading the distribution modifies nothing.
- **Idempotent = true** — The same symbol returns the same distribution snapshot per call.
- **Open World = true** — Distribution recomputes from external sector data as the market moves;
  not server-controlled.

### company
- **Read Only = true** — Reads the company profile (name, description, employees, CEO,
  founded_year, website, exchange, industry, market_cap, business summary). Read-only fetch.
- **Destructive = false** — Reading the profile writes nothing.
- **Idempotent = true** — The same symbol returns the same profile per call.
- **Open World = true** — Company data is maintained externally on the Longbridge backend and
  updates with disclosures (e.g. market_cap moves daily); outside server control.

### executive
- **Read Only = true** — Reads executive and board information (name, title, appointed_date, age,
  biography, compensation). Query only.
- **Destructive = false** — Reading executive records modifies nothing.
- **Idempotent = true** — Same symbol returns the same roster per call.
- **Open World = true** — Executive data reflects external issuer disclosures and changes with
  appointments/departures; not server-controlled.

### shareholder
- **Read Only = true** — Reads institutional shareholders (institution, shares, ratio, change,
  change_type, reported_at). Read-only fetch.
- **Destructive = false** — Reading holdings writes nothing.
- **Idempotent = true** — Same symbol returns the same shareholder list per call.
- **Open World = true** — Holdings come from external regulatory filings (e.g. 13F) and update each
  reporting period; outside server control.

### shareholder_top
- **Read Only = true** — Reads the Top 20 major shareholders across reporting periods (period,
  object_id, name, title, shares_held, percent_shares_held, shares_changed, filing_date). Read-only.
- **Destructive = false** — Reading the top-holder list modifies nothing.
- **Idempotent = true** — The same symbol returns the same top-holder data per call.
- **Open World = true** — Top-holder data is sourced from external filings and revises each
  reporting period; not server-controlled.

### shareholder_detail
- **Read Only = true** — Reads a single holder's holding and trade history by object_id (name,
  owner_source, tradings with accum_buy/accum_sell/net_buy and trading_details, holding/trading
  summaries). Query only.
- **Destructive = false** — Reading a holder's history writes nothing.
- **Idempotent = true** — The same object_id returns the same holder history per call.
- **Open World = true** — Holder activity comes from external filings (13F / Form 4) and grows with
  new disclosures; outside server control.

### fund_holder
- **Read Only = true** — Reads funds and ETFs holding a symbol (fund_name, fund_symbol, shares,
  ratio, change, reported_at). Read-only fetch.
- **Destructive = false** — Reading fund holdings modifies nothing.
- **Idempotent = true** — Same symbol returns the same fund-holder list per call.
- **Open World = true** — Fund-holding data derives from external filings and updates each
  reporting period; not server-controlled.

### corp_action
- **Read Only = true** — Reads corporate actions (splits, buybacks, name changes) with action_type,
  effective_date, ratio, description. Query only.
- **Destructive = false** — Reading the corporate-action log writes nothing.
- **Idempotent = true** — The same symbol returns the same action list per call.
- **Open World = true** — Corporate actions are external issuer disclosures that accrue over time;
  outside server control.

### invest_relation
- **Read Only = true** — Reads investor relations events/announcements (title, event_type,
  event_date, url, description). Read-only fetch.
- **Destructive = false** — Reading IR events modifies nothing.
- **Idempotent = true** — Same symbol returns the same event list per call.
- **Open World = true** — IR events are externally published and grow as the company announces;
  not server-controlled.

### operating
- **Read Only = true** — Reads company operating metrics (HK stocks only) such as passenger
  traffic, cargo volumes, or store counts (period, metric_name, value, unit). Query only.
- **Destructive = false** — Reading operating metrics writes nothing.
- **Idempotent = true** — The same symbol returns the same operating series per call.
- **Open World = true** — Operating data is sourced from external issuer disclosures and updates
  each period; outside server control.

### broker_holding
- **Read Only = true** — Reads top broker holdings for an HK stock (broker_name, holding_quantity,
  holding_change, holding_ratio) for a period (rct_1/5/20/60), sourced from HKEX CCASS disclosure.
  Read-only.
- **Destructive = false** — Reading CCASS broker holdings modifies nothing.
- **Idempotent = true** — Same symbol/period returns the same holdings per call.
- **Open World = true** — Data comes from external HKEX CCASS participant disclosure that updates
  daily; not server-controlled.

### broker_holding_detail
- **Read Only = true** — Reads the full broker holding list for an HK stock (broker_id,
  broker_name, holding_quantity, holding_ratio, holding_change, date) from HKEX CCASS. Query only.
- **Destructive = false** — Reading the detail list writes nothing.
- **Idempotent = true** — Same symbol returns the same broker list per call.
- **Open World = true** — Sourced from external HKEX CCASS disclosure updated daily; outside server
  control.

### broker_holding_daily
- **Read Only = true** — Reads a specific broker's daily holding history in a symbol (date,
  holding_quantity, holding_change, holding_ratio) from HKEX CCASS. Read-only fetch.
- **Destructive = false** — Reading the daily history modifies nothing.
- **Idempotent = true** — Same symbol/broker_id returns the same history per call.
- **Open World = true** — Daily CCASS records accumulate from external HKEX disclosure; not
  server-controlled.

### finance_calendar
- **Read Only = true** — Reads finance-calendar events (report/dividend/split/ipo/macrodata/closed)
  by category, market, and date range via `GET /v1/quote/finance_calendar`. Read-only.
- **Destructive = false** — Reading scheduled events writes nothing.
- **Idempotent = true** — The same category/market/date-range returns the same events per call.
- **Open World = true** — Calendar events (earnings dates, macro releases, holidays) are externally
  scheduled and revised as schedules change; outside server control.

### profit_analysis
- **Read Only = true** — Reads a portfolio profit-and-loss analysis summary over an optional date
  range. Query only; it computes/reads results without changing account state.
- **Destructive = false** — Reading P&L analysis writes nothing to the account.
- **Idempotent = true** — The same start/end range returns the same summary per call.
- **Open World = true** — The analysis is computed on the external Longbridge backend from account
  and market data that update over time; not server-controlled.

### profit_analysis_detail
- **Read Only = true** — Reads detailed per-symbol profit-and-loss analysis over an optional date
  range. Read-only fetch.
- **Destructive = false** — Reading per-symbol P&L modifies no account state.
- **Idempotent = true** — The same symbol/range returns the same detail per call.
- **Open World = true** — Computed on the external backend from account and market data that change
  over time; outside server control.

### business_segments
- **Read Only = true** — Reads the current-period business segment revenue breakdown (name,
  percent, total, currency). Query only.
- **Destructive = false** — Reading the segment breakdown writes nothing.
- **Idempotent = true** — The same symbol returns the same breakdown per call.
- **Open World = true** — Segment data is sourced from external issuer disclosures and updates each
  period; not server-controlled.

### business_segments_history
- **Read Only = true** — Reads historical segment revenue trends by period and category (date,
  total, currency, business[], regionals[]). Read-only fetch.
- **Destructive = false** — Reading the historical trend modifies nothing.
- **Idempotent = true** — The same symbol returns the same historical series per call.
- **Open World = true** — Historical segment data extends with each external disclosure; outside
  server control.

### institutional_views
- **Read Only = true** — Reads the monthly institutional rating distribution timeline (date, buy,
  outperform, hold, underperform, sell, total). Query only.
- **Destructive = false** — Reading the rating timeline writes nothing.
- **Idempotent = true** — The same symbol returns the same timeline per call.
- **Open World = true** — The timeline aggregates external analyst ratings that change monthly;
  not server-controlled.

### industry_rank
- **Read Only = true** — Reads an industry ranking list by market and indicator (leaders, trend,
  heat, market cap, revenue, profit, growth) returning counter_id, name, chg, lists[]. Read-only.
- **Destructive = false** — Reading the ranking modifies nothing.
- **Idempotent = true** — The same market/indicator/sort_type returns the same ranking per call.
- **Open World = true** — Rankings recompute from external market data and shift daily; outside
  server control.

### industry_peers
- **Read Only = true** — Reads the hierarchical sub-sector peer tree for an industry group
  (chain{name, counter_id, stock_num, chg, ytd_chg, next[]}, top{name, market}). Query only.
- **Destructive = false** — Reading the peer tree writes nothing.
- **Idempotent = true** — The same counter_id returns the same tree per call.
- **Open World = true** — Peer-group structure and its change figures track external market data;
  not server-controlled.

### short_margin
- **Read Only = true** — Reads short margin deposit details for the current account (margin_amount,
  margin_rate, interest_rate, symbol, quantity per position) via
  `GET /v1/asset/cash/short-margin`. Read-only.
- **Destructive = false** — Reading short-margin details modifies no account or position state.
- **Idempotent = true** — Repeated calls return the same margin details with no side effect.
- **Open World = true** — Margin figures are maintained on the external Longbridge backend and
  update with positions, rates, and market data; outside server control.

### short_trades
- **Read Only = true** — Reads daily short-sale volume history for HK or US stocks (timestamp,
  short_vol, rate, close; US-only nasdaq_vol/nyse_vol; HK-only balance/market_vol). Read-only.
- **Destructive = false** — Reading short-sale history writes nothing.
- **Idempotent = true** — The same symbol/page parameters return the same history per call.
- **Open World = true** — Data is sourced externally (US: FINRA/NASDAQ daily; HK: HKEX daily) and
  extends each trading day; not server-controlled.
