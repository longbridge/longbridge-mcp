//! `search`: rank the tool catalogue by natural-language or Chinese keywords.

use std::collections::HashMap;
use std::sync::OnceLock;

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::tools::omni::index::{Doc, Field, Index};
use crate::tools::support::text::clip_chars;
use crate::tools::tool_json;

// Nothing in this module is called outside tests yet; wiring the `/omni`
// dispatcher to `search` (a later task) is its first production caller.
#[allow(dead_code)]
const DEFAULT_LIMIT: usize = 10;
#[allow(dead_code)]
const MAX_LIMIT: usize = 50;
#[allow(dead_code)]
const SUMMARY_CHARS: usize = 160;

/// Parameters for `search`.
#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
pub struct SearchParam {
    /// Natural-language or keyword query; English and Chinese both work, e.g. "latest quote", "市场温度".
    pub query: String,
    /// Optional regex applied to tool names; intersected with `query` results, e.g. "^grid_".
    pub pattern: Option<String>,
    /// Optional category filter: a scope id or name from the tool manifest, e.g. "Trade Execution".
    pub category: Option<String>,
    /// Max hits, default 10, max 50.
    pub limit: Option<usize>,
}

/// One tool's translated title, description, and parameter descriptions for
/// a single locale, used to broaden the search index beyond English text.
#[allow(dead_code)]
struct Localized {
    title: String,
    description: String,
    params: Vec<String>,
}

#[allow(dead_code)]
fn localized() -> &'static HashMap<String, Vec<Localized>> {
    static MAP: OnceLock<HashMap<String, Vec<Localized>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map: HashMap<String, Vec<Localized>> = HashMap::new();
        for (_, raw) in crate::auth::TOOL_LOCALES {
            let parsed: serde_json::Value =
                serde_json::from_str(raw).expect("locale file must be valid JSON");
            let Some(tools) = parsed.get("tools").and_then(|v| v.as_object()) else {
                continue;
            };
            for (name, entry) in tools {
                let params = entry
                    .get("properties")
                    .and_then(|v| v.as_object())
                    .map(|m| {
                        m.values()
                            .filter_map(|v| v.as_str())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();
                map.entry(name.clone()).or_default().push(Localized {
                    title: entry
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    description: entry
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    params,
                });
            }
        }
        map
    })
}

#[allow(dead_code)]
fn categories() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    // tool name -> (scope id, scope name)
    static SCOPES: OnceLock<serde_json::Value> = OnceLock::new();
    static MAP: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();
    let scopes = SCOPES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../data/scopes.json"))
            .expect("scopes.json must be valid JSON")
    });
    MAP.get_or_init(|| {
        let mut map = HashMap::new();
        for scope in scopes["scopes"].as_array().expect("scopes array") {
            let id = scope["id"].as_str().expect("scope id");
            let name = scope["name"].as_str().expect("scope name");
            for tool in scope["tools"].as_array().expect("scope tools") {
                map.insert(tool.as_str().expect("tool name"), (id, name));
            }
        }
        map
    })
}

/// Scope name of a tool, from `data/scopes.json`.
#[allow(dead_code)]
pub(crate) fn category_of(tool: &str) -> Option<&'static str> {
    categories().get(tool).map(|(_, name)| *name)
}

#[allow(dead_code)]
fn schema_param_text(tool: &rmcp::model::Tool) -> String {
    let mut out = String::new();
    if let Some(props) = tool
        .input_schema
        .get("properties")
        .and_then(|v| v.as_object())
    {
        for (name, schema) in props {
            if name == "_jq" {
                continue;
            }
            out.push_str(name);
            out.push(' ');
            if let Some(desc) = schema.get("description").and_then(|v| v.as_str()) {
                out.push_str(desc);
                out.push(' ');
            }
        }
    }
    out
}

