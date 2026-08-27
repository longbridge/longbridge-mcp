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
    if p.kind.is_none()
        && crate::tools::support::us_market::is_us_fundamental(mctx, &p.symbol).await
    {
        let ctx = longbridge::fundamental::FundamentalContext::new(mctx.create_config());
        // Same report-vocabulary bug as financial_statement and
        // financial_report_key_metrics: raw verification data shows
        // report="annual" returns a handful of periods that mix FY/Q1/H1
        // labels rather than annual-only, and "quarterly" returns an empty
        // list — both symptoms match the other two US fundamental endpoints
        // silently ignoring an invalid report value. Defaulting to "af" for
        // consistency; not independently re-confirmed against this specific
        // endpoint in this pass (the raw dump only tried annual/quarterly).
        let report = p
            .report_type
            .unwrap_or_else(|| "af".to_string())
            .to_lowercase();
        let result = ctx
            .us_financial_overview(p.symbol, report)
            .await
            .map_err(crate::error::Error::longbridge)?;
        return crate::tools::tool_json(&result);
    }
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

/// Pull one half of `institution_rating`'s response out of a sub-request,
/// turning a failure into a JSON `null` plus a warning that names the cause.
fn rating_part(label: &str, result: Result<CallToolResult, McpError>) -> (String, Option<String>) {
    match result {
        Ok(part) => {
            let text = part
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.clone())
                .unwrap_or_else(|| "null".to_string());
            (text, None)
        }
        Err(e) => (
            "null".to_string(),
            Some(format!("{label} is unavailable: {}", e.message)),
        ),
    }
}

