//! Tools discovery: list, search, and describe available MCP tools.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let end = s[..max].rfind(' ').unwrap_or(max);
    format!("{}…", &s[..end])
}

fn extract_params(
    schema: &serde_json::Map<String, serde_json::Value>,
) -> Vec<serde_json::Value> {
    let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) else {
        return vec![];
    };
    let required: std::collections::HashSet<&str> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    properties
        .iter()
        .map(|(name, prop)| {
            let type_str = prop
                .get("anyOf")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.get("type").and_then(|v| v.as_str()))
                        .filter(|&t| t != "null")
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .or_else(|| {
                    prop.get("type")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "unknown".to_string());
            let desc = prop
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            serde_json::json!({
                "name": name,
                "type": type_str,
                "required": required.contains(name.as_str()),
                "description": desc,
            })
        })
        .collect()
}

fn make_result(value: serde_json::Value) -> Result<CallToolResult, McpError> {
    let json = serde_json::to_string(&value).map_err(crate::error::Error::Serialize)?;
    let structured = serde_json::from_str::<serde_json::Value>(&json).ok();
    let mut result = CallToolResult::success(vec![rmcp::model::Content::text(json)]);
    result.structured_content = structured;
    Ok(result)
}

/// Parameters for tools_list.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToolsListParam {
    /// Keyword filter — case-insensitive, matched against tool name and description.
    /// Examples: "quote", "trade", "alert", "order", "ipo", "fundamental".
    pub category: Option<String>,
}

/// Parameters for tools_search.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToolsSearchParam {
    /// Search keyword — case-insensitive, matched against tool name and description.
    pub query: String,
    /// Max results to return (default: 20).
    pub limit: Option<u32>,
}

/// Parameters for tools_describe.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToolsDescribeParam {
    /// Exact tool name, e.g. "quote", "depth", "submit_order".
    /// Use tools_list() or tools_search() to discover tool names.
    pub name: String,
}

/// List tools — returns one-line summaries, optionally filtered by keyword.
pub fn tools_list(p: ToolsListParam) -> Result<CallToolResult, McpError> {
    let tools = crate::tools::list_tools();
    let filter = p.category.as_deref().map(|s| s.to_lowercase());
    let entries: Vec<serde_json::Value> = tools
        .iter()
        .filter(|t| match &filter {
            Some(f) => {
                t.name.to_lowercase().contains(f.as_str())
                    || t.description
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(f.as_str())
            }
            None => true,
        })
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": truncate(t.description.as_deref().unwrap_or(""), 120),
            })
        })
        .collect();
    make_result(serde_json::json!({
        "total": entries.len(),
        "hint": "Use tools_describe(name) for full parameter details, tools_search(query) to find tools by keyword.",
        "tools": entries,
    }))
}

/// Search tools — full-text search across names and descriptions.
pub fn tools_search(p: ToolsSearchParam) -> Result<CallToolResult, McpError> {
    let tools = crate::tools::list_tools();
    let q = p.query.to_lowercase();
    let limit = p.limit.unwrap_or(20).min(200) as usize;
    let matches: Vec<serde_json::Value> = tools
        .iter()
        .filter(|t| {
            t.name.to_lowercase().contains(&q)
                || t.description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
        })
        .take(limit)
        .map(|t| {
            let params = extract_params(t.input_schema.as_ref());
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": params,
            })
        })
        .collect();
    make_result(serde_json::json!({
        "query": p.query,
        "total_matches": matches.len(),
        "tools": matches,
    }))
}

/// Describe tool — returns full parameter documentation for one tool.
pub fn tools_describe(p: ToolsDescribeParam) -> Result<CallToolResult, McpError> {
    let tools = crate::tools::list_tools();
    let tool = tools.iter().find(|t| t.name.to_string() == p.name).ok_or_else(|| {
        McpError::invalid_params(
            format!(
                "Tool '{}' not found. Use tools_list() to see all available tools.",
                p.name
            ),
            None,
        )
    })?;
    let params = extract_params(tool.input_schema.as_ref());
    make_result(serde_json::json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": params,
    }))
}
