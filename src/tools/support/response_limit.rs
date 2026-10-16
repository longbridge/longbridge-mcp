//! Keeps tool results under the size MCP clients accept.
//!
//! Claude clients cap a single tool result (Claude Code: 25k tokens by
//! default) and drop or truncate anything larger on their side, where the
//! server cannot see it: the call looks like a success here and a failure to
//! the user. Some tools return far more with default arguments
//! (`finance_calendar` ~300k chars, `history_candlesticks_by_date` ~170k), so
//! oversized results are trimmed here instead, keeping valid JSON and telling
//! the model what was cut and how to ask for less.

use rmcp::model::{CallToolResult, Content};
use serde_json::Value;

/// Largest result text, in UTF-8 bytes, returned as-is. Sized for the
/// worst case, not prose: numeric JSON (candlesticks, quotes) tokenizes at
/// only ~2 bytes per token — Claude Code rejected a 63.7K-char candlestick
/// result against its 25K-token limit — so 36K bytes keeps it near 17K
/// tokens. Bytes rather than chars on purpose: CJK text is ~3 bytes per char
/// and also costs more tokens per char, so a byte budget tracks tokens better.
pub const MAX_RESPONSE_BYTES: usize = 36_000;

/// One array that was cut to fit.
struct Cut {
    path: String,
    kept: usize,
    total: usize,
    kept_latest: bool,
}

/// Trim `result` to [`MAX_RESPONSE_BYTES`] when it is a successful JSON result
/// over the limit. Returns the result and whether it was trimmed.
pub fn limit(mut result: CallToolResult) -> (CallToolResult, bool) {
    if result.is_error == Some(true) || result.content.len() != 1 {
        return (result, false);
    }
    // Every result passes through here; only copy the text when it is over.
    let over = result.content[0]
        .as_text()
        .is_some_and(|t| t.text.len() > MAX_RESPONSE_BYTES);
    if !over {
        return (result, false);
    }
    let Some(text) = result.content[0].as_text().map(|t| t.text.clone()) else {
        return (result, false);
    };
    let Ok(mut value) = serde_json::from_str::<Value>(&text) else {
        let mut end = MAX_RESPONSE_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let clipped = text[..end].to_string();
        // The note goes first: some clients surface only the first block, and
        // a model must not read a partial result as complete.
        result.content = vec![
            Content::text(format!(
                "[TRUNCATED — partial result: the result was {} bytes, over the \
                 {MAX_RESPONSE_BYTES}-byte limit; narrow the request]",
                text.len()
            )),
            Content::text(clipped),
        ];
        return (result, true);
    };

    let mut cuts: Vec<Cut> = Vec::new();
    let mut size = text.len();
    while size > MAX_RESPONSE_BYTES {
        let Some(path) = largest_array(&value, String::new()) else {
            break;
        };
        let array = pointer_mut(&mut value, &path).and_then(Value::as_array_mut);
        let Some(array) = array else { break };
        let total = array.len();
        // Shrink proportionally to the overshoot, with a margin for the rest of
        // the document; always make progress.
        let ratio = MAX_RESPONSE_BYTES as f64 / size as f64 * 0.9;
        let keep = ((total as f64 * ratio) as usize).clamp(1, total - 1);
        let latest_last = keep_tail_by_time(array);
        if latest_last {
            array.drain(..total - keep);
        } else {
            array.truncate(keep);
        }
        let display = if path.is_empty() {
            "(top-level array)".to_string()
        } else {
            path.clone()
        };
        match cuts.iter_mut().find(|c| c.path == display) {
            Some(cut) => cut.kept = keep,
            None => cuts.push(Cut {
                path: display,
                kept: keep,
                total,
                kept_latest: latest_last,
            }),
        }
        size = value.to_string().len();
    }

    let all: Vec<String> = cuts
        .iter()
        .map(|c| {
            let which = if c.kept_latest { "latest" } else { "first" };
            format!(
                "{} kept the {which} {} of {} items",
                c.path, c.kept, c.total
            )
        })
        .collect();
    // Many equal series (one per indicator) would make a long notice; name a
    // few and count the rest.
    const NAMED_CUTS: usize = 3;
    let mut summary: Vec<String> = all.iter().take(NAMED_CUTS).cloned().collect();
    if all.len() > NAMED_CUTS + 2 {
        summary.push(format!(
            "and {} more arrays cut the same way",
            all.len() - NAMED_CUTS
        ));
    } else {
        summary = all;
    }
    let note = format!(
        "[TRUNCATED — partial result, do not treat it as complete: {}. The full result was {} \
         bytes, over the client's tool-result size limit. Narrow the request (shorter date \
         range, a market filter, a smaller count) or select fields with _jq to get the rest.]",
        summary.join("; "),
        text.len()
    );
    // Object results also carry the marker inside the JSON, so it survives a
    // client that drops the note block or a model that only reads the data.
    if let Value::Object(map) = &mut value {
        map.insert(
            "_truncated".to_string(),
            serde_json::json!({ "cuts": summary, "full_bytes": text.len() }),
        );
    }
    // The note goes first: some clients surface only the first block.
    result.content = vec![Content::text(note), Content::text(value.to_string())];
    result.structured_content = None;
    (result, true)
}

