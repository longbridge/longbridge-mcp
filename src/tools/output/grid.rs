//! Typed output schemas for the grid trading tools. Mirror the subset of the
//! SDK grid response shape that `tool_json` emits (snake_case, decimals as
//! strings, timestamps as RFC3339). All fields optional — documented subset.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `grid_submit`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GridSubmitResponse {
    /// The newly-created grid order ID. Pass to grid_detail / grid_cancel /
    /// grid_suspend / grid_restart / grid_replace.
    pub order_id: String,
}

/// A grid order summary (list rows). Documented subset of the SDK `GridOrder`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GridOrderSummary {
    /// Grid order ID. Pass to grid_detail / grid_cancel / grid_suspend / grid_restart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// Security symbol, e.g. "700.HK".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Display name of the security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_name: Option<String>,
    /// Order status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Grid strategy status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_status: Option<String>,
    /// Base price the grid is anchored to (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitted_base_price: Option<String>,
    /// Upper price bound (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_limit_price: Option<String>,
    /// Lower price bound (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_limit_price: Option<String>,
    /// Trigger price type (1 = spread, 2 = percent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_price_type: Option<i32>,
    /// Total quantity bought so far (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_buy_quantity: Option<String>,
    /// Total quantity sold so far (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_sell_quantity: Option<String>,
    /// Total realized profit balance (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_profit_balance: Option<String>,
    /// Settlement currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_currency: Option<String>,
    /// Creation time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Returned by `grid_list`. The SDK response keys the array as `grid_order`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GridListResponse {
    /// Grid orders on this page.
    pub grid_order: Vec<GridOrderSummary>,
    /// Whether more pages exist.
    pub has_more: bool,
}

/// Returned by `grid_list_by_ids`. Tool wraps the SDK `Vec<GridOrder>`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GridOrdersResponse {
    /// Grid orders matching the requested IDs.
    pub grid_orders: Vec<GridOrderSummary>,
}

/// A single triggered order. Documented subset of the SDK `TriggerOrder`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GridTriggerOrder {
    /// Triggered order ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Order status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Security symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Buy / sell direction (raw int).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<i32>,
    /// Order price (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    /// Order quantity (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    /// Executed average price (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_price: Option<String>,
    /// Executed total quantity (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_qty: Option<String>,
    /// Trigger time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_at: Option<String>,
}

/// Returned by `grid_trigger_history`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GridTriggerHistoryResponse {
    /// Triggered orders on this page.
    pub trigger_orders: Vec<GridTriggerOrder>,
    /// Whether more pages exist.
    pub has_more: bool,
}

/// Returned by `grid_detail`. Summary plus embedded sub-orders and history.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GridOrderDetailResponse {
    /// Grid order ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// Security symbol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Order status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Grid strategy status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_status: Option<String>,
    /// Suspension reason, if suspended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspend_reason: Option<String>,
    /// Base price the grid is anchored to (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitted_base_price: Option<String>,
    /// Upper price bound (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_limit_price: Option<String>,
    /// Lower price bound (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_limit_price: Option<String>,
    /// Settlement currency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_currency: Option<String>,
    /// GTD expiry time (RFC3339).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_time: Option<String>,
    /// Child orders spawned by the grid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_sub_orders: Option<Vec<serde_json::Value>>,
    /// Lifecycle history entries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_order_history: Option<Vec<serde_json::Value>>,
}

/// Returned by `grid_symbol_info`. Documented subset of the SDK `GridOrderInfo`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GridSymbolInfoResponse {
    /// Security name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Latest quote price (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_done: Option<String>,
    /// Board lot size (decimal string).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lot_size: Option<String>,
    /// Channel / authorization info: strategy grant flag, RTH support, currencies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_info: Option<serde_json::Value>,
}
