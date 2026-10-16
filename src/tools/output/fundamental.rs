//! Typed output schemas for the fundamental-data tools in
//! [`crate::tools::fundamental`].
//!
//! These tools are almost entirely thin pass-throughs (`http_get_tool` /
//! `http_get_tool_unix`) of the upstream quote API, so the SDK has no Rust
//! types for the responses. The structs below exist only to describe the
//! response shape to MCP clients via `output_schema`; the tools never
//! deserialize into them. They mirror the post-transform JSON the tools
//! actually emit, as captured from live production responses:
//!
//! - object keys are camelCase → snake_case
//! - `*_at` timestamps and the paths each tool lists in `unix_paths` become
//!   RFC3339 `String`s; other epoch-like fields stay as upstream sent them
//! - decimal figures stay strings, exactly as upstream sends them
//!
//! Every field is `Option<T>` (nothing is marked required): upstream omits
//! empty fields, and `dividend` / `consensus` / `valuation` / `company`
//! return a different field set for US accounts querying a `.US` symbol, so
//! both variants must validate against one schema. Fields that only the US
//! variant carries say so in their doc comment.
//!
//! Each `*Response` struct is referenced from `#[tool(output_schema = ...)]`
//! on the corresponding tool method in [`crate::tools`]. We only declare a
//! struct when the root JSON shape is an object (the MCP spec requires the
//! `outputSchema` root to be `type: "object"`).

use std::collections::BTreeMap;

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `institution_rating`.
///
/// The tool combines two upstream calls into `{"analyst": {...},
/// "instratings": {...}}`; when one call fails the other half is still
/// returned and `warnings` explains what is missing.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingResponse {
    /// Analyst rating distribution, target-price range and industry
    /// ranking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyst: Option<InstitutionRatingAnalyst>,
    /// Latest consensus recommendation and target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instratings: Option<InstitutionRatingSummary>,
    /// Present when one of the two upstream calls failed: the other
    /// half is still returned, and this names what is missing and why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

/// `analyst` block of `institution_rating`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingAnalyst {
    /// Current analyst rating distribution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluate: Option<RatingDistribution>,
    /// Analyst target-price range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<InstitutionRatingTargetRange>,
    /// Upstream industry id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_id: Option<i64>,
    /// Industry name (localized).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_name: Option<String>,
    /// The stock's rank within its industry by analyst rating.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_rank: Option<i64>,
    /// Number of stocks ranked in the industry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_total: Option<i64>,
    /// Industry mean of the ranking metric, as upstream reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_mean: Option<i64>,
    /// Industry median of the ranking metric, as upstream reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_median: Option<i64>,
}

/// Analyst rating counts in `institution_rating`'s `analyst.evaluate`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RatingDistribution {
    /// Number of "buy" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy: Option<i64>,
    /// Number of "outperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over: Option<i64>,
    /// Number of "hold" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<i64>,
    /// Number of "underperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<i64>,
    /// Number of "sell" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell: Option<i64>,
    /// Number of analysts with no opinion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_opinion: Option<i64>,
    /// Total number of ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    /// Window start (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// Window end (RFC3339), or `"0"` when the window is open-ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

/// `analyst.target` block of `institution_rating`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingTargetRange {
    /// Highest analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_price: Option<String>,
    /// Lowest analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lowest_price: Option<String>,
    /// Previous close price, for comparison.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_close: Option<String>,
    /// Window start (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// Window end (RFC3339), or `"0"` when the window is open-ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

/// `instratings` block of `institution_rating`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingSummary {
    /// Last update date, as a localized display string (e.g. "2026 年 10
    /// 月 14 日").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Consensus recommendation, e.g. `"strong_buy"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommend: Option<String>,
    /// Consensus (mean) target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Upside of `target` versus the current price, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
}

/// Returned by `institution_rating_detail`. Historical rating distribution
/// (`evaluate.list`) and consensus target-price history (`target.list`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingDetailResponse {
    /// Rating distribution history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluate: Option<InstitutionRatingDetailEvaluate>,
    /// Consensus target-price history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<InstitutionRatingDetailTarget>,
    /// Currency symbol of the prices, e.g. "$" or "HK$".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy_symbol: Option<String>,
}

/// `evaluate` block of `institution_rating_detail`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingDetailEvaluate {
    /// One rating-distribution snapshot per date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<InstitutionRatingDetailEvaluateItem>>,
}

/// One snapshot in `institution_rating_detail`'s `evaluate.list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingDetailEvaluateItem {
    /// Number of "strong buy" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strong_buy: Option<i64>,
    /// Number of "buy" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy: Option<i64>,
    /// Number of "hold" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<i64>,
    /// Number of "underperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<i64>,
    /// Number of "sell" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell: Option<i64>,
    /// Snapshot date, e.g. "2021/10/31".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// `target` block of `institution_rating_detail`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingDetailTarget {
    /// Last update date, as a localized display string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Historical accuracy of the consensus target, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction_accuracy: Option<String>,
    /// Data coverage ratio (0-1), as upstream reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_percent: Option<String>,
    /// One target-price sample per date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<InstitutionRatingDetailItem>>,
}

