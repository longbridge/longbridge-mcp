//! Tolerant deserializers for MCP client quirks.
//!
//! Some MCP clients (notably Claude Code) occasionally send array and numeric
//! parameters as JSON-encoded strings (e.g. `"[\"AAPL.US\"]"` instead of
//! `["AAPL.US"]`, or `"42"` instead of `42`). These helpers accept both forms
//! so the server stays interoperable without requiring clients to be perfect.

use serde::Deserialize;
use serde::de::{self, Deserializer};

#[derive(Deserialize)]
#[serde(
    untagged,
    expecting = "an array of strings, e.g. [\"700.HK\", \"AAPL.US\"], or that array as a JSON string"
)]
enum VecOrString {
    Vec(Vec<String>),
    Str(String),
}

#[derive(Deserialize)]
#[serde(
    untagged,
    expecting = "an array of integers, e.g. [1, 2], or that array as a JSON string"
)]
enum VecI32OrString {
    Vec(Vec<i32>),
    Str(String),
}

#[derive(Deserialize)]
#[serde(untagged, expecting = "a string or a number, e.g. \"100\" or 100")]
enum StrOrNumber {
    Str(String),
    Num(serde_json::Number),
}

impl StrOrNumber {
    fn into_string(self) -> String {
        match self {
            StrOrNumber::Str(s) => s,
            StrOrNumber::Num(n) => n.to_string(),
        }
    }
}

/// `String` that may arrive as a JSON number: quantities, prices and ids are
/// declared as strings (exact decimals), but models often send `100` or
/// `1.5`.
pub fn tolerant_string<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    StrOrNumber::deserialize(d).map(StrOrNumber::into_string)
}

/// `Option<String>` tolerant variant of [`tolerant_string`].
pub fn tolerant_option_string<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<StrOrNumber>::deserialize(d).map(|v| v.map(StrOrNumber::into_string))
}

#[derive(Deserialize)]
#[serde(
    untagged,
    expecting = "an integer or a numeric string, e.g. 42 or \"42\""
)]
enum IntOrString {
    Int(i64),
    Uint(u64),
    Str(String),
}

#[derive(Deserialize)]
#[serde(untagged, expecting = "a boolean or \"true\"/\"false\"")]
enum BoolOrString {
    Bool(bool),
    Str(String),
}

fn parse_bool<E: de::Error>(s: &str) -> Result<bool, E> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(E::custom(format!("expected boolean, got {other:?}"))),
    }
}

/// `bool` that may arrive as a boolean or as a string (`"true"` / `"false"` / `"1"` / `"0"`).
pub fn tolerant_bool<'de, D>(d: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match BoolOrString::deserialize(d)? {
        BoolOrString::Bool(b) => Ok(b),
        BoolOrString::Str(s) => parse_bool(&s),
    }
}

/// `Option<bool>` tolerant variant. Empty string → `None`.
#[allow(dead_code)]
pub fn tolerant_option_bool<'de, D>(d: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<BoolOrString>::deserialize(d)? {
        None => Ok(None),
        Some(BoolOrString::Bool(b)) => Ok(Some(b)),
        Some(BoolOrString::Str(s)) if s.is_empty() => Ok(None),
        Some(BoolOrString::Str(s)) => parse_bool(&s).map(Some),
    }
}

fn parse_json_vec<T, E>(s: &str) -> Result<Vec<T>, E>
where
    T: for<'a> serde::Deserialize<'a>,
    E: de::Error,
{
    serde_json::from_str(s).map_err(|e| E::custom(format!("expected JSON array, got {s:?}: {e}")))
}

/// `Vec<String>` that may arrive as an array or as a JSON string containing an array.
pub fn tolerant_vec_string<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match VecOrString::deserialize(d)? {
        VecOrString::Vec(v) => Ok(v),
        VecOrString::Str(s) => parse_json_vec(&s),
    }
}

