//! Custom Serializer wrapper that transforms JSON output during serialization:
//! - Field names -> snake_case
//! - Fields ending with `_at` containing i64/u64 -> RFC3339 UTC string
//! - Any string in `time`'s default `OffsetDateTime` shape (e.g.
//!   `2026-06-02 20:00:00.0 +00:00:00`) -> RFC3339, regardless of field name
//! - Field `counter_id` (string) -> renamed to `symbol`, value converted
//! - Field `counter_ids` (array of strings) -> renamed to `symbols`, each converted
//! - Fields `aaid` and `account_channel` -> value set to null
//!
//! Zero intermediate allocation for SDK types (`to_tool_json`).

mod counter_id;
mod timestamp;
pub mod transform;

use serde::ser::{Serialize, Serializer};

use crate::serialize::transform::TransformSerializer;

macro_rules! delegate_simple {
    ($method:ident, $ty:ty) => {
        fn $method(self, v: $ty) -> Result<Self::Ok, Self::Error> {
            self.inner.$method(v)
        }
    };
}
pub(crate) use delegate_simple;

/// Serialize a Rust value with field transformations, zero intermediate Value.
pub fn to_tool_json(value: &impl Serialize) -> Result<String, serde_json::Error> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::new(&mut buf);
    value.serialize(TransformSerializer { inner: &mut ser })?;
    Ok(String::from_utf8(buf).expect("serde_json produces valid UTF-8"))
}

/// Stream-transcode raw JSON bytes with field transformations.
/// No intermediate `serde_json::Value` allocation -- reads tokens from input
/// and writes transformed tokens directly to output.
pub fn transform_json(input: &[u8]) -> Result<String, serde_json::Error> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::new(&mut buf);
    let mut de = serde_json::Deserializer::from_slice(input);
    serde_transcode::transcode(&mut de, TransformSerializer { inner: &mut ser })?;
    Ok(String::from_utf8(buf).expect("serde_json produces valid UTF-8"))
}

/// Recursively drop `null`-valued entries from every object in `value`,
/// recursing through nested objects and arrays.
///
/// MCP consumers treat an absent key and an explicit `null` identically, so
/// dropping nulls is lossless for them and cuts tokens. The motivating case is
/// wide "all possible fields" SDK structs (e.g. `SecurityCalcIndex`, which
/// serializes ~40 fields where every index the caller did not request comes
/// back as `null`). Safe against `output_schema` validation because a field
/// that can be `null` is `Option`-derived and therefore not `required`.
pub(crate) fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            for v in map.values_mut() {
                strip_nulls(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_nulls(v);
            }
        }
        _ => {}
    }
}

/// Recursively drop object entries whose value is an empty string.
///
/// The sibling of [`strip_nulls`] for upstreams that signal "no value" with `""`
/// rather than `null` (e.g. the `est_value`/`cmp` columns that only apply to a
/// forecast row and are blank on an actuals row). To an AI consumer an absent
/// key and an empty-string key both mean "no data", so removing them is
/// lossless. Non-empty strings, and empty *arrays*/*objects*, are left intact.
pub(crate) fn strip_empty_strings(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !matches!(v, serde_json::Value::String(s) if s.is_empty()));
            for v in map.values_mut() {
                strip_empty_strings(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_empty_strings(v);
            }
        }
        _ => {}
    }
}

/// Recursively cap the fractional precision of decimal-valued strings to at
/// most `dp` digits.
///
/// Only touches strings that parse as a plain decimal and carry *more* than
/// `dp` fractional digits (e.g. an SDK leverage field serialized as
/// `"11.50005084745763"`). Symbols (`700.HK`), dates, integers, and values
/// already within `dp` are left byte-for-byte unchanged, so display formatting
/// like `"438.400"` is preserved. `dp` is a floor: callers pass 6 to keep at
/// least six decimal places.
pub(crate) fn round_decimals(value: &mut serde_json::Value, dp: u32) {
    match value {
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                round_decimals(v, dp);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                round_decimals(v, dp);
            }
        }
        serde_json::Value::String(s) => {
            let frac = match s.rsplit_once('.') {
                Some((_, frac)) => frac.len(),
                None => return,
            };
            if frac > dp as usize
                && let Ok(d) = s.parse::<rust_decimal::Decimal>()
            {
                *s = d.round_dp(dp).to_string();
            }
        }
        _ => {}
    }
}