/// One sample in `institution_rating_detail`'s `target.list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingDetailItem {
    /// Sample time (RFC3339, UTC).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Sample date in the exchange's local calendar, e.g. "2021/10/17".
    /// Can differ from `timestamp`'s UTC date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Share price at this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    /// Mean analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_target: Option<String>,
    /// Lowest analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_target: Option<String>,
    /// Highest analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_target: Option<String>,
    /// Upstream `meet` flag for this sample (whether the target was
    /// met).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meet: Option<bool>,
}

/// Returned by `dividend`.
///
/// The generic shape is `{list, total}`. US accounts querying a `.US`
/// symbol get a US-specific variant with `recent_dividends`,
/// `dividend_history`, `payout_ratios` and `dividend_payout_history`
/// instead.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DividendResponse {
    /// Dividend events, newest first. Generic (non-US) shape only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<DividendItem>>,
    /// Number of events in `list`, as a string. Generic (non-US) shape
    /// only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
    /// US-account variant only: trailing-twelve-month summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_dividends: Option<UsRecentDividends>,
    /// US-account variant only: per-fiscal-year dividend history.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_history: Option<Vec<UsDividendHistoryYear>>,
    /// US-account variant only: per-fiscal-year payout ratios.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payout_ratios: Option<Vec<UsDividendHistoryYear>>,
    /// US-account variant only: individual dividend payment events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_payout_history: Option<Vec<UsDividendPayout>>,
}

/// `recent_dividends` block of `dividend`'s US-account variant.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsRecentDividends {
    /// Trailing-twelve-month dividend per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_ttm: Option<String>,
    /// Trailing-twelve-month yield. Percent value, e.g. 0.34 means
    /// 0.34% (not a 0-1 fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_yield_ttm: Option<f64>,
    /// Number of payouts in the trailing twelve months.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payouts: Option<String>,
    /// Dividend currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

/// One fiscal year's entry in `dividend`'s US-account `dividend_history` /
/// `payout_ratios`. Both arrays share this shape; only the populated fields
/// differ per array.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsDividendHistoryYear {
    /// Fiscal year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fiscal_year: Option<String>,
    /// Fiscal year date range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fiscal_year_range: Option<String>,
    /// Dividend per share for the year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend: Option<String>,
    /// Percent value (e.g. 0.58 means 0.58%), present in
    /// `dividend_history`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_yield: Option<f64>,
    /// Year-over-year dividend growth.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_growth_rate: Option<String>,
    /// Dividend payout ratio, present in `payout_ratios`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_payout_ratio: Option<String>,
    /// Dividend-to-cash-flow ratio, present in `payout_ratios`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_to_cashflow_ratio: Option<String>,
    /// Dividend currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
}

/// One dividend payment event in `dividend`'s US-account
/// `dividend_payout_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsDividendPayout {
    /// Dividend per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend: Option<String>,
    /// Dividend type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_type: Option<String>,
    /// Dividend currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Ex-dividend date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ex_date: Option<String>,
    /// Payment date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_date: Option<String>,
    /// Record date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_date: Option<String>,
}

/// One dividend event in `dividend`'s `list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DividendItem {
    /// Upstream event id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Human-readable distribution summary, e.g. "每股派息 0.25 USD".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// Upstream counter id of the security, e.g. "ST/US/NVDA".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counter_id: Option<String>,
    /// Record date, e.g. "2026.09.10".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_date: Option<String>,
    /// Ex-dividend date, e.g. "2026.09.10".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ex_date: Option<String>,
    /// Payment date, e.g. "2026.10.01".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_date: Option<String>,
}

/// Returned by `dividend_detail`. Wraps a `list` of distribution schemes.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DividendDetailResponse {
    /// Distribution schemes, newest first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<DividendDetailItem>>,
}

/// One distribution scheme in `dividend_detail`'s `list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DividendDetailItem {
    /// Human-readable scheme, e.g. "1股派息 \n0.25美元" (may contain a
    /// newline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// Upstream counter id; observed empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counter_id: Option<String>,
    /// Record date, e.g. "2026-09-10".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_date: Option<String>,
    /// Ex-dividend date, e.g. "2026-09-10".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ex_date: Option<String>,
    /// Payment date, e.g. "2026-10-01".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_date: Option<String>,
}

/// Returned by `forecast_eps`. Wraps an `items` array of EPS consensus
/// snapshots over time.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ForecastEpsResponse {
    /// EPS consensus snapshots, one per forecast window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ForecastEpsItem>>,
}

