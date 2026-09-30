use longbridge::quote::{
    CalcIndex, RequestCreateWatchlistGroup, RequestUpdateWatchlistGroup, SecuritiesUpdateMode,
};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::http_client::{
    http_get_tool, http_get_tool_unix, http_get_tool_unix_dropping, http_post_raw,
    http_post_tool_reshape, http_post_value,
};
use crate::tools::support::parse;
use crate::tools::support::tolerant::{
    tolerant_bool, tolerant_i64, tolerant_option_usize, tolerant_option_vec_i32,
    tolerant_option_vec_string, tolerant_usize, tolerant_vec_string,
};
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolsParam {
    /// Security symbols, e.g. ["700.HK", "AAPL.US"]. Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    #[serde(deserialize_with = "tolerant_vec_string")]
    pub symbols: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OptionSymbolsParam {
    /// Option contract symbols, e.g. ["AAPL230317P160000.US"]. These are NOT
    /// plain stock symbols — get valid ones from the `symbol` field of each
    /// `option_chain_info_by_date` contract (after listing expiry dates with
    /// `option_chain_expiry_date_list`).
    #[serde(deserialize_with = "tolerant_vec_string")]
    pub symbols: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IntradayParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
    /// Trade sessions to include: "intraday" (default, regular hours only) or "all" (include pre-market and post-market).
    pub trade_sessions: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolCountParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
    /// Maximum number of results (max 1000)
    #[serde(deserialize_with = "tolerant_usize")]
    pub count: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CandlesticksParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year (default: day)
    #[serde(default = "default_candlestick_period")]
    pub period: String,
    /// Number of candlesticks (optional, max 1000; default 100)
    #[serde(
        default = "default_candlestick_count",
        deserialize_with = "tolerant_usize"
    )]
    pub count: usize,
    /// Whether to forward-adjust for splits/dividends (default: false / no adjust)
    #[serde(default, deserialize_with = "tolerant_bool")]
    pub forward_adjust: bool,
    /// Trade sessions: "intraday" (regular hours only) or "all" (include pre-market and post-market; default "all")
    #[serde(default = "default_trade_sessions")]
    pub trade_sessions: String,
}

fn default_candlestick_period() -> String {
    "day".to_string()
}

fn default_candlestick_count() -> usize {
    100
}

