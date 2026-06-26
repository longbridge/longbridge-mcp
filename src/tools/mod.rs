use std::{borrow::Cow, sync::Arc};

use rmcp::ErrorData as McpError;
use rmcp::RoleServer;
use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, ErrorData as McpErrorData, ListResourcesResult, RawResource,
    ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
};
use rmcp::service::RequestContext;
use rmcp::tool;
use rmcp::tool_handler;
use rmcp::tool_router;

use crate::auth::middleware::{AgentEndpoint, BearerToken, RestrictedEndpoint, RestrictedVersion};
use crate::error::Error;
use crate::serialize::to_tool_json;

/// Registered name of the reverse-auth tool, used as the filter key in both
/// `tools_main_endpoint` (excluded) and `tools_agent_endpoint` (only this one).
/// Tied to `fn authenticate` via `measured_tool_call(AUTHENTICATE_TOOL_NAME, ...)`.
/// Change this constant if the method is ever renamed so all three sites stay in sync.
const AUTHENTICATE_TOOL_NAME: &str = "authenticate";
const OUTPUT_SCHEMA_RESOURCE_PREFIX: &str = "lb://tools/";
const OUTPUT_SCHEMA_RESOURCE_SUFFIX: &str = "/output-schema";
const OUTPUT_SCHEMA_RESOURCE_MIME: &str = "application/schema+json";
const TOOL_DESCRIPTION_MAX_CHARS: usize = 240;

tokio::task_local! {
    /// The name of the tool currently executing, scoped around each tool call by
    /// [`measured_tool_call`]. Read by [`McpContext::create_http_client`] and
    /// [`McpContext::create_config`] to tag every upstream Longbridge request
    /// with an `x-mcp-tool` header (the MCP equivalent of the CLI's `x-cli-cmd`),
    /// so server-side stats can attribute requests per tool. Absent outside a
    /// tool call (e.g. server init), in which case no tag is added.
    pub(crate) static CURRENT_TOOL: &'static str;
}

async fn measured_tool_call<F, Fut>(name: &'static str, f: F) -> Result<CallToolResult, McpError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<CallToolResult, McpError>>,
{
    CURRENT_TOOL
        .scope(name, async move {
            let start = std::time::Instant::now();
            let result = f().await;
            let duration = start.elapsed().as_secs_f64();
            crate::metrics::record_tool_call(name, duration, result.is_err());
            result
        })
        .await
}

mod alert;
mod atm;
mod authenticate;
mod calendar;
mod content;
mod dca;
mod fundamental;
mod ipo;
mod market;
mod output;
mod portfolio;
mod quant;
mod quote;
mod screener;
mod search;
mod sharelist;
mod statement;
mod support;
mod trade;

/// Helper to build a JSON Schema `Arc<JsonObject>` from a `JsonSchema`-derived
/// type, suitable for passing to `#[tool(output_schema = ...)]`.
fn schema_for<T>() -> std::sync::Arc<rmcp::model::JsonObject>
where
    T: rmcp::schemars::JsonSchema + 'static,
{
    rmcp::handler::server::common::schema_for_output::<T>()
        .expect("output schema must be a valid JSON Schema with root type \"object\"")
}

/// Longbridge MCP tool server (stateless).
#[derive(Debug, Clone)]
pub struct Longbridge;

pub(crate) fn tool_result(json: String) -> CallToolResult {
    // MCP spec §tool-result: a tool that declares an `outputSchema` MUST
    // return `structuredContent`. We populate it for every response so the
    // invariant holds regardless of which tools gain a schema in the future.
    let structured = serde_json::from_str::<serde_json::Value>(&json)
        .ok()
        .filter(serde_json::Value::is_object);
    let mut result = CallToolResult::success(vec![Content::text(json)]);
    result.structured_content = structured;
    result
}

fn tool_json<T>(value: &T) -> Result<CallToolResult, McpError>
where
    T: serde::Serialize,
{
    let json = to_tool_json(value).map_err(Error::Serialize)?;
    Ok(tool_result(json))
}

/// Per-request context extracted from HTTP headers.
pub struct McpContext {
    pub token: String,
    pub language: Option<String>,
    /// The originating MCP client's `User-Agent` (e.g. `claude-code/2.1.89 (cli)`),
    /// when the client sent one. Some clients (e.g. Codex) send none.
    pub client_user_agent: Option<String>,
    /// Extra headers to forward to upstream Longbridge services.
    pub extra_headers: Vec<(String, String)>,
}

/// Server-side beacon endpoint. Quote operations flow over the WebSocket quote
/// channel and never reach the HTTP access log; a request to this fake path lets
/// the server record (and count) that a WS-backed quote tool ran. The path only
/// needs to exist server-side to be logged.
pub(crate) const QUOTE_CMD_PATH: &str = "/v1/quote/cmd";

/// Send the tracking beacon over `client`. The (empty) body and any transport
/// error are ignored — the server only needs the access-log entry. Extracted as
/// its own awaitable function so the integration test can drive it against a
/// local server deterministically.
pub(crate) async fn send_quote_cmd(client: &longbridge::httpclient::HttpClient) {
    let _ = client
        .request(reqwest::Method::GET, QUOTE_CMD_PATH)
        .response::<String>()
        .send()
        .await;
}

/// Serializes tests that mutate the process-global `LONGBRIDGE_HTTP_URL` env var
/// to redirect the SDK base URL at a local capture server. Multiple such tests
/// run concurrently in one binary and would otherwise clobber each other's URL.
///
/// `tokio::sync::Mutex` is used so the guard can be held across `.await` points
/// without blocking the executor thread (needed by authenticate.rs's test, which
/// must keep the env var set for the duration of an async call).
#[cfg(test)]
pub(crate) static HTTP_URL_ENV_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

impl McpContext {
    /// This server's own identity as an RFC 9110 product token.
    const SELF_USER_AGENT: &'static str = concat!("longbridge-mcp/", env!("CARGO_PKG_VERSION"));

    /// Builds the upstream `User-Agent` as an RFC 9110 product-token chain: the
    /// originating MCP client's UA (when present) followed by this server's own
    /// token, e.g. `claude-code/2.1.89 (cli) longbridge-mcp/0.4.6`. Acting as a
    /// proxy, we append our token rather than inventing a custom header, so the
    /// backend sees both the client and this server in a single standard field.
    /// Falls back to just our token when the client sent no UA.
    fn user_agent(&self) -> String {
        match self.client_user_agent.as_deref() {
            Some(ua) if !ua.trim().is_empty() => format!("{ua} {}", Self::SELF_USER_AGENT),
            _ => Self::SELF_USER_AGENT.to_string(),
        }
    }

    pub fn create_config(&self) -> Arc<longbridge::Config> {
        let mut config =
            longbridge::Config::from_oauth(longbridge::oauth::OAuth::from_token(&self.token))
                .dont_print_quote_packages()
                .enable_overnight()
                // Identify MCP-originated requests on the Context path (REST and
                // WebSocket upgrades), mirroring how longbridge-cli tags itself.
                .header("user-agent", self.user_agent());
        if let Some(ref lang) = self.language {
            let lb_lang = if lang.contains("zh-CN") || lang.contains("zh-Hans") {
                longbridge::Language::ZH_CN
            } else if lang.contains("zh") {
                longbridge::Language::ZH_HK
            } else {
                longbridge::Language::EN
            };
            config = config.language(lb_lang);
        }
        // Tag the originating tool so server-side stats can attribute Context
        // (REST + WS upgrade) requests per tool, mirroring the CLI's x-cli-cmd.
        if let Ok(tool) = CURRENT_TOOL.try_with(|t| *t) {
            config = config.header("x-mcp-tool", tool);
        }
        Arc::new(config)
    }

    pub fn create_http_client(&self) -> longbridge::httpclient::HttpClient {
        let mut client = longbridge::httpclient::HttpClient::new(
            longbridge::httpclient::HttpClientConfig::from_oauth(
                longbridge::oauth::OAuth::from_token(&self.token),
            ),
        );
        // NOTE: This is very important for passing headers to upstream Longbridge services.
        // Do not remove this unless you have a good reason and know exactly which headers to forward instead.
        for (key, value) in &self.extra_headers {
            client = client.header(key.as_str(), value.as_str());
        }
        // Tag the originating tool so server-side stats can attribute requests
        // per tool, mirroring the CLI's x-cli-cmd.
        if let Ok(tool) = CURRENT_TOOL.try_with(|t| *t) {
            client = client.header("x-mcp-tool", tool);
        }
        // Identify MCP-originated upstream requests. The client's UA is appended
        // with our token by `user_agent`; set last so a forwarded header cannot
        // shadow it.
        client.header("user-agent", self.user_agent())
    }

    /// Fire a best-effort `GET /v1/quote/cmd` so the server records a log entry
    /// for this WS-backed quote operation. Quote traffic flows over the
    /// WebSocket quote channel and is therefore invisible to HTTP access logs;
    /// this ping makes the call countable server-side. It reuses an `HttpClient`
    /// from [`create_http_client`], which already carries the `user-agent`, OAuth
    /// token, forwarded headers, and the `x-mcp-tool` tool tag (see
    /// [`CURRENT_TOOL`]). Fire-and-forget: spawned on the runtime with its result
    /// and errors ignored, never blocking the call. The client is built
    /// synchronously here (reading the task-local before spawning), so the spawn
    /// not inheriting task-locals is fine.
    fn track_quote_cmd(&self) {
        let client = self.create_http_client();
        tokio::spawn(async move { send_quote_cmd(&client).await });
    }

    /// Return the cached `QuoteContext` for this token, creating one on first
    /// use. Also fires the WS beacon so the quote operation appears in
    /// server-side access logs.
    pub async fn get_quote_context(&self) -> longbridge::quote::QuoteContext {
        self.track_quote_cmd();
        // Pass a lazy closure: create_config() is only called on a cache miss.
        // Cache hits avoid the Arc<Config> allocation entirely.
        crate::ws_pool::get_or_init_quote(&self.token, || self.create_config()).await
    }

    /// Evict the cached `QuoteContext` for this token. Call this after any
    /// Longbridge error on a quote API so the next request creates a fresh
    /// WebSocket connection rather than reusing a broken one.
    pub fn evict_quote_context(&self) {
        crate::ws_pool::evict(&self.token);
    }

    /// Extracts `account_channel` from the JWT bearer token's `sub` claim.
    /// Falls back to `"lb"` when the token cannot be decoded.
    pub fn account_channel(&self) -> String {
        decode_jwt_account_channel(&self.token).unwrap_or_else(|| "lb".to_string())
    }
}

/// Decodes the JWT payload (no signature verification) and extracts `account_channel`
/// from the `sub` claim, which Longbridge encodes as a nested JSON string.
fn decode_jwt_account_channel(token: &str) -> Option<String> {
    let payload_b64 = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload_b64)?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let sub_str = claims["sub"].as_str()?;
    let sub: serde_json::Value = serde_json::from_str(sub_str).ok()?;
    sub["account_channel"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Minimal base64url decoder (no padding required, no external crate).
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut table = [0xffu8; 256];
    for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[c as usize] = i as u8;
    }
    // base64url uses - and _ instead of + and /
    table[b'-' as usize] = 62;
    table[b'_' as usize] = 63;

    let input: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut i = 0;
    while i < input.len() {
        let get = |pos: usize| -> Option<u8> {
            input.get(pos).and_then(|&b| {
                let v = table[b as usize];
                if v == 0xff { None } else { Some(v) }
            })
        };
        let b0 = get(i)?;
        let b1 = get(i + 1)?;
        out.push((b0 << 2) | (b1 >> 4));
        if let Some(b2) = get(i + 2) {
            out.push((b1 << 4) | (b2 >> 2));
            if let Some(b3) = get(i + 3) {
                out.push((b2 << 6) | b3);
            }
        }
        i += 4;
    }
    Some(out)
}

/// Headers that must not be forwarded to upstream Longbridge services.
/// These are either hop-by-hop headers or MCP/HTTP-level headers that only
/// make sense for the client↔MCP-server leg, not the MCP-server↔upstream leg.
const SKIP_FORWARD_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "te",
    "trailer",
    "upgrade",
    "keep-alive",
    "proxy-authorization",
    "proxy-authenticate",
    "content-type",
    "accept",
    "accept-encoding",
    "mcp-session-id",
    "authorization",
    // Captured separately in `extract_context` and folded into the synthesized
    // upstream User-Agent; never forwarded raw.
    "user-agent",
];

fn collect_headers(headers: &axum::http::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let key = name.as_str().to_lowercase();
            if SKIP_FORWARD_HEADERS.contains(&key.as_str()) {
                return None;
            }
            Some((key, value.to_str().ok()?.to_string()))
        })
        .collect()
}

/// Whether the current request carries Longbridge credentials.
///
/// True when the auth middleware attached a `BearerToken` (i.e. the client sent
/// an `Authorization: Bearer` header). Used to (a) gate the tool list shown to
/// unauthenticated sessions and (b) let `authenticate` report when a session is
/// already authenticated.
fn is_authenticated(ctx: &RequestContext<RoleServer>) -> bool {
    ctx.extensions
        .get::<axum::http::request::Parts>()
        .map(|parts| parts.extensions.get::<BearerToken>().is_some())
        .unwrap_or(false)
}

/// Whether the request arrived on the optional-auth `/agent` endpoint.
///
/// The auth middleware inserts [`AgentEndpoint`] only for token-less requests
/// on `/agent`. On the main endpoint a token-less request is rejected with 401
/// before reaching a handler, so this is never true there. Used to decide
/// whether to surface the `authenticate` tool to an unauthenticated session.
fn is_agent_endpoint(ctx: &RequestContext<RoleServer>) -> bool {
    ctx.extensions
        .get::<axum::http::request::Parts>()
        .map(|parts| parts.extensions.get::<AgentEndpoint>().is_some())
        .unwrap_or(false)
}

/// The restricted public endpoint version this request arrived on, if any.
///
/// The auth middleware inserts [`RestrictedEndpoint`] (carrying a
/// [`RestrictedVersion`]) for every request that proceeds on `/v1` or `/v2`.
/// When `Some`, `list_tools` exposes and `call_tool` accepts only that
/// version's allowlist.
fn restricted_version(ctx: &RequestContext<RoleServer>) -> Option<RestrictedVersion> {
    ctx.extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<RestrictedEndpoint>())
        .map(|marker| marker.0)
}

/// Whether `name` is on the active restricted endpoint's allowlist.
fn is_restricted_tool_allowed(version: RestrictedVersion, name: &str) -> bool {
    match version {
        RestrictedVersion::V1 => is_v1_public_tool(name),
        RestrictedVersion::V2 => is_v2_public_tool(name),
    }
}

fn extract_context(ctx: &RequestContext<RoleServer>) -> Result<McpContext, McpError> {
    let parts = ctx
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::internal_error("missing request parts", None))?;
    let token = parts.extensions.get::<BearerToken>().ok_or_else(|| {
        McpError::invalid_request(
            format!(
                "Not authenticated. Provide credentials by calling the `authenticate` tool \
                     with a one-time authorization code generated at {}, or connect with an \
                     `Authorization: Bearer <token>` header obtained via the OAuth flow.",
                crate::tools::authenticate::connect_page_url()
            ),
            None,
        )
    })?;
    let language = parts
        .headers
        .get("accept-language")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let client_user_agent = parts
        .headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    Ok(McpContext {
        token: token.0.clone(),
        language,
        client_user_agent,
        // NOTE: This is very important for passing headers to upstream Longbridge services.
        // Do not remove this unless you have a good reason and know exactly which headers to forward instead.
        extra_headers: collect_headers(&parts.headers),
    })
}

