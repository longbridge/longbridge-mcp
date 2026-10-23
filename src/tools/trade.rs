use std::future::Future;

use longbridge::DcRegion;
use longbridge::httpclient::Json;
use longbridge::trade::{
    AccountBalance, CancelOrderOptions, Execution, GetHistoryExecutionsOptions,
    GetOrderDetailOptions, GetTodayExecutionsOptions, GetTodayOrdersOptions, GetUSHistoryOrders,
    GetUSTransactionHistoryOptions, Order, OrderDetail, StockPositionsResponse, TradeContext,
};
use reqwest::Method;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::serialize::{strip_nulls, strip_trailing_zeros};
use crate::tools::support::dry_run;
use crate::tools::support::http_client::http_get_tool;
use crate::tools::support::parse;
use crate::tools::support::tolerant::{
    tolerant_id_string, tolerant_option_id_string, tolerant_option_string, tolerant_string,
};
use crate::tools::support::us_align;
use crate::tools::support::us_market;
use crate::tools::{tool_json, tool_result};

pub use crate::tools::quote::SymbolParam;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OrderDetailParam {
    /// Order ID to look up. A parent order ID, or (with is_attached=true) the
    /// ID of an attached take-profit / stop-loss leg.
    #[serde(deserialize_with = "tolerant_id_string")]
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
    /// of an attached take-profit / stop-loss leg. US accounts: matches the
    /// parent order ID only.
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
    /// US accounts only: return just this page (default: every page, up to
    /// 500 orders). Ignored for AP accounts.
    pub us_page: Option<i32>,
    /// US accounts only: page size when paging with us_page (default 50).
    /// Ignored for AP accounts.
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
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
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
    #[serde(deserialize_with = "tolerant_string")]
    pub submitted_quantity: String,
    /// Order validity: "Day" (Day Order, expires end of session), "GTC" (Good Til Canceled), "GTD" (Good Til Date, requires expire_date)
    pub time_in_force: String,
    /// Limit price. Required for: LO, ELO, ALO, ODD, LIT, SLO
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub submitted_price: Option<String>,
    /// Trigger (activation) price. Required for: LIT, MIT, TSLPAMT, TSLPPCT
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub trigger_price: Option<String>,
    /// Limit offset from the trailing stop price. Required for: TSLPAMT, TSLPPCT
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub limit_offset: Option<String>,
    /// Trailing amount (absolute price distance). Required for TSLPAMT
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub trailing_amount: Option<String>,
    /// Trailing percent as decimal (e.g. 0.05 = 5%). Required for TSLPPCT
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub trailing_percent: Option<String>,
    /// Expiry date (yyyy-mm-dd). Required when time_in_force is GTD
    pub expire_date: Option<String>,
    /// Outside regular trading hours: "RTH_ONLY" (regular trading hours only), "ANY_TIME" (any time including pre/post market), "OVERNIGHT" (overnight session, US only), "OPTION_PRE_MARKET" (US option pre-market session)
    pub outside_rth: Option<String>,
    /// Order remark (max 255 characters)
    pub remark: Option<String>,
    /// Idempotency key, unique per intended order (e.g. a UUID). When two
    /// requests carry the same value the exchange places one order and returns
    /// the original `order_id` for the second, so a retry after a timeout or a
    /// dropped connection cannot double-fill. Set it whenever you may retry;
    /// generate a NEW value for a genuinely new order.
    pub client_request_id: Option<String>,
    /// Attach a take-profit / stop-loss leg to this order: "PROFIT_TAKER"
    /// (take-profit only), "STOP_LOSS" (stop-loss only) or "BRACKET" (both).
    /// Omit for a plain order; every other attached_* field is ignored without
    /// it.
    pub attached_order_type: Option<String>,
    /// Take-profit trigger price. Required for PROFIT_TAKER and BRACKET.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_profit_taker_price: Option<String>,
    /// Stop-loss trigger price. Required for STOP_LOSS and BRACKET.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_stop_loss_price: Option<String>,
    /// Limit price of the take-profit leg, for an LO attached_activate_order_type.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_profit_taker_submit_price: Option<String>,
    /// Limit price of the stop-loss leg, for an LO attached_activate_order_type.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_stop_loss_submit_price: Option<String>,
    /// Time-in-force of the attached leg: "Day" / "GTC" / "GTD". Defaults to
    /// the parent order's setting when omitted.
    pub attached_time_in_force: Option<String>,
    /// Expiry of the attached leg as a unix timestamp in seconds (e.g.
    /// "1767139200"). Required when attached_time_in_force is GTD.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_expire_time: Option<String>,
    /// Order type the attached leg is submitted as once triggered, e.g. "LO"
    /// (then set the matching attached_*_submit_price) or "MO".
    pub attached_activate_order_type: Option<String>,
    /// Outside-RTH setting of the triggered leg: "RTH_ONLY" / "ANY_TIME" /
    /// "OVERNIGHT" / "OPTION_PRE_MARKET".
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

/// One leg of a multi-leg combination order.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct MultiLegParam {
    /// Option symbol in `<CODE>.<MARKET>` format, e.g. "QQQ260731C764000.US"
    pub symbol: String,
    /// Leg ratio, a positive number (e.g. "1", "2"). The buy/sell direction of
    /// each leg comes from `strategy` together with the order `side`, never
    /// from the sign here — a zero or negative ratio is rejected upstream.
    pub ratio_quantity: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SubmitMultiLegOrderParam {
    /// Strategy, which fixes how many legs are required and the direction of
    /// each: CoveredCall / CoveredPut (stock + option) / VerticalCallSpread /
    /// VerticalPutSpread / Collar / Straddle / Strangle / CalendarCallSpread /
    /// CalendarPutSpread (the two calendar spreads pair the same strike across
    /// two expiries)
    pub strategy: String,
    /// Buy or Sell — the direction of the strategy as a whole
    pub side: String,
    /// Order type: LO (Limit, requires submitted_price) or MO (Market)
    pub order_type: String,
    /// Number of strategy units to trade (each unit is one set of legs in the
    /// ratios given by `legs`)
    pub submitted_quantity: String,
    /// The legs of the combination, in strategy order. Option symbols only.
    pub legs: Vec<MultiLegParam>,
    /// Net limit price for the whole combination. Required for LO.
    pub submitted_price: Option<String>,
    /// Order remark (max 255 characters)
    pub remark: Option<String>,
    /// Idempotency key, unique per intended order. See the field of the same
    /// name on `submit_order`.
    pub client_request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReplaceOrderParam {
    /// Order ID to replace (returned by submit_order or listed in today_orders / history_orders)
    #[serde(deserialize_with = "tolerant_id_string")]
    pub order_id: String,
    /// New order quantity (number of shares)
    #[serde(deserialize_with = "tolerant_string")]
    pub quantity: String,
    /// New limit price (for limit-style orders)
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub price: Option<String>,
    /// New trigger (activation) price (for LIT / MIT / trailing-stop orders)
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub trigger_price: Option<String>,
    /// New limit offset from the trailing stop price (for TSLPAMT / TSLPPCT)
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub limit_offset: Option<String>,
    /// New trailing amount as absolute price distance (for TSLPAMT)
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub trailing_amount: Option<String>,
    /// New trailing percent as decimal e.g. 0.05 = 5% (for TSLPPCT)
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub trailing_percent: Option<String>,
    /// Set to true to cancel every attached take-profit / stop-loss leg of this
    /// order, leaving the order itself in place.
    pub attached_cancel_all: Option<bool>,
    /// Attached leg to add or update: "PROFIT_TAKER", "STOP_LOSS" or "BRACKET".
    /// Required unless the only attached change is attached_cancel_all.
    pub attached_order_type: Option<String>,
    /// ID of the existing take-profit leg to update (from
    /// order_detail's attached_orders[]). Omit to add a new leg.
    #[serde(default, deserialize_with = "tolerant_option_id_string")]
    pub attached_profit_taker_id: Option<String>,
    /// ID of the existing stop-loss leg to update (from order_detail's
    /// attached_orders[]). Omit to add a new leg.
    #[serde(default, deserialize_with = "tolerant_option_id_string")]
    pub attached_stop_loss_id: Option<String>,
    /// New take-profit trigger price.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_profit_taker_price: Option<String>,
    /// New stop-loss trigger price.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_stop_loss_price: Option<String>,
    /// New limit price for the take-profit leg.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_profit_taker_submit_price: Option<String>,
    /// New limit price for the stop-loss leg.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_stop_loss_submit_price: Option<String>,
    /// New time-in-force for the attached leg: "Day" / "GTC" / "GTD".
    pub attached_time_in_force: Option<String>,
    /// New expiry for the attached leg as a unix timestamp in seconds.
    /// Required when attached_time_in_force is GTD.
    #[serde(default, deserialize_with = "tolerant_option_string")]
    pub attached_expire_time: Option<String>,
    /// New order type for the triggered leg, e.g. "LO" or "MO".
    pub attached_activate_order_type: Option<String>,
    /// New outside-RTH setting for the triggered leg: "RTH_ONLY" / "ANY_TIME"
    /// / "OVERNIGHT" / "OPTION_PRE_MARKET".
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
    #[serde(deserialize_with = "tolerant_id_string")]
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
    /// Start time: RFC3339 (e.g. 2026-09-01T00:00:00Z), or a date (yyyy-mm-dd) for the start of that day in UTC (US accounts: in New York time)
    pub start_at: String,
    /// End time: RFC3339 (e.g. 2026-09-30T23:59:59Z), or a date (yyyy-mm-dd) for the end of that day in UTC (US accounts: in New York time)
    pub end_at: String,
    /// US accounts only: return just this page (default: every page, up to
    /// 500 orders). Ignored for AP accounts (the region is inferred from the
    /// account — do not pass it).
    pub us_page: Option<i32>,
    /// US accounts only: page size when paging with us_page (default 50).
    /// Ignored for AP accounts.
    pub us_limit: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryExecutionsParam {
    /// Filter by symbol (optional)
    pub symbol: Option<String>,
    /// Start time: RFC3339 (e.g. 2026-09-01T00:00:00Z), or a date (yyyy-mm-dd) for the start of that day in UTC (US accounts: in New York time)
    pub start_at: String,
    /// End time: RFC3339 (e.g. 2026-09-30T23:59:59Z), or a date (yyyy-mm-dd) for the end of that day in UTC (US accounts: in New York time)
    pub end_at: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CashFlowParam {
    /// Start time: RFC3339 (e.g. 2026-09-01T00:00:00Z), or a date (yyyy-mm-dd) for the start of that day in UTC
    pub start_at: String,
    /// End time: RFC3339 (e.g. 2026-09-30T23:59:59Z), or a date (yyyy-mm-dd) for the end of that day in UTC
    pub end_at: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EstimateMaxQtyParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
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
        return match us_order_detail_json(mctx, order_id).await {
            Ok(value) => value,
            Err(e) => order_lookup_failed(&e.to_string()),
        };
    }
    let mut opts = GetOrderDetailOptions::new(order_id);
    if is_attached {
        opts = opts.is_attached();
    }
    match ctx.order_detail(opts).await {
        Ok(result) => serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
        Err(e) => order_lookup_failed(&e.to_string()),
    }
}

/// What a cancel/replace preview shows when the order could not be looked
/// up: a bare `null` read as "nothing to worry about", so say why and what to
/// check before the user is asked to confirm.
fn order_lookup_failed(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "lookup_failed": reason,
        "note": "This order could not be found. Check the order_id with today_orders or \
                 history_orders before asking the user to confirm.",
    })
}

pub async fn account_balance(
    mctx: &crate::tools::McpContext,
    p: AccountBalanceParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    // The AP balance endpoint is not deployed in the US data center (it fails
    // with `code 12`); US accounts are served by the US cash/buying-power
    // endpoint instead, which has its own shape.
    if mctx.dc_region().await == DcRegion::Us {
        let result = ctx
            .us_cash_and_buy_power(p.currency.as_deref())
            .await
            .map_err(Error::longbridge)?;
        let mut value = us_align::account_balance(&result).map_err(Error::Serialize)?;
        strip_trailing_zeros(&mut value);
        return tool_json(&value);
    }
    let result = ctx
        .account_balance(p.currency.as_deref())
        .await
        .map_err(Error::longbridge)?;
    tool_json(&ap_account_balance_json(&result).map_err(Error::Serialize)?)
}

/// The AP `account_balance` output. Cash/margin amounts are padded to two
/// decimals ("123000.00", "0.00"); strip the trailing zeros (lossless).
fn ap_account_balance_json(
    result: &[AccountBalance],
) -> Result<serde_json::Value, serde_json::Error> {
    let mut value = serde_json::to_value(result)?;
    strip_trailing_zeros(&mut value);
    Ok(value)
}

pub async fn stock_positions(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    // The AP positions endpoint is not deployed in the US data center (it
    // fails with `code 12`), so US accounts get the US asset overview alone:
    // it already carries the stock, option, crypto and cash holdings.
    if mctx.dc_region().await == DcRegion::Us {
        let result = ctx.us_asset_overview().await.map_err(Error::longbridge)?;
        let mut value = us_align::stock_positions(&result).map_err(Error::Serialize)?;
        strip_trailing_zeros(&mut value);
        return tool_json(&value);
    }
    let result = ctx.stock_positions(None).await.map_err(Error::longbridge)?;
    tool_json(&ap_stock_positions_json(&result).map_err(Error::Serialize)?)
}

/// The AP `stock_positions` output. `cost_price` is padded to a fixed
/// decimal width ("41.430"); strip the trailing zeros (lossless).
fn ap_stock_positions_json(
    result: &StockPositionsResponse,
) -> Result<serde_json::Value, serde_json::Error> {
    let mut value = serde_json::to_value(result)?;
    strip_trailing_zeros(&mut value);
    Ok(value)
}

pub async fn fund_positions(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.fund_positions(None).await.map_err(Error::longbridge)?;
    // Cost/NAV amounts are padded to a fixed decimal width; strip trailing zeros.
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    strip_trailing_zeros(&mut value);
    tool_json(&value)
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
        let (start_at, end_at) = us_market::us_today_orders_window(time::OffsetDateTime::now_utc());
        let opts = GetUSHistoryOrders {
            symbol: p.symbol,
            side,
            // Confirmed via live testing that start_at/end_at do filter
            // correctly on the backend (unlike query_type, see below). The
            // window runs from 20:00 ET yesterday (the overnight session that
            // trades into today opens then) to the end of today in New York,
            // so an overnight order does not vanish at New York midnight —
            // agents are told to check today_orders before resending an
            // unconfirmed write, and a missing order would invite a duplicate.
            start_at: start_at.unix_timestamp(),
            end_at: end_at.unix_timestamp(),
            // query_type (0=all/1=pending/2=history) does not actually filter
            // on the backend as of this writing (confirmed via live testing —
            // "pending" returned the identical set as "all", "history"
            // returned nothing despite matching orders existing) — always
            // request 0 (all) rather than expose a filter that silently does
            // nothing or hides real data.
            query_type: 0,
            page: 1,
            limit: US_ORDER_PAGE_SIZE,
        };
        let mut orders = us_orders(&ctx, opts, p.us_page, p.us_limit).await?;
        // The US endpoint has no order-ID filter; apply it here so order_id
        // narrows the result as it does for AP accounts.
        if let Some(order_id) = p.order_id.as_deref() {
            orders.retain(|o| o.get("id").and_then(|id| id.as_str()) == Some(order_id));
        }
        return tool_json(&us_orders_json(&orders));
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
    tool_json(&ap_orders_json(&result).map_err(Error::Serialize)?)
}

/// The AP `today_orders`/`history_orders` output. Plain market/limit orders
/// leave the ~9 conditional-order fields (trigger_*, trailing_*,
/// monitor_price, limit_*) as null — drop them. Prices/amounts come padded to
/// a fixed decimal width; strip trailing zeros.
fn ap_orders_json(result: &[Order]) -> Result<serde_json::Value, serde_json::Error> {
    let mut value = serde_json::to_value(result)?;
    strip_nulls(&mut value);
    strip_trailing_zeros(&mut value);
    Ok(value)
}

/// The AP `order_detail` output. Prices/amounts come padded to a fixed
/// decimal width; strip trailing zeros.
fn ap_order_detail_json(result: &OrderDetail) -> Result<serde_json::Value, serde_json::Error> {
    let mut value = serde_json::to_value(result)?;
    strip_trailing_zeros(&mut value);
    Ok(value)
}

/// Page size requested from the US order endpoint when walking every page.
const US_ORDER_PAGE_SIZE: i32 = 50;

/// Most US order pages `today_orders`/`history_orders` walk (so at most
/// `US_ORDER_PAGE_SIZE * US_ORDER_MAX_PAGES` orders) when no page is given.
const US_ORDER_MAX_PAGES: i32 = 10;

/// Raw US orders for `opts`: the one page the caller named with
/// `page`/`limit`, or else every page up to `US_ORDER_MAX_PAGES` — AP returns
/// the whole range in one array, so a single default page would silently
/// hide orders.
async fn us_orders(
    ctx: &TradeContext,
    opts: GetUSHistoryOrders,
    page: Option<i32>,
    limit: Option<i32>,
) -> Result<Vec<serde_json::Value>, McpError> {
    if page.is_some() || limit.is_some() {
        let opts = GetUSHistoryOrders {
            page: page.unwrap_or(1),
            limit: limit.unwrap_or(US_ORDER_PAGE_SIZE),
            ..opts
        };
        let result = ctx.us_query_orders(opts).await.map_err(Error::longbridge)?;
        return Ok(result.orders);
    }
    let pages = collect_us_pages(US_ORDER_PAGE_SIZE, US_ORDER_MAX_PAGES, |page| {
        let opts = GetUSHistoryOrders {
            page,
            limit: US_ORDER_PAGE_SIZE,
            ..opts.clone()
        };
        async move { ctx.us_query_orders(opts).await.map(|r| r.orders) }
    })
    .await
    .map_err(Error::longbridge)?;
    if pages.truncated {
        tracing::info!(
            orders = pages.items.len(),
            "US order query stopped at the page cap"
        );
    }
    Ok(pages.items)
}

/// US orders in the AP orders output shape (a top-level array).
fn us_orders_json(orders: &[serde_json::Value]) -> serde_json::Value {
    let mut value = us_align::orders(orders);
    strip_nulls(&mut value);
    strip_trailing_zeros(&mut value);
    value
}

/// One US order's detail in the AP `order_detail` shape. Read as raw JSON:
/// the typed SDK response drops `counter_id`, which the US endpoint may
/// return as its only security identifier until the gateway converts it.
async fn us_order_detail_json(
    mctx: &crate::tools::McpContext,
    order_id: &str,
) -> Result<serde_json::Value, McpError> {
    if order_id.is_empty() || !order_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(McpError::invalid_params(
            "order_id must be the order's numeric ID",
            None,
        ));
    }
    let client = mctx.create_http_client();
    let path = format!("/v1/us/orders/{order_id}");
    let Json(data) = client
        .request(Method::GET, path.as_str())
        .dc_restrict(DcRegion::Us)
        .response::<Json<serde_json::Value>>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let mut value = us_align::order_detail(&data);
    strip_trailing_zeros(&mut value);
    Ok(value)
}

