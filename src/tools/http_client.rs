use longbridge::httpclient::{HttpClient, Json};
use reqwest::Method;
use rmcp::model::{CallToolResult, Content, ErrorData as McpError};

use crate::error::Error;
use crate::serialize::transform_json;

fn result_from_raw_json(raw: &str) -> Result<CallToolResult, McpError> {
    let json = transform_json(raw.as_bytes()).map_err(Error::Serialize)?;
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