/// One snapshot in `forecast_eps`'s `items`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ForecastEpsItem {
    /// Snapshot window start (RFC3339; rewritten from a unix-epoch
    /// field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_start_date: Option<String>,
    /// Snapshot window end (RFC3339; rewritten from a unix-epoch
    /// field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_end_date: Option<String>,
    /// Mean analyst EPS forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_eps_mean: Option<String>,
    /// Median analyst EPS forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_eps_median: Option<String>,
    /// Lowest analyst EPS forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_eps_lowest: Option<String>,
    /// Highest analyst EPS forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast_eps_highest: Option<String>,
    /// Number of contributing institutions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub institution_total: Option<i64>,
    /// Number of institutions that revised up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub institution_up: Option<i64>,
    /// Number of institutions that revised down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub institution_down: Option<i64>,
}

/// Returned by `consensus`.
///
/// Both variants carry a per-period `list`. The generic shape adds
/// `current_index` / `currency` / `opt_periods` / `current_period` /
/// `legend` and per-metric `details[]` in each period; the US-account
/// variant (a `.US` symbol queried by a US account) adds `ai_summary` /
/// `opt_reports` / `report` and per-metric `revenue` / `eps` / `ebit`
/// objects instead.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConsensusResponse {
    /// Estimate-vs-actual records, one per fiscal period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<ConsensusPeriod>>,
    /// Index into `list` of the current (latest reported) period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_index: Option<i64>,
    /// Currency of the monetary figures.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Period types available for this symbol, e.g. `["qf", "saf",
    /// "af"]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opt_periods: Option<Vec<String>>,
    /// Period type used for `list`, e.g. "qf".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_period: Option<String>,
    /// Display name and definition of each metric, keyed by the
    /// `details[].key` value (e.g. "revenue", "eps").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<BTreeMap<String, ConsensusLegendEntry>>,
    /// US-account variant only: AI-generated summary of the latest
    /// consensus vs. actual results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_summary: Option<String>,
    /// US-account variant only: valid `report` values for this symbol,
    /// e.g. `["qf", "af"]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opt_reports: Option<Vec<String>>,
    /// US-account variant only: the report period actually used, e.g.
    /// "af".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
}

/// One fiscal period in `consensus`'s `list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConsensusPeriod {
    /// Fiscal year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fiscal_year: Option<i64>,
    /// Fiscal period within the year, e.g. "1".."4" for quarters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fiscal_period: Option<String>,
    /// Display label, e.g. "Q1 2027".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_text: Option<String>,
    /// Per-metric estimate vs. actual.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Vec<ConsensusDetail>>,
    /// US-account variant only: display label of the period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_txt: Option<String>,
    /// US-account variant only: revenue estimate vs. actual.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revenue: Option<UsConsensusEstimate>,
    /// US-account variant only: EPS estimate vs. actual.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eps: Option<UsConsensusEstimate>,
    /// US-account variant only: EBIT estimate vs. actual.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ebit: Option<UsConsensusEstimate>,
}

/// One metric in a `consensus` period's `details`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConsensusDetail {
    /// Metric key, e.g. "revenue", "ebit", "net_income", "eps". See the
    /// top-level `legend`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Whether the actual result for this period has been released.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_released: Option<bool>,
    /// Consensus estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<String>,
    /// Actual reported value. Absent until released.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    /// Actual vs. estimate outcome, e.g. "beat_est". Absent until
    /// released.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comp: Option<String>,
    /// Actual minus estimate. Absent until released.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comp_value: Option<String>,
    /// Localized description of `comp`, e.g. "超出预期".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comp_desc: Option<String>,
}

/// One metric's label in `consensus`'s `legend`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConsensusLegendEntry {
    /// Localized metric name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Localized metric definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// One estimate-vs-actual pair (e.g. `revenue`, `eps`, `ebit`) in a
/// US-account `consensus` period.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UsConsensusEstimate {
    /// Actual reported value. Empty string when not yet reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    /// Consensus estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<String>,
}

/// Returned by `valuation`. The tool always queries the PE indicator, so
/// `metrics` carries only `pe`.
///
/// US accounts querying a `.US` symbol get a US-specific variant whose
/// `metrics.pe` has different sub-fields (see `ValuationMetric`), plus the
/// top-level fields marked US-account only.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationResponse {
    /// Valuation indicator blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ValuationMetrics>,
    /// Lookback window in years.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<i64>,
    /// US-account variant only: which indicator `metrics.pe` describes,
    /// e.g. "pe".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator: Option<String>,
    /// US-account variant only: as-of date, e.g. "Jul 7, 2026".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// US-account variant only: currency symbol, e.g. "$".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy_symbol: Option<String>,
    /// US-account variant only: AI-generated summary (duplicates
    /// `metrics.pe.desc`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_summary: Option<String>,
}

