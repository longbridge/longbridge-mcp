//! Typed output schemas for the quote-domain tools whose post-transform JSON
//! shape is a stable JSON object.
//!
//! Mirrors the shape produced by [`crate::tools::tool_json`] (and the HTTP
//! passthrough path) after the standard snake_case + RFC3339 + counter_id
//! transforms run against the upstream SDK / API response.
//!
//! Only tools whose response root is a JSON **object** appear here. The MCP
//! spec requires `outputSchema` to have root `type: "object"`, so the many
//! quote tools that return a top-level JSON array (`static_info`, `quote`,
//! `option_quote`, `warrant_quote`, `participants`, `trades`, `intraday`,
//! `candlesticks`, the history-candlestick pair, `option_chain_*`,
//! `capital_flow`, `trading_session`, `watchlist`, `filings`,
//! `warrant_issuers`, `warrant_list`, `calc_indexes`) are intentionally
//! omitted.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `security_list`. Top-level pagination envelope built in
/// `quote::security_list` around the upstream `Vec<Security>`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SecurityListResponse {
    /// Total number of securities available for this market/category (before
    /// pagination).
    pub total: usize,
    /// 1-based page number echoed back from the request.
    pub page: usize,
    /// Records-per-page echoed back from the request.
    pub count: usize,
    /// The securities on this page.
    pub items: Vec<SecurityListItem>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SecurityListItem {
    /// Security symbol, e.g. "AAPL.US".
    pub symbol: String,
    /// Security name (zh-CN).
    pub name_cn: String,
    /// Security name (en).
    pub name_en: String,
    /// Security name (zh-HK).
    pub name_hk: String,
}

/// Returned by `create_watchlist_group`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CreateWatchlistGroupResponse {
    /// The newly-created watchlist group ID. Pass this to
    /// `update_watchlist_group` / `delete_watchlist_group`.
    pub id: i64,
}

/// Returned by `delete_watchlist_group`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteWatchlistGroupResponse {
    /// The deleted watchlist group ID (echoed from the request).
    pub id: i64,
    /// Always `true` on success.
    pub deleted: bool,
}

/// Returned by `update_watchlist_group`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct UpdateWatchlistGroupResponse {
    /// The updated watchlist group ID (echoed from the request).
    pub id: i64,
    /// Always `true` on success.
    pub updated: bool,
}
