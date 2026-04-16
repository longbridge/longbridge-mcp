use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::tools::http_client::http_get_tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfitAnalysisDetailParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
}

pub async fn exchange_rate(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/asset/exchange_rates", &[]).await
}

pub async fn profit_analysis(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/portfolio/profit-analysis-summary", &[]).await
}

pub async fn profit_analysis_detail(
    mctx: &crate::tools::McpContext,
    p: ProfitAnalysisDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/portfolio/profit-analysis/detail",
        &[("counter_id", cid.as_str())],
    )
    .await
}
