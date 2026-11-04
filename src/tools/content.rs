use longbridge::ContentContext;
use longbridge::httpclient::{Json, Method};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::tools::support::tolerant::{
    tolerant_option_i32, tolerant_option_usize, tolerant_option_vec_string,
};
use crate::tools::tool_json;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewsParam {
    /// Security symbol, e.g. "700.HK". Use the canonical form — a padded code like "00700.HK" returns an empty record, not an error.
    pub symbol: String,
    /// Number of articles to return, newest first. Defaults to 20 (~700 bytes
    /// each); the upstream returns about 50.
    #[serde(default, deserialize_with = "tolerant_option_usize")]
    #[schemars(extend("default" = 20))]
    pub count: Option<usize>,
}

/// Articles `news` returns when `count` is omitted.
const DEFAULT_NEWS_COUNT: usize = 20;

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
    #[schemars(extend("default" = 1))]
    pub page: Option<i32>,
    /// Records per page, 1-50 (default: 20).
    #[serde(default, deserialize_with = "tolerant_option_i32")]
    #[schemars(extend("default" = 20))]
    pub size: Option<i32>,
}

pub async fn news(
    mctx: &crate::tools::McpContext,
    p: NewsParam,
) -> Result<CallToolResult, McpError> {
    // Raw passthrough of `GET /v1/content/{symbol}/news` rather than the SDK's
    // typed `NewsItem`, so fields the upstream adds (the `content_via` /
    // `source*` citation fields) reach callers without an SDK release.
    let resp = mctx
        .create_http_client()
        .request(Method::GET, format!("/v1/content/{}/news", p.symbol))
        .response::<Json<serde_json::Value>>()
        .send()
        .await
        .map_err(|e| Error::longbridge(e.into()))?;
    let mut value = match resp.0.get("items") {
        Some(items) if items.is_array() => items.clone(),
        _ => serde_json::Value::Array(Vec::new()),
    };
    shape_news(&mut value, p.count);
    tool_json(&value)
}

/// Normalize the `news` items in place: RFC3339 `published_at`, readable
/// tickers in `description`, newest first, at most `count` items.
fn shape_news(value: &mut serde_json::Value, count: Option<usize>) {
    crate::serialize::convert_unix_paths(value, &["*.published_at"]);
    // News items sourced from community posts embed `[st]…#Name.HK[/st]` ticker
    // markup in their description; rewrite it to the readable ticker.
    crate::serialize::strip_cashtags_in_field(value, "description");
    sort_newest_first(value);
    if let Some(items) = value.as_array_mut() {
        items.truncate(count.unwrap_or(DEFAULT_NEWS_COUNT).max(1));
    }
}

fn sort_newest_first(value: &mut serde_json::Value) {
    if let Some(items) = value.as_array_mut() {
        items.sort_by(|a, b| {
            let at =
                |v: &serde_json::Value| v["published_at"].as_str().unwrap_or_default().to_string();
            at(b).cmp(&at(a))
        });
    }
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
    let value = serde_json::Value::deserialize(d)?;
    match value {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(|n| n.to_string())
            .ok_or_else(|| rmcp::serde::de::Error::custom("id must be an integer article ID")),
        other => Err(rmcp::serde::de::Error::custom(format!(
            "id must be a string or integer article ID, got {other}"
        ))),
    }
}

/// `GET /v1/content/news/{id}` — one article's full detail. This endpoint is
/// not part of the language SDKs, so it is called through the signed raw HTTP
/// client.
pub async fn news_detail(
    mctx: &crate::tools::McpContext,
    p: NewsIdParam,
) -> Result<CallToolResult, McpError> {
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
        .map_err(|e| match &e {
            // 1901107: content does not exist or has been deleted — the
            // caller's id was wrong, not a server fault.
            longbridge::httpclient::HttpClientError::OpenApi { code: 1901107, .. } => {
                McpError::invalid_params(
                    format!("news article {id} not found; get a valid id from news/news_search"),
                    None,
                )
            }
            _ => Error::longbridge(e.into()).into(),
        })?;
    // The tool declares an outputSchema, so it must return a structured
    // object — never a bare `null` if the payload is missing `item`.
    let item = &resp.0["item"];
    if !item.is_object() {
        // The id didn't resolve to an article — that's the caller's input,
        // not a server fault, so signal it as such.
        return Err(McpError::invalid_params(
            format!("news article {id} not found; get a valid id from news/news_search"),
            None,
        ));
    }
    let mut item = item.clone();
    // Rewrite `[st]…#Name.HK[/st]` ticker markup to the readable ticker, and
    // drop the display-only author avatar URL.
    for field in ["description", "body"] {
        crate::serialize::strip_cashtags_in_field(&mut item, field);
    }
    crate::serialize::drop_keys(&mut item, &["avatar"]);
    tool_json(&item)
}

