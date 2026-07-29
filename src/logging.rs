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
        None => registry.with(fmt::layer()).init(),
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
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use tracing::Level;

    use super::*;

    /// Collects formatted log output so a test can assert on it.
    #[derive(Clone)]
    struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

    impl SharedBuffer {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }

        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().expect("buffer poisoned").clone())
                .expect("log output is not utf-8")
        }
    }

    impl Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("buffer poisoned").write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

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

    /// The guard has to hold even when the operator asks for everything, which
    /// is the case that used to leak.
    #[test]
    fn rust_log_trace_still_cannot_write_payloads() {
        let buffer = SharedBuffer::new();
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
            tracing::info!(target: "longbridge_mcp::tools", count = 151, "tools registered");
        });

        let logged = buffer.contents();
        assert!(!logged.contains("http response"), "leaked: {logged}");
        assert!(!logged.contains("ws request"), "leaked: {logged}");
        assert!(!logged.contains("push event"), "leaked: {logged}");
        assert!(!logged.contains("response message"), "leaked: {logged}");
        assert!(!logged.contains("total_cash"), "leaked: {logged}");
        assert!(!logged.contains("secret"), "leaked: {logged}");
        // Our own events still get through.
        assert!(logged.contains("tools registered"), "missing: {logged}");
    }
}
