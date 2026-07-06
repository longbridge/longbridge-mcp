//! JSON post-processors ported from longbridge-terminal's US-market
//! fundamental commands, adapted to operate on `serde_json::Value` in place.

use serde_json::{Number, Value};

/// Strip frontend-only rendering fields, and coerce known numeric-as-string
/// fields (timestamps, valuation metrics) to actual numbers, recursively.
/// Applied to `us_valuation_overview` and `us_analyst_consensus` output.
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

/// Normalizes a single US order JSON object in place: numeric `action`/
/// `time_in_force` codes become readable strings, a trailing "Status" suffix
/// is stripped from `status` (e.g. "RejectedStatus" -> "Rejected"), and
/// internal/frontend-only fields are removed. Safe to call on any object;
/// no-ops on fields that are absent.
pub fn normalize_us_order(v: &mut Value) {
    const INTERNAL_FIELDS: &[&str] = &[
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
    ];

    let Value::Object(map) = v else { return };
    for key in INTERNAL_FIELDS {
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
    map.retain(|_, val| {
        !matches!(val, Value::Null)
            && !matches!(val, Value::String(s) if s.is_empty())
            && !matches!(val, Value::Array(a) if a.is_empty())
    });
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
}
