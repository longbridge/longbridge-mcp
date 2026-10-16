use longbridge::Market;
use longbridge::quote::{
    AdjustType, CalcIndex, FilterWarrantExpiryDate, FilterWarrantInOutBoundsType, Period,
    SecurityListCategory, SortOrderType, TradeSessions, WarrantSortBy, WarrantStatus, WarrantType,
};
use rmcp::model::ErrorData as McpError;
use time::Date;

const DATE_FORMAT: &[time::format_description::BorrowedFormatItem<'_>] =
    time::macros::format_description!("[year]-[month]-[day]");

const PRIMITIVE_DATETIME_FORMAT: &[time::format_description::BorrowedFormatItem<'_>] =
    time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");

pub fn parse_period(s: &str) -> Result<Period, McpError> {
    match s.trim().to_lowercase().as_str() {
        "1m" => Ok(Period::OneMinute),
        "2m" => Ok(Period::TwoMinute),
        "3m" => Ok(Period::ThreeMinute),
        "5m" => Ok(Period::FiveMinute),
        "10m" => Ok(Period::TenMinute),
        "15m" => Ok(Period::FifteenMinute),
        "20m" => Ok(Period::TwentyMinute),
        "30m" => Ok(Period::ThirtyMinute),
        "45m" => Ok(Period::FortyFiveMinute),
        "60m" => Ok(Period::SixtyMinute),
        "120m" => Ok(Period::TwoHour),
        "180m" => Ok(Period::ThreeHour),
        "240m" => Ok(Period::FourHour),
        "day" => Ok(Period::Day),
        "week" => Ok(Period::Week),
        "month" => Ok(Period::Month),
        "quarter" => Ok(Period::Quarter),
        "year" => Ok(Period::Year),
        _ => Err(McpError::invalid_params(
            format!(
                "invalid period: '{s}', expected one of: 1m, 2m, 3m, 5m, 10m, 15m, 20m, 30m, \
                 45m, 60m, 120m, 180m, 240m, day, week, month, quarter, year"
            ),
            None,
        )),
    }
}

pub fn parse_trade_sessions(s: &str) -> Result<TradeSessions, McpError> {
    match s.trim().to_lowercase().as_str() {
        "intraday" => Ok(TradeSessions::Intraday),
        "all" => Ok(TradeSessions::All),
        _ => Err(McpError::invalid_params(
            format!("invalid trade_sessions: '{s}', expected 'intraday' or 'all'"),
            None,
        )),
    }
}

pub fn parse_market(s: &str) -> Result<Market, McpError> {
    s.parse::<Market>()
        .map_err(|e| McpError::invalid_params(format!("invalid market: {s} ({e})"), None))
}

pub fn parse_date(s: &str) -> Result<Date, McpError> {
    Date::parse(s, DATE_FORMAT)
        .map_err(|e| McpError::invalid_params(format!("invalid date '{s}': {e}"), None))
}

/// Parse the start of a time range: an RFC3339 datetime, or a bare
/// `yyyy-mm-dd` date read as the start of that day in UTC.
pub fn parse_rfc3339(s: &str) -> Result<time::OffsetDateTime, McpError> {
    parse_datetime_or_date(s, time::Time::MIDNIGHT)
}

/// Parse the end of a time range: an RFC3339 datetime, or a bare
/// `yyyy-mm-dd` date read as the end of that day in UTC, so the day is
/// included.
pub fn parse_rfc3339_end(s: &str) -> Result<time::OffsetDateTime, McpError> {
    parse_datetime_or_date(s, time::macros::time!(23:59:59))
}

fn parse_datetime_or_date(
    s: &str,
    time_of_day: time::Time,
) -> Result<time::OffsetDateTime, McpError> {
    let s = s.trim();
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .or_else(|e| {
            Date::parse(s, DATE_FORMAT)
                .map(|d| d.with_time(time_of_day).assume_utc())
                .map_err(|_| e)
        })
        .map_err(|e| {
            McpError::invalid_params(
                format!(
                    "invalid datetime '{s}': {e}; expected RFC3339 such as \
                     2026-09-01T00:00:00Z, or a date such as 2026-09-01"
                ),
                None,
            )
        })
}

pub fn parse_primitive_datetime(s: &str) -> Result<time::PrimitiveDateTime, McpError> {
    time::PrimitiveDateTime::parse(s, PRIMITIVE_DATETIME_FORMAT).map_err(|e| {
        McpError::invalid_params(
            format!("invalid datetime '{s}': {e}, expected format: yyyy-mm-ddTHH:MM:SS"),
            None,
        )
    })
}

pub fn parse_adjust_type(forward_adjust: bool) -> AdjustType {
    if forward_adjust {
        AdjustType::ForwardAdjust
    } else {
        AdjustType::NoAdjust
    }
}

pub fn parse_warrant_sort_by(s: &str) -> Result<WarrantSortBy, McpError> {
    match s {
        "LastDone" => Ok(WarrantSortBy::LastDone),
        "ChangeRate" => Ok(WarrantSortBy::ChangeRate),
        "ChangeValue" => Ok(WarrantSortBy::ChangeValue),
        "Volume" => Ok(WarrantSortBy::Volume),
        "Turnover" => Ok(WarrantSortBy::Turnover),
        "ExpiryDate" => Ok(WarrantSortBy::ExpiryDate),
        "StrikePrice" => Ok(WarrantSortBy::StrikePrice),
        "UpperStrikePrice" => Ok(WarrantSortBy::UpperStrikePrice),
        "LowerStrikePrice" => Ok(WarrantSortBy::LowerStrikePrice),
        "OutstandingQuantity" => Ok(WarrantSortBy::OutstandingQuantity),
        "OutstandingRatio" => Ok(WarrantSortBy::OutstandingRatio),
        "Premium" => Ok(WarrantSortBy::Premium),
        "ItmOtm" => Ok(WarrantSortBy::ItmOtm),
        "ImpliedVolatility" => Ok(WarrantSortBy::ImpliedVolatility),
        "Delta" => Ok(WarrantSortBy::Delta),
        _ => Err(McpError::invalid_params(
            format!("invalid sort_by: {s}"),
            None,
        )),
    }
}

pub fn parse_sort_order_type(s: &str) -> Result<SortOrderType, McpError> {
    match s {
        "Ascending" => Ok(SortOrderType::Ascending),
        "Descending" => Ok(SortOrderType::Descending),
        _ => Err(McpError::invalid_params(
            format!("invalid sort_order: {s}, expected 'Ascending' or 'Descending'"),
            None,
        )),
    }
}

/// Every calc index `calc_indexes` accepts, by its canonical name.
const CALC_INDEXES: &[(&str, CalcIndex)] = &[
    ("LastDone", CalcIndex::LastDone),
    ("ChangeValue", CalcIndex::ChangeValue),
    ("ChangeRate", CalcIndex::ChangeRate),
    ("Volume", CalcIndex::Volume),
    ("Turnover", CalcIndex::Turnover),
    ("YtdChangeRate", CalcIndex::YtdChangeRate),
    ("TurnoverRate", CalcIndex::TurnoverRate),
    ("TotalMarketValue", CalcIndex::TotalMarketValue),
    ("CapitalFlow", CalcIndex::CapitalFlow),
    ("Amplitude", CalcIndex::Amplitude),
    ("VolumeRatio", CalcIndex::VolumeRatio),
    ("PeTtmRatio", CalcIndex::PeTtmRatio),
    ("PbRatio", CalcIndex::PbRatio),
    ("DividendRatioTtm", CalcIndex::DividendRatioTtm),
    ("FiveDayChangeRate", CalcIndex::FiveDayChangeRate),
    ("TenDayChangeRate", CalcIndex::TenDayChangeRate),
    ("HalfYearChangeRate", CalcIndex::HalfYearChangeRate),
    ("FiveMinutesChangeRate", CalcIndex::FiveMinutesChangeRate),
    ("ExpiryDate", CalcIndex::ExpiryDate),
    ("StrikePrice", CalcIndex::StrikePrice),
    ("UpperStrikePrice", CalcIndex::UpperStrikePrice),
    ("LowerStrikePrice", CalcIndex::LowerStrikePrice),
    ("OutstandingQty", CalcIndex::OutstandingQty),
    ("OutstandingRatio", CalcIndex::OutstandingRatio),
    ("Premium", CalcIndex::Premium),
    ("ItmOtm", CalcIndex::ItmOtm),
    ("ImpliedVolatility", CalcIndex::ImpliedVolatility),
    ("WarrantDelta", CalcIndex::WarrantDelta),
    ("CallPrice", CalcIndex::CallPrice),
    ("ToCallPrice", CalcIndex::ToCallPrice),
    ("EffectiveLeverage", CalcIndex::EffectiveLeverage),
    ("LeverageRatio", CalcIndex::LeverageRatio),
    ("ConversionRatio", CalcIndex::ConversionRatio),
    ("BalancePoint", CalcIndex::BalancePoint),
    ("OpenInterest", CalcIndex::OpenInterest),
    ("Delta", CalcIndex::Delta),
    ("Gamma", CalcIndex::Gamma),
    ("Theta", CalcIndex::Theta),
    ("Vega", CalcIndex::Vega),
    ("Rho", CalcIndex::Rho),
];

/// Parse a calc index name. Case, `_`, `-` and spaces are ignored, so
/// "pe_ttm_ratio" and "peTtmRatio" both mean `PeTtmRatio`.
pub fn parse_calc_index(s: &str) -> Result<CalcIndex, McpError> {
    let wanted: String = s
        .chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .collect::<String>()
        .to_lowercase();
    CALC_INDEXES
        .iter()
        .find(|(name, _)| name.to_lowercase() == wanted)
        .map(|(_, index)| *index)
        .ok_or_else(|| {
            let names: Vec<&str> = CALC_INDEXES.iter().map(|(name, _)| *name).collect();
            McpError::invalid_params(
                format!(
                    "invalid calc_index: '{s}', expected one of: {}",
                    names.join(", ")
                ),
                None,
            )
        })
}

pub fn parse_security_list_category(s: &str) -> Result<SecurityListCategory, McpError> {
    match s {
        "Overnight" => Ok(SecurityListCategory::Overnight),
        _ => Err(McpError::invalid_params(
            format!("invalid category: {s}, expected 'Overnight'"),
            None,
        )),
    }
}

pub fn parse_warrant_type(s: &str) -> Result<WarrantType, McpError> {
    match s {
        "Call" => Ok(WarrantType::Call),
        "Put" => Ok(WarrantType::Put),
        "Bull" => Ok(WarrantType::Bull),
        "Bear" => Ok(WarrantType::Bear),
        "Inline" => Ok(WarrantType::Inline),
        _ => Err(McpError::invalid_params(
            format!("invalid warrant_type: {s}, expected Call/Put/Bull/Bear/Inline"),
            None,
        )),
    }
}

pub fn parse_warrant_expiry_date(s: &str) -> Result<FilterWarrantExpiryDate, McpError> {
    match s {
        "LT_3" => Ok(FilterWarrantExpiryDate::LT_3),
        "Between_3_6" => Ok(FilterWarrantExpiryDate::Between_3_6),
        "Between_6_12" => Ok(FilterWarrantExpiryDate::Between_6_12),
        "GT_12" => Ok(FilterWarrantExpiryDate::GT_12),
        _ => Err(McpError::invalid_params(
            format!("invalid expiry_date: {s}, expected LT_3/Between_3_6/Between_6_12/GT_12"),
            None,
        )),
    }
}

pub fn parse_warrant_price_type(s: &str) -> Result<FilterWarrantInOutBoundsType, McpError> {
    match s {
        "In" => Ok(FilterWarrantInOutBoundsType::In),
        "Out" => Ok(FilterWarrantInOutBoundsType::Out),
        _ => Err(McpError::invalid_params(
            format!("invalid price_type: {s}, expected In/Out"),
            None,
        )),
    }
}

pub fn parse_warrant_status(s: &str) -> Result<WarrantStatus, McpError> {
    match s {
        "Suspend" => Ok(WarrantStatus::Suspend),
        "PrepareList" => Ok(WarrantStatus::PrepareList),
        "Normal" => Ok(WarrantStatus::Normal),
        _ => Err(McpError::invalid_params(
            format!("invalid status: {s}, expected Suspend/PrepareList/Normal"),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_index_names_ignore_case_and_separators() {
        for name in ["PeTtmRatio", "pe_ttm_ratio", "pettmratio", "Pe-Ttm-Ratio"] {
            assert!(
                matches!(parse_calc_index(name), Ok(CalcIndex::PeTtmRatio)),
                "{name} must parse as PeTtmRatio"
            );
        }
        let err = parse_calc_index("Beta").expect_err("Beta is not a calc index");
        assert!(
            err.message.contains("expected one of: LastDone"),
            "the error must list the valid names: {}",
            err.message
        );
    }

    #[test]
    fn range_bounds_accept_a_bare_date() {
        let start = parse_rfc3339("2026-09-01").expect("a date must parse as a start");
        let end = parse_rfc3339_end("2026-09-01").expect("a date must parse as an end");
        assert_eq!(
            start.unix_timestamp(),
            1_788_220_800,
            "start is 00:00:00 UTC"
        );
        assert_eq!(
            end.unix_timestamp() - start.unix_timestamp(),
            86_399,
            "end is the last second of the same day"
        );
        let full =
            parse_rfc3339_end("2026-09-01T08:00:00+08:00").expect("RFC3339 must still parse");
        assert_eq!(
            full.unix_timestamp(),
            start.unix_timestamp(),
            "an explicit offset is honored"
        );
        assert!(
            parse_rfc3339("yesterday").is_err(),
            "free text must still be rejected"
        );
    }

    #[test]
    fn period_is_case_insensitive_and_trims() {
        assert!(matches!(parse_period("Day").unwrap(), Period::Day));
        assert!(matches!(
            parse_period(" 15M ").unwrap(),
            Period::FifteenMinute
        ));
        assert!(matches!(parse_period("YEAR").unwrap(), Period::Year));
    }

    #[test]
    fn period_error_lists_the_accepted_values() {
        let err = parse_period("daily").unwrap_err();
        assert!(
            err.message.contains("day") && err.message.contains("week"),
            "error should list accepted periods, got: {}",
            err.message
        );
    }

    #[test]
    fn trade_sessions_is_case_insensitive_and_trims() {
        assert!(matches!(
            parse_trade_sessions("All").unwrap(),
            TradeSessions::All
        ));
        assert!(matches!(
            parse_trade_sessions(" INTRADAY ").unwrap(),
            TradeSessions::Intraday
        ));
    }

    #[test]
    fn trade_sessions_rejects_anything_else() {
        assert!(parse_trade_sessions("premarket").is_err());
    }
}
