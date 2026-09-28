//! Fund (mutual fund) channel — read-only tools.
//!
//! These wrap `longbridge::fund::FundContext`. Every fund is identified by its
//! `counter_id` (e.g. `UT/FD/HK0000384492`), which callers obtain from
//! [`fund_hot`] / [`fund_list`]. The SDK handles the `counter_id`-query and
//! `counter_ids`-batch request shapes internally, so the tool layer only ever
//! passes a single `counter_id` (string) or an `order_id` (i64).

use longbridge::fund::{
    FundContext, FundNavRangeOptions, FundPageOptions, GetFundAnalysisOptions,
    GetFundHoldingsOptions, GetFundOrdersOptions, GetFundPositionDividendsOptions,
    GetFundPositionOptions, GetFundPositionProfitsOptions, GetFundPositionsOptions,
    GetFundStockHoldingsOptions, GetFundTransactionsOptions, GetFundsOptions,
    SubmitFundOrderOptions, ValidateFundOrderOptions,
};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::dry_run;
use crate::tools::{McpContext, tool_json};

/// Build a [`FundContext`] for this request.
fn context(mctx: &McpContext) -> FundContext {
    FundContext::new(mctx.create_config())
}

// ── param structs ───────────────────────────────────────────────────────────

/// A fund `counter_id` on its own.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CounterIdParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492`. Obtain it from `fund_hot` or
    /// `fund_list`.
    pub counter_id: String,
}

/// A fund `counter_id` plus an optional analysis period.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CounterIdPeriodParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Analysis period selector (server-defined, e.g. trailing 1M/3M/1Y ranges).
    /// Omit for the default period.
    pub period: Option<i32>,
}

/// A fund `counter_id` plus page/size paging.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CounterIdPageParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// 1-based page number. Omit for the first page.
    pub page: Option<i32>,
    /// Page size (rows per page). Omit for the server default.
    pub size: Option<i32>,
}

/// A fund `counter_id` plus a relative net-value window.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CounterIdNavRangeParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Number of months before now to include. Mutually complementary with
    /// `year_before`; omit both for the server default window.
    pub month_before: Option<i32>,
    /// Number of years before now to include.
    pub year_before: Option<i32>,
}

/// Fund list filters.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundListParam {
    /// Server-defined filter object (as returned/described by `fund_filters`).
    /// Omit for the unfiltered list.
    pub filter: Option<serde_json::Value>,
    /// Quick-filter ids (from `fund_filters`).
    pub quick_ids: Option<Vec<i64>>,
    /// Earning-rate time intervals to include in each row (e.g. `["1m","1y"]`).
    pub time_interval: Option<Vec<String>>,
}

/// Fund holdings (top-10) options.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundHoldingsParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Holdings scene selector (server-defined, e.g. stock vs. bond breakdown).
    /// Omit for the default.
    pub scene: Option<i32>,
}

/// Stock-holdings (reverse lookup) options.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundStockHoldingsParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Maximum number of held stocks to return. Omit for the server default.
    pub limit: Option<i32>,
}

/// Single held-fund position detail.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundPositionParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Range start (YYYY-MM-DD). Omit for the server default window.
    pub start: Option<String>,
    /// Range end (YYYY-MM-DD).
    pub end: Option<String>,
}

/// Held-fund cumulative-profit series.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundPositionProfitsParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Range start (YYYY-MM-DD). Omit for the server default window.
    pub start: Option<String>,
    /// Range end (YYYY-MM-DD).
    pub end: Option<String>,
    /// 1-based page number.
    pub page: Option<i32>,
    /// Page size (rows per page).
    pub size: Option<i32>,
}

/// Held-fund dividend records.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundPositionDividendsParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Currency filter (e.g. `HKD`, `USD`). Omit for all currencies.
    pub currency: Option<String>,
    /// Range start as a Unix timestamp in seconds. Omit for the default window.
    pub start: Option<i64>,
    /// Range end as a Unix timestamp in seconds.
    pub end: Option<i64>,
    /// 1-based page number.
    pub page: Option<i32>,
    /// Page size (rows per page).
    pub size: Option<i32>,
}

