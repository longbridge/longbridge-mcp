//! Longbridge OpenAPI documentation snapshot: local search over
//! `data/docs-index.json`, whole-page reads, and an optional live fetch.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::tools::omni::docs::Lang;
use crate::tools::omni::index::{Doc, Field, Index};
use crate::tools::support::text::clip_chars;

const SITE: &str = "https://open.longbridge.com";
const EXCERPT_CHARS: usize = 400;
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// One `##` section of a documentation page.
#[derive(Debug, Deserialize)]
pub(crate) struct Section {
    pub heading: String,
    pub text: String,
}

/// One documentation page in one language.
#[derive(Debug, Deserialize)]
pub(crate) struct Page {
    pub path: String,
    pub lang: String,
    pub title: String,
    pub markdown: String,
    pub sections: Vec<Section>,
}

/// The bundled snapshot.
#[derive(Debug, Deserialize)]
pub(crate) struct Snapshot {
    pub generated_at: String,
    pub pages: Vec<Page>,
    pub tool_pages: HashMap<String, Vec<String>>,
}

/// The snapshot, parsed once.
pub(crate) fn snapshot() -> &'static Snapshot {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(|| {
        serde_json::from_str(include_str!("../../../data/docs-index.json"))
            .expect("docs-index.json must be valid")
    })
}

/// The snapshot language a `Lang` reads from (`zh-HK` has no crawled pages,
/// so it falls back to Simplified Chinese, the closest bundled text).
fn snapshot_lang(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "en",
        Lang::ZhCn | Lang::ZhHk => "zh-CN",
    }
}

/// Section index per language, keyed `"<page index>:<section index>"`.
fn section_index(lang: Lang) -> &'static Index {
    static EN: OnceLock<Index> = OnceLock::new();
    static ZH: OnceLock<Index> = OnceLock::new();
    let cell = match snapshot_lang(lang) {
        "en" => &EN,
        _ => &ZH,
    };
    cell.get_or_init(|| {
        let want = snapshot_lang(lang);
        let docs = snapshot()
            .pages
            .iter()
            .enumerate()
            .filter(|(_, p)| p.lang == want)
            .flat_map(|(pi, page)| {
                page.sections.iter().enumerate().map(move |(si, s)| Doc {
                    key: format!("{pi}:{si}"),
                    fields: vec![
                        Field {
                            weight: 8.0,
                            text: page.title.clone(),
                        },
                        Field {
                            weight: 5.0,
                            text: page.path.replace(['/', '-'], " "),
                        },
                        Field {
                            weight: 4.0,
                            text: s.heading.clone(),
                        },
                        Field {
                            weight: 1.0,
                            text: s.text.clone(),
                        },
                    ],
                })
            })
            .collect();
        Index::build(docs)
    })
}

/// Public URL of a page.
pub(crate) fn page_url(path: &str, lang: Lang) -> String {
    match lang {
        Lang::En => format!("{SITE}/docs/{path}"),
        Lang::ZhCn => format!("{SITE}/zh-CN/docs/{path}"),
        Lang::ZhHk => format!("{SITE}/zh-HK/docs/{path}"),
    }
}

/// Search sections and roll hits up to one entry per page: a page's score is
/// the sum of its matching sections' BM25 scores (a page relevant across
/// several sections outranks one short section that happens to repeat the
/// query terms densely), and its excerpt comes from its single best section.
/// Shape: `{page,title,heading,url,excerpt,score}`.
pub(crate) fn search_docs(query: &str, lang: Lang, limit: usize) -> Vec<Value> {
    let s = snapshot();
    let index = section_index(lang);
    let mut totals: HashMap<usize, f32> = HashMap::new();
    let mut best: HashMap<usize, (f32, usize)> = HashMap::new();
    for hit in index.search(query, index.len()) {
        let (pi, si) = hit
            .key
            .split_once(':')
            .expect("key format is `page:section`");
        let pi: usize = pi.parse().expect("page index must be numeric");
        let si: usize = si.parse().expect("section index must be numeric");
        *totals.entry(pi).or_insert(0.0) += hit.score;
        let entry = best.entry(pi).or_insert((hit.score, si));
        if hit.score > entry.0 {
            *entry = (hit.score, si);
        }
    }
    let mut ranked: Vec<(usize, f32)> = totals.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| s.pages[a.0].path.cmp(&s.pages[b.0].path))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|(pi, total)| {
            let page = &s.pages[pi];
            let (_, si) = best[&pi];
            let section = &page.sections[si];
            serde_json::json!({
                "page": page.path,
                "title": page.title,
                "heading": section.heading,
                "url": page_url(&page.path, lang),
                "excerpt": clip_chars(&section.text, EXCERPT_CHARS),
                "score": (total * 10.0).round() / 10.0,
            })
        })
        .collect()
}