/// JSON-pointer-like path (`/list/0/infos`) of the array with the largest
/// serialized size and more than one element, or `None`.
fn largest_array(value: &Value, path: String) -> Option<String> {
    // Longest first (element count), then largest: cutting the longest arrays
    // shortens series (history, events) and keeps the breadth of the record.
    // Cutting the largest outer array instead dropped whole sections, e.g.
    // financial_report kept 2 of 8 balance-sheet indicators with 42 quarters
    // each, rather than every indicator's recent quarters.
    let mut best: Option<((usize, usize), String)> = None;
    let mut consider = |candidate: Option<String>, value: &Value| {
        if let Some(candidate) = candidate {
            let node = pointer(value, &candidate);
            let rank = (
                node.and_then(Value::as_array).map_or(0, Vec::len),
                node.map_or(0, |v| v.to_string().len()),
            );
            if best.as_ref().is_none_or(|(best_rank, _)| rank > *best_rank) {
                best = Some((rank, candidate));
            }
        }
    };
    let root = value;
    let mut stack = vec![(path, value)];
    while let Some((path, node)) = stack.pop() {
        match node {
            Value::Array(items) => {
                if items.len() > 1 {
                    consider(Some(path.clone()), root);
                }
                for (i, item) in items.iter().enumerate() {
                    stack.push((format!("{path}/{i}"), item));
                }
            }
            Value::Object(map) => {
                for (key, child) in map {
                    stack.push((format!("{path}/{}", escape(key)), child));
                }
            }
            _ => {}
        }
    }
    best.map(|(_, path)| path)
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn pointer<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        Some(value)
    } else {
        value.pointer(path)
    }
}

fn pointer_mut<'a>(value: &'a mut Value, path: &str) -> Option<&'a mut Value> {
    if path.is_empty() {
        Some(value)
    } else {
        value.pointer_mut(path)
    }
}

/// True when the array is a past time series in ascending order (oldest
/// first, newest last), so the newest entries are the ones to keep. A series
/// that runs into the future — a calendar of upcoming events — keeps its head
/// instead: the nearest dates matter most there.
fn keep_tail_by_time(items: &[Value]) -> bool {
    let key_of = |item: &Value| -> Option<String> {
        let map = item.as_object()?;
        ["timestamp", "date", "time", "datetime", "trade_date"]
            .iter()
            .find_map(|k| map.get(*k))
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
    };
    let (Some(first), Some(last)) = (
        items.first().and_then(key_of),
        items.last().and_then(key_of),
    ) else {
        return false;
    };
    let (Some(first), Some(last)) = (to_unix(&first), to_unix(&last)) else {
        return false;
    };
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    first < last && last <= now
}

