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
    match s {
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
            format!("invalid period: {s}"),
            None,
        )),
    }
}

pub fn parse_trade_sessions(s: &str) -> Result<TradeSessions, McpError> {
    match s {
        "intraday" => Ok(TradeSessions::Intraday),
        "all" => Ok(TradeSessions::All),
        _ => Err(McpError::invalid_params(
            format!("invalid trade_sessions: {s}, expected 'intraday' or 'all'"),
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

pub fn parse_rfc3339(s: &str) -> Result<time::OffsetDateTime, McpError> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map_err(|e| McpError::invalid_params(format!("invalid RFC3339 datetime '{s}': {e}"), None))
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

pub fn parse_calc_index(s: &str) -> Result<CalcIndex, McpError> {
    match s {
        "LastDone" => Ok(CalcIndex::LastDone),
        "ChangeValue" => Ok(CalcIndex::ChangeValue),
        "ChangeRate" => Ok(CalcIndex::ChangeRate),
        "Volume" => Ok(CalcIndex::Volume),
        "Turnover" => Ok(CalcIndex::Turnover),
        "YtdChangeRate" => Ok(CalcIndex::YtdChangeRate),
        "TurnoverRate" => Ok(CalcIndex::TurnoverRate),
        "TotalMarketValue" => Ok(CalcIndex::TotalMarketValue),
        "CapitalFlow" => Ok(CalcIndex::CapitalFlow),
        "Amplitude" => Ok(CalcIndex::Amplitude),
        "VolumeRatio" => Ok(CalcIndex::VolumeRatio),
        "PeTtmRatio" => Ok(CalcIndex::PeTtmRatio),
        "PbRatio" => Ok(CalcIndex::PbRatio),
        "DividendRatioTtm" => Ok(CalcIndex::DividendRatioTtm),
        "FiveDayChangeRate" => Ok(CalcIndex::FiveDayChangeRate),
        "TenDayChangeRate" => Ok(CalcIndex::TenDayChangeRate),
        "HalfYearChangeRate" => Ok(CalcIndex::HalfYearChangeRate),
        "FiveMinutesChangeRate" => Ok(CalcIndex::FiveMinutesChangeRate),
        "ExpiryDate" => Ok(CalcIndex::ExpiryDate),
        "StrikePrice" => Ok(CalcIndex::StrikePrice),
        "UpperStrikePrice" => Ok(CalcIndex::UpperStrikePrice),
        "LowerStrikePrice" => Ok(CalcIndex::LowerStrikePrice),
        "OutstandingQty" => Ok(CalcIndex::OutstandingQty),
        "OutstandingRatio" => Ok(CalcIndex::OutstandingRatio),
        "Premium" => Ok(CalcIndex::Premium),
        "ItmOtm" => Ok(CalcIndex::ItmOtm),
        "ImpliedVolatility" => Ok(CalcIndex::ImpliedVolatility),
        "WarrantDelta" => Ok(CalcIndex::WarrantDelta),
        "CallPrice" => Ok(CalcIndex::CallPrice),
        "ToCallPrice" => Ok(CalcIndex::ToCallPrice),
        "EffectiveLeverage" => Ok(CalcIndex::EffectiveLeverage),
        "LeverageRatio" => Ok(CalcIndex::LeverageRatio),
        "ConversionRatio" => Ok(CalcIndex::ConversionRatio),
        "BalancePoint" => Ok(CalcIndex::BalancePoint),
        "OpenInterest" => Ok(CalcIndex::OpenInterest),
        "Delta" => Ok(CalcIndex::Delta),
        "Gamma" => Ok(CalcIndex::Gamma),
        "Theta" => Ok(CalcIndex::Theta),
        "Vega" => Ok(CalcIndex::Vega),
        "Rho" => Ok(CalcIndex::Rho),
        _ => Err(McpError::invalid_params(
            format!("invalid calc_index: {s}"),
            None,
        )),
    }
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