/// Recursively strip non-significant trailing zeros from every plain decimal
/// string in `value` (e.g. `"459962879.0000"` → `"459962879"`, `"4.50"` →
/// `"4.5"`, `"0.0000"` → `"0"`).
///
/// This is lossless — it never rounds, only removes zeros that carry no value —
/// so unlike [`round_decimals`] it is safe to apply blindly. Only a bare decimal
/// (optional sign, digits, one `.`, digits) is touched; dates (`"2026.09.09"`),
/// versions and ids are left alone. For passthrough tools whose upstream pads
/// integer counts with a fake fractional part (e.g. share-count deltas stored as
/// `"123.0000"`).
pub(crate) fn strip_trailing_zeros(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                strip_trailing_zeros(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_trailing_zeros(v);
            }
        }
        serde_json::Value::String(s) => {
            if let Some(trimmed) = trimmed_decimal(s) {
                *s = trimmed;
            }
        }
        _ => {}
    }
}

/// If `s` is a plain decimal string with trailing zeros in its fractional part,
/// return the trimmed form; otherwise `None`. Requires exactly one `.` with
/// digits on both sides, so multi-dot strings (dates, versions) are ignored.
fn trimmed_decimal(s: &str) -> Option<String> {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int_part, frac_part) = body.split_once('.')?;
    let is_digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if !is_digits(int_part) || !is_digits(frac_part) || !frac_part.ends_with('0') {
        return None;
    }
    let trimmed_frac = frac_part.trim_end_matches('0');
    let sign = if s.starts_with('-') { "-" } else { "" };
    Some(if trimmed_frac.is_empty() {
        format!("{sign}{int_part}")
    } else {
        format!("{sign}{int_part}.{trimmed_frac}")
    })
}

/// Rewrite `[st]TYPE/MARKET/CODE#Readable[/st]` cashtag markup to just
/// `Readable` (the text after `#`, or the inner text when there is no `#`).
/// Community posts wrap every ticker mention in this markup, which is pure
/// noise to a model reading the prose.
fn strip_cashtags(input: &str) -> String {
    if !input.contains("[st]") {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("[st]") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "[st]".len()..];
        match after.find("[/st]") {
            Some(end) => {
                let inner = &after[..end];
                let readable = inner.rsplit_once('#').map_or(inner, |(_, r)| r);
                out.push_str(readable);
                rest = &after[end + "[/st]".len()..];
            }
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Apply [`strip_cashtags`] to every `field`-keyed string in `value`,
/// recursively (through nested objects and arrays).
pub(crate) fn strip_cashtags_in_field(value: &mut serde_json::Value, field: &str) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if k == field
                    && let serde_json::Value::String(s) = v
                {
                    *s = strip_cashtags(s);
                } else {
                    strip_cashtags_in_field(v, field);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_cashtags_in_field(v, field);
            }
        }
        _ => {}
    }
}

/// Recursively drop entries whose (snake_case) key is in `keys` from every
/// object in `value`, at any depth and through arrays.
///
/// For passthrough tools that echo upstream fields with no analytic value:
/// duplicates (`code` == `symbol` without suffix), derivable fields
/// (`market`), constant flags (`delay`), and display assets (`icon`). Keys
/// must be given in the post-transform snake_case form.
pub(crate) fn drop_keys(value: &mut serde_json::Value, keys: &[&str]) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|k, _| !keys.contains(&k.as_str()));
            for v in map.values_mut() {
                drop_keys(v, keys);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                drop_keys(v, keys);
            }
        }
        _ => {}
    }
}

