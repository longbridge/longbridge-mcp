//! `docs`: full tool documentation, topic guides and the Longbridge OpenAPI
//! documentation snapshot.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use serde_json::Value;

use crate::tools::McpContext;
use crate::tools::omni::dispatch::{envelope, unknown_tool};
use crate::tools::omni::docs_index;
use crate::tools::omni::search::{category_of, localized};
use crate::tools::omni::truncate::{MAX_OUTPUT_TOKENS, truncate_result};
use crate::tools::{
    all_tools_full_cached, is_region_scoped, output_schema_map, tool_json, v2_tool_names,
};

/// Max tool names accepted at once by `tools`.
const MAX_TOOLS: usize = 5;

/// Parameters for `docs`. With no fields set, returns the catalogue.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DocsParam {
    /// Tool name: returns its full description, input schema, output schema and notes.
    pub tool: Option<String>,
    /// Up to 5 tool names at once.
    pub tools: Option<Vec<String>>,
    /// Topic guide id: getting-started, pipelines, symbols, orders, errors, jq.
    pub topic: Option<String>,
    /// Search the Longbridge OpenAPI documentation (open.longbridge.com), English or Chinese.
    pub query: Option<String>,
    /// Documentation page path to read in full, e.g. "trade/order/submit".
    pub page: Option<String>,
    /// en, zh-CN or zh-HK; default follows Accept-Language.
    pub lang: Option<String>,
    /// Max `query` hits, default 5, max 20.
    pub limit: Option<usize>,
    /// With `page`: fetch the live page instead of the bundled snapshot.
    pub fresh: Option<bool>,
}

/// Embedded topic guides, `(id, markdown)`.
pub(crate) const TOPICS: &[(&str, &str)] = &[
    (
        "getting-started",
        include_str!("../../../docs/omni/getting-started.md"),
    ),
    ("pipelines", include_str!("../../../docs/omni/pipelines.md")),
    ("symbols", include_str!("../../../docs/omni/symbols.md")),
    ("orders", include_str!("../../../docs/omni/orders.md")),
    ("errors", include_str!("../../../docs/omni/errors.md")),
    ("jq", include_str!("../../../docs/omni/jq.md")),
];

/// Documentation language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    /// English (default).
    En,
    /// Simplified Chinese (`zh-CN`).
    ZhCn,
    /// Traditional Chinese / Hong Kong (`zh-HK`), also used for other Chinese tags.
    ZhHk,
}

impl Lang {
    /// Explicit `lang` wins; otherwise the first Chinese tag in Accept-Language; else English.
    pub(crate) fn parse(explicit: Option<&str>, accept_language: Option<&str>) -> Lang {
        let pick = |s: &str| -> Option<Lang> {
            if s.contains("zh-CN") || s.contains("zh-Hans") {
                Some(Lang::ZhCn)
            } else if s.starts_with("zh") || s.contains("zh-") {
                Some(Lang::ZhHk)
            } else if s.starts_with("en") {
                Some(Lang::En)
            } else {
                None
            }
        };
        explicit
            .and_then(pick)
            .or_else(|| accept_language.and_then(pick))
            .unwrap_or(Lang::En)
    }

    /// The locale code, matching `crate::auth::TOOL_LOCALES`.
    pub(crate) fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::ZhCn => "zh-CN",
            Lang::ZhHk => "zh-HK",
        }
    }
}

/// Usage notes for one tool: write-tool confirmation flow, region scoping,
/// `/v2` availability, and the universal `_jq` filter.
fn notes_for(tool: &rmcp::model::Tool) -> Vec<String> {
    let mut notes = Vec::new();
    if tool.annotations.as_ref().and_then(|a| a.destructive_hint) == Some(true) {
        notes.push(
            "Write tool: call once without `execute` to get a confirmation_code and show the \
             preview to the user; only after explicit confirmation call again with `execute` \
             set to that code."
                .to_string(),
        );
    }
    if is_region_scoped(&tool.name) {
        notes.push(
            "Region-scoped: only available for accounts in one data-center region; unavailable \
             accounts get `tool_unavailable_in_region`."
                .to_string(),
        );
    }
    if !v2_tool_names().contains(&tool.name.as_ref()) {
        notes.push("Not available on the restricted /v2 endpoint.".to_string());
    }
    notes.push("Accepts a top-level `_jq` filter on the response.".to_string());
    notes
}

