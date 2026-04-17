//! Custom Serializer wrapper that transforms JSON output during serialization:
//! - Field names -> snake_case
//! - Fields ending with `_at` containing i64/u64 -> RFC3339 UTC string
//! - Field `counter_id` (string) -> renamed to `symbol`, value converted
//! - Field `counter_ids` (array of strings) -> renamed to `symbols`, each converted
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

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum FieldKind {
    Normal,
    Timestamp,
    CounterId,
    CounterIds,
}

pub(crate) fn classify_field(snake_name: &str) -> FieldKind {
    if snake_name.contains("counter_ids") {
        FieldKind::CounterIds
    } else if snake_name.contains("counter_id") {
        FieldKind::CounterId
    } else if snake_name.ends_with("_at") {
        FieldKind::Timestamp
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
    fn prefixed_counter_ids_field() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"underlyingCounterIds":["ST/US/AAPL","ST/HK/700"]}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"underlying_symbols\""), "got: {output}");
        assert!(output.contains("AAPL.US"), "got: {output}");
        assert!(output.contains("700.HK"), "got: {output}");
    }
}
