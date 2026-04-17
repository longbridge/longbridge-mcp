use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::error::Error;
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

    let (summary_result, sublist_result) = tokio::join!(
        http_get_tool(&client, "/v1/portfolio/profit-analysis-summary", &[]),
        http_get_tool(
            &client,
            "/v1/portfolio/profit-analysis-sublist",
            &[("profit_or_loss", "all")]
        ),
    );

    let summary_text = summary_result?
        .content
        .into_iter()
        .next()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    let sublist_text = sublist_result?
        .content
        .into_iter()
        .next()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();

    let summary: serde_json::Value =
        serde_json::from_str(&summary_text).map_err(|e| Error::Other(e.to_string()))?;
    let sublist: serde_json::Value =
        serde_json::from_str(&sublist_text).map_err(|e| Error::Other(e.to_string()))?;

    let mut merged = match summary {
        serde_json::Value::Object(m) => m,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("data".to_owned(), other);
            map
        }
    };
    merged.insert("sublist".to_owned(), sublist);

    Ok(CallToolResult::success(vec![Content::text(
        serde_json::Value::Object(merged).to_string(),
    )]))
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