fn default_trade_sessions() -> String {
    "all".to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryCandlesticksByOffsetParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year (default: day)
    #[serde(default = "default_candlestick_period")]
    pub period: String,
    /// Whether to forward-adjust for splits/dividends (default: false / no adjust)
    #[serde(default, deserialize_with = "tolerant_bool")]
    pub forward_adjust: bool,
    /// Whether to query forward in time (true) or backward (false; default)
    #[serde(default, deserialize_with = "tolerant_bool")]
    pub forward: bool,
    /// Reference datetime (yyyy-mm-ddTHH:MM:SS), omit to start from latest
    pub time: Option<String>,
    /// Number of candlesticks (optional, max 1000; default 100)
    #[serde(
        default = "default_candlestick_count",
        deserialize_with = "tolerant_usize"
    )]
    pub count: usize,
    /// Trade sessions: "intraday" (regular hours only) or "all" (include pre-market and post-market; default "all")
    #[serde(default = "default_trade_sessions")]
    pub trade_sessions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryCandlesticksByDateParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
    /// Period: 1m, 5m, 15m, 30m, 60m, day, week, month, year (default: day)
    #[serde(default = "default_candlestick_period")]
    pub period: String,
    /// Whether to forward-adjust for splits/dividends (default: false / no adjust)
    #[serde(default, deserialize_with = "tolerant_bool")]
    pub forward_adjust: bool,
    /// Start date (yyyy-mm-dd), optional
    pub start: Option<String>,
    /// End date (yyyy-mm-dd), optional
    pub end: Option<String>,
    /// Trade sessions: "intraday" (regular hours only) or "all" (include pre-market and post-market; default "all")
    #[serde(default = "default_trade_sessions")]
    pub trade_sessions: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarketParam {
    /// Market code: HK, US, CN, SG
    pub market: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MarketDateRangeParam {
    /// Market code: HK, US, CN, SG
    pub market: String,
    /// Start date (yyyy-mm-dd)
    pub start: String,
    /// End date (yyyy-mm-dd)
    pub end: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OptionChainByDateParam {
    /// Underlying security symbol, e.g. "AAPL.US". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
    /// Expiry date (yyyy-mm-dd). Required — list the tradable ones with
    /// `option_chain_expiry_date_list`.
    pub date: String,
    /// Return standard contracts only. Omitted or false returns everything,
    /// including the legacy contracts left over from a corporate action
    /// (`standard_attr: "Old"`), which are rarely what a caller wants.
    pub standard_only: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WarrantListParam {
    /// Underlying symbol, e.g. "700.HK"
    pub symbol: String,
    /// Sort field: LastDone, ChangeRate, ChangeValue, Volume, Turnover, ExpiryDate, StrikePrice, UpperStrikePrice, LowerStrikePrice, OutstandingQuantity, OutstandingRatio, Premium, ItmOtm, ImpliedVolatility, Delta
    pub sort_by: String,
    /// Sort order: Ascending or Descending
    pub sort_order: String,
    /// Filter by warrant type (optional): "Call", "Put", "Bull", "Bear", "Inline"
    #[serde(default, deserialize_with = "tolerant_option_vec_string")]
    pub warrant_type: Option<Vec<String>>,
    /// Filter by issuer ID (optional), use issuer_id from warrant_issuers tool
    #[serde(default, deserialize_with = "tolerant_option_vec_i32")]
    pub issuer: Option<Vec<i32>>,
    /// Filter by expiry date range (optional): "LT_3" (<3 months), "Between_3_6" (3-6 months), "Between_6_12" (6-12 months), "GT_12" (>12 months)
    #[serde(default, deserialize_with = "tolerant_option_vec_string")]
    pub expiry_date: Option<Vec<String>>,
    /// Filter by in/out of bounds (optional): "In" (in bounds), "Out" (out of bounds). Only for Inline warrants.
    #[serde(default, deserialize_with = "tolerant_option_vec_string")]
    pub price_type: Option<Vec<String>>,
    /// Filter by status (optional): "Suspend" (suspended), "PrepareList" (pending listing), "Normal" (normal trading)
    #[serde(default, deserialize_with = "tolerant_option_vec_string")]
    pub status: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CalcIndexesParam {
    /// Security symbols, e.g. ["700.HK", "AAPL.US"]. Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    #[serde(deserialize_with = "tolerant_vec_string")]
    pub symbols: Vec<String>,
    /// Calc indexes (optional; defaults to LastDone, ChangeValue, ChangeRate, Volume, PeTtmRatio, PbRatio, DividendRatioTtm, TurnoverRate, TotalMarketValue): LastDone, ChangeValue, ChangeRate, Volume, Turnover, YtdChangeRate, TurnoverRate, TotalMarketValue, CapitalFlow, Amplitude, VolumeRatio, PeTtmRatio, PbRatio, DividendRatioTtm, FiveDayChangeRate, TenDayChangeRate, HalfYearChangeRate, FiveMinutesChangeRate, ExpiryDate, StrikePrice, UpperStrikePrice, LowerStrikePrice, OutstandingQty, OutstandingRatio, Premium, ItmOtm, ImpliedVolatility, WarrantDelta, CallPrice, ToCallPrice, EffectiveLeverage, LeverageRatio, ConversionRatio, BalancePoint, OpenInterest, Delta, Gamma, Theta, Vega, Rho
    #[serde(default, deserialize_with = "tolerant_vec_string")]
    pub indexes: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateWatchlistGroupParam {
    /// Group name
    pub name: String,
    /// Securities to add, e.g. ["700.HK", "AAPL.US"]
    #[serde(default, deserialize_with = "tolerant_option_vec_string")]
    pub securities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteWatchlistGroupParam {
    /// Watchlist group id
    #[serde(deserialize_with = "tolerant_i64")]
    pub id: i64,
    /// Whether to also remove the securities from other groups
    #[serde(deserialize_with = "tolerant_bool")]
    pub purge: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateWatchlistGroupParam {
    /// Watchlist group id
    #[serde(deserialize_with = "tolerant_i64")]
    pub id: i64,
    /// New group name (optional)
    pub name: Option<String>,
    /// Securities list (optional)
    #[serde(default, deserialize_with = "tolerant_option_vec_string")]
    pub securities: Option<Vec<String>>,
    /// Update mode for securities: "add", "remove", or "replace" (default: "replace")
    pub mode: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SecurityListParam {
    /// Market code: US, HK, CN, SG
    pub market: String,
    /// Category filter. Currently only "Overnight" is supported; omitting defaults to Overnight.
    pub category: Option<String>,
    /// Page number, 1-based (default: 1)
    #[serde(default, deserialize_with = "tolerant_option_usize")]
    pub page: Option<usize>,
    /// Records per page (default: 50)
    #[serde(default, deserialize_with = "tolerant_option_usize")]
    pub count: Option<usize>,
}

pub async fn static_info(
    mctx: &crate::tools::McpContext,
    p: SymbolsParam,
) -> Result<CallToolResult, McpError> {
    use crate::tools::support::us_market::is_us_crypto_symbol;

    let (crypto_symbols, other_symbols): (Vec<String>, Vec<String>) =
        if mctx.dc_region().await == longbridge::DcRegion::Us {
            p.symbols.into_iter().partition(|s| is_us_crypto_symbol(s))
        } else {
            (Vec::new(), p.symbols)
        };

    let mut results: Vec<serde_json::Value> = Vec::new();
    // US-crypto overview is already an HTTP-backed SDK call; keep using it (only
    // spin up a WS context when there are crypto symbols to serve).
    if !crypto_symbols.is_empty() {
        let ctx = mctx.get_quote_context().await;
        for symbol in crypto_symbols {
            let overview = ctx.us_crypto_overview(symbol).await.map_err(|e| {
                mctx.evict_quote_context();
                Error::longbridge(e)
            })?;
            let mut value = serde_json::to_value(&overview).map_err(Error::Serialize)?;
            crate::tools::support::us_normalize::normalize_crypto_profile(&mut value);
            results.push(value);
        }
    }
    if !other_symbols.is_empty() {
        // WS→HTTP (id 503, POST /quote/static-info). Unwrap `secu_static_info`,
        // drop the extra `listing_date`, and fold the `stock_derivatives` int
        // array into the SDK `DerivativeType` bitflags name (`[2]` → `"WARRANT"`).
        let client = mctx.create_http_client();
        let mut arr = http_post_value(
            &client,
            "/quote/static-info",
            serde_json::json!({ "symbol": other_symbols }),
            Some("secu_static_info"),
            &[],
            &["listing_date"],
            &[],
        )
        .await?;
        if let Some(items) = arr.as_array_mut() {
            for it in items.iter_mut() {
                if let Some(obj) = it.as_object_mut()
                    && let Some(sd) = obj.get("stock_derivatives") {
                        let bits: u8 = sd
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(serde_json::Value::as_i64)
                                    .fold(0u8, |acc, n| acc | (n as u8))
                            })
                            .unwrap_or(0);
                        let dt = longbridge::quote::DerivativeType::from_bits_truncate(bits);
                        if let Ok(v) = serde_json::to_value(dt) {
                            obj.insert("stock_derivatives".to_string(), v);
                        }
                    }
            }
        }
        if let serde_json::Value::Array(items) = arr {
            results.extend(items);
        }
    }
    // eps/eps_ttm/bps/dividend_yield arrive with ~16 fractional digits of bogus
    // precision (e.g. eps "27.3458639853059994"); cap at 6 dp. Integer share
    // counts and non-numeric fields are left untouched.
    let mut results = serde_json::Value::Array(results);
    crate::serialize::round_decimals(&mut results, 6);
    tool_json(&results)
}

/// True when an extended-hours session has actually traded. The upstream fills
/// untraded sessions with `last_done == 0`, which must not be reported as a
/// real (and wildly negative) price move.
fn session_has_data(session: &serde_json::Map<String, serde_json::Value>) -> bool {
    match session.get("last_done") {
        Some(serde_json::Value::String(s)) => s.parse::<f64>().is_ok_and(|v| v > 0.0),
        Some(serde_json::Value::Number(n)) => n.as_f64().is_some_and(|v| v > 0.0),
        _ => false,
    }
}

/// Rewrites the extended-hours keys on `quote` output: the `_quote` suffix is
/// dropped (`post_market` / `overnight` / `pre_market`, in that order), and
/// sessions that never traded are emitted as `null` instead of a zero-filled
/// object, so callers don't read a bogus -100% move after the close.
fn normalize_extended_sessions(value: &mut serde_json::Value) {
    let Some(items) = value.as_array_mut() else {
        return;
    };
    for item in items {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        // Remove all three before inserting: `serde_json::Map::remove` is a
        // swap-remove, so interleaving remove/insert would scramble key order.
        let post = clean_session(obj.remove("post_market_quote"));
        let overnight = clean_session(obj.remove("overnight_quote"));
        let pre = clean_session(obj.remove("pre_market_quote"));
        obj.insert("post_market".to_string(), post);
        obj.insert("overnight".to_string(), overnight);
        obj.insert("pre_market".to_string(), pre);
    }
}

/// Maps a removed extended-session value to its normalized form: a traded
/// session is kept as-is, anything else (missing, null, or a zero `last_done`)
/// becomes `null`.
fn clean_session(removed: Option<serde_json::Value>) -> serde_json::Value {
    match removed {
        Some(serde_json::Value::Object(map)) if session_has_data(&map) => {
            serde_json::Value::Object(map)
        }
        _ => serde_json::Value::Null,
    }
}

/// Replace an integer at `segments` (supporting `*` for object/array wildcards)
/// with the SDK enum `E`'s serialized name, so an HTTP proto-JSON int (e.g.
/// `trade_status: 0`) renders exactly as the WS/SDK path did (`"Normal"`).
/// Reuses the SDK enum's own `TryFrom<i32>` + `Serialize`, so there is no
/// hand-maintained int→name table to drift.
fn map_enum_path<E>(value: &mut serde_json::Value, segments: &[&str])
where
    E: TryFrom<i32> + serde::Serialize,
{
    map_enum_path_via::<E, E>(value, segments)
}

/// Two-step variant for SDK enums built from a proto enum via `From` (e.g. the
/// SDK `TradeSession` is `From<longbridge_proto::quote::TradeSession>`): decode
/// the wire int with `P: TryFrom<i32>`, convert to the SDK enum `S`, serialize.
/// On a failed decode the int is left unchanged.
fn map_enum_path_via<P, S>(value: &mut serde_json::Value, segments: &[&str])
where
    P: TryFrom<i32>,
    S: From<P> + serde::Serialize,
{
    let Some((seg, rest)) = segments.split_first() else {
        if let Some(n) = value.as_i64()
            && let Ok(p) = P::try_from(n as i32)
                && let Ok(v) = serde_json::to_value(S::from(p)) {
                    *value = v;
                }
        return;
    };
    match value {
        serde_json::Value::Array(arr) if *seg == "*" => {
            for v in arr.iter_mut() {
                map_enum_path_via::<P, S>(v, rest);
            }
        }
        serde_json::Value::Object(map) if *seg == "*" => {
            for v in map.values_mut() {
                map_enum_path_via::<P, S>(v, rest);
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(v) = map.get_mut(*seg) {
                map_enum_path_via::<P, S>(v, rest);
            }
        }
        _ => {}
    }
}

/// Replace a string code at `segments` (supporting `*`) with the SDK enum `E`'s
/// serialized name via its `FromStr` (strum) parse — e.g. option `direction`
/// `"C"` → `"Call"`, `option_type` `"W"` → `"Weekly"`, `standard_attr` `""` →
/// `"Normal"`. Leaves the value unchanged if it does not parse.
fn map_str_enum<E>(value: &mut serde_json::Value, segments: &[&str])
where
    E: std::str::FromStr + serde::Serialize,
{
    let Some((seg, rest)) = segments.split_first() else {
        if let Some(s) = value.as_str()
            && let Ok(e) = s.parse::<E>()
                && let Ok(v) = serde_json::to_value(e) {
                    *value = v;
                }
        return;
    };
    match value {
        serde_json::Value::Array(arr) if *seg == "*" => {
            for v in arr.iter_mut() {
                map_str_enum::<E>(v, rest);
            }
        }
        serde_json::Value::Object(map) if *seg == "*" => {
            for v in map.values_mut() {
                map_str_enum::<E>(v, rest);
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(v) = map.get_mut(*seg) {
                map_str_enum::<E>(v, rest);
            }
        }
        _ => {}
    }
}

pub async fn quote(
    mctx: &crate::tools::McpContext,
    p: SymbolsParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 504, POST /quote/quotes). Unwrap `secu_quote`, rename
    // `over_night_quote` → `overnight_quote`, drop the redundant `volume_str`,
    // convert unix `timestamp`, and map `trade_status` int → the SDK enum name.
    // The downstream shape then matches the SDK `SecurityQuote`, so the existing
    // extended-session/null/zero transforms apply unchanged.
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/quotes",
        serde_json::json!({ "symbol": p.symbols }),
        Some("secu_quote"),
        &[("over_night_quote", "overnight_quote")],
        &["volume_str"],
        &["*.timestamp"],
    )
    .await?;
    map_enum_path::<longbridge::quote::TradeStatus>(&mut value, &["*", "trade_status"]);
    normalize_extended_sessions(&mut value);
    // Non-US symbols carry `pre_market_quote`/`post_market_quote`/
    // `overnight_quote` as `null`; drop those (and any other absent optional).
    crate::serialize::strip_nulls(&mut value);
    // Prices and turnover come padded to a fixed decimal width ("432.000",
    // "…550.800"), on the main quote and every extended-session block; strip
    // the non-significant trailing zeros (lossless).
    crate::serialize::strip_trailing_zeros(&mut value);
    tool_json(&value)
}

pub async fn option_quote(
    mctx: &crate::tools::McpContext,
    p: OptionSymbolsParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 505, POST /quote/options/quotes). Unwrap `secu_quote`, drop
    // `volume_str`, convert `timestamp`, map `trade_status`. Then merge option
    // Greeks (which this endpoint doesn't carry) from calc-indexes, keyed by
    // symbol — best-effort, a Greek lookup failure leaves quotes unchanged.
    // NOTE: not runtime-verified — the canary test account lacks US option quote
    // access (301604), so both WS and REST return no data there.
    let client = mctx.create_http_client();
    let mut value = match http_post_value(
        &client,
        "/quote/options/quotes",
        serde_json::json!({ "symbol": p.symbols.clone() }),
        Some("secu_quote"),
        &[],
        &["volume_str"],
        &["*.timestamp"],
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            if let Some(ok) = crate::tools::terminal_none_ok("option_quote", &err) {
                return Ok(ok);
            }
            return Err(err);
        }
    };
    // The REST payload nests the option-specific fields (strike_price, direction,
    // implied_volatility, underlying_symbol, …) under `option_extend`; the SDK
    // `OptionQuote` carries them flat, so lift them to each row's top level.
    if let Some(arr) = value.as_array_mut() {
        for item in arr.iter_mut() {
            if let Some(obj) = item.as_object_mut() {
                if let Some(serde_json::Value::Object(ext)) = obj.remove("option_extend") {
                    for (k, v) in ext {
                        obj.insert(k, v);
                    }
                }
            }
        }
    }
    map_enum_path::<longbridge::quote::TradeStatus>(&mut value, &["*", "trade_status"]);

    let greek_ints: Vec<i32> = [
        CalcIndex::Delta,
        CalcIndex::Gamma,
        CalcIndex::Theta,
        CalcIndex::Vega,
        CalcIndex::Rho,
    ]
    .iter()
    .map(|i| longbridge_proto::quote::CalcIndex::from(*i) as i32)
    .collect();
    if let Ok(mut greeks) = http_post_value(
        &client,
        "/quote/calc-indexes",
        serde_json::json!({ "symbols": p.symbols, "calc_index": greek_ints }),
        Some("security_calc_index"),
        &[],
        &[],
        &[],
    )
    .await
    {
        let mut by_symbol: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        if let Some(arr) = greeks.as_array_mut() {
            for g in arr.iter_mut() {
                if let Some(obj) = g.as_object_mut() {
                    for k in ["vega", "rho"] {
                        if let Some(s) = obj.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
                            && let Ok(d) = s.parse::<rust_decimal::Decimal>() {
                                obj.insert(
                                    k.to_string(),
                                    serde_json::Value::String(
                                        (d / rust_decimal::Decimal::ONE_HUNDRED).to_string(),
                                    ),
                                );
                            }
                    }
                    if let Some(sym) = obj.get("symbol").and_then(|s| s.as_str()).map(String::from) {
                        let pick = |k: &str| obj.get(k).cloned().unwrap_or(serde_json::Value::Null);
                        by_symbol.insert(
                            sym,
                            serde_json::json!({
                                "delta": pick("delta"),
                                "gamma": pick("gamma"),
                                "theta": pick("theta"),
                                "vega": pick("vega"),
                                "rho": pick("rho"),
                            }),
                        );
                    }
                }
            }
        }
        if let Some(arr) = value.as_array_mut() {
            for item in arr {
                let sym = item.get("symbol").and_then(|s| s.as_str()).map(String::from);
                if let (Some(sym), Some(obj)) = (sym, item.as_object_mut())
                    && let Some(g) = by_symbol.get(&sym).and_then(|v| v.as_object())
                {
                    for (k, v) in g {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }

    tool_json(&value)
}

pub async fn warrant_quote(
    mctx: &crate::tools::McpContext,
    p: SymbolsParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 506, POST /quote/warrants/quotes). Unwrap `secu_quote`, drop the
    // redundant `volume_str`, convert unix `timestamp`, map `trade_status` int →
    // SDK name. Cap warrant analytics precision at 6 dp (same as warrant_list).
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/warrants/quotes",
        serde_json::json!({ "symbol": p.symbols }),
        Some("secu_quote"),
        &[],
        &["volume_str"],
        &["*.timestamp"],
    )
    .await?;
    // REST nests warrant-specific fields under `warrant_extend`; the SDK
    // `WarrantQuote` carries them flat — lift them to each row's top level.
    if let Some(arr) = value.as_array_mut() {
        for item in arr.iter_mut() {
            if let Some(obj) = item.as_object_mut() {
                if let Some(serde_json::Value::Object(ext)) = obj.remove("warrant_extend") {
                    for (k, v) in ext {
                        obj.insert(k, v);
                    }
                }
            }
        }
    }
    map_enum_path::<longbridge::quote::TradeStatus>(&mut value, &["*", "trade_status"]);
    crate::serialize::round_decimals(&mut value, 6);
    Ok(crate::tools::tool_result(
        serde_json::to_string(&value).map_err(Error::Serialize)?,
    ))
}

pub async fn depth(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 507, POST /quote/depth). Reshape the gateway proto-JSON back to
    // the SDK `SecurityDepth` shape: `ask`/`bid` → `asks`/`bids`, drop the echoed
    // `symbol` and the redundant `volume_str`. Order-book prices come padded to a
    // fixed decimal width ("432.200"); strip the non-significant trailing zeros.
    let client = mctx.create_http_client();
    http_post_tool_reshape(
        &client,
        "/quote/depth",
        serde_json::json!({ "symbol": p.symbol }),
        None,
        &[("ask", "asks"), ("bid", "bids")],
        &["symbol", "volume_str"],
        &[],
        true,
    )
    .await
}

pub async fn brokers(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 508, POST /quote/brokers). SDK `SecurityBrokers` uses the same
    // `ask_brokers`/`bid_brokers` field names as the proto-JSON, so only the echoed
    // `symbol` needs dropping; broker rows carry no enum/timestamp fields.
    let client = mctx.create_http_client();
    http_post_tool_reshape(
        &client,
        "/quote/brokers",
        serde_json::json!({ "symbol": p.symbol }),
        None,
        &[],
        &["symbol"],
        &[],
        false,
    )
    .await
}

pub async fn participants(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 509, POST /quote/participants). Unwrap the `participant_broker_numbers`
    // container; the proto rows use `participant_name_*` — rename to the SDK's `name_*`,
    // then drop `name_hk` (the Traditional-script twin of `name_cn` on every row).
    let client = mctx.create_http_client();
    http_post_tool_reshape(
        &client,
        "/quote/participants",
        serde_json::json!({}),
        Some("participant_broker_numbers"),
        &[
            ("participant_name_cn", "name_cn"),
            ("participant_name_en", "name_en"),
        ],
        &["participant_name_hk"],
        &[],
        false,
    )
    .await
}

pub async fn trades(
    mctx: &crate::tools::McpContext,
    p: SymbolCountParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 510, POST /quote/trades). Unwrap `trades`, drop the echoed
    // `symbol`, convert unix `timestamp`; map `direction` (int → SDK
    // `TradeDirection` name) and `trade_session` (wire int → proto → SDK
    // `TradeSession` name). Prices come padded to a fixed decimal width
    // ("431.000"); strip the non-significant trailing zeros. Up to 1000 trades.
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/trades",
        serde_json::json!({ "symbol": p.symbol, "count": p.count }),
        Some("trades"),
        &[],
        &["symbol"],
        &["*.timestamp"],
    )
    .await?;
    map_enum_path::<longbridge::quote::TradeDirection>(&mut value, &["*", "direction"]);
    map_enum_path_via::<longbridge_proto::quote::TradeSession, longbridge::quote::TradeSession>(
        &mut value,
        &["*", "trade_session"],
    );
    crate::serialize::strip_trailing_zeros(&mut value);
    Ok(crate::tools::tool_result(
        serde_json::to_string(&value).map_err(Error::Serialize)?,
    ))
}

pub async fn intraday(
    mctx: &crate::tools::McpContext,
    p: IntradayParam,
) -> Result<CallToolResult, McpError> {
    let sessions = match p.trade_sessions.as_deref() {
        Some(s) => parse::parse_trade_sessions(s)?,
        None => longbridge::quote::TradeSessions::Intraday,
    };
    // WS→HTTP (id 511, POST /quote/intraday). Unwrap `lines`, drop the redundant
    // `volume_str`, convert unix `timestamp` to RFC3339, strip padded price zeros.
    let client = mctx.create_http_client();
    http_post_tool_reshape(
        &client,
        "/quote/intraday",
        serde_json::json!({ "symbol": p.symbol, "trade_session": sessions as i32 }),
        Some("lines"),
        &[],
        &["volume_str"],
        &["*.timestamp"],
        true,
    )
    .await
}

/// Upstream's own "symbol count out of limit" business code — seen firing
/// even when the requested count exactly equals the reported limit (e.g.
/// `requested:100/limit:100`), which means the true ceiling for some account
/// tiers is exclusive. [`with_candlestick_count_boundary_retry`] retries once
/// just under that boundary rather than surfacing the raw upstream error.
const CANDLESTICK_COUNT_OUT_OF_LIMIT: i64 = 301607;

fn validate_candlestick_count(count: usize) -> Result<(), McpError> {
    if count == 0 {
        return Err(McpError::invalid_params("count must be at least 1", None));
    }
    Ok(())
}

/// Retry once with `count - 1` when upstream rejects the exact requested
/// count at an account's own quota boundary.
async fn with_candlestick_count_boundary_retry<T, Fut>(
    count: usize,
    mut call: impl FnMut(usize) -> Fut,
) -> Result<T, Box<longbridge::Error>>
where
    Fut: std::future::Future<Output = Result<T, longbridge::Error>>,
{
    match call(count).await {
        // Retry the count-boundary case (limit>0), but NOT the zero-quota case
        // (`limit:0`): retrying with count-1 can't conjure a quota the account
        // doesn't have, so it would just burn a second upstream call before the
        // terminal degrade. `is_terminal_none` handles limit:0 downstream.
        Err(e)
            if count > 1
                && e.openapi_error_code() == Some(CANDLESTICK_COUNT_OUT_OF_LIMIT)
                && !e.to_string().to_lowercase().contains("limit:0") =>
        {
            call(count - 1).await.map_err(Box::new)
        }
        result => result.map_err(Box::new),
    }
}

/// Reshape a `/quote/candlesticks` or `/quote/history-candlesticks` raw response
/// (candlestick list) to the SDK `Candlestick` shape: unwrap `candlesticks`, drop
/// the redundant `volume_str`, convert unix `timestamp`, map `trade_session` int
/// → SDK name, strip padded price/turnover trailing zeros.
/// NOTE: the REST payload omits the SDK's `open_updated` flag — pending a
/// gateway/backend addition (see verification report); not synthesized here.
fn reshape_candlesticks(resp: &str) -> Result<CallToolResult, McpError> {
    let transformed = crate::serialize::transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    if let Some(inner) = value.get_mut("candlesticks").map(serde_json::Value::take) {
        value = inner;
    }
    crate::serialize::drop_keys(&mut value, &["volume_str"]);
    crate::serialize::convert_unix_paths(&mut value, &["*.timestamp"]);
    map_enum_path_via::<longbridge_proto::quote::TradeSession, longbridge::quote::TradeSession>(
        &mut value,
        &["*", "trade_session"],
    );
    crate::serialize::strip_trailing_zeros(&mut value);
    Ok(crate::tools::tool_result(
        serde_json::to_string(&value).map_err(Error::Serialize)?,
    ))
}

pub async fn candlesticks(
    mctx: &crate::tools::McpContext,
    p: CandlesticksParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 512, POST /quote/candlesticks). Preserve the 301607
    // count-boundary retry around the HTTP call; reshape via `reshape_candlesticks`.
    validate_candlestick_count(p.count)?;
    let period = parse::parse_period(&p.period)?;
    let sessions = parse::parse_trade_sessions(&p.trade_sessions)?;
    let adjust = if p.forward_adjust {
        longbridge::quote::AdjustType::ForwardAdjust
    } else {
        longbridge::quote::AdjustType::NoAdjust
    };
    let client = mctx.create_http_client();
    let resp = with_candlestick_count_boundary_retry(p.count, |count| {
        http_post_raw(
            &client,
            "/quote/candlesticks",
            serde_json::json!({
                "symbol": p.symbol.clone(),
                "period": period as i32,
                "count": count,
                "adjust_type": adjust as i32,
                "trade_session": sessions as i32,
            }),
        )
    })
    .await
    .map_err(|e| Error::longbridge(*e))?;
    reshape_candlesticks(&resp)
}

