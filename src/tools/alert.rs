use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use rmcp::serde::Deserialize;

use crate::counter::symbol_to_counter_id;
use crate::tools::http_client::{http_delete_tool, http_get_tool, http_post_tool};
use crate::tools::tool_result;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AlertAddParam {
    /// Security symbol, e.g. "700.HK"
    pub symbol: String,
    /// Alert condition: "price_rise", "price_fall", "percent_rise", "percent_fall"
    pub condition: String,
    /// Threshold price or percentage value
    pub price: String,
    /// Alert frequency: "once", "daily", "every"
    pub frequency: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AlertIdParam {
    /// Alert indicator id
    pub alert_id: String,
}

pub async fn alert_list(mctx: &crate::tools::McpContext) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    http_get_tool(&client, "/v1/notify/reminders", &[]).await
}

pub async fn alert_add(
    mctx: &crate::tools::McpContext,
    p: AlertAddParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let cid = symbol_to_counter_id(&p.symbol);
    let indicator_id: i32 = match p.condition.as_str() {
        "percent_fall" => 4,
        "percent_rise" => 3,
        "price_fall" => 2,
        _ => 1,
    };
    let freq: i32 = match p.frequency.as_deref() {
        Some("daily") => 1,
        Some("every") => 2,
        _ => 3,
    };
    let setting_key = if indicator_id == 3 || indicator_id == 4 {
        "chg"
    } else {
        "price"
    };
    let body = serde_json::json!({
        "counter_id": cid,
        "indicator_id": indicator_id.to_string(),
        "value_map": { setting_key: p.price },
        "frequency": freq,
        "enabled": true,
        "scope": 0,
        "state": [1],
    });
    http_post_tool(&client, "/v1/notify/reminders", body).await
}

pub async fn alert_delete(
    mctx: &crate::tools::McpContext,
    p: AlertIdParam,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let id_num: i64 = p
        .alert_id
        .parse()
        .map_err(|_| McpError::invalid_params("invalid alert_id", None))?;
    let body = serde_json::json!({ "ids": [id_num] });
    http_delete_tool(&client, "/v1/notify/reminders", body).await
}

pub async fn alert_enable(
    mctx: &crate::tools::McpContext,
    p: AlertIdParam,
) -> Result<CallToolResult, McpError> {
    alert_set_enabled(mctx, &p.alert_id, true).await
}

pub async fn alert_disable(
    mctx: &crate::tools::McpContext,
    p: AlertIdParam,
) -> Result<CallToolResult, McpError> {
    alert_set_enabled(mctx, &p.alert_id, false).await
}

async fn alert_set_enabled(
    mctx: &crate::tools::McpContext,
    alert_id: &str,
    enabled: bool,
) -> Result<CallToolResult, McpError> {
    let client = mctx.create_http_client();
    let id_num: i64 = alert_id
        .parse()
        .map_err(|_| McpError::invalid_params("invalid alert_id", None))?;

    let list_data = {
        use longbridge::httpclient::{Json, Method};
        let params: Vec<(&str, &str)> = vec![];
        let resp = client
            .request(Method::GET, "/v1/notify/reminders")
            .query_params(params)
            .response::<Json<serde_json::Value>>()
            .send()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        resp.0
    };

    let stocks = list_data["lists"]
        .as_array()
        .or_else(|| list_data["list"].as_array())
        .cloned()
        .unwrap_or_default();

    for stock in &stocks {
        let counter_id = stock["counter_id"].as_str().unwrap_or("");
        if let Some(indicators) = stock["indicators"].as_array() {
            for ind in indicators {
                let ind_id = ind["id"]
                    .as_str()
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(0);
                if ind_id == id_num {
                    let body = serde_json::json!({
                        "id": ind_id,
                        "counter_id": counter_id,
                        "indicator_id": ind["indicator_id"].as_str().unwrap_or("1"),
                        "value_map": ind["value_map"],
                        "frequency": ind["frequency"],
                        "enabled": enabled,
                        "scope": ind["scope"],
                        "state": ind["state"],
                    });
                    http_post_tool(&client, "/v1/notify/reminders", body).await?;
                    let action = if enabled { "enabled" } else { "disabled" };
                    return Ok(tool_result(format!("alert {alert_id} {action}")));
                }
            }
        }
    }

    Err(McpError::invalid_params(
        format!("alert id {alert_id} not found"),
        None,
    ))
}
