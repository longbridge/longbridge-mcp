use std::collections::{BTreeMap, HashMap};

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::serialize::{convert_unix_paths, transform_json};

/// Maximum pages to fetch per request (matches CLI behaviour).
const MAX_PAGES: usize = 20;

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

/// Extract the optional `next_date` cursor from a raw API page response.
fn next_date_of(raw: &serde_json::Value) -> Option<String> {
    raw["next_date"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Merge list items from multiple raw API pages, deduplicating events by `id`
/// within each date bucket (using `datetime+market` as fallback key for
/// events without an id, e.g. market-close entries).
fn merge_pages(pages: impl IntoIterator<Item = serde_json::Value>) -> serde_json::Value {
    let empty = vec![];
    // BTreeMap keeps date buckets sorted.
    let mut groups: BTreeMap<String, HashMap<String, serde_json::Value>> = BTreeMap::new();

    for page in pages {
        for bucket in page["list"].as_array().unwrap_or(&empty) {
            let date = bucket["date"].as_str().unwrap_or("").to_string();
            let slot = groups.entry(date).or_default();
            for info in bucket["infos"].as_array().unwrap_or(&empty) {
                let key = if let Some(id) = info["id"].as_str().filter(|s| !s.is_empty()) {
                    id.to_string()
                } else {
                    format!(
                        "{}_{}",
                        info["datetime"].as_str().unwrap_or(""),
                        info["market"].as_str().unwrap_or("")
                    )
                };
                slot.insert(key, info.clone());
            }
        }
    }

    let list: Vec<serde_json::Value> = groups
        .into_iter()
        .map(|(date, infos_map)| {
            let infos: Vec<serde_json::Value> = infos_map.into_values().collect();
            serde_json::json!({ "date": date, "infos": infos })
        })
        .collect();

    serde_json::json!({ "list": list })
}

pub async fn finance_calendar(
    mctx: &crate::tools::McpContext,
    p: FinanceCalendarParam,
) -> Result<CallToolResult, McpError> {
    let market_upper = p.market.as_deref().map(str::to_uppercase);

    let mut pages: Vec<serde_json::Value> = Vec::new();
    let mut current_date = p.start.clone();

    for _ in 0..MAX_PAGES {
        let mut params: Vec<(&str, &str)> = vec![
            ("date", current_date.as_str()),
            ("date_end", p.end.as_str()),
            ("types[]", p.category.as_str()),
            ("next", "later"),
            ("count", "100"),
            ("offset", "0"),
        ];
        if let Some(ref m) = market_upper {
            params.push(("markets[]", m.as_str()));
        }

        // Create a fresh client per page — the upstream server closes the
        // connection after each response, causing SendRequest on reuse.
        let resp: String = mctx
            .create_http_client()
            .request(reqwest::Method::GET, "/v1/quote/finance_calendar")
            .query_params(params)
            .response::<String>()
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        let raw: serde_json::Value = serde_json::from_str(&resp).map_err(Error::Serialize)?;
        let next_date = next_date_of(&raw);
        pages.push(raw);

        match next_date {
            Some(nd) if nd.as_str() <= p.end.as_str() && nd != current_date => {
                current_date = nd;
            }
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
    Ok(crate::tools::tool_result(json))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── next_date_of ────────────────────────────────────────────────────────

    #[test]
    fn next_date_of_returns_value() {
        let raw = serde_json::json!({"list": [], "next_date": "2026-05-27"});
        assert_eq!(next_date_of(&raw).as_deref(), Some("2026-05-27"));
    }

    #[test]
    fn next_date_of_absent_returns_none() {
        assert!(next_date_of(&serde_json::json!({"list": []})).is_none());
    }

    #[test]
    fn next_date_of_null_returns_none() {
        assert!(next_date_of(&serde_json::json!({"next_date": null})).is_none());
    }

    #[test]
    fn next_date_of_empty_string_returns_none() {
        assert!(next_date_of(&serde_json::json!({"next_date": ""})).is_none());
    }

    // ── merge_pages ─────────────────────────────────────────────────────────

    #[test]
    fn merge_pages_concatenates_distinct_dates() {
        let page1 = serde_json::json!({
            "list": [{"date": "2026-05-23", "infos": [{"id": "1", "symbol": "AAPL.US", "datetime": "", "market": "US"}]}],
            "next_date": "2026-05-27"
        });
        let page2 = serde_json::json!({
            "list": [
                {"date": "2026-05-27", "infos": [{"id": "2", "symbol": "CRM.US", "datetime": "", "market": "US"}]},
                {"date": "2026-05-28", "infos": [{"id": "3", "symbol": "PDD.US", "datetime": "", "market": "US"}]}
            ]
        });
        let merged = merge_pages([page1, page2]);
        let list = merged["list"].as_array().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0]["date"], "2026-05-23");
        assert_eq!(list[1]["date"], "2026-05-27");
        assert_eq!(list[2]["date"], "2026-05-28");
    }

    #[test]
    fn merge_pages_deduplicates_by_id() {
        let dup_event =
            serde_json::json!({"id": "42", "symbol": "TSLA.US", "datetime": "", "market": "US"});
        let page1 = serde_json::json!({
            "list": [{"date": "2026-05-27", "infos": [dup_event.clone()]}]
        });
        let page2 = serde_json::json!({
            "list": [{"date": "2026-05-27", "infos": [dup_event]}]
        });
        let merged = merge_pages([page1, page2]);
        let infos = &merged["list"][0]["infos"];
        assert_eq!(
            infos.as_array().unwrap().len(),
            1,
            "duplicate id should collapse"
        );
    }

    #[test]
    fn merge_pages_deduplicates_no_id_by_datetime_market() {
        let event = serde_json::json!({"id": "", "symbol": "CLOSED", "datetime": "1748390400", "market": "US"});
        let page1 = serde_json::json!({"list": [{"date": "2026-05-27", "infos": [event.clone()]}]});
        let page2 = serde_json::json!({"list": [{"date": "2026-05-27", "infos": [event]}]});
        let merged = merge_pages([page1, page2]);
        assert_eq!(merged["list"][0]["infos"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn merge_pages_single_page() {
        let page = serde_json::json!({"list": [{"date": "2026-05-23", "infos": []}]});
        let merged = merge_pages([page]);
        assert_eq!(merged["list"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn merge_pages_empty() {
        let merged = merge_pages([serde_json::json!({"list": []})]);
        assert_eq!(merged["list"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn merge_pages_date_buckets_sorted() {
        let page1 = serde_json::json!({"list": [{"date": "2026-05-28", "infos": []}]});
        let page2 = serde_json::json!({"list": [{"date": "2026-05-23", "infos": []}]});
        let merged = merge_pages([page1, page2]);
        let dates: Vec<&str> = merged["list"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["date"].as_str().unwrap())
            .collect();
        assert_eq!(
            dates,
            vec!["2026-05-23", "2026-05-28"],
            "dates must be sorted"
        );
    }

    // ── pagination boundary ─────────────────────────────────────────────────

    #[test]
    fn next_date_after_end_stops() {
        assert!("2026-05-31" > "2026-05-30");
    }

    #[test]
    fn next_date_equal_end_continues() {
        assert!("2026-05-30" <= "2026-05-30");
    }
}