pub async fn history_candlesticks_by_offset(
    mctx: &crate::tools::McpContext,
    p: HistoryCandlesticksByOffsetParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 521, POST /quote/history-candlesticks, query_type=QueryByOffset).
    // Nested `offset_request` body; output reshaped via `reshape_candlesticks`.
    // Preserves the 301607 count-boundary retry and the terminal-none degrade.
    // NOTE: the nested request-body shape is per proto but not yet runtime-verified
    // on a data-bearing symbol.
    validate_candlestick_count(p.count)?;
    let period = parse::parse_period(&p.period)?;
    let adjust = parse::parse_adjust_type(p.forward_adjust);
    let sessions = parse::parse_trade_sessions(&p.trade_sessions)?;
    let time = match p.time {
        Some(ref s) => Some(parse::parse_primitive_datetime(s)?),
        None => None,
    };
    let (date_s, minute_s) = match time {
        Some(t) => (
            format!("{:04}{:02}{:02}", t.year(), t.month() as u8, t.day()),
            format!("{:02}{:02}", t.hour(), t.minute()),
        ),
        None => (String::new(), String::new()),
    };
    let client = mctx.create_http_client();
    let outcome = with_candlestick_count_boundary_retry(p.count, |count| {
        http_post_raw(
            &client,
            "/quote/history-candlesticks",
            serde_json::json!({
                "symbol": p.symbol.clone(),
                "period": period as i32,
                "adjust_type": adjust as i32,
                "query_type": 1,
                "offset_request": {
                    "direction": if p.forward { 1 } else { 0 },
                    "date": date_s.clone(),
                    "minute": minute_s.clone(),
                    "count": count,
                },
                "trade_session": sessions as i32,
            }),
        )
    })
    .await;
    let resp = match outcome {
        Ok(v) => v,
        Err(e) => {
            let err: McpError = Error::longbridge(*e).into();
            if let Some(ok) = crate::tools::terminal_none_ok("history_candlesticks_by_offset", &err)
            {
                return Ok(ok);
            }
            return Err(err);
        }
    };
    reshape_candlesticks(&resp)
}

