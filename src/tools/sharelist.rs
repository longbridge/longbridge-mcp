//! Community sharelist (shared stock list) tools.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::tools::support::http_client::{http_delete_tool, http_get_tool, http_post_tool};
use crate::tools::support::tolerant::{tolerant_option_u32, tolerant_vec_string};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistCountParam {
    /// Number of lists to return (default 20)
    #[serde(default, deserialize_with = "tolerant_option_u32")]
    pub count: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistIdParam {
    /// Sharelist ID
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistCreateParam {
    /// List name (also used as description if `description` is omitted).
    pub name: String,
    /// List description. Defaults to `name` when omitted.
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SharelistItemsParam {
    /// Sharelist ID
    pub id: String,
    /// Security symbols, e.g. ["AAPL.US", "700.HK"]
    #[serde(deserialize_with = "tolerant_vec_string")]
    pub symbols: Vec<String>,
}

pub async fn sharelist_list(
    mctx: &crate::tools::McpContext,
    p: SharelistCountParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let size = p.count.unwrap_or(20).to_string();
    let params = [
        ("size", size.as_str()),
        ("self", "true"),
        ("subscription", "true"),
    ];
    http_get_tool(&client, "/v1/sharelists", &params).await
}

pub async fn sharelist_detail(
    mctx: &crate::tools::McpContext,
    p: SharelistIdParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}", p.id);
    let params = [
        ("constituent", "true"),
        ("quote", "true"),
        ("subscription", "true"),
    ];
    http_get_tool(&client, &path, &params).await
}

pub async fn sharelist_create(
    mctx: &crate::tools::McpContext,
    p: SharelistCreateParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let desc = p.description.as_deref().unwrap_or(&p.name).to_string();
    let body = serde_json::json!({
        "name": p.name,
        "description": desc,
        "cover": "https://pub.pbkrs.com/files/202107/kaJSk6BsvPt6NJ3Q/sharelist_v1.png",
    });
    http_post_tool(&client, "/v1/sharelists", body).await
}

pub async fn sharelist_delete(
    mctx: &crate::tools::McpContext,
    p: SharelistIdParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}", p.id);
    http_delete_tool(&client, &path, serde_json::json!({})).await
}

pub async fn sharelist_add(
    mctx: &crate::tools::McpContext,
    p: SharelistItemsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}/items", p.id);
    let cids: Vec<String> = p.symbols.iter().map(|s| symbol_to_counter_id(s)).collect();
    let body = serde_json::json!({ "counter_ids": cids.join(",") });
    http_post_tool(&client, &path, body).await
}

pub async fn sharelist_remove(
    mctx: &crate::tools::McpContext,
    p: SharelistItemsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}/items", p.id);
    let cids: Vec<String> = p.symbols.iter().map(|s| symbol_to_counter_id(s)).collect();
    let body = serde_json::json!({ "counter_ids": cids.join(",") });
    http_delete_tool(&client, &path, body).await
}

pub async fn sharelist_sort(
    mctx: &crate::tools::McpContext,
    p: SharelistItemsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let path = format!("/v1/sharelists/{}/items/sort", p.id);
    let cids: Vec<String> = p.symbols.iter().map(|s| symbol_to_counter_id(s)).collect();
    let body = serde_json::json!({ "counter_ids": cids.join(",") });
    http_post_tool(&client, &path, body).await
}

pub async fn sharelist_popular(
    mctx: &crate::tools::McpContext,
    p: SharelistCountParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let size = p.count.unwrap_or(20).to_string();
    let params = [("size", size.as_str())];
    http_get_tool(&client, "/v1/sharelists/popular", &params).await
}