/// Fund orders (trade/execution records) filters.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundOrdersParam {
    /// Filter by fund symbols. Omit for all funds.
    pub symbols: Option<Vec<String>>,
    /// Filter by actions, comma-separated (e.g. `buy,sell`). Omit for all.
    pub actions: Option<String>,
    /// Filter by order states, comma-separated. Omit for all states.
    pub states: Option<String>,
    /// Currency filter (e.g. `HKD`, `USD`). Omit for all currencies.
    pub currency: Option<String>,
    /// Range start as a Unix timestamp in seconds. Omit for the default window.
    pub start: Option<i64>,
    /// Range end as a Unix timestamp in seconds.
    pub end: Option<i64>,
    /// 1-based page number.
    pub page: Option<i32>,
    /// Page size (rows per page).
    pub size: Option<i32>,
}

/// Fund order detail by order id.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundOrderParam {
    /// Fund order id (from `fund_orders`).
    pub order_id: i64,
}

/// Fund transactions (cash-flow records) filters.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundTransactionsParam {
    /// Business-type filter (server-defined). Omit for all.
    pub business_type: Option<String>,
    /// Category filter (server-defined). Omit for all.
    pub category: Option<String>,
    /// Currencies filter, comma-separated (e.g. `HKD,USD`). Omit for all.
    pub currencies: Option<String>,
    /// Range start as a Unix timestamp in seconds. Omit for the default window.
    pub start: Option<i64>,
    /// Range end as a Unix timestamp in seconds.
    pub end: Option<i64>,
    /// 1-based page number.
    pub page: Option<i32>,
    /// Page size (rows per page).
    pub size: Option<i32>,
}

/// Fund order pre-trade validation (places NO order).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundValidateOrderParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Order action: `buy` or `sell`.
    pub action: String,
    /// Order currency, e.g. `HKD`, `USD`.
    pub currency: String,
    /// Subscription amount (cash to invest). Use for a buy by amount; mutually
    /// exclusive with `units`.
    pub amount: Option<String>,
    /// Redemption units (shares to sell). Use for a sell / buy by units;
    /// mutually exclusive with `amount`.
    pub units: Option<String>,
    /// Dividend handling option (server-defined, e.g. reinvest vs. cash payout).
    pub dividend_option: Option<i32>,
    /// Funding source selector (server-defined). Omit for the default.
    pub fund_source: Option<i32>,
    /// Account channel (server-defined). Omit for the default.
    pub account_channel: Option<String>,
}

/// Fund order submission (places a real buy/sell order).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundSubmitOrderParam {
    /// Fund counter_id, e.g. `UT/FD/HK0000384492` (from `fund_hot` / `fund_list`).
    pub counter_id: String,
    /// Order action: `buy` or `sell`.
    pub action: String,
    /// Order currency, e.g. `HKD`, `USD`.
    pub currency: String,
    /// Subscription amount (cash to invest). Use for a buy by amount; mutually
    /// exclusive with `units`.
    pub amount: Option<String>,
    /// Redemption units (shares to sell). Use for a sell / buy by units;
    /// mutually exclusive with `amount`.
    pub units: Option<String>,
    /// Dividend handling option (server-defined, e.g. reinvest vs. cash payout).
    pub dividend_option: Option<i32>,
    /// Fee amount to apply. Omit for the server default.
    pub fee: Option<String>,
    /// Sell the entire held position; overrides `units` for a full redemption.
    pub is_sell_all: Option<bool>,
    /// Order remark / note.
    pub remark: Option<String>,
    /// Trade method selector (server-defined). Omit for the default.
    pub trade_method: Option<i32>,
    /// The `confirmation_code` from this order's dry run. WITHOUT IT NOTHING IS
    /// SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the fund channel.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed that exact order. The code is single use,
    /// expires in 10 minutes, and applies only to this exact order — change any
    /// field and it stops working. Never quote it back on your own initiative,
    /// and never in the same turn the user first asks.
    pub execute: Option<String>,
}