pub async fn institution_rating(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let params = [("counter_id", cid.as_str())];

    // Two independent upstream calls. Run them concurrently, and let one
    // failure degrade the response instead of discarding the half that worked.
    let (analyst, instratings) = tokio::join!(
        http_get_tool(&client, "/v1/quote/institution-rating-latest", &params),
        http_get_tool(&client, "/v1/quote/institution-ratings", &params),
    );

    // Nothing to report if neither half came back — surface the first cause.
    if let (Err(e), Err(_)) = (&analyst, &instratings) {
        return Err(e.clone());
    }

    let (analyst_text, analyst_warning) = rating_part("analyst", analyst);
    let (instratings_text, instratings_warning) = rating_part("instratings", instratings);
    let warnings: Vec<String> = [analyst_warning, instratings_warning]
        .into_iter()
        .flatten()
        .collect();

    let combined = if warnings.is_empty() {
        format!(r#"{{"analyst":{analyst_text},"instratings":{instratings_text}}}"#)
    } else {
        let warnings_json =
            serde_json::to_string(&warnings).map_err(crate::error::Error::Serialize)?;
        format!(
            r#"{{"analyst":{analyst_text},"instratings":{instratings_text},"warnings":{warnings_json}}}"#
        )
    };

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
    Ok(crate::tools::tool_result(out))
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
    if crate::tools::support::us_market::is_us_fundamental(mctx, &p.symbol).await {
        let ctx = longbridge::fundamental::FundamentalContext::new(mctx.create_config());
        const PCT_KEYS: &[&str] = &["dividend_yield", "dividend_yield_ttm"];
        if crate::counter::is_etf(&p.symbol) {
            let result = ctx
                .us_etf_dividend_info(p.symbol)
                .await
                .map_err(crate::error::Error::longbridge)?;
            let mut value =
                serde_json::to_value(&result).map_err(crate::error::Error::Serialize)?;
            crate::tools::support::us_normalize::normalize_pct_fields(&mut value, PCT_KEYS);
            return crate::tools::tool_json(&value);
        }
        let result = ctx
            .us_company_dividends(p.symbol)
            .await
            .map_err(crate::error::Error::longbridge)?;
        let mut value = serde_json::to_value(&result).map_err(crate::error::Error::Serialize)?;
        crate::tools::support::us_normalize::normalize_pct_fields(&mut value, PCT_KEYS);
        return crate::tools::tool_json(&value);
    }
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
    if crate::tools::support::us_market::is_us_fundamental(mctx, &p.symbol).await {
        let ctx = longbridge::fundamental::FundamentalContext::new(mctx.create_config());
        // Same report-vocabulary bug as the other US fundamental endpoints:
        // report="annual" silently returns an empty result (confirmed live —
        // ai_summary populated but currency/report/list/opt_reports all
        // empty). The raw verification dump's real call used report="af"
        // and got a full 5-period list, whose own opt_reports field
        // ["qf","af"] confirms af/qf are the only valid values.
        let result = ctx
            .us_analyst_consensus(p.symbol, "af")
            .await
            .map_err(crate::error::Error::longbridge)?;
        let mut value = serde_json::to_value(&result).map_err(crate::error::Error::Serialize)?;
        crate::tools::support::us_normalize::fix_valuation_value(&mut value);
        return crate::tools::tool_json(&value);
    }
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
    if crate::tools::support::us_market::is_us_fundamental(mctx, &p.symbol).await {
        let ctx = longbridge::fundamental::FundamentalContext::new(mctx.create_config());
        let result = ctx
            .us_valuation_overview(p.symbol)
            .await
            .map_err(crate::error::Error::longbridge)?;
        let mut value = serde_json::to_value(&result).map_err(crate::error::Error::Serialize)?;
        crate::tools::support::us_normalize::fix_valuation_value(&mut value);
        return crate::tools::tool_json(&value);
    }
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
    if crate::tools::support::us_market::is_us_fundamental(mctx, &p.symbol).await {
        let ctx = longbridge::fundamental::FundamentalContext::new(mctx.create_config());
        let result = ctx
            .us_company_overview(p.symbol)
            .await
            .map_err(crate::error::Error::longbridge)?;
        return crate::tools::tool_json(&result);
    }
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
        &[("counter_id", cid.as_str()), ("position", "detail")],
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
    if crate::tools::support::us_market::is_us_fundamental(mctx, &p.symbol).await {
        let ctx = longbridge::fundamental::FundamentalContext::new(mctx.create_config());
        let kind = p.kind.unwrap_or_else(|| "ALL".to_string()).to_uppercase();
        // Despite the SDK's own doc comment claiming "annual"/"quarterly",
        // live staging testing confirmed the US endpoint actually uses the
        // same af/saf/qf/q1-q3 vocabulary as the generic path — "annual"
        // silently returns an empty list.
        let report = p.report.unwrap_or_else(|| "af".to_string()).to_lowercase();
        // The backend does not support kind=ALL directly (confirmed via live
        // staging testing: it returns an empty list, while IS/BS/CF each
        // return full data individually) — fan out and merge so ALL still
        // behaves as advertised instead of silently returning nothing.
        if kind == "ALL" {
            let (is, bs, cf) = tokio::try_join!(
                ctx.us_financial_statement(p.symbol.clone(), "IS".to_string(), report.clone()),
                ctx.us_financial_statement(p.symbol.clone(), "BS".to_string(), report.clone()),
                ctx.us_financial_statement(p.symbol.clone(), "CF".to_string(), report),
            )
            .map_err(crate::error::Error::longbridge)?;
            let combined = serde_json::json!({
                "income_statement": is,
                "balance_sheet": bs,
                "cash_flow": cf,
            });
            return crate::tools::tool_json(&combined);
        }
        let result = ctx
            .us_financial_statement(p.symbol, kind, report)
            .await
            .map_err(crate::error::Error::longbridge)?;
        return crate::tools::tool_json(&result);
    }
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolReportParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Report period: "af" (annual, default), "saf" (semi-annual), "qf"
    /// (quarterly full), "q1"/"q2"/"q3".
    pub report: Option<String>,
}

/// Get key financial metrics for a US symbol. US accounts only — no HK
/// equivalent exists for this interface.
pub async fn financial_report_key_metrics(
    mctx: &crate::tools::McpContext,
    p: SymbolReportParam,
) -> Result<CallToolResult, McpError> {
    let ctx = longbridge::fundamental::FundamentalContext::new(mctx.create_config());
    // Same sibling-endpoint doc-comment bug as financial_statement: raw
    // verification data confirms report="af" returns a clean 11-period
    // annual-only list, while "annual" (this endpoint's own former default,
    // matching its incorrect doc comment) returns 140+ unfiltered periods
    // mixing FY/Q1/Q2/Q3/H0/H1 — i.e. the report filter is silently ignored.
    // Lowercased for consistency with financial_statement's handling of the
    // same report vocabulary — the backend appears to match report values
    // exactly rather than case-insensitively (see the "annual" note above).
    let report = p.report.unwrap_or_else(|| "af".to_string()).to_lowercase();
    let result = ctx
        .us_key_financial_metrics(p.symbol, report)
        .await
        .map_err(crate::error::Error::longbridge)?;
    crate::tools::tool_json(&result)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EtfDocsParam {
    /// ETF symbol, e.g. "SPY.US"
    pub symbol: String,
    /// Maximum number of documents to return. Omit for all.
    pub limit: Option<u32>,
}

/// Get regulatory/prospectus documents for a US ETF. US accounts only — no
/// HK equivalent exists for this interface.
pub async fn etf_docs(
    mctx: &crate::tools::McpContext,
    p: EtfDocsParam,
) -> Result<CallToolResult, McpError> {
    let ctx = longbridge::fundamental::FundamentalContext::new(mctx.create_config());
    let result = ctx
        .us_etf_files(p.symbol, p.limit)
        .await
        .map_err(crate::error::Error::longbridge)?;
    crate::tools::tool_json(&result)
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
    /// BK counter_id from `industry_rank`, e.g. "BK/US/IN00258".
    pub symbol: String,
}

pub async fn industry_peers(
    mctx: &crate::tools::McpContext,
    p: IndustryPeersParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mkt = if p.symbol.contains('/') {
        // BK counter_id: BK/US/IN00258 → market = "US"
        p.symbol.split('/').nth(1).unwrap_or("US").to_uppercase()
    } else {
        p.symbol
            .rsplit_once('.')
            .map(|(_, m)| m.to_uppercase())
            .unwrap_or_else(|| "US".to_string())
    };
    // Accept BK counter_ids directly (contain '/').
    // Industry symbols from industry_rank are transformed to IN00xxx.US by transform_json;
    // detect them by the leading "IN" prefix and map back to BK/<market>/<code>.
    let cid = if p.symbol.contains('/') {
        p.symbol.clone()
    } else if let Some((code, market)) = p.symbol.rsplit_once('.') {
        if code.to_uppercase().starts_with("IN") {
            format!("BK/{}/{}", market.to_uppercase(), code.to_uppercase())
        } else {
            symbol_to_counter_id(&p.symbol)
        }
    } else {
        symbol_to_counter_id(&p.symbol)
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
    /// Fiscal period, e.g. "1" "2" "3" "4"
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShareholderTopParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
}

pub async fn shareholder_top(
    mctx: &crate::tools::McpContext,
    p: ShareholderTopParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    http_get_tool(
        &client,
        "/v1/quote/shareholders/top",
        &[("counter_id", cid.as_str())],
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShareholderDetailParam {
    /// Security symbol, e.g. "AAPL.US"
    pub symbol: String,
    /// Shareholder object_id from shareholder_top tool
    pub object_id: i64,
}

pub async fn shareholder_detail(
    mctx: &crate::tools::McpContext,
    p: ShareholderDetailParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let oid = p.object_id.to_string();
    http_get_tool(
        &client,
        "/v1/quote/shareholders/holding",
        &[("counter_id", cid.as_str()), ("object_id", oid.as_str())],
    )
    .await
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValuationComparisonParam {
    /// Security symbol to compare, e.g. "AAPL.US"
    pub symbol: String,
    /// Currency: "USD" | "HKD" | "CNY"
    pub currency: String,
    /// Comparison symbols, comma-separated, max 4, e.g. "MSFT.US,GOOGL.US".
    /// Note: pending backend support — currently server auto-selects industry peers.
    pub comparison_symbols: Option<String>,
}

pub async fn valuation_comparison(
    mctx: &crate::tools::McpContext,
    p: ValuationComparisonParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let mut params: Vec<(&str, &str)> = vec![
        ("counter_id", cid.as_str()),
        ("currency", p.currency.as_str()),
    ];
    // iOS serializes comparison_counter_ids as a JSON array string
    // e.g. comparison_counter_ids=["ST/HK/700","ST/HK/80700"]
    let comp_json: String;
    if let Some(ref syms) = p.comparison_symbols {
        let cids: Vec<String> = syms
            .split(',')
            .map(|s| symbol_to_counter_id(s.trim()))
            .collect();
        comp_json = serde_json::to_string(&cids).unwrap_or_default();
        params.push(("comparison_counter_ids", comp_json.as_str()));
    }
    http_get_tool_unix(
        &client,
        "/v1/quote/compare/valuation",
        &params,
        &["list.*.history.*.date"],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::rating_part;
    use rmcp::ErrorData as McpError;
    use rmcp::model::Content;

    #[test]
    fn rating_part_returns_the_payload_and_no_warning_on_success() {
        let ok = rmcp::model::CallToolResult::success(vec![Content::text(r#"{"a":1}"#)]);
        let (text, warning) = rating_part("analyst", Ok(ok));
        assert_eq!(text, r#"{"a":1}"#);
        assert!(warning.is_none());
    }

    #[test]
    fn rating_part_degrades_to_null_and_names_the_cause() {
        let err = McpError::internal_error("upstream 503", None);
        let (text, warning) = rating_part("instratings", Err(err));
        assert_eq!(text, "null");
        let warning = warning.expect("a failure must produce a warning");
        assert!(
            warning.contains("instratings") && warning.contains("upstream 503"),
            "warning should name the part and the cause, got: {warning}"
        );
    }

    #[test]
    fn rating_part_treats_an_empty_body_as_json_null() {
        let empty = rmcp::model::CallToolResult::success(vec![]);
        let (text, warning) = rating_part("analyst", Ok(empty));
        assert_eq!(text, "null");
        // An empty-but-successful response is not a failure.
        assert!(warning.is_none());
    }
}
