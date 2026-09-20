//! Strategy-signal tools. Wrap the SDK `longbridge::signal::SignalContext` —
//! one `*Param` struct + one async fn per tool.

use longbridge::signal::{SecurityFactsOptions, SignalContext, SignalsOptions};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::output::fact::{SecurityFactItem, SecurityFactsResponse};
use crate::tools::output::signal::{SignalItem, SignalsResponse};
use crate::tools::support::parse::parse_rfc3339;
use crate::tools::support::tolerant::tolerant_option_i32;
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SignalsParam {
    /// Filter by security symbol, e.g. "AAPL.US" or "700.HK". If omitted, returns signals for all symbols.
    pub symbol_name: Option<String>,
    /// Filter by strategy id (e.g., "buffett-value"). Preferred over the deprecated strategy_name; takes precedence when both are provided.
    pub strategy_id: Option<String>,
    /// Filter by strategy name. If omitted, returns signals from all strategies.
    pub strategy_name: Option<String>,
    /// Filter by the name of the factor that triggered the signal, e.g. "EARNINGS_RELEASED" or "macd_12_26_9" — not the display label returned in key_catalyst. If omitted, signals with any catalyst name are returned.
    pub catalyst_name: Option<String>,
    /// Filter by the catalyst type that triggered the signal, e.g. "News", "Fundamental", "Technical". If omitted, signals with any catalyst type are returned.
    pub catalyst_type: Option<String>,
    /// Filter records created at or after this time. ISO 8601 datetime with timezone, e.g. 2024-01-15T10:30:00Z. If omitted, no lower bound.
    pub start_time: Option<String>,
    /// Filter records created at or before this time. ISO 8601 datetime with timezone. If omitted, no upper bound.
    pub end_time: Option<String>,
    /// Maximum number of results to return. Defaults to 20.
    #[serde(default, deserialize_with = "tolerant_option_i32")]
    #[schemars(extend("default" = 20))]
    pub limit: Option<i32>,
    /// Number of results to skip for pagination. Defaults to 0.
    #[serde(default, deserialize_with = "tolerant_option_i32")]
    #[schemars(extend("default" = 0))]
    pub offset: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SignalIdParam {
    /// Signal ID, e.g. "sign_992_1a00c9425c3_48ab". Get IDs from `signals`.
    pub signal_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SecurityFactsParam {
    /// Security symbol to query, e.g. "AAPL.US" or "700.HK".
    pub symbol: String,
    /// The optional start time of the fact query, formatted as 2006-01-02T15:04:05Z in UTC Timezone. If left empty, the query will include the earliest available data.
    pub begin_time: Option<String>,
    /// The end time of the fact to be queried, formatted as 2006-01-02T15:04:05Z in UTC Timezone. If left empty, the query will default to retrieving the latest data.
    pub end_time: Option<String>,
    /// The maximum number of facts to return. If the number of facts in the time range exceeds this limit, only the latest 'limit' facts will be returned. Defaults to 100.
    #[serde(default, deserialize_with = "tolerant_option_i32")]
    #[schemars(extend("default" = 100))]
    pub limit: Option<i32>,
}

/// `GET /v1/signals` — a page of signals, without the per-signal analysis
/// document (several KB each); `signal_detail` carries that.
pub async fn signals(
    mctx: &crate::tools::McpContext,
    p: SignalsParam,
) -> Result<CallToolResult, McpError> {
    let ctx = SignalContext::new(mctx.create_config());
    let resp = ctx
        .signals(SignalsOptions {
            symbol_name: p.symbol_name,
            strategy_id: p.strategy_id,
            strategy_name: p.strategy_name,
            catalyst_name: p.catalyst_name,
            catalyst_type: p.catalyst_type,
            start_time: p.start_time.as_deref().map(parse_rfc3339).transpose()?,
            end_time: p.end_time.as_deref().map(parse_rfc3339).transpose()?,
            limit: p.limit,
            offset: p.offset,
        })
        .await
        .map_err(Error::longbridge)?;

    tool_json(&SignalsResponse {
        signals: resp.signals.into_iter().map(SignalItem::from).collect(),
        total: resp.total,
    })
}

/// Several signal fields arrive as a JSON document embedded in a string. Hand
/// them to the caller as real JSON so they can be read without a second parse;
/// a payload that does not parse is passed through as the original string
/// rather than dropped.
fn unwrap_embedded_json(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_owned()))
}

