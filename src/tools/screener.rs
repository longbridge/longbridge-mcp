//! Stock screener tools — strategy lists, strategy detail, search, and indicator metadata.

use reqwest::Method;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::http_client::http_get_tool;

/// Platform-recommended screener strategies (no params required).
pub async fn screener_recommend_strategies(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/quote/screener/strategies/recommend", &[]).await
}

/// User's own saved screener strategies (no params required).
pub async fn screener_user_strategies(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/quote/screener/strategies/mine", &[]).await
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
    http_get_tool(
        &client,
        "/v1/quote/screener/strategy",
        &[("id", p.id.as_str())],
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerSearchParam {
    /// Market: "US" | "HK" | "CN" | "SG". Overridden by market embedded in strategy (Mode A).
    pub market: String,
    /// Mode A — Strategy ID from screener_recommend_strategies or screener_user_strategies.
    /// The tool auto-fetches the strategy and builds filters. Omit for Mode B.
    pub strategy_id: Option<String>,
    /// Mode B (simple) — Filter conditions as "KEY:MIN:MAX" strings.
    /// KEY is the indicator key from screener_indicators (the filter_ prefix is added automatically).
    /// Omit either bound to leave it open: "pettm:10:" means PE >= 10, "pettm::50" means PE <= 50.
    /// Example: ["pettm:10:50", "roe:5:", "marketcap:100:"]
    /// Returns[] is auto-built from condition keys. Omit when using Mode A or advanced filters.
    pub conditions: Option<Vec<String>>,
    /// Mode B (advanced) — Full filter array. Each item: {"key":"filter_pettm","min":"10","max":"50","tech_values":{}}.
    /// Use only when conditions[] is insufficient. Requires returns[] to also be set.
    pub filters: Option<serde_json::Value>,
    /// Mode B (advanced) — Indicator keys to include per result stock, matching keys in filters[].
    pub returns: Option<serde_json::Value>,
    /// Sort field index into returns[] (default: 0)
    pub sort_by: Option<u32>,
    /// Sort order: 0=ascending, 1=descending (default: 1)
    pub sort_order: Option<u32>,
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
}

pub async fn screener_search(
    mctx: &crate::tools::McpContext,
    p: ScreenerSearchParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();

    let (market, filters, returns) = if let Some(ref sid) = p.strategy_id {
        // Mode A: fetch strategy and build filters/returns automatically
        let raw: String = client
            .request(Method::GET, "/v1/quote/screener/strategy")
            .query_params(vec![("id", sid.as_str())])
            .response::<String>()
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        let strategy: serde_json::Value = serde_json::from_str(&raw).map_err(Error::Serialize)?;

        let mut mkt = p.market.to_uppercase();
        let mut filters: Vec<serde_json::Value> = Vec::new();
        let mut returns: Vec<String> = Vec::new();

        if let Some(groups) = strategy
            .get("data")
            .and_then(|d| d.get("groups"))
            .or_else(|| strategy.get("groups"))
            .and_then(|g| g.as_array())
        {
            for group in groups {
                if let Some(indicators) = group.get("indicators").and_then(|v| v.as_array()) {
                    for ind in indicators {
                        let key = ind
                            .get("key")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let id = ind.get("id").and_then(|v| v.as_i64()).unwrap_or(0);

                        if id == -1 && key == "filter_market" {
                            // Market selector — extract market value
                            if let Some(v) = ind
                                .get("value")
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.is_empty() && *s != "-")
                            {
                                mkt = v.to_string();
                            }
                        } else {
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
                            let has_range =
                                (!min.is_empty() && min != "-") || (!max.is_empty() && max != "-");
                            if has_range || id > 0 {
                                filters.push(serde_json::json!({
                                    "key": key,
                                    "min": min,
                                    "max": max,
                                    "tech_values": {}
                                }));
                                returns.push(key);
                            }
                        }
                    }
                }
            }
        }

        (
            mkt,
            serde_json::Value::Array(filters),
            serde_json::Value::Array(returns.into_iter().map(serde_json::Value::String).collect()),
        )
    } else if let Some(ref conditions) = p.conditions {
        // Mode B simple: build filters+returns from "KEY:MIN:MAX" conditions
        let mut filters: Vec<serde_json::Value> = Vec::new();
        let mut returns: Vec<String> = Vec::new();
        for cond in conditions {
            let parts: Vec<&str> = cond.splitn(3, ':').collect();
            let raw_key = parts.first().copied().unwrap_or("");
            if raw_key.is_empty() {
                continue;
            }
            let key = if raw_key.starts_with("filter_") {
                raw_key.to_string()
            } else {
                format!("filter_{raw_key}")
            };
            let min = parts.get(1).copied().unwrap_or("").to_string();
            let max = parts.get(2).copied().unwrap_or("").to_string();
            filters.push(serde_json::json!({
                "key": key,
                "min": min,
                "max": max,
                "tech_values": {}
            }));
            returns.push(key);
        }
        (
            p.market.to_uppercase(),
            serde_json::Value::Array(filters),
            serde_json::Value::Array(returns.into_iter().map(serde_json::Value::String).collect()),
        )
    } else {
        // Mode B advanced: use caller-supplied filters/returns
        (
            p.market.to_uppercase(),
            p.filters.unwrap_or(serde_json::Value::Array(vec![])),
            p.returns.unwrap_or(serde_json::Value::Array(vec![])),
        )
    };

    let body = serde_json::json!({
        "market": market,
        "filters": filters,
        "returns": returns,
        "sort_by": p.sort_by.unwrap_or(0),
        "sort_order": p.sort_order.unwrap_or(1),
        "industries": [],
        "page": p.page.unwrap_or(1),
        "size": p.size.unwrap_or(20),
    });

    let resp: String = client
        .request(Method::POST, "/v1/quote/screener/search")
        .body(longbridge::httpclient::Json(body))
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::Other(e.to_string()))?;

    let json = crate::serialize::transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
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
    http_get_tool(&client, "/v1/quote/screener/indicators", &params).await
}