/// Whole-page Markdown from the snapshot.
pub(crate) fn page_from_snapshot(path: &str, lang: Lang) -> Option<String> {
    let want = snapshot_lang(lang);
    snapshot()
        .pages
        .iter()
        .find(|p| p.path == path && p.lang == want)
        .map(|p| p.markdown.clone())
}

/// `a/b-c/d` style paths only: lowercase, digits, `-`, `_`, single `/` separators.
pub(crate) fn valid_page_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 200
        && path.split('/').all(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        })
}

/// Fetch the live Markdown source of a page (no user credentials are sent).
pub(crate) async fn fetch_page(path: &str, lang: Lang) -> Result<String, String> {
    if !valid_page_path(path) {
        return Err("invalid page path".into());
    }
    let url = format!("{}.md", page_url(path, lang));
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent("longbridge-mcp omni-docs")
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {url}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

/// Documentation pages related to a tool (from `data/docs-tool-pages.json`).
pub(crate) fn related_pages(tool: &str) -> Vec<String> {
    snapshot().tool_pages.get(tool).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::omni::docs::Lang;

    #[test]
    fn snapshot_loads_with_both_languages_and_generated_at() {
        let s = snapshot();
        assert!(
            s.pages.iter().any(|p| p.lang == "en"),
            "snapshot must contain English pages"
        );
        assert!(
            s.pages.iter().any(|p| p.lang == "zh-CN"),
            "snapshot must contain zh-CN pages"
        );
        assert!(
            !s.generated_at.is_empty(),
            "snapshot must record a generation timestamp"
        );
    }

    #[test]
    fn english_and_chinese_queries_find_submit_order_page() {
        let hits = search_docs("submit order", Lang::En, 5);
        assert!(
            hits.iter().any(|h| h["page"] == "trade/order/submit"),
            "got {hits:?}"
        );
        let hits = search_docs("提交订单 委托", Lang::ZhCn, 5);
        assert!(
            hits.iter().any(|h| h["page"] == "trade/order/submit"),
            "got {hits:?}"
        );
        let hit = &hits[0];
        for key in ["page", "title", "heading", "url", "excerpt", "score"] {
            assert!(hit.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn page_from_snapshot_and_url() {
        let md = page_from_snapshot("trade/order/submit", Lang::En).expect("page present");
        assert!(md.contains("## Request"), "got {md}");
        assert_eq!(
            page_url("trade/order/submit", Lang::ZhCn),
            "https://open.longbridge.com/zh-CN/docs/trade/order/submit",
            "zh-CN page URLs must use the /zh-CN/docs/ prefix"
        );
        assert_eq!(
            page_url("trade/order/submit", Lang::En),
            "https://open.longbridge.com/docs/trade/order/submit",
            "English page URLs must have no language prefix"
        );
        assert!(
            page_from_snapshot("trade/order/submit", Lang::ZhHk).is_some(),
            "zh-HK falls back to zh-CN"
        );
    }

    #[test]
    fn page_path_validation_blocks_traversal_and_schemes() {
        assert!(
            valid_page_path("trade/order/submit"),
            "a normal path must be valid"
        );
        assert!(
            !valid_page_path("../etc/passwd"),
            "a path-traversal attempt must be rejected"
        );
        assert!(
            !valid_page_path("https://evil"),
            "a full URL must be rejected"
        );
        assert!(
            !valid_page_path("trade//order"),
            "an empty path segment must be rejected"
        );
        assert!(!valid_page_path(""), "an empty path must be rejected");
    }

    #[test]
    fn every_tool_page_mapping_points_at_a_snapshot_page() {
        let s = snapshot();
        for (tool, pages) in &s.tool_pages {
            assert!(
                crate::tools::omni::dispatch::is_known_tool(tool),
                "unknown tool {tool} in docs-tool-pages.json"
            );
            for page in pages {
                assert!(
                    s.pages.iter().any(|p| &p.path == page),
                    "tool {tool} maps to missing page {page}"
                );
            }
        }
        assert!(
            related_pages("submit_order").contains(&"trade/order/submit".to_string()),
            "submit_order must map to trade/order/submit"
        );
    }
}
