use longbridge::ContentContext;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::{create_config, tool_json};

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
    /// Topic title
    pub title: String,
    /// Topic body content
    pub body: String,
    /// Related security symbols
    pub symbols: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicCreateReplyParam {
    /// Topic ID to reply to
    pub topic_id: String,
    /// Reply body content
    pub body: String,
}

pub async fn news(token: &str, p: SymbolParam) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(create_config(token));
    let result = ctx.news(p.symbol).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn topic(token: &str, p: SymbolParam) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(create_config(token));
    let result = ctx.topics(p.symbol).await.map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn topic_detail(token: &str, p: TopicIdParam) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(create_config(token));
    let result = ctx
        .topic_detail(p.topic_id)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn topic_replies(token: &str, p: TopicIdParam) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(create_config(token));
    let result = ctx
        .list_topic_replies(p.topic_id, Default::default())
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}

pub async fn topic_create(token: &str, p: TopicCreateParam) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(create_config(token));
    let opts = longbridge::content::CreateTopicOptions {
        title: p.title,
        body: p.body,
        topic_type: None,
        tickers: p.symbols,
        hashtags: None,
    };
    let id = ctx.create_topic(opts).await.map_err(Error::longbridge)?;
    tool_json(&serde_json::json!({ "id": id }))
}

pub async fn topic_create_reply(
    token: &str,
    p: TopicCreateReplyParam,
) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(create_config(token));
    let opts = longbridge::content::CreateReplyOptions {
        body: p.body,
        reply_to_id: None,
    };
    let result = ctx
        .create_topic_reply(p.topic_id, opts)
        .await
        .map_err(Error::longbridge)?;
    tool_json(&result)
}
