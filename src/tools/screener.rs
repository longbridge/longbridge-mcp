//! Stock screener tools — strategy lists, strategy detail, search, and indicator metadata.

const DEFAULT_RETURNS: &[&str] = &[
    "filter_prevclose",
    "filter_prevchg",
    "filter_marketcap",
    "filter_salesgrowthyoy",
    "filter_pettm",
    "filter_pbmrq",
    "filter_industry",
];

use reqwest::Method;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::http_client::http_get_tool;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerRecommendStrategiesParam {
    /// Market filter: "US" | "HK" | "CN" | "SG" (default: "US")
    pub market: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerUserStrategiesParam {
    /// Market filter: "US" | "HK" | "CN" | "SG" (default: "US")
    pub market: Option<String>,
}

/// Strip "filter_" prefix from every `key` field inside strategy `filter.filters[]`.
/// Consistent with screener_indicators: keys are returned without prefix so callers
/// can pass them directly; screener_search Mode A re-adds the prefix before the API call.
fn strip_filter_prefix_from_strategy(v: &mut serde_json::Value) {
    if let Some(filters) = v
        .get_mut("filter")
        .and_then(|f| f.get_mut("filters"))
        .and_then(|f| f.as_array_mut())
    {
        for item in filters.iter_mut() {
            if let Some(k) = item.get("key").and_then(|k| k.as_str()) {
                let stripped = k.strip_prefix("filter_").unwrap_or(k).to_string();
                if let Some(obj) = item.as_object_mut() {
                    obj.insert("key".to_string(), serde_json::Value::String(stripped));
                }
            }
        }
    }
}

fn strip_strategy_keys(result: rmcp::model::CallToolResult) -> rmcp::model::CallToolResult {
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
    // single strategy object
    strip_filter_prefix_from_strategy(&mut d);
    // strategy list (strategys[])
    if let Some(list) = d.get_mut("strategys").and_then(|v| v.as_array_mut()) {
        for s in list.iter_mut() {
            strip_filter_prefix_from_strategy(s);
        }
    }
    // Each filter condition carries a `min` and a `max`; the unbounded side is
    // an empty string. Drop the blanks (an absent bound reads the same as "").
    crate::serialize::strip_empty_strings(&mut d);
    let Ok(json) = serde_json::to_string(&d) else {
        return result;
    };
    crate::tools::tool_result(json)
}

/// Platform-recommended screener strategies.
pub async fn screener_recommend_strategies(
    mctx: &crate::tools::McpContext,
    p: ScreenerRecommendStrategiesParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market = p.market.unwrap_or_else(|| "US".to_string());
    let result = http_get_tool(
        &client,
        "/v1/quote/ai/screener/strategies/recommend",
        &[("market", market.as_str())],
    )
    .await?;
    Ok(strip_strategy_keys(result))
}

/// User's own saved screener strategies.
pub async fn screener_user_strategies(
    mctx: &crate::tools::McpContext,
    p: ScreenerUserStrategiesParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let market = p.market.unwrap_or_else(|| "US".to_string());
    let result = http_get_tool(
        &client,
        "/v1/quote/ai/screener/strategies/mine",
        &[("market", market.as_str())],
    )
    .await?;
    Ok(strip_strategy_keys(result))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerStrategyParam {
    /// Strategy ID from screener_recommend_strategies or screener_user_strategies screeners[].id
    pub id: String,
}

pub async fn screener_strategy(
    mctx: &crate::tools::McpContext,
    p: ScreenerStrategyParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/quote/ai/screener/strategy/{}", p.id);
    let result = http_get_tool(&client, &path, &[]).await?;
    Ok(strip_strategy_keys(result))
}

/// A single Mode-B filter condition. `key` selects the indicator; the other
/// fields describe the filter. `tech_values` is accepted either as a JSON
/// string (the schema-advertised form, required for OpenAI compatibility) or
/// as an object (tolerated for Anthropic/Gemini clients).
#[derive(Debug, Deserialize)]
pub struct ScreenerCondition {
    pub key: String,
    pub min: Option<String>,
    pub max: Option<String>,
    pub tech_values: Option<serde_json::Value>,
}

/// Inlined JSON Schema for the `conditions` array.
///
/// Emitted by hand rather than derived from [`ScreenerCondition`] so the tool
/// manifest is OpenAI-function-calling compatible: the array `items` is a
/// concrete object (no `$ref`/`$defs`), and `tech_values` is a JSON *string*
/// (OpenAI strict mode cannot express the free-form technical-params object).
/// OpenAI also forces every property `required` and sends unused fields as
/// empty strings, so the consumer treats `""` as "not provided".
fn conditions_schema(_: &mut rmcp::schemars::SchemaGenerator) -> rmcp::schemars::Schema {
    rmcp::schemars::json_schema!({
        "type": "array",
        "description": "Mode B — filter conditions. Omit when using strategy_id (Mode A).",
        "items": {
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Indicator key; the \"filter_\" prefix is added automatically if missing.\nFundamental: pettm, pbmrq, roe, roa, netmargin, salesgrowthyoy, netincomegrowthyoy, marketcap, circulating_marketcap, prevclose, prevchg, divyld, la, epsttm, netincome, sales, turnover_rate, balance.\nTechnical: macd_day, macd_week, rsi_day, rsi_week, kdj_day, kdj_week, boll_day, boll_week."
                },
                "min": {
                    "type": "string",
                    "description": "Lower bound as a numeric string, e.g. \"10\". Pass an empty string when unbounded or for technical keys."
                },
                "max": {
                    "type": "string",
                    "description": "Upper bound as a numeric string, e.g. \"50\". Pass an empty string when unbounded or for technical keys."
                },
                "tech_values": {
                    "type": "string",
                    "description": "Technical-indicator params as a JSON string (empty string for fundamental keys). Every technical key needs a category; period is filled from the key suffix (_day/_week) when omitted:\nmacd_day/week: {\"category\":\"goldenfork\"|\"deadcross\"|\"upzero\"|\"downzero\"}\nkdj_day/week, rsi_day/week: {\"category\":\"goldenfork\"|\"deadcross\"}\nboll_day/week: {\"category\":\"riseupperband\"|\"fallbelowband\"|\"risemiddleband\"|\"fallmiddleband\"}"
                }
            },
            "required": ["key"]
        }
    })
}