pub async fn history_candlesticks_by_date(
    mctx: &crate::tools::McpContext,
    p: HistoryCandlesticksByDateParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 521, POST /quote/history-candlesticks, query_type=QueryByDate).
    // Nested `date_request` body; output reshaped via `reshape_candlesticks`.
    // NOTE: nested request-body shape is per proto, not yet runtime-verified.
    let period = parse::parse_period(&p.period)?;
    let adjust = parse::parse_adjust_type(p.forward_adjust);
    let sessions = parse::parse_trade_sessions(&p.trade_sessions)?;
    let ymd = |d: time::Date| format!("{:04}{:02}{:02}", d.year(), d.month() as u8, d.day());
    let start_s = match p.start {
        Some(ref s) => ymd(parse::parse_date(s)?),
        None => String::new(),
    };
    let end_s = match p.end {
        Some(ref s) => ymd(parse::parse_date(s)?),
        None => String::new(),
    };
    let client = mctx.create_http_client();
    let resp = match http_post_raw(
        &client,
        "/quote/history-candlesticks",
        serde_json::json!({
            "symbol": p.symbol,
            "period": period as i32,
            "adjust_type": adjust as i32,
            "query_type": 2,
            "date_request": { "start_date": start_s, "end_date": end_s },
            "trade_session": sessions as i32,
        }),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            let err: McpError = Error::longbridge(e).into();
            if let Some(ok) = crate::tools::terminal_none_ok("history_candlesticks_by_date", &err) {
                return Ok(ok);
            }
            return Err(err);
        }
    };
    reshape_candlesticks(&resp)
}

