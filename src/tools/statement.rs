use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::http_client::http_get_tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatementListParam {
    /// Statement type: "daily" or "monthly"
    pub statement_type: Option<String>,
    /// Start date (yyyy-mm-dd), optional
    pub start_date: Option<String>,
    /// Number of records to return
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatementExportParam {
    /// File key from statement_list
    pub file_key: String,
    /// Sections to export, e.g. ["equity_holdings", "cash_flow"]
    pub sections: Option<Vec<String>>,
}

pub async fn statement_list(
    mctx: &crate::tools::McpContext,
    p: StatementListParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let st = p.statement_type.as_deref().unwrap_or("daily");
    let st_val = if st == "monthly" { "monthly" } else { "daily" };
    let limit_str = p.limit.unwrap_or(30).to_string();
    let mut params: Vec<(&str, &str)> = vec![("type", st_val), ("page_size", limit_str.as_str())];
    let start;
    if let Some(ref s) = p.start_date {
        start = s.replace('-', "");
        params.push(("start_date", start.as_str()));
    }
    http_get_tool(&client, "/v1/asset/statements", &params).await
}

pub async fn statement_export(
    mctx: &crate::tools::McpContext,
    p: StatementExportParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut params: Vec<(&str, &str)> = vec![("file_key", p.file_key.as_str())];
    let sections_str;
    if let Some(ref sections) = p.sections {
        sections_str = sections.join(",");
        params.push(("sections", sections_str.as_str()));
    }
    http_get_tool(&client, "/v1/asset/statement", &params).await
}
