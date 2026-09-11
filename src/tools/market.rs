use longbridge::FundamentalContext;
use longbridge::fundamental::types::AssetAllocationResponse;
use reqwest::Method;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::{Deserialize, Serialize};

use crate::counter::{index_symbol_to_counter_id, is_etf, symbol_to_counter_id};
use crate::error::Error;
use crate::serialize::{convert_unix_paths, transform_json};
use crate::tools::support::http_client::{
    http_get_tool, http_get_tool_dropping, http_get_tool_trimming_zeros, http_get_tool_unix,
    http_get_tool_unix_dropping,
};
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnomalyParam {
    /// Market code: HK, US, CN, SG
    pub market: String,
    /// Filter to a specific symbol, e.g. "700.HK" or "AAPL.US"
    pub symbol: Option<String>,
    /// Number of results to return (default: 50, max: 100)
    pub count: Option<u32>,
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
    i32::try_from(code)
        .map(longbridge::market::TradeStatus::from)
        .unwrap_or_default()
        .name()
}

pub async fn market_status(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let raw: String = client
        .request(Method::GET, "/v1/quote/market-status")
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;

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
    // Each entry's `chg` is an integer share delta padded with a fake ".0000"
    // fractional part; strip the trailing zeros (lossless).
    http_get_tool_trimming_zeros(
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
    // The share-count delta fields (shares.chg_*) are integers padded with a
    // fake ".0000" fractional part across all rows; strip the trailing zeros
    // (lossless) — they are ~half the bytes of this ~100 KB payload.
    http_get_tool_trimming_zeros(
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
    http_get_tool_unix_dropping(
        &client,
        "/v1/quote/ahpremium/klines",
        &[
            ("counter_id", cid.as_str()),
            ("line_type", line_type),
            ("line_num", count_str.as_str()),
        ],
        &["klines.*.timestamp"],
        &["price_spread"],
    )
    .await
}

/// Hoist the per-bar prior-close fields (`apreclose`, `hpreclose`) out of the
/// `klines` array to the top level.
///
/// These are the A-share and H-share *previous* closing prices: fixed for the
/// whole session by definition, so upstream repeats the identical value on every
/// one of the ~240 minute bars. Lifting them to a single top-level copy is
/// lossless and keeps a stable shape (always hoisted). The FX `currency_rate` is
/// deliberately left per-bar — unlike a prior close it can move intraday.
fn hoist_intraday_prev_closes(value: &mut serde_json::Value) {
    let first = value
        .get("klines")
        .and_then(serde_json::Value::as_array)
        .and_then(|k| k.first());
    let Some(first) = first else {
        return;
    };
    let apreclose = first.get("apreclose").cloned();
    let hpreclose = first.get("hpreclose").cloned();
    if apreclose.is_none() && hpreclose.is_none() {
        return;
    }
    if let Some(bars) = value
        .get_mut("klines")
        .and_then(serde_json::Value::as_array_mut)
    {
        for bar in bars.iter_mut() {
            if let Some(obj) = bar.as_object_mut() {
                obj.remove("apreclose");
                obj.remove("hpreclose");
            }
        }
    }
    if let Some(obj) = value.as_object_mut() {
        if let Some(v) = apreclose {
            obj.insert("apreclose".to_owned(), v);
        }
        if let Some(v) = hpreclose {
            obj.insert("hpreclose".to_owned(), v);
        }
    }
}

pub async fn ah_premium_intraday(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let resp: String = client
        .request(Method::GET, "/v1/quote/ahpremium/timeshares")
        .query_params(vec![("counter_id", cid.as_str()), ("days", "1")])
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let transformed = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    convert_unix_paths(&mut value, &["klines.*.timestamp"]);
    // `price_spread` is empty on every intraday minute bar.
    crate::serialize::drop_keys(&mut value, &["price_spread"]);
    hoist_intraday_prev_closes(&mut value);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(crate::tools::tool_result(json))
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
    p: AnomalyParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market_upper = p.market.to_uppercase();
    let count = p.count.unwrap_or(50).min(100).to_string();
    let mut params: Vec<(&str, &str)> = vec![
        ("category", "0"),
        ("size", count.as_str()),
        ("market", market_upper.as_str()),
    ];
    let cid;
    if let Some(ref sym) = p.symbol {
        cid = symbol_to_counter_id(sym);
        params.push(("counter_id", cid.as_str()));
    }
    http_get_tool(&client, "/v1/quote/changes", &params).await
}

pub async fn constituent(
    mctx: &crate::tools::McpContext,
    p: IndexSymbolParam,
) -> Result<CallToolResult, McpError> {
    // When the symbol resolves to an ETF counter (e.g. `ETF/US/QQQ`), return the
    // ETF's asset allocation instead of index constituents. Indexes keep the
    // original index-constituents behaviour. When the symbol is an ETF but the
    // upstream reports no allocation groups (some ETFs are not covered), fall
    // through to the index-constituents source below.
    if is_etf(&p.symbol)
        && let Some(result) = etf_asset_allocation(mctx, &p.symbol).await?
    {
        return tool_json(&result);
    }

    let client = mctx.create_http_client();
    let cid = index_symbol_to_counter_id(&p.symbol);
    // Per-constituent noise: `intro` (long blurb), `market` (derivable from
    // symbol), constant `delay`/`trade_status`.
    http_get_tool_dropping(
        &client,
        "/v1/quote/index-constituents",
        &[("counter_id", cid.as_str())],
        &["intro", "market", "delay", "trade_status"],
    )
    .await
}

/// Fetch an ETF's asset allocation via the SDK `FundamentalContext`.
///
/// Returns `Ok(None)` when the upstream reports no allocation groups, so the
/// caller can fall back to another data source.
async fn etf_asset_allocation(
    mctx: &crate::tools::McpContext,
    symbol: &str,
) -> Result<Option<AssetAllocationResponse>, McpError> {
    let ctx = FundamentalContext::new(mctx.create_config());
    let result = ctx
        .etf_asset_allocation(symbol)
        .await
        .map_err(Error::longbridge)?;
    if result.info.is_empty() {
        Ok(None)
    } else {
        Ok(Some(result))
    }
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
        .map_err(|e| Error::longbridge(e.into()))?;
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
    crate::tools::tool_result(json)
}

/// Pagination cursor returned by `top_movers`; pass it back verbatim to fetch the next page.
#[derive(Debug, Deserialize, Serialize)]
pub struct TopMoversNextParams {
    /// Event IDs already seen in previous pages.
    pub visited: Vec<String>,
}

/// Inlined JSON Schema for `next_params`.
///
/// Emitted by hand (not derived from [`TopMoversNextParams`]) so the manifest
/// stays OpenAI-function-calling compatible: a concrete object with enumerated
/// fields — no `$ref`/`$defs`, and no `anyOf`/nullable union that OpenAI's
/// strict mode collapses.
fn next_params_schema(_: &mut rmcp::schemars::SchemaGenerator) -> rmcp::schemars::Schema {
    rmcp::schemars::json_schema!({
        "type": "object",
        "description": "Pagination cursor from the previous response. Pass its next_params back verbatim to fetch the next page; omit for the first page.",
        "properties": {
            "visited": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Event IDs already seen in previous pages. Pass back verbatim from the previous response — do not fabricate."
            }
        },
        "required": ["visited"]
    })
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
    #[serde(default)]
    #[schemars(schema_with = "next_params_schema")]
    pub next_params: Option<TopMoversNextParams>,
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
        "next_params": p.next_params.map_or_else(|| serde_json::json!({}), |v| serde_json::to_value(v).unwrap_or_default()),
    });
    if let Some(ref d) = p.date {
        body["date"] = serde_json::Value::String(d.clone());
    }
    // Per-event stock display noise: `profile` (~150-word paragraph), `logo`
    // (URL), `full_name` (== name), and constant `latency`/`duplicate` flags.
    crate::tools::support::http_client::http_post_tool_unix_dropping(
        &client,
        "/v1/quote/market/stock-events",
        body,
        &["events.*.timestamp"],
        &["profile", "logo", "full_name", "latency", "duplicate"],
    )
    .await
}