pub async fn order_detail(
    mctx: &crate::tools::McpContext,
    p: OrderDetailParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    if mctx.dc_region().await == longbridge::DcRegion::Us {
        return tool_json(&us_order_detail_json(mctx, &p.order_id).await?);
    }
    let mut opts = GetOrderDetailOptions::new(p.order_id);
    if p.is_attached == Some(true) {
        opts = opts.is_attached();
    }
    let result = ctx.order_detail(opts).await.map_err(Error::longbridge)?;
    tool_json(&ap_order_detail_json(&result).map_err(Error::Serialize)?)
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
    // Execution prices/amounts come padded to a fixed decimal width; strip
    // trailing zeros (lossless).
    let mut value = serde_json::Value::Array(result);
    crate::serialize::strip_trailing_zeros(&mut value);
    tool_json(&value)
}

pub async fn history_orders(
    mctx: &crate::tools::McpContext,
    p: HistoryOrdersParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let region = mctx.dc_region().await;
    let (start, end) = history_range(region, &p.start_at, &p.end_at)?;
    if region == DcRegion::Us {
        // A bare date is a New York (US-market) day. today_orders' window
        // opens at 20:00 ET on the previous day, so a range ending yesterday
        // overlaps it for yesterday's 20:00–24:00 ET orders; callers combining
        // the two dedupe by order_id.
        let opts = GetUSHistoryOrders {
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
            page: 1,
            limit: US_ORDER_PAGE_SIZE,
        };
        let orders = us_orders(&ctx, opts, p.us_page, p.us_limit).await?;
        return tool_json(&us_orders_json(&orders));
    }
    let mut opts = longbridge::trade::GetHistoryOrdersOptions::new()
        .start_at(start)
        .end_at(end);
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    let result = ctx.history_orders(opts).await.map_err(Error::longbridge)?;
    tool_json(&ap_orders_json(&result).map_err(Error::Serialize)?)
}