/// `metrics` block of `valuation`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationMetrics {
    /// Price-to-earnings block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<ValuationMetric>,
}

/// The `metrics.pe` block of `valuation`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationMetric {
    /// Lower bound of the historical fair range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<String>,
    /// Historical median.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median: Option<String>,
    /// Upper bound of the historical fair range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<String>,
    /// Plain-text summary of where the current value sits (percentile,
    /// industry rank, industry median).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// Daily time series over the lookback window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<ValuationPoint>>,
    /// US-account variant only: industry median value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_median: Option<f64>,
    /// US-account variant only: current metric value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<f64>,
}

/// One sample in a valuation time series (`valuation` /
/// `valuation_history`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationPoint {
    /// Sample time (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Metric value at this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Returned by `valuation_history`. PE overview, historical band, industry
/// peers, and a name / market-cap lookup for the industry's stocks.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryResponse {
    /// Current-value overview.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<ValuationHistoryOverview>,
    /// Historical band and time series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<ValuationHistoryBlock>,
    /// Industry peer comparison.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peers: Option<ValuationHistoryPeers>,
    /// Industry stocks keyed by upstream counter id (e.g. "ST/US/AMD").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stocks: Option<BTreeMap<String, ValuationHistoryStock>>,
}

/// `overview` block of `valuation_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryOverview {
    /// Current value per indicator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ValuationHistoryOverviewMetrics>,
    /// Indicator the overview describes, e.g. "pe".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator: Option<String>,
    /// Lookback window in years.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<i64>,
    /// As-of date, as a localized display string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Currency symbol, e.g. "$" or "HK$".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy_symbol: Option<String>,
}

/// `overview.metrics` block of `valuation_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryOverviewMetrics {
    /// Price-to-earnings overview.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<ValuationHistoryOverviewMetric>,
}

/// One indicator in `valuation_history`'s `overview.metrics`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryOverviewMetric {
    /// Current value as a display string, e.g. "27.65x".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    /// Plain-text summary of where the current value sits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// Industry median value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_median: Option<String>,
}

/// `history` block of `valuation_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryBlock {
    /// Per-indicator historical band.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ValuationHistoryMetrics>,
    /// Lookback window in years.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<i64>,
}

/// `history.metrics` block of `valuation_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryMetrics {
    /// Price-to-earnings band and series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<ValuationBand>,
}

/// A historical band and time series in `valuation_history`'s
/// `history.metrics`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationBand {
    /// Lower bound of the historical fair range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<String>,
    /// Historical median.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median: Option<String>,
    /// Upper bound of the historical fair range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<String>,
    /// Plain-text summary of where the current value sits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// Monthly time series over the lookback window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<ValuationPoint>>,
}

/// `peers` block of `valuation_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryPeers {
    /// Peers compared by price-to-earnings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<ValuationHistoryPeerGroup>,
}

/// One indicator's peer comparison in `valuation_history`'s `peers`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryPeerGroup {
    /// Industry median value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_median: Option<String>,
    /// Selected peers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<ValuationHistoryPeer>>,
}

/// One peer in `valuation_history`'s `peers.*.list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryPeer {
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Security symbol, e.g. "AVGO.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// The peer's indicator value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// One entry of `valuation_history`'s `stocks` map.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ValuationHistoryStock {
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Market capitalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_cap: Option<String>,
}

/// Returned by `industry_valuation`. Wraps a `list` of the stock and its
/// industry peers.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationResponse {
    /// The queried stock and its peers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<IndustryValuationItem>>,
}

/// One stock in `industry_valuation`'s `list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationItem {
    /// Security symbol, e.g. "AAPL.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Market code, e.g. "US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Trading currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Latest close price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_close: Option<String>,
    /// Market capitalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_value: Option<String>,
    /// Trading volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    /// Price-to-earnings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<String>,
    /// Price-to-book.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pb: Option<String>,
    /// Price-to-sales.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ps: Option<String>,
    /// Dividend yield, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub div_yld: Option<String>,
    /// Return on assets, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roa: Option<String>,
    /// Return on equity, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roe: Option<String>,
    /// Asset turnover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turnover: Option<String>,
    /// Net profit margin, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_margin: Option<String>,
    /// Financial leverage (assets / equity).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leverage: Option<String>,
    /// Liabilities-to-assets ratio, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liabilities_assets: Option<String>,
    /// Earnings per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eps: Option<String>,
    /// Sales per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sales_ps: Option<String>,
    /// Book value per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bps: Option<String>,
    /// Dividend per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dps: Option<String>,
    /// Five-year average dividend per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub five_y_avg_dps: Option<String>,
    /// Dividend payout ratio, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub div_payout_ratio: Option<String>,
    /// Total assets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    /// Total liabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liabilities: Option<String>,
    /// Revenue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sales: Option<String>,
    /// Net income.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net_income: Option<String>,
    /// Historical PE/PB/PS samples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<IndustryValuationHistoryPoint>>,
}