pub async fn trading_days(
    mctx: &crate::tools::McpContext,
    p: MarketDateRangeParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 517, POST /quote/markets/trading-days). Body: market as its
    // canonical string ("HK"), begin/end as YYYYMMDD. Response fields are proto
    // `trade_day`/`half_trade_day` (singular, YYYYMMDD) → rename to the SDK's
    // `trade_days`/`half_trade_days` and reformat each date to YYYY-MM-DD.
    let market = parse::parse_market(&p.market)?;
    let start = parse::parse_date(&p.start)?;
    let end = parse::parse_date(&p.end)?;
    let market_str = serde_json::to_value(market)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| p.market.to_uppercase());
    let ymd = |d: time::Date| format!("{:04}{:02}{:02}", d.year(), d.month() as u8, d.day());
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/markets/trading-days",
        serde_json::json!({ "market": market_str, "beg_day": ymd(start), "end_day": ymd(end) }),
        None,
        &[("trade_day", "trade_days"), ("half_trade_day", "half_trade_days")],
        &[],
        &[],
    )
    .await?;
    for key in ["trade_days", "half_trade_days"] {
        if let Some(arr) = value.get_mut(key).and_then(|a| a.as_array_mut()) {
            for v in arr.iter_mut() {
                if let Some(s) = v.as_str()
                    && s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
                        *v = serde_json::Value::String(format!(
                            "{}-{}-{}",
                            &s[0..4],
                            &s[4..6],
                            &s[6..8]
                        ));
                    }
            }
        }
    }
    tool_json(&value)
}

pub async fn option_chain_expiry_date_list(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 513, POST /quote/options/expiry-dates). Unwrap `expiry_date`
    // and reformat each `YYYYMMDD` string to the SDK's `YYYY-MM-DD`.
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/options/expiry-dates",
        serde_json::json!({ "symbol": p.symbol }),
        Some("expiry_date"),
        &[],
        &[],
        &[],
    )
    .await?;
    if let Some(arr) = value.as_array_mut() {
        for v in arr.iter_mut() {
            if let Some(s) = v.as_str()
                && s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
                    *v = serde_json::Value::String(format!("{}-{}-{}", &s[0..4], &s[4..6], &s[6..8]));
                }
        }
    }
    Ok(crate::tools::tool_result(
        serde_json::to_string(&value).map_err(Error::Serialize)?,
    ))
}

pub async fn option_chain_info_by_date(
    mctx: &crate::tools::McpContext,
    p: OptionChainByDateParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 496, POST /quote/options/strikes). Unwrap `list`; map the
    // string-code enums to the SDK names (`direction` C/P→Call/Put, `option_type`
    // ""/W/Q→Monthly/Weekly/Quarterly, `standard_attr` ""/old→Normal/Old); reformat
    // each `expiry_date` YYYYMMDD → YYYY-MM-DD. Backend is lb-gemini-app.
    let date = parse::parse_date(&p.date)?;
    let ymd = date
        .format(time::macros::format_description!("[year][month][day]"))
        .map_err(|e| Error::Other(e.to_string()))?;
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/options/strikes",
        serde_json::json!({
            "symbol": p.symbol,
            "expiry_date": ymd,
            "standard_only": p.standard_only.unwrap_or(false),
        }),
        Some("list"),
        &[],
        &[],
        &[],
    )
    .await?;
    map_str_enum::<longbridge::quote::OptionDirection>(&mut value, &["*", "direction"]);
    map_str_enum::<longbridge::quote::OptionExpiryCycleType>(&mut value, &["*", "option_type"]);
    map_str_enum::<longbridge::quote::OptionStandardAttr>(&mut value, &["*", "standard_attr"]);
    if let Some(arr) = value.as_array_mut() {
        for it in arr.iter_mut() {
            if let Some(ed) = it.get_mut("expiry_date")
                && let Some(s) = ed.as_str()
                    && s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
                        *ed = serde_json::Value::String(format!(
                            "{}-{}-{}",
                            &s[0..4],
                            &s[4..6],
                            &s[6..8]
                        ));
                    }
        }
    }
    Ok(crate::tools::tool_result(
        serde_json::to_string(&value).map_err(Error::Serialize)?,
    ))
}

pub async fn capital_flow(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 518, POST /quote/capital-flow). Unwrap `capital_flow_lines`,
    // drop the echoed `symbol`, convert unix `timestamp` to RFC3339.
    // NOTE: `inflow` currently comes back ~1e4 smaller than the WS value — the
    // backend has confirmed this and will fix it (align REST to WS). Not scaled
    // here on purpose, so nothing double-scales once the backend fix lands.
    let client = mctx.create_http_client();
    http_post_tool_reshape(
        &client,
        "/quote/capital-flow",
        serde_json::json!({ "symbol": p.symbol }),
        Some("capital_flow_lines"),
        &[],
        &["symbol"],
        &["*.timestamp"],
        false,
    )
    .await
}

pub async fn capital_distribution(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 519, POST /quote/capital-distribution). Not container-wrapped;
    // drop the echoed `symbol`. Preserve the existing "no data" semantics:
    // symbols with no capital-flow data (indices, some ETFs) come back zero-filled
    // and stamped at the Unix epoch, which is never a real trading instant.
    // NOTE: large/medium/small currently arrive ~1e4 smaller than the WS value —
    // the backend has confirmed this and will fix it (align REST to WS); not
    // scaled here on purpose, so nothing double-scales once the fix lands.
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/capital-distribution",
        serde_json::json!({ "symbol": p.symbol }),
        None,
        &[],
        &["symbol"],
        &[],
    )
    .await?;
    let raw_ts = value
        .get("timestamp")
        .and_then(|t| t.as_i64().or_else(|| t.as_str().and_then(|s| s.parse::<i64>().ok())));
    let data_available = raw_ts.map(|n| n != 0).unwrap_or(false);
    crate::serialize::convert_unix_paths(&mut value, &["timestamp"]);
    // The backend sends `""` for a zero bucket; the SDK rendered that as `"0"`.
    for group in ["capital_in", "capital_out"] {
        if let Some(obj) = value.get_mut(group).and_then(|g| g.as_object_mut()) {
            for k in ["large", "medium", "small"] {
                if let Some(v) = obj.get_mut(k)
                    && v.as_str() == Some("") {
                        *v = serde_json::Value::String("0".to_string());
                    }
            }
        }
    }
    if let Some(obj) = value.as_object_mut() {
        obj.insert("data_available".to_string(), serde_json::Value::Bool(data_available));
    }
    tool_json(&value)
}