/// Seconds since the epoch for the time formats tool results use: unix
/// seconds (number or numeric string), `YYYY-MM-DD…` (RFC 3339 or date) and
/// `YYYYMMDD`. Dates count as their UTC midnight.
fn to_unix(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Ok(secs) = text.parse::<f64>() {
        // `YYYYMMDD` also parses as a number; it is far below plausible
        // timestamps (1e9+) only when it is not one.
        if text.len() == 8 && !text.contains('.') {
            return ymd(&text[..4], &text[4..6], &text[6..8]);
        }
        return Some(secs as i64);
    }
    let date = text.get(..10)?;
    let mut parts = date.split(['-', '/', '.']);
    let midnight = ymd(parts.next()?, parts.next()?, parts.next()?)?;
    // Keep the time of day (`…THH:MM:SS`, assumed UTC as tool results emit
    // `Z`): intraday bars of one day would otherwise all compare equal.
    let time_of_day = text
        .get(10..19)
        .filter(|t| t.starts_with(['T', ' ']))
        .and_then(|t| {
            let h: i64 = t.get(1..3)?.parse().ok()?;
            let m: i64 = t.get(4..6)?.parse().ok()?;
            let s: i64 = t.get(7..9)?.parse().ok()?;
            Some(h * 3600 + m * 60 + s)
        })
        .unwrap_or(0);
    Some(midnight + time_of_day)
}

