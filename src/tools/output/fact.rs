//! Typed output schema for `security_facts`.
//!
//! Facts come back fully typed from the SDK, so like the signal schemas these
//! are exhaustive rather than a documented subset. The one place the upstream
//! shape is not self-describing is [`FactNlInfo`], whose prose fields arrive as
//! a JSON document embedded in a string; [`NlField`] unwraps them.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;
use time::format_description::well_known::Rfc3339;

/// Kind of fact, and of the source that produced it.
///
/// `Unknown` is the upstream fallback: the API returned a kind this server does
/// not model yet.
#[derive(Debug, Serialize, JsonSchema)]
pub enum FactType {
    News,
    Fundamental,
    Technical,
    Unknown,
}

impl From<longbridge::signal::FactType> for FactType {
    fn from(t: longbridge::signal::FactType) -> Self {
        use longbridge::signal::FactType as Sdk;

        match t {
            Sdk::News => Self::News,
            Sdk::Fundamental => Self::Fundamental,
            Sdk::Technical => Self::Technical,
            Sdk::Unknown => Self::Unknown,
        }
    }
}

/// Side a fact or factor points to.
///
/// The empty string is the upstream fallback: the API stated no side.
#[derive(Debug, Serialize, JsonSchema)]
pub enum FactDirection {
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
    #[serde(rename = "neutral")]
    Neutral,
    #[serde(rename = "")]
    Unknown,
}

impl From<longbridge::signal::FactDirection> for FactDirection {
    fn from(d: longbridge::signal::FactDirection) -> Self {
        use longbridge::signal::FactDirection as Sdk;

        match d {
            Sdk::Long => Self::Long,
            Sdk::Short => Self::Short,
            Sdk::Neutral => Self::Neutral,
            Sdk::Unknown => Self::Unknown,
        }
    }
}

/// One `{tag, value}` entry from a natural-language field.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NlTag {
    /// What the entry is about, e.g. "RSI".
    pub tag: String,
    /// The prose.
    pub value: String,
}

/// A natural-language field, which upstream carries as a JSON array of
/// `{tag, value}` entries embedded in a string.
///
/// Unwrapped into real entries so callers do not have to parse a second time.
/// A payload that does not parse is passed through as its original string
/// rather than dropped.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum NlField {
    /// The parsed entries.
    Tags(Vec<NlTag>),
    /// The raw text, for a payload that did not parse.
    Raw(String),
}

impl NlField {
    fn parse(raw: &str) -> Self {
        match serde_json::from_str::<Vec<longbridge::signal::NlTag>>(raw) {
            Ok(tags) => Self::Tags(
                tags.into_iter()
                    .map(|t| NlTag {
                        tag: t.tag,
                        value: t.value,
                    })
                    .collect(),
            ),
            Err(_) => Self::Raw(raw.to_owned()),
        }
    }
}

/// Natural-language rendering of a fact, in the caller's language.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FactNlInfo {
    /// Headline.
    pub title: String,
    /// Sub-headline.
    pub sub_title: String,
    /// What happened.
    pub summary: NlField,
    /// What it may mean for an investor.
    pub invest_anal: NlField,
    /// A plain-language walk-through of the fact.
    pub eli_explain: NlField,
}

impl From<longbridge::signal::FactNlInfo> for FactNlInfo {
    fn from(n: longbridge::signal::FactNlInfo) -> Self {
        Self {
            title: n.title,
            sub_title: n.sub_title,
            summary: NlField::parse(&n.summary),
            invest_anal: NlField::parse(&n.invest_anal),
            eli_explain: NlField::parse(&n.eli_explain),
        }
    }
}

/// Where a fact came from.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FactDataSource {
    /// Source name, e.g. "Nasdaq".
    pub source_name: String,
    /// Kind of source.
    #[serde(rename = "type")]
    pub source_type: FactType,
    /// Link to the source; empty when it has none.
    pub url: String,
    /// Source icon URL; empty when it has none.
    pub icon: String,
}

impl From<longbridge::signal::FactDataSource> for FactDataSource {
    fn from(s: longbridge::signal::FactDataSource) -> Self {
        Self {
            source_name: s.source_name,
            source_type: s.source_type.into(),
            url: s.url,
            icon: s.icon,
        }
    }
}

/// Thresholds an anomaly test was scored against.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AnomalyThresholds {
    /// Low threshold.
    pub low: String,
    /// Medium threshold.
    pub medium: String,
    /// High threshold.
    pub high: String,
}

impl From<longbridge::signal::AnomalyThresholds> for AnomalyThresholds {
    fn from(t: longbridge::signal::AnomalyThresholds) -> Self {
        Self {
            low: t.low,
            medium: t.medium,
            high: t.high,
        }
    }
}

/// Outcome of the anomaly test behind a factor. Fields are empty for factors
/// that did not run one.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AnomalyDetection {
    /// Test outcome.
    pub anomaly_result: String,
    /// Significance level of the test.
    pub significance_level: String,
    /// Method used, e.g. a statistical test name.
    pub test_method: String,
    /// Thresholds the result was scored against.
    pub thresholds: AnomalyThresholds,
}

impl From<longbridge::signal::AnomalyDetection> for AnomalyDetection {
    fn from(d: longbridge::signal::AnomalyDetection) -> Self {
        Self {
            anomaly_result: d.anomaly_result,
            significance_level: d.significance_level,
            test_method: d.test_method,
            thresholds: d.thresholds.into(),
        }
    }
}

