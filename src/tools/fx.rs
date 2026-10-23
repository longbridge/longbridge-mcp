//! Public foreign-exchange reference rates.
//!
//! Backed by [Frankfurter](https://frankfurter.dev), an open-source API over the
//! European Central Bank's daily euro foreign-exchange reference rates. Unlike
//! `exchange_rate` (Longbridge's internal asset-conversion rate, used to value
//! positions in an account's currency), this is a public, citable market
//! reference with full daily history back to 1999-01-04.
//!
//! The ECB publishes once per TARGET business day around 16:00 CET; there are
//! no intraday, weekend or bid/ask rates. A date with no publication resolves
//! to the most recent prior business day, reported in the response's `date`.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use time::Date;
use time::macros::{date, format_description};

use crate::tools::tool_result;

/// Public Frankfurter API base URL.
const DEFAULT_BASE_URL: &str = "https://api.frankfurter.dev/v1";

/// Environment variable overriding [`DEFAULT_BASE_URL`], e.g. to point at a
/// self-hosted Frankfurter instance where the public host is unreachable.
const BASE_URL_ENV: &str = "LONGBRIDGE_MCP_FX_URL";

/// Longest `start`..`end` span accepted, in days. Keeps a time series within a
/// size an LLM can usefully read.
const MAX_RANGE_DAYS: i64 = 366;

/// Most quote currencies a time series may request. With the day cap this
/// bounds a series to roughly 1,300 data points.
const MAX_SERIES_CURRENCIES: usize = 5;

/// First day of ECB reference-rate publication.
const MIN_DATE: Date = date!(1999 - 01 - 04);

/// Currencies the ECB currently publishes reference rates for (Frankfurter's
/// `/currencies`). Checked locally because the upstream answers an unknown code
/// with a bare `not found`, which gives a caller nothing to correct. Retired
/// codes (e.g. RUB, HRK) are left out even though old dates carry them.
const SUPPORTED_CURRENCIES: &[&str] = &[
    "AUD", "BRL", "CAD", "CHF", "CNY", "CZK", "DKK", "EUR", "GBP", "HKD", "HUF", "IDR", "ILS",
    "INR", "ISK", "JPY", "KRW", "MXN", "MYR", "NOK", "NZD", "PHP", "PLN", "RON", "SEK", "SGD",
    "THB", "TRY", "USD", "ZAR",
];

/// Cache lifetime for answers that can still change: `latest`, and any date
/// or series touching the last few days (the ECB may not have published yet).
const FRESH_TTL: Duration = Duration::from_secs(10 * 60);

/// Cache lifetime for answers about settled dates, which never change.
const SETTLED_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// A date at least this many days old has been published for certain.
const SETTLED_AFTER_DAYS: i64 = 3;

/// Most cached responses kept. A series response is at most ~25 KB, so the
/// cache stays within a few MB.
const CACHE_CAPACITY: usize = 128;

/// Attribution attached to every response.
const SOURCE: &str = "European Central Bank reference rates via Frankfurter (frankfurter.dev)";

static BASE_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var(BASE_URL_ENV)
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
});

static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("longbridge-mcp/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("fx http client must build with static settings")
});

/// Successful responses keyed by request URL. ECB rates change once a day,
/// while every user of this deployment shares one egress IP to the rate
/// source, so repeated questions are served from memory.
static CACHE: LazyLock<Mutex<HashMap<String, CacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// One gate per URL with a request in flight, so concurrent identical misses
/// make a single upstream call: the first holder fetches, the rest wait and
/// then read its result from [`CACHE`]. Entries are removed once no caller
/// holds them, so the map only ever holds in-flight URLs.
static IN_FLIGHT: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct CacheEntry {
    stored: Instant,
    ttl: Duration,
    json: String,
}

fn cache_get(key: &str) -> Option<String> {
    let cache = CACHE.lock().ok()?;
    cache
        .get(key)
        .filter(|e| e.stored.elapsed() < e.ttl)
        .map(|e| e.json.clone())
}

