use reqwest::Method;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::{index_symbol_to_counter_id, symbol_to_counter_id};
use crate::error::Error;
use crate::serialize::convert_unix_paths;
use crate::tools::support::http_client::{http_get_tool, http_get_tool_unix};
use crate::tools::tool_json;

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
pub struct BrokerHoldingParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
    /// Period: "rct_1" (1 day, default), "rct_5" (5 days), "rct_20" (20 days), "rct_60" (60 days)
    pub period: Option<String>,
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

fn trade_status_label(code: i64) -> &'static str {
    match code {
        101 => "Pre-Open",
        102 | 103 | 105 | 202 | 203 => "Trading",
        104 => "Lunch Break",
        106 => "Post-Trading",
        108 => "Closed",
        201 => "Pre-Market",
        204 => "Post-Market",
        _ => "Unknown",
    }
}

pub async fn market_status(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let raw: String = client
        .request(Method::GET, "/v1/quote/market-status")
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::Other(e.to_string()))?;

    let mut data: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| Error::Other(e.to_string()))?;

    if let Some(list) = data.get_mut("market_time").and_then(|v| v.as_array_mut()) {
        for item in list.iter_mut() {
            let code = item["trade_status"].as_i64().unwrap_or(0);
            item["trade_status"] = serde_json::json!(trade_status_label(code));
            let delay_code = item["delay_trade_status"].as_i64().unwrap_or(0);
            item["delay_trade_status"] = serde_json::json!(trade_status_label(delay_code));
        }
    }

    convert_unix_paths(
        &mut data,
        &["market_time.*.timestamp", "market_time.*.delay_timestamp"],
    );

    tool_json(&data)
}

pub async fn broker_holding(
    mctx: &crate::tools::McpContext,
    p: BrokerHoldingParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let period = p.period.as_deref().unwrap_or("rct_1");
    http_get_tool(
        &client,
        "/v1/quote/broker-holding",
        &[("counter_id", cid.as_str()), ("type", period)],
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
    http_get_tool_unix(
        &client,
        "/v1/quote/ahpremium/klines",
        &[
            ("counter_id", cid.as_str()),
            ("line_type", line_type),
            ("line_num", count_str.as_str()),
        ],
        &["klines.*.timestamp"],
    )
    .await
}

pub async fn ah_premium_intraday(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/ahpremium/timeshares",
        &[("counter_id", cid.as_str()), ("days", "1")],
        &["klines.*.timestamp"],
    )
    .await
}

pub async fn trade_stats(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/trades-statistics",
        &[("counter_id", cid.as_str())],
        &["statistics.timestamp", "statistics.trade_date.*"],
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IndustryRankParam {
    /// Market: "US" | "HK" | "SG" | "CN"
    pub market: String,
    /// Ranking indicator (default: "0"):
    ///   "0" = 领涨行业, "1" = 今日走势, "2" = 行业人气, "3" = 市值,
    ///   "4" = 营收, "5" = 营收增长率, "6" = 净利润, "7" = 净利润增长率
    pub indicator: Option<String>,
    /// Number of results to return (default: returns all)
    pub limit: Option<String>,
    /// Sort type: "0" = 单级 (default) | "1" = 多层
    pub sort_type: Option<String>,
}

pub async fn industry_rank(
    mctx: &crate::tools::McpContext,
    p: IndustryRankParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let indicator = p.indicator.unwrap_or_else(|| "0".to_string());
    let sort_type = p.sort_type.unwrap_or_else(|| "0".to_string());
    let mut params: Vec<(&str, &str)> = vec![
        ("market", p.market.as_str()),
        ("indicator", indicator.as_str()),
        ("sort_type", sort_type.as_str()),
    ];
    let limit = p.limit.unwrap_or_default();
    if !limit.is_empty() {
        params.push(("limit", limit.as_str()));
    }
    // Use the raw HTTP response to preserve BK counter_ids as-is.
    // http_get_tool applies transform_json which renames counter_id → symbol,
    // losing the BK format needed by industry_peers.
    use reqwest::Method;
    let raw: String = client
        .request(Method::GET, "/v1/quote/industry/rank")
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| crate::error::Error::Other(e.to_string()))?;
    let data: serde_json::Value =
        serde_json::from_str(&raw).map_err(crate::error::Error::Serialize)?;
    let out = serde_json::to_string(&data).map_err(crate::error::Error::Serialize)?;
    let structured = serde_json::from_str::<serde_json::Value>(&out).ok();
    let mut res = rmcp::model::CallToolResult::success(vec![rmcp::model::Content::text(out)]);
    res.structured_content = structured;
    Ok(res)
}
