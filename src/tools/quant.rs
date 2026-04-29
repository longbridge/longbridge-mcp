use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use time::Time;

use crate::counter::symbol_to_counter_id;
use crate::tools::support::http_client::http_post_tool;
use crate::tools::support::parse;

/// Parameters for running an indicator script against historical K-line data:
/// target symbol, date range, K-line period, the script source itself, and
/// optional script inputs.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunScriptParam {
    /// Symbol in <CODE>.<MARKET> format, e.g. TSLA.US, 700.HK
    pub symbol: String,
    /// K-line period: 1m, 5m, 15m, 30m, 1h, day, week, month, year (default: day)
    #[serde(default = "default_period")]
    pub period: String,
    /// Start date (YYYY-MM-DD) for the K-line range
    pub start: String,
    /// End date (YYYY-MM-DD) for the K-line range
    pub end: String,
    /// Indicator script source (PineScript V6 syntax).
    pub script: Option<String>,
    /// Script input values as a JSON array, e.g. "[14,2.0]". Must match the order of input.*() calls in the script.
    pub input: Option<String>,
}

fn default_period() -> String {
    "day".to_string()
}

/// Map CLI period string to the numeric `line_type` expected by the API.
fn period_to_line_type(period: &str) -> Result<i32, McpError> {
    match period {
        "1m" | "minute" => Ok(1),
        "5m" => Ok(5),
        "15m" => Ok(15),
        "30m" => Ok(30),
        "1h" | "60m" | "hour" => Ok(60),
        "day" | "d" | "1d" => Ok(1000),
        "week" | "w" => Ok(2000),
        "month" | "m" | "1mo" => Ok(3000),
        "year" | "y" => Ok(4000),
        _ => Err(McpError::invalid_params(
            format!("Unknown period '{period}'. Use: 1m 5m 15m 30m 1h day week month year"),
            None,
        )),
    }
}

/// Run a quant indicator script against historical K-line data on the server.
pub async fn run_script(
    mctx: &crate::tools::McpContext,
    p: RunScriptParam,
) -> Result<CallToolResult, McpError> {
    let counter_id = symbol_to_counter_id(&p.symbol);
    let line_type = period_to_line_type(&p.period)?;

    let start_dt = parse::parse_date(&p.start)?
        .with_time(Time::MIDNIGHT)
        .assume_utc();
    let end_time_of_day = Time::from_hms(23, 59, 59).expect("23:59:59 is a valid time-of-day");
    let end_dt = parse::parse_date(&p.end)?
        .with_time(end_time_of_day)
        .assume_utc();
    let start_time = start_dt.unix_timestamp();
    let end_time = end_dt.unix_timestamp();

    let script = p.script.unwrap_or_default();
    if script.trim().is_empty() {
        return Err(McpError::invalid_params(
            "script is required and must be non-empty",
            None,
        ));
    }

    // Validate and normalise input_json: default to empty array.
    let input_json = match p.input {
        Some(s) => {
            let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| {
                McpError::invalid_params(format!("input must be a valid JSON array: {e}"), None)
            })?;
            if !v.is_array() {
                return Err(McpError::invalid_params(
                    "input must be a JSON array, e.g. \"[14,2.0]\"",
                    None,
                ));
            }
            s
        }
        None => "[]".to_string(),
    };

    let body = serde_json::json!({
        "counter_id": counter_id,
        "start_time": start_time,
        "end_time": end_time,
        "script": script,
        "input_json": input_json,
        "line_type": line_type,
    });

    let client = mctx.create_http_client();
    http_post_tool(&client, "/v1/quant/run_script", body).await
}