/// Get available rank tab category configurations.
pub async fn rank_categories(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let result = http_get_tool(&client, "/v1/quote/market/rank/categories", &[]).await?;
    Ok(strip_ib_prefix_from_rank_keys(result))
}

/// Strip the "ib_" prefix from all `key` fields inside rank category tags.
/// rank_list auto-prepends "ib_" before sending to the API, so the prefix
/// is an implementation detail that should not be exposed to callers.
fn strip_ib_prefix_from_rank_keys(
    result: rmcp::model::CallToolResult,
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
    fn strip_key(v: &mut serde_json::Value) {
        if let Some(k) = v.get("key").and_then(|k| k.as_str()) {
            let stripped = k.strip_prefix("ib_").unwrap_or(k).to_string();
            if let Some(obj) = v.as_object_mut() {
                obj.insert("key".to_string(), serde_json::Value::String(stripped));
            }
        }
        for field in ["second_tags"] {
            if let Some(arr) = v.get_mut(field).and_then(|v| v.as_array_mut()) {
                for item in arr.iter_mut() {
                    strip_key(item);
                }
            }
        }
    }
    if let Some(tags) = d.get_mut("first_tags").and_then(|v| v.as_array_mut()) {
        for tag in tags.iter_mut() {
            strip_key(tag);
        }
    }
    let Ok(json) = serde_json::to_string(&d) else {
        return result;
    };
    crate::tools::tool_result(json)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RankListParam {
    /// Tab key from rank_categories second_tags[].key, e.g. "hot_all-us" (US total heat),
    /// "hot_up-hk" (HK rising heat), "trade_heat-us" (US hot trades).
    /// The "ib_" prefix is stripped from rank_categories keys and added back automatically.
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
    // Per-row upstream fields with no analytic value: `code` (== symbol without
    // suffix), `market` (single-market query, derivable), constant/empty flags
    // (`delay`, `is_pre_post`, `pre_post_*`, `extend_*`), the editorial `intro`
    // blurb, and the `article` object.
    http_get_tool_dropping(
        &client,
        "/v1/quote/market/rank/list",
        &[
            ("key", key.as_str()),
            ("delay_bmp", "false"),
            ("need_article", need_article.as_str()),
            ("market", key_market.as_str()),
            ("size", size.as_str()),
        ],
        &[
            "code",
            "market",
            "delay",
            "is_pre_post",
            "pre_post_price",
            "pre_post_chg",
            "extend_state",
            "extend_price",
            "extend_chg",
            "article",
            "intro",
        ],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{hoist_intraday_prev_closes, trade_status_label};

    #[test]
    fn hoist_intraday_prev_closes_lifts_prior_closes_and_keeps_live_fields() {
        let mut v = serde_json::json!({
            "klines": [
                {"aprice": "55.160", "apreclose": "52.890", "hprice": "53.750",
                 "hpreclose": "56.250", "currency_rate": "0.855300",
                 "ahpremium_rate": "-0.166563", "timestamp": "2026-09-11T01:30:00Z"},
                {"aprice": "54.810", "apreclose": "52.890", "hprice": "53.550",
                 "hpreclose": "56.250", "currency_rate": "0.855300",
                 "ahpremium_rate": "-0.164362", "timestamp": "2026-09-11T01:31:00Z"}
            ]
        });
        hoist_intraday_prev_closes(&mut v);

        // Prior closes are lifted once to the top level.
        assert_eq!(v["apreclose"], "52.890", "A-share prior close is hoisted");
        assert_eq!(v["hpreclose"], "56.250", "H-share prior close is hoisted");
        // …and removed from every bar.
        for bar in v["klines"].as_array().expect("klines is an array") {
            assert!(
                bar.get("apreclose").is_none(),
                "per-bar apreclose is dropped"
            );
            assert!(
                bar.get("hpreclose").is_none(),
                "per-bar hpreclose is dropped"
            );
            // Live per-bar fields stay put — including currency_rate.
            assert!(bar.get("aprice").is_some(), "live aprice is kept");
            assert!(bar.get("hprice").is_some(), "live hprice is kept");
            assert!(
                bar.get("currency_rate").is_some(),
                "currency_rate stays per-bar (it can move intraday)"
            );
            assert!(bar.get("ahpremium_rate").is_some(), "premium rate is kept");
        }
    }

    #[test]
    fn hoist_intraday_prev_closes_tolerates_empty_klines() {
        let mut v = serde_json::json!({ "klines": [] });
        hoist_intraday_prev_closes(&mut v);
        assert_eq!(
            v,
            serde_json::json!({ "klines": [] }),
            "empty input is unchanged"
        );
    }

    #[test]
    fn trade_status_label_matches_openapi_status() {
        let cases = [
            (101, "Closed"),
            (103, "Morning Break"),
            (106, "Mid-Day Break"),
            (123, "Temporary Break"),
            (202, "Trading"),
            (204, "Closed"),
            (206, "Pre-Market"),
            (210, "Trading"),
            (456, "Unknown"),
        ];

        for (code, expected) in cases {
            assert_eq!(trade_status_label(code), expected, "code {code}");
        }
    }
}