/// Returns all registered MCP tools with full schema metadata, sorted by name.
///
/// This is used for documentation resources where verbose field descriptions
/// are useful and do not need to live in the hot `tools/list` descriptor.
fn all_tools_full_cached() -> &'static [rmcp::model::Tool] {
    static TOOLS: std::sync::OnceLock<Vec<rmcp::model::Tool>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(|| {
        cached_router()
            .list_all()
            .into_iter()
            .map(|mut tool| {
                let mut schema = serde_json::Value::Object((*tool.input_schema).clone());
                strip_null_from_type_arrays(&mut schema);
                if let serde_json::Value::Object(obj) = schema {
                    tool.input_schema = std::sync::Arc::new(obj);
                }
                tool
            })
            .collect()
    })
}

/// Returns all registered MCP tools sorted by name.
///
/// Returns all tools, processed once and cached for the lifetime of the process.
///
/// Building the router (151 entries) and recursively traversing every JSON Schema
/// is expensive; doing it on each `tools/list` request was the primary CPU hotspot.
/// The result is immutable after startup, so a `OnceLock` is safe.
fn all_tools_cached() -> &'static [rmcp::model::Tool] {
    static TOOLS: std::sync::OnceLock<Vec<rmcp::model::Tool>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(|| {
        all_tools_full_cached()
            .iter()
            .cloned()
            .map(|mut tool| {
                compact_output_schema_for_tool_list(&mut tool);
                compact_tool_description_for_tool_list(&mut tool);
                tool
            })
            .collect()
    })
}

/// Input schemas are post-processed to remove `null` from `type` arrays so that
/// optional parameters are represented as plain scalar types (e.g. `"type": "string"`
/// instead of `"type": ["string", "null"]`).  Optionality is already expressed by
/// the field being absent from the `required` array, which is the MCP convention.
pub fn list_tools() -> Vec<rmcp::model::Tool> {
    all_tools_cached().to_vec()
}

/// Tools for the main endpoint (`/mcp`, root): full set minus `authenticate`.
/// Computed once; every `tools/list` response clones from this slice.
fn tools_main_endpoint() -> &'static [rmcp::model::Tool] {
    static MAIN: std::sync::OnceLock<Vec<rmcp::model::Tool>> = std::sync::OnceLock::new();
    MAIN.get_or_init(|| {
        all_tools_cached()
            .iter()
            .filter(|t| t.name != AUTHENTICATE_TOOL_NAME)
            .cloned()
            .collect()
    })
}

/// Tools for the unauthenticated `/agent` endpoint: only `authenticate`.
/// Computed once; every `tools/list` response clones from this one-element slice.
fn tools_agent_endpoint() -> &'static [rmcp::model::Tool] {
    static AGENT: std::sync::OnceLock<Vec<rmcp::model::Tool>> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        all_tools_cached()
            .iter()
            .filter(|t| t.name == AUTHENTICATE_TOOL_NAME)
            .cloned()
            .collect()
    })
}

/// Endpoint version bit flags used by [`TOOL_ENDPOINTS`].
const V1: u8 = 1 << 0;
const V2: u8 = 1 << 1;

/// Single source of truth for restricted-endpoint membership of every
/// registered tool — the per-tool version annotation.
///
/// `/v1` and `/v2` are **whitelists**: a tool is exposed only when its flags
/// include the matching bit. This is deliberately **default-out** — a tool
/// absent from this table (or marked `0`) appears on neither restricted
/// endpoint, so a newly added tool can never silently leak onto a
/// directory-submitted surface.
///
/// - `/v1` — read-only market-analysis surface (OpenAI/Claude/Grok submission).
/// - `/v2` — broader surface that additionally exposes read-only
///   account/portfolio, read-only order/execution history, IPO market data,
///   watchlist, alerts, sharelist, and community tools. `/v2` still **never**
///   exposes trade execution (`submit_order`/`cancel_order`/`replace_order`),
///   DCA automation, IPO order management (`ipo_orders`/`ipo_order_detail`/
///   `ipo_profit_loss`), or money movement (`deposits`/`withdrawals`/
///   `bank_cards`).
///
/// Every `/v1` tool is also valid on `/v2`, so v1 entries carry `V1 | V2`.
///
/// Invariants enforced by unit tests: every name is live, every live tool is
/// listed exactly once, `V1 ⊆ V2`, and the v2 set is disjoint from the
/// trade-write / DCA / IPO-order / money-movement tools.
const TOOL_ENDPOINTS: &[(&str, u8)] = &[
    // v1 + v2 — read-only market / fundamental / research analysis.
    ("business_segments", V1 | V2),
    ("calc_indexes", V1 | V2),
    ("candlesticks", V1 | V2),
    ("company", V1 | V2),
    ("consensus", V1 | V2),
    ("depth", V1 | V2),
    ("dividend", V1 | V2),
    ("filings", V1 | V2),
    ("finance_calendar", V1 | V2),
    ("financial_report_latest", V1 | V2),
    ("financial_statement", V1 | V2),
    ("forecast_eps", V1 | V2),
    ("institution_rating", V1 | V2),
    ("intraday", V1 | V2),
    ("market_status", V1 | V2),
    ("news", V1 | V2),
    ("news_search", V1 | V2),
    ("quote", V1 | V2),
    ("rank_list", V1 | V2),
    ("screener_search", V1 | V2),
    ("shareholder_top", V1 | V2),
    ("short_positions", V1 | V2),
    ("top_movers", V1 | V2),
    ("trades", V1 | V2),
    ("valuation", V1 | V2),
    ("watchlist", V1 | V2),
    // v2 only — broader read surface plus non-trade write tools (watchlist,
    // alert, sharelist, community).
    ("account_balance", V2),
    ("ah_premium", V2),
    ("ah_premium_intraday", V2),
    ("alert_add", V2),
    ("alert_delete", V2),
    ("alert_disable", V2),
    ("alert_enable", V2),
    ("alert_list", V2),
    ("anomaly", V2),
    ("broker_holding", V2),
    ("broker_holding_daily", V2),
    ("broker_holding_detail", V2),
    ("brokers", V2),
    ("business_segments_history", V2),
    ("capital_distribution", V2),
    ("capital_flow", V2),
    ("cash_flow", V2),
    ("constituent", V2),
    ("corp_action", V2),
    ("create_watchlist_group", V2),
    ("delete_watchlist_group", V2),
    ("dividend_detail", V2),
    ("estimate_max_purchase_quantity", V2),
    ("exchange_rate", V2),
    ("executive", V2),
    ("financial_report", V2),
    ("financial_report_snapshot", V2),
    ("fund_holder", V2),
    ("fund_positions", V2),
    ("history_candlesticks_by_date", V2),
    ("history_candlesticks_by_offset", V2),
    ("history_executions", V2),
    ("history_market_temperature", V2),
    ("history_orders", V2),
    ("industry_peers", V2),
    ("industry_rank", V2),
    ("industry_valuation", V2),
    ("industry_valuation_dist", V2),
    ("institution_rating_detail", V2),
    ("institution_rating_history", V2),
    ("institution_rating_industry_rank", V2),
    ("institutional_views", V2),
    ("invest_relation", V2),
    ("ipo_calendar", V2),
    ("ipo_detail", V2),
    ("ipo_listed", V2),
    ("ipo_subscriptions", V2),
    ("margin_ratio", V2),
    ("market_temperature", V2),
    ("now", V2),
    ("operating", V2),
    ("option_chain_expiry_date_list", V2),
    ("option_chain_info_by_date", V2),
    ("option_quote", V2),
    ("option_volume", V2),
    ("option_volume_daily", V2),
    ("order_detail", V2),
    ("participants", V2),
    ("profit_analysis", V2),
    ("profit_analysis_detail", V2),
    ("quant_run", V2),
    ("rank_categories", V2),
    ("screener_indicators", V2),
    ("screener_recommend_strategies", V2),
    ("screener_strategy", V2),
    ("screener_user_strategies", V2),
    ("security_list", V2),
    ("shareholder", V2),
    ("shareholder_detail", V2),
    ("sharelist_add", V2),
    ("sharelist_create", V2),
    ("sharelist_delete", V2),
    ("sharelist_detail", V2),
    ("sharelist_list", V2),
    ("sharelist_popular", V2),
    ("sharelist_remove", V2),
    ("sharelist_sort", V2),
    ("short_margin", V2),
    ("short_trades", V2),
    ("statement_export", V2),
    ("statement_list", V2),
    ("static_info", V2),
    ("stock_positions", V2),
    ("today_executions", V2),
    ("today_orders", V2),
    ("topic", V2),
    ("topic_create", V2),
    ("topic_create_reply", V2),
    ("topic_detail", V2),
    ("topic_replies", V2),
    ("topic_search", V2),
    ("trade_stats", V2),
    ("trading_days", V2),
    ("trading_session", V2),
    ("update_watchlist_group", V2),
    ("valuation_comparison", V2),
    ("valuation_history", V2),
    ("valuation_rank", V2),
    ("warrant_issuers", V2),
    ("warrant_list", V2),
    ("warrant_quote", V2),
    // Excluded from every restricted endpoint — DCA automation, order-write,
    // IPO order management, money movement / PCI.
    ("bank_cards", 0),
    ("cancel_order", 0),
    ("dca_check", 0),
    ("dca_create", 0),
    ("dca_history", 0),
    ("dca_list", 0),
    ("dca_pause", 0),
    ("dca_resume", 0),
    ("dca_stats", 0),
    ("dca_stop", 0),
    ("dca_update", 0),
    ("deposits", 0),
    ("ipo_order_detail", 0),
    ("ipo_orders", 0),
    ("ipo_profit_loss", 0),
    ("replace_order", 0),
    ("submit_order", 0),
    ("withdrawals", 0),
    // Reverse-auth tool — only surfaced on the unauthenticated `/agent`
    // endpoint, never on a restricted endpoint.
    ("authenticate", 0),
];

/// Endpoint membership flags for `name`, or `0` when the tool is on no
/// restricted endpoint (or is unknown).
fn endpoint_flags(name: &str) -> u8 {
    TOOL_ENDPOINTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, f)| *f)
        .unwrap_or(0)
}

/// Whether `name` is exposed on the restricted public `/v1` endpoint.
fn is_v1_public_tool(name: &str) -> bool {
    endpoint_flags(name) & V1 != 0
}

/// Whether `name` is exposed on the restricted public `/v2` endpoint.
fn is_v2_public_tool(name: &str) -> bool {
    endpoint_flags(name) & V2 != 0
}

/// Tool names exposed on the `/v1` endpoint. Used by the `/v1/tools.json`
/// manifest handler to prune the manifest to the allowlist.
pub fn v1_tool_names() -> Vec<&'static str> {
    TOOL_ENDPOINTS
        .iter()
        .filter(|(_, f)| f & V1 != 0)
        .map(|(n, _)| *n)
        .collect()
}

/// Tool names exposed on the `/v2` endpoint. Used by the `/v2/tools.json`
/// manifest handler to prune the manifest to the allowlist.
pub fn v2_tool_names() -> Vec<&'static str> {
    TOOL_ENDPOINTS
        .iter()
        .filter(|(_, f)| f & V2 != 0)
        .map(|(n, _)| *n)
        .collect()
}

/// Tools for the restricted public `/v1` endpoint. Computed once; every
/// `tools/list` response clones from this slice.
fn tools_v1_endpoint() -> &'static [rmcp::model::Tool] {
    static V1_TOOLS: std::sync::OnceLock<Vec<rmcp::model::Tool>> = std::sync::OnceLock::new();
    V1_TOOLS.get_or_init(|| {
        all_tools_cached()
            .iter()
            .filter(|t| is_v1_public_tool(t.name.as_ref()))
            .cloned()
            .collect()
    })
}

/// Tools for the restricted public `/v2` endpoint. Computed once; every
/// `tools/list` response clones from this slice.
fn tools_v2_endpoint() -> &'static [rmcp::model::Tool] {
    static V2_TOOLS: std::sync::OnceLock<Vec<rmcp::model::Tool>> = std::sync::OnceLock::new();
    V2_TOOLS.get_or_init(|| {
        all_tools_cached()
            .iter()
            .filter(|t| is_v2_public_tool(t.name.as_ref()))
            .cloned()
            .collect()
    })
}

/// Tool descriptors exposed on the restricted public `/v1` endpoint.
/// Used by the `/v1/tools.json` manifest handler.
pub fn v1_list_tools() -> Vec<rmcp::model::Tool> {
    tools_v1_endpoint().to_vec()
}

/// Tool descriptors exposed on the restricted public `/v2` endpoint.
/// Used by the `/v2/tools.json` manifest handler.
pub fn v2_list_tools() -> Vec<rmcp::model::Tool> {
    tools_v2_endpoint().to_vec()
}

/// Returns the tool router, built once and cached for the lifetime of the process.
///
/// Shared by `ServerHandler::call_tool` (dispatch) and `ServerHandler::get_tool`
/// (task-support validation, called by rmcp on every `CallToolRequest`).
fn cached_router() -> &'static rmcp::handler::server::router::tool::ToolRouter<Longbridge> {
    use rmcp::handler::server::router::tool::ToolRouter;
    static ROUTER: std::sync::OnceLock<ToolRouter<Longbridge>> = std::sync::OnceLock::new();
    ROUTER.get_or_init(Longbridge::tool_router)
}

/// Recursively remove `"null"` from JSON Schema `type` arrays.
/// When the array is left with a single element it is unwrapped to a plain string.
fn strip_null_from_type_arrays(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::Array(types)) = map.get_mut("type") {
                let filtered: Vec<serde_json::Value> = types
                    .iter()
                    .filter(|t| t.as_str() != Some("null"))
                    .cloned()
                    .collect();
                if filtered.len() == 1 {
                    *map.get_mut("type").unwrap() = filtered.into_iter().next().unwrap();
                } else if filtered.len() < types.len() {
                    *types = filtered;
                }
            }
            for v in map.values_mut() {
                strip_null_from_type_arrays(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_null_from_type_arrays(v);
            }
        }
        _ => {}
    }
}

/// Recursively remove documentation-only JSON Schema keys from tool descriptors.
/// Validation keywords stay in `tools/list`; verbose descriptions remain
/// available through `lb://tools/{tool}/output-schema` resources.
fn strip_schema_documentation_keys(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("$schema");
            map.remove("title");
            map.remove("description");
            for v in map.values_mut() {
                strip_schema_documentation_keys(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_schema_documentation_keys(v);
            }
        }
        _ => {}
    }
}

fn compact_output_schema_for_tool_list(tool: &mut rmcp::model::Tool) {
    let Some(schema) = tool.output_schema.as_ref() else {
        return;
    };
    let mut schema = serde_json::Value::Object(schema.as_ref().clone());
    strip_schema_documentation_keys(&mut schema);
    if let serde_json::Value::Object(obj) = schema {
        tool.output_schema = Some(std::sync::Arc::new(obj));
    }
}

fn compact_tool_description_for_tool_list(tool: &mut rmcp::model::Tool) {
    if tool.output_schema.is_none() {
        return;
    }
    let Some(description) = tool.description.as_ref() else {
        return;
    };

    let compacted = compact_typed_output_tool_description(description.as_ref());
    if compacted != description.as_ref() {
        tool.description = Some(Cow::Owned(compacted));
    }
}

fn compact_typed_output_tool_description(description: &str) -> String {
    let without_return_shapes = strip_output_shape_sentences(description);
    trim_description_to_char_limit(&without_return_shapes, TOOL_DESCRIPTION_MAX_CHARS)
}

fn strip_output_shape_sentences(description: &str) -> String {
    let mut kept = Vec::new();
    for sentence in description_sentences(description) {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }
        let lower = sentence.to_ascii_lowercase();
        if lower.starts_with("returns ")
            || lower.starts_with("return ")
            || lower.starts_with("unified data[]")
            || lower.starts_with("us-only:")
            || lower.starts_with("hk-only:")
        {
            continue;
        }
        kept.push(sentence.trim_end_matches('.'));
    }

    if kept.is_empty() {
        description.trim().to_string()
    } else {
        format!("{}.", kept.join(". "))
    }
}

