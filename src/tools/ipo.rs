use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::tools::support::http_client::http_get_tool;

fn make_result(json: String) -> CallToolResult {
    let structured = serde_json::from_str::<serde_json::Value>(&json).ok();
    let mut result = CallToolResult::success(vec![rmcp::model::Content::text(json)]);
    result.structured_content = structured;
    result
}

fn get_json(r: &CallToolResult) -> &str {
    r.content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("null")
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoListedParam {
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoDetailParam {
    /// Security symbol, e.g. "6871.HK" or "ARM.US"
    pub symbol: String,
    /// Market: "HK" or "US" (default: inferred from symbol suffix)
    pub market: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoOrdersParam {
    /// Filter by symbol, e.g. "6871.HK"
    pub symbol: Option<String>,
    /// Filter by market: "HK" or "US"
    pub market: Option<String>,
    /// Filter by order status
    pub status: Option<String>,
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoOrderDetailParam {
    /// IPO order ID
    pub order_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoProfitLossParam {
    /// Period filter: "all", "ytd", "1y", "3y" (default: "all")
    pub period: Option<String>,
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
}

/// List IPO stocks currently in subscription or pre-filing stage (HK and US).
pub async fn ipo_subscriptions(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let hk = http_get_tool(&client, "/v1/ipo/subscriptions", &[]).await?;
    let us = http_get_tool(&client, "/v1/ipo/us/subscriptions", &[]).await?;
    let combined = format!(r#"{{"hk":{},"us":{}}}"#, get_json(&hk), get_json(&us));
    Ok(make_result(combined))
}

/// Show the IPO calendar (all upcoming and recent IPOs).
pub async fn ipo_calendar(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/ipo/calendar", &[]).await
}

/// List recently listed IPO stocks (HK and US).
pub async fn ipo_listed(
    mctx: &crate::tools::McpContext,
    p: IpoListedParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();
    let params = [("page", page_str.as_str()), ("size", size_str.as_str())];
    let hk = http_get_tool(&client, "/v1/ipo/listed", &params).await?;
    let us = http_get_tool(&client, "/v1/ipo/us/listed", &params).await?;
    let combined = format!(r#"{{"hk":{},"us":{}}}"#, get_json(&hk), get_json(&us));
    Ok(make_result(combined))
}

/// Show IPO detail: profile + timeline + eligibility for a symbol.
pub async fn ipo_detail(
    mctx: &crate::tools::McpContext,
    p: IpoDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let market = p.market.unwrap_or_else(|| {
        p.symbol
            .rsplit_once('.')
            .map_or("HK", |(_, m)| m)
            .to_string()
    });
    let profile_params = [("counter_id", cid.as_str())];
    let timeline_params = [
        ("counter_id", cid.as_str()),
        ("market", market.as_str()),
        ("flag", "0"),
    ];
    let eligibility_params = [("counter_id", cid.as_str())];
    let profile = http_get_tool(&client, "/v1/ipo/profile", &profile_params).await?;
    let timeline = http_get_tool(&client, "/v1/ipo/timeline", &timeline_params).await?;
    let eligibility = http_get_tool(&client, "/v1/ipo/eligibility", &eligibility_params).await?;
    let combined = format!(
        r#"{{"profile":{},"timeline":{},"eligibility":{}}}"#,
        get_json(&profile),
        get_json(&timeline),
        get_json(&eligibility),
    );
    Ok(make_result(combined))
}

/// List IPO orders (active + history) for the current account.
pub async fn ipo_orders(
    mctx: &crate::tools::McpContext,
    p: IpoOrdersParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();
    let cid = p.symbol.as_deref().map(symbol_to_counter_id);
    let mut active_params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref c) = cid {
        active_params.push(("counter_id", c.as_str()));
    }
    let mut hist_params: Vec<(&str, &str)> =
        vec![("page", page_str.as_str()), ("limit", size_str.as_str())];
    if let Some(ref m) = p.market {
        hist_params.push(("market", m.as_str()));
    }
    if let Some(ref s) = p.status {
        hist_params.push(("status", s.as_str()));
    }
    let active = http_get_tool(&client, "/v1/ipo/orders", &active_params).await?;
    let history = http_get_tool(&client, "/v1/ipo/orders/history", &hist_params).await?;
    let combined = format!(
        r#"{{"orders":{},"history":{}}}"#,
        get_json(&active),
        get_json(&history),
    );
    Ok(make_result(combined))
}

/// Show IPO order detail by order ID.
pub async fn ipo_order_detail(
    mctx: &crate::tools::McpContext,
    p: IpoOrderDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/ipo/orders/{}", p.order_id);
    http_get_tool(&client, &path, &[]).await
}

/// Show IPO profit/loss summary and breakdown items for the given period.
pub async fn ipo_profit_loss(
    mctx: &crate::tools::McpContext,
    p: IpoProfitLossParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let period = p.period.as_deref().unwrap_or("all").to_string();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();
    let summary_params = [("period", period.as_str())];
    let items_params = [
        ("period", period.as_str()),
        ("page", page_str.as_str()),
        ("size", size_str.as_str()),
    ];
    let summary = http_get_tool(&client, "/v1/ipo/profit-loss", &summary_params).await?;
    let items = http_get_tool(&client, "/v1/ipo/profit-loss/items", &items_params).await?;
    let combined = format!(
        r#"{{"summary":{},"items":{}}}"#,
        get_json(&summary),
        get_json(&items),
    );
    Ok(make_result(combined))
}