fn cache_put(key: String, ttl: Duration, json: String) {
    if let Ok(mut cache) = CACHE.lock() {
        insert_bounded(&mut cache, key, ttl, json);
    }
}

/// Insert into `cache`, first dropping expired entries and then the oldest
/// one when it is full, so it never exceeds [`CACHE_CAPACITY`].
fn insert_bounded(
    cache: &mut HashMap<String, CacheEntry>,
    key: String,
    ttl: Duration,
    json: String,
) {
    if cache.len() >= CACHE_CAPACITY && !cache.contains_key(&key) {
        cache.retain(|_, e| e.stored.elapsed() < e.ttl);
        if cache.len() >= CACHE_CAPACITY
            && let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, e)| e.stored)
                .map(|(k, _)| k.clone())
        {
            cache.remove(&oldest);
        }
    }
    cache.insert(
        key,
        CacheEntry {
            stored: Instant::now(),
            ttl,
            json,
        },
    );
}

/// Take (creating if needed) the in-flight gate for `key`.
fn gate_acquire(key: &str) -> Option<Arc<tokio::sync::Mutex<()>>> {
    let mut gates = IN_FLIGHT.lock().ok()?;
    Some(gates.entry(key.to_string()).or_default().clone())
}

/// Hand back a gate from [`gate_acquire`], dropping the map entry when no
/// other caller still holds it. Both sides run under the `IN_FLIGHT` lock, so
/// the reference count seen here is exact.
fn gate_release(key: &str, gate: Arc<tokio::sync::Mutex<()>>) {
    let Ok(mut gates) = IN_FLIGHT.lock() else {
        return;
    };
    // One reference is the map's, one is ours.
    if Arc::strong_count(&gate) <= 2 {
        gates.remove(key);
    }
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FxRateParam {
    /// Base currency, ISO 4217 code (default "USD"). Rates are the amount of each quote currency worth 1 base unit.
    pub base: Option<String>,
    /// Quote currencies as ISO 4217 codes, e.g. ["HKD", "CNY"] (not security symbols like "700.HK"). Omit for all 30 supported currencies. Required with `start`, at most 5. CNH, TWD and crypto are not covered.
    #[serde(alias = "symbols")]
    pub currencies: Option<Vec<String>>,
    /// Historical date (yyyy-mm-dd, from 1999-01-04). Returns the rate published on that day, or the closest prior business day. Mutually exclusive with `start`/`end`.
    pub date: Option<String>,
    /// Time-series start date (yyyy-mm-dd). Returns one entry per business day from `start` to `end`.
    pub start: Option<String>,
    /// Time-series end date (yyyy-mm-dd, default: latest). Only valid with `start`; the span may not exceed 366 days.
    pub end: Option<String>,
}

/// What to request from Frankfurter, after validation.
#[derive(Debug, PartialEq, Eq)]
struct FxQuery {
    /// Path below the base URL: `latest`, `2024-01-02`, or `2024-01-02..2024-02-01`.
    path: String,
    base: String,
    /// Comma-joined quote currencies (Frankfurter's `symbols` query parameter).
    symbols: Option<String>,
    /// Whether the answer covers only dates old enough to never change.
    settled: bool,
}

fn invalid(msg: impl Into<String>) -> McpError {
    McpError::invalid_params(msg.into(), None)
}

fn currency_code(raw: &str) -> Result<String, McpError> {
    let code = raw.trim().to_ascii_uppercase();
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(invalid(format!(
            "invalid currency code '{raw}': expected a 3-letter ISO 4217 code such as USD"
        )));
    }
    if SUPPORTED_CURRENCIES.contains(&code.as_str()) {
        return Ok(code);
    }
    let hint = if code == "CNH" {
        " CNY (the onshore rate) is the closest available; say so if you use it."
    } else {
        ""
    };
    Err(invalid(format!(
        "{code} is not covered by ECB reference rates.{hint} Supported: {}",
        SUPPORTED_CURRENCIES.join(", ")
    )))
}

