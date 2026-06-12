//! Typed output schemas for account-related tools (trade / portfolio /
//! statement / atm) whose post-transform JSON shape is known statically.
//!
//! Mirrors the shape produced by [`crate::serialize::to_tool_json`] after the
//! standard snake_case + RFC3339 + counter_id transforms run against the
//! upstream SDK response. Same conventions as [`super`]: a struct is declared
//! only when the response root is a JSON object (MCP requires outputSchema root
//! `type: "object"`) and the shape is small and stable.
//!
//! Most account tools are intentionally absent here because their response root
//! is **not** an object and therefore has no valid `outputSchema`:
//! - `account_balance`, `cash_flow`, `today_orders`, `today_executions`,
//!   `history_orders`, `history_executions` all return a top-level JSON array.
//! - `cancel_order` / `replace_order` return plain text.
//! - `exchange_rate`, `profit_analysis`, `profit_analysis_detail`,
//!   `short_margin`, `bank_cards`, `withdrawals`, `deposits` are raw HTTP
//!   passthroughs whose unwrapped `data` shape is not statically typed.

use rmcp::schemars::JsonSchema;
use rmcp::serde::Serialize;

/// Returned by `statement_list`.
///
/// Wraps a `list` array of statement entries. The SDK's `StatementItem`
/// (`{ dt: i32, file_key: String }`) is emitted unchanged by the transform
/// pipeline: `dt` is a plain integer date (`yyyymmdd`, e.g. `20240115`) that is
/// not a `*_at` field and so is left as a number, and `file_key` does not match
/// the counter_id pattern.
#[derive(Debug, Serialize, JsonSchema)]
pub struct StatementListResponse {
    /// Available statements in the requested range.
    pub list: Vec<StatementItem>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StatementItem {
    /// Statement date as a `yyyymmdd` integer (e.g. `20240115`).
    pub dt: i32,
    /// Opaque file key identifying this statement. Pass to `statement_export`
    /// to obtain a pre-signed download URL.
    pub file_key: String,
}
