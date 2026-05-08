use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolResult, Content};
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::error::Error;
use crate::serialize::convert_unix_paths;
use crate::tools::support::http_client::{http_get_tool, http_get_tool_unix};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfitAnalysisParam {
    /// Start date (yyyy-mm-dd). Must be paired with `end`; passing only one returns empty results.
    pub start: Option<String>,
    /// End date (yyyy-mm-dd). Must be paired with `start`; passing only one returns empty results.
    pub end: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProfitAnalysisDetailParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
    /// Start date (yyyy-mm-dd). Must be paired with `end`; passing only one returns empty results.
    pub start: Option<String>,
    /// End date (yyyy-mm-dd). Must be paired with `start`; passing only one returns empty results.
    pub end: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PortfolioSymbolParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MultiLegPositionsParam {
    /// List of multi-leg combination identifiers
    pub multi_legs: Vec<String>,
    /// Strategy type (optional)
    pub strategy: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WithdrawalRecordsParam {
    /// Page number (optional)
    pub page: Option<u32>,
    /// Page size (optional)
    pub size: Option<u32>,
    /// Account channel (optional)
    pub account_channel: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DepositRecordsParam {
    /// Page number (optional)
    pub page: Option<u32>,
    /// Page size (optional)
    pub size: Option<u32>,
    /// Account channel (optional)
    pub account_channel: Option<String>,
    /// Currency filter (optional)
    pub currencies: Option<String>,
    /// Status filter: 0=processing, 1=credited, 2=failed (optional)
    pub states: Option<String>,
}

fn date_to_unix(s: &str, end_of_day: bool) -> Result<i64, McpError> {
    let date = time::Date::parse(s, time::macros::format_description!("[year]-[month]-[day]"))
        .map_err(|e| McpError::invalid_params(format!("invalid date '{s}': {e}"), None))?;
    let t = if end_of_day {
        time::Time::from_hms(23, 59, 59).expect("valid time")
    } else {
        time::Time::MIDNIGHT
    };
    Ok(time::PrimitiveDateTime::new(date, t)
        .assume_utc()
        .unix_timestamp())
}

pub async fn exchange_rate(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/asset/exchange_rates", &[]).await
}

pub async fn profit_analysis(
    mctx: &crate::tools::McpContext,
    p: ProfitAnalysisParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();

    let start_ts = p
        .start
        .as_deref()
        .map(|s| date_to_unix(s, false))
        .transpose()?;
    let end_ts = p
        .end
        .as_deref()
        .map(|s| date_to_unix(s, true))
        .transpose()?;

    let start_str = start_ts.map(|v| v.to_string());
    let end_str = end_ts.map(|v| v.to_string());

    let mut summary_params: Vec<(&str, &str)> = Vec::new();
    let mut sublist_params: Vec<(&str, &str)> = vec![("profit_or_loss", "all")];

    if let Some(ref s) = start_str {
        summary_params.push(("start", s.as_str()));
        sublist_params.push(("start", s.as_str()));
    }
    if let Some(ref e) = end_str {
        summary_params.push(("end", e.as_str()));
        sublist_params.push(("end", e.as_str()));
    }

    let (summary_result, sublist_result) = tokio::join!(
        http_get_tool(
            &client,
            "/v1/portfolio/profit-analysis-summary",
            &summary_params
        ),
        http_get_tool(
            &client,
            "/v1/portfolio/profit-analysis-sublist",
            &sublist_params
        ),
    );

    let summary_text = summary_result?
        .content
        .into_iter()
        .next()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    let sublist_text = sublist_result?
        .content
        .into_iter()
        .next()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();

    let summary: serde_json::Value =
        serde_json::from_str(&summary_text).map_err(|e| Error::Other(e.to_string()))?;
    let sublist: serde_json::Value =
        serde_json::from_str(&sublist_text).map_err(|e| Error::Other(e.to_string()))?;

    let mut merged = match summary {
        serde_json::Value::Object(m) => m,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("data".to_owned(), other);
            map
        }
    };
    merged.insert("sublist".to_owned(), sublist);

    let mut value = serde_json::Value::Object(merged);
    convert_unix_paths(&mut value, &["end_time", "trade_update_time"]);

    Ok(CallToolResult::success(vec![Content::text(
        value.to_string(),
    )]))
}

pub async fn profit_analysis_detail(
    mctx: &crate::tools::McpContext,
    p: ProfitAnalysisDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);

    let start_ts = p
        .start
        .as_deref()
        .map(|s| date_to_unix(s, false))
        .transpose()?;
    let end_ts = p
        .end
        .as_deref()
        .map(|s| date_to_unix(s, true))
        .transpose()?;

    let start_str = start_ts.map(|v| v.to_string());
    let end_str = end_ts.map(|v| v.to_string());

    let mut params: Vec<(&str, &str)> = vec![("counter_id", cid.as_str())];
    if let Some(ref s) = start_str {
        params.push(("start", s.as_str()));
    }
    if let Some(ref e) = end_str {
        params.push(("end", e.as_str()));
    }

    http_get_tool_unix(
        &client,
        "/v1/portfolio/profit-analysis/detail",
        &params,
        &["start", "end"],
    )
    .await
}

pub async fn short_margin(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/asset/cash/short-margin", &[]).await
}

pub async fn holding_period(
    mctx: &crate::tools::McpContext,
    p: PortfolioSymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/asset/positions/holding-period",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn multi_leg_positions(
    mctx: &crate::tools::McpContext,
    p: MultiLegPositionsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut params: Vec<(&str, &str)> = p
        .multi_legs
        .iter()
        .map(|leg| ("multi_legs", leg.as_str()))
        .collect();
    if let Some(ref strategy) = p.strategy {
        params.push(("strategy", strategy.as_str()));
    }
    http_get_tool(&client, "/v1/asset/positions/multi-leg", &params).await
}

pub async fn position_trade_info(
    mctx: &crate::tools::McpContext,
    p: PortfolioSymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/asset/positions/trade-info",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn order_statistics(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/asset/trade-analysis", &[]).await
}

pub async fn bank_cards(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/account/bank-cards", &[]).await
}

pub async fn withdrawal_records(
    mctx: &crate::tools::McpContext,
    p: WithdrawalRecordsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.map(|v| v.to_string());
    let size_str = p.size.map(|v| v.to_string());
    let mut params: Vec<(&str, &str)> = vec![];
    if let Some(ref s) = page_str {
        params.push(("page", s.as_str()));
    }
    if let Some(ref s) = size_str {
        params.push(("size", s.as_str()));
    }
    if let Some(ref s) = p.account_channel {
        params.push(("account_channel", s.as_str()));
    }
    http_get_tool(&client, "/v1/account/withdrawals", &params).await
}

pub async fn deposit_records(
    mctx: &crate::tools::McpContext,
    p: DepositRecordsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.map(|v| v.to_string());
    let size_str = p.size.map(|v| v.to_string());
    let mut params: Vec<(&str, &str)> = vec![];
    if let Some(ref s) = page_str {
        params.push(("page", s.as_str()));
    }
    if let Some(ref s) = size_str {
        params.push(("size", s.as_str()));
    }
    if let Some(ref s) = p.account_channel {
        params.push(("account_channel", s.as_str()));
    }
    if let Some(ref s) = p.currencies {
        params.push(("currencies", s.as_str()));
    }
    if let Some(ref s) = p.states {
        params.push(("states", s.as_str()));
    }
    http_get_tool(&client, "/v1/account/deposits", &params).await
}
