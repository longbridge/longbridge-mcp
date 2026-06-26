use longbridge::fundamental::{FundamentalContext, MacroeconomicCountry};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacroeconomicIndicatorsParam {
    /// Keyword to search indicator names (e.g. "CPI", "非农", "GDP").
    pub keyword: Option<String>,
    /// Filter by country code. One of: "US", "CN", "HK", "EU", "JP", "SG".
    /// Omit to return all countries.
    pub country: Option<String>,
    /// Pagination offset, default 0.
    pub offset: Option<i32>,
    /// Maximum number of indicators to return (default 100, max 1000).
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacroeconomicParam {
    /// Indicator code from `macrodata_indicators`, e.g. `"USCPI"`.
    pub indicator_code: String,
    /// Earliest release date to include (YYYY-MM-DD, e.g. `"2024-01-01"`).
    pub start_date: Option<String>,
    /// Latest release date to include (YYYY-MM-DD, e.g. `"2024-12-31"`).
    pub end_date: Option<String>,
    /// Pagination offset for historical data points, default 0.
    pub offset: Option<i32>,
    /// Maximum number of data points to return (default 100, max 100).
    pub limit: Option<i32>,
}

fn parse_country(s: &str) -> Result<MacroeconomicCountry, McpError> {
    match s {
        "US" => Ok(MacroeconomicCountry::UnitedStates),
        "CN" => Ok(MacroeconomicCountry::China),
        "HK" => Ok(MacroeconomicCountry::HongKong),
        "EU" => Ok(MacroeconomicCountry::EuroZone),
        "JP" => Ok(MacroeconomicCountry::Japan),
        "SG" => Ok(MacroeconomicCountry::Singapore),
        other => Err(McpError::invalid_params(
            format!("invalid country '{other}'. Valid values: US, CN, HK, EU, JP, SG"),
            None,
        )),
    }
}

/// Remove fields that v2 SDK does not populate (always empty/null).
/// Reduces noise in AI context.
fn strip_indicator(obj: &mut serde_json::Map<String, serde_json::Value>) {
    for key in &["source_org", "adjustment_factor", "category", "start_date"] {
        obj.remove(*key);
    }
}

fn strip_data_point(obj: &mut serde_json::Map<String, serde_json::Value>) {
    for key in &["revised_value", "next_release_at", "unit_prefix"] {
        obj.remove(*key);
    }
}

pub async fn macrodata_indicators(
    mctx: &crate::tools::McpContext,
    p: MacroeconomicIndicatorsParam,
) -> Result<CallToolResult, McpError> {
    let country = p.country.as_deref().map(parse_country).transpose()?;
    let ctx = FundamentalContext::new(mctx.create_config());
    let result = ctx
        .macroeconomic_indicators(country, p.keyword, p.offset, p.limit)
        .await
        .map_err(Error::longbridge)?;
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    if let Some(list) = value["list"].as_array_mut() {
        for item in list {
            if let Some(obj) = item.as_object_mut() {
                strip_indicator(obj);
            }
        }
    }
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        json,
    )]))
}

pub async fn macrodata(
    mctx: &crate::tools::McpContext,
    p: MacroeconomicParam,
) -> Result<CallToolResult, McpError> {
    let ctx = FundamentalContext::new(mctx.create_config());
    let result = ctx
        .macroeconomic(
            p.indicator_code,
            p.start_date,
            p.end_date,
            p.offset,
            p.limit,
        )
        .await
        .map_err(Error::longbridge)?;
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    // Strip unmapped fields from info
    if let Some(obj) = value["info"].as_object_mut() {
        strip_indicator(obj);
    }
    // Strip unmapped fields from each data point
    if let Some(data) = value["data"].as_array_mut() {
        for item in data {
            if let Some(obj) = item.as_object_mut() {
                strip_data_point(obj);
            }
        }
    }
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        json,
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_country_valid() {
        assert!(matches!(
            parse_country("US"),
            Ok(MacroeconomicCountry::UnitedStates)
        ));
        assert!(matches!(
            parse_country("CN"),
            Ok(MacroeconomicCountry::China)
        ));
        assert!(matches!(
            parse_country("HK"),
            Ok(MacroeconomicCountry::HongKong)
        ));
        assert!(matches!(
            parse_country("EU"),
            Ok(MacroeconomicCountry::EuroZone)
        ));
        assert!(matches!(
            parse_country("JP"),
            Ok(MacroeconomicCountry::Japan)
        ));
        assert!(matches!(
            parse_country("SG"),
            Ok(MacroeconomicCountry::Singapore)
        ));
    }

    #[test]
    fn parse_country_invalid() {
        assert!(parse_country("United States").is_err());
        assert!(parse_country("usa").is_err());
        assert!(parse_country("").is_err());
    }

    #[test]
    fn macroeconomic_param_accepts_date_and_offset() {
        let p: MacroeconomicParam = serde_json::from_str(
            r#"{"indicator_code":"USCPI","start_date":"2024-01-01","end_date":"2024-12-31","offset":100}"#,
        )
        .unwrap();
        assert_eq!(p.indicator_code, "USCPI");
        assert_eq!(p.offset, Some(100));
    }
}