/// Technical screener indicators and the `category` values each accepts, as
/// served by `screener_indicators` (verified live). Each exists as `<name>_day`
/// and `<name>_week`, and the upstream also requires a `period` equal to that
/// suffix — omitting it fails with an opaque HTTP 500 (`not have period`).
const TECH_INDICATORS: &[(&str, &[&str])] = &[
    ("macd", &["goldenfork", "deadcross", "upzero", "downzero"]),
    ("kdj", &["goldenfork", "deadcross"]),
    ("rsi", &["goldenfork", "deadcross"]),
    (
        "boll",
        &[
            "riseupperband",
            "fallbelowband",
            "risemiddleband",
            "fallmiddleband",
        ],
    ),
];

/// Upstream code for a rejected screener request; the gateway masks the real
/// reason (bad key, missing technical param) as HTTP 500.
const SCREENER_REJECTED_CODE: i64 = 2_301_500;

/// Complete and check `tech_values` for a technical-indicator condition,
/// before the request goes upstream where a mistake only comes back as a 500.
/// `key` carries the `filter_` prefix. Non-technical keys pass through as-is.
///
/// A missing category is rejected (upstream always requires one). A category
/// outside [`TECH_INDICATORS`] is still sent — the list could lag behind
/// upstream — but is pushed onto `suspects` so a 2301500 rejection can name it.
fn check_tech_values(
    key: &str,
    tech_values: Option<serde_json::Value>,
    suspects: &mut Vec<String>,
) -> Result<Option<serde_json::Value>, McpError> {
    let bare = key.strip_prefix("filter_").unwrap_or(key);
    let Some((name, period)) = bare.rsplit_once('_') else {
        return Ok(tech_values);
    };
    let Some((_, categories)) = TECH_INDICATORS.iter().find(|(n, _)| *n == name) else {
        return Ok(tech_values);
    };
    if !matches!(period, "day" | "week") {
        return Ok(tech_values);
    }
    let mut values = match tech_values {
        None => serde_json::Map::new(),
        Some(serde_json::Value::Object(map)) => map,
        Some(other) => {
            return Err(McpError::invalid_params(
                format!("tech_values for `{bare}` must be a JSON object, got {other}"),
                Some(serde_json::json!({ "error_code": "invalid_tech_values" })),
            ));
        }
    };
    let Some(category) = values.get("category").and_then(|v| v.as_str()) else {
        return Err(McpError::invalid_params(
            format!(
                "`{bare}` needs tech_values.category, one of: {}",
                categories.join(", ")
            ),
            Some(serde_json::json!({ "error_code": "invalid_tech_values" })),
        ));
    };
    if !categories.contains(&category) {
        suspects.push(format!(
            "`{bare}` category \"{category}\" is not one of the known values ({})",
            categories.join(", ")
        ));
    }
    values
        .entry("period")
        .or_insert_with(|| serde_json::Value::String(period.to_string()));
    Ok(Some(serde_json::Value::Object(values)))
}

