use longbridge::trade::{
    CancelOrderOptions, GetOrderDetailOptions, GetTodayExecutionsOptions, GetTodayOrdersOptions,
    TradeContext,
};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::dry_run;
use crate::tools::support::http_client::http_get_tool;
use crate::tools::support::parse;
use crate::tools::{tool_json, tool_result};

pub use crate::tools::quote::SymbolParam;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrderDetailParam {
    /// Order ID to look up. A parent order ID, or (with is_attached=true) the
    /// ID of an attached take-profit / stop-loss leg.
    pub order_id: String,
    /// Set to true when order_id is the ID of an attached take-profit /
    /// stop-loss leg rather than a parent order. The response is then that leg
    /// itself, with charge_detail null. Omit (or false) for parent orders. Has
    /// no effect for US accounts, which are served by the US order endpoint.
    pub is_attached: Option<bool>,
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
    /// Filter by order ID: a parent order ID, or (with is_attached=true) the ID
    /// of an attached take-profit / stop-loss leg. Has no effect for
    /// US accounts, which are served by the US order endpoint.
    pub order_id: Option<String>,
    /// Only meaningful together with order_id: it says that order_id is the ID
    /// of an attached take-profit / stop-loss leg, and the response then
    /// carries that leg itself as an order entry. On its own it does nothing,
    /// and it has no effect for US accounts either.
    pub is_attached: Option<bool>,
    /// US accounts only: filter by side, "Buy" or "Sell". Omit for
    /// all. Ignored for AP accounts (the region is inferred from the
    /// account — do not pass it).
    pub us_action: Option<String>,
    /// US accounts only: page number (default 1). Ignored for
    /// AP accounts.
    pub us_page: Option<i32>,
    /// US accounts only: page size (default 20). Ignored for
    /// AP accounts.
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
    /// Attach a take-profit / stop-loss leg to this order: "PROFIT_TAKER"
    /// (take-profit only), "STOP_LOSS" (stop-loss only) or "BRACKET" (both).
    /// Omit for a plain order; every other attached_* field is ignored without
    /// it.
    pub attached_order_type: Option<String>,
    /// Take-profit trigger price. Required for PROFIT_TAKER and BRACKET.
    pub attached_profit_taker_price: Option<String>,
    /// Stop-loss trigger price. Required for STOP_LOSS and BRACKET.
    pub attached_stop_loss_price: Option<String>,
    /// Limit price of the take-profit leg, for an LO attached_activate_order_type.
    pub attached_profit_taker_submit_price: Option<String>,
    /// Limit price of the stop-loss leg, for an LO attached_activate_order_type.
    pub attached_stop_loss_submit_price: Option<String>,
    /// Time-in-force of the attached leg: "Day" / "GTC" / "GTD". Defaults to
    /// the parent order's setting when omitted.
    pub attached_time_in_force: Option<String>,
    /// Expiry of the attached leg as a unix timestamp in seconds (e.g.
    /// "1767139200"). Required when attached_time_in_force is GTD.
    pub attached_expire_time: Option<String>,
    /// Order type the attached leg is submitted as once triggered, e.g. "LO"
    /// (then set the matching attached_*_submit_price) or "MO".
    pub attached_activate_order_type: Option<String>,
    /// Outside-RTH setting of the triggered leg: "RTH_ONLY" / "ANY_TIME" /
    /// "OVERNIGHT".
    pub attached_outside_rth: Option<String>,
    /// The `confirmation_code` from this order's dry run. WITHOUT IT NOTHING IS
    /// SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed that exact order. The code is single use,
    /// expires in 10 minutes, and applies only to this exact order — change any
    /// field and it stops working. Never quote it back on your own initiative,
    /// and never in the same turn the user first asks.
    pub execute: Option<String>,
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
    /// Set to true to cancel every attached take-profit / stop-loss leg of this
    /// order, leaving the order itself in place.
    pub attached_cancel_all: Option<bool>,
    /// Attached leg to add or update: "PROFIT_TAKER", "STOP_LOSS" or "BRACKET".
    /// Required unless the only attached change is attached_cancel_all.
    pub attached_order_type: Option<String>,
    /// ID of the existing take-profit leg to update (from
    /// order_detail's attached_orders[]). Omit to add a new leg.
    pub attached_profit_taker_id: Option<String>,
    /// ID of the existing stop-loss leg to update (from order_detail's
    /// attached_orders[]). Omit to add a new leg.
    pub attached_stop_loss_id: Option<String>,
    /// New take-profit trigger price.
    pub attached_profit_taker_price: Option<String>,
    /// New stop-loss trigger price.
    pub attached_stop_loss_price: Option<String>,
    /// New limit price for the take-profit leg.
    pub attached_profit_taker_submit_price: Option<String>,
    /// New limit price for the stop-loss leg.
    pub attached_stop_loss_submit_price: Option<String>,
    /// New time-in-force for the attached leg: "Day" / "GTC" / "GTD".
    pub attached_time_in_force: Option<String>,
    /// New expiry for the attached leg as a unix timestamp in seconds.
    /// Required when attached_time_in_force is GTD.
    pub attached_expire_time: Option<String>,
    /// New order type for the triggered leg, e.g. "LO" or "MO".
    pub attached_activate_order_type: Option<String>,
    /// New outside-RTH setting for the triggered leg: "RTH_ONLY" / "ANY_TIME"
    /// / "OVERNIGHT".
    pub attached_outside_rth: Option<String>,
    /// ID of the parent order that owns the attached leg, when the leg is
    /// modified on its own rather than through its parent.
    pub attached_main_id: Option<String>,
    /// New quantity for the attached leg.
    pub attached_quantity: Option<String>,
    /// Reference market price for the attached leg.
    pub attached_market_price: Option<String>,
    /// The `confirmation_code` from this order's dry run. WITHOUT IT NOTHING IS
    /// SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed that exact order. The code is single use,
    /// expires in 10 minutes, and applies only to this exact order — change any
    /// field and it stops working. Never quote it back on your own initiative,
    /// and never in the same turn the user first asks.
    pub execute: Option<String>,
}

