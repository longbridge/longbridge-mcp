//! Compact jaq runtime error messages.
//!
//! jaq renders the offending values into its messages, e.g. `cannot index
//! <whole input> with "foo"`. Echoing a large input back costs more than the
//! success path and returns data the filter never selected, so large values
//! are replaced by a shape summary such as `array[391] of {inflow, timestamp}`.

use serde_json::Value;

/// Upper bound on a described message, in characters.
pub(super) const MAX_CHARS: usize = 300;
/// A rendered value at most this long is kept verbatim.
const MAX_INLINE_CHARS: usize = 40;
/// Most keys listed in an object or array-of-objects summary.
const MAX_KEYS: usize = 8;
const ELLIPSIS: &str = " … ";

/// Message templates of jaq-core 3 / jaq-json 2 that start with a rendered
/// value (`Error::index`, `Error::typ`, `Error::math`, `Error::path_expr`).
const PREFIXES: [&str; 4] = [
    "cannot index ",
    "cannot use ",
    "cannot calculate ",
    "invalid path expression with input ",
];
/// Separators after which those templates render a second value.
const SEPARATORS: [&str; 6] = [" with ", " + ", " - ", " * ", " / ", " % "];

/// Rewrite a jaq runtime error message into a bounded, shape-only form.
///
/// Recognized templates get each large value replaced by its shape; anything
/// else (a custom `error(...)`, an unexpected format) keeps its head and tail,
/// where jaq puts the prefix and the key or type being asked for.
pub(super) fn describe(message: &str) -> String {
    let described = summarize_values(message).unwrap_or_else(|| message.to_string());
    clip_middle(&described, MAX_CHARS)
}

fn summarize_values(message: &str) -> Option<String> {
    let prefix = PREFIXES.iter().find(|p| message.starts_with(**p))?;
    let (first, rest) = take_value(&message[prefix.len()..])?;
    let mut out = format!("{prefix}{first}");
    match SEPARATORS.iter().find(|s| rest.starts_with(**s)) {
        Some(separator) => {
            let (second, tail) = take_value(&rest[separator.len()..])?;
            out.push_str(separator);
            out.push_str(&second);
            out.push_str(tail);
        }
        None => out.push_str(rest),
    }
    Some(out)
}

/// Parse the JSON value at the start of `s` (jaq renders values as compact
/// JSON) and return its description plus the unparsed remainder.
fn take_value(s: &str) -> Option<(String, &str)> {
    let mut stream = serde_json::Deserializer::from_str(s).into_iter::<Value>();
    let value = stream.next()?.ok()?;
    let end = stream.byte_offset();
    Some((shape(&value, &s[..end]), &s[end..]))
}

fn shape(value: &Value, rendered: &str) -> String {
    if rendered.chars().count() <= MAX_INLINE_CHARS {
        return rendered.to_string();
    }
    match value {
        Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
            let keys = items
                .iter()
                .filter_map(Value::as_object)
                .flat_map(|object| object.keys());
            format!("array[{}] of {}", items.len(), key_list(keys))
        }
        Value::Array(items) => format!("array[{}]", items.len()),
        Value::Object(object) => format!("object {}", key_list(object.keys())),
        Value::String(text) => format!("string ({} chars)", text.chars().count()),
        Value::Number(_) | Value::Bool(_) | Value::Null => {
            let head: String = rendered.chars().take(MAX_INLINE_CHARS).collect();
            format!("{head}…")
        }
    }
}

/// Distinct keys in first-seen order, capped at [`MAX_KEYS`].
fn key_list<'a, I>(keys: I) -> String
where
    I: Iterator<Item = &'a String>,
{
    let mut distinct: Vec<&str> = Vec::new();
    let mut more = false;
    for key in keys {
        if distinct.contains(&key.as_str()) {
            continue;
        }
        if distinct.len() == MAX_KEYS {
            more = true;
            break;
        }
        distinct.push(key);
    }
    if more {
        distinct.push("…");
    }
    format!("{{{}}}", distinct.join(", "))
}

