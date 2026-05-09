use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::http_client::http_get_tool_unix;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FinanceCalendarParam {
    /// Event category. One of:
    /// - "report": earnings reports (includes financial statements)
    /// - "dividend": dividend announcements
    /// - "split": stock splits and reverse splits (share consolidations)
    /// - "ipo": upcoming IPO listings
    /// - "macrodata": macro economic data releases (CPI, NFP, rate decisions, etc.)
    /// - "closed": market closure days
    pub category: String,
    /// Start date in YYYY-MM-DD format (inclusive)
    pub start: String,
    /// End date in YYYY-MM-DD format (inclusive)
    pub end: String,
    /// Optional market filter. One of: HK, US, CN, SG, JP, UK, DE, AU.
    /// Omit to include all markets.
    pub market: Option<String>,
}

pub async fn finance_calendar(
    mctx: &crate::tools::McpContext,
    p: FinanceCalendarParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut params: Vec<(&str, &str)> = vec![
        ("date", p.start.as_str()),
        ("date_end", p.end.as_str()),
        ("types[]", p.category.as_str()),
    ];
    let market_upper;
    if let Some(ref m) = p.market {
        market_upper = m.to_uppercase();
        params.push(("markets[]", market_upper.as_str()));
    }
    http_get_tool_unix(
        &client,
        "/v1/quote/finance_calendar",
        &params,
        &["list.*.infos.*.datetime"],
    )
    .await
}