/// Build the `annotations` sub-object in this crate's snake_case convention
/// (see `search`'s `read_only` field), rather than `rmcp::model::ToolAnnotations`'s
/// own `#[serde(rename_all = "camelCase")]` wire format. `title` is left out;
/// `tool_doc`'s own `title` field already covers it.
fn annotations_doc(annotations: Option<&rmcp::model::ToolAnnotations>) -> Value {
    serde_json::json!({
        "read_only_hint": annotations.and_then(|a| a.read_only_hint),
        "destructive_hint": annotations.and_then(|a| a.destructive_hint),
        "idempotent_hint": annotations.and_then(|a| a.idempotent_hint),
        "open_world_hint": annotations.and_then(|a| a.open_world_hint),
    })
}

/// Full documentation of one tool, localized when a translation exists.
pub(crate) fn tool_doc(name: &str, lang: Lang) -> Option<Value> {
    let tool = all_tools_full_cached().iter().find(|t| t.name == name)?;
    let localized = localized()
        .get(name)
        .and_then(|entries| entries.iter().find(|e| e.lang == lang.code()));
    let description = localized
        .map(|l| l.description.as_str())
        .filter(|d| !d.is_empty())
        .unwrap_or(tool.description.as_deref().unwrap_or_default());
    let title = localized
        .map(|l| l.title.clone())
        .filter(|t| !t.is_empty())
        .or_else(|| tool.title.clone());
    let mut doc = serde_json::json!({
        "name": tool.name,
        "title": title,
        "lang": lang.code(),
        "category": category_of(name),
        "description": description,
        "input_schema": Value::Object((*tool.input_schema).clone()),
        "annotations": annotations_doc(tool.annotations.as_ref()),
        "notes": notes_for(tool),
        "related_pages": docs_index::related_pages(name),
    });
    if let Some(schema) = output_schema_map().get(name) {
        doc["output_schema"] = Value::Object((**schema).clone());
    }
    Some(doc)
}

/// The catalogue: topics, categories with tool names, and doc-site sections.
pub(crate) fn catalog() -> Value {
    let mut categories: serde_json::Map<String, Value> = serde_json::Map::new();
    for tool in all_tools_full_cached() {
        let cat = category_of(&tool.name).unwrap_or("Other").to_string();
        categories
            .entry(cat)
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .expect("category entries are always inserted as arrays")
            .push(Value::String(tool.name.to_string()));
    }
    serde_json::json!({
        "topics": TOPICS
            .iter()
            .map(|(id, md)| serde_json::json!({
                "id": id,
                "title": md.lines().next().unwrap_or_default().trim_start_matches("# "),
            }))
            .collect::<Vec<_>>(),
        "categories": categories,
        "docs_site": "Use `query` to search open.longbridge.com documentation, or `page` (e.g. trade/order/submit) to read one page.",
        "docs_snapshot_generated_at": docs_index::snapshot().generated_at,
    })
}

/// Validate `docs` parameters that do not depend on which mode is requested.
pub(crate) fn validate_param(p: &DocsParam) -> Result<(), McpError> {
    if p.tools.as_ref().is_some_and(|t| t.len() > MAX_TOOLS) {
        return Err(McpError::invalid_params(
            format!("tools accepts at most {MAX_TOOLS} names"),
            None,
        ));
    }
    if p.limit.is_some_and(|l| l == 0 || l > 20) {
        return Err(McpError::invalid_params(
            "limit must be between 1 and 20",
            None,
        ));
    }
    Ok(())
}

