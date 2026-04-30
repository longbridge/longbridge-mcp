//! Typed output schemas for tools whose post-transform JSON shape is known
//! statically. Mirrors the shape produced by [`tool_json`] after the standard
//! snake_case + RFC3339 + counter_id transforms run against the upstream SDK
//! response.
//!
//! Each struct here is referenced from `#[tool(output_schema = ...)]` on the
//! corresponding tool method in [`super`]. We only declare a struct when:
//! - the upstream response shape is small and stable
//! - the tool's response is a JSON object (MCP spec requires root `type:
//!   "object"` for outputSchema)

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `submit_order`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OrderIdResponse {
    /// The newly-created order ID. Pass this to `cancel_order` /
    /// `replace_order` / `order_detail`.
    pub order_id: String,
}

/// Returned by `statement_export`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct StatementUrlResponse {
    /// Pre-signed HTTPS URL for downloading the statement JSON. Short-lived
    /// — fetch it promptly.
    pub url: String,
}

/// Returned by `estimate_max_purchase_quantity`.
///
/// Both quantities are `Decimal` upstream and become strings after the
/// `to_tool_json` serializer pipeline (snake_case + decimal stringification).
#[derive(Debug, Serialize, JsonSchema)]
pub struct EstimateMaxQtyResponse {
    /// Maximum buy/sell quantity using cash buying power.
    pub cash_max_qty: String,
    /// Maximum buy/sell quantity using margin buying power.
    pub margin_max_qty: String,
}

/// Returned by `margin_ratio`.
///
/// Decimals are stringified by `to_tool_json`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MarginRatioResponse {
    /// Initial-margin ratio (`im_factor`).
    pub im_factor: String,
    /// Maintenance-margin ratio (`mm_factor`).
    pub mm_factor: String,
    /// Forced close-out margin ratio (`fm_factor`).
    pub fm_factor: String,
}

/// Returned by `stock_positions`. Top-level wraps a `list` array
/// (one entry per linked broker channel), each carrying its own positions.
#[derive(Debug, Serialize, JsonSchema)]
pub struct StockPositionsResponse {
    /// Position channels — one entry per broker channel.
    pub list: Vec<StockPositionChannel>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StockPositionChannel {
    /// Broker channel identifier. Always emitted as `null` for privacy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_channel: Option<String>,
    /// Stock positions held in this channel.
    pub stock_info: Vec<StockPosition>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StockPosition {
    /// Security symbol, e.g. "700.HK".
    pub symbol: String,
    /// Display name of the security.
    pub symbol_name: String,
    /// Total holding quantity.
    pub quantity: String,
    /// Quantity available to sell (excludes locked / pending).
    pub available_quantity: String,
    /// Settlement currency, e.g. "USD" / "HKD".
    pub currency: String,
    /// Cost price (per the client's choice of average or diluted cost).
    pub cost_price: String,
    /// Market code, e.g. "US" / "HK".
    pub market: String,
    /// Holding quantity at market open (pre-market baseline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_quantity: Option<String>,
}

/// Returned by `fund_positions`. Same channel-list shape as
/// `StockPositionsResponse`, but with fund-specific position fields.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FundPositionsResponse {
    pub list: Vec<FundPositionChannel>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FundPositionChannel {
    /// Broker channel identifier. Always emitted as `null` for privacy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_channel: Option<String>,
    /// Fund positions held in this channel.
    pub fund_info: Vec<FundPosition>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FundPosition {
    /// Fund ISIN code.
    pub symbol: String,
    /// Display name of the fund.
    pub symbol_name: String,
    /// Settlement currency.
    pub currency: String,
    /// Number of fund units held.
    pub holding_units: String,
    /// Net asset value at last settlement.
    pub current_net_asset_value: String,
    /// Settlement timestamp (RFC3339).
    pub net_asset_value_day: String,
    /// Cost net asset value.
    pub cost_net_asset_value: String,
}

/// Returned by `trading_days`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TradingDaysResponse {
    /// Full trading days in the requested range (yyyy-mm-dd).
    pub trading_days: Vec<String>,
    /// Half-day trading sessions in the requested range (yyyy-mm-dd).
    pub half_trading_days: Vec<String>,
}

/// Returned by `market_temperature`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MarketTemperatureResponse {
    /// Temperature value (0-100).
    pub temperature: i32,
    /// Human-readable temperature description (locale-aware).
    pub description: String,
    /// Market valuation indicator (0-100).
    pub valuation: i32,
    /// Market sentiment indicator (0-100).
    pub sentiment: i32,
    /// Snapshot timestamp (RFC3339).
    pub timestamp: String,
}

/// Returned by `history_market_temperature`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct HistoryMarketTemperatureResponse {
    /// Granularity, e.g. "day".
    #[serde(rename = "type")]
    pub granularity: String,
    /// Per-period samples in chronological order.
    #[serde(rename = "list")]
    pub records: Vec<MarketTemperatureResponse>,
}