/// Parse a history tool's `start_at`/`end_at` for the account's region.
///
/// A bare `yyyy-mm-dd` is a New York (US-market) day for US accounts and a
/// UTC day otherwise; full RFC3339 timestamps are taken as given.
fn history_range(
    region: DcRegion,
    start_at: &str,
    end_at: &str,
) -> Result<(time::OffsetDateTime, time::OffsetDateTime), McpError> {
    if region == DcRegion::Us {
        Ok((
            parse::parse_rfc3339_new_york(start_at)?,
            parse::parse_rfc3339_end_new_york(end_at)?,
        ))
    } else {
        Ok((
            parse::parse_rfc3339(start_at)?,
            parse::parse_rfc3339_end(end_at)?,
        ))
    }
}

/// Upstream endpoint `history_executions` reads fills from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionsSource {
    /// v1 `/trade/execution/history` (walked page by page by the SDK).
    V1History,
    /// v3 `/trade/execution/all`, filtered by execution time.
    V3All,
}

/// Pick the executions endpoint for a region: v3 `/trade/execution/all` is not
/// deployed in the US data center (it fails with `code 12`), so US accounts
/// use v1 `/trade/execution/history`.
fn executions_source(region: DcRegion) -> ExecutionsSource {
    if region == DcRegion::Us {
        ExecutionsSource::V1History
    } else {
        ExecutionsSource::V3All
    }
}

