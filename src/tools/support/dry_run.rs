//! The execution gate shared by every money-moving tool.
//!
//! `submit_order` / `cancel_order` / `replace_order` and the grid writes
//! (`grid_submit` / `grid_replace` / `grid_cancel` / `grid_suspend` /
//! `grid_restart`) all preview by default and act only when the caller passes
//! `execute: true`. They share this payload so the instruction a model reads is
//! identical no matter which one it called.

use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;

/// Deliberately imperative: the model must relay the preview and obtain a human
/// "yes" before retrying with `execute`.
pub const NEXT_STEP: &str = concat!(
    "DRY RUN — nothing was sent to the exchange. ",
    "Show this preview to the user and ask them to confirm it. ",
    "Only call this tool again with execute=true after the user has explicitly ",
    "confirmed this exact order. Never set execute=true on your own initiative."
);

/// The standard dry-run envelope: `dry_run` distinguishes it from a real result,
/// `preview` carries what would have been sent.
pub fn result(preview: serde_json::Value) -> Result<CallToolResult, McpError> {
    crate::tools::tool_json(&serde_json::json!({
        "dry_run": true,
        "preview": preview,
        "next_step": NEXT_STEP,
    }))
}
