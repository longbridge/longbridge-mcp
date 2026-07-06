//! JSON post-processors ported from longbridge-terminal's US-market
//! fundamental commands, adapted to operate on `serde_json::Value` in place.

use serde_json::{Number, Value};

/// Text fields that come back from upstream as HTML-formatted prose (e.g.
/// `<strong>$143.8B</strong> vs. <strong>$138.5B</strong> expected`) — plain
/// text is more useful to an LLM caller than markup it can't render.
const HTML_TEXT_KEYS: &[&str] = &["desc", "tooltip", "description", "ai_summary"];

/// Removes `<...>` tags from `s`, keeping the text between them. Does not
/// decode HTML entities (e.g. `&amp;`) — upstream values observed so far only
/// use tags for inline emphasis, not entities.
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Strip frontend-only rendering fields, strip HTML markup from prose text
/// fields, and coerce known numeric-as-string fields (timestamps, valuation
/// metrics) to actual numbers, recursively. Applied to `us_valuation_overview`
/// and `us_analyst_consensus` output.
pub fn fix_valuation_value(v: &mut Value) {
    const DROP_KEYS: &[&str] = &[
        "aichat_data",
        "h5_data",
        "layouts",
        "stocks",
        "peers",
        "circle",
        "part",
    ];
    const FLOAT_KEYS: &[&str] = &["value", "industry_median", "median", "high", "low"];

    match v {
        Value::Object(map) => {
            for key in DROP_KEYS {
                map.remove(*key);
            }
            for val in map.values_mut() {
                fix_valuation_value(val);
            }
            if let Some(n) = map
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<i64>().ok())
            {
                map.insert("timestamp".to_string(), Value::Number(Number::from(n)));
            }
            for key in FLOAT_KEYS {
                if let Some(n) = map
                    .get(*key)
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<f64>().ok())
                    .and_then(Number::from_f64)
                {
                    map.insert((*key).to_string(), Value::Number(n));
                }
            }
            if let Some(n) = map.get("metric").and_then(Value::as_str).and_then(|s| {
                let stripped = s.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.');
                stripped.parse::<f64>().ok()
            }) && let Some(num) = Number::from_f64(n)
            {
                map.insert("metric".to_string(), Value::Number(num));
            }
            for key in HTML_TEXT_KEYS {
                if let Some(s) = map.get(*key).and_then(Value::as_str) {
                    let cleaned = strip_html(s);
                    if cleaned != s {
                        map.insert((*key).to_string(), Value::String(cleaned));
                    }
                }
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(fix_valuation_value),
        _ => {}
    }
}

/// order side int (1/2) -> readable label. Unknown codes pass through as
/// "Unknown" rather than panicking, since the upstream may add codes.
fn order_side_label(code: i64) -> &'static str {
    match code {
        1 => "Buy",
        2 => "Sell",
        _ => "Unknown",
    }
}

/// order time_in_force int -> readable label.
fn time_in_force_label(code: i64) -> &'static str {
    match code {
        1 => "Day",
        3 => "GTC",
        4 => "GTD",
        5 => "IOC",
        6 => "FOK",
        _ => "Unknown",
    }
}

/// Internal/backend-routing fields with no meaning to a tool caller, plus the
/// account holder's real name — `order`/`order_histories` on the US order
/// endpoints are untyped `serde_json::Value` passthrough (unlike the
/// strongly-typed HK/AP order struct), so whatever fields the backend
/// includes reach this function unfiltered; PII must be stripped explicitly
/// rather than relying on a fixed struct shape to exclude it.
const ORDER_INTERNAL_FIELDS: &[&str] = &[
    "button_control",
    "current_millisecond",
    "deductions_status",
    "free_status",
    "platform_deductions_status",
    "force_only_rth",
    "limit_depth_level",
    "tag",
    "trend",
    "trigger_count",
    "trigger_status",
    "trigger_at",
    "account_channel",
    // Internal bookkeeping IDs with no meaning to a caller.
    "aaid",
    "org_id",
    "ploy_type",
    // Exchange tick-size metadata, not order-specific data — observed as a
    // large nested table on every order (e.g. bid_size_list), unrelated to
    // this order's own price/quantity.
    "bid_size_list",
    "ticker_size",
    // Duplicates `symbol` (the underlying counter code without the market
    // suffix).
    "code",
    // Backend settlement/routing metadata, not user-facing order data.
    "settlement_account",
    "display_account",
    "op_entrust_way",
    "op_entrust_way_name",
    "settlement_channel",
    "short_sell_type",
    "activate_rth",
    // PII: the account holder's real name. Never surface this in tool output.
    "real_name",
    "en_name",
];

/// Internal fields on a single `order_histories[]` entry — a state-transition
/// log line, not an order — with no meaning to a caller.
const ORDER_HISTORY_ENTRY_INTERNAL_FIELDS: &[&str] = &[
    "is_manually",
    "exec_type",
    "opp_party_id",
    "trd_match_id",
    "operator",
    "op_entrust_way",
    "cxl_rej_response_to",
    "withdrawal_reason",
    "opp_name",
    "exec_id",
];