/// The embedded analysis document repeats the signal's `summary` and `title`
/// verbatim inside `analysis.signal`; the canonical copies already sit at the
/// top level, so drop the nested duplicates. The summary alone runs to ~1 KB of
/// Markdown, so this is the largest single redundancy in a `signal_detail`
/// payload. `outlook_desc` is deliberately kept — it is the *localized* outlook
/// label (e.g. "强烈看多"), not a verbatim copy of the English `outlook` enum.
fn drop_analysis_duplicates(item: &mut serde_json::Value) {
    if let Some(signal) = item
        .get_mut("analysis")
        .and_then(|a| a.get_mut("signal"))
        .and_then(serde_json::Value::as_object_mut)
    {
        signal.remove("summary");
        signal.remove("title");
    }
}

/// `GET /v1/signals/{signal_id}` — one signal with its full analysis.
pub async fn signal_detail(
    mctx: &crate::tools::McpContext,
    p: SignalIdParam,
) -> Result<CallToolResult, McpError> {
    let ctx = SignalContext::new(mctx.create_config());
    let signal = ctx.signal(p.signal_id).await.map_err(Error::longbridge)?;

    let analysis = unwrap_embedded_json(&signal.json_data);
    let mut item = SignalItem::from(signal);
    item.analysis = Some(analysis);
    let mut value = serde_json::to_value(&item).map_err(Error::Serialize)?;
    drop_analysis_duplicates(&mut value);
    tool_json(&value)
}

/// `GET /v1/facts/security_facts` — the fact (catalyst) events behind signals.
pub async fn security_facts(
    mctx: &crate::tools::McpContext,
    p: SecurityFactsParam,
) -> Result<CallToolResult, McpError> {
    let ctx = SignalContext::new(mctx.create_config());
    let facts = ctx
        .security_facts(SecurityFactsOptions {
            symbol: p.symbol,
            begin_time: p.begin_time.as_deref().map(parse_rfc3339).transpose()?,
            end_time: p.end_time.as_deref().map(parse_rfc3339).transpose()?,
            limit: p.limit,
        })
        .await
        .map_err(Error::longbridge)?;

    tool_json(&SecurityFactsResponse {
        facts: facts.into_iter().map(SecurityFactItem::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_is_unwrapped_into_json() {
        let v = unwrap_embedded_json(r#"{"total_score":81,"outlook":"Bearish"}"#);
        assert_eq!(v["total_score"], 81, "embedded document must be parsed");
    }

    #[test]
    fn unparsable_analysis_is_kept_as_a_string() {
        let v = unwrap_embedded_json("not json");
        assert_eq!(
            v,
            serde_json::Value::String("not json".into()),
            "an unparsable payload must survive rather than be dropped"
        );
    }

    #[test]
    fn drop_analysis_duplicates_removes_nested_summary_and_title_only() {
        let mut value = serde_json::json!({
            "summary": "canonical summary",
            "title": "canonical title",
            "outlook": "Bullish",
            "outlook_desc": "强烈看多",
            "analysis": {
                "confidence": "high",
                "signal": {
                    "summary": "canonical summary",
                    "title": "canonical title",
                    "outlook_desc": "强烈看多",
                    "strategy_fit": 88
                }
            }
        });
        drop_analysis_duplicates(&mut value);

        let signal = &value["analysis"]["signal"];
        assert!(
            signal.get("summary").is_none(),
            "the nested verbatim summary duplicate must be dropped"
        );
        assert!(
            signal.get("title").is_none(),
            "the nested verbatim title duplicate must be dropped"
        );
        // Distinct nested data survives.
        assert_eq!(
            signal["strategy_fit"], 88,
            "unrelated nested fields must be preserved"
        );
        // Top-level canonical copies survive.
        assert_eq!(
            value["summary"], "canonical summary",
            "the top-level summary must be kept"
        );
        assert_eq!(
            value["title"], "canonical title",
            "the top-level title must be kept"
        );
        // The localized outlook label is kept, not treated as a duplicate.
        assert_eq!(
            value["outlook_desc"], "强烈看多",
            "the localized outlook label must be preserved"
        );
        assert_eq!(
            value["analysis"]["signal"]["outlook_desc"], "强烈看多",
            "the nested localized outlook label must be preserved"
        );
    }
}
