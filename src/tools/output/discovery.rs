//! Typed output schemas for the IPO, stock-screener, and community-sharelist
//! tool families ("discovery" tools).
//!
//! Mirrors the shape produced by each tool *after* its post-transform pipeline
//! runs (snake_case + RFC3339 + `Decimal`→`String`).
//! Most tools here are HTTP passthroughs whose full upstream payload is not
//! statically known, so every struct below intentionally models only the
//! subset of fields named in the tool's `description`. All such fields are
//! `Option` because the passthrough may omit them, and callers should treat
//! these schemas as a documented subset rather than an exhaustive contract.
//!
//! MCP requires `outputSchema` roots to be JSON objects, so only object-rooted
//! responses get a struct here. Write tools that return an unspecified
//! "upstream API response" are deliberately omitted.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// One IPO in `ipo_subscriptions`. Subset of upstream fields; prices and
/// amounts are strings.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoSubscriptionItem {
    /// Security symbol, e.g. "6157.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Exchange code, e.g. "06157".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Business description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Issue price, or a range such as "3.920 ~ 5.070".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_price: Option<String>,
    /// Cash needed for one lot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrance_fee: Option<String>,
    /// Cash needed for one lot when financed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub financing_entrance_fee: Option<String>,
    /// Subscription currency, e.g. "HKD".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Last subscription day, e.g. "2026.10.20".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_end_date: Option<String>,
    /// Subscription cut-off (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_deadline: Option<String>,
    /// Days left to subscribe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_day: Option<i64>,
    /// Shares offered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_planned: Option<String>,
    /// Whether the account can subscribe now (1 = yes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_subscribe: Option<i64>,
    /// Subscription state code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<i64>,
    /// Promotional tags, e.g. financing terms.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// One market side of `ipo_subscriptions`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoSubscriptionFeed {
    /// IPOs open for subscription in this market.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<IpoSubscriptionItem>>,
}

/// Returned by `ipo_subscriptions`: the HK and US feeds under `{hk, us}`.
/// A side is null when that market has nothing open.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoSubscriptionsResponse {
    /// Hong Kong subscription / pre-filing feed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hk: Option<IpoSubscriptionFeed>,
    /// US subscription / pre-filing feed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub us: Option<IpoSubscriptionFeed>,
}

/// One IPO in `ipo_calendar`. Milestone dates are RFC3339.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoCalendarItem {
    /// Security symbol, e.g. "6157.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Exchange code, e.g. "06157".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Subscription start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_date: Option<String>,
    /// Subscription end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_end_date: Option<String>,
    /// Allotment result date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_date: Option<String>,
    /// Grey-market (dark pool) trading date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mart_date: Option<String>,
    /// Listing date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipo_date: Option<String>,
    /// IPO state code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<i64>,
    /// IPO stage code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_stage: Option<i64>,
    /// Display tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Returned by `ipo_calendar`: counts by stage plus the calendar entries.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoCalendarResponse {
    /// IPOs in subscription.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_num: Option<i64>,
    /// IPOs awaiting allotment results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_num: Option<i64>,
    /// IPOs in grey-market trading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preipo_num: Option<i64>,
    /// IPOs listing soon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipo_num: Option<i64>,
    /// Snapshot time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Calendar entries for upcoming and recent IPOs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<IpoCalendarItem>>,
}

/// One recently listed IPO. Prices are strings; changes are percentages.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoListedItem {
    /// Security symbol, e.g. "2636.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Exchange code, e.g. "02636".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Business description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Listing date (yyyymmdd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipo_date: Option<String>,
    /// Issue price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_price: Option<String>,
    /// Previous close.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_close: Option<String>,
    /// Latest price.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_done: Option<String>,
    /// Change since listing versus the issue price (%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipo_change: Option<String>,
    /// Grey-market change versus the issue price (%), HK only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preipo_change: Option<String>,
    /// Turnover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,
    /// Whether today is the listing day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listed_day: Option<bool>,
}

/// One market side of `ipo_listed`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoListedMarketFeed {
    /// Total recently listed IPOs in this market.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    /// Recently listed IPOs for this page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<IpoListedItem>>,
}

/// Returned by `ipo_listed`: the HK and US feeds under `{hk, us}`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoListedResponse {
    /// Hong Kong recently-listed feed.
    pub hk: IpoListedMarketFeed,
    /// US recently-listed feed.
    pub us: IpoListedMarketFeed,
}

/// Returned by `ipo_detail`. The tool combines three upstream payloads
/// (`profile`, `timeline`, `eligibility`) under one wrapper object. Each part
/// is a passthrough; only the documented portions are typed here.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoDetailResponse {
    /// Business overview / profile payload (passthrough, shape upstream-defined).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<serde_json::Value>,
    /// Timeline events. The upstream payload may wrap this differently; the
    /// documented portion is a list of `{event, date}` entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<serde_json::Value>,
    /// Subscription eligibility payload (passthrough, shape upstream-defined).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eligibility: Option<serde_json::Value>,
}

