//! AI Agent tools — workspace listing, agent listing, and conversation management.

use std::collections::HashMap;

use longbridge::httpclient::Json;
use reqwest::Method;
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::error::Error;
use crate::serialize::transform_json;
use crate::tools::McpContext;
use crate::tools::support::http_client::http_get_tool;

/// Parameters for [`ai_agents`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AiAgentsParam {
    /// Workspace ID, obtained from ai_workspaces.
    pub workspace_id: String,
    /// Page number, 1-based (default 1).
    pub page: Option<i32>,
    /// Items per page (default 20, max 100).
    pub limit: Option<i32>,
    /// Fuzzy filter by agent name.
    pub name: Option<String>,
}

/// Parameters for [`ai_conversation`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AiConversationParam {
    /// Agent UID (uid field from ai_agents response).
    pub agent_id: String,
    /// User question or instruction.
    pub query: String,
    /// Session ID; omit to start a new session, pass to continue the same session.
    pub chat_uid: Option<String>,
}

/// Parameters for [`ai_continue_conversation`].
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AiContinueConversationParam {
    /// Agent UID (uid field from ai_agents response).
    pub agent_id: String,
    /// Session ID from the previous ai_conversation response.
    pub chat_uid: String,
    /// Message ID from the previous ai_conversation response.
    pub message_id: String,
    /// Answers to pending questions. Outer key: tool_call_id from interrupt.tool_call_id.
    /// Inner key: question text from interrupt.questions[i].question; inner value: answer string.
    pub answers: HashMap<String, HashMap<String, String>>,
}

/// POST to `path` with `Accept: application/json` and return a normalized JSON tool result.
///
/// The conversation endpoints default to SSE streaming; the Accept header switches
/// the response to non-streaming JSON so the MCP layer can decode it as a string.
async fn post_accept_json(
    client: &longbridge::httpclient::HttpClient,
    path: &str,
    body: serde_json::Value,
) -> Result<CallToolResult, McpError> {
    let resp: String = client
        .request(Method::POST, path)
        .body(Json(body))
        .header("accept", "application/json")
        .response::<String>()
        .send()
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    let json = transform_json(resp.as_bytes()).map_err(Error::Serialize)?;
    Ok(crate::tools::tool_result(json))
}

/// List all AI agent workspaces for the authenticated account.
pub async fn ai_workspaces(mctx: &McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/ai/workspaces", &[]).await
}

/// List AI agents in a workspace.
pub async fn ai_agents(mctx: &McpContext, p: AiAgentsParam) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let page_str = p.page.map(|v| v.to_string());
    let limit_str = p.limit.map(|v| v.to_string());
    let mut params: Vec<(&str, &str)> = Vec::new();
    if let Some(ref ps) = page_str {
        params.push(("page", ps.as_str()));
    }
    if let Some(ref ls) = limit_str {
        params.push(("limit", ls.as_str()));
    }
    if let Some(ref name) = p.name {
        params.push(("name", name.as_str()));
    }
    let path = format!("/v1/ai/workspaces/{}/agents", p.workspace_id);
    http_get_tool(&client, &path, &params).await
}

/// Start or continue a conversation with an AI agent.
pub async fn ai_conversation(
    mctx: &McpContext,
    p: AiConversationParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let mut body = serde_json::json!({ "query": p.query });
    if let Some(chat_uid) = p.chat_uid {
        body["chat_uid"] = serde_json::Value::String(chat_uid);
    }
    let path = format!("/v1/ai/agents/{}/conversations", p.agent_id);
    post_accept_json(&client, &path, body).await
}

/// Resume an interrupted AI agent conversation by providing answers to pending questions.
pub async fn ai_continue_conversation(
    mctx: &McpContext,
    p: AiContinueConversationParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let body = serde_json::json!({ "answers": p.answers });
    let path = format!(
        "/v1/ai/agents/{}/conversations/{}/messages/{}/continue",
        p.agent_id, p.chat_uid, p.message_id
    );
    post_accept_json(&client, &path, body).await
}