pub async fn topic(
    mctx: &crate::tools::McpContext,
    p: SymbolParam,
) -> Result<CallToolResult, McpError> {
    let ctx = ContentContext::new(mctx.create_config());
    let result = ctx.topics(p.symbol).await.map_err(Error::longbridge)?;
    // Community posts wrap every ticker mention in `[st]…#Name.HK[/st]` markup;
    // rewrite it to the readable ticker.
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    crate::serialize::strip_cashtags_in_field(&mut value, "description");
    tool_json(&value)
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
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    for field in ["description", "body"] {
        crate::serialize::strip_cashtags_in_field(&mut value, field);
    }
    tool_json(&value)
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
    let mut value = serde_json::to_value(&result).map_err(Error::Serialize)?;
    // Reply bodies can embed `[st]…#Name.HK[/st]` ticker markup like topics do;
    // rewrite it to the readable ticker.
    crate::serialize::strip_cashtags_in_field(&mut value, "body");
    // Drop the per-author avatar image URL — pure display, no analytic value.
    crate::serialize::drop_keys(&mut value, &["avatar"]);
    tool_json(&value)
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

#[cfg(test)]
mod tests {
    /// `news` passes the upstream items through, so fields the upstream adds
    /// (the `content_via`/`source*` citation fields) reach callers unchanged;
    /// `published_at` becomes RFC3339 and the list is newest first.
    #[test]
    fn news_items_are_passed_through_and_sorted() {
        let mut items = serde_json::json!([
            {
                "id": "1", "title": "older", "description": "a [st]ST/US/NVDA#NVIDIA.US[/st] b",
                "url": "https://longbridge.com/news/1", "published_at": "1793697522",
                "comments_count": 0, "likes_count": 0, "shares_count": 0,
            },
            {
                "id": "2", "title": "newer", "description": "x",
                "url": "https://longbridge.com/news/2", "published_at": "1793772665",
                "comments_count": 1, "likes_count": 2, "shares_count": 3,
                "content_via": 3, "source": "MSN",
                "source_url": "https://www.msn.com/a", "source_logo": "https://pub.pbkrs.com/msn.png",
            },
        ]);
        super::shape_news(&mut items, Some(10));
        assert_eq!(items[0]["id"], "2");
        assert_eq!(items[0]["published_at"], "2026-11-04T06:11:05Z");
        assert_eq!(items[0]["content_via"], 3);
        assert_eq!(items[0]["source"], "MSN");
        assert_eq!(items[0]["source_url"], "https://www.msn.com/a");
        assert_eq!(items[0]["source_logo"], "https://pub.pbkrs.com/msn.png");
        assert_eq!(items[1]["published_at"], "2026-11-03T09:18:42Z");
        assert!(
            !items[1]["description"].as_str().unwrap().contains("[st]"),
            "ticker markup must be rewritten"
        );

        let mut one = items.clone();
        super::shape_news(&mut one, Some(1));
        assert_eq!(one.as_array().unwrap().len(), 1);
    }

    #[test]
    fn news_is_sorted_newest_first() {
        let mut v = serde_json::json!([
            {"published_at": "2026-10-16T01:41:16Z"},
            {"published_at": "2026-10-16T01:15:11Z"},
            {"published_at": "2026-10-16T03:57:28Z"}
        ]);
        super::sort_newest_first(&mut v);
        let order: Vec<&str> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["published_at"].as_str().unwrap())
            .collect();
        assert_eq!(
            order,
            [
                "2026-10-16T03:57:28Z",
                "2026-10-16T01:41:16Z",
                "2026-10-16T01:15:11Z"
            ]
        );
    }
}