/// Format an `HHMM` integer (e.g. `930`) as the SDK's `"HH:MM:00.0"` time string
/// (`"09:30:00.0"`). Leaves the value untouched if it is not an integer.
fn hhmm_to_time(v: &serde_json::Value) -> serde_json::Value {
    match v.as_i64() {
        Some(n) => serde_json::Value::String(format!("{:02}:{:02}:00.0", n / 100, n % 100)),
        None => v.clone(),
    }
}

pub async fn trading_session(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 516, POST /quote/markets/trading-sessions). Unwrap
    // `market_trade_session`; rename each market's inner `trade_session` array to
    // `trade_sessions`; within each session rename `beg_time`→`begin_time`,
    // convert the `HHMM` int times to `"HH:MM:00.0"`, and map the `trade_session`
    // int (wire → proto → SDK `TradeSession` name).
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/markets/trading-sessions",
        serde_json::json!({}),
        Some("market_trade_session"),
        &[("trade_session", "trade_sessions")],
        &[],
        &[],
    )
    .await?;
    if let Some(markets) = value.as_array_mut() {
        for market in markets.iter_mut() {
            let Some(sessions) = market
                .get_mut("trade_sessions")
                .and_then(|s| s.as_array_mut())
            else {
                continue;
            };
            for session in sessions.iter_mut() {
                if let Some(obj) = session.as_object_mut() {
                    if let Some(bt) = obj.remove("beg_time") {
                        obj.insert("begin_time".to_string(), hhmm_to_time(&bt));
                    }
                    if let Some(et) = obj.get_mut("end_time") {
                        *et = hhmm_to_time(et);
                    }
                }
                map_enum_path_via::<
                    longbridge_proto::quote::TradeSession,
                    longbridge::quote::TradeSession,
                >(session, &["trade_session"]);
            }
        }
    }
    Ok(crate::tools::tool_result(
        serde_json::to_string(&value).map_err(Error::Serialize)?,
    ))
}

pub async fn market_temperature(
    mctx: &crate::tools::McpContext,
    p: MarketParam,
) -> Result<CallToolResult, McpError> {
    let market = parse::parse_market(&p.market)?;
    let ctx = mctx.get_quote_context().await;
    let result = ctx.market_temperature(market).await.map_err(|e| {
        mctx.evict_quote_context();
        Error::longbridge(e)
    })?;
    tool_json(&result)
}

pub async fn history_market_temperature(
    mctx: &crate::tools::McpContext,
    p: MarketDateRangeParam,
) -> Result<CallToolResult, McpError> {
    let market = parse::parse_market(&p.market)?;
    let start = parse::parse_date(&p.start)?;
    let end = parse::parse_date(&p.end)?;
    let ctx = mctx.get_quote_context().await;
    let result = ctx
        .history_market_temperature(market, start, end)
        .await
        .map_err(|e| {
            mctx.evict_quote_context();
            Error::longbridge(e)
        })?;
    // The history series is numeric only: `description` (the point-in-time label
    // populated by `market_temperature`) is empty on every row here — verified
    // across markets and years. Drop it.
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    crate::serialize::drop_keys(&mut value, &["description"]);
    tool_json(&value)
}

pub async fn watchlist(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let ctx = mctx.get_quote_context().await;
    let result = ctx.watchlist().await.map_err(|e| {
        mctx.evict_quote_context();
        Error::longbridge(e)
    })?;
    // `market` on every security is derivable from the symbol suffix (and is
    // "Unknown" for crypto).
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    crate::serialize::drop_keys(&mut value, &["market"]);
    tool_json(&value)
}

pub async fn filings(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let ctx = mctx.get_quote_context().await;
    let result = ctx.filings(p.symbol).await.map_err(|e| {
        mctx.evict_quote_context();
        Error::longbridge(e)
    })?;
    tool_json(&result)
}

pub async fn warrant_issuers(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    // WS→HTTP (id 514, POST /quote/warrants/issuers). Unwrap `issuer_info`, rename
    // the proto `id` to the SDK's `issuer_id`, drop `name_hk` (Traditional-script
    // twin of `name_cn` on every issuer row).
    let client = mctx.create_http_client();
    http_post_tool_reshape(
        &client,
        "/quote/warrants/issuers",
        serde_json::json!({}),
        Some("issuer_info"),
        &[("id", "issuer_id")],
        &["name_hk"],
        &[],
        false,
    )
    .await
}

pub async fn warrant_list(
    mctx: &crate::tools::McpContext,
    p: WarrantListParam,
) -> Result<CallToolResult, McpError> {
    let sort_by = parse::parse_warrant_sort_by(&p.sort_by)?;
    let sort_order = parse::parse_sort_order_type(&p.sort_order)?;

    let warrant_types: Option<Vec<_>> = p
        .warrant_type
        .as_deref()
        .map(|v| {
            v.iter()
                .map(|s| parse::parse_warrant_type(s))
                .collect::<Result<_, _>>()
        })
        .transpose()?;
    let expiry_dates: Option<Vec<_>> = p
        .expiry_date
        .as_deref()
        .map(|v| {
            v.iter()
                .map(|s| parse::parse_warrant_expiry_date(s))
                .collect::<Result<_, _>>()
        })
        .transpose()?;
    let price_types: Option<Vec<_>> = p
        .price_type
        .as_deref()
        .map(|v| {
            v.iter()
                .map(|s| parse::parse_warrant_price_type(s))
                .collect::<Result<_, _>>()
        })
        .transpose()?;
    let statuses: Option<Vec<_>> = p
        .status
        .as_deref()
        .map(|v| {
            v.iter()
                .map(|s| parse::parse_warrant_status(s))
                .collect::<Result<_, _>>()
        })
        .transpose()?;

    // WS→HTTP (id 515, POST /quote/warrants). Nested `filter_config` (sort_by/
    // sort_order/sort_offset/sort_count + optional type/issuer/expiry_date/
    // price_type/status int arrays). Unwrap `warrant_list` (drops `total_count`),
    // rename `change_val`→`change_value` & `type`→`warrant_type`, map the
    // `warrant_type`/`status` ints to SDK enum names, `""`→null, round to 6 dp.
    // NOTE: `name` differs by language (WS returns the code/EN name, REST the
    // localized name) — request `language` mapping to be confirmed; and the
    // filter-enum int encodings are per proto but only the no-filter path is
    // runtime-verified so far.
    let mut fc = serde_json::json!({
        "sort_by": sort_by as i32,
        "sort_order": sort_order as i32,
        "sort_offset": 0,
        "sort_count": 20,
    });
    if let Some(v) = &warrant_types {
        fc["type"] = v.iter().map(|x| *x as i32).collect();
    }
    if let Some(v) = &p.issuer {
        fc["issuer"] = v.clone().into();
    }
    if let Some(v) = &expiry_dates {
        fc["expiry_date"] = v.iter().map(|x| *x as i32).collect();
    }
    if let Some(v) = &price_types {
        fc["price_type"] = v.iter().map(|x| *x as i32).collect();
    }
    if let Some(v) = &statuses {
        fc["status"] = v.iter().map(|x| *x as i32).collect();
    }
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/warrants",
        serde_json::json!({ "symbol": p.symbol, "filter_config": fc, "language": 1 }),
        Some("warrant_list"),
        &[("change_val", "change_value"), ("type", "warrant_type")],
        &[],
        &[],
    )
    .await?;
    map_enum_path::<longbridge::quote::WarrantType>(&mut value, &["*", "warrant_type"]);
    map_enum_path::<longbridge::quote::WarrantStatus>(&mut value, &["*", "status"]);
    crate::serialize::empty_str_to_null(&mut value);
    crate::serialize::round_decimals(&mut value, 6);
    tool_json(&value)
}

