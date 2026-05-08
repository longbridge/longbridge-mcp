use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::http_client::http_get_tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewsSearchParam {
    /// Search keyword
    pub keyword: String,
    /// Max results to return (default: 20)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicSearchParam {
    /// Search keyword
    pub keyword: String,
    /// Max results to return (default: 20)
    pub limit: Option<u32>,
}

pub async fn news_search(
    mctx: &crate::tools::McpContext,
    p: NewsSearchParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let limit_str = p.limit.unwrap_or(20).to_string();
    http_get_tool(
        &client,
        "/v1/search/news",
        &[("k", p.keyword.as_str()), ("limit", limit_str.as_str())],
    )
    .await
}

pub async fn topic_search(
    mctx: &crate::tools::McpContext,
    p: TopicSearchParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let limit_str = p.limit.unwrap_or(20).to_string();
    http_get_tool(
        &client,
        "/v1/search/topics",
        &[("k", p.keyword.as_str()), ("limit", limit_str.as_str())],
    )
    .await
}
