use longbridge::AssetContext;
use longbridge::asset::{GetStatementListOptions, GetStatementOptions, StatementType};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use time::OffsetDateTime;

use crate::error::Error;
use crate::tools::support::tolerant::tolerant_option_i32;
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatementListParam {
    /// Statement type: "daily" or "monthly"
    pub statement_type: Option<String>,
    /// Start date (yyyy-mm-dd), optional
    pub start_date: Option<String>,
    /// Number of records to return
    #[serde(default, deserialize_with = "tolerant_option_i32")]
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatementExportParam {
    /// File key from statement_list, e.g. "/statement_data/data/.../20975338.json"
    pub file_key: String,
}

pub async fn statement_list(
    mctx: &crate::tools::McpContext,
    p: StatementListParam,
) -> Result<CallToolResult, McpError> {
    let st = p.statement_type.as_deref().unwrap_or("daily");
    let is_monthly = matches!(st.to_lowercase().as_str(), "monthly" | "m");
    let statement_type = if is_monthly {
        StatementType::Monthly
    } else {
        StatementType::Daily
    };

    let start_date: i32 = if let Some(ref s) = p.start_date {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() == 3 {
            let y: i32 = parts[0].parse().unwrap_or(2024);
            let m: i32 = parts[1].parse().unwrap_or(1);
            let d: i32 = parts[2].parse().unwrap_or(1);
            y * 10000 + m * 100 + d
        } else {
            1
        }
    } else {
        let now = OffsetDateTime::now_utc();
        if is_monthly {
            let total_months = now.year() * 12 + now.month() as i32 - 1 - 12;
            let year = total_months / 12;
            let month = total_months % 12 + 1;
            year * 10000 + month * 100 + 1
        } else {
            let d = now - time::Duration::days(30);
            d.year() * 10000 + i32::from(d.month() as u8) * 100 + i32::from(d.day())
        }
    };

    let limit = p.limit.unwrap_or(if is_monthly { 12 } else { 30 });
    let options = GetStatementListOptions::new(statement_type)
        .page(start_date)
        .page_size(limit);

    let ctx = AssetContext::new(mctx.create_config());
    let result = ctx.statements(options).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn statement_export(
    mctx: &crate::tools::McpContext,
    p: StatementExportParam,
) -> Result<CallToolResult, McpError> {
    let ctx = AssetContext::new(mctx.create_config());
    let options = GetStatementOptions::new(p.file_key);
    let resp = ctx
        .statement_download_url(options)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&resp)
}
