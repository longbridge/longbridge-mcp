//! IPO (new share subscription) tools.
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::tools::support::http_client::{http_get_tool, http_get_tool_unix};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoSymbolParam {
    /// Security symbol, e.g. "9988.HK"
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoWaitListingParam {
    /// Day timestamp in seconds
    pub day_time: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoListedParam {
    /// Page number
    pub page: u32,
    /// Page size
    pub size: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoTimelineParam {
    /// Security symbol, e.g. "9988.HK"
    pub symbol: String,
    /// Market, e.g. "HK"
    pub market: String,
    /// Subscription type: 0 = regular, 2 = international allocation
    pub flag: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoOrdersParam {
    /// Account channel, defaults to "lb"
    pub account_channel: Option<String>,
    /// Filter by security symbol (optional)
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoOrderDetailParam {
    /// IPO order id
    pub order_id: String,
    /// Account channel, defaults to "lb"
    pub account_channel: Option<String>,
    /// Whether to enter amend mode ("true"/"false", optional)
    pub modify: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoProfitLossParam {
    /// Time period: "1m" / "3m" / "6m" / "1y" / "all"
    pub period: String,
    /// Account channel, defaults to "lb"
    pub account_channel: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoProfitLossItemsParam {
    /// Time period: "1m" / "3m" / "6m" / "1y" / "all"
    pub period: String,
    /// Page number
    pub page: String,
    /// Page size
    pub size: String,
    /// Account channel, defaults to "lb"
    pub account_channel: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IpoHoldingsDetailParam {
    /// Security symbol, e.g. "9988.HK"
    pub symbol: String,
    /// Whether to include real-time data, defaults to "true"
    pub need_realtime: Option<String>,
    /// Account channel, defaults to "lb"
    pub account_channel: Option<String>,
}

pub async fn ipo_subscriptions(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/ipo/subscriptions", &[]).await
}

pub async fn ipo_us_subscriptions(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/ipo/us/subscriptions", &[]).await
}

pub async fn ipo_wait_listing(
    mctx: &crate::tools::McpContext,
    p: IpoWaitListingParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(
        &client,
        "/v1/ipo/wait-listing",
        &[("day_time", p.day_time.as_str())],
    )
    .await
}

pub async fn ipo_us_wait_listing(
    mctx: &crate::tools::McpContext,
    p: IpoWaitListingParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(
        &client,
        "/v1/ipo/us/wait-listing",
        &[("day_time", p.day_time.as_str())],
    )
    .await
}

pub async fn ipo_listed(
    mctx: &crate::tools::McpContext,
    p: IpoListedParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.to_string();
    let size_str = p.size.to_string();
    http_get_tool(
        &client,
        "/v1/ipo/listed",
        &[("page", page_str.as_str()), ("size", size_str.as_str())],
    )
    .await
}

pub async fn ipo_us_listed(
    mctx: &crate::tools::McpContext,
    p: IpoListedParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.to_string();
    let size_str = p.size.to_string();
    http_get_tool(
        &client,
        "/v1/ipo/us/listed",
        &[("page", page_str.as_str()), ("size", size_str.as_str())],
    )
    .await
}

pub async fn ipo_calendar(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool_unix(
        &client,
        "/v1/ipo/calendar",
        &[],
        &[
            "list.*.sub_date",
            "list.*.sub_end_date",
            "list.*.result_date",
            "list.*.mart_date",
            "list.*.ipo_date",
        ],
    )
    .await
}

pub async fn ipo_basic(
    mctx: &crate::tools::McpContext,
    p: IpoSymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(&client, "/v1/ipo/basic", &[("counter_id", cid.as_str())]).await
}

pub async fn ipo_profile(
    mctx: &crate::tools::McpContext,
    p: IpoSymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/ipo/profile",
        &[("counter_id", cid.as_str())],
        &["hk.ipo_date", "hk.mart_begin", "hk.mart_end", "us.ipo_date"],
    )
    .await
}

pub async fn ipo_timeline(
    mctx: &crate::tools::McpContext,
    p: IpoTimelineParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let flag_str = p.flag.to_string();
    http_get_tool(
        &client,
        "/v1/ipo/timeline",
        &[
            ("counter_id", cid.as_str()),
            ("market", p.market.as_str()),
            ("flag", flag_str.as_str()),
        ],
    )
    .await
}

pub async fn ipo_active_order(
    mctx: &crate::tools::McpContext,
    p: IpoSymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/ipo/active-order",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn ipo_orders(
    mctx: &crate::tools::McpContext,
    p: IpoOrdersParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let channel = p
        .account_channel
        .as_deref()
        .unwrap_or("lb")
        .to_string();
    let mut params: Vec<(&str, &str)> = vec![("account_channel", channel.as_str())];
    let cid = p.symbol.as_deref().map(symbol_to_counter_id);
    if let Some(ref c) = cid {
        params.push(("counter_id", c.as_str()));
    }
    http_get_tool(&client, "/v1/ipo/orders", &params).await
}

pub async fn ipo_order_detail(
    mctx: &crate::tools::McpContext,
    p: IpoOrderDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let url = format!("/v1/ipo/orders/{}", p.order_id);
    let channel = p
        .account_channel
        .as_deref()
        .unwrap_or("lb")
        .to_string();
    let mut params: Vec<(&str, &str)> = vec![("account_channel", channel.as_str())];
    if let Some(ref m) = p.modify {
        params.push(("modify", m.as_str()));
    }
    http_get_tool(&client, &url, &params).await
}

pub async fn ipo_eligibility(
    mctx: &crate::tools::McpContext,
    p: IpoSymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/ipo/eligibility",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn ipo_profit_loss(
    mctx: &crate::tools::McpContext,
    p: IpoProfitLossParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let channel = p
        .account_channel
        .as_deref()
        .unwrap_or("lb")
        .to_string();
    http_get_tool(
        &client,
        "/v1/ipo/profit-loss",
        &[
            ("period", p.period.as_str()),
            ("account_channel", channel.as_str()),
        ],
    )
    .await
}

pub async fn ipo_profit_loss_items(
    mctx: &crate::tools::McpContext,
    p: IpoProfitLossItemsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let channel = p
        .account_channel
        .as_deref()
        .unwrap_or("lb")
        .to_string();
    http_get_tool(
        &client,
        "/v1/ipo/profit-loss/items",
        &[
            ("period", p.period.as_str()),
            ("page", p.page.as_str()),
            ("size", p.size.as_str()),
            ("account_channel", channel.as_str()),
        ],
    )
    .await
}

pub async fn ipo_holdings_detail(
    mctx: &crate::tools::McpContext,
    p: IpoHoldingsDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let realtime = p
        .need_realtime
        .as_deref()
        .unwrap_or("true")
        .to_string();
    let channel = p
        .account_channel
        .as_deref()
        .unwrap_or("lb")
        .to_string();
    http_get_tool(
        &client,
        "/v1/ipo/holdings",
        &[
            ("counter_id", cid.as_str()),
            ("need_realtime", realtime.as_str()),
            ("account_channel", channel.as_str()),
        ],
    )
    .await
}
