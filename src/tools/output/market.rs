//! Typed output schemas for market / calendar tools whose post-transform JSON
//! shape is known statically.
//!
//! These tools are mostly HTTP pass-throughs (`http_get_tool` /
//! `http_get_tool_unix` / `http_post_tool_unix`) that stream the upstream
//! Longbridge response through the standard transform pipeline
//! (`transform_json`): keys are converted to snake_case, `aaid`/`account_channel`
//! are nulled, `*_at` fields and timestamps become RFC3339 strings, and any
//! `unix_paths` the call site declares are converted to RFC3339 strings too.
//!
//! Because the upstream contracts are only partially documented (via each
//! tool's `description`), every field below is modeled as `Option` and each
//! struct documents that it is a **subset** of the wire response: unknown
//! extra fields are dropped by `#[serde(...)]`-free structs at serialize time
//! only if we owned the type, but here the wire JSON is emitted verbatim — the
//! struct exists purely to describe the shape to MCP clients via
//! `output_schema`. We therefore only declare fields the tool's description
//! states explicitly, and never invent ones.
//!
//! Each struct here is referenced from `#[tool(output_schema = ...)]` on the
//! corresponding tool method in [`crate::tools`]. We only declare a struct when
//! the response root is a JSON object (MCP requires `outputSchema` root
//! `type: "object"`). Tools whose documented field names contradict the actual
//! wire keys, or whose root is a heterogeneous union, are intentionally left
//! without a schema.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `market_status`. Wraps a `market_time` array, one entry per
/// market. Subset of the wire response — `trade_status` is mapped from the
/// upstream numeric code to a human label, and `timestamp` is converted to
/// RFC3339.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MarketStatusResponse {
    /// Per-market trading status entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_time: Option<Vec<MarketStatusEntry>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MarketStatusEntry {
    /// Market code, e.g. "US" / "HK" / "CN" / "SG".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Trading status label, e.g. Trading / Closed / Mid-Day Break /
    /// Pre-Market / Post-Market / Overnight / Unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_status: Option<String>,
    /// Status snapshot timestamp (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Delayed-quote trading status label (same value set as `trade_status`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_trade_status: Option<String>,
    /// Delayed-quote status timestamp (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_timestamp: Option<String>,
}

/// Returned by `broker_holding`. Top HKEX CCASS participants by net
/// shareholding change over the requested period (rct_1/rct_5/rct_20/rct_60),
/// split into net buyers and net sellers. Empty arrays for non-HK symbols.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingResponse {
    /// Participants with the largest net increase in holding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buy: Option<Vec<BrokerHoldingItem>>,
    /// Participants with the largest net decrease in holding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sell: Option<Vec<BrokerHoldingItem>>,
    /// Disclosure date of the data, e.g. "2026.01.02" (not RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// One participant row in `broker_holding`'s `buy[]` / `sell[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingItem {
    /// Participant (broker) name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// CCASS participant number, e.g. "B01451". Pass as `broker_id` to
    /// `broker_holding_daily`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parti_number: Option<String>,
    /// Net change in shares held over the period (integer string; negative
    /// for sellers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg: Option<String>,
    /// Upstream highlight flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strong: Option<bool>,
}

/// Returned by `broker_holding_detail`. Full HKEX CCASS participant holding
/// list for an HK stock. Empty `list` for non-HK symbols.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingDetailResponse {
    /// One row per participant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<BrokerHoldingDetailItem>>,
    /// Disclosure date of the data, e.g. "2026.01.02" (not RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// One participant row in `broker_holding_detail`'s `list[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingDetailItem {
    /// Participant (broker) name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// CCASS participant number. Pass as `broker_id` to `broker_holding_daily`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parti_number: Option<String>,
    /// Holding ratio (fraction of shares, e.g. "0.0108") and its changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<BrokerHoldingMetric>,
    /// Shares held and their changes (integer strings).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shares: Option<BrokerHoldingMetric>,
    /// Upstream highlight flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strong: Option<bool>,
}

/// A holding metric with its change over trailing windows.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingMetric {
    /// Current value (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Change over the 1-day window (cf. `broker_holding` period rct_1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg_1: Option<String>,
    /// Change over the 5-day window (cf. `broker_holding` period rct_5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg_5: Option<String>,
    /// Change over the 20-day window (cf. `broker_holding` period rct_20).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg_20: Option<String>,
    /// Change over the 60-day window (cf. `broker_holding` period rct_60).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg_60: Option<String>,
}

/// Returned by `broker_holding_daily`: one broker's daily holding history in
/// an HK stock, newest first.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingDailyResponse {
    /// Daily holding history entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<BrokerHoldingDailyItem>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingDailyItem {
    /// Disclosure date, e.g. "2026.10.15".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Shares held by this broker on that date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding: Option<String>,
    /// Change in shares held versus the prior day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg: Option<String>,
    /// Holding as a fraction of issued shares (0.3236 = 32.36%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<String>,
}