/// Returned by `capital_distribution`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CapitalDistributionResponse {
    /// Snapshot timestamp (RFC3339).
    pub timestamp: String,
    /// Inflow capital broken down by order size.
    pub capital_in: CapitalDistribution,
    /// Outflow capital broken down by order size.
    pub capital_out: CapitalDistribution,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CapitalDistribution {
    /// Capital from large orders.
    pub large: String,
    /// Capital from medium orders.
    pub medium: String,
    /// Capital from small orders.
    pub small: String,
}

/// Returned by `depth`. Snapshot of the bid/ask order book.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DepthResponse {
    /// Bid levels, best price first.
    pub bids: Vec<DepthLevel>,
    /// Ask levels, best price first.
    pub asks: Vec<DepthLevel>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DepthLevel {
    /// Position number (1-based, depth ordering).
    pub position: i32,
    /// Price at this level. May be null when the level is empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    /// Total quantity at this price level.
    pub volume: i64,
    /// Number of orders sitting at this price level.
    pub order_num: i64,
}

/// Returned by `brokers`. Bid/ask broker queues for a security.
#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokersResponse {
    /// Bid brokers, best price first.
    pub bid_brokers: Vec<BrokerLevel>,
    /// Ask brokers, best price first.
    pub ask_brokers: Vec<BrokerLevel>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BrokerLevel {
    /// Position number (1-based, depth ordering).
    pub position: i32,
    /// Broker IDs queueing at this level. Map them to names via `participants`.
    pub broker_ids: Vec<i32>,
}

/// Returned by `order_detail`. Single order with full lifecycle metadata.
#[derive(Debug, Serialize, JsonSchema)]
pub struct OrderDetailResponse {
    /// Order ID.
    pub order_id: String,
    /// Status enum (e.g. `Filled`, `WaitToNew`, `Canceled`).
    pub status: String,
    /// Security symbol, e.g. "700.HK".
    pub symbol: String,
    /// Display name of the security.
    pub stock_name: String,
    /// Submitted quantity.
    pub quantity: String,
    /// Quantity already executed.
    pub executed_quantity: String,
    /// Submitted limit price (null for market orders).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    /// Volume-weighted average executed price (null when unfilled).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_price: Option<String>,
    /// Order submission time (RFC3339).
    pub submitted_at: String,
    /// Buy or Sell.
    pub side: String,
    /// Order type enum, e.g. `LO`, `MO`, `LIT`.
    pub order_type: String,
    /// Latest price snapshot at order time (null if missing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_done: Option<String>,
    /// Trigger price for LIT/MIT/trailing orders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price: Option<String>,
    /// Reject message or remark.
    pub msg: String,
    /// Order tag (e.g. `Normal`, `LongTerm`).
    pub tag: String,
    /// Time-in-force: `Day` / `GTC` / `GTD`.
    pub time_in_force: String,
    /// GTD expiry date (yyyy-mm-dd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_date: Option<String>,
    /// Last update time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Conditional-order trigger time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_at: Option<String>,
    /// Trailing-stop trail amount (TSLPAMT).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_amount: Option<String>,
    /// Trailing-stop trail percent (TSLPPCT, decimal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trailing_percent: Option<String>,
    /// Trailing-stop limit offset (TSLPAMT/TSLPPCT).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_offset: Option<String>,
    /// Trigger status, e.g. `Deactive` / `Active` / `Released`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_status: Option<String>,
    /// Settlement currency.
    pub currency: String,
    /// Outside-RTH setting: `RTH_ONLY` / `ANY_TIME` / `OVERNIGHT`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outside_rth: Option<String>,
}
