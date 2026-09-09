use axum::http::StatusCode;
use axum::response::IntoResponse;
use prometheus::{Encoder, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder};

use std::sync::LazyLock;

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

tokio::task_local! {
    /// 当前 MCP 请求的来源客户端桶(`"claude"` / `"chatgpt"` / `"other"` /
    /// `"unknown"`),由 `mcp_auth_layer` 在每个 MCP 请求外层设置,供
    /// [`record_tool_call`] 给工具指标打 `client` label。在 MCP 请求之外
    /// (如服务初始化或单元测试)未设置,此时回落为 `"unknown"`。
    pub(crate) static CURRENT_CLIENT: &'static str;
}

/// 把 MCP 客户端的 `User-Agent` 映射到一个有界的产品桶,以便作为低基数的
/// Prometheus label。大小写不敏感的子串匹配;缺失或空白归为 `"unknown"`。
///
/// Claude 各产品发的 UA 有区分度(实测):claude.ai 网页/connector 发
/// `Mozilla/5.0 ... Claude-User/1.0 ... +claude-user@anthropic.com`;其余
/// 是 `claude-code/<ver> (<surface>)`,`<surface>` 标明宿主(cli、sdk-cli、
/// sdk-ts、local-agent、claude-desktop、claude-vscode)。匹配顺序从具体到
/// 笼统:先认 claude.ai / desktop / vscode,再落到通用的 claude_code。
/// 「全部 Claude 流量」在查询侧用 `client=~"claude.*"` 聚合。
pub fn classify_client(user_agent: Option<&str>) -> &'static str {
    match user_agent {
        Some(ua) if !ua.trim().is_empty() => {
            let ua = ua.to_ascii_lowercase();
            if ua.contains("claude-user") {
                "claude_ai"
            } else if ua.contains("claude-desktop") {
                "claude_desktop"
            } else if ua.contains("claude-vscode") {
                "claude_vscode"
            } else if ua.contains("claude-code") {
                "claude_code"
            } else if ua.contains("claude") || ua.contains("anthropic") {
                "claude"
            } else if ua.contains("chatgpt") || ua.contains("openai") {
                "chatgpt"
            } else {
                "other"
            }
        }
        _ => "unknown",
    }
}

static TOOL_CALLS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let counter = IntCounterVec::new(
        Opts::new("mcp_tool_calls_total", "Total tool calls"),
        &["tool_name", "client"],
    )
    .unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

static TOOL_CALL_ERRORS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let counter = IntCounterVec::new(
        Opts::new("mcp_tool_call_errors_total", "Total tool call errors"),
        &["tool_name", "client"],
    )
    .unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

static TOOL_CALL_DURATION: LazyLock<HistogramVec> = LazyLock::new(|| {
    let histogram = HistogramVec::new(
        prometheus::HistogramOpts::new(
            "mcp_tool_call_duration_seconds",
            "Tool call duration in seconds",
        ),
        &["tool_name", "client"],
    )
    .unwrap();
    REGISTRY.register(Box::new(histogram.clone())).unwrap();
    histogram
});

static QUOTE_WS_POOL_EVENTS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let counter = IntCounterVec::new(
        Opts::new(
            "mcp_quote_ws_pool_events_total",
            "Quote WebSocket context pool events",
        ),
        &["event"],
    )
    .unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

static QUOTE_WS_POOL_ENTRIES: LazyLock<IntGauge> = LazyLock::new(|| {
    let gauge = IntGauge::new(
        "mcp_quote_ws_pool_entries",
        "Current cached quote WebSocket contexts in this process",
    )
    .unwrap();
    REGISTRY.register(Box::new(gauge.clone())).unwrap();
    gauge
});

static OAUTH_TOKEN_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let counter = IntCounterVec::new(
        Opts::new(
            "mcp_oauth_token_total",
            "OAuth token exchanges proxied through this server",
        ),
        &["grant_type", "result"],
    )
    .unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

static OAUTH_REVOKE_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let counter = IntCounterVec::new(
        Opts::new(
            "mcp_oauth_revoke_total",
            "OAuth token revocations proxied through this server",
        ),
        &["result"],
    )
    .unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

pub fn record_tool_call(tool_name: &str, duration_secs: f64, is_error: bool) {
    let client = CURRENT_CLIENT.try_with(|c| *c).unwrap_or("unknown");
    TOOL_CALLS_TOTAL
        .with_label_values(&[tool_name, client])
        .inc();
    TOOL_CALL_DURATION
        .with_label_values(&[tool_name, client])
        .observe(duration_secs);
    if is_error {
        TOOL_CALL_ERRORS_TOTAL
            .with_label_values(&[tool_name, client])
            .inc();
    }
}