/// Normalise a `tech_values` input into the object the screener API expects.
/// Accepts an object (used verbatim), a JSON string (parsed), and treats empty
/// string / empty object / null as "not provided".
fn normalize_tech_values(v: &serde_json::Value) -> Option<serde_json::Value> {
    match v {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) if s.trim().is_empty() => None,
        serde_json::Value::String(s) => serde_json::from_str(s)
            .ok()
            .filter(|parsed| !matches!(parsed, serde_json::Value::Object(o) if o.is_empty())),
        serde_json::Value::Object(o) if o.is_empty() => None,
        other => Some(other.clone()),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerSearchParam {
    /// Market: "US" | "HK" | "CN" | "SG".
    /// Mode A: overridden by the market embedded in the strategy; pass any value or omit.
    /// Mode B: required — determines which market to screen.
    pub market: Option<String>,

    /// Mode A — Strategy ID from screener_recommend_strategies screeners[].id.
    /// The tool auto-fetches the strategy and builds filters. Omit for Mode B.
    pub strategy_id: Option<String>,

    /// Mode B — Filter conditions, passed directly to the API. Omit for Mode A.
    ///
    /// Fundamental keys (pass with or without filter_ prefix):
    ///   pettm  pbmrq  roe  roa  netmargin
    ///   salesgrowthyoy  netincomegrowthyoy  marketcap(亿)
    ///   circulating_marketcap(亿)  prevclose  prevchg(%)
    ///   divyld  la  epsttm  netincome(亿)  sales(亿)  turnover_rate  balance(万)
    ///
    /// Technical indicator keys (tech_values with a category required; period
    /// defaults to the key suffix):
    ///   macd_day/week          → {"category":"goldenfork"|"deadcross"|"upzero"|"downzero"}
    ///   kdj_day/week, rsi_*    → {"category":"goldenfork"|"deadcross"}
    ///   boll_day/week          → {"category":"riseupperband"|"fallbelowband"|"risemiddleband"|"fallmiddleband"}
    // Schema is emitted inline (not via ScreenerCondition's `$ref`) and keeps
    // `tech_values` as a JSON string so the manifest stays OpenAI-function-calling
    // compatible (no `$defs`/`$ref`, no free-form object).
    #[serde(default)]
    #[schemars(schema_with = "conditions_schema")]
    pub conditions: Option<Vec<ScreenerCondition>>,

    /// Extra indicator keys to include in each result row (display-only, not used as filters).
    /// Same key naming as conditions (filter_ prefix added automatically).
    /// Example: ["marketcap", "prevclose", "epsttm"]
    pub extra_returns: Option<Vec<String>>,

    /// Indicator key to sort results by (e.g. "marketcap", "roe").
    /// Defaults to the first condition key. Must be one of the condition or extra_returns keys.
    pub sort_by_key: Option<String>,

    /// Sort order: "asc" | "desc" (default: "desc")
    pub sort_order: Option<String>,

    /// Page number, 0-based (default: 0)
    pub page: Option<u32>,
    /// Page size (default: 20, max: 100)
    pub size: Option<u32>,
}

pub async fn screener_search(
    mctx: &crate::tools::McpContext,
    p: ScreenerSearchParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();

    let mut suspects: Vec<String> = Vec::new();
    let (market, filters, returns) = if let Some(ref sid) = p.strategy_id {
        // Mode A: fetch strategy and build filters/returns automatically
        let strategy_path = format!("/v1/quote/ai/screener/strategy/{sid}");
        let raw: String = client
            .request(Method::GET, &strategy_path)
            .response::<String>()
            .send()
            .await
            .map_err(|e| Error::longbridge(e.into()))?;

        let strategy: serde_json::Value = serde_json::from_str(&raw).map_err(Error::Serialize)?;

        // AI endpoint: market is top-level; filters are under filter.filters[]
        let mkt = strategy
            .get("market")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty() && *s != "-")
            .map(|s| s.to_uppercase())
            .unwrap_or_else(|| p.market.as_deref().unwrap_or("US").to_uppercase());

        let mut filters: Vec<serde_json::Value> = Vec::new();
        let mut returns: Vec<String> = Vec::new();

        if let Some(f) = strategy
            .get("filter")
            .and_then(|f| f.get("filters"))
            .and_then(|v| v.as_array())
        {
            for ind in f {
                let raw_key = ind
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if raw_key.is_empty() || raw_key == "-" {
                    continue;
                }
                // Re-add filter_ prefix (stripped in strategy display, required by search API)
                let key = if raw_key.starts_with("filter_") {
                    raw_key
                } else {
                    format!("filter_{raw_key}")
                };
                let min = ind
                    .get("min")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let max = ind
                    .get("max")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tech_values = ind
                    .get("tech_values")
                    .cloned()
                    .filter(|v| v.is_object())
                    .unwrap_or_else(|| serde_json::json!({}));
                filters.push(serde_json::json!({
                    "key": key,
                    "min": min,
                    "max": max,
                    "tech_values": tech_values
                }));
                returns.push(key);
            }
        }

        (
            mkt,
            serde_json::Value::Array(filters),
            serde_json::Value::Array(returns.into_iter().map(serde_json::Value::String).collect()),
        )
    } else {
        // Mode B: each condition is a filter object.
        // The "filter_" prefix is added automatically to the key if missing,
        // consistent with extra_returns and sort_by_key.
        let mut filters: Vec<serde_json::Value> = Vec::new();
        let mut returns: Vec<String> = Vec::new();

        for cond in p.conditions.as_deref().unwrap_or(&[]) {
            let raw_key = cond.key.trim();
            if raw_key.is_empty() {
                continue;
            }
            let key = if raw_key.starts_with("filter_") {
                raw_key.to_string()
            } else {
                format!("filter_{raw_key}")
            };
            returns.push(key.clone());
            // Build the filter object with the normalised key, forwarding only
            // the documented fields. OpenAI strict mode sends every property,
            // filling unused ones with empty strings, so treat "" as absent.
            let mut f = serde_json::Map::new();
            f.insert("key".to_string(), serde_json::Value::String(key.clone()));
            if let Some(min) = cond.min.as_deref().filter(|s| !s.trim().is_empty()) {
                f.insert(
                    "min".to_string(),
                    serde_json::Value::String(min.to_string()),
                );
            }
            if let Some(max) = cond.max.as_deref().filter(|s| !s.trim().is_empty()) {
                f.insert(
                    "max".to_string(),
                    serde_json::Value::String(max.to_string()),
                );
            }
            let tech_values = cond.tech_values.as_ref().and_then(normalize_tech_values);
            if let Some(tech_values) = check_tech_values(&key, tech_values, &mut suspects)? {
                f.insert("tech_values".to_string(), tech_values);
            }
            filters.push(serde_json::Value::Object(f));
        }

        (
            p.market.as_deref().unwrap_or("US").to_uppercase(),
            serde_json::Value::Array(filters),
            serde_json::Value::Array(returns.into_iter().map(serde_json::Value::String).collect()),
        )
    };

    // Build final returns: condition keys + extra_returns + DEFAULT_RETURNS (deduplicated).
    let returns = {
        let mut all: Vec<serde_json::Value> = returns.as_array().cloned().unwrap_or_default();
        let extend = p
            .extra_returns
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|s| s.as_str())
            .chain(DEFAULT_RETURNS.iter().copied());
        for raw in extend {
            let key = if raw.starts_with("filter_") {
                raw.to_string()
            } else {
                format!("filter_{raw}")
            };
            if !all.contains(&serde_json::Value::String(key.clone())) {
                all.push(serde_json::Value::String(key));
            }
        }
        serde_json::Value::Array(all)
    };

    // Resolve sort_by_key → index into returns[].
    let sort_by: u32 = p.sort_by_key.as_deref().map_or(0, |raw_key| {
        let key = if raw_key.starts_with("filter_") {
            raw_key.to_string()
        } else {
            format!("filter_{raw_key}")
        };
        returns
            .as_array()
            .and_then(|arr| arr.iter().position(|v| v.as_str() == Some(key.as_str())))
            .unwrap_or(0) as u32
    });

    let sort_order: u32 = match p.sort_order.as_deref().unwrap_or("desc") {
        "asc" => 0,
        _ => 1,
    };

    let body = serde_json::json!({
        "market": market,
        "filters": filters,
        "returns": returns,
        "sort_by": sort_by,
        "sort_order": sort_order,
        "industries": [],
        "page": p.page.unwrap_or(0),
        "size": p.size.unwrap_or(20),
    });

    let resp: String = match client
        .request(Method::POST, "/v1/quote/ai/screener/search")
        .body(longbridge::httpclient::Json(body))
        .response::<String>()
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            let err: McpError = Error::longbridge(e.into()).into();
            if crate::tools::openapi_error_code_of(&err) != Some(SCREENER_REJECTED_CODE) {
                return Err(err);
            }
            let mut message = String::from(
                "The screener rejected these filters (2301500). Usual causes: a key that is not \
                 an indicator key from screener_indicators (e.g. `pb`, `name`, `symbol` are not \
                 keys; use `pbmrq`), also in extra_returns/sort_by_key; or a technical key whose \
                 tech_values are invalid. Call screener_indicators for the valid keys and values.",
            );
            if !suspects.is_empty() {
                message.push_str(" Likely culprit: ");
                message.push_str(&suspects.join("; "));
                message.push('.');
            }
            return Err(McpError::invalid_params(
                message,
                Some(serde_json::json!({ "error_code": "screener_rejected" })),
            ));
        }
    };

    let json = crate::serialize::transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    // Strip the filter_ prefix from indicators[].key (consistent with
    // screener_indicators and the conditions input), hoist the repeated
    // per-key name/unit labels into a top-level legend, and cap value precision.
    let json = postprocess_search_results(json);
    Ok(crate::tools::tool_result(json))
}