/// `Option<Vec<String>>` tolerant variant. Empty string → `None`.
pub fn tolerant_option_vec_string<'de, D>(d: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<VecOrString>::deserialize(d)? {
        None => Ok(None),
        Some(VecOrString::Vec(v)) => Ok(Some(v)),
        Some(VecOrString::Str(s)) if s.is_empty() => Ok(None),
        Some(VecOrString::Str(s)) => parse_json_vec(&s).map(Some),
    }
}

/// `Option<String>` holding a comma-separated list, which may also arrive as
/// an array of strings; the array is joined with commas.
pub fn tolerant_option_csv<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Option::<VecOrString>::deserialize(d)? {
        None => None,
        // A JSON-encoded array ("[\"US\",\"HK\"]") is joined like a real one.
        Some(VecOrString::Str(s)) if s.trim_start().starts_with('[') => {
            Some(parse_json_vec::<String, _>(&s)?.join(","))
        }
        Some(VecOrString::Str(s)) => Some(s),
        Some(VecOrString::Vec(v)) => Some(v.join(",")),
    })
}

/// `Option<Vec<i32>>` tolerant variant (used by `warrant_list.issuer`).
pub fn tolerant_option_vec_i32<'de, D>(d: D) -> Result<Option<Vec<i32>>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<VecI32OrString>::deserialize(d)? {
        None => Ok(None),
        Some(VecI32OrString::Vec(v)) => Ok(Some(v)),
        Some(VecI32OrString::Str(s)) if s.is_empty() => Ok(None),
        Some(VecI32OrString::Str(s)) => parse_json_vec(&s).map(Some),
    }
}

#[derive(Deserialize)]
#[serde(
    untagged,
    expecting = "an array of integers or numeric strings, e.g. [4, \"7\"], or that array as a JSON string"
)]
enum VecIntOrString {
    Vec(Vec<IntOrString>),
    Str(String),
}

/// Parse a numeric string, naming the offending value on failure (the bare
/// `ParseIntError` text, "invalid digit found in string", says neither which
/// value nor what was expected).
fn parse_int<T, E>(s: &str) -> Result<T, E>
where
    T: std::str::FromStr,
    E: de::Error,
{
    s.trim()
        .parse::<T>()
        .map_err(|_| E::custom(format!("expected an integer, got {s:?}")))
}

fn int_of<E: de::Error>(v: IntOrString) -> Result<i64, E> {
    match v {
        IntOrString::Int(i) => Ok(i),
        IntOrString::Uint(u) => i64::try_from(u).map_err(E::custom),
        IntOrString::Str(s) => parse_int::<i64, E>(&s),
    }
}

/// `Option<Vec<i64>>` whose items may be numbers or numeric strings (ids that
/// another tool returns as strings), or the whole array a JSON string.
pub fn tolerant_option_vec_i64<'de, D>(d: D) -> Result<Option<Vec<i64>>, D::Error>
where
    D: Deserializer<'de>,
{
    let items = match Option::<VecIntOrString>::deserialize(d)? {
        None => return Ok(None),
        Some(VecIntOrString::Str(s)) if s.trim().is_empty() => return Ok(None),
        Some(VecIntOrString::Str(s)) => parse_json_vec::<IntOrString, _>(&s)?,
        Some(VecIntOrString::Vec(v)) => v,
    };
    items
        .into_iter()
        .map(int_of)
        .collect::<Result<_, _>>()
        .map(Some)
}

