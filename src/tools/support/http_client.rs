use longbridge::httpclient::{HttpClient, Json};
use reqwest::Method;
use rmcp::model::{CallToolResult, ContentBlock, ErrorData as McpError};

use crate::error::Error;
use crate::serialize::{convert_unix_paths, drop_redundant_counter_ids, transform_json};

/// Build a tool result from a transformed JSON string, additionally populating
/// `structured_content` when the JSON is an object. MCP requires a tool that
/// declares an `output_schema` to return `structuredContent`; mirroring the
/// SDK-typed path (`crate::tools::tool_result`) here keeps every object-shaped
/// response spec-compliant whether or not the tool currently declares a schema.
/// Array- or scalar-rooted responses leave `structured_content` unset (the MCP
/// `structuredContent` field must be an object).
///
/// Any redundant `counter_id`/`counter_ids` a passthrough response echoes
/// alongside the equivalent `symbol`/`symbols` is dropped here, so every
/// passthrough tool is symbol-based without a per-tool cleanup call.
fn success_with_structured(json: String) -> CallToolResult {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&json) else {
        // `transform_json` always emits valid JSON; on the theoretical parse
        // failure, fall back to the raw text with no structured content.
        return CallToolResult::success(vec![ContentBlock::text(json)]);
    };
    drop_redundant_counter_ids(&mut value);
    let json = serde_json::to_string(&value).unwrap_or(json);
    let structured = value.is_object().then_some(value);
    let mut result = CallToolResult::success(vec![ContentBlock::text(json)]);
    result.structured_content = structured;
    result
}