fn trim_description_to_char_limit(description: &str, max_chars: usize) -> String {
    let trimmed = description.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let mut result = String::new();
    for sentence in description_sentences(trimmed) {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }
        let candidate = if result.is_empty() {
            format!("{}.", sentence.trim_end_matches('.'))
        } else {
            format!("{} {}.", result, sentence.trim_end_matches('.'))
        };
        if candidate.chars().count() > max_chars {
            break;
        }
        result = candidate;
    }
    if !result.is_empty() {
        return result;
    }

    let mut clipped: String = trimmed.chars().take(max_chars.saturating_sub(3)).collect();
    clipped = clipped
        .trim_end_matches(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == ':')
        .to_string();
    format!("{clipped}...")
}

fn description_sentences(description: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;
    for (idx, _) in description.match_indices(". ") {
        let prefix = &description[..idx];
        let token = prefix.split_whitespace().last().unwrap_or_default();
        if matches!(token, "e.g" | "i.e") {
            continue;
        }
        sentences.push(&description[start..idx]);
        start = idx + 2;
    }
    let tail = description[start..].trim_end_matches('.');
    if !tail.trim().is_empty() {
        sentences.push(tail);
    }
    sentences
}

fn output_schema_resource_uri(tool_name: &str) -> String {
    format!("{OUTPUT_SCHEMA_RESOURCE_PREFIX}{tool_name}{OUTPUT_SCHEMA_RESOURCE_SUFFIX}")
}

fn output_schema_tool_name(uri: &str) -> Option<&str> {
    let tool_name = uri
        .strip_prefix(OUTPUT_SCHEMA_RESOURCE_PREFIX)?
        .strip_suffix(OUTPUT_SCHEMA_RESOURCE_SUFFIX)?;
    (!tool_name.is_empty() && !tool_name.contains('/')).then_some(tool_name)
}

fn output_schema_resources() -> Vec<Resource> {
    all_tools_full_cached()
        .iter()
        .filter_map(|tool| {
            let schema = tool.output_schema.as_ref()?;
            let uri = output_schema_resource_uri(&tool.name);
            let size = serde_json::to_vec(schema.as_ref())
                .ok()
                .and_then(|bytes| u32::try_from(bytes.len()).ok());
            let title = tool.title.clone().unwrap_or_else(|| tool.name.to_string());
            let mut raw = RawResource::new(uri, format!("{}.output_schema", tool.name))
                .with_title(format!("{title} Output Schema"))
                .with_description(format!(
                    "Full JSON Schema output contract for the `{}` tool.",
                    tool.name
                ))
                .with_mime_type(OUTPUT_SCHEMA_RESOURCE_MIME);
            if let Some(size) = size {
                raw = raw.with_size(size);
            }
            Some(rmcp::model::Annotated::new(raw, None))
        })
        .collect()
}

fn read_output_schema_resource(uri: &str) -> Result<ReadResourceResult, McpError> {
    let Some(tool_name) = output_schema_tool_name(uri) else {
        return Err(McpErrorData::resource_not_found(
            "unknown Longbridge resource URI",
            Some(serde_json::json!({ "uri": uri })),
        ));
    };
    let Some(schema) = all_tools_full_cached()
        .iter()
        .find(|tool| tool.name == tool_name)
        .and_then(|tool| tool.output_schema.as_ref())
    else {
        return Err(McpErrorData::resource_not_found(
            "unknown Longbridge output schema resource",
            Some(serde_json::json!({ "uri": uri })),
        ));
    };
    let text = serde_json::to_string_pretty(schema.as_ref()).map_err(|err| {
        McpErrorData::internal_error(
            "failed to serialize output schema resource",
            Some(serde_json::json!({ "uri": uri, "error": err.to_string() })),
        )
    })?;
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type(OUTPUT_SCHEMA_RESOURCE_MIME),
    ]))
}

use crate::tools::quote::{
    CalcIndexesParam, CandlesticksParam, CreateWatchlistGroupParam, DeleteWatchlistGroupParam,
    HistoryCandlesticksByDateParam, HistoryCandlesticksByOffsetParam, MarketDateRangeParam,
    MarketParam, OptionVolumeDailyParam, OptionVolumeParam, SecurityListParam, ShortPositionsParam,
    SymbolCountParam, SymbolDateParam, SymbolParam, SymbolsParam, UpdateWatchlistGroupParam,
    WarrantListParam,
};
use crate::tools::trade::{
    CashFlowParam, EstimateMaxQtyParam, HistoryOrdersParam, OrderDetailParam, OrderIdParam,
    ReplaceOrderParam, SubmitOrderParam,
};

