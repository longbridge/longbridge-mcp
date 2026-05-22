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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShortTradesParam {
    /// Security symbol, e.g. "AAPL.US" (US) or "700.HK" (HK). Market is inferred from suffix.
    pub symbol: String,
    /// Query cutoff timestamp in seconds (pass current timestamp for latest data)
    pub last_timestamp: String,
    /// Page size: 1–100 (default: 20)
    pub page_size: Option<String>,
}

pub async fn short_trades(
    mctx: &crate::tools::McpContext,
    p: ShortTradesParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let page_size = p.page_size.unwrap_or_else(|| "20".to_string());
    let is_hk = p.symbol.to_uppercase().ends_with(".HK");
    let path = if is_hk {
        "/v1/quote/short-trades/hk"
    } else {
        "/v1/quote/short-trades/us"
    };
    let result = http_get_tool_unix(
        &client,
        path,
        &[
            ("counter_id", cid.as_str()),
            ("last_timestamp", p.last_timestamp.as_str()),
            ("page_size", page_size.as_str()),
        ],
        &["data.*.timestamp"],
    )
    .await?;
    Ok(normalize_short_trades(result, is_hk))
}

/// Normalize short_trades response to a unified schema regardless of market.
///
/// Unified data[] item fields:
///   timestamp     RFC3339
///   short_vol     daily short-sale volume (US: total_amount across all venues; HK: amount)
///   rate          decimal ratio (e.g. 0.36 = 36% of total volume was short)
///   close         close price
///   nasdaq_vol    US only — NASDAQ short volume (nus_amount)
///   nyse_vol      US only — NYSE short volume (ny_amount)
///   balance       HK only — outstanding short balance (HKD)
///   market_vol    HK only — total market trading volume for the day (total_amount)
fn normalize_short_trades(
    result: rmcp::model::CallToolResult,
    is_hk: bool,
) -> rmcp::model::CallToolResult {
    let Some(text) = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
    else {
        return result;
    };
    let Ok(mut d) = serde_json::from_str::<serde_json::Value>(text) else {
        return result;
    };

    if let Some(items) = d.get_mut("data").and_then(|v| v.as_array_mut()) {
        for item in items.iter_mut() {
            let Some(obj) = item.as_object_mut() else {
                continue;
            };
            if is_hk {
                // amount → short_vol
                if let Some(v) = obj.remove("amount") {
                    obj.insert("short_vol".to_string(), v);
                }
                // total_amount → market_vol (HK: this is total market volume, not short volume)
                if let Some(v) = obj.remove("total_amount") {
                    obj.insert("market_vol".to_string(), v);
                }
            } else {
                // total_amount → short_vol (US: total short volume across all venues)
                if let Some(v) = obj.remove("total_amount") {
                    obj.insert("short_vol".to_string(), v);
                }
                // nus_amount → nasdaq_vol
                if let Some(v) = obj.remove("nus_amount") {
                    obj.insert("nasdaq_vol".to_string(), v);
                }
                // ny_amount → nyse_vol
                if let Some(v) = obj.remove("ny_amount") {
                    obj.insert("nyse_vol".to_string(), v);
                }
            }
        }
    }

    let Ok(json) = serde_json::to_string(&d) else {
        return result;
    };
    rmcp::model::CallToolResult::success(vec![rmcp::model::Content::text(json)])
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StockEventsParam {
    /// Market filter: comma-separated list of markets to include.
    /// Supported values: "HK", "US", "CN", "SG". Omit to return all markets.
    /// Example: "HK,US"
    pub markets: Option<String>,
    /// Sort order (default: "2"):
    ///   "0" = by time (most recent first)
    ///   "1" = by price change magnitude (largest move first)
    ///   "2" = by popularity (most-viewed first)
    pub sort: Option<String>,
    /// Date to query in "YYYY-MM-DD" format. Omit for today's movers.
    pub date: Option<String>,
    /// Number of events to return per page (default: 20, max: 100)
    pub limit: Option<u32>,
    /// Pagination cursor from previous response next_params field.
    /// Pass the entire next_params object returned by the previous call to get the next page.
    /// Omit for the first page.
    pub next_params: Option<serde_json::Value>,
}

pub async fn top_movers(
    mctx: &crate::tools::McpContext,
    p: StockEventsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let limit = p.limit.unwrap_or(20);
    let sort: u32 = p.sort.as_deref().unwrap_or("2").parse().unwrap_or(2);
    let markets: Vec<serde_json::Value> = p
        .markets
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::Value::String(s.trim().to_uppercase()))
        .collect();
    let mut body = serde_json::json!({
        "limit": limit,
        "sort": sort,
        "markets": markets,
        "next_params": p.next_params.unwrap_or_else(|| serde_json::json!({})),
    });
    if let Some(ref d) = p.date {
        body["date"] = serde_json::Value::String(d.clone());
    }
    crate::tools::support::http_client::http_post_tool_unix(
        &client,
        "/v1/quote/market/stock-events",
        body,
        &["events.*.timestamp"],
    )
    .await
}

/// Get available rank tab category configurations.
pub async fn rank_categories(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/quote/market/rank/categories", &[]).await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RankListParam {
    /// Tab key from rank_categories second_tags[].key, e.g. "ib_hot_all-us" (US total heat),
    /// "ib_hot_up-hk" (HK rising heat), "ib_hot_trade-us" (US hot trades).
    pub key: String,
    /// Market override: "US" | "HK" | "CN" | "SG".
    /// Defaults to the market suffix in the key (e.g. "ib_hot_all-hk" → HK), then "US".
    pub market: Option<String>,
    /// Number of results to return (default: 20)
    pub size: Option<u32>,
    /// Whether to include related news articles (default: false)
    pub need_article: Option<bool>,
}

pub async fn rank_list(
    mctx: &crate::tools::McpContext,
    p: RankListParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let need_article = p.need_article.unwrap_or(false).to_string();
    // Auto-prepend "ib_" if the key doesn't already start with it.
    let key = if p.key.starts_with("ib_") {
        p.key.clone()
    } else {
        format!("ib_{}", p.key)
    };
    // Infer market from key suffix (e.g. "ib_hot_all-hk" → "HK"), fall back to param or "US".
    let key_market = key
        .rsplit_once('-')
        .map(|(_, m)| m.to_uppercase())
        .filter(|m| matches!(m.as_str(), "US" | "HK" | "CN" | "SG"))
        .or_else(|| p.market.as_deref().map(|m| m.to_uppercase()))
        .unwrap_or_else(|| "US".to_string());
    let size = p.size.unwrap_or(20).to_string();
    http_get_tool(
        &client,
        "/v1/quote/market/rank/list",
        &[
            ("key", key.as_str()),
            ("delay_bmp", "false"),
            ("need_article", need_article.as_str()),
            ("market", key_market.as_str()),
            ("size", size.as_str()),
        ],
    )
    .await
}
