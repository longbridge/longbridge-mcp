use longbridge::ContentContext;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::tolerant::{tolerant_option_i32, tolerant_option_vec_string};
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicIdParam {
    /// Topic ID
    pub topic_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicCreateParam {
    /// Topic title. Required when topic_type is "article", optional for "post".
    pub title: String,
    /// Topic body. "post" type is plain text only; "article" type accepts Markdown.
    pub body: String,
    /// Related security symbols, e.g. ["700.HK", "TSLA.US"] (max 10).
    #[serde(default, deserialize_with = "tolerant_option_vec_string")]
    pub symbols: Option<Vec<String>>,
    /// Topic type: "post" (default, plain text) or "article" (Markdown, title required).
    pub topic_type: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicCreateReplyParam {
    /// Topic ID to reply to.
    pub topic_id: String,
    /// Reply body (plain text only).
    pub body: String,
    /// Optional parent reply ID for nested replies. Get IDs from `topic_replies`. Omit for a top-level reply.
    pub reply_to_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicRepliesParam {
    /// Topic ID.
    pub topic_id: String,
    /// Page number, 1-based (default: 1).
    #[serde(default, deserialize_with = "tolerant_option_i32")]
    pub page: Option<i32>,
    /// Records per page, 1-50 (default: 20).
    #[serde(default, deserialize_with = "tolerant_option_i32")]
    pub size: Option<i32>,
}

pub async fn news(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(mctx.create_config());
    let result = ctx.news(p.symbol).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewsIdParam {
    /// News article ID (numeric), e.g. "7123456789012345678". Get IDs from `news` or `news_search`.
    #[serde(deserialize_with = "tolerant_id_string")]
    #[schemars(with = "String")]
    pub id: String,
}

/// Accept the article id as either a JSON string or a bare number — models
/// often echo the numeric id from `news_search` results as a number.
fn tolerant_id_string<'de, D: rmcp::serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        String(String),
        Number(i64),
    }
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
    })
}

/// `GET /v1/content/news/{id}` — one article's full detail. This endpoint is
/// not part of the language SDKs, so it is called through the signed raw HTTP
/// client.
pub async fn news_detail(
    mctx: &crate::tools::McpContext,
    p: NewsIdParam,
) -> Result<CallToolResult, McpError> {
    use longbridge::httpclient::{Json, Method};

    let id: i64 = p.id.trim().parse().map_err(|_| {
        McpError::invalid_params(
            "id must be a numeric news article ID (from news/news_search)",
            None,
        )
    })?;
    let resp = mctx
        .create_http_client()
        .request(Method::GET, format!("/v1/content/news/{id}"))
        .response::<Json<serde_json::Value>>()
        .send()
        .await
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    // The tool declares an outputSchema, so it must return a structured
    // object — never a bare `null` if the payload is missing `item`.
    let item = &resp.0["item"];
    if !item.is_object() {
        return Err(McpError::internal_error(
            format!("news article {id} not found or response missing `item`"),
            None,
        ));
    }
    tool_json(item)
}

pub async fn topic(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(mctx.create_config());
    let result = ctx.topics(p.symbol).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn topic_detail(
    mctx: &crate::tools::McpContext,
    p: TopicIdParam,
) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(mctx.create_config());
    let result = ctx
        .topic_detail(p.topic_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn topic_replies(
    mctx: &crate::tools::McpContext,
    p: TopicRepliesParam,
) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(mctx.create_config());
    let opts = longbridge::content::ListTopicRepliesOptions {
        page: p.page,
        size: p.size,
    };
    let result = ctx
        .list_topic_replies(p.topic_id, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn topic_create(
    mctx: &crate::tools::McpContext,
    p: TopicCreateParam,
) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(mctx.create_config());
    let opts = longbridge::content::CreateTopicOptions {
        title: p.title,
        body: p.body,
        topic_type: p.topic_type,
        tickers: p.symbols,
        hashtags: None,
    };
    let id = ctx.create_topic(opts).await.map_err(Error::longbridge)?;
    tool_json(&serde_json::json!({ "id": id }))
}

pub async fn topic_create_reply(
    mctx: &crate::tools::McpContext,
    p: TopicCreateReplyParam,
) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(mctx.create_config());
    let opts = longbridge::content::CreateReplyOptions {
        body: p.body,
        reply_to_id: p.reply_to_id,
    };
    let result = ctx
        .create_topic_reply(p.topic_id, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}