pub async fn history_executions(
    mctx: &crate::tools::McpContext,
    p: HistoryExecutionsParam,
) -> Result<CallToolResult, McpError> {
    let region = mctx.dc_region().await;
    let (start, end) = history_range(region, &p.start_at, &p.end_at)?;

    let (ctx, _) = TradeContext::new(mctx.create_config());
    let executions = match executions_source(region) {
        ExecutionsSource::V1History => {
            let mut opts = GetHistoryExecutionsOptions::new()
                .start_at(start)
                .end_at(end);
            if let Some(symbol) = p.symbol {
                opts = opts.symbol(symbol);
            }
            ctx.history_executions(opts)
                .await
                .map_err(Error::longbridge)?
        }
        ExecutionsSource::V3All => v3_all_executions(&ctx, start, end, p.symbol).await?,
    };
    executions_json(&executions)
}

/// Every fill from v3 `/trade/execution/all`, which filters by execution time
/// and caps each page at 1000 records; walk `page` until `has_more` is false.
async fn v3_all_executions(
    ctx: &TradeContext,
    start: time::OffsetDateTime,
    end: time::OffsetDateTime,
    symbol: Option<String>,
) -> Result<Vec<Execution>, McpError> {
    let mut executions: Vec<Execution> = Vec::new();
    for page in 1..=1000u64 {
        let mut exec_opts = longbridge::trade::GetAllExecutionsOptions::new()
            .start_at(start)
            .end_at(end)
            .page(page);
        if let Some(ref symbol) = symbol {
            exec_opts = exec_opts.symbol(symbol.clone());
        }
        let resp = ctx
            .all_executions(exec_opts)
            .await
            .map_err(Error::longbridge)?;
        if resp.trades.is_empty() {
            break;
        }
        executions.extend(resp.trades);
        if !resp.has_more {
            break;
        }
    }
    Ok(executions)
}

/// Render fills as the tool's top-level array, with `side` spelled out.
fn executions_json(executions: &[Execution]) -> Result<CallToolResult, McpError> {
    let result: Vec<serde_json::Value> = executions
        .iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).unwrap_or_default();
            if let serde_json::Value::Object(ref mut map) = v {
                map.insert(
                    "side".to_string(),
                    serde_json::Value::String(format!("{:?}", e.side)),
                );
            }
            v
        })
        .collect();
    // Execution prices/amounts come padded to a fixed decimal width; strip
    // trailing zeros (lossless).
    let mut value = serde_json::Value::Array(result);
    crate::serialize::strip_trailing_zeros(&mut value);
    tool_json(&value)
}

