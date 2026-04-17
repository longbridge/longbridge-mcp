use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::{index_symbol_to_counter_id, symbol_to_counter_id};
use crate::tools::http_client::http_get_tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarketParam {
    /// Market code: HK, US, CN, SG
    pub market: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BrokerHoldingDailyParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
    /// Broker participant number
    pub broker_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AhPremiumParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
    /// K-line period: "1m", "5m", "15m", "30m", "60m", "day" (default), "week", "month", "year"
    pub period: Option<String>,
    /// Number of K-lines to return (default: 100)
    pub count: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IndexSymbolParam {
    /// Index symbol, e.g. "HSI.HK"
    pub symbol: String,
}

pub async fn market_status(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/quote/market-status", &[]).await
}

pub async fn broker_holding(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/broker-holding",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn broker_holding_detail(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/broker-holding/detail",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn broker_holding_daily(
    mctx: &crate::tools::McpContext,
    p: BrokerHoldingDailyParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/broker-holding/daily",
        &[
            ("counter_id", cid.as_str()),
            ("parti_number", p.broker_id.as_str()),
        ],
    )
    .await
}

pub async fn ah_premium(
    mctx: &crate::tools::McpContext,
    p: AhPremiumParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let line_type = match p.period.as_deref().unwrap_or("day") {
        "1m" => "1",
        "5m" => "5",
        "15m" => "15",
        "30m" => "30",
        "60m" => "60",
        "week" => "2000",
        "month" => "3000",
        "year" => "4000",
        _ => "1000", // day
    };
    let count_str = p.count.unwrap_or(100).to_string();
    http_get_tool(
        &client,
        "/v1/quote/ahpremium/klines",
        &[
            ("counter_id", cid.as_str()),
            ("line_type", line_type),
            ("line_num", count_str.as_str()),
        ],
    )
    .await
}

pub async fn ah_premium_intraday(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/ahpremium/timeshares",
        &[("counter_id", cid.as_str()), ("days", "1")],
    )
    .await
}

pub async fn trade_stats(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/trades-statistics",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn anomaly(
    mctx: &crate::tools::McpContext,
    p: MarketParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market_upper = p.market.to_uppercase();
    http_get_tool(
        &client,
        "/v1/quote/changes",
        &[("market", market_upper.as_str()), ("category", "0")],
    )
    .await
}

pub async fn constituent(
    mctx: &crate::tools::McpContext,
    p: IndexSymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = index_symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/index-constituents",
        &[("counter_id", cid.as_str())],
    )
    .await
}
