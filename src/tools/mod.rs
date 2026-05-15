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
    let structured = serde_json::from_str::<serde_json::Value>(&json)
        .ok()
        .filter(serde_json::Value::is_object);
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
    /// Extra headers to forward to upstream Longbridge services.
    pub extra_headers: Vec<(String, String)>,
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
        let mut client = longbridge::httpclient::HttpClient::new(
            longbridge::httpclient::HttpClientConfig::from_oauth(
                longbridge::oauth::OAuth::from_token(&self.token),
            ),
        );
        // NOTE: This is very important for passing headers to upstream Longbridge services.
        // Do not remove this unless you have a good reason and know exactly which headers to forward instead.
        for (key, value) in &self.extra_headers {
            client = client.header(key.as_str(), value.as_str());
        }
        client
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

/// Headers that must not be forwarded to upstream Longbridge services.
/// These are either hop-by-hop headers or MCP/HTTP-level headers that only
/// make sense for the client↔MCP-server leg, not the MCP-server↔upstream leg.
const SKIP_FORWARD_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
    "proxy-authorization",
    "proxy-authenticate",
    "content-type",
    "accept",
    "accept-encoding",
    "mcp-session-id",
    "authorization",
];

fn collect_headers(headers: &axum::http::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let key = name.as_str().to_lowercase();
            if SKIP_FORWARD_HEADERS.contains(&key.as_str()) {
                return None;
            }
            Some((key, value.to_str().ok()?.to_string()))
        })
        .collect()
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
        // NOTE: This is very important for passing headers to upstream Longbridge services.
        // Do not remove this unless you have a good reason and know exactly which headers to forward instead.
        extra_headers: collect_headers(&parts.headers),
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
        description = "Get current UTC time as an RFC3339 string (e.g. \"2025-01-15T08:30:00Z\"). Use to determine current date/time before making date-based queries."
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
        description = "Get static info for securities. Returns per symbol: symbol, name_cn, name_en, exchange (e.g. NASDAQ), type (e.g. US_Stock), lot_size, listed_date, delisted (bool)."
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
        description = "Get latest price quotes. Returns per symbol: last_done, prev_close, open, high, low, volume, turnover, change_rate, change_value, trade_status, timestamp."
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
        description = "Get option quotes (max 500 symbols). Returns last_done, prev_close, open, high, low, volume, turnover, implied_volatility, delta, gamma, theta, vega, rho, open_interest per symbol."
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
        description = "Get warrant quotes. Returns last_done, prev_close, open, high, low, volume, turnover, implied_volatility, delta, leverage_ratio, effective_leverage per symbol."
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
        description = "Get order book depth for a symbol. Returns {bids[]{position, price, volume, order_num}, asks[]{position, price, volume, order_num}}. Up to 10 price levels."
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
        description = "Get broker queue (HK only). Returns bid_brokers/ask_brokers arrays, each with position (1-based) and broker_ids. Map broker IDs to names via participants."
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
        description = "Get HK market participant broker information. Returns participants[]{broker_ids[], name_en, name_cn, name_hk}. Use broker_ids to interpret broker queue data."
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
        description = "Get recent trades (max 1000). Returns trades[]{price, volume, timestamp, trade_type, direction} for the symbol."
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
        description = "Get trading days for a market between dates. Returns trading_days[] and half_trading_days[] as \"yyyy-mm-dd\" strings. market: HK/US/CN/SG."
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
        description = "Get option chain expiry dates for a symbol (e.g. AAPL.US). Returns expiry_dates[] as \"yyyy-mm-dd\" strings. Use with option_chain_info_by_date to get strikes and Greeks."
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
        description = "Get option chain for an expiry date. Returns strikePrices[]{strike_price, call{symbol, last_done, iv, delta, gamma}, put{symbol, last_done, iv, delta, gamma}}."
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
        description = "Get capital inflow/outflow time series. Returns items[]{timestamp, inflow, outflow, net_flow} for the symbol (same-day data)."
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
        description = "Get capital distribution for a symbol. Returns {timestamp, capital_in{large, medium, small}, capital_out{large, medium, small}} (decimal strings in settlement currency)."
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
        description = "Get trading session schedule for all markets. Returns market_sessions[]{market, trade_sessions[]{beg_time, end_time, trade_session_type}}."
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
        description = "Get current market sentiment temperature. Returns {temperature (0-100), description, valuation (0-100), sentiment (0-100), timestamp}. market: HK/US/CN/SG."
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
        description = "Get historical market temperature time series. Returns {type, list[]{temperature, description, valuation, sentiment, timestamp}} for the given market and date range."
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
        description = "Get all watchlist groups and their securities. Returns groups[]{id, name, securities[]{symbol, market, name, watched_price, watched_at}}."
    )]
    async fn watchlist(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("watchlist", || quote::watchlist(&mctx)).await
    }

    /// Get filings for a symbol.
    #[tool(
        title = "Filings",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get regulatory filings (8-K, 10-Q, 10-K, etc.). Returns items[]{id, title, type, language, filing_date, url} for the symbol."
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
        description = "Get HK warrant issuer information. Returns issuers[]{id, name_en, name_cn}. Use id in warrant_list issuer filter."
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
        description = "Get filtered warrant list for an underlying symbol. Returns warrants[]{symbol, name, last_done, change_rate, implied_volatility, expiry_date, strike_price, leverage_ratio, outstanding_ratio}."
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
        description = "Calculate financial indexes for symbols. Pass symbols and indexes (e.g. [\"PeTtmRatio\",\"PbRatio\",\"LastDone\",\"TurnoverRate\"]). Returns per-symbol index values."
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
        description = "Create a new watchlist group. Returns the created group {id, name}. Optionally pass securities (e.g. [\"AAPL.US\", \"700.HK\"]) to pre-populate."
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
        description = "Delete a watchlist group by id (numeric). Set purge=true to also remove its securities from all other groups. Returns upstream API response."
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
        description = "Update a watchlist group by id. Can rename (name param) or modify securities (securities + mode: add/remove/replace). Returns upstream API response."
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
        description = "Get account cash balance and asset summary. Returns balances[]{currency, total_cash, max_finance_amount, remaining_finance_amount, risk_level, margin_call}. Filter by currency (e.g. \"USD\", \"HKD\")."
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
        description = "Get current stock positions across all channels. Returns list[].stock_info[]{symbol, symbol_name, quantity, available_quantity, currency, cost_price, market}."
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
        description = "Get current fund positions. Returns list[].fund_info[]{symbol, symbol_name, currency, holding_units, current_net_asset_value, cost_net_asset_value, net_asset_value_day}."
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
        description = "Get margin ratio for a symbol. Returns {im_factor (initial margin), mm_factor (maintenance margin), fm_factor (forced liquidation)} as decimal strings."
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
        description = "Get orders placed today. Returns orders[]{order_id, symbol, side, order_type, status, quantity, price, submitted_at, executed_quantity, executed_price}. Pass symbol to filter."
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
        description = "Get detailed information about a specific order. Returns {order_id, symbol, status, side, order_type, quantity, price, executed_quantity, executed_price, submitted_at, time_in_force, msg}."
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
        description = "Cancel an open order by order_id. Returns plain text \"order cancelled\" on success; errors if the order is already filled or cancelled."
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
        description = "Get today's trade executions (fills). Returns executions[]{order_id, symbol, side, quantity, price, trade_done_at}. Pass symbol or order_id to filter."
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
        description = "Get historical orders between dates (excludes today). Returns orders[]{order_id, symbol, side, status, quantity, price, submitted_at}. start_at/end_at in RFC3339."
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
        description = "Get historical trade executions between dates. Returns executions[]{order_id, symbol, side, quantity, price, trade_done_at}. start_at/end_at in RFC3339."
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
        description = "Get cash flow records (deposits, withdrawals, dividends). Returns items[]{transaction_type, amount, currency, balance, created_at, remark}. start_at/end_at in RFC3339."
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
        description = "Modify an open order's quantity, price, trigger_price, or trailing params. Returns \"order replaced\" on success. Only open/pending orders can be modified."
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
        description = "Estimate maximum buy/sell quantity for a symbol. Returns {cash_max_qty, margin_max_qty} (decimal strings). Requires symbol, side (Buy/Sell), order_type, and optionally price."
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
        description = "Get financial reports (income statement, balance sheet, cash flow). kind: IS/BS/CF/ALL. report_type: af (annual), saf (semi-annual), q1/q2/q3, qf (quarterly full)."
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
        description = "Get institution rating summary. Returns analyst{buy, outperform, hold, underperform, sell counts, target_price, consensus_rating} and instratings list."
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
        description = "Get detailed historical institution ratings and target price history. Returns target.list[]{analyst, firm, rating, target_price, timestamp} per institution."
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
        description = "Get dividend history. Returns items[]{ex_date, pay_date, record_date, dividend_type, amount, currency, status} for the symbol."
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
        description = "Get detailed dividend distribution scheme. Returns details[]{period, cash_dividend, stock_dividend, record_date, ex_date, pay_date, currency}."
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
        description = "Get EPS forecast and analyst estimate history. Returns items[]{forecast_start_date, forecast_end_date, eps_estimate, eps_actual, surprise_pct, analyst_count}."
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
        description = "Get financial consensus estimates. Returns items[]{period, revenue_estimate, eps_estimate, net_income_estimate, analyst_count, last_updated} for upcoming periods."
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
        description = "Get valuation overview with peer comparison. Returns metrics.pe/pb/ps/dividend_yield{current, industry_avg, 5yr_avg, percentile} and peer comparison list."
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
        description = "Get detailed valuation history time series. Returns history.metrics{pe/pb/ps/dividend_yield}[]{timestamp, value} for long-term percentile analysis."
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
        description = "Get industry valuation comparison for peers. Returns list[]{symbol, name, pe, pb, ps, dividend_yield, history[]{date, pe, pb}} for peers in the same industry."
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
        description = "Get industry PE/PB/PS valuation distribution. Returns distributions{pe/pb/ps}{min, p25, median, p75, max, current_percentile} to see where the stock sits in its sector."
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
        description = "Get company overview. Returns name, description, employees, CEO, founded_year, website, exchange, industry, market_cap, and business profile summary."
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
        description = "Get company executive and board member information. Returns members[]{name, title, appointed_date, age, biography, compensation}."
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
        description = "Get institutional shareholders for a symbol. Returns shareholders[]{institution, shares, ratio, change, change_type, reported_at}."
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
        description = "Get funds and ETFs that hold a given symbol. Returns fund_holders[]{fund_name, fund_symbol, shares, ratio, change, reported_at}."
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
        description = "Get corporate actions (splits, buybacks, name changes). Returns items[]{action_type, effective_date, ratio, description} for the symbol."
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
        description = "Get investor relations events and announcements. Returns items[]{title, event_type, event_date, url, description} for the symbol."
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
        description = "Get company operating metrics (HK stocks only). Returns items[]{period, metric_name, value, unit} such as passenger traffic, cargo volumes, or store counts."
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
        description = "Get current market trading status for all markets. Returns market_time[]{market, trade_status (Pre-Open/Trading/Lunch Break/Post-Trading/Closed/Pre-Market/Post-Market), timestamp}."
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
        description = "Get top broker holding data for a symbol. Returns items[]{broker_name, holding_quantity, holding_change, holding_ratio} for the given period (rct_1/rct_5/rct_20/rct_60)."
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
        description = "Get full broker holding detail list for a symbol. Returns items[]{broker_id, broker_name, holding_quantity, holding_ratio, holding_change, date}."
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
        description = "Get daily holding history for a specific broker (by broker_id) in a symbol. Returns items[]{date, holding_quantity, holding_change, holding_ratio}."
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
        description = "Get A/H share premium historical K-line data. Returns items[]{timestamp, open, high, low, close} representing the premium percentage over the given period."
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
        description = "Get A/H share premium intraday time-share data. Returns items[]{timestamp, premium_rate} showing the intraday A/H premium percentage minute by minute."
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
        description = "Get trade statistics (buy/sell/neutral volume distribution). Returns items[]{price_range, buy_volume, sell_volume, neutral_volume} for price-volume profile."
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
        description = "Get market anomaly alerts (unusual price/volume changes). Returns items[]{symbol, name, anomaly_type, change_rate, volume, timestamp} for the given market."
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
        description = "Get constituent stocks of an index (e.g. HSI.HK, .DJI.US). Returns constituents[]{symbol, name, last_done, change_rate, market_cap, weight}."
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
        description = "Get exchange rates for all supported currencies. Returns list[]{from_currency, to_currency, rate, timestamp} covering USD, HKD, CNY, SGD and others."
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
        description = "Get all configured price alerts. Returns lists[]{counter_id, indicators[]{id, indicator_id, condition, price, frequency, enabled, triggered_at}}."
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
        description = "Add a price alert. condition: price_rise/price_fall (absolute price) or percent_rise/percent_fall (relative %). frequency: once/daily/every. Returns created alert object."
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
        description = "Delete a price alert by alert_id (numeric string from alert_list). Returns upstream API response on success; errors if alert_id is invalid."
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
        description = "Enable a price alert by alert_id. Returns {alert_id, enabled: true} on success. Use alert_list to find the numeric alert_id."
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
        description = "Disable a price alert by alert_id. Returns {alert_id, enabled: false} on success. Use alert_list to find the numeric alert_id."
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
        description = "Get latest news articles for a symbol. Returns items[]{id, title, source, publish_time, summary, url, related_symbols[]}."
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
        description = "Get discussion topics for a symbol. Returns items[]{id, title, author, created_at, like_count, comment_count, content_summary}."
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
        description = "Get discussion topic detail by topic_id. Returns {id, title, content, author, created_at, like_count, comment_count, symbols[]}."
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
        description = "List available account statements (daily/monthly). Returns list[]{id, type (daily/monthly), date, status}. Use the id with statement_export to download."
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
        description = "Get real-time option call/put volume stats for a US stock. Returns {call_volume, put_volume, put_call_ratio, call_oi, put_oi} and top active contracts."
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
        description = "Get daily historical option stats for a US stock. Returns items[]{date, call_volume, put_volume, put_call_vol_ratio, call_oi, put_oi, put_call_oi_ratio}."
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
        description = "List DCA recurring investment plans. Returns plans[]{plan_id, symbol, amount, currency, frequency, status, next_execution_date}. Filter by status (Active/Suspended/Finished) or symbol."
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
        description = "Update an existing DCA plan by plan_id. Can change amount, frequency (Daily/Weekly/Monthly), day_of_week (Mon-Fri), or day_of_month (1-28). Returns updated plan."
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
        description = "Pause (suspend) a DCA plan by plan_id. The plan stops executing until resumed. Returns upstream API response. Use dca_resume to restart."
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
        description = "Resume a suspended DCA plan by plan_id. Resumes automated execution on the configured schedule. Returns upstream API response."
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
        description = "Permanently stop a DCA plan by plan_id. This cannot be undone. To temporarily pause, use dca_pause instead. Returns upstream API response."
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
        description = "Get execution history records for a DCA plan by plan_id. Returns executions[]{date, quantity, amount, price, status, order_id}."
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
        description = "Get DCA investment statistics. Returns {total_invested, total_value, total_return, return_rate, plan_count, items[]{symbol, invested, value, return_rate}}."
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
        description = "Check whether given symbols support DCA recurring investment. Returns items[]{symbol, support_dca (bool), reason} for each queried symbol."
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
        description = "List user's own and subscribed community sharelists. Returns lists[]{id, name, description, symbol_count, is_owner, follower_count}."
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
        description = "Get community sharelist detail by id. Returns {id, name, description, constituents[]{symbol, name, last_done, change_rate}, quote data, subscription status}."
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
        description = "Create a new community sharelist with a name and optional description. Returns the created sharelist object including its id, name, and description."
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
        description = "Delete a community sharelist by id (own lists only; subscribed lists cannot be deleted). Returns upstream API response on success."
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
        description = "Add securities to a community sharelist by id. Provide symbols (e.g. [\"AAPL.US\", \"700.HK\"]) to add. Returns upstream API response."
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
        description = "Remove securities from a community sharelist by id. Provide symbols to remove. Returns upstream API response on success."
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
        description = "Reorder securities in a community sharelist by id. Provide symbols in the desired new order. Returns upstream API response on success."
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
        description = "Get popular/trending community sharelists. Returns lists[]{id, name, description, symbol_count, follower_count, creator} sorted by popularity."
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
        description = "Get the latest financial report summary for a security. Returns {period, revenue, net_income, eps, roe, gross_margin, report_date} and key financial highlights."
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
        description = "Get institution rating history. Returns target_history[]{firm, analyst, old_target, new_target, date} and evaluate_history[]{firm, old_rating, new_rating, date}."
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
        description = "Get peers ranked by institution analyst ratings in the same industry. Returns list[]{symbol, name, buy_count, sell_count, consensus_rating, target_price}. Paginated."
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
        description = "Get short margin deposit details for the current account. Returns short positions with margin_amount, margin_rate, interest_rate, symbol, quantity per position."
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
        description = "List linked withdrawal bank cards for the current account. Returns cards[]{id, bank_name, account_number (masked), currency, status}."
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
        description = "List withdrawal history for the current account. Returns items[]{id, amount, currency, status, created_at, bank_name, account_number (masked)}."
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
        description = "List deposit history for the current account. Returns items[]{id, amount, currency, status, created_at, updated_at}. states: comma-separated (Pending/Finished/Failed). currencies: comma-separated codes."
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
        description = "List IPO stocks in subscription/pre-filing stage (HK+US). Returns items[]{symbol, name, market, sub_start_date, sub_end_date, listing_date, issue_price, min_lot_size}."
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
        description = "Show the IPO calendar. Returns items[]{symbol, name, market, sub_start_date, sub_end_date, listing_date, status} for upcoming and recent IPOs."
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
        description = "List recently listed IPO stocks (HK+US). Returns items[]{symbol, name, listing_date, issue_price, first_day_close, first_day_return, volume, market}."
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
        description = "Show IPO detail for a symbol. Returns profile (business overview), timeline[]{event, date}, subscription eligibility, pricing_range, lot_size, allotment_rules."
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
        description = "List IPO orders (active+history). Returns orders[]{order_id, symbol, market, quantity, total_amount, status, submitted_at}. Filter by symbol, market, or status."
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
        description = "Show detailed information for a specific IPO order by order_id. Returns {order_id, symbol, market, quantity, allotted_quantity, total_amount, status, submitted_at}."
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
        description = "Show IPO profit/loss summary and per-stock breakdown. Returns {total_cost, total_value, total_return, items[]{symbol, cost, current_value, return_rate}}. period: all/ytd/1y/3y."
    )]
    async fn ipo_profit_loss(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoProfitLossParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_profit_loss", || ipo::ipo_profit_loss(&mctx, p)).await
    }

    /// Get current-period business segment revenue breakdown.
    #[tool(
        title = "Business Segments",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get current-period business segment revenue breakdown for a symbol (name, percent, total, currency)"
    )]
    async fn business_segments(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::BusinessSegmentsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("business_segments", || {
            fundamental::business_segments(&mctx, p)
        })
        .await
    }

    /// Get historical business segment revenue trends.
    #[tool(
        title = "Business Segments History",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get historical business segment revenue trends (by period and category). Returns historical[].{date, total, currency, business[{name,percent,value}], regionals[{name,percent,value}]}"
    )]
    async fn business_segments_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::BusinessSegmentsHistoryParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("business_segments_history", || {
            fundamental::business_segments_history(&mctx, p)
        })
        .await
    }

    /// Get monthly institutional rating distribution timeline.
    #[tool(
        title = "Institutional Views",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get monthly institutional rating distribution timeline. Returns months[]{date, buy, outperform, hold, underperform, sell, total} for trend analysis."
    )]
    async fn institutional_views(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institutional_views", || {
            fundamental::institutional_views(&mctx, p)
        })
        .await
    }

    /// Get industry ranking list by market and indicator.
    #[tool(
        title = "Industry Rank",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Industry ranking list by market (US/HK/CN/SG) and indicator (0=领涨/1=今日走势/2=人气/3=市值/4=营收/5=营收增长率/6=净利润/7=净利润增长率). sort_type: 0=单级 1=多层. Returns items[]{counter_id(BK/US/IN00258), name, chg, lists[]}. Pass counter_id directly to industry_peers."
    )]
    async fn industry_rank(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::IndustryRankParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("industry_rank", || market::industry_rank(&mctx, p)).await
    }

    /// Get hierarchical industry peer group tree for an industry index symbol.
    #[tool(
        title = "Industry Peers",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Hierarchical sub-sector tree for an industry group. Accepts BK counter_id from industry_rank (e.g. BK/US/IN00258). Returns chain{name,counter_id,stock_num,chg,ytd_chg,next[{...}]} and top{name,market}. Each node shows stock count, daily change, and YTD change."
    )]
    async fn industry_peers(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::IndustryPeersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("industry_peers", || fundamental::industry_peers(&mctx, p)).await
    }

    /// Get financial report snapshot with actual vs forecast comparison.
    #[tool(
        title = "Financial Report Snapshot",
        annotations(read_only_hint = true, idempotent_hint = true, open_world_hint = true),
        description = "Get financial report snapshot: report_desc (text summary), fo_revenue/fo_ebit/fo_eps (actual vs forecast with yoy/cmp), fr_* financial ratios (ROE, margins, assets, cash flow). report: qf/saf/af."
    )]
    async fn financial_report_snapshot(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialReportSnapshotParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_report_snapshot", || {
            fundamental::financial_report_snapshot(&mctx, p)
        })
        .await
    }
}