impl ReplaceOrderParam {
    /// Whether this replace touches the order's attached legs at all.
    ///
    /// Every attached field is optional and `attached_cancel_all` on its own is
    /// a complete request, so the presence of any one of them is what decides —
    /// sending attached params on a plain replace would change legs the caller
    /// never mentioned.
    fn has_attached_change(&self) -> bool {
        self.attached_cancel_all.is_some()
            || self.attached_order_type.is_some()
            || self.attached_profit_taker_id.is_some()
            || self.attached_stop_loss_id.is_some()
            || self.attached_profit_taker_price.is_some()
            || self.attached_stop_loss_price.is_some()
            || self.attached_profit_taker_submit_price.is_some()
            || self.attached_stop_loss_submit_price.is_some()
            || self.attached_time_in_force.is_some()
            || self.attached_expire_time.is_some()
            || self.attached_activate_order_type.is_some()
            || self.attached_outside_rth.is_some()
            || self.attached_main_id.is_some()
            || self.attached_quantity.is_some()
            || self.attached_market_price.is_some()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CancelOrderParam {
    /// Order ID to cancel (from today's orders or order history)
    pub order_id: String,
    /// Set to true to cancel an attached take-profit / stop-loss leg by its own
    /// order_id, leaving the parent order in place. Omit (or false) to cancel a
    /// parent order, which cancels its attached legs with it.
    pub is_attached: Option<bool>,
    /// The `confirmation_code` from this order's dry run. WITHOUT IT NOTHING IS
    /// SENT.
    ///
    /// Omitted (the default) makes this a DRY RUN: the request is validated and
    /// echoed back with a three-digit `confirmation_code`, and nothing reaches
    /// the exchange.
    ///
    /// Required protocol: call once without `execute`, show the returned
    /// preview to the user, and call again quoting the code only after the user
    /// has explicitly confirmed that exact order. The code is single use,
    /// expires in 10 minutes, and applies only to this exact order — change any
    /// field and it stops working. Never quote it back on your own initiative,
    /// and never in the same turn the user first asks.
    pub execute: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryOrdersParam {
    /// Filter by symbol (optional)
    pub symbol: Option<String>,
    /// Start time (RFC3339)
    pub start_at: String,
    /// End time (RFC3339)
    pub end_at: String,
    /// US accounts only: page number (default 1). Ignored for
    /// AP accounts (the region is inferred from the account — do not pass it).
    pub us_page: Option<i32>,
    /// US accounts only: page size (default 20). Ignored for
    /// AP accounts.
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
///
/// `is_attached` says `order_id` names an attached take-profit / stop-loss leg,
/// which lives in its own ID space: looking it up as a parent order would
/// preview the wrong order, or none at all.
async fn preview_existing_order(
    mctx: &crate::tools::McpContext,
    ctx: &TradeContext,
    order_id: &str,
    is_attached: bool,
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
    let mut opts = GetOrderDetailOptions::new(order_id);
    if is_attached {
        opts = opts.is_attached();
    }
    match ctx.order_detail(opts).await {
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
    if mctx.dc_region().await == longbridge::DcRegion::Us {
        // The US overview is supplementary: its failure should annotate the
        // positions rather than discard them, but staying silent would let the
        // caller read a US account as having no overview at all.
        match ctx.us_asset_overview().await {
            Ok(us_overview) => {
                if let (Some(obj), Ok(mut us_value)) = (
                    value.as_object_mut(),
                    serde_json::to_value(&us_overview).map_err(Error::Serialize),
                ) {
                    crate::tools::support::us_normalize::normalize_us_stock_list(&mut us_value);
                    obj.insert("us_asset_overview".to_string(), us_value);
                }
            }
            Err(e) => {
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "warnings".to_string(),
                        serde_json::json!([format!("us_asset_overview is unavailable: {e}")]),
                    );
                }
            }
        }
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
    if let Some(order_id) = p.order_id {
        opts = opts.order_id(order_id);
    }
    if p.is_attached == Some(true) {
        opts = opts.is_attached();
    }
    let result = ctx.today_orders(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn order_detail(
    mctx: &crate::tools::McpContext,
    p: OrderDetailParam,
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
    let mut opts = GetOrderDetailOptions::new(p.order_id);
    if p.is_attached == Some(true) {
        opts = opts.is_attached();
    }
    let result = ctx.order_detail(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn cancel_order(
    mctx: &crate::tools::McpContext,
    p: CancelOrderParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let is_attached = p.is_attached == Some(true);
    // Attached leg IDs live in their own ID space, so the same digits can name
    // both a leg and an unrelated parent order: the two cancels must not share
    // a confirmation code.
    let scope = dry_run::Scope::on_order(
        if is_attached {
            "cancel attached"
        } else {
            "cancel"
        },
        &p.order_id,
    );
    // Two-step by design: without a confirmation code this cancels nothing.
    let Some(code) = p.execute.clone() else {
        let existing = preview_existing_order(mctx, &ctx, &p.order_id, is_attached).await;
        return dry_run::result(
            &scope,
            serde_json::json!({
                "action": "cancel_order",
                "order_id": p.order_id,
                "is_attached": is_attached,
                "order": existing,
            }),
        );
    };
    scope.verify(&code)?;
    let mut opts = CancelOrderOptions::new(p.order_id);
    if is_attached {
        opts = opts.is_attached();
    }
    ctx.cancel_order(opts).await.map_err(Error::longbridge)?;
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

/// Parse a decimal-valued attached-order field, naming the field in the error
/// instead of leaving the caller with a bare parse failure.
fn attached_decimal(field: &str, value: &str) -> Result<longbridge::Decimal, McpError> {
    use longbridge::Decimal;
    use std::str::FromStr;

    Decimal::from_str(value)
        .map_err(|e| McpError::invalid_params(format!("invalid {field}: {e}"), None))
}

/// Parse an ID- or timestamp-valued attached-order field.
fn attached_i64(field: &str, value: &str) -> Result<i64, McpError> {
    value
        .parse::<i64>()
        .map_err(|e| McpError::invalid_params(format!("invalid {field}: {e}"), None))
}

/// Parse an enum-valued attached-order field (time-in-force, order type,
/// outside-RTH).
fn attached_enum<T>(field: &str, value: &str) -> Result<T, McpError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|e| McpError::invalid_params(format!("invalid {field}: {e}"), None))
}

/// The attached take-profit / stop-loss leg of a new order.
///
/// `attached_order_type` is what turns the feature on, so it arrives
/// separately: every other field is optional.
fn attached_submit_params(
    p: &SubmitOrderParam,
    attached_order_type: &str,
) -> Result<longbridge::trade::SubmitAttachedParams, McpError> {
    use longbridge::trade::{
        AttachedOrderType, OrderType, OutsideRTH, SubmitAttachedParams, TimeInForceType,
    };

    let mut ap = SubmitAttachedParams::new(attached_enum::<AttachedOrderType>(
        "attached_order_type",
        attached_order_type,
    )?);
    if let Some(ref v) = p.attached_profit_taker_price {
        ap = ap.profit_taker_price(attached_decimal("attached_profit_taker_price", v)?);
    }
    if let Some(ref v) = p.attached_stop_loss_price {
        ap = ap.stop_loss_price(attached_decimal("attached_stop_loss_price", v)?);
    }
    if let Some(ref v) = p.attached_profit_taker_submit_price {
        ap = ap
            .profit_taker_submit_price(attached_decimal("attached_profit_taker_submit_price", v)?);
    }
    if let Some(ref v) = p.attached_stop_loss_submit_price {
        ap = ap.stop_loss_submit_price(attached_decimal("attached_stop_loss_submit_price", v)?);
    }
    if let Some(ref v) = p.attached_time_in_force {
        ap = ap.time_in_force(attached_enum::<TimeInForceType>(
            "attached_time_in_force",
            v,
        )?);
    }
    if let Some(ref v) = p.attached_expire_time {
        ap = ap.expire_time(attached_i64("attached_expire_time", v)?);
    }
    if let Some(ref v) = p.attached_activate_order_type {
        ap = ap.activate_order_type(attached_enum::<OrderType>(
            "attached_activate_order_type",
            v,
        )?);
    }
    if let Some(ref v) = p.attached_outside_rth {
        ap = ap.activate_rth(attached_enum::<OutsideRTH>("attached_outside_rth", v)?);
    }
    Ok(ap)
}

/// The attached-leg changes of a replace.
///
/// A bare `attached_cancel_all` carries no type, and the API takes the type as
/// a required field, so `Unknown` stands for "no particular leg type" there.
fn attached_replace_params(
    p: &ReplaceOrderParam,
) -> Result<longbridge::trade::ReplaceAttachedParams, McpError> {
    use longbridge::trade::{
        AttachedOrderType, OrderType, OutsideRTH, ReplaceAttachedParams, TimeInForceType,
    };

    let attached_order_type = match p.attached_order_type.as_deref() {
        Some(v) => attached_enum::<AttachedOrderType>("attached_order_type", v)?,
        None => AttachedOrderType::Unknown,
    };
    let mut ap = ReplaceAttachedParams::new(attached_order_type);
    if p.attached_cancel_all == Some(true) {
        ap = ap.cancel_all_attached();
    }
    if let Some(ref v) = p.attached_profit_taker_id {
        ap = ap.profit_taker_id(attached_i64("attached_profit_taker_id", v)?);
    }
    if let Some(ref v) = p.attached_stop_loss_id {
        ap = ap.stop_loss_id(attached_i64("attached_stop_loss_id", v)?);
    }
    if let Some(ref v) = p.attached_profit_taker_price {
        ap = ap.profit_taker_price(attached_decimal("attached_profit_taker_price", v)?);
    }
    if let Some(ref v) = p.attached_stop_loss_price {
        ap = ap.stop_loss_price(attached_decimal("attached_stop_loss_price", v)?);
    }
    if let Some(ref v) = p.attached_profit_taker_submit_price {
        ap = ap
            .profit_taker_submit_price(attached_decimal("attached_profit_taker_submit_price", v)?);
    }
    if let Some(ref v) = p.attached_stop_loss_submit_price {
        ap = ap.stop_loss_submit_price(attached_decimal("attached_stop_loss_submit_price", v)?);
    }
    if let Some(ref v) = p.attached_time_in_force {
        ap = ap.time_in_force(attached_enum::<TimeInForceType>(
            "attached_time_in_force",
            v,
        )?);
    }
    if let Some(ref v) = p.attached_expire_time {
        ap = ap.expire_time(attached_i64("attached_expire_time", v)?);
    }
    if let Some(ref v) = p.attached_activate_order_type {
        ap = ap.activate_order_type(attached_enum::<OrderType>(
            "attached_activate_order_type",
            v,
        )?);
    }
    if let Some(ref v) = p.attached_outside_rth {
        ap = ap.activate_rth(attached_enum::<OutsideRTH>("attached_outside_rth", v)?);
    }
    if let Some(ref v) = p.attached_main_id {
        ap = ap.main_id(attached_i64("attached_main_id", v)?);
    }
    if let Some(ref v) = p.attached_quantity {
        ap = ap.quantity(attached_decimal("attached_quantity", v)?);
    }
    if let Some(ref v) = p.attached_market_price {
        ap = ap.market_price(attached_decimal("attached_market_price", v)?);
    }
    Ok(ap)
}

/// The attached-order clause of a `submit_order` preview, or `null` for a plain
/// order: a preview is what the user confirms, so it has to show the protective
/// legs about to be placed alongside the order.
fn attached_submit_preview(p: &SubmitOrderParam) -> serde_json::Value {
    let Some(ref attached_order_type) = p.attached_order_type else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "attached_order_type": attached_order_type,
        "profit_taker_price": p.attached_profit_taker_price,
        "stop_loss_price": p.attached_stop_loss_price,
        "profit_taker_submit_price": p.attached_profit_taker_submit_price,
        "stop_loss_submit_price": p.attached_stop_loss_submit_price,
        "time_in_force": p.attached_time_in_force,
        "expire_time": p.attached_expire_time,
        "activate_order_type": p.attached_activate_order_type,
        "outside_rth": p.attached_outside_rth,
    })
}

/// The attached-order clause of a `replace_order` preview, or `null` when the
/// replace leaves the attached legs alone.
fn attached_replace_preview(p: &ReplaceOrderParam) -> serde_json::Value {
    if !p.has_attached_change() {
        return serde_json::Value::Null;
    }
    serde_json::json!({
        "cancel_all": p.attached_cancel_all,
        "attached_order_type": p.attached_order_type,
        "profit_taker_id": p.attached_profit_taker_id,
        "stop_loss_id": p.attached_stop_loss_id,
        "profit_taker_price": p.attached_profit_taker_price,
        "stop_loss_price": p.attached_stop_loss_price,
        "profit_taker_submit_price": p.attached_profit_taker_submit_price,
        "stop_loss_submit_price": p.attached_stop_loss_submit_price,
        "time_in_force": p.attached_time_in_force,
        "expire_time": p.attached_expire_time,
        "activate_order_type": p.attached_activate_order_type,
        "outside_rth": p.attached_outside_rth,
        "main_id": p.attached_main_id,
        "quantity": p.attached_quantity,
        "market_price": p.attached_market_price,
    })
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
    if let Some(ref v) = p.attached_order_type {
        opts = opts.attached_params(attached_submit_params(&p, v)?);
    }

    let mut scope = dry_run::Scope::order(
        &p.side,
        &p.symbol,
        &p.submitted_quantity,
        p.submitted_price.as_deref().unwrap_or(""),
    );
    // The protective legs are part of the order the user approves: a code
    // confirmed for one take-profit/stop-loss pair must not place another.
    if let Some(ref v) = p.attached_order_type {
        scope = scope.and("attached", v);
        if let Some(ref v) = p.attached_profit_taker_price {
            scope = scope.and("tp", v);
        }
        if let Some(ref v) = p.attached_stop_loss_price {
            scope = scope.and("sl", v);
        }
    }
    // Two-step by design: without a confirmation code this places nothing.
    let Some(code) = p.execute.clone() else {
        return dry_run::result(
            &scope,
            serde_json::json!({
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
                "attached": attached_submit_preview(&p),
            }),
        );
    };
    scope.verify(&code)?;

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
    if p.has_attached_change() {
        opts = opts.attached_params(attached_replace_params(&p)?);
    }
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let mut scope =
        dry_run::Scope::replace(&p.order_id, &p.quantity, p.price.as_deref().unwrap_or(""));
    // Cancelling or repricing the protective legs changes what the user is
    // agreeing to, so a code confirmed for one set of legs must not apply to
    // another.
    if p.attached_cancel_all == Some(true) {
        scope = scope.and("cancel_attached", "true");
    }
    if let Some(ref v) = p.attached_order_type {
        scope = scope.and("attached", v);
    }
    if let Some(ref v) = p.attached_profit_taker_price {
        scope = scope.and("tp", v);
    }
    if let Some(ref v) = p.attached_stop_loss_price {
        scope = scope.and("sl", v);
    }
    // Two-step by design: without a confirmation code this changes nothing.
    let Some(code) = p.execute.clone() else {
        let existing = preview_existing_order(mctx, &ctx, &p.order_id, false).await;
        return dry_run::result(
            &scope,
            serde_json::json!({
                "action": "replace_order",
                "order_id": p.order_id,
                "current_order": existing,
                "new_quantity": p.quantity,
                "new_price": p.price,
                "new_trigger_price": p.trigger_price,
                "new_limit_offset": p.limit_offset,
                "new_trailing_amount": p.trailing_amount,
                "new_trailing_percent": p.trailing_percent,
                "attached": attached_replace_preview(&p),
            }),
        );
    };
    scope.verify(&code)?;
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
        assert!(submit.execute.is_none());

        let cancel: CancelOrderParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1" }))
                .expect("cancel_order params without execute must deserialize");
        assert!(cancel.execute.is_none());

        let replace: ReplaceOrderParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1", "quantity": "10" }))
                .expect("replace_order params without execute must deserialize");
        assert!(replace.execute.is_none());

        let grid: crate::tools::grid::GridOrderIdParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1" }))
                .expect("grid cancel/suspend/restart params without execute must deserialize");
        assert!(grid.execute.is_none());
    }

    #[test]
    fn execute_is_a_code_string_not_a_boolean() {
        // `execute: true` was the earlier shape. Accepting it now would let a
        // caller go live without ever producing a preview.
        let boolean = serde_json::from_value::<CancelOrderParam>(
            serde_json::json!({ "order_id": "1", "execute": true }),
        );
        assert!(boolean.is_err(), "execute must not accept a boolean");

        let coded: CancelOrderParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1", "execute": "473" }))
                .expect("a confirmation code must deserialize");
        assert_eq!(coded.execute.as_deref(), Some("473"));
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
        // The original response schema remains available as a resource even
        // when jq projections have a different shape. It must admit dry runs.
        let tools = crate::tools::all_tools_full_cached();
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
            for needle in ["confirmation_code", "DRY RUN", "confirm"] {
                assert!(
                    description.contains(needle),
                    "{name} description must mention `{needle}`"
                );
            }
        }
    }
}

#[cfg(test)]
mod attached_order_tests {
    //! Attached take-profit / stop-loss legs.
    //!
    //! These legs are the user's downside protection, so what the tool sends
    //! must be exactly what the preview showed: a leg silently dropped, or a
    //! trigger price that arrives changed, is a loss the caller cannot see
    //! coming.

    use super::{
        CancelOrderParam, OrderDetailParam, ReplaceOrderParam, SubmitOrderParam, TodayOrdersParam,
        attached_replace_params, attached_replace_preview, attached_submit_params,
        attached_submit_preview,
    };

    fn bracket_order() -> SubmitOrderParam {
        serde_json::from_value(serde_json::json!({
            "symbol": "700.HK",
            "order_type": "LO",
            "side": "Buy",
            "submitted_quantity": "100",
            "time_in_force": "Day",
            "submitted_price": "400",
            "attached_order_type": "BRACKET",
            "attached_profit_taker_price": "450",
            "attached_stop_loss_price": "380",
            "attached_activate_order_type": "MO",
        }))
        .expect("a bracket order must deserialize")
    }

    /// The leg type is whatever the SDK's own `FromStr` accepts — the wire
    /// spellings — and it reaches the API unchanged. Anything else is the
    /// SDK's parse error, named after the parameter it came from.
    #[test]
    fn the_leg_type_is_the_sdk_spelling() {
        let mut p = bracket_order();
        for spelling in ["PROFIT_TAKER", "STOP_LOSS", "BRACKET"] {
            p.attached_order_type = Some(spelling.to_string());
            let params = attached_submit_params(&p, spelling)
                .unwrap_or_else(|e| panic!("'{spelling}' must parse: {e}"));
            let sent = serde_json::to_value(&params).expect("attached params must serialize");
            assert_eq!(
                sent["attached_order_type"],
                serde_json::json!(spelling),
                "'{spelling}' must be sent unchanged"
            );
        }
        let err = attached_submit_params(&p, "ProfitTaker")
            .expect_err("a spelling the SDK does not know must be an error");
        assert!(
            err.message.contains("attached_order_type"),
            "the error must name the parameter: {}",
            err.message
        );
    }

    /// The prices the caller sent are the prices the request carries.
    #[test]
    fn submit_sends_the_legs_it_was_given() {
        let p = bracket_order();
        let params = attached_submit_params(&p, "BRACKET").expect("a full bracket must build");
        let sent = serde_json::to_value(&params).expect("attached params must serialize");
        assert_eq!(sent["attached_order_type"], serde_json::json!("BRACKET"));
        assert_eq!(sent["profit_taker_price"], serde_json::json!("450"));
        assert_eq!(sent["stop_loss_price"], serde_json::json!("380"));
        assert_eq!(sent["activate_order_type"], serde_json::json!("MO"));
    }

    /// The preview is what the user confirms, so the legs have to be in it —
    /// and absent for the plain order that has none.
    #[test]
    fn the_preview_shows_the_legs() {
        let preview = attached_submit_preview(&bracket_order());
        assert_eq!(preview["attached_order_type"], serde_json::json!("BRACKET"));
        assert_eq!(preview["profit_taker_price"], serde_json::json!("450"));
        assert_eq!(preview["stop_loss_price"], serde_json::json!("380"));

        let mut plain = bracket_order();
        plain.attached_order_type = None;
        assert!(
            attached_submit_preview(&plain).is_null(),
            "a plain order's preview must not grow an attached section"
        );
    }

    /// A replace that says nothing about the legs must not touch them.
    #[test]
    fn a_plain_replace_leaves_the_legs_alone() {
        let plain: ReplaceOrderParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1", "quantity": "100" }))
                .expect("a plain replace must deserialize");
        assert!(!plain.has_attached_change());
        assert!(attached_replace_preview(&plain).is_null());
    }

    /// Cancelling every leg is a complete request on its own: it carries no
    /// leg type, and must still reach the API as a cancel-all.
    #[test]
    fn replace_can_cancel_every_leg_without_naming_a_type() {
        let cancel_all: ReplaceOrderParam = serde_json::from_value(serde_json::json!({
            "order_id": "1",
            "quantity": "100",
            "attached_cancel_all": true,
        }))
        .expect("a cancel-all replace must deserialize");
        assert!(cancel_all.has_attached_change());
        let params = attached_replace_params(&cancel_all).expect("a cancel-all must build");
        let sent = serde_json::to_value(&params).expect("attached params must serialize");
        assert_eq!(sent["cancel_all_attached"], serde_json::json!(true));
        assert_eq!(
            attached_replace_preview(&cancel_all)["cancel_all"],
            serde_json::json!(true)
        );
    }

    /// Repricing one existing leg targets it by its own ID.
    #[test]
    fn replace_can_reprice_one_existing_leg() {
        let reprice: ReplaceOrderParam = serde_json::from_value(serde_json::json!({
            "order_id": "1",
            "quantity": "100",
            "attached_order_type": "STOP_LOSS",
            "attached_stop_loss_id": "9876543210",
            "attached_stop_loss_price": "375.5",
        }))
        .expect("a leg reprice must deserialize");
        let params = attached_replace_params(&reprice).expect("a leg reprice must build");
        let sent = serde_json::to_value(&params).expect("attached params must serialize");
        assert_eq!(sent["attached_order_type"], serde_json::json!("STOP_LOSS"));
        assert_eq!(sent["stop_loss_id"], serde_json::json!(9_876_543_210_i64));
        assert_eq!(sent["stop_loss_price"], serde_json::json!("375.5"));
    }

    /// `is_attached` stays optional everywhere it appears, so the common case
    /// (a parent order) needs no extra argument.
    #[test]
    fn is_attached_is_optional_on_every_tool_that_takes_it() {
        let detail: OrderDetailParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1" }))
                .expect("order_detail params must deserialize without is_attached");
        assert!(detail.is_attached.is_none());

        let today: TodayOrdersParam = serde_json::from_value(serde_json::json!({}))
            .expect("today_orders params must deserialize without is_attached");
        assert!(today.order_id.is_none() && today.is_attached.is_none());

        let cancel: CancelOrderParam =
            serde_json::from_value(serde_json::json!({ "order_id": "1" }))
                .expect("cancel_order params must deserialize without is_attached");
        assert!(cancel.is_attached.is_none());
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
