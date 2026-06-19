//! Typed output schemas for market / calendar tools whose post-transform JSON
//! shape is known statically.
//!
//! These tools are mostly HTTP pass-throughs (`http_get_tool` /
//! `http_get_tool_unix` / `http_post_tool_unix`) that stream the upstream
//! Longbridge response through the standard transform pipeline
//! (`transform_json`): keys are converted to snake_case, `counter_id`/
//! `counter_ids` are renamed to `symbol`/`symbols`, `aaid`/`account_channel`
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

/// Returned by `broker_holding`. Wraps an `items` array of top broker holdings
/// for an HK stock (HKEX CCASS participant disclosure). Subset of the wire
/// response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingResponse {
    /// Top broker holding entries for the requested period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<BrokerHoldingItem>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingItem {
    /// Broker (participant) name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broker_name: Option<String>,
    /// Shares held by this broker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_quantity: Option<String>,
    /// Change in shares held over the period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_change: Option<String>,
    /// Holding as a ratio of total issued shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_ratio: Option<String>,
}

/// Returned by `broker_holding_detail`. Wraps an `items` array of the full
/// broker holding list for an HK stock (HKEX CCASS participant disclosure).
/// Subset of the wire response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingDetailResponse {
    /// Full broker holding detail entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<BrokerHoldingDetailItem>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingDetailItem {
    /// Broker (participant) number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broker_id: Option<String>,
    /// Broker (participant) name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broker_name: Option<String>,
    /// Shares held by this broker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_quantity: Option<String>,
    /// Holding as a ratio of total issued shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_ratio: Option<String>,
    /// Change in shares held.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_change: Option<String>,
    /// Disclosure date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// Returned by `broker_holding_daily`. Wraps an `items` array of the daily
/// holding history for one broker in an HK stock. Subset of the wire response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingDailyResponse {
    /// Daily holding history entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<BrokerHoldingDailyItem>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerHoldingDailyItem {
    /// Disclosure date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Shares held by this broker on that date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_quantity: Option<String>,
    /// Change in shares held versus the prior day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_change: Option<String>,
    /// Holding as a ratio of total issued shares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_ratio: Option<String>,
}

/// Returned by `anomaly`. Wraps a `changes` array of unusual price/volume
/// alerts plus an `all_off` flag. Subset of the wire response — the
/// description marks `changes[]` as having further undocumented fields.
#[derive(Debug, Serialize, JsonSchema)]
pub struct AnomalyResponse {
    /// Anomaly alert entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<AnomalyChange>>,
    /// Whether anomaly alerting is globally off for the market.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_off: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AnomalyChange {
    /// Security symbol, e.g. "700.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Price change rate (decimal ratio).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_rate: Option<String>,
    /// Traded volume associated with the anomaly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
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
    /// Short company introduction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intro: Option<String>,
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
