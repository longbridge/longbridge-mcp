use longbridge::httpclient::HttpClient;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::serialize::{convert_unix_paths, strip_empty_strings, strip_nulls};
use crate::tools::support::exchange_time::localize_tool_output;
use crate::tools::support::http_client::{http_get_tool, http_get_tool_unix};
use crate::tools::support::parse;

fn make_result(json: String) -> CallToolResult {
    // Cap bogus decimal precision (e.g. investor `capital_ratio` values like
    // "32.03116022394741") at 6 dp. When the payload parses, the rounded value
    // is authoritative for both the text and structured content; a payload that
    // does not parse is passed through unchanged.
    match serde_json::from_str::<serde_json::Value>(&json) {
        Ok(mut value) => {
            crate::serialize::round_decimals(&mut value, 6);
            let json = serde_json::to_string(&value).unwrap_or(json);
            let mut result = CallToolResult::success(vec![rmcp::model::ContentBlock::text(json)]);
            result.structured_content = Some(value);
            result
        }
        Err(_) => CallToolResult::success(vec![rmcp::model::ContentBlock::text(json)]),
    }
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

/// One market's half of a combined HK + US IPO list.
///
/// A failed market is kept as the reason instead of failing the whole call:
/// the other market's list is still useful. The US IPO endpoints have rejected
/// tokens the HK ones accept (403308) and return the odd 500, which used to
/// throw away a complete HK list.
async fn ipo_market(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
) -> Result<serde_json::Value, McpError> {
    let result = http_get_tool(client, path, params).await?;
    Ok(serde_json::from_str(get_json(&result)).unwrap_or(serde_json::Value::Null))
}

/// Combine the HK and US halves into `{hk, us}`, with `unavailable` naming the
/// market that failed. Both failing is an error: there is nothing to show.
fn combine_ipo_markets(
    tool: &str,
    hk: Result<serde_json::Value, McpError>,
    us: Result<serde_json::Value, McpError>,
) -> Result<serde_json::Value, McpError> {
    let mut unavailable = serde_json::Map::new();
    let mut side = |market: &str, part: Result<serde_json::Value, McpError>| match part {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(
                tool,
                market,
                "IPO market fetch failed; returning the other market"
            );
            tracing::info!(
                target: "longbridge_mcp::tools::error_detail",
                tool,
                market,
                error = %err.message,
                "IPO market fetch error detail"
            );
            // A public market list: don't relay auth codes that read as "the
            // user must re-authorize".
            unavailable.insert(
                market.to_string(),
                serde_json::Value::String(format!(
                    "{} IPO data is temporarily unavailable; the other market's list is complete.",
                    market.to_uppercase()
                )),
            );
            serde_json::Value::Null
        }
    };
    let (hk, us) = match (hk, us) {
        (Err(err), Err(_)) => return Err(err),
        (hk, us) => (side("hk", hk), side("us", us)),
    };
    let mut value = serde_json::json!({ "hk": hk, "us": us });
    if !unavailable.is_empty() {
        value["unavailable"] = serde_json::Value::Object(unavailable);
    }
    Ok(value)
}

/// List IPO stocks currently in subscription or pre-filing stage (HK and US).
pub async fn ipo_subscriptions(
    mctx: &crate::tools::McpContext,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let (hk, us) = tokio::join!(
        ipo_market(&client, "/v1/ipo/subscriptions", &[]),
        ipo_market(&client, "/v1/us/ipo/subscriptions", &[]),
    );
    let mut value = combine_ipo_markets("ipo_subscriptions", hk, us)?;
    // Each entry carries an `icon` display URL, an app-page `can_subscribe` flag
    // (models relayed it to users verbatim) and empty stock_desc/order_method/
    // declaration strings; drop them.
    crate::serialize::drop_keys(&mut value, &["icon", "can_subscribe"]);
    crate::serialize::strip_empty_strings(&mut value);
    // `sub_deadline` is the subscription cut-off as raw unix seconds.
    crate::serialize::convert_unix_paths(
        &mut value,
        &["hk.list.*.sub_deadline", "us.list.*.sub_deadline"],
    );
    Ok(make_result(value.to_string()))
}

