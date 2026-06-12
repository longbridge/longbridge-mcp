//! Typed output schemas for the fundamental-data tools in
//! [`crate::tools::fundamental`].
//!
//! These tools are almost entirely thin pass-throughs (`http_get_tool` /
//! `http_get_tool_unix`) of the upstream quote API: the SDK has no Rust types
//! for the responses, so the structs below are reconstructed *solely* from the
//! `Returns ...` clauses in each tool's `#[tool(description = ...)]`. The
//! shapes mirror the post-transform JSON produced by `http_get_tool`:
//!
//! - object keys are camelCase → snake_case
//! - `counter_id` → `symbol` (string), `counter_ids` → `symbols`
//! - `*_at` integer / `OffsetDateTime` timestamps → RFC3339 `String`
//! - the `unix_paths` arg of `http_get_tool_unix` rewrites those unix-epoch
//!   integer fields into RFC3339 `String`
//! - `Decimal` / numeric monetary values are stringified
//!
//! Accuracy rules followed here, given the shapes are *inferred* from prose:
//! - every struct documents only the fields the description explicitly lists
//! - every field is `Option<T>` (nothing is marked required); upstream may
//!   return additional fields not modelled here
//! - no fields are invented; nested array elements get their own structs
//!
//! Each struct is referenced from `#[tool(output_schema = ...)]` on the
//! corresponding tool method in [`crate::tools`]. We only declare a struct
//! when the root JSON shape is an object (the MCP spec requires the
//! `outputSchema` root to be `type: "object"`).

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `institution_rating`.
///
/// The tool combines two upstream calls into
/// `{"analyst": {...}, "instratings": [...]}`. Only the `analyst` fields are
/// documented; the `instratings` payload shape is unspecified and left as raw
/// JSON. Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingResponse {
    /// Analyst rating consensus summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyst: Option<InstitutionRatingAnalyst>,
    /// Per-institution rating list. Shape is unspecified by the tool
    /// description; passed through as raw JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instratings: Option<serde_json::Value>,
}

/// Analyst consensus block of `institution_rating`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingAnalyst {
    /// Number of analysts rating "buy".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy: Option<i64>,
    /// Number of analysts rating "outperform".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outperform: Option<i64>,
    /// Number of analysts rating "hold".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<i64>,
    /// Number of analysts rating "underperform".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underperform: Option<i64>,
    /// Number of analysts rating "sell".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell: Option<i64>,
    /// Consensus target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_price: Option<String>,
    /// Consensus rating label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_rating: Option<String>,
}

/// Returned by `institution_rating_detail`.
///
/// Detailed historical institution ratings and target price history, grouped
/// under `target.list[]`. Subset of documented fields; upstream may return
/// more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingDetailResponse {
    /// Target-price / rating history container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<InstitutionRatingDetailTarget>,
}

/// `target` block of `institution_rating_detail`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingDetailTarget {
    /// Per-institution rating records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<InstitutionRatingDetailItem>>,
}

/// One per-institution record in `institution_rating_detail`'s `target.list`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingDetailItem {
    /// Analyst name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyst: Option<String>,
    /// Issuing firm / institution name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firm: Option<String>,
    /// Rating label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<String>,
    /// Target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_price: Option<String>,
    /// Rating timestamp (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// Returned by `dividend`. Wraps an `items` array of dividend events.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DividendResponse {
    /// Dividend events for the symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<DividendItem>>,
}

/// One dividend event in `dividend`'s `items`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DividendItem {
    /// Ex-dividend date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ex_date: Option<String>,
    /// Payment date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay_date: Option<String>,
    /// Record date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_date: Option<String>,
    /// Dividend type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_type: Option<String>,
    /// Dividend amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    /// Settlement currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Dividend status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// Returned by `dividend_detail`. Wraps a `details` array of distribution
/// schemes.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DividendDetailResponse {
    /// Per-period distribution schemes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<DividendDetailItem>>,
}