fn result_from_raw_json(raw: &str) -> Result<CallToolResult, McpError> {
    let json = transform_json(raw.as_bytes()).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

/// Like `result_from_raw_json` but additionally converts unix-seconds strings
/// at the given paths to RFC3339. Paths are evaluated against the **post-
/// transform** JSON (after snake_case + `_at` handling), so they should be
/// written using the output field names (e.g. `statistics.trade_date.*`).
fn result_from_raw_json_with_unix_paths(
    raw: &str,
    unix_paths: &[&str],
) -> Result<CallToolResult, McpError> {
    let transformed = transform_json(raw.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    convert_unix_paths(&mut value, unix_paths);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

/// GET `path` and return the response as a transformed JSON `Value` (snake_case
/// keys, `*_at` timestamps as RFC3339), for tools that reshape the payload
/// before returning it. Pair with [`value_result`].
pub async fn http_get_value(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
) -> Result<serde_json::Value, McpError> {
    let params: Vec<(&str, &str)> = params.to_vec();
    let resp: String = client
        .request(Method::GET, path)
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let transformed = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    Ok(serde_json::from_str(&transformed).map_err(Error::Serialize)?)
}

/// Attempts [`http_get_value_paced`] makes before giving up on a rate limit.
const PACED_ATTEMPTS: usize = 3;

/// [`http_get_value`] for a tool that sends several requests to the same
/// route in one call (paging, or one request per filter value). The gateway
/// rate-limits these routes to about one call a second per member, so a
/// rate-limited request waits the delay the upstream suggests and is retried
/// here, instead of failing the whole tool call.
pub async fn http_get_value_paced(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
) -> Result<serde_json::Value, McpError> {
    let mut attempt = 1;
    loop {
        match http_get_value(client, path, params).await {
            Err(err) if attempt < PACED_ATTEMPTS && crate::tools::is_backoff(&err) => {
                tokio::time::sleep(crate::tools::rate_limit_delay(&err)).await;
                attempt += 1;
            }
            result => return result,
        }
    }
}

/// Rewrite every RFC3339 UTC timestamp (`…Z`) in `value` as Hong Kong time
/// (`…+08:00`), at any depth. For the AP-account tools, whose users read dates
/// as Hong Kong days: a deposit at 01:00 HKT must not read as the day before.
pub fn localize_hong_kong(value: &mut serde_json::Value) {
    use time::format_description::well_known::Rfc3339;
    match value {
        serde_json::Value::String(text) if text.ends_with('Z') => {
            if let Ok(utc) = time::OffsetDateTime::parse(text, &Rfc3339)
                && let Ok(local) = utc.to_offset(time::macros::offset!(+8)).format(&Rfc3339)
            {
                *text = local;
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(localize_hong_kong),
        serde_json::Value::Object(map) => map.values_mut().for_each(localize_hong_kong),
        _ => {}
    }
}

/// [`value_result`] for an AP-account tool: timestamps in Hong Kong time.
pub fn value_result_hong_kong(mut value: serde_json::Value) -> Result<CallToolResult, McpError> {
    localize_hong_kong(&mut value);
    value_result(&value)
}

/// Build a tool result from an already reshaped JSON `Value`.
pub fn value_result(value: &serde_json::Value) -> Result<CallToolResult, McpError> {
    let json = serde_json::to_string(value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

pub async fn http_get_tool(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
) -> Result<CallToolResult, McpError> {
    let params: Vec<(&str, &str)> = params.to_vec();
    let resp: String = client
        .request(Method::GET, path)
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    result_from_raw_json(&resp)
}

/// Same as `http_get_tool`, but after the standard transform runs, the given
/// (snake_case) `drop` keys are removed from every object in the response at any
/// depth. For passthrough tools that echo upstream fields with no analytic
/// value (duplicate `code`, derivable `market`, constant flags, display URLs).
pub async fn http_get_tool_dropping(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
    drop: &[&str],
) -> Result<CallToolResult, McpError> {
    let params: Vec<(&str, &str)> = params.to_vec();
    let resp: String = client
        .request(Method::GET, path)
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let json = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value = serde_json::from_str(&json).map_err(Error::Serialize)?;
    crate::serialize::drop_keys(&mut value, drop);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

/// Combines `http_get_tool_unix` (unix-seconds → RFC3339 at `unix_paths`) with
/// `drop` key removal, for passthrough tools that need both.
pub async fn http_get_tool_unix_dropping(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
    unix_paths: &[&str],
    drop: &[&str],
) -> Result<CallToolResult, McpError> {
    let params: Vec<(&str, &str)> = params.to_vec();
    let resp: String = client
        .request(Method::GET, path)
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let transformed = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    convert_unix_paths(&mut value, unix_paths);
    crate::serialize::drop_keys(&mut value, drop);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

/// Same as `http_get_tool`, but after the standard transform runs, every string
/// number in the response with more than `dp` fractional digits is rounded to
/// `dp` places. For passthrough tools whose upstream emits absurd precision
/// (e.g. 16-19 fractional digits on valuation ratios) that no consumer needs.
pub async fn http_get_tool_rounding(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
    dp: u32,
) -> Result<CallToolResult, McpError> {
    let params: Vec<(&str, &str)> = params.to_vec();
    let resp: String = client
        .request(Method::GET, path)
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let transformed = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    crate::serialize::round_decimals(&mut value, dp);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

/// Same as `http_get_tool`, but after the standard transform runs, every plain
/// decimal string has its non-significant trailing zeros stripped (lossless).
/// For passthrough tools whose upstream pads integer counts with a fake
/// fractional part (e.g. share-count deltas stored as `"123.0000"`).
pub async fn http_get_tool_trimming_zeros(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
) -> Result<CallToolResult, McpError> {
    let params: Vec<(&str, &str)> = params.to_vec();
    let resp: String = client
        .request(Method::GET, path)
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let transformed = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    crate::serialize::strip_trailing_zeros(&mut value);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

/// Combines `http_get_tool_unix` (unix-seconds → RFC3339 at `unix_paths`) with
/// decimal-precision capping: every string number in the response with more
/// than `dp` fractional digits is rounded to `dp` places. For passthrough tools
/// whose upstream emits absurd precision (e.g. 19-decimal prices) that no
/// consumer needs.
pub async fn http_get_tool_unix_rounding(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
    unix_paths: &[&str],
    dp: u32,
) -> Result<CallToolResult, McpError> {
    let params: Vec<(&str, &str)> = params.to_vec();
    let resp: String = client
        .request(Method::GET, path)
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let transformed = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    convert_unix_paths(&mut value, unix_paths);
    crate::serialize::round_decimals(&mut value, dp);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

/// Same as `http_get_tool`, but after the standard transform runs, the
/// specified `unix_paths` are walked and any unix-seconds strings found are
/// converted to RFC3339 in place. Use this for tools whose upstream returns
/// unix timestamps in fields whose names don't end with `_at` (e.g.
/// `timestamp`, `start_date`, `trade_date`).
pub async fn http_get_tool_unix(
    client: &HttpClient,
    path: &str,
    params: &[(&str, &str)],
    unix_paths: &[&str],
) -> Result<CallToolResult, McpError> {
    let params: Vec<(&str, &str)> = params.to_vec();
    let resp: String = client
        .request(Method::GET, path)
        .query_params(params)
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    result_from_raw_json_with_unix_paths(&resp, unix_paths)
}

pub async fn http_post_tool(
    client: &HttpClient,
    path: &str,
    body: serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let resp: String = client
        .request(Method::POST, path)
        .body(Json(body))
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    result_from_raw_json(&resp)
}

/// POST `body` to `path`, then reshape the response to match the pre-migration
/// (SDK-typed) output shape: rename top-level keys, recursively drop keys,
/// convert unix timestamp paths, and optionally strip padded trailing zeros.
///
/// Used by the quote WS→HTTP migration so a gateway proto-JSON response keeps the
/// same tool-output shape the WS `QuoteContext` path produced.
/// POST `body` to `path` and return the raw response body as a `String`, or the
/// underlying `longbridge::Error` (so callers can inspect `openapi_error_code()`,
/// e.g. the candlestick count-boundary retry). No transform/reshape is applied.
// Returns the un-boxed `longbridge::Error` deliberately: callers inspect
// `openapi_error_code()`, and `with_candlestick_count_boundary_retry` requires a
// `Future<Output = Result<_, longbridge::Error>>` (it boxes the error itself).
#[allow(clippy::result_large_err)]
pub async fn http_post_raw(
    client: &HttpClient,
    path: &str,
    body: serde_json::Value,
) -> Result<String, longbridge::Error> {
    client
        .request(Method::POST, path)
        .body(Json(body))
        .response::<String>()
        .send()
        .await
        .map_err(Into::into)
}

/// POST `body` to `path` and return the reshaped JSON `Value`: base transform,
/// unwrap the proto container field, rename keys, convert unix timestamps, drop
/// keys — the shared building block for the quote WS→HTTP migration. Callers that
/// need extra per-endpoint steps (enum int→name, null/zero stripping, session
/// normalization) apply them to the returned value before building the result.
pub async fn http_post_value(
    client: &HttpClient,
    path: &str,
    body: serde_json::Value,
    unwrap: Option<&str>,
    renames: &[(&str, &str)],
    drop: &[&str],
    unix_paths: &[&str],
) -> Result<serde_json::Value, McpError> {
    let resp: String = client
        .request(Method::POST, path)
        .body(Json(body))
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let transformed = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    // The gateway wraps the payload in the proto response message's single
    // container field (e.g. `secu_quote`, `lines`); unwrap it so the tool root
    // matches the SDK's (array- or object-rooted) shape.
    if let Some(key) = unwrap
        && let Some(inner) = value.get_mut(key).map(serde_json::Value::take)
    {
        value = inner;
    }
    crate::serialize::rename_keys(&mut value, renames);
    convert_unix_paths(&mut value, unix_paths);
    crate::serialize::drop_keys(&mut value, drop);
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
pub async fn http_post_tool_reshape(
    client: &HttpClient,
    path: &str,
    body: serde_json::Value,
    unwrap: Option<&str>,
    renames: &[(&str, &str)],
    drop: &[&str],
    unix_paths: &[&str],
    strip_zeros: bool,
) -> Result<CallToolResult, McpError> {
    let mut value = http_post_value(client, path, body, unwrap, renames, drop, unix_paths).await?;
    if strip_zeros {
        crate::serialize::strip_trailing_zeros(&mut value);
    }
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

/// POST passthrough with unix conversion at `unix_paths` and `drop` key
/// removal.
pub async fn http_post_tool_unix_dropping(
    client: &HttpClient,
    path: &str,
    body: serde_json::Value,
    unix_paths: &[&str],
    drop: &[&str],
) -> Result<CallToolResult, McpError> {
    let resp: String = client
        .request(Method::POST, path)
        .body(Json(body))
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let transformed = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    let mut value: serde_json::Value =
        serde_json::from_str(&transformed).map_err(Error::Serialize)?;
    convert_unix_paths(&mut value, unix_paths);
    crate::serialize::drop_keys(&mut value, drop);
    let json = serde_json::to_string(&value).map_err(Error::Serialize)?;
    Ok(success_with_structured(json))
}

pub async fn http_delete_tool(
    client: &HttpClient,
    path: &str,
    body: serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let resp: String = client
        .request(Method::DELETE, path)
        .body(Json(body))
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    result_from_raw_json(&resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localize_hong_kong_shifts_utc_only() {
        let mut value = serde_json::json!({
            "created_at": "2026-11-25T18:00:00Z",
            "nested": [{"send_time": "2026-01-01T00:00:00Z"}],
            "note": "Z",
            "already": "2026-11-25T18:00:00+08:00",
        });
        localize_hong_kong(&mut value);
        assert_eq!(
            value["created_at"], "2026-11-26T02:00:00+08:00",
            "next HK day"
        );
        assert_eq!(
            value["nested"][0]["send_time"], "2026-01-01T08:00:00+08:00",
            "nested"
        );
        assert_eq!(value["note"], "Z", "non-timestamps untouched");
        assert_eq!(value["already"], "2026-11-25T18:00:00+08:00", "offset kept");
    }
}
