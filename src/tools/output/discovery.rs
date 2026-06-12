//! Typed output schemas for the IPO, stock-screener, and community-sharelist
//! tool families ("discovery" tools).
//!
//! Mirrors the shape produced by each tool *after* its post-transform pipeline
//! runs (snake_case + RFC3339 + `counter_id`→`symbol` + `Decimal`→`String`).
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

/// A single IPO entry as it appears in the subscription / calendar / listed
/// feeds. Subset of the upstream item; field availability varies by feed and
/// market. Numeric/price fields are stringified by the transform pipeline.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoItem {
    /// Security symbol, e.g. "6871.HK" or "ARM.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Market code, e.g. "HK" / "US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
    /// Subscription window start date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_start_date: Option<String>,
    /// Subscription window end date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_end_date: Option<String>,
    /// Listing date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listing_date: Option<String>,
    /// Issue price (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_price: Option<String>,
    /// Minimum lot size for subscription.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_lot_size: Option<String>,
    /// IPO status (calendar feed), e.g. upcoming / listed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

/// One side (HK or US) of an IPO feed that splits results by market. Each side
/// is the raw upstream payload; the documented portion is `items[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoMarketFeed {
    /// IPO entries for this market (documented subset of upstream fields).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<IpoItem>>,
}

/// Returned by `ipo_subscriptions`. HK and US subscription feeds combined under
/// a `{hk, us}` wrapper object built by the tool.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoSubscriptionsResponse {
    /// Hong Kong subscription / pre-filing feed.
    pub hk: IpoMarketFeed,
    /// US subscription / pre-filing feed.
    pub us: IpoMarketFeed,
}

/// Returned by `ipo_calendar`. Passthrough of the upstream calendar payload;
/// the documented portion is `items[]`. The upstream `timestamp` is converted
/// to RFC3339 by the unix-path transform.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoCalendarResponse {
    /// Calendar entries for upcoming and recent IPOs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<IpoItem>>,
}

/// A single recently-listed IPO entry. Subset of upstream fields; numeric and
/// price fields are stringified by the transform pipeline.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoListedItem {
    /// Security symbol, e.g. "6871.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Listing date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listing_date: Option<String>,
    /// Issue price (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_price: Option<String>,
    /// First-day close price (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_day_close: Option<String>,
    /// First-day return (stringified decimal / percentage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_day_return: Option<String>,
    /// First-day trading volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    /// Market code, e.g. "HK" / "US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<String>,
}

/// One side (HK or US) of the listed feed. The documented portion is `items[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoListedMarketFeed {
    /// Recently-listed IPO entries (documented subset of upstream fields).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<IpoListedItem>>,
}

/// Returned by `ipo_listed`. HK and US listed feeds combined under a
/// `{hk, us}` wrapper object built by the tool.
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

/// One side of the IPO orders feed (active or historical). The documented
/// portion is `orders[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoOrdersFeed {
    /// IPO order entries (documented subset of upstream fields).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orders: Option<Vec<IpoOrderItem>>,
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

/// A single per-stock IPO profit/loss breakdown item. Subset of upstream
/// fields; monetary and rate fields are stringified by the transform pipeline.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoProfitLossItem {
    /// Security symbol, e.g. "6871.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Cost basis for this stock (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<String>,
    /// Current market value for this stock (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_value: Option<String>,
    /// Return rate for this stock (stringified decimal / percentage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_rate: Option<String>,
}

/// The summary side of the IPO profit/loss feed. Documented totals are
/// stringified decimals.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoProfitLossSummary {
    /// Total cost across all IPO holdings (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<String>,
    /// Total current value across all IPO holdings (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_value: Option<String>,
    /// Total return across all IPO holdings (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_return: Option<String>,
}

/// The items side of the IPO profit/loss feed. The documented portion is
/// `items[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoProfitLossItems {
    /// Per-stock profit/loss breakdown entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<IpoProfitLossItem>>,
}

/// Returned by `ipo_profit_loss`. Summary and per-stock breakdown combined
/// under a `{summary, items}` wrapper object built by the tool.
#[derive(Debug, Serialize, JsonSchema)]
pub struct IpoProfitLossResponse {
    /// Aggregate cost/value/return totals.
    pub summary: IpoProfitLossSummary,
    /// Per-stock breakdown items.
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
/// `filter_` prefix is stripped from `key` by the tool.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerResultIndicator {
    /// Indicator key (without the `filter_` prefix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Indicator display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Indicator value (stringified by the transform pipeline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Value unit, where applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// A single screener search result row. Subset of upstream fields.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerResultItem {
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
    /// Result rows for the current page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ScreenerResultItem>>,
}

/// Default value range for a screener indicator.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerIndicatorRange {
    /// Default lower bound (string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<String>,
    /// Default upper bound (string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<String>,
}

/// A single screener indicator's metadata. The `filter_` prefix is stripped
/// from `key` by the tool. `tech_values`, when present, is a synthesized schema
/// (`{tech_key: [{value, label}, ...]}`) describing the options a technical
/// indicator accepts.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerIndicator {
    /// Indicator ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Indicator key (without the `filter_` prefix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Indicator display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Value unit, where applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Default value range for the indicator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_range: Option<ScreenerIndicatorRange>,
    /// For technical indicators: synthesized schema of accepted option values,
    /// keyed by technical sub-key, each mapping to a list of `{value, label}`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tech_values: Option<serde_json::Value>,
}

/// A named group of screener indicators.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ScreenerIndicatorGroup {
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
    /// Sharelist ID.
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
    pub symbol_count: Option<i64>,
    /// Whether the current user owns this list (`sharelist_list` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_owner: Option<bool>,
    /// Number of followers / subscribers of this list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follower_count: Option<i64>,
    /// Creator info (`sharelist_popular` only); passthrough, shape
    /// upstream-defined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<serde_json::Value>,
}

/// Returned by `sharelist_list` and `sharelist_popular`. Documented portion is
/// `lists[]`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SharelistListResponse {
    /// Sharelist summaries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lists: Option<Vec<SharelistSummary>>,
}

/// A single constituent of a sharelist detail. Subset of upstream fields;
/// quote fields are stringified by the transform pipeline.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SharelistConstituent {
    /// Security symbol, e.g. "AAPL.US".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Latest traded price (stringified decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_done: Option<String>,
    /// Change rate (stringified decimal / percentage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_rate: Option<String>,
}

/// Returned by `sharelist_detail`. Subset of the upstream detail payload: list
/// metadata plus the constituent rows. Additional quote and subscription
/// fields may be present but are not enumerated here.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SharelistDetailResponse {
    /// Sharelist ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// List name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// List description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Constituent securities with quote snapshots.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constituents: Option<Vec<SharelistConstituent>>,
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
