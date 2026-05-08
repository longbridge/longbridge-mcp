use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::serialize::convert_unix_paths;
use crate::tools::support::http_client::{http_get_tool, http_get_tool_unix};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FinancialReportParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Statement kind: "IS" (income statement), "BS" (balance sheet), "CF" (cash flow), "ALL" (default)
    pub kind: Option<String>,
    /// Report period: "af" (annual), "saf" (semi-annual), "q1"/"q2"/"q3" (quarterly), "qf" (quarterly full)
    pub report_type: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValuationRankParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Start date in YYYYMMDD format, e.g. "20250101"
    pub start_date: String,
    /// End date in YYYYMMDD format, e.g. "20250131"
    pub end_date: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalystEstimatesParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Indicator type (optional)
    pub item: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InstitutionRatingIndustryRankParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Page number (optional)
    pub page: Option<String>,
    /// Page size (optional)
    pub size: Option<String>,
}

pub async fn financial_report(
    mctx: &crate::tools::McpContext,
    p: FinancialReportParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let kind = p.kind.unwrap_or_else(|| "ALL".to_string());
    let mut params: Vec<(&str, &str)> = vec![("counter_id", cid.as_str()), ("kind", kind.as_str())];
    let report_type = p.report_type.unwrap_or_default();
    if !report_type.is_empty() {
        params.push(("report", report_type.as_str()));
    }
    http_get_tool(&client, "/v1/quote/financial-reports", &params).await
}

pub async fn institution_rating(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let params = [("counter_id", cid.as_str())];
    let ratings = http_get_tool(&client, "/v1/quote/institution-rating-latest", &params).await;
    let instratings = http_get_tool(&client, "/v1/quote/institution-ratings", &params).await;
    match (ratings, instratings) {
        (Ok(r), Ok(i)) => {
            let r_text = r
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str())
                .unwrap_or("null");
            let i_text = i
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.as_str())
                .unwrap_or("null");
            let combined = format!(r#"{{"analyst":{r_text},"instratings":{i_text}}}"#);
            let mut value: serde_json::Value =
                serde_json::from_str(&combined).map_err(crate::error::Error::Serialize)?;
            convert_unix_paths(
                &mut value,
                &[
                    "analyst.evaluate.start_date",
                    "analyst.evaluate.end_date",
                    "analyst.target.start_date",
                    "analyst.target.end_date",
                ],
            );
            let out = serde_json::to_string(&value).map_err(crate::error::Error::Serialize)?;
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                out,
            )]))
        }
        (Err(e), _) | (_, Err(e)) => Err(e),
    }
}

pub async fn institution_rating_detail(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/institution-ratings/detail",
        &[("counter_id", cid.as_str())],
        &["target.list.*.timestamp"],
    )
    .await
}

pub async fn dividend(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/dividends",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn dividend_detail(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/dividends/details",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn forecast_eps(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/forecast-eps",
        &[("counter_id", cid.as_str())],
        &["items.*.forecast_start_date", "items.*.forecast_end_date"],
    )
    .await
}

pub async fn consensus(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/financial-consensus-detail",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn valuation(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/valuation",
        &[
            ("counter_id", cid.as_str()),
            ("indicator", "pe"),
            ("range", "1"),
        ],
        &["metrics.pe.list.*.timestamp"],
    )
    .await
}

pub async fn valuation_history(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/valuation/detail",
        &[("counter_id", cid.as_str())],
        &["history.metrics.pe.list.*.timestamp"],
    )
    .await
}

pub async fn industry_valuation(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/industry-valuation-comparison",
        &[("counter_id", cid.as_str())],
        &["list.*.history.*.date"],
    )
    .await
}

pub async fn industry_valuation_dist(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/industry-valuation-distribution",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn company(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/comp-overview",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn executive(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/company-professionals",
        &[("counter_ids", cid.as_str())],
    )
    .await
}

pub async fn shareholder(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/shareholders",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn fund_holder(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/fund-holders",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn corp_action(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/company-act",
        &[
            ("counter_id", cid.as_str()),
            ("req_type", "1"),
            ("version", "3"),
        ],
    )
    .await
}

pub async fn invest_relation(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/invest-relations",
        &[("counter_id", cid.as_str()), ("count", "0")],
    )
    .await
}

pub async fn operating(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/operatings",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn latest_financial_report(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/financials/latest-report",
        &[("counter_id", cid.as_str())],
    )
    .await
}

pub async fn valuation_rank(
    mctx: &crate::tools::McpContext,
    p: ValuationRankParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/valuation/rank",
        &[
            ("counter_id", cid.as_str()),
            ("start_date", p.start_date.as_str()),
            ("end_date", p.end_date.as_str()),
        ],
        &[
            "pe.*.timestamp",
            "pb.*.timestamp",
            "ps.*.timestamp",
            "dvd.*.timestamp",
        ],
    )
    .await
}

pub async fn analyst_estimates(
    mctx: &crate::tools::McpContext,
    p: AnalystEstimatesParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let mut params: Vec<(&str, &str)> = vec![("counter_id", cid.as_str())];
    if let Some(ref item) = p.item {
        params.push(("item", item.as_str()));
    }
    http_get_tool(&client, "/v1/quote/estimates", &params).await
}

pub async fn rating_history(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/ratings/history",
        &[("counter_id", cid.as_str())],
        &[
            "evaluate_history.*.start_date",
            "evaluate_history.*.end_date",
        ],
    )
    .await
}

pub async fn institution_rating_industry_rank(
    mctx: &crate::tools::McpContext,
    p: InstitutionRatingIndustryRankParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let mut params: Vec<(&str, &str)> = vec![("counter_id", cid.as_str())];
    if let Some(ref page) = p.page {
        params.push(("page", page.as_str()));
    }
    if let Some(ref size) = p.size {
        params.push(("size", size.as_str()));
    }
    http_get_tool(
        &client,
        "/v1/quote/institution-ratings/industry-rank",
        &params,
    )
    .await
}
