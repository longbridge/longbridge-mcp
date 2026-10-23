//! Maps US-account responses onto the AP output shape of the same tool.
//!
//! US accounts are served by separate US endpoints whose responses use their
//! own field names, codes and timestamp formats. These builders rename the
//! fields that mean the same thing to the AP names, convert values to the AP
//! formats (RFC3339 times, `Buy`/`Sell`, `In`/`Out`, fully-qualified
//! symbols) and keep the top-level shape the AP path returns. Fields with no
//! AP counterpart are kept under distinct names (or a `us` sub-object); AP
//! fields with no US source are left out rather than invented.

use std::str::FromStr;

use longbridge::Decimal;
use longbridge::trade::{
    USAssetOverview, USCashAndBuyPower, USExchangeRate, USStockEntry, USTransaction,
};
use serde_json::{Map, Number, Value};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::serialize::strip_empty_strings;
use crate::tools::support::us_normalize::normalize_us_crypto_list;

/// Rate type preferred when the US feed lists one pair under several types.
const PREFERRED_RATE_SPEC: &str = "reference";

/// Fallback rate type when the preferred one is missing for a pair.
const FALLBACK_RATE_SPEC: &str = "am";

/// `TYPE/MARKET/CODE` (e.g. `ST/US/INTC`, `VA/BKKT/BTCUSD`) to the
/// user-facing `CODE.MARKET` (`INTC.US`, `BTCUSD.BKKT`). `None` unless all
/// three parts are present.
pub fn symbol_from_counter_id(counter_id: &str) -> Option<String> {
    let mut parts = counter_id.splitn(3, '/');
    let (_, market, code) = (parts.next()?, parts.next()?, parts.next()?);
    if market.is_empty() || code.is_empty() {
        return None;
    }
    Some(format!("{code}.{market}"))
}

/// A unix-seconds string (`"1790935832"`) as the RFC3339 UTC string AP
/// outputs use (`"2026-10-02T10:10:32Z"`). `None` for empty, zero or
/// unparsable input.
pub fn unix_to_rfc3339(s: &str) -> Option<String> {
    let secs = s.trim().parse::<i64>().ok().filter(|secs| *secs > 0)?;
    rfc3339(OffsetDateTime::from_unix_timestamp(secs).ok()?)
}

fn rfc3339(t: OffsetDateTime) -> Option<String> {
    t.format(&Rfc3339).ok()
}

/// The AP cash-flow `direction` for a US transaction: the sign of `amount`,
/// or of `quantity` when no cash moved (crypto transfers and fees carry
/// `amount` "0.00" and a signed quantity). `Unknown` when both are zero.
pub fn cash_flow_direction(amount: &str, quantity: &str) -> &'static str {
    let sign = |s: &str| Decimal::from_str(s.trim()).ok().filter(|d| !d.is_zero());
    match sign(amount).or_else(|| sign(quantity)) {
        Some(d) if d.is_sign_negative() => "Out",
        Some(_) => "In",
        None => "Unknown",
    }
}

