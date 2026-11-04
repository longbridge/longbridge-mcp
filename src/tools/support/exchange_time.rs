//! Render instants in the local time of the exchange a symbol trades on.
//!
//! Daily-and-longer series are stamped at local midnight of the trading day. In
//! UTC that is the *previous* calendar day for Asian exchanges (a Hong Kong bar
//! for 2026-10-09 is `2026-10-08T16:00:00Z`), so a consumer that reads the date
//! prefix of the timestamp — a model, or chart code slicing the string —
//! reports the wrong trading day. Extended-hours US minutes have the mirror
//! problem: 20:00 New York is already the next day in UTC. Emitting the
//! exchange's own UTC offset keeps the same instant while making the date
//! prefix the date at the exchange.
//!
//! Which tool fields are rewritten is listed in [`SPECS`]. Only fields verified
//! to be instants or local-midnight stamps belong there: a date stored as UTC
//! midnight (`forecast_eps`) would be shifted to the wrong day by this.

use rmcp::model::CallToolResult;
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};
use time_tz::timezones::db::america::NEW_YORK;
use time_tz::timezones::db::asia::TOKYO;
use time_tz::timezones::db::australia::SYDNEY;
use time_tz::timezones::db::europe::{BERLIN, LONDON};
use time_tz::{OffsetDateTimeExt, Tz};

/// Mainland China, Hong Kong and Singapore exchanges all sit at UTC+8 year round.
const UTC_PLUS_8: UtcOffset = match UtcOffset::from_hms(8, 0, 0) {
    Ok(offset) => offset,
    Err(_) => panic!("UTC+8 is a valid offset"),
};

/// The clock of one market.
enum Zone {
    /// A fixed UTC offset, for markets that do not observe daylight saving.
    Fixed(UtcOffset),
    /// A named zone, so EST/EDT, CET/CEST and the like follow the calendar.
    Named(&'static Tz),
}

/// The clock of the market `id` names: a symbol (`700.HK`, `600519.SH`,
/// `AAPL.US`, chosen by its market suffix) or a bare market code (`HK`, `CN`,
/// `US`, `JP`, `UK`, `DE`, `AU`). `None` for any other market, or an id without
/// one.
fn zone_of(id: &str) -> Option<Zone> {
    let market = id.rsplit_once('.').map_or(id, |(_, market)| market);
    match market.to_ascii_uppercase().as_str() {
        "HK" | "SH" | "SZ" | "BJ" | "SG" | "CN" => Some(Zone::Fixed(UTC_PLUS_8)),
        "US" => Some(Zone::Named(NEW_YORK)),
        "JP" => Some(Zone::Named(TOKYO)),
        "UK" => Some(Zone::Named(LONDON)),
        "DE" => Some(Zone::Named(BERLIN)),
        "AU" => Some(Zone::Named(SYDNEY)),
        _ => None,
    }
}

/// `instant` expressed in the local time of the market `id` names (see
/// [`zone_of`]). A market with no known clock is returned unchanged.
pub fn to_exchange_time(instant: OffsetDateTime, id: &str) -> OffsetDateTime {
    match zone_of(id) {
        Some(Zone::Fixed(offset)) => instant.to_offset(offset),
        Some(Zone::Named(tz)) => instant.to_timezone(tz),
        None => instant,
    }
}

/// Where the market of a spec's entries comes from.
enum Id {
    /// The symbol the caller asked for.
    Param,
    /// The first of these fields of each entry that holds a symbol or market
    /// code (dotted path).
    Fields(&'static [&'static str]),
}

/// One localization rule: in `tool`'s output, visit every entry under
/// `entries` (dotted path, `*` matches every array element or object value, an
/// empty path is the whole output) and rewrite the RFC3339 strings at `fields`
/// (paths relative to the entry) into the exchange time named by `id`.
struct Spec {
    tool: &'static str,
    entries: &'static str,
    id: Id,
    fields: &'static [&'static str],
}

