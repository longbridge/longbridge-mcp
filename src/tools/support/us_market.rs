//! Helpers for routing MCP tools to US-market SDK endpoints based on the
//! calling account's DC region and, for symbol-keyed fundamental tools, the
//! symbol's market suffix.

use time::{OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};
use time_tz::timezones::db::america::NEW_YORK;
use time_tz::{OffsetDateTimeExt, OffsetResult, PrimitiveDateTimeExt};

/// Start of the current US-market day: midnight in New York
/// (America/New_York, so EST or EDT as daylight saving applies) for the
/// instant `now`, returned in UTC.
pub fn us_market_start_of_day(now: OffsetDateTime) -> OffsetDateTime {
    let date = now.to_timezone(NEW_YORK).date();
    match PrimitiveDateTime::new(date, Time::MIDNIGHT).assume_timezone(NEW_YORK) {
        OffsetResult::Some(midnight) | OffsetResult::Ambiguous(midnight, _) => {
            midnight.to_offset(UtcOffset::UTC)
        }
        // New York never skips midnight (its transitions happen at 02:00),
        // but fall back to the current offset rather than fail.
        OffsetResult::None => PrimitiveDateTime::new(date, Time::MIDNIGHT)
            .assume_offset(now.to_timezone(NEW_YORK).offset())
            .to_offset(UtcOffset::UTC),
    }
}

/// True when `symbol` is a US-listed equity/ETF symbol, e.g. "AAPL.US".
/// Case-insensitive on the suffix.
pub fn is_us_symbol(symbol: &str) -> bool {
    symbol
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("US"))
}

/// True when `symbol` is a US-DC crypto symbol, e.g. "BTCUSD.BKKT".
/// Case-insensitive on the suffix. `.HAS`/`.OSL` crypto symbols stay on the
/// existing HK-region static-info path and are not matched here.
pub fn is_us_crypto_symbol(symbol: &str) -> bool {
    symbol
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("BKKT"))
}

/// True when a US-region account is asking about a US symbol — the gate used
/// by every fundamental tool that has a US-specific SDK method.
pub async fn is_us_fundamental(mctx: &crate::tools::McpContext, symbol: &str) -> bool {
    is_us_symbol(symbol) && mctx.dc_region().await == longbridge::DcRegion::Us
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn us_market_day_starts_at_new_york_midnight() {
        assert_eq!(
            us_market_start_of_day(datetime!(2026-10-23 15:00 UTC)),
            datetime!(2026-10-23 04:00 UTC),
            "EDT: New York midnight is 04:00 UTC"
        );
        assert_eq!(
            us_market_start_of_day(datetime!(2026-12-01 15:00 UTC)),
            datetime!(2026-12-01 05:00 UTC),
            "EST: New York midnight is 05:00 UTC"
        );
        assert_eq!(
            us_market_start_of_day(datetime!(2026-10-24 02:00 UTC)),
            datetime!(2026-10-23 04:00 UTC),
            "after UTC midnight it is still the previous day in New York"
        );
    }

    #[test]
    fn us_market_day_start_handles_dst_transition_days() {
        // 2026-03-08: clocks spring forward at 02:00 EST; midnight was EST.
        assert_eq!(
            us_market_start_of_day(datetime!(2026-03-08 18:00 UTC)),
            datetime!(2026-03-08 05:00 UTC),
            "spring-forward day starts at EST midnight"
        );
        // 2026-11-01: clocks fall back at 02:00 EDT; midnight was EDT.
        assert_eq!(
            us_market_start_of_day(datetime!(2026-11-01 18:00 UTC)),
            datetime!(2026-11-01 04:00 UTC),
            "fall-back day starts at EDT midnight"
        );
    }

    #[test]
    fn is_us_symbol_matches_us_suffix_case_insensitively() {
        assert!(is_us_symbol("AAPL.US"));
        assert!(is_us_symbol("aapl.us"));
        assert!(!is_us_symbol("700.HK"));
        assert!(!is_us_symbol("AAPL"));
    }

    #[test]
    fn is_us_crypto_symbol_matches_only_bkkt() {
        assert!(is_us_crypto_symbol("BTCUSD.BKKT"));
        assert!(is_us_crypto_symbol("btcusd.bkkt"));
        assert!(!is_us_crypto_symbol("BTCUSD.HAS"));
        assert!(!is_us_crypto_symbol("BTCUSD.OSL"));
        assert!(!is_us_crypto_symbol("AAPL.US"));
    }
}