/// Return `true` iff `s` matches the `<PREFIX>/<MARKET>/<CODE>` counter_id
/// pattern used internally by Longbridge (e.g. `ST/US/AAPL`, `ETF/HK/2800`,
/// `IX/HK/HSI`, `OP/US/AAPL270115C300000`). Used to distinguish dynamic map
/// keys that happen to carry a counter_id value from ordinary camelCase field
/// names which must still go through snake_case conversion.
///
/// Zero-allocation: does not allocate on the common (negative) path.
pub(crate) fn looks_like_counter_id(s: &str) -> bool {
    // Prefix must be 1-4 ASCII uppercase letters followed by '/'.
    let rest = match s.as_bytes().iter().position(|&b| b == b'/') {
        Some(i) if (1..=4).contains(&i) => {
            if !s.as_bytes()[..i].iter().all(|&b| b.is_ascii_uppercase()) {
                return false;
            }
            &s[i + 1..]
        }
        _ => return false,
    };
    // Market must be exactly 2 ASCII uppercase letters followed by '/'.
    if rest.len() < 3 || rest.as_bytes()[2] != b'/' {
        return false;
    }
    if !rest.as_bytes()[..2].iter().all(|&b| b.is_ascii_uppercase()) {
        return false;
    }
    let code = &rest[3..];
    // Code must be non-empty and not contain a further slash.
    !code.is_empty() && !code.as_bytes().contains(&b'/')
}

pub(crate) fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

pub(crate) fn timestamp_to_rfc3339(ts: i64) -> String {
    use time::OffsetDateTime;
    match OffsetDateTime::from_unix_timestamp(ts) {
        Ok(dt) => dt
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| ts.to_string()),
        Err(_) => ts.to_string(),
    }
}

/// Convert a string emitted by `time`'s default human-readable `OffsetDateTime`
/// serialization (e.g. `"2026-06-02 20:00:00.0 +00:00:00"`) into RFC3339.
///
/// SDK response types carry `OffsetDateTime` fields which serialize to this
/// non-RFC3339 shape; our timestamp transform only handles unix-seconds, so
/// such values would otherwise pass through unchanged. `time`'s `Serialize` and
/// `Deserialize` for `OffsetDateTime` share one format and are symmetric, so we
/// round-trip the string back through serde rather than hand-write a fragile
/// parser. Returns `None` (leaving the value untouched) for anything that is
/// not in this exact shape — including values already in RFC3339.
pub(crate) fn datetime_str_to_rfc3339(s: &str) -> Option<String> {
    // Fast reject: the format is `YYYY-MM-DD HH:MM:SS...` — digit-led, `-` at
    // index 4, space at index 10. RFC3339 uses `T` at index 10, so it is
    // rejected here and left as-is.
    let b = s.as_bytes();
    if b.len() < 19 || !b[0].is_ascii_digit() || b.get(4) != Some(&b'-') || b.get(10) != Some(&b' ')
    {
        return None;
    }
    let quoted = serde_json::to_string(s).ok()?;
    let dt: time::OffsetDateTime = serde_json::from_str(&quoted).ok()?;
    dt.format(&time::format_description::well_known::Rfc3339)
        .ok()
}

/// Parse a string as a plausible unix-seconds timestamp. Returns `None` for
/// non-numeric input, or numbers outside 2000-01-01..2100-01-01 UTC (which
/// filters out sentinel values like `"0"`, `"-62135596800"`, counts, ids).
pub(crate) fn try_parse_unix_string(s: &str) -> Option<i64> {
    const MIN: i64 = 946_684_800; // 2000-01-01T00:00:00Z
    const MAX: i64 = 4_102_444_800; // 2100-01-01T00:00:00Z
    let n: i64 = s.trim().parse().ok()?;
    (MIN..=MAX).contains(&n).then_some(n)
}

/// Walk a JSON value and convert unix-seconds strings at the given paths to
/// RFC3339 in place.
///
/// Path syntax:
/// - `a.b.c` — dot-separated field names, applied against `Object` values
/// - `*` — wildcard that matches either every array element or every map value
///   at the current level
///
/// Example: `"statistics.trade_date.*"` converts each element of the array at
/// `statistics.trade_date`; `"plans.*.next_trd_date"` converts `next_trd_date`
/// inside every element of the `plans` array.
///
/// Only strings that parse as unix seconds inside [2000-01-01, 2100-01-01] are
/// transformed; non-numeric strings and out-of-range sentinels (`"0"`,
/// `"-62135596800"`) are left untouched so the caller's "no value" semantics
/// survive.
pub fn convert_unix_paths(value: &mut serde_json::Value, paths: &[&str]) {
    for path in paths {
        let segments: Vec<&str> = path.split('.').collect();
        walk_convert(value, &segments);
    }
}

