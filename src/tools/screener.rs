//! Stock screener tools — strategy lists, strategy detail, search, and indicator metadata.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::http_client::{http_get_tool, http_post_tool};

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
    /// Market (required): "US" | "HK" | "CN" | "SG".
    /// When using a strategy, the market embedded in the strategy overrides this value.
    pub market: String,
    /// filters: array of filter conditions to apply.
    /// Each item: {"key": "filter_balance", "min": "100", "max": "", "tech_values": {}}.
    /// Keys come from screener_strategy groups[].indicators[].key or screener_indicators.
    /// For Mode A (strategy): build from screener_strategy groups[].indicators[] —
    ///   skip indicators with id=-1 (market selector), use key/min/max from each indicator.
    /// For Mode B (custom): build manually using keys from screener_indicators.
    pub filters: Option<serde_json::Value>,
    /// returns: list of indicator keys to include in the response for each stock.
    /// Should match the keys in filters. Example: ["filter_balance", "filter_marketcap"]
    pub returns: Option<serde_json::Value>,
    /// Sort field index (default: 0 = first indicator in returns)
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
    let body = serde_json::json!({
        "market": p.market,
        "filters": p.filters.unwrap_or(serde_json::Value::Array(vec![])),
        "returns": p.returns.unwrap_or(serde_json::Value::Array(vec![])),
        "sort_by": p.sort_by.unwrap_or(0),
        "sort_order": p.sort_order.unwrap_or(1),
        "industries": [],
        "page": p.page.unwrap_or(1),
        "size": p.size.unwrap_or(20),
    });
    http_post_tool(&client, "/v1/quote/screener/search", body).await
}

pub async fn screener_indicators(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/quote/screener/indicators", &[]).await
}
