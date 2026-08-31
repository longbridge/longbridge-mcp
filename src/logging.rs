//! Tracing setup, including the hard caps that keep customer data out of the
//! log files.
//!
//! Several dependencies log complete request/response payloads:
//!
//! - `longbridge_httpcli` logs the OpenAPI request body and the full response
//!   body at INFO — cash balances, positions, order history.
//! - `longbridge_wscli` logs every WebSocket frame at INFO, including the
//!   session auth frame that carries the customer's token and OTP.
//! - `longbridge::trade` logs order push events at INFO.
//! - `rmcp` logs the decoded MCP request and the full tool result at DEBUG, and
//!   raw JSON-RPC frames at TRACE.
//!
//! This crate's own code can too: `measured_tool_call` (`tools/mod.rs`) logs
//! a tool failure's message text on `longbridge_mcp::tools::error_detail`.
//! `src/error.rs` sanitizes the one known `longbridge` SDK error variant that
//! embeds a raw upstream HTTP response body, but that target is capped here
//! as well — a second line of defense for any error path the sanitizer
//! doesn't cover, not a defense against dependencies alone.
//!
//! Those lines write customer account data and credentials to disk. Naming the
//! quiet levels in a default `RUST_LOG` string is not enough, because setting
//! `RUST_LOG` replaces that string wholesale: an operator exporting
//! `RUST_LOG=debug` to chase an unrelated bug silently turns payload logging
//! back on. The caps therefore live in [`payload_guard`], a filter layer that
//! runs alongside the `EnvFilter` and drops those events regardless of what
//! `RUST_LOG` asks for.
//!
//! Set `LONGBRIDGE_MCP_LOG_PAYLOADS=1` to lift the caps when debugging locally
//! against a test account.

use std::path::Path;

use tracing::Metadata;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, filter, fmt};

/// Filter used when `RUST_LOG` is unset.
///
/// The payload targets are listed here too so the common case stays quiet at
/// the source; [`payload_guard`] is what enforces them.
const DEFAULT_DIRECTIVES: &str =
    "info,longbridge_mcp=debug,longbridge_httpcli=warn,longbridge_wscli=warn,rmcp=warn";

/// Set this to `1`/`true`/`yes` to log request/response payloads anyway.
const LOG_PAYLOADS_ENV: &str = "LONGBRIDGE_MCP_LOG_PAYLOADS";

/// Env vars that make the SDK install its own subscriber, out of our reach.
const SDK_LOG_PATH_ENVS: &[&str] = &["LONGBRIDGE_LOG_PATH", "LONGPORT_LOG_PATH"];

/// The most verbose level each payload-logging target may emit.
///
/// Matched by prefix, the same way `EnvFilter` matches targets, so
/// `longbridge_httpcli::request` is covered by the `longbridge_httpcli` entry.
const PAYLOAD_CAPS: &[(&str, LevelFilter)] = &[
    // Full OpenAPI request and response bodies at INFO.
    ("longbridge_httpcli", LevelFilter::WARN),
    // Full WebSocket frames at INFO, auth token included.
    ("longbridge_wscli", LevelFilter::WARN),
    // Order push events at INFO.
    ("longbridge::trade", LevelFilter::WARN),
    // Decoded MCP requests and tool results at DEBUG, raw frames at TRACE.
    // INFO stays allowed: it only names the tool being called.
    ("rmcp", LevelFilter::INFO),
    // `measured_tool_call`'s failure log (src/tools/mod.rs) renders an
    // `McpError`'s message text. For most errors that's our own short,
    // structured text, but for an upstream response the SDK couldn't parse
    // into its envelope, `longbridge::Error`'s Display embeds the raw HTTP
    // response body verbatim — the same class of payload the caps above
    // exist to keep out of logs. The tool/code/latency fields that make the
    // event useful for triage are logged separately, uncapped, at the
    // `longbridge_mcp::tools` target; only the free-text detail lives here,
    // off by default.
    ("longbridge_mcp::tools::error_detail", LevelFilter::OFF),
];