/// One distribution scheme in `dividend_detail`'s `details`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DividendDetailItem {
    /// Reporting period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Cash dividend per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cash_dividend: Option<String>,
    /// Stock dividend ratio / amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_dividend: Option<String>,
    /// Record date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_date: Option<String>,
    /// Ex-dividend date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ex_date: Option<String>,
    /// Payment date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay_date: Option<String>,
    /// Settlement currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

/// Returned by `forecast_eps`. Wraps an `items` array of EPS estimates.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ForecastEpsResponse {
    /// EPS forecast / actual records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ForecastEpsItem>>,
}

/// One record in `forecast_eps`'s `items`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ForecastEpsItem {
    /// Forecast period start (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_start_date: Option<String>,
    /// Forecast period end (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_end_date: Option<String>,
    /// Consensus EPS estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eps_estimate: Option<String>,
    /// Actual reported EPS.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eps_actual: Option<String>,
    /// Surprise percentage (actual vs estimate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surprise_pct: Option<String>,
    /// Number of contributing analysts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyst_count: Option<i64>,
}

/// Returned by `consensus`. Wraps an `items` array of consensus estimates.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConsensusResponse {
    /// Consensus estimate records for upcoming periods.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ConsensusItem>>,
}

/// One record in `consensus`'s `items`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConsensusItem {
    /// Estimate period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Revenue estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue_estimate: Option<String>,
    /// EPS estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eps_estimate: Option<String>,
    /// Net income estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_income_estimate: Option<String>,
    /// Number of contributing analysts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyst_count: Option<i64>,
    /// Last update time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

/// Returned by `valuation`. The valuation overview groups per-metric blocks
/// under `metrics`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationResponse {
    /// Valuation metric blocks keyed by indicator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ValuationMetrics>,
}

/// `metrics` block of `valuation`. Each indicator carries the same shape.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationMetrics {
    /// Price-to-earnings block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<ValuationMetric>,
    /// Price-to-book block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pb: Option<ValuationMetric>,
    /// Price-to-sales block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ps: Option<ValuationMetric>,
    /// Dividend-yield block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_yield: Option<ValuationMetric>,
}

/// A single valuation indicator block in `valuation`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationMetric {
    /// Current value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    /// Industry average.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_avg: Option<String>,
    /// 5-year average. (camelCase `5yr_avg` per description.)
    #[serde(rename = "5yr_avg", skip_serializing_if = "Option::is_none")]
    pub five_yr_avg: Option<String>,
    /// Historical percentile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentile: Option<String>,
}

/// Returned by `valuation_history`. Time-series valuation metrics grouped
/// under `history.metrics`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryResponse {
    /// History container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<ValuationHistoryBlock>,
}

/// `history` block of `valuation_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryBlock {
    /// Per-indicator time series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ValuationHistoryMetrics>,
}

/// `history.metrics` block of `valuation_history`. Each indicator is an array
/// of time-series samples.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryMetrics {
    /// Price-to-earnings series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<Vec<ValuationHistoryPoint>>,
    /// Price-to-book series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pb: Option<Vec<ValuationHistoryPoint>>,
    /// Price-to-sales series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ps: Option<Vec<ValuationHistoryPoint>>,
    /// Dividend-yield series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_yield: Option<Vec<ValuationHistoryPoint>>,
}

/// One sample in a `valuation_history` metric time series.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryPoint {
    /// Sample timestamp (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Metric value at this timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Returned by `industry_valuation`. Wraps a `list` of industry peers.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationResponse {
    /// Peers in the same industry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<IndustryValuationItem>>,
}

/// One peer in `industry_valuation`'s `list`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationItem {
    /// Security symbol (transformed from `counter_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Price-to-earnings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<String>,
    /// Price-to-book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pb: Option<String>,
    /// Price-to-sales.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ps: Option<String>,
    /// Dividend yield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_yield: Option<String>,
    /// Per-date history of PE/PB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<IndustryValuationHistoryPoint>>,
}

/// One history point in `industry_valuation`'s nested `history`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationHistoryPoint {
    /// Sample date (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Price-to-earnings at this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<String>,
    /// Price-to-book at this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pb: Option<String>,
}