fn ymd(year: &str, month: &str, day: &str) -> Option<i64> {
    let month = time::Month::try_from(month.parse::<u8>().ok()?).ok()?;
    let date = time::Date::from_calendar_date(year.parse().ok()?, month, day.parse().ok()?).ok()?;
    Some(date.midnight().assume_utc().unix_timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text_of(result: &CallToolResult, i: usize) -> String {
        result.content[i].as_text().unwrap().text.clone()
    }

    #[test]
    fn small_results_pass_through() {
        let result = CallToolResult::success(vec![Content::text(r#"{"a":1}"#)]);
        let (out, trimmed) = limit(result.clone());
        assert!(!trimmed, "a small result must not be trimmed");
        assert_eq!(out, result);
    }

    #[test]
    fn ascending_time_series_keep_the_newest_entries() {
        // A past series: 2000 daily bars ending today.
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let first = now - 1999 * 86_400;
        let candles: Vec<Value> = (0..2000)
            .map(|i| json!({"timestamp": format!("{}", first + i * 86_400), "close": "123.456789", "volume": "123456789"}))
            .collect();
        let result = CallToolResult::success(vec![Content::text(json!(candles).to_string())]);
        let (out, trimmed) = limit(result);
        assert!(trimmed, "an oversized result must be trimmed");
        let body = text_of(&out, 1);
        assert!(body.len() <= MAX_RESPONSE_BYTES, "{}", body.len());
        let kept: Vec<Value> = serde_json::from_str(&body).expect("still valid JSON");
        assert_eq!(
            kept.last().unwrap()["timestamp"],
            (first + 1999 * 86_400).to_string()
        );
        let note = text_of(&out, 0);
        assert!(
            note.contains("kept the latest") && note.contains("of 2000"),
            "{note}"
        );
    }

    #[test]
    fn intraday_series_of_one_day_keep_the_latest_bars() {
        // Minute bars, all on one (past) day: dates alone would compare equal.
        let start = time::OffsetDateTime::now_utc()
            .date()
            .saturating_sub(time::Duration::days(1));
        let bars: Vec<Value> = (0..960)
            .map(|i| {
                let t = start.midnight().assume_utc() + time::Duration::minutes(i);
                let ts = t
                    .format(&time::format_description::well_known::Rfc3339)
                    .expect("RFC 3339 formats");
                json!({"timestamp": ts, "close": "123.456789", "volume": "123456789", "turnover": "98765432.1"})
            })
            .collect();
        let result = CallToolResult::success(vec![Content::text(json!(bars).to_string())]);
        let (out, trimmed) = limit(result);
        assert!(trimmed, "960 minute bars exceed the limit");
        let kept: Vec<Value> = serde_json::from_str(&text_of(&out, 1)).expect("still valid JSON");
        let last = start.midnight().assume_utc() + time::Duration::minutes(959);
        let last = last
            .format(&time::format_description::well_known::Rfc3339)
            .expect("RFC 3339 formats");
        assert_eq!(
            kept.last().unwrap()["timestamp"],
            last,
            "the newest bar must be kept"
        );
    }

    #[test]
    fn long_series_are_shortened_before_whole_sections_are_dropped() {
        // financial_report-like: 20 indicators, each with 42 newest-first periods.
        let indicators: Vec<Value> = (0..20)
            .map(|i| {
                let values: Vec<Value> = (0..42)
                    .map(|q| json!({"period": format!("Q{} {}", q % 4 + 1, 2026 - q / 4), "value": "2494184186539.2273", "yoy": "12.345678"}))
                    .collect();
                json!({"title": format!("indicator {i}"), "accounts": [{"name": "x", "values": values}]})
            })
            .collect();
        let doc = json!({"list": {"b_s": {"indicators": indicators}}});
        let result = CallToolResult::success(vec![Content::text(doc.to_string())]);
        let (out, trimmed) = limit(result);
        assert!(trimmed, "the report exceeds the limit");
        let body: Value = serde_json::from_str(&text_of(&out, 1)).expect("still valid JSON");
        let kept = body["list"]["b_s"]["indicators"]
            .as_array()
            .expect("indicators");
        assert_eq!(kept.len(), 20, "every indicator must survive");
        let values = kept[0]["accounts"][0]["values"].as_array().expect("values");
        assert!(values.len() < 42, "the history is what gets shortened");
        assert_eq!(values[0]["period"], "Q1 2026", "the newest period is kept");
        let note = text_of(&out, 0);
        assert!(note.contains("more arrays cut the same way"), "{note}");
    }

    #[test]
    fn upcoming_calendars_keep_the_nearest_dates() {
        let today = time::OffsetDateTime::now_utc().date();
        let days: Vec<Value> = (0..60)
            .map(|i| {
                let date = today.saturating_add(time::Duration::days(i));
                json!({"date": date.to_string(), "infos": ["event ".repeat(400)]})
            })
            .collect();
        let result =
            CallToolResult::success(vec![Content::text(json!({"list": days}).to_string())]);
        let (out, trimmed) = limit(result);
        assert!(trimmed, "an oversized result must be trimmed");
        let body: Value = serde_json::from_str(&text_of(&out, 1)).unwrap();
        assert_eq!(
            body["list"][0]["date"],
            today.to_string(),
            "nearest day must be kept"
        );
        assert!(
            text_of(&out, 0).contains("kept the first"),
            "the notice must say the head was kept"
        );
    }

    #[test]
    fn nested_lists_are_cut_and_named() {
        let infos: Vec<Value> = (0..3000)
            .map(
                |i| json!({"symbol": format!("S{i}.US"), "content": "earnings release ".repeat(4)}),
            )
            .collect();
        let doc = json!({"list": [{"date": "2026-10-16", "infos": infos}], "partial": false});
        let result = CallToolResult::success(vec![Content::text(doc.to_string())]);
        let (out, trimmed) = limit(result);
        assert!(trimmed, "an oversized result must be trimmed");
        let body: Value = serde_json::from_str(&text_of(&out, 1)).unwrap();
        assert_eq!(body["partial"], false);
        assert!(
            body["list"][0]["infos"].as_array().unwrap().len() < 3000,
            "the nested infos array must be cut"
        );
        assert!(
            text_of(&out, 0).contains("/list/0/infos kept the first"),
            "the notice must name the cut path"
        );
        assert!(
            body["_truncated"]["cuts"][0]
                .as_str()
                .unwrap()
                .contains("/list/0/infos")
        );
    }

    #[test]
    fn errors_are_never_trimmed() {
        let big = "x".repeat(MAX_RESPONSE_BYTES * 2);
        let result = CallToolResult::error(vec![Content::text(big)]);
        let (_, trimmed) = limit(result);
        assert!(!trimmed, "an error result must never be trimmed");
    }
}
