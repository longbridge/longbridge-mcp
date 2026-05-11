use std::sync::Arc;

use rmcp::ErrorData as McpError;
use rmcp::RoleServer;
use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content};
use rmcp::service::RequestContext;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;

use crate::auth::middleware::BearerToken;
use crate::error::Error;
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
mod atm;
mod calendar;
mod content;
mod dca;
mod fundamental;
mod ipo;
mod market;
mod output;
mod portfolio;
mod quant;
mod quote;
mod search;
mod sharelist;
mod statement;
mod support;
mod trade;

/// Helper to build a JSON Schema `Arc<JsonObject>` from a `JsonSchema`-derived
/// type, suitable for passing to `#[tool(output_schema = ...)]`.
fn schema_for<T>() -> std::sync::Arc<rmcp::model::JsonObject>
where
    T: rmcp::schemars::JsonSchema + 'static,
{
    rmcp::handler::server::common::schema_for_output::<T>()
        .expect("output schema must be a valid JSON Schema with root type \"object\"")
}

/// Longbridge MCP tool server (stateless).
#[derive(Debug, Clone)]
pub struct Longbridge;

fn tool_result(json: String) -> CallToolResult {
    // MCP spec §tool-result: a tool that declares an `outputSchema` MUST
    // return `structuredContent`. We populate it for every response so the
    // invariant holds regardless of which tools gain a schema in the future.
    let structured = serde_json::from_str::<serde_json::Value>(&json).ok();
    let mut result = CallToolResult::success(vec![Content::text(json)]);
    result.structured_content = structured;
    result
}

fn tool_json<T>(value: &T) -> Result<CallToolResult, McpError>
where
    T: serde::Serialize,
{
    let json = to_tool_json(value).map_err(Error::Serialize)?;
    Ok(tool_result(json))
}

/// Per-request context extracted from HTTP headers.
pub struct McpContext {
    pub token: String,
    pub language: Option<String>,
}

impl McpContext {
    pub fn create_config(&self) -> Arc<longbridge::Config> {
        let mut config =
            longbridge::Config::from_oauth(longbridge::oauth::OAuth::from_token(&self.token))
                .dont_print_quote_packages()
                .enable_overnight();
        if let Some(ref lang) = self.language {
            let lb_lang = if lang.contains("zh-CN") || lang.contains("zh-Hans") {
                longbridge::Language::ZH_CN
            } else if lang.contains("zh") {
                longbridge::Language::ZH_HK
            } else {
                longbridge::Language::EN
            };
            config = config.language(lb_lang);
        }
        Arc::new(config)
    }

    pub fn create_http_client(&self) -> longbridge::httpclient::HttpClient {
        longbridge::httpclient::HttpClient::new(
            longbridge::httpclient::HttpClientConfig::from_oauth(
                longbridge::oauth::OAuth::from_token(&self.token),
            ),
        )
    }

    /// Extracts `account_channel` from the JWT bearer token's `sub` claim.
    /// Falls back to `"lb"` when the token cannot be decoded.
    pub fn account_channel(&self) -> String {
        decode_jwt_account_channel(&self.token).unwrap_or_else(|| "lb".to_string())
    }
}

/// Decodes the JWT payload (no signature verification) and extracts `account_channel`
/// from the `sub` claim, which Longbridge encodes as a nested JSON string.
fn decode_jwt_account_channel(token: &str) -> Option<String> {
    let payload_b64 = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload_b64)?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let sub_str = claims["sub"].as_str()?;
    let sub: serde_json::Value = serde_json::from_str(sub_str).ok()?;
    sub["account_channel"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Minimal base64url decoder (no padding required, no external crate).
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut table = [0xffu8; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[c as usize] = i as u8;
    }
    // base64url uses - and _ instead of + and /
    table[b'-' as usize] = 62;
    table[b'_' as usize] = 63;

    let input: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut i = 0;
    while i < input.len() {
        let get = |pos: usize| -> Option<u8> {
            input.get(pos).and_then(|&b| {
                let v = table[b as usize];
                if v == 0xff { None } else { Some(v) }
            })
        };
        let b0 = get(i)?;
        let b1 = get(i + 1)?;
        out.push((b0 << 2) | (b1 >> 4));
        if let Some(b2) = get(i + 2) {
            out.push((b1 << 4) | (b2 >> 2));
            if let Some(b3) = get(i + 3) {
                out.push((b2 << 6) | b3);
            }
        }
        i += 4;
    }
    Some(out)
}

fn extract_context(ctx: &RequestContext<RoleServer>) -> Result<McpContext, McpError> {
    let parts = ctx
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::internal_error("missing request parts", None))?;
    let token = parts
        .extensions
        .get::<BearerToken>()
        .ok_or_else(|| McpError::internal_error("not authenticated", None))?;
    let language = parts
        .headers
        .get("accept-language")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    Ok(McpContext {
        token: token.0.clone(),
        language,
    })
}

/// Returns all registered MCP tools sorted by name.
pub fn list_tools() -> Vec<rmcp::model::Tool> {
    Longbridge::tool_router().list_all()
}

