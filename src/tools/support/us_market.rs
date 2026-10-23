//! Helpers for routing MCP tools to US-market SDK endpoints based on the
//! calling account's DC region and, for symbol-keyed fundamental tools, the
//! symbol's market suffix.

use time::macros::time;
use time::{Date, Duration, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};
use time_tz::timezones::db::america::NEW_YORK;
use time_tz::{OffsetDateTimeExt, OffsetResult, PrimitiveDateTimeExt};

/// New York wall-clock time at which the US overnight session opens
/// (20:00 ET). Orders placed from then on belong to the next trading day.
const OVERNIGHT_SESSION_OPEN: Time = time!(20:00);

/// The instant `time` on `date` in New York (America/New_York, so EST or EDT
/// as daylight saving applies), returned in UTC. An ambiguous wall-clock time
/// (the repeated hour on fall-back day) resolves to its first occurrence; a
/// skipped one (the spring-forward gap) uses the offset in force before the
/// gap.
pub fn new_york_instant(date: Date, time: Time) -> OffsetDateTime {
    let local = PrimitiveDateTime::new(date, time);
    match local.assume_timezone(NEW_YORK) {
        OffsetResult::Some(instant) | OffsetResult::Ambiguous(instant, _) => {
            instant.to_offset(UtcOffset::UTC)
        }
        OffsetResult::None => {
            let before = PrimitiveDateTime::new(date, Time::MIDNIGHT)
                .assume_utc()
                .to_timezone(NEW_YORK)
                .offset();
            local.assume_offset(before).to_offset(UtcOffset::UTC)
        }
    }
}

/// The `[start, end]` window (UTC) that US `today_orders` covers at `now`:
/// from 20:00 New York time on the previous calendar day — when the overnight
/// session (20:00–04:00 ET) that trades into today opens — through 23:59:59
/// New York time today. Starting at 20:00 rather than midnight keeps an order
/// placed in the overnight session visible after the clock passes midnight;
/// ending at the end of the New York day rather than `now` keeps an order
/// stamped slightly ahead of this server's clock from being cut off.
pub fn us_today_orders_window(now: OffsetDateTime) -> (OffsetDateTime, OffsetDateTime) {
    let today = now.to_timezone(NEW_YORK).date();
    let yesterday = today - Duration::days(1);
    (
        new_york_instant(yesterday, OVERNIGHT_SESSION_OPEN),
        new_york_instant(today, time!(23:59:59)),
    )
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
    use time::macros::{date, datetime};

    use super::*;

    #[test]
    fn us_today_window_starts_at_the_previous_overnight_open() {
        assert_eq!(
            us_today_orders_window(datetime!(2026-10-23 15:00 UTC)),
            (
                datetime!(2026-10-23 00:00 UTC),
                datetime!(2026-10-24 03:59:59 UTC)
            ),
            "EDT: 2026-10-22 20:00 ET through 2026-10-23 23:59:59 ET"
        );
        assert_eq!(
            us_today_orders_window(datetime!(2026-12-01 15:00 UTC)),
            (
                datetime!(2026-12-01 01:00 UTC),
                datetime!(2026-12-02 04:59:59 UTC)
            ),
            "EST: 2026-11-30 20:00 ET through 2026-12-01 23:59:59 ET"
        );
        assert_eq!(
            us_today_orders_window(datetime!(2026-10-24 02:00 UTC)).0,
            datetime!(2026-10-23 00:00 UTC),
            "after UTC midnight it is still 2026-10-23 in New York"
        );
    }

    #[test]
    fn us_today_window_keeps_overnight_orders_across_new_york_midnight() {
        // Placed at 20:30 ET on 2026-10-23, in the overnight session.
        let order = datetime!(2026-10-24 00:30 UTC);
        for (now, label) in [
            (datetime!(2026-10-24 03:30 UTC), "23:30 ET, same day"),
            (datetime!(2026-10-24 04:30 UTC), "00:30 ET, after midnight"),
            (datetime!(2026-10-24 07:59 UTC), "03:59 ET, overnight close"),
        ] {
            let (start, end) = us_today_orders_window(now);
            assert!(
                start <= order && order <= end,
                "{label}: the overnight order must stay in today's window {start}..{end}"
            );
            assert!(now <= end, "{label}: the window must cover now");
        }
    }

    #[test]
    fn us_today_window_handles_dst_transition_days() {
        // 2026-03-08: clocks spring forward at 02:00 EST. The window opens at
        // 20:00 EST on 03-07 and closes at 23:59:59 EDT on 03-08.
        assert_eq!(
            us_today_orders_window(datetime!(2026-03-08 18:00 UTC)),
            (
                datetime!(2026-03-08 01:00 UTC),
                datetime!(2026-03-09 03:59:59 UTC)
            ),
            "spring-forward day"
        );
        // 2026-11-01: clocks fall back at 02:00 EDT. The window opens at
        // 20:00 EDT on 10-31 and closes at 23:59:59 EST on 11-01.
        assert_eq!(
            us_today_orders_window(datetime!(2026-11-01 18:00 UTC)),
            (
                datetime!(2026-11-01 00:00 UTC),
                datetime!(2026-11-02 04:59:59 UTC)
            ),
            "fall-back day"
        );
        // The day after spring-forward opens at 20:00 EDT on the transition day.
        assert_eq!(
            us_today_orders_window(datetime!(2026-03-09 15:00 UTC)).0,
            datetime!(2026-03-09 00:00 UTC),
            "the day after spring-forward"
        );
    }

    #[test]
    fn new_york_instant_resolves_dst_edge_times() {
        assert_eq!(
            new_york_instant(date!(2026 - 11 - 01), time!(01:30)),
            datetime!(2026-11-01 05:30 UTC),
            "the repeated fall-back hour resolves to its first (EDT) occurrence"
        );
        assert_eq!(
            new_york_instant(date!(2026 - 03 - 08), time!(02:30)),
            datetime!(2026-03-08 07:30 UTC),
            "a time inside the spring-forward gap uses the pre-gap EST offset"
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