fn today() -> Date {
    time::OffsetDateTime::now_utc().date()
}

/// Parse a `yyyy-mm-dd` date within the published range. The upper bound is
/// one day past today in UTC, so a caller east of UTC (e.g. Hong Kong between
/// 00:00 and 08:00) can still ask for its own "today"; the rate source answers
/// such a date with its latest publication.
fn parse_date(field: &str, raw: &str) -> Result<Date, McpError> {
    let d = Date::parse(raw.trim(), format_description!("[year]-[month]-[day]")).map_err(|e| {
        invalid(format!(
            "invalid {field} '{raw}': expected yyyy-mm-dd ({e})"
        ))
    })?;
    if d < MIN_DATE {
        return Err(invalid(format!(
            "{field} {d} is before {MIN_DATE}, the first ECB reference-rate publication"
        )));
    }
    if d > today() + time::Duration::days(1) {
        return Err(invalid(format!("{field} {d} is in the future")));
    }
    Ok(d)
}

fn build_query(p: FxRateParam) -> Result<FxQuery, McpError> {
    let base = currency_code(p.base.as_deref().unwrap_or("USD"))?;

    let symbols = match p.currencies {
        Some(list) if !list.is_empty() => {
            let mut codes = Vec::with_capacity(list.len());
            for s in &list {
                let code = currency_code(s)?;
                if code != base && !codes.contains(&code) {
                    codes.push(code);
                }
            }
            if codes.is_empty() {
                return Err(invalid(
                    "currencies must name at least one currency other than base",
                ));
            }
            Some(codes)
        }
        _ => None,
    };

    let settled_before = today() - time::Duration::days(SETTLED_AFTER_DAYS);
    let mut settled = false;
    let path = match (p.date, p.start, p.end) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            return Err(invalid("pass either date or start/end, not both"));
        }
        (None, None, Some(_)) => return Err(invalid("end requires start")),
        (Some(date), None, None) => {
            let date = parse_date("date", &date)?;
            settled = date < settled_before;
            date.to_string()
        }
        (None, Some(start), end) => {
            match &symbols {
                None => {
                    return Err(invalid(
                        "currencies is required for a time series (start/end), e.g. [\"HKD\"]",
                    ));
                }
                Some(codes) if codes.len() > MAX_SERIES_CURRENCIES => {
                    return Err(invalid(format!(
                        "a time series may request at most {MAX_SERIES_CURRENCIES} currencies; \
                         split the request"
                    )));
                }
                Some(_) => {}
            }
            let start = parse_date("start", &start)?;
            let end = end.map(|e| parse_date("end", &e)).transpose()?;
            let span_end = end.unwrap_or_else(today);
            if span_end < start {
                return Err(invalid("end must not be earlier than start"));
            }
            if (span_end - start).whole_days() > MAX_RANGE_DAYS {
                return Err(invalid(format!(
                    "time series may span at most {MAX_RANGE_DAYS} days; split the request"
                )));
            }
            settled = end.is_some_and(|end| end < settled_before);
            match end {
                Some(end) => format!("{start}..{end}"),
                None => format!("{start}.."),
            }
        }
        (None, None, None) => "latest".to_string(),
    };

    Ok(FxQuery {
        path,
        base,
        symbols: symbols.map(|codes| codes.join(",")),
        settled,
    })
}

/// Leading fields of a reshaped response, in order: the date context comes
/// before `rates`, which can hold 30 entries and would otherwise bury it.
const LEADING_FIELDS: &[&str] = &["base", "date", "start_date", "end_date"];

