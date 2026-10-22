use std::str::FromStr;

use longbridge::trade::{TradeContext, USExchangeRate};
use longbridge::{DcRegion, Decimal};
use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::serialize::convert_unix_paths;
use crate::tools::support::http_client::{
    http_get_tool, http_get_tool_dropping, http_get_tool_unix,
};
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfitAnalysisParam {
    /// Start date (yyyy-mm-dd). Must be paired with `end`; passing only one returns empty results.
    pub start: Option<String>,
    /// End date (yyyy-mm-dd). Must be paired with `start`; passing only one returns empty results.
    pub end: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfitAnalysisDetailParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
    /// Start date (yyyy-mm-dd). Must be paired with `end`; passing only one returns empty results.
    pub start: Option<String>,
    /// End date (yyyy-mm-dd). Must be paired with `start`; passing only one returns empty results.
    pub end: Option<String>,
}

fn date_to_unix(s: &str, end_of_day: bool) -> Result<i64, McpError> {
    let date = time::Date::parse(s, time::macros::format_description!("[year]-[month]-[day]"))
        .map_err(|e| McpError::invalid_params(format!("invalid date '{s}': {e}"), None))?;
    let t = if end_of_day {
        time::Time::from_hms(23, 59, 59).expect("valid time")
    } else {
        time::Time::MIDNIGHT
    };
    Ok(time::PrimitiveDateTime::new(date, t)
        .assume_utc()
        .unix_timestamp())
}

pub async fn exchange_rate(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    if mctx.dc_region().await == DcRegion::Us {
        // `/v1/asset/exchange_rates` is not deployed in the US data center (it
        // fails with `code 12`); US accounts read the US account's own rates.
        let (ctx, _) = TradeContext::new(mctx.create_config());
        let result = ctx.us_exchange_rates().await.map_err(Error::longbridge)?;
        let exchanges: Vec<serde_json::Value> =
            result.exchanges.iter().map(us_exchange_rate_json).collect();
        return tool_json(&serde_json::json!({ "exchanges": exchanges }));
    }
    let client = mctx.create_http_client();
    // Longbridge's asset FX feed is a single mid-reference rate: `bid_rate` and
    // `offer_rate` always equal `average_rate` (verified across all supported
    // currencies), so they carry no spread information. Keep only `average_rate`.
    http_get_tool_dropping(
        &client,
        "/v1/asset/exchange_rates",
        &[],
        &["bid_rate", "offer_rate"],
    )
    .await
}

/// One US exchange rate as `exchange_rate` returns it: `bid_rate` /
/// `offer_rate` are dropped when they equal `average_rate`, matching the AP
/// path (where the feed is a single mid rate), and kept when they carry a
/// real spread.
fn us_exchange_rate_json(rate: &USExchangeRate) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "base_currency".to_string(),
        rate.base_currency.clone().into(),
    );
    obj.insert(
        "other_currency".to_string(),
        rate.other_currency.clone().into(),
    );
    obj.insert("average_rate".to_string(), rate.average_rate.clone().into());
    for (key, side) in [
        ("bid_rate", &rate.bid_rate),
        ("offer_rate", &rate.offer_rate),
    ] {
        if !side.is_empty() && !same_rate(side, &rate.average_rate) {
            obj.insert(key.to_string(), side.clone().into());
        }
    }
    obj.insert(
        "universal_key".to_string(),
        rate.universal_key.clone().into(),
    );
    obj.insert("spec_key".to_string(), rate.spec_key.clone().into());
    serde_json::Value::Object(obj)
}