/// Whether an event from `target` at `level` may be logged.
fn payload_allowed(target: &str, level: tracing::Level) -> bool {
    PAYLOAD_CAPS
        .iter()
        .filter(|(prefix, _)| target.starts_with(prefix))
        .all(|(_, cap)| *cap >= level)
}

/// Whether the operator opted into payload logging.
fn log_payloads_enabled() -> bool {
    std::env::var(LOG_PAYLOADS_ENV)
        .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn metadata_allowed(meta: &Metadata<'_>) -> bool {
    payload_allowed(meta.target(), *meta.level())
}

/// Filter layer that drops payload-bearing events above their cap.
///
/// `None` when the operator opted into payload logging, leaving only the
/// `EnvFilter` in effect.
fn payload_guard() -> Option<filter::FilterFn> {
    if log_payloads_enabled() {
        return None;
    }
    Some(filter::filter_fn(
        metadata_allowed as fn(&Metadata<'_>) -> bool,
    ))
}

fn env_filter(default: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default))
}

/// Warn when the SDK has been pointed at a log directory of its own.
///
/// `longbridge::Config` reads that path from the environment and installs a
/// private subscriber that writes every `longbridge*` event at INFO — request
/// and response bodies included — into that directory. It bypasses the
/// subscriber we install here, so [`payload_guard`] cannot cover it.
fn warn_on_sdk_log_path() {
    if log_payloads_enabled() {
        return;
    }
    for env in SDK_LOG_PATH_ENVS {
        if std::env::var_os(env).is_some() {
            tracing::warn!(
                env,
                "SDK log directory is set: the Longbridge SDK writes unfiltered request and \
                 response payloads there, outside this server's log filter. Unset it in production."
            );
        }
    }
}

/// Initialise logging for the HTTP server.
///
/// Writes rolling daily files to `log_dir` when given, otherwise stdout.
pub fn init(log_dir: Option<&Path>) {
    let registry = tracing_subscriber::registry()
        .with(env_filter(DEFAULT_DIRECTIVES))
        .with(payload_guard());

    match log_dir {
        Some(dir) => {
            let file_appender = tracing_appender::rolling::daily(dir, "longbridge-mcp.log");
            registry
                .with(fmt::layer().with_writer(file_appender).with_ansi(false))
                .init();
        }
        // Production runs in a container with stdout piped to a log collector,
        // not an interactive terminal — the default ANSI color codes land in the
        // collected log as literal `\x1b[2m`-style escapes instead of rendering,
        // which is what made the exported CSV unreadable. Same fix as the
        // file-writer branch above.
        None => registry.with(fmt::layer().with_ansi(false)).init(),
    }

    warn_on_sdk_log_path();
}

/// Initialise logging for `--stdio` mode, where stdout is the MCP transport and
/// every log line has to go to stderr.
pub fn init_stdio() {
    tracing_subscriber::registry()
        .with(env_filter("info"))
        .with(payload_guard())
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();

    warn_on_sdk_log_path();
}

#[cfg(test)]
mod tests {
    use tracing::Level;

    use super::*;
    use crate::test_support::SharedBuffer;

    #[test]
    fn http_and_ws_payloads_are_capped_at_warn() {
        // `http request` / `http response` and `ws request` / `ws response` are
        // emitted at INFO and carry account data and credentials.
        assert!(!payload_allowed("longbridge_httpcli::request", Level::INFO));
        assert!(!payload_allowed(
            "longbridge_httpcli::request",
            Level::DEBUG
        ));
        assert!(!payload_allowed("longbridge_wscli::client", Level::INFO));
        assert!(payload_allowed("longbridge_httpcli::request", Level::WARN));
        assert!(payload_allowed("longbridge_wscli::client", Level::ERROR));
    }