use crate::tools::quote::{
    CalcIndexesParam, CandlesticksParam, CreateWatchlistGroupParam, DeleteWatchlistGroupParam,
    HistoryCandlesticksByDateParam, HistoryCandlesticksByOffsetParam, MarketDateRangeParam,
    MarketParam, OptionVolumeDailyParam, OptionVolumeParam, SecurityListParam, ShortPositionsParam,
    SymbolCountParam, SymbolDateParam, SymbolParam, SymbolsParam, UpdateWatchlistGroupParam,
    WarrantListParam,
};
use crate::tools::trade::{
    CashFlowParam, EstimateMaxQtyParam, HistoryOrdersParam, OrderIdParam, ReplaceOrderParam,
    SubmitOrderParam,
};

#[tool_router(vis = "pub(crate)")]
impl Longbridge {
    /// Get current UTC time in RFC3339 format.
    #[tool(
        title = "Current Time",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get current UTC time"
    )]
    async fn now(&self) -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .expect("failed to format current time")
    }

    /// Get basic information of securities.
    #[tool(
        title = "Security Static Info",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get basic information of securities (name_cn, name_en, exchange, type, lot_size)"
    )]
    async fn static_info(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("static_info", || quote::static_info(&mctx, p)).await
    }

    /// Get the latest price quotes.
    #[tool(
        title = "Quote",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get latest price quotes (last_done, open, high, low, volume, turnover)"
    )]
    async fn quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("quote", || quote::quote(&mctx, p)).await
    }

    /// Get option quotes.
    #[tool(
        title = "Option Quote",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get option quotes (max 500 symbols)"
    )]
    async fn option_quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_quote", || quote::option_quote(&mctx, p)).await
    }

    /// Get warrant quotes.
    #[tool(
        title = "Warrant Quote",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get warrant quotes"
    )]
    async fn warrant_quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("warrant_quote", || quote::warrant_quote(&mctx, p)).await
    }

    /// Get the order book depth.
    #[tool(
        title = "Order Book Depth",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::DepthResponse>(),
        description = "Get order book depth (asks/bids arrays with price, volume, order_count)"
    )]
    async fn depth(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("depth", || quote::depth(&mctx, p)).await
    }

    /// Get broker queue data.
    #[tool(
        title = "Broker Queue",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::BrokersResponse>(),
        description = "Get broker queue data"
    )]
    async fn brokers(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("brokers", || quote::brokers(&mctx, p)).await
    }

    /// Get market participant broker information.
    #[tool(
        title = "Market Participants",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get market participant broker information"
    )]
    async fn participants(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("participants", || quote::participants(&mctx)).await
    }

    /// Get recent trades.
    #[tool(
        title = "Recent Trades",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get recent trades (max 1000)"
    )]
    async fn trades(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolCountParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("trades", || quote::trades(&mctx, p)).await
    }

    /// Get intraday line data.
    #[tool(
        title = "Intraday Line",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get intraday minute-by-minute price/volume data. trade_sessions: \"intraday\" (default, regular hours) or \"all\" (include pre-market and post-market)"
    )]
    async fn intraday(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<quote::IntradayParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("intraday", || quote::intraday(&mctx, p)).await
    }

    /// Get candlestick (K-line) data.
    #[tool(
        title = "Candlesticks",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get candlestick data (OHLCV). period: 1m/5m/15m/30m/60m/day/week/month/year. trade_sessions: intraday/all"
    )]
    async fn candlesticks(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CandlesticksParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("candlesticks", || quote::candlesticks(&mctx, p)).await
    }

    /// Get historical candlesticks by offset.
    #[tool(
        title = "Historical Candlesticks by Offset",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get historical candlestick data by offset from a reference time. period: 1m/5m/15m/30m/60m/day/week/month/year"
    )]
    async fn history_candlesticks_by_offset(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryCandlesticksByOffsetParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_candlesticks_by_offset", || {
            quote::history_candlesticks_by_offset(&mctx, p)
        })
        .await
    }

    /// Get historical candlesticks by date range.
    #[tool(
        title = "Historical Candlesticks by Date",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get historical candlestick data by date range. period: 1m/5m/15m/30m/60m/day/week/month/year"
    )]
    async fn history_candlesticks_by_date(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryCandlesticksByDateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_candlesticks_by_date", || {
            quote::history_candlesticks_by_date(&mctx, p)
        })
        .await
    }

    /// Get trading days between dates.
    #[tool(
        title = "Trading Days",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::TradingDaysResponse>(),
        description = "Get trading days for a market between dates"
    )]
    async fn trading_days(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketDateRangeParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("trading_days", || quote::trading_days(&mctx, p)).await
    }

    /// Get option chain expiry date list.
    #[tool(
        title = "Option Expiry Dates",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get option chain expiry dates for a symbol"
    )]
    async fn option_chain_expiry_date_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_chain_expiry_date_list", || {
            quote::option_chain_expiry_date_list(&mctx, p)
        })
        .await
    }

    /// Get option chain info by expiry date.
    #[tool(
        title = "Option Chain by Date",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get option chain strike prices and Greeks for an expiry date"
    )]
    async fn option_chain_info_by_date(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolDateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_chain_info_by_date", || {
            quote::option_chain_info_by_date(&mctx, p)
        })
        .await
    }

    /// Get capital flow of a security.
    #[tool(
        title = "Capital Flow",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get capital inflow/outflow time series"
    )]
    async fn capital_flow(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("capital_flow", || quote::capital_flow(&mctx, p)).await
    }

    /// Get capital distribution.
    #[tool(
        title = "Capital Distribution",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::CapitalDistributionResponse>(),
        description = "Get capital distribution (large/medium/small holder flows)"
    )]
    async fn capital_distribution(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("capital_distribution", || {
            quote::capital_distribution(&mctx, p)
        })
        .await
    }

    /// Get trading session schedule.
    #[tool(
        title = "Trading Sessions",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get trading session schedule for all markets"
    )]
    async fn trading_session(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("trading_session", || quote::trading_session(&mctx)).await
    }

    /// Get market temperature.
    #[tool(
        title = "Market Temperature",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::MarketTemperatureResponse>(),
        description = "Get current market sentiment temperature (0-100)"
    )]
    async fn market_temperature(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("market_temperature", || quote::market_temperature(&mctx, p)).await
    }

    /// Get historical market temperature.
    #[tool(
        title = "Historical Market Temperature",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::HistoryMarketTemperatureResponse>(),
        description = "Get historical market temperature time series"
    )]
    async fn history_market_temperature(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketDateRangeParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_market_temperature", || {
            quote::history_market_temperature(&mctx, p)
        })
        .await
    }

    /// Get watchlist groups.
    #[tool(
        title = "Watchlist",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get all watchlist groups and their securities"
    )]
    async fn watchlist(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("watchlist", || quote::watchlist(&mctx)).await
    }

    /// Get filings for a symbol.
    #[tool(
        title = "Filings",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get regulatory filings (8-K, 10-Q, 10-K, etc.)"
    )]
    async fn filings(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("filings", || quote::filings(&mctx, p)).await
    }

    /// Get warrant issuers.
    #[tool(
        title = "Warrant Issuers",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get warrant issuer information"
    )]
    async fn warrant_issuers(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("warrant_issuers", || quote::warrant_issuers(&mctx)).await
    }

    /// Get warrant list for a symbol.
    #[tool(
        title = "Warrant List",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get filtered warrant list for an underlying symbol"
    )]
    async fn warrant_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<WarrantListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("warrant_list", || quote::warrant_list(&mctx, p)).await
    }

    /// Calculate indexes for symbols.
    #[tool(
        title = "Calc Indexes",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Calculate financial indexes (PE, PB, dividend ratio, etc.) for symbols"
    )]
    async fn calc_indexes(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CalcIndexesParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("calc_indexes", || quote::calc_indexes(&mctx, p)).await
    }

    /// Create a watchlist group.
    #[tool(
        title = "Create Watchlist Group",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Create a new watchlist group with optional initial securities"
    )]
    async fn create_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CreateWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("create_watchlist_group", || {
            quote::create_watchlist_group(&mctx, p)
        })
        .await
    }

    /// Delete a watchlist group.
    #[tool(
        title = "Delete Watchlist Group",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Delete a watchlist group by id"
    )]
    async fn delete_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<DeleteWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("delete_watchlist_group", || {
            quote::delete_watchlist_group(&mctx, p)
        })
        .await
    }

    /// Update a watchlist group.
    #[tool(
        title = "Update Watchlist Group",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Update a watchlist group (rename or add/remove/replace securities)"
    )]
    async fn update_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<UpdateWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("update_watchlist_group", || {
            quote::update_watchlist_group(&mctx, p)
        })
        .await
    }

    /// Get security list by market and category.
    #[tool(
        title = "Security List",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get security list for a market. category must be \"Overnight\"; other values or omitting it will cause an error. Currently only market=\"US\" is supported; other markets will also return an error"
    )]
    async fn security_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SecurityListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("security_list", || quote::security_list(&mctx, p)).await
    }

    /// Get account balance.
    #[tool(
        title = "Account Balance",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get account cash balance and asset summary. Pass currency (e.g. \"USD\", \"HKD\") to filter; omit to return all currencies."
    )]
    async fn account_balance(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<trade::AccountBalanceParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("account_balance", || trade::account_balance(&mctx, p)).await
    }

    /// Get stock positions.
    #[tool(
        title = "Stock Positions",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::StockPositionsResponse>(),
        description = "Get current stock positions across all channels"
    )]
    async fn stock_positions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("stock_positions", || trade::stock_positions(&mctx)).await
    }

    /// Get fund positions.
    #[tool(
        title = "Fund Positions",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::FundPositionsResponse>(),
        description = "Get current fund positions"
    )]
    async fn fund_positions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("fund_positions", || trade::fund_positions(&mctx)).await
    }

    /// Get margin ratio.
    #[tool(
        title = "Margin Ratio",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::MarginRatioResponse>(),
        description = "Get margin ratio (initial/maintenance/forced liquidation)"
    )]
    async fn margin_ratio(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("margin_ratio", || trade::margin_ratio(&mctx, p)).await
    }

    /// Get today's orders.
    #[tool(
        title = "Today's Orders",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get orders placed today. Pass symbol to filter; omit to return all."
    )]
    async fn today_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<trade::TodayOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("today_orders", || trade::today_orders(&mctx, p)).await
    }

    /// Get order detail.
    #[tool(
        title = "Order Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::OrderDetailResponse>(),
        description = "Get detailed information about a specific order"
    )]
    async fn order_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("order_detail", || trade::order_detail(&mctx, p)).await
    }

    /// Cancel an order.
    #[tool(
        title = "Cancel Order",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Cancel an open order by order_id"
    )]
    async fn cancel_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("cancel_order", || trade::cancel_order(&mctx, p)).await
    }

    /// Get today's trade executions.
    #[tool(
        title = "Today's Executions",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get today's trade executions (fills). Pass symbol or order_id to filter; omit both to return all."
    )]
    async fn today_executions(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<trade::TodayExecutionsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("today_executions", || trade::today_executions(&mctx, p)).await
    }

    /// Get historical orders (not including today).
    #[tool(
        title = "Historical Orders",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get historical orders between dates (excludes today)"
    )]
    async fn history_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_orders", || trade::history_orders(&mctx, p)).await
    }

    /// Get historical executions.
    #[tool(
        title = "Historical Executions",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get historical trade executions between dates"
    )]
    async fn history_executions(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_executions", || trade::history_executions(&mctx, p)).await
    }

    /// Get cash flow records.
    #[tool(
        title = "Cash Flow",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get cash flow records (deposits, withdrawals, dividends)"
    )]
    async fn cash_flow(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CashFlowParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("cash_flow", || trade::cash_flow(&mctx, p)).await
    }

    /// Submit an order.
    #[tool(
        title = "Submit Order",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::OrderIdResponse>(),
        description = "Submit a buy/sell order. order_type: LO (Limit) / ELO (Enhanced Limit, HK) / MO (Market) / AO (At-auction, HK) / ALO (At-auction Limit, HK) / ODD (Odd Lots, HK) / LIT (Limit If Touched) / MIT (Market If Touched) / TSLPAMT (Trailing Limit by Amount) / TSLPPCT (Trailing Limit by Percent) / SLO (Special Limit, HK). side: Buy/Sell. time_in_force: Day/GTC/GTD"
    )]
    async fn submit_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SubmitOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("submit_order", || trade::submit_order(&mctx, p)).await
    }

    /// Replace (modify) an order.
    #[tool(
        title = "Replace Order",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Replace/modify an existing order"
    )]
    async fn replace_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ReplaceOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("replace_order", || trade::replace_order(&mctx, p)).await
    }

    /// Estimate max purchase quantity.
    #[tool(
        title = "Estimate Max Purchase Quantity",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::EstimateMaxQtyResponse>(),
        description = "Estimate maximum buy/sell quantity for a symbol"
    )]
    async fn estimate_max_purchase_quantity(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<EstimateMaxQtyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("estimate_max_purchase_quantity", || {
            trade::estimate_max_purchase_quantity(&mctx, p)
        })
        .await
    }

    /// Get financial reports (income statement, balance sheet, cash flow).
    #[tool(
        title = "Financial Report",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get financial reports for a symbol. report_type: annual or quarterly"
    )]
    async fn financial_report(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialReportParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_report", || {
            fundamental::financial_report(&mctx, p)
        })
        .await
    }

    /// Get institution rating summary (analyst consensus + target price).
    #[tool(
        title = "Institution Rating",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get institution rating summary with analyst consensus and target price"
    )]
    async fn institution_rating(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institution_rating", || {
            fundamental::institution_rating(&mctx, p)
        })
        .await
    }

    /// Get institution rating detail (historical ratings and target prices).
    #[tool(
        title = "Institution Rating Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get detailed historical institution ratings and target price history"
    )]
    async fn institution_rating_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institution_rating_detail", || {
            fundamental::institution_rating_detail(&mctx, p)
        })
        .await
    }

    /// Get dividend history.
    #[tool(
        title = "Dividend",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get dividend history for a symbol"
    )]
    async fn dividend(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dividend", || fundamental::dividend(&mctx, p)).await
    }

    /// Get dividend distribution details.
    #[tool(
        title = "Dividend Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get detailed dividend distribution scheme"
    )]
    async fn dividend_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dividend_detail", || fundamental::dividend_detail(&mctx, p)).await
    }

    /// Get EPS forecast data.
    #[tool(
        title = "Forecast EPS",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get EPS forecast and analyst estimate history"
    )]
    async fn forecast_eps(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("forecast_eps", || fundamental::forecast_eps(&mctx, p)).await
    }

    /// Get financial consensus estimates.
    #[tool(
        title = "Analyst Consensus",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get financial consensus estimates (revenue, EPS, net income)"
    )]
    async fn consensus(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("consensus", || fundamental::consensus(&mctx, p)).await
    }

    /// Get valuation overview (PE, PB, PS, dividend yield).
    #[tool(
        title = "Valuation",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get valuation overview with peer comparison"
    )]
    async fn valuation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("valuation", || fundamental::valuation(&mctx, p)).await
    }

    /// Get detailed valuation history.
    #[tool(
        title = "Valuation History",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get detailed valuation history time series"
    )]
    async fn valuation_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("valuation_history", || {
            fundamental::valuation_history(&mctx, p)
        })
        .await
    }

    /// Get industry valuation comparison.
    #[tool(
        title = "Industry Valuation",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get industry valuation comparison for peers"
    )]
    async fn industry_valuation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("industry_valuation", || {
            fundamental::industry_valuation(&mctx, p)
        })
        .await
    }

    /// Get industry valuation distribution.
    #[tool(
        title = "Industry Valuation Distribution",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get industry PE/PB/PS valuation distribution"
    )]
    async fn industry_valuation_dist(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("industry_valuation_dist", || {
            fundamental::industry_valuation_dist(&mctx, p)
        })
        .await
    }

    /// Get company overview.
    #[tool(
        title = "Company Profile",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get company overview (name, CEO, employees, profile)"
    )]
    async fn company(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("company", || fundamental::company(&mctx, p)).await
    }

    /// Get company executives.
    #[tool(
        title = "Executive",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get company executive and board member information"
    )]
    async fn executive(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("executive", || fundamental::executive(&mctx, p)).await
    }

    /// Get shareholders.
    #[tool(
        title = "Shareholders",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get institutional shareholders for a symbol"
    )]
    async fn shareholder(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("shareholder", || fundamental::shareholder(&mctx, p)).await
    }

    /// Get fund holders.
    #[tool(
        title = "Fund Holders",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get funds and ETFs that hold a given symbol"
    )]
    async fn fund_holder(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("fund_holder", || fundamental::fund_holder(&mctx, p)).await
    }

    /// Get corporate actions.
    #[tool(
        title = "Corporate Actions",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get corporate actions (splits, buybacks, name changes)"
    )]
    async fn corp_action(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("corp_action", || fundamental::corp_action(&mctx, p)).await
    }

    /// Get investor relations events.
    #[tool(
        title = "Investor Relations",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get investor relations events and announcements"
    )]
    async fn invest_relation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("invest_relation", || fundamental::invest_relation(&mctx, p)).await
    }

    /// Get operating metrics.
    #[tool(
        title = "Operating Performance",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get company operating metrics"
    )]
    async fn operating(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("operating", || fundamental::operating(&mctx, p)).await
    }

    /// Get market trading status.
    #[tool(
        title = "Market Status",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get current market trading status for all markets"
    )]
    async fn market_status(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("market_status", || market::market_status(&mctx)).await
    }

    /// Get broker holding data.
    #[tool(
        title = "Broker Holding",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get top broker holding data for a symbol"
    )]
    async fn broker_holding(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::BrokerHoldingParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("broker_holding", || market::broker_holding(&mctx, p)).await
    }

    /// Get broker holding detail.
    #[tool(
        title = "Broker Holding Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get full broker holding detail list"
    )]
    async fn broker_holding_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("broker_holding_detail", || {
            market::broker_holding_detail(&mctx, p)
        })
        .await
    }

    /// Get daily broker holding for a specific broker.
    #[tool(
        title = "Broker Holding (Daily)",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get daily holding history for a specific broker"
    )]
    async fn broker_holding_daily(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::BrokerHoldingDailyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("broker_holding_daily", || {
            market::broker_holding_daily(&mctx, p)
        })
        .await
    }

    /// Get AH premium K-line data.
    #[tool(
        title = "A/H Premium",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get A/H share premium historical K-line data"
    )]
    async fn ah_premium(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::AhPremiumParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ah_premium", || market::ah_premium(&mctx, p)).await
    }

    /// Get AH premium intraday data.
    #[tool(
        title = "A/H Premium (Intraday)",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get A/H share premium intraday time-share data"
    )]
    async fn ah_premium_intraday(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ah_premium_intraday", || {
            market::ah_premium_intraday(&mctx, p)
        })
        .await
    }

    /// Get trade statistics.
    #[tool(
        title = "Trade Statistics",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get trade statistics (buy/sell/neutral volume distribution)"
    )]
    async fn trade_stats(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("trade_stats", || market::trade_stats(&mctx, p)).await
    }

    /// Get market anomalies.
    #[tool(
        title = "Market Anomaly",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get market anomaly alerts (unusual price/volume changes)"
    )]
    async fn anomaly(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::MarketParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("anomaly", || market::anomaly(&mctx, p)).await
    }

    /// Get index constituents.
    #[tool(
        title = "Index Constituents",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get constituent stocks of an index (e.g. HSI.HK)"
    )]
    async fn constituent(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::IndexSymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("constituent", || market::constituent(&mctx, p)).await
    }

    /// Get finance calendar events.
    #[tool(
        title = "Financial Calendar",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get finance calendar events by category and date range. category: report (earnings + financials) / dividend / split (splits & reverse splits) / ipo / macrodata (CPI, NFP, rate decisions) / closed (market holidays). market: HK/US/CN/SG/JP/UK/DE/AU (optional)."
    )]
    async fn finance_calendar(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<calendar::FinanceCalendarParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("finance_calendar", || calendar::finance_calendar(&mctx, p)).await
    }

    /// Get exchange rates.
    #[tool(
        title = "Exchange Rate",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get exchange rates for all supported currencies"
    )]
    async fn exchange_rate(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("exchange_rate", || portfolio::exchange_rate(&mctx)).await
    }

    /// Get profit analysis summary.
    #[tool(
        title = "Profit Analysis",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get portfolio profit and loss analysis summary. start/end: optional date range in yyyy-mm-dd format. Both must be provided together — passing only one returns empty results."
    )]
    async fn profit_analysis(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<portfolio::ProfitAnalysisParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("profit_analysis", || portfolio::profit_analysis(&mctx, p)).await
    }

    /// Get profit analysis detail for a symbol.
    #[tool(
        title = "Profit Analysis Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get detailed profit and loss analysis for a specific symbol. start/end: optional date range in yyyy-mm-dd format. Both must be provided together — passing only one returns empty results."
    )]
    async fn profit_analysis_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<portfolio::ProfitAnalysisDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("profit_analysis_detail", || {
            portfolio::profit_analysis_detail(&mctx, p)
        })
        .await
    }

    /// Get price alert list.
    #[tool(
        title = "List Price Alerts",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get all configured price alerts"
    )]
    async fn alert_list(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_list", || alert::alert_list(&mctx)).await
    }

    /// Add a price alert.
    #[tool(
        title = "Add Price Alert",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Add a price alert. condition: price_rise/price_fall/percent_rise/percent_fall"
    )]
    async fn alert_add(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertAddParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_add", || alert::alert_add(&mctx, p)).await
    }

    /// Delete a price alert.
    #[tool(
        title = "Delete Price Alert",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Delete a price alert by alert_id"
    )]
    async fn alert_delete(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_delete", || alert::alert_delete(&mctx, p)).await
    }

    /// Enable a price alert.
    #[tool(
        title = "Enable Price Alert",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Enable a price alert by alert_id"
    )]
    async fn alert_enable(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_enable", || alert::alert_enable(&mctx, p)).await
    }

    /// Disable a price alert.
    #[tool(
        title = "Disable Price Alert",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Disable a price alert by alert_id"
    )]
    async fn alert_disable(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_disable", || alert::alert_disable(&mctx, p)).await
    }

    /// Get news for a symbol.
    #[tool(
        title = "News",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get latest news articles for a symbol"
    )]
    async fn news(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("news", || content::news(&mctx, p)).await
    }

    /// Get discussion topics for a symbol.
    #[tool(
        title = "Topic List",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get discussion topics for a symbol"
    )]
    async fn topic(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic", || content::topic(&mctx, p)).await
    }

    /// Get topic detail.
    #[tool(
        title = "Topic Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get discussion topic detail by topic_id"
    )]
    async fn topic_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_detail", || content::topic_detail(&mctx, p)).await
    }

    /// Get topic replies.
    #[tool(
        title = "Topic Replies",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get replies to a discussion topic, paginated (page default 1, size default 20, range 1-50)"
    )]
    async fn topic_replies(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicRepliesParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_replies", || content::topic_replies(&mctx, p)).await
    }

    /// Create a discussion topic.
    #[tool(
        title = "Create Topic",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Create a new discussion topic. topic_type=\"post\" (default) is plain text; \"article\" requires a non-empty title and accepts Markdown body."
    )]
    async fn topic_create(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicCreateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_create", || content::topic_create(&mctx, p)).await
    }

    /// Reply to a discussion topic.
    #[tool(
        title = "Create Topic Reply",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Create a reply to a discussion topic. Pass reply_to_id to nest under another reply; omit for a top-level reply."
    )]
    async fn topic_create_reply(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicCreateReplyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_create_reply", || {
            content::topic_create_reply(&mctx, p)
        })
        .await
    }

    /// List account statements.
    #[tool(
        title = "Statement List",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List available account statements (daily/monthly)"
    )]
    async fn statement_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<statement::StatementListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("statement_list", || statement::statement_list(&mctx, p)).await
    }

    /// Get the pre-signed download URL for a statement file.
    #[tool(
        title = "Export Statement",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::StatementUrlResponse>(),
        description = "Get a pre-signed download URL for a statement data file (obtained from statement_list). Returns {url}; fetch that URL to get the statement JSON."
    )]
    async fn statement_export(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<statement::StatementExportParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("statement_export", || statement::statement_export(&mctx, p)).await
    }

    /// Get short position data for a US stock.
    #[tool(
        title = "Short Positions",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get short interest data for a US stock (short ratio, short shares, days to cover). Only US market is supported."
    )]
    async fn short_positions(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ShortPositionsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("short_positions", || quote::short_positions(&mctx, p)).await
    }

    /// Get real-time option call/put volume stats.
    #[tool(
        title = "Option Volume",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get real-time option call/put volume and put/call ratio for a US stock"
    )]
    async fn option_volume(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OptionVolumeParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_volume", || quote::option_volume(&mctx, p)).await
    }

    /// Get daily historical option volume stats.
    #[tool(
        title = "Option Volume (Daily)",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get daily historical option call/put volume, open interest, and put/call ratios for a US stock"
    )]
    async fn option_volume_daily(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OptionVolumeDailyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_volume_daily", || {
            quote::option_volume_daily(&mctx, p)
        })
        .await
    }

    /// List DCA (recurring investment) plans.
    #[tool(
        title = "List DCA Plans",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List DCA recurring investment plans. Filter by status (Active/Suspended/Finished) or symbol."
    )]
    async fn dca_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_list", || dca::dca_list(&mctx, p)).await
    }

    /// Create a DCA (recurring investment) plan.
    #[tool(
        title = "Create DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Create a DCA recurring investment plan. frequency: Daily/Weekly/Monthly. day_of_week (Weekly): Mon/Tue/Wed/Thu/Fri. day_of_month (Monthly): 1-28."
    )]
    async fn dca_create(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaCreateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_create", || dca::dca_create(&mctx, p)).await
    }

    /// Update a DCA plan.
    #[tool(
        title = "Update DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Update an existing DCA recurring investment plan by plan_id"
    )]
    async fn dca_update(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaUpdateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_update", || dca::dca_update(&mctx, p)).await
    }

    /// Pause a DCA plan.
    #[tool(
        title = "Pause DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Pause (suspend) a DCA recurring investment plan by plan_id"
    )]
    async fn dca_pause(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaPlanIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_pause", || dca::dca_pause(&mctx, p)).await
    }

    /// Resume a paused DCA plan.
    #[tool(
        title = "Resume DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Resume a suspended DCA recurring investment plan by plan_id"
    )]
    async fn dca_resume(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaPlanIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_resume", || dca::dca_resume(&mctx, p)).await
    }

    /// Stop a DCA plan permanently.
    #[tool(
        title = "Stop DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Permanently stop a DCA recurring investment plan by plan_id"
    )]
    async fn dca_stop(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaPlanIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_stop", || dca::dca_stop(&mctx, p)).await
    }

    /// Get DCA plan execution history.
    #[tool(
        title = "DCA Execution History",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get execution history records for a DCA plan by plan_id"
    )]
    async fn dca_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaHistoryParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_history", || dca::dca_history(&mctx, p)).await
    }

    /// Get DCA statistics.
    #[tool(
        title = "DCA Statistics",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get DCA investment statistics summary. Optionally filter by symbol."
    )]
    async fn dca_stats(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaStatsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_stats", || dca::dca_stats(&mctx, p)).await
    }

    /// Check if symbols support DCA.
    #[tool(
        title = "Check DCA Support",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Check whether given symbols support DCA recurring investment"
    )]
    async fn dca_check(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaCheckParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_check", || dca::dca_check(&mctx, p)).await
    }

    /// List community sharelists.
    #[tool(
        title = "List Sharelists",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List user's own and subscribed community sharelists"
    )]
    async fn sharelist_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistCountParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_list", || sharelist::sharelist_list(&mctx, p)).await
    }

    /// Get sharelist detail.
    #[tool(
        title = "Sharelist Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get community sharelist detail including constituent stocks and quotes by id"
    )]
    async fn sharelist_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_detail", || sharelist::sharelist_detail(&mctx, p)).await
    }

    /// Create a community sharelist.
    #[tool(
        title = "Create Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Create a new community sharelist"
    )]
    async fn sharelist_create(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistCreateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_create", || sharelist::sharelist_create(&mctx, p)).await
    }

    /// Delete a community sharelist.
    #[tool(
        title = "Delete Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Delete a community sharelist by id"
    )]
    async fn sharelist_delete(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_delete", || sharelist::sharelist_delete(&mctx, p)).await
    }

    /// Add stocks to a sharelist.
    #[tool(
        title = "Add to Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Add securities to a community sharelist"
    )]
    async fn sharelist_add(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistItemsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_add", || sharelist::sharelist_add(&mctx, p)).await
    }

    /// Remove stocks from a sharelist.
    #[tool(
        title = "Remove from Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Remove securities from a community sharelist"
    )]
    async fn sharelist_remove(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistItemsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_remove", || sharelist::sharelist_remove(&mctx, p)).await
    }

    /// Reorder stocks in a sharelist.
    #[tool(
        title = "Sort Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Reorder securities in a community sharelist (provide symbols in desired order)"
    )]
    async fn sharelist_sort(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistItemsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_sort", || sharelist::sharelist_sort(&mctx, p)).await
    }

    /// Get popular community sharelists.
    #[tool(
        title = "Popular Sharelists",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get popular/trending community sharelists"
    )]
    async fn sharelist_popular(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistCountParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_popular", || {
            sharelist::sharelist_popular(&mctx, p)
        })
        .await
    }

    /// Run a quant indicator script against historical K-line data on the server.
    #[tool(
        title = "Quant — Run Indicator Script",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Run a quant indicator script against historical K-line data on the server. Executes the script server-side and returns the computed indicator/plot values as JSON. Periods: 1m, 5m, 15m, 30m, 1h, day, week, month, year (default: day). The optional input parameter accepts a JSON array matching the order of input.*() calls in the script, e.g. \"[14,2.0]\"."
    )]
    async fn quant_run(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<quant::RunScriptParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("quant_run", || quant::run_script(&mctx, p)).await
    }

    /// Search news by keyword.
    #[tool(
        title = "News Search",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Search news articles by keyword. Returns id, title, time, source and URL."
    )]
    async fn news_search(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<search::NewsSearchParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("news_search", || search::news_search(&mctx, p)).await
    }

    /// Search community topics by keyword.
    #[tool(
        title = "Topic Search",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Search community topics/posts by keyword. Returns id, author, time, and excerpt."
    )]
    async fn topic_search(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<search::TopicSearchParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_search", || search::topic_search(&mctx, p)).await
    }

    /// Get financial statements for a security.
    #[tool(
        title = "Financial Statements",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get financial statements (income statement, balance sheet, or cash flow) for a security. kind: IS/BS/CF/ALL. report: af (annual), saf (semi-annual), qf (quarterly full), q1/q2/q3."
    )]
    async fn financial_statement(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialStatementParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_statement", || {
            fundamental::financial_statement(&mctx, p)
        })
        .await
    }

    /// Get latest financial report summary for a security.
    #[tool(
        title = "Latest Financial Report",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get the latest financial report summary for a security including key metrics."
    )]
    async fn financial_report_latest(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_report_latest", || {
            fundamental::financial_report_latest(&mctx, p)
        })
        .await
    }

    /// Get daily valuation rank (PE/PB percentile) for a security.
    #[tool(
        title = "Valuation Rank",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get daily valuation rank (PE/PB/PS/dividend yield industry percentile) for a security over a date range. start/end in yyyymmdd format."
    )]
    async fn valuation_rank(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::ValuationRankParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("valuation_rank", || fundamental::valuation_rank(&mctx, p)).await
    }

    /// Get institution rating history for a security.
    #[tool(
        title = "Institution Rating History",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get institution rating history (target price changes and rating changes) for a security."
    )]
    async fn institution_rating_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institution_rating_history", || {
            fundamental::institution_rating_history(&mctx, p)
        })
        .await
    }

    /// Get institution rating industry rank for a security.
    #[tool(
        title = "Institution Rating Industry Rank",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get peers ranked by institution analyst ratings within the same industry as the given security."
    )]
    async fn institution_rating_industry_rank(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::InstitutionRatingIndustryRankParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institution_rating_industry_rank", || {
            fundamental::institution_rating_industry_rank(&mctx, p)
        })
        .await
    }

    /// Get short margin deposit details for the current account.
    #[tool(
        title = "Short Margin",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get short margin deposit details for the current account."
    )]
    async fn short_margin(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("short_margin", || trade::short_margin(&mctx)).await
    }

    /// List linked withdrawal bank cards.
    #[tool(
        title = "Bank Cards",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List linked withdrawal bank cards for the current account."
    )]
    async fn bank_cards(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("bank_cards", || atm::bank_cards(&mctx)).await
    }

    /// List withdrawal history.
    #[tool(
        title = "Withdrawals",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List withdrawal history for the current account."
    )]
    async fn withdrawals(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<atm::WithdrawalParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("withdrawals", || atm::withdrawals(&mctx, p)).await
    }

    /// List deposit history.
    #[tool(
        title = "Deposits",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List deposit history for the current account. states: comma-separated deposit states. currencies: comma-separated currency codes."
    )]
    async fn deposits(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<atm::DepositParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("deposits", || atm::deposits(&mctx, p)).await
    }

    /// List IPO stocks currently in subscription stage (HK and US).
    #[tool(
        title = "IPO Subscriptions",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List IPO stocks currently in subscription or pre-filing stage (HK and US combined)."
    )]
    async fn ipo_subscriptions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_subscriptions", || ipo::ipo_subscriptions(&mctx)).await
    }

    /// Show the IPO calendar.
    #[tool(
        title = "IPO Calendar",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Show the IPO calendar with all upcoming and recent IPOs including subscription dates and listing dates."
    )]
    async fn ipo_calendar(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_calendar", || ipo::ipo_calendar(&mctx)).await
    }

    /// List recently listed IPO stocks.
    #[tool(
        title = "IPO Listed",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List recently listed IPO stocks (HK and US) with issue price, first-day performance, and trading volume."
    )]
    async fn ipo_listed(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoListedParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_listed", || ipo::ipo_listed(&mctx, p)).await
    }

    /// Show IPO detail for a symbol.
    #[tool(
        title = "IPO Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Show IPO detail including profile (prospectus summary), timeline, and subscription eligibility for a symbol."
    )]
    async fn ipo_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_detail", || ipo::ipo_detail(&mctx, p)).await
    }

    /// List IPO orders (active and history).
    #[tool(
        title = "IPO Orders",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "List IPO orders (active + history) for the current account. Optionally filter by symbol, market, or status."
    )]
    async fn ipo_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_orders", || ipo::ipo_orders(&mctx, p)).await
    }

    /// Show IPO order detail by order ID.
    #[tool(
        title = "IPO Order Detail",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Show detailed information for a specific IPO order by its order ID."
    )]
    async fn ipo_order_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoOrderDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_order_detail", || ipo::ipo_order_detail(&mctx, p)).await
    }

    /// Show IPO profit/loss summary and breakdown.
    #[tool(
        title = "IPO Profit / Loss",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Show IPO profit/loss summary and per-stock breakdown for the current account. period: all (default), ytd, 1y, 3y."
    )]
    async fn ipo_profit_loss(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoProfitLossParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_profit_loss", || ipo::ipo_profit_loss(&mctx, p)).await
    }
}

#[tool_handler(
    name = "longbridge-mcp",
    instructions = "Longbridge OpenAPI MCP Server - provides market data, trading, and financial analysis tools"
)]
impl ServerHandler for Longbridge {}