fn walk_convert(value: &mut serde_json::Value, segments: &[&str]) {
    if segments.is_empty() {
        if let serde_json::Value::String(s) = value
            && let Some(ts) = try_parse_unix_string(s)
        {
            *value = serde_json::Value::String(timestamp_to_rfc3339(ts));
        }
        return;
    }
    let (seg, rest) = (segments[0], &segments[1..]);
    match value {
        serde_json::Value::Object(map) => {
            if seg == "*" {
                for v in map.values_mut() {
                    walk_convert(v, rest);
                }
            } else if let Some(v) = map.get_mut(seg) {
                walk_convert(v, rest);
            }
        }
        serde_json::Value::Array(arr) if seg == "*" => {
            for v in arr.iter_mut() {
                walk_convert(v, rest);
            }
        }
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FieldKind {
    Normal,
    Timestamp,
    CounterId,
    CounterIds,
    Nullified,
}

pub(crate) fn classify_field(snake_name: &str) -> FieldKind {
    if snake_name.contains("counter_ids") {
        FieldKind::CounterIds
    } else if snake_name.contains("counter_id") {
        FieldKind::CounterId
    } else if snake_name.ends_with("_at") {
        FieldKind::Timestamp
    } else if matches!(snake_name, "aaid" | "account_channel") {
        FieldKind::Nullified
    } else {
        FieldKind::Normal
    }
}

pub(crate) fn output_key<'a>(snake_name: &'a str, kind: FieldKind) -> std::borrow::Cow<'a, str> {
    match kind {
        FieldKind::CounterId => {
            if snake_name == "counter_id" {
                std::borrow::Cow::Borrowed("symbol")
            } else {
                std::borrow::Cow::Owned(snake_name.replace("counter_id", "symbol"))
            }
        }
        FieldKind::CounterIds => {
            if snake_name == "counter_ids" {
                std::borrow::Cow::Borrowed("symbols")
            } else {
                std::borrow::Cow::Owned(snake_name.replace("counter_ids", "symbols"))
            }
        }
        _ => std::borrow::Cow::Borrowed(snake_name),
    }
}

pub(crate) struct Transformed<'a, T: ?Sized> {
    pub(crate) value: &'a T,
}

impl<T: Serialize + ?Sized> Serialize for Transformed<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value
            .serialize(TransformSerializer { inner: serializer })
    }
}

