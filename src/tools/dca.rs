//! DCA (Dollar-Cost Averaging / recurring investment) tools.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::confirm::{Confirmer, Decision};
use crate::tools::support::dry_run::Scope;
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
    /// The `confirmation_code` from this request's dry run. WITHOUT IT NOTHING
    /// IS SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing changes.
    /// Call once without `execute`, show the returned preview to the user, and
    /// call again quoting the code only after the user has explicitly confirmed
    /// that exact request. Never quote it back on your own initiative.
    pub execute: Option<String>,
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
    /// The `confirmation_code` from this request's dry run. WITHOUT IT NOTHING
    /// IS SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing changes.
    /// Call once without `execute`, show the returned preview to the user, and
    /// call again quoting the code only after the user has explicitly confirmed
    /// that exact request. Never quote it back on your own initiative.
    pub execute: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DcaPlanIdParam {
    /// Plan ID
    pub plan_id: String,
    /// The `confirmation_code` from this request's dry run. WITHOUT IT NOTHING
    /// IS SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing changes.
    /// Call once without `execute`, show the returned preview to the user, and
    /// call again quoting the code only after the user has explicitly confirmed
    /// that exact request. Never quote it back on your own initiative.
    pub execute: Option<String>,
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

    let mut params: Vec<(&str, &str)> = vec![("page", &page), ("limit", &limit)];
    if let Some(ref s) = p.status {
        params.push(("status", s.as_str()));
    }
    if let Some(ref sym) = p.symbol {
        params.push(("symbol", sym.as_str()));
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
    confirm: &Confirmer,
    p: DcaCreateParam,
) -> Result<CallToolResult, McpError> {
    let allow_margin = if p.allow_margin.unwrap_or(false) {
        1
    } else {
        0
    };

    let mut body = serde_json::json!({
        "symbol": p.symbol,
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

    // A plan keeps buying on its own once created, so every field that shapes
    // what it buys and when is part of what the user approves.
    let scope = plan_scope(Scope::request("dca create").and("symbol", &p.symbol), &body);
    let preview = serde_json::json!({ "action": "dca_create", "plan": body });
    if let Decision::Respond(r) = confirm
        .confirm(&scope, p.execute.as_deref(), preview)
        .await?
    {
        return Ok(r);
    }
    let client = mctx.create_http_client();
    http_post_tool(&client, "/v1/dailycoins/create", body).await
}

pub async fn dca_update(
    mctx: &crate::tools::McpContext,
    confirm: &Confirmer,
    p: DcaUpdateParam,
) -> Result<CallToolResult, McpError> {
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

    let scope = plan_scope(Scope::on("dca update", "plan", &p.plan_id), &body);
    let preview = serde_json::json!({ "action": "dca_update", "changes": body });
    if let Decision::Respond(r) = confirm
        .confirm(&scope, p.execute.as_deref(), preview)
        .await?
    {
        return Ok(r);
    }
    let client = mctx.create_http_client();
    http_post_tool(&client, "/v1/dailycoins/update", body).await
}

pub async fn dca_pause(
    mctx: &crate::tools::McpContext,
    confirm: &Confirmer,
    p: DcaPlanIdParam,
) -> Result<CallToolResult, McpError> {
    toggle(mctx, confirm, p, "dca pause", "Suspended").await
}

pub async fn dca_resume(
    mctx: &crate::tools::McpContext,
    confirm: &Confirmer,
    p: DcaPlanIdParam,
) -> Result<CallToolResult, McpError> {
    toggle(mctx, confirm, p, "dca resume", "Active").await
}

pub async fn dca_stop(
    mctx: &crate::tools::McpContext,
    confirm: &Confirmer,
    p: DcaPlanIdParam,
) -> Result<CallToolResult, McpError> {
    toggle(mctx, confirm, p, "dca stop", "Finished").await
}

/// Pause, resume or stop a plan, after the user confirms it.
async fn toggle(
    mctx: &crate::tools::McpContext,
    confirm: &Confirmer,
    p: DcaPlanIdParam,
    action: &str,
    status: &str,
) -> Result<CallToolResult, McpError> {
    let scope = Scope::on(action, "plan", &p.plan_id);
    let preview = serde_json::json!({
        "action": action.replace(' ', "_"),
        "plan_id": p.plan_id,
        "new_status": status,
    });
    if let Decision::Respond(r) = confirm
        .confirm(&scope, p.execute.as_deref(), preview)
        .await?
    {
        return Ok(r);
    }
    let client = mctx.create_http_client();
    let body = serde_json::json!({ "plan_id": p.plan_id, "status": status });
    http_post_tool(&client, "/v1/dailycoins/toggle", body).await
}

/// Fold every field of a plan request body into `scope`, in a fixed order, so
/// a code confirmed for one amount or schedule cannot apply another.
fn plan_scope(mut scope: Scope, body: &serde_json::Value) -> Scope {
    for (label, key) in [
        ("amount", "per_invest_amount"),
        ("frequency", "invest_frequency"),
        ("day_of_week", "invest_day_of_week"),
        ("day_of_month", "invest_day_of_month"),
        ("margin", "allow_margin_finance"),
    ] {
        match &body[key] {
            serde_json::Value::Null => {}
            serde_json::Value::String(v) => scope = scope.and(label, v),
            other => scope = scope.and(label, &other.to_string()),
        }
    }
    scope
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
    let mut params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref sym) = p.symbol {
        params.push(("symbol", sym.as_str()));
    }
    http_get_tool(&client, "/v1/dailycoins/statistic", &params).await
}

pub async fn dca_check(
    mctx: &crate::tools::McpContext,
    p: DcaCheckParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let body = serde_json::json!({ "symbols": p.symbols });
    http_post_tool(&client, "/v1/dailycoins/batch-check-support", body).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(amount: &str) -> serde_json::Value {
        serde_json::json!({
            "symbol": "AAPL.US",
            "per_invest_amount": amount,
            "invest_frequency": "Weekly",
            "invest_day_of_week": "Mon",
            "allow_margin_finance": 0,
        })
    }

    #[test]
    fn plan_scope_binds_amount_schedule_and_margin() {
        let scope = |b: &serde_json::Value| plan_scope(Scope::request("dca create"), b);
        assert_eq!(
            scope(&body("100")).to_string(),
            "dca create + amount 100 + frequency WEEKLY + day_of_week MON + margin 0"
        );
        // A code confirmed for one amount must not create a bigger plan.
        assert_ne!(scope(&body("100")).code(), scope(&body("1000")).code());
        assert_eq!(scope(&body("100")).code(), scope(&body("100.00")).code());
    }

    #[test]
    fn omitting_execute_deserializes_to_a_dry_run() {
        let p: DcaPlanIdParam =
            serde_json::from_value(serde_json::json!({ "plan_id": "1" })).unwrap();
        assert!(p.execute.is_none(), "no execute must mean a dry run");
    }
}
