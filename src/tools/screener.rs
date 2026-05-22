//! Stock screener tools — strategy lists, strategy detail, search, and indicator metadata.

const DEFAULT_RETURNS: &[&str] = &[
    "filter_prevclose",
    "filter_prevchg",
    "filter_marketcap",
    "filter_salesgrowthyoy",
    "filter_pettm",
    "filter_pbmrq",
    "filter_industry",
];

use reqwest::Method;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::http_client::http_get_tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerRecommendStrategiesParam {
    /// Market filter: "US" | "HK" | "CN" | "SG" (default: "US")
    pub market: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerUserStrategiesParam {
    /// Market filter: "US" | "HK" | "CN" | "SG" (default: "US")
    pub market: Option<String>,
}

/// Strip "filter_" prefix from every `key` field inside strategy `filter.filters[]`.
/// Consistent with screener_indicators: keys are returned without prefix so callers
/// can pass them directly; screener_search Mode A re-adds the prefix before the API call.
fn strip_filter_prefix_from_strategy(v: &mut serde_json::Value) {
    if let Some(filters) = v
        .get_mut("filter")
        .and_then(|f| f.get_mut("filters"))
        .and_then(|f| f.as_array_mut())
    {
        for item in filters.iter_mut() {
            if let Some(k) = item.get("key").and_then(|k| k.as_str()) {
                let stripped = k.strip_prefix("filter_").unwrap_or(k).to_string();
                if let Some(obj) = item.as_object_mut() {
                    obj.insert("key".to_string(), serde_json::Value::String(stripped));
                }
            }
        }
    }
}

fn strip_strategy_keys(result: rmcp::model::CallToolResult) -> rmcp::model::CallToolResult {
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
    // single strategy object
    strip_filter_prefix_from_strategy(&mut d);
    // strategy list (strategys[])
    if let Some(list) = d.get_mut("strategys").and_then(|v| v.as_array_mut()) {
        for s in list.iter_mut() {
            strip_filter_prefix_from_strategy(s);
        }
    }
    let Ok(json) = serde_json::to_string(&d) else {
        return result;
    };
    rmcp::model::CallToolResult::success(vec![rmcp::model::Content::text(json)])
}

/// Platform-recommended screener strategies.
pub async fn screener_recommend_strategies(
    mctx: &crate::tools::McpContext,
    p: ScreenerRecommendStrategiesParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market = p.market.unwrap_or_else(|| "US".to_string());
    let result = http_get_tool(
        &client,
        "/v1/quote/ai/screener/strategies/recommend",
        &[("market", market.as_str())],
    )
    .await?;
    Ok(strip_strategy_keys(result))
}

/// User's own saved screener strategies.
pub async fn screener_user_strategies(
    mctx: &crate::tools::McpContext,
    p: ScreenerUserStrategiesParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market = p.market.unwrap_or_else(|| "US".to_string());
    let result = http_get_tool(
        &client,
        "/v1/quote/ai/screener/strategies/mine",
        &[("market", market.as_str())],
    )
    .await?;
    Ok(strip_strategy_keys(result))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerStrategyParam {
    /// Strategy ID from screener_recommend_strategies or screener_user_strategies screeners[].id
    pub id: String,
}

pub async fn screener_strategy(
    mctx: &crate::tools::McpContext,
    p: ScreenerStrategyParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/quote/ai/screener/strategy/{}", p.id);
    let result = http_get_tool(&client, &path, &[]).await?;
    Ok(strip_strategy_keys(result))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerSearchParam {
    /// Market: "US" | "HK" | "CN" | "SG".
    /// Mode A: overridden by the market embedded in the strategy; pass any value or omit.
    /// Mode B: required — determines which market to screen.
    pub market: Option<String>,

    /// Mode A — Strategy ID from screener_recommend_strategies screeners[].id.
    /// The tool auto-fetches the strategy and builds filters. Omit for Mode B.
    pub strategy_id: Option<String>,

    /// Mode B — Filter conditions as objects, passed directly to the API.
    /// Each item: {"key": "KEY", "min": "10", "max": "50", "tech_values": {}}
    /// The "filter_" prefix is added automatically to the key if missing.
    ///
    /// Fundamental keys (pass with or without filter_ prefix):
    ///   pettm  pbmrq  roe  roa  netmargin
    ///   salesgrowthyoy  netincomegrowthyoy  marketcap(亿)
    ///   circulating_marketcap(亿)  prevclose  prevchg(%)
    ///   divyld  la  epsttm  netincome(亿)  sales(亿)  turnover_rate  balance(万)
    ///
    /// Technical indicator keys (tech_values required; call screener_indicators for schema):
    ///   macd_day/week  → {"category":"goldenfork"|"deadcross","period":"day"|"week"}
    ///   rsi_day/week   → {"value_type":"overbought"|"oversold"}
    ///   kdj_day/week   → {"category":"goldenfork"|"deadcross"}
    ///   boll_day/week  → {"category":"breakthrough_up"|"breakthrough_down"}
    pub conditions: Option<Vec<serde_json::Value>>,

    /// Extra indicator keys to include in each result row (display-only, not used as filters).
    /// Same key naming as conditions (filter_ prefix added automatically).
    /// Example: ["marketcap", "prevclose", "epsttm"]
    pub extra_returns: Option<Vec<String>>,

    /// Indicator key to sort results by (e.g. "marketcap", "roe").
    /// Defaults to the first condition key. Must be one of the condition or extra_returns keys.
    pub sort_by_key: Option<String>,

    /// Sort order: "asc" | "desc" (default: "desc")
    pub sort_order: Option<String>,

    /// Page number, 0-based (default: 0)
    pub page: Option<u32>,
    /// Page size (default: 20, max: 100)
    pub size: Option<u32>,
}

pub async fn screener_search(
    mctx: &crate::tools::McpContext,
    p: ScreenerSearchParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();

    let (market, filters, returns) = if let Some(ref sid) = p.strategy_id {
        // Mode A: fetch strategy and build filters/returns automatically
        let strategy_path = format!("/v1/quote/ai/screener/strategy/{sid}");
        let raw: String = client
            .request(Method::GET, &strategy_path)
            .response::<String>()
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        let strategy: serde_json::Value = serde_json::from_str(&raw).map_err(Error::Serialize)?;

        // AI endpoint: market is top-level; filters are under filter.filters[]
        let mkt = strategy
            .get("market")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && *s != "-")
            .map(|s| s.to_uppercase())
            .unwrap_or_else(|| p.market.as_deref().unwrap_or("US").to_uppercase());

        let mut filters: Vec<serde_json::Value> = Vec::new();
        let mut returns: Vec<String> = Vec::new();

        if let Some(f) = strategy
            .get("filter")
            .and_then(|f| f.get("filters"))
            .and_then(|v| v.as_array())
        {
            for ind in f {
                let raw_key = ind
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if raw_key.is_empty() || raw_key == "-" {
                    continue;
                }
                // Re-add filter_ prefix (stripped in strategy display, required by search API)
                let key = if raw_key.starts_with("filter_") {
                    raw_key
                } else {
                    format!("filter_{raw_key}")
                };
                let min = ind
                    .get("min")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let max = ind
                    .get("max")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tech_values = ind
                    .get("tech_values")
                    .cloned()
                    .filter(|v| v.is_object())
                    .unwrap_or_else(|| serde_json::json!({}));
                filters.push(serde_json::json!({
                    "key": key,
                    "min": min,
                    "max": max,
                    "tech_values": tech_values
                }));
                returns.push(key);
            }
        }

        (
            mkt,
            serde_json::Value::Array(filters),
            serde_json::Value::Array(returns.into_iter().map(serde_json::Value::String).collect()),
        )
    } else {
        // Mode B: each condition is a filter object.
        // The "filter_" prefix is added automatically to the key if missing,
        // consistent with extra_returns and sort_by_key.
        let mut filters: Vec<serde_json::Value> = Vec::new();
        let mut returns: Vec<String> = Vec::new();

        for item in p.conditions.as_deref().unwrap_or(&[]) {
            if let Some(raw_key) = item.get("key").and_then(|v| v.as_str()) {
                if raw_key.is_empty() {
                    continue;
                }
                let key = if raw_key.starts_with("filter_") {
                    raw_key.to_string()
                } else {
                    format!("filter_{raw_key}")
                };
                returns.push(key.clone());
                // Rebuild the filter object with the normalised key
                let mut f = item.clone();
                if let Some(obj) = f.as_object_mut() {
                    obj.insert("key".to_string(), serde_json::Value::String(key));
                }
                filters.push(f);
            }
        }

        (
            p.market.as_deref().unwrap_or("US").to_uppercase(),
            serde_json::Value::Array(filters),
            serde_json::Value::Array(returns.into_iter().map(serde_json::Value::String).collect()),
        )
    };

    // Build final returns: condition keys + extra_returns + DEFAULT_RETURNS (deduplicated).
    let returns = {
        let mut all: Vec<serde_json::Value> = returns.as_array().cloned().unwrap_or_default();
        let extend = p
            .extra_returns
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|s| s.as_str())
            .chain(DEFAULT_RETURNS.iter().copied());
        for raw in extend {
            let key = if raw.starts_with("filter_") {
                raw.to_string()
            } else {
                format!("filter_{raw}")
            };
            if !all.contains(&serde_json::Value::String(key.clone())) {
                all.push(serde_json::Value::String(key));
            }
        }
        serde_json::Value::Array(all)
    };

    // Resolve sort_by_key → index into returns[].
    let sort_by: u32 = p.sort_by_key.as_deref().map_or(0, |raw_key| {
        let key = if raw_key.starts_with("filter_") {
            raw_key.to_string()
        } else {
            format!("filter_{raw_key}")
        };
        returns
            .as_array()
            .and_then(|arr| arr.iter().position(|v| v.as_str() == Some(key.as_str())))
            .unwrap_or(0) as u32
    });

    let sort_order: u32 = match p.sort_order.as_deref().unwrap_or("desc") {
        "asc" => 0,
        _ => 1,
    };

    let body = serde_json::json!({
        "market": market,
        "filters": filters,
        "returns": returns,
        "sort_by": sort_by,
        "sort_order": sort_order,
        "industries": [],
        "page": p.page.unwrap_or(0),
        "size": p.size.unwrap_or(20),
    });

    let resp: String = client
        .request(Method::POST, "/v1/quote/ai/screener/search")
        .body(longbridge::httpclient::Json(body))
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::Other(e.to_string()))?;

    let json = crate::serialize::transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    // Note: transform_json already renames counter_id → symbol in every item.
    Ok(rmcp::model::CallToolResult::success(vec![
        rmcp::model::Content::text(json),
    ]))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerIndicatorsParam {
    /// Optional security symbol to filter indicators for a specific stock, e.g. "AAPL.US"
    pub symbol: Option<String>,
}

pub async fn screener_indicators(
    mctx: &crate::tools::McpContext,
    p: ScreenerIndicatorsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut params: Vec<(&str, &str)> = vec![];
    let cid;
    if let Some(ref sym) = p.symbol {
        cid = crate::counter::symbol_to_counter_id(sym);
        params.push(("counter_id", cid.as_str()));
    }
    let result = http_get_tool(&client, "/v1/quote/ai/screener/indicators", &params).await?;
    Ok(strip_filter_prefix_from_indicators(result))
}

/// Strip the "filter_" prefix from indicator key fields so that the keys
/// returned by screener_indicators can be passed directly to screener_search
/// conditions[].key without any transformation.
fn strip_filter_prefix_from_indicators(
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
    if let Some(groups) = d.get_mut("groups").and_then(|v| v.as_array_mut()) {
        for group in groups.iter_mut() {
            if let Some(indicators) = group.get_mut("indicators").and_then(|v| v.as_array_mut()) {
                for ind in indicators.iter_mut() {
                    if let Some(k) = ind.get("key").and_then(|v| v.as_str()) {
                        let stripped = k.strip_prefix("filter_").unwrap_or(k).to_string();
                        if let Some(obj) = ind.as_object_mut() {
                            obj.insert("key".to_string(), serde_json::Value::String(stripped));
                        }
                    }
                }
            }
        }
    }
    let Ok(json) = serde_json::to_string(&d) else {
        return result;
    };
    rmcp::model::CallToolResult::success(vec![rmcp::model::Content::text(json)])
}