macro_rules! int_helper {
    ($name:ident, $option_name:ident, $t:ty) => {
        #[doc = concat!("`", stringify!($t), "` that may arrive as a number or as a string.")]
        #[allow(dead_code)]
        pub fn $name<'de, D>(d: D) -> Result<$t, D::Error>
        where
            D: Deserializer<'de>,
        {
            match IntOrString::deserialize(d)? {
                IntOrString::Int(i) => <$t>::try_from(i).map_err(de::Error::custom),
                IntOrString::Uint(u) => <$t>::try_from(u).map_err(de::Error::custom),
                IntOrString::Str(s) => parse_int::<$t, _>(&s),
            }
        }

        #[doc = concat!("`Option<", stringify!($t), ">` tolerant variant. Empty string → `None`.")]
        #[allow(dead_code)]
        pub fn $option_name<'de, D>(d: D) -> Result<Option<$t>, D::Error>
        where
            D: Deserializer<'de>,
        {
            match Option::<IntOrString>::deserialize(d)? {
                None => Ok(None),
                Some(IntOrString::Int(i)) => <$t>::try_from(i).map(Some).map_err(de::Error::custom),
                Some(IntOrString::Uint(u)) => {
                    <$t>::try_from(u).map(Some).map_err(de::Error::custom)
                }
                Some(IntOrString::Str(s)) if s.is_empty() => Ok(None),
                Some(IntOrString::Str(s)) => parse_int::<$t, _>(&s).map(Some),
            }
        }
    };
}

