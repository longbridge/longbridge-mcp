use longbridge::fundamental::FundamentalContext;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MacrodataIndicatorsParam {
    /// Pagination offset, default 0.
    pub offset: Option<i32>,
    /// Maximum number of indicators to return (default 100, max 1000).
    /// There are ~619 indicators total; pass 1000 to fetch all at once.
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
    /// Maximum number of data points to return (default 100, max 100).
    pub limit: Option<i32>,
}

pub async fn macrodata_indicators(
    mctx: &crate::tools::McpContext,
    p: MacrodataIndicatorsParam,
) -> Result<CallToolResult, McpError> {
    let ctx = FundamentalContext::new(mctx.create_config());
    let result = ctx
        .macrodata_indicators(p.offset, p.limit)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn macrodata(
    mctx: &crate::tools::McpContext,
    p: MacrodataParam,
) -> Result<CallToolResult, McpError> {
    let code = p.indicator_code.clone();
    let ctx = FundamentalContext::new(mctx.create_config());
    ctx.macrodata(p.indicator_code, p.start_date, p.end_date, p.limit)
        .await
        .map_err(|e| {
            // The API returns {"info": null} when the code does not exist,
            // which causes a deserialize error inside the SDK.
            let msg = e.to_string();
            if msg.contains("null") && msg.contains("MacrodataIndicator") {
                McpError::invalid_params(format!("indicator_code '{code}' not found"), None)
            } else {
                Error::longbridge(e).into()
            }
        })
        .and_then(|result| tool_json(&result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macrodata_param_accepts_date_only_format() {
        // The SDK wraps YYYY-MM-DD into YYYY-MM-DDT00:00:00Z / T23:59:59Z internally.
        // Verify the param struct deserialises as expected.
        let p: MacrodataParam = serde_json::from_str(
            r#"{"indicator_code":"USCPI","start_date":"2024-01-01","end_date":"2024-12-31"}"#,
        )
        .unwrap();
        assert_eq!(p.indicator_code, "USCPI");
        assert_eq!(p.start_date.as_deref(), Some("2024-01-01"));
        assert_eq!(p.end_date.as_deref(), Some("2024-12-31"));
    }
}
