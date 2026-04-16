use std::sync::Arc;

use rmcp::ErrorData as McpError;
use rmcp::RoleServer;
use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content};
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use rmcp::service::RequestContext;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;

use crate::error::Error;
use crate::registry::UserRegistry;
use crate::serialize::to_tool_json;

async fn measured_tool_call<F, Fut>(name: &str, f: F) -> Result<CallToolResult, McpError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<CallToolResult, McpError>>,
{
    let start = std::time::Instant::now();
    let result = f().await;
    let duration = start.elapsed().as_secs_f64();
    crate::metrics::record_tool_call(name, duration, result.is_err());
    result
}

mod alert;
mod calendar;
mod content;
mod fundamental;
pub mod http_client;
mod market;
mod parse;
mod portfolio;
mod quote;
mod statement;
mod trade;

/// User identity injected via HTTP extension middleware.
#[derive(Clone, Debug)]
pub struct UserIdentity {
    pub user_id: String,
}

/// Longbridge MCP tool server.
///
/// User identity is not stored in the struct; it is extracted from
/// `RequestContext` extensions on each tool call (injected by auth middleware).
#[derive(Debug, Clone)]
pub struct Longbridge {
    pub registry: Arc<UserRegistry>,
}

fn tool_result(json: String) -> CallToolResult {
    CallToolResult::success(vec![Content::text(json)])
}

fn tool_json<T>(value: &T) -> Result<CallToolResult, McpError>
where
    T: serde::Serialize,
{
    let json = to_tool_json(value).map_err(Error::Serialize)?;
    Ok(tool_result(json))
}

fn extract_user_id(ctx: &RequestContext<RoleServer>) -> Result<String, McpError> {
    let parts = ctx
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::internal_error("missing request parts", None))?;
    let identity = parts
        .extensions
        .get::<UserIdentity>()
        .ok_or_else(|| McpError::internal_error("not authenticated", None))?;
    Ok(identity.user_id.clone())
}