/// One history point in `industry_valuation`'s nested `history`.
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
    /// Price-to-sales at this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ps: Option<String>,
}

/// Returned by `industry_valuation_dist`. Where the stock sits within its
/// industry's distribution, per indicator.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationDistResponse {
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

/// One indicator's distribution in `industry_valuation_dist`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryValuationDistribution {
    /// Industry low.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<String>,
    /// Industry median.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median: Option<String>,
    /// Industry high.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<String>,
    /// The stock's current value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The stock's percentile within the industry (0-1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking: Option<String>,
    /// The stock's rank position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_index: Option<String>,
    /// Number of ranked stocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank_total: Option<String>,
}

/// Returned by `company`. Company profile.
///
/// US accounts querying a `.US` symbol get a US-specific variant with only
/// the fields marked US-account only.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CompanyResponse {
    /// Short display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Full legal company name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,
    /// Business description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Founding date or year, e.g. "1999-11-23" or "1993".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub founded: Option<String>,
    /// Listing date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listing_date: Option<String>,
    /// Listing exchange name (localized).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Exchange ticker code, e.g. "00700".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticker: Option<String>,
    /// Market region, e.g. "HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// Upstream sector code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sector: Option<i64>,
    /// Company category (localized).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Registered address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Office address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub office_address: Option<String>,
    /// Postal code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zip_code: Option<String>,
    /// Company website.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    /// Logo image URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Phone number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// Fax number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fax: Option<String>,
    /// Investor-relations email.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Number of employees, as a string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employees: Option<String>,
    /// Chairman.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chairman: Option<String>,
    /// General manager / CEO.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manager: Option<String>,
    /// Company secretary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secretary: Option<String>,
    /// Legal representative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_repr: Option<String>,
    /// Fiscal year end, e.g. "12 月 31 日".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_end: Option<String>,
    /// IPO issue price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_price: Option<String>,
    /// Shares offered at IPO.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_offered: Option<String>,
    /// Auditor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_inst: Option<String>,
    /// Accounting firm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accounting_firm: Option<String>,
    /// Legal counsel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legal_counsel: Option<String>,
    /// Securities representative.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub securities_rep: Option<String>,
    /// Business license number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_license: Option<String>,
    /// ADS conversion ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ads_ratio: Option<String>,
    /// US-account variant only: market capitalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_cap: Option<String>,
    /// US-account variant only: short company tagline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intro: Option<String>,
    /// US-account variant only: currency symbol, e.g. "$".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ccy_symbol: Option<String>,
    /// US-account variant only: badges such as watchlist-popularity
    /// rank.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_rank_tags: Option<Vec<serde_json::Value>>,
    /// US-account variant only: link to the full company profile page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail_url: Option<String>,
    /// US-account variant only: community stock lists containing this
    /// symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_list: Option<Vec<serde_json::Value>>,
}

/// Returned by `executive`. Wraps a `professional_list` with one entry per
/// queried symbol.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ExecutiveResponse {
    /// Executive lists, one per symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub professional_list: Option<Vec<ExecutiveGroup>>,
}

/// One symbol's entry in `executive`'s `professional_list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ExecutiveGroup {
    /// Security symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Number of executives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    /// Executives and board members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub professionals: Option<Vec<ExecutiveMember>>,
    /// Link to the company's management page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_url: Option<String>,
}

/// One person in `executive`'s `professionals`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ExecutiveMember {
    /// Upstream person id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Full name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Title / role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Biography.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biography: Option<String>,
    /// Link to the person's profile page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_url: Option<String>,
}

/// Returned by `shareholder`. Wraps a `shareholder_list` of recent holding
/// changes.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderResponse {
    /// Shareholders with their latest reported change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shareholder_list: Option<Vec<ShareholderItem>>,
    /// Upstream total count (observed as 0 regardless of list length).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

/// One holder in `shareholder`'s `shareholder_list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderItem {
    /// Shareholder name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shareholder_name: Option<String>,
    /// Ownership, in percent of shares outstanding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_of_shares: Option<String>,
    /// Change in shares held (negative = reduced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_changed: Option<String>,
    /// Report date, e.g. "2026-10-02".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_date: Option<String>,
    /// Listed securities associated with this shareholder (e.g. the
    /// holder's own stock).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stocks: Option<Vec<ShareholderStock>>,
}

/// One related security in a `shareholder` row's `stocks`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderStock {
    /// Security symbol, e.g. "BLK.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Ticker code without market suffix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Market code, e.g. "US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Price change, e.g. "0.72%".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg: Option<String>,
}