/// Keep the head and tail of `s` within `max` characters.
fn clip_middle(s: &str, max: usize) -> String {
    let len = s.chars().count();
    if len <= max {
        return s.to_string();
    }
    let budget = max - ELLIPSIS.chars().count();
    let head_len = budget * 2 / 3;
    let tail_len = budget - head_len;
    let head: String = s.chars().take(head_len).collect();
    let tail: String = s.chars().skip(len - tail_len).collect();
    format!("{head}{ELLIPSIS}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn series(n: usize) -> Value {
        Value::Array(
            (0..n)
                .map(|i| json!({"inflow": format!("{i}.123"), "timestamp": "2026-09-22T13:30:00Z"}))
                .collect(),
        )
    }

    #[test]
    fn index_error_summarizes_a_large_array_of_objects() {
        let message = format!("cannot index {} with \"foo\"", series(391));
        assert_eq!(
            describe(&message),
            r#"cannot index array[391] of {inflow, timestamp} with "foo""#
        );
    }

    #[test]
    fn index_error_summarizes_a_large_object_and_keeps_a_short_index() {
        let input = json!({"steps": series(3), "meta": "x".repeat(100)});
        let message = format!("cannot index {input} with 0");
        assert_eq!(
            describe(&message),
            "cannot index object {steps, meta} with 0"
        );
    }

    #[test]
    fn type_error_summarizes_a_long_string() {
        let message = format!("cannot use {} as iterable", json!("x".repeat(50_000)));
        assert_eq!(
            describe(&message),
            "cannot use string (50000 chars) as iterable"
        );
    }

    #[test]
    fn math_error_summarizes_both_operands() {
        let message = format!(
            "cannot calculate {} - {}",
            series(10),
            json!("y".repeat(80))
        );
        assert_eq!(
            describe(&message),
            "cannot calculate array[10] of {inflow, timestamp} - string (80 chars)"
        );
    }

    #[test]
    fn path_error_and_mixed_arrays_are_summarized() {
        let input = json!([1, "two", {"three": 3}, "x".repeat(60)]);
        let message = format!("invalid path expression with input {input}");
        assert_eq!(
            describe(&message),
            "invalid path expression with input array[4]"
        );
    }

    #[test]
    fn short_values_are_kept_verbatim() {
        for message in [
            r#"cannot index 1 with "foo""#,
            r#"cannot use "abc" as object"#,
            "cannot calculate null + [1,2]",
        ] {
            assert_eq!(describe(message), message, "{message}");
        }
    }

    #[test]
    fn key_lists_are_capped() {
        let wide: serde_json::Map<String, Value> =
            (0..20).map(|i| (format!("k{i}"), json!(i))).collect();
        let message = format!("cannot index {} with \"x\"", Value::Object(wide));
        assert_eq!(
            describe(&message),
            r#"cannot index object {k0, k1, k2, k3, k4, k5, k6, k7, …} with "x""#
        );
    }

    #[test]
    fn unrecognized_messages_keep_head_and_tail_within_the_cap() {
        let message = format!("{}MIDDLE{}the end", "a".repeat(400), "b".repeat(400));
        let out = describe(&message);
        assert!(
            out.chars().count() <= MAX_CHARS,
            "{} chars",
            out.chars().count()
        );
        assert!(out.starts_with("aaa"), "head kept: {out}");
        assert!(out.ends_with("the end"), "tail kept: {out}");
        assert!(!out.contains("MIDDLE"), "middle dropped: {out}");
        assert_eq!(describe("index 3 out of bounds"), "index 3 out of bounds");
    }

    #[test]
    fn malformed_values_fall_back_to_head_and_tail() {
        let message = format!("cannot index [{} with \"foo\"", "1,".repeat(500));
        let out = describe(&message);
        assert!(
            out.chars().count() <= MAX_CHARS,
            "{} chars",
            out.chars().count()
        );
        assert!(out.ends_with(r#"with "foo""#), "tail kept: {out}");
    }
}
