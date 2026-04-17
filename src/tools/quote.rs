use longbridge::quote::{
    QuoteContext, RequestCreateWatchlistGroup, RequestUpdateWatchlistGroup, SecuritiesUpdateMode,
};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::parse;
use crate::tools::{tool_json, tool_result};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolsParam {
    /// Security symbols, e.g. ["700.HK", "AAPL.US"]
    pub symbols: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolCountParam {
    pub symbol: String,
    /// Maximum number of results (max 1000)
    pub count: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CandlesticksParam {
    pub symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year
    pub period: String,
    /// Number of candlesticks (max 1000)
    pub count: usize,
    /// Whether to forward-adjust for splits/dividends
    pub forward_adjust: bool,
    /// Trade sessions: "intraday" or "all"
    pub trade_sessions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryCandlesticksByOffsetParam {
    pub symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year
    pub period: String,
    /// Whether to forward-adjust for splits/dividends
    pub forward_adjust: bool,
    /// Whether to query forward in time (true) or backward (false)
    pub forward: bool,
    /// Reference datetime (yyyy-mm-ddTHH:MM:SS), omit to start from latest
    pub time: Option<String>,
    /// Number of candlesticks (max 1000)
    pub count: usize,
    /// Trade sessions: "intraday" or "all"
    pub trade_sessions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryCandlesticksByDateParam {
    pub symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year
    pub period: String,
    /// Whether to forward-adjust for splits/dividends
    pub forward_adjust: bool,
    /// Start date (yyyy-mm-dd), optional
    pub start: Option<String>,
    /// End date (yyyy-mm-dd), optional
    pub end: Option<String>,
    /// Trade sessions: "intraday" or "all"
    pub trade_sessions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarketParam {
    /// Market code: HK, US, CN, SG
    pub market: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarketDateRangeParam {
    /// Market code: HK, US, CN, SG
    pub market: String,
    /// Start date (yyyy-mm-dd)
    pub start: String,
    /// End date (yyyy-mm-dd)
    pub end: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolDateParam {
    pub symbol: String,
    /// Date (yyyy-mm-dd)
    pub date: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WarrantListParam {
    /// Underlying symbol, e.g. "700.HK"
    pub symbol: String,
    /// Sort field: LastDone, ChangeRate, ChangeValue, Volume, Turnover, ExpiryDate, StrikePrice, UpperStrikePrice, LowerStrikePrice, OutstandingQuantity, OutstandingRatio, Premium, ItmOtm, ImpliedVolatility, Delta
    pub sort_by: String,
    /// Sort order: Ascending or Descending
    pub sort_order: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalcIndexesParam {
    /// Security symbols, e.g. ["700.HK", "AAPL.US"]
    pub symbols: Vec<String>,
    /// Calc indexes: LastDone, ChangeValue, ChangeRate, Volume, Turnover, YtdChangeRate, TurnoverRate, TotalMarketValue, CapitalFlow, Amplitude, VolumeRatio, PeTtmRatio, PbRatio, DividendRatioTtm, FiveDayChangeRate, TenDayChangeRate, HalfYearChangeRate, FiveMinutesChangeRate, ExpiryDate, StrikePrice, UpperStrikePrice, LowerStrikePrice, OutstandingQty, OutstandingRatio, Premium, ItmOtm, ImpliedVolatility, WarrantDelta, CallPrice, ToCallPrice, EffectiveLeverage, LeverageRatio, ConversionRatio, BalancePoint, OpenInterest, Delta, Gamma, Theta, Vega, Rho
    pub indexes: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateWatchlistGroupParam {
    /// Group name
    pub name: String,
    /// Securities to add, e.g. ["700.HK", "AAPL.US"]
    pub securities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteWatchlistGroupParam {
    /// Watchlist group id
    pub id: i64,
    /// Whether to also remove the securities from other groups
    pub purge: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateWatchlistGroupParam {
    /// Watchlist group id
    pub id: i64,
    /// New group name (optional)
    pub name: Option<String>,
    /// Securities list (optional)
    pub securities: Option<Vec<String>>,
    /// Update mode for securities: "add", "remove", or "replace" (default: "replace")
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SecurityListParam {
    /// Market code: HK, US, CN, SG
    pub market: String,
    /// Category filter. Currently only "Overnight" is supported; passing any other value or omitting this field will result in a param_error.
    pub category: Option<String>,
}

pub async fn static_info(
    mctx: &crate::tools::McpContext,
    p: SymbolsParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .static_info(p.symbols)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn quote(
    mctx: &crate::tools::McpContext,
    p: SymbolsParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx.quote(p.symbols).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn option_quote(
    mctx: &crate::tools::McpContext,
    p: SymbolsParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .option_quote(p.symbols)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn warrant_quote(
    mctx: &crate::tools::McpContext,
    p: SymbolsParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .warrant_quote(p.symbols)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn depth(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx.depth(p.symbol).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn brokers(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx.brokers(p.symbol).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn participants(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx.participants().await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn trades(
    mctx: &crate::tools::McpContext,
    p: SymbolCountParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .trades(p.symbol, p.count)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn intraday(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .intraday(p.symbol, longbridge::quote::TradeSessions::Intraday)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn candlesticks(
    mctx: &crate::tools::McpContext,
    p: CandlesticksParam,
) -> Result<CallToolResult, McpError> {
    let period = parse::parse_period(&p.period)?;
    let sessions = parse::parse_trade_sessions(&p.trade_sessions)?;
    let adjust = if p.forward_adjust {
        longbridge::quote::AdjustType::ForwardAdjust
    } else {
        longbridge::quote::AdjustType::NoAdjust
    };
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .candlesticks(p.symbol, period, p.count, adjust, sessions)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn history_candlesticks_by_offset(
    mctx: &crate::tools::McpContext,
    p: HistoryCandlesticksByOffsetParam,
) -> Result<CallToolResult, McpError> {
    let period = parse::parse_period(&p.period)?;
    let adjust = parse::parse_adjust_type(p.forward_adjust);
    let sessions = parse::parse_trade_sessions(&p.trade_sessions)?;
    let time = match p.time {
        Some(ref s) => Some(parse::parse_primitive_datetime(s)?),
        None => None,
    };
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .history_candlesticks_by_offset(
            p.symbol, period, adjust, p.forward, time, p.count, sessions,
        )
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn history_candlesticks_by_date(
    mctx: &crate::tools::McpContext,
    p: HistoryCandlesticksByDateParam,
) -> Result<CallToolResult, McpError> {
    let period = parse::parse_period(&p.period)?;
    let adjust = parse::parse_adjust_type(p.forward_adjust);
    let sessions = parse::parse_trade_sessions(&p.trade_sessions)?;
    let start = match p.start {
        Some(ref s) => Some(parse::parse_date(s)?),
        None => None,
    };
    let end = match p.end {
        Some(ref s) => Some(parse::parse_date(s)?),
        None => None,
    };
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .history_candlesticks_by_date(p.symbol, period, adjust, start, end, sessions)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn trading_days(
    mctx: &crate::tools::McpContext,
    p: MarketDateRangeParam,
) -> Result<CallToolResult, McpError> {
    let market = parse::parse_market(&p.market)?;
    let start = parse::parse_date(&p.start)?;
    let end = parse::parse_date(&p.end)?;
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .trading_days(market, start, end)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn option_chain_expiry_date_list(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let dates = ctx
        .option_chain_expiry_date_list(p.symbol)
        .await
        .map_err(Error::longbridge)?;
    let strs: Vec<String> = dates
        .into_iter()
        .map(|d| {
            d.format(time::macros::format_description!("[year]-[month]-[day]"))
                .expect("failed to format date")
        })
        .collect();
    tool_json(&strs)
}

pub async fn option_chain_info_by_date(
    mctx: &crate::tools::McpContext,
    p: SymbolDateParam,
) -> Result<CallToolResult, McpError> {
    let date = parse::parse_date(&p.date)?;
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .option_chain_info_by_date(p.symbol, date)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn capital_flow(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .capital_flow(p.symbol)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn capital_distribution(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .capital_distribution(p.symbol)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn trading_session(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx.trading_session().await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn market_temperature(
    mctx: &crate::tools::McpContext,
    p: MarketParam,
) -> Result<CallToolResult, McpError> {
    let market = parse::parse_market(&p.market)?;
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .market_temperature(market)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn history_market_temperature(
    mctx: &crate::tools::McpContext,
    p: MarketDateRangeParam,
) -> Result<CallToolResult, McpError> {
    let market = parse::parse_market(&p.market)?;
    let start = parse::parse_date(&p.start)?;
    let end = parse::parse_date(&p.end)?;
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .history_market_temperature(market, start, end)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn watchlist(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx.watchlist().await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn filings(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx.filings(p.symbol).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn warrant_issuers(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx.warrant_issuers().await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn warrant_list(
    mctx: &crate::tools::McpContext,
    p: WarrantListParam,
) -> Result<CallToolResult, McpError> {
    let sort_by = parse::parse_warrant_sort_by(&p.sort_by)?;
    let sort_order = parse::parse_sort_order_type(&p.sort_order)?;
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .warrant_list(p.symbol, sort_by, sort_order, None, None, None, None, None)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn calc_indexes(
    mctx: &crate::tools::McpContext,
    p: CalcIndexesParam,
) -> Result<CallToolResult, McpError> {
    let indexes: Vec<longbridge::quote::CalcIndex> = p
        .indexes
        .iter()
        .map(|s| parse::parse_calc_index(s))
        .collect::<Result<_, _>>()?;
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .calc_indexes(p.symbols, indexes)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn create_watchlist_group(
    mctx: &crate::tools::McpContext,
    p: CreateWatchlistGroupParam,
) -> Result<CallToolResult, McpError> {
    let mut req = RequestCreateWatchlistGroup::new(p.name);
    if let Some(securities) = p.securities {
        req = req.securities(securities);
    }
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let id = ctx
        .create_watchlist_group(req)
        .await
        .map_err(Error::longbridge)?;
    Ok(tool_result(id.to_string()))
}

pub async fn delete_watchlist_group(
    mctx: &crate::tools::McpContext,
    p: DeleteWatchlistGroupParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    ctx.delete_watchlist_group(p.id, p.purge)
        .await
        .map_err(Error::longbridge)?;
    Ok(tool_result("watchlist group deleted".to_string()))
}

pub async fn update_watchlist_group(
    mctx: &crate::tools::McpContext,
    p: UpdateWatchlistGroupParam,
) -> Result<CallToolResult, McpError> {
    let mut req = RequestUpdateWatchlistGroup::new(p.id);
    if let Some(name) = p.name {
        req = req.name(name);
    }
    if let Some(securities) = p.securities {
        req = req.securities(securities);
        let mode = match p.mode.as_deref() {
            Some("add") => SecuritiesUpdateMode::Add,
            Some("remove") => SecuritiesUpdateMode::Remove,
            _ => SecuritiesUpdateMode::Replace,
        };
        req = req.mode(mode);
    }
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    ctx.update_watchlist_group(req)
        .await
        .map_err(Error::longbridge)?;
    Ok(tool_result("watchlist group updated".to_string()))
}

pub async fn security_list(
    mctx: &crate::tools::McpContext,
    p: SecurityListParam,
) -> Result<CallToolResult, McpError> {
    let market = parse::parse_market(&p.market)?;
    let category = match p.category {
        Some(ref s) => Some(parse::parse_security_list_category(s)?),
        None => None,
    };
    let (ctx, _) = QuoteContext::new(mctx.create_config());
    let result = ctx
        .security_list(market, category)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}
