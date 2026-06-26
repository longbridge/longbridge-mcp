use axum::http::StatusCode;
use axum::response::IntoResponse;
use prometheus::{Encoder, HistogramVec, IntCounterVec, IntGauge, Opts, Registry, TextEncoder};

use std::sync::LazyLock;

static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::new);

static TOOL_CALLS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let counter = IntCounterVec::new(
        Opts::new("mcp_tool_calls_total", "Total tool calls"),
        &["tool_name"],
    )
    .unwrap();
    REGISTRY.register(Box::new(counter.clone())).unwrap();
    counter
});

static TOOL_CALL_ERRORS_TOTAL: LazyLock<IntCounterVec> = LazyLock::new(|| {
    let counter = IntCounterVec::new(
        Opts::new("mcp_tool_call_errors_total", "Total tool call errors"),
        &["tool_name"],
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
        &["tool_name"],
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

pub fn record_tool_call(tool_name: &str, duration_secs: f64, is_error: bool) {
    TOOL_CALLS_TOTAL.with_label_values(&[tool_name]).inc();
    TOOL_CALL_DURATION
        .with_label_values(&[tool_name])
        .observe(duration_secs);
    if is_error {
        TOOL_CALL_ERRORS_TOTAL.with_label_values(&[tool_name]).inc();
    }
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