/// Reshape a Frankfurter body: put the base and dates first, drop the constant
/// `amount` (always 1 here), and attach the data source last.
fn shape(body: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(mut obj) = body else {
        return body;
    };
    obj.shift_remove("amount");
    let mut out = serde_json::Map::with_capacity(obj.len() + 1);
    for key in LEADING_FIELDS {
        if let Some(v) = obj.shift_remove(*key) {
            out.insert((*key).to_string(), v);
        }
    }
    out.extend(obj);
    out.insert(
        "source".to_string(),
        serde_json::Value::String(SOURCE.to_string()),
    );
    serde_json::Value::Object(out)
}

/// The rate source failed. Says outright not to substitute a remembered rate:
/// a model left with no data otherwise tends to answer from training data.
fn unavailable(detail: &str) -> McpError {
    McpError::internal_error(
        format!(
            "FX reference rates are temporarily unavailable ({detail}). Do not estimate or \
             recall a rate; tell the user it could not be retrieved, or retry later."
        ),
        None,
    )
}

async fn fetch(base_url: &str, query: FxQuery) -> Result<CallToolResult, McpError> {
    let mut params = vec![("base", query.base.as_str())];
    if let Some(symbols) = query.symbols.as_deref() {
        params.push(("symbols", symbols));
    }
    let qs = serde_urlencoded::to_string(&params).map_err(|e| unavailable(&e.to_string()))?;
    let url = format!("{base_url}/{}?{qs}", query.path);
    if let Some(json) = cache_get(&url) {
        crate::metrics::record_fx_rate_cache(true);
        return Ok(tool_result(json));
    }

    // A poisoned map only loses the dedupe; the request still goes through.
    let Some(gate) = gate_acquire(&url) else {
        return request(&url, query.settled).await.map(tool_result);
    };
    let result = {
        let _held = gate.lock().await;
        match cache_get(&url) {
            Some(json) => {
                crate::metrics::record_fx_rate_cache(true);
                Ok(json)
            }
            None => request(&url, query.settled).await,
        }
    };
    gate_release(&url, gate);
    result.map(tool_result)
}

/// Fetch `url` from the rate source, cache a success, and return its JSON.
async fn request(url: &str, settled: bool) -> Result<String, McpError> {
    crate::metrics::record_fx_rate_cache(false);
    let resp = HTTP_CLIENT
        .get(url)
        .send()
        .await
        .map_err(|e| unavailable(&e.to_string()))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| unavailable(&e.to_string()))?;

    if !status.is_success() {
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("message")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("HTTP {status}"));
        // Codes and dates are validated locally, so a 4xx here is a gap in
        // that validation rather than a typo; still a caller-fixable request.
        return Err(if status.is_client_error() {
            invalid(format!(
                "fx rate source rejected the request ({detail}); check currency codes and dates"
            ))
        } else {
            unavailable(&detail)
        });
    }

    let body: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| unavailable(&e.to_string()))?;
    let json = serde_json::to_string(&shape(body)).unwrap_or_default();
    let ttl = if settled { SETTLED_TTL } else { FRESH_TTL };
    cache_put(url.to_string(), ttl, json.clone());
    Ok(json)
}