fn drop_empty(map: &mut serde_json::Map<String, Value>) {
    map.retain(|_, val| {
        !matches!(val, Value::Null)
            && !matches!(val, Value::String(s) if s.is_empty())
            && !matches!(val, Value::Array(a) if a.is_empty())
    });
}

/// Normalizes one `order_histories[]` entry in place: strips a trailing
/// "Status" suffix from `status`, renames `time` to `occurred_at` (so the
/// generic `_at`-suffix unix-timestamp conversion in `to_tool_json` picks it
/// up), and removes internal fields.
fn normalize_order_history_entry(entry: &mut Value) {
    let Value::Object(map) = entry else { return };
    for key in ORDER_HISTORY_ENTRY_INTERNAL_FIELDS {
        map.remove(*key);
    }
    if let Some(status) = map.get("status").and_then(Value::as_str)
        && let Some(stripped) = status.strip_suffix("Status")
    {
        map.insert("status".to_string(), Value::String(stripped.to_string()));
    }
    if let Some(t) = map.remove("time") {
        map.insert("occurred_at".to_string(), t);
    }
    drop_empty(map);
}

/// Normalizes a single US order JSON object in place: numeric `action`/
/// `time_in_force` codes become readable strings, a trailing "Status" suffix
/// is stripped from `status` (e.g. "RejectedStatus" -> "Rejected"), nested
/// `order_histories[]` entries are normalized the same way, and internal/PII
/// fields are removed. Safe to call on any object; no-ops on fields that are
/// absent.
pub fn normalize_us_order(v: &mut Value) {
    let Value::Object(map) = v else { return };
    for key in ORDER_INTERNAL_FIELDS {
        map.remove(*key);
    }
    if let Some(code) = map.get("action").and_then(Value::as_i64) {
        map.insert(
            "action".to_string(),
            Value::String(order_side_label(code).to_string()),
        );
    }
    if let Some(code) = map.get("time_in_force").and_then(Value::as_i64) {
        map.insert(
            "time_in_force".to_string(),
            Value::String(time_in_force_label(code).to_string()),
        );
    }
    if let Some(status) = map.get("status").and_then(Value::as_str)
        && let Some(stripped) = status.strip_suffix("Status")
    {
        map.insert("status".to_string(), Value::String(stripped.to_string()));
    }
    if let Some(Value::Array(histories)) = map.get_mut("order_histories") {
        for entry in histories.iter_mut() {
            normalize_order_history_entry(entry);
        }
    }
    drop_empty(map);
}

