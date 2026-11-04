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
    /// Max results to return (default: 20; upstream returns at most 40)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TopicSearchParam {
    /// Search keyword
    pub keyword: String,
    /// Max results to return (default: 20; upstream returns at most 30)
    pub limit: Option<u32>,
}

const DEFAULT_LIMIT: u32 = 20;

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

/// Read a unix-seconds timestamp that the upstream may encode as a JSON number
/// *or* a string (Longbridge commonly serializes `int64` as a string to avoid
/// JS precision loss). Returns `None` for a missing, unparseable, or
/// non-positive value so callers can omit the field rather than emit a
/// misleading `1970-01-01T00:00:00Z`.
fn as_unix_ts(v: &serde_json::Value) -> Option<i64> {
    let ts = match v {
        serde_json::Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64))?,
        serde_json::Value::String(s) => s.trim().parse::<i64>().ok()?,
        _ => return None,
    };
    (ts > 0).then_some(ts)
}

fn val_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Reshape one `/v1/search/news` hit to the field names `news` and
/// `news_detail` use (`description`, `source`, `published_at`), keeping the
/// citation fields (`content_via`, `source_url`, `source_logo`) a client needs
/// to tell a Longbridge article from a third-party repost and link the original.
fn transform_news_item(item: &serde_json::Value) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    if let Some(map) = item.as_object() {
        for (k, v) in map {
            match k.as_str() {
                "id" | "content_via" | "source_url" | "source_logo" => {
                    obj.insert(k.clone(), v.clone());
                }
                "source_name" => {
                    obj.insert("source".to_string(), v.clone());
                }
                "title" => {
                    obj.insert(
                        k.clone(),
                        serde_json::Value::String(strip_html(&val_str(v))),
                    );
                }
                "publish_at_timestamp" => {
                    // Omit `published_at` on a missing/zero/unparseable timestamp
                    // rather than emit a misleading `1970-01-01T00:00:00Z`.
                    if let Some(ts) = as_unix_ts(v) {
                        obj.insert(
                            "published_at".to_string(),
                            serde_json::Value::String(fmt_unix_ts(ts)),
                        );
                    }
                }
                "description" => {
                    // A short excerpt keeps 40 hits small; `news_detail` has the
                    // full text.
                    let excerpt = truncate_chars(&strip_html(&val_str(v)), 80);
                    obj.insert(
                        "description".to_string(),
                        serde_json::Value::String(excerpt),
                    );
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
                    if let Some(ts) = as_unix_ts(v) {
                        obj.insert(
                            "time".to_string(),
                            serde_json::Value::String(fmt_unix_ts(ts)),
                        );
                    }
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

/// Upstream search RPCs take no page-size field (only the keyword and
/// pagination cursors) and always return a fixed-size page, so `limit` is
/// applied here after the fetch.
fn effective_limit(limit: Option<u32>) -> usize {
    limit.unwrap_or(DEFAULT_LIMIT) as usize
}

fn make_result(value: serde_json::Value) -> CallToolResult {
    let json = serde_json::to_string(&value).unwrap_or_default();
    // MCP requires `structuredContent` to be an object; array- or scalar-rooted
    // responses (news_search / topic_search return arrays) must leave it unset,
    // or clients reject the result with a schema-validation error. Mirrors
    // `support::http_client::success_with_structured`.
    let structured = serde_json::from_str::<serde_json::Value>(&json)
        .ok()
        .filter(serde_json::Value::is_object);
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
    let limit = effective_limit(p.limit);
    let raw = http_get_tool(&client, "/v1/search/news", &[("k", p.keyword.as_str())]).await?;
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
        .map(|arr| arr.iter().take(limit).map(transform_news_item).collect())
        .unwrap_or_default();
    Ok(make_result(serde_json::Value::Array(items)))
}

/// Search community topics by keyword.
pub async fn topic_search(
    mctx: &crate::tools::McpContext,
    p: TopicSearchParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let limit = effective_limit(p.limit);
    let raw = http_get_tool(&client, "/v1/search/topics", &[("k", p.keyword.as_str())]).await?;
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
        .map(|arr| arr.iter().take(limit).map(transform_topic_item).collect())
        .unwrap_or_default();
    Ok(make_result(serde_json::Value::Array(items)))
}

#[cfg(test)]
mod tests {
    use super::{effective_limit, transform_news_item, transform_topic_item};

    /// The upstream may send `publish_at_timestamp` as a JSON string; it must
    /// still produce a real `time`, not `1970-01-01T00:00:00Z` (the epoch-0
    /// value a failed `as_i64()` used to fall back to).
    #[test]
    fn string_timestamp_parses_to_real_time() {
        // 2025-09-16T00:00:00Z
        let item = serde_json::json!({"id": "1", "publish_at_timestamp": "1757980800"});
        let time = transform_news_item(&item)["published_at"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(time.starts_with("2025-09-16"), "unexpected time: {time}");

        // topic path uses `created_at_timestamp`, same rule.
        let topic = serde_json::json!({"id": "1", "created_at_timestamp": "1757980800"});
        assert!(
            transform_topic_item(&topic)["time"]
                .as_str()
                .unwrap()
                .starts_with("2025-09-16")
        );
    }

    /// A numeric timestamp keeps working.
    #[test]
    fn numeric_timestamp_parses_to_real_time() {
        let item = serde_json::json!({"id": "1", "publish_at_timestamp": 1757980800_i64});
        assert!(
            transform_news_item(&item)["published_at"]
                .as_str()
                .unwrap()
                .starts_with("2025-09-16")
        );
    }

    /// A missing, zero, or unparseable timestamp omits `time` entirely rather
    /// than emitting a misleading `1970-01-01T00:00:00Z`.
    #[test]
    fn missing_or_zero_timestamp_omits_time() {
        for bad in [
            serde_json::json!({"id": "1"}),
            serde_json::json!({"id": "1", "publish_at_timestamp": 0}),
            serde_json::json!({"id": "1", "publish_at_timestamp": "0"}),
            serde_json::json!({"id": "1", "publish_at_timestamp": ""}),
            serde_json::json!({"id": "1", "publish_at_timestamp": "not-a-number"}),
            serde_json::json!({"id": "1", "publish_at_timestamp": serde_json::Value::Null}),
        ] {
            let out = transform_news_item(&bad);
            assert!(
                out.get("published_at").is_none(),
                "expected no `published_at` for {bad}, got {:?}",
                out.get("published_at")
            );
        }
    }

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
        let excerpt = transform_news_item(&item)["description"]
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
        assert_eq!(transformed["description"].as_str().unwrap(), "short");
    }

    /// A third-party article keeps the citation fields the upstream sends, under
    /// the same names `news` and `news_detail` use (live sample, 2026-11-04).
    #[test]
    fn news_item_carries_source_fields_with_aligned_names() {
        let raw = serde_json::json!({
            "id": "304086865",
            "title": "NVDA shares inch higher overnight",
            "description": "Nvidia shares rose 0.8% overnight.",
            "source_name": "MSN",
            "source_name_zh": "MSN",
            "content_via": 3,
            "source_url": "https://www.msn.com/en-us/money/top-stocks/ar-AA2gDMOO",
            "publish_at_timestamp": "1793697522",
            "publish_at": "2026-11-03 09:18:42",
            "score": "259.08334",
            "index": "news_v2_20261101",
            "kind": 1,
        });
        let item = transform_news_item(&raw);
        assert_eq!(item["id"], "304086865");
        assert_eq!(item["source"], "MSN");
        assert_eq!(item["content_via"], 3);
        assert_eq!(
            item["source_url"],
            "https://www.msn.com/en-us/money/top-stocks/ar-AA2gDMOO"
        );
        assert_eq!(item["published_at"], "2026-11-03T09:18:42Z");
        assert_eq!(item["description"], "Nvidia shares rose 0.8% overnight.");
        // Old names and search-ranking internals are gone.
        for gone in [
            "source_name",
            "excerpt",
            "time",
            "score",
            "index",
            "publish_at",
            "publish_at_timestamp",
        ] {
            assert!(item.get(gone).is_none(), "unexpected `{gone}` in {item}");
        }
    }

    /// Longbridge-original or licensed content says so with `content_via: 0`,
    /// and `source_logo` passes through whenever the upstream starts sending it.
    #[test]
    fn news_item_passes_content_via_zero_and_source_logo() {
        let raw = serde_json::json!({
            "id": "1",
            "content_via": 0,
            "source_name": "Zhitong",
            "source_url": "https://www.zhitongcaijing.com/content/detail/1518081.html",
            "source_logo": "https://pub.pbkrs.com/logo.png",
        });
        let item = transform_news_item(&raw);
        assert_eq!(item["content_via"], 0);
        assert_eq!(item["source_logo"], "https://pub.pbkrs.com/logo.png");
    }

    /// MCP requires `structuredContent` to be an object. `news_search` /
    /// `topic_search` return arrays, so the result must leave
    /// `structured_content` unset — otherwise clients reject the response with a
    /// schema-validation error ("expected record, received array").
    #[test]
    fn array_result_leaves_structured_content_unset() {
        let arr = super::make_result(serde_json::Value::Array(vec![
            serde_json::json!({"id": "1"}),
        ]));
        assert!(
            arr.structured_content.is_none(),
            "array-rooted result must not populate structuredContent"
        );
    }

    #[test]
    fn object_result_sets_structured_content() {
        let obj = super::make_result(serde_json::json!({"news_list": []}));
        assert!(
            obj.structured_content.is_some(),
            "object-rooted result should populate structuredContent"
        );
    }

    #[test]
    fn limit_defaults_to_20_and_is_honored() {
        assert_eq!(effective_limit(None), 20);
        assert_eq!(effective_limit(Some(3)), 3);
        let upstream: Vec<serde_json::Value> = (0..40)
            .map(|i| serde_json::json!({"id": i.to_string()}))
            .collect();
        let taken: Vec<_> = upstream
            .iter()
            .take(effective_limit(Some(3)))
            .map(transform_news_item)
            .collect();
        assert_eq!(taken.len(), 3);
    }
}
