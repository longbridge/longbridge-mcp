//! Stock screener tools — strategy lists, strategy detail, search, and indicator metadata.

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

/// Platform-recommended screener strategies.
pub async fn screener_recommend_strategies(
    mctx: &crate::tools::McpContext,
    p: ScreenerRecommendStrategiesParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market = p.market.unwrap_or_else(|| "US".to_string());
    http_get_tool(
        &client,
        "/v1/quote/ai/screener/strategies/recommend",
        &[("market", market.as_str())],
    )
    .await
}

/// User's own saved screener strategies.
pub async fn screener_user_strategies(
    mctx: &crate::tools::McpContext,
    p: ScreenerUserStrategiesParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market = p.market.unwrap_or_else(|| "US".to_string());
    http_get_tool(
        &client,
        "/v1/quote/ai/screener/strategies/mine",
        &[("market", market.as_str())],
    )
    .await
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
    http_get_tool(&client, &path, &[]).await
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

    /// Mode B — Filter conditions as "KEY:MIN:MAX" strings. Omit when using Mode A.
    /// The filter_ prefix is added automatically; omit either bound to leave it open.
    ///   "pettm:10:50"      → 10 ≤ P/E TTM ≤ 50
    ///   "roe:15:"          → ROE ≥ 15 %
    ///   "marketcap:100:"   → market-cap ≥ 100 亿 (A/HK); units vary by market — check screener_indicators
    ///
    /// Verified keys (strip filter_ prefix when passing here):
    ///   pettm              P/E TTM                     (dimensionless)
    ///   pbmrq              P/B MRQ                     (dimensionless)
    ///   psttm              P/S TTM                     (dimensionless)
    ///   roe                Return on equity TTM        (%)
    ///   roa                Return on assets TTM        (%)
    ///   netmargin          Net profit margin           (%)
    ///   salesgrowthyoy     Revenue growth YoY TTM      (%)
    ///   netincomegrowthyoy Net income growth YoY TTM   (%)
    ///   marketcap          Market cap                  (亿 for A/HK; see screener_indicators for US)
    ///   prevclose          Previous close price        (currency)
    ///   divyld             Dividend yield TTM          (%)
    ///   la                 Debt / assets ratio         (%)
    ///   epsttm             EPS TTM                     (currency)
    ///   netincome          Net income TTM              (亿)
    ///   sales              Revenue TTM                 (亿)
    ///   turnover_rate      Turnover rate               (%)
    ///   group_balance      Daily turnover amount       (dimensionless)
    ///
    /// When uncertain about a key or getting empty results, call screener_indicators first.
    pub conditions: Option<Vec<String>>,

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
                let key = ind
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if key.is_empty() || key == "-" {
                    continue;
                }
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
        // Mode B: build filters+returns from "KEY:MIN:MAX" conditions
        let mut filters: Vec<serde_json::Value> = Vec::new();
        let mut returns: Vec<String> = Vec::new();
        for cond in p.conditions.as_deref().unwrap_or(&[]) {
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
            p.market.as_deref().unwrap_or("US").to_uppercase(),
            serde_json::Value::Array(filters),
            serde_json::Value::Array(returns.into_iter().map(serde_json::Value::String).collect()),
        )
    };

    // Append extra_returns (display-only columns, not filter conditions).
    let returns = {
        let mut all: Vec<serde_json::Value> = returns.as_array().cloned().unwrap_or_default();
        for raw in p.extra_returns.as_deref().unwrap_or(&[]) {
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
    http_get_tool(&client, "/v1/quote/ai/screener/indicators", &params).await
}
