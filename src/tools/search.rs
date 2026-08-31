//! News and topic search tools.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::support::http_client::http_get_tool;
use crate::tools::support::text::truncate_chars;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NewsSearchParam {
    /// Search keyword
    pub keyword: String,
    /// Max results to return (default: 20)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicSearchParam {
    /// Search keyword
    pub keyword: String,
    /// Max results to return (default: 20)
    pub limit: Option<u32>,
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn fmt_unix_ts(ts: i64) -> String {
    use time::OffsetDateTime;
    match OffsetDateTime::from_unix_timestamp(ts) {
        Ok(dt) => dt
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| ts.to_string()),
        Err(_) => ts.to_string(),
    }
}

fn val_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn transform_news_item(item: &serde_json::Value) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(map) = item.as_object() {
        for (k, v) in map {
            match k.as_str() {
                "id" | "source_name" => {
                    obj.insert(k.clone(), v.clone());
                }
                "title" => {
                    obj.insert(
                        k.clone(),
                        serde_json::Value::String(strip_html(&val_str(v))),
                    );
                }
                "publish_at_timestamp" => {
                    let ts = v.as_i64().unwrap_or(0);
                    obj.insert(
                        "time".to_string(),
                        serde_json::Value::String(fmt_unix_ts(ts)),
                    );
                }
                "description" => {
                    let excerpt = truncate_chars(&strip_html(&val_str(v)), 80);
                    obj.insert("excerpt".to_string(), serde_json::Value::String(excerpt));
                }
                _ => {}
            }
        }
    }
    if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
        let url = format!("https://longbridge.com/news/{id}.md");
        obj.insert("url".to_string(), serde_json::Value::String(url));
    }
    serde_json::Value::Object(obj)
}

fn transform_topic_item(item: &serde_json::Value) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(map) = item.as_object() {
        for (k, v) in map {
            match k.as_str() {
                "id" | "comments_count" | "likes_count" | "creator_name" => {
                    obj.insert(k.clone(), v.clone());
                }
                "title" => {
                    obj.insert(
                        k.clone(),
                        serde_json::Value::String(strip_html(&val_str(v))),
                    );
                }
                "created_at_timestamp" => {
                    let ts = v.as_i64().unwrap_or(0);
                    obj.insert(
                        "time".to_string(),
                        serde_json::Value::String(fmt_unix_ts(ts)),
                    );
                }
                "description" => {
                    let excerpt = truncate_chars(&strip_html(&val_str(v)), 80);
                    obj.insert("excerpt".to_string(), serde_json::Value::String(excerpt));
                }
                _ => {}
            }
        }
    }
    if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
        let url = format!("https://longbridge.com/topics/{id}.md");
        obj.insert("url".to_string(), serde_json::Value::String(url));
    }
    serde_json::Value::Object(obj)
}

fn make_result(value: serde_json::Value) -> CallToolResult {
    let json = serde_json::to_string(&value).unwrap_or_default();
    let structured = serde_json::from_str::<serde_json::Value>(&json).ok();
    let mut result = CallToolResult::success(vec![rmcp::model::Content::text(json)]);
    result.structured_content = structured;
    result
}

/// Search news articles by keyword.
pub async fn news_search(
    mctx: &crate::tools::McpContext,
    p: NewsSearchParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let limit_str = p.limit.unwrap_or(20).to_string();
    let raw = http_get_tool(
        &client,
        "/v1/search/news",
        &[("k", p.keyword.as_str()), ("limit", limit_str.as_str())],
    )
    .await?;
    let json_str = raw
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("null");
    let data: serde_json::Value =
        serde_json::from_str(json_str).map_err(crate::error::Error::Serialize)?;
    let items: Vec<serde_json::Value> = data["news_list"]
        .as_array()
        .map(|arr| arr.iter().map(transform_news_item).collect())
        .unwrap_or_default();
    Ok(make_result(serde_json::Value::Array(items)))
}

/// Search community topics by keyword.
pub async fn topic_search(
    mctx: &crate::tools::McpContext,
    p: TopicSearchParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let limit_str = p.limit.unwrap_or(20).to_string();
    let raw = http_get_tool(
        &client,
        "/v1/search/topics",
        &[("k", p.keyword.as_str()), ("limit", limit_str.as_str())],
    )
    .await?;
    let json_str = raw
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str())
        .unwrap_or("null");
    let data: serde_json::Value =
        serde_json::from_str(json_str).map_err(crate::error::Error::Serialize)?;
    let items: Vec<serde_json::Value> = data["topic_list"]
        .as_array()
        .map(|arr| arr.iter().map(transform_topic_item).collect())
        .unwrap_or_default();
    Ok(make_result(serde_json::Value::Array(items)))
}

#[cfg(test)]
mod tests {
    use super::transform_news_item;

    /// Trimming and the `...` marker are intentional — `truncate_chars`
    /// (shared with `measured_tool_call`'s error logging) applies both, a
    /// deliberate change from the plain `.chars().take(80)` this call site
    /// used before. Pinned here so it reads as a decision, not drift.
    #[test]
    fn excerpt_is_trimmed_and_marked_when_truncated() {
        let item = serde_json::json!({
            "id": "1",
            "description": format!("  <p>{}</p>  ", "a".repeat(100)),
        });
        let excerpt = transform_news_item(&item)["excerpt"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(excerpt.ends_with("..."));
        assert!(!excerpt.starts_with(' ') && !excerpt.trim_end_matches("...").ends_with(' '));
    }

    #[test]
    fn short_excerpt_is_trimmed_but_unmarked() {
        let item = serde_json::json!({
            "id": "1",
            "description": "  <p>short</p>  ",
        });
        let transformed = transform_news_item(&item);
        assert_eq!(transformed["excerpt"].as_str().unwrap(), "short");
    }
}
