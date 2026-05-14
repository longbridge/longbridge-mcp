use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::{counter_id_to_symbol, symbol_to_counter_id};
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FinancialStatementParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Statement kind: "IS" (income statement), "BS" (balance sheet), "CF" (cash flow), "ALL" (default)
    pub kind: Option<String>,
    /// Report period: "af" (annual), "saf" (semi-annual), "qf" (quarterly full), "q1"/"q2"/"q3"
    pub report: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValuationRankParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Start date in yyyymmdd format (default: 30 days ago)
    pub start: Option<String>,
    /// End date in yyyymmdd format (default: today)
    pub end: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct InstitutionRatingIndustryRankParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Page number (default: 1)
    pub page: Option<u32>,
    /// Page size (default: 20)
    pub size: Option<u32>,
}

/// Get financial statements (income statement, balance sheet, or cash flow).
pub async fn financial_statement(
    mctx: &crate::tools::McpContext,
    p: FinancialStatementParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let kind = p.kind.unwrap_or_else(|| "ALL".to_string()).to_uppercase();
    let report = p.report.unwrap_or_else(|| "af".to_string()).to_lowercase();
    http_get_tool(
        &client,
        "/v1/quote/financials/statements",
        &[
            ("counter_id", cid.as_str()),
            ("kind", kind.as_str()),
            ("report", report.as_str()),
        ],
    )
    .await
}

/// Get latest financial report summary for a security.
pub async fn financial_report_latest(
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

/// Get daily valuation rank (PE/PB/PS/dividend yield percentile) for a security.
pub async fn valuation_rank(
    mctx: &crate::tools::McpContext,
    p: ValuationRankParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let mut params: Vec<(&str, &str)> = vec![("counter_id", cid.as_str())];
    if let Some(ref s) = p.start {
        params.push(("start_date", s.as_str()));
    }
    if let Some(ref e) = p.end {
        params.push(("end_date", e.as_str()));
    }
    http_get_tool_unix(
        &client,
        "/v1/quote/valuation/rank",
        &params,
        &[
            "pe.*.timestamp",
            "pb.*.timestamp",
            "ps.*.timestamp",
            "dvd.*.timestamp",
        ],
    )
    .await
}

/// Get institution rating history (target price + evaluate history) for a security.
pub async fn institution_rating_history(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/ratings/history",
        &[("counter_id", cid.as_str())],
    )
    .await
}

/// Get institution rating industry rank for a security (peers ranked by analyst ratings).
pub async fn institution_rating_industry_rank(
    mctx: &crate::tools::McpContext,
    p: InstitutionRatingIndustryRankParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let page_str = p.page.unwrap_or(1).to_string();
    let size_str = p.size.unwrap_or(20).to_string();
    let resp = http_get_tool(
        &client,
        "/v1/quote/institution-ratings/industry-rank",
        &[
            ("counter_id", cid.as_str()),
            ("page", page_str.as_str()),
            ("size", size_str.as_str()),
        ],
    )
    .await?;
    // Convert counter_id fields to symbol format in items list
    let json_str = resp
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("null");
    let mut value: serde_json::Value =
        serde_json::from_str(json_str).map_err(crate::error::Error::Serialize)?;
    if let Some(items) = value.get_mut("items").and_then(|v| v.as_array_mut()) {
        for item in items.iter_mut() {
            if let Some(cid_val) = item.get("counter_id").and_then(|v| v.as_str()) {
                let symbol = counter_id_to_symbol(cid_val);
                if let Some(obj) = item.as_object_mut() {
                    obj.remove("counter_id");
                    obj.insert("symbol".to_string(), serde_json::Value::String(symbol));
                }
            }
        }
    }
    let out = serde_json::to_string(&value).map_err(crate::error::Error::Serialize)?;
    let structured = serde_json::from_str::<serde_json::Value>(&out).ok();
    let mut result = rmcp::model::CallToolResult::success(vec![rmcp::model::Content::text(out)]);
    result.structured_content = structured;
    Ok(result)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BusinessSegmentsParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
}

pub async fn business_segments(
    mctx: &crate::tools::McpContext,
    p: BusinessSegmentsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/fundamentals/business-segments",
        &[("counter_id", cid.as_str())],
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BusinessSegmentsHistoryParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Report period: "qf" (quarterly), "saf" (semi-annual), "af" (annual)
    pub report: Option<String>,
    /// Segment category filter
    pub cate: Option<String>,
}

pub async fn business_segments_history(
    mctx: &crate::tools::McpContext,
    p: BusinessSegmentsHistoryParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let mut params: Vec<(&str, &str)> = vec![("counter_id", cid.as_str())];
    let report = p.report.unwrap_or_default();
    let cate = p.cate.unwrap_or_default();
    if !report.is_empty() {
        params.push(("report", report.as_str()));
    }
    if !cate.is_empty() {
        params.push(("cate", cate.as_str()));
    }
    http_get_tool(
        &client,
        "/v1/quote/fundamentals/business-segments/history",
        &params,
    )
    .await
}

pub async fn institutional_views(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool_unix(
        &client,
        "/v1/quote/ratings/institutional",
        &[("counter_id", cid.as_str())],
        &["elist.*.date"],
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IndustryPeersParam {
    /// Industry counter_id in BK/MARKET/INxxxxx format (e.g. "BK/US/IN00446"),
    /// as returned by the `industry_rank` tool.
    /// Also accepts symbol format INxxxxx.MARKET (e.g. "IN00446.US").
    pub counter_id: String,
}

pub async fn industry_peers(
    mctx: &crate::tools::McpContext,
    p: IndustryPeersParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let (mkt, cid) = if p.counter_id.contains('/') {
        let market = p
            .counter_id
            .splitn(3, '/')
            .nth(1)
            .unwrap_or("US")
            .to_uppercase();
        (market, p.counter_id.clone())
    } else {
        let market = p
            .counter_id
            .rsplit_once('.')
            .map(|(_, m)| m.to_uppercase())
            .unwrap_or_else(|| "US".to_string());
        let cid = symbol_to_counter_id(&p.counter_id);
        (market, cid)
    };
    http_get_tool(
        &client,
        "/v1/quote/industries/peers",
        &[
            ("type", "1"),
            ("market", mkt.as_str()),
            ("industry_id", ""),
            ("counter_id", cid.as_str()),
        ],
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FinancialReportSnapshotParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Report type: "qf" (quarterly), "saf" (semi-annual), "af" (annual)
    pub report: Option<String>,
    /// Fiscal year, e.g. 2024
    pub fiscal_year: Option<u32>,
    /// Fiscal period number, e.g. "1", "2", "3", "4"
    pub fiscal_period: Option<String>,
}

pub async fn financial_report_snapshot(
    mctx: &crate::tools::McpContext,
    p: FinancialReportSnapshotParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let fiscal_year = p.fiscal_year.map(|y| y.to_string());
    let mut params: Vec<(&str, &str)> = vec![("counter_id", cid.as_str())];
    let report = p.report.unwrap_or_default();
    let period = p.fiscal_period.unwrap_or_default();
    if !report.is_empty() {
        params.push(("report", report.as_str()));
    }
    if let Some(ref y) = fiscal_year {
        params.push(("fiscal_year", y.as_str()));
    }
    if !period.is_empty() {
        params.push(("fiscal_period", period.as_str()));
    }
    http_get_tool(&client, "/v1/quote/financials/earnings-snapshot", &params).await
}
