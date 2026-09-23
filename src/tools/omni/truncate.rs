//! Bound the size of what `execute` returns to the model.

use std::future::Future;

use rmcp::ErrorData as McpError;
use rmcp::model::{CallToolRequestParams, CallToolResult, Content};

use crate::tools::jq;

/// Default cap on the final response, in estimated tokens.
pub(crate) const MAX_OUTPUT_TOKENS: usize = 6000;
const CHARS_PER_TOKEN: usize = 4;
const MARKER: &str = "\n--- TRUNCATED ---\nResponse exceeded the output budget. Narrow it with `return`, a per-step `jq`, or a top-level `_jq`.";

/// Rough token estimate: one token per four characters, rounded up.
pub(crate) fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(CHARS_PER_TOKEN)
}

/// Cut text content beyond `max_tokens`, append a marker and drop
/// `structured_content` (it would no longer match the text).
pub(crate) fn truncate_result(mut result: CallToolResult, max_tokens: usize) -> CallToolResult {
    let total: usize = result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .map(|t| estimate_tokens(&t.text))
        .sum();
    if total <= max_tokens {
        return result;
    }
    let budget_chars = max_tokens * CHARS_PER_TOKEN;
    let joined: String = result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut cut: String = joined.chars().take(budget_chars).collect();
    cut.push_str(MARKER);
    result.content = vec![Content::text(cut)];
    result.structured_content = None;
    result
}

/// Run an omni meta-tool through the caller's top-level `_jq`, then bound the
/// filtered result to [`MAX_OUTPUT_TOKENS`].
///
/// The order matters: `_jq` is how a caller narrows an oversized response, so
/// it must see the complete JSON. Cutting first would hand the filter a
/// marker-suffixed string that no longer parses.
pub(crate) async fn call_with_budget<F, Fut>(
    request: CallToolRequestParams,
    invoke: F,
) -> Result<CallToolResult, McpError>
where
    F: FnOnce(CallToolRequestParams) -> Fut,
    Fut: Future<Output = Result<CallToolResult, McpError>>,
{
    jq::call(request, invoke)
        .await
        .map(|result| truncate_result(result, MAX_OUTPUT_TOKENS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_is_ceil_of_quarter_chars() {
        assert_eq!(estimate_tokens(""), 0, "empty text costs no tokens");
        assert_eq!(
            estimate_tokens("abcd"),
            1,
            "exactly four chars is one token"
        );
        assert_eq!(
            estimate_tokens("abcde"),
            2,
            "five chars rounds up to two tokens"
        );
    }

    #[test]
    fn small_result_is_untouched() {
        let r = CallToolResult::success(vec![Content::text("{\"a\":1}")]);
        let out = truncate_result(r.clone(), 100);
        assert_eq!(
            out.content[0].as_text().expect("text").text,
            "{\"a\":1}",
            "a result under budget must pass through unchanged"
        );
    }

    #[test]
    fn large_result_is_cut_with_marker_and_structured_content_dropped() {
        let big = "x".repeat(10_000);
        let mut r = CallToolResult::success(vec![Content::text(big)]);
        r.structured_content = Some(serde_json::json!({"k": "v"}));
        let out = truncate_result(r, 100);
        let text = &out.content[0].as_text().expect("text").text;
        assert!(text.contains("--- TRUNCATED ---"), "marker missing");
        assert!(
            text.chars().count() < 1000,
            "should be cut near 400 chars plus hint"
        );
        assert!(
            out.structured_content.is_none(),
            "structured_content no longer matches the cut text and must be dropped"
        );
    }

    fn request(jq: Option<&str>) -> CallToolRequestParams {
        let mut request = CallToolRequestParams::new("execute");
        if let Some(jq) = jq {
            request.arguments = Some(
                serde_json::json!({ "_jq": jq })
                    .as_object()
                    .expect("object literal")
                    .clone(),
            );
        }
        request
    }

    fn big_series() -> CallToolResult {
        let rows: Vec<serde_json::Value> = (0..2_000)
            .map(|i| serde_json::json!({"inflow": format!("{i}.123"), "timestamp": "2026-09-22T13:30:00Z"}))
            .collect();
        CallToolResult::success(vec![Content::text(
            serde_json::Value::Array(rows).to_string(),
        )])
    }

    #[tokio::test]
    async fn top_level_jq_sees_the_full_result_before_the_budget_cuts_it() {
        let out = call_with_budget(request(Some(".[-1].inflow")), |_| async {
            Ok(big_series())
        })
        .await
        .expect("call succeeds");
        assert_ne!(
            out.is_error,
            Some(true),
            "the filter must run on parsed JSON, not cut text"
        );
        assert_eq!(
            out.content[0].as_text().expect("text").text,
            "\"1999.123\"",
            "the filter must see the last element of the untruncated result"
        );
    }

    #[tokio::test]
    async fn unfiltered_oversized_result_is_still_cut() {
        let out = call_with_budget(request(None), |_| async { Ok(big_series()) })
            .await
            .expect("call succeeds");
        assert!(
            out.content[0]
                .as_text()
                .expect("text")
                .text
                .contains("--- TRUNCATED ---"),
            "without `_jq` the budget still applies"
        );
    }
}