pub(crate) fn key_to_string<T: Serialize + ?Sized>(key: &T) -> Result<String, String> {
    let s = serde_json::to_string(key).map_err(|e| e.to_string())?;
    Ok(if s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[test]
    fn snake_case_conversion() {
        assert_eq!(to_snake_case("createdAt"), "created_at");
        assert_eq!(to_snake_case("counterIds"), "counter_ids");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn strip_nulls_drops_null_keys_recursively() {
        let mut v = serde_json::json!({
            "pe": "22.5",
            "pb": null,
            "nested": {"a": 1, "b": null},
            "rows": [{"x": 1, "y": null}, {"x": null}]
        });
        strip_nulls(&mut v);
        assert_eq!(
            v,
            serde_json::json!({
                "pe": "22.5",
                "nested": {"a": 1},
                "rows": [{"x": 1}, {}]
            })
        );
    }

    #[test]
    fn strip_nulls_keeps_non_null_and_empty() {
        // Empty string / empty array / zero are NOT null — they stay.
        let mut v = serde_json::json!({"s": "", "arr": [], "n": 0, "gone": null});
        strip_nulls(&mut v);
        assert_eq!(v, serde_json::json!({"s": "", "arr": [], "n": 0}));
    }

    #[test]
    fn strip_empty_strings_drops_only_empty_string_keys() {
        let mut v = serde_json::json!({
            "fr_revenue": {"value": "416161000000", "yoy": "6.43", "est_value": "",
                           "est_yoy": "", "cmp": "", "cmp_desc": ""},
            "kept": "x",
            "arr": [{"a": "", "b": "1"}],
            "empty_arr": [],
            "empty_obj": {},
            "zero": 0,
            "nul": null
        });
        strip_empty_strings(&mut v);
        assert_eq!(
            v,
            serde_json::json!({
                "fr_revenue": {"value": "416161000000", "yoy": "6.43"},
                "kept": "x",
                "arr": [{"b": "1"}],
                "empty_arr": [],
                "empty_obj": {},
                "zero": 0,
                "nul": null
            }),
            "only empty-string values are dropped, recursively; null, empty array/object, and 0 stay"
        );
    }

    #[test]
    fn round_decimals_caps_precision_at_six() {
        let mut v = serde_json::json!({
            "leverage": "11.50005084745763",
            "premium": "0.23161592505854794",
            "price": "438.400",
            "symbol": "700.HK",
            "date": "2026-09-10",
            "ts": "1789007822"
        });
        round_decimals(&mut v, 6);
        assert_eq!(v["leverage"], "11.500051");
        assert_eq!(v["premium"], "0.231616");
        assert_eq!(v["price"], "438.400", "already <=6 dp: untouched");
        assert_eq!(v["symbol"], "700.HK", "non-numeric: untouched");
        assert_eq!(v["date"], "2026-09-10", "date: untouched");
        assert_eq!(v["ts"], "1789007822", "integer: untouched");
    }

    #[test]
    fn strip_trailing_zeros_is_lossless_and_skips_non_decimals() {
        let mut v = serde_json::json!({
            "shares": {"value": "459962879", "chg_1": "123.0000", "chg_5": "-45.0000"},
            "ratio": "0.0505",
            "padded": "4.50",
            "zero": "0.0000",
            "date": "2026.09.09",
            "iso": "2026-09-10",
            "symbol": "700.HK",
            "int": "42"
        });
        strip_trailing_zeros(&mut v);
        assert_eq!(v["shares"]["value"], "459962879", "integer untouched");
        assert_eq!(v["shares"]["chg_1"], "123", "fake .0000 stripped");
        assert_eq!(v["shares"]["chg_5"], "-45", "negative fake .0000 stripped");
        assert_eq!(v["ratio"], "0.0505", "significant digits kept");
        assert_eq!(v["padded"], "4.5", "trailing zero stripped");
        assert_eq!(v["zero"], "0", "0.0000 becomes 0");
        assert_eq!(v["date"], "2026.09.09", "multi-dot date untouched");
        assert_eq!(v["iso"], "2026-09-10", "iso date untouched");
        assert_eq!(v["symbol"], "700.HK", "ticker untouched");
        assert_eq!(v["int"], "42", "plain integer untouched");
    }

    #[test]
    fn strip_cashtags_in_field_keeps_readable_ticker() {
        let mut v = serde_json::json!({
            "items": [
                {"description": "[st]ST/HK/700#TENCENT.HK[/st] CFO said prepaid 50bn"},
                {"description": "no markup here"},
                {"description": "[st]ST/US/BABA#Alibaba.US[/st][st]ST/HK/700#TENCENT.HK[/st] two tags"}
            ]
        });
        strip_cashtags_in_field(&mut v, "description");
        assert_eq!(
            v["items"][0]["description"],
            "TENCENT.HK CFO said prepaid 50bn"
        );
        assert_eq!(v["items"][1]["description"], "no markup here");
        assert_eq!(
            v["items"][2]["description"],
            "Alibaba.USTENCENT.HK two tags"
        );
    }

    #[test]
    fn drop_keys_removes_named_keys_recursively() {
        let mut v = serde_json::json!({
            "items": [
                {"symbol": "9988.HK", "code": "09988", "market": "HK", "chg": "0.01"},
                {"symbol": "700.HK", "code": "00700", "market": "HK", "chg": "0.02"}
            ],
            "market": "HK"
        });
        drop_keys(&mut v, &["code", "market"]);
        assert_eq!(
            v,
            serde_json::json!({
                "items": [
                    {"symbol": "9988.HK", "chg": "0.01"},
                    {"symbol": "700.HK", "chg": "0.02"}
                ]
            })
        );
    }

    #[test]
    fn timestamp_field() {
        #[derive(Serialize)]
        struct Data {
            created_at: i64,
            name: String,
        }
        let d = Data {
            created_at: 1700000000,
            name: "test".to_string(),
        };
        let json = to_tool_json(&d).unwrap();
        assert!(json.contains("2023-11-14T"), "got: {json}");
        assert!(json.contains("\"name\":\"test\""), "got: {json}");
    }

    #[test]
    fn counter_id_field() {
        #[derive(Serialize)]
        struct Data {
            counter_id: String,
        }
        let d = Data {
            counter_id: "ST/US/TSLA".to_string(),
        };
        let json = to_tool_json(&d).unwrap();
        assert!(json.contains("\"symbol\":\"TSLA.US\""), "got: {json}");
        assert!(!json.contains("counter_id"), "got: {json}");
    }

    #[test]
    fn counter_ids_field() {
        #[derive(Serialize)]
        struct Data {
            counter_ids: Vec<String>,
        }
        let d = Data {
            counter_ids: vec!["ST/US/TSLA".to_string(), "ETF/US/SPY".to_string()],
        };
        let json = to_tool_json(&d).unwrap();
        assert!(json.contains("\"symbols\""), "got: {json}");
        assert!(json.contains("TSLA.US"), "got: {json}");
        assert!(json.contains("SPY.US"), "got: {json}");
    }

    #[test]
    fn transform_json_via_value() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"counterId":"ST/US/TSLA","createdAt":1700000000}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"symbol\":\"TSLA.US\""), "got: {output}");
        assert!(output.contains("2023-11-14T"), "got: {output}");
    }

    #[test]
    fn nested_objects() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"order":{"counterId":"ST/HK/700","submittedAt":1700000000}}"#)
                .unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"symbol\":\"700.HK\""), "got: {output}");
        assert!(output.contains("2023-11-14T"), "got: {output}");
    }

    #[test]
    fn array_of_objects() {
        let input: serde_json::Value =
            serde_json::from_str(r#"[{"counterId":"ST/US/AAPL"},{"counterId":"ST/HK/700"}]"#)
                .unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("AAPL.US"), "got: {output}");
        assert!(output.contains("700.HK"), "got: {output}");
    }

    #[test]
    fn camel_case_keys() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"lastPrice":100.5,"tradeVolume":1000}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"last_price\""), "got: {output}");
        assert!(output.contains("\"trade_volume\""), "got: {output}");
    }

    #[test]
    fn prefixed_counter_id_field() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"underlyingCounterId":"ST/US/AAPL"}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"underlying_symbol\""), "got: {output}");
        assert!(output.contains("\"AAPL.US\""), "got: {output}");
        assert!(!output.contains("counter_id"), "got: {output}");
    }

    #[test]
    fn counter_id_as_map_key() {
        let input: serde_json::Value = serde_json::from_str(
            r#"{"stocks":{"ST/US/AAPL":{"name":"苹果"},"ST/US/SNDK":{"name":"闪迪"},"IX/HK/HSI":{"name":"恒生指数"}}}"#,
        )
        .unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"AAPL.US\""), "got: {output}");
        assert!(output.contains("\"SNDK.US\""), "got: {output}");
        assert!(output.contains("\"HSI.HK\""), "got: {output}");
        assert!(!output.contains("s_t/"), "leaked snake-cased key: {output}");
        assert!(
            !output.contains("i_x/"),
            "leaked snake-cased IX key: {output}"
        );
    }

    #[test]
    fn looks_like_counter_id_positive() {
        assert!(looks_like_counter_id("ST/US/AAPL"));
        assert!(looks_like_counter_id("ETF/US/SPY"));
        assert!(looks_like_counter_id("IX/HK/HSI"));
        assert!(looks_like_counter_id("OP/US/AAPL270115C300000"));
        assert!(looks_like_counter_id("ST/HK/00700"));
    }

    #[test]
    fn looks_like_counter_id_negative() {
        assert!(!looks_like_counter_id("AAPL.US"));
        assert!(!looks_like_counter_id("lastPrice"));
        assert!(!looks_like_counter_id("created_at"));
        assert!(!looks_like_counter_id("ST/US")); // incomplete
        assert!(!looks_like_counter_id("")); // empty
        assert!(!looks_like_counter_id("st/us/aapl")); // lowercase prefix/market
        assert!(!looks_like_counter_id("ST/USA/AAPL")); // 3-letter market
        assert!(!looks_like_counter_id("ST/US/")); // empty code
    }

    #[test]
    fn prefixed_counter_ids_field() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"underlyingCounterIds":["ST/US/AAPL","ST/HK/700"]}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"underlying_symbols\""), "got: {output}");
        assert!(output.contains("AAPL.US"), "got: {output}");
        assert!(output.contains("700.HK"), "got: {output}");
    }

    #[test]
    fn string_unix_on_at_field() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"created_at":"1700000000"}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(
            output.contains("\"created_at\":\"2023-11-14T"),
            "got: {output}"
        );
    }

    #[test]
    fn bare_timestamp_field_no_longer_whitelisted() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"timestamp":"1776756761"}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        // Without path-level opt-in, `timestamp` (not ending in `_at`) is left as-is.
        assert!(
            output.contains("\"timestamp\":\"1776756761\""),
            "got: {output}"
        );
    }

    #[test]
    fn out_of_range_at_string_kept_as_is() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"created_at":"0","edited_at":"-62135596800"}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"created_at\":\"0\""), "got: {output}");
        assert!(
            output.contains("\"edited_at\":\"-62135596800\""),
            "got: {output}"
        );
    }

    #[test]
    fn unrelated_fields_with_numeric_strings_not_converted() {
        let input: serde_json::Value = serde_json::from_str(
            r#"{"volume":"1700000000","total":"1776652800","count":"1000000000"}"#,
        )
        .unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(
            output.contains("\"volume\":\"1700000000\""),
            "got: {output}"
        );
        assert!(output.contains("\"total\":\"1776652800\""), "got: {output}");
        assert!(output.contains("\"count\":\"1000000000\""), "got: {output}");
    }

    #[test]
    fn try_parse_unix_string_bounds() {
        assert_eq!(try_parse_unix_string("1700000000"), Some(1_700_000_000));
        assert_eq!(try_parse_unix_string(" 1700000000 "), Some(1_700_000_000));
        assert_eq!(try_parse_unix_string("0"), None);
        assert_eq!(try_parse_unix_string("-62135596800"), None);
        assert_eq!(try_parse_unix_string("946684799"), None); // below MIN
        assert_eq!(try_parse_unix_string("4102444801"), None); // above MAX
        assert_eq!(try_parse_unix_string("2026.04.20"), None);
        assert_eq!(try_parse_unix_string(""), None);
    }

    #[test]
    fn convert_unix_paths_simple_field() {
        let mut v: serde_json::Value =
            serde_json::from_str(r#"{"timestamp":"1700000000","other":"1700000000"}"#).unwrap();
        convert_unix_paths(&mut v, &["timestamp"]);
        assert_eq!(v["timestamp"], "2023-11-14T22:13:20Z");
        // `other` is not in paths — untouched.
        assert_eq!(v["other"], "1700000000");
    }

    #[test]
    fn convert_unix_paths_nested() {
        let mut v: serde_json::Value =
            serde_json::from_str(r#"{"statistics":{"timestamp":"1700000000","preclose":"522.5"}}"#)
                .unwrap();
        convert_unix_paths(&mut v, &["statistics.timestamp"]);
        assert_eq!(v["statistics"]["timestamp"], "2023-11-14T22:13:20Z");
        assert_eq!(v["statistics"]["preclose"], "522.5");
    }

    #[test]
    fn convert_unix_paths_array_wildcard() {
        let mut v: serde_json::Value =
            serde_json::from_str(r#"{"statistics":{"trade_date":["1776643200","1776729600"]}}"#)
                .unwrap();
        convert_unix_paths(&mut v, &["statistics.trade_date.*"]);
        assert_eq!(v["statistics"]["trade_date"][0], "2026-04-20T00:00:00Z");
        assert_eq!(v["statistics"]["trade_date"][1], "2026-04-21T00:00:00Z");
    }

    #[test]
    fn convert_unix_paths_field_inside_array_elements() {
        let mut v: serde_json::Value = serde_json::from_str(
            r#"{"plans":[{"id":1,"next_trd_date":"1778853600"},{"id":2,"next_trd_date":"1781445600"}]}"#,
        )
        .unwrap();
        convert_unix_paths(&mut v, &["plans.*.next_trd_date"]);
        assert_eq!(v["plans"][0]["next_trd_date"], "2026-05-15T14:00:00Z");
        assert_eq!(v["plans"][1]["next_trd_date"], "2026-06-14T14:00:00Z");
        assert_eq!(v["plans"][0]["id"], 1);
    }

    #[test]
    fn convert_unix_paths_preserves_sentinels() {
        let mut v: serde_json::Value =
            serde_json::from_str(r#"{"end_date":"0","edited_at":"-62135596800"}"#).unwrap();
        convert_unix_paths(&mut v, &["end_date", "edited_at"]);
        assert_eq!(v["end_date"], "0");
        assert_eq!(v["edited_at"], "-62135596800");
    }

    #[test]
    fn convert_unix_paths_skips_non_numeric_strings() {
        let mut v: serde_json::Value =
            serde_json::from_str(r#"{"start_date":"2026.04.20","other":"notanumber"}"#).unwrap();
        convert_unix_paths(&mut v, &["start_date", "other"]);
        assert_eq!(v["start_date"], "2026.04.20");
        assert_eq!(v["other"], "notanumber");
    }

    #[test]
    fn convert_unix_paths_missing_path_is_noop() {
        let mut v: serde_json::Value = serde_json::from_str(r#"{"a":1}"#).unwrap();
        let before = v.clone();
        convert_unix_paths(&mut v, &["missing", "a.b.c"]);
        assert_eq!(v, before);
    }

    #[test]
    fn nullified_fields_to_tool_json() {
        #[derive(Serialize)]
        struct Data {
            aaid: String,
            account_channel: String,
            name: String,
        }
        let json = to_tool_json(&Data {
            aaid: "20975338".to_string(),
            account_channel: "lb_papertrading".to_string(),
            name: "keep".to_string(),
        })
        .unwrap();
        assert!(json.contains("\"aaid\":null"), "got: {json}");
        assert!(json.contains("\"account_channel\":null"), "got: {json}");
        assert!(json.contains("\"name\":\"keep\""), "got: {json}");
    }

    #[test]
    fn nullified_fields_transform_json() {
        let raw = r#"{"planId":"1","aaid":"999","accountChannel":"lb","market":"US"}"#;
        let output = transform_json(raw.as_bytes()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(v["aaid"], serde_json::Value::Null);
        assert_eq!(v["account_channel"], serde_json::Value::Null);
        assert_eq!(v["plan_id"], "1");
    }

    #[test]
    fn datetime_str_to_rfc3339_conversions() {
        // time's default human-readable OffsetDateTime format -> RFC3339.
        assert_eq!(
            datetime_str_to_rfc3339("2026-06-02 20:00:00.0 +00:00:00").as_deref(),
            Some("2026-06-02T20:00:00Z")
        );
        // Non-UTC offset is preserved.
        assert_eq!(
            datetime_str_to_rfc3339("2026-06-02 04:00:00.0 +08:00:00").as_deref(),
            Some("2026-06-02T04:00:00+08:00")
        );
        // Already RFC3339 ('T' at index 10) -> left untouched.
        assert_eq!(datetime_str_to_rfc3339("2026-06-02T20:00:00Z"), None);
        // Plain strings / dates / unix seconds -> untouched.
        assert_eq!(datetime_str_to_rfc3339("hello world"), None);
        assert_eq!(datetime_str_to_rfc3339("2026-06-02"), None);
        assert_eq!(datetime_str_to_rfc3339("1700000000"), None);
    }

    #[test]
    fn offset_datetime_fields_serialize_as_rfc3339() {
        use time::OffsetDateTime;
        #[derive(Serialize)]
        struct Data {
            // Bare `timestamp` (Normal path) and `created_at` (_at Timestamp path)
            // both carry SDK-style OffsetDateTime values.
            timestamp: OffsetDateTime,
            created_at: OffsetDateTime,
        }
        let dt = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let json = to_tool_json(&Data {
            timestamp: dt,
            created_at: dt,
        })
        .unwrap();
        assert!(
            json.contains("\"timestamp\":\"2023-11-14T22:13:20Z\""),
            "got: {json}"
        );
        assert!(
            json.contains("\"created_at\":\"2023-11-14T22:13:20Z\""),
            "got: {json}"
        );
        // time's default separator/offset shape must not leak through.
        assert!(!json.contains("+00:00:00"), "got: {json}");
    }
}