/// Fund order cancellation.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FundCancelOrderParam {
    /// Fund order id to cancel (from `fund_orders`).
    pub order_id: i64,
    /// The `confirmation_code` from this order's dry run. WITHOUT IT NOTHING IS
    /// SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the fund channel.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed that exact order. The code is single use,
    /// expires in 10 minutes, and applies only to this exact order — change any
    /// field and it stops working. Never quote it back on your own initiative,
    /// and never in the same turn the user first asks.
    pub execute: Option<String>,
}

// ── fund catalog / market data ───────────────────────────────────────────────

pub async fn fund_hot(mctx: &McpContext) -> Result<CallToolResult, McpError> {
    let data = context(mctx).hot_funds().await.map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_list(mctx: &McpContext, p: FundListParam) -> Result<CallToolResult, McpError> {
    let mut opts = GetFundsOptions::new();
    if let Some(filter) = p.filter {
        opts = opts.filter(filter);
    }
    if let Some(quick_ids) = p.quick_ids {
        opts = opts.quick_ids(quick_ids);
    }
    if let Some(time_interval) = p.time_interval {
        opts = opts.time_interval(time_interval);
    }
    let data = context(mctx).funds(opts).await.map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_filters(mctx: &McpContext) -> Result<CallToolResult, McpError> {
    let data = context(mctx).filters().await.map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_detail(mctx: &McpContext, p: CounterIdParam) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .detail(p.counter_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_analysis(
    mctx: &McpContext,
    p: CounterIdPeriodParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .analysis(p.counter_id, period_opts(p.period))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_analysis_detail(
    mctx: &McpContext,
    p: CounterIdPeriodParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .analysis_detail(p.counter_id, period_opts(p.period))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_trend(
    mctx: &McpContext,
    p: CounterIdPeriodParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .trend(p.counter_id, period_opts(p.period))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_annual_returns(
    mctx: &McpContext,
    p: CounterIdPageParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .annual_returns(p.counter_id, page_opts(p.page, p.size))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_quarterly_returns(
    mctx: &McpContext,
    p: CounterIdPageParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .quarterly_returns(p.counter_id, page_opts(p.page, p.size))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_performance(
    mctx: &McpContext,
    p: CounterIdParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .performance(p.counter_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_performance_comparison(
    mctx: &McpContext,
    p: CounterIdPeriodParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .performance_comparison(p.counter_id, period_opts(p.period))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_nav(mctx: &McpContext, p: CounterIdParam) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .nav(p.counter_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_nav_history(
    mctx: &McpContext,
    p: CounterIdPageParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .nav_history(p.counter_id, page_opts(p.page, p.size))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_nav_range(
    mctx: &McpContext,
    p: CounterIdNavRangeParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .nav_range(p.counter_id, nav_range_opts(p.month_before, p.year_before))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_holdings(
    mctx: &McpContext,
    p: FundHoldingsParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetFundHoldingsOptions::new();
    if let Some(scene) = p.scene {
        opts = opts.scene(scene);
    }
    let data = context(mctx)
        .holdings(p.counter_id, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_stock_holdings(
    mctx: &McpContext,
    p: FundStockHoldingsParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetFundStockHoldingsOptions::new();
    if let Some(limit) = p.limit {
        opts = opts.limit(limit);
    }
    let data = context(mctx)
        .stock_holdings(p.counter_id, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

// ── user fund positions ──────────────────────────────────────────────────────

pub async fn fund_position_overview(mctx: &McpContext) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .positions(None::<GetFundPositionsOptions>)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_position(
    mctx: &McpContext,
    p: FundPositionParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetFundPositionOptions::new();
    if let Some(start) = p.start {
        opts = opts.start(start);
    }
    if let Some(end) = p.end {
        opts = opts.end(end);
    }
    let data = context(mctx)
        .position(p.counter_id, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_position_performance(
    mctx: &McpContext,
    p: CounterIdParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .position_performance(p.counter_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_position_profits(
    mctx: &McpContext,
    p: FundPositionProfitsParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetFundPositionProfitsOptions::new();
    if let Some(start) = p.start {
        opts = opts.start(start);
    }
    if let Some(end) = p.end {
        opts = opts.end(end);
    }
    if let Some(page) = p.page {
        opts = opts.page(page);
    }
    if let Some(size) = p.size {
        opts = opts.size(size);
    }
    let data = context(mctx)
        .position_profits(p.counter_id, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_position_nav(
    mctx: &McpContext,
    p: CounterIdNavRangeParam,
) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .position_nav(p.counter_id, nav_range_opts(p.month_before, p.year_before))
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_position_dividends(
    mctx: &McpContext,
    p: FundPositionDividendsParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetFundPositionDividendsOptions::new();
    if let Some(currency) = p.currency {
        opts = opts.currency(currency);
    }
    if let Some(start) = p.start {
        opts = opts.start(start);
    }
    if let Some(end) = p.end {
        opts = opts.end(end);
    }
    if let Some(page) = p.page {
        opts = opts.page(page);
    }
    if let Some(size) = p.size {
        opts = opts.size(size);
    }
    let data = context(mctx)
        .position_dividends(p.counter_id, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

// ── fund orders & transactions ───────────────────────────────────────────────

pub async fn fund_orders(
    mctx: &McpContext,
    p: FundOrdersParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetFundOrdersOptions::new();
    if let Some(symbols) = p.symbols {
        opts = opts.symbols(symbols);
    }
    if let Some(actions) = p.actions {
        opts = opts.actions(actions);
    }
    if let Some(states) = p.states {
        opts = opts.states(states);
    }
    if let Some(currency) = p.currency {
        opts = opts.currency(currency);
    }
    if let Some(start) = p.start {
        opts = opts.start(start);
    }
    if let Some(end) = p.end {
        opts = opts.end(end);
    }
    if let Some(page) = p.page {
        opts = opts.page(page);
    }
    if let Some(size) = p.size {
        opts = opts.size(size);
    }
    let data = context(mctx)
        .orders(opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_order(mctx: &McpContext, p: FundOrderParam) -> Result<CallToolResult, McpError> {
    let data = context(mctx)
        .order(p.order_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_transactions(
    mctx: &McpContext,
    p: FundTransactionsParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetFundTransactionsOptions::new();
    if let Some(business_type) = p.business_type {
        opts = opts.business_type(business_type);
    }
    if let Some(category) = p.category {
        opts = opts.category(category);
    }
    if let Some(currencies) = p.currencies {
        opts = opts.currencies(currencies);
    }
    if let Some(start) = p.start {
        opts = opts.start(start);
    }
    if let Some(end) = p.end {
        opts = opts.end(end);
    }
    if let Some(page) = p.page {
        opts = opts.page(page);
    }
    if let Some(size) = p.size {
        opts = opts.size(size);
    }
    let data = context(mctx)
        .transactions(opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

// ── fund order actions ─────────────────────────────────────────────────────────

pub async fn fund_validate_order(
    mctx: &McpContext,
    p: FundValidateOrderParam,
) -> Result<CallToolResult, McpError> {
    // The SDK ctor arg is named `symbol` but carries the fund counter_id.
    let mut opts = ValidateFundOrderOptions::new(p.counter_id, p.action, p.currency);
    if let Some(amount) = p.amount {
        opts = opts.amount(amount);
    }
    if let Some(units) = p.units {
        opts = opts.units(units);
    }
    if let Some(dividend_option) = p.dividend_option {
        opts = opts.dividend_option(dividend_option);
    }
    if let Some(fund_source) = p.fund_source {
        opts = opts.fund_source(fund_source);
    }
    if let Some(account_channel) = p.account_channel {
        opts = opts.account_channel(account_channel);
    }
    let data = context(mctx)
        .validate_order(opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&data)
}

pub async fn fund_submit_order(
    mctx: &McpContext,
    p: FundSubmitOrderParam,
) -> Result<CallToolResult, McpError> {
    // The confirmation code covers what a wrong order would get wrong: the
    // action, the fund, the size, the currency, and whether the size is a cash
    // amount or a unit count (dropping that distinction would let a "sell 10
    // units" code place a "sell 10 dollars" order).
    let size = p.amount.as_deref().or(p.units.as_deref()).unwrap_or("");
    let by = if p.units.is_some() && p.amount.is_none() {
        "units"
    } else {
        "amount"
    };
    let mut scope = dry_run::Scope::order(&p.action, &p.counter_id, size, "")
        .and("currency", &p.currency)
        .and("by", by);
    if p.is_sell_all == Some(true) {
        scope = scope.and("sell_all", "true");
    }

    // Two-step by design: without a confirmation code this places nothing.
    let Some(code) = p.execute.clone() else {
        return dry_run::result(
            &scope,
            serde_json::json!({
                "action": "fund_submit_order",
                "counter_id": p.counter_id,
                "fund_action": p.action,
                "currency": p.currency,
                "amount": p.amount,
                "units": p.units,
                "dividend_option": p.dividend_option,
                "fee": p.fee,
                "is_sell_all": p.is_sell_all,
                "remark": p.remark,
                "trade_method": p.trade_method,
            }),
        );
    };
    scope.verify(&code)?;

    let mut opts = SubmitFundOrderOptions::new(p.counter_id, p.action, p.currency);
    if let Some(amount) = p.amount {
        opts = opts.amount(amount);
    }
    if let Some(units) = p.units {
        opts = opts.units(units);
    }
    if let Some(dividend_option) = p.dividend_option {
        opts = opts.dividend_option(dividend_option);
    }
    if let Some(fee) = p.fee {
        opts = opts.fee(fee);
    }
    if let Some(is_sell_all) = p.is_sell_all {
        opts = opts.is_sell_all(is_sell_all);
    }
    if let Some(remark) = p.remark {
        opts = opts.remark(remark);
    }
    if let Some(trade_method) = p.trade_method {
        opts = opts.trade_method(trade_method);
    }
    let result = context(mctx)
        .submit_order(opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&serde_json::json!({
        "dry_run": false,
        "result": result,
    }))
}

pub async fn fund_cancel_order(
    mctx: &McpContext,
    p: FundCancelOrderParam,
) -> Result<CallToolResult, McpError> {
    let ctx = context(mctx);
    let scope = dry_run::Scope::on_order("cancel fund", &p.order_id.to_string());

    // Two-step by design: without a confirmation code this cancels nothing.
    let Some(code) = p.execute.clone() else {
        // Best-effort snapshot of the targeted order so the user can confirm it
        // is the right one; a lookup failure must not break the dry run.
        let existing = match ctx.order(p.order_id).await {
            Ok(order) => serde_json::to_value(&order).unwrap_or(serde_json::Value::Null),
            Err(_) => serde_json::Value::Null,
        };
        return dry_run::result(
            &scope,
            serde_json::json!({
                "action": "fund_cancel_order",
                "order_id": p.order_id,
                "order": existing,
            }),
        );
    };
    scope.verify(&code)?;

    ctx.cancel_order(p.order_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&serde_json::json!({
        "dry_run": false,
        "order_id": p.order_id,
        "status": "cancelled",
    }))
}

// ── option builders ──────────────────────────────────────────────────────────

fn period_opts(period: Option<i32>) -> GetFundAnalysisOptions {
    let mut opts = GetFundAnalysisOptions::new();
    if let Some(period) = period {
        opts = opts.period(period);
    }
    opts
}

fn page_opts(page: Option<i32>, size: Option<i32>) -> FundPageOptions {
    let mut opts = FundPageOptions::new();
    if let Some(page) = page {
        opts = opts.page(page);
    }
    if let Some(size) = size {
        opts = opts.size(size);
    }
    opts
}

fn nav_range_opts(month_before: Option<i32>, year_before: Option<i32>) -> FundNavRangeOptions {
    let mut opts = FundNavRangeOptions::new();
    if let Some(month_before) = month_before {
        opts = opts.month_before(month_before);
    }
    if let Some(year_before) = year_before {
        opts = opts.year_before(year_before);
    }
    opts
}
