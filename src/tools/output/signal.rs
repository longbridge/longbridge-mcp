//! Typed output schemas for the signal tools.
//!
//! Signals come back fully typed from the SDK, so unlike most passthrough
//! tools these schemas are exhaustive rather than a documented subset.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;
use time::format_description::well_known::Rfc3339;

/// One strategy signal.
///
/// `analysis` is only present on `signal_detail`; the list view omits it
/// because the analysis document runs to several KB per signal.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SignalItem {
    /// Signal ID, e.g. "sign_992_1a00c9425c3_48ab". Pass it to `signal_detail`.
    pub id: String,
    /// Security symbol, e.g. "992.HK".
    pub symbol: String,
    /// Company name.
    pub company_name: String,
    /// Market the security trades in, e.g. "HK".
    pub market: String,
    /// Signal headline.
    pub title: String,
    /// Natural-language summary of the signal, in Markdown.
    pub summary: String,
    /// Strategy ID that produced the signal.
    pub strategy_id: String,
    /// Strategy name that produced the signal.
    pub strategy_name: String,
    /// Who recommended the signal; empty for strategy-generated signals.
    pub recommend_by: String,
    /// Strategy expression, e.g. "992.HK:GROWTH:long".
    pub expression: String,
    /// ID of the fact that triggered the signal — look it up with
    /// `security_facts`.
    pub key_fact_id: String,
    /// Display name of the catalyst that triggered the signal.
    pub key_catalyst: String,
    /// Price the analysis was based on.
    pub analysis_price: f64,
    /// Conservative-scenario target price.
    pub conservative_price: f64,
    /// Benchmark-scenario target price.
    pub benchmark_price: f64,
    /// Optimistic-scenario target price.
    pub optimistic_price: f64,
    /// Outlook the strategy takes on the security: "Strong bullish",
    /// "Bullish", "Neutral", "Bearish" or "Strong bearish".
    pub outlook: String,
    /// Localized outlook label.
    pub outlook_desc: String,
    /// Risk level, e.g. "R4".
    pub risk_level: String,
    /// Signal status.
    pub status: i32,
    /// Creation time (RFC3339).
    pub created_at: String,
    /// Last update time (RFC3339).
    pub updated_at: String,
    /// Full strategy analysis: fit scores, valuation scenarios, evidence
    /// sources and related fact IDs. Only returned by `signal_detail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<serde_json::Value>,
}

impl From<longbridge::signal::Signal> for SignalItem {
    fn from(s: longbridge::signal::Signal) -> Self {
        Self {
            id: s.id,
            symbol: s.symbol,
            company_name: s.company_name,
            market: s.market,
            title: s.title,
            summary: s.summary,
            strategy_id: s.strategy_id,
            strategy_name: s.strategy_name,
            recommend_by: s.recommend_by,
            expression: s.expression,
            key_fact_id: s.key_fact_id,
            key_catalyst: s.key_catalyst,
            analysis_price: s.analysis_price,
            conservative_price: s.conservative_price,
            benchmark_price: s.benchmark_price,
            optimistic_price: s.optimistic_price,
            outlook: s.outlook.to_string(),
            outlook_desc: s.outlook_desc,
            risk_level: s.risk_level,
            status: s.status,
            created_at: s.created_at.format(&Rfc3339).unwrap_or_default(),
            updated_at: s.updated_at.format(&Rfc3339).unwrap_or_default(),
            analysis: None,
        }
    }
}

/// Returned by `signals`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SignalsResponse {
    /// Signals on this page, newest first.
    pub signals: Vec<SignalItem>,
    /// Total number of signals matching the filters — page through the rest
    /// with `offset`.
    pub total: i64,
}
