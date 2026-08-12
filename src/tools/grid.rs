//! Grid trading tools. Wrap the SDK `longbridge::grid::GridContext`
//! (parallel to `TradeContext`) — one `*Param` struct + one async fn per tool.

use longbridge::grid::{
    GetGridOrderDetailOptions, GetGridOrdersByIdsOptions, GetGridOrdersOptions,
    GetGridTriggerHistoryOptions, GridContext,
};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridSymbolParam {
    /// Security symbol, e.g. "700.HK".
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridListParam {
    /// Filter by symbol, e.g. "700.HK". Omit for all grid orders.
    pub symbol: Option<String>,
    /// Comma-joined status filter, e.g. "Performing,Suspended". Omit for all.
    pub status: Option<String>,
    /// Page number (default 1).
    pub page: Option<i32>,
    /// Records per page (default 20).
    pub limit: Option<i32>,
    /// Sort field (e.g. "created_at").
    pub sort_by: Option<String>,
    /// Sort order ("asc" / "desc").
    pub sort_order: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridIdsParam {
    /// Grid order IDs to fetch, e.g. ["123", "456"].
    pub order_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridDetailParam {
    /// Grid order ID.
    pub order_id: String,
    /// History cursor for paging the embedded trigger history.
    pub history_id: Option<String>,
    /// Page size for the embedded sub-order / history lists.
    pub limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridTriggerHistoryParam {
    /// Grid order ID whose trigger history to fetch.
    pub order_id: String,
    /// Page number (default 1).
    pub page: Option<i32>,
    /// Records per page (default 20).
    pub limit: Option<i32>,
}

pub async fn grid_symbol_info(
    mctx: &crate::tools::McpContext,
    p: GridSymbolParam,
) -> Result<CallToolResult, McpError> {
    let ctx = GridContext::new(mctx.create_config());
    let result = ctx.order_info(p.symbol).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn grid_list(
    mctx: &crate::tools::McpContext,
    p: GridListParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetGridOrdersOptions::new();
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    if let Some(status) = p.status {
        opts = opts.status(status);
    }
    if let Some(page) = p.page {
        opts = opts.page(page);
    }
    if let Some(limit) = p.limit {
        opts = opts.limit(limit);
    }
    if let Some(sort_by) = p.sort_by {
        opts = opts.sort_by(sort_by);
    }
    if let Some(sort_order) = p.sort_order {
        opts = opts.sort_order(sort_order);
    }
    let ctx = GridContext::new(mctx.create_config());
    let result = ctx.list(opts).await.map_err(Error::longbridge)?;
    // The SDK response wrapper is Deserialize-only; rebuild from its public
    // fields (the inner GridOrder items are Serialize).
    tool_json(&serde_json::json!({
        "grid_order": result.grid_order,
        "has_more": result.has_more,
    }))
}

pub async fn grid_list_by_ids(
    mctx: &crate::tools::McpContext,
    p: GridIdsParam,
) -> Result<CallToolResult, McpError> {
    let ctx = GridContext::new(mctx.create_config());
    let result = ctx
        .list_by_ids(GetGridOrdersByIdsOptions::new(p.order_ids))
        .await
        .map_err(Error::longbridge)?;
    // SDK returns a bare Vec<GridOrder>; wrap so the tool root is an object.
    tool_json(&serde_json::json!({ "grid_orders": result }))
}

pub async fn grid_detail(
    mctx: &crate::tools::McpContext,
    p: GridDetailParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetGridOrderDetailOptions::new(p.order_id);
    if let Some(history_id) = p.history_id {
        opts = opts.history_id(history_id);
    }
    if let Some(limit) = p.limit {
        opts = opts.limit(limit);
    }
    let ctx = GridContext::new(mctx.create_config());
    let result = ctx.detail(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn grid_trigger_history(
    mctx: &crate::tools::McpContext,
    p: GridTriggerHistoryParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetGridTriggerHistoryOptions::new(p.order_id);
    if let Some(page) = p.page {
        opts = opts.page(page);
    }
    if let Some(limit) = p.limit {
        opts = opts.limit(limit);
    }
    let ctx = GridContext::new(mctx.create_config());
    let result = ctx.trigger_history(opts).await.map_err(Error::longbridge)?;
    // Deserialize-only wrapper; rebuild from public fields.
    tool_json(&serde_json::json!({
        "trigger_orders": result.trigger_orders,
        "has_more": result.has_more,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn grid_context_type_is_reachable() {
        // Compile-time proof the PR-branch SDK exposes the grid module.
        fn _assert(_: fn(std::sync::Arc<longbridge::Config>) -> longbridge::grid::GridContext) {}
        _assert(longbridge::grid::GridContext::new);
    }
}
