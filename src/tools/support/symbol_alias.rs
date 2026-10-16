//! Rewrites common non-canonical symbol spellings to the form Longbridge
//! accepts, before a tool call reaches upstream: US index aliases, zero-padded
//! HK codes and Yahoo's `.SS` Shanghai suffix.
//!
//! Models reach for the Yahoo/Bloomberg spellings they have seen elsewhere —
//! `SPX.US`, `^GSPC.US`, `.VIX`, bare `NDX` — and each one fails upstream with
//! `invalid symbol` / `symbol not found`. The canonical form is the dotted
//! `.SPX.US`. Only the index codes verified live to resolve are rewritten;
//! anything else passes through untouched so a real ticker is never altered.

use serde_json::Value;

/// Index codes that resolve upstream in their dotted `.<CODE>.US` form
/// (verified live), plus the Yahoo alias mapped onto each.
const US_INDEXES: &[(&str, &str)] = &[
    ("SPX", "SPX"),
    ("GSPC", "SPX"),
    ("NDX", "NDX"),
    ("VIX", "VIX"),
    ("DJI", "DJI"),
    ("IXIC", "IXIC"),
];

/// The canonical `.<CODE>.US` form of `symbol` when it is a recognised US
/// index spelling, else `None`.
pub fn canonical_us_index(symbol: &str) -> Option<String> {
    let trimmed = symbol.trim();
    let upper = trimmed.to_ascii_uppercase();
    let base = upper.strip_suffix(".US").unwrap_or(&upper);
    let base = base.trim_start_matches(['^', '.']);
    let (_, code) = US_INDEXES.iter().find(|(alias, _)| *alias == base)?;
    let canonical = format!(".{code}.US");
    (canonical != trimmed).then_some(canonical)
}

/// The canonical form of a non-index symbol that models commonly misspell:
/// zero-padded HK codes (`00700.HK`, `0700.HK` → `700.HK`; several endpoints,
/// e.g. valuation comparison, fail on the padded form with an opaque 500) and
/// Yahoo's Shanghai suffix (`600519.SS` → `600519.SH`). `None` when the symbol
/// is already canonical or not one of these shapes.
pub fn canonical_listing(symbol: &str) -> Option<String> {
    let trimmed = symbol.trim();
    let (code, market) = trimmed.rsplit_once('.')?;
    let market = market.to_ascii_uppercase();
    let canonical = match market.as_str() {
        "HK" if code.len() > 1
            && code.starts_with('0')
            && code.bytes().all(|b| b.is_ascii_digit()) =>
        {
            let unpadded = code.trim_start_matches('0');
            format!("{}.HK", if unpadded.is_empty() { "0" } else { unpadded })
        }
        "SS" if code.bytes().all(|b| b.is_ascii_digit()) => format!("{code}.SH"),
        _ => return None,
    };
    (canonical != trimmed).then_some(canonical)
}

/// Rewrite the `symbol` / `symbols` arguments of a tool call in place.
/// Returns how many values were rewritten.
pub fn normalize_arguments(arguments: &mut serde_json::Map<String, Value>) -> usize {
    let mut rewritten = 0;
    let mut fix = |value: &mut Value| {
        if let Some(canonical) = value
            .as_str()
            .and_then(|s| canonical_us_index(s).or_else(|| canonical_listing(s)))
        {
            *value = Value::String(canonical);
            rewritten += 1;
        }
    };
    if let Some(value) = arguments.get_mut("symbol") {
        fix(value);
    }
    if let Some(Value::Array(values)) = arguments.get_mut("symbols") {
        values.iter_mut().for_each(&mut fix);
    }
    // valuation_comparison takes its peers as one comma-separated string.
    if let Some(Value::String(list)) = arguments.get_mut("comparison_symbols") {
        let parts: Vec<String> = list
            .split(',')
            .map(|part| {
                let part = part.trim();
                canonical_us_index(part)
                    .or_else(|| canonical_listing(part))
                    .inspect(|_| rewritten += 1)
                    .unwrap_or_else(|| part.to_string())
            })
            .collect();
        *list = parts.join(",");
    }
    rewritten
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rewrites_observed_index_spellings() {
        for (input, expected) in [
            ("SPX.US", ".SPX.US"),
            (".SPX", ".SPX.US"),
            ("^GSPC.US", ".SPX.US"),
            ("^VIX.US", ".VIX.US"),
            ("VIX", ".VIX.US"),
            ("ndx.us", ".NDX.US"),
            ("IXIC.US", ".IXIC.US"),
            ("^DJI.US", ".DJI.US"),
        ] {
            assert_eq!(
                canonical_us_index(input).as_deref(),
                Some(expected),
                "{input}"
            );
        }
    }

    #[test]
    fn leaves_canonical_and_unrelated_symbols_alone() {
        for input in [
            ".SPX.US",
            "AAPL.US",
            "SPXW.US",
            ".VVIX.US",
            "700.HK",
            "UVIX261030P34500.US",
            "VIX.HK",
        ] {
            assert_eq!(canonical_us_index(input), None, "{input}");
        }
    }

    #[test]
    fn unpads_hk_codes_and_maps_the_yahoo_shanghai_suffix() {
        for (input, expected) in [
            ("00700.HK", Some("700.HK")),
            ("0700.HK", Some("700.HK")),
            ("00005.HK", Some("5.HK")),
            ("06088.HK", Some("6088.HK")),
            ("600519.SS", Some("600519.SH")),
            ("700.HK", None),
            ("14055.HK", None),
            ("600519.SH", None),
            ("000001.SZ", None),
            ("AAPL.US", None),
        ] {
            assert_eq!(canonical_listing(input).as_deref(), expected, "{input}");
        }
    }

    #[test]
    fn normalizes_comma_separated_comparison_symbols() {
        let mut args = json!({"comparison_symbols": "09988.HK, 03690.HK,MSFT.US"})
            .as_object()
            .unwrap()
            .clone();
        assert_eq!(
            normalize_arguments(&mut args),
            2,
            "both padded peers are rewritten"
        );
        assert_eq!(args["comparison_symbols"], "9988.HK,3690.HK,MSFT.US");
    }

    #[test]
    fn normalizes_symbol_and_symbols_arguments() {
        let mut args =
            json!({"symbol": "SPX.US", "symbols": ["NDX.US", "AAPL.US"], "other": "SPX.US"})
                .as_object()
                .unwrap()
                .clone();
        assert_eq!(normalize_arguments(&mut args), 2);
        assert_eq!(
            Value::Object(args),
            json!({"symbol": ".SPX.US", "symbols": [".NDX.US", "AAPL.US"], "other": "SPX.US"})
        );
    }
}