/// Inserts `value` under `key` unless it is empty.
fn put(map: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.is_empty() {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

/// Inserts the non-empty string `src[from]` under `to`.
fn copy_str(map: &mut Map<String, Value>, src: &Value, from: &str, to: &str) {
    if let Some(s) = src.get(from).and_then(Value::as_str) {
        put(map, to, s);
    }
}

/// Inserts the non-zero integer `src[from]` under `to`.
fn copy_nonzero_int(map: &mut Map<String, Value>, src: &Value, from: &str, to: &str) {
    if let Some(n) = src.get(from).and_then(Value::as_i64).filter(|n| *n != 0) {
        map.insert(to.to_string(), Value::from(n));
    }
}

/// Whether `s` is a decimal equal to zero ("0", "0.00").
fn is_zero_decimal(s: &str) -> bool {
    Decimal::from_str(s.trim()).is_ok_and(|d| d.is_zero())
}

/// `account_balance` for a US account, in the AP shape: a top-level array
/// with one entry for the account currency.
///
/// Mapped: `total_cash` (that currency's `total_amount`), `net_assets`
/// (`nlv`), `buy_power` (`cash_buy_power`), `maintenance_margin` (`mmr`),
/// and `cash_infos[]` per currency. `settled_cash` is not AP's
/// `settling_cash` and stays under its own name. The full US buying-power
/// and margin blocks, plus per-currency `outstanding`/`external_amount`, go
/// under `us`. AP-only fields (`max_finance_amount`,
/// `remaining_finance_amount`, `risk_level`, `margin_call`, `init_margin`,
/// `frozen_transaction_fees`) have no US source and are omitted.
pub fn account_balance(src: &USCashAndBuyPower) -> Result<Value, serde_json::Error> {
    let currency = [
        src.margin_status.currency.as_str(),
        src.buy_limit_info.currency.as_str(),
    ]
    .into_iter()
    .chain(src.cash_list.iter().map(|c| c.currency.as_str()))
    .find(|c| !c.is_empty());
    let Some(currency) = currency else {
        return Ok(Value::Array(Vec::new()));
    };

    let mut entry = Map::new();
    put(&mut entry, "currency", currency);
    if let Some(cash) = src.cash_list.iter().find(|c| c.currency == currency) {
        put(&mut entry, "total_cash", &cash.total_amount);
    }
    put(&mut entry, "net_assets", &src.margin_status.nlv);
    put(&mut entry, "buy_power", &src.buy_limit_info.cash_buy_power);
    put(&mut entry, "maintenance_margin", &src.margin_status.mmr);

    let mut cash_infos = Vec::new();
    let mut us_cash = Vec::new();
    for cash in &src.cash_list {
        let mut info = Map::new();
        put(&mut info, "currency", &cash.currency);
        put(&mut info, "available_cash", &cash.available_cash);
        put(&mut info, "withdraw_cash", &cash.withdraw_cash);
        put(&mut info, "frozen_cash", &cash.frozen_cash);
        put(&mut info, "settled_cash", &cash.settled_cash);
        cash_infos.push(Value::Object(info));

        let mut extra = Map::new();
        put(&mut extra, "currency", &cash.currency);
        put(&mut extra, "outstanding", &cash.outstanding);
        put(&mut extra, "external_amount", &cash.external_amount);
        us_cash.push(Value::Object(extra));
    }
    entry.insert("cash_infos".to_string(), Value::Array(cash_infos));

    let mut us = Map::new();
    us.insert(
        "buy_limit_info".to_string(),
        serde_json::to_value(&src.buy_limit_info)?,
    );
    us.insert(
        "margin_status".to_string(),
        serde_json::to_value(&src.margin_status)?,
    );
    us.insert("cash_list".to_string(), Value::Array(us_cash));
    entry.insert("us".to_string(), Value::Object(us));
    Ok(Value::Array(vec![Value::Object(entry)]))
}

/// One US stock holding as an AP `stock_info[]` entry. `available_quantity`
/// has no source in the US overview and is omitted; US quote/P&L fields are
/// kept as extras.
fn stock_position(stock: &USStockEntry) -> Value {
    let symbol = [stock.full_symbol.clone(), stock.symbol.clone()]
        .into_iter()
        .find(|s| s.contains('.'))
        .unwrap_or_else(|| stock.symbol.clone());
    let mut map = Map::new();
    put(&mut map, "symbol", &symbol);
    put(&mut map, "symbol_name", &stock.name);
    put(&mut map, "quantity", &stock.quantity);
    put(&mut map, "currency", &stock.currency);
    put(&mut map, "cost_price", &stock.average_cost);
    put(&mut map, "market", &stock.market);
    put(&mut map, "asset_type", &stock.asset_type);
    put(&mut map, "position_side", &stock.position_side);
    put(&mut map, "last_done", &stock.last_done);
    put(&mut map, "market_price", &stock.market_price);
    put(&mut map, "prev_close", &stock.prev_close);
    put(&mut map, "today_pl", &stock.today_pl);
    put(&mut map, "industry_name", &stock.industry_name);
    Value::Object(map)
}

/// `stock_positions` for a US account, in the AP shape: `list[].stock_info[]`
/// with a single channel (the account channel is not part of the US
/// overview, and AP hides it anyway). Everything else in the overview —
/// cash, options, crypto (with qualified symbols), buying power — is kept
/// under `us`, minus empty strings.
pub fn stock_positions(src: &USAssetOverview) -> Result<Value, serde_json::Error> {
    let stocks: Vec<Value> = src.stock_list.iter().map(stock_position).collect();
    let list = if stocks.is_empty() {
        Vec::new()
    } else {
        vec![serde_json::json!({ "stock_info": stocks })]
    };
    let mut us = serde_json::to_value(src)?;
    if let Value::Object(map) = &mut us {
        map.remove("stock_list");
    }
    normalize_us_crypto_list(&mut us);
    strip_empty_strings(&mut us);
    Ok(serde_json::json!({ "list": list, "us": us }))
}

/// One US transaction as an AP `cash_flow` entry. `balance` is the signed
/// amount, as AP returns it (0 for crypto movements, where `direction`
/// follows the quantity's sign instead); `business_type`
/// (AP's cash/stock/fund code) has no US equivalent and is omitted in favour
/// of `biz_code`/`biz_code_name`.
pub fn cash_flow_item(t: &USTransaction) -> Value {
    let mut map = Map::new();
    put(&mut map, "transaction_flow_name", &t.title);
    map.insert(
        "direction".to_string(),
        Value::String(cash_flow_direction(&t.amount, &t.quantity).to_string()),
    );
    put(&mut map, "balance", &t.amount);
    put(&mut map, "currency", &t.currency);
    if let Some(time) = t.done_at.or(t.create_at).and_then(rfc3339) {
        map.insert("business_time".to_string(), Value::String(time));
    }
    put(&mut map, "symbol", &t.symbol);
    put(&mut map, "description", &t.remark);
    put(&mut map, "biz_code", &t.biz_code);
    put(&mut map, "biz_code_name", &t.biz_code_name);
    if !is_zero_decimal(&t.quantity) {
        put(&mut map, "quantity", &t.quantity);
    }
    put(&mut map, "quantity_description", &t.quantity_description);
    put(&mut map, "ref_no", &t.ref_no);
    put(&mut map, "equity_type", &t.equity_type);
    Value::Object(map)
}

/// `cash_flow` for a US account: a top-level array, like AP.
pub fn cash_flow(items: &[USTransaction]) -> Value {
    Value::Array(items.iter().map(cash_flow_item).collect())
}

/// A rate string as a JSON number (AP's `average_rate` is numeric); kept as
/// a string if it does not parse.
fn rate_value(s: &str) -> Value {
    Number::from_str(s.trim()).map_or_else(|_| Value::String(s.to_string()), Value::Number)
}

/// Whether two rate strings name the same number ("7.80" and "7.8" do).
fn same_rate(a: &str, b: &str) -> bool {
    match (Decimal::from_str(a), Decimal::from_str(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// `exchange_rate` for a US account, in the AP shape `{exchanges[]}`. The US
/// feed can list a currency pair once per rate type; one row per pair is
/// kept, preferring the `reference` rate and falling back to `am` (then to
/// whatever type is listed), with the chosen type kept as `spec_key`.
/// `bid_rate`/`offer_rate` are kept only when they differ from
/// `average_rate` (AP drops them because its feed is a single mid rate).
pub fn exchange_rates(rates: &[USExchangeRate]) -> Value {
    let rank = |spec: &str| match spec {
        PREFERRED_RATE_SPEC => 0,
        FALLBACK_RATE_SPEC => 1,
        _ => 2,
    };
    let mut chosen: Vec<&USExchangeRate> = Vec::new();
    for rate in rates {
        let same_pair = |r: &&USExchangeRate| {
            r.base_currency == rate.base_currency && r.other_currency == rate.other_currency
        };
        match chosen.iter_mut().find(|r| same_pair(r)) {
            Some(slot) if rank(&rate.spec_key) < rank(&slot.spec_key) => *slot = rate,
            Some(_) => {}
            None => chosen.push(rate),
        }
    }
    let exchanges: Vec<Value> = chosen
        .into_iter()
        .map(|rate| {
            let mut map = Map::new();
            put(&mut map, "base_currency", &rate.base_currency);
            put(&mut map, "other_currency", &rate.other_currency);
            map.insert("average_rate".to_string(), rate_value(&rate.average_rate));
            for (key, side) in [
                ("bid_rate", &rate.bid_rate),
                ("offer_rate", &rate.offer_rate),
            ] {
                if !side.is_empty() && !same_rate(side, &rate.average_rate) {
                    map.insert(key.to_string(), rate_value(side));
                }
            }
            put(&mut map, "spec_key", &rate.spec_key);
            Value::Object(map)
        })
        .collect();
    serde_json::json!({ "exchanges": exchanges })
}

/// AP `side` for a US `action` code.
fn order_side(action: Option<i64>) -> &'static str {
    match action {
        Some(1) => "Buy",
        Some(2) => "Sell",
        _ => "Unknown",
    }
}

/// One AP `history[]` entry from a US `order_histories[]` entry.
fn order_history_entry(src: &Value) -> Value {
    let mut map = Map::new();
    copy_str(&mut map, src, "price", "price");
    copy_str(&mut map, src, "qty", "quantity");
    copy_str(&mut map, src, "status", "status");
    if let Some(msg) = src.get("msg").and_then(Value::as_str) {
        map.insert("msg".to_string(), Value::String(msg.to_string()));
    }
    if let Some(time) = src
        .get("time")
        .and_then(Value::as_str)
        .and_then(unix_to_rfc3339)
    {
        map.insert("time".to_string(), Value::String(time));
    }
    Value::Object(map)
}

/// One raw US order object (from `/v1/us/orders/query` or
/// `/v1/us/orders/{id}`) in the AP order shape.
///
/// Mapped to AP names: `order_id`, `status` (already AP's form, e.g.
/// `FilledStatus`), `stock_name`, `quantity`, `executed_quantity`, `price`,
/// `executed_price`, `submitted_at`/`updated_at` (RFC3339), `side`, `symbol`
/// (as given, or derived from `counter_id` where the gateway has not
/// converted it yet; `underlying_symbol` likewise for options),
/// `order_type`, `last_done`, `trigger_price`, `msg`, `currency`, `remark`,
/// the trailing/limit-offset fields and, for a detail, `history[]`.
/// `time_in_force` and `tag` use codes whose meaning differs from AP's, so
/// they are kept raw as `us_time_in_force`/`us_tag`; likewise
/// `us_trigger_status`, `us_attached_orders` and `us_charge_detail`.
/// Personal and bookkeeping fields (names, accounts, internal ids) are never
/// copied.
pub fn order(src: &Value) -> Value {
    let mut map = Map::new();
    copy_str(&mut map, src, "id", "order_id");
    copy_str(&mut map, src, "status", "status");
    copy_str(&mut map, src, "name", "stock_name");
    copy_str(&mut map, src, "quantity", "quantity");
    copy_str(&mut map, src, "executed_qty", "executed_quantity");
    copy_str(&mut map, src, "price", "price");
    if let Some(price) = src
        .get("executed_price")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty() && !is_zero_decimal(p))
    {
        put(&mut map, "executed_price", price);
    }
    for key in ["submitted_at", "updated_at"] {
        if let Some(time) = src
            .get(key)
            .and_then(Value::as_str)
            .and_then(unix_to_rfc3339)
        {
            map.insert(key.to_string(), Value::String(time));
        }
    }
    map.insert(
        "side".to_string(),
        Value::String(order_side(src.get("action").and_then(Value::as_i64)).to_string()),
    );
    for (symbol_key, counter_id_key) in [
        ("symbol", "counter_id"),
        ("underlying_symbol", "underlying_counter_id"),
    ] {
        if let Some(symbol) = order_symbol(src, symbol_key, counter_id_key) {
            map.insert(symbol_key.to_string(), Value::String(symbol));
        }
    }
    copy_str(&mut map, src, "order_type", "order_type");
    copy_str(&mut map, src, "last_done", "last_done");
    copy_str(&mut map, src, "trigger_price", "trigger_price");
    if let Some(msg) = src.get("msg").and_then(Value::as_str) {
        map.insert("msg".to_string(), Value::String(msg.to_string()));
    }
    copy_str(&mut map, src, "currency", "currency");
    copy_str(&mut map, src, "remark", "remark");
    copy_str(&mut map, src, "tailing_amount", "trailing_amount");
    copy_str(&mut map, src, "tailing_percent", "trailing_percent");
    copy_str(&mut map, src, "limit_offset", "limit_offset");
    copy_str(&mut map, src, "monitor_price", "monitor_price");
    copy_nonzero_int(&mut map, src, "limit_depth_level", "limit_depth_level");
    copy_nonzero_int(&mut map, src, "trigger_count", "trigger_count");
    if let Some(Value::Array(histories)) = src.get("order_histories") {
        let history: Vec<Value> = histories.iter().map(order_history_entry).collect();
        map.insert("history".to_string(), Value::Array(history));
    }

    if let Some(code) = src.get("time_in_force").and_then(Value::as_i64) {
        map.insert("us_time_in_force".to_string(), Value::from(code));
    }
    if let Some(code) = src.get("tag").and_then(Value::as_i64) {
        map.insert("us_tag".to_string(), Value::from(code));
    }
    copy_nonzero_int(&mut map, src, "trigger_status", "us_trigger_status");
    copy_str(&mut map, src, "gtd", "gtd");
    copy_str(&mut map, src, "security_type", "security_type");
    copy_str(&mut map, src, "operate_direction", "operate_direction");
    copy_str(&mut map, src, "executed_amount", "executed_amount");
    if let Some(fee) = src
        .get("estimated_fee")
        .and_then(Value::as_str)
        .filter(|f| !is_zero_decimal(f))
    {
        put(&mut map, "estimated_fee", fee);
    }
    if let Some(time) = src
        .get("done_at")
        .and_then(Value::as_str)
        .and_then(unix_to_rfc3339)
    {
        map.insert("done_at".to_string(), Value::String(time));
    }
    if let Some(Value::Array(legs)) = src.get("attached_orders")
        && !legs.is_empty()
    {
        map.insert(
            "us_attached_orders".to_string(),
            Value::Array(legs.iter().map(attached_leg).collect()),
        );
    }
    if let Some(charge) = src.get("charge_detail").filter(|c| !c.is_null()) {
        map.insert("us_charge_detail".to_string(), charge.clone());
    }
    Value::Object(map)
}

/// `src[symbol_key]` when the gateway already converted it, else derived
/// from `src[counter_id_key]`: the gateway's counter_id-to-symbol rename for
/// the US order endpoints is not deployed everywhere yet.
fn order_symbol(src: &Value, symbol_key: &str, counter_id_key: &str) -> Option<String> {
    let non_empty = |key: &str| {
        src.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    non_empty(symbol_key)
        .map(str::to_string)
        .or_else(|| non_empty(counter_id_key).and_then(symbol_from_counter_id))
}

/// A US attached (take-profit/stop-loss) leg with its empty fields dropped.
fn attached_leg(src: &Value) -> Value {
    let Value::Object(leg) = src else {
        return src.clone();
    };
    let mut leg = leg.clone();
    leg.retain(|_, v| !matches!(v, Value::Null) && !matches!(v, Value::String(s) if s.is_empty()));
    Value::Object(leg)
}

/// `order_detail` for a US account from the raw `/v1/us/orders/{id}` data
/// (`{order, current_attached_order, …}`): the order itself in the AP order
/// shape at the top level, like AP, with an active attached leg (if any)
/// under `us_current_attached_order`.
pub fn order_detail(data: &Value) -> Value {
    let mut value = data.get("order").map_or_else(|| order(&Value::Null), order);
    if let (Value::Object(map), Some(current)) = (
        &mut value,
        data.get("current_attached_order").filter(|c| c.is_object()),
    ) {
        map.insert("us_current_attached_order".to_string(), order(current));
    }
    value
}

/// `today_orders`/`history_orders` for a US account: a top-level array of
/// orders in the AP order shape.
pub fn orders(raw: &[Value]) -> Value {
    Value::Array(raw.iter().map(order).collect())
}

#[cfg(test)]
mod tests {
    use longbridge::trade::QueryUSOrdersResponse;
    use serde_json::json;

    use super::*;
    use crate::serialize::{strip_nulls, strip_trailing_zeros, to_tool_json};

    /// What the model receives for `value`: the same post-processing and
    /// serializer the US paths in `trade.rs`/`portfolio.rs` apply.
    fn rendered(mut value: Value) -> Value {
        strip_nulls(&mut value);
        strip_trailing_zeros(&mut value);
        let json = to_tool_json(&value).expect("tool JSON serializes");
        serde_json::from_str(&json).expect("tool JSON parses")
    }

    /// Like `rendered`, for `cash_flow`, whose US path (like AP's) keeps the
    /// upstream decimals.
    fn rendered_raw(value: &Value) -> Value {
        let json = to_tool_json(value).expect("tool JSON serializes");
        serde_json::from_str(&json).expect("tool JSON parses")
    }

    fn fixture(name: &str) -> Value {
        let text = match name {
            "cash_and_buy_power" => include_str!("fixtures/us_align/cash_and_buy_power.json"),
            "asset_overview" => include_str!("fixtures/us_align/asset_overview.json"),
            "transactions" => include_str!("fixtures/us_align/transactions.json"),
            "exchange_rates" => include_str!("fixtures/us_align/exchange_rates.json"),
            "orders" => include_str!("fixtures/us_align/orders.json"),
            "order_detail" => include_str!("fixtures/us_align/order_detail.json"),
            _ => unreachable!("unknown fixture `{name}`"),
        };
        serde_json::from_str(text).expect("fixture is valid JSON")
    }

    fn golden(name: &str) -> Value {
        let text = match name {
            "account_balance" => include_str!("fixtures/us_align/account_balance.expected.json"),
            "stock_positions" => include_str!("fixtures/us_align/stock_positions.expected.json"),
            "cash_flow" => include_str!("fixtures/us_align/cash_flow.expected.json"),
            "exchange_rate" => include_str!("fixtures/us_align/exchange_rate.expected.json"),
            "orders" => include_str!("fixtures/us_align/orders.expected.json"),
            "order_detail" => include_str!("fixtures/us_align/order_detail.expected.json"),
            _ => unreachable!("unknown golden `{name}`"),
        };
        serde_json::from_str(text).expect("golden is valid JSON")
    }

    fn assert_golden(name: &str, actual: Value) {
        let expected = golden(name);
        assert_eq!(
            actual,
            expected,
            "`{name}` US output drifted from its golden:\n{}",
            serde_json::to_string_pretty(&actual).unwrap_or_default()
        );
    }

    #[test]
    fn account_balance_matches_golden() {
        let src: USCashAndBuyPower = serde_json::from_value(fixture("cash_and_buy_power"))
            .expect("fixture deserializes as the SDK type");
        let value = account_balance(&src).expect("balance maps");
        assert_golden("account_balance", rendered(value));
    }

    #[test]
    fn account_balance_without_any_currency_is_an_empty_array() {
        let value = account_balance(&USCashAndBuyPower::default()).expect("balance maps");
        assert_eq!(
            value,
            json!([]),
            "nothing to report yields AP's empty array"
        );
    }

    #[test]
    fn stock_positions_matches_golden() {
        let src: USAssetOverview = serde_json::from_value(fixture("asset_overview"))
            .expect("fixture deserializes as the SDK type");
        let value = stock_positions(&src).expect("positions map");
        assert_golden("stock_positions", rendered(value));
    }

    #[test]
    fn stock_positions_without_stocks_has_an_empty_list() {
        let value = stock_positions(&USAssetOverview::default()).expect("positions map");
        assert_eq!(value["list"], json!([]), "no stock holdings, no channel");
        assert!(value["us"].is_object(), "the US overview is still returned");
    }

    #[test]
    fn cash_flow_matches_golden() {
        let items: Vec<USTransaction> = serde_json::from_value(fixture("transactions"))
            .expect("fixture deserializes as the SDK type");
        assert_golden("cash_flow", rendered_raw(&cash_flow(&items)));
    }

    #[test]
    fn exchange_rate_matches_golden() {
        let src: longbridge::trade::USExchangeRates =
            serde_json::from_value(fixture("exchange_rates"))
                .expect("fixture deserializes as the SDK type");
        assert_golden("exchange_rate", rendered(exchange_rates(&src.exchanges)));
    }

    #[test]
    fn orders_match_golden() {
        let src: QueryUSOrdersResponse =
            serde_json::from_value(fixture("orders")).expect("fixture deserializes");
        assert_golden("orders", rendered(orders(&src.orders)));
    }

    #[test]
    fn order_detail_matches_golden() {
        assert_golden(
            "order_detail",
            rendered(order_detail(&fixture("order_detail"))),
        );
    }

    #[test]
    fn direction_follows_amount_then_quantity() {
        assert_eq!(cash_flow_direction("59.99", "-60"), "In", "cash in wins");
        assert_eq!(cash_flow_direction("-60.02", "60"), "Out", "cash out wins");
        assert_eq!(
            cash_flow_direction("0.00", "0.00012"),
            "In",
            "no cash moved: a positive quantity is an inflow"
        );
        assert_eq!(
            cash_flow_direction("0.00", "-0.002"),
            "Out",
            "no cash moved: a negative quantity is an outflow"
        );
        assert_eq!(cash_flow_direction("0", "0"), "Unknown", "nothing moved");
        assert_eq!(cash_flow_direction("", ""), "Unknown", "no data");
    }

    #[test]
    fn cash_flow_balance_keeps_the_sign() {
        let item = cash_flow_item(&USTransaction {
            amount: "-4300.00".to_string(),
            quantity: "0".to_string(),
            ..Default::default()
        });
        assert_eq!(
            item["balance"],
            json!("-4300.00"),
            "balance is signed, as AP returns it"
        );
        assert_eq!(
            item["direction"],
            json!("Out"),
            "a negative amount is outflow"
        );
        assert!(item.get("quantity").is_none(), "a zero quantity is dropped");
        let crypto = cash_flow_item(&USTransaction {
            amount: "0.00".to_string(),
            quantity: "-0.01".to_string(),
            ..Default::default()
        });
        assert_eq!(crypto["balance"], json!("0.00"), "no cash moved: balance 0");
        assert_eq!(crypto["direction"], json!("Out"), "quantity sign decides");
        assert!(item.get("symbol").is_none(), "an empty symbol is dropped");
        assert!(
            item.get("business_type").is_none(),
            "AP's business_type has no US source"
        );
    }

    #[test]
    fn counter_id_converts_to_symbol() {
        for (counter_id, symbol) in [
            ("ST/US/INTC", "INTC.US"),
            ("ETF/US/MULL", "MULL.US"),
            ("VA/BKKT/BTCUSD", "BTCUSD.BKKT"),
            ("OP/US/AAPL261120C345000", "AAPL261120C345000.US"),
        ] {
            assert_eq!(
                symbol_from_counter_id(counter_id).as_deref(),
                Some(symbol),
                "`{counter_id}` converts"
            );
        }
        for bad in ["", "INTC", "ST/US", "ST//INTC", "ST/US/"] {
            assert_eq!(symbol_from_counter_id(bad), None, "`{bad}` is rejected");
        }
    }

    #[test]
    fn order_prefers_a_gateway_converted_symbol() {
        let v = order(&json!({
            "id": "1",
            "symbol": "AAPL261120C345000.US",
            "counter_id": "OP/US/OTHER",
            "underlying_symbol": "AAPL.US",
            "action": 2
        }));
        assert_eq!(
            v["symbol"],
            json!("AAPL261120C345000.US"),
            "symbol used as given"
        );
        assert_eq!(
            v["underlying_symbol"],
            json!("AAPL.US"),
            "underlying used as given"
        );
        assert_eq!(v["side"], json!("Sell"), "action 2 is Sell");
    }

    #[test]
    fn order_derives_symbols_from_counter_ids_when_missing() {
        let v = order(&json!({
            "id": "1",
            "symbol": "",
            "counter_id": "OP/US/AAPL261120C345000",
            "underlying_counter_id": "ST/US/AAPL"
        }));
        assert_eq!(
            v["symbol"],
            json!("AAPL261120C345000.US"),
            "empty symbol: derived"
        );
        assert_eq!(
            v["underlying_symbol"],
            json!("AAPL.US"),
            "underlying: derived"
        );

        let v = order(&json!({"id": "1", "underlying_counter_id": ""}));
        assert!(v.get("symbol").is_none(), "no source, no symbol");
        assert!(
            v.get("underlying_symbol").is_none(),
            "no source, no underlying"
        );
    }

    #[test]
    fn converted_and_raw_order_responses_map_the_same() {
        // The same order before and after the gateway's counter_id rename.
        let raw = fixture("orders")["orders"][0].clone();
        let mut converted = raw.clone();
        let map = converted
            .as_object_mut()
            .expect("fixture order is an object");
        map.remove("counter_id");
        map.remove("underlying_counter_id");
        map.insert("symbol".to_string(), json!("EXMC.US"));
        map.insert("underlying_symbol".to_string(), json!(""));
        assert_eq!(
            order(&raw),
            order(&converted),
            "both shapes map identically"
        );
    }

    #[test]
    fn order_maps_fields_to_ap_names() {
        let v = order(&json!({
            "id": "42",
            "status": "FilledStatus",
            "name": "Example Corp",
            "price": "",
            "quantity": "3",
            "executed_qty": "3",
            "executed_price": "0",
            "submitted_at": "1790000000",
            "updated_at": "0",
            "action": 1,
            "counter_id": "ST/US/EXMP",
            "order_type": "MO",
            "time_in_force": 1,
            "tag": 2,
            "tailing_amount": "0.5",
            "real_name": "Somebody",
            "aaid": "1"
        }));
        assert_eq!(v["order_id"], json!("42"), "id becomes order_id");
        assert_eq!(v["status"], json!("FilledStatus"), "status keeps AP's form");
        assert_eq!(
            v["stock_name"],
            json!("Example Corp"),
            "name becomes stock_name"
        );
        assert_eq!(v["executed_quantity"], json!("3"), "executed_qty renamed");
        assert_eq!(v["side"], json!("Buy"), "action 1 is Buy");
        assert_eq!(
            v["symbol"],
            json!("EXMP.US"),
            "symbol derives from counter_id"
        );
        assert_eq!(
            v["submitted_at"],
            json!("2026-09-21T14:13:20Z"),
            "unix seconds become RFC3339"
        );
        assert_eq!(
            v["trailing_amount"],
            json!("0.5"),
            "tailing_ is AP's trailing_"
        );
        assert_eq!(
            v["us_time_in_force"],
            json!(1),
            "tif code kept raw, renamed"
        );
        assert_eq!(v["us_tag"], json!(2), "tag code kept raw, renamed");
        for absent in [
            "price",
            "executed_price",
            "updated_at",
            "time_in_force",
            "tag",
            "real_name",
            "aaid",
            "id",
            "name",
            "action",
            "counter_id",
        ] {
            assert!(
                v.get(absent).is_none(),
                "`{absent}` must not be emitted: {v}"
            );
        }
    }

    #[test]
    fn exchange_rate_prefers_reference_then_am() {
        let rate = |spec: &str, other: &str, average: &str| USExchangeRate {
            universal_key: "acct".to_string(),
            spec_key: spec.to_string(),
            base_currency: "USD".to_string(),
            other_currency: other.to_string(),
            bid_rate: average.to_string(),
            offer_rate: average.to_string(),
            average_rate: average.to_string(),
        };
        let v = exchange_rates(&[
            rate("am", "HKD", "0.128"),
            rate("reference", "HKD", "0.1285"),
            rate("am", "EUR", "1.12"),
            rate("other", "JPY", "0.0067"),
        ]);
        assert_eq!(
            v,
            json!({"exchanges": [
                {"base_currency": "USD", "other_currency": "HKD", "average_rate": 0.1285, "spec_key": "reference"},
                {"base_currency": "USD", "other_currency": "EUR", "average_rate": 1.12, "spec_key": "am"},
                {"base_currency": "USD", "other_currency": "JPY", "average_rate": 0.0067, "spec_key": "other"}
            ]}),
            "one row per pair: reference over am over anything else"
        );
    }

    #[test]
    fn exchange_rate_keeps_a_real_spread_only() {
        let v = exchange_rates(&[USExchangeRate {
            universal_key: "acct".to_string(),
            spec_key: "reference".to_string(),
            base_currency: "USD".to_string(),
            other_currency: "HKD".to_string(),
            bid_rate: "7.79".to_string(),
            offer_rate: "7.80".to_string(),
            average_rate: "7.8".to_string(),
        }]);
        let row = &v["exchanges"][0];
        assert_eq!(
            row["bid_rate"],
            json!(7.79),
            "a bid off the average is kept"
        );
        assert!(
            row.get("offer_rate").is_none(),
            "an offer equal to it is not"
        );
        assert!(
            row.get("universal_key").is_none(),
            "the account channel is hidden"
        );
    }
}