/// Returned by `fund_holder`. Wraps a `lists` array of funds / ETFs holding
/// the symbol.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FundHolderResponse {
    /// Funds / ETFs that hold the symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lists: Option<Vec<FundHolderItem>>,
}

/// One fund in `fund_holder`'s `lists`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FundHolderItem {
    /// Fund symbol, e.g. "NVDX.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Fund name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Fund currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Weight of the symbol in the fund's portfolio, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_ratio: Option<String>,
    /// Holding report date, e.g. "2026.10.12".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_date: Option<String>,
}

/// Returned by `corp_action`. Wraps an `items` array of corporate events.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CorpActionResponse {
    /// Corporate events (earnings dates, dividends, splits, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<CorpActionItem>>,
}

/// One event in `corp_action`'s `items`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CorpActionItem {
    /// Upstream event id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Event date, `YYYYMMDD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Localized meaning of `date`, e.g. "派息日".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_type: Option<String>,
    /// Localized event category, e.g. "分配方案".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub act_type: Option<String>,
    /// Event description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub act_desc: Option<String>,
    /// Event kind code, e.g. "ReportDate", "DividendExDate".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Whether the event is the most recent one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent: Option<bool>,
    /// Whether the event was delayed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_delay: Option<bool>,
    /// Delay description; empty when not delayed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_content: Option<String>,
    /// Associated live broadcast (e.g. earnings call); null when none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live: Option<CorpActionLive>,
}

/// `live` block of a `corp_action` event.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CorpActionLive {
    /// Upstream live-broadcast id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Broadcast title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Upstream broadcast status code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i64>,
    /// Start time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// Whether the user has booked the broadcast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub booked: Option<bool>,
    /// Securities linked to the broadcast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<Vec<String>>,
}

/// Returned by `invest_relation`. Wraps an `invest_securities` array of
/// listed companies the symbol's company holds stakes in.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InvestRelationResponse {
    /// Invested listed securities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invest_securities: Option<Vec<InvestRelationItem>>,
    /// Link to the investment-relationship page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_url: Option<String>,
    /// Upstream total count (observed as 0 regardless of list length).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

/// One holding in `invest_relation`'s `invest_securities`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InvestRelationItem {
    /// Invested company name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,
    /// Invested company symbol, e.g. "HUYA.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Currency of `shares_value`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Stake, in percent of the company's shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_of_shares: Option<String>,
    /// Market value of the stake.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_value: Option<String>,
    /// Rank of this holding by value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_rank: Option<String>,
}

/// Returned by `operating`. Wraps a `list` of reporting periods with
/// management commentary and key indicators.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OperatingResponse {
    /// Reporting periods, newest first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<OperatingReport>>,
}

/// One reporting period in `operating`'s `list`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OperatingReport {
    /// Upstream report id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Report type, e.g. "qf", "saf", "af".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Period title, e.g. "2026 财年中报".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Management commentary / operating review text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txt: Option<String>,
    /// Whether this is the latest report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<bool>,
    /// Key financial indicators for the period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub financial: Option<OperatingFinancial>,
}

/// `financial` block of an `operating` report.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OperatingFinancial {
    /// Reporting currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Key indicators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicators: Option<Vec<OperatingIndicator>>,
}

/// One indicator in an `operating` report's `financial.indicators`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OperatingIndicator {
    /// Indicator key, e.g. "operating_revenue".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    /// Localized indicator name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator_name: Option<String>,
    /// Display value, e.g. "4592 亿".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator_value: Option<String>,
    /// Year-over-year change, in percent. Absent when not applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yoy: Option<String>,
}

/// Returned by `financial_report_latest`. Key indicators of the latest
/// financial report.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportLatestResponse {
    /// Report period code, e.g. "2026.H1".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Report period label, e.g. "2026 财年中报".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_txt: Option<String>,
    /// Reporting currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Key indicators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicators: Option<Vec<FinancialReportIndicator>>,
}

/// One indicator in `financial_report_latest`'s `indicators`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportIndicator {
    /// Indicator key, e.g. "operating_revenue", "net_profit", "eps",
    /// "roe".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    /// Localized indicator name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator_name: Option<String>,
    /// Value; ratio indicators carry a "%" suffix, e.g. "20.93%".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicator_value: Option<String>,
    /// Year-over-year change as a fraction (0.1627 = +16.27%); empty
    /// for ratio indicators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yoy: Option<String>,
}

/// Returned by `institution_rating_history`. Target-price range history and
/// rating-distribution history.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingHistoryResponse {
    /// Target-price range samples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_history: Option<Vec<TargetHistoryItem>>,
    /// Rating distributions; consecutive identical distributions are
    /// merged into one `start_date`..`end_date` span.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluate_history: Option<Vec<EvaluateHistoryItem>>,
}

