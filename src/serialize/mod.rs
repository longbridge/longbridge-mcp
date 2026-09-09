//! Custom Serializer wrapper that transforms JSON output during serialization:
//! - Field names -> snake_case
//! - Fields ending with `_at` containing i64/u64 -> RFC3339 UTC string
//! - Any string in `time`'s default `OffsetDateTime` shape (e.g.
//!   `2026-06-02 20:00:00.0 +00:00:00`) -> RFC3339, regardless of field name
//! - Fields `aaid` and `account_channel` -> value set to null
//!
//! Zero intermediate allocation for SDK types (`to_tool_json`).

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

/// Return `true` iff `s` is shaped like a field name — ASCII letters, digits
/// and underscores only.
///
/// Map keys are not always field names: several endpoints key a map by the
/// security itself (e.g. `{"symbols": {"AAPL.US": {...}}}`). Those keys are
/// data and must be passed through verbatim, since snake_case conversion would
/// mangle `AAPL.US` into `a_a_p_l._u_s`.
///
/// Zero-allocation: inspects bytes without allocating.
pub(crate) fn is_field_name(s: &str) -> bool {
    !s.is_empty()
        && s.as_bytes()
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'_')
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
    Nullified,
}

pub(crate) fn classify_field(snake_name: &str) -> FieldKind {
    if snake_name.ends_with("_at") {
        FieldKind::Timestamp
    } else if matches!(snake_name, "aaid" | "account_channel") {
        FieldKind::Nullified
    } else {
        FieldKind::Normal
    }
}

pub(crate) fn output_key<'a>(snake_name: &'a str, _kind: FieldKind) -> std::borrow::Cow<'a, str> {
    std::borrow::Cow::Borrowed(snake_name)
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
    fn transform_json_via_value() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"lastDone":"250.5","createdAt":1700000000}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"last_done\":\"250.5\""), "got: {output}");
        assert!(output.contains("2023-11-14T"), "got: {output}");
    }

    #[test]
    fn nested_objects() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"order":{"stockName":"Tencent","submittedAt":1700000000}}"#)
                .unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(
            output.contains("\"stock_name\":\"Tencent\""),
            "got: {output}"
        );
        assert!(output.contains("2023-11-14T"), "got: {output}");
    }

    #[test]
    fn array_of_objects() {
        let input: serde_json::Value =
            serde_json::from_str(r#"[{"lastDone":"1"},{"lastDone":"2"}]"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert_eq!(
            output, r#"[{"last_done":"1"},{"last_done":"2"}]"#,
            "got: {output}"
        );
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
    fn counter_id_is_passed_through_untouched() {
        // The backend sends `symbol` alongside every `counter_id`, so the
        // transform no longer rewrites either one -- doing so would emit the
        // key `symbol` twice.
        let input: serde_json::Value =
            serde_json::from_str(r#"{"counter_id":"ST/HK/700","symbol":"700.HK"}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert_eq!(
            output, r#"{"counter_id":"ST/HK/700","symbol":"700.HK"}"#,
            "got: {output}"
        );
    }

    #[test]
    fn symbol_map_keys_survive_snake_case() {
        let input: serde_json::Value = serde_json::from_str(
            r#"{"symbols":{"AAPL.US":{"lastPrice":1},"700.HK":{"lastPrice":2}}}"#,
        )
        .unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"AAPL.US\""), "got: {output}");
        assert!(output.contains("\"700.HK\""), "got: {output}");
        assert!(!output.contains("a_a_p_l"), "mangled symbol key: {output}");
        // Field names nested under a data key are still converted.
        assert!(output.contains("\"last_price\""), "got: {output}");
    }

    #[test]
    fn is_field_name_separates_field_names_from_data_keys() {
        assert!(is_field_name("lastPrice"));
        assert!(is_field_name("created_at"));
        assert!(is_field_name("counter_id"));
        assert!(!is_field_name("AAPL.US"));
        assert!(!is_field_name("ST/US/AAPL"));
        assert!(!is_field_name(".DJI.US"));
        assert!(!is_field_name(""));
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