/// A tool whose output is one series for the requested symbol.
const fn for_symbol(tool: &'static str, fields: &'static [&'static str]) -> Spec {
    Spec {
        tool,
        entries: "",
        id: Id::Param,
        fields,
    }
}

/// A tool whose output holds entries that each name their own market.
const fn per_entry(
    tool: &'static str,
    entries: &'static str,
    id: &'static [&'static str],
    fields: &'static [&'static str],
) -> Spec {
    Spec {
        tool,
        entries,
        id: Id::Fields(id),
        fields,
    }
}

const SPECS: &[Spec] = &[
    // Series for the requested symbol.
    for_symbol("candlesticks", &["*.timestamp"]),
    for_symbol("intraday", &["*.timestamp"]),
    for_symbol("trades", &["*.timestamp"]),
    for_symbol("capital_flow", &["*.timestamp"]),
    for_symbol("capital_distribution", &["timestamp"]),
    for_symbol("short_positions", &["data.*.timestamp"]),
    for_symbol("short_trades", &["data.*.timestamp"]),
    for_symbol("option_volume_daily", &["stats.*.timestamp"]),
    // Keyed by market code (`HK`, `US`, ...) rather than a symbol.
    for_symbol("market_temperature", &["timestamp"]),
    for_symbol("history_market_temperature", &["list.*.timestamp"]),
    for_symbol("ah_premium", &["klines.*.timestamp"]),
    for_symbol("ah_premium_intraday", &["klines.*.timestamp"]),
    for_symbol("valuation", &["metrics.pe.list.*.timestamp"]),
    for_symbol(
        "valuation_history",
        &["history.metrics.pe.list.*.timestamp"],
    ),
    for_symbol(
        "valuation_rank",
        &[
            "pe.*.timestamp",
            "pb.*.timestamp",
            "ps.*.timestamp",
            "dvd.*.timestamp",
        ],
    ),
    for_symbol("institutional_views", &["elist.*.date", "tlist.*.date"]),
    for_symbol(
        "trade_stats",
        &["statistics.timestamp", "statistics.trade_date.*"],
    ),
    // Entries that carry their own symbol or market.
    per_entry(
        "quote",
        "*",
        &["symbol"],
        &[
            "timestamp",
            "pre_market.timestamp",
            "post_market.timestamp",
            "overnight.timestamp",
        ],
    ),
    per_entry("option_quote", "*", &["symbol"], &["timestamp"]),
    per_entry("warrant_quote", "*", &["symbol"], &["timestamp"]),
    per_entry(
        "valuation_comparison",
        "list.*",
        &["symbol"],
        &["history.*.date"],
    ),
    per_entry(
        "industry_valuation",
        "list.*",
        &["symbol"],
        &["history.*.date"],
    ),
    per_entry("anomaly", "changes.*", &["symbol"], &["alert_time"]),
    per_entry("top_movers", "events.*", &["stock.symbol"], &["timestamp"]),
    per_entry(
        "market_status",
        "market_time.*",
        &["market"],
        &["timestamp", "delay_timestamp"],
    ),
    // `finance_calendar` depends on the category: earnings and macro releases
    // are real instants and closed days are local-midnight stamps, but dividend
    // (and split/ipo) dates are UTC midnight, which a local offset would push
    // to the previous day — so those categories have no rule.
    per_entry(
        "finance_calendar:report",
        "list.*.infos.*",
        &["symbol", "market"],
        &["datetime"],
    ),
    per_entry(
        "finance_calendar:macrodata",
        "list.*.infos.*",
        &["symbol", "market"],
        &["datetime"],
    ),
    per_entry(
        "finance_calendar:closed",
        "list.*.infos.*",
        &["symbol", "market"],
        &["datetime"],
    ),
];