/// One sample in `institution_rating_history`'s `target_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TargetHistoryItem {
    /// Sample time, unix epoch seconds as a string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Close price at this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<String>,
    /// Highest analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high_target_price: Option<String>,
    /// Lowest analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_target_price: Option<String>,
}

/// One span in `institution_rating_history`'s `evaluate_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EvaluateHistoryItem {
    /// Number of "buy" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy: Option<String>,
    /// Number of "outperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over: Option<String>,
    /// Number of "hold" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<String>,
    /// Number of "underperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<String>,
    /// Number of "sell" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell: Option<String>,
    /// Number of analysts with no opinion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_opinion: Option<String>,
    /// Span start, unix epoch seconds as a string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// Span end, unix epoch seconds as a string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

/// Returned by `institution_rating_industry_rank`. Industry peers ranked by
/// analyst ratings, plus the queried stock's own rank.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingIndustryRankResponse {
    /// Upstream industry id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_id: Option<i64>,
    /// Industry name (localized).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_name: Option<String>,
    /// The queried stock's rank within the industry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_rank: Option<i64>,
    /// Number of ranked stocks in the industry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_total: Option<i64>,
    /// Industry mean of the ranking metric, as upstream reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_mean: Option<i64>,
    /// Industry median of the ranking metric, as upstream reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_median: Option<i64>,
    /// Ranked peers for the requested page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<InstitutionRatingIndustryRankItem>>,
}

/// One peer in `institution_rating_industry_rank`'s `items`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionRatingIndustryRankItem {
    /// Rank within the industry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,
    /// Security symbol, e.g. "AAPL.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Number of "buy" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy: Option<i64>,
    /// Number of "outperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over: Option<i64>,
    /// Number of "hold" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<i64>,
    /// Number of "underperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<i64>,
    /// Number of "sell" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell: Option<i64>,
    /// Number of analysts with no opinion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_opinion: Option<i64>,
    /// Total number of ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}

/// Returned by `business_segments_history`. Wraps a `historical` array of
/// per-period segment snapshots.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BusinessSegmentsHistoryResponse {
    /// Per-period segment snapshots, oldest first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub historical: Option<Vec<BusinessSegmentsHistoryPeriod>>,
    /// Ids of the business segments that appear across `historical`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_ids: Option<Vec<String>>,
}

/// One period snapshot in `business_segments_history`'s `historical`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BusinessSegmentsHistoryPeriod {
    /// Period end date, `YYYYMMDD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Report type, e.g. "qf".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Period label, e.g. "2017.Q1".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_txt: Option<String>,
    /// Fiscal period start, e.g. "2016.12.31".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fp_start: Option<String>,
    /// Fiscal period end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fp_end: Option<String>,
    /// Report publication date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpt_date: Option<String>,
    /// Total revenue for the period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
    /// Year-over-year change of `total`, in percent. Absent for the
    /// first period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yoy: Option<String>,
    /// Reporting currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Revenue by business line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub business: Option<Vec<SegmentBreakdown>>,
    /// Revenue by region.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regionals: Option<Vec<SegmentBreakdown>>,
    /// Per-period business segment ids (observed empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bus_ids: Option<Vec<String>>,
    /// Per-period region segment ids (observed empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reg_ids: Option<Vec<String>>,
}

/// One segment entry in `business_segments_history` (`business[]` /
/// `regionals[]`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct SegmentBreakdown {
    /// Segment id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Segment / region name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Segment revenue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Share of total revenue, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<String>,
    /// Year-over-year change, in percent. Absent when there is no
    /// prior-year base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yoy: Option<String>,
}

/// Returned by `institutional_views`. Rating-distribution history (`elist`)
/// and target-price range history (`tlist`).
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionalViewsResponse {
    /// Rating-distribution snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elist: Option<Vec<InstitutionalViewsRating>>,
    /// Target-price range samples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tlist: Option<Vec<InstitutionalViewsTarget>>,
}

/// One snapshot in `institutional_views`'s `elist`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionalViewsRating {
    /// Snapshot date (RFC3339; rewritten from a unix-epoch field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Number of "buy" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy: Option<String>,
    /// Number of "outperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over: Option<String>,
    /// Number of "hold" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold: Option<String>,
    /// Number of "underperform" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<String>,
    /// Number of "sell" ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell: Option<String>,
    /// Number of analysts with no opinion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_opinion: Option<String>,
    /// Total number of ratings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
}

/// One sample in `institutional_views`'s `tlist`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct InstitutionalViewsTarget {
    /// Sample time, unix epoch seconds as a string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Close price at this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<String>,
    /// Highest analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high_price: Option<String>,
    /// Lowest analyst target price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_price: Option<String>,
}