/// Default calc indexes when the caller omits `indexes`: common quote fields
/// (last_done, change_value, change_rate, volume) plus the `longbridge
/// calc-index` CLI valuation default (pe, pb, dps_rate, turnover_rate, mktcap).
const DEFAULT_CALC_INDEXES: [&str; 9] = [
    "LastDone",
    "ChangeValue",
    "ChangeRate",
    "Volume",
    "PeTtmRatio",
    "PbRatio",
    "DividendRatioTtm",
    "TurnoverRate",
    "TotalMarketValue",
];

pub async fn calc_indexes(
    mctx: &crate::tools::McpContext,
    p: CalcIndexesParam,
) -> Result<CallToolResult, McpError> {
    let index_strs: Vec<&str> = if p.indexes.is_empty() {
        DEFAULT_CALC_INDEXES.to_vec()
    } else {
        p.indexes.iter().map(String::as_str).collect()
    };
    let indexes: Vec<longbridge::quote::CalcIndex> = index_strs
        .iter()
        .map(|s| parse::parse_calc_index(s))
        .collect::<Result<_, _>>()?;
    // WS→HTTP (id 520, POST /quote/calc-indexes). Body uses `symbols` (plural) +
    // `calc_index` proto ints. Unwrap `security_calc_index`, rename `change_val`
    // → `change_value`, drop `volume_str`. The backend fills unrequested fields
    // with `""`; convert to null and strip so only requested indexes remain
    // (matching the SDK's null-then-strip). vega/rho are ÷100 (normalize_greeks).
    let index_ints: Vec<i32> = indexes
        .iter()
        .map(|i| longbridge_proto::quote::CalcIndex::from(*i) as i32)
        .collect();
    let client = mctx.create_http_client();
    let mut value = http_post_value(
        &client,
        "/quote/calc-indexes",
        serde_json::json!({ "symbols": p.symbols, "calc_index": index_ints }),
        Some("security_calc_index"),
        &[("change_val", "change_value")],
        &["volume_str"],
        &[],
    )
    .await?;
    if let Some(arr) = value.as_array_mut() {
        for row in arr.iter_mut() {
            if let Some(obj) = row.as_object_mut() {
                for k in ["vega", "rho"] {
                    if let Some(s) = obj.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
                        && let Ok(d) = s.parse::<rust_decimal::Decimal>() {
                            obj.insert(
                                k.to_string(),
                                serde_json::Value::String(
                                    (d / rust_decimal::Decimal::ONE_HUNDRED).to_string(),
                                ),
                            );
                        }
                }
            }
        }
    }
    crate::serialize::empty_str_to_null(&mut value);
    crate::serialize::strip_nulls(&mut value);
    tool_json(&value)
}

/// Normalize the option Greeks on a calc-index row to the values the Longbridge
/// app displays.
///
/// - `theta`: the API already returns a per-day value (the raw annualized value
///   has been divided by 365 on the server), so it is used as-is.
/// - `vega`:  the API returns the value scaled by 100 (per 1% IV change);
///   divide by 100 for the per-unit value.
/// - `rho`:   the API returns the value scaled by 100 (per 1% rate change);
///   divide by 100 for the per-unit value.
pub async fn create_watchlist_group(
    mctx: &crate::tools::McpContext,
    p: CreateWatchlistGroupParam,
) -> Result<CallToolResult, McpError> {
    let mut req = RequestCreateWatchlistGroup::new(p.name);
    if let Some(securities) = p.securities {
        req = req.securities(securities);
    }
    let ctx = mctx.get_quote_context().await;
    let id = ctx.create_watchlist_group(req).await.map_err(|e| {
        mctx.evict_quote_context();
        Error::longbridge(e)
    })?;
    tool_json(&serde_json::json!({ "id": id }))
}

pub async fn delete_watchlist_group(
    mctx: &crate::tools::McpContext,
    p: DeleteWatchlistGroupParam,
) -> Result<CallToolResult, McpError> {
    let ctx = mctx.get_quote_context().await;
    let id = p.id;
    ctx.delete_watchlist_group(id, p.purge).await.map_err(|e| {
        mctx.evict_quote_context();
        Error::longbridge(e)
    })?;
    tool_json(&serde_json::json!({ "id": id, "deleted": true }))
}

pub async fn update_watchlist_group(
    mctx: &crate::tools::McpContext,
    p: UpdateWatchlistGroupParam,
) -> Result<CallToolResult, McpError> {
    let id = p.id;
    let mut req = RequestUpdateWatchlistGroup::new(id);
    if let Some(name) = p.name {
        req = req.name(name);
    }
    if let Some(securities) = p.securities {
        req = req.securities(securities);
        let mode = match p.mode.as_deref() {
            Some("add") => SecuritiesUpdateMode::Add,
            Some("remove") => SecuritiesUpdateMode::Remove,
            _ => SecuritiesUpdateMode::Replace,
        };
        req = req.mode(mode);
    }
    let ctx = mctx.get_quote_context().await;
    ctx.update_watchlist_group(req).await.map_err(|e| {
        mctx.evict_quote_context();
        Error::longbridge(e)
    })?;
    tool_json(&serde_json::json!({ "id": id, "updated": true }))
}