/// Run `docs`.
pub(crate) async fn docs(mctx: &McpContext, p: DocsParam) -> Result<CallToolResult, McpError> {
    validate_param(&p)?;
    let lang = Lang::parse(p.lang.as_deref(), mctx.language.as_deref());
    if let Some(topic) = &p.topic {
        let known = TOPICS
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
            .join(", ");
        return match TOPICS.iter().find(|(id, _)| id == topic) {
            Some((id, md)) => tool_json(&serde_json::json!({"topic": id, "markdown": md})),
            None => Ok(envelope(
                "unknown_topic",
                format!("unknown topic `{topic}`; known: {known}"),
                "fix_params",
                "Retry with one of the listed topic ids, or call `docs` with no arguments for \
                 the catalogue.",
                Value::Null,
            )),
        };
    }
    if let Some(name) = &p.tool {
        return match tool_doc(name, lang) {
            Some(doc) => Ok(truncate_result(tool_json(&doc)?, MAX_OUTPUT_TOKENS)),
            None => Ok(unknown_tool(name)),
        };
    }
    if let Some(names) = &p.tools {
        let docs: Vec<Value> = names
            .iter()
            .map(|n| {
                tool_doc(n, lang)
                    .unwrap_or_else(|| serde_json::json!({"name": n, "error": "unknown tool"}))
            })
            .collect();
        return Ok(truncate_result(tool_json(&docs)?, MAX_OUTPUT_TOKENS));
    }
    if let Some(page) = &p.page {
        if !docs_index::valid_page_path(page) {
            return Ok(envelope(
                "invalid_page",
                format!("page `{page}` must look like trade/order/submit"),
                "fix_params",
                "Use a slash-separated docs path without `.`/`..` segments, or call `docs` with \
                 `query` to find one.",
                Value::Null,
            ));
        }
        let markdown = if p.fresh == Some(true) {
            docs_index::fetch_page(page, lang)
                .await
                .map_err(|e| McpError::internal_error(format!("failed to fetch page: {e}"), None))?
        } else if let Some(md) = docs_index::page_from_snapshot(page, lang) {
            md
        } else {
            match docs_index::fetch_page(page, lang).await {
                Ok(md) => md,
                Err(_) => {
                    return Ok(envelope(
                        "unknown_page",
                        format!("unknown page `{page}`"),
                        "fix_params",
                        "The page is neither in the bundled snapshot nor live; call `docs` with \
                         `query` to find the right page path.",
                        Value::Null,
                    ));
                }
            }
        };
        let result = tool_json(&serde_json::json!({
            "page": page,
            "lang": lang.code(),
            "url": docs_index::page_url(page, lang),
            "markdown": markdown,
        }))?;
        return Ok(truncate_result(result, MAX_OUTPUT_TOKENS));
    }
    if let Some(query) = &p.query {
        if query.trim().is_empty() {
            return Ok(envelope(
                "invalid_query",
                "`query` must be non-empty".into(),
                "fix_params",
                "Pass keywords to search the Longbridge OpenAPI docs, e.g. \"submit order\", or \
                 use `topic` for a guide.",
                Value::Null,
            ));
        }
        return tool_json(&docs_index::search_docs(query, lang, p.limit.unwrap_or(5)));
    }
    tool_json(&catalog())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_doc_schema_matches_full_tool_list() {
        let doc = tool_doc("quote", Lang::En).expect("quote exists");
        let full = crate::tools::all_tools_full_cached()
            .iter()
            .find(|t| t.name == "quote")
            .expect("quote");
        assert_eq!(
            doc["input_schema"],
            serde_json::Value::Object((*full.input_schema).clone()),
            "input_schema must match the full tool list's schema"
        );
        assert!(
            doc["description"]
                .as_str()
                .expect("description must be a string")
                .contains("last_done"),
            "quote description must mention last_done"
        );
        assert!(
            tool_doc("nope", Lang::En).is_none(),
            "unknown tool must return None"
        );
    }

    #[test]
    fn write_tool_doc_has_two_step_note_and_chinese_description() {
        let doc = tool_doc("submit_order", Lang::ZhCn).expect("submit_order exists");
        assert!(
            doc["notes"]
                .as_array()
                .expect("notes must be an array")
                .iter()
                .any(|n| n
                    .as_str()
                    .expect("note must be a string")
                    .contains("confirmation_code")),
            "write tool notes must mention confirmation_code"
        );
        assert!(
            doc["description"]
                .as_str()
                .expect("description must be a string")
                .contains("委托"),
            "zh-CN description must contain 委托"
        );
        assert_eq!(
            doc["annotations"]["destructive_hint"], true,
            "submit_order must be flagged destructive"
        );
    }

    #[test]
    fn output_schema_included_when_tool_declares_one() {
        let doc = tool_doc("depth", Lang::En).expect("depth exists");
        assert!(
            doc.get("output_schema").is_some_and(|s| s.is_object()),
            "depth declares an output_schema"
        );
    }

    #[test]
    fn topics_are_all_present_and_catalog_lists_them() {
        for id in [
            "getting-started",
            "pipelines",
            "symbols",
            "orders",
            "errors",
            "jq",
        ] {
            assert!(TOPICS.iter().any(|(t, _)| *t == id), "missing topic {id}");
        }
        let cat = catalog();
        assert_eq!(
            cat["topics"]
                .as_array()
                .expect("topics must be an array")
                .len(),
            TOPICS.len(),
            "catalog must list every topic"
        );
        assert!(
            cat["categories"]
                .as_object()
                .expect("categories must be an object")
                .contains_key("General"),
            "catalog categories must include General"
        );
    }

    #[test]
    fn lang_resolution_prefers_explicit_then_accept_language() {
        assert!(
            matches!(Lang::parse(Some("zh-HK"), Some("en")), Lang::ZhHk),
            "explicit lang must win over Accept-Language"
        );
        assert!(
            matches!(Lang::parse(None, Some("zh-CN,zh;q=0.9")), Lang::ZhCn),
            "Accept-Language zh-CN must resolve to ZhCn"
        );
        assert!(
            matches!(Lang::parse(None, Some("zh-TW")), Lang::ZhHk),
            "non zh-CN Chinese tags must fall back to ZhHk"
        );
        assert!(
            matches!(Lang::parse(None, None), Lang::En),
            "no lang info must default to English"
        );
    }

    #[test]
    fn too_many_tools_is_invalid_params() {
        let p = DocsParam {
            tool: None,
            tools: Some((0..6).map(|i| format!("t{i}")).collect()),
            topic: None,
            query: None,
            page: None,
            lang: None,
            limit: None,
            fresh: None,
        };
        assert!(
            validate_param(&p).is_err(),
            "more than MAX_TOOLS names must be rejected"
        );
    }

    #[test]
    fn tool_doc_includes_related_pages() {
        let doc = tool_doc("submit_order", Lang::En).expect("submit_order exists");
        let related = doc["related_pages"]
            .as_array()
            .expect("related_pages must be an array");
        assert!(
            related.iter().any(|p| p == "trade/order/submit"),
            "submit_order must be related to trade/order/submit, got {related:?}"
        );
    }

    #[test]
    fn catalog_includes_docs_snapshot_generated_at() {
        let cat = catalog();
        assert!(
            cat["docs_snapshot_generated_at"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
            "catalog must report a non-empty docs snapshot timestamp"
        );
    }

    fn no_lang_param() -> DocsParam {
        DocsParam {
            tool: None,
            tools: None,
            topic: None,
            query: None,
            page: None,
            lang: None,
            limit: None,
            fresh: None,
        }
    }

    fn ctx() -> McpContext {
        McpContext {
            token: "test-token".into(),
            language: None,
            client_user_agent: None,
            extra_headers: Vec::new(),
        }
    }

    /// A known snapshot page, so `docs` never falls through to `fetch_page`
    /// (no network access is allowed in tests).
    #[tokio::test]
    async fn docs_page_reads_from_bundled_snapshot() {
        let p = DocsParam {
            page: Some("trade/order/submit".into()),
            ..no_lang_param()
        };
        let result = docs(&ctx(), p).await.expect("known page must succeed");
        let value = crate::tools::jq::result_value(&result);
        assert_eq!(
            value["page"], "trade/order/submit",
            "the response must echo back the requested page path"
        );
        assert!(
            value["markdown"]
                .as_str()
                .is_some_and(|md| md.contains("## Request")),
            "got {value:?}"
        );
    }

    /// `docs` never returns a protocol error for a fixable argument: the model
    /// gets a `fix_params` envelope it can act on.
    fn assert_fix_params_envelope(result: &CallToolResult, code: &str) {
        assert_eq!(
            result.is_error,
            Some(true),
            "a rejected `docs` call must be an error result"
        );
        let v = crate::tools::jq::result_value(result);
        assert_eq!(
            v["error_code"], code,
            "the envelope must carry the `{code}` error code, got {v}"
        );
        assert_eq!(
            v["recoverable"], "fix_params",
            "the caller can fix the argument and retry"
        );
    }

    #[tokio::test]
    async fn docs_unknown_topic_returns_a_fix_params_envelope() {
        let p = DocsParam {
            topic: Some("nope".into()),
            ..no_lang_param()
        };
        let result = docs(&ctx(), p)
            .await
            .expect("an unknown topic must not be a protocol error");
        assert_fix_params_envelope(&result, "unknown_topic");
        assert!(
            crate::tools::jq::result_value(&result)["message"]
                .as_str()
                .is_some_and(|m| m.contains("pipelines")),
            "the message must list the known topic ids"
        );
    }

    #[tokio::test]
    async fn docs_invalid_page_path_is_rejected_before_any_fetch() {
        let p = DocsParam {
            page: Some("../etc/passwd".into()),
            ..no_lang_param()
        };
        let result = docs(&ctx(), p)
            .await
            .expect("a path-traversal page must not be a protocol error");
        assert_fix_params_envelope(&result, "invalid_page");
    }

    #[tokio::test]
    async fn docs_query_returns_search_hits() {
        let p = DocsParam {
            query: Some("submit order".into()),
            ..no_lang_param()
        };
        let result = docs(&ctx(), p).await.expect("query must succeed");
        let value = crate::tools::jq::result_value(&result);
        let hits = value.as_array().expect("query result must be an array");
        assert!(
            hits.iter().any(|h| h["page"] == "trade/order/submit"),
            "got {hits:?}"
        );
    }

    #[tokio::test]
    async fn docs_blank_query_returns_a_fix_params_envelope() {
        let p = DocsParam {
            query: Some("   ".into()),
            ..no_lang_param()
        };
        let result = docs(&ctx(), p)
            .await
            .expect("a blank query must not be a protocol error");
        assert_fix_params_envelope(&result, "invalid_query");
    }
}