/// Get ECB reference exchange rates (latest, a historical date, or a series).
pub async fn fx_rate(p: FxRateParam) -> Result<CallToolResult, McpError> {
    let query = build_query(p)?;
    fetch(&BASE_URL, query).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param() -> FxRateParam {
        FxRateParam::default()
    }

    fn strs(v: &[&str]) -> Option<Vec<String>> {
        Some(v.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn defaults_to_latest_usd_all_currencies() {
        let q = build_query(param()).unwrap();
        assert_eq!(
            q,
            FxQuery {
                path: "latest".into(),
                base: "USD".into(),
                symbols: None,
                settled: false,
            }
        );
    }

    #[test]
    fn normalizes_and_dedupes_codes() {
        let q = build_query(FxRateParam {
            base: Some(" hkd ".into()),
            currencies: strs(&["usd", "CNY", "Usd", "HKD"]),
            ..param()
        })
        .unwrap();
        assert_eq!(q.base, "HKD");
        assert_eq!(q.symbols.as_deref(), Some("USD,CNY"));
    }

    #[test]
    fn rejects_malformed_codes() {
        for bad in ["US", "USDT", "U1D", ""] {
            assert!(
                build_query(FxRateParam {
                    base: Some(bad.into()),
                    ..param()
                })
                .is_err(),
                "base '{bad}' should be rejected"
            );
        }
    }

    #[test]
    fn rejects_currencies_equal_to_base_only() {
        let err = build_query(FxRateParam {
            currencies: strs(&["USD"]),
            ..param()
        })
        .unwrap_err();
        assert!(err.message.contains("other than base"), "{}", err.message);
    }

    #[test]
    fn historical_date_path() {
        let q = build_query(FxRateParam {
            date: Some("2024-01-02".into()),
            ..param()
        })
        .unwrap();
        assert_eq!(q.path, "2024-01-02");
    }

    #[test]
    fn series_paths() {
        let closed = build_query(FxRateParam {
            currencies: strs(&["HKD"]),
            start: Some("2024-01-02".into()),
            end: Some("2024-03-01".into()),
            ..param()
        })
        .unwrap();
        assert_eq!(closed.path, "2024-01-02..2024-03-01");

        let today = time::OffsetDateTime::now_utc().date();
        let open = build_query(FxRateParam {
            currencies: strs(&["HKD"]),
            start: Some((today - time::Duration::days(10)).to_string()),
            ..param()
        })
        .unwrap();
        assert!(open.path.ends_with(".."), "{}", open.path);
    }

    #[test]
    fn rejects_bad_date_combinations() {
        let cases = [
            FxRateParam {
                date: Some("2024-01-02".into()),
                start: Some("2024-01-02".into()),
                ..param()
            },
            FxRateParam {
                end: Some("2024-01-02".into()),
                ..param()
            },
            FxRateParam {
                start: Some("2024-01-02".into()),
                ..param()
            },
            FxRateParam {
                currencies: strs(&["HKD"]),
                start: Some("2024-03-01".into()),
                end: Some("2024-01-02".into()),
                ..param()
            },
            FxRateParam {
                currencies: strs(&["HKD"]),
                start: Some("2022-01-01".into()),
                end: Some("2024-01-01".into()),
                ..param()
            },
            FxRateParam {
                date: Some("2024/01/02".into()),
                ..param()
            },
        ];
        for (i, p) in cases.into_iter().enumerate() {
            assert!(build_query(p).is_err(), "case {i} should be rejected");
        }
    }

    #[test]
    fn rejects_unsupported_currencies_with_the_supported_list() {
        let err = build_query(FxRateParam {
            currencies: strs(&["TWD"]),
            ..param()
        })
        .unwrap_err();
        assert!(
            err.message.contains("TWD is not covered"),
            "{}",
            err.message
        );
        assert!(err.message.contains("HKD, HUF"), "{}", err.message);

        let err = build_query(FxRateParam {
            base: Some("cnh".into()),
            ..param()
        })
        .unwrap_err();
        assert!(
            err.message.contains("CNY (the onshore rate)"),
            "{}",
            err.message
        );
    }

    #[test]
    fn rejects_dates_outside_the_published_range() {
        let before = build_query(FxRateParam {
            date: Some("1998-12-31".into()),
            ..param()
        })
        .unwrap_err();
        assert!(before.message.contains("1999-01-04"), "{}", before.message);

        // Tomorrow in UTC is still "today" east of UTC, so it is accepted.
        assert!(
            build_query(FxRateParam {
                date: Some((today() + time::Duration::days(1)).to_string()),
                ..param()
            })
            .is_ok(),
            "tomorrow (UTC) must be accepted for callers east of UTC"
        );

        let future = build_query(FxRateParam {
            date: Some((today() + time::Duration::days(2)).to_string()),
            ..param()
        })
        .unwrap_err();
        assert!(future.message.contains("future"), "{}", future.message);
    }

    #[test]
    fn caps_currencies_in_a_series() {
        let err = build_query(FxRateParam {
            currencies: strs(&["HKD", "CNY", "JPY", "SGD", "GBP", "AUD"]),
            start: Some("2024-01-02".into()),
            end: Some("2024-02-01".into()),
            ..param()
        })
        .unwrap_err();
        assert!(err.message.contains("at most 5"), "{}", err.message);

        // The cap applies to series only; a single day may ask for more.
        assert!(
            build_query(FxRateParam {
                currencies: strs(&["HKD", "CNY", "JPY", "SGD", "GBP", "AUD"]),
                ..param()
            })
            .is_ok(),
            "latest with six currencies should be accepted"
        );
    }

    #[test]
    fn supported_list_is_sorted_and_unique() {
        assert!(
            SUPPORTED_CURRENCIES.windows(2).all(|w| w[0] < w[1]),
            "SUPPORTED_CURRENCIES must stay sorted and free of duplicates"
        );
    }

    #[test]
    fn accepts_symbols_as_an_alias_for_currencies() {
        let p: FxRateParam = serde_json::from_value(serde_json::json!({"symbols": ["HKD"]}))
            .expect("alias must deserialize");
        assert_eq!(p.currencies, strs(&["HKD"]));
    }

    #[test]
    fn only_old_dates_are_settled() {
        let recent = (today() - time::Duration::days(1)).to_string();
        let cases = [
            (param(), false),
            (
                FxRateParam {
                    date: Some("2024-01-02".into()),
                    ..param()
                },
                true,
            ),
            (
                FxRateParam {
                    date: Some(recent.clone()),
                    ..param()
                },
                false,
            ),
            (
                FxRateParam {
                    currencies: strs(&["HKD"]),
                    start: Some("2024-01-02".into()),
                    end: Some("2024-02-01".into()),
                    ..param()
                },
                true,
            ),
            (
                FxRateParam {
                    currencies: strs(&["HKD"]),
                    start: Some((today() - time::Duration::days(20)).to_string()),
                    end: Some(recent),
                    ..param()
                },
                false,
            ),
        ];
        for (i, (p, want)) in cases.into_iter().enumerate() {
            assert_eq!(build_query(p).unwrap().settled, want, "case {i}");
        }
    }

    #[test]
    fn shape_puts_dates_before_rates() {
        let out = shape(serde_json::json!({
            "amount": 1.0, "base": "USD", "start_date": "2024-01-02",
            "end_date": "2024-01-03", "rates": {"2024-01-02": {"HKD": 7.8}}
        }));
        let keys: Vec<&str> = out
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["base", "start_date", "end_date", "rates", "source"]);
    }

    #[test]
    fn cache_evicts_to_stay_within_capacity() {
        // A local map, not the shared `CACHE`: filling the global one would
        // evict entries other tests running in parallel rely on.
        let mut cache = HashMap::new();
        for i in 0..CACHE_CAPACITY + 10 {
            insert_bounded(&mut cache, format!("k{i}"), FRESH_TTL, String::new());
        }
        assert_eq!(cache.len(), CACHE_CAPACITY, "cache must stay at capacity");
        assert!(
            cache.contains_key(&format!("k{}", CACHE_CAPACITY + 9)),
            "newest entry must survive eviction"
        );
    }

    #[test]
    fn shape_drops_amount_and_adds_source() {
        let out = shape(serde_json::json!({
            "amount": 1.0, "base": "USD", "date": "2026-10-21", "rates": {"HKD": 7.8472}
        }));
        assert!(out.get("amount").is_none(), "amount should be dropped");
        assert_eq!(out["rates"]["HKD"], 7.8472);
        assert_eq!(out["source"], SOURCE);
    }

    /// Serve one canned HTTP response on a local port and return its base URL.
    async fn mock_server(status: u16, body: &'static str) -> String {
        counting_mock_server(status, body, Duration::ZERO).await.0
    }

    /// Like [`mock_server`], answering after `delay` and also returning a
    /// count of requests served.
    async fn counting_mock_server(
        status: u16,
        body: &'static str,
        delay: Duration,
    ) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use axum::http::StatusCode;
        use axum::routing::get;

        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let code = StatusCode::from_u16(status).expect("valid status");
        let app = axum::Router::new().fallback(get(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            async move {
                tokio::time::sleep(delay).await;
                (code, body)
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), hits)
    }

    #[tokio::test]
    async fn repeated_requests_are_served_from_cache() {
        use std::sync::atomic::Ordering;

        let (base, hits) = counting_mock_server(
            200,
            r#"{"amount":1.0,"base":"USD","date":"2026-10-21","rates":{"HKD":7.8472}}"#,
            Duration::ZERO,
        )
        .await;
        for _ in 0..3 {
            let result = fetch(&base, build_query(param()).unwrap()).await.unwrap();
            assert_eq!(result.is_error, Some(false));
        }
        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "later calls must hit the cache"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_identical_requests_share_one_upstream_call() {
        use std::sync::atomic::Ordering;

        let (base, hits) = counting_mock_server(
            200,
            r#"{"amount":1.0,"base":"USD","date":"2026-10-21","rates":{"HKD":7.8472}}"#,
            // Slow enough that all eight calls are in flight together.
            Duration::from_millis(300),
        )
        .await;
        let calls: Vec<_> = (0..8)
            .map(|_| {
                let base = base.clone();
                tokio::spawn(async move { fetch(&base, build_query(param()).unwrap()).await })
            })
            .collect();
        for call in calls {
            let result = call.await.expect("task").expect("fetch");
            assert_eq!(result.is_error, Some(false));
        }
        assert_eq!(hits.load(Ordering::SeqCst), 1, "misses must be coalesced");
        let in_flight = IN_FLIGHT.lock().expect("in-flight lock");
        assert!(
            !in_flight.keys().any(|k| k.starts_with(&base)),
            "gates must be released once the requests finish"
        );
    }

    #[tokio::test]
    async fn failures_are_not_cached() {
        use std::sync::atomic::Ordering;

        let (base, hits) = counting_mock_server(502, "bad gateway", Duration::ZERO).await;
        for _ in 0..2 {
            assert!(fetch(&base, build_query(param()).unwrap()).await.is_err());
        }
        assert_eq!(hits.load(Ordering::SeqCst), 2, "errors must not be cached");
    }

    #[tokio::test]
    async fn fetch_returns_shaped_structured_result() {
        let base = mock_server(
            200,
            r#"{"amount":1.0,"base":"USD","date":"2026-10-21","rates":{"HKD":7.8472}}"#,
        )
        .await;
        let result = fetch(&base, build_query(param()).unwrap()).await.unwrap();
        let structured = result.structured_content.expect("object result");
        assert_eq!(structured["rates"]["HKD"], 7.8472);
        assert_eq!(structured["source"], SOURCE);
        assert!(
            structured.get("amount").is_none(),
            "amount should be dropped"
        );
    }

    #[tokio::test]
    async fn fetch_maps_client_errors_to_invalid_params() {
        let base = mock_server(404, r#"{"message":"not found"}"#).await;
        let err = fetch(&base, build_query(param()).unwrap())
            .await
            .unwrap_err();
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("not found"), "{}", err.message);
    }

    #[tokio::test]
    async fn fetch_maps_server_errors_to_internal() {
        let base = mock_server(502, "bad gateway").await;
        let err = fetch(&base, build_query(param()).unwrap())
            .await
            .unwrap_err();
        assert_eq!(err.code, rmcp::model::ErrorCode::INTERNAL_ERROR);
        assert!(err.message.contains("Do not estimate"), "{}", err.message);
    }
}
