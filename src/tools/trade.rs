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
    /// Order ID (returned by submit_order or listed in today_orders / history_orders)
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
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryOrdersParam {
    /// Filter by symbol (optional)
    pub symbol: Option<String>,
    /// Start time (RFC3339)
    pub start_at: String,
    /// End time (RFC3339)
    pub end_at: String,
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
    /// Buy or Sell
    pub side: String,
    /// Order type: LO (Limit Order) / ELO (Enhanced Limit Order) / MO (Market Order) / AO (At-auction) / ALO (At-auction Limit Order)
    pub order_type: String,
    /// Limit price for limit-style orders. Omit for market orders.
    pub price: Option<String>,
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
    tool_json(&result)
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
    let mut opts = GetTodayOrdersOptions::new();
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.today_orders(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn order_detail(
    mctx: &crate::tools::McpContext,
    p: OrderIdParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx
        .order_detail(p.order_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn cancel_order(
    mctx: &crate::tools::McpContext,
    p: OrderIdParam,
) -> Result<CallToolResult, McpError> {
    let (ctx, _) = TradeContext::new(mctx.create_config());
    ctx.cancel_order(p.order_id)
        .await
        .map_err(Error::longbridge)?;
    Ok(tool_result("order cancelled".to_string()))
}

pub async fn today_executions(
    mctx: &crate::tools::McpContext,
    p: TodayExecutionsParam,
) -> Result<CallToolResult, McpError> {
    let mut opts = GetTodayExecutionsOptions::new();
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    if let Some(order_id) = p.order_id {
        opts = opts.order_id(order_id);
    }
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx
        .today_executions(opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn history_orders(
    mctx: &crate::tools::McpContext,
    p: HistoryOrdersParam,
) -> Result<CallToolResult, McpError> {
    let start = parse::parse_rfc3339(&p.start_at)?;
    let end = parse::parse_rfc3339(&p.end_at)?;
    let mut opts = longbridge::trade::GetHistoryOrdersOptions::new()
        .start_at(start)
        .end_at(end);
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.history_orders(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn history_executions(
    mctx: &crate::tools::McpContext,
    p: HistoryOrdersParam,
) -> Result<CallToolResult, McpError> {
    let start = parse::parse_rfc3339(&p.start_at)?;
    let end = parse::parse_rfc3339(&p.end_at)?;
    let mut opts = longbridge::trade::GetHistoryExecutionsOptions::new()
        .start_at(start)
        .end_at(end);
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx
        .history_executions(opts)
        .await
        .map_err(Error::longbridge)?;
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

    let mut opts = SubmitOrderOptions::new(p.symbol, order_type, side, quantity, tif);

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

    let (ctx, _) = TradeContext::new(mctx.create_config());
    let result = ctx.submit_order(opts).await.map_err(Error::longbridge)?;
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
    let mut opts = ReplaceOrderOptions::new(p.order_id, quantity);
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

    let side = p
        .side
        .parse::<OrderSide>()
        .map_err(|e| McpError::invalid_params(format!("invalid side: {e}"), None))?;
    let order_type = p
        .order_type
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
