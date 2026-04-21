//! DCA (Dollar-Cost Averaging / recurring investment) tools.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::tools::support::http_client::{http_get_tool, http_get_tool_unix, http_post_tool};
use crate::tools::support::tolerant::{
    tolerant_option_bool, tolerant_option_u32, tolerant_vec_string,
};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaListParam {
    /// Filter by status: Active, Suspended, Finished. Omit to return all.
    pub status: Option<String>,
    /// Filter by symbol, e.g. "AAPL.US". Omit to return all plans.
    pub symbol: Option<String>,
    /// Page number (default 1)
    #[serde(default, deserialize_with = "tolerant_option_u32")]
    pub page: Option<u32>,
    /// Records per page (default 20)
    #[serde(default, deserialize_with = "tolerant_option_u32")]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaCreateParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Amount to invest per cycle, e.g. "100"
    pub amount: String,
    /// Investment frequency: Daily, Weekly, Monthly
    pub frequency: String,
    /// Day of week for Weekly frequency: Mon, Tue, Wed, Thu, Fri
    pub day_of_week: Option<String>,
    /// Day of month for Monthly frequency (1-28)
    #[serde(default, deserialize_with = "tolerant_option_u32")]
    pub day_of_month: Option<u32>,
    /// Allow margin financing (default false)
    #[serde(default, deserialize_with = "tolerant_option_bool")]
    pub allow_margin: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaUpdateParam {
    /// Plan ID to update
    pub plan_id: String,
    /// New investment amount per cycle
    pub amount: Option<String>,
    /// New investment frequency: Daily, Weekly, Monthly
    pub frequency: Option<String>,
    /// Day of week for Weekly frequency: Mon, Tue, Wed, Thu, Fri
    pub day_of_week: Option<String>,
    /// Day of month for Monthly frequency (1-28)
    #[serde(default, deserialize_with = "tolerant_option_u32")]
    pub day_of_month: Option<u32>,
    /// Allow margin financing
    #[serde(default, deserialize_with = "tolerant_option_bool")]
    pub allow_margin: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaPlanIdParam {
    /// Plan ID
    pub plan_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaHistoryParam {
    /// Plan ID
    pub plan_id: String,
    /// Page number (default 1)
    #[serde(default, deserialize_with = "tolerant_option_u32")]
    pub page: Option<u32>,
    /// Records per page (default 20)
    #[serde(default, deserialize_with = "tolerant_option_u32")]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaStatsParam {
    /// Filter by symbol, e.g. "AAPL.US". Omit to return stats for all plans.
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaCheckParam {
    /// Security symbols to check, e.g. ["AAPL.US", "TSLA.US"]
    #[serde(deserialize_with = "tolerant_vec_string")]
    pub symbols: Vec<String>,
}

pub async fn dca_list(
    mctx: &crate::tools::McpContext,
    p: DcaListParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page = p.page.unwrap_or(1).to_string();
    let limit = p.limit.unwrap_or(20).to_string();
    let cid = p.symbol.as_deref().map(symbol_to_counter_id);

    let mut params: Vec<(&str, &str)> = vec![("page", &page), ("limit", &limit)];
    if let Some(ref s) = p.status {
        params.push(("status", s.as_str()));
    }
    if let Some(ref c) = cid {
        params.push(("counter_id", c.as_str()));
    }
    http_get_tool_unix(
        &client,
        "/v1/dailycoins/query",
        &params,
        &["plans.*.next_trd_date"],
    )
    .await
}

pub async fn dca_create(
    mctx: &crate::tools::McpContext,
    p: DcaCreateParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let allow_margin = if p.allow_margin.unwrap_or(false) {
        1
    } else {
        0
    };

    let mut body = serde_json::json!({
        "counter_id": cid,
        "per_invest_amount": p.amount,
        "invest_frequency": p.frequency,
        "allow_margin_finance": allow_margin,
    });

    if let Some(ref dow) = p.day_of_week {
        body["invest_day_of_week"] = serde_json::json!(dow);
    }
    if let Some(dom) = p.day_of_month {
        body["invest_day_of_month"] = serde_json::json!(dom.to_string());
    }

    http_post_tool(&client, "/v1/dailycoins/create", body).await
}

pub async fn dca_update(
    mctx: &crate::tools::McpContext,
    p: DcaUpdateParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut body = serde_json::json!({ "plan_id": p.plan_id });

    if let Some(ref amount) = p.amount {
        body["per_invest_amount"] = serde_json::json!(amount);
    }
    if let Some(ref freq) = p.frequency {
        body["invest_frequency"] = serde_json::json!(freq);
    }
    if let Some(ref dow) = p.day_of_week {
        body["invest_day_of_week"] = serde_json::json!(dow);
    }
    if let Some(dom) = p.day_of_month {
        body["invest_day_of_month"] = serde_json::json!(dom.to_string());
    }
    if let Some(allow) = p.allow_margin {
        body["allow_margin_finance"] = serde_json::json!(if allow { 1 } else { 0 });
    }

    http_post_tool(&client, "/v1/dailycoins/update", body).await
}

pub async fn dca_pause(
    mctx: &crate::tools::McpContext,
    p: DcaPlanIdParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let body = serde_json::json!({ "plan_id": p.plan_id, "status": "Suspended" });
    http_post_tool(&client, "/v1/dailycoins/toggle", body).await
}

pub async fn dca_resume(
    mctx: &crate::tools::McpContext,
    p: DcaPlanIdParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let body = serde_json::json!({ "plan_id": p.plan_id, "status": "Active" });
    http_post_tool(&client, "/v1/dailycoins/toggle", body).await
}

pub async fn dca_stop(
    mctx: &crate::tools::McpContext,
    p: DcaPlanIdParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let body = serde_json::json!({ "plan_id": p.plan_id, "status": "Finished" });
    http_post_tool(&client, "/v1/dailycoins/toggle", body).await
}

pub async fn dca_history(
    mctx: &crate::tools::McpContext,
    p: DcaHistoryParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page = p.page.unwrap_or(1).to_string();
    let limit = p.limit.unwrap_or(20).to_string();
    let params = [
        ("plan_id", p.plan_id.as_str()),
        ("page", page.as_str()),
        ("limit", limit.as_str()),
    ];
    http_get_tool(&client, "/v1/dailycoins/query-records", &params).await
}

pub async fn dca_stats(
    mctx: &crate::tools::McpContext,
    p: DcaStatsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = p.symbol.as_deref().map(symbol_to_counter_id);
    let mut params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref c) = cid {
        params.push(("counter_id", c.as_str()));
    }
    http_get_tool(&client, "/v1/dailycoins/statistic", &params).await
}

pub async fn dca_check(
    mctx: &crate::tools::McpContext,
    p: DcaCheckParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cids: Vec<String> = p.symbols.iter().map(|s| symbol_to_counter_id(s)).collect();
    let body = serde_json::json!({ "counter_ids": cids });
    http_post_tool(&client, "/v1/dailycoins/batch-check-support", body).await
}