/// Strips a trailing `%` from the named string fields (recursively) and
/// parses the remainder as a decimal number, e.g. `"1.85%"` -> `1.85`.
/// Applied to `dividend_yield`/`dividend_yield_ttm` on the US dividend tools.
pub fn normalize_pct_fields(v: &mut Value, keys: &[&str]) {
    match v {
        Value::Object(map) => {
            for key in keys {
                if let Some(n) = map
                    .get(*key)
                    .and_then(Value::as_str)
                    .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
                    .and_then(Number::from_f64)
                {
                    map.insert((*key).to_string(), Value::Number(n));
                }
            }
            for val in map.values_mut() {
                normalize_pct_fields(val, keys);
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(|x| normalize_pct_fields(x, keys)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fix_valuation_value_drops_frontend_fields_and_coerces_numbers() {
        let mut v = json!({
            "timestamp": "1700000000",
            "value": "27.78",
            "metric": "35.3x",
            "layouts": {"foo": "bar"},
            "nested": {"h5_data": null, "median": "12.5"}
        });
        fix_valuation_value(&mut v);
        assert_eq!(v["timestamp"], json!(1_700_000_000_i64));
        assert_eq!(v["value"], json!(27.78));
        assert_eq!(v["metric"], json!(35.3));
        assert!(v.get("layouts").is_none());
        assert!(v["nested"].get("h5_data").is_none());
        assert_eq!(v["nested"]["median"], json!(12.5));
    }

    #[test]
    fn fix_valuation_value_strips_html_from_text_fields() {
        let mut v = json!({
            "ai_summary": "revenue of <strong>$143.8B</strong> vs. <strong>$138.5B</strong> expected",
            "desc": "plain text unchanged",
            "nested": {"tooltip": "<em>note</em>"}
        });
        fix_valuation_value(&mut v);
        assert_eq!(
            v["ai_summary"],
            json!("revenue of $143.8B vs. $138.5B expected")
        );
        assert_eq!(v["desc"], json!("plain text unchanged"));
        assert_eq!(v["nested"]["tooltip"], json!("note"));
    }

    #[test]
    fn normalize_us_order_maps_codes_and_strips_internal_fields() {
        let mut v = json!({
            "action": 1,
            "time_in_force": 3,
            "status": "RejectedStatus",
            "tag": "internal",
            "trigger_at": "",
            "trend": [],
            "symbol": "AAPL.US"
        });
        normalize_us_order(&mut v);
        assert_eq!(v["action"], json!("Buy"));
        assert_eq!(v["time_in_force"], json!("GTC"));
        assert_eq!(v["status"], json!("Rejected"));
        assert!(v.get("tag").is_none());
        assert!(v.get("trigger_at").is_none());
        assert!(v.get("trend").is_none());
        assert_eq!(v["symbol"], json!("AAPL.US"));
    }

    #[test]
    fn normalize_us_order_leaves_unknown_codes_as_unknown() {
        let mut v = json!({"action": 99, "time_in_force": 0});
        normalize_us_order(&mut v);
        assert_eq!(v["action"], json!("Unknown"));
        assert_eq!(v["time_in_force"], json!("Unknown"));
    }

    #[test]
    fn normalize_us_order_strips_exchange_metadata_and_bookkeeping_ids() {
        // Shape observed from a real staging today_orders response.
        let mut v = json!({
            "symbol": "AAPL.US",
            "code": "AAPL",
            "aaid": null,
            "org_id": "1",
            "ploy_type": "0",
            "ticker_size": "0.01",
            "bid_size_list": [{"str_proceed": "0", "end_proceed": "1", "bid_size": "0.0001"}]
        });
        normalize_us_order(&mut v);
        assert_eq!(v["symbol"], json!("AAPL.US"));
        assert!(v.get("code").is_none());
        assert!(v.get("aaid").is_none());
        assert!(v.get("org_id").is_none());
        assert!(v.get("ploy_type").is_none());
        assert!(v.get("ticker_size").is_none());
        assert!(v.get("bid_size_list").is_none());
    }

    #[test]
    fn normalize_us_order_strips_pii_and_settlement_metadata() {
        // Shape observed from a real staging order_detail response.
        let mut v = json!({
            "symbol": "AAPL.US",
            "real_name": "John Doe",
            "en_name": "Doe John",
            "settlement_account": "4DH07862",
            "display_account": "4DH07862",
            "op_entrust_way": 0,
            "op_entrust_way_name": "APP",
            "settlement_channel": "settlement_apex_us",
            "short_sell_type": 7,
            "activate_rth": 0
        });
        normalize_us_order(&mut v);
        assert_eq!(v["symbol"], json!("AAPL.US"));
        assert!(v.get("real_name").is_none());
        assert!(v.get("en_name").is_none());
        assert!(v.get("settlement_account").is_none());
        assert!(v.get("display_account").is_none());
        assert!(v.get("op_entrust_way").is_none());
        assert!(v.get("op_entrust_way_name").is_none());
        assert!(v.get("settlement_channel").is_none());
        assert!(v.get("short_sell_type").is_none());
        assert!(v.get("activate_rth").is_none());
    }

    #[test]
    fn normalize_us_order_normalizes_nested_order_histories() {
        let mut v = json!({
            "symbol": "NVDA260608C210000.US",
            "order_histories": [{
                "status": "RejectedStatus",
                "time": "1780925402",
                "price": "0.9000",
                "is_manually": false,
                "exec_type": 7,
                "opp_party_id": "",
                "exec_id": "536ALONGU1"
            }]
        });
        normalize_us_order(&mut v);
        let entry = &v["order_histories"][0];
        assert_eq!(entry["status"], json!("Rejected"));
        assert_eq!(entry["occurred_at"], json!("1780925402"));
        assert!(entry.get("time").is_none());
        assert!(entry.get("is_manually").is_none());
        assert!(entry.get("exec_type").is_none());
        assert!(entry.get("opp_party_id").is_none());
        assert!(entry.get("exec_id").is_none());
        assert_eq!(entry["price"], json!("0.9000"));
    }

    #[test]
    fn normalize_pct_fields_strips_percent_sign_and_parses_float() {
        let mut v = json!({
            "dividend_yield": "1.85%",
            "dividend_yield_ttm": "1.7%",
            "other": "unchanged",
            "nested": {"dividend_yield": "2%"}
        });
        normalize_pct_fields(&mut v, &["dividend_yield", "dividend_yield_ttm"]);
        assert_eq!(v["dividend_yield"], json!(1.85));
        assert_eq!(v["dividend_yield_ttm"], json!(1.7));
        assert_eq!(v["other"], json!("unchanged"));
        assert_eq!(v["nested"]["dividend_yield"], json!(2.0));
    }

    #[test]
    fn normalize_pct_fields_leaves_empty_string_untouched() {
        let mut v = json!({"dividend_yield": ""});
        normalize_pct_fields(&mut v, &["dividend_yield"]);
        assert_eq!(v["dividend_yield"], json!(""));
    }
}
