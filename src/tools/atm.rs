use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::http_client::http_get_tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WithdrawalParam {
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepositParam {
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
    /// Filter by deposit states (comma-separated)
    pub states: Option<String>,
    /// Filter by currencies (comma-separated, e.g. "USD,HKD")
    pub currencies: Option<String>,
}

/// List linked withdrawal bank cards for the current account.
pub async fn bank_cards(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/account/bank-cards", &[]).await
}

/// List withdrawal history for the current account.
pub async fn withdrawals(
    mctx: &crate::tools::McpContext,
    p: WithdrawalParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();
    let channel = mctx.account_channel();
    http_get_tool(
        &client,
        "/v1/account/withdrawals",
        &[
            ("page", page_str.as_str()),
            ("size", size_str.as_str()),
            ("account_channel", channel.as_str()),
        ],
    )
    .await
}

/// List deposit history for the current account.
pub async fn deposits(
    mctx: &crate::tools::McpContext,
    p: DepositParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();
    let channel = mctx.account_channel();
    let mut params: Vec<(&str, &str)> = vec![
        ("page", page_str.as_str()),
        ("size", size_str.as_str()),
        ("account_channel", channel.as_str()),
    ];
    if let Some(ref s) = p.states {
        params.push(("states", s.as_str()));
    }
    if let Some(ref c) = p.currencies {
        params.push(("currencies", c.as_str()));
    }
    http_get_tool(&client, "/v1/account/deposits", &params).await
}
