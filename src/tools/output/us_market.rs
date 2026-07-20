//! Typed output schemas for tools with a US-region-specific response shape,
//! covering `financial_statement`, `financial_report`,
//! `financial_report_key_metrics`, `profit_analysis_realized`, `etf_docs`,
//! `stock_positions`'s `us_asset_overview` field, and `order_detail`'s
//! US-wrapped shape.
//!
//! Shapes are reconstructed from real staging responses (captured via a raw
//! HTTP verification pass, bypassing the SDK) plus live MCP tool calls in
//! this repo's development. Every field is `Option<T>`; nothing here is
//! marked required, since upstream may omit or add fields and, for the
//! multi-shape tools, only one of the alternative shapes is present per
//! response. The generic (non-US) `financial_statement`/`financial_report`
//! paths are assumed to share the same list/fields envelope as the
//! confirmed US shape (per the existing code comment that the US and
//! generic paths share report-period vocabulary) — this has not been
//! independently re-verified against a live HK/generic call in this pass.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// One line item in a financial-statement/financial-report `fields[]` array
/// (e.g. one row of an income statement: "Revenue", "Total Revenue", ...).
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportField {
    /// Field key, e.g. "total_rev". Empty for section-header rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Localized row label, e.g. "总营业收入".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Row value. Empty string when not released/applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Value type hint, e.g. "bignumber".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_type: Option<String>,
    /// Year-over-year change. Empty string when not applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yoy: Option<String>,
    /// Row identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Indentation/hierarchy level (1 = section header).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,
    /// Display order within the period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_order: Option<i32>,
}

/// One reporting period in `financial_statement`'s `list[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialStatementPeriod {
    /// Fiscal period number within the fiscal year (e.g. "1" = Q1). Empty
    /// for full-year rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ff_period: Option<String>,
    /// Fiscal year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ff_year: Option<i32>,
    /// Fiscal period end date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fp_end: Option<String>,
    /// Human-readable period label, e.g. "FY 2024", "Q1 2025".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_txt: Option<String>,
    /// Report publish date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpt_date: Option<String>,
    /// Line items for this period. Empty when the underlying data isn't
    /// available for this report/period combination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<FinancialReportField>>,
}

/// One statement kind's data (income statement, balance sheet, or cash
/// flow), as returned by `financial_statement` for a single `kind` and by
/// `financial_report_key_metrics`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialStatementKind {
    /// Reporting currency, e.g. "USD".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Report period requested, e.g. "af", "qf".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Field keys with no data across every period in `list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_fields: Option<Vec<String>>,
    /// Periods, most recent first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<FinancialStatementPeriod>>,
}

/// Returned by `financial_statement`.
///
/// Two mutually-exclusive shapes: `kind=ALL` (default) returns
/// `income_statement`/`balance_sheet`/`cash_flow`; a single `kind` (IS/BS/CF)
/// returns the `currency`/`report`/`empty_fields`/`list` fields directly at
/// the root instead.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialStatementResponse {
    /// Present when `kind=ALL` (default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub income_statement: Option<FinancialStatementKind>,
    /// Present when `kind=ALL` (default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_sheet: Option<FinancialStatementKind>,
    /// Present when `kind=ALL` (default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_flow: Option<FinancialStatementKind>,
    /// Present when a single `kind` (IS/BS/CF) is requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Present when a single `kind` (IS/BS/CF) is requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Present when a single `kind` (IS/BS/CF) is requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_fields: Option<Vec<String>>,
    /// Present when a single `kind` (IS/BS/CF) is requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<FinancialStatementPeriod>>,
}

/// Returned by `financial_report_key_metrics`. Same envelope as a single
/// `financial_statement` kind, confirmed via a live US staging call.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportKeyMetricsResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<FinancialStatementPeriod>>,
}

/// One period in `financial_report`'s `is_list`/`bs_list`/`cf_list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportPeriodMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_txt: Option<String>,
}

/// One period in `financial_report`'s `is_list` (income statement summary).
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportIncomePeriod {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_income: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_margin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<FinancialReportPeriodMeta>,
}

/// One period in `financial_report`'s `bs_list` (balance sheet summary).
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportBalancePeriod {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debt_assets_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_assets: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_liabilities: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<FinancialReportPeriodMeta>,
}