fn extract_language(ctx: &RequestContext<RoleServer>) -> Option<String> {
    let parts = ctx.extensions.get::<axum::http::request::Parts>()?;
    parts
        .headers
        .get("accept-language")?
        .to_str()
        .ok()
        .map(|s| s.to_string())
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SymbolsParam {
    /// Security symbols, e.g. ["700.HK", "AAPL.US"]
    symbols: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SymbolParam {
    /// Security symbol, e.g. "700.HK"
    symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SymbolCountParam {
    symbol: String,
    /// Maximum number of results (max 1000)
    count: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CandlesticksParam {
    symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year
    period: String,
    /// Number of candlesticks (max 1000)
    count: usize,
    /// Whether to forward-adjust for splits/dividends
    forward_adjust: bool,
    /// Trade sessions: "intraday" or "all"
    trade_sessions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HistoryCandlesticksByOffsetParam {
    symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year
    period: String,
    /// Whether to forward-adjust for splits/dividends
    forward_adjust: bool,
    /// Whether to query forward in time (true) or backward (false)
    forward: bool,
    /// Reference datetime (yyyy-mm-ddTHH:MM:SS), omit to start from latest
    time: Option<String>,
    /// Number of candlesticks (max 1000)
    count: usize,
    /// Trade sessions: "intraday" or "all"
    trade_sessions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HistoryCandlesticksByDateParam {
    symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year
    period: String,
    /// Whether to forward-adjust for splits/dividends
    forward_adjust: bool,
    /// Start date (yyyy-mm-dd), optional
    start: Option<String>,
    /// End date (yyyy-mm-dd), optional
    end: Option<String>,
    /// Trade sessions: "intraday" or "all"
    trade_sessions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MarketParam {
    /// Market code: HK, US, CN, SG
    market: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MarketDateRangeParam {
    /// Market code: HK, US, CN, SG
    market: String,
    /// Start date (yyyy-mm-dd)
    start: String,
    /// End date (yyyy-mm-dd)
    end: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SymbolDateParam {
    symbol: String,
    /// Date (yyyy-mm-dd)
    date: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WarrantListParam {
    /// Underlying symbol, e.g. "700.HK"
    symbol: String,
    /// Sort field: LastDone, ChangeRate, ChangeValue, Volume, Turnover, ExpiryDate, StrikePrice, UpperStrikePrice, LowerStrikePrice, OutstandingQuantity, OutstandingRatio, Premium, ItmOtm, ImpliedVolatility, Delta
    sort_by: String,
    /// Sort order: Ascending or Descending
    sort_order: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CalcIndexesParam {
    /// Security symbols, e.g. ["700.HK", "AAPL.US"]
    symbols: Vec<String>,
    /// Calc indexes: LastDone, ChangeValue, ChangeRate, Volume, Turnover, YtdChangeRate, TurnoverRate, TotalMarketValue, CapitalFlow, Amplitude, VolumeRatio, PeTtmRatio, PbRatio, DividendRatioTtm, FiveDayChangeRate, TenDayChangeRate, HalfYearChangeRate, FiveMinutesChangeRate, ExpiryDate, StrikePrice, UpperStrikePrice, LowerStrikePrice, OutstandingQty, OutstandingRatio, Premium, ItmOtm, ImpliedVolatility, WarrantDelta, CallPrice, ToCallPrice, EffectiveLeverage, LeverageRatio, ConversionRatio, BalancePoint, OpenInterest, Delta, Gamma, Theta, Vega, Rho
    indexes: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateWatchlistGroupParam {
    /// Group name
    name: String,
    /// Securities to add, e.g. ["700.HK", "AAPL.US"]
    securities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DeleteWatchlistGroupParam {
    /// Watchlist group id
    id: i64,
    /// Whether to also remove the securities from other groups
    purge: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct UpdateWatchlistGroupParam {
    /// Watchlist group id
    id: i64,
    /// New group name (optional)
    name: Option<String>,
    /// Securities list (optional)
    securities: Option<Vec<String>>,
    /// Update mode for securities: "add", "remove", or "replace" (default: "replace")
    mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SecurityListParam {
    /// Market code: HK, US, CN, SG
    market: String,
    /// Category filter (optional): "Overnight"
    category: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct OrderIdParam {
    order_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SubmitOrderParam {
    symbol: String,
    /// LO/ELO/MO/AO/ALO/ODD/LIT/MIT/TSLPAMT/TSLPPCT/SLO
    order_type: String,
    /// Buy or Sell
    side: String,
    submitted_quantity: String,
    /// Day/GTC/GTD
    time_in_force: String,
    /// For LO, ELO, ALO, ODD, LIT
    submitted_price: Option<String>,
    /// For LIT, MIT
    trigger_price: Option<String>,
    /// For TSLPAMT, TSLPPCT
    limit_offset: Option<String>,
    /// For TSLPAMT
    trailing_amount: Option<String>,
    /// For TSLPPCT (0-1)
    trailing_percent: Option<String>,
    /// Format: yyyy-mm-dd
    expire_date: Option<String>,
    /// RTH_ONLY/ANY_TIME/OVERNIGHT
    outside_rth: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ReplaceOrderParam {
    order_id: String,
    quantity: String,
    price: Option<String>,
    trigger_price: Option<String>,
    limit_offset: Option<String>,
    trailing_amount: Option<String>,
    trailing_percent: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct HistoryOrdersParam {
    /// Filter by symbol (optional)
    symbol: Option<String>,
    /// Start time (RFC3339)
    start_at: String,
    /// End time (RFC3339)
    end_at: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CashFlowParam {
    /// Start time (RFC3339)
    start_at: String,
    /// End time (RFC3339)
    end_at: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct EstimateMaxQtyParam {
    symbol: String,
    /// Buy or Sell
    side: String,
    /// LO/ELO/MO/AO/ALO
    order_type: String,
    price: Option<String>,
}

#[tool_router]
impl Longbridge {
    /// Get current UTC time in RFC3339 format.
    #[tool(description = "Get current UTC time")]
    async fn now(&self) -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .expect("failed to format current time")
    }

    /// Get basic information of securities.
    #[tool(description = "Get basic information of securities (name, exchange, type, lot_size)")]
    async fn static_info(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("static_info", || quote::static_info(reg, &user_id, p)).await
    }

    /// Get the latest price quotes.
    #[tool(description = "Get latest price quotes (last_done, open, high, low, volume, turnover)")]
    async fn quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("quote", || quote::quote(reg, &user_id, p)).await
    }

    /// Get option quotes.
    #[tool(description = "Get option quotes (max 500 symbols)")]
    async fn option_quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("option_quote", || quote::option_quote(reg, &user_id, p)).await
    }

    /// Get warrant quotes.
    #[tool(description = "Get warrant quotes")]
    async fn warrant_quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("warrant_quote", || quote::warrant_quote(reg, &user_id, p)).await
    }

    /// Get the order book depth.
    #[tool(description = "Get order book depth (bid/ask levels)")]
    async fn depth(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("depth", || quote::depth(reg, &user_id, p)).await
    }

    /// Get broker queue data.
    #[tool(description = "Get broker queue data")]
    async fn brokers(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("brokers", || quote::brokers(reg, &user_id, p)).await
    }

    /// Get market participant broker information.
    #[tool(description = "Get market participant broker information")]
    async fn participants(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("participants", || quote::participants(reg, &user_id)).await
    }

    /// Get recent trades.
    #[tool(description = "Get recent trades (max 1000)")]
    async fn trades(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolCountParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("trades", || quote::trades(reg, &user_id, p)).await
    }

    /// Get intraday line data.
    #[tool(description = "Get intraday minute-by-minute price/volume data")]
    async fn intraday(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("intraday", || quote::intraday(reg, &user_id, p)).await
    }

    /// Get candlestick (K-line) data.
    #[tool(
        description = "Get candlestick data (OHLCV). period: 1m/5m/15m/30m/60m/day/week/month/year. trade_sessions: intraday/all"
    )]
    async fn candlesticks(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CandlesticksParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("candlesticks", || quote::candlesticks(reg, &user_id, p)).await
    }

    /// Get historical candlesticks by offset.
    #[tool(
        description = "Get historical candlestick data by offset from a reference time. period: 1m/5m/15m/30m/60m/day/week/month/year"
    )]
    async fn history_candlesticks_by_offset(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryCandlesticksByOffsetParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("history_candlesticks_by_offset", || {
            quote::history_candlesticks_by_offset(reg, &user_id, p)
        })
        .await
    }

    /// Get historical candlesticks by date range.
    #[tool(
        description = "Get historical candlestick data by date range. period: 1m/5m/15m/30m/60m/day/week/month/year"
    )]
    async fn history_candlesticks_by_date(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryCandlesticksByDateParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("history_candlesticks_by_date", || {
            quote::history_candlesticks_by_date(reg, &user_id, p)
        })
        .await
    }

    /// Get trading days between dates.
    #[tool(description = "Get trading days for a market between dates")]
    async fn trading_days(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketDateRangeParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("trading_days", || quote::trading_days(reg, &user_id, p)).await
    }

    /// Get option chain expiry date list.
    #[tool(description = "Get option chain expiry dates for a symbol")]
    async fn option_chain_expiry_date_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("option_chain_expiry_date_list", || {
            quote::option_chain_expiry_date_list(reg, &user_id, p)
        })
        .await
    }

    /// Get option chain info by expiry date.
    #[tool(description = "Get option chain strike prices and Greeks for an expiry date")]
    async fn option_chain_info_by_date(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolDateParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("option_chain_info_by_date", || {
            quote::option_chain_info_by_date(reg, &user_id, p)
        })
        .await
    }

    /// Get capital flow of a security.
    #[tool(description = "Get capital inflow/outflow time series")]
    async fn capital_flow(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("capital_flow", || quote::capital_flow(reg, &user_id, p)).await
    }

    /// Get capital distribution.
    #[tool(description = "Get capital distribution (large/medium/small holder flows)")]
    async fn capital_distribution(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("capital_distribution", || {
            quote::capital_distribution(reg, &user_id, p)
        })
        .await
    }

    /// Get trading session schedule.
    #[tool(description = "Get trading session schedule for all markets")]
    async fn trading_session(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("trading_session", || quote::trading_session(reg, &user_id)).await
    }

    /// Get market temperature.
    #[tool(description = "Get current market sentiment temperature (0-100)")]
    async fn market_temperature(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("market_temperature", || {
            quote::market_temperature(reg, &user_id, p)
        })
        .await
    }

    /// Get historical market temperature.
    #[tool(description = "Get historical market temperature time series")]
    async fn history_market_temperature(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketDateRangeParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("history_market_temperature", || {
            quote::history_market_temperature(reg, &user_id, p)
        })
        .await
    }

    /// Get watchlist groups.
    #[tool(description = "Get all watchlist groups and their securities")]
    async fn watchlist(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("watchlist", || quote::watchlist(reg, &user_id)).await
    }

    /// Get filings for a symbol.
    #[tool(description = "Get regulatory filings (8-K, 10-Q, 10-K, etc.)")]
    async fn filings(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("filings", || quote::filings(reg, &user_id, p)).await
    }

    /// Get warrant issuers.
    #[tool(description = "Get warrant issuer information")]
    async fn warrant_issuers(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("warrant_issuers", || quote::warrant_issuers(reg, &user_id)).await
    }

    /// Get warrant list for a symbol.
    #[tool(description = "Get filtered warrant list for an underlying symbol")]
    async fn warrant_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<WarrantListParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("warrant_list", || quote::warrant_list(reg, &user_id, p)).await
    }

    /// Calculate indexes for symbols.
    #[tool(description = "Calculate financial indexes (PE, PB, dividend ratio, etc.) for symbols")]
    async fn calc_indexes(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CalcIndexesParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("calc_indexes", || quote::calc_indexes(reg, &user_id, p)).await
    }

    /// Create a watchlist group.
    #[tool(description = "Create a new watchlist group with optional initial securities")]
    async fn create_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CreateWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("create_watchlist_group", || {
            quote::create_watchlist_group(reg, &user_id, p)
        })
        .await
    }

    /// Delete a watchlist group.
    #[tool(description = "Delete a watchlist group by id")]
    async fn delete_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<DeleteWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("delete_watchlist_group", || {
            quote::delete_watchlist_group(reg, &user_id, p)
        })
        .await
    }

    /// Update a watchlist group.
    #[tool(description = "Update a watchlist group (rename or add/remove/replace securities)")]
    async fn update_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<UpdateWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("update_watchlist_group", || {
            quote::update_watchlist_group(reg, &user_id, p)
        })
        .await
    }

    /// Get security list by market and category.
    #[tool(description = "Get security list for a market, optionally filtered by category")]
    async fn security_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SecurityListParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("security_list", || quote::security_list(reg, &user_id, p)).await
    }

    /// Get account balance.
    #[tool(description = "Get account cash balance and asset summary")]
    async fn account_balance(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("account_balance", || trade::account_balance(reg, &user_id)).await
    }

    /// Get stock positions.
    #[tool(description = "Get current stock positions across all channels")]
    async fn stock_positions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("stock_positions", || trade::stock_positions(reg, &user_id)).await
    }

    /// Get fund positions.
    #[tool(description = "Get current fund positions")]
    async fn fund_positions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("fund_positions", || trade::fund_positions(reg, &user_id)).await
    }

    /// Get margin ratio.
    #[tool(description = "Get margin ratio (initial/maintenance/forced liquidation)")]
    async fn margin_ratio(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("margin_ratio", || trade::margin_ratio(reg, &user_id, p)).await
    }

    /// Get today's orders.
    #[tool(description = "Get orders placed today")]
    async fn today_orders(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("today_orders", || trade::today_orders(reg, &user_id)).await
    }

    /// Get order detail.
    #[tool(description = "Get detailed information about a specific order")]
    async fn order_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("order_detail", || trade::order_detail(reg, &user_id, p)).await
    }

    /// Cancel an order.
    #[tool(description = "Cancel an open order by order_id")]
    async fn cancel_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("cancel_order", || trade::cancel_order(reg, &user_id, p)).await
    }

    /// Get today's trade executions.
    #[tool(description = "Get today's trade executions (fills)")]
    async fn today_executions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("today_executions", || {
            trade::today_executions(reg, &user_id)
        })
        .await
    }

    /// Get historical orders (not including today).
    #[tool(description = "Get historical orders between dates (excludes today)")]
    async fn history_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("history_orders", || trade::history_orders(reg, &user_id, p)).await
    }

    /// Get historical executions.
    #[tool(description = "Get historical trade executions between dates")]
    async fn history_executions(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("history_executions", || {
            trade::history_executions(reg, &user_id, p)
        })
        .await
    }

    /// Get cash flow records.
    #[tool(description = "Get cash flow records (deposits, withdrawals, dividends)")]
    async fn cash_flow(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CashFlowParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("cash_flow", || trade::cash_flow(reg, &user_id, p)).await
    }

    /// Submit an order.
    #[tool(
        description = "Submit a buy/sell order. order_type: LO/ELO/MO/AO/ALO/ODD/LIT/MIT/TSLPAMT/TSLPPCT/SLO. side: Buy/Sell. time_in_force: Day/GTC/GTD"
    )]
    async fn submit_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SubmitOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("submit_order", || trade::submit_order(reg, &user_id, p)).await
    }

    /// Replace (modify) an order.
    #[tool(description = "Replace/modify an existing order")]
    async fn replace_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ReplaceOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("replace_order", || trade::replace_order(reg, &user_id, p)).await
    }

    /// Estimate max purchase quantity.
    #[tool(description = "Estimate maximum buy/sell quantity for a symbol")]
    async fn estimate_max_purchase_quantity(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<EstimateMaxQtyParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("estimate_max_purchase_quantity", || {
            trade::estimate_max_purchase_quantity(reg, &user_id, p)
        })
        .await
    }

    /// Get financial reports (income statement, balance sheet, cash flow).
    #[tool(description = "Get financial reports for a symbol. report_type: annual or quarterly")]
    async fn financial_report(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialReportParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("financial_report", || {
            fundamental::financial_report(reg, &user_id, p)
        })
        .await
    }

    /// Get institution rating summary (analyst consensus + target price).
    #[tool(description = "Get institution rating summary with analyst consensus and target price")]
    async fn institution_rating(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("institution_rating", || {
            fundamental::institution_rating(reg, &user_id, p)
        })
        .await
    }

    /// Get institution rating detail (historical ratings and target prices).
    #[tool(description = "Get detailed historical institution ratings and target price history")]
    async fn institution_rating_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("institution_rating_detail", || {
            fundamental::institution_rating_detail(reg, &user_id, p)
        })
        .await
    }

    /// Get dividend history.
    #[tool(description = "Get dividend history for a symbol")]
    async fn dividend(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("dividend", || fundamental::dividend(reg, &user_id, p)).await
    }

    /// Get dividend distribution details.
    #[tool(description = "Get detailed dividend distribution scheme")]
    async fn dividend_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("dividend_detail", || {
            fundamental::dividend_detail(reg, &user_id, p)
        })
        .await
    }

    /// Get EPS forecast data.
    #[tool(description = "Get EPS forecast and analyst estimate history")]
    async fn forecast_eps(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("forecast_eps", || {
            fundamental::forecast_eps(reg, &user_id, p)
        })
        .await
    }

    /// Get financial consensus estimates.
    #[tool(description = "Get financial consensus estimates (revenue, EPS, net income)")]
    async fn consensus(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("consensus", || fundamental::consensus(reg, &user_id, p)).await
    }

    /// Get valuation overview (PE, PB, PS, dividend yield).
    #[tool(description = "Get valuation overview with peer comparison")]
    async fn valuation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("valuation", || fundamental::valuation(reg, &user_id, p)).await
    }

    /// Get detailed valuation history.
    #[tool(description = "Get detailed valuation history time series")]
    async fn valuation_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("valuation_history", || {
            fundamental::valuation_history(reg, &user_id, p)
        })
        .await
    }

    /// Get industry valuation comparison.
    #[tool(description = "Get industry valuation comparison for peers")]
    async fn industry_valuation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("industry_valuation", || {
            fundamental::industry_valuation(reg, &user_id, p)
        })
        .await
    }

    /// Get industry valuation distribution.
    #[tool(description = "Get industry PE/PB/PS valuation distribution")]
    async fn industry_valuation_dist(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("industry_valuation_dist", || {
            fundamental::industry_valuation_dist(reg, &user_id, p)
        })
        .await
    }

    /// Get company overview.
    #[tool(description = "Get company overview (name, CEO, employees, profile)")]
    async fn company(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("company", || fundamental::company(reg, &user_id, p)).await
    }

    /// Get company executives.
    #[tool(description = "Get company executive and board member information")]
    async fn executive(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("executive", || fundamental::executive(reg, &user_id, p)).await
    }

    /// Get shareholders.
    #[tool(description = "Get institutional shareholders for a symbol")]
    async fn shareholder(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("shareholder", || fundamental::shareholder(reg, &user_id, p)).await
    }

    /// Get fund holders.
    #[tool(description = "Get funds and ETFs that hold a given symbol")]
    async fn fund_holder(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("fund_holder", || fundamental::fund_holder(reg, &user_id, p)).await
    }

    /// Get corporate actions.
    #[tool(description = "Get corporate actions (splits, buybacks, name changes)")]
    async fn corp_action(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("corp_action", || fundamental::corp_action(reg, &user_id, p)).await
    }

    /// Get investor relations events.
    #[tool(description = "Get investor relations events and announcements")]
    async fn invest_relation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("invest_relation", || {
            fundamental::invest_relation(reg, &user_id, p)
        })
        .await
    }

    /// Get operating metrics.
    #[tool(description = "Get company operating metrics")]
    async fn operating(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("operating", || fundamental::operating(reg, &user_id, p)).await
    }

    /// Get market trading status.
    #[tool(description = "Get current market trading status for all markets")]
    async fn market_status(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("market_status", || market::market_status(reg, &user_id)).await
    }

    /// Get broker holding data.
    #[tool(description = "Get top broker holding data for a symbol")]
    async fn broker_holding(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("broker_holding", || {
            market::broker_holding(reg, &user_id, p)
        })
        .await
    }

    /// Get broker holding detail.
    #[tool(description = "Get full broker holding detail list")]
    async fn broker_holding_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("broker_holding_detail", || {
            market::broker_holding_detail(reg, &user_id, p)
        })
        .await
    }

    /// Get daily broker holding for a specific broker.
    #[tool(description = "Get daily holding history for a specific broker")]
    async fn broker_holding_daily(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::BrokerHoldingDailyParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("broker_holding_daily", || {
            market::broker_holding_daily(reg, &user_id, p)
        })
        .await
    }

    /// Get AH premium K-line data.
    #[tool(description = "Get A/H share premium historical K-line data")]
    async fn ah_premium(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("ah_premium", || market::ah_premium(reg, &user_id, p)).await
    }

    /// Get AH premium intraday data.
    #[tool(description = "Get A/H share premium intraday time-share data")]
    async fn ah_premium_intraday(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("ah_premium_intraday", || {
            market::ah_premium_intraday(reg, &user_id, p)
        })
        .await
    }

    /// Get trade statistics.
    #[tool(description = "Get trade statistics (buy/sell/neutral volume distribution)")]
    async fn trade_stats(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("trade_stats", || market::trade_stats(reg, &user_id, p)).await
    }

    /// Get market anomalies.
    #[tool(description = "Get market anomaly alerts (unusual price/volume changes)")]
    async fn anomaly(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::MarketParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("anomaly", || market::anomaly(reg, &user_id, p)).await
    }

    /// Get index constituents.
    #[tool(description = "Get constituent stocks of an index (e.g. HSI.HK)")]
    async fn constituent(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::IndexSymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("constituent", || market::constituent(reg, &user_id, p)).await
    }

    /// Get finance calendar events.
    #[tool(
        description = "Get finance calendar events. category: financial/report/dividend/ipo/macrodata/closed"
    )]
    async fn finance_calendar(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<calendar::FinanceCalendarParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("finance_calendar", || {
            calendar::finance_calendar(reg, &user_id, p)
        })
        .await
    }

    /// Get exchange rates.
    #[tool(description = "Get exchange rates for all supported currencies")]
    async fn exchange_rate(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("exchange_rate", || portfolio::exchange_rate(reg, &user_id)).await
    }

    /// Get profit analysis summary.
    #[tool(description = "Get portfolio profit and loss analysis summary")]
    async fn profit_analysis(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("profit_analysis", || {
            portfolio::profit_analysis(reg, &user_id)
        })
        .await
    }

    /// Get profit analysis detail for a symbol.
    #[tool(description = "Get detailed profit and loss analysis for a specific symbol")]
    async fn profit_analysis_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<portfolio::ProfitAnalysisDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("profit_analysis_detail", || {
            portfolio::profit_analysis_detail(reg, &user_id, p)
        })
        .await
    }

    /// Get price alert list.
    #[tool(description = "Get all configured price alerts")]
    async fn alert_list(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("alert_list", || alert::alert_list(reg, &user_id)).await
    }

    /// Add a price alert.
    #[tool(
        description = "Add a price alert. condition: price_rise/price_fall/percent_rise/percent_fall"
    )]
    async fn alert_add(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertAddParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("alert_add", || alert::alert_add(reg, &user_id, p)).await
    }

    /// Delete a price alert.
    #[tool(description = "Delete a price alert by alert_id")]
    async fn alert_delete(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("alert_delete", || alert::alert_delete(reg, &user_id, p)).await
    }

    /// Enable a price alert.
    #[tool(description = "Enable a price alert by alert_id")]
    async fn alert_enable(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("alert_enable", || alert::alert_enable(reg, &user_id, p)).await
    }

    /// Disable a price alert.
    #[tool(description = "Disable a price alert by alert_id")]
    async fn alert_disable(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("alert_disable", || alert::alert_disable(reg, &user_id, p)).await
    }

    /// Get news for a symbol.
    #[tool(description = "Get latest news articles for a symbol")]
    async fn news(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("news", || content::news(reg, &user_id, p)).await
    }

    /// Get news detail by id.
    #[tool(description = "Get full news article content by news_id")]
    async fn news_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::NewsDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        let language = extract_language(&ctx);
        measured_tool_call("news_detail", || {
            content::news_detail(reg, &user_id, p, language)
        })
        .await
    }

    /// Get discussion topics for a symbol.
    #[tool(description = "Get discussion topics for a symbol")]
    async fn topic(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("topic", || content::topic(reg, &user_id, p)).await
    }

    /// Get topic detail.
    #[tool(description = "Get discussion topic detail by topic_id")]
    async fn topic_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        let language = extract_language(&ctx);
        measured_tool_call("topic_detail", || {
            content::topic_detail(reg, &user_id, p, language)
        })
        .await
    }

    /// Get topic replies.
    #[tool(description = "Get replies to a discussion topic")]
    async fn topic_replies(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        let language = extract_language(&ctx);
        measured_tool_call("topic_replies", || {
            content::topic_replies(reg, &user_id, p, language)
        })
        .await
    }

    /// Create a discussion topic.
    #[tool(description = "Create a new discussion topic")]
    async fn topic_create(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicCreateParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("topic_create", || content::topic_create(reg, &user_id, p)).await
    }

    /// Reply to a discussion topic.
    #[tool(description = "Create a reply to a discussion topic")]
    async fn topic_create_reply(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicCreateReplyParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("topic_create_reply", || {
            content::topic_create_reply(reg, &user_id, p)
        })
        .await
    }

    /// Get filing detail.
    #[tool(description = "Get regulatory filing detail by filing_id")]
    async fn filing_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::FilingDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        let language = extract_language(&ctx);
        measured_tool_call("filing_detail", || {
            content::filing_detail(reg, &user_id, p, language)
        })
        .await
    }

    /// List account statements.
    #[tool(description = "List available account statements (daily/monthly)")]
    async fn statement_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<statement::StatementListParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("statement_list", || {
            statement::statement_list(reg, &user_id, p)
        })
        .await
    }

    /// Export account statement.
    #[tool(description = "Export account statement sections by file_key")]
    async fn statement_export(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<statement::StatementExportParam>,
    ) -> Result<CallToolResult, McpError> {
        let user_id = extract_user_id(&ctx)?;
        let reg = &self.registry;
        measured_tool_call("statement_export", || {
            statement::statement_export(reg, &user_id, p)
        })
        .await
    }
}

#[tool_handler(
    name = "longbridge-mcp",
    version = "0.1.0",
    instructions = "Longbridge OpenAPI MCP Server - provides market data, trading, and financial analysis tools"
)]
impl ServerHandler for Longbridge {}
