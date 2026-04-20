use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::tools::http_client::{http_get_tool, http_post_tool};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaListParam {
    /// Page number (1-based)
    pub page: u32,
    /// Number of results per page
    pub limit: u32,
    /// Filter by status: "Active", "Suspended", "Finished" (optional)
    pub status: Option<String>,
    /// Filter by symbol, e.g. "TSLA.US" (optional)
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaCreateParam {
    /// Security symbol, e.g. "TSLA.US"
    pub symbol: String,
    /// Amount per investment (as string, e.g. "100.00")
    pub per_invest_amount: String,
    /// Investment frequency: "daily", "weekly", "monthly"
    pub invest_frequency: String,
    /// Day of week for weekly plans (0=Sunday … 6=Saturday, optional)
    pub invest_day_of_week: Option<String>,
    /// Day of month for monthly plans (1-31, optional)
    pub invest_day_of_month: Option<String>,
    /// Allow margin financing: 0 = no, 1 = yes
    pub allow_margin_finance: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaUpdateParam {
    /// DCA plan ID
    pub plan_id: String,
    /// New amount per investment (optional)
    pub per_invest_amount: Option<String>,
    /// New investment frequency (optional): "daily", "weekly", "monthly"
    pub invest_frequency: Option<String>,
    /// Day of week for weekly plans (optional)
    pub invest_day_of_week: Option<String>,
    /// Day of month for monthly plans (optional)
    pub invest_day_of_month: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaToggleParam {
    /// DCA plan ID
    pub plan_id: String,
    /// New status: "Suspended" (pause) or "Active" (resume)
    pub status: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaRecordsParam {
    /// DCA plan ID
    pub plan_id: String,
    /// Page number (1-based)
    pub page: u32,
    /// Number of results per page
    pub limit: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaStatisticsParam {
    /// Filter by symbol, e.g. "TSLA.US" (optional)
    pub symbol: Option<String>,
}

pub async fn dca_list(
    mctx: &crate::tools::McpContext,
    p: DcaListParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.to_string();
    let limit_str = p.limit.to_string();
    let cid = p.symbol.as_deref().map(symbol_to_counter_id);
    let mut params: Vec<(&str, &str)> =
        vec![("page", page_str.as_str()), ("limit", limit_str.as_str())];
    if let Some(s) = p.status.as_deref() {
        params.push(("status", s));
    }
    if let Some(ref c) = cid {
        params.push(("counter_id", c.as_str()));
    }
    http_get_tool(&client, "/v1/dailycoins/query", &params).await
}

pub async fn dca_create(
    mctx: &crate::tools::McpContext,
    p: DcaCreateParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let mut body = serde_json::json!({
        "counter_id": cid,
        "per_invest_amount": p.per_invest_amount,
        "invest_frequency": p.invest_frequency,
        "allow_margin_finance": p.allow_margin_finance,
    });
    if let Some(v) = p.invest_day_of_week {
        body["invest_day_of_week"] = serde_json::Value::String(v);
    }
    if let Some(v) = p.invest_day_of_month {
        body["invest_day_of_month"] = serde_json::Value::String(v);
    }
    http_post_tool(&client, "/v1/dailycoins/create", body).await
}

pub async fn dca_update(
    mctx: &crate::tools::McpContext,
    p: DcaUpdateParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut body = serde_json::json!({ "plan_id": p.plan_id });
    if let Some(v) = p.per_invest_amount {
        body["per_invest_amount"] = serde_json::Value::String(v);
    }
    if let Some(v) = p.invest_frequency {
        body["invest_frequency"] = serde_json::Value::String(v);
    }
    if let Some(v) = p.invest_day_of_week {
        body["invest_day_of_week"] = serde_json::Value::String(v);
    }
    if let Some(v) = p.invest_day_of_month {
        body["invest_day_of_month"] = serde_json::Value::String(v);
    }
    http_post_tool(&client, "/v1/dailycoins/update", body).await
}

pub async fn dca_toggle(
    mctx: &crate::tools::McpContext,
    p: DcaToggleParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let body = serde_json::json!({
        "plan_id": p.plan_id,
        "status": p.status,
    });
    http_post_tool(&client, "/v1/dailycoins/toggle", body).await
}

pub async fn dca_records(
    mctx: &crate::tools::McpContext,
    p: DcaRecordsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.to_string();
    let limit_str = p.limit.to_string();
    http_get_tool(
        &client,
        "/v1/dailycoins/query-records",
        &[
            ("plan_id", p.plan_id.as_str()),
            ("page", page_str.as_str()),
            ("limit", limit_str.as_str()),
        ],
    )
    .await
}

pub async fn dca_statistics(
    mctx: &crate::tools::McpContext,
    p: DcaStatisticsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = p.symbol.as_deref().map(symbol_to_counter_id);
    let mut params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref c) = cid {
        params.push(("counter_id", c.as_str()));
    }
    http_get_tool(&client, "/v1/dailycoins/statistic", &params).await
}