pub async fn cash_flow(
    mctx: &crate::tools::McpContext,
    p: CashFlowParam,
) -> Result<CallToolResult, McpError> {
    let start = parse::parse_rfc3339(&p.start_at)?;
    let end = parse::parse_rfc3339_end(&p.end_at)?;
    let (ctx, _) = TradeContext::new(mctx.create_config());
    if mctx.dc_region().await == DcRegion::Us {
        // The AP cash-flow endpoint is not deployed in the US data center (it
        // fails with `code 12`); US accounts are served by the paged US
        // transaction history instead.
        let pages = collect_us_pages(US_TRANSACTION_PAGE_SIZE, US_TRANSACTION_MAX_PAGES, |page| {
            let mut opts = GetUSTransactionHistoryOptions::new(start, end);
            opts.page = page;
            opts.size = US_TRANSACTION_PAGE_SIZE;
            let ctx = &ctx;
            async move { ctx.us_transaction_history(opts).await.map(|r| r.items) }
        })
        .await
        .map_err(Error::longbridge)?;
        // AP returns a bare array with no room for a truncation flag; the
        // record cap (newest first) is documented in the tool description.
        if pages.truncated {
            tracing::info!(
                records = pages.items.len(),
                "US cash_flow stopped at the page cap"
            );
        }
        // Amounts keep their upstream decimals, as AP's `balance` does.
        return tool_json(&us_align::cash_flow(&pages.items));
    }
    let opts = longbridge::trade::GetCashFlowOptions::new(start, end);
    let result = ctx.cash_flow(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

/// Page size requested from the US transaction history.
const US_TRANSACTION_PAGE_SIZE: i32 = 100;

/// Most US transaction-history pages `cash_flow` walks before it stops and
/// reports the result as truncated.
const US_TRANSACTION_MAX_PAGES: i32 = 20;

/// Records collected from a paged US endpoint that returns no total count.
#[derive(Debug)]
struct UsPages<T> {
    /// Every record fetched, in page order.
    items: Vec<T>,
    /// True when the page cap was reached while the last page was still full,
    /// so more records may exist.
    truncated: bool,
}

/// Walk a paged US endpoint from page 1 until a page comes back shorter than
/// `size` (the API's only end-of-data signal), or `max_pages` pages have been
/// fetched.
async fn collect_us_pages<T, E, F, Fut>(
    size: i32,
    max_pages: i32,
    mut fetch: F,
) -> Result<UsPages<T>, E>
where
    F: FnMut(i32) -> Fut,
    Fut: Future<Output = Result<Vec<T>, E>>,
{
    let full_page = usize::try_from(size).unwrap_or(0);
    let mut items = Vec::new();
    for page in 1..=max_pages {
        let batch = fetch(page).await?;
        let last = batch.len() < full_page || batch.is_empty();
        items.extend(batch);
        if last {
            return Ok(UsPages {
                items,
                truncated: false,
            });
        }
    }
    Ok(UsPages {
        items,
        truncated: true,
    })
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

/// What a `replace_order` confirmation code covers.
fn replace_scope(p: &ReplaceOrderParam) -> dry_run::Scope {
    let mut scope =
        dry_run::Scope::replace(&p.order_id, &p.quantity, p.price.as_deref().unwrap_or(""));
    // Cancelling or repricing the protective legs changes what the user is
    // agreeing to, so a code confirmed for one set of legs must not apply to
    // another.
    // Any explicit value is sent (it marks the request as an attached-leg
    // change), so any explicit value is part of what was approved.
    if let Some(cancel_all) = p.attached_cancel_all {
        scope = scope.and("cancel_attached", if cancel_all { "true" } else { "false" });
    }
    scope = with_present(
        scope,
        &[
            ("trigger", &p.trigger_price),
            ("offset", &p.limit_offset),
            ("trail_amount", &p.trailing_amount),
            ("trail_percent", &p.trailing_percent),
            ("attached", &p.attached_order_type),
            ("tp_id", &p.attached_profit_taker_id),
            ("sl_id", &p.attached_stop_loss_id),
            ("tp", &p.attached_profit_taker_price),
            ("sl", &p.attached_stop_loss_price),
            ("tp_limit", &p.attached_profit_taker_submit_price),
            ("sl_limit", &p.attached_stop_loss_submit_price),
            ("leg_tif", &p.attached_time_in_force),
            ("leg_expire", &p.attached_expire_time),
            ("leg_type", &p.attached_activate_order_type),
            ("leg_rth", &p.attached_outside_rth),
            ("main_id", &p.attached_main_id),
            ("leg_qty", &p.attached_quantity),
            ("leg_market_price", &p.attached_market_price),
        ],
    );
    scope
}

/// What a `submit_order` confirmation code covers.
fn submit_scope(p: &SubmitOrderParam) -> dry_run::Scope {
    let mut scope = dry_run::Scope::order(
        &p.side,
        &p.symbol,
        &p.submitted_quantity,
        p.submitted_price.as_deref().unwrap_or(""),
    );
    // Every field that changes what is traded is part of what the user
    // approves. `remark` (a note) and `client_request_id` (a caller-made
    // idempotency key, often generated only on the execute call) change
    // nothing traded and are left out, so adding them later is not a false
    // rejection: a code
    // confirmed for an LO Day order must not place an MO, a GTC, or one with
    // another trigger or trail.
    scope = scope
        .and("type", &p.order_type)
        .and("tif", &p.time_in_force);
    scope = with_present(
        scope,
        &[
            ("trigger", &p.trigger_price),
            ("offset", &p.limit_offset),
            ("trail_amount", &p.trailing_amount),
            ("trail_percent", &p.trailing_percent),
            ("expire", &p.expire_date),
            ("rth", &p.outside_rth),
        ],
    );
    // The protective legs are part of the order the user approves: a code
    // confirmed for one take-profit/stop-loss pair must not place another.
    if let Some(ref v) = p.attached_order_type {
        scope = scope.and("attached", v);
        scope = with_present(
            scope,
            &[
                ("tp", &p.attached_profit_taker_price),
                ("sl", &p.attached_stop_loss_price),
                ("tp_limit", &p.attached_profit_taker_submit_price),
                ("sl_limit", &p.attached_stop_loss_submit_price),
                ("leg_tif", &p.attached_time_in_force),
                ("leg_expire", &p.attached_expire_time),
                ("leg_type", &p.attached_activate_order_type),
                ("leg_rth", &p.attached_outside_rth),
            ],
        );
    }
    scope
}

/// Add a clause to `scope` for each field that is set, so a confirmation code
/// covers every value the preview showed.
fn with_present(mut scope: dry_run::Scope, fields: &[(&str, &Option<String>)]) -> dry_run::Scope {
    for (label, value) in fields {
        if let Some(value) = value {
            scope = scope.and(label, value);
        }
    }
    scope
}

/// Order types `submit_order` accepts, as the SDK spells them.
const ORDER_TYPES: &[&str] = &[
    "LO", "ELO", "MO", "AO", "ALO", "ODD", "LIT", "MIT", "TSLPAMT", "TSLPPCT", "TSMAMT", "TSMPCT",
    "SLO",
];

/// `outside_rth` values, as the SDK spells them.
const OUTSIDE_RTH: &[&str] = &["RTH_ONLY", "ANY_TIME", "OVERNIGHT", "OPTION_PRE_MARKET"];

/// The canonical spelling of `value` among `choices`, ignoring case, `_`, `-`
/// and spaces; an unknown value lists the choices.
fn canonical_choice(field: &str, value: &str, choices: &[&str]) -> Result<String, McpError> {
    let key = |s: &str| {
        s.chars()
            .filter(|c| !matches!(c, '_' | '-' | ' '))
            .collect::<String>()
            .to_lowercase()
    };
    let wanted = key(value);
    choices
        .iter()
        .find(|c| key(c) == wanted)
        .map(|c| (*c).to_string())
        .ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "invalid {field}: '{value}', expected one of: {}",
                    choices.join(", ")
                ),
                None,
            )
        })
}