/// Returned by `anomaly`: recent unusual price/volume alerts plus an
/// `all_off` flag.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AnomalyResponse {
    /// Anomaly alert entries, newest first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<AnomalyChange>>,
    /// Whether anomaly alerting is globally off for the market.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_off: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AnomalyChange {
    /// Alert id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Security symbol, e.g. "700.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Alert kind code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_type: Option<i64>,
    /// Alert kind label, e.g. "急速拉升" (rapid rise).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_name: Option<String>,
    /// When the alert fired (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_time: Option<String>,
    /// Values behind the alert as display strings, e.g. ["+2.04%"].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_values: Option<Vec<String>>,
    /// Direction: 1 bullish, 2 bearish.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<i64>,
}

/// Returned by `short_trades`. Wraps a unified `data` array of daily short-sale
/// volume history for HK or US stocks. Market-specific fields are populated
/// only for their respective market (US: `nasdaq_vol`/`nyse_vol`; HK:
/// `balance`/`market_vol`). Subset of the wire response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ShortTradesResponse {
    /// Daily short-sale volume entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<ShortTradesItem>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ShortTradesItem {
    /// Trade date (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Daily short-sale volume in shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_vol: Option<String>,
    /// Short volume as a ratio of total volume (decimal, e.g. 0.36 = 36%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<String>,
    /// Close price for the day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<String>,
    /// US only — NASDAQ short volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nasdaq_vol: Option<String>,
    /// US only — NYSE short volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nyse_vol: Option<String>,
    /// HK only — outstanding short balance (HKD).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance: Option<String>,
    /// HK only — total market trading volume for the day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_vol: Option<String>,
}

/// Returned by `top_movers`. Wraps an `events` array of stocks whose price
/// fluctuation exceeded the 20-trading-day standard deviation, with correlated
/// news reasons, plus pagination metadata. Subset of the wire response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TopMoversResponse {
    /// Mover events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<TopMoverEvent>>,
    /// Last refresh time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Pagination cursor. Pass back verbatim as `next_params` to fetch the
    /// next page. Opaque object — exact fields are an implementation detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TopMoverEvent {
    /// Event time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Human-readable reason for the alert.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_reason: Option<String>,
    /// Alert type/category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_type: Option<String>,
    /// The stock that moved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock: Option<TopMoverStock>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TopMoverStock {
    /// Security symbol, e.g. "700.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Price change (decimal ratio, e.g. 0.0445 = +4.45%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    /// Latest traded price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_done: Option<String>,
    /// Tag labels associated with the stock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// Short company introduction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intro: Option<String>,
}

/// Returned by `rank_categories`. Wraps a `first_tags` array of rank tab
/// category configurations for the popularity leaderboard. Subset of the wire
/// response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RankCategoriesResponse {
    /// Top-level rank category tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_tags: Option<Vec<RankFirstTag>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RankFirstTag {
    /// Category key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Sub-categories. Pass a `second_tags[].key` to `rank_list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_tags: Option<Vec<RankSecondTag>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RankSecondTag {
    /// Tab key to pass to `rank_list` (e.g. "hot_all-us").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Market this tab covers, e.g. "US" / "HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}

/// Returned by `rank_list`. Wraps a `lists` array of ranked stocks for a
/// leaderboard tab, plus a refresh time. Subset of the wire response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct RankListResponse {
    /// Ranked stock entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lists: Option<Vec<RankListItem>>,
    /// Last refresh time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RankListItem {
    /// Security symbol, e.g. "700.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Latest traded price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_done: Option<String>,
    /// Price change (decimal ratio).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg: Option<String>,
    /// Net capital inflow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inflow: Option<String>,
    /// Total market capitalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_cap: Option<String>,
    /// Pre-/post-market price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_post_price: Option<String>,
    /// Pre-/post-market price change (decimal ratio).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_post_chg: Option<String>,
    /// Intraday amplitude.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amplitude: Option<String>,
    /// Turnover rate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turnover_rate: Option<String>,
    /// Volume ratio versus average.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_rate: Option<String>,
    /// 5-day price change (decimal ratio).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub five_day_chg: Option<String>,
    /// 10-day price change (decimal ratio).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ten_day_chg: Option<String>,
    /// 20-day price change (decimal ratio).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twenty_day_chg: Option<String>,
    /// Year-to-date price change (decimal ratio).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub this_year_chg: Option<String>,
    /// Industry/sector name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub industry: Option<String>,
}

/// Returned by `finance_calendar`. Wraps a `list` array of date buckets, each
/// holding an `infos` array of events. Subset of the wire response — the
/// event field set varies by `category` (report / dividend / split / ipo /
/// macrodata / closed) and is only partially documented, so only the keys the
/// merge/dedup pipeline relies on are modeled here.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FinanceCalendarResponse {
    /// Date buckets, sorted ascending by date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<FinanceCalendarBucket>>,
    /// Present and `true` when the walk stopped before covering the whole
    /// requested window, so `list` is known to be incomplete.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
    /// Why the result is partial, and which part of the window is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_reason: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FinanceCalendarBucket {
    /// Bucket date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Events occurring on this date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub infos: Option<Vec<FinanceCalendarEvent>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FinanceCalendarEvent {
    /// Event ID (may be empty for events without one, e.g. market closures).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Security symbol when the event is stock-specific, e.g. "AAPL.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Event time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datetime: Option<String>,
    /// Market code, e.g. "US" / "HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}