#[tool_router(vis = "pub(crate)")]
impl Longbridge {
    /// Authenticate this MCP session using a one-time authorization code.
    #[tool(
        title = "Authenticate",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Authenticate when you have no Longbridge credentials yet (e.g. your client could not complete the browser OAuth flow). The user generates a one-time authorization code at https://open.longbridge.com/connect and pastes it to you; pass it as `auth_code`. On success the server returns an access token to use as the Bearer credential on subsequent requests, unlocking the full tool set. If you are not authenticated and the user has not provided a code, direct them to https://open.longbridge.com/connect to generate one."
    )]
    async fn authenticate(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<authenticate::AuthenticateParam>,
    ) -> Result<CallToolResult, McpError> {
        let already = is_authenticated(&ctx);
        measured_tool_call(AUTHENTICATE_TOOL_NAME, || {
            authenticate::authenticate(already, p)
        })
        .await
    }

    /// Get current UTC time in RFC3339 format.
    #[tool(
        title = "Current Time",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get current UTC time as an RFC3339 string (e.g. \"2025-01-15T08:30:00Z\"). Use to determine current date/time before making date-based queries."
    )]
    async fn now(&self) -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .expect("failed to format current time")
    }

    /// Get basic information of securities.
    #[tool(
        title = "Security Static Info",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get static info for securities. Returns per symbol: symbol, name_cn, name_en, exchange (e.g. NASDAQ), type (e.g. US_Stock), lot_size, listed_date, delisted (bool)."
    )]
    async fn static_info(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("static_info", || quote::static_info(&mctx, p)).await
    }

    /// Get the latest price quotes.
    #[tool(
        title = "Quote",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get latest price quotes. Returns per symbol: last_done, prev_close, open, high, low, volume, turnover, change_rate, change_value, trade_status, timestamp."
    )]
    async fn quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("quote", || quote::quote(&mctx, p)).await
    }

    /// Get option quotes.
    #[tool(
        title = "Option Quote",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get option quotes (max 500 symbols). Returns last_done, prev_close, open, high, low, volume, turnover, implied_volatility, delta, gamma, theta, vega, rho, open_interest per symbol."
    )]
    async fn option_quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_quote", || quote::option_quote(&mctx, p)).await
    }

    /// Get warrant quotes.
    #[tool(
        title = "Warrant Quote",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get warrant quotes. Returns last_done, prev_close, open, high, low, volume, turnover, implied_volatility, delta, leverage_ratio, effective_leverage per symbol."
    )]
    async fn warrant_quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("warrant_quote", || quote::warrant_quote(&mctx, p)).await
    }

    /// Get the order book depth.
    #[tool(
        title = "Order Book Depth",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::DepthResponse>(),
        description = "Get order book depth for a symbol. Returns {bids[]{position, price, volume, order_num}, asks[]{position, price, volume, order_num}}. Up to 10 price levels."
    )]
    async fn depth(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("depth", || quote::depth(&mctx, p)).await
    }

    /// Get broker queue data.
    #[tool(
        title = "Broker Queue",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::BrokersResponse>(),
        description = "Get broker queue (HK stocks only). Returns bid_brokers/ask_brokers arrays, each with position (1-based) and broker_ids. Map broker IDs to names via participants."
    )]
    async fn brokers(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("brokers", || quote::brokers(&mctx, p)).await
    }

    /// Get market participant broker information.
    #[tool(
        title = "Market Participants",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get HK market participant broker information. Returns participants[]{broker_ids[], name_en, name_cn, name_hk}. Use broker_ids to interpret broker queue data."
    )]
    async fn participants(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("participants", || quote::participants(&mctx)).await
    }

    /// Get recent trades.
    #[tool(
        title = "Recent Trades",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get recent trades (max 1000). Returns trades[]{price, volume, timestamp, trade_type, direction} for the symbol."
    )]
    async fn trades(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolCountParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("trades", || quote::trades(&mctx, p)).await
    }

    /// Get intraday line data.
    #[tool(
        title = "Intraday Line",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get intraday minute-by-minute price/volume data. trade_sessions: \"intraday\" (default, regular hours) or \"all\" (include pre-market and post-market)"
    )]
    async fn intraday(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<quote::IntradayParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("intraday", || quote::intraday(&mctx, p)).await
    }

    /// Get candlestick (K-line) data.
    #[tool(
        title = "Candlesticks",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get candlestick data (OHLCV). Only symbol is required; period defaults to day, count to 100 (max 1000), forward_adjust to false, trade_sessions to all. period: 1m/5m/15m/30m/60m/day/week/month/year. trade_sessions: intraday/all"
    )]
    async fn candlesticks(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CandlesticksParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("candlesticks", || quote::candlesticks(&mctx, p)).await
    }

    /// Get historical candlesticks by offset.
    #[tool(
        title = "Historical Candlesticks by Offset",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get historical candlestick data by offset from a reference time. period: 1m/5m/15m/30m/60m/day/week/month/year"
    )]
    async fn history_candlesticks_by_offset(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryCandlesticksByOffsetParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_candlesticks_by_offset", || {
            quote::history_candlesticks_by_offset(&mctx, p)
        })
        .await
    }

    /// Get historical candlesticks by date range.
    #[tool(
        title = "Historical Candlesticks by Date",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get historical candlestick data by date range. period: 1m/5m/15m/30m/60m/day/week/month/year"
    )]
    async fn history_candlesticks_by_date(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryCandlesticksByDateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_candlesticks_by_date", || {
            quote::history_candlesticks_by_date(&mctx, p)
        })
        .await
    }

    /// Get trading days between dates.
    #[tool(
        title = "Trading Days",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::TradingDaysResponse>(),
        description = "Get trading days for a market between dates. Returns trading_days[] and half_trading_days[] as \"yyyy-mm-dd\" strings. market: HK/US/CN/SG."
    )]
    async fn trading_days(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketDateRangeParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("trading_days", || quote::trading_days(&mctx, p)).await
    }

    /// Get option chain expiry date list.
    #[tool(
        title = "Option Expiry Dates",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get option chain expiry dates for a symbol (e.g. AAPL.US). Returns expiry_dates[] as \"yyyy-mm-dd\" strings. Use with option_chain_info_by_date to get strikes and Greeks."
    )]
    async fn option_chain_expiry_date_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_chain_expiry_date_list", || {
            quote::option_chain_expiry_date_list(&mctx, p)
        })
        .await
    }

    /// Get option chain info by expiry date.
    #[tool(
        title = "Option Chain by Date",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get option chain for an expiry date. Returns strikePrices[]{strike_price, call{symbol, last_done, iv, delta, gamma}, put{symbol, last_done, iv, delta, gamma}}."
    )]
    async fn option_chain_info_by_date(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolDateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_chain_info_by_date", || {
            quote::option_chain_info_by_date(&mctx, p)
        })
        .await
    }

    /// Get capital flow of a security.
    #[tool(
        title = "Capital Flow",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get capital inflow/outflow time series. Returns items[]{timestamp, inflow, outflow, net_flow} for the symbol (same-day data)."
    )]
    async fn capital_flow(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("capital_flow", || quote::capital_flow(&mctx, p)).await
    }

    /// Get capital distribution.
    #[tool(
        title = "Capital Distribution",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::CapitalDistributionResponse>(),
        description = "Get capital distribution for a symbol. Returns {timestamp, capital_in{large, medium, small}, capital_out{large, medium, small}} (decimal strings in settlement currency)."
    )]
    async fn capital_distribution(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("capital_distribution", || {
            quote::capital_distribution(&mctx, p)
        })
        .await
    }

    /// Get trading session schedule.
    #[tool(
        title = "Trading Sessions",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get trading session schedule for all markets. Returns market_sessions[]{market, trade_sessions[]{beg_time, end_time, trade_session_type}}."
    )]
    async fn trading_session(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("trading_session", || quote::trading_session(&mctx)).await
    }

    /// Get market temperature.
    #[tool(
        title = "Market Temperature",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::MarketTemperatureResponse>(),
        description = "Get current market sentiment temperature. Returns {temperature (0-100), description, valuation (0-100), sentiment (0-100), timestamp}. market: HK/US/CN/SG."
    )]
    async fn market_temperature(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("market_temperature", || quote::market_temperature(&mctx, p)).await
    }

    /// Get historical market temperature.
    #[tool(
        title = "Historical Market Temperature",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::HistoryMarketTemperatureResponse>(),
        description = "Get historical market temperature time series. Returns {type, list[]{temperature, description, valuation, sentiment, timestamp}} for the given market and date range."
    )]
    async fn history_market_temperature(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<MarketDateRangeParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_market_temperature", || {
            quote::history_market_temperature(&mctx, p)
        })
        .await
    }

    /// Get watchlist groups.
    #[tool(
        title = "Watchlist",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get all watchlist groups and their securities. Returns groups[]{id, name, securities[]{symbol, market, name, watched_price, watched_at}}."
    )]
    async fn watchlist(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("watchlist", || quote::watchlist(&mctx)).await
    }

    /// Get filings for a symbol.
    #[tool(
        title = "Filings",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get regulatory filings (8-K, 10-Q, 10-K, etc.). Returns items[]{id, title, type, language, filing_date, url} for the symbol."
    )]
    async fn filings(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("filings", || quote::filings(&mctx, p)).await
    }

    /// Get warrant issuers.
    #[tool(
        title = "Warrant Issuers",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get HK warrant issuer information. Returns issuers[]{id, name_en, name_cn}. Use id in warrant_list issuer filter."
    )]
    async fn warrant_issuers(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("warrant_issuers", || quote::warrant_issuers(&mctx)).await
    }

    /// Get warrant list for a symbol.
    #[tool(
        title = "Warrant List",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get filtered warrant list for an underlying symbol. Returns warrants[]{symbol, name, last_done, change_rate, implied_volatility, expiry_date, strike_price, leverage_ratio, outstanding_ratio}."
    )]
    async fn warrant_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<WarrantListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("warrant_list", || quote::warrant_list(&mctx, p)).await
    }

    /// Calculate indexes for symbols.
    #[tool(
        title = "Calc Indexes",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Calculate financial indexes for symbols. Pass symbols, and optionally indexes (e.g. [\"PeTtmRatio\",\"PbRatio\",\"LastDone\",\"TurnoverRate\"]). When indexes is omitted or empty, defaults to [\"LastDone\",\"ChangeValue\",\"ChangeRate\",\"Volume\",\"PeTtmRatio\",\"PbRatio\",\"DividendRatioTtm\",\"TurnoverRate\",\"TotalMarketValue\"]. Returns per-symbol index values."
    )]
    async fn calc_indexes(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CalcIndexesParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("calc_indexes", || quote::calc_indexes(&mctx, p)).await
    }

    /// Create a watchlist group.
    #[tool(
        title = "Create Watchlist Group",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::quote::CreateWatchlistGroupResponse>(),
        description = "Create a new watchlist group. Returns the created group {id, name}. Optionally pass securities (e.g. [\"AAPL.US\", \"700.HK\"]) to pre-populate."
    )]
    async fn create_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CreateWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("create_watchlist_group", || {
            quote::create_watchlist_group(&mctx, p)
        })
        .await
    }

    /// Delete a watchlist group.
    #[tool(
        title = "Delete Watchlist Group",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::quote::DeleteWatchlistGroupResponse>(),
        description = "Delete a watchlist group by id (numeric). Set purge=true to also remove its securities from all other groups. Returns upstream API response."
    )]
    async fn delete_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<DeleteWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("delete_watchlist_group", || {
            quote::delete_watchlist_group(&mctx, p)
        })
        .await
    }

    /// Update a watchlist group.
    #[tool(
        title = "Update Watchlist Group",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::quote::UpdateWatchlistGroupResponse>(),
        description = "Update a watchlist group by id. Can rename (name param) or modify securities (securities + mode: add/remove/replace). Returns upstream API response."
    )]
    async fn update_watchlist_group(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<UpdateWatchlistGroupParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("update_watchlist_group", || {
            quote::update_watchlist_group(&mctx, p)
        })
        .await
    }

    /// Get security list by market and category.
    #[tool(
        title = "Security List",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::quote::SecurityListResponse>(),
        description = "Get security list for a market. Supports market: US, HK, CN, SG. category: \"Overnight\" (default). page: 1-based page number (default 1). count: records per page (default 50). Returns {total, page, count, items[]{symbol, name_en, name_cn}}."
    )]
    async fn security_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SecurityListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("security_list", || quote::security_list(&mctx, p)).await
    }

    /// Get account balance.
    #[tool(
        title = "Account Balance",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get account cash balance and asset summary. Returns balances[]{currency, total_cash, max_finance_amount, remaining_finance_amount, risk_level, margin_call}. Filter by currency (e.g. \"USD\", \"HKD\")."
    )]
    async fn account_balance(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<trade::AccountBalanceParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("account_balance", || trade::account_balance(&mctx, p)).await
    }

    /// Get stock positions.
    #[tool(
        title = "Stock Positions",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::StockPositionsResponse>(),
        description = "Get current stock positions across all channels. Returns list[].stock_info[]{symbol, symbol_name, quantity, available_quantity, currency, cost_price, market}."
    )]
    async fn stock_positions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("stock_positions", || trade::stock_positions(&mctx)).await
    }

    /// Get fund positions.
    #[tool(
        title = "Fund Positions",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::FundPositionsResponse>(),
        description = "Get current fund positions. Returns list[].fund_info[]{symbol, symbol_name, currency, holding_units, current_net_asset_value, cost_net_asset_value, net_asset_value_day}."
    )]
    async fn fund_positions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("fund_positions", || trade::fund_positions(&mctx)).await
    }

    /// Get margin ratio.
    #[tool(
        title = "Margin Ratio",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::MarginRatioResponse>(),
        description = "Get margin ratio for a symbol. Returns {im_factor (initial margin), mm_factor (maintenance margin), fm_factor (forced liquidation)} as decimal strings."
    )]
    async fn margin_ratio(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("margin_ratio", || trade::margin_ratio(&mctx, p)).await
    }

    /// Get today's orders.
    #[tool(
        title = "Today's Orders",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get orders placed today. Returns orders[]{order_id, symbol, side, order_type, status, quantity, price, submitted_at, executed_quantity, executed_price, attached_orders[]}. Pass symbol to filter by symbol, or order_id to filter by a specific order. To query a take-profit/stop-loss sub-order by its own order_id, set both order_id and is_attached=true; the response returns that sub-order itself as an Order entry. is_attached has no effect without order_id."
    )]
    async fn today_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<trade::TodayOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("today_orders", || trade::today_orders(&mctx, p)).await
    }

    /// Get order detail.
    #[tool(
        title = "Order Detail",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::OrderDetailResponse>(),
        description = "Get detailed information about a specific order. For a parent order, returns full detail including attached_orders[] (take-profit/stop-loss legs). To query a sub-order by its own order_id instead, set is_attached=true; the response returns the sub-order itself, with charge_detail=null."
    )]
    async fn order_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OrderDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("order_detail", || trade::order_detail(&mctx, p)).await
    }

    /// Cancel an order.
    #[tool(
        title = "Cancel Order",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Cancel an open order by order_id. Returns plain text \"order cancelled\" on success; errors if the order is already filled or cancelled."
    )]
    async fn cancel_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("cancel_order", || trade::cancel_order(&mctx, p)).await
    }

    /// Get today's trade executions.
    #[tool(
        title = "Today's Executions",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get today's trade executions (fills). Returns executions[]{order_id, symbol, side, quantity, price, trade_done_at}. Pass symbol or order_id to filter."
    )]
    async fn today_executions(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<trade::TodayExecutionsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("today_executions", || trade::today_executions(&mctx, p)).await
    }

    /// Get historical orders (not including today).
    #[tool(
        title = "Historical Orders",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get historical orders between dates (excludes today). Returns orders[]{order_id, symbol, side, status, quantity, price, submitted_at}. start_at/end_at in RFC3339."
    )]
    async fn history_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_orders", || trade::history_orders(&mctx, p)).await
    }

    /// Get historical executions.
    #[tool(
        title = "Historical Executions",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get historical trade executions between dates. Returns executions[]{order_id, symbol, side, quantity, price, trade_done_at}. start_at/end_at in RFC3339."
    )]
    async fn history_executions(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_executions", || trade::history_executions(&mctx, p)).await
    }

    /// Get cash flow records.
    #[tool(
        title = "Cash Flow",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get cash flow records (deposits, withdrawals, dividends). Returns items[]{transaction_type, amount, currency, balance, created_at, remark}. start_at/end_at in RFC3339."
    )]
    async fn cash_flow(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CashFlowParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("cash_flow", || trade::cash_flow(&mctx, p)).await
    }

    /// Submit an order.
    #[tool(
        title = "Submit Order",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::OrderIdResponse>(),
        description = "Submit a buy/sell order. order_type: LO (Limit) / ELO (Enhanced Limit, HK) / MO (Market) / AO (At-auction, HK) / ALO (At-auction Limit, HK) / ODD (Odd Lots, HK) / LIT (Limit If Touched) / MIT (Market If Touched) / TSLPAMT (Trailing Limit by Amount) / TSLPPCT (Trailing Limit by Percent) / SLO (Special Limit, HK). side: Buy/Sell. time_in_force: Day/GTC/GTD. To attach a take-profit/stop-loss: set attached_order_type (ProfitTaker/StopLoss/Bracket) with profit_taker_price and/or stop_loss_price."
    )]
    async fn submit_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SubmitOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("submit_order", || trade::submit_order(&mctx, p)).await
    }

    /// Replace (modify) an order.
    #[tool(
        title = "Replace Order",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Modify an open order's quantity, price, trigger_price, or trailing params. Returns \"order replaced\" on success. Only open/pending orders can be modified. To modify attached take-profit/stop-loss: provide attached_order_type with updated prices. To cancel all attached orders: set cancel_all_attached=true. To update a specific attached leg by ID: use profit_taker_id or stop_loss_id."
    )]
    async fn replace_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ReplaceOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("replace_order", || trade::replace_order(&mctx, p)).await
    }

    /// Estimate max purchase quantity.
    #[tool(
        title = "Estimate Max Purchase Quantity",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::EstimateMaxQtyResponse>(),
        description = "Estimate maximum buy/sell quantity for a symbol. Returns {cash_max_qty, margin_max_qty} (decimal strings). Only symbol is required; side (case-insensitive Buy/Sell) defaults to Buy, order_type (case-insensitive) defaults to LO, and price is optional."
    )]
    async fn estimate_max_purchase_quantity(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<EstimateMaxQtyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("estimate_max_purchase_quantity", || {
            trade::estimate_max_purchase_quantity(&mctx, p)
        })
        .await
    }

    /// Get financial reports (income statement, balance sheet, cash flow).
    #[tool(
        title = "Financial Report",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get financial reports (income statement, balance sheet, cash flow). kind: IS/BS/CF/ALL. report_type: af (annual), saf (semi-annual), q1/q2/q3, qf (quarterly full)."
    )]
    async fn financial_report(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialReportParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_report", || {
            fundamental::financial_report(&mctx, p)
        })
        .await
    }

    /// Get institution rating summary (analyst consensus + target price).
    #[tool(
        title = "Institution Rating",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::InstitutionRatingResponse>(),
        description = "Get institution rating summary. Returns analyst{buy, outperform, hold, underperform, sell counts, target_price, consensus_rating} and instratings list."
    )]
    async fn institution_rating(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institution_rating", || {
            fundamental::institution_rating(&mctx, p)
        })
        .await
    }

    /// Get institution rating detail (historical ratings and target prices).
    #[tool(
        title = "Institution Rating Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::InstitutionRatingDetailResponse>(),
        description = "Get detailed historical institution ratings and target price history. Returns target.list[]{analyst, firm, rating, target_price, timestamp} per institution."
    )]
    async fn institution_rating_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institution_rating_detail", || {
            fundamental::institution_rating_detail(&mctx, p)
        })
        .await
    }

    /// Get dividend history.
    #[tool(
        title = "Dividend",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::DividendResponse>(),
        description = "Get dividend history. Returns items[]{ex_date, pay_date, record_date, dividend_type, amount, currency, status} for the symbol."
    )]
    async fn dividend(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dividend", || fundamental::dividend(&mctx, p)).await
    }

    /// Get dividend distribution details.
    #[tool(
        title = "Dividend Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::DividendDetailResponse>(),
        description = "Get detailed dividend distribution scheme. Returns details[]{period, cash_dividend, stock_dividend, record_date, ex_date, pay_date, currency}."
    )]
    async fn dividend_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dividend_detail", || fundamental::dividend_detail(&mctx, p)).await
    }

    /// Get EPS forecast data.
    #[tool(
        title = "Forecast EPS",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ForecastEpsResponse>(),
        description = "Get EPS forecast and analyst estimate history. Returns items[]{forecast_start_date, forecast_end_date, eps_estimate, eps_actual, surprise_pct, analyst_count}."
    )]
    async fn forecast_eps(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("forecast_eps", || fundamental::forecast_eps(&mctx, p)).await
    }

    /// Get financial consensus estimates.
    #[tool(
        title = "Analyst Consensus",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ConsensusResponse>(),
        description = "Get financial consensus estimates. Returns items[]{period, revenue_estimate, eps_estimate, net_income_estimate, analyst_count, last_updated} for upcoming periods."
    )]
    async fn consensus(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("consensus", || fundamental::consensus(&mctx, p)).await
    }

    /// Get valuation overview (PE, PB, PS, dividend yield).
    #[tool(
        title = "Valuation",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ValuationResponse>(),
        description = "Get valuation overview with peer comparison. Returns metrics.pe/pb/ps/dividend_yield{current, industry_avg, 5yr_avg, percentile} and peer comparison list."
    )]
    async fn valuation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("valuation", || fundamental::valuation(&mctx, p)).await
    }

    /// Get detailed valuation history.
    #[tool(
        title = "Valuation History",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ValuationHistoryResponse>(),
        description = "Get detailed valuation history time series. Returns history.metrics{pe/pb/ps/dividend_yield}[]{timestamp, value} for long-term percentile analysis."
    )]
    async fn valuation_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("valuation_history", || {
            fundamental::valuation_history(&mctx, p)
        })
        .await
    }

    /// Get industry valuation comparison.
    #[tool(
        title = "Industry Valuation",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::IndustryValuationResponse>(),
        description = "Get industry valuation comparison for peers. Returns list[]{symbol, name, pe, pb, ps, dividend_yield, history[]{date, pe, pb}} for peers in the same industry."
    )]
    async fn industry_valuation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("industry_valuation", || {
            fundamental::industry_valuation(&mctx, p)
        })
        .await
    }

    /// Get industry valuation distribution.
    #[tool(
        title = "Industry Valuation Distribution",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::IndustryValuationDistResponse>(),
        description = "Get industry PE/PB/PS valuation distribution. Returns distributions{pe/pb/ps}{min, p25, median, p75, max, current_percentile} to see where the stock sits in its sector."
    )]
    async fn industry_valuation_dist(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("industry_valuation_dist", || {
            fundamental::industry_valuation_dist(&mctx, p)
        })
        .await
    }

    /// Get company overview.
    #[tool(
        title = "Company Profile",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::CompanyResponse>(),
        description = "Get company overview. Returns name, description, employees, CEO, founded_year, website, exchange, industry, market_cap, and business profile summary."
    )]
    async fn company(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("company", || fundamental::company(&mctx, p)).await
    }

    /// Get company executives.
    #[tool(
        title = "Executive",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ExecutiveResponse>(),
        description = "Get company executive and board member information. Returns members[]{name, title, appointed_date, age, biography, compensation}."
    )]
    async fn executive(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("executive", || fundamental::executive(&mctx, p)).await
    }

    /// Get shareholders.
    #[tool(
        title = "Shareholders",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ShareholderResponse>(),
        description = "Get institutional shareholders for a symbol. Returns shareholders[]{institution, shares, ratio, change, change_type, reported_at}."
    )]
    async fn shareholder(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("shareholder", || fundamental::shareholder(&mctx, p)).await
    }

    /// Get fund holders.
    #[tool(
        title = "Fund Holders",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::FundHolderResponse>(),
        description = "Get funds and ETFs that hold a given symbol. Returns fund_holders[]{fund_name, fund_symbol, shares, ratio, change, reported_at}."
    )]
    async fn fund_holder(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("fund_holder", || fundamental::fund_holder(&mctx, p)).await
    }

    /// Get corporate actions.
    #[tool(
        title = "Corporate Actions",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::CorpActionResponse>(),
        description = "Get corporate actions (splits, buybacks, name changes). Returns items[]{action_type, effective_date, ratio, description} for the symbol."
    )]
    async fn corp_action(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("corp_action", || fundamental::corp_action(&mctx, p)).await
    }

    /// Get investor relations events.
    #[tool(
        title = "Investor Relations",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::InvestRelationResponse>(),
        description = "Get investor relations events and announcements. Returns items[]{title, event_type, event_date, url, description} for the symbol."
    )]
    async fn invest_relation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("invest_relation", || fundamental::invest_relation(&mctx, p)).await
    }

    /// Get operating metrics.
    #[tool(
        title = "Operating Performance",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::OperatingResponse>(),
        description = "Get company operating metrics (HK stocks only). Returns items[]{period, metric_name, value, unit} such as passenger traffic, cargo volumes, or store counts."
    )]
    async fn operating(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("operating", || fundamental::operating(&mctx, p)).await
    }

    /// Get market trading status.
    #[tool(
        title = "Market Status",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::MarketStatusResponse>(),
        description = "Get current market trading status for all markets. Returns market_time[]{market, trade_status (Trading/Closed/Mid-Day Break/Pre-Market/Post-Market/Overnight), timestamp}."
    )]
    async fn market_status(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("market_status", || market::market_status(&mctx)).await
    }

    /// Get broker holding data.
    #[tool(
        title = "Broker Holding",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::BrokerHoldingResponse>(),
        description = "Get top broker holding data for a symbol (HK stocks only; sourced from HKEX CCASS participant disclosure). Returns items[]{broker_name, holding_quantity, holding_change, holding_ratio} for the given period (rct_1/rct_5/rct_20/rct_60)."
    )]
    async fn broker_holding(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::BrokerHoldingParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("broker_holding", || market::broker_holding(&mctx, p)).await
    }

    /// Get broker holding detail.
    #[tool(
        title = "Broker Holding Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::BrokerHoldingDetailResponse>(),
        description = "Get full broker holding detail list for a symbol (HK stocks only; sourced from HKEX CCASS participant disclosure). Returns items[]{broker_id, broker_name, holding_quantity, holding_ratio, holding_change, date}."
    )]
    async fn broker_holding_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("broker_holding_detail", || {
            market::broker_holding_detail(&mctx, p)
        })
        .await
    }

    /// Get daily broker holding for a specific broker.
    #[tool(
        title = "Broker Holding (Daily)",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::BrokerHoldingDailyResponse>(),
        description = "Get daily holding history for a specific broker (by broker_id) in a symbol (HK stocks only; sourced from HKEX CCASS participant disclosure). Returns items[]{date, holding_quantity, holding_change, holding_ratio}."
    )]
    async fn broker_holding_daily(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::BrokerHoldingDailyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("broker_holding_daily", || {
            market::broker_holding_daily(&mctx, p)
        })
        .await
    }

    /// Get AH premium K-line data.
    #[tool(
        title = "A/H Premium",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get A/H share premium historical K-line data. Returns items[]{timestamp, open, high, low, close} representing the premium percentage over the given period."
    )]
    async fn ah_premium(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::AhPremiumParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ah_premium", || market::ah_premium(&mctx, p)).await
    }

    /// Get AH premium intraday data.
    #[tool(
        title = "A/H Premium (Intraday)",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get A/H share premium intraday time-share data. Returns items[]{timestamp, premium_rate} showing the intraday A/H premium percentage minute by minute."
    )]
    async fn ah_premium_intraday(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ah_premium_intraday", || {
            market::ah_premium_intraday(&mctx, p)
        })
        .await
    }

    /// Get trade statistics.
    #[tool(
        title = "Trade Statistics",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get trade statistics (buy/sell/neutral volume distribution). Returns items[]{price_range, buy_volume, sell_volume, neutral_volume} for price-volume profile."
    )]
    async fn trade_stats(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("trade_stats", || market::trade_stats(&mctx, p)).await
    }

    /// Get market anomalies.
    #[tool(
        title = "Market Anomaly",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::AnomalyResponse>(),
        description = "Get market anomaly alerts (unusual price/volume changes). market: HK/US/CN/SG. symbol: optional, filter to a specific stock. count: results per page (default 50, max 100). Returns changes[]{symbol, name, change_rate, volume, ...}, all_off."
    )]
    async fn anomaly(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::AnomalyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("anomaly", || market::anomaly(&mctx, p)).await
    }

    /// Get index constituents or ETF asset allocation.
    #[tool(
        title = "Index Constituents / ETF Asset Allocation",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get the constituents of an index or the asset allocation of an ETF. For an index (e.g. HSI.HK, .DJI.US) returns constituents[]{symbol, name, last_done, change_rate, market_cap, weight}. For an ETF (e.g. QQQ.US, 2800.HK) returns the asset allocation as info[] grouped by asset_type: 1=Holdings (top constituents with code, symbol, holding_detail), 2=Regional (country/region breakdown), 3=AssetClass (stock/bond/cash etc.), 4=Industry (sector breakdown). Each group has report_date and lists[]{name, position_ratio, name_locales}; Holdings groups additionally include code, symbol and holding_detail{industry_name, index_name, holding_type_name}."
    )]
    async fn constituent(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::IndexSymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("constituent", || market::constituent(&mctx, p)).await
    }

    /// Get finance calendar events.
    #[tool(
        title = "Financial Calendar",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::FinanceCalendarResponse>(),
        description = "Get finance calendar events by category and date range. category: report (earnings + financials) / dividend / split (splits & reverse splits) / ipo / macrodata (CPI, NFP, rate decisions) / closed (market holidays). market: HK/US/CN/SG/JP/UK/DE/AU (optional). Keep the date range to 2 weeks or less; for longer periods split into multiple calls to avoid truncation."
    )]
    async fn finance_calendar(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<calendar::FinanceCalendarParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("finance_calendar", || calendar::finance_calendar(&mctx, p)).await
    }

    /// Get exchange rates.
    #[tool(
        title = "Exchange Rate",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get exchange rates for all supported currencies. Returns list[]{from_currency, to_currency, rate, timestamp} covering USD, HKD, CNY, SGD and others."
    )]
    async fn exchange_rate(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("exchange_rate", || portfolio::exchange_rate(&mctx)).await
    }

    /// Get profit analysis summary.
    #[tool(
        title = "Profit Analysis",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get portfolio profit and loss analysis summary. start/end: optional date range in yyyy-mm-dd format. Both must be provided together — passing only one returns empty results."
    )]
    async fn profit_analysis(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<portfolio::ProfitAnalysisParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("profit_analysis", || portfolio::profit_analysis(&mctx, p)).await
    }

    /// Get profit analysis detail for a symbol.
    #[tool(
        title = "Profit Analysis Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get detailed profit and loss analysis for a specific symbol. start/end: optional date range in yyyy-mm-dd format. Both must be provided together — passing only one returns empty results."
    )]
    async fn profit_analysis_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<portfolio::ProfitAnalysisDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("profit_analysis_detail", || {
            portfolio::profit_analysis_detail(&mctx, p)
        })
        .await
    }

    /// Get price alert list.
    #[tool(
        title = "List Price Alerts",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::AlertListResponse>(),
        description = "Get all configured price alerts. Returns lists[]{counter_id, indicators[]{id, indicator_id, condition, price, frequency, enabled, triggered_at}}."
    )]
    async fn alert_list(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_list", || alert::alert_list(&mctx)).await
    }

    /// Add a price alert.
    #[tool(
        title = "Add Price Alert",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Add a price alert. condition: price_rise/price_fall (absolute price) or percent_rise/percent_fall (relative %). frequency: once/daily/every. Returns created alert object."
    )]
    async fn alert_add(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertAddParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_add", || alert::alert_add(&mctx, p)).await
    }

    /// Delete a price alert.
    #[tool(
        title = "Delete Price Alert",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Delete a price alert by alert_id (numeric string from alert_list). Returns upstream API response on success; errors if alert_id is invalid."
    )]
    async fn alert_delete(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_delete", || alert::alert_delete(&mctx, p)).await
    }

    /// Enable a price alert.
    #[tool(
        title = "Enable Price Alert",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::AlertToggleResponse>(),
        description = "Enable a price alert by alert_id. Returns {alert_id, enabled: true} on success. Use alert_list to find the numeric alert_id."
    )]
    async fn alert_enable(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_enable", || alert::alert_enable(&mctx, p)).await
    }

    /// Disable a price alert.
    #[tool(
        title = "Disable Price Alert",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::AlertToggleResponse>(),
        description = "Disable a price alert by alert_id. Returns {alert_id, enabled: false} on success. Use alert_list to find the numeric alert_id."
    )]
    async fn alert_disable(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<alert::AlertIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("alert_disable", || alert::alert_disable(&mctx, p)).await
    }

    /// Get news for a symbol.
    #[tool(
        title = "News",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get latest news articles for a symbol. Returns items[]{id, title, source, publish_time, summary, url, related_symbols[]}."
    )]
    async fn news(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("news", || content::news(&mctx, p)).await
    }

    /// Get discussion topics for a symbol.
    #[tool(
        title = "Topic List",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get discussion topics for a symbol. Returns items[]{id, title, author, created_at, like_count, comment_count, content_summary}."
    )]
    async fn topic(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic", || content::topic(&mctx, p)).await
    }

    /// Get topic detail.
    #[tool(
        title = "Topic Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::TopicDetailResponse>(),
        description = "Get discussion topic detail by topic_id. Returns {id, title, content, author, created_at, like_count, comment_count, symbols[]}."
    )]
    async fn topic_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_detail", || content::topic_detail(&mctx, p)).await
    }

    /// Get topic replies.
    #[tool(
        title = "Topic Replies",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get replies to a discussion topic, paginated (page default 1, size default 20, range 1-50)"
    )]
    async fn topic_replies(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicRepliesParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_replies", || content::topic_replies(&mctx, p)).await
    }

    /// Create a discussion topic.
    #[tool(
        title = "Create Topic",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::TopicCreateResponse>(),
        description = "Create a new discussion topic. topic_type=\"post\" (default) is plain text; \"article\" requires a non-empty title and accepts Markdown body."
    )]
    async fn topic_create(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicCreateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_create", || content::topic_create(&mctx, p)).await
    }

    /// Reply to a discussion topic.
    #[tool(
        title = "Create Topic Reply",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::TopicCreateReplyResponse>(),
        description = "Create a reply to a discussion topic. Pass reply_to_id to nest under another reply; omit for a top-level reply."
    )]
    async fn topic_create_reply(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::TopicCreateReplyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_create_reply", || {
            content::topic_create_reply(&mctx, p)
        })
        .await
    }

    /// List account statements.
    #[tool(
        title = "Statement List",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::account::StatementListResponse>(),
        description = "List available account statements (daily/monthly). Returns list[]{id, type (daily/monthly), date, status}. Use the id with statement_export to download."
    )]
    async fn statement_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<statement::StatementListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("statement_list", || statement::statement_list(&mctx, p)).await
    }

    /// Get the pre-signed download URL for a statement file.
    #[tool(
        title = "Export Statement",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::StatementUrlResponse>(),
        description = "Get a pre-signed download URL for a statement data file (obtained from statement_list). Returns {url}; fetch that URL to get the statement JSON."
    )]
    async fn statement_export(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<statement::StatementExportParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("statement_export", || statement::statement_export(&mctx, p)).await
    }

    /// Get short position (outstanding short) data for HK or US stocks.
    #[tool(
        title = "Short Positions",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get short interest history (open short positions) for HK or US stocks. Market inferred from symbol suffix. count: 1–100 (default 20). Unified data[]{timestamp(RFC3339), short_shares(open short position in shares), rate(decimal ratio e.g. 0.009=0.9%), close}. US-only: avg_daily_vol, days_to_cover. HK-only: balance(outstanding short position in HKD). US source: FINRA bi-weekly. HK source: HKEX daily."
    )]
    async fn short_positions(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ShortPositionsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("short_positions", || quote::short_positions(&mctx, p)).await
    }

    /// Get real-time option call/put volume stats.
    #[tool(
        title = "Option Volume",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get real-time option call/put volume stats for a US stock. Returns {call_volume, put_volume, put_call_ratio, call_oi, put_oi} and top active contracts."
    )]
    async fn option_volume(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OptionVolumeParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_volume", || quote::option_volume(&mctx, p)).await
    }

    /// Get daily historical option volume stats.
    #[tool(
        title = "Option Volume (Daily)",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get daily historical option stats for a US stock. Returns items[]{date, call_volume, put_volume, put_call_vol_ratio, call_oi, put_oi, put_call_oi_ratio}."
    )]
    async fn option_volume_daily(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OptionVolumeDailyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_volume_daily", || {
            quote::option_volume_daily(&mctx, p)
        })
        .await
    }

    /// List DCA (recurring investment) plans.
    #[tool(
        title = "List DCA Plans",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::DcaListResponse>(),
        description = "List DCA recurring investment plans. Returns plans[]{plan_id, symbol, amount, currency, frequency, status, next_execution_date}. Filter by status (Active/Suspended/Finished) or symbol."
    )]
    async fn dca_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_list", || dca::dca_list(&mctx, p)).await
    }

    /// Create a DCA (recurring investment) plan.
    #[tool(
        title = "Create DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Create a DCA recurring investment plan. frequency: Daily/Weekly/Monthly. day_of_week (Weekly): Mon/Tue/Wed/Thu/Fri. day_of_month (Monthly): 1-28."
    )]
    async fn dca_create(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaCreateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_create", || dca::dca_create(&mctx, p)).await
    }

    /// Update a DCA plan.
    #[tool(
        title = "Update DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Update an existing DCA plan by plan_id. Can change amount, frequency (Daily/Weekly/Monthly), day_of_week (Mon-Fri), or day_of_month (1-28). Returns updated plan."
    )]
    async fn dca_update(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaUpdateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_update", || dca::dca_update(&mctx, p)).await
    }

    /// Pause a DCA plan.
    #[tool(
        title = "Pause DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Pause (suspend) a DCA plan by plan_id. The plan stops executing until resumed. Returns upstream API response. Use dca_resume to restart."
    )]
    async fn dca_pause(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaPlanIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_pause", || dca::dca_pause(&mctx, p)).await
    }

    /// Resume a paused DCA plan.
    #[tool(
        title = "Resume DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Resume a suspended DCA plan by plan_id. Resumes automated execution on the configured schedule. Returns upstream API response."
    )]
    async fn dca_resume(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaPlanIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_resume", || dca::dca_resume(&mctx, p)).await
    }

    /// Stop a DCA plan permanently.
    #[tool(
        title = "Stop DCA Plan",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Permanently stop a DCA plan by plan_id. This cannot be undone. To temporarily pause, use dca_pause instead. Returns upstream API response."
    )]
    async fn dca_stop(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaPlanIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_stop", || dca::dca_stop(&mctx, p)).await
    }

    /// Get DCA plan execution history.
    #[tool(
        title = "DCA Execution History",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::DcaHistoryResponse>(),
        description = "Get execution history records for a DCA plan by plan_id. Returns executions[]{date, quantity, amount, price, status, order_id}."
    )]
    async fn dca_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaHistoryParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_history", || dca::dca_history(&mctx, p)).await
    }

    /// Get DCA statistics.
    #[tool(
        title = "DCA Statistics",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::DcaStatsResponse>(),
        description = "Get DCA investment statistics. Returns {total_invested, total_value, total_return, return_rate, plan_count, items[]{symbol, invested, value, return_rate}}."
    )]
    async fn dca_stats(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaStatsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_stats", || dca::dca_stats(&mctx, p)).await
    }

    /// Check if symbols support DCA.
    #[tool(
        title = "Check DCA Support",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::DcaCheckResponse>(),
        description = "Check whether given symbols support DCA recurring investment. Returns items[]{symbol, support_dca (bool), reason} for each queried symbol."
    )]
    async fn dca_check(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<dca::DcaCheckParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dca_check", || dca::dca_check(&mctx, p)).await
    }

    /// List community sharelists.
    #[tool(
        title = "List Sharelists",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::SharelistListResponse>(),
        description = "List user's own and subscribed community sharelists. Returns lists[]{id, name, description, symbol_count, is_owner, follower_count}."
    )]
    async fn sharelist_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistCountParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_list", || sharelist::sharelist_list(&mctx, p)).await
    }

    /// Get sharelist detail.
    #[tool(
        title = "Sharelist Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::SharelistDetailResponse>(),
        description = "Get community sharelist detail by id. Returns {id, name, description, constituents[]{symbol, name, last_done, change_rate}, quote data, subscription status}."
    )]
    async fn sharelist_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_detail", || sharelist::sharelist_detail(&mctx, p)).await
    }

    /// Create a community sharelist.
    #[tool(
        title = "Create Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::SharelistCreateResponse>(),
        description = "Create a new community sharelist with a name and optional description. Returns the created sharelist object including its id, name, and description."
    )]
    async fn sharelist_create(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistCreateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_create", || sharelist::sharelist_create(&mctx, p)).await
    }

    /// Delete a community sharelist.
    #[tool(
        title = "Delete Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Delete a community sharelist by id (own lists only; subscribed lists cannot be deleted). Returns upstream API response on success."
    )]
    async fn sharelist_delete(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_delete", || sharelist::sharelist_delete(&mctx, p)).await
    }

    /// Add stocks to a sharelist.
    #[tool(
        title = "Add to Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        description = "Add securities to a community sharelist by id. Provide symbols (e.g. [\"AAPL.US\", \"700.HK\"]) to add. Returns upstream API response."
    )]
    async fn sharelist_add(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistItemsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_add", || sharelist::sharelist_add(&mctx, p)).await
    }

    /// Remove stocks from a sharelist.
    #[tool(
        title = "Remove from Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Remove securities from a community sharelist by id. Provide symbols to remove. Returns upstream API response on success."
    )]
    async fn sharelist_remove(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistItemsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_remove", || sharelist::sharelist_remove(&mctx, p)).await
    }

    /// Reorder stocks in a sharelist.
    #[tool(
        title = "Sort Sharelist",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Reorder securities in a community sharelist by id. Provide symbols in the desired new order. Returns upstream API response on success."
    )]
    async fn sharelist_sort(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistItemsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_sort", || sharelist::sharelist_sort(&mctx, p)).await
    }

    /// Get popular community sharelists.
    #[tool(
        title = "Popular Sharelists",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::SharelistListResponse>(),
        description = "Get popular/trending community sharelists. Returns lists[]{id, name, description, symbol_count, follower_count, creator} sorted by popularity."
    )]
    async fn sharelist_popular(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<sharelist::SharelistCountParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("sharelist_popular", || {
            sharelist::sharelist_popular(&mctx, p)
        })
        .await
    }

    /// Run a quant indicator script against historical K-line data on the server.
    #[tool(
        title = "Quant — Run Indicator Script",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Run a quant indicator script against historical K-line data on the server. Executes the script server-side and returns the computed indicator/plot values as JSON. Periods: 1m, 5m, 15m, 30m, 1h, day, week, month, year (default: day). The optional input parameter accepts a JSON array matching the order of input.*() calls in the script, e.g. \"[14,2.0]\"."
    )]
    async fn quant_run(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<quant::RunScriptParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("quant_run", || quant::run_script(&mctx, p)).await
    }

    /// Search news by keyword.
    #[tool(
        title = "News Search",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Search news articles by keyword. Returns news_list[]{id, title, description, source_name, publish_at (RFC3339), score}. Paginate with score+publish_at_timestamp+id cursors."
    )]
    async fn news_search(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<search::NewsSearchParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("news_search", || search::news_search(&mctx, p)).await
    }

    /// Search community topics by keyword.
    #[tool(
        title = "Topic Search",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Search community topics/posts by keyword. Returns id, author, time, and excerpt."
    )]
    async fn topic_search(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<search::TopicSearchParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("topic_search", || search::topic_search(&mctx, p)).await
    }

    /// Get financial statements for a security.
    #[tool(
        title = "Financial Statements",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get financial statements (income statement, balance sheet, or cash flow) for a security. kind: IS/BS/CF/ALL. report: af (annual), saf (semi-annual), qf (quarterly full), q1/q2/q3."
    )]
    async fn financial_statement(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialStatementParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_statement", || {
            fundamental::financial_statement(&mctx, p)
        })
        .await
    }

    /// Get latest financial report summary for a security.
    #[tool(
        title = "Latest Financial Report",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::FinancialReportLatestResponse>(),
        description = "Get the latest financial report summary for a security. Returns {period, revenue, net_income, eps, roe, gross_margin, report_date} and key financial highlights."
    )]
    async fn financial_report_latest(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_report_latest", || {
            fundamental::financial_report_latest(&mctx, p)
        })
        .await
    }

    /// Get daily valuation rank (PE/PB percentile) for a security.
    #[tool(
        title = "Valuation Rank",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get daily valuation rank (PE/PB/PS/dividend yield industry percentile) for a security over a date range. start/end in yyyymmdd format."
    )]
    async fn valuation_rank(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::ValuationRankParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("valuation_rank", || fundamental::valuation_rank(&mctx, p)).await
    }

    /// Get institution rating history for a security.
    #[tool(
        title = "Institution Rating History",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::InstitutionRatingHistoryResponse>(),
        description = "Get institution rating history. Returns target_history[]{firm, analyst, old_target, new_target, date} and evaluate_history[]{firm, old_rating, new_rating, date}."
    )]
    async fn institution_rating_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institution_rating_history", || {
            fundamental::institution_rating_history(&mctx, p)
        })
        .await
    }

    /// Get institution rating industry rank for a security.
    #[tool(
        title = "Institution Rating Industry Rank",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::InstitutionRatingIndustryRankResponse>(),
        description = "Get peers ranked by institution analyst ratings in the same industry. Returns list[]{symbol, name, buy_count, sell_count, consensus_rating, target_price}. Paginated."
    )]
    async fn institution_rating_industry_rank(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::InstitutionRatingIndustryRankParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institution_rating_industry_rank", || {
            fundamental::institution_rating_industry_rank(&mctx, p)
        })
        .await
    }

    /// Get short margin deposit details for the current account.
    #[tool(
        title = "Short Margin",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get short margin deposit details for the current account. Returns short positions with margin_amount, margin_rate, interest_rate, symbol, quantity per position."
    )]
    async fn short_margin(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("short_margin", || trade::short_margin(&mctx)).await
    }

    /// List linked withdrawal bank cards.
    #[tool(
        title = "Bank Cards",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "List linked withdrawal bank cards for the current account. Returns cards[]{id, bank_name, account_number (masked), currency, status}."
    )]
    async fn bank_cards(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("bank_cards", || atm::bank_cards(&mctx)).await
    }

    /// List withdrawal history.
    #[tool(
        title = "Withdrawals",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "List withdrawal history for the current account. Returns items[]{id, amount, currency, status, created_at, bank_name, account_number (masked)}."
    )]
    async fn withdrawals(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<atm::WithdrawalParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("withdrawals", || atm::withdrawals(&mctx, p)).await
    }

    /// List deposit history.
    #[tool(
        title = "Deposits",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "List deposit history for the current account. Returns items[]{id, amount, currency, status, created_at, updated_at}. states: comma-separated (Pending/Finished/Failed). currencies: comma-separated codes."
    )]
    async fn deposits(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<atm::DepositParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("deposits", || atm::deposits(&mctx, p)).await
    }

    /// List IPO stocks currently in subscription stage (HK and US).
    #[tool(
        title = "IPO Subscriptions",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::IpoSubscriptionsResponse>(),
        description = "List IPO stocks in subscription/pre-filing stage (HK+US). Returns items[]{symbol, name, market, sub_start_date, sub_end_date, listing_date, issue_price, min_lot_size}."
    )]
    async fn ipo_subscriptions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_subscriptions", || ipo::ipo_subscriptions(&mctx)).await
    }

    /// Show the IPO calendar.
    #[tool(
        title = "IPO Calendar",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::IpoCalendarResponse>(),
        description = "Show the IPO calendar. Returns items[]{symbol, name, market, sub_start_date, sub_end_date, listing_date, status} for upcoming and recent IPOs."
    )]
    async fn ipo_calendar(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_calendar", || ipo::ipo_calendar(&mctx)).await
    }

    /// List recently listed IPO stocks.
    #[tool(
        title = "IPO Listed",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::IpoListedResponse>(),
        description = "List recently listed IPO stocks (HK+US). Returns items[]{symbol, name, listing_date, issue_price, first_day_close, first_day_return, volume, market}."
    )]
    async fn ipo_listed(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoListedParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_listed", || ipo::ipo_listed(&mctx, p)).await
    }

    /// Show IPO detail for a symbol.
    #[tool(
        title = "IPO Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::IpoDetailResponse>(),
        description = "Show IPO detail for a symbol. Returns profile (business overview), timeline[]{event, date}, subscription eligibility, pricing_range, lot_size, allotment_rules."
    )]
    async fn ipo_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_detail", || ipo::ipo_detail(&mctx, p)).await
    }

    /// List IPO orders (active and history).
    #[tool(
        title = "IPO Orders",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::IpoOrdersResponse>(),
        description = "List IPO orders (active+history). Returns orders[]{order_id, symbol, market, quantity, total_amount, status, submitted_at}. Filter by symbol, market, or status."
    )]
    async fn ipo_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_orders", || ipo::ipo_orders(&mctx, p)).await
    }

    /// Show IPO order detail by order ID.
    #[tool(
        title = "IPO Order Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::IpoOrderDetailResponse>(),
        description = "Show detailed information for a specific IPO order by order_id. Returns {order_id, symbol, market, quantity, allotted_quantity, total_amount, status, submitted_at}."
    )]
    async fn ipo_order_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoOrderDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_order_detail", || ipo::ipo_order_detail(&mctx, p)).await
    }

    /// Show IPO profit/loss summary and breakdown.
    #[tool(
        title = "IPO Profit / Loss",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::IpoProfitLossResponse>(),
        description = "Show IPO profit/loss summary and per-stock breakdown. Returns {total_cost, total_value, total_return, items[]{symbol, cost, current_value, return_rate}}. period: all/ytd/1y/3y."
    )]
    async fn ipo_profit_loss(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ipo::IpoProfitLossParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("ipo_profit_loss", || ipo::ipo_profit_loss(&mctx, p)).await
    }

    /// Get current-period business segment revenue breakdown.
    #[tool(
        title = "Business Segments",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Get current-period business segment revenue breakdown for a symbol (name, percent, total, currency)"
    )]
    async fn business_segments(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::BusinessSegmentsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("business_segments", || {
            fundamental::business_segments(&mctx, p)
        })
        .await
    }

    /// Get historical business segment revenue trends.
    #[tool(
        title = "Business Segments History",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::BusinessSegmentsHistoryResponse>(),
        description = "Get historical business segment revenue trends (by period and category). Returns historical[].{date, total, currency, business[{name,percent,value}], regionals[{name,percent,value}]}"
    )]
    async fn business_segments_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::BusinessSegmentsHistoryParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("business_segments_history", || {
            fundamental::business_segments_history(&mctx, p)
        })
        .await
    }

    /// Get monthly institutional rating distribution timeline.
    #[tool(
        title = "Institutional Views",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::InstitutionalViewsResponse>(),
        description = "Get monthly institutional rating distribution timeline. Returns months[]{date, buy, outperform, hold, underperform, sell, total} for trend analysis."
    )]
    async fn institutional_views(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("institutional_views", || {
            fundamental::institutional_views(&mctx, p)
        })
        .await
    }

    /// Get industry ranking list by market and indicator.
    #[tool(
        title = "Industry Rank",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Industry ranking list by market (US/HK/CN/SG) and indicator (0=领涨/1=今日走势/2=人气/3=市值/4=营收/5=营收增长率/6=净利润/7=净利润增长率). sort_type: 0=单级 1=多层. Returns items[]{counter_id(BK/US/IN00258), name, chg, lists[]}. Pass counter_id directly to industry_peers."
    )]
    async fn industry_rank(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::IndustryRankParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("industry_rank", || market::industry_rank(&mctx, p)).await
    }

    /// Get hierarchical industry peer group tree for an industry index symbol.
    #[tool(
        title = "Industry Peers",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::IndustryPeersResponse>(),
        description = "Hierarchical sub-sector tree for an industry group. Accepts BK counter_id from industry_rank (e.g. BK/US/IN00258). Returns chain{name,counter_id,stock_num,chg,ytd_chg,next[{...}]} and top{name,market}. Each node shows stock count, daily change, and YTD change."
    )]
    async fn industry_peers(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::IndustryPeersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("industry_peers", || fundamental::industry_peers(&mctx, p)).await
    }

    /// Get financial report snapshot with actual vs forecast comparison.
    #[tool(
        title = "Financial Report Snapshot",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::FinancialReportSnapshotResponse>(),
        description = "Get financial report snapshot: report_desc (text summary), fo_revenue/fo_ebit/fo_eps (actual vs forecast with yoy/cmp), fr_* financial ratios (ROE, margins, assets, cash flow). report: qf/saf/af."
    )]
    async fn financial_report_snapshot(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialReportSnapshotParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_report_snapshot", || {
            fundamental::financial_report_snapshot(&mctx, p)
        })
        .await
    }

    /// Get Top 20 major shareholders with multi-period holdings.
    #[tool(
        title = "Top 20 Shareholders",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ShareholderTopResponse>(),
        description = "Get Top 20 major shareholders (institutions, individuals, insiders) across reporting periods. Returns info[]{period, share_holders[]{object_id, name, title, shares_held, percent_shares_held, shares_changed, filing_date}}. Use object_id with shareholder_detail to drill into a holder's full trade history."
    )]
    async fn shareholder_top(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::ShareholderTopParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("shareholder_top", || fundamental::shareholder_top(&mctx, p)).await
    }

    /// Get single shareholder's holding history and trade details by object_id.
    #[tool(
        title = "Shareholder Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ShareholderDetailResponse>(),
        description = "Get a single shareholder's holding and trade history. Requires object_id from shareholder_top. Returns name, owner_source (Company/Institution/Person/Insider), tradings[]{period, accum_buy, accum_sell, net_buy, trading_details[]{trading_date, trading_type, trading_shares, trading_price, security_type, filing_date}}, holding_summary, holding_periods, trading_periods. Note: trading_details[] is empty for institutional (13F) holders — it is only populated for insider/individual filers (Form 4)."
    )]
    async fn shareholder_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::ShareholderDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("shareholder_detail", || {
            fundamental::shareholder_detail(&mctx, p)
        })
        .await
    }

    /// Compare valuation metrics across multiple stocks in the same industry.
    #[tool(
        title = "Stock Comparison",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fundamental::ValuationComparisonResponse>(),
        description = "Stock valuation comparison. Mode A (single): pass only symbol — server returns stock + auto-selected industry peers. Mode B (multi): pass symbol as primary + comparison_symbols (comma-separated, e.g. 'MSFT.US,GOOGL.US') for explicit peer comparison. currency: USD/HKD/CNY. Returns list[]{symbol, name, market_value, price_close, pe, pb, ps, history[]{date, pe, pb, ps}}."
    )]
    async fn valuation_comparison(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::ValuationComparisonParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("valuation_comparison", || {
            fundamental::valuation_comparison(&mctx, p)
        })
        .await
    }

    /// Get short-sale trade volume history for HK or US stocks.
    #[tool(
        title = "Short Trades",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::ShortTradesResponse>(),
        description = "Get daily short-sale volume history for HK or US stocks. Market inferred from symbol suffix. last_timestamp: unix seconds (omit for latest). page_size: 1–100 (default 20). Unified data[]{timestamp(RFC3339), short_vol(daily short volume in shares), rate(decimal ratio e.g. 0.36=36%), close}. US-only: nasdaq_vol(NASDAQ short), nyse_vol(NYSE short). HK-only: balance(HKD), market_vol(total market volume that day). US source: FINRA/NASDAQ daily. HK source: HKEX daily."
    )]
    async fn short_trades(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::ShortTradesParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("short_trades", || market::short_trades(&mctx, p)).await
    }

    /// Get top movers — stocks whose price exceeds the 20-day standard deviation.
    #[tool(
        title = "Top Movers",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::TopMoversResponse>(),
        description = "Get stocks whose price fluctuation exceeds the 20-trading-day standard deviation, with correlated news reasons. markets: comma-separated HK/US/CN/SG (omit=all). sort: 0=time 1=change-magnitude 2=popularity/heat (default). limit: results per page (default 20). next_params: pass next_params from previous response to paginate. Returns events[]{timestamp(RFC3339), alert_reason, alert_type, stock{symbol, name, change(decimal ratio e.g. 0.0445=+4.45%), last_done, labels[], intro}}, updated_at, next_params."
    )]
    async fn top_movers(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::StockEventsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("top_movers", || market::top_movers(&mctx, p)).await
    }

    /// Get rank tab category configurations for the popularity leaderboard.
    #[tool(
        title = "Rank Categories",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::RankCategoriesResponse>(),
        description = "Get rank tab category configurations for the popularity leaderboard. Returns first_tags[]{key, name, second_tags[]{key, name, market}}. Pass a second_tags key (e.g. `hot_all-us`) to rank_list."
    )]
    async fn rank_categories(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("rank_categories", || market::rank_categories(&mctx)).await
    }

    /// Get ranked stock list by leaderboard tab key.
    #[tool(
        title = "Rank List",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::market::RankListResponse>(),
        description = "Get ranked stock list by leaderboard tab key. key: from rank_categories second_tags[].key (e.g. \"hot_all-us\", \"hot_up-hk\", \"trade_heat-us\"). market: inferred from key suffix (-us/-hk) or pass explicitly. size: results (default 20). Returns lists[]{symbol, name, last_done, chg(decimal), inflow, market_cap, pre_post_price, pre_post_chg, amplitude, turnover_rate, volume_rate, five_day_chg, ten_day_chg, twenty_day_chg, this_year_chg, industry, intro}, updated_at."
    )]
    async fn rank_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<market::RankListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("rank_list", || market::rank_list(&mctx, p)).await
    }

    /// List platform-preset stock screener strategies.
    #[tool(
        title = "Screener Recommend Strategies",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::ScreenerStrategiesResponse>(),
        description = "List platform-preset screener strategies. market: US|HK|CN|SG (default: US). Returns strategys[]{id, name, description, market, three_months_chg, risk}. Pass id to screener_search strategy_id to run, or screener_strategy to inspect filter conditions."
    )]
    async fn screener_recommend_strategies(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<screener::ScreenerRecommendStrategiesParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("screener_recommend_strategies", || {
            screener::screener_recommend_strategies(&mctx, p)
        })
        .await
    }

    /// List user's own saved stock screener strategies.
    #[tool(
        title = "Screener User Strategies",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::ScreenerStrategiesResponse>(),
        description = "List the current user's saved screener strategies. market: US|HK|CN|SG (default: US). Returns strategys[]{id, name, description, market, three_months_chg, risk}. Pass id to screener_search strategy_id to run, or screener_strategy to inspect conditions."
    )]
    async fn screener_user_strategies(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<screener::ScreenerUserStrategiesParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("screener_user_strategies", || {
            screener::screener_user_strategies(&mctx, p)
        })
        .await
    }

    /// Get single screener strategy detail by id.
    #[tool(
        title = "Screener Strategy",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::ScreenerStrategyResponse>(),
        description = "Inspect a screener strategy's filter conditions before running it. Returns market, filter{filters[]{key, min, max, tech_values}}. Use screener_search strategy_id to execute the strategy."
    )]
    async fn screener_strategy(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<screener::ScreenerStrategyParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("screener_strategy", || {
            screener::screener_strategy(&mctx, p)
        })
        .await
    }

    /// Execute a stock screener search by strategy or custom conditions.
    #[tool(
        title = "Screener Search",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::ScreenerSearchResponse>(),
        description = "Screen stocks. market: US|HK|CN|SG (Mode B required; Mode A uses strategy's market). Mode A: strategy_id from screener_recommend_strategies — auto-runs saved strategy. Mode B: conditions=[{\"key\":\"KEY\",\"min\":\"10\",\"max\":\"50\",\"tech_values\":{}},...]. extra_returns=[\"key\",...] adds display-only columns. sort_by_key: key name to sort by; sort_order: asc|desc (default desc). page: 0-based (default 0). Returns {total, items[]{symbol, name, indicators[]{key, name, value, unit}}}. Fundamental keys: pettm pbmrq roe roa netmargin salesgrowthyoy netincomegrowthyoy marketcap(亿) circulating_marketcap(亿) prevclose prevchg(%) divyld la epsttm netincome(亿) sales(亿) turnover_rate balance(万). Technical keys (call screener_indicators for tech_values schema): macd_day/week rsi_day/week kdj_day/week boll_day/week."
    )]
    async fn screener_search(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<screener::ScreenerSearchParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("screener_search", || screener::screener_search(&mctx, p)).await
    }

    /// Get all available stock screener indicator metadata.
    #[tool(
        title = "Screener Indicators",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::discovery::ScreenerIndicatorsResponse>(),
        description = "Get all available screener indicator keys with units and default value ranges. Technical indicators include a tech_values field showing available options (e.g. macd_day: {category:[goldenfork,deadcross], period:[day,week]}). Optional symbol (e.g. AAPL.US) narrows to stock-specific indicators. Returns groups[]{group_name, indicators[]{id, key, name, unit, default_range{min,max}, tech_values?{key:[{value,label},...]}}}."
    )]
    async fn screener_indicators(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<screener::ScreenerIndicatorsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("screener_indicators", || {
            screener::screener_indicators(&mctx, p)
        })
        .await
    }
}

