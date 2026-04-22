use longbridge::httpclient::{HttpClient, Json};
use reqwest::Method;
use rmcp::model::{CallToolResult, Content, ErrorData as McpError};

use crate::error::Error;
use crate::serialize::{convert_unix_paths, transform_json};

fn result_from_raw_json(raw: &str) -> Result<CallToolResult, McpError> {
    let json = transform_json(raw.as_bytes()).map_err(Error::Serialize)?;
    Ok(CallToolResult::success(vec![Content::text(json)]))
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
    Ok(CallToolResult::success(vec![Content::text(json)]))
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
        .map_err(|e| Error::Other(e.to_string()))?;
    result_from_raw_json(&resp)
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
        .map_err(|e| Error::Other(e.to_string()))?;
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
        .map_err(|e| Error::Other(e.to_string()))?;
    result_from_raw_json(&resp)
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
        .map_err(|e| Error::Other(e.to_string()))?;
    result_from_raw_json(&resp)
}
