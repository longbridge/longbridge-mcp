use longbridge::fundamental::{FundamentalContext, MacrodataCountry};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacrodataIndicatorsParam {
    /// Filter by country code. One of: "US", "CN", "HK", "EU", "JP", "SG".
    /// Omit to return all countries.
    pub country: Option<String>,
    /// Pagination offset, default 0.
    pub offset: Option<i32>,
    /// Maximum number of indicators to return (default 100, max 1000).
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacrodataParam {
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

fn parse_country(s: &str) -> Result<MacrodataCountry, McpError> {
    match s {
        "US" => Ok(MacrodataCountry::UnitedStates),
        "CN" => Ok(MacrodataCountry::China),
        "HK" => Ok(MacrodataCountry::HongKong),
        "EU" => Ok(MacrodataCountry::EuroZone),
        "JP" => Ok(MacrodataCountry::Japan),
        "SG" => Ok(MacrodataCountry::Singapore),
        other => Err(McpError::invalid_params(
            format!("invalid country '{other}'. Valid values: US, CN, HK, EU, JP, SG"),
            None,
        )),
    }
}

pub async fn macrodata_indicators(
    mctx: &crate::tools::McpContext,
    p: MacrodataIndicatorsParam,
) -> Result<CallToolResult, McpError> {
    let country = p.country.as_deref().map(parse_country).transpose()?;
    let ctx = FundamentalContext::new(mctx.create_config());
    let result = ctx
        .macrodata_indicators(country, p.offset, p.limit)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn macrodata(
    mctx: &crate::tools::McpContext,
    p: MacrodataParam,
) -> Result<CallToolResult, McpError> {
    let ctx = FundamentalContext::new(mctx.create_config());
    let result = ctx
        .macrodata(
            p.indicator_code,
            p.start_date,
            p.end_date,
            p.offset,
            p.limit,
        )
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_country_valid() {
        assert!(matches!(
            parse_country("US"),
            Ok(MacrodataCountry::UnitedStates)
        ));
        assert!(matches!(parse_country("CN"), Ok(MacrodataCountry::China)));
        assert!(matches!(
            parse_country("HK"),
            Ok(MacrodataCountry::HongKong)
        ));
        assert!(matches!(
            parse_country("EU"),
            Ok(MacrodataCountry::EuroZone)
        ));
        assert!(matches!(parse_country("JP"), Ok(MacrodataCountry::Japan)));
        assert!(matches!(
            parse_country("SG"),
            Ok(MacrodataCountry::Singapore)
        ));
    }

    #[test]
    fn parse_country_invalid() {
        assert!(parse_country("United States").is_err());
        assert!(parse_country("usa").is_err());
        assert!(parse_country("").is_err());
    }

    #[test]
    fn macrodata_param_accepts_date_and_offset() {
        let p: MacrodataParam = serde_json::from_str(
            r#"{"indicator_code":"USCPI","start_date":"2024-01-01","end_date":"2024-12-31","offset":100}"#,
        )
        .unwrap();
        assert_eq!(p.indicator_code, "USCPI");
        assert_eq!(p.offset, Some(100));
    }
}