/// Returned by `industry_valuation_dist`. Per-indicator distribution stats
/// grouped under `distributions`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationDistResponse {
    /// Per-indicator distribution blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distributions: Option<IndustryValuationDistributions>,
}

/// `distributions` block of `industry_valuation_dist`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationDistributions {
    /// Price-to-earnings distribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<IndustryValuationDistribution>,
    /// Price-to-book distribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pb: Option<IndustryValuationDistribution>,
    /// Price-to-sales distribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ps: Option<IndustryValuationDistribution>,
}

/// One indicator's distribution stats in `industry_valuation_dist`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationDistribution {
    /// Minimum value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    /// 25th percentile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p25: Option<String>,
    /// Median.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median: Option<String>,
    /// 75th percentile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p75: Option<String>,
    /// Maximum value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
    /// Where the stock currently sits in this distribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_percentile: Option<String>,
}

/// Returned by `company`. Company overview / profile.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CompanyResponse {
    /// Company name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Business profile / description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Number of employees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employees: Option<i64>,
    /// Chief Executive Officer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ceo: Option<String>,
    /// Year the company was founded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub founded_year: Option<i64>,
    /// Company website.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    /// Listing exchange.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange: Option<String>,
    /// Industry classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry: Option<String>,
    /// Market capitalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_cap: Option<String>,
}

/// Returned by `executive`. Wraps a `members` array of executives / board
/// members.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ExecutiveResponse {
    /// Executive and board members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<ExecutiveMember>>,
}

/// One person in `executive`'s `members`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ExecutiveMember {
    /// Full name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Title / role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Date appointed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appointed_date: Option<String>,
    /// Age.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age: Option<i64>,
    /// Biography.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biography: Option<String>,
    /// Compensation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compensation: Option<String>,
}

/// Returned by `shareholder`. Wraps a `shareholders` array.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderResponse {
    /// Institutional shareholders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shareholders: Option<Vec<ShareholderItem>>,
}

/// One holder in `shareholder`'s `shareholders`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderItem {
    /// Institution name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub institution: Option<String>,
    /// Shares held.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares: Option<String>,
    /// Ownership ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<String>,
    /// Change in shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    /// Direction / kind of change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_type: Option<String>,
    /// Report date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reported_at: Option<String>,
}

/// Returned by `fund_holder`. Wraps a `fund_holders` array.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FundHolderResponse {
    /// Funds / ETFs that hold the symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fund_holders: Option<Vec<FundHolderItem>>,
}

/// One holder in `fund_holder`'s `fund_holders`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FundHolderItem {
    /// Fund name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fund_name: Option<String>,
    /// Fund symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fund_symbol: Option<String>,
    /// Shares held.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares: Option<String>,
    /// Ownership ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<String>,
    /// Change in shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    /// Report date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reported_at: Option<String>,
}

/// Returned by `corp_action`. Wraps an `items` array of corporate actions.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CorpActionResponse {
    /// Corporate action events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<CorpActionItem>>,
}

/// One event in `corp_action`'s `items`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CorpActionItem {
    /// Action type (split, buyback, name change, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_type: Option<String>,
    /// Effective date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_date: Option<String>,
    /// Ratio (e.g. for splits).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<String>,
    /// Free-text description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Returned by `invest_relation`. Wraps an `items` array of IR events.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InvestRelationResponse {
    /// Investor-relations events and announcements.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<InvestRelationItem>>,
}

/// One event in `invest_relation`'s `items`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InvestRelationItem {
    /// Event title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Event type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    /// Event date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_date: Option<String>,
    /// Related URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Free-text description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Returned by `operating`. Wraps an `items` array of operating metrics
/// (HK stocks only).
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OperatingResponse {
    /// Operating metric records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<OperatingItem>>,
}

/// One record in `operating`'s `items`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OperatingItem {
    /// Reporting period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Metric name (e.g. passenger traffic, cargo volume).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric_name: Option<String>,
    /// Metric value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Unit of measure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// Returned by `financial_report_latest`. Latest financial report summary.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportLatestResponse {
    /// Reporting period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Revenue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue: Option<String>,
    /// Net income.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_income: Option<String>,
    /// Earnings per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eps: Option<String>,
    /// Return on equity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roe: Option<String>,
    /// Gross margin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gross_margin: Option<String>,
    /// Report date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_date: Option<String>,
}