/// A single IPO order entry. Subset of upstream fields; amount fields are
/// stringified by the transform pipeline and `submitted_at` is RFC3339.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoOrderItem {
    /// IPO order ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// Security symbol, e.g. "6871.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Market code, e.g. "HK" / "US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Subscription quantity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    /// Total subscription amount (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_amount: Option<String>,
    /// Order status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Order submission time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<String>,
}

/// One side of the IPO orders feed (active or historical): `orders[]`, plus
/// `has_more` on the paginated history feed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoOrdersFeed {
    /// IPO order entries (documented subset of upstream fields).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orders: Option<Vec<IpoOrderItem>>,
    /// Whether more history pages exist (history feed only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
}

/// Returned by `ipo_orders`. Active orders and order history combined under an
/// `{orders, history}` wrapper object built by the tool.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoOrdersResponse {
    /// Active IPO orders feed.
    pub orders: IpoOrdersFeed,
    /// Historical IPO orders feed.
    pub history: IpoOrdersFeed,
}

/// Returned by `ipo_order_detail`. Passthrough of a single IPO order; the
/// documented subset is typed here. Amount fields are stringified decimals and
/// `submitted_at` is RFC3339.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoOrderDetailResponse {
    /// IPO order ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// Security symbol, e.g. "6871.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Market code, e.g. "HK" / "US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Subscription quantity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    /// Allotted quantity after the IPO drawing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allotted_quantity: Option<String>,
    /// Total subscription amount (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_amount: Option<String>,
    /// Order status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Order submission time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<String>,
}

/// One IPO in the profit/loss breakdown. Amounts are strings; rates are
/// fractions (0.2296 = 22.96%).
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoProfitLossItem {
    /// Security symbol, e.g. "9611.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_name: Option<String>,
    /// Market code, e.g. "HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Subscription date (yyyymmdd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipo_dt: Option<String>,
    /// Whether shares were allotted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_lucky: Option<bool>,
    /// Profit or loss on this IPO.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profit: Option<String>,
    /// Profit rate on this IPO (fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profit_rate: Option<String>,
}

/// Totals of the IPO profit/loss feed. Amounts are strings; rates are
/// fractions.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoProfitLossSummary {
    /// Period covered, e.g. "all".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// Net profit across all IPOs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sum_profit: Option<String>,
    /// Net profit rate (fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sum_profit_rate: Option<String>,
    /// Total gain on profitable IPOs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profit_amount: Option<String>,
    /// Share of IPOs that made money (fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profit_percentage: Option<String>,
    /// Total loss on losing IPOs (negative).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deficit_amount: Option<String>,
    /// Share of IPOs that lost money (fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deficit_percentage: Option<String>,
    /// Value of IPO shares still held.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holding_value: Option<String>,
    /// Proceeds of IPO shares sold.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sold_value: Option<String>,
    /// Total cost of allotted IPOs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipo_cost: Option<String>,
    /// Allotment rate (fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lucky_ipo_rate: Option<String>,
    /// Return on allotted IPOs (fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lucky_ipo_earning_rate: Option<String>,
    /// Whether any IPO shares were traded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_traded: Option<bool>,
    /// Last update (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// The per-stock side of the IPO profit/loss feed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoProfitLossItems {
    /// Per-IPO profit/loss entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipo_stocks: Option<Vec<IpoProfitLossItem>>,
}

/// Returned by `ipo_profit_loss`: `{summary, items}` built by the tool.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoProfitLossResponse {
    /// Aggregate profit/loss totals.
    pub summary: IpoProfitLossSummary,
    /// Per-IPO breakdown.
    pub items: IpoProfitLossItems,
}

/// A single screener strategy entry. Subset of upstream fields; the change
/// figure is stringified by the transform pipeline.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerStrategyItem {
    /// Strategy ID. Pass to `screener_search` `strategy_id` to run, or to
    /// `screener_strategy` to inspect the filter conditions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Strategy display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Strategy description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Market the strategy targets, e.g. "US" / "HK" / "CN" / "SG".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Trailing three-month change (stringified decimal / percentage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub three_months_chg: Option<String>,
    /// Risk classification label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
}

/// Returned by `screener_recommend_strategies` and `screener_user_strategies`.
/// The documented portion is `strategys[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerStrategiesResponse {
    /// Screener strategies (note the upstream `strategys` spelling).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategys: Option<Vec<ScreenerStrategyItem>>,
}

/// A single filter condition within a screener strategy. The `filter_` prefix
/// is stripped from `key` by the tool so it matches `screener_indicators` and
/// `screener_search` condition input.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerStrategyFilter {
    /// Indicator key (without the `filter_` prefix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Lower bound for the condition (string, may be empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    /// Upper bound for the condition (string, may be empty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
    /// Technical-indicator value selection for technical keys. Passthrough
    /// object whose shape depends on the indicator (see `screener_indicators`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tech_values: Option<serde_json::Value>,
}