/// One factor that contributed to a fact.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FactFactor {
    /// Factor name, e.g. "rsi_14". This is what the `catalyst_name` filter on
    /// `signals` matches.
    pub name: String,
    /// Groups the factor belongs to, e.g. "MOMENTUM".
    pub factor_groups: Vec<String>,
    /// Side the factor points to.
    pub long_short_direction: FactDirection,
    /// Condition that fired the factor.
    pub trigger_condition: String,
    /// Anomaly test behind the factor.
    pub anomaly_detection: AnomalyDetection,
}

impl From<longbridge::signal::FactFactor> for FactFactor {
    fn from(f: longbridge::signal::FactFactor) -> Self {
        Self {
            name: f.name,
            factor_groups: f.factor_groups,
            long_short_direction: f.long_short_direction.into(),
            trigger_condition: f.trigger_condition,
            anomaly_detection: f.anomaly_detection.into(),
        }
    }
}

/// A security a fact is about.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FactSymbol {
    /// Security symbol, e.g. "AAPL.US".
    pub symbol: String,
    /// Security name in the caller's language.
    pub security_name: String,
}

impl From<longbridge::signal::FactSymbol> for FactSymbol {
    fn from(s: longbridge::signal::FactSymbol) -> Self {
        Self {
            symbol: s.symbol,
            security_name: s.security_name,
        }
    }
}

/// One fact (catalyst) event: something that happened to a security, with the
/// factors, sources and prose behind it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SecurityFactItem {
    /// Fact ID, e.g. "technical_rsi_14_short_1783674041337603409". A signal
    /// names the fact that triggered it in `key_fact_id`.
    pub fact_id: String,
    /// What kind of fact this is.
    pub fact_type: FactType,
    /// Side the fact points to.
    pub direction: FactDirection,
    /// When the fact occurred (RFC3339).
    pub occur_time: String,
    /// Securities the fact is about.
    pub symbols_info: Vec<FactSymbol>,
    /// Factors that contributed to the fact.
    pub factors: Vec<FactFactor>,
    /// Where the fact came from.
    pub data_source: Vec<FactDataSource>,
    /// Natural-language rendering of the fact.
    pub nl_info: FactNlInfo,
}

impl From<longbridge::signal::SecurityFact> for SecurityFactItem {
    fn from(f: longbridge::signal::SecurityFact) -> Self {
        Self {
            fact_id: f.fact_id,
            fact_type: f.fact_type.into(),
            direction: f.direction.into(),
            occur_time: f.occur_time.format(&Rfc3339).unwrap_or_default(),
            symbols_info: f.symbols_info.into_iter().map(FactSymbol::from).collect(),
            factors: f.factors.into_iter().map(FactFactor::from).collect(),
            data_source: f
                .data_source
                .into_iter()
                .map(FactDataSource::from)
                .collect(),
            nl_info: f.nl_info.into(),
        }
    }
}

/// Returned by `security_facts`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SecurityFactsResponse {
    /// Facts in the requested window, newest first.
    pub facts: Vec<SecurityFactItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nl_field_unwraps_the_embedded_document() {
        let field = NlField::parse(r#"[{"tag":"RSI","value":"balanced"}]"#);
        let json = serde_json::to_value(&field).expect("field must serialize");
        assert_eq!(
            json[0]["tag"], "RSI",
            "an embedded document must become real entries"
        );
    }

    #[test]
    fn nl_field_keeps_an_unparsable_payload_as_a_string() {
        let field = NlField::parse("not json");
        assert_eq!(
            serde_json::to_value(&field).expect("field must serialize"),
            serde_json::Value::String("not json".into()),
            "an unparsable payload must survive rather than be dropped"
        );
    }

    /// The wire values are the schema's `enum` constraint and part of the tool
    /// contract, so they are pinned rather than derived from variant names.
    #[test]
    fn fact_enums_serialize_to_their_documented_labels() {
        assert_eq!(
            serde_json::to_string(&FactType::Fundamental).expect("type must serialize"),
            "\"Fundamental\"",
            "fact_type wire value must stay stable"
        );
        assert_eq!(
            serde_json::to_string(&FactDirection::Long).expect("direction must serialize"),
            "\"long\"",
            "direction wire value must stay stable"
        );
        assert_eq!(
            serde_json::to_string(&FactDirection::Unknown).expect("direction must serialize"),
            "\"\"",
            "the upstream no-side fallback must stay the empty string"
        );
    }

    #[test]
    fn output_schema_constrains_the_fact_enums() {
        let schema = serde_json::to_value(rmcp::schemars::schema_for!(SecurityFactsResponse))
            .expect("schema must serialize");
        let defs = &schema["$defs"];
        assert_eq!(
            defs["FactType"]["enum"],
            serde_json::json!(["News", "Fundamental", "Technical", "Unknown"]),
            "the schema must constrain fact_type to its documented labels"
        );
        assert_eq!(
            defs["FactDirection"]["enum"],
            serde_json::json!(["long", "short", "neutral", ""]),
            "the schema must constrain direction to its documented labels"
        );
        assert!(
            defs["NlField"]["anyOf"].is_array(),
            "a natural-language field must admit both its parsed and raw form"
        );
    }
}
