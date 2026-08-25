//! Strategy-signal tools. Wrap the SDK `longbridge::signal::SignalContext` —
//! one `*Param` struct + one async fn per tool.

use longbridge::signal::{SecurityFactsOptions, SignalContext, SignalsOptions};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
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
    pub limit: Option<i32>,
    /// Number of results to skip for pagination. Defaults to 0.
    #[serde(default, deserialize_with = "tolerant_option_i32")]
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

/// Unwrap the `{tag, value}` documents `nl_info.summary`, `invest_anal` and
/// `eli_explain` carry as strings, in place.
fn unwrap_fact_nl_info(fact: &mut serde_json::Value) {
    let Some(nl_info) = fact.get_mut("nl_info") else {
        return;
    };
    for field in ["summary", "invest_anal", "eli_explain"] {
        if let Some(raw) = nl_info.get(field).and_then(|v| v.as_str()) {
            let unwrapped = unwrap_embedded_json(raw);
            nl_info[field] = unwrapped;
        }
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
    tool_json(&item)
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

    let mut facts = serde_json::to_value(facts).map_err(Error::Serialize)?;
    if let Some(list) = facts.as_array_mut() {
        for fact in list {
            unwrap_fact_nl_info(fact);
        }
    }
    tool_json(&serde_json::json!({ "facts": facts }))
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
    fn fact_nl_info_documents_are_unwrapped() {
        let mut fact = serde_json::json!({
            "fact_id": "technical_rsi_14_short_1",
            "nl_info": {
                "title": "RSI_14",
                "summary": r#"[{"tag":"RSI","value":"balanced"}]"#,
                "invest_anal": "not json",
            }
        });
        unwrap_fact_nl_info(&mut fact);
        assert_eq!(
            fact["nl_info"]["summary"][0]["tag"], "RSI",
            "summary must become a real array"
        );
        assert_eq!(
            fact["nl_info"]["invest_anal"], "not json",
            "an unparsable field must survive as its original string"
        );
    }
}