/// Show the IPO calendar (all upcoming and recent IPOs).
pub async fn ipo_calendar(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    // Besides the top-level `timestamp`, every per-IPO milestone date
    // (subscription window, allotment, dark-pool, listing) arrives as a raw
    // unix-seconds number a model cannot interpret; convert them to RFC3339.
    http_get_tool_unix(
        &client,
        "/v1/ipo/calendar",
        &[],
        &[
            "timestamp",
            "list.*.sub_date",
            "list.*.sub_end_date",
            "list.*.result_date",
            "list.*.mart_date",
            "list.*.ipo_date",
        ],
    )
    .await
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
    let (hk, us) = tokio::join!(
        ipo_market(&client, "/v1/ipo/listed", &params),
        ipo_market(&client, "/v1/us/ipo/listed", &params),
    );
    let mut value = combine_ipo_markets("ipo_listed", hk, us)?;
    // Each entry echoes `market` (already implied by the hk/us grouping key) and
    // an `icon` display URL; drop both.
    crate::serialize::drop_keys(&mut value, &["market", "icon"]);
    Ok(make_result(value.to_string()))
}

/// Show IPO detail: profile + timeline for a symbol.
pub async fn ipo_detail(
    mctx: &crate::tools::McpContext,
    p: IpoDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market = p.market.unwrap_or_else(|| {
        p.symbol
            .rsplit_once('.')
            .map_or("HK", |(_, m)| m)
            .to_string()
    });
    let profile_params = [("symbol", p.symbol.as_str())];
    let timeline_params = [
        ("symbol", p.symbol.as_str()),
        ("market", market.as_str()),
        ("flag", "0"),
    ];
    let (profile, timeline) = tokio::join!(
        http_get_tool(&client, "/v1/ipo/profile", &profile_params),
        http_get_tool(&client, "/v1/ipo/timeline", &timeline_params),
    );
    let parse = |r: &CallToolResult| {
        serde_json::from_str::<serde_json::Value>(get_json(r)).unwrap_or(serde_json::Value::Null)
    };
    let value = shape_ipo_detail(&p.symbol, &market, parse(&profile?), parse(&timeline?));
    Ok(make_result(value.to_string()))
}

/// Profile keys that only drive the app's IPO page: display toggles, empty
/// identifiers, and a `can_subscribe` flag that disagrees with the timeline's.
const IPO_PROFILE_UI_KEYS: &[&str] =
    &["can_subscribe", "show_mart", "counter_id", "symbol", "tags"];

/// Profile fields that arrive as raw unix seconds.
const IPO_PROFILE_TIME_KEYS: &[&str] = &["ipo_date", "mart_begin", "mart_end"];

