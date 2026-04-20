use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::tools::http_client::{http_delete_tool, http_get_tool, http_post_tool};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistListParam {
    /// Maximum number of results
    pub size: Option<u32>,
    /// Include own sharelists (default true)
    pub include_self: Option<bool>,
    /// Include subscribed sharelists (default false)
    pub subscription: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistIdParam {
    /// Sharelist ID
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistCreateParam {
    /// Sharelist name
    pub name: String,
    /// Description (optional)
    pub description: Option<String>,
    /// Cover image URL (optional)
    pub cover: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistItemsParam {
    /// Sharelist ID
    pub id: String,
    /// Security symbols, e.g. ["700.HK", "AAPL.US"]
    pub symbols: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistPopularParam {
    /// Maximum number of results
    pub size: Option<u32>,
}

pub async fn sharelist_list(
    mctx: &crate::tools::McpContext,
    p: SharelistListParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let size_str = p.size.map(|s| s.to_string());
    let mut params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref s) = size_str {
        params.push(("size", s.as_str()));
    }
    if p.include_self.unwrap_or(true) {
        params.push(("self", "true"));
    }
    if p.subscription.unwrap_or(false) {
        params.push(("subscription", "true"));
    }
    http_get_tool(&client, "/v1/sharelists", &params).await
}

pub async fn sharelist_detail(
    mctx: &crate::tools::McpContext,
    p: SharelistIdParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}", p.id);
    http_get_tool(
        &client,
        &path,
        &[("constituent", "true"), ("quote", "true")],
    )
    .await
}

pub async fn sharelist_create(
    mctx: &crate::tools::McpContext,
    p: SharelistCreateParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut body = serde_json::json!({ "name": p.name });
    if let Some(desc) = p.description {
        body["description"] = serde_json::Value::String(desc);
    }
    if let Some(cover) = p.cover {
        body["cover"] = serde_json::Value::String(cover);
    }
    http_post_tool(&client, "/v1/sharelists", body).await
}

pub async fn sharelist_delete(
    mctx: &crate::tools::McpContext,
    p: SharelistIdParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}", p.id);
    http_delete_tool(&client, &path, serde_json::Value::Null).await
}

pub async fn sharelist_add_items(
    mctx: &crate::tools::McpContext,
    p: SharelistItemsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}/items", p.id);
    let counter_ids: Vec<String> = p.symbols.iter().map(|s| symbol_to_counter_id(s)).collect();
    let body = serde_json::json!({ "counter_ids": counter_ids.join(",") });
    http_post_tool(&client, &path, body).await
}

pub async fn sharelist_remove_items(
    mctx: &crate::tools::McpContext,
    p: SharelistItemsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}/items", p.id);
    let counter_ids: Vec<String> = p.symbols.iter().map(|s| symbol_to_counter_id(s)).collect();
    let body = serde_json::json!({ "counter_ids": counter_ids.join(",") });
    http_delete_tool(&client, &path, body).await
}

pub async fn sharelist_popular(
    mctx: &crate::tools::McpContext,
    p: SharelistPopularParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let size_str = p.size.map(|s| s.to_string());
    let mut params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref s) = size_str {
        params.push(("size", s.as_str()));
    }
    http_get_tool(&client, "/v1/sharelists/popular", &params).await
}
