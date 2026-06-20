use longbridge::trade::{GetTodayExecutionsOptions, GetTodayOrdersOptions};
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

pub async fn account_balance(
    mctx: &crate::tools::McpContext,
    p: AccountBalanceParam,
) -> Result<CallToolResult, McpError> {
    let ctx = mctx.get_trade_context().await;
    let result = ctx
        .account_balance(p.currency.as_deref())
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn stock_positions(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let ctx = mctx.get_trade_context().await;
    let result = ctx.stock_positions(None).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn fund_positions(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let ctx = mctx.get_trade_context().await;
    let result = ctx.fund_positions(None).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn margin_ratio(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let ctx = mctx.get_trade_context().await;
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
    let ctx = mctx.get_trade_context().await;
    let result = ctx.today_orders(opts).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn order_detail(
    mctx: &crate::tools::McpContext,
    p: OrderIdParam,
) -> Result<CallToolResult, McpError> {
    let ctx = mctx.get_trade_context().await;
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
    let ctx = mctx.get_trade_context().await;
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

    let ctx = mctx.get_trade_context().await;
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
    let mut opts = longbridge::trade::GetHistoryOrdersOptions::new()
        .start_at(start)
        .end_at(end);
    if let Some(symbol) = p.symbol {
        opts = opts.symbol(symbol);
    }
    let ctx = mctx.get_trade_context().await;
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

    let ctx = mctx.get_trade_context().await;
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
    let ctx = mctx.get_trade_context().await;
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

    let ctx = mctx.get_trade_context().await;
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
    let ctx = mctx.get_trade_context().await;
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
    let ctx = mctx.get_trade_context().await;
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
}
