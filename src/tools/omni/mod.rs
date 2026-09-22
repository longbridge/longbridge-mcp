//! The `/omni` endpoint: three meta-tools (`search`, `docs`, `execute`) that
//! cover the whole tool catalogue without listing it.

pub(crate) mod dispatch;
pub(crate) mod docs;
pub(crate) mod docs_index;
pub(crate) mod execute;
pub(crate) mod index;
pub(crate) mod pipeline;
pub(crate) mod search;
pub(crate) mod truncate;

use std::sync::Arc;
use std::sync::OnceLock;

use rmcp::ErrorData as McpError;
use rmcp::RoleServer;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Tool};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_router};
use serde_json::Value;

use crate::tools::omni::docs::DocsParam;
use crate::tools::omni::execute::ExecuteParam;
use crate::tools::omni::search::SearchParam;
use crate::tools::{
    Longbridge, extract_context, jq, measured_tool_call, strip_nonstandard_formats,
    strip_null_from_type_arrays,
};

/// `initialize` instructions for `/omni` (the shared jq guidance is appended by the caller).
pub(crate) const INSTRUCTIONS: &str = "Longbridge MCP (omni). Three tools. `search`: find tools by natural-language or Chinese keywords. `docs`: a tool's full input/output schema and notes, topic guides (getting-started, pipelines, symbols, orders, errors, jq), or Longbridge OpenAPI documentation (`query` to search, `page` to read). `execute`: run one tool by name, or a multi-step pipeline where later steps reference earlier results via {\"$from\": id, \"jq\": expr}; use per-step `jq` and `return` so only the final summary comes back. Flow: search → docs (when parameters are non-trivial) → execute. Write tools are two-step: run once without `execute` to get a confirmation_code, show the preview, then re-run with `execute=code`; a pipeline may contain at most one write step and nothing may depend on it. On failure, tools return a JSON envelope with `error_code` and `recoverable` (reauth / backoff / fix_params / none).";

#[tool_router(router = omni_router_build, vis = "pub(crate)")]
impl Longbridge {
    /// Find tools by keywords.
    #[tool(
        title = "Search tools",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        description = "Find Longbridge MCP tools by natural-language or Chinese keywords (e.g. \"latest quote\", \"市场温度\", \"place order\"). Returns compact hits: name, title, summary, required params, read_only, category. Then call `docs` for the full schema and `execute` to run."
    )]
    async fn search(
        &self,
        Parameters(p): Parameters<SearchParam>,
    ) -> Result<CallToolResult, McpError> {
        measured_tool_call("search", format!("{p:?}"), || async { search::search(p) }).await
    }

    /// Read documentation.
    #[tool(
        title = "Docs",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Documentation. `tool`/`tools`: a tool's full input schema, output schema, localized description and notes. `topic`: guides (getting-started, pipelines, symbols, orders, errors, jq). `query`: search Longbridge OpenAPI docs (open.longbridge.com); `page`: read one docs page (e.g. trade/order/submit). No arguments: the catalogue."
    )]
    async fn docs(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<DocsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("docs", format!("{p:?}"), || docs::docs(&mctx, p)).await
    }

    /// Execute a tool or a pipeline.
    #[tool(
        title = "Execute",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Run a Longbridge MCP tool by name: {\"tool\": \"quote\", \"arguments\": {\"symbols\": [\"700.HK\"]}}. Or a pipeline: {\"steps\": [{\"id\", \"tool\", \"arguments\", \"jq\"}...], \"return\": [ids]} where any argument may be {\"$from\": id, \"jq\": expr}. Max 10 steps, 30 s. Write tools are two-step (dry-run then execute=confirmation_code); at most one write step per pipeline and its symbol/side/quantity/price must be literal. Use `search` to find tools and `docs` for schemas."
    )]
    async fn execute(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ExecuteParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        let server = self.clone();
        measured_tool_call("execute", format!("{p:?}"), || {
            execute::execute(server, ctx, &mctx, p)
        })
        .await
    }
}

/// The omni router, built once.
pub(crate) fn omni_router() -> &'static ToolRouter<Longbridge> {
    static ROUTER: OnceLock<ToolRouter<Longbridge>> = OnceLock::new();
    ROUTER.get_or_init(Longbridge::omni_router_build)
}

/// The three omni tools as listed to clients (with `_jq` described).
pub(crate) fn tools() -> &'static [Tool] {
    static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        omni_router()
            .list_all()
            .into_iter()
            .map(|mut tool| {
                let mut schema = Value::Object((*tool.input_schema).clone());
                strip_null_from_type_arrays(&mut schema);
                strip_nonstandard_formats(&mut schema);
                if let Value::Object(map) = schema {
                    tool.input_schema = Arc::new(map);
                }
                jq::describe(&mut tool);
                tool
            })
            .collect()
    })
}

/// Whether `name` is one of the omni meta-tools.
pub(crate) fn is_omni_tool(name: &str) -> bool {
    matches!(name, "search" | "docs" | "execute")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omni_lists_exactly_three_tools_with_jq_and_small_payload() {
        let tools = tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["docs", "execute", "search"],
            "the omni endpoint must list exactly the three meta-tools"
        );
        for t in tools {
            assert!(
                t.input_schema
                    .get("properties")
                    .and_then(|p| p.get("_jq"))
                    .is_some(),
                "{} lacks _jq",
                t.name
            );
            assert!(
                t.output_schema.is_none(),
                "{} must not carry an output schema",
                t.name
            );
        }
        let json = serde_json::to_string(tools).expect("omni tools are serializable");
        assert!(
            json.len() < 8 * 1024,
            "omni tools/list is {} bytes",
            json.len()
        );
    }

    #[test]
    fn execute_is_marked_destructive_and_search_docs_read_only() {
        let by = |n: &str| {
            tools()
                .iter()
                .find(|t| t.name == n)
                .unwrap_or_else(|| panic!("omni tool `{n}` is listed"))
                .annotations
                .clone()
                .unwrap_or_else(|| panic!("omni tool `{n}` carries annotations"))
        };
        assert_eq!(
            by("execute").destructive_hint,
            Some(true),
            "`execute` can run write tools, so it must be marked destructive"
        );
        assert_eq!(
            by("search").read_only_hint,
            Some(true),
            "`search` must be marked read-only"
        );
        assert_eq!(
            by("docs").read_only_hint,
            Some(true),
            "`docs` must be marked read-only"
        );
    }

    #[test]
    fn main_tool_list_is_unchanged_by_omni() {
        assert!(
            !crate::tools::list_tools()
                .iter()
                .any(|t| is_omni_tool(&t.name)),
            "omni meta-tools must not leak into the main tool list"
        );
        assert!(
            is_omni_tool("execute") && !is_omni_tool("quote"),
            "is_omni_tool must match exactly the three meta-tools"
        );
    }
}