/// Returned by `institution_rating_history`. Two history arrays: target-price
/// revisions and rating-evaluation changes.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingHistoryResponse {
    /// Target-price revisions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_history: Option<Vec<TargetHistoryItem>>,
    /// Rating-evaluation changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluate_history: Option<Vec<EvaluateHistoryItem>>,
}

/// One target-price revision in `institution_rating_history`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TargetHistoryItem {
    /// Issuing firm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firm: Option<String>,
    /// Analyst name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyst: Option<String>,
    /// Prior target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_target: Option<String>,
    /// New target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_target: Option<String>,
    /// Revision date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// One rating-evaluation change in `institution_rating_history`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EvaluateHistoryItem {
    /// Issuing firm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firm: Option<String>,
    /// Prior rating.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_rating: Option<String>,
    /// New rating.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_rating: Option<String>,
    /// Change date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// Returned by `institution_rating_industry_rank`. Peers ranked by analyst
/// ratings.
///
/// The tool description says `list[]`, while the implementation transforms a
/// top-level `items[]` array (rewriting `counter_id` → `symbol`). Both names
/// are modelled so the schema matches whichever the upstream emits. Subset of
/// documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingIndustryRankResponse {
    /// Ranked peers (description's documented key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<InstitutionRatingIndustryRankItem>>,
    /// Ranked peers (key the implementation transforms in place).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<InstitutionRatingIndustryRankItem>>,
}

/// One peer in `institution_rating_industry_rank`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingIndustryRankItem {
    /// Security symbol (transformed from `counter_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Buy rating count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy_count: Option<i64>,
    /// Sell rating count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell_count: Option<i64>,
    /// Consensus rating label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consensus_rating: Option<String>,
    /// Target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_price: Option<String>,
}

/// Returned by `business_segments_history`. Wraps a `historical` array of
/// per-period segment snapshots.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BusinessSegmentsHistoryResponse {
    /// Per-period segment snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub historical: Option<Vec<BusinessSegmentsHistoryPeriod>>,
}

/// One period snapshot in `business_segments_history`'s `historical`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BusinessSegmentsHistoryPeriod {
    /// Period date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Total revenue for the period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
    /// Settlement currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Revenue by business line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub business: Option<Vec<SegmentBreakdown>>,
    /// Revenue by region.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regionals: Option<Vec<SegmentBreakdown>>,
}

/// One segment breakdown entry in `business_segments_history`
/// (`business[]` / `regionals[]`).
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SegmentBreakdown {
    /// Segment / region name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Percentage of total.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<String>,
    /// Absolute value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Returned by `institutional_views`. Wraps a `months` array of monthly
/// rating-distribution snapshots.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionalViewsResponse {
    /// Monthly rating-distribution snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub months: Option<Vec<InstitutionalViewsMonth>>,
}

/// One month in `institutional_views`'s `months`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionalViewsMonth {
    /// Month date (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Buy count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy: Option<i64>,
    /// Outperform count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outperform: Option<i64>,
    /// Hold count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<i64>,
    /// Underperform count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underperform: Option<i64>,
    /// Sell count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell: Option<i64>,
    /// Total ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

/// Returned by `industry_peers`. A hierarchical sub-sector tree (`chain`) plus
/// the originating industry group (`top`).
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryPeersResponse {
    /// Root node of the sub-sector tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<IndustryPeersNode>,
    /// The originating industry group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<IndustryPeersTop>,
}

/// One node in `industry_peers`' `chain` tree. Self-referential via `next`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryPeersNode {
    /// Node name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Node identifier (transformed from `counter_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counter_id: Option<String>,
    /// Number of stocks in this sub-sector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_num: Option<i64>,
    /// Daily change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg: Option<String>,
    /// Year-to-date change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ytd_chg: Option<String>,
    /// Child sub-sector nodes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Vec<IndustryPeersNode>>,
}

