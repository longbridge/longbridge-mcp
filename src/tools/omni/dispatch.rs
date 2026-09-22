//! Dispatch an inner tool by name through the full tool router, so every
//! existing gate (metrics, error envelopes, dry-run confirmation) applies.

use rmcp::RoleServer;
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::model::{CallToolRequestParams, CallToolResult, Content, JsonObject};
use rmcp::service::RequestContext;
use serde_json::Value;

use crate::tools::Longbridge;
use crate::tools::omni::search::suggest;

/// Build the standard error envelope (`error_code`/`message`/`recoverable`/`hint`/`data`).
pub(crate) fn envelope(
    error_code: &str,
    message: String,
    recoverable: &str,
    hint: &str,
    data: Value,
) -> CallToolResult {
    CallToolResult::error(vec![Content::text(
        serde_json::json!({
            "error_code": error_code,
            "message": message,
            "recoverable": recoverable,
            "hint": hint,
            "data": data,
        })
        .to_string(),
    )])
}

/// Whether `name` is a dispatchable inner tool (present in the full router).
pub(crate) fn is_known_tool(name: &str) -> bool {
    crate::tools::cached_router().has_route(name)
}

/// Whether `name` is a write tool (`destructive_hint = true`).
#[allow(dead_code)]
pub(crate) fn is_write_tool(name: &str) -> bool {
    crate::tools::cached_router()
        .get(name)
        .and_then(|t| t.annotations.as_ref())
        .and_then(|a| a.destructive_hint)
        .unwrap_or(false)
}

/// `fix_params` envelope for an unknown inner tool, with near-miss suggestions.
pub(crate) fn unknown_tool(name: &str) -> CallToolResult {
    envelope(
        "unknown_tool",
        format!("Unknown tool `{name}`."),
        "fix_params",
        "Use `search` to find the right tool name, or pick one of data.suggestions.",
        serde_json::json!({ "suggestions": suggest(name, 5) }),
    )
}

/// Per-request dispatcher bound to the caller's context.
#[allow(dead_code)]
pub(crate) struct Dispatcher {
    server: Longbridge,
    ctx: RequestContext<RoleServer>,
    region: Option<longbridge::DcRegion>,
}

impl Dispatcher {
    /// `region` is `None` when no requested tool is region-scoped.
    #[allow(dead_code)]
    pub(crate) fn new(
        server: Longbridge,
        ctx: RequestContext<RoleServer>,
        region: Option<longbridge::DcRegion>,
    ) -> Self {
        Self {
            server,
            ctx,
            region,
        }
    }

    /// Call `tool` with `arguments`. Never returns a protocol error: unknown
    /// tools, hidden tools and argument-parse failures all become envelopes.
    #[allow(dead_code)]
    pub(crate) async fn call(&self, tool: &str, arguments: JsonObject) -> CallToolResult {
        if !is_known_tool(tool) {
            return unknown_tool(tool);
        }
        if let Some(region) = self.region
            && crate::tools::is_hidden_for_dc_region(tool, region)
        {
            return envelope(
                "tool_unavailable_in_region",
                format!("Tool `{tool}` is not available for accounts in the {region} data center."),
                "none",
                "Do not retry; tell the user this tool is not offered for their account region.",
                Value::Null,
            );
        }
        let request = CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments);
        let tcc = ToolCallContext::new(&self.server, request, self.ctx.clone());
        match crate::tools::cached_router().call(tcc).await {
            Ok(result) => result,
            // Argument deserialization failures surface here as INVALID_PARAMS
            // protocol errors; `tool_error` maps that code to `fix_params`.
            Err(err) => crate::tools::tool_error(tool, &err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Dispatcher` is wrapped in `Arc` and used from spawned tasks (task 7),
    /// so it must be `Send + Sync`; this fails to compile otherwise.
    fn assert_send_sync<T>()
    where
        T: Send + Sync,
    {
    }

    #[test]
    fn dispatcher_is_send_and_sync() {
        assert_send_sync::<Dispatcher>();
    }

    #[test]
    fn known_and_write_tool_detection() {
        assert!(is_known_tool("quote"), "quote is a real dispatchable tool");
        assert!(!is_known_tool("search"), "meta-tools are not dispatchable");
        assert!(!is_known_tool("nope"), "nope is not a registered tool name");
        assert!(is_write_tool("submit_order"), "submit_order is destructive");
        assert!(is_write_tool("cancel_order"), "cancel_order is destructive");
        assert!(!is_write_tool("quote"), "quote is read-only");
    }

    #[test]
    fn unknown_tool_envelope_carries_suggestions() {
        let r = unknown_tool("qoute");
        assert_eq!(
            r.is_error,
            Some(true),
            "unknown tool must be an error result"
        );
        let v = crate::tools::jq::result_value(&r);
        assert_eq!(
            v["error_code"], "unknown_tool",
            "error_code must identify the failure kind"
        );
        assert_eq!(
            v["recoverable"], "fix_params",
            "caller should retry with a fixed tool name"
        );
        let s = v["data"]["suggestions"].as_array().expect("suggestions");
        assert!(s.iter().any(|x| x == "quote"), "got {s:?}");
    }

    #[test]
    fn region_scoped_matches_both_lists() {
        assert!(
            crate::tools::is_region_scoped("profit_analysis_realized"),
            "profit_analysis_realized is US-only and thus region-scoped"
        );
        assert!(
            !crate::tools::is_region_scoped("quote"),
            "quote is not restricted to any DC region"
        );
    }
}