/// The `filter` wrapper of a screener strategy, holding the condition list.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerStrategyFilterGroup {
    /// Filter conditions making up the strategy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<ScreenerStrategyFilter>>,
}

/// Returned by `screener_strategy`. Documented portion is `market` plus the
/// `filter.filters[]` condition list.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerStrategyResponse {
    /// Market the strategy targets, e.g. "US" / "HK" / "CN" / "SG".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Filter group containing the strategy's conditions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<ScreenerStrategyFilterGroup>,
}

/// A single indicator value attached to a screener search result row. The
/// `filter_` prefix is stripped from `key` by the tool; its display name and
/// unit are in the response's `legend`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerResultIndicator {
    /// Indicator key (without the `filter_` prefix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Indicator value (stringified by the transform pipeline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// A single screener search result row. Subset of upstream fields.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerResultItem {
    /// Internal counter id, e.g. "ST/US/AAPL".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counter_id: Option<String>,
    /// Security symbol, e.g. "AAPL.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Per-indicator values for this row (condition + extra-return columns).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicators: Option<Vec<ScreenerResultIndicator>>,
}

/// Returned by `screener_search`. Documented portion is `total` plus the
/// `items[]` result rows.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerSearchResponse {
    /// Total number of matching securities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    /// Per-indicator display metadata, keyed by indicator key: `{ key: { name,
    /// unit } }`. The `name`/`unit` labels are identical for a given key across
    /// every result row, so they are listed once here instead of being repeated
    /// on each row's `indicators[]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<serde_json::Value>,
    /// Result rows for the current page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ScreenerResultItem>>,
}

/// A single screener indicator's metadata. The `filter_` prefix is stripped
/// from `key` by the tool. `tech_values`, when present, is a synthesized schema
/// (`{tech_key: [{value, label}, ...]}`) built from `tech_indicators`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerIndicator {
    /// Indicator ID (negative for the built-in market/industry scope filters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// Indicator key (without the `filter_` prefix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Indicator display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Value unit; empty when unitless.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Number of decimal places for display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub places: Option<i64>,
    /// Upstream indicator category code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<i64>,
    /// For technical indicators: synthesized schema of accepted option values,
    /// keyed by technical sub-key, each mapping to a list of `{value, label}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tech_values: Option<serde_json::Value>,
}

/// A named group of screener indicators.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerIndicatorGroup {
    /// Group type identifier, e.g. "range", "ScaleFinancials".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_type: Option<String>,
    /// Group display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    /// Indicators in this group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indicators: Option<Vec<ScreenerIndicator>>,
}

/// Returned by `screener_indicators`. Documented portion is `groups[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerIndicatorsResponse {
    /// Indicator metadata grouped by category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groups: Option<Vec<ScreenerIndicatorGroup>>,
}

/// A single sharelist summary entry. Subset of upstream fields.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SharelistSummary {
    /// Sharelist ID; pass to sharelist_detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// List name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// List description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Number of securities in the list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stocks_count: Option<i64>,
    /// Number of subscribers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribers_count: Option<i64>,
    /// Whether the current user subscribes to the list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribed: Option<bool>,
    /// Today's change of the list (fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chg: Option<String>,
    /// Year-to-date change of the list (fraction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub this_year_chg: Option<String>,
    /// Creation time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Last edit time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    /// Creator profile; passthrough, shape upstream-defined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<serde_json::Value>,
    /// Constituents preview; passthrough, shape upstream-defined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stocks: Option<serde_json::Value>,
}

/// Returned by `sharelist_list` and `sharelist_popular`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SharelistListResponse {
    /// Sharelist summaries: the user's own lists for `sharelist_list`, the
    /// popular lists for `sharelist_popular`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharelists: Option<Vec<SharelistSummary>>,
    /// Lists the user subscribes to (`sharelist_list` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribed_sharelists: Option<Vec<SharelistSummary>>,
}

/// Returned by `sharelist_detail`: the list itself, the user's relation to
/// it, and per-constituent topics. Constituents carry no prices.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SharelistDetailResponse {
    /// The list: id, name, description, stocks_count, subscribers_count, chg, this_year_chg, creator and its stocks[]{symbol, name, market, code, intro, …} (no prices; pass the symbols to quote).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharelist: Option<serde_json::Value>,
    /// The user's relation to the list: {self, subscription, official, industry}.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<serde_json::Value>,
    /// Discussion topics per constituent; passthrough.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_topics: Option<serde_json::Value>,
    /// Watchlist group the list is synced to, or null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<serde_json::Value>,
}

/// Returned by `sharelist_create`. The created sharelist object; documented
/// fields are `id`, `name`, and `description`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SharelistCreateResponse {
    /// Newly-created sharelist ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// List name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// List description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