    #[test]
    fn trade_push_events_are_capped_at_warn() {
        assert!(!payload_allowed("longbridge::trade::core", Level::INFO));
        assert!(payload_allowed("longbridge::trade::core", Level::WARN));
        // Quote-side logs carry symbols only, so they keep their INFO level.
        assert!(payload_allowed("longbridge::quote::core", Level::INFO));
    }

    #[test]
    fn mcp_request_and_result_logging_is_capped_at_info() {
        // `received request` and `response message` are DEBUG, raw frames TRACE.
        assert!(!payload_allowed("rmcp::service", Level::DEBUG));
        assert!(!payload_allowed(
            "rmcp::transport::streamable_http_server::tower",
            Level::TRACE
        ));
        // INFO only names the tool being called.
        assert!(payload_allowed("rmcp::handler::server", Level::INFO));
    }

    #[test]
    fn unrelated_targets_are_untouched() {
        assert!(payload_allowed("longbridge_mcp::tools", Level::TRACE));
        assert!(payload_allowed("longbridge_mcp::auth", Level::DEBUG));
        assert!(payload_allowed("axum::serve", Level::TRACE));
        assert!(payload_allowed("", Level::TRACE));
    }

    /// `measured_tool_call`'s failure message can, for some upstream error
    /// paths, embed a raw HTTP response body rather than our own short text
    /// (see the comment at that call site) — same class of payload as the
    /// other caps in this table, so it stays off entirely by default.
    #[test]
    fn tool_call_error_detail_is_off_by_default() {
        assert!(
            !payload_allowed("longbridge_mcp::tools::error_detail", Level::ERROR),
            "error_detail must stay capped even at ERROR, the least verbose level"
        );
        // The safe fields (tool name, code, latency) live on the plain
        // `longbridge_mcp::tools` target, which stays uncapped.
        assert!(
            payload_allowed("longbridge_mcp::tools", Level::WARN),
            "the classification event's own target must not be capped"
        );
    }

    /// The guard has to hold even when the operator asks for everything, which
    /// is the case that used to leak.
    #[test]
    fn rust_log_trace_still_cannot_write_payloads() {
        let buffer = SharedBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new("trace"))
            .with(filter::filter_fn(
                metadata_allowed as fn(&Metadata<'_>) -> bool,
            ))
            .with(
                fmt::layer()
                    .with_writer(move || writer.clone())
                    .with_ansi(false)
                    .without_time(),
            );

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "longbridge_httpcli::request", body = "{\"total_cash\":\"123456\"}", "http response");
            tracing::info!(target: "longbridge_wscli::client", message = "AuthRequest { token: \"secret\" }", "ws request");
            tracing::info!(target: "longbridge::trade::core", event = "OrderChanged { quantity: 100 }", "push event");
            tracing::debug!(target: "rmcp::service", result = "{\"positions\":[]}", "response message");
            // The one first-party payload-bearing target, alongside the
            // vendor ones above — `measured_tool_call`'s error-detail line.
            tracing::warn!(target: "longbridge_mcp::tools::error_detail", error = "unexpected HTTP response: status=502, body=<html>upstream gateway error</html>", "tool call error detail");
            tracing::info!(target: "longbridge_mcp::tools", count = 151, "tools registered");
        });

        let logged = buffer.contents();
        assert!(!logged.contains("http response"), "leaked: {logged}");
        assert!(!logged.contains("ws request"), "leaked: {logged}");
        assert!(!logged.contains("push event"), "leaked: {logged}");
        assert!(!logged.contains("response message"), "leaked: {logged}");
        assert!(!logged.contains("total_cash"), "leaked: {logged}");
        assert!(!logged.contains("secret"), "leaked: {logged}");
        assert!(
            !logged.contains("upstream gateway error"),
            "leaked: {logged}"
        );
        // Our own events still get through.
        assert!(logged.contains("tools registered"), "missing: {logged}");
    }
}
