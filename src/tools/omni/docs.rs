//! `docs`: full tool documentation, topic guides and (Task 9) the Longbridge
//! OpenAPI documentation snapshot.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;
use serde_json::Value;

use crate::tools::McpContext;
use crate::tools::omni::dispatch::{envelope, unknown_tool};
use crate::tools::omni::search::{category_of, localized};
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
    // Not read until Task 10 wires `page`/`fresh` to a live documentation
    // fetch; `query`/`page` already return `docs_unavailable` in this task.
    #[allow(dead_code)]
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
        "annotations": tool.annotations,
        "notes": notes_for(tool),
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
// Not yet called outside tests: the `/omni` `docs` tool registration (task 9)
// is its first production caller.
#[allow(dead_code)]
pub(crate) async fn docs(mctx: &McpContext, p: DocsParam) -> Result<CallToolResult, McpError> {
    validate_param(&p)?;
    let lang = Lang::parse(p.lang.as_deref(), mctx.language.as_deref());
    if let Some(topic) = &p.topic {
        return match TOPICS.iter().find(|(id, _)| id == topic) {
            Some((id, md)) => tool_json(&serde_json::json!({"topic": id, "markdown": md})),
            None => Err(McpError::invalid_params(
                format!(
                    "unknown topic `{topic}`; known: {}",
                    TOPICS
                        .iter()
                        .map(|(id, _)| *id)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None,
            )),
        };
    }
    if let Some(name) = &p.tool {
        return match tool_doc(name, lang) {
            Some(doc) => tool_json(&doc),
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
        return tool_json(&docs);
    }
    if p.query.is_some() || p.page.is_some() {
        return Ok(envelope(
            "docs_unavailable",
            "Documentation site lookup is not wired yet.".into(),
            "none",
            "Use `tool` or `topic` for now.",
            Value::Null,
        ));
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
            doc["annotations"]["destructiveHint"], true,
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
}