/// 记录一次经本服务代理的 OAuth token 交换。`grant_type` 已由调用方归到有界桶,
/// `result` 为 `"success"`(上游 2xx)或 `"error"`。
pub fn record_oauth_token(grant_type: &str, result: &str) {
    OAUTH_TOKEN_TOTAL
        .with_label_values(&[grant_type, result])
        .inc();
}

/// 记录一次经本服务代理的 OAuth token 撤销(断连信号)。`result` 为
/// `"success"`(上游 2xx)或 `"error"`。
pub fn record_oauth_revoke(result: &str) {
    OAUTH_REVOKE_TOTAL.with_label_values(&[result]).inc();
}

pub fn record_quote_ws_pool_event(event: &str, count: u64) {
    QUOTE_WS_POOL_EVENTS_TOTAL
        .with_label_values(&[event])
        .inc_by(count);
}

pub fn set_quote_ws_pool_entries(entries: usize) {
    QUOTE_WS_POOL_ENTRIES.set(entries as i64);
}

pub async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    match encoder.encode(&metric_families, &mut buffer) {
        Ok(()) => (
            StatusCode::OK,
            [("content-type", "text/plain; version=0.0.4")],
            buffer,
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "text/plain; version=0.0.4")],
            format!("encode error: {e}").into_bytes(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_client_buckets() {
        // Real UAs observed hitting the server (401-rejection logs).
        assert_eq!(
            classify_client(Some(
                "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko; compatible; Claude-User/1.0; +claude-user@anthropic.com)"
            )),
            "claude_ai"
        );
        assert_eq!(
            classify_client(Some(
                "claude-code/2.1.260 (claude-desktop, agent-sdk/0.3.260)"
            )),
            "claude_desktop"
        );
        assert_eq!(
            classify_client(Some(
                "claude-code/2.1.263 (claude-vscode, agent-sdk/0.3.263)"
            )),
            "claude_vscode"
        );
        assert_eq!(
            classify_client(Some("claude-code/2.1.220 (cli)")),
            "claude_code"
        );
        assert_eq!(
            classify_client(Some("claude-code/2.1.218 (sdk-ts, agent-sdk/0.3.218)")),
            "claude_code"
        );
        assert_eq!(classify_client(Some("Anthropic-Internal")), "claude");
        assert_eq!(classify_client(Some("ChatGPT-User/1.0")), "chatgpt");
        assert_eq!(classify_client(Some("x-openai-mcp/2")), "chatgpt");
        assert_eq!(classify_client(Some("curl/8.0")), "other");
        assert_eq!(classify_client(Some("")), "unknown");
        assert_eq!(classify_client(Some("   ")), "unknown");
        assert_eq!(classify_client(None), "unknown");
    }

    #[test]
    fn record_tool_call_labels_by_client() {
        let before = TOOL_CALLS_TOTAL
            .with_label_values(&["telemetry_test_tool", "chatgpt"])
            .get();
        CURRENT_CLIENT.sync_scope("chatgpt", || {
            record_tool_call("telemetry_test_tool", 0.01, false);
        });
        let after = TOOL_CALLS_TOTAL
            .with_label_values(&["telemetry_test_tool", "chatgpt"])
            .get();
        assert_eq!(after - before, 1);
    }

    #[test]
    fn record_tool_call_falls_back_to_unknown_outside_scope() {
        let before = TOOL_CALLS_TOTAL
            .with_label_values(&["telemetry_test_tool2", "unknown"])
            .get();
        record_tool_call("telemetry_test_tool2", 0.01, true);
        let calls = TOOL_CALLS_TOTAL
            .with_label_values(&["telemetry_test_tool2", "unknown"])
            .get();
        let errs = TOOL_CALL_ERRORS_TOTAL
            .with_label_values(&["telemetry_test_tool2", "unknown"])
            .get();
        assert_eq!(calls - before, 1);
        assert!(errs >= 1);
    }

    #[test]
    fn record_oauth_token_increments() {
        let before = OAUTH_TOKEN_TOTAL
            .with_label_values(&["authorization_code", "success"])
            .get();
        record_oauth_token("authorization_code", "success");
        let after = OAUTH_TOKEN_TOTAL
            .with_label_values(&["authorization_code", "success"])
            .get();
        assert_eq!(after - before, 1);
    }

    #[test]
    fn record_oauth_revoke_increments() {
        let before = OAUTH_REVOKE_TOTAL.with_label_values(&["error"]).get();
        record_oauth_revoke("error");
        let after = OAUTH_REVOKE_TOTAL.with_label_values(&["error"]).get();
        assert_eq!(after - before, 1);
    }
}