/// Rewrite the timestamps of `tool`'s output `value` into exchange time, per
/// [`SPECS`]. `symbol` is the symbol the caller requested, for tools whose
/// output is one series. A tool with no rule, or an entry whose market cannot be
/// determined, is left untouched.
pub fn localize_tool_output(tool: &str, value: &mut Value, symbol: Option<&str>) {
    for spec in SPECS.iter().filter(|spec| spec.tool == tool) {
        for_each_mut(value, spec.entries, &mut |entry| {
            let id = match &spec.id {
                Id::Param => symbol.map(str::to_owned),
                // The first field that names a market we know the clock of: a
                // symbol without a suffix must not shadow the `market` field.
                Id::Fields(keys) => keys.iter().find_map(|key| {
                    get_path(entry, key)
                        .and_then(Value::as_str)
                        .filter(|id| zone_of(id).is_some())
                        .map(str::to_owned)
                }),
            };
            let Some(id) = id else {
                return;
            };
            for field in spec.fields {
                for_each_mut(entry, field, &mut |leaf| localize_leaf(leaf, &id));
            }
        });
    }
}

/// [`localize_tool_output`] over a tool result whose text is JSON. A result that
/// is not a JSON text is returned unchanged.
pub fn localize_result(tool: &str, result: CallToolResult, symbol: Option<&str>) -> CallToolResult {
    let Some(mut value) = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .and_then(|t| serde_json::from_str::<Value>(&t.text).ok())
    else {
        return result;
    };
    localize_tool_output(tool, &mut value, symbol);
    match serde_json::to_string(&value) {
        Ok(json) => crate::tools::tool_result(json),
        Err(_) => result,
    }
}

/// Rewrite `leaf` in place when it is an RFC3339 string.
fn localize_leaf(leaf: &mut Value, id: &str) {
    let Some(local) = leaf
        .as_str()
        .and_then(|s| OffsetDateTime::parse(s, &Rfc3339).ok())
        .map(|instant| to_exchange_time(instant, id))
        .and_then(|local| local.format(&Rfc3339).ok())
    else {
        return;
    };
    *leaf = Value::String(local);
}

/// The value at the dotted object path `path`, if every step exists.
fn get_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(value, |value, key| value.get(key))
}