/// Returned by `industry_peers`. A hierarchical sub-sector tree (`chain`)
/// plus the originating industry group (`top`). Both are null when the
/// symbol is not an industry symbol.
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
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryPeersNode {
    /// Node name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Industry code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Industry id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_id: Option<String>,
    /// Sharelist of the sub-sector's stocks; pass to sharelist_detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharelist_id: Option<String>,
    /// Node identifier, an industry symbol such as `"IN00258.US"`. Pass
    /// it back to `industry_peers` to walk into the sub-sector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
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
#[derive(Debug, Serialize, JsonSchema)]
pub struct IndustryPeersTop {
    /// Industry group name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Industry id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry_id: Option<String>,
}

/// Returned by `financial_report_snapshot`. Headline results (`fo_*`) and
/// financial ratios / statement highlights (`fr_*`) for one fiscal period.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinancialReportSnapshotResponse {
    /// Company display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Exchange ticker code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticker: Option<String>,
    /// Market code, e.g. "US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Reporting currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Report type of this snapshot, e.g. "qf".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Report types available, e.g. `["qf", "saf", "af"]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opt_reports: Option<Vec<String>>,
    /// Fiscal year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fiscal_year: Option<i64>,
    /// Fiscal period within the year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fiscal_period: Option<String>,
    /// Fiscal period start, e.g. "2026.04.26".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fp_start: Option<String>,
    /// Fiscal period end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fp_end: Option<String>,
    /// Text summary of the results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_desc: Option<String>,
    /// Revenue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fo_revenue: Option<ForecastActual>,
    /// EBIT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fo_ebit: Option<ForecastActual>,
    /// Earnings per share.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fo_eps: Option<ForecastActual>,
    /// Revenue.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_revenue: Option<SnapshotFigure>,
    /// Net profit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_profit: Option<SnapshotFigure>,
    /// Net profit margin, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_profit_margin: Option<String>,
    /// Total assets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_total_assets: Option<SnapshotFigure>,
    /// Total liabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_total_liability: Option<SnapshotFigure>,
    /// Debt-to-assets ratio, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_debt_assets_ratio: Option<String>,
    /// Operating cash flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_operate_cash: Option<SnapshotFigure>,
    /// Investing cash flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_invest_cash: Option<SnapshotFigure>,
    /// Financing cash flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_finance_cash: Option<SnapshotFigure>,
    /// Trailing-twelve-month return on equity, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_roe_ttm: Option<String>,
    /// Trailing-twelve-month net profit margin, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_profit_margin_ttm: Option<String>,
    /// Trailing-twelve-month asset turnover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_asset_turn_ttm: Option<String>,
    /// Trailing-twelve-month financial leverage (assets / equity).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fr_leverage_ttm: Option<String>,
}

/// A headline result block in `financial_report_snapshot` (`fo_revenue` /
/// `fo_ebit` / `fo_eps`). The estimate / comparison fields are omitted when
/// upstream leaves them blank.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ForecastActual {
    /// Actual reported value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Year-over-year change, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yoy: Option<String>,
    /// Consensus estimate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub est_value: Option<String>,
    /// Year-over-year change implied by the estimate, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub est_yoy: Option<String>,
    /// Actual vs. estimate outcome code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmp: Option<String>,
    /// Localized description of `cmp`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmp_desc: Option<String>,
}

/// A value with its year-over-year change in `financial_report_snapshot`'s
/// `fr_*` blocks.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SnapshotFigure {
    /// Value for the period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Year-over-year change, in percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yoy: Option<String>,
}

/// Returned by `shareholder_top`. Wraps an `info` array of per-period
/// snapshots, each with a `share_holders` list.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderTopResponse {
    /// Period labels, in the same order as `info` (the first is "最新",
    /// the latest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub periods: Option<Vec<String>>,
    /// Per-period holder snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<Vec<ShareholderTopPeriod>>,
}

/// One period snapshot in `shareholder_top`'s `info`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderTopPeriod {
    /// Period label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Top holders for this period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_holders: Option<Vec<ShareholderTopHolder>>,
}

/// One holder in `shareholder_top`'s `share_holders`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShareholderTopHolder {
    /// Holder id, as a numeric string. Pass it (as an integer) to
    /// `shareholder_detail`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    /// Holder name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Filing date, e.g. "2026/06/30".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filing_date: Option<String>,
    /// Shares held.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_held: Option<String>,
    /// Ownership, e.g. "8.04%".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_shares_held: Option<String>,
    /// Change in shares held versus the prior filing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares_changed: Option<String>,
    /// Change in ownership, e.g. "0.12%".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent_shares_changed: Option<String>,
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
    /// Security symbol, e.g. `"AAPL.US"`.
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