#[tool_handler(
    name = "longbridge-mcp",
    instructions = "Longbridge OpenAPI MCP Server - provides market data, trading, and financial analysis tools"
)]
impl ServerHandler for Longbridge {}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    use super::collect_headers;

    #[test]
    fn collects_all_valid_headers() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("x-custom"),
            HeaderValue::from_static("hello"),
        );
        map.insert(
            HeaderName::from_static("accept-language"),
            HeaderValue::from_static("zh-CN"),
        );
        let headers = collect_headers(&map);
        assert!(headers.iter().any(|(k, v)| k == "x-custom" && v == "hello"));
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "accept-language" && v == "zh-CN")
        );
    }

    #[test]
    fn skips_non_utf8_values() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("x-valid"),
            HeaderValue::from_static("ok"),
        );
        map.insert(
            HeaderName::from_static("x-binary"),
            HeaderValue::from_bytes(&[0x80, 0x81]).unwrap(),
        );
        let headers = collect_headers(&map);
        assert!(headers.iter().any(|(k, v)| k == "x-valid" && v == "ok"));
        assert!(!headers.iter().any(|(k, _)| k == "x-binary"));
    }

    #[test]
    fn empty_map_returns_empty() {
        assert!(collect_headers(&HeaderMap::new()).is_empty());
    }
}
