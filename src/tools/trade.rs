use longbridge::trade::{GetTodayExecutionsOptions, GetTodayOrdersOptions, TradeContext};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::http_client::http_get_tool;
use crate::tools::support::parse;
use crate::tools::{tool_json, tool_result};

pub use crate::tools::quote::SymbolParam;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrderIdParam {
    /// Order ID (from today's orders or order history)
    pub order_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AccountBalanceParam {
    /// Filter by currency code (e.g. "USD", "HKD"). Omit to return all currencies.
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodayOrdersParam {
    /// Filter by symbol, e.g. "700.HK". Omit to return all today's orders.
    pub symbol: Option<String>,
    /// US accounts only: filter by side, "Buy" or "Sell". Omit for all.
    pub us_action: Option<String>,
    /// US accounts only: page number (default 1).
    pub us_page: Option<i32>,
    /// US accounts only: page size (default 20).
    pub us_limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodayExecutionsParam {
    /// Filter by symbol, e.g. "700.HK".
    pub symbol: Option<String>,
    /// Filter by a specific order_id.
    pub order_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubmitOrderParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
    /// Order type (HK supports all; US supports LO/MO/LIT/MIT/TSLPAMT/TSLPPCT only):
    /// - LO (Limit Order): requires submitted_price
    /// - ELO (Enhanced Limit Order, HK only): requires submitted_price
    /// - MO (Market Order): no price required
    /// - AO (At-auction Order, HK only): executed at auction price, no price required
    /// - ALO (At-auction Limit Order, HK only): requires submitted_price
    /// - ODD (Odd Lots Order, HK only): requires submitted_price, for non-standard lot sizes
    /// - LIT (Limit If Touched): requires submitted_price and trigger_price; activates when market price touches trigger_price
    /// - MIT (Market If Touched): requires trigger_price only; executes at market when trigger_price is touched
    /// - TSLPAMT (Trailing Limit If Touched by Amount): requires trailing_amount and limit_offset; trailing stop by fixed amount
    /// - TSLPPCT (Trailing Limit If Touched by Percent): requires trailing_percent (0-1) and limit_offset; trailing stop by percentage
    /// - SLO (Special Limit Order, HK only): requires submitted_price; cannot be replaced after submission
    pub order_type: String,
    /// Buy or Sell
    pub side: String,
    /// Order quantity (number of shares)
    pub submitted_quantity: String,
    /// Order validity: "Day" (Day Order, expires end of session), "GTC" (Good Til Canceled), "GTD" (Good Til Date, requires expire_date)
    pub time_in_force: String,
    /// Limit price. Required for: LO, ELO, ALO, ODD, LIT, SLO
    pub submitted_price: Option<String>,
    /// Trigger (activation) price. Required for: LIT, MIT, TSLPAMT, TSLPPCT
    pub trigger_price: Option<String>,
    /// Limit offset from the trailing stop price. Required for: TSLPAMT, TSLPPCT
    pub limit_offset: Option<String>,
    /// Trailing amount (absolute price distance). Required for TSLPAMT
    pub trailing_amount: Option<String>,
    /// Trailing percent as decimal (e.g. 0.05 = 5%). Required for TSLPPCT
    pub trailing_percent: Option<String>,
    /// Expiry date (yyyy-mm-dd). Required when time_in_force is GTD
    pub expire_date: Option<String>,
    /// Outside regular trading hours: "RTH_ONLY" (regular trading hours only), "ANY_TIME" (any time including pre/post market), "OVERNIGHT" (overnight session, US only)
    pub outside_rth: Option<String>,
    /// Order remark (max 255 characters)
    pub remark: Option<String>,
    /// Set to true ONLY to actually place/modify/cancel the order.
    ///
    /// Omitted or false (the default) makes this a DRY RUN: the request is
    /// validated and echoed back, and nothing reaches the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again with `execute: true` only after the
    /// user has explicitly confirmed that exact order. Never set it on your own
    /// initiative, and never set it in the same turn the user first asks.
    pub execute: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReplaceOrderParam {
    /// Order ID to replace (returned by submit_order or listed in today_orders / history_orders)
    pub order_id: String,
    /// New order quantity (number of shares)
    pub quantity: String,
    /// New limit price (for limit-style orders)
    pub price: Option<String>,
    /// New trigger (activation) price (for LIT / MIT / trailing-stop orders)
    pub trigger_price: Option<String>,
    /// New limit offset from the trailing stop price (for TSLPAMT / TSLPPCT)
    pub limit_offset: Option<String>,
    /// New trailing amount as absolute price distance (for TSLPAMT)
    pub trailing_amount: Option<String>,
    /// New trailing percent as decimal e.g. 0.05 = 5% (for TSLPPCT)
    pub trailing_percent: Option<String>,
    /// Set to true ONLY to actually place/modify/cancel the order.
    ///
    /// Omitted or false (the default) makes this a DRY RUN: the request is
    /// validated and echoed back, and nothing reaches the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again with `execute: true` only after the
    /// user has explicitly confirmed that exact order. Never set it on your own
    /// initiative, and never set it in the same turn the user first asks.
    pub execute: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CancelOrderParam {
    /// Order ID to cancel (from today's orders or order history)
    pub order_id: String,
    /// Set to true ONLY to actually place/modify/cancel the order.
    ///
    /// Omitted or false (the default) makes this a DRY RUN: the request is
    /// validated and echoed back, and nothing reaches the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again with `execute: true` only after the
    /// user has explicitly confirmed that exact order. Never set it on your own
    /// initiative, and never set it in the same turn the user first asks.
    pub execute: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryOrdersParam {
    /// Filter by symbol (optional)
    pub symbol: Option<String>,
    /// Start time (RFC3339)
    pub start_at: String,
    /// End time (RFC3339)
    pub end_at: String,
    /// US accounts only, history_orders tool only: page number (default 1).
    pub us_page: Option<i32>,
    /// US accounts only, history_orders tool only: page size (default 20).
    pub us_limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CashFlowParam {
    /// Start time (RFC3339)
    pub start_at: String,
    /// End time (RFC3339)
    pub end_at: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EstimateMaxQtyParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
    /// Buy or Sell (case-insensitive; default: Buy)
    #[serde(default = "default_order_side")]
    pub side: String,
    /// Order type, case-insensitive (default: LO): LO (Limit Order) / ELO (Enhanced Limit Order) / MO (Market Order) / AO (At-auction) / ALO (At-auction Limit Order)
    #[serde(default = "default_order_type")]
    pub order_type: String,
    /// Limit price for limit-style orders. Omit for market orders.
    pub price: Option<String>,
}

fn default_order_side() -> String {
    "Buy".to_string()
}

fn default_order_type() -> String {
    "LO".to_string()
}

/// Best-effort snapshot of the order a cancel/replace preview is about to touch,
/// so the user can confirm it is the order they meant. A lookup failure must not
/// break the dry run, so every error collapses to `null`.
async fn preview_existing_order(
    mctx: &crate::tools::McpContext,
    ctx: &TradeContext,
    order_id: &str,
) -> serde_json::Value {
    if mctx.dc_region().await == longbridge::DcRegion::Us {
        let Ok(result) = ctx.us_order_detail(order_id.to_string()).await else {
            return serde_json::Value::Null;
        };
        let Ok(mut value) = serde_json::to_value(&result) else {
            return serde_json::Value::Null;
        };
        if let Some(order) = value.get_mut("order") {
            crate::tools::support::us_normalize::normalize_us_order(order);
        }
        return value;
    }
    match ctx.order_detail(order_id.to_string()).await {
        Ok(result) => serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
        Err(_) => serde_json::Value::Null,
    }
}

pub async fn account_balance(
    mctx: &crate::tools::McpContext,
    p: AccountBalanceParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx
        .account_balance(p.currency.as_deref())
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn stock_positions(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.stock_positions(None).await.map_err(Error::longbridge)?;
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    if mctx.dc_region().await == longbridge::DcRegion::Us
        && let Ok(us_overview) = ctx.us_asset_overview().await
        && let (Some(obj), Ok(mut us_value)) = (
            value.as_object_mut(),
            serde_json::to_value(&us_overview).map_err(Error::Serialize),
        )
    {
        crate::tools::support::us_normalize::normalize_us_stock_list(&mut us_value);
        obj.insert("us_asset_overview".to_string(), us_value);
    }
    tool_json(&value)
}

pub async fn fund_positions(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.fund_positions(None).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn margin_ratio(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx
        .margin_ratio(p.symbol)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn today_orders(
    mctx: &crate::tools::McpContext,
    p: TodayOrdersParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    if mctx.dc_region().await == longbridge::DcRegion::Us {
        let side = match p.us_action.as_deref() {
            Some(s) if s.eq_ignore_ascii_case("buy") => longbridge::trade::OrderSide::Buy,
            Some(s) if s.eq_ignore_ascii_case("sell") => longbridge::trade::OrderSide::Sell,
            _ => longbridge::trade::OrderSide::Unknown,
        };
        let now = time::OffsetDateTime::now_utc();
        let start_of_day = now.replace_time(time::Time::MIDNIGHT);
        let opts = longbridge::trade::GetUSHistoryOrders {
            symbol: p.symbol,
            side,
            // Confirmed via live testing that start_at/end_at do filter
            // correctly on the backend (unlike query_type, see below) — start
            // of the current UTC day gives "today" instead of a multi-month
            // window that would make this indistinguishable from
            // history_orders.
            start_at: start_of_day.unix_timestamp(),
            end_at: now.unix_timestamp(),
            // query_type (0=all/1=pending/2=history) does not actually filter
            // on the backend as of this writing (confirmed via live testing —
            // "pending" returned the identical set as "all", "history"
            // returned nothing despite matching orders existing) — always
            // request 0 (all) rather than expose a filter that silently does
            // nothing or hides real data.
            query_type: 0,
            page: p.us_page.unwrap_or(1),
            limit: p.us_limit.unwrap_or(20),
        };
        let result = ctx.us_query_orders(opts).await.map_err(Error::longbridge)?;
        let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
        if let Some(orders) = value.get_mut("orders").and_then(|v| v.as_array_mut()) {
            for order in orders {
                crate::tools::support::us_normalize::normalize_us_order(order);
            }
        }
        return tool_json(&value);
    }
    let mut opts = GetTodayOrdersOptions::new();
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    let result = ctx.today_orders(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn order_detail(
    mctx: &crate::tools::McpContext,
    p: OrderIdParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    if mctx.dc_region().await == longbridge::DcRegion::Us {
        let result = ctx
            .us_order_detail(p.order_id)
            .await
            .map_err(Error::longbridge)?;
        let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
        if let Some(order) = value.get_mut("order") {
            crate::tools::support::us_normalize::normalize_us_order(order);
        }
        if let Some(obj) = value.as_object_mut() {
            crate::tools::support::us_normalize::drop_empty(obj);
        }
        return tool_json(&value);
    }
    let result = ctx
        .order_detail(p.order_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn cancel_order(
    mctx: &crate::tools::McpContext,
    p: CancelOrderParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    // Two-step by design: without execute=true this cancels nothing.
    if !p.execute.unwrap_or(false) {
        let existing = preview_existing_order(mctx, &ctx, &p.order_id).await;
        return crate::tools::support::dry_run::result(serde_json::json!({
            "action": "cancel_order",
            "order_id": p.order_id,
            "order": existing,
        }));
    }
    ctx.cancel_order(p.order_id)
        .await
        .map_err(Error::longbridge)?;
    Ok(tool_result("order cancelled".to_string()))
}

pub async fn today_executions(
    mctx: &crate::tools::McpContext,
    p: TodayExecutionsParam,
) -> Result<CallToolResult, McpError> {
    use std::collections::HashMap;

    let mut exec_opts = GetTodayExecutionsOptions::new();
    let mut order_opts = GetTodayOrdersOptions::new();
    if let Some(ref symbol) = p.symbol {
        exec_opts = exec_opts.symbol(symbol.clone());
        order_opts = order_opts.symbol(symbol.clone());
    }
    if let Some(order_id) = p.order_id {
        exec_opts = exec_opts.order_id(order_id);
    }

    let (ctx, _) = TradeContext::new(mctx.create_config());
    let (executions, orders) = tokio::try_join!(
        ctx.today_executions(exec_opts),
        ctx.today_orders(order_opts),
    )
    .map_err(Error::longbridge)?;

    let side_map: HashMap<String, String> = orders
        .into_iter()
        .map(|o| (o.order_id, format!("{:?}", o.side)))
        .collect();

    let result: Vec<serde_json::Value> = executions
        .iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).unwrap_or_default();
            if let serde_json::Value::Object(ref mut map) = v {
                let side = side_map.get(&e.order_id).cloned().unwrap_or_default();
                map.insert("side".to_string(), serde_json::Value::String(side));
            }
            v
        })
        .collect();
    tool_json(&result)
}

pub async fn history_orders(
    mctx: &crate::tools::McpContext,
    p: HistoryOrdersParam,
) -> Result<CallToolResult, McpError> {
    let start = parse::parse_rfc3339(&p.start_at)?;
    let end = parse::parse_rfc3339(&p.end_at)?;
    let (ctx, _) = TradeContext::new(mctx.create_config());
    if mctx.dc_region().await == longbridge::DcRegion::Us {
        let opts = longbridge::trade::GetUSHistoryOrders {
            symbol: p.symbol,
            side: longbridge::trade::OrderSide::Unknown,
            start_at: start.unix_timestamp(),
            end_at: end.unix_timestamp(),
            // See the identical note in today_orders: query_type does not
            // filter on the backend as of this writing, so 2 ("history") was
            // confirmed to always return zero results even when matching
            // orders exist in range. 0 (all) is the only value confirmed to
            // return data.
            query_type: 0,
            page: p.us_page.unwrap_or(1),
            limit: p.us_limit.unwrap_or(20),
        };
        let result = ctx.us_query_orders(opts).await.map_err(Error::longbridge)?;
        let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
        if let Some(orders) = value.get_mut("orders").and_then(|v| v.as_array_mut()) {
            for order in orders {
                crate::tools::support::us_normalize::normalize_us_order(order);
            }
        }
        return tool_json(&value);
    }
    let mut opts = longbridge::trade::GetHistoryOrdersOptions::new()
        .start_at(start)
        .end_at(end);
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    let result = ctx.history_orders(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn history_executions(
    mctx: &crate::tools::McpContext,
    p: HistoryOrdersParam,
) -> Result<CallToolResult, McpError> {
    use std::collections::HashMap;

    let start = parse::parse_rfc3339(&p.start_at)?;
    let end = parse::parse_rfc3339(&p.end_at)?;

    let mut exec_opts = longbridge::trade::GetHistoryExecutionsOptions::new()
        .start_at(start)
        .end_at(end);
    let mut order_opts = longbridge::trade::GetHistoryOrdersOptions::new()
        .start_at(start)
        .end_at(end);
    if let Some(ref symbol) = p.symbol {
        exec_opts = exec_opts.symbol(symbol.clone());
        order_opts = order_opts.symbol(symbol.clone());
    }

    let (ctx, _) = TradeContext::new(mctx.create_config());
    let (executions, orders) = tokio::try_join!(
        ctx.history_executions(exec_opts),
        ctx.history_orders(order_opts),
    )
    .map_err(Error::longbridge)?;

    let side_map: HashMap<String, String> = orders
        .into_iter()
        .map(|o| (o.order_id, format!("{:?}", o.side)))
        .collect();

    let result: Vec<serde_json::Value> = executions
        .iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).unwrap_or_default();
            if let serde_json::Value::Object(ref mut map) = v {
                let side = side_map.get(&e.order_id).cloned().unwrap_or_default();
                map.insert("side".to_string(), serde_json::Value::String(side));
            }
            v
        })
        .collect();
    tool_json(&result)
}

pub async fn cash_flow(
    mctx: &crate::tools::McpContext,
    p: CashFlowParam,
) -> Result<CallToolResult, McpError> {
    let start = parse::parse_rfc3339(&p.start_at)?;
    let end = parse::parse_rfc3339(&p.end_at)?;
    let opts = longbridge::trade::GetCashFlowOptions::new(start, end);
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.cash_flow(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn submit_order(
    mctx: &crate::tools::McpContext,
    p: SubmitOrderParam,
) -> Result<CallToolResult, McpError> {
    use longbridge::Decimal;
    use longbridge::trade::{
        OrderSide, OrderType, OutsideRTH, SubmitOrderOptions, TimeInForceType,
    };
    use std::str::FromStr;

    let order_type = p
        .order_type
        .parse::<OrderType>()
        .map_err(|e| McpError::invalid_params(format!("invalid order_type: {e}"), None))?;
    let side = p
        .side
        .parse::<OrderSide>()
        .map_err(|e| McpError::invalid_params(format!("invalid side: {e}"), None))?;
    let quantity = Decimal::from_str(&p.submitted_quantity)
        .map_err(|e| McpError::invalid_params(format!("invalid quantity: {e}"), None))?;
    let tif = p
        .time_in_force
        .parse::<TimeInForceType>()
        .map_err(|e| McpError::invalid_params(format!("invalid time_in_force: {e}"), None))?;

    let mut opts = SubmitOrderOptions::new(p.symbol.clone(), order_type, side, quantity, tif);

    if let Some(ref price) = p.submitted_price {
        opts = opts.submitted_price(Decimal::from_str(price).map_err(|e| {
            McpError::invalid_params(format!("invalid submitted_price: {e}"), None)
        })?);
    }
    if let Some(ref price) = p.trigger_price {
        opts =
            opts.trigger_price(Decimal::from_str(price).map_err(|e| {
                McpError::invalid_params(format!("invalid trigger_price: {e}"), None)
            })?);
    }
    if let Some(ref v) = p.limit_offset {
        opts =
            opts.limit_offset(Decimal::from_str(v).map_err(|e| {
                McpError::invalid_params(format!("invalid limit_offset: {e}"), None)
            })?);
    }
    if let Some(ref v) = p.trailing_amount {
        opts = opts.trailing_amount(Decimal::from_str(v).map_err(|e| {
            McpError::invalid_params(format!("invalid trailing_amount: {e}"), None)
        })?);
    }
    if let Some(ref v) = p.trailing_percent {
        opts = opts.trailing_percent(Decimal::from_str(v).map_err(|e| {
            McpError::invalid_params(format!("invalid trailing_percent: {e}"), None)
        })?);
    }
    if let Some(ref date) = p.expire_date {
        opts = opts.expire_date(parse::parse_date(date)?);
    }
    if let Some(ref rth) = p.outside_rth {
        opts = opts
            .outside_rth(rth.parse::<OutsideRTH>().map_err(|e| {
                McpError::invalid_params(format!("invalid outside_rth: {e}"), None)
            })?);
    }
    if let Some(ref v) = p.remark {
        opts = opts.remark(v.clone());
    }

    // Two-step by design: without execute=true this places nothing.
    if !p.execute.unwrap_or(false) {
        return crate::tools::support::dry_run::result(serde_json::json!({
            "action": "submit_order",
            "symbol": p.symbol,
            "side": p.side,
            "order_type": p.order_type,
            "quantity": p.submitted_quantity,
            "time_in_force": p.time_in_force,
            "price": p.submitted_price,
            "trigger_price": p.trigger_price,
            "limit_offset": p.limit_offset,
            "trailing_amount": p.trailing_amount,
            "trailing_percent": p.trailing_percent,
            "expire_date": p.expire_date,
            "outside_rth": p.outside_rth,
            "remark": p.remark,
        }));
    }

    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.submit_order(opts).await.map_err(Error::longbridge)?;
    // Same envelope as the dry run so both outcomes validate against
    // `output::SubmitOrderResult`, and `dry_run` alone tells them apart.
    tool_json(&serde_json::json!({
        "dry_run": false,
        "order_id": result.order_id,
    }))
}

pub async fn replace_order(
    mctx: &crate::tools::McpContext,
    p: ReplaceOrderParam,
) -> Result<CallToolResult, McpError> {
    use longbridge::Decimal;
    use longbridge::trade::ReplaceOrderOptions;
    use std::str::FromStr;

    let quantity = Decimal::from_str(&p.quantity)
        .map_err(|e| McpError::invalid_params(format!("invalid quantity: {e}"), None))?;
    let mut opts = ReplaceOrderOptions::new(p.order_id.clone(), quantity);
    if let Some(ref v) = p.price {
        opts = opts.price(
            Decimal::from_str(v)
                .map_err(|e| McpError::invalid_params(format!("invalid price: {e}"), None))?,
        );
    }
    if let Some(ref v) = p.trigger_price {
        opts =
            opts.trigger_price(Decimal::from_str(v).map_err(|e| {
                McpError::invalid_params(format!("invalid trigger_price: {e}"), None)
            })?);
    }
    if let Some(ref v) = p.limit_offset {
        opts =
            opts.limit_offset(Decimal::from_str(v).map_err(|e| {
                McpError::invalid_params(format!("invalid limit_offset: {e}"), None)
            })?);
    }
    if let Some(ref v) = p.trailing_amount {
        opts = opts.trailing_amount(Decimal::from_str(v).map_err(|e| {
            McpError::invalid_params(format!("invalid trailing_amount: {e}"), None)
        })?);
    }
    if let Some(ref v) = p.trailing_percent {
        opts = opts.trailing_percent(Decimal::from_str(v).map_err(|e| {
            McpError::invalid_params(format!("invalid trailing_percent: {e}"), None)
        })?);
    }
    let (ctx, _) = TradeContext::new(mctx.create_config());
    // Two-step by design: without execute=true this changes nothing.
    if !p.execute.unwrap_or(false) {
        let existing = preview_existing_order(mctx, &ctx, &p.order_id).await;
        return crate::tools::support::dry_run::result(serde_json::json!({
            "action": "replace_order",
            "order_id": p.order_id,
            "current_order": existing,
            "new_quantity": p.quantity,
            "new_price": p.price,
            "new_trigger_price": p.trigger_price,
            "new_limit_offset": p.limit_offset,
            "new_trailing_amount": p.trailing_amount,
            "new_trailing_percent": p.trailing_percent,
        }));
    }
    ctx.replace_order(opts).await.map_err(Error::longbridge)?;
    Ok(tool_result("order replaced".to_string()))
}

pub async fn estimate_max_purchase_quantity(
    mctx: &crate::tools::McpContext,
    p: EstimateMaxQtyParam,
) -> Result<CallToolResult, McpError> {
    use longbridge::Decimal;
    use longbridge::trade::{EstimateMaxPurchaseQuantityOptions, OrderSide, OrderType};
    use std::str::FromStr;

    // SDK FromStr is case-sensitive ("Buy"/"LO"). Normalize first so callers can
    // pass any case: side -> "Buy"/"Sell", order_type -> upper-case (all variants
    // serialize as upper-case acronyms).
    let side_norm = match p.side.trim().to_ascii_lowercase().as_str() {
        "buy" => "Buy".to_string(),
        "sell" => "Sell".to_string(),
        _ => p.side.trim().to_string(),
    };
    let side = side_norm
        .parse::<OrderSide>()
        .map_err(|e| McpError::invalid_params(format!("invalid side: {e}"), None))?;
    let order_type = p
        .order_type
        .trim()
        .to_ascii_uppercase()
        .parse::<OrderType>()
        .map_err(|e| McpError::invalid_params(format!("invalid order_type: {e}"), None))?;
    let mut opts = EstimateMaxPurchaseQuantityOptions::new(p.symbol, order_type, side);
    if let Some(ref v) = p.price {
        opts = opts.price(
            Decimal::from_str(v)
                .map_err(|e| McpError::invalid_params(format!("invalid price: {e}"), None))?,
        );
    }
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx
        .estimate_max_purchase_quantity(opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

/// Get short margin deposit details for the current account.
pub async fn short_margin(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/asset/cash/short-margin", &[]).await
}

#[cfg(test)]
mod execute_gate_tests {
    //! The order-execution safety gate.
    //!
    //! `submit_order` / `cancel_order` / `replace_order` must stay dry-run by
    //! default, so a model can never move real money without a human first
    //! seeing the order. A failure here is a safety regression, not a chore.

    use super::{CancelOrderParam, ReplaceOrderParam, SubmitOrderParam};

    /// Every tool that can move real money. Grid writes count: a live grid keeps
    /// placing orders on its own, so it is at least as consequential as a single
    /// order.
    const GATED_TOOLS: [&str; 8] = [
        "submit_order",
        "cancel_order",
        "replace_order",
        "grid_submit",
        "grid_replace",
        "grid_cancel",
        "grid_suspend",
        "grid_restart",
    ];

    #[test]
    fn omitting_execute_deserializes_to_a_dry_run() {
        let submit: SubmitOrderParam = serde_json::from_value(serde_json::json!({
            "symbol": "TSLA.US",
            "order_type": "LO",
            "side": "Buy",
            "submitted_quantity": "10",
            "time_in_force": "Day",
        }))
        .expect("submit_order params without execute must deserialize");
        assert!(!submit.execute.unwrap_or(false));

        let cancel: CancelOrderParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1" }))
                .expect("cancel_order params without execute must deserialize");
        assert!(!cancel.execute.unwrap_or(false));

        let replace: ReplaceOrderParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1", "quantity": "10" }))
                .expect("replace_order params without execute must deserialize");
        assert!(!replace.execute.unwrap_or(false));

        let grid: crate::tools::grid::GridOrderIdParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1" }))
                .expect("grid cancel/suspend/restart params without execute must deserialize");
        assert!(!grid.execute.unwrap_or(false));
    }

    #[test]
    fn execute_is_an_optional_schema_property_on_every_gated_tool() {
        let tools = crate::tools::list_tools();
        for name in GATED_TOOLS {
            let tool = tools
                .iter()
                .find(|t| t.name == name)
                .unwrap_or_else(|| panic!("{name} must be a live tool"));
            let schema = tool.input_schema.as_ref();
            assert!(
                schema
                    .get("properties")
                    .and_then(|v| v.as_object())
                    .is_some_and(|props| props.contains_key("execute")),
                "{name} must expose an `execute` parameter"
            );
            // Required would force the model to answer the question every call;
            // optional-and-absent is what makes the default a dry run.
            let required = schema
                .get("required")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_default();
            assert!(
                !required.contains(&"execute"),
                "{name}'s `execute` must stay optional so the default is a dry run"
            );
        }
    }

    #[test]
    fn gated_output_schemas_admit_the_dry_run_shape() {
        // MCP requires every response from a tool with an `outputSchema` to
        // validate against it. The dry run has no order ID, so `order_id` must
        // not be required — otherwise the safe path returns an invalid result.
        let tools = crate::tools::list_tools();
        for name in ["submit_order", "grid_submit"] {
            let schema = tools
                .iter()
                .find(|t| t.name == name)
                .and_then(|t| t.output_schema.clone())
                .unwrap_or_else(|| panic!("{name} must declare an output schema"));
            let required = schema
                .get("required")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_default();
            assert_eq!(
                required,
                ["dry_run"],
                "{name}: only `dry_run` may be required so both outcomes validate"
            );
        }
    }

    #[test]
    fn every_gated_tool_description_states_the_two_step_protocol() {
        let tools = crate::tools::list_tools();
        for name in GATED_TOOLS {
            let description = tools
                .iter()
                .find(|t| t.name == name)
                .and_then(|t| t.description.clone())
                .unwrap_or_else(|| panic!("{name} must have a description"));
            for needle in ["execute=true", "DRY RUN", "confirm"] {
                assert!(
                    description.contains(needle),
                    "{name} description must mention `{needle}`"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::serialize::to_tool_json;

    /// Simulate the raw JSON that the Longbridge SDK's `FundPositionsResponse`
    /// would produce after serde serialization, then verify that `to_tool_json`
    /// transforms it correctly.
    #[allow(clippy::too_many_arguments)]
    fn sdk_fund_positions_json(
        account_channel: &str,
        symbol: &str,
        symbol_name: &str,
        currency: &str,
        holding_units: &str,
        current_nav: &str,
        cost_nav: &str,
        nav_day: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "list": [{
                "account_channel": account_channel,
                "fund_info": [{
                    "symbol": symbol,
                    "symbol_name": symbol_name,
                    "currency": currency,
                    "holding_units": holding_units,
                    "current_net_asset_value": current_nav,
                    "cost_net_asset_value": cost_nav,
                    "net_asset_value_day": nav_day
                }]
            }]
        })
    }

    #[test]
    fn fund_positions_all_fields_present() {
        let input = sdk_fund_positions_json(
            "lb",
            "HK0000038064",
            "高腾微金美元货币基金A",
            "USD",
            "1447.29",
            "15.22",
            "14.50",
            "2026-05-29T00:00:00Z",
        );
        let output = to_tool_json(&input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        let pos = &v["list"][0]["fund_info"][0];

        assert_eq!(pos["symbol"], "HK0000038064", "symbol mismatch: {output}");
        assert_eq!(
            pos["symbol_name"], "高腾微金美元货币基金A",
            "symbol_name mismatch: {output}"
        );
        assert_eq!(pos["currency"], "USD", "currency mismatch: {output}");
        assert_eq!(
            pos["holding_units"], "1447.29",
            "holding_units mismatch: {output}"
        );
        assert_eq!(
            pos["current_net_asset_value"], "15.22",
            "current_net_asset_value mismatch: {output}"
        );
        assert_eq!(
            pos["cost_net_asset_value"], "14.50",
            "cost_net_asset_value mismatch: {output}"
        );
        assert_eq!(
            pos["net_asset_value_day"], "2026-05-29T00:00:00Z",
            "net_asset_value_day mismatch: {output}"
        );
    }

    /// `account_channel` must be nulled by the transform regardless of the
    /// value returned by the SDK (privacy requirement).
    #[test]
    fn fund_positions_account_channel_nulled() {
        let input = sdk_fund_positions_json(
            "lb",
            "HK0000038064",
            "高腾微金美元货币基金A",
            "USD",
            "1447.29",
            "15.22",
            "14.50",
            "2026-05-29T00:00:00Z",
        );
        let output = to_tool_json(&input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(
            v["list"][0]["account_channel"].is_null(),
            "account_channel should be null, got: {output}"
        );
    }

    /// Regression: when the backend returns empty strings for `symbol_name` /
    /// `currency` and "0" for numeric fields, the response must still be valid
    /// JSON with those exact values preserved (not dropped or replaced).
    #[test]
    fn fund_positions_empty_fields_preserved() {
        let input = sdk_fund_positions_json(
            "lb",
            "HK0000038064",
            "",
            "",
            "0",
            "15.22",
            "0",
            "2026-05-29T00:00:00Z",
        );
        let output = to_tool_json(&input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        let pos = &v["list"][0]["fund_info"][0];

        assert_eq!(
            pos["symbol_name"], "",
            "symbol_name should be empty string: {output}"
        );
        assert_eq!(
            pos["currency"], "",
            "currency should be empty string: {output}"
        );
        assert_eq!(
            pos["holding_units"], "0",
            "holding_units should be \"0\": {output}"
        );
        assert_eq!(
            pos["cost_net_asset_value"], "0",
            "cost_nav should be \"0\": {output}"
        );
    }

    /// An account with no fund positions at all should produce `{"list": []}`.
    #[test]
    fn fund_positions_empty_list() {
        let input = serde_json::json!({ "list": [] });
        let output = to_tool_json(&input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(v["list"], serde_json::json!([]), "got: {output}");
    }

    /// `USOrderDetailResponse`'s top-level `order_histories`/
    /// `current_attached_order` are near-always empty/null in practice (the
    /// real state-transition log lives nested inside `order.order_histories`,
    /// which is normalized separately). The empty top-level duplicate must be
    /// dropped so callers don't mistake it for "no history exists".
    #[test]
    fn order_detail_envelope_drops_empty_top_level_fields() {
        use crate::tools::support::us_normalize::{drop_empty, normalize_us_order};

        let mut value = serde_json::json!({
            "order": {
                "symbol": "AAPL.US",
                "status": "FilledStatus",
                "order_histories": [{"status": "FilledStatus", "time": "1780925402"}]
            },
            "order_histories": [],
            "current_attached_order": null
        });

        if let Some(order) = value.get_mut("order") {
            normalize_us_order(order);
        }
        if let Some(obj) = value.as_object_mut() {
            drop_empty(obj);
        }

        assert!(
            value.get("order_histories").is_none(),
            "empty top-level order_histories should be dropped: {value}"
        );
        assert!(
            value.get("current_attached_order").is_none(),
            "null current_attached_order should be dropped: {value}"
        );
        assert_eq!(
            value["order"]["order_histories"][0]["occurred_at"],
            serde_json::json!("1780925402"),
            "nested order_histories must survive: {value}"
        );
    }
}
