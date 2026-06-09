use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::http_client::http_get_tool_unix;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacrodataListParam {
    /// Pagination offset, default 0.
    pub offset: Option<i32>,
    /// Maximum number of indicators to return (default 100, max 1000).
    /// There are ~619 indicators total; pass 1000 to fetch all at once.
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacrodataParam {
    /// Indicator code from `macrodata_indicators`, e.g. `"USCPI"`.
    pub indicator_code: String,
    /// Earliest release time to include (RFC3339, e.g. `"2024-01-01T00:00:00Z"`).
    pub start_time: Option<String>,
    /// Latest release time to include (RFC3339, e.g. `"2024-12-31T23:59:59Z"`).
    pub end_time: Option<String>,
    /// Maximum number of data points to return (default 100, max 100).
    pub limit: Option<i32>,
}

pub async fn macrodata_indicators(
    mctx: &crate::tools::McpContext,
    p: MacrodataListParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let offset = p.offset.unwrap_or(0).to_string();
    let limit = p.limit.unwrap_or(100).to_string();
    let params = [("offset", offset.as_str()), ("limit", limit.as_str())];
    http_get_tool_unix(
        &client,
        "/v1/quote/macrodata",
        &params,
        &["items.*.start_date"],
    )
    .await
}

pub async fn macrodata(
    mctx: &crate::tools::McpContext,
    p: MacrodataParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/quote/macrodata/{}", p.indicator_code);

    let mut params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref s) = p.start_time {
        params.push(("start_time", s.as_str()));
    }
    if let Some(ref e) = p.end_time {
        params.push(("end_time", e.as_str()));
    }
    let limit_str;
    if let Some(l) = p.limit {
        limit_str = l.to_string();
        params.push(("limit", limit_str.as_str()));
    }

    // release_at / next_release_at end with _at and are converted automatically
    // by transform_json; only start_date (no _at suffix) needs explicit listing.
    http_get_tool_unix(&client, &path, &params, &["info.start_date"]).await
}