/// Fields an order type cannot be placed without, per the `order_type` docs.
/// Checked before the dry run so a preview the user confirms is one the
/// exchange can accept, rather than failing only on execute.
fn missing_order_fields(p: &SubmitOrderParam) -> Vec<&'static str> {
    let order_type = p.order_type.trim().to_uppercase();
    let mut required: Vec<(&'static str, bool)> = Vec::new();
    if matches!(
        order_type.as_str(),
        "LO" | "ELO" | "ALO" | "ODD" | "LIT" | "SLO"
    ) {
        required.push(("submitted_price", p.submitted_price.is_some()));
    }
    if matches!(order_type.as_str(), "LIT" | "MIT") {
        required.push(("trigger_price", p.trigger_price.is_some()));
    }
    if order_type == "TSLPAMT" {
        required.push(("trailing_amount", p.trailing_amount.is_some()));
    }
    if order_type == "TSLPPCT" {
        required.push(("trailing_percent", p.trailing_percent.is_some()));
    }
    if matches!(order_type.as_str(), "TSLPAMT" | "TSLPPCT") {
        required.push(("limit_offset", p.limit_offset.is_some()));
    }
    if p.time_in_force.trim().eq_ignore_ascii_case("GTD") {
        required.push(("expire_date", p.expire_date.is_some()));
    }
    required
        .into_iter()
        .filter(|(_, present)| !present)
        .map(|(field, _)| field)
        .collect()
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

    // Normalize the enum spellings up front ("buy" → "Buy", "lo" → "LO") so
    // the preview, the confirmation scope and the order all use one form.
    let mut p = p;
    p.order_type = canonical_choice("order_type", &p.order_type, ORDER_TYPES)?;
    p.side = canonical_choice("side", &p.side, &["Buy", "Sell"])?;
    p.time_in_force = canonical_choice("time_in_force", &p.time_in_force, &["Day", "GTC", "GTD"])?;
    if let Some(rth) = p.outside_rth.take() {
        p.outside_rth = Some(canonical_choice("outside_rth", &rth, OUTSIDE_RTH)?);
    }
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

    let missing = missing_order_fields(&p);
    if !missing.is_empty() {
        return Err(McpError::invalid_params(
            format!(
                "{} {} order requires {}",
                p.order_type,
                p.time_in_force,
                missing.join(", ")
            ),
            None,
        ));
    }

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
    if let Some(ref v) = p.client_request_id {
        opts = opts.client_request_id(v.clone());
    }
    if let Some(ref v) = p.attached_order_type {
        opts = opts.attached_params(attached_submit_params(&p, v)?);
    }

    let scope = submit_scope(&p);
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
                "client_request_id": p.client_request_id,
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

/// Submit a multi-leg option combination order (vertical spreads, straddles,
/// strangles, collars, covered calls/puts). Every leg is submitted together as
/// a single strategy order and fills or rests as one.
pub async fn submit_multileg_order(
    mctx: &crate::tools::McpContext,
    p: SubmitMultiLegOrderParam,
) -> Result<CallToolResult, McpError> {
    use longbridge::Decimal;
    use longbridge::trade::{
        MultiLegStrategy, OrderSide, OrderType, SubmitMultiLegOrderLeg, SubmitMultiLegOrderOptions,
    };
    use std::str::FromStr;

    if p.legs.is_empty() {
        return Err(McpError::invalid_params(
            "legs must not be empty: a multi-leg order needs at least one leg",
            None,
        ));
    }

    let strategy = p
        .strategy
        .parse::<MultiLegStrategy>()
        .map_err(|e| McpError::invalid_params(format!("invalid strategy: {e}"), None))?;
    let side = p
        .side
        .parse::<OrderSide>()
        .map_err(|e| McpError::invalid_params(format!("invalid side: {e}"), None))?;
    let order_type = p
        .order_type
        .parse::<OrderType>()
        .map_err(|e| McpError::invalid_params(format!("invalid order_type: {e}"), None))?;
    let quantity = Decimal::from_str(&p.submitted_quantity)
        .map_err(|e| McpError::invalid_params(format!("invalid submitted_quantity: {e}"), None))?;

    let legs = p
        .legs
        .into_iter()
        .map(|leg| {
            let ratio = Decimal::from_str(&leg.ratio_quantity).map_err(|e| {
                McpError::invalid_params(
                    format!("invalid ratio_quantity for leg `{}`: {e}", leg.symbol),
                    None,
                )
            })?;
            if ratio <= Decimal::ZERO {
                return Err(McpError::invalid_params(
                    format!(
                        "ratio_quantity for leg `{}` must be positive; leg direction comes from `strategy` and `side`",
                        leg.symbol
                    ),
                    None,
                ));
            }
            Ok(SubmitMultiLegOrderLeg::new(leg.symbol, ratio))
        })
        .collect::<Result<Vec<_>, McpError>>()?;

    let mut opts = SubmitMultiLegOrderOptions::new(side, order_type, quantity, strategy, legs);
    if let Some(ref price) = p.submitted_price {
        opts = opts.submitted_price(Decimal::from_str(price).map_err(|e| {
            McpError::invalid_params(format!("invalid submitted_price: {e}"), None)
        })?);
    }
    if let Some(ref v) = p.remark {
        opts = opts.remark(v.clone());
    }
    if let Some(ref v) = p.client_request_id {
        opts = opts.client_request_id(v.clone());
    }

    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.submit_multileg(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
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
    let scope = replace_scope(&p);
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
    const GATED_TOOLS: [&str; 10] = [
        "submit_order",
        "cancel_order",
        "replace_order",
        "grid_submit",
        "grid_replace",
        "grid_cancel",
        "grid_suspend",
        "grid_restart",
        "fund_submit_order",
        "fund_cancel_order",
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
    use longbridge::DcRegion;
    use rmcp::schemars::schema_for;
    use time::macros::datetime;

    use super::{
        ExecutionsSource, HistoryExecutionsParam, ORDER_TYPES, ReplaceOrderParam, SubmitOrderParam,
        UsPages, canonical_choice, collect_us_pages, executions_source, history_range,
        missing_order_fields, replace_scope, submit_scope,
    };

    #[test]
    fn history_range_reads_bare_dates_in_new_york_for_us_accounts() {
        let (start, end) = history_range(DcRegion::Us, "2026-10-22", "2026-10-22")
            .expect("bare dates must parse for a US account");
        assert_eq!(start, datetime!(2026-10-22 04:00 UTC));
        assert_eq!(end, datetime!(2026-10-23 03:59:59 UTC));
    }

    #[test]
    fn history_range_reads_bare_dates_in_utc_for_ap_accounts() {
        let (start, end) = history_range(DcRegion::Ap, "2026-10-22", "2026-10-22")
            .expect("bare dates must parse for an AP account");
        assert_eq!(start, datetime!(2026-10-22 00:00 UTC));
        assert_eq!(end, datetime!(2026-10-22 23:59:59 UTC));
    }

    #[test]
    fn history_range_takes_rfc3339_bounds_as_given_in_every_region() {
        for region in [DcRegion::Us, DcRegion::Ap] {
            let (start, end) =
                history_range(region, "2026-10-22T13:30:00Z", "2026-10-22T20:00:00Z")
                    .expect("RFC3339 bounds must parse");
            assert_eq!(start, datetime!(2026-10-22 13:30 UTC), "{region:?} start");
            assert_eq!(end, datetime!(2026-10-22 20:00 UTC), "{region:?} end");
        }
    }

    #[test]
    fn history_executions_uses_v1_history_for_us_and_v3_all_elsewhere() {
        assert_eq!(executions_source(DcRegion::Us), ExecutionsSource::V1History);
        assert_eq!(executions_source(DcRegion::Ap), ExecutionsSource::V3All);
    }

    #[test]
    fn history_executions_schema_has_no_us_paging_fields() {
        let schema = serde_json::to_value(schema_for!(HistoryExecutionsParam))
            .expect("schema must serialize");
        let props = schema["properties"]
            .as_object()
            .expect("schema must have properties");
        let mut keys: Vec<&str> = props.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["end_at", "start_at", "symbol"]);
    }

    /// Drive `collect_us_pages` over a fake endpoint holding `total` records,
    /// returning what it collected and the pages it asked for.
    async fn walk(total: usize, size: i32, max_pages: i32) -> (UsPages<usize>, Vec<i32>) {
        let mut requested = Vec::new();
        let pages = collect_us_pages(size, max_pages, |page| {
            requested.push(page);
            let size = size as usize;
            let start = (page as usize - 1) * size;
            let batch: Vec<usize> = (start..total.min(start + size)).collect();
            async move { Ok::<_, std::convert::Infallible>(batch) }
        })
        .await
        .unwrap_or_else(|e| match e {});
        (pages, requested)
    }

    #[tokio::test]
    async fn collect_us_pages_stops_at_the_first_short_page() {
        let (pages, requested) = walk(250, 100, 20).await;
        assert_eq!(pages.items.len(), 250, "every record must be collected");
        assert_eq!(requested, [1, 2, 3], "a short third page ends the walk");
        assert!(!pages.truncated, "a short last page is not a truncation");
    }

    #[tokio::test]
    async fn collect_us_pages_fetches_one_more_page_after_an_exact_fit() {
        // No total count upstream: a full page could be followed by more, so
        // the walk only ends on the (empty) page after it.
        let (pages, requested) = walk(200, 100, 20).await;
        assert_eq!(pages.items.len(), 200, "every record must be collected");
        assert_eq!(requested, [1, 2, 3], "an empty page ends the walk");
        assert!(!pages.truncated, "an empty last page is not a truncation");
    }

    #[tokio::test]
    async fn collect_us_pages_reports_truncation_at_the_page_cap() {
        let (pages, requested) = walk(1000, 100, 3).await;
        assert_eq!(
            pages.items.len(),
            300,
            "only the capped pages are collected"
        );
        assert_eq!(requested, [1, 2, 3], "no page past the cap is requested");
        assert!(
            pages.truncated,
            "a full page at the cap may hide more records"
        );
    }

    #[tokio::test]
    async fn collect_us_pages_propagates_errors() {
        let result: Result<UsPages<usize>, &str> =
            collect_us_pages(100, 20, |_| async { Err("upstream down") }).await;
        assert_eq!(
            result.map(|p| p.items.len()),
            Err("upstream down"),
            "a failed page must fail the call"
        );
    }

    #[test]
    fn a_replace_code_does_not_carry_over_to_another_leg_change() {
        let replace = |extra: serde_json::Value| -> ReplaceOrderParam {
            let mut base = serde_json::json!({
                "order_id": "1", "quantity": "100", "attached_order_type": "STOP_LOSS",
                "attached_stop_loss_id": "9", "attached_quantity": "100"
            });
            if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
                b.extend(e.clone());
            }
            serde_json::from_value(base).expect("the test replace must deserialize")
        };
        let code = replace_scope(&replace(serde_json::json!({}))).code();
        for change in [
            serde_json::json!({"attached_quantity": "1000"}),
            serde_json::json!({"attached_main_id": "2"}),
            serde_json::json!({"attached_market_price": "10"}),
            serde_json::json!({"attached_cancel_all": false}),
        ] {
            assert!(
                replace_scope(&replace(change.clone()))
                    .verify(&code)
                    .is_err(),
                "a code for the previewed leg change must not apply {change}"
            );
        }
    }

    #[test]
    fn a_confirmation_code_does_not_carry_over_to_a_changed_order() {
        let base = serde_json::json!({"order_type": "LO", "submitted_price": "400"});
        let code = submit_scope(&order(base.clone())).code();
        for change in [
            serde_json::json!({"order_type": "MO"}),
            serde_json::json!({"time_in_force": "GTC"}),
            serde_json::json!({"trigger_price": "380"}),
            serde_json::json!({"outside_rth": "ANY_TIME"}),
        ] {
            let mut changed = base.clone();
            if let (Some(c), Some(d)) = (changed.as_object_mut(), change.as_object()) {
                c.extend(d.clone());
            }
            assert!(
                submit_scope(&order(changed)).verify(&code).is_err(),
                "a code for the LO Day preview must not place {change}"
            );
        }
        assert!(
            submit_scope(&order(base.clone())).verify(&code).is_ok(),
            "the same order must still verify"
        );
        let mut keyed = base;
        if let Some(o) = keyed.as_object_mut() {
            o.insert(
                "client_request_id".into(),
                serde_json::json!("key-made-on-execute"),
            );
            o.insert("remark".into(), serde_json::json!("note"));
        }
        assert!(
            submit_scope(&order(keyed)).verify(&code).is_ok(),
            "an idempotency key or remark added on execute must not reject the approved order"
        );
    }

    #[test]
    fn canonical_choice_ignores_case_and_lists_choices() {
        assert_eq!(
            canonical_choice("side", "buy", &["Buy", "Sell"]).expect("buy is a side"),
            "Buy",
            "case must not matter"
        );
        assert_eq!(
            canonical_choice("order_type", "lo", ORDER_TYPES).expect("lo is an order type"),
            "LO",
            "order types are upper-cased"
        );
        assert_eq!(
            canonical_choice("outside_rth", "any time", super::OUTSIDE_RTH)
                .expect("any time is valid"),
            "ANY_TIME",
            "separators must not matter"
        );
        let err =
            canonical_choice("side", "long", &["Buy", "Sell"]).expect_err("long is not a side");
        assert!(
            err.message.contains("expected one of: Buy, Sell"),
            "{}",
            err.message
        );
    }
    use crate::serialize::to_tool_json;

    fn order(extra: serde_json::Value) -> SubmitOrderParam {
        let mut base = serde_json::json!({
            "symbol": "700.HK", "side": "Buy", "submitted_quantity": 100, "time_in_force": "Day"
        });
        if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
            b.extend(e.clone());
        }
        serde_json::from_value(base).expect("the test order must deserialize")
    }

    #[test]
    fn missing_order_fields_follow_the_order_type() {
        assert_eq!(
            missing_order_fields(&order(serde_json::json!({"order_type": "LO"}))),
            ["submitted_price"],
            "a limit order needs a price"
        );
        assert!(
            missing_order_fields(&order(
                serde_json::json!({"order_type": "LO", "submitted_price": 1.5})
            ))
            .is_empty(),
            "a priced limit order is complete"
        );
        assert!(
            missing_order_fields(&order(serde_json::json!({"order_type": "MO"}))).is_empty(),
            "a market order needs no price"
        );
        assert_eq!(
            missing_order_fields(&order(serde_json::json!({"order_type": "TSLPPCT"}))),
            ["trailing_percent", "limit_offset"],
            "a trailing order needs its trail and offset"
        );
        assert_eq!(
            missing_order_fields(&order(
                serde_json::json!({"order_type": "MO", "time_in_force": "GTD"})
            )),
            ["expire_date"],
            "a GTD order needs an expiry"
        );
    }

    /// Simulate the raw JSON that the Longbridge SDK's `FundPositionsResponse`
    /// would produce after serde serialization, then verify that `to_tool_json`
    /// transforms it correctly.
    #[allow(clippy::too_many_arguments)]
    fn sdk_fund_positions_json(
        account_channel: &str,
        counter_id: &str,
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
                    "counter_id": counter_id,
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

        assert_eq!(
            pos["counter_id"], "HK0000038064",
            "counter_id mismatch: {output}"
        );
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
}

/// AP output goldens: these tools gained a US variant, and their AP output
/// must stay byte-for-byte what it was. Each fixture is a raw AP response
/// run through the same post-processing and serializer the AP path uses.
#[cfg(test)]
mod ap_output_tests {
    use longbridge::trade::{AccountBalance, CashFlow, Order, OrderDetail, StockPositionsResponse};
    use serde::de::DeserializeOwned;
    use serde_json::Value;

    use super::{
        ap_account_balance_json, ap_order_detail_json, ap_orders_json, ap_stock_positions_json,
    };
    use crate::serialize::to_tool_json;

    fn fixture<T>(text: &str) -> T
    where
        T: DeserializeOwned,
    {
        serde_json::from_str(text).expect("AP fixture deserializes as the SDK type")
    }

    /// What the model receives: `tool_json`'s serializer over `value`.
    fn rendered<T>(value: &T) -> Value
    where
        T: serde::Serialize,
    {
        let json = to_tool_json(value).expect("tool JSON serializes");
        serde_json::from_str(&json).expect("tool JSON parses")
    }

    fn assert_golden(name: &str, actual: Value, golden: &str) {
        let expected: Value = serde_json::from_str(golden).expect("golden is valid JSON");
        assert_eq!(
            actual,
            expected,
            "AP `{name}` output changed:\n{}",
            serde_json::to_string_pretty(&actual).unwrap_or_default()
        );
    }

    #[test]
    fn ap_account_balance_output_is_unchanged() {
        let result: Vec<AccountBalance> = fixture(include_str!("fixtures/ap/account_balance.json"));
        let value = ap_account_balance_json(&result).expect("AP balance serializes");
        assert_golden(
            "account_balance",
            rendered(&value),
            include_str!("fixtures/ap/account_balance.expected.json"),
        );
    }

    #[test]
    fn ap_stock_positions_output_is_unchanged() {
        let result: StockPositionsResponse =
            fixture(include_str!("fixtures/ap/stock_positions.json"));
        let value = ap_stock_positions_json(&result).expect("AP positions serialize");
        assert_golden(
            "stock_positions",
            rendered(&value),
            include_str!("fixtures/ap/stock_positions.expected.json"),
        );
    }

    #[test]
    fn ap_cash_flow_output_is_unchanged() {
        let result: Vec<CashFlow> = fixture(include_str!("fixtures/ap/cash_flow.json"));
        assert_golden(
            "cash_flow",
            rendered(&result),
            include_str!("fixtures/ap/cash_flow.expected.json"),
        );
    }

    #[test]
    fn ap_orders_output_is_unchanged() {
        let result: Vec<Order> = fixture(include_str!("fixtures/ap/orders.json"));
        let value = ap_orders_json(&result).expect("AP orders serialize");
        assert_golden(
            "orders",
            rendered(&value),
            include_str!("fixtures/ap/orders.expected.json"),
        );
    }

    #[test]
    fn ap_order_detail_output_is_unchanged() {
        let result: OrderDetail = fixture(include_str!("fixtures/ap/order_detail.json"));
        let value = ap_order_detail_json(&result).expect("AP order detail serializes");
        assert_golden(
            "order_detail",
            rendered(&value),
            include_str!("fixtures/ap/order_detail.expected.json"),
        );
    }
}
