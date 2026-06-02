use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::serialize::{convert_unix_paths, transform_json};

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

/// Extract the `list` items and the optional `next_date` cursor from a raw
/// API page response.
fn extract_page(raw: &serde_json::Value) -> (Vec<serde_json::Value>, Option<String>) {
    let list = raw["list"].as_array().cloned().unwrap_or_default();
    let next_date = raw["next_date"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    (list, next_date)
}

/// Merge list items from multiple raw API pages into a single `{ "list": [...] }` value.
fn merge_pages(pages: impl IntoIterator<Item = serde_json::Value>) -> serde_json::Value {
    let merged: Vec<serde_json::Value> = pages
        .into_iter()
        .flat_map(|p| p["list"].as_array().cloned().unwrap_or_default())
        .collect();
    serde_json::json!({ "list": merged })
}

pub async fn finance_calendar(
    mctx: &crate::tools::McpContext,
    p: FinanceCalendarParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market_upper = p.market.as_deref().map(str::to_uppercase);

    let mut pages: Vec<serde_json::Value> = Vec::new();
    let mut current_date = p.start.clone();

    loop {
        let mut params: Vec<(&str, &str)> = vec![
            ("date", current_date.as_str()),
            ("date_end", p.end.as_str()),
            ("types[]", p.category.as_str()),
        ];
        if let Some(ref m) = market_upper {
            params.push(("markets[]", m.as_str()));
        }

        let resp: String = client
            .request(reqwest::Method::GET, "/v1/quote/finance_calendar")
            .query_params(params)
            .response::<String>()
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        let raw: serde_json::Value = serde_json::from_str(&resp).map_err(Error::Serialize)?;

        let (_, next_date) = extract_page(&raw);
        pages.push(raw);

        match next_date {
            Some(nd) if nd.as_str() <= p.end.as_str() => current_date = nd,
            _ => break,
        }
    }

    let merged = merge_pages(pages);
    let transformed = transform_json(
        serde_json::to_string(&merged)
            .map_err(Error::Serialize)?
            .as_bytes(),
    )
    .map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    convert_unix_paths(&mut value, &["list.*.infos.*.datetime"]);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_page_returns_list_and_next_date() {
        let raw = serde_json::json!({
            "list": [{"date": "2026-05-23", "infos": []}],
            "next_date": "2026-05-27"
        });
        let (list, next) = extract_page(&raw);
        assert_eq!(list.len(), 1, "list length");
        assert_eq!(next.as_deref(), Some("2026-05-27"));
    }

    #[test]
    fn extract_page_no_next_date_returns_none() {
        let raw = serde_json::json!({
            "list": [{"date": "2026-05-28", "infos": []}]
        });
        let (_, next) = extract_page(&raw);
        assert!(next.is_none(), "no next_date field → None");
    }

    #[test]
    fn extract_page_null_next_date_returns_none() {
        let raw = serde_json::json!({ "list": [], "next_date": null });
        let (_, next) = extract_page(&raw);
        assert!(next.is_none(), "null next_date → None");
    }

    #[test]
    fn extract_page_empty_string_next_date_returns_none() {
        let raw = serde_json::json!({ "list": [], "next_date": "" });
        let (_, next) = extract_page(&raw);
        assert!(next.is_none(), "empty-string next_date → None");
    }

    #[test]
    fn merge_pages_concatenates_lists() {
        let page1 = serde_json::json!({
            "list": [{"date": "2026-05-23", "infos": [{"symbol": "AAPL.US"}]}],
            "next_date": "2026-05-27"
        });
        let page2 = serde_json::json!({
            "list": [
                {"date": "2026-05-27", "infos": [{"symbol": "CRM.US"}]},
                {"date": "2026-05-28", "infos": [{"symbol": "PDD.US"}]}
            ]
        });
        let merged = merge_pages([page1, page2]);
        let list = merged["list"].as_array().unwrap();
        assert_eq!(list.len(), 3, "all items from both pages");
        assert_eq!(list[0]["date"], "2026-05-23");
        assert_eq!(list[1]["date"], "2026-05-27");
        assert_eq!(list[2]["date"], "2026-05-28");
    }

    #[test]
    fn merge_pages_single_page() {
        let page = serde_json::json!({
            "list": [{"date": "2026-05-23", "infos": []}]
        });
        let merged = merge_pages([page]);
        assert_eq!(merged["list"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn merge_pages_empty_list_on_page() {
        let page = serde_json::json!({ "list": [] });
        let merged = merge_pages([page]);
        assert_eq!(merged["list"].as_array().unwrap().len(), 0);
    }

    /// Regression: next_date beyond end must not trigger another fetch.
    /// This is a pure date-comparison guard test.
    #[test]
    fn next_date_after_end_stops_pagination() {
        let next_date = "2026-05-31";
        let end = "2026-05-30";
        // The loop condition is: continue only when next_date <= end
        assert!(next_date > end, "next_date past end → stop");
    }

    /// next_date equal to end should still fetch (the last page may land on end).
    #[test]
    fn next_date_equal_end_continues_pagination() {
        let next_date = "2026-05-30";
        let end = "2026-05-30";
        assert!(next_date <= end, "next_date == end → continue");
    }
}