/// One period in `financial_report`'s `cf_list` (cash flow summary).
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportCashFlowPeriod {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub investing: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub financing: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<FinancialReportPeriodMeta>,
}

/// Returned by `financial_report`.
///
/// US-routed shape (no `kind` passed, `.US` symbol, US account — confirmed
/// via live staging call): `ccy_symbol`/`report_type`/`is_list`/`bs_list`/
/// `cf_list`. When `kind` is passed explicitly (any account) the generic
/// path is used instead, assumed to share `financial_statement`'s
/// `currency`/`report`/`empty_fields`/`list` envelope — not independently
/// re-verified against a live call in this pass.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportResponse {
    /// US-routed shape only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy_symbol: Option<String>,
    /// US-routed shape only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_type: Option<String>,
    /// US-routed shape only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_list: Option<Vec<FinancialReportIncomePeriod>>,
    /// US-routed shape only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bs_list: Option<Vec<FinancialReportBalancePeriod>>,
    /// US-routed shape only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_list: Option<Vec<FinancialReportCashFlowPeriod>>,
    /// Generic path only (kind passed explicitly). Assumed, not
    /// independently re-verified in this pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Generic path only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Generic path only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_fields: Option<Vec<String>>,
    /// Generic path only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<FinancialStatementPeriod>>,
}

/// One category's realized P&L in `profit_analysis_realized`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RealizedPlCategory {
    /// "All" / "Stock" / "Option" / "Crypto" / "Unknown".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Vec<RealizedPlMetric>>,
}

/// One period's realized P&L metric.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RealizedPlMetric {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<i32>,
    /// Signed rate, e.g. "-0.9303".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<String>,
    /// Always "decimal_fraction": `rate` is a fraction (-0.9303 = -93.03%),
    /// not a percent value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_unit: Option<String>,
}

/// Returned by `profit_analysis_realized`. US accounts only.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PortfolioRealizedPlResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realized_pl_list: Option<Vec<RealizedPlCategory>>,
}

/// One document in `etf_docs`'s `files[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EtfDocFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    /// Download URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Last-updated date, e.g. "06/18/2026".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_date: Option<String>,
    /// Backend document-type code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// File format, e.g. "PDF", "HTM".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// Returned by `etf_docs`. US accounts only.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EtfDocsResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<EtfDocFile>>,
}

/// `stock_positions`'s `us_asset_overview` field (US accounts only).
/// Reconstructed from a real staging `/v1/us/assets/overview` response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsAssetOverview {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_buy_power: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overnight_buy_power: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_list: Option<Vec<UsCashPosition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_list: Option<Vec<UsStockPosition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_list: Option<Vec<UsOptionPosition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crypto_list: Option<Vec<UsCryptoPosition>>,
    /// Multi-leg strategy holdings (e.g. covered calls). Shape is complex
    /// and not fully modeled; passed through as raw JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi_leg: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UsCashPosition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settled_cash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen_buy_cash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outstanding: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UsStockPosition {
    /// Symbol, e.g. "AAPL.US". Promoted from `full_symbol` (see
    /// `normalize_us_stock_list`) so it matches every other tool's
    /// fully-qualified `symbol` convention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_cost: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_done: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_close: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub today_pl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_name: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UsOptionPosition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_cost: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike_price: Option<String>,
    /// "Call" or "Put".
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub option_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlying_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub today_pl: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UsCryptoPosition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_cost: Option<String>,
}

/// `order_detail`'s US-region shape: the order is nested under `order`
/// instead of at the response root (unlike the generic `OrderDetailResponse`
/// shape). Confirmed via a live US staging `order_detail` call.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsOrderDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// "Buy" or "Sell".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_qty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_amount: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operate_direction: Option<String>,
    /// "Day" / "GTC" / "GTD" / "Unknown".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_histories: Option<Vec<UsOrderHistoryEntry>>,
}

/// One state-transition entry in `UsOrderDetail.order_histories`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsOrderHistoryEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty: Option<String>,
    /// Renamed from the raw `time` field so the generic `_at`-suffix
    /// timestamp convention applies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occurred_at: Option<String>,
}