/// The tool-catalogue index, built once per process.
#[allow(dead_code)]
pub(crate) fn tool_index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| {
        let docs = crate::tools::all_tools_full_cached()
            .iter()
            .map(|tool| {
                let name = tool.name.to_string();
                let mut fields = vec![
                    Field {
                        weight: 10.0,
                        text: name.replace('_', " "),
                    },
                    Field {
                        weight: 6.0,
                        text: tool.title.clone().unwrap_or_default(),
                    },
                    Field {
                        weight: 3.0,
                        text: tool.description.as_deref().unwrap_or_default().to_string(),
                    },
                    Field {
                        weight: 2.0,
                        text: schema_param_text(tool),
                    },
                    Field {
                        weight: 1.0,
                        text: category_of(&name).unwrap_or_default().to_string(),
                    },
                ];
                for loc in localized()
                    .get(&name)
                    .map(|v| v.as_slice())
                    .unwrap_or_default()
                {
                    fields.push(Field {
                        weight: 6.0,
                        text: loc.title.clone(),
                    });
                    fields.push(Field {
                        weight: 3.0,
                        text: loc.description.clone(),
                    });
                    fields.push(Field {
                        weight: 2.0,
                        text: loc.params.join(" "),
                    });
                }
                Doc { key: name, fields }
            })
            .collect();
        Index::build(docs)
    })
}

#[allow(dead_code)]
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Tool names closest to `name` by edit distance, then by index score.
#[allow(dead_code)]
pub(crate) fn suggest(name: &str, limit: usize) -> Vec<String> {
    let mut candidates: Vec<(usize, String)> = crate::tools::all_tools_full_cached()
        .iter()
        .map(|t| (edit_distance(name, &t.name), t.name.to_string()))
        .collect();
    candidates.sort();
    let mut out: Vec<String> = candidates.into_iter().take(limit).map(|(_, n)| n).collect();
    for hit in tool_index().search(&name.replace('_', " "), limit) {
        if !out.contains(&hit.key) && out.len() < limit {
            out.push(hit.key);
        }
    }
    out.truncate(limit);
    out
}

