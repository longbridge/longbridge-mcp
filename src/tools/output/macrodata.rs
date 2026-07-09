//! Typed output schemas for macro-economic data tools.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `macrodata_indicators`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MacroeconomicIndicatorsResponse {
    /// Indicator list.
    pub list: Vec<MacroeconomicIndicator>,
    /// Total number of indicators matching the query.
    pub count: i32,
}

/// Metadata for one macro-economic indicator.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MacroeconomicIndicator {
    /// Indicator code used as input to `macrodata`.
    pub indicator_code: String,
    /// Country or market code, e.g. US / CN / HK / EU / JP / SG.
    pub country: String,
    /// Localized indicator name.
    pub name: String,
    /// Description of the indicator.
    pub describe: String,
    /// Release periodicity, e.g. day / week / month / quarter / half_year / year.
    pub periodicity: String,
    /// Importance level: 1 = low, 2 = medium, 3 = high.
    pub importance: i32,
}

/// Returned by `macrodata`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MacroeconomicResponse {
    /// Indicator metadata.
    pub info: MacroeconomicIndicator,
    /// Historical data points.
    pub data: Vec<MacroeconomicDataPoint>,
    /// Total number of historical data points matching the query.
    pub count: i32,
}

/// One historical data point for a macro-economic indicator.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MacroeconomicDataPoint {
    /// Statistical period. Format varies by periodicity.
    pub period: String,
    /// Release timestamp (RFC3339), or null if not set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_at: Option<String>,
    /// Actual released value. Empty when not yet released.
    pub actual_value: String,
    /// Previous period's value.
    pub previous_value: String,
    /// Forecast or consensus value.
    pub forecast_value: String,
    /// Unit for the value.
    pub unit: String,
}