/// Call `visit` on every value `path` reaches under `value`. `*` steps over
/// every array element or object value; a step that does not exist reaches
/// nothing.
fn for_each_mut(value: &mut Value, path: &str, visit: &mut dyn FnMut(&mut Value)) {
    if path.is_empty() {
        visit(value);
        return;
    }
    let (step, rest) = path.split_once('.').unwrap_or((path, ""));
    match value {
        Value::Array(items) if step == "*" => {
            for item in items {
                for_each_mut(item, rest, visit);
            }
        }
        Value::Object(map) if step == "*" => {
            for item in map.values_mut() {
                for_each_mut(item, rest, visit);
            }
        }
        Value::Object(map) => {
            if let Some(item) = map.get_mut(step) {
                for_each_mut(item, rest, visit);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn localized(tool: &str, mut value: Value, symbol: Option<&str>) -> Value {
        localize_tool_output(tool, &mut value, symbol);
        value
    }

    #[test]
    fn hk_daily_bar_date_prefix_is_the_trading_date() {
        // 2026-11-03 trading day, stamped at local midnight.
        let out = localized(
            "candlesticks",
            json!([{ "timestamp": "2026-11-02T16:00:00Z", "close": "1" }]),
            Some("HSI.HK"),
        );
        assert_eq!(out[0]["timestamp"], "2026-11-03T00:00:00+08:00");
    }

    #[test]
    fn mainland_and_singapore_use_utc_plus_8() {
        for symbol in ["600519.SH", "000001.SZ", "830799.BJ", "D05.SG"] {
            let out = localized(
                "candlesticks",
                json!([{ "timestamp": "2026-11-02T16:00:00Z" }]),
                Some(symbol),
            );
            assert_eq!(out[0]["timestamp"], "2026-11-03T00:00:00+08:00", "{symbol}");
        }
    }

    #[test]
    fn us_follows_new_york_daylight_saving() {
        // EDT (UTC-4) before the 2026-11-01 fall-back, EST (UTC-5) after it.
        let out = localized(
            "candlesticks",
            json!([
                { "timestamp": "2026-10-30T04:00:00Z" },
                { "timestamp": "2026-11-03T05:00:00Z" },
            ]),
            Some("AAPL.US"),
        );
        assert_eq!(out[0]["timestamp"], "2026-10-30T00:00:00-04:00");
        assert_eq!(out[1]["timestamp"], "2026-11-03T00:00:00-05:00");
    }

    #[test]
    fn us_after_hours_minute_keeps_its_new_york_date() {
        // 20:00 ET on 2026-11-03 is already 2026-11-04 in UTC.
        let out = localized(
            "intraday",
            json!([{ "timestamp": "2026-11-04T01:00:00Z" }]),
            Some("AAPL.US"),
        );
        assert_eq!(out[0]["timestamp"], "2026-11-03T20:00:00-05:00");
    }

    #[test]
    fn symbol_market_is_case_insensitive_and_unknown_markets_stay_utc() {
        let stamp = json!([{ "timestamp": "2026-11-02T16:00:00Z" }]);
        let out = localized("candlesticks", stamp.clone(), Some("hsi.hk"));
        assert_eq!(out[0]["timestamp"], "2026-11-03T00:00:00+08:00");
        for symbol in ["XYZ.ZZ", "700", ""] {
            assert_eq!(
                localized("candlesticks", stamp.clone(), Some(symbol)),
                stamp,
                "{symbol}"
            );
        }
        // No symbol to go by.
        assert_eq!(localized("candlesticks", stamp.clone(), None), stamp);
    }

    #[test]
    fn instant_is_preserved() {
        let original = OffsetDateTime::parse("2026-11-02T16:00:00Z", &Rfc3339).unwrap();
        for id in [
            "700.HK",
            "AAPL.US",
            "600519.SH",
            "CN",
            "JP",
            "UK",
            "DE",
            "AU",
            "XYZ",
        ] {
            assert_eq!(to_exchange_time(original, id), original, "{id}");
        }
    }

    #[test]
    fn malformed_values_are_left_alone() {
        let before = json!([
            { "timestamp": "not a time" },
            { "close": "1" },
            { "timestamp": 12345 },
            { "timestamp": null },
            "scalar",
        ]);
        assert_eq!(
            localized("candlesticks", before.clone(), Some("700.HK")),
            before
        );
        let object = json!({ "timestamp": "2026-11-02T16:00:00Z" });
        assert_eq!(
            localized("candlesticks", object.clone(), Some("700.HK")),
            object
        );
        // A tool with no rule is never touched: `forecast_eps` dates are UTC
        // midnight, which a local offset would push to the wrong time of day.
        let other = json!({ "items": [{ "forecast_start_date": "2026-10-27T00:00:00Z" }] });
        assert_eq!(
            localized("forecast_eps", other.clone(), Some("700.HK")),
            other
        );
    }

    #[test]
    fn quote_follows_each_entrys_own_market() {
        let out = localized(
            "quote",
            json!([
                {
                    "symbol": "AAPL.US",
                    "timestamp": "2026-11-03T21:00:01Z",
                    "pre_market": null,
                    "post_market": { "timestamp": "2026-11-04T00:59:53Z", "last_done": "1" },
                    "overnight": { "timestamp": "2026-11-04T06:29:06Z" },
                },
                { "symbol": "700.HK", "timestamp": "2026-11-04T06:29:29Z" },
                { "timestamp": "2026-11-04T06:29:29Z" },
            ]),
            None,
        );
        assert_eq!(out[0]["timestamp"], "2026-11-03T16:00:01-05:00");
        assert_eq!(
            out[0]["post_market"]["timestamp"],
            "2026-11-03T19:59:53-05:00"
        );
        assert_eq!(
            out[0]["overnight"]["timestamp"],
            "2026-11-04T01:29:06-05:00"
        );
        assert_eq!(out[0]["pre_market"], Value::Null);
        assert_eq!(out[1]["timestamp"], "2026-11-04T14:29:29+08:00");
        // No symbol, no market: left in UTC.
        assert_eq!(out[2]["timestamp"], "2026-11-04T06:29:29Z");
    }

    /// Each HK series stamps its days at Hong Kong midnight (live samples).
    #[test]
    fn hk_daily_series_report_the_hong_kong_date() {
        let midnight = "2026-11-02T16:00:00Z";
        let local = "2026-11-03T00:00:00+08:00";
        let cases = [
            (
                "short_positions",
                json!({ "data": [{ "timestamp": midnight }] }),
            ),
            (
                "short_trades",
                json!({ "data": [{ "timestamp": midnight }] }),
            ),
            (
                "ah_premium",
                json!({ "klines": [{ "timestamp": midnight }] }),
            ),
            (
                "valuation",
                json!({ "metrics": { "pe": { "list": [{ "timestamp": midnight }] } } }),
            ),
            (
                "valuation_history",
                json!({ "history": { "metrics": { "pe": { "list": [{ "timestamp": midnight }] } } } }),
            ),
            (
                "valuation_rank",
                json!({ "pe": [{ "timestamp": midnight }], "dvd": [{ "timestamp": midnight }] }),
            ),
            (
                "institutional_views",
                json!({ "elist": [{ "date": midnight }], "tlist": [{ "date": midnight }] }),
            ),
            (
                "trade_stats",
                json!({ "statistics": { "trade_date": [midnight] } }),
            ),
        ];
        for (tool, value) in cases {
            let out = localized(tool, value, Some("700.HK")).to_string();
            assert!(out.contains(local), "{tool}: {out}");
            assert!(!out.contains(midnight), "{tool} left a UTC stamp: {out}");
        }
        // The instant next to the dates is converted too, to the same exchange.
        let stats = localized(
            "trade_stats",
            json!({ "statistics": { "timestamp": "2026-11-04T06:35:19Z" } }),
            Some("700.HK"),
        );
        assert_eq!(
            stats["statistics"]["timestamp"],
            "2026-11-04T14:35:19+08:00"
        );
    }

    #[test]
    fn market_temperature_follows_the_requested_market_code() {
        // Live daily points: HK/CN/SG at `16:00Z`, US at New York midnight.
        let history = |stamp: &str| json!({ "list": [{ "timestamp": stamp }] });
        for market in ["HK", "CN", "SG"] {
            let out = localized(
                "history_market_temperature",
                history("2026-11-02T16:00:00Z"),
                Some(market),
            );
            assert_eq!(
                out["list"][0]["timestamp"], "2026-11-03T00:00:00+08:00",
                "{market}"
            );
        }
        let out = localized(
            "history_market_temperature",
            history("2026-11-02T05:00:00Z"),
            Some("US"),
        );
        assert_eq!(out["list"][0]["timestamp"], "2026-11-02T00:00:00-05:00");
        let now = json!({ "timestamp": "2026-11-04T06:50:00Z" });
        let out = localized("market_temperature", now, Some("CN"));
        assert_eq!(out["timestamp"], "2026-11-04T14:50:00+08:00");
    }

    /// US FINRA series are stamped at New York midnight (live `04:00Z`/`05:00Z`),
    /// so converting them lands on the record date rather than the day before.
    #[test]
    fn us_short_series_keep_their_record_date() {
        for tool in ["short_positions", "short_trades"] {
            let out = localized(
                tool,
                json!({ "data": [
                    { "timestamp": "2026-10-29T04:00:00Z" },
                    { "timestamp": "2026-11-03T05:00:00Z" },
                ] }),
                Some("AAPL.US"),
            );
            assert_eq!(
                out["data"][0]["timestamp"], "2026-10-29T00:00:00-04:00",
                "{tool}"
            );
            assert_eq!(
                out["data"][1]["timestamp"], "2026-11-03T00:00:00-05:00",
                "{tool}"
            );
        }
    }

    #[test]
    fn capital_distribution_no_data_sentinel_is_not_a_date() {
        // A symbol with no capital-flow data comes back stamped "0".
        let none = json!({ "timestamp": "0", "data_available": false });
        assert_eq!(
            localized("capital_distribution", none.clone(), Some("AAPL.US")),
            none
        );
    }

    #[test]
    fn peer_tables_follow_each_peers_market() {
        for tool in ["valuation_comparison", "industry_valuation"] {
            let out = localized(
                tool,
                json!({ "list": [
                    { "symbol": "700.HK", "history": [{ "date": "2026-10-31T16:00:00Z" }] },
                    { "symbol": "AAPL.US", "history": [{ "date": "2026-10-31T04:00:00Z" }] },
                ] }),
                Some("700.HK"),
            );
            assert_eq!(
                out["list"][0]["history"][0]["date"],
                "2026-11-01T00:00:00+08:00"
            );
            assert_eq!(
                out["list"][1]["history"][0]["date"],
                "2026-10-31T00:00:00-04:00"
            );
        }
    }

    #[test]
    fn event_feeds_follow_each_entrys_market() {
        let out = localized(
            "anomaly",
            json!({ "changes": [{ "symbol": "9987.HK", "alert_time": "2026-11-04T06:35:35Z" }] }),
            None,
        );
        assert_eq!(out["changes"][0]["alert_time"], "2026-11-04T14:35:35+08:00");

        let out = localized(
            "top_movers",
            json!({ "events": [{ "stock": { "symbol": "AAPL.US" }, "timestamp": "2026-11-04T01:30:00Z" }] }),
            None,
        );
        assert_eq!(out["events"][0]["timestamp"], "2026-11-03T20:30:00-05:00");

        let out = localized(
            "market_status",
            json!({ "market_time": [
                { "market": "US", "timestamp": "2026-11-04T06:35:36Z" },
                { "market": "CN", "timestamp": "2026-11-04T06:35:36Z" },
                { "market": "KR", "timestamp": "2026-11-04T06:35:36Z" },
            ] }),
            None,
        );
        assert_eq!(
            out["market_time"][0]["timestamp"],
            "2026-11-04T01:35:36-05:00"
        );
        assert_eq!(
            out["market_time"][1]["timestamp"],
            "2026-11-04T14:35:36+08:00"
        );
        assert_eq!(out["market_time"][2]["timestamp"], "2026-11-04T06:35:36Z");
    }

    #[test]
    fn finance_calendar_converts_instants_and_closed_days_by_market() {
        let out = localized(
            "finance_calendar:report",
            json!({ "list": [{ "date": "2026-11-04", "infos": [
                { "symbol": "1785.HK", "market": "HK", "datetime": "2026-11-04T01:30:00Z" },
                { "symbol": "AXICY.US", "market": "US", "datetime": "2026-11-04T20:00:00Z" },
                { "symbol": "", "market": "US", "datetime": "2026-11-04T13:30:00Z" },
                { "market": "KR", "datetime": "2026-11-04T13:30:00Z" },
            ] }] }),
            None,
        );
        let infos = &out["list"][0]["infos"];
        assert_eq!(infos[0]["datetime"], "2026-11-04T09:30:00+08:00");
        assert_eq!(infos[1]["datetime"], "2026-11-04T15:00:00-05:00");
        // An empty symbol is skipped in favour of the market code.
        assert_eq!(infos[2]["datetime"], "2026-11-04T08:30:00-05:00");
        // A market with no rule stays in UTC.
        assert_eq!(infos[3]["datetime"], "2026-11-04T13:30:00Z");

        // Closed days are local-midnight stamps (live: HK/SG/CN `16:00Z`, US `05:00Z`).
        let out = localized(
            "finance_calendar:closed",
            json!({ "list": [{ "infos": [
                { "market": "HK", "datetime": "2026-12-24T16:00:00Z" },
                { "market": "CN", "datetime": "2026-09-30T16:00:00Z" },
                { "market": "US", "datetime": "2026-11-26T05:00:00Z" },
            ] }] }),
            None,
        );
        let infos = &out["list"][0]["infos"];
        assert_eq!(infos[0]["datetime"], "2026-12-25T00:00:00+08:00");
        assert_eq!(infos[1]["datetime"], "2026-10-01T00:00:00+08:00");
        assert_eq!(infos[2]["datetime"], "2026-11-26T00:00:00-05:00");

        // Other markets stamp their closed days at their own local midnight.
        let out = localized(
            "finance_calendar:closed",
            json!({ "list": [{ "infos": [
                { "market": "JP", "datetime": "2026-11-02T15:00:00Z" },
                { "market": "UK", "datetime": "2026-12-25T00:00:00Z" },
                { "market": "DE", "datetime": "2026-12-24T23:00:00Z" },
                { "market": "AU", "datetime": "2026-12-24T13:00:00Z" },
            ] }] }),
            None,
        );
        let infos = &out["list"][0]["infos"];
        assert_eq!(infos[0]["datetime"], "2026-11-03T00:00:00+09:00");
        assert_eq!(infos[1]["datetime"], "2026-12-25T00:00:00Z");
        assert_eq!(infos[2]["datetime"], "2026-12-25T00:00:00+01:00");
        assert_eq!(infos[3]["datetime"], "2026-12-25T00:00:00+11:00");
    }

    #[test]
    fn entry_market_falls_back_when_the_symbol_names_none() {
        // `BRK` has no market suffix, so the `market` field decides.
        let out = localized(
            "finance_calendar:report",
            json!({ "list": [{ "infos": [
                { "symbol": "BRK", "market": "HK", "datetime": "2026-11-04T01:30:00Z" },
                { "symbol": "700", "market": "US", "datetime": "2026-11-04T13:30:00Z" },
            ] }] }),
            None,
        );
        let infos = &out["list"][0]["infos"];
        assert_eq!(infos[0]["datetime"], "2026-11-04T09:30:00+08:00");
        assert_eq!(infos[1]["datetime"], "2026-11-04T08:30:00-05:00");
    }

    #[test]
    fn finance_calendar_dividend_dates_are_left_alone() {
        // Dividend dates are UTC midnight, not a local-midnight stamp: shifting
        // them to New York would name the previous day.
        let dividends = json!({ "list": [{ "infos": [
            { "symbol": "EQT.US", "market": "US", "datetime": "2026-11-04T00:00:00Z" },
            { "symbol": "1336.HK", "market": "HK", "datetime": "2026-11-04T00:00:00Z" },
        ] }] });
        for tool in [
            "finance_calendar:dividend",
            "finance_calendar:split",
            "finance_calendar",
        ] {
            assert_eq!(
                localized(tool, dividends.clone(), None),
                dividends,
                "{tool}"
            );
        }
    }

    /// The tool name at each call site is a plain string, so a typo or a rename
    /// would silently stop localizing. Keep call sites and [`SPECS`] in step.
    #[test]
    fn call_sites_and_specs_name_the_same_tools() {
        let sources = [
            include_str!("../quote.rs"),
            include_str!("../market.rs"),
            include_str!("../fundamental.rs"),
            include_str!("../calendar.rs"),
        ];
        let mut wired = std::collections::BTreeSet::new();
        for source in sources {
            for call in ["localize_tool_output(", "localize_result("] {
                for (at, _) in source.match_indices(call) {
                    // rustfmt may move the name to its own line.
                    let rest = source[at + call.len()..].trim_start();
                    // A computed name (`&format!(..)`) is checked separately.
                    let Some(rest) = rest.strip_prefix('"') else {
                        continue;
                    };
                    let name = rest.split('"').next().expect("closing quote");
                    wired.insert(name.to_string());
                }
            }
        }
        // `finance_calendar` builds its name from the category.
        assert!(
            include_str!("../calendar.rs").contains("\"finance_calendar:{category}\""),
            "finance_calendar must pick its rule by category"
        );
        let specced: std::collections::BTreeSet<String> = SPECS
            .iter()
            .map(|spec| spec.tool)
            .filter(|tool| !tool.starts_with("finance_calendar:"))
            .map(str::to_string)
            .collect();
        assert_eq!(
            wired, specced,
            "tool names at localize call sites differ from SPECS"
        );
    }

    /// Tools that turn unix-second time fields into RFC3339 but are deliberately
    /// not rendered in exchange time, each with the reason.
    const NOT_LOCALIZED: &[(&str, &str)] = &[
        (
            "institution_rating",
            "analyst start/end dates are not verified as local-midnight stamps",
        ),
        (
            "institution_rating_detail",
            "`date` and `timestamp` disagree by a day upstream; the intended one is unclear",
        ),
        (
            "forecast_eps",
            "dates are UTC midnight; a local offset would name the previous day",
        ),
        (
            "ipo_calendar",
            "empty at review time, so its stamps could not be checked against live data",
        ),
        (
            "profit_analysis",
            "account-level times, not a market series",
        ),
    ];

    /// A tool that converts a unix time field must either be localized (in
    /// [`SPECS`]) or be listed in [`NOT_LOCALIZED`] with a reason, so a new tool
    /// cannot slip through with raw UTC stamps unnoticed.
    #[test]
    fn unix_time_tools_are_localized_or_explicitly_excluded() {
        let sources = [
            include_str!("../quote.rs"),
            include_str!("../market.rs"),
            include_str!("../fundamental.rs"),
            include_str!("../calendar.rs"),
            include_str!("../ipo.rs"),
            include_str!("../portfolio.rs"),
        ];
        let known: std::collections::BTreeSet<&str> = SPECS
            .iter()
            .map(|spec| spec.tool.split(':').next().expect("tool name"))
            .chain(NOT_LOCALIZED.iter().map(|(tool, _)| *tool))
            .collect();
        let time_words = ["timestamp", "_time", "datetime", "_date", "alert_time"];

        let mut unaccounted = Vec::new();
        for source in sources {
            // Test fixtures after the module are not tool code.
            let code = source.split("#[cfg(test)]").next().expect("source");
            for chunk in code.split("pub async fn ").skip(1) {
                let name = chunk.split('(').next().expect("fn name").trim();
                let converts_unix = chunk.contains("convert_unix_paths") || chunk.contains("_unix");
                let names_a_time_field = chunk
                    .split('"')
                    .skip(1)
                    .step_by(2)
                    .any(|literal| time_words.iter().any(|word| literal.contains(word)));
                if converts_unix && names_a_time_field && !known.contains(name) {
                    unaccounted.push(name.to_string());
                }
            }
        }
        assert!(
            unaccounted.is_empty(),
            "these tools convert a unix time field but are in neither SPECS nor NOT_LOCALIZED: {unaccounted:?}"
        );
    }

    #[test]
    fn localize_result_rewrites_json_text_and_ignores_the_rest() {
        let text =
            |s: &str| CallToolResult::success(vec![rmcp::model::Content::text(s.to_string())]);
        let read = |result: CallToolResult| {
            result
                .content
                .first()
                .and_then(|c| c.as_text())
                .map(|t| t.text.clone())
                .expect("text result")
        };
        let out = localize_result(
            "intraday",
            text(r#"[{"timestamp":"2026-11-04T01:00:00Z"}]"#),
            Some("AAPL.US"),
        );
        assert_eq!(read(out), r#"[{"timestamp":"2026-11-03T20:00:00-05:00"}]"#);
        assert_eq!(
            read(localize_result(
                "intraday",
                text("not json"),
                Some("AAPL.US")
            )),
            "not json"
        );
    }
}