/// `top` block of `industry_peers`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryPeersTop {
    /// Industry group name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Market code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}

/// Returned by `financial_report_snapshot`. Actual-vs-forecast comparison
/// plus financial ratios.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportSnapshotResponse {
    /// Text summary of the report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_desc: Option<String>,
    /// Revenue: actual vs forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fo_revenue: Option<ForecastActual>,
    /// EBIT: actual vs forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fo_ebit: Option<ForecastActual>,
    /// EPS: actual vs forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fo_eps: Option<ForecastActual>,
}

/// An actual-vs-forecast comparison block in `financial_report_snapshot`
/// (`fo_revenue` / `fo_ebit` / `fo_eps`).
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ForecastActual {
    /// Year-over-year change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yoy: Option<String>,
    /// Actual vs forecast comparison.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmp: Option<String>,
}

/// Returned by `shareholder_top`. Wraps an `info` array of per-period
/// snapshots, each with a `share_holders` list.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderTopResponse {
    /// Per-period holder snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<Vec<ShareholderTopPeriod>>,
}

/// One period snapshot in `shareholder_top`'s `info`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderTopPeriod {
    /// Reporting period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Holders for this period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_holders: Option<Vec<ShareholderTopHolder>>,
}

/// One holder in `shareholder_top`'s `share_holders`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderTopHolder {
    /// Holder object id. Pass to `shareholder_detail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<i64>,
    /// Holder name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Holder title / role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Shares held.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_held: Option<String>,
    /// Percentage of shares held.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_shares_held: Option<String>,
    /// Change in shares held.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_changed: Option<String>,
    /// Filing date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filing_date: Option<String>,
}

/// Returned by `shareholder_detail`. A single holder's holding and trade
/// history.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderDetailResponse {
    /// Holder name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Holder source: Company / Institution / Person / Insider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_source: Option<String>,
    /// Per-period trading records.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradings: Option<Vec<ShareholderTrading>>,
    /// Holding summary. Shape unspecified by the description; raw JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_summary: Option<serde_json::Value>,
    /// Holding periods. Shape unspecified by the description; raw JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_periods: Option<serde_json::Value>,
    /// Trading periods. Shape unspecified by the description; raw JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trading_periods: Option<serde_json::Value>,
}

/// One per-period trading record in `shareholder_detail`'s `tradings`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderTrading {
    /// Reporting period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Accumulated buys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accum_buy: Option<String>,
    /// Accumulated sells.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accum_sell: Option<String>,
    /// Net buys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_buy: Option<String>,
    /// Individual trades. Empty for institutional (13F) holders; populated
    /// only for insider / individual filers (Form 4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trading_details: Option<Vec<ShareholderTradingDetail>>,
}

/// One trade in `shareholder_detail`'s `trading_details`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderTradingDetail {
    /// Trade date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trading_date: Option<String>,
    /// Trade type (buy / sell).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trading_type: Option<String>,
    /// Number of shares traded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trading_shares: Option<String>,
    /// Trade price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trading_price: Option<String>,
    /// Security type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_type: Option<String>,
    /// Filing date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filing_date: Option<String>,
}

/// Returned by `valuation_comparison`. Wraps a `list` of compared stocks.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationComparisonResponse {
    /// Compared stocks (primary + peers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<ValuationComparisonItem>>,
}

/// One stock in `valuation_comparison`'s `list`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationComparisonItem {
    /// Security symbol (transformed from `counter_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Market value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_value: Option<String>,
    /// Latest close price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_close: Option<String>,
    /// Price-to-earnings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<String>,
    /// Price-to-book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pb: Option<String>,
    /// Price-to-sales.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ps: Option<String>,
    /// Per-date valuation history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<ValuationComparisonHistoryPoint>>,
}

/// One history point in `valuation_comparison`'s nested `history`.
///
/// Subset of documented fields; upstream may return more.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationComparisonHistoryPoint {
    /// Sample date (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Price-to-earnings at this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<String>,
    /// Price-to-book at this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pb: Option<String>,
    /// Price-to-sales at this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ps: Option<String>,
}