/// Run `search`.
#[allow(dead_code)]
pub(crate) fn search(p: SearchParam) -> Result<CallToolResult, McpError> {
    if p.query.trim().is_empty() {
        return Err(McpError::invalid_params("query must be non-empty", None));
    }
    let limit = p.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(McpError::invalid_params(
            format!("limit must be between 1 and {MAX_LIMIT}"),
            None,
        ));
    }
    let pattern = p
        .pattern
        .as_deref()
        .map(regex::Regex::new)
        .transpose()
        .map_err(|e| McpError::invalid_params(format!("invalid pattern: {e}"), None))?;
    let category = p
        .category
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    let tools: HashMap<&str, &rmcp::model::Tool> = crate::tools::all_tools_full_cached()
        .iter()
        .map(|t| (t.name.as_ref(), t))
        .collect();
    let index = tool_index();
    let pool = if pattern.is_some() || category.is_some() {
        index.len()
    } else {
        limit
    };
    let hits: Vec<serde_json::Value> = index
        .search(&p.query, pool)
        .into_iter()
        .filter(|h| pattern.as_ref().is_none_or(|re| re.is_match(&h.key)))
        .filter(|h| {
            category.is_none_or(|c| {
                categories()
                    .get(h.key.as_str())
                    .is_some_and(|(id, name)| *id == c || name.eq_ignore_ascii_case(c))
            })
        })
        .take(limit)
        .filter_map(|h| {
            let tool = tools.get(h.key.as_str())?;
            let required: Vec<&str> = tool
                .input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            Some(serde_json::json!({
                "name": h.key,
                "title": tool.title,
                "summary": clip_chars(tool.description.as_deref().unwrap_or_default(), SUMMARY_CHARS),
                "required": required,
                "read_only": tool.annotations.as_ref().and_then(|a| a.read_only_hint).unwrap_or(false),
                "category": category_of(&h.key),
                "score": (h.score * 10.0).round() / 10.0,
            }))
        })
        .collect();
    tool_json(&hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(result: &CallToolResult) -> Vec<String> {
        let value = crate::tools::jq::result_value(result);
        value
            .as_array()
            .expect("search result must be a JSON array")
            .iter()
            .map(|h| {
                h["name"]
                    .as_str()
                    .expect("hit must have a string name")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn english_query_finds_quote_in_top_three() {
        let r = search(SearchParam {
            query: "latest price quote".into(),
            pattern: None,
            category: None,
            limit: None,
        })
        .expect("search should succeed");
        assert!(
            names(&r)[..3].contains(&"quote".to_string()),
            "expected quote in top 3, got {:?}",
            names(&r)
        );
    }

    #[test]
    fn chinese_query_finds_quote_and_market_temperature() {
        let r = search(SearchParam {
            query: "行情快照".into(),
            pattern: None,
            category: None,
            limit: None,
        })
        .expect("search should succeed");
        assert!(
            names(&r)[..3].contains(&"quote".to_string()),
            "expected quote in top 3, got {:?}",
            names(&r)
        );
        let r = search(SearchParam {
            query: "市场温度".into(),
            pattern: None,
            category: None,
            limit: None,
        })
        .expect("search should succeed");
        assert!(
            names(&r)[..3].contains(&"market_temperature".to_string()),
            "expected market_temperature in top 3, got {:?}",
            names(&r)
        );
    }

    #[test]
    fn chinese_order_query_finds_submit_order() {
        let r = search(SearchParam {
            query: "下单 委托".into(),
            pattern: None,
            category: None,
            limit: None,
        })
        .expect("search should succeed");
        assert!(
            names(&r)[..5].contains(&"submit_order".to_string()),
            "expected submit_order in top 5, got {:?}",
            names(&r)
        );
    }

    #[test]
    fn pattern_and_category_filter_results() {
        let r = search(SearchParam {
            query: "order".into(),
            pattern: Some("^grid_".into()),
            category: None,
            limit: Some(50),
        })
        .expect("search should succeed");
        assert!(!names(&r).is_empty(), "expected at least one grid_ hit");
        assert!(
            names(&r).iter().all(|n| n.starts_with("grid_")),
            "expected all hits to start with grid_, got {:?}",
            names(&r)
        );
        let r = search(SearchParam {
            query: "order".into(),
            pattern: None,
            category: Some("Trade Execution".into()),
            limit: Some(50),
        })
        .expect("search should succeed");
        assert!(
            !names(&r).is_empty(),
            "category filter must keep matching tools"
        );
        assert!(
            names(&r)
                .iter()
                .all(|n| category_of(n) == Some("Trade Execution")),
            "expected all hits to be in Trade Execution, got {:?}",
            names(&r)
        );
    }

    #[test]
    fn hit_shape_is_compact() {
        let r = search(SearchParam {
            query: "quote".into(),
            pattern: None,
            category: None,
            limit: Some(1),
        })
        .expect("search should succeed");
        let hit = &crate::tools::jq::result_value(&r)[0];
        for key in [
            "name",
            "title",
            "summary",
            "required",
            "read_only",
            "category",
            "score",
        ] {
            assert!(hit.get(key).is_some(), "missing {key}");
        }
        assert!(
            hit["summary"]
                .as_str()
                .expect("summary must be a string")
                .chars()
                .count()
                <= 160,
            "summary must be at most 160 chars"
        );
    }

    #[test]
    fn empty_query_and_bad_pattern_and_bad_limit_are_invalid_params() {
        assert!(
            search(SearchParam {
                query: "  ".into(),
                pattern: None,
                category: None,
                limit: None,
            })
            .is_err(),
            "blank query must be rejected"
        );
        assert!(
            search(SearchParam {
                query: "q".into(),
                pattern: Some("(".into()),
                category: None,
                limit: None,
            })
            .is_err(),
            "invalid regex pattern must be rejected"
        );
        assert!(
            search(SearchParam {
                query: "q".into(),
                pattern: None,
                category: None,
                limit: Some(51),
            })
            .is_err(),
            "limit over the max must be rejected"
        );
    }

    #[test]
    fn suggest_returns_near_names_for_typos() {
        let s = suggest("qoute", 5);
        assert!(
            s.contains(&"quote".to_string()),
            "expected quote in suggestions, got {s:?}"
        );
    }
}