/// Post-process a screener_search response:
/// 1. Strip the `filter_` prefix from every `indicators[].key`.
/// 2. Hoist the per-key `name`/`unit` display labels — identical for a given
///    key across all rows — into a single top-level `legend` (`{ key: { name,
///    unit } }`), dropping them from each row's indicators.
/// 3. Cap indicator value precision at 6 dp (upstream emits ~15 fractional
///    digits on ratios like pettm/roe).
fn postprocess_search_results(json: String) -> String {
    let Ok(mut d) = serde_json::from_str::<serde_json::Value>(&json) else {
        return json;
    };
    let mut legend = serde_json::Map::new();
    if let Some(items) = d.get_mut("items").and_then(|v| v.as_array_mut()) {
        for item in items.iter_mut() {
            let Some(indicators) = item.get_mut("indicators").and_then(|v| v.as_array_mut()) else {
                continue;
            };
            for ind in indicators.iter_mut() {
                let Some(obj) = ind.as_object_mut() else {
                    continue;
                };
                let key = obj
                    .get("key")
                    .and_then(|v| v.as_str())
                    .map(|k| k.strip_prefix("filter_").unwrap_or(k).to_string());
                let Some(key) = key else {
                    continue;
                };
                let is_empty =
                    |v: &serde_json::Value| v.is_null() || v.as_str().is_some_and(str::is_empty);
                let name = obj.remove("name").filter(|v| !is_empty(v));
                let unit = obj.remove("unit").filter(|v| !is_empty(v));
                // Fill each label from the first NON-empty occurrence across
                // rows. Labels are stable per key upstream, but a blank first row
                // must not permanently shadow a later real label (and a key with
                // no label anywhere gets no legend entry at all).
                if name.is_some() || unit.is_some() {
                    let meta = legend
                        .entry(key.clone())
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let Some(meta) = meta.as_object_mut() {
                        if let Some(name) = name {
                            meta.entry("name").or_insert(name);
                        }
                        if let Some(unit) = unit {
                            meta.entry("unit").or_insert(unit);
                        }
                    }
                }
                obj.insert("key".to_string(), serde_json::Value::String(key));
            }
        }
    }
    crate::serialize::round_decimals(&mut d, 6);
    if let Some(obj) = d.as_object_mut()
        && !legend.is_empty()
    {
        obj.insert("legend".to_string(), serde_json::Value::Object(legend));
    }
    serde_json::to_string(&d).unwrap_or(json)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScreenerIndicatorsParam {
    /// Optional security symbol to filter indicators for a specific stock, e.g. "AAPL.US"
    pub symbol: Option<String>,
}

pub async fn screener_indicators(
    mctx: &crate::tools::McpContext,
    p: ScreenerIndicatorsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut params: Vec<(&str, &str)> = vec![];
    if let Some(ref sym) = p.symbol {
        params.push(("symbol", sym.as_str()));
    }
    let result = http_get_tool(&client, "/v1/quote/ai/screener/indicators", &params).await?;
    Ok(strip_filter_prefix_from_indicators(result))
}

/// Process screener_indicators response:
/// 1. Strip "filter_" prefix from key fields (consistent with all other screener tools).
/// 2. Build "tech_values" schema from "tech_indicators" so the AI knows what values to
///    pass for technical indicators (MACD/RSI/KDJ/BOLL).
///    tech_indicators[]{tech_key, tech_items[]{item_value, item_name}}
///    → tech_values: {tech_key: [{value, label}, ...]}
fn strip_filter_prefix_from_indicators(
    result: rmcp::model::CallToolResult,
) -> rmcp::model::CallToolResult {
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
    if let Some(groups) = d.get_mut("groups").and_then(|v| v.as_array_mut()) {
        for group in groups.iter_mut() {
            if let Some(indicators) = group.get_mut("indicators").and_then(|v| v.as_array_mut()) {
                for ind in indicators.iter_mut() {
                    let Some(obj) = ind.as_object_mut() else {
                        continue;
                    };
                    // Strip filter_ from key
                    if let Some(k) = obj.get("key").and_then(|v| v.as_str()) {
                        let stripped = k.strip_prefix("filter_").unwrap_or(k).to_string();
                        obj.insert("key".to_string(), serde_json::Value::String(stripped));
                    }
                    // Build tech_values schema from tech_indicators
                    if let Some(tech_inds) = obj.get("tech_indicators").and_then(|v| v.as_array()) {
                        let tv: serde_json::Map<String, serde_json::Value> = tech_inds
                            .iter()
                            .filter_map(|ti| {
                                let key = ti.get("tech_key")?.as_str()?.to_string();
                                let opts: Vec<serde_json::Value> = ti
                                    .get("tech_items")
                                    .and_then(|v| v.as_array())
                                    .map(|items| {
                                        items
                                            .iter()
                                            .map(|item| {
                                                serde_json::json!({
                                                    "value": item.get("item_value")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or(""),
                                                    "label": item.get("item_name")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or(""),
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                Some((key, serde_json::Value::Array(opts)))
                            })
                            .collect();
                        if !tv.is_empty() {
                            obj.insert("tech_values".to_string(), serde_json::Value::Object(tv));
                        }
                    }
                }
            }
        }
    }
    let Ok(json) = serde_json::to_string(&d) else {
        return result;
    };
    crate::tools::tool_result(json)
}

#[cfg(test)]
mod tests {
    use super::postprocess_search_results;

    #[test]
    fn technical_conditions_get_their_period_and_a_checked_category() {
        let mut suspects = Vec::new();
        let filled = super::check_tech_values(
            "filter_rsi_day",
            Some(serde_json::json!({"category": "goldenfork"})),
            &mut suspects,
        )
        .unwrap();
        assert_eq!(
            filled,
            Some(serde_json::json!({"category": "goldenfork", "period": "day"}))
        );
        assert!(
            suspects.is_empty(),
            "a known category is not a suspect: {suspects:?}"
        );

        // The value_type form the old docs advertised has no category: rejected
        // locally with the valid categories, instead of an upstream 500.
        let err = super::check_tech_values(
            "filter_rsi_week",
            Some(serde_json::json!({"value_type": "oversold"})),
            &mut suspects,
        )
        .unwrap_err();
        assert!(
            err.message.contains("goldenfork, deadcross"),
            "{}",
            err.message
        );

        // An unknown category is still sent (the list may lag upstream), but
        // recorded so an upstream rejection can name it.
        let sent = super::check_tech_values(
            "filter_boll_day",
            Some(serde_json::json!({"category": "breakthrough_up"})),
            &mut suspects,
        )
        .unwrap()
        .unwrap();
        assert_eq!(sent["period"], "day");
        assert!(
            suspects.len() == 1 && suspects[0].contains("riseupperband"),
            "{suspects:?}"
        );
    }

    #[test]
    fn fundamental_conditions_pass_through_untouched() {
        for key in ["filter_marketcap", "filter_pettm", "filter_turnover_rate"] {
            let checked = super::check_tech_values(key, None, &mut Vec::new()).unwrap();
            assert_eq!(checked, None, "{key}");
        }
    }

    #[test]
    fn postprocess_hoists_legend_strips_prefix_and_rounds() {
        let input = serde_json::json!({
            "total": 839,
            "items": [
                {"symbol": "2460.HK", "name": "华润饮料", "indicators": [
                    {"key": "filter_pettm", "name": "市盈率(TTM)", "value": "19.991124831097526", "unit": ""},
                    {"key": "filter_marketcap", "name": "市值", "value": "17027195860", "unit": "亿"}
                ]},
                {"symbol": "866.HK", "name": "中国秦发", "indicators": [
                    {"key": "filter_pettm", "name": "市盈率(TTM)", "value": "19.96675598973656", "unit": ""},
                    {"key": "filter_marketcap", "name": "市值", "value": "5609528857.974999", "unit": "亿"}
                ]}
            ]
        })
        .to_string();
        let out: serde_json::Value =
            serde_json::from_str(&postprocess_search_results(input)).expect("valid JSON");

        // Legend carries each key's labels exactly once.
        assert_eq!(out["legend"]["pettm"]["name"], "市盈率(TTM)");
        assert_eq!(out["legend"]["marketcap"]["unit"], "亿");
        // Empty unit is omitted from the legend entry.
        assert!(
            out["legend"]["pettm"].get("unit").is_none(),
            "an empty unit is not recorded"
        );

        let rows = out["items"].as_array().expect("items is an array");
        for row in rows {
            for ind in row["indicators"]
                .as_array()
                .expect("indicators is an array")
            {
                // filter_ prefix stripped; name/unit dropped from every row.
                assert!(
                    ind["key"]
                        .as_str()
                        .is_some_and(|k| !k.starts_with("filter_")),
                    "filter_ prefix is stripped"
                );
                assert!(ind.get("name").is_none(), "per-row name is hoisted away");
                assert!(ind.get("unit").is_none(), "per-row unit is hoisted away");
            }
        }
        // Bogus precision is capped; already-short/integer values untouched.
        assert_eq!(rows[0]["indicators"][0]["value"], "19.991125");
        assert_eq!(
            rows[0]["indicators"][1]["value"], "17027195860",
            "integers are left alone"
        );
        assert_eq!(
            rows[1]["indicators"][1]["value"], "5609528857.974999",
            "exactly-6dp values are left alone"
        );
    }

    #[test]
    fn postprocess_passes_through_unparsable_payload() {
        assert_eq!(postprocess_search_results("not json".into()), "not json");
    }
}
