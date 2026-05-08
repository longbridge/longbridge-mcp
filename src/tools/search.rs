//! Search tools — news and community topic search.
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::http_client::{http_get_tool, http_get_tool_unix};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchNewsParam {
    /// Search keyword
    pub keyword: String,
    /// Pagination cursor: score of the last result in the previous page
    pub score: Option<String>,
    /// Pagination cursor: publish timestamp of the last result in the previous page
    pub publish_at_timestamp: Option<String>,
    /// Pagination cursor: id of the last result in the previous page
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchTopicsParam {
    /// Search keyword
    pub keyword: String,
    /// Pagination cursor: lowest score in the previous page
    pub score: Option<String>,
    /// Pagination cursor: timestamp of the last topic in the previous page
    pub created_at_timestamp: Option<String>,
    /// Topic type: "1" for long articles; omit for all types
    pub topic_type: Option<String>,
    /// Filter results to topics posted by this member_id
    pub member_id: Option<String>,
}

pub async fn search_news(
    mctx: &crate::tools::McpContext,
    p: SearchNewsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut params: Vec<(&str, &str)> = vec![("k", p.keyword.as_str())];
    if let Some(ref s) = p.score {
        params.push(("score", s.as_str()));
    }
    if let Some(ref s) = p.publish_at_timestamp {
        params.push(("publish_at_timestamp", s.as_str()));
    }
    if let Some(ref s) = p.id {
        params.push(("id", s.as_str()));
    }
    http_get_tool(&client, "/v1/search/news", &params).await
}

pub async fn search_topics(
    mctx: &crate::tools::McpContext,
    p: SearchTopicsParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut params: Vec<(&str, &str)> = vec![("k", p.keyword.as_str())];
    if let Some(ref s) = p.score {
        params.push(("score", s.as_str()));
    }
    if let Some(ref s) = p.created_at_timestamp {
        params.push(("created_at_timestamp", s.as_str()));
    }
    if let Some(ref t) = p.topic_type {
        params.push(("type", t.as_str()));
    }
    if let Some(ref m) = p.member_id {
        params.push(("member_id", m.as_str()));
    }
    http_get_tool_unix(
        &client,
        "/v1/search/topics",
        &params,
        &["topic_list.*.created_at_timestamp"],
    )
    .await
}
