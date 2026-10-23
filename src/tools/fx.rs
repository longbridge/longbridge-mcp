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

use std::sync::LazyLock;
use std::time::Duration;

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use time::Date;
use time::macros::format_description;

use crate::tools::tool_result;

/// Public Frankfurter API base URL.
const DEFAULT_BASE_URL: &str = "https://api.frankfurter.dev/v1";

/// Environment variable overriding [`DEFAULT_BASE_URL`], e.g. to point at a
/// self-hosted Frankfurter instance where the public host is unreachable.
const BASE_URL_ENV: &str = "LONGBRIDGE_MCP_FX_URL";

/// Longest `start`..`end` span accepted, in days. Keeps a time series within a
/// size an LLM can usefully read.
const MAX_RANGE_DAYS: i64 = 366;

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

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FxRateParam {
    /// Base currency, ISO 4217 code (default "USD"). Rates are the amount of each quote currency worth 1 base unit.
    pub base: Option<String>,
    /// Quote currencies, ISO 4217 codes, e.g. ["HKD", "CNY"]. Omit for every supported currency (about 30, including USD, EUR, HKD, CNY, JPY, SGD, GBP, AUD). Required with `start`.
    pub symbols: Option<Vec<String>>,
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
    symbols: Option<String>,
}

fn invalid(msg: impl Into<String>) -> McpError {
    McpError::invalid_params(msg.into(), None)
}

fn currency_code(raw: &str) -> Result<String, McpError> {
    let code = raw.trim().to_ascii_uppercase();
    if code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(code)
    } else {
        Err(invalid(format!(
            "invalid currency code '{raw}': expected a 3-letter ISO 4217 code such as USD"
        )))
    }
}

fn parse_date(field: &str, raw: &str) -> Result<Date, McpError> {
    Date::parse(raw.trim(), format_description!("[year]-[month]-[day]")).map_err(|e| {
        invalid(format!(
            "invalid {field} '{raw}': expected yyyy-mm-dd ({e})"
        ))
    })
}

fn build_query(p: FxRateParam) -> Result<FxQuery, McpError> {
    let base = currency_code(p.base.as_deref().unwrap_or("USD"))?;

    let symbols = match p.symbols {
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
                    "symbols must name at least one currency other than base",
                ));
            }
            Some(codes.join(","))
        }
        _ => None,
    };

    let path = match (p.date, p.start, p.end) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            return Err(invalid("pass either date or start/end, not both"));
        }
        (None, None, Some(_)) => return Err(invalid("end requires start")),
        (Some(date), None, None) => parse_date("date", &date)?.to_string(),
        (None, Some(start), end) => {
            if symbols.is_none() {
                return Err(invalid(
                    "symbols is required for a time series (start/end), e.g. [\"HKD\"]",
                ));
            }
            let start = parse_date("start", &start)?;
            let end = end.map(|e| parse_date("end", &e)).transpose()?;
            let span_end = end.unwrap_or_else(|| time::OffsetDateTime::now_utc().date());
            if span_end < start {
                return Err(invalid("end must not be earlier than start"));
            }
            if (span_end - start).whole_days() > MAX_RANGE_DAYS {
                return Err(invalid(format!(
                    "time series may span at most {MAX_RANGE_DAYS} days; split the request"
                )));
            }
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
        symbols,
    })
}

/// Reshape a Frankfurter body: drop the constant `amount` (always 1 here) and
/// attach the data source.
fn shape(mut body: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = body.as_object_mut() {
        obj.remove("amount");
        obj.insert(
            "source".to_string(),
            serde_json::Value::String(SOURCE.to_string()),
        );
    }
    body
}

async fn fetch(base_url: &str, query: FxQuery) -> Result<CallToolResult, McpError> {
    let url = format!("{base_url}/{}", query.path);
    let mut params = vec![("base", query.base.as_str())];
    if let Some(symbols) = query.symbols.as_deref() {
        params.push(("symbols", symbols));
    }

    let resp = HTTP_CLIENT
        .get(&url)
        .query(&params)
        .send()
        .await
        .map_err(|e| McpError::internal_error(format!("fx rate source unreachable: {e}"), None))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| {
        McpError::internal_error(format!("failed to read fx rate response: {e}"), None)
    })?;

    if !status.is_success() {
        let detail = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("message")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("HTTP {status}"));
        // 404 = unknown currency, 422 = unsupported date: both fixable by the caller.
        return Err(if status.is_client_error() {
            invalid(format!(
                "fx rate source rejected the request ({detail}); check currency codes and dates"
            ))
        } else {
            McpError::internal_error(format!("fx rate source error: {detail}"), None)
        });
    }

    let body: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| McpError::internal_error(format!("invalid fx rate response: {e}"), None))?;
    let json = serde_json::to_string(&shape(body)).unwrap_or_default();
    Ok(tool_result(json))
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
            }
        );
    }

    #[test]
    fn normalizes_and_dedupes_codes() {
        let q = build_query(FxRateParam {
            base: Some(" hkd ".into()),
            symbols: strs(&["usd", "CNY", "Usd", "HKD"]),
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
    fn rejects_symbols_equal_to_base_only() {
        let err = build_query(FxRateParam {
            symbols: strs(&["USD"]),
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
            symbols: strs(&["HKD"]),
            start: Some("2024-01-02".into()),
            end: Some("2024-03-01".into()),
            ..param()
        })
        .unwrap();
        assert_eq!(closed.path, "2024-01-02..2024-03-01");

        let today = time::OffsetDateTime::now_utc().date();
        let open = build_query(FxRateParam {
            symbols: strs(&["HKD"]),
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
                symbols: strs(&["HKD"]),
                start: Some("2024-03-01".into()),
                end: Some("2024-01-02".into()),
                ..param()
            },
            FxRateParam {
                symbols: strs(&["HKD"]),
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
        use axum::http::StatusCode;
        use axum::routing::get;

        let code = StatusCode::from_u16(status).expect("valid status");
        let app = axum::Router::new().fallback(get(move || async move { (code, body) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
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
    }
}