#[tool_handler(
    name = "longbridge-mcp",
    instructions = "Longbridge OpenAPI MCP Server - provides market data, trading, and financial analysis tools"
)]
impl ServerHandler for Longbridge {
    // `get_info` mirrors the `#[tool_handler]` default tool metadata, plus the
    // resources capability for `lb://tools/{name}/output-schema` documents. The
    // reverse-auth flow does not need a `tools.listChanged` push: the agent
    // client obtains the token from `authenticate` and reconnects with the
    // `Authorization` header, which re-initializes the session and re-fetches
    // the full tool list.

    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::new(
            "longbridge-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Longbridge OpenAPI MCP Server - provides market data, trading, and financial analysis tools",
        )
    }

    /// `initialize`, with endpoint-aware `instructions`.
    ///
    /// Main endpoint: byte-for-byte the macro default (`get_info`), so the
    /// pre-feature `initialize` response is unchanged. Unauthenticated
    /// `/agent` sessions instead get instructions that explicitly frame the
    /// endpoint as a temporary authorization channel, so AI clients do not
    /// mistake `<host>/agent` for the Longbridge MCP service address itself.
    async fn initialize(
        &self,
        request: rmcp::model::InitializeRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::InitializeResult, rmcp::ErrorData> {
        // Mirror the rmcp default implementation.
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        let mut info = self.get_info();
        if is_agent_endpoint(&context) && !is_authenticated(&context) {
            info.instructions = Some(format!(
                "Longbridge MCP AUTHORIZATION endpoint. The `/agent` path is only a temporary \
                 channel for completing authorization — it is NOT the Longbridge MCP service \
                 address. Call the `authenticate` tool with a one-time authorization code (the \
                 user can generate one at {}), then follow the tool result: connect to the main \
                 Longbridge MCP service (same host, without the `/agent` path) using the \
                 returned `Authorization: Bearer` token.",
                crate::tools::authenticate::connect_page_url()
            ));
        } else if let Some(version) = restricted_version(&context) {
            // Restricted public endpoints describe only their (non-execution)
            // capabilities and do not mention trading. Keep the first sentence
            // self-contained (clients surface the first ~512 chars).
            info.instructions = Some(match version {
                RestrictedVersion::V1 => {
                    "Longbridge MCP — market and investment analysis for US, HK, A-share, and SG \
                     markets. Provides live quotes, candlestick charts, order-book depth, \
                     fundamentals, valuations, analyst ratings and estimates, news, filings, and \
                     screeners."
                        .to_string()
                }
                RestrictedVersion::V2 => {
                    "Longbridge MCP — market analysis and portfolio insight for US, HK, A-share, \
                     and SG markets. Provides live quotes, charts, order-book depth, fundamentals, \
                     valuations, analyst research, news, filings, screeners, watchlists, and \
                     read-only account, portfolio, and order history. Does not place, modify, or \
                     cancel orders."
                        .to_string()
                }
            });
        }
        Ok(info)
    }

    /// List tools, gated on endpoint and authentication state.
    ///
    /// Three cases:
    /// - **Main endpoint** (`/mcp`, root): a token is always present (the auth
    ///   middleware rejects token-less requests with 401 before they reach
    ///   here), so the full tool set is returned — the `authenticate` tool is
    ///   filtered out, keeping the main endpoint's tool list byte-for-byte
    ///   identical to its pre-feature behaviour.
    /// - **`/agent` endpoint, authenticated**: behaves exactly like the main
    ///   endpoint — full tool set, no `authenticate`.
    /// - **`/agent` endpoint, unauthenticated**: only the `authenticate` tool is
    ///   exposed, so an OAuth-incapable client can complete the handshake and
    ///   self-authorize. After `authenticate` succeeds and the client starts
    ///   sending the returned token, the next `tools/list` returns the full set.
    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        cached_router().get(name).cloned()
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        // Execution-layer gate for a restricted endpoint: a tool hidden from
        // `tools/list` must also be un-callable by name, or the listing filter
        // is merely cosmetic. Off-allowlist tools are rejected here.
        if let Some(version) = restricted_version(&context)
            && !is_restricted_tool_allowed(version, request.name.as_ref())
        {
            return Err(McpError::invalid_request(
                format!(
                    "Tool `{}` is not available on this endpoint. \
                     This endpoint does not expose trade execution, recurring-investment, \
                     IPO order, or money-movement tools.",
                    request.name
                ),
                None,
            ));
        }
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        cached_router().call(tcc).await
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        // Both slices are pre-filtered once at startup; each request only pays
        // the clone cost (Arc ref-bumps + String title copies), not filter work.
        let tools = if is_agent_endpoint(&context) && !is_authenticated(&context) {
            tools_agent_endpoint().to_vec()
        } else if let Some(version) = restricted_version(&context) {
            match version {
                RestrictedVersion::V1 => tools_v1_endpoint().to_vec(),
                RestrictedVersion::V2 => tools_v2_endpoint().to_vec(),
            }
        } else {
            tools_main_endpoint().to_vec()
        };
        Ok(rmcp::model::ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        Ok(ListResourcesResult::with_all_items(
            output_schema_resources(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        read_output_schema_resource(&request.uri)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    use super::collect_headers;

    #[test]
    fn collects_all_valid_headers() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("x-custom"),
            HeaderValue::from_static("hello"),
        );
        map.insert(
            HeaderName::from_static("accept-language"),
            HeaderValue::from_static("zh-CN"),
        );
        let headers = collect_headers(&map);
        assert!(headers.iter().any(|(k, v)| k == "x-custom" && v == "hello"));
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "accept-language" && v == "zh-CN")
        );
    }

    #[test]
    fn does_not_forward_raw_user_agent() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("claude-code/2.1.89 (cli)"),
        );
        // The client UA is folded into the synthesized upstream UA, not forwarded raw.
        assert!(!collect_headers(&map).iter().any(|(k, _)| k == "user-agent"));
    }

    fn ctx_with_ua(ua: Option<&str>) -> super::McpContext {
        super::McpContext {
            token: String::new(),
            language: None,
            client_user_agent: ua.map(str::to_owned),
            extra_headers: Vec::new(),
        }
    }

    #[test]
    fn user_agent_chains_client_then_self() {
        let ua = ctx_with_ua(Some("claude-code/2.1.89 (cli)")).user_agent();
        assert!(ua.starts_with("claude-code/2.1.89 (cli) longbridge-mcp/"));
    }

    #[test]
    fn user_agent_falls_back_to_self_when_client_absent() {
        assert!(
            ctx_with_ua(None)
                .user_agent()
                .starts_with("longbridge-mcp/")
        );
        assert!(
            ctx_with_ua(Some("   "))
                .user_agent()
                .starts_with("longbridge-mcp/")
        );
    }

    /// End-to-end: the client produced by `create_http_client` must put the
    /// synthesized `User-Agent` (client UA + our token) on the wire as the
    /// primary value. A minimal TCP server captures the real request headers;
    /// the SDK base URL is redirected to it via the `HTTP_URL` env var.
    #[tokio::test]
    async fn upstream_request_carries_synthesized_user_agent() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let cap = Arc::clone(&captured);
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let mut data = Vec::new();
                // Accumulate until the full HTTP header block arrives (matching
                // the async spawn_capture_server pattern used in quote_cmd_tests).
                loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n]);
                    if data.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let req = String::from_utf8_lossy(&data);
                for line in req.lines() {
                    if line.to_ascii_lowercase().starts_with("user-agent:") {
                        *cap.lock().unwrap() = Some(line["user-agent:".len()..].trim().to_string());
                        break;
                    }
                }
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}",
                );
            }
        });

        let mctx = super::McpContext {
            token: "dummy-token".to_string(),
            language: None,
            client_user_agent: Some("claude-code/2.1.89 (cli)".to_string()),
            extra_headers: Vec::new(),
        };
        // Build the client with the SDK base URL redirected at the local server.
        // Serialized against other env-mutating tests; the guard is released
        // before the await so it is never held across a suspension point.
        let client = {
            let _env_guard = super::HTTP_URL_ENV_LOCK.lock().await;
            // SAFETY: guarded by HTTP_URL_ENV_LOCK; set before build, cleared after.
            unsafe { std::env::set_var("LONGBRIDGE_HTTP_URL", format!("http://{addr}")) };
            let client = mctx.create_http_client();
            unsafe { std::env::remove_var("LONGBRIDGE_HTTP_URL") };
            client
        };
        let _ = client
            .request(reqwest::Method::GET, "/v1/ping")
            .response::<String>()
            .send()
            .await;

        let ua = captured
            .lock()
            .unwrap()
            .clone()
            .expect("echo server did not receive a request");
        assert!(
            ua.starts_with("claude-code/2.1.89 (cli) longbridge-mcp/"),
            "unexpected upstream User-Agent: {ua}"
        );
    }

    #[test]
    fn skips_non_utf8_values() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("x-valid"),
            HeaderValue::from_static("ok"),
        );
        map.insert(
            HeaderName::from_static("x-binary"),
            HeaderValue::from_bytes(&[0x80, 0x81]).unwrap(),
        );
        let headers = collect_headers(&map);
        assert!(headers.iter().any(|(k, v)| k == "x-valid" && v == "ok"));
        assert!(!headers.iter().any(|(k, _)| k == "x-binary"));
    }

    #[test]
    fn empty_map_returns_empty() {
        assert!(collect_headers(&HeaderMap::new()).is_empty());
    }

    /// Read the `tools` array of one scope from `data/scopes.json`.
    fn scope_tools(key: &str) -> Vec<String> {
        let scopes: serde_json::Value =
            serde_json::from_str(include_str!("../../data/scopes.json"))
                .expect("scopes.json must be valid JSON");
        scopes["scopes"]
            .as_array()
            .expect("scopes array")
            .iter()
            .find(|s| s["key"].as_str() == Some(key))
            .and_then(|s| s["tools"].as_array())
            .unwrap_or_else(|| panic!("scope `{key}` must exist with a tools array"))
            .iter()
            .map(|t| t.as_str().expect("tool name must be a string").to_string())
            .collect()
    }

    /// The version table is the single source of truth for `/v1` and `/v2`
    /// membership. It must classify every live tool exactly once and reference
    /// only live tools, so adding or renaming a tool forces an explicit
    /// classification (and can never silently leak onto a restricted endpoint).
    #[test]
    fn endpoint_table_is_complete_and_live() {
        use std::collections::HashSet;

        let live: HashSet<String> = crate::tools::list_tools()
            .iter()
            .map(|t| t.name.to_string())
            .collect();

        let mut seen = HashSet::new();
        for (name, _) in super::TOOL_ENDPOINTS {
            assert!(
                seen.insert(*name),
                "TOOL_ENDPOINTS lists `{name}` more than once"
            );
            assert!(
                live.contains(*name),
                "TOOL_ENDPOINTS references unknown tool `{name}` (renamed or removed?)"
            );
        }

        for name in &live {
            assert!(
                seen.contains(name.as_str()),
                "live tool `{name}` is not classified in TOOL_ENDPOINTS \
                 (add it with V1|V2, V2, or 0)"
            );
        }
    }

    /// Every `/v1` tool must also be on `/v2`: the v2 surface is a superset of
    /// the read-only analysis surface.
    #[test]
    fn v1_is_subset_of_v2() {
        for name in super::v1_tool_names() {
            assert!(
                super::is_v2_public_tool(name),
                "`{name}` is on /v1 but not /v2 — v1 must be a subset of v2"
            );
        }
    }

    /// The public `/v1` allowlist must be read-only and disjoint from the
    /// trading/account scopes. Guards against a write/trading/account tool
    /// landing on the endpoint submitted to OpenAI/Claude/Grok.
    #[test]
    fn v1_allowlist_is_read_only_and_disjoint_from_trading_scopes() {
        use std::collections::HashSet;

        let live = crate::tools::list_tools();
        let by_name: std::collections::HashMap<&str, &rmcp::model::Tool> =
            live.iter().map(|t| (t.name.as_ref(), t)).collect();
        for name in super::v1_tool_names() {
            let tool = by_name
                .get(name)
                .unwrap_or_else(|| panic!("/v1 allowlist references unknown tool `{name}`"));
            let read_only = tool.annotations.as_ref().and_then(|a| a.read_only_hint);
            assert_eq!(
                read_only,
                Some(true),
                "/v1 allowlist tool `{name}` is not marked read_only_hint = true"
            );
        }

        let allow: HashSet<&str> = super::v1_tool_names().into_iter().collect();
        for forbidden in ["trade.write", "trade.read", "account.read"] {
            for tool in scope_tools(forbidden) {
                assert!(
                    !allow.contains(tool.as_str()),
                    "/v1 allowlist must not contain `{tool}` from forbidden scope `{forbidden}`"
                );
            }
        }
    }

    /// The public `/v2` allowlist must never expose trade execution, DCA
    /// automation, IPO order management, or money movement / PCI tools — the
    /// hard-exclusion set, even though `/v2` is otherwise a near-full surface.
    #[test]
    fn v2_allowlist_excludes_execution_dca_ipo_orders_and_money_movement() {
        use std::collections::HashSet;

        let allow: HashSet<&str> = super::v2_tool_names().into_iter().collect();

        // trade.write (submit/cancel/replace + DCA writes) must be fully absent.
        for tool in scope_tools("trade.write") {
            assert!(
                !allow.contains(tool.as_str()),
                "/v2 allowlist must not contain `{tool}` from scope `trade.write`"
            );
        }

        // Explicit hard-exclusion set, independent of scope grouping.
        let hard_excluded = [
            // All DCA automation.
            "dca_check",
            "dca_history",
            "dca_list",
            "dca_stats",
            "dca_create",
            "dca_pause",
            "dca_resume",
            "dca_stop",
            "dca_update",
            // Order write operations.
            "submit_order",
            "cancel_order",
            "replace_order",
            // IPO order management.
            "ipo_orders",
            "ipo_order_detail",
            "ipo_profit_loss",
            // Money movement / PCI.
            "deposits",
            "withdrawals",
            "bank_cards",
        ];
        for tool in hard_excluded {
            assert!(
                !allow.contains(tool),
                "/v2 allowlist must not contain hard-excluded tool `{tool}`"
            );
        }
    }
}

