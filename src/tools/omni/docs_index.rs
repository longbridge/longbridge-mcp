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

/// Page index per language, one `Doc` per page: `title` (weight 12 — raised
/// from an initial 8), the last path segment on its own (weight 30 — see
/// below; e.g. `quote` in `quote/pull/quote`, a page named exactly after the
/// query term is usually what a one-word query wants), the full path with
/// `/`/`-` turned into spaces (5), all section headings joined (4), and all
/// section texts joined (1). Ranking happens here, over whole pages, so
/// BM25's document-length normalisation compares like with like — a page
/// covering a topic across several sections is not outranked by a single
/// short, term-dense section from an unrelated page (see `section_index`,
/// used only to pick an excerpt within the page `search_docs` already
/// chose).
///
/// The last-path-segment weight is tuned well past the 6 first tried: the
/// Longbridge docs site has many sibling `quote/...`/`socket/...` pages that
/// all mention "quote" throughout title/path/headings/body, so single-word
/// queries land in a narrow BM25 score band (roughly 2.6–2.9 for `"quote"`
/// across 15+ candidates); at weight 6 or even 16, `quote/pull/quote` lands
/// just outside the top 5 behind several less-central pages. Weight 30 gives
/// an exact last-segment match (the page *named* after the query) enough of
/// a lead to clear that band with a comfortable margin — see the ranking
/// regression tests below, and Task 10's fix-round-1 report for the
/// per-weight rankings that were tried.
fn page_index(lang: Lang) -> &'static Index {
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
            .map(|(pi, page)| Doc {
                key: pi.to_string(),
                fields: vec![
                    Field {
                        weight: 12.0,
                        text: page.title.clone(),
                    },
                    Field {
                        weight: 30.0,
                        text: page
                            .path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&page.path)
                            .replace('-', " "),
                    },
                    Field {
                        weight: 5.0,
                        text: page.path.replace(['/', '-'], " "),
                    },
                    Field {
                        weight: 4.0,
                        text: page
                            .sections
                            .iter()
                            .map(|s| s.heading.as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                    },
                    Field {
                        weight: 1.0,
                        text: page
                            .sections
                            .iter()
                            .map(|s| s.text.as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                    },
                ],
            })
            .collect();
        Index::build(docs)
    })
}

/// Section index per language, keyed `"<page index>:<section index>"`.
/// Ranking uses `page_index`; this index only locates, within a page
/// `search_docs` already picked, the single section whose own text best
/// matches the query, to serve as `heading`/`excerpt`.
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

/// Rank pages with `page_index`, then pick the `heading`/`excerpt` for each
/// winning page from `section_index`: the query's best-scoring section
/// within that page, falling back to the page's first section, or (a page
/// with no sections) an empty heading and the start of its raw Markdown.
/// One hit per page. Shape: `{page,title,heading,url,excerpt,score}`.
pub(crate) fn search_docs(query: &str, lang: Lang, limit: usize) -> Vec<Value> {
    let s = snapshot();
    let section_idx = section_index(lang);
    let mut best_section: HashMap<usize, usize> = HashMap::new();
    for hit in section_idx.search(query, section_idx.len()) {
        let (pi, si) = hit
            .key
            .split_once(':')
            .expect("key format is `page:section`");
        let pi: usize = pi.parse().expect("page index must be numeric");
        let si: usize = si.parse().expect("section index must be numeric");
        // `search` is sorted by descending score, so the first entry seen
        // for a page is that page's best-scoring section.
        best_section.entry(pi).or_insert(si);
    }
    page_index(lang)
        .search(query, limit)
        .into_iter()
        .map(|hit| {
            let pi: usize = hit.key.parse().expect("page index must be numeric");
            let page = &s.pages[pi];
            let (heading, excerpt) = match best_section
                .get(&pi)
                .copied()
                .and_then(|si| page.sections.get(si))
                .or(page.sections.first())
            {
                Some(section) => (
                    section.heading.clone(),
                    clip_chars(&section.text, EXCERPT_CHARS),
                ),
                None => (String::new(), clip_chars(&page.markdown, EXCERPT_CHARS)),
            };
            serde_json::json!({
                "page": page.path,
                "title": page.title,
                "heading": heading,
                "url": page_url(&page.path, lang),
                "excerpt": excerpt,
                "score": (hit.score * 10.0).round() / 10.0,
            })
        })
        .collect()
}

/// `(path, lang) -> page index`, built once, so `page_from_snapshot` looks
/// pages up in constant time instead of scanning the whole snapshot.
fn page_lookup() -> &'static HashMap<(String, String), usize> {
    static LOOKUP: OnceLock<HashMap<(String, String), usize>> = OnceLock::new();
    LOOKUP.get_or_init(|| {
        snapshot()
            .pages
            .iter()
            .enumerate()
            .map(|(i, p)| ((p.path.clone(), p.lang.clone()), i))
            .collect()
    })
}

/// Whole-page Markdown from the snapshot.
pub(crate) fn page_from_snapshot(path: &str, lang: Lang) -> Option<String> {
    let want = snapshot_lang(lang);
    let &i = page_lookup().get(&(path.to_string(), want.to_string()))?;
    Some(snapshot().pages[i].markdown.clone())
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
    fn common_queries_rank_the_focused_page_over_boilerplate_matches() {
        let hits = search_docs("quote", Lang::En, 5);
        assert!(
            hits.iter().any(|h| h["page"] == "quote/pull/quote"),
            "expected quote/pull/quote in the top 5 for `quote`, got {hits:?}"
        );
        assert!(
            !hits.iter().any(|h| h["page"] == "changelog"),
            "changelog must not crowd out focused pages for `quote`, got {hits:?}"
        );

        let hits = search_docs("order", Lang::En, 5);
        assert!(
            hits.iter().any(|h| h["page"] == "trade/order/submit"),
            "expected trade/order/submit in the top 5 for `order`, got {hits:?}"
        );
        assert!(
            !hits.iter().any(|h| h["page"] == "changelog"),
            "changelog must not crowd out focused pages for `order`, got {hits:?}"
        );

        let hits = search_docs("cancel order", Lang::En, 3);
        assert!(
            hits.iter().any(|h| h["page"] == "trade/order/withdraw"),
            "expected trade/order/withdraw in the top 3 for `cancel order`, got {hits:?}"
        );
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