/// Reshape the raw profile + timeline into what a reader needs.
///
/// The profile comes back as `{hk, us}` with only the symbol's own market
/// filled; keep that one. Drop the app-page flags (`can_subscribe` appears
/// three times upstream — profile, timeline and the eligibility endpoint — and
/// the three disagree, e.g. `true` for an IPO that already listed), turn the
/// unix-second dates into exchange-local RFC3339, and give the timeline steps
/// readable names: `light` marks a step already reached.
fn shape_ipo_detail(
    symbol: &str,
    market: &str,
    profile: serde_json::Value,
    timeline: serde_json::Value,
) -> serde_json::Value {
    let mut profile = profile
        .get(market.to_lowercase())
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if let Some(obj) = profile.as_object_mut() {
        for key in IPO_PROFILE_UI_KEYS {
            obj.remove(*key);
        }
        // "0M" is the upstream placeholder for an unknown share count.
        obj.retain(|_, v| v.as_str() != Some("0M"));
    }
    convert_unix_paths(&mut profile, IPO_PROFILE_TIME_KEYS);
    localize_tool_output("ipo_detail", &mut profile, Some(symbol));

    let steps: Vec<serde_json::Value> = timeline
        .get("timeline")
        .and_then(serde_json::Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .map(|step| {
                    serde_json::json!({
                        "event": step.get("name"),
                        "time": step
                            .get("time")
                            .and_then(serde_json::Value::as_str)
                            .map(|t| t.replace('\n', " ")),
                        "reached": step.get("light"),
                        "desc": step.get("desc"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut value = serde_json::json!({
        "symbol": symbol,
        "market": market.to_uppercase(),
        "profile": profile,
        "timeline": {
            "steps": steps,
            "pay_end_date": timeline.get("pay_end_date"),
            "tags": timeline.get("tags"),
        },
    });
    strip_empty_strings(&mut value);
    strip_nulls(&mut value);
    drop_empty_arrays(&mut value);
    value
}

/// Remove `[]` values (no investors, no tags, ...) from every object.
fn drop_empty_arrays(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !matches!(v, serde_json::Value::Array(a) if a.is_empty()));
            map.values_mut().for_each(drop_empty_arrays);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(drop_empty_arrays),
        _ => {}
    }
}

/// List IPO orders (active + history) for the current account.
pub async fn ipo_orders(
    mctx: &crate::tools::McpContext,
    p: IpoOrdersParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();

    let mut active_params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref sym) = p.symbol {
        active_params.push(("symbol", sym.as_str()));
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
    let order_id = parse::path_segment("order_id", &p.order_id)?;
    let path = format!("/v1/ipo/orders/{order_id}");
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

#[cfg(test)]
mod shape_tests {
    use super::*;

    fn denied() -> McpError {
        McpError::internal_error(
            "openapi error: code=403308: Target API's scope is not in authorized scopes",
            None,
        )
    }

    #[test]
    fn one_failed_market_keeps_the_other() {
        let hk = Ok(serde_json::json!({"list": [{"symbol": "3275.HK"}]}));
        let value = combine_ipo_markets("ipo_listed", hk, Err(denied()))
            .expect("one market succeeding is not an error");
        assert_eq!(
            value["hk"]["list"][0]["symbol"], "3275.HK",
            "the HK list survives"
        );
        assert!(value["us"].is_null(), "the failed market is null");
        let note = value["unavailable"]["us"]
            .as_str()
            .expect("the failure is named");
        assert!(
            !note.contains("403308") && !note.to_lowercase().contains("authoriz"),
            "the note must not read as an auth problem: {note}"
        );
        assert!(
            value["unavailable"].get("hk").is_none(),
            "HK is not listed as unavailable"
        );
    }

    #[test]
    fn both_markets_failing_is_an_error() {
        let result = combine_ipo_markets("ipo_listed", Err(denied()), Err(denied()));
        assert!(result.is_err(), "nothing to show when both markets fail");
    }

    #[test]
    fn ipo_detail_drops_app_flags_and_localizes_dates() {
        let profile = serde_json::json!({
            "hk": {
                "ipo_date": 1796313600, "mart_begin": 1796285700, "mart_end": 1796293800,
                "show_mart": true, "can_subscribe": 1, "counter_id": "", "symbol": "",
                "tags": [], "investors": [], "share_planned": "", "total_shares": "0M",
                "issue_price": "12.070 ～ 15.690", "trade_unit": 500
            },
            "us": null
        });
        let timeline = serde_json::json!({
            "timeline": [
                {"name": "开始认购", "light": true, "time": "11.23\n09:00", "desc": ""},
                {"name": "上市", "light": false, "time": "12.04", "desc": ""}
            ],
            "can_subscribe": true, "begin": true, "pay_end_date": "2026.12.01 09:00",
            "tags": ["银行融资利率 0%"], "issue_price": "12.070 ～ 15.690"
        });
        let v = shape_ipo_detail("3275.HK", "HK", profile, timeline);
        let p = &v["profile"];
        for key in [
            "show_mart",
            "can_subscribe",
            "counter_id",
            "symbol",
            "tags",
            "investors",
        ] {
            assert!(p.get(key).is_none(), "profile.{key} must be dropped");
        }
        assert!(
            p.get("total_shares").is_none(),
            "the 0M placeholder is dropped"
        );
        assert_eq!(
            p["ipo_date"], "2026-12-04T00:00:00+08:00",
            "listing date in HK time"
        );
        assert_eq!(
            p["mart_begin"], "2026-12-03T16:15:00+08:00",
            "grey market start in HK time"
        );
        assert_eq!(v["market"], "HK", "the requested market is named");
        let steps = v["timeline"]["steps"].as_array().expect("steps");
        assert_eq!(steps[0]["event"], "开始认购", "step name");
        assert_eq!(
            steps[0]["time"], "11.23 09:00",
            "the newline becomes a space"
        );
        assert_eq!(steps[0]["reached"], true, "light becomes reached");
        assert!(steps[0].get("desc").is_none(), "an empty desc is dropped");
        let t = &v["timeline"];
        assert!(
            t.get("can_subscribe").is_none() && t.get("begin").is_none(),
            "timeline flags dropped"
        );
        assert_eq!(t["pay_end_date"], "2026.12.01 09:00", "pay_end_date kept");
    }
}
