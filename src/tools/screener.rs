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
    /// Strategy ID from screener_strategies
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
    /// Market (required): "US" | "HK" | "CN" | "SG"
    pub market: String,
    /// Mode A — Strategy ID from screener_recommend_strategies or screener_user_strategies
    /// screeners[].id. Omit when using Mode B (custom indicators).
    pub id: Option<String>,
    /// Mode B — Custom filter conditions. Each item: id (from screener_indicators
    /// groups[].indicators[].id), op ("gt"/"lt"/"between"/"eq"), value (scalar for gt/lt/eq,
    /// or [min, max] array for "between"). Omit when using Mode A (strategy id).
    pub indicators: Option<serde_json::Value>,
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
    let mut body = serde_json::json!({
        "market": p.market,
        "page": p.page.unwrap_or(1),
        "size": p.size.unwrap_or(20),
    });
    if let Some(id) = p.id {
        body["id"] = serde_json::Value::String(id);
    }
    if let Some(indicators) = p.indicators {
        body["indicators"] = indicators;
    }
    http_post_tool(&client, "/v1/quote/screener/search", body).await
}

pub async fn screener_indicators(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/quote/screener/indicators", &[]).await
}
