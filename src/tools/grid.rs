//! Grid trading tools. Wrap the SDK `longbridge::grid::GridContext`
//! (parallel to `TradeContext`) — one `*Param` struct + one async fn per tool.

use std::str::FromStr;

use longbridge::Decimal;
use longbridge::grid::{
    GetGridOrderDetailOptions, GetGridOrdersByIdsOptions, GetGridOrdersOptions,
    GetGridTriggerHistoryOptions, GridContext, GridLimitEvent, GridTimeInForce, GridTradeRule,
    ReplaceGridOrderOptions, SubmitGridOrderOptions, SubmitStrategyQuestionnaireOptions,
    TriggerPriceType,
};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::dry_run;
use crate::tools::{tool_json, tool_result};

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
    let result = ctx.symbol_info(p.symbol).await.map_err(Error::longbridge)?;
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

/// Full grid trading rule — passed through to submit / replace verbatim.
/// Prices and quantities are decimal strings; enum-like fields are raw ints.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridRuleParam {
    /// Base price the grid is anchored to (decimal string).
    pub submitted_base_price: Option<String>,
    /// Upper price bound (decimal string).
    pub upper_limit_price: Option<String>,
    /// Lower price bound (decimal string).
    pub lower_limit_price: Option<String>,
    /// Trigger price type: 1 = spread (absolute), 2 = percent.
    pub trigger_price_type: Option<i32>,
    /// Upward trigger spread, absolute (decimal string; use with type 1).
    pub trigger_spread_up: Option<String>,
    /// Downward trigger spread, absolute (decimal string; use with type 1).
    pub trigger_spread_down: Option<String>,
    /// Upward trigger percent (decimal string; use with type 2).
    pub trigger_percent_up: Option<String>,
    /// Downward trigger percent (decimal string; use with type 2).
    pub trigger_percent_down: Option<String>,
    /// Whether one grid level may trigger multiple times.
    pub multiple_trigger: Option<bool>,
    /// Time in force: 0 = Day, 1 = GTC, 6 = GTD.
    pub time_in_force: Option<i32>,
    /// Quantity handled when the upper bound is reached (decimal string).
    pub upper_limit_quantity: Option<String>,
    /// Quantity handled when the lower bound is reached (decimal string).
    pub lower_limit_quantity: Option<String>,
    /// Expiry time in unix seconds (use with GTD).
    pub expire_time: Option<i64>,
    /// Action at upper bound: 1 = ignore (keep running), 2 = close at last price.
    pub upper_limit_event: Option<i32>,
    /// Action at lower bound: 1 = ignore (keep running), 2 = close at last price.
    pub lower_limit_event: Option<i32>,
    /// Sell-side order-book depth (-5..5; 0 = use grid_order_type_up).
    pub trigger_sell_depth: Option<i32>,
    /// Buy-side order-book depth (-5..5; 0 = use grid_order_type_down).
    pub trigger_buy_depth: Option<i32>,
    /// Quantity per trigger (decimal string).
    pub trigger_quantity: Option<String>,
    /// Whether short selling is allowed.
    pub support_shortsell: Option<bool>,
    /// Regular-trading-hours flag: 0 / 1 / 2.
    pub rth: Option<i32>,
    /// Sell-side order type when depth is 0: GMO / GLO / GTG.
    pub grid_order_type_up: Option<String>,
    /// Buy-side order type when depth is 0: GMO / GLO / GTG.
    pub grid_order_type_down: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridSubmitParam {
    /// Security symbol, e.g. "700.HK".
    pub symbol: String,
    /// Settlement currency, e.g. "HKD".
    pub settlement_currency: String,
    #[serde(flatten)]
    pub rule: GridRuleParam,
    /// The `confirmation_code` from this request's dry run. WITHOUT IT NOTHING
    /// IS SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed it. The code is single use, expires in 10
    /// minutes, and applies only to this exact request — change any field and
    /// it stops working. A grid strategy keeps placing orders on its own once
    /// live, so never quote the code back on your own initiative.
    pub execute: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridReplaceParam {
    /// Grid order ID to replace.
    pub order_id: String,
    #[serde(flatten)]
    pub rule: GridRuleParam,
    /// The `confirmation_code` from this request's dry run. WITHOUT IT NOTHING
    /// IS SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed it. The code is single use, expires in 10
    /// minutes, and applies only to this exact request — change any field and
    /// it stops working. A grid strategy keeps placing orders on its own once
    /// live, so never quote the code back on your own initiative.
    pub execute: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridOrderIdParam {
    /// Grid order ID.
    pub order_id: String,
    /// The `confirmation_code` from this request's dry run. WITHOUT IT NOTHING
    /// IS SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed it. The code is single use, expires in 10
    /// minutes, and applies only to this exact request — change any field and
    /// it stops working. A grid strategy keeps placing orders on its own once
    /// live, so never quote the code back on your own initiative.
    pub execute: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GridQuestionnaireParam {}

fn parse_decimal(field: &str, value: &Option<String>) -> Result<Option<Decimal>, McpError> {
    match value {
        Some(s) => Decimal::from_str(s)
            .map(Some)
            .map_err(|e| McpError::invalid_params(format!("invalid {field}: {e}"), None)),
        None => Ok(None),
    }
}

/// Validate an enum-like integer code against its allowed set before it is
/// mapped into an SDK enum. Under the SDK's `num_enum` `catch_all`, an
/// out-of-range code maps to `Unknown(i32)` and serializes verbatim to the
/// trading gateway — so reject it here rather than ship an unvalidated code.
fn checked_code(field: &str, value: Option<i32>, allowed: &[i32]) -> Result<Option<i32>, McpError> {
    match value {
        Some(v) if !allowed.contains(&v) => Err(McpError::invalid_params(
            format!("invalid {field}: {v} (allowed: {allowed:?})"),
            None,
        )),
        other => Ok(other),
    }
}

fn build_rule(p: GridRuleParam) -> Result<GridTradeRule, McpError> {
    Ok(GridTradeRule {
        submitted_base_price: parse_decimal("submitted_base_price", &p.submitted_base_price)?,
        upper_limit_price: parse_decimal("upper_limit_price", &p.upper_limit_price)?,
        lower_limit_price: parse_decimal("lower_limit_price", &p.lower_limit_price)?,
        trigger_price_type: checked_code("trigger_price_type", p.trigger_price_type, &[1, 2])?
            .map(TriggerPriceType::from),
        trigger_spread_up: parse_decimal("trigger_spread_up", &p.trigger_spread_up)?,
        trigger_spread_down: parse_decimal("trigger_spread_down", &p.trigger_spread_down)?,
        trigger_percent_up: parse_decimal("trigger_percent_up", &p.trigger_percent_up)?,
        trigger_percent_down: parse_decimal("trigger_percent_down", &p.trigger_percent_down)?,
        multiple_trigger: p.multiple_trigger,
        time_in_force: checked_code("time_in_force", p.time_in_force, &[0, 1, 6])?
            .map(GridTimeInForce::from),
        upper_limit_quantity: parse_decimal("upper_limit_quantity", &p.upper_limit_quantity)?,
        lower_limit_quantity: parse_decimal("lower_limit_quantity", &p.lower_limit_quantity)?,
        expire_time: p.expire_time,
        upper_limit_event: checked_code("upper_limit_event", p.upper_limit_event, &[1, 2])?
            .map(GridLimitEvent::from),
        lower_limit_event: checked_code("lower_limit_event", p.lower_limit_event, &[1, 2])?
            .map(GridLimitEvent::from),
        trigger_sell_depth: p.trigger_sell_depth,
        trigger_buy_depth: p.trigger_buy_depth,
        trigger_quantity: parse_decimal("trigger_quantity", &p.trigger_quantity)?,
        support_shortsell: p.support_shortsell,
        rth: p.rth,
        grid_order_type_up: p.grid_order_type_up,
        grid_order_type_down: p.grid_order_type_down,
    })
}

pub async fn grid_submit(
    mctx: &crate::tools::McpContext,
    p: GridSubmitParam,
) -> Result<CallToolResult, McpError> {
    let execute = p.execute.clone();
    // Build (and validate) the rule first so the dry run reports the same errors
    // a real submit would.
    let rule = build_rule(p.rule)?;
    let opts = SubmitGridOrderOptions::new(p.symbol.clone(), p.settlement_currency.clone(), rule);
    let rule_json = serde_json::to_value(&opts).unwrap_or_default();
    // The whole rule is fingerprinted: a grid differing by one trigger price is
    // a different strategy, and must not inherit another preview's code.
    let request = dry_run::fingerprint(&[
        "grid_submit",
        &p.symbol,
        &p.settlement_currency,
        &rule_json.to_string(),
    ]);
    // Two-step by design: without a confirmation code this submits nothing.
    let Some(code) = execute else {
        return dry_run::result(
            &mctx.token,
            &request,
            serde_json::json!({
                "action": "grid_submit",
                "symbol": p.symbol,
                "settlement_currency": p.settlement_currency,
                "rule": rule_json,
            }),
        );
    };
    dry_run::consume(&mctx.token, &request, &code)?;
    let ctx = GridContext::new(mctx.create_config());
    let result = ctx.submit(opts).await.map_err(Error::longbridge)?;
    // Same envelope as the dry run so both outcomes validate against
    // `output::grid::GridSubmitResponse`, and `dry_run` alone tells them apart.
    tool_json(&serde_json::json!({
        "dry_run": false,
        "order_id": result.order_id,
    }))
}

pub async fn grid_replace(
    mctx: &crate::tools::McpContext,
    p: GridReplaceParam,
) -> Result<CallToolResult, McpError> {
    let execute = p.execute.clone();
    let rule = build_rule(p.rule)?;
    let opts = ReplaceGridOrderOptions::new(p.order_id.clone(), rule);
    let rule_json = serde_json::to_value(&opts).unwrap_or_default();
    let request = dry_run::fingerprint(&["grid_replace", &p.order_id, &rule_json.to_string()]);
    // Two-step by design: without a confirmation code this changes nothing.
    let Some(code) = execute else {
        return dry_run::result(
            &mctx.token,
            &request,
            serde_json::json!({
                "action": "grid_replace",
                "order_id": p.order_id,
                "rule": rule_json,
            }),
        );
    };
    dry_run::consume(&mctx.token, &request, &code)?;
    let ctx = GridContext::new(mctx.create_config());
    ctx.replace(opts).await.map_err(Error::longbridge)?;
    Ok(tool_result("grid order replaced".to_string()))
}

pub async fn grid_cancel(
    mctx: &crate::tools::McpContext,
    p: GridOrderIdParam,
) -> Result<CallToolResult, McpError> {
    let request = dry_run::fingerprint(&["grid_cancel", &p.order_id]);
    // Two-step by design: without a confirmation code this does nothing.
    let Some(code) = p.execute.clone() else {
        return dry_run::result(
            &mctx.token,
            &request,
            serde_json::json!({
                "action": "grid_cancel",
                "order_id": p.order_id,
            }),
        );
    };
    dry_run::consume(&mctx.token, &request, &code)?;
    let ctx = GridContext::new(mctx.create_config());
    ctx.cancel(p.order_id).await.map_err(Error::longbridge)?;
    Ok(tool_result("grid order cancelled".to_string()))
}

pub async fn grid_suspend(
    mctx: &crate::tools::McpContext,
    p: GridOrderIdParam,
) -> Result<CallToolResult, McpError> {
    let request = dry_run::fingerprint(&["grid_suspend", &p.order_id]);
    // Two-step by design: without a confirmation code this does nothing.
    let Some(code) = p.execute.clone() else {
        return dry_run::result(
            &mctx.token,
            &request,
            serde_json::json!({
                "action": "grid_suspend",
                "order_id": p.order_id,
            }),
        );
    };
    dry_run::consume(&mctx.token, &request, &code)?;
    let ctx = GridContext::new(mctx.create_config());
    ctx.suspend(p.order_id).await.map_err(Error::longbridge)?;
    Ok(tool_result("grid order suspended".to_string()))
}

pub async fn grid_restart(
    mctx: &crate::tools::McpContext,
    p: GridOrderIdParam,
) -> Result<CallToolResult, McpError> {
    let request = dry_run::fingerprint(&["grid_restart", &p.order_id]);
    // Two-step by design: without a confirmation code this does nothing.
    let Some(code) = p.execute.clone() else {
        return dry_run::result(
            &mctx.token,
            &request,
            serde_json::json!({
                "action": "grid_restart",
                "order_id": p.order_id,
            }),
        );
    };
    dry_run::consume(&mctx.token, &request, &code)?;
    let ctx = GridContext::new(mctx.create_config());
    ctx.restart(p.order_id).await.map_err(Error::longbridge)?;
    Ok(tool_result("grid order restarted".to_string()))
}

pub async fn grid_questionnaire(
    mctx: &crate::tools::McpContext,
    _p: GridQuestionnaireParam,
) -> Result<CallToolResult, McpError> {
    let ctx = GridContext::new(mctx.create_config());
    ctx.submit_strategy_questionnaire(SubmitStrategyQuestionnaireOptions::new())
        .await
        .map_err(Error::longbridge)?;
    Ok(tool_result(
        "strategy risk-disclosure questionnaire submitted".to_string(),
    ))
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
