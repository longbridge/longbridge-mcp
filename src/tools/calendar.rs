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
    /// Start date in YYYY-MM-DD format (inclusive). Ignored when `next_date` is provided.
    pub start: String,
    /// End date in YYYY-MM-DD format (inclusive)
    pub end: String,
    /// Optional market filter. One of: HK, US, CN, SG, JP, UK, DE, AU.
    /// Omit to include all markets.
    pub market: Option<String>,
    /// Pagination cursor returned as `next_date` in a previous response.
    /// When present, overrides `start` and fetches the next page.
    /// Keep calling with the returned `next_date` until the response contains
    /// no `next_date` (or an empty one) to collect all pages.
    pub next_date: Option<String>,
}

pub async fn finance_calendar(
    mctx: &crate::tools::McpContext,
    p: FinanceCalendarParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let date = p.next_date.as_deref().unwrap_or(p.start.as_str());
    let mut params: Vec<(&str, &str)> = vec![
        ("date", date),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// When `next_date` is absent the `date` query param must equal `start`.
    #[test]
    fn uses_start_when_no_next_date() {
        let p = FinanceCalendarParam {
            category: "report".into(),
            start: "2026-05-23".into(),
            end: "2026-05-30".into(),
            market: None,
            next_date: None,
        };
        let date = p.next_date.as_deref().unwrap_or(p.start.as_str());
        assert_eq!(date, "2026-05-23");
    }

    /// When `next_date` is provided it must override `start` as the `date` param.
    #[test]
    fn uses_next_date_when_provided() {
        let p = FinanceCalendarParam {
            category: "report".into(),
            start: "2026-05-23".into(),
            end: "2026-05-30".into(),
            market: None,
            next_date: Some("2026-05-27".into()),
        };
        let date = p.next_date.as_deref().unwrap_or(p.start.as_str());
        assert_eq!(date, "2026-05-27");
    }
}