#[cfg(test)]
mod quote_cmd_tests {
    use super::{CURRENT_TOOL, HTTP_URL_ENV_LOCK, McpContext, QUOTE_CMD_PATH, send_quote_cmd};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Start a throwaway HTTP server on an ephemeral port that captures the raw
    /// bytes of the first request, replies `200`, and hands the request back over
    /// a oneshot channel. A real socket — no HTTP mocking — so the test exercises
    /// the actual SDK `HttpClient` send path and survives future refactors.
    async fn spawn_capture_server() -> (u16, tokio::sync::oneshot::Receiver<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buf = [0u8; 4096];
            let mut data = Vec::new();
            // Read until the end of the request headers.
            while !data.windows(4).any(|w| w == b"\r\n\r\n") {
                match socket.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => data.extend_from_slice(&buf[..n]),
                }
            }
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await;
            let _ = socket.flush().await;
            let _ = tx.send(String::from_utf8_lossy(&data).into_owned());
        });
        (port, rx)
    }

    /// Every upstream request built within a tool scope must carry both the
    /// synthesized `user-agent` (client UA chained with this server's token) and
    /// an `x-mcp-tool` header naming the tool, so the server can attribute the
    /// call per tool. Drives the real `create_http_client` path (which reads the
    /// `CURRENT_TOOL` task-local set by `measured_tool_call`) and sends the beacon
    /// to `GET /v1/quote/cmd` against a local server — no HTTP mocking.
    #[tokio::test]
    async fn upstream_request_carries_x_mcp_tool_and_user_agent() {
        let (port, rx) = spawn_capture_server().await;

        let mctx = McpContext {
            token: "test-token".to_string(),
            language: None,
            client_user_agent: Some("claude-test/1.0 (cli)".to_string()),
            extra_headers: Vec::new(),
        };

        // Build the client inside a `CURRENT_TOOL` scope (as `measured_tool_call`
        // does for real tool calls), with the SDK base URL redirected at the
        // local server. `sync_scope` keeps the locked region free of any await.
        let client = {
            let _env_guard = HTTP_URL_ENV_LOCK.lock().await;
            // SAFETY: guarded by HTTP_URL_ENV_LOCK; set before build, cleared after.
            unsafe { std::env::set_var("LONGBRIDGE_HTTP_URL", format!("http://127.0.0.1:{port}")) };
            let client = CURRENT_TOOL.sync_scope("depth", || mctx.create_http_client());
            unsafe { std::env::remove_var("LONGBRIDGE_HTTP_URL") };
            client
        };

        send_quote_cmd(&client).await;

        let request = tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .expect("capture server did not receive a request in time")
            .expect("capture server dropped the request");
        let lower = request.to_lowercase();

        let request_line = request.lines().next().unwrap_or_default();
        assert!(
            request_line.starts_with(&format!("GET {QUOTE_CMD_PATH}")),
            "expected `GET {QUOTE_CMD_PATH}`, got request line: {request_line}"
        );
        assert!(
            lower.contains("x-mcp-tool: depth"),
            "x-mcp-tool tracking header missing; request was:\n{request}"
        );
        let expected_ua = format!(
            "user-agent: claude-test/1.0 (cli) longbridge-mcp/{}",
            env!("CARGO_PKG_VERSION")
        );
        assert!(
            lower.contains(&expected_ua.to_lowercase()),
            "synthesized user-agent missing; request was:\n{request}"
        );
    }

    /// Recursively collect `.rs` files under `dir`.
    fn rs_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    out.extend(rs_files(&path));
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        out
    }

    /// Guard: every quote tool must obtain its `QuoteContext` via
    /// `mctx.get_quote_context()` (which fires the `/v1/quote/cmd` beacon),
    /// never `QuoteContext::new(...)` directly. The sole sanctioned constructor
    /// call lives outside `src/tools`, so every tool file must
    /// be free of `QuoteContext::new(`. This makes "every WS quote tool is
    /// tracked" an enforced invariant: a new tool that constructs its own
    /// `QuoteContext` fails this test.
    #[test]
    fn quote_tools_use_tracking_context_constructor() {
        // Scan all of src/ so files outside src/tools/ (e.g. a future
        // src/subscriptions.rs) are also caught.
        // Allowed construction sites:
        //   - src/ws_pool.rs       — the sanctioned pool that wraps QuoteContext::new
        //   - src/tools/mod.rs     — this file (defines get_quote_context; comments
        //                            reference QuoteContext::new() in doc strings)
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let allowed: std::collections::HashSet<_> = [
            src_dir.join("ws_pool.rs"),
            src_dir.join("tools").join("mod.rs"),
        ]
        .into();
        let mut offenders = Vec::new();
        for file in rs_files(&src_dir) {
            if allowed.contains(&file) {
                continue;
            }
            let src = std::fs::read_to_string(&file).unwrap();
            for (i, line) in src.lines().enumerate() {
                if line.contains("QuoteContext::new(") {
                    offenders.push(format!("{}:{}", file.display(), i + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "QuoteContext::new() is only allowed in src/ws_pool.rs. \
             All other code must use `mctx.get_quote_context()` so calls go \
             through the connection pool and the /v1/quote/cmd beacon. \
             Untracked constructor at:\n{}",
            offenders.join("\n")
        );
    }

    fn schema_contains_key(value: &serde_json::Value, key: &str) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                map.contains_key(key) || map.values().any(|v| schema_contains_key(v, key))
            }
            serde_json::Value::Array(values) => values.iter().any(|v| schema_contains_key(v, key)),
            _ => false,
        }
    }

    #[test]
    fn tool_list_output_schemas_are_compact_validation_contracts() {
        let depth = super::list_tools()
            .into_iter()
            .find(|tool| tool.name == "depth")
            .expect("depth tool must be registered");
        let output_schema = depth
            .output_schema
            .expect("depth tool must keep an outputSchema in tools/list");
        let output_schema = serde_json::Value::Object(output_schema.as_ref().clone());

        assert!(
            output_schema.get("properties").is_some(),
            "compact outputSchema must keep validation structure"
        );
        for stripped_key in ["$schema", "title", "description"] {
            assert!(
                !schema_contains_key(&output_schema, stripped_key),
                "`{stripped_key}` should move out of the tools/list outputSchema"
            );
        }
    }

    #[test]
    fn tool_list_output_schema_tools_omit_redundant_return_field_lists() {
        let screener_search = super::list_tools()
            .into_iter()
            .find(|tool| tool.name == "screener_search")
            .expect("screener_search tool must be registered");

        assert!(
            screener_search.output_schema.is_some(),
            "fixture must cover a typed-output tool"
        );
        assert!(
            !screener_search
                .description
                .as_deref()
                .unwrap_or_default()
                .contains("Returns "),
            "typed-output tools should not duplicate output field lists in top-level descriptions"
        );
    }

    #[test]
    fn tool_metadata_lint_keeps_typed_output_descriptions_compact() {
        let offenders: Vec<String> = super::list_tools()
            .into_iter()
            .filter(|tool| tool.output_schema.is_some())
            .filter_map(|tool| {
                let description = tool.description.as_deref().unwrap_or_default();
                let lower = description.to_ascii_lowercase();
                let has_return_shape = (lower.contains("returns ")
                    && (lower.contains("[]") || lower.contains("returns {")))
                    || lower.contains("unified data[]")
                    || lower.contains("us-only:")
                    || lower.contains("hk-only:");
                (description.chars().count() > 240 || has_return_shape).then(|| {
                    format!(
                        "{}: {} chars, return_shape={}",
                        tool.name,
                        description.chars().count(),
                        has_return_shape
                    )
                })
            })
            .collect();

        assert!(
            offenders.is_empty(),
            "typed-output tool descriptions should stay under 240 chars and avoid duplicated return field lists:\n{}",
            offenders.join("\n")
        );
    }

    #[test]
    fn compact_tool_description_does_not_cut_inside_abbreviations() {
        let valuation = super::list_tools()
            .into_iter()
            .find(|tool| tool.name == "valuation_comparison")
            .expect("valuation_comparison tool must be registered");
        let description = valuation.description.as_deref().unwrap_or_default();

        assert!(
            !description.ends_with("e.g."),
            "description should not be truncated immediately after an abbreviation: {description}"
        );
        assert!(
            !description.ends_with("e.g"),
            "description should not be truncated inside an abbreviation: {description}"
        );
    }

    #[test]
    fn output_schema_resources_expose_full_lb_schema_documents() {
        let resources = super::output_schema_resources();
        let depth_resource = resources
            .iter()
            .find(|resource| resource.raw.uri == "lb://tools/depth/output-schema")
            .expect("depth output schema resource must be listed");

        assert_eq!(depth_resource.raw.name, "depth.output_schema");
        assert_eq!(
            depth_resource.raw.mime_type.as_deref(),
            Some("application/schema+json")
        );

        let result = super::read_output_schema_resource("lb://tools/depth/output-schema")
            .expect("depth output schema resource must be readable");
        let [
            rmcp::model::ResourceContents::TextResourceContents {
                uri,
                mime_type,
                text,
                ..
            },
        ] = result.contents.as_slice()
        else {
            panic!("expected one text resource content");
        };

        assert_eq!(uri, "lb://tools/depth/output-schema");
        assert_eq!(mime_type.as_deref(), Some("application/schema+json"));
        let schema: serde_json::Value =
            serde_json::from_str(text.as_str()).expect("resource text must be JSON schema");
        assert!(
            schema_contains_key(&schema, "$schema"),
            "resource should keep full schema metadata"
        );
        assert!(
            schema_contains_key(&schema, "description"),
            "resource should keep field descriptions"
        );
        assert!(
            schema.get("properties").is_some(),
            "resource should keep validation structure"
        );
    }

    #[test]
    fn unknown_output_schema_resource_returns_not_found() {
        let err = super::read_output_schema_resource("lb://tools/not_a_tool/output-schema")
            .expect_err("unknown output schema resource must fail");

        assert_eq!(err.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
    }

    #[test]
    fn server_info_declares_tools_and_resources_capabilities() {
        let info = <super::Longbridge as rmcp::ServerHandler>::get_info(&super::Longbridge);

        assert!(
            info.capabilities.tools.is_some(),
            "tool capability must remain advertised"
        );
        assert!(
            info.capabilities.resources.is_some(),
            "lb:// schema documents require the resources capability"
        );
    }

    /// Measures the speedup of the cached tool list vs. the old rebuild-on-every-call path.
    /// Also benchmarks the actual production hot path (`tools_main_endpoint().to_vec()`).
    ///
    /// Run with:
    ///   cargo test bench_list_tools_speedup -- --nocapture --ignored
    #[test]
    #[ignore]
    fn bench_list_tools_speedup() {
        use super::{Longbridge, list_tools, strip_null_from_type_arrays, tools_main_endpoint};
        use std::hint::black_box;
        use std::time::Instant;

        let n = 500u32;

        // Warm up all OnceLock caches (ROUTER → TOOLS → MAIN).
        for _ in 0..10 {
            let _ = list_tools();
            let _ = tools_main_endpoint();
        }

        // ── Production hot path: tools_main_endpoint().to_vec() ─────────────
        let start = Instant::now();
        for _ in 0..n {
            let _ = black_box(tools_main_endpoint().to_vec());
        }
        let hot_elapsed = start.elapsed();

        // ── list_tools() path: all_tools_cached().to_vec() (all 151 tools) ──
        let start = Instant::now();
        for _ in 0..n {
            let _ = black_box(list_tools());
        }
        let cached_elapsed = start.elapsed();

        // ── Rebuild path (old behaviour: build router + traverse every schema) ─
        let start = Instant::now();
        for _ in 0..n {
            let _ = black_box(
                Longbridge::tool_router()
                    .list_all()
                    .into_iter()
                    .map(|mut tool| {
                        let mut schema = serde_json::Value::Object((*tool.input_schema).clone());
                        strip_null_from_type_arrays(&mut schema);
                        if let serde_json::Value::Object(obj) = schema {
                            tool.input_schema = std::sync::Arc::new(obj);
                        }
                        tool
                    })
                    .collect::<Vec<_>>(),
            );
        }
        let rebuild_elapsed = start.elapsed();

        let hot_us = hot_elapsed.as_micros() as f64 / n as f64;
        let cached_us = cached_elapsed.as_micros() as f64 / n as f64;
        let rebuild_us = rebuild_elapsed.as_micros() as f64 / n as f64;

        eprintln!(
            "\nhot path (tools_main_endpoint): {:>8.1} µs/call  ({n} calls, {:?} total)",
            hot_us, hot_elapsed
        );
        eprintln!(
            "list_tools (all 151 tools):     {:>8.1} µs/call  ({n} calls, {:?} total)",
            cached_us, cached_elapsed
        );
        eprintln!(
            "rebuild path (old behaviour):   {:>8.1} µs/call  ({n} calls, {:?} total)",
            rebuild_us, rebuild_elapsed
        );
        eprintln!("speedup (hot vs rebuild): {:.1}×", rebuild_us / hot_us);

        assert!(
            hot_us * 5.0 < rebuild_us,
            "expected hot path ({hot_us:.1}µs) to be at least 5× faster \
             than rebuild ({rebuild_us:.1}µs)"
        );
    }
}
