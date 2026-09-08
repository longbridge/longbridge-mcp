//! Typed output schemas for the signal tools.
//!
//! Signals come back fully typed from the SDK, so unlike most passthrough
//! tools these schemas are exhaustive rather than a documented subset.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;
use time::format_description::well_known::Rfc3339;

/// Direction a strategy expects the security to take.
///
/// `Unknown` is the upstream fallback: the API returned an outlook this server
/// does not model yet.
#[derive(Debug, Serialize, JsonSchema)]
pub enum SignalOutlook {
    #[serde(rename = "Strong bullish")]
    StrongBullish,
    Bullish,
    Neutral,
    Bearish,
    #[serde(rename = "Strong bearish")]
    StrongBearish,
    Unknown,
}

impl From<longbridge::signal::Outlook> for SignalOutlook {
    fn from(o: longbridge::signal::Outlook) -> Self {
        use longbridge::signal::Outlook;

        match o {
            Outlook::StrongBullish => Self::StrongBullish,
            Outlook::Bullish => Self::Bullish,
            Outlook::Neutral => Self::Neutral,
            Outlook::Bearish => Self::Bearish,
            Outlook::StrongBearish => Self::StrongBearish,
            Outlook::Unknown => Self::Unknown,
        }
    }
}

/// Where a signal is in its lifecycle.
///
/// `Active` is the only status the API serves today. `Unknown` is the upstream
/// fallback: the API returned a status this server does not model yet.
#[derive(Debug, Serialize, JsonSchema)]
pub enum SignalStatus {
    Pending,
    Active,
    Deleted,
    AiFailed,
    FilteredByManual,
    AiSubmitFailed,
    Unknown,
}

impl From<longbridge::signal::SignalStatus> for SignalStatus {
    fn from(s: longbridge::signal::SignalStatus) -> Self {
        use longbridge::signal::SignalStatus as Sdk;

        match s {
            Sdk::Pending => Self::Pending,
            Sdk::Active => Self::Active,
            Sdk::Deleted => Self::Deleted,
            Sdk::AiFailed => Self::AiFailed,
            Sdk::FilteredByManual => Self::FilteredByManual,
            Sdk::AiSubmitFailed => Self::AiSubmitFailed,
            Sdk::Unknown => Self::Unknown,
        }
    }
}

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
    /// Display label of the catalyst that triggered the signal, e.g.
    /// "Q1 Revenue Surge". This is prose for display — the `catalyst_name`
    /// filter on `signals` matches the underlying factor name instead.
    pub key_catalyst: String,
    /// Price the analysis was based on.
    pub analysis_price: f64,
    /// Conservative-scenario target price.
    pub conservative_price: f64,
    /// Benchmark-scenario target price.
    pub benchmark_price: f64,
    /// Optimistic-scenario target price.
    pub optimistic_price: f64,
    /// Outlook the strategy takes on the security.
    pub outlook: SignalOutlook,
    /// Localized outlook label.
    pub outlook_desc: String,
    /// Where the signal is in its lifecycle.
    pub status: SignalStatus,
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
            outlook: s.outlook.into(),
            outlook_desc: s.outlook_desc,
            status: s.status.into(),
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
    pub total: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire values are the schema's `enum` constraint and part of the tool
    /// contract, so they are pinned rather than derived from variant names.
    #[test]
    fn outlook_and_status_serialize_to_their_documented_labels() {
        let cases = [
            (SignalOutlook::StrongBullish, "\"Strong bullish\""),
            (SignalOutlook::Bullish, "\"Bullish\""),
            (SignalOutlook::Neutral, "\"Neutral\""),
            (SignalOutlook::Bearish, "\"Bearish\""),
            (SignalOutlook::StrongBearish, "\"Strong bearish\""),
            (SignalOutlook::Unknown, "\"Unknown\""),
        ];
        for (value, expected) in cases {
            assert_eq!(
                serde_json::to_string(&value).expect("outlook must serialize"),
                expected,
                "outlook wire value must stay stable"
            );
        }

        assert_eq!(
            serde_json::to_string(&SignalStatus::FilteredByManual).expect("status must serialize"),
            "\"FilteredByManual\"",
            "status wire value must stay stable"
        );
        assert_eq!(
            serde_json::to_string(&SignalStatus::Unknown).expect("status must serialize"),
            "\"Unknown\"",
            "the upstream fallback must be representable"
        );
    }

    #[test]
    fn output_schema_constrains_outlook_and_status() {
        let schema = serde_json::to_value(rmcp::schemars::schema_for!(SignalItem))
            .expect("schema must serialize");
        let defs = &schema["$defs"];
        assert_eq!(
            defs["SignalOutlook"]["enum"],
            serde_json::json!([
                "Strong bullish",
                "Bullish",
                "Neutral",
                "Bearish",
                "Strong bearish",
                "Unknown"
            ]),
            "the schema must constrain outlook to its documented labels"
        );
        assert_eq!(
            defs["SignalStatus"]["enum"],
            serde_json::json!([
                "Pending",
                "Active",
                "Deleted",
                "AiFailed",
                "FilteredByManual",
                "AiSubmitFailed",
                "Unknown"
            ]),
            "the schema must constrain status to its documented labels"
        );
    }
}