/// Whether two rate strings name the same number ("7.80" and "7.8" do).
fn same_rate(a: &str, b: &str) -> bool {
    match (Decimal::from_str(a), Decimal::from_str(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub async fn profit_analysis(
    mctx: &crate::tools::McpContext,
    p: ProfitAnalysisParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();

    let start_ts = p
        .start
        .as_deref()
        .map(|s| date_to_unix(s, false))
        .transpose()?;
    let end_ts = p
        .end
        .as_deref()
        .map(|s| date_to_unix(s, true))
        .transpose()?;

    let start_str = start_ts.map(|v| v.to_string());
    let end_str = end_ts.map(|v| v.to_string());

    let mut summary_params: Vec<(&str, &str)> = Vec::new();
    let mut sublist_params: Vec<(&str, &str)> = vec![("profit_or_loss", "all")];

    if let Some(ref s) = start_str {
        summary_params.push(("start", s.as_str()));
        sublist_params.push(("start", s.as_str()));
    }
    if let Some(ref e) = end_str {
        summary_params.push(("end", e.as_str()));
        sublist_params.push(("end", e.as_str()));
    }

    let (summary_result, sublist_result) = tokio::join!(
        http_get_tool(
            &client,
            "/v1/portfolio/profit-analysis-summary",
            &summary_params
        ),
        http_get_tool(
            &client,
            "/v1/portfolio/profit-analysis-sublist",
            &sublist_params
        ),
    );

    let summary_text = summary_result?
        .content
        .into_iter()
        .next()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    let sublist_text = sublist_result?
        .content
        .into_iter()
        .next()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();

    let summary: serde_json::Value =
        serde_json::from_str(&summary_text).map_err(|e| Error::Other(e.to_string()))?;
    let sublist: serde_json::Value =
        serde_json::from_str(&sublist_text).map_err(|e| Error::Other(e.to_string()))?;

    let mut merged = match summary {
        serde_json::Value::Object(m) => m,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("data".to_owned(), other);
            map
        }
    };
    merged.insert("sublist".to_owned(), sublist);

    let mut value = serde_json::Value::Object(merged);
    convert_unix_paths(&mut value, &["start_time", "end_time", "trade_update_time"]);

    Ok(CallToolResult::success(vec![Content::text(
        value.to_string(),
    )]))
}

pub async fn profit_analysis_detail(
    mctx: &crate::tools::McpContext,
    p: ProfitAnalysisDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();

    let start_ts = p
        .start
        .as_deref()
        .map(|s| date_to_unix(s, false))
        .transpose()?;
    let end_ts = p
        .end
        .as_deref()
        .map(|s| date_to_unix(s, true))
        .transpose()?;

    let start_str = start_ts.map(|v| v.to_string());
    let end_str = end_ts.map(|v| v.to_string());

    let mut params: Vec<(&str, &str)> = vec![("symbol", p.symbol.as_str())];
    if let Some(ref s) = start_str {
        params.push(("start", s.as_str()));
    }
    if let Some(ref e) = end_str {
        params.push(("end", e.as_str()));
    }

    http_get_tool_unix(
        &client,
        "/v1/portfolio/profit-analysis/detail",
        &params,
        &["start", "end"],
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfitAnalysisRealizedParam {
    /// Currency to report in, e.g. "USD" (default: "USD"). US accounts only.
    pub currency: Option<String>,
    /// Filter by category: "STOCK", "OPTION", "CRYPTO", or omit for all.
    pub category: Option<String>,
}

/// Maps a friendly category name to the numeric-string code the backend
/// actually accepts. The SDK's own doc comment on `GetUSRealizedPLOptions`
/// claims `"STOCK"`/`"OPTION"`/`"CRYPTO"` are valid, but sending those
/// literal strings 500s in practice (confirmed against a real staging
/// account) — only the numeric codes ("1"/"2"/"3", ""/"0" = all) work.
/// Anything not recognized is passed through unchanged (covers callers who
/// already send a numeric code, and preserves forward-compatibility with
/// any new backend-defined value).
fn map_realized_pl_category(input: &str) -> String {
    match input.trim().to_ascii_uppercase().as_str() {
        "STOCK" => "1".to_string(),
        "OPTION" => "2".to_string(),
        "CRYPTO" => "3".to_string(),
        "ALL" => String::new(),
        _ => input.to_string(),
    }
}

/// Get realized profit-and-loss for a US account, broken down by category
/// (stock/option/crypto) and period. US-region accounts only — calling this
/// from a non-US account fails with a DcRegionRestricted error, since the
/// underlying SDK method is US-DC-restricted.
pub async fn profit_analysis_realized(
    mctx: &crate::tools::McpContext,
    p: ProfitAnalysisRealizedParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = longbridge::trade::TradeContext::new(mctx.create_config());
    let category = p
        .category
        .as_deref()
        .map(map_realized_pl_category)
        .unwrap_or_default();
    let opts = longbridge::trade::GetUSRealizedPLOptions {
        currency: p.currency.unwrap_or_else(|| "USD".to_string()),
        category,
    };
    let result = ctx.us_realized_pl(opts).await.map_err(Error::longbridge)?;
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    crate::tools::support::us_normalize::normalize_us_realized_pl(&mut value);
    tool_json(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(bid: &str, offer: &str, average: &str) -> USExchangeRate {
        USExchangeRate {
            universal_key: "us_lb_apex".to_string(),
            spec_key: "am".to_string(),
            base_currency: "USD".to_string(),
            other_currency: "HKD".to_string(),
            bid_rate: bid.to_string(),
            offer_rate: offer.to_string(),
            average_rate: average.to_string(),
        }
    }

    #[test]
    fn us_exchange_rate_drops_sides_equal_to_the_average() {
        let v = us_exchange_rate_json(&rate("7.80", "7.8", "7.8"));
        assert_eq!(v["average_rate"], "7.8", "average_rate is always kept");
        assert!(
            v.get("bid_rate").is_none(),
            "bid equal to average is dropped"
        );
        assert!(
            v.get("offer_rate").is_none(),
            "offer equal to average is dropped"
        );
        assert_eq!(v["base_currency"], "USD", "base currency is kept");
        assert_eq!(v["other_currency"], "HKD", "other currency is kept");
        assert_eq!(v["universal_key"], "us_lb_apex", "account channel is kept");
        assert_eq!(v["spec_key"], "am", "rate type is kept");
    }

    #[test]
    fn us_exchange_rate_keeps_a_real_spread() {
        let v = us_exchange_rate_json(&rate("7.79", "7.81", "7.8"));
        assert_eq!(v["bid_rate"], "7.79", "a bid off the average is kept");
        assert_eq!(v["offer_rate"], "7.81", "an offer off the average is kept");
    }

    #[test]
    fn us_exchange_rate_drops_missing_sides() {
        let v = us_exchange_rate_json(&rate("", "", "7.8"));
        assert!(v.get("bid_rate").is_none(), "an absent bid is not emitted");
        assert!(
            v.get("offer_rate").is_none(),
            "an absent offer is not emitted"
        );
    }

    #[test]
    fn map_realized_pl_category_translates_friendly_names_to_backend_codes() {
        assert_eq!(map_realized_pl_category("STOCK"), "1");
        assert_eq!(map_realized_pl_category("stock"), "1");
        assert_eq!(map_realized_pl_category("OPTION"), "2");
        assert_eq!(map_realized_pl_category("CRYPTO"), "3");
        assert_eq!(map_realized_pl_category("ALL"), "");
        assert_eq!(map_realized_pl_category(""), "");
    }

    #[test]
    fn map_realized_pl_category_passes_through_unrecognized_values() {
        // Callers who already send a numeric code, or any future
        // backend-defined value, must not be silently altered.
        assert_eq!(map_realized_pl_category("1"), "1");
        assert_eq!(map_realized_pl_category("something-else"), "something-else");
    }
}