pub async fn security_list(
    mctx: &crate::tools::McpContext,
    p: SecurityListParam,
) -> Result<CallToolResult, McpError> {
    let market = parse::parse_market(&p.market)?;
    let category = match p.category {
        Some(ref s) => Some(parse::parse_security_list_category(s)?),
        None => None,
    };
    let ctx = mctx.get_quote_context().await;
    let all = ctx.security_list(market, category).await.map_err(|e| {
        mctx.evict_quote_context();
        Error::longbridge(e)
    })?;

    let page = p.page.unwrap_or(1).max(1);
    let count = p.count.unwrap_or(50).max(1);
    let total = all.len();
    let start = (page - 1) * count;
    let items: Vec<_> = all.into_iter().skip(start).take(count).collect();
    tool_json(&serde_json::json!({
        "total": total,
        "page": page,
        "count": count,
        "items": items,
    }))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShortPositionsParam {
    /// Security symbol, e.g. "AAPL.US" (US) or "700.HK" (HK). Market is inferred from suffix.
    pub symbol: String,
    /// Number of records to return (1-100, default 20)
    #[serde(default, deserialize_with = "tolerant_option_usize")]
    pub count: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OptionVolumeParam {
    /// Underlying symbol (US market only), e.g. "AAPL.US"
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OptionVolumeDailyParam {
    /// Underlying symbol (US market only), e.g. "AAPL.US"
    pub symbol: String,
    /// Number of trading days to return (default 20)
    #[serde(default, deserialize_with = "tolerant_option_usize")]
    pub count: Option<usize>,
}

pub async fn short_positions(
    mctx: &crate::tools::McpContext,
    p: ShortPositionsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let count = p.count.unwrap_or(20).clamp(1, 100);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let page_size = count.to_string();
    let params = [
        ("symbol", p.symbol.as_str()),
        ("last_timestamp", now.as_str()),
        ("page_size", page_size.as_str()),
    ];
    // Route to HK or US endpoint based on symbol suffix
    let is_hk = p.symbol.to_uppercase().ends_with(".HK");
    let path = if is_hk {
        "/v1/quote/short-positions/hk"
    } else {
        "/v1/quote/short-positions/us"
    };
    let unix_paths: &[&str] = if is_hk {
        &["data.*.timestamp", "update_timestamp"]
    } else {
        &["data.*.timestamp"]
    };
    let result = http_get_tool_unix(&client, path, &params, unix_paths).await?;
    Ok(normalize_short_positions(result, is_hk))
}

/// Normalize short_positions response to a unified schema regardless of market.
///
/// Unified data[] item fields:
///   timestamp   RFC3339
///   short_shares  number of open short shares (US: current_shares_short; HK: amount)
///   rate          decimal ratio (e.g. 0.0092 = 0.92%)
///   close         previous close price (US: close; HK: cost → renamed)
///   days_to_cover US only
///   avg_daily_vol US only (from avg_daily_share_volume)
///   balance       HK only — outstanding short position value (HKD)
fn normalize_short_positions(result: CallToolResult, is_hk: bool) -> CallToolResult {
    let Some(text) = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
    else {
        return result;
    };
    let Ok(mut d) = serde_json::from_str::<serde_json::Value>(text) else {
        return result;
    };

    // Remove top-level update_timestamp (redundant with data[].timestamp)
    if let Some(obj) = d.as_object_mut() {
        obj.remove("update_timestamp");
    }

    if let Some(items) = d.get_mut("data").and_then(|v| v.as_array_mut()) {
        for item in items.iter_mut() {
            let Some(obj) = item.as_object_mut() else {
                continue;
            };
            if is_hk {
                // amount → short_shares
                if let Some(v) = obj.remove("amount") {
                    obj.insert("short_shares".to_string(), v);
                }
                // cost → close
                if let Some(v) = obj.remove("cost") {
                    obj.insert("close".to_string(), v);
                }
            } else {
                // current_shares_short → short_shares
                if let Some(v) = obj.remove("current_shares_short") {
                    obj.insert("short_shares".to_string(), v);
                }
                // avg_daily_share_volume → avg_daily_vol
                if let Some(v) = obj.remove("avg_daily_share_volume") {
                    obj.insert("avg_daily_vol".to_string(), v);
                }
            }
        }
    }

    let Ok(json) = serde_json::to_string(&d) else {
        return result;
    };
    CallToolResult::success(vec![rmcp::model::Content::text(json)])
}

pub async fn option_volume(
    mctx: &crate::tools::McpContext,
    p: OptionVolumeParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let params = [("symbol", p.symbol.as_str())];
    http_get_tool(&client, "/v1/quote/option-volume-stats", &params).await
}

pub async fn option_volume_daily(
    mctx: &crate::tools::McpContext,
    p: OptionVolumeDailyParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let line_num = p.count.unwrap_or(20).to_string();
    let params = [
        ("symbol", p.symbol.as_str()),
        ("timestamp", now.as_str()),
        ("line_num", line_num.as_str()),
        ("direction", "1"),
    ];
    // `underlying_symbol` on every row == the queried `symbol`.
    http_get_tool_unix_dropping(
        &client,
        "/v1/quote/option-volume-stats/daily",
        &params,
        &["stats.*.timestamp"],
        &["underlying_symbol"],
    )
    .await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::normalize_extended_sessions;

    #[test]
    fn normalizes_sessions_string_last_done() {
        // Mirrors `serde_json::to_value(&Vec<SecurityQuote>)` when Decimal
        // serializes to strings: untraded sessions carry `last_done == "0"`.
        let mut value = json!([{
            "symbol": "MRVL.US",
            "last_done": "290.79",
            "pre_market_quote": { "last_done": "0", "volume": 0 },
            "post_market_quote": { "last_done": "318.765", "volume": 8071540 },
            "overnight_quote": serde_json::Value::Null,
        }]);
        normalize_extended_sessions(&mut value);
        let obj = value[0].as_object().unwrap();

        // `_quote` suffix dropped.
        assert!(!obj.contains_key("pre_market_quote"));
        assert!(!obj.contains_key("post_market_quote"));
        assert!(!obj.contains_key("overnight_quote"));
        // Traded session kept, untraded/zero and null sessions become null.
        assert!(obj["post_market"].is_object());
        assert!(obj["overnight"].is_null());
        assert!(obj["pre_market"].is_null());
        // Order is post / overnight / pre.
        let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        let pos = |k| keys.iter().position(|x| *x == k).unwrap();
        assert!(pos("post_market") < pos("overnight"));
        assert!(pos("overnight") < pos("pre_market"));
    }

    #[test]
    fn normalizes_sessions_numeric_last_done() {
        // When Decimal serializes to numbers instead of strings.
        let mut value = json!([{
            "symbol": "X",
            "post_market_quote": { "last_done": 100.5 },
            "overnight_quote": { "last_done": 0 },
            "pre_market_quote": serde_json::Value::Null,
        }]);
        normalize_extended_sessions(&mut value);
        let obj = value[0].as_object().unwrap();
        assert!(obj["post_market"].is_object());
        assert!(obj["overnight"].is_null());
        assert!(obj["pre_market"].is_null());
    }

    #[test]
    fn history_candlesticks_by_date_fills_defaults_from_symbol_alone() {
        let p: super::HistoryCandlesticksByDateParam =
            serde_json::from_value(json!({"symbol": "700.HK"}))
                .expect("symbol alone should be a valid call");
        assert_eq!(p.period, "day");
        assert_eq!(p.trade_sessions, "all");
        assert!(!p.forward_adjust);
        assert!(p.start.is_none() && p.end.is_none());
    }

    #[test]
    fn history_candlesticks_by_offset_fills_defaults_from_symbol_alone() {
        let p: super::HistoryCandlesticksByOffsetParam =
            serde_json::from_value(json!({"symbol": "AAPL.US"}))
                .expect("symbol alone should be a valid call");
        assert_eq!(p.period, "day");
        assert_eq!(p.trade_sessions, "all");
        assert_eq!(p.count, 100);
        assert!(!p.forward_adjust);
        assert!(!p.forward);
        assert!(p.time.is_none());
    }

    #[test]
    fn history_candlesticks_params_still_honour_explicit_values() {
        let p: super::HistoryCandlesticksByDateParam = serde_json::from_value(json!({
            "symbol": "700.HK",
            "period": "week",
            "forward_adjust": "true",
            "trade_sessions": "intraday",
            "start": "2026-01-01",
        }))
        .expect("explicit values should deserialize");
        assert_eq!(p.period, "week");
        assert_eq!(p.trade_sessions, "intraday");
        assert!(p.forward_adjust);
        assert_eq!(p.start.as_deref(), Some("2026-01-01"));
    }

    fn count_out_of_limit_error() -> longbridge::Error {
        longbridge::Error::WsClient(longbridge::wsclient::WsClientError::ResponseError {
            status: 7,
            detail: Some(longbridge::wsclient::WsResponseErrorDetail {
                code: 301607,
                msg: "history candlestick symbol count out of limit, requested:100/limit:100"
                    .to_string(),
            }),
        })
    }

    #[test]
    fn validate_candlestick_count_rejects_zero() {
        assert!(
            super::validate_candlestick_count(0).is_err(),
            "count=0 must be rejected"
        );
        assert!(
            super::validate_candlestick_count(1).is_ok(),
            "count=1 is the minimum valid value"
        );
    }

    #[tokio::test]
    async fn boundary_retry_retries_once_at_the_reported_limit() {
        let mut attempts = Vec::new();
        let result = super::with_candlestick_count_boundary_retry(100, |count| {
            attempts.push(count);
            async move {
                if count == 100 {
                    Err(count_out_of_limit_error())
                } else {
                    Ok(count)
                }
            }
        })
        .await;
        assert_eq!(attempts, vec![100, 99]);
        assert_eq!(result.unwrap(), 99);
    }

    #[tokio::test]
    async fn boundary_retry_does_not_retry_below_count_one() {
        let mut attempts = Vec::new();
        let result = super::with_candlestick_count_boundary_retry(1, |count| {
            attempts.push(count);
            async move { Err::<usize, _>(count_out_of_limit_error()) }
        })
        .await;
        assert_eq!(attempts, vec![1]);
        assert!(
            result.is_err(),
            "count=1 must not retry at count-1=0, so the error should propagate"
        );
    }

    #[tokio::test]
    async fn boundary_retry_does_not_retry_unrelated_errors() {
        let mut attempts = Vec::new();
        let result = super::with_candlestick_count_boundary_retry(100, |count| {
            attempts.push(count);
            async move {
                Err::<usize, _>(longbridge::Error::WsClient(
                    longbridge::wsclient::WsClientError::ResponseError {
                        status: 7,
                        detail: Some(longbridge::wsclient::WsResponseErrorDetail {
                            code: 301600,
                            msg: "invalid symbol".to_string(),
                        }),
                    },
                ))
            }
        })
        .await;
        assert_eq!(attempts, vec![100]);
        assert!(
            result.is_err(),
            "an unrelated error code (301600, not 301607) must not trigger a retry"
        );
    }
}
