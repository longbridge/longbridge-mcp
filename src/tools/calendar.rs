use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::http_client::http_get_tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FinanceCalendarParam {
    /// Market code: HK, US, CN, SG. Omit to query across all markets.
    pub market: Option<String>,
    /// Start date (yyyy-mm-dd)
    pub start: String,
    /// End date (yyyy-mm-dd)
    pub end: String,
    /// Event category:
    /// - "financial": earnings/financial results announcements
    /// - "report": scheduled financial report release dates
    /// - "dividend": dividend ex-dates and payment dates
    /// - "ipo": IPO listing dates
    /// - "macrodata": macroeconomic data releases (GDP, CPI, etc.)
    /// - "closed": market holiday / trading halt dates
    pub category: String,
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
    http_get_tool(&client, "/v1/quote/finance_calendar", &params).await
}