int_helper!(tolerant_i64, tolerant_option_i64, i64);
int_helper!(tolerant_i32, tolerant_option_i32, i32);
int_helper!(tolerant_usize, tolerant_option_usize, usize);
int_helper!(tolerant_u32, tolerant_option_u32, u32);

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Ids {
        #[serde(default, deserialize_with = "tolerant_option_vec_i64")]
        ids: Option<Vec<i64>>,
    }

    #[derive(Deserialize)]
    struct Qty {
        #[serde(deserialize_with = "tolerant_string")]
        quantity: String,
        #[serde(default, deserialize_with = "tolerant_option_string")]
        price: Option<String>,
    }

    #[test]
    fn numbers_are_accepted_for_decimal_strings() {
        let q: Qty =
            serde_json::from_str(r#"{"quantity":100,"price":1.5}"#).expect("numbers must parse");
        assert_eq!(q.quantity, "100", "an integer keeps its digits");
        assert_eq!(q.price.as_deref(), Some("1.5"), "a float keeps its digits");
        let q: Qty = serde_json::from_str(r#"{"quantity":"100"}"#).expect("strings must parse");
        assert_eq!(q.price, None, "an omitted price stays None");
        let q: Qty = serde_json::from_str(r#"{"quantity":1269569447752900608}"#)
            .expect("a large id must parse");
        assert_eq!(
            q.quantity, "1269569447752900608",
            "a 64-bit id must not lose precision"
        );
    }

    #[test]
    fn a_wrong_shape_names_the_expected_one() {
        let err = serde_json::from_str::<Syms>(r#"{"symbols":[1]}"#)
            .err()
            .expect("numbers are not symbols");
        assert!(
            err.to_string().contains("an array of strings"),
            "the error must say what was expected: {err}"
        );
    }

    #[test]
    fn vec_i64_accepts_numbers_strings_and_encoded_arrays() {
        for raw in [
            r#"{"ids":[4,7]}"#,
            r#"{"ids":["4","7"]}"#,
            r#"{"ids":"[4,\"7\"]"}"#,
        ] {
            let v: Ids = serde_json::from_str(raw).expect("must deserialize");
            assert_eq!(v.ids, Some(vec![4, 7]), "{raw} must yield [4, 7]");
        }
        let v: Ids = serde_json::from_str(r#"{"ids":""}"#).expect("empty string must deserialize");
        assert_eq!(v.ids, None, "an empty string is no filter");
        assert!(
            serde_json::from_str::<Ids>(r#"{"ids":["x"]}"#).is_err(),
            "a non-numeric id must be rejected"
        );
    }

    #[derive(Deserialize)]
    struct Syms {
        #[serde(deserialize_with = "tolerant_vec_string")]
        symbols: Vec<String>,
    }

    #[derive(Deserialize)]
    struct OptSyms {
        #[serde(default, deserialize_with = "tolerant_option_vec_string")]
        symbols: Option<Vec<String>>,
    }

    #[derive(Deserialize)]
    struct IntId {
        #[serde(deserialize_with = "tolerant_i64")]
        id: i64,
    }

    #[derive(Deserialize)]
    struct UsizeCount {
        #[serde(deserialize_with = "tolerant_usize")]
        count: usize,
    }

    #[test]
    fn vec_accepts_array() {
        let v: Syms = serde_json::from_str(r#"{"symbols":["AAPL.US","700.HK"]}"#).unwrap();
        assert_eq!(v.symbols, vec!["AAPL.US", "700.HK"]);
    }

    #[test]
    fn vec_accepts_json_string() {
        let v: Syms = serde_json::from_str(r#"{"symbols":"[\"AAPL.US\",\"700.HK\"]"}"#).unwrap();
        assert_eq!(v.symbols, vec!["AAPL.US", "700.HK"]);
    }

    #[test]
    fn opt_vec_missing() {
        let v: OptSyms = serde_json::from_str(r#"{}"#).unwrap();
        assert!(v.symbols.is_none());
    }

    #[test]
    fn opt_vec_empty_string_is_none() {
        let v: OptSyms = serde_json::from_str(r#"{"symbols":""}"#).unwrap();
        assert!(v.symbols.is_none());
    }

    #[test]
    fn opt_vec_null_is_none() {
        let v: OptSyms = serde_json::from_str(r#"{"symbols":null}"#).unwrap();
        assert!(v.symbols.is_none());
    }

    #[test]
    fn opt_vec_accepts_json_string() {
        let v: OptSyms = serde_json::from_str(r#"{"symbols":"[\"AAPL.US\"]"}"#).unwrap();
        assert_eq!(v.symbols, Some(vec!["AAPL.US".to_string()]));
    }

    #[test]
    fn int_accepts_number() {
        let v: IntId = serde_json::from_str(r#"{"id":4360498}"#).unwrap();
        assert_eq!(v.id, 4360498);
    }

    #[test]
    fn int_accepts_string() {
        let v: IntId = serde_json::from_str(r#"{"id":"4360498"}"#).unwrap();
        assert_eq!(v.id, 4360498);
    }

    #[test]
    fn usize_accepts_number_and_string() {
        let v1: UsizeCount = serde_json::from_str(r#"{"count":5}"#).unwrap();
        let v2: UsizeCount = serde_json::from_str(r#"{"count":"5"}"#).unwrap();
        assert_eq!(v1.count, 5);
        assert_eq!(v2.count, 5);
    }

    #[test]
    fn int_rejects_garbage() {
        let r: Result<IntId, _> = serde_json::from_str(r#"{"id":"not-a-number"}"#);
        assert!(r.is_err());
    }

    #[test]
    fn vec_rejects_garbage() {
        let r: Result<Syms, _> = serde_json::from_str(r#"{"symbols":"not-json"}"#);
        assert!(r.is_err());
    }

    #[derive(Deserialize)]
    struct BoolFlag {
        #[serde(deserialize_with = "tolerant_bool")]
        flag: bool,
    }

    #[test]
    fn bool_accepts_native() {
        let v: BoolFlag = serde_json::from_str(r#"{"flag":true}"#).unwrap();
        assert!(v.flag);
    }

    #[test]
    fn bool_accepts_string() {
        let t: BoolFlag = serde_json::from_str(r#"{"flag":"true"}"#).unwrap();
        let f: BoolFlag = serde_json::from_str(r#"{"flag":"false"}"#).unwrap();
        let one: BoolFlag = serde_json::from_str(r#"{"flag":"1"}"#).unwrap();
        assert!(t.flag && !f.flag && one.flag);
    }

    #[test]
    fn bool_rejects_garbage() {
        let r: Result<BoolFlag, _> = serde_json::from_str(r#"{"flag":"maybe"}"#);
        assert!(r.is_err());
    }
}
