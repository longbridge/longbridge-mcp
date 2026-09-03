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
use crate::tools::support::text::{clip_chars, truncate_chars};

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

/// Cheap per-call correlation id for [`measured_tool_call`]'s log lines.
/// Not a request id — `rmcp`'s `RequestContext::id` isn't in scope this deep
/// without threading it through every one of the ~110 `#[tool]` call sites —
/// just enough to pair up one call's classification and detail log lines
/// when another concurrent call to the same tool interleaves in the stream.
fn next_call_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

async fn measured_tool_call<F, Fut>(
    name: &'static str,
    params: String,
    f: F,
) -> Result<CallToolResult, McpError>
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
            // Failures only — this is the choke point every `#[tool]` method
            // routes through via `measured_tool_call(name, ...)`, so it's
            // cheaper to log here once than at each of the 100+ call sites.
            // Success volume (rate, latency) is already covered by the metric
            // just above; a log line per successful call would just duplicate
            // that at far higher, harder-to-query volume for a hosted,
            // multi-tenant server.
            if let Err(err) = &result {
                let code = err.code.0;
                let elapsed_ms = (duration * 1000.0) as u64;
                // `tool` name alone isn't unique on a hosted, multi-tenant
                // server — two concurrent calls to the same tool can each
                // fail and interleave their log lines. `call_id` is the only
                // field shared between this call's classification line and
                // its detail line below, so a reader can still pair them up.
                let call_id = next_call_id();
                // A bad-argument call (typo'd symbol, malformed date — the
                // kind rmcp's own arg deserialization doesn't catch, so it
                // reaches this closure and comes back as INVALID_PARAMS) is a
                // routine, expected event on a hosted multi-tenant server,
                // not an incident. Logging it at the same severity as a real
                // backend failure would drown WARN-based alerting in normal
                // user typos; downgrade that class — and its detail line
                // below, so enabling payload logging doesn't quietly bring
                // WARN-level noise back — to INFO, and keep WARN for
                // everything else (auth, upstream, internal).
                let routine = err.code == rmcp::model::ErrorCode::INVALID_PARAMS;
                // The message text is our own short, structured error string
                // on almost every path; `src/error.rs` sanitizes the one
                // `longbridge` SDK variant that would otherwise embed a raw
                // upstream HTTP response body verbatim (both here and in
                // `tool_error`'s client-facing text below). `params` is the
                // caller's own input (a symbol, an order quantity, ...) —
                // not upstream content, but still account-identifying enough
                // to withhold by default. This target stays capped off by
                // default (`logging.rs`'s `PAYLOAD_CAPS`) as a second line of
                // defense for any error path that sanitizer doesn't cover.
                // `truncate_chars(...)` is inlined into the macro call, not
                // pre-bound to a local, so it's only evaluated when the
                // target is actually enabled.
                if routine {
                    tracing::info!(tool = name, call_id, elapsed_ms, code, "tool call rejected");
                    tracing::info!(
                        target: "longbridge_mcp::tools::error_detail",
                        tool = name,
                        call_id,
                        params = %truncate_chars(&params, 500),
                        error = %truncate_chars(err.message.as_ref(), 300),
                        "tool call error detail"
                    );
                } else {
                    tracing::warn!(tool = name, call_id, elapsed_ms, code, "tool call failed");
                    tracing::warn!(
                        target: "longbridge_mcp::tools::error_detail",
                        tool = name,
                        call_id,
                        params = %truncate_chars(&params, 500),
                        error = %truncate_chars(err.message.as_ref(), 300),
                        "tool call error detail"
                    );
                }
            }
            // The request was routed and the tool ran, so a failure here is a
            // tool-level error, not a protocol one. MCP clients render protocol
            // errors opaquely ("tool result missing due to internal error"),
            // which hides the reason from the model and denies it any chance to
            // correct the call. Returning `isError` puts the message in front of
            // the model instead. Protocol errors stay with the framework, which
            // rejects unroutable calls (unknown tool, unparseable arguments)
            // before this wrapper is ever reached.
            Ok(result.unwrap_or_else(|err| tool_error(name, &err)))
        })
        .await
}

/// Render a failed tool call as a caller-visible structured envelope.
///
/// Non-terminal errors return an `isError` JSON envelope
/// (`error_code`/`message`/`recoverable`/`hint`/`data`) so the consumer can act
/// on `recoverable`. `structured_content` is left unset on this path: a tool that
/// declares an `outputSchema` must not return structured content that fails to
/// match it, and an error payload never does.
///
/// The two known *terminal* quote conditions (301604 no-access, 301603 no-quotes)
/// instead return a schema-valid `isError:false` success so they are not counted
/// as tool-result errors, with a `note` making clear the empty payload is a
/// permission/no-data placeholder — not real quote data.
fn tool_error(name: &str, err: &McpError) -> CallToolResult {
    if is_terminal_none(err) {
        let envelope = serde_json::json!({
            "error_code": openapi_error_code_of(err),
            "message": err.message.as_ref(),
            "recoverable": "none",
            "hint": error_hint(err),
            "note": "Empty placeholder for a no-access / no-data condition — NOT real quote data.",
        });
        let mut result = CallToolResult::success(vec![Content::text(envelope.to_string())]);
        if TERMINAL_OBJECT_ROOTED.contains(&name)
            && let Some(schema) = output_schema_map().get(name)
        {
            result.structured_content = Some(minimal_valid_instance(schema));
        }
        return result;
    }

    let envelope = serde_json::json!({
        "error_code": openapi_error_code_of(err),
        "message": err.message.as_ref(),
        "recoverable": recoverable_of(err),
        "hint": error_hint(err),
        "data": serde_json::Value::Null,
    });
    CallToolResult::error(vec![Content::text(envelope.to_string())])
}

/// Business error codes matched structurally in `error_hint()` — see
/// `Error::openapi_error_code()`, which populates `McpError::data` with
/// `{"openapi_error_code": ...}` for any error that wraps a `longbridge`
/// business error. Preferred over string-matching the display text, which
/// can misfire on an unrelated field (trace id, order id) that happens to
/// contain the same digits.
const NO_QUOTE_ACCESS_CODE: i64 = 301604;

/// The structured `openapi_error_code` from `McpError::data`, when present.
fn openapi_error_code_of(err: &McpError) -> Option<i64> {
    err.data.as_ref()?.get("openapi_error_code")?.as_i64()
}

/// Needles for [`matches_error_class`]. A named struct (rather than two
/// adjacent `&[&str]` parameters) so a future call site can't silently swap
/// `text`/`numeric` — a transposed pair of same-typed positional args would
/// still compile, quietly reintroducing the numeric-substring false-positive
/// risk this whole mechanism exists to avoid.
struct ErrorClassNeedles<'a> {
    /// Descriptive phrases, safe to match unconditionally: these don't
    /// plausibly appear as substrings of an unrelated field (trace id,
    /// order id) the way a bare 3-6 digit numeric needle can.
    text: &'a [&'a str],
    /// Bare numeric substrings — only matched when no structured code is
    /// present at all, since once we *have* a code, a coincidental digit
    /// match elsewhere in the text is more likely to be a false positive
    /// than a code we simply haven't enumerated.
    numeric: &'a [&'a str],
}

/// True if `err` matches a known error class, either by a structured business
/// code (authoritative — `known_code` decides, e.g. a numeric-range check
/// for a whole code family, not just individually enumerated values) or by
/// `needles` in the display text.
fn matches_error_class(
    code: Option<i64>,
    msg: &str,
    known_code: impl Fn(i64) -> bool,
    needles: ErrorClassNeedles<'_>,
) -> bool {
    code.is_some_and(known_code)
        || needles.text.iter().any(|needle| msg.contains(needle))
        || (code.is_none() && needles.numeric.iter().any(|needle| msg.contains(needle)))
}

/// Actionable follow-up for the error classes users hit most, so the model can
/// either fix the call itself or tell the user what to do.
fn error_hint(err: &McpError) -> Option<&'static str> {
    if err.code == rmcp::model::ErrorCode::INVALID_PARAMS {
        return Some(
            "Hint: check the argument values against this tool's input schema, then retry.",
        );
    }

    let code = openapi_error_code_of(err);
    let msg = err.message.to_lowercase();

    // Checked before the rate-limit class below: `openapi_error_code()` and
    // `dc_region_restricted()` are mutually exclusive on any given
    // `longbridge::Error`, so a DC-region-restricted error always has
    // `code == None` — if this ran after the rate-limit check, its message
    // text (a route path) would be exposed to that check's numeric-needle
    // fallback before this authoritative structured signal gets a chance.
    let is_dc_region_restricted = err
        .data
        .as_ref()
        .is_some_and(|d| d.get("dc_region_restricted").is_some())
        || ["data center", "dcregionrestricted"]
            .iter()
            .any(|needle| msg.contains(needle));
    if is_dc_region_restricted {
        return Some(
            "Hint: this tool is restricted to accounts in a specific Longbridge data center \
             (US vs. AP/HK). It cannot succeed for this account regardless of retries or \
             arguments — use the equivalent tool for the account's own region instead, or tell \
             the user this data isn't available for their account.",
        );
    }
    if matches_error_class(
        code,
        &msg,
        // Observed rate-limit codes (429002, 429003) share the HTTP
        // 429-Too-Many-Requests prefix also seen elsewhere in this
        // codebase's other business-code families (401xxx/403xxx) — a
        // range check covers sibling codes in the same family that
        // haven't been individually enumerated, instead of requiring an
        // exact-match list that's one upstream addition away from stale.
        |c| (429_000..430_000).contains(&c),
        ErrorClassNeedles {
            text: &["rate limit", "区间调用上限", "最小间隔"],
            numeric: &["429002", "429003"],
        },
    ) {
        return Some(
            "Hint: this call was rate-limited by the upstream API. Wait a moment and retry — \
             if this recurs, space out repeated calls rather than firing them back-to-back.",
        );
    }
    if matches_error_class(
        code,
        &msg,
        |c| c == NO_QUOTE_ACCESS_CODE,
        ErrorClassNeedles {
            text: &["no quote access"],
            numeric: &["301604"],
        },
    ) {
        return Some(
            "Hint: the account lacks a market data subscription/permission for this \
             symbol's market. Retrying won't help — tell the user they need to subscribe to \
             the relevant market data package.",
        );
    }
    if matches_error_class(
        code,
        &msg,
        |c| matches!(c, 301_607 | 701_007),
        ErrorClassNeedles {
            text: &["too many symbols", "exceed_name_length"],
            numeric: &["301607", "701007"],
        },
    ) {
        return Some(
            "Hint: the request exceeded a size limit (too many symbols in one call, or a name \
             that is too long). Retry with fewer symbols per call, or a shorter name.",
        );
    }
    if matches_error_class(
        code,
        &msg,
        |c| (403_000..404_000).contains(&c),
        ErrorClassNeedles {
            text: &["permission", "forbidden", "not authorized", "scope"],
            numeric: &["403"],
        },
    ) {
        return Some(
            "Hint: this is a permission/scope error. The OAuth authorization does not include the \
             scope this API needs. A plain token refresh returns the same scopes and will not \
             help — ask the user to reconnect this MCP server and re-authorize, approving the \
             full set of permissions (watchlist, portfolio, and trading scopes).",
        );
    }
    if matches_error_class(
        code,
        &msg,
        |c| (401_000..402_000).contains(&c),
        ErrorClassNeedles {
            text: &["unauthorized", "token", "expired"],
            numeric: &["401"],
        },
    ) {
        return Some(
            "Hint: the access token is missing, expired, or invalid. Ask the user to reconnect \
             this MCP server to re-authorize, granting the full set of permissions.",
        );
    }
    None
}

/// Classify a failed call by what the caller should do about it: one of
/// `"reauth"`, `"backoff"`, `"fix_params"`, or `"none"`. Defaults to `"none"`
/// so an unrecognized error is never optimistically retried. Matches on the
/// structured business code first, then message-text needles (the codes behind
/// most real traffic — 401103/403308/429003 — are undocumented, so text is a
/// necessary fallback).
fn recoverable_of(err: &McpError) -> &'static str {
    if err.code == rmcp::model::ErrorCode::INVALID_PARAMS {
        return "fix_params";
    }
    let code = openapi_error_code_of(err);
    let msg = err.message.to_lowercase();

    if matches_error_class(
        code,
        &msg,
        |c| (401_000..402_000).contains(&c) || c == 403_308,
        ErrorClassNeedles {
            text: &[
                "token is expired",
                "token verification failed",
                "not in authorized scopes",
                "unauthorized",
            ],
            numeric: &["401103", "401102", "401003", "403308"],
        },
    ) {
        return "reauth";
    }
    if matches_error_class(
        code,
        &msg,
        |c| {
            (429_000..430_000).contains(&c)
                || matches!(c, 500 | 500_000 | 2_301_500 | 2_601_500 | 202_203 | 408)
        },
        ErrorClassNeedles {
            text: &["rate limit", "too frequent", "区间调用上限", "最小间隔"],
            numeric: &["429002", "429003"],
        },
    ) {
        return "backoff";
    }
    if matches_error_class(
        code,
        &msg,
        |c| matches!(c, 400 | 301_600 | 301_607 | 701_007),
        ErrorClassNeedles {
            text: &[
                "too many symbols",
                "invalid request",
                "syntax error",
                "exceed_name_length",
            ],
            numeric: &["301607", "301600", "701007"],
        },
    ) {
        return "fix_params";
    }
    "none"
}

/// True only for the known *terminal* quote conditions that return
/// `isError:false` with a schema-valid empty result: 301604 (no quote access)
/// and 301603 (no quotes). Deliberately narrow — a bare "no access" needle is
/// omitted so a 403 permission error can't be mistaken for a terminal quote
/// condition.
fn is_terminal_none(err: &McpError) -> bool {
    let code = openapi_error_code_of(err);
    let msg = err.message.to_lowercase();
    matches_error_class(
        code,
        &msg,
        |c| matches!(c, 301_604 | 301_603),
        ErrorClassNeedles {
            text: &["no quote access", "no quotes"],
            numeric: &["301604", "301603"],
        },
    )
}

mod alert;
mod atm;
mod authenticate;
mod calendar;
mod content;
mod dca;
mod fundamental;
mod grid;
mod ipo;
mod macrodata;
mod market;
mod output;
mod portfolio;
mod quant;
mod quote;
mod screener;
mod search;
mod sharelist;
mod signal;
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

/// A JSON value that satisfies `schema` by filling every `required` property
/// with a type-appropriate zero (`string`→`""`, `integer`/`number`→`0`,
/// `boolean`→`false`, `array`→`[]`, `object`→recurse). If a property declares an
/// `enum`, its first member is used. Used to return a schema-conforming empty
/// result for the terminal `isError:false` path (see `tool_error`).
fn minimal_valid_instance(schema: &rmcp::model::JsonObject) -> serde_json::Value {
    let required: Vec<&str> = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    let props = schema
        .get("properties")
        .and_then(serde_json::Value::as_object);
    let mut obj = serde_json::Map::new();
    for key in required {
        let prop = props
            .and_then(|p| p.get(key))
            .and_then(serde_json::Value::as_object);
        obj.insert(key.to_string(), zero_for(prop));
    }
    serde_json::Value::Object(obj)
}

/// Zero value for a single property schema. See [`minimal_valid_instance`].
fn zero_for(schema: Option<&rmcp::model::JsonObject>) -> serde_json::Value {
    let Some(s) = schema else {
        return serde_json::Value::Null;
    };
    if let Some(first) = s
        .get("enum")
        .and_then(serde_json::Value::as_array)
        .and_then(|a| a.first())
    {
        return first.clone();
    }
    let ty = s.get("type").and_then(|t| match t {
        serde_json::Value::String(s) => Some(s.as_str()),
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(serde_json::Value::as_str)
            .find(|s| *s != "null"),
        _ => None,
    });
    match ty {
        Some("string") => serde_json::Value::String(String::new()),
        Some("integer") | Some("number") => serde_json::Value::Number(0.into()),
        Some("boolean") => serde_json::Value::Bool(false),
        Some("array") => serde_json::Value::Array(Vec::new()),
        Some("object") => minimal_valid_instance(s),
        _ => serde_json::Value::Null,
    }
}

/// `tool name -> full output schema`, built once from the uncompacted tool list
/// so nested `properties`/`required` survive. Backs the terminal `isError:false`
/// path (see `tool_error`).
fn output_schema_map()
-> &'static std::collections::HashMap<String, std::sync::Arc<rmcp::model::JsonObject>> {
    static MAP: std::sync::OnceLock<
        std::collections::HashMap<String, std::sync::Arc<rmcp::model::JsonObject>>,
    > = std::sync::OnceLock::new();
    MAP.get_or_init(|| {
        all_tools_full_cached()
            .iter()
            .filter_map(|t| {
                t.output_schema
                    .as_ref()
                    .map(|s| (t.name.to_string(), s.clone()))
            })
            .collect()
    })
}

/// The object-rooted market-data tools whose terminal (301604/301603) result
/// must carry schema-valid `structuredContent`: they return an object AND
/// declare an `output_schema`. Array-rooted quote tools are excluded — their
/// normal success leaves `structuredContent` unset (MCP requires it to be an
/// object), so their terminal result matches by also leaving it unset. Other
/// object-rooted quote tools (static_info/intraday/capital_flow/calc_indexes)
/// declare no `output_schema`, so they have no structured-content contract and
/// likewise leave it unset.
const TERMINAL_OBJECT_ROOTED: &[&str] = &["depth", "capital_distribution"];

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

/// Global-gateway endpoints, pinned for US-data-center tokens.
///
/// # Which access point can serve which data center
///
/// Every Longbridge credential carries its data center as a prefix: `us_…` for
/// the US data center, `ap_…` (or unprefixed) for Asia-Pacific. That prefix
/// decides which access point can serve it:
///
/// | Data center | `.com` | `.cn` |
/// |-------------|--------|-------|
/// | `us`        | yes — the only usable access point | no |
/// | `ap`        | yes    | yes   |
///
/// `.cn` has no path to the US data center. This is a hard constraint, not a
/// latency or preference question.
///
/// # Why this is pinned
///
/// Left unset, the SDK picks an access point by geolocation at request time,
/// which resolves to `.cn` on a China Mainland network. A US token sent to `.cn`
/// still authenticates — the WebSocket connects and basic calls such as
/// `static_info` succeed — but every market-data request comes back
/// `301604 no quote access`, because `.cn` cannot source US-account quotes. The
/// failure reads like a missing permission and is not one, so pin the endpoints
/// rather than letting geolocation decide.
///
/// AP tokens are deliberately left to geolocation: both access points serve
/// them, so the nearer one is the right choice.
const US_HTTP_URL: &str = "https://openapi.longbridge.com";
const US_QUOTE_WS_URL: &str = "wss://openapi-quote.longbridge.com/v2";
const US_TRADE_WS_URL: &str = "wss://openapi-trade.longbridge.com/v2";

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

    /// Whether this request's token belongs to the US data center and so needs
    /// the global gateway pinned instead of geotest-selected endpoints.
    ///
    /// `LONGBRIDGE_HTTP_URL` takes precedence when set, so tests and local mock
    /// servers can still redirect the SDK.
    fn pin_us_endpoints(&self) -> bool {
        std::env::var("LONGBRIDGE_HTTP_URL").is_err()
            && longbridge::DcRegion::from_credential(&self.token) == longbridge::DcRegion::Us
    }

    pub fn create_config(&self) -> Arc<longbridge::Config> {
        let mut config =
            longbridge::Config::from_oauth(longbridge::oauth::OAuth::from_token(&self.token))
                .dont_print_quote_packages()
                .enable_overnight()
                // Identify MCP-originated requests on the Context path (REST and
                // WebSocket upgrades), mirroring how longbridge-cli tags itself.
                .header("user-agent", self.user_agent());
        if self.pin_us_endpoints() {
            config = config
                .http_url(US_HTTP_URL)
                .quote_ws_url(US_QUOTE_WS_URL)
                .trade_ws_url(US_TRADE_WS_URL);
        }
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
        let mut http_config = longbridge::httpclient::HttpClientConfig::from_oauth(
            longbridge::oauth::OAuth::from_token(&self.token),
        );
        // Same US pinning as `create_config`, so REST calls do not drift to the
        // CN node while the WebSocket is pinned to the global one.
        if self.pin_us_endpoints() {
            http_config = http_config.http_url(US_HTTP_URL);
        }
        let mut client = longbridge::httpclient::HttpClient::new(http_config);
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

    /// The DC region (`Us` or `Ap`) this session's credentials resolve to,
    /// derived from the bearer token — no network round-trip for token-based
    /// sessions (see `longbridge_httpcli::DcRegion::from_credential`).
    pub async fn dc_region(&self) -> longbridge::DcRegion {
        self.create_http_client().dc_region().await
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
    // Alibaba Cloud ALB appends this header to detect routing loops. In the CN
    // deployment, both mcp.longbridge.cn and openapi.longbridge.cn enter through
    // the same public ALB. Forwarding the trace received by MCP back to OpenAPI
    // therefore sends an ALB-generated rule trace through that ALB a second
    // time. A repeated rule ID, or a trace chain over ALB's limit, is rejected
    // at the load balancer with HTTP 463 before the request reaches OpenAPI.
    // This is hop-specific routing metadata and must never cross the MCP-to-
    // OpenAPI boundary.
    "alicloud-alb-trace",
    // Captured separately in `extract_context` and folded into the synthesized
    // upstream User-Agent; never forwarded raw.
    "user-agent",
];

const MAX_FORWARDED_FOR_ADDRESSES: usize = 10;

fn collect_headers(headers: &axum::http::HeaderMap) -> Vec<(String, String)> {
    let mut forwarded = Vec::new();
    let mut forwarded_for = Vec::new();

    for (name, value) in headers {
        let key = name.as_str().to_lowercase();
        if SKIP_FORWARD_HEADERS.contains(&key.as_str()) {
            continue;
        }
        let Ok(value) = value.to_str() else {
            continue;
        };
        if key == "x-forwarded-for" {
            forwarded_for.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|address| !address.is_empty())
                    .take(MAX_FORWARDED_FOR_ADDRESSES - forwarded_for.len())
                    .map(str::to_owned),
            );
        } else {
            forwarded.push((key, value.to_string()));
        }
    }

    if !forwarded_for.is_empty() {
        forwarded.push(("x-forwarded-for".to_string(), forwarded_for.join(", ")));
    }
    forwarded
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
/// [`RestrictedVersion`]) for every request that proceeds on `/v2`.
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
        RestrictedVersion::V2 => is_v2_public_tool(name),
    }
}

/// Tools whose underlying SDK method is US-DC-only (calls
/// `.dc_restrict(DcRegion::Us)` internally, confirmed against the SDK
/// source) — hidden from accounts in the AP region. This is a completely
/// separate mechanism from the `/v1`/`/v2` restricted-endpoint allowlists
/// above; it applies only on the main (`/mcp`) and authenticated `/agent`
/// endpoints and never touches `TOOL_ENDPOINTS`/`is_v2_public_tool`.
const US_ONLY_TOOLS: &[&str] = &[
    "profit_analysis_realized",
    "financial_report_key_metrics",
    "etf_docs",
];

/// Tools whose underlying SDK method is AP-DC-only (calls
/// `.dc_restrict(DcRegion::Ap)` / checks `DcRegion::Ap` internally, confirmed
/// against the SDK source: broker queue/holding data and the entire
/// recurring-investment (DCA) API) — hidden from accounts in the US region.
const AP_ONLY_TOOLS: &[&str] = &[
    "brokers",
    "broker_holding",
    "broker_holding_detail",
    "broker_holding_daily",
    "operating",
    "dca_list",
    "dca_create",
    "dca_update",
    "dca_pause",
    "dca_resume",
    "dca_stop",
    "dca_history",
    "dca_stats",
    "dca_check",
];

/// True when `name` is restricted to the DC region opposite `region` — i.e.
/// it should be hidden from `tools/list` and rejected by `call_tool` for an
/// account in `region`.
fn is_hidden_for_dc_region(name: &str, region: longbridge::DcRegion) -> bool {
    match region {
        longbridge::DcRegion::Us => AP_ONLY_TOOLS.contains(&name),
        longbridge::DcRegion::Ap => US_ONLY_TOOLS.contains(&name),
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
/// Building the router (152 entries) and recursively traversing every JSON Schema
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
const V2: u8 = 1 << 0;

/// Single source of truth for restricted-endpoint membership of every
/// registered tool — the per-tool version annotation.
///
/// `/v2` is a **whitelist**: a tool is exposed only when its flags include the
/// matching bit. This is deliberately **default-out** — a tool absent from this
/// table (or marked `0`) appears on no restricted endpoint, so a newly added
/// tool can never silently leak onto a directory-submitted surface.
///
/// - `/v2` — read-only market/fundamental/research analysis plus read-only
///   account/portfolio, read-only order/execution history, IPO market data,
///   watchlist, alerts, sharelist, and community tools. `/v2` still **never**
///   exposes trade execution (`submit_order`/`cancel_order`/`replace_order`),
///   DCA automation, IPO order management (`ipo_orders`/`ipo_order_detail`/
///   `ipo_profit_loss`), or money movement (`deposits`/`withdrawals`/
///   `bank_cards`).
///
/// Invariants enforced by unit tests: every name is live, every live tool is
/// listed exactly once, and the v2 set is disjoint from the trade-write / DCA /
/// IPO-order / money-movement tools.
const TOOL_ENDPOINTS: &[(&str, u8)] = &[
    // v2 — read-only market / fundamental / research analysis.
    ("business_segments", V2),
    ("calc_indexes", V2),
    ("candlesticks", V2),
    ("company", V2),
    ("consensus", V2),
    ("depth", V2),
    ("dividend", V2),
    ("filings", V2),
    ("finance_calendar", V2),
    ("financial_report_latest", V2),
    ("financial_statement", V2),
    ("forecast_eps", V2),
    ("institution_rating", V2),
    ("intraday", V2),
    ("market_status", V2),
    ("news", V2),
    ("news_detail", V2),
    ("news_search", V2),
    ("quote", V2),
    ("rank_list", V2),
    ("screener_search", V2),
    ("shareholder_top", V2),
    ("short_positions", V2),
    ("top_movers", V2),
    ("trades", V2),
    ("valuation", V2),
    ("watchlist", V2),
    // v2 — broader read surface plus non-trade write tools (watchlist,
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
    ("etf_docs", V2),
    ("exchange_rate", V2),
    ("executive", V2),
    ("financial_report", V2),
    ("financial_report_key_metrics", V2),
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
    ("macrodata", V2),
    ("macrodata_indicators", V2),
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
    ("profit_analysis_realized", V2),
    ("quant_run", V2),
    ("rank_categories", V2),
    ("screener_indicators", V2),
    ("screener_recommend_strategies", V2),
    ("screener_strategy", V2),
    ("screener_user_strategies", V2),
    ("security_facts", V2),
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
    ("signal_detail", V2),
    ("signals", V2),
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
    ("grid_cancel", 0),
    ("grid_detail", 0),
    ("grid_list", 0),
    ("grid_list_by_ids", 0),
    ("grid_questionnaire", 0),
    ("grid_replace", 0),
    ("grid_restart", 0),
    ("grid_submit", 0),
    ("grid_suspend", 0),
    ("grid_symbol_info", 0),
    ("grid_trigger_history", 0),
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

/// Whether `name` is exposed on the restricted public `/v2` endpoint.
fn is_v2_public_tool(name: &str) -> bool {
    endpoint_flags(name) & V2 != 0
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
            for (key, v) in map.iter_mut() {
                // The values of these keywords are maps keyed by *names*
                // (property / definition names), not schema objects — a
                // property legitimately named "title" or "description" must
                // not be stripped. Recurse into each named child schema
                // directly instead.
                if matches!(
                    key.as_str(),
                    "properties" | "patternProperties" | "$defs" | "definitions"
                ) && let serde_json::Value::Object(children) = v
                {
                    for child in children.values_mut() {
                        strip_schema_documentation_keys(child);
                    }
                    continue;
                }
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

    let clipped = clip_chars(trimmed, max_chars.saturating_sub(3));
    let clipped =
        clipped.trim_end_matches(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == ':');
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
    CashFlowParam, EstimateMaxQtyParam, HistoryOrdersParam, OrderIdParam, ReplaceOrderParam,
    SubmitOrderParam,
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
        measured_tool_call(AUTHENTICATE_TOOL_NAME, format!("{p:?}"), || {
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
        description = "Get static info for securities. Returns per symbol: symbol, name_cn, name_en, exchange (e.g. NASDAQ), type (e.g. US_Stock), lot_size, listed_date, delisted (bool). US accounts only: .BKKT crypto symbols (e.g. BTCUSD.BKKT) are routed to a separate US crypto overview endpoint; .HAS/.OSL crypto symbols are unaffected."
    )]
    async fn static_info(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("static_info", format!("{p:?}"), || {
            quote::static_info(&mctx, p)
        })
        .await
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
        measured_tool_call("quote", format!("{p:?}"), || quote::quote(&mctx, p)).await
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
        description = "Get option quotes (max 500 symbols). Symbols must be option contract symbols (e.g. \"AAPL230317P160000.US\"), NOT plain stock symbols — obtain valid ones from option_chain_info_by_date's call.symbol/put.symbol fields. Returns last_done, prev_close, open, high, low, volume, turnover, implied_volatility, delta, gamma, theta, vega, rho, open_interest per symbol."
    )]
    async fn option_quote(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<quote::OptionSymbolsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("option_quote", format!("{p:?}"), || {
            quote::option_quote(&mctx, p)
        })
        .await
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
        measured_tool_call("warrant_quote", format!("{p:?}"), || {
            quote::warrant_quote(&mctx, p)
        })
        .await
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
        measured_tool_call("depth", format!("{p:?}"), || quote::depth(&mctx, p)).await
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
        measured_tool_call("brokers", format!("{p:?}"), || quote::brokers(&mctx, p)).await
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
        measured_tool_call("participants", String::new(), || quote::participants(&mctx)).await
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
        measured_tool_call("trades", format!("{p:?}"), || quote::trades(&mctx, p)).await
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
        measured_tool_call("intraday", format!("{p:?}"), || quote::intraday(&mctx, p)).await
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
        description = "Get candlestick data (OHLCV). Only symbol is required; period defaults to day, count to 100 (max 1000), forward_adjust to false, trade_sessions to all. period: 1m/5m/15m/30m/60m/day/week/month/year. trade_sessions: intraday/all. If the account's entitlement caps out below the requested count, this returns as many candles as allowed instead of erroring — check the returned array length against count if an exact number matters."
    )]
    async fn candlesticks(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<CandlesticksParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("candlesticks", format!("{p:?}"), || {
            quote::candlesticks(&mctx, p)
        })
        .await
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
        description = "Get historical candlestick data by offset from a reference time. Only symbol is required; period defaults to day (1m/5m/15m/30m/60m/day/week/month/year), count to 100, forward_adjust/forward to false, trade_sessions to all. If the account's entitlement caps out below the requested count, this returns as many candles as allowed instead of erroring — check the returned array length against count if an exact number matters."
    )]
    async fn history_candlesticks_by_offset(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryCandlesticksByOffsetParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_candlesticks_by_offset", format!("{p:?}"), || {
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
        description = "Get historical candlestick data by date range. Only symbol is required; period defaults to day (1m/5m/15m/30m/60m/day/week/month/year), forward_adjust to false, trade_sessions to all."
    )]
    async fn history_candlesticks_by_date(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryCandlesticksByDateParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_candlesticks_by_date", format!("{p:?}"), || {
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
        measured_tool_call("trading_days", format!("{p:?}"), || {
            quote::trading_days(&mctx, p)
        })
        .await
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
        measured_tool_call("option_chain_expiry_date_list", format!("{p:?}"), || {
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
        measured_tool_call("option_chain_info_by_date", format!("{p:?}"), || {
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
        measured_tool_call("capital_flow", format!("{p:?}"), || {
            quote::capital_flow(&mctx, p)
        })
        .await
    }

    /// Get capital distribution.
    #[tool(
        title = "Capital Distribution",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::CapitalDistributionResponse>(),
        description = "Get capital distribution for a symbol. Returns {timestamp, capital_in{large, medium, small}, capital_out{large, medium, small}, data_available} (decimal strings in settlement currency). data_available is false for symbols with no capital-flow data (e.g. indices) — the other fields are still present but meaningless zeros in that case."
    )]
    async fn capital_distribution(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("capital_distribution", format!("{p:?}"), || {
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
        measured_tool_call("trading_session", String::new(), || {
            quote::trading_session(&mctx)
        })
        .await
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
        measured_tool_call("market_temperature", format!("{p:?}"), || {
            quote::market_temperature(&mctx, p)
        })
        .await
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
        measured_tool_call("history_market_temperature", format!("{p:?}"), || {
            quote::history_market_temperature(&mctx, p)
        })
        .await
    }

    /// List all supported macro-economic indicators.
    #[tool(
        title = "Macro Indicator List",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::macrodata::MacroeconomicIndicatorsResponse>(),
        description = "List macro-economic indicators. Filter by keyword and country (US/CN/HK/EU/JP/SG). Use the returned indicator_code with macrodata. Supports offset/limit pagination."
    )]
    async fn macrodata_indicators(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<macrodata::MacroeconomicIndicatorsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("macrodata_indicators", format!("{p:?}"), || {
            macrodata::macrodata_indicators(&mctx, p)
        })
        .await
    }

    /// Get historical data for a macro-economic indicator.
    #[tool(
        title = "Macro Indicator Data",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::macrodata::MacroeconomicResponse>(),
        description = "Get historical observations for one macro-economic indicator. Use indicator_code from macrodata_indicators; start_date/end_date accept YYYY-MM-DD. Supports offset/limit pagination."
    )]
    async fn macrodata(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<macrodata::MacroeconomicParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("macrodata", format!("{p:?}"), || {
            macrodata::macrodata(&mctx, p)
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
        measured_tool_call("watchlist", String::new(), || quote::watchlist(&mctx)).await
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
        measured_tool_call("filings", format!("{p:?}"), || quote::filings(&mctx, p)).await
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
        measured_tool_call("warrant_issuers", String::new(), || {
            quote::warrant_issuers(&mctx)
        })
        .await
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
        measured_tool_call("warrant_list", format!("{p:?}"), || {
            quote::warrant_list(&mctx, p)
        })
        .await
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
        measured_tool_call("calc_indexes", format!("{p:?}"), || {
            quote::calc_indexes(&mctx, p)
        })
        .await
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
        measured_tool_call("create_watchlist_group", format!("{p:?}"), || {
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
        measured_tool_call("delete_watchlist_group", format!("{p:?}"), || {
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
        measured_tool_call("update_watchlist_group", format!("{p:?}"), || {
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
        measured_tool_call("security_list", format!("{p:?}"), || {
            quote::security_list(&mctx, p)
        })
        .await
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
        measured_tool_call("account_balance", format!("{p:?}"), || {
            trade::account_balance(&mctx, p)
        })
        .await
    }

    /// Get stock positions.
    #[tool(
        title = "Stock Positions",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::StockPositionsResponse>(),
        description = "Get current stock positions across all channels. Returns list[].stock_info[]{symbol, symbol_name, quantity, available_quantity, currency, cost_price, market}. US accounts only: an additional us_asset_overview field {cash_list, stock_list, option_list, crypto_list, cash_buy_power, overnight_buy_power} is included alongside the existing data."
    )]
    async fn stock_positions(
        &self,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("stock_positions", String::new(), || {
            trade::stock_positions(&mctx)
        })
        .await
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
        measured_tool_call("fund_positions", String::new(), || {
            trade::fund_positions(&mctx)
        })
        .await
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
        measured_tool_call("margin_ratio", format!("{p:?}"), || {
            trade::margin_ratio(&mctx, p)
        })
        .await
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
        description = "Get orders placed today. Returns orders[]{order_id, symbol, side, order_type, status, quantity, price, submitted_at, executed_quantity, executed_price}. Pass symbol to filter. US accounts only: us_action (Buy/Sell), us_page, us_limit filter/paginate via a separate US order endpoint."
    )]
    async fn today_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<trade::TodayOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("today_orders", format!("{p:?}"), || {
            trade::today_orders(&mctx, p)
        })
        .await
    }

    /// Get order detail.
    #[tool(
        title = "Order Detail",
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::OrderDetailResponse>(),
        description = "Get detailed information about a specific order. Returns {order_id, symbol, status, side, order_type, quantity, price, executed_quantity, executed_price, submitted_at, time_in_force, msg}."
    )]
    async fn order_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<OrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("order_detail", format!("{p:?}"), || {
            trade::order_detail(&mctx, p)
        })
        .await
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
        description = "Cancel an open order by order_id. Returns plain text \"order cancelled\" on success; errors if the order is already filled or cancelled. TWO-STEP CONFIRMATION IS MANDATORY: this tool is a DRY RUN unless you pass the confirmation_code its own dry run returned. Call it first without execute, show the returned preview to the user, and only call it again with execute=\"<confirmation_code>\" after the user has explicitly confirmed that exact order. The code is derived from the order itself, so it applies only to that exact order. Never quote it back on your own initiative, and never in the same turn the user first asks. The dry run also echoes the order being targeted so the user can verify it is the right one."
    )]
    async fn cancel_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<trade::CancelOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("cancel_order", format!("{p:?}"), || {
            trade::cancel_order(&mctx, p)
        })
        .await
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
        measured_tool_call("today_executions", format!("{p:?}"), || {
            trade::today_executions(&mctx, p)
        })
        .await
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
        description = "Get historical orders between dates (excludes today). Returns orders[]{order_id, symbol, side, status, quantity, price, submitted_at}. start_at/end_at in RFC3339. US accounts only: us_page, us_limit paginate via a separate US order endpoint (default page size 20 — pass us_page to see more than the first page)."
    )]
    async fn history_orders(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<HistoryOrdersParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("history_orders", format!("{p:?}"), || {
            trade::history_orders(&mctx, p)
        })
        .await
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
        measured_tool_call("history_executions", format!("{p:?}"), || {
            trade::history_executions(&mctx, p)
        })
        .await
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
        measured_tool_call("cash_flow", format!("{p:?}"), || trade::cash_flow(&mctx, p)).await
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
        output_schema = schema_for::<output::SubmitOrderResult>(),
        description = "Submit a buy/sell order. DRY RUN unless execute is the confirmation_code from its own dry run: call once without execute, show the preview to the user, then re-call quoting the code only after they explicitly confirm. order_type: LO (Limit) / ELO (Enhanced Limit, HK) / MO (Market) / AO (At-auction, HK) / ALO (At-auction Limit, HK) / ODD (Odd Lots, HK) / LIT (Limit If Touched) / MIT (Market If Touched) / TSLPAMT (Trailing Limit by Amount) / TSLPPCT (Trailing Limit by Percent) / SLO (Special Limit, HK). side: Buy/Sell. time_in_force: Day/GTC/GTD"
    )]
    async fn submit_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<SubmitOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("submit_order", format!("{p:?}"), || {
            trade::submit_order(&mctx, p)
        })
        .await
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
        description = "Modify an open order's quantity, price, trigger_price, or trailing params. Returns \"order replaced\" on success. Only open/pending orders can be modified. TWO-STEP CONFIRMATION IS MANDATORY: this tool is a DRY RUN unless you pass the confirmation_code its own dry run returned. Call it first without execute, show the returned preview to the user, and only call it again with execute=\"<confirmation_code>\" after the user has explicitly confirmed that exact order. The code is derived from the order itself, so it applies only to that exact order. Never quote it back on your own initiative, and never in the same turn the user first asks. The dry run echoes the current order alongside the requested change."
    )]
    async fn replace_order(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<ReplaceOrderParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("replace_order", format!("{p:?}"), || {
            trade::replace_order(&mctx, p)
        })
        .await
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
        measured_tool_call("estimate_max_purchase_quantity", format!("{p:?}"), || {
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
        output_schema = schema_for::<output::us_market::FinancialReportResponse>(),
        description = "Get financial reports (income statement, balance sheet, cash flow). kind: IS/BS/CF/ALL. report_type: af (annual), saf (semi-annual), q1/q2/q3, qf (quarterly full). US accounts querying a .US symbol without kind are routed to a US-specific overview endpoint; passing kind explicitly always uses the generic path."
    )]
    async fn financial_report(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialReportParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_report", format!("{p:?}"), || {
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
        measured_tool_call("institution_rating", format!("{p:?}"), || {
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
        measured_tool_call("institution_rating_detail", format!("{p:?}"), || {
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
        description = "Get dividend history for the symbol. US accounts querying a .US symbol get a differently-shaped response not matching output_schema (dividend_yield_ttm etc. are percent values, e.g. 0.34 means 0.34%); other combinations match output_schema."
    )]
    async fn dividend(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("dividend", format!("{p:?}"), || {
            fundamental::dividend(&mctx, p)
        })
        .await
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
        measured_tool_call("dividend_detail", format!("{p:?}"), || {
            fundamental::dividend_detail(&mctx, p)
        })
        .await
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
        measured_tool_call("forecast_eps", format!("{p:?}"), || {
            fundamental::forecast_eps(&mctx, p)
        })
        .await
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
        description = "Get financial consensus estimates for upcoming periods. US accounts querying a .US symbol get a differently-shaped response not matching output_schema (ai_summary plus a details[] list per period); other combinations match output_schema."
    )]
    async fn consensus(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("consensus", format!("{p:?}"), || {
            fundamental::consensus(&mctx, p)
        })
        .await
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
        description = "Get valuation overview with peer comparison. US accounts querying a .US symbol get a differently-shaped response not matching output_schema (ai_summary plus a metrics.pe object with different sub-fields); other combos match output_schema."
    )]
    async fn valuation(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("valuation", format!("{p:?}"), || {
            fundamental::valuation(&mctx, p)
        })
        .await
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
        measured_tool_call("valuation_history", format!("{p:?}"), || {
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
        measured_tool_call("industry_valuation", format!("{p:?}"), || {
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
        measured_tool_call("industry_valuation_dist", format!("{p:?}"), || {
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
        description = "Get company overview. US accounts querying a .US symbol get a differently-shaped response not matching output_schema (intro, market_cap, top_rank_tags, sharelist, detail_url); other combinations match output_schema."
    )]
    async fn company(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("company", format!("{p:?}"), || {
            fundamental::company(&mctx, p)
        })
        .await
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
        measured_tool_call("executive", format!("{p:?}"), || {
            fundamental::executive(&mctx, p)
        })
        .await
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
        measured_tool_call("shareholder", format!("{p:?}"), || {
            fundamental::shareholder(&mctx, p)
        })
        .await
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
        measured_tool_call("fund_holder", format!("{p:?}"), || {
            fundamental::fund_holder(&mctx, p)
        })
        .await
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
        measured_tool_call("corp_action", format!("{p:?}"), || {
            fundamental::corp_action(&mctx, p)
        })
        .await
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
        measured_tool_call("invest_relation", format!("{p:?}"), || {
            fundamental::invest_relation(&mctx, p)
        })
        .await
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
        measured_tool_call("operating", format!("{p:?}"), || {
            fundamental::operating(&mctx, p)
        })
        .await
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
        measured_tool_call("market_status", String::new(), || {
            market::market_status(&mctx)
        })
        .await
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
        measured_tool_call("broker_holding", format!("{p:?}"), || {
            market::broker_holding(&mctx, p)
        })
        .await
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
        measured_tool_call("broker_holding_detail", format!("{p:?}"), || {
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
        measured_tool_call("broker_holding_daily", format!("{p:?}"), || {
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
        measured_tool_call("ah_premium", format!("{p:?}"), || {
            market::ah_premium(&mctx, p)
        })
        .await
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
        measured_tool_call("ah_premium_intraday", format!("{p:?}"), || {
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
        measured_tool_call("trade_stats", format!("{p:?}"), || {
            market::trade_stats(&mctx, p)
        })
        .await
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
        measured_tool_call("anomaly", format!("{p:?}"), || market::anomaly(&mctx, p)).await
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
        measured_tool_call("constituent", format!("{p:?}"), || {
            market::constituent(&mctx, p)
        })
        .await
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
        description = "Finance calendar by category: report (earnings) / dividend / split / ipo / macrodata (CPI, NFP, rates) / closed (holidays). start and end (YYYY-MM-DD) are optional, default today plus 7 days; keep ranges under 2 weeks or results truncate."
    )]
    async fn finance_calendar(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<calendar::FinanceCalendarParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("finance_calendar", format!("{p:?}"), || {
            calendar::finance_calendar(&mctx, p)
        })
        .await
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
        measured_tool_call("exchange_rate", String::new(), || {
            portfolio::exchange_rate(&mctx)
        })
        .await
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
        measured_tool_call("profit_analysis", format!("{p:?}"), || {
            portfolio::profit_analysis(&mctx, p)
        })
        .await
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
        measured_tool_call("profit_analysis_detail", format!("{p:?}"), || {
            portfolio::profit_analysis_detail(&mctx, p)
        })
        .await
    }

    /// Get realized profit-and-loss for a US account (stock/option/crypto breakdown).
    #[tool(
        title = "Profit Analysis (Realized, US)",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::us_market::PortfolioRealizedPlResponse>(),
        description = "Get realized P&L for a US account, broken down by category (stock/option/crypto) and period. US accounts only; errors with DcRegionRestricted for HK/CN/SG accounts."
    )]
    async fn profit_analysis_realized(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<portfolio::ProfitAnalysisRealizedParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("profit_analysis_realized", format!("{p:?}"), || {
            portfolio::profit_analysis_realized(&mctx, p)
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
        measured_tool_call("alert_list", String::new(), || alert::alert_list(&mctx)).await
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
        measured_tool_call("alert_add", format!("{p:?}"), || alert::alert_add(&mctx, p)).await
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
        measured_tool_call("alert_delete", format!("{p:?}"), || {
            alert::alert_delete(&mctx, p)
        })
        .await
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
        measured_tool_call("alert_enable", format!("{p:?}"), || {
            alert::alert_enable(&mctx, p)
        })
        .await
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
        measured_tool_call("alert_disable", format!("{p:?}"), || {
            alert::alert_disable(&mctx, p)
        })
        .await
    }

    /// Query strategy signals.
    #[tool(
        title = "Signals",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::signal::SignalsResponse>(),
        description = "Query strategy signals — a strategy's take on a security, triggered by a catalyst. Filter by symbol, strategy, catalyst and time range; page with limit/offset. Returns each signal's title, summary, outlook and conservative/benchmark/optimistic target prices, plus the total for paging. The full strategy analysis is omitted here — fetch it with signal_detail."
    )]
    async fn signals(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<signal::SignalsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("signals", format!("{p:?}"), || signal::signals(&mctx, p)).await
    }

    /// Get one signal by ID.
    #[tool(
        title = "Signal Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::signal::SignalItem>(),
        description = "Get one signal by ID (from `signals`). Same fields as the list, plus `analysis` — the full strategy analysis: fit scores, valuation scenarios, evidence sources and related fact IDs."
    )]
    async fn signal_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<signal::SignalIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("signal_detail", format!("{p:?}"), || {
            signal::signal_detail(&mctx, p)
        })
        .await
    }

    /// Get the fact (catalyst) events for a symbol.
    #[tool(
        title = "Security Facts",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::fact::SecurityFactsResponse>(),
        description = "List a security's fact (catalyst) events — anomaly detections, factor readings, data sources and natural-language summaries — filtered by time range and count. Facts are what strategies react to: a signal names its trigger in key_fact_id."
    )]
    async fn security_facts(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<signal::SecurityFactsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("security_facts", format!("{p:?}"), || {
            signal::security_facts(&mctx, p)
        })
        .await
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
        measured_tool_call("news", format!("{p:?}"), || content::news(&mctx, p)).await
    }

    /// Get one news article's full detail.
    #[tool(
        title = "News Detail",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::social::NewsDetailResponse>(),
        description = "Get one news article's full detail by id (from news/news_search). Returns {id, title, description, body (Markdown), url, author{id,name,avatar}, images[], comments_count, likes_count, shares_count, published_at, tickers[]}."
    )]
    async fn news_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<content::NewsIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("news_detail", format!("{p:?}"), || {
            content::news_detail(&mctx, p)
        })
        .await
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
        measured_tool_call("topic", format!("{p:?}"), || content::topic(&mctx, p)).await
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
        measured_tool_call("topic_detail", format!("{p:?}"), || {
            content::topic_detail(&mctx, p)
        })
        .await
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
        measured_tool_call("topic_replies", format!("{p:?}"), || {
            content::topic_replies(&mctx, p)
        })
        .await
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
        measured_tool_call("topic_create", format!("{p:?}"), || {
            content::topic_create(&mctx, p)
        })
        .await
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
        measured_tool_call("topic_create_reply", format!("{p:?}"), || {
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
        measured_tool_call("statement_list", format!("{p:?}"), || {
            statement::statement_list(&mctx, p)
        })
        .await
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
        measured_tool_call("statement_export", format!("{p:?}"), || {
            statement::statement_export(&mctx, p)
        })
        .await
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
        measured_tool_call("short_positions", format!("{p:?}"), || {
            quote::short_positions(&mctx, p)
        })
        .await
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
        measured_tool_call("option_volume", format!("{p:?}"), || {
            quote::option_volume(&mctx, p)
        })
        .await
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
        measured_tool_call("option_volume_daily", format!("{p:?}"), || {
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
        measured_tool_call("dca_list", format!("{p:?}"), || dca::dca_list(&mctx, p)).await
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
        measured_tool_call("dca_create", format!("{p:?}"), || dca::dca_create(&mctx, p)).await
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
        measured_tool_call("dca_update", format!("{p:?}"), || dca::dca_update(&mctx, p)).await
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
        measured_tool_call("dca_pause", format!("{p:?}"), || dca::dca_pause(&mctx, p)).await
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
        measured_tool_call("dca_resume", format!("{p:?}"), || dca::dca_resume(&mctx, p)).await
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
        measured_tool_call("dca_stop", format!("{p:?}"), || dca::dca_stop(&mctx, p)).await
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
        measured_tool_call("dca_history", format!("{p:?}"), || {
            dca::dca_history(&mctx, p)
        })
        .await
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
        measured_tool_call("dca_stats", format!("{p:?}"), || dca::dca_stats(&mctx, p)).await
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
        measured_tool_call("dca_check", format!("{p:?}"), || dca::dca_check(&mctx, p)).await
    }

    /// Pre-trade grid setup info for a symbol (lot sizes, price steps, authorization).
    #[tool(
        title = "Grid Symbol Info",
        annotations(read_only_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::grid::GridSymbolInfoResponse>(),
        description = "Pre-trade grid setup info for a security (takes a symbol, not an order_id): security name, last price, board lot sizes (buy/sell), price-step (bid_size) table, and channel/authorization info (strategy grant flag, RTH support, supported settlement currencies). Use before grid_submit to learn the symbol's grid constraints."
    )]
    async fn grid_symbol_info(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridSymbolParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_symbol_info", format!("{p:?}"), || {
            grid::grid_symbol_info(&mctx, p)
        })
        .await
    }

    /// List grid trading orders.
    #[tool(
        title = "List Grid Orders",
        annotations(read_only_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::grid::GridListResponse>(),
        description = "List grid trading orders. Filter by symbol or comma-joined status (e.g. \"Performing,Suspended\"); supports page/limit and sort_by/sort_order. Returns grid_order[] summaries + has_more."
    )]
    async fn grid_list(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridListParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_list", format!("{p:?}"), || grid::grid_list(&mctx, p)).await
    }

    /// Fetch grid orders by IDs.
    #[tool(
        title = "Get Grid Orders By IDs",
        annotations(read_only_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::grid::GridOrdersResponse>(),
        description = "Fetch specific grid orders by their IDs. Returns grid_orders[] summaries."
    )]
    async fn grid_list_by_ids(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridIdsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_list_by_ids", format!("{p:?}"), || {
            grid::grid_list_by_ids(&mctx, p)
        })
        .await
    }

    /// Grid order detail.
    #[tool(
        title = "Grid Order Detail",
        annotations(read_only_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::grid::GridOrderDetailResponse>(),
        description = "Full detail for one grid order: rule parameters, status, embedded child orders (grid_sub_orders) and lifecycle history (grid_order_history). Supports history_id cursor + limit paging."
    )]
    async fn grid_detail(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridDetailParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_detail", format!("{p:?}"), || {
            grid::grid_detail(&mctx, p)
        })
        .await
    }

    /// Grid order trigger history.
    #[tool(
        title = "Grid Trigger History",
        annotations(read_only_hint = true, open_world_hint = true),
        output_schema = schema_for::<output::grid::GridTriggerHistoryResponse>(),
        description = "Trigger history for one grid order: each triggered child order with price, quantity, executed price/qty, and trigger time. Supports page/limit."
    )]
    async fn grid_trigger_history(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridTriggerHistoryParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_trigger_history", format!("{p:?}"), || {
            grid::grid_trigger_history(&mctx, p)
        })
        .await
    }

    /// Submit a grid trading order.
    #[tool(
        title = "Submit Grid Order",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::grid::GridSubmitResponse>(),
        description = "Submit a grid trading order. DRY RUN unless execute is the confirmation_code from its own dry run: call once without execute, show the preview, then re-call quoting the code only after the user confirms. A live grid keeps trading on its own. Requires symbol, settlement_currency, and the grid rule: base/upper/lower price, trigger_price_type (1=spread, 2=percent) with the matching spread/percent up/down, trigger_quantity, upper/lower_limit_quantity, time_in_force (0=Day, 1=GTC, 6=GTD), grid_order_type_up/down (GMO/GLO/GTG), and boundary events (1=ignore, 2=close-at-last). Prices/quantities are decimal strings. Requires the one-time grid_questionnaire consent."
    )]
    async fn grid_submit(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridSubmitParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_submit", format!("{p:?}"), || {
            grid::grid_submit(&mctx, p)
        })
        .await
    }

    /// Replace (modify) a grid trading order.
    #[tool(
        title = "Replace Grid Order",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Replace an existing grid order's rule by order_id. Accepts the same grid rule fields as grid_submit. Overwrites the order's entire rule. TWO-STEP CONFIRMATION IS MANDATORY: this tool is a DRY RUN unless you pass the confirmation_code its own dry run returned. Call it first without execute, show the returned preview to the user, and only call it again with execute=\"<confirmation_code>\" after the user has explicitly confirmed it. The code is derived from the order itself, so it applies only to that exact request. Never quote it back on your own initiative. The dry run echoes the rule that would replace the current one."
    )]
    async fn grid_replace(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridReplaceParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_replace", format!("{p:?}"), || {
            grid::grid_replace(&mctx, p)
        })
        .await
    }

    /// Cancel a grid trading order.
    #[tool(
        title = "Cancel Grid Order",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Cancel (terminate) a grid order by order_id. TWO-STEP CONFIRMATION IS MANDATORY: this tool is a DRY RUN unless you pass the confirmation_code its own dry run returned. Call it first without execute, show the returned preview to the user, and only call it again with execute=\"<confirmation_code>\" after the user has explicitly confirmed it. The code is derived from the order itself, so it applies only to that exact request. Never quote it back on your own initiative."
    )]
    async fn grid_cancel(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridOrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_cancel", format!("{p:?}"), || {
            grid::grid_cancel(&mctx, p)
        })
        .await
    }

    /// Suspend a grid trading order.
    #[tool(
        title = "Suspend Grid Order",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Suspend (pause) a running grid order by order_id. Resume with grid_restart. TWO-STEP CONFIRMATION IS MANDATORY: this tool is a DRY RUN unless you pass the confirmation_code its own dry run returned. Call it first without execute, show the returned preview to the user, and only call it again with execute=\"<confirmation_code>\" after the user has explicitly confirmed it. The code is derived from the order itself, so it applies only to that exact request. Never quote it back on your own initiative."
    )]
    async fn grid_suspend(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridOrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_suspend", format!("{p:?}"), || {
            grid::grid_suspend(&mctx, p)
        })
        .await
    }

    /// Restart a suspended grid trading order.
    #[tool(
        title = "Restart Grid Order",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Restart (resume) a suspended grid order by order_id. TWO-STEP CONFIRMATION IS MANDATORY: this tool is a DRY RUN unless you pass the confirmation_code its own dry run returned. Call it first without execute, show the returned preview to the user, and only call it again with execute=\"<confirmation_code>\" after the user has explicitly confirmed it. The code is derived from the order itself, so it applies only to that exact request. Never quote it back on your own initiative. A restarted grid resumes placing orders on its own."
    )]
    async fn grid_restart(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridOrderIdParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_restart", format!("{p:?}"), || {
            grid::grid_restart(&mctx, p)
        })
        .await
    }

    /// Submit the grid strategy risk-disclosure questionnaire.
    #[tool(
        title = "Grid Strategy Consent",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        description = "Record the one-time grid strategy risk-disclosure consent required before submitting grid orders. Takes no parameters."
    )]
    async fn grid_questionnaire(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<grid::GridQuestionnaireParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("grid_questionnaire", format!("{p:?}"), || {
            grid::grid_questionnaire(&mctx, p)
        })
        .await
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
        measured_tool_call("sharelist_list", format!("{p:?}"), || {
            sharelist::sharelist_list(&mctx, p)
        })
        .await
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
        measured_tool_call("sharelist_detail", format!("{p:?}"), || {
            sharelist::sharelist_detail(&mctx, p)
        })
        .await
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
        measured_tool_call("sharelist_create", format!("{p:?}"), || {
            sharelist::sharelist_create(&mctx, p)
        })
        .await
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
        measured_tool_call("sharelist_delete", format!("{p:?}"), || {
            sharelist::sharelist_delete(&mctx, p)
        })
        .await
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
        measured_tool_call("sharelist_add", format!("{p:?}"), || {
            sharelist::sharelist_add(&mctx, p)
        })
        .await
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
        measured_tool_call("sharelist_remove", format!("{p:?}"), || {
            sharelist::sharelist_remove(&mctx, p)
        })
        .await
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
        measured_tool_call("sharelist_sort", format!("{p:?}"), || {
            sharelist::sharelist_sort(&mctx, p)
        })
        .await
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
        measured_tool_call("sharelist_popular", format!("{p:?}"), || {
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
        measured_tool_call("quant_run", format!("{p:?}"), || {
            quant::run_script(&mctx, p)
        })
        .await
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
        measured_tool_call("news_search", format!("{p:?}"), || {
            search::news_search(&mctx, p)
        })
        .await
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
        measured_tool_call("topic_search", format!("{p:?}"), || {
            search::topic_search(&mctx, p)
        })
        .await
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
        output_schema = schema_for::<output::us_market::FinancialStatementResponse>(),
        description = "Get financial statements (income statement, balance sheet, or cash flow) for a security. kind: IS/BS/CF/ALL. report: af (annual, default), saf (semi-annual), qf (quarterly full), q1/q2/q3. US accounts querying a .US symbol are routed to a US-specific statement endpoint (same report vocabulary as the generic path); kind=ALL/default fans out to IS+BS+CF and returns {income_statement, balance_sheet, cash_flow} since the backend doesn't support a combined request; all other symbol/account combinations use the generic path."
    )]
    async fn financial_statement(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::FinancialStatementParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_statement", format!("{p:?}"), || {
            fundamental::financial_statement(&mctx, p)
        })
        .await
    }

    /// Get key financial metrics for a US symbol.
    #[tool(
        title = "Financial Report Key Metrics (US)",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::us_market::FinancialReportKeyMetricsResponse>(),
        description = "Get key financial metrics (fin-keyfactor) for a US symbol. report: af (annual, default), saf, qf, q1/q2/q3. US accounts only; errors with DcRegionRestricted for HK/CN/SG accounts."
    )]
    async fn financial_report_key_metrics(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::SymbolReportParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("financial_report_key_metrics", format!("{p:?}"), || {
            fundamental::financial_report_key_metrics(&mctx, p)
        })
        .await
    }

    /// Get regulatory/prospectus documents for a US ETF.
    #[tool(
        title = "ETF Documents (US)",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        ),
        output_schema = schema_for::<output::us_market::EtfDocsResponse>(),
        description = "Get regulatory/prospectus documents (etf-files) for a US ETF. US accounts only; errors with DcRegionRestricted for HK/CN/SG accounts."
    )]
    async fn etf_docs(
        &self,
        ctx: RequestContext<RoleServer>,
        Parameters(p): Parameters<fundamental::EtfDocsParam>,
    ) -> Result<CallToolResult, McpError> {
        let mctx = extract_context(&ctx)?;
        measured_tool_call("etf_docs", format!("{p:?}"), || {
            fundamental::etf_docs(&mctx, p)
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
        measured_tool_call("financial_report_latest", format!("{p:?}"), || {
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
        measured_tool_call("valuation_rank", format!("{p:?}"), || {
            fundamental::valuation_rank(&mctx, p)
        })
        .await
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
        measured_tool_call("institution_rating_history", format!("{p:?}"), || {
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
        measured_tool_call("institution_rating_industry_rank", format!("{p:?}"), || {
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
        measured_tool_call("short_margin", String::new(), || trade::short_margin(&mctx)).await
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
        measured_tool_call("bank_cards", String::new(), || atm::bank_cards(&mctx)).await
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
        measured_tool_call("withdrawals", format!("{p:?}"), || {
            atm::withdrawals(&mctx, p)
        })
        .await
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
        measured_tool_call("deposits", format!("{p:?}"), || atm::deposits(&mctx, p)).await
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
        measured_tool_call("ipo_subscriptions", String::new(), || {
            ipo::ipo_subscriptions(&mctx)
        })
        .await
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
        measured_tool_call("ipo_calendar", String::new(), || ipo::ipo_calendar(&mctx)).await
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
        measured_tool_call("ipo_listed", format!("{p:?}"), || ipo::ipo_listed(&mctx, p)).await
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
        measured_tool_call("ipo_detail", format!("{p:?}"), || ipo::ipo_detail(&mctx, p)).await
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
        measured_tool_call("ipo_orders", format!("{p:?}"), || ipo::ipo_orders(&mctx, p)).await
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
        measured_tool_call("ipo_order_detail", format!("{p:?}"), || {
            ipo::ipo_order_detail(&mctx, p)
        })
        .await
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
        measured_tool_call("ipo_profit_loss", format!("{p:?}"), || {
            ipo::ipo_profit_loss(&mctx, p)
        })
        .await
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
        measured_tool_call("business_segments", format!("{p:?}"), || {
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
        measured_tool_call("business_segments_history", format!("{p:?}"), || {
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
        measured_tool_call("institutional_views", format!("{p:?}"), || {
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
        measured_tool_call("industry_rank", format!("{p:?}"), || {
            market::industry_rank(&mctx, p)
        })
        .await
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
        measured_tool_call("industry_peers", format!("{p:?}"), || {
            fundamental::industry_peers(&mctx, p)
        })
        .await
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
        measured_tool_call("financial_report_snapshot", format!("{p:?}"), || {
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
        measured_tool_call("shareholder_top", format!("{p:?}"), || {
            fundamental::shareholder_top(&mctx, p)
        })
        .await
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
        measured_tool_call("shareholder_detail", format!("{p:?}"), || {
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
        measured_tool_call("valuation_comparison", format!("{p:?}"), || {
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
        measured_tool_call("short_trades", format!("{p:?}"), || {
            market::short_trades(&mctx, p)
        })
        .await
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
        measured_tool_call("top_movers", format!("{p:?}"), || {
            market::top_movers(&mctx, p)
        })
        .await
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
        measured_tool_call("rank_categories", String::new(), || {
            market::rank_categories(&mctx)
        })
        .await
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
        measured_tool_call("rank_list", format!("{p:?}"), || {
            market::rank_list(&mctx, p)
        })
        .await
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
        measured_tool_call("screener_recommend_strategies", format!("{p:?}"), || {
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
        measured_tool_call("screener_user_strategies", format!("{p:?}"), || {
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
        measured_tool_call("screener_strategy", format!("{p:?}"), || {
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
        measured_tool_call("screener_search", format!("{p:?}"), || {
            screener::screener_search(&mctx, p)
        })
        .await
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
        measured_tool_call("screener_indicators", format!("{p:?}"), || {
            screener::screener_indicators(&mctx, p)
        })
        .await
    }
}

#[tool_handler(
    name = "longbridge-mcp",
    instructions = "Longbridge OpenAPI MCP Server - provides market data, trading, and financial analysis tools. Order execution requires two-step confirmation: submit_order, cancel_order, replace_order and every grid write (grid_submit, grid_replace, grid_cancel, grid_suspend, grid_restart) are dry runs that return a single-use confirmation_code. Always call them once without execute, show the returned preview to the user, and only re-call with execute set to that code after the user has explicitly confirmed it."
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
            "Longbridge OpenAPI MCP Server - provides market data, trading, and financial analysis tools. Order execution requires two-step confirmation: submit_order, cancel_order, replace_order and every grid write (grid_submit, grid_replace, grid_cancel, grid_suspend, grid_restart) are dry runs that return a single-use confirmation_code. Always call them once without execute, show the returned preview to the user, and only re-call with execute set to that code after the user has explicitly confirmed it.",
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
        // DC-region execution gate, independent of the /v1/v2 restricted-endpoint
        // check above: on the main (`/mcp`) and authenticated `/agent` endpoints,
        // a tool hidden from `tools/list` for this account's region must also be
        // un-callable by name, or the listing filter is merely cosmetic.
        if restricted_version(&context).is_none()
            && let Ok(mctx) = extract_context(&context)
        {
            let region = mctx.dc_region().await;
            if is_hidden_for_dc_region(request.name.as_ref(), region) {
                return Err(McpError::invalid_request(
                    format!(
                        "Tool `{}` is not available for accounts in the {region} data center.",
                        request.name
                    ),
                    None,
                ));
            }
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
                RestrictedVersion::V2 => tools_v2_endpoint().to_vec(),
            }
        } else {
            let mut tools = tools_main_endpoint().to_vec();
            if let Ok(mctx) = extract_context(&context) {
                let region = mctx.dc_region().await;
                tools.retain(|t| !is_hidden_for_dc_region(t.name.as_ref(), region));
            }
            tools
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
    use super::strip_schema_documentation_keys;

    #[test]
    fn schema_compactor_keeps_properties_named_title_or_description() {
        // "title"/"description" are documentation keywords on a *schema*
        // object, but inside a `properties` map they are property *names* —
        // stripping them there deletes real fields (news_detail's headline
        // fields) from the advertised outputSchema.
        let mut schema = serde_json::json!({
            "title": "NewsDetailResponse",
            "description": "doc",
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Title." },
                "description": { "type": "string", "description": "Excerpt." },
                "body": { "type": "string", "description": "Markdown." }
            }
        });
        strip_schema_documentation_keys(&mut schema);
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("title"), "property name must survive");
        assert!(
            props.contains_key("description"),
            "property name must survive"
        );
        // Schema-level annotations are stripped, including on child schemas.
        assert!(schema.get("title").is_none());
        assert!(schema.get("description").is_none());
        assert!(props["body"].get("description").is_none());
    }

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

    #[test]
    fn does_not_forward_alicloud_alb_trace() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("alicloud-alb-trace"),
            HeaderValue::from_static("0123456789abcdef"),
        );

        assert!(
            !collect_headers(&map)
                .iter()
                .any(|(key, _)| key == "alicloud-alb-trace")
        );
    }

    #[test]
    fn forwards_short_x_forwarded_for_chain() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("x-forwarded-for"),
            HeaderValue::from_static("192.0.2.1, 192.0.2.2"),
        );

        let headers = collect_headers(&map);
        assert!(
            headers.iter().any(|(key, value)| {
                key == "x-forwarded-for" && value == "192.0.2.1, 192.0.2.2"
            })
        );
    }

    #[test]
    fn parses_x_forwarded_for_chain_without_spaces() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("x-forwarded-for"),
            HeaderValue::from_static("192.0.2.1,192.0.2.2,192.0.2.3"),
        );

        let headers = collect_headers(&map);
        assert!(headers.iter().any(|(key, value)| {
            key == "x-forwarded-for" && value == "192.0.2.1, 192.0.2.2, 192.0.2.3"
        }));
    }

    #[test]
    fn caps_x_forwarded_for_chain_at_ten_addresses() {
        let mut map = HeaderMap::new();
        map.insert(
            HeaderName::from_static("x-forwarded-for"),
            HeaderValue::from_static(
                "192.0.2.1, 192.0.2.2, 192.0.2.3, 192.0.2.4, 192.0.2.5, 192.0.2.6, 192.0.2.7, 192.0.2.8, 192.0.2.9, 192.0.2.10, 192.0.2.11, 192.0.2.12",
            ),
        );

        let headers = collect_headers(&map);
        assert!(headers.iter().any(|(key, value)| {
            key == "x-forwarded-for"
                && value
                    == "192.0.2.1, 192.0.2.2, 192.0.2.3, 192.0.2.4, 192.0.2.5, 192.0.2.6, 192.0.2.7, 192.0.2.8, 192.0.2.9, 192.0.2.10"
        }));
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
                 (add it with V2 or 0)"
            );
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

    #[test]
    fn dc_region_tool_lists_reference_live_tools() {
        use std::collections::HashSet;

        let live: HashSet<String> = crate::tools::list_tools()
            .iter()
            .map(|t| t.name.to_string())
            .collect();

        for name in super::US_ONLY_TOOLS.iter().chain(super::AP_ONLY_TOOLS) {
            assert!(
                live.contains(*name),
                "US_ONLY_TOOLS/AP_ONLY_TOOLS references unknown tool `{name}` (renamed or removed?)"
            );
        }
    }

    #[test]
    fn dc_region_hiding_is_symmetric_and_disjoint() {
        use longbridge::DcRegion;

        for name in super::US_ONLY_TOOLS {
            assert!(
                super::is_hidden_for_dc_region(name, DcRegion::Ap),
                "US-only tool `{name}` must be hidden for AP accounts"
            );
            assert!(
                !super::is_hidden_for_dc_region(name, DcRegion::Us),
                "US-only tool `{name}` must not be hidden for US accounts"
            );
        }
        for name in super::AP_ONLY_TOOLS {
            assert!(
                super::is_hidden_for_dc_region(name, DcRegion::Us),
                "AP-only tool `{name}` must be hidden for US accounts"
            );
            assert!(
                !super::is_hidden_for_dc_region(name, DcRegion::Ap),
                "AP-only tool `{name}` must not be hidden for AP accounts"
            );
        }
        assert!(
            super::US_ONLY_TOOLS
                .iter()
                .all(|n| !super::AP_ONLY_TOOLS.contains(n)),
            "US_ONLY_TOOLS and AP_ONLY_TOOLS must be disjoint"
        );
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
    fn macrodata_tools_expose_chatgpt_required_metadata() {
        for name in ["macrodata", "macrodata_indicators"] {
            let tool = super::list_tools()
                .into_iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("{name} tool must be registered"));
            let annotations = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{name} must declare annotations"));

            assert_eq!(annotations.read_only_hint, Some(true));
            assert_eq!(annotations.destructive_hint, Some(false));
            assert_eq!(annotations.open_world_hint, Some(true));
            assert!(
                tool.output_schema.is_some(),
                "{name} must declare an outputSchema"
            );
        }
    }

    #[test]
    fn us_market_tools_expose_required_metadata() {
        for name in [
            "financial_statement",
            "financial_report",
            "financial_report_key_metrics",
            "profit_analysis_realized",
            "etf_docs",
            "stock_positions",
            "order_detail",
            "dividend",
            "consensus",
            "valuation",
            "company",
        ] {
            let tool = super::list_tools()
                .into_iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("{name} tool must be registered"));
            let annotations = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{name} must declare annotations"));

            assert_eq!(annotations.read_only_hint, Some(true));
            assert_eq!(annotations.destructive_hint, Some(false));
            assert_eq!(annotations.open_world_hint, Some(true));
            assert!(
                tool.output_schema.is_some(),
                "{name} must declare an outputSchema"
            );
        }
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

        // ── list_tools() path: all_tools_cached().to_vec() (all 152 tools) ──
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
            "list_tools (all 152 tools):     {:>8.1} µs/call  ({n} calls, {:?} total)",
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

#[cfg(test)]
mod tool_error_tests {
    use super::{error_hint, measured_tool_call, tool_error, tool_result};
    use crate::test_support::SharedBuffer;
    use rmcp::ErrorData as McpError;
    use rmcp::model::CallToolResult;

    /// `tracing`'s per-callsite interest cache is process-global: whichever
    /// thread first reaches the `tool call rejected`/`failed`/`error detail`
    /// sites in `measured_tool_call` decides — for the rest of the process —
    /// whether they're enabled, before consulting any subscriber a *later*
    /// thread installs. Every test in this module that calls
    /// `measured_tool_call` shares this lock so they can't race each other
    /// for that cache, the same problem `HTTP_URL_ENV_LOCK` above solves for
    /// a mutable env var.
    static LOG_CALLSITE_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

    fn text_of(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn tool_error_emits_a_structured_envelope() {
        let err = McpError::internal_error("upstream exploded", None);
        let result = tool_error("finance_calendar", &err);
        assert_eq!(result.is_error, Some(true));
        assert!(result.structured_content.is_none());
        let v: serde_json::Value =
            serde_json::from_str(&text_of(&result)).expect("content must be JSON");
        assert_eq!(v["message"], "upstream exploded");
        assert_eq!(v["recoverable"], "none");
        assert!(v["error_code"].is_null());
        assert!(v["data"].is_null());
    }

    #[test]
    fn tool_error_reauth_envelope_carries_code_and_hint() {
        let err = McpError::internal_error(
            "openapi error: code=401103: token is expired".to_string(),
            None,
        );
        let result = tool_error("quote", &err);
        assert_eq!(result.is_error, Some(true));
        let v: serde_json::Value = serde_json::from_str(&text_of(&result)).unwrap();
        assert_eq!(v["recoverable"], "reauth");
        assert!(
            v["hint"]
                .as_str()
                .is_some_and(|h| h.contains("re-auth") || h.contains("reconnect"))
        );
    }

    #[test]
    fn terminal_object_rooted_returns_success_with_schema_valid_content() {
        let err = McpError::internal_error(
            "WsResponseErrorDetail { code: 301604, msg: \"no quote access\" }".to_string(),
            None,
        );
        let result = tool_error("depth", &err);
        assert_eq!(
            result.is_error,
            Some(false),
            "terminal condition must be isError:false"
        );
        let v: serde_json::Value = serde_json::from_str(&text_of(&result)).unwrap();
        assert_eq!(v["recoverable"], "none");
        assert!(
            v["note"].as_str().is_some(),
            "must carry an explanatory note"
        );
        let sc = result
            .structured_content
            .expect("object-rooted tool must set structuredContent");
        assert!(sc.is_object());
        let schema = super::output_schema_map()
            .get("depth")
            .expect("depth schema");
        for req in schema
            .get("required")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
        {
            let key = req.as_str().unwrap();
            assert!(sc.get(key).is_some(), "missing required field {key}");
        }
    }

    #[test]
    fn terminal_array_rooted_returns_success_without_structured_content() {
        let err = McpError::internal_error(
            "WsResponseErrorDetail { code: 301603, msg: \"no quotes\" }".to_string(),
            Some(serde_json::json!({ "openapi_error_code": 301603 })),
        );
        let result = tool_error("option_quote", &err);
        assert_eq!(result.is_error, Some(false));
        assert!(
            result.structured_content.is_none(),
            "array-rooted terminal must leave structuredContent unset"
        );
        let v: serde_json::Value = serde_json::from_str(&text_of(&result)).unwrap();
        assert_eq!(v["error_code"], 301603);
    }

    #[test]
    fn every_schema_backed_terminal_instance_matches_its_schema() {
        let err = McpError::internal_error(
            "WsResponseErrorDetail { code: 301604, msg: \"no quote access\" }".to_string(),
            None,
        );
        for name in super::TERMINAL_OBJECT_ROOTED {
            let result = tool_error(name, &err);
            let sc = result
                .structured_content
                .unwrap_or_else(|| panic!("{name} must set structuredContent"));
            let schema = super::output_schema_map()
                .get(*name)
                .unwrap_or_else(|| panic!("{name} must have a schema"));
            for req in schema
                .get("required")
                .and_then(|r| r.as_array())
                .into_iter()
                .flatten()
            {
                let key = req.as_str().unwrap();
                assert!(
                    sc.get(key).is_some(),
                    "{name}: missing required field {key}"
                );
            }
        }
    }

    #[test]
    fn invalid_params_hint_points_at_the_input_schema() {
        let err = McpError::invalid_params("invalid period 'daily'", None);
        assert!(
            error_hint(&err).is_some_and(|h| h.contains("input schema")),
            "expected an input-schema hint"
        );
    }

    #[test]
    fn permission_errors_hint_at_reconnecting_for_full_scopes() {
        for message in [
            "403 Forbidden",
            "no permission for this endpoint",
            "missing scope: trade",
        ] {
            let err = McpError::internal_error(message.to_string(), None);
            let hint = error_hint(&err).unwrap_or_else(|| panic!("no hint for {message:?}"));
            assert!(
                hint.contains("reconnect") && hint.contains("scopes"),
                "hint for {message:?} should mention reconnecting for full scopes, got: {hint}"
            );
        }
    }

    #[test]
    fn expired_token_errors_hint_at_reauthorizing() {
        let err = McpError::internal_error("access token expired", None);
        assert!(
            error_hint(&err).is_some_and(|h| h.contains("reconnect")),
            "expected a re-authorization hint"
        );
    }

    #[test]
    fn rate_limit_errors_hint_at_backing_off() {
        for message in [
            "openapi error: code=429002: 已达到 1S 区间调用上限，请 0.4 秒后重试",
            "openapi error: code=429003: minimum interval between two calls should be 0.02 seconds",
            "rate limit of 1-second interval has been reached, please retry after: 1s",
        ] {
            let err = McpError::internal_error(message.to_string(), None);
            let hint = error_hint(&err).unwrap_or_else(|| panic!("no hint for {message:?}"));
            assert!(
                hint.contains("rate-limited"),
                "hint for {message:?} should mention rate limiting, got: {hint}"
            );
        }
    }

    #[test]
    fn dc_region_restricted_errors_hint_at_using_the_account_own_region() {
        let err = McpError::internal_error(
            "this API (/v1/us/stock-info/fin-keyfactor) is only available in the US data \
             center and is not supported for your AP-region account"
                .to_string(),
            None,
        );
        let hint = error_hint(&err).expect("expected a DC-region hint");
        assert!(
            hint.contains("data center") && hint.contains("region"),
            "unexpected hint: {hint}"
        );
    }

    #[test]
    fn no_quote_access_errors_hint_at_a_missing_subscription() {
        let err = McpError::internal_error(
            "response error: 7: detail:Some(WsResponseErrorDetail { code: 301604, msg: \
             \"no quote access\" })"
                .to_string(),
            None,
        );
        let hint = error_hint(&err).expect("expected a no-quote-access hint");
        assert!(hint.contains("subscription"), "unexpected hint: {hint}");
    }

    #[test]
    fn structured_error_code_is_preferred_over_substring_matching() {
        // A message that happens to embed "301604" in an unrelated field
        // (e.g. a trace id) must NOT get the no-quote-access hint once a
        // structured code is available and says otherwise.
        let err = McpError::internal_error(
            "openapi error: code=401103: token is expired (trace_id=301604-abc)".to_string(),
            Some(serde_json::json!({ "openapi_error_code": 401103 })),
        );
        let hint = error_hint(&err).expect("expected a hint");
        assert!(
            hint.contains("reconnect") && !hint.contains("subscription"),
            "structured code 401103 should win over the substring '301604' in the trace id, got: {hint}"
        );
    }

    #[test]
    fn structured_rate_limit_code_is_matched_even_without_matching_text() {
        let err = McpError::internal_error(
            "openapi error: unexpected rejection".to_string(),
            Some(serde_json::json!({ "openapi_error_code": 429002 })),
        );
        let hint = error_hint(&err).expect("expected a rate-limit hint from the structured code");
        assert!(hint.contains("rate-limited"), "unexpected hint: {hint}");
    }

    #[test]
    fn descriptive_text_still_hints_under_an_unenumerated_error_code() {
        // Regression test: an earlier version of error_hint() only fell back
        // to string matching when `code` was entirely absent, so any
        // structured code not in RATE_LIMIT_CODES/NO_QUOTE_ACCESS_CODE
        // silently disabled the hint even when the message text plainly
        // said "rate limit" / "no quote access".
        let rate_limited = McpError::internal_error(
            "openapi error: code=429001: rate limit exceeded".to_string(),
            Some(serde_json::json!({ "openapi_error_code": 429001 })),
        );
        let hint = error_hint(&rate_limited)
            .expect("an unenumerated rate-limit code with descriptive text must still hint");
        assert!(hint.contains("rate-limited"), "unexpected hint: {hint}");

        let no_quote_access = McpError::internal_error(
            "longbridge: response error: 7: detail:Some(WsResponseErrorDetail { code: 301609, \
             msg: \"no quote access\" })"
                .to_string(),
            Some(serde_json::json!({ "openapi_error_code": 301609 })),
        );
        let hint = error_hint(&no_quote_access)
            .expect("an unenumerated no-quote-access code with descriptive text must still hint");
        assert!(hint.contains("subscription"), "unexpected hint: {hint}");
    }

    #[test]
    fn structured_dc_region_restriction_hints_without_matching_text() {
        let err = McpError::internal_error(
            "openapi error: unexpected rejection".to_string(),
            Some(serde_json::json!({
                "dc_region_restricted": { "path": "/v1/us/foo", "required": "us", "current": "ap" }
            })),
        );
        let hint = error_hint(&err).expect("expected a DC-region hint from the structured field");
        assert!(hint.contains("data center"), "unexpected hint: {hint}");
    }

    #[test]
    fn dc_region_check_runs_before_rate_limit_so_it_cannot_be_shadowed() {
        // A DC-region-restricted error always has code=None (mutually
        // exclusive with openapi_error_code on any given longbridge::Error),
        // so its message text is visible to the rate-limit check's
        // numeric-needle fallback. If the DC-region check ran second, this
        // message (which happens to contain "429002" in its path) would get
        // the wrong hint.
        let err = McpError::internal_error(
            "this API (/v1/us/429002/foo) is only available in the US data center".to_string(),
            Some(serde_json::json!({
                "dc_region_restricted": { "path": "/v1/us/429002/foo", "required": "us", "current": "ap" }
            })),
        );
        let hint = error_hint(&err).expect("expected a hint");
        assert!(
            hint.contains("data center") && !hint.contains("rate-limited"),
            "DC-region's structured signal must win even though the path contains a \
             rate-limit-looking numeric needle, got: {hint}"
        );
    }

    #[test]
    fn a_rate_limit_code_outside_the_hardcoded_list_still_matches_via_the_429_range() {
        // No text needle matches here on purpose — this must be caught by
        // the numeric-range check on the structured code, not by string
        // matching, unlike descriptive_text_still_hints_under_an_unenumerated_error_code
        // above which covers the text-needle path for an unenumerated code.
        let err = McpError::internal_error(
            "openapi error: code=429001: too many concurrent connections".to_string(),
            Some(serde_json::json!({ "openapi_error_code": 429001 })),
        );
        let hint = error_hint(&err).expect("429xxx codes must match via the range check");
        assert!(hint.contains("rate-limited"), "unexpected hint: {hint}");
    }

    #[test]
    fn a_bare_401_substring_does_not_misfire_when_a_different_code_is_present() {
        // Same false-positive class the rate-limit/no-quote-access branches
        // were hardened against: a bare "401" can appear in an unrelated
        // field (here, a trace id) — it must not win once a different,
        // authoritative structured code is present.
        let err = McpError::internal_error(
            "openapi error: code=403308: scope not authorized (trace_id=401-abc)".to_string(),
            Some(serde_json::json!({ "openapi_error_code": 403308 })),
        );
        let hint = error_hint(&err).expect("expected a permission hint");
        assert!(
            hint.contains("scopes") && !hint.contains("access token"),
            "structured code 403308 should win over the substring '401' in the trace id (403 \
             scope hint, not the 401 token hint), got: {hint}"
        );
    }

    #[test]
    fn a_permission_code_outside_the_hardcoded_needles_still_matches_via_the_403_range() {
        let err = McpError::internal_error(
            "openapi error: code=403309: unexpected access rejection".to_string(),
            Some(serde_json::json!({ "openapi_error_code": 403309 })),
        );
        let hint = error_hint(&err).expect("403xxx codes must match via the range check");
        assert!(hint.contains("scopes"), "unexpected hint: {hint}");
    }

    #[test]
    fn a_token_code_outside_the_hardcoded_needles_still_matches_via_the_401_range() {
        let err = McpError::internal_error(
            "openapi error: code=401104: session invalid".to_string(),
            Some(serde_json::json!({ "openapi_error_code": 401104 })),
        );
        let hint = error_hint(&err).expect("401xxx codes must match via the range check");
        assert!(hint.contains("re-authorize"), "unexpected hint: {hint}");
    }

    #[test]
    fn ordinary_errors_get_no_hint() {
        let err = McpError::internal_error("symbol not found", None);
        assert!(error_hint(&err).is_none());
    }

    #[tokio::test]
    async fn measured_tool_call_reports_failures_as_tool_errors() {
        let _lock = LOG_CALLSITE_LOCK.lock().await;
        let result = measured_tool_call("some_tool", "test-params".to_string(), || async {
            Err(McpError::internal_error("upstream 500", None))
        })
        .await
        .expect("a failing tool must not surface as a protocol error");

        assert_eq!(result.is_error, Some(true));
        assert!(text_of(&result).contains("upstream 500"));
    }

    #[tokio::test]
    async fn measured_tool_call_passes_success_through_untouched() {
        let _lock = LOG_CALLSITE_LOCK.lock().await;
        let result = measured_tool_call("some_tool", "test-params".to_string(), || async {
            Ok(tool_result(r#"{"ok":true}"#.to_string()))
        })
        .await
        .expect("successful call");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result.structured_content,
            Some(serde_json::json!({"ok": true}))
        );
    }

    #[tokio::test]
    async fn measured_tool_call_logs_failures_only() {
        let _lock = LOG_CALLSITE_LOCK.lock().await;
        let buf = SharedBuffer::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .without_time()
            .finish();
        // Thread-local default; valid across the `.await`s below because
        // `#[tokio::test]` defaults to a single-threaded runtime.
        let _guard = tracing::subscriber::set_default(subscriber);
        // `tracing`'s per-callsite interest cache is process-global: another
        // test's thread can reach one of `measured_tool_call`'s log sites
        // first with no subscriber installed and cache it as disabled
        // forever, which silently swallows our events under `cargo test`'s
        // default parallelism. Force a fresh interest computation against
        // the subscriber we just installed. (Note: this test's own
        // subscriber has no `payload_guard`, so the off-by-default
        // `error_detail` target is NOT capped here — that cap is asserted
        // separately in `logging.rs`'s tests, against the real filter stack.)
        tracing::callsite::rebuild_interest_cache();

        // Success logs nothing — volume for successful calls is covered by
        // the metric recorded alongside, not by a per-call log line.
        measured_tool_call("logged_ok_tool", "test-params".to_string(), || async {
            Ok(tool_result(r#"{"ok":true}"#.to_string()))
        })
        .await
        .expect("successful call");

        measured_tool_call(
            "logged_failing_tool",
            "sym=BADSTOCK".to_string(),
            || async { Err(McpError::internal_error("upstream 500", None)) },
        )
        .await
        .expect("a failing tool must not surface as a protocol error");

        measured_tool_call("logged_bad_params_tool", "sym=???".to_string(), || async {
            Err(McpError::invalid_params("bad symbol", None))
        })
        .await
        .expect("a failing tool must not surface as a protocol error");

        let logged = buf.contents();
        let lines: Vec<&str> = logged.lines().collect();
        let line_with = |needle: &str, other: &str| -> &str {
            lines
                .iter()
                .find(|l| l.contains(needle) && l.contains(other))
                .unwrap_or_else(|| panic!("no line containing {needle:?} and {other:?}: {logged}"))
        };
        fn call_id_of(line: &str) -> &str {
            line.split("call_id=")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .unwrap_or_else(|| panic!("no call_id field in line: {line}"))
        }

        assert!(
            !logged.contains("logged_ok_tool"),
            "successful call must not be logged: {logged}"
        );

        // Backend/upstream failures stay at WARN on both lines.
        let failed = line_with("logged_failing_tool", "tool call failed");
        assert!(failed.contains("WARN"), "expected WARN: {failed}");
        // Message text lives on the separate, cappable `error_detail` target
        // (see logging.rs's PAYLOAD_CAPS), not on the safe `tool call failed`
        // event itself.
        let failed_detail = line_with("logged_failing_tool", "tool call error detail");
        assert!(
            failed_detail.contains("WARN") && failed_detail.contains("upstream 500"),
            "expected WARN detail line: {failed_detail}"
        );
        assert!(
            failed_detail.contains("sym=BADSTOCK"),
            "expected the caller's input params on the detail line: {failed_detail}"
        );
        // Both of this call's lines carry the same call_id, so a reader can
        // pair them up even if another call's lines interleave.
        assert_eq!(call_id_of(failed), call_id_of(failed_detail));

        // A caller mistake (INVALID_PARAMS) is routine, not an incident — both
        // its classification and detail line are downgraded to INFO, not WARN,
        // so enabling payload logging doesn't reintroduce WARN-level noise for
        // routine typos.
        let rejected = line_with("logged_bad_params_tool", "tool call rejected");
        assert!(rejected.contains("INFO"), "expected INFO: {rejected}");
        let rejected_detail = line_with("logged_bad_params_tool", "tool call error detail");
        assert!(
            rejected_detail.contains("INFO"),
            "expected INFO detail line: {rejected_detail}"
        );
        assert!(
            rejected_detail.contains("sym=???"),
            "expected the caller's input params on the detail line: {rejected_detail}"
        );
        assert_eq!(call_id_of(rejected), call_id_of(rejected_detail));

        // Different calls get different ids.
        assert_ne!(call_id_of(failed), call_id_of(rejected));
    }

    #[test]
    fn recoverable_of_classifies_each_action_class() {
        use super::recoverable_of;
        let cases = [
            ("openapi error: code=401103: token is expired", "reauth"),
            (
                "openapi error: code=403308: Target API's scope is not in authorized scopes",
                "reauth",
            ),
            (
                "openapi error: code=429003: minimum interval between two calls",
                "backoff",
            ),
            (
                "response error: 7: detail:Some(WsResponseErrorDetail { code: 301607, msg: \"too many symbols in one page\" })",
                "fix_params",
            ),
            (
                "response error: 7: detail:Some(WsResponseErrorDetail { code: 301604, msg: \"no quote access\" })",
                "none",
            ),
            (
                "response error: 7: detail:Some(WsResponseErrorDetail { code: 301603, msg: \"no quotes\" })",
                "none",
            ),
            ("something we have never seen code=999999", "none"),
        ];
        for (message, expected) in cases {
            let err = McpError::internal_error(message.to_string(), None);
            assert_eq!(recoverable_of(&err), expected, "message: {message:?}");
        }
        assert_eq!(
            recoverable_of(&McpError::invalid_params("bad period", None)),
            "fix_params",
            "INVALID_PARAMS must be fix_params"
        );
    }

    #[test]
    fn is_terminal_none_only_for_no_access_and_no_quotes() {
        use super::is_terminal_none;
        for msg in ["no quote access", "no quotes"] {
            let err = McpError::internal_error(
                format!("WsResponseErrorDetail {{ code: 301604, msg: \"{msg}\" }}"),
                None,
            );
            assert!(is_terminal_none(&err), "should be terminal: {msg}");
        }
        for msg in [
            "token is expired",
            "no access to trade",
            "rate limit reached",
        ] {
            let err = McpError::internal_error(msg.to_string(), None);
            assert!(!is_terminal_none(&err), "should NOT be terminal: {msg}");
        }
    }

    #[test]
    fn scope_error_hint_warns_a_plain_refresh_wont_help() {
        let err = McpError::internal_error(
            "openapi error: code=403308: Target API's scope is not in authorized scopes"
                .to_string(),
            None,
        );
        let hint = error_hint(&err).expect("expected a scope hint");
        assert!(
            hint.contains("re-authorize") && hint.contains("scope"),
            "scope hint should tell the user to re-authorize granting the scope, got: {hint}"
        );
    }

    #[test]
    fn too_many_symbols_hint_tells_caller_to_reduce_symbols() {
        let err = McpError::internal_error(
            "WsResponseErrorDetail { code: 301607, msg: \"too many symbols in one page\" }"
                .to_string(),
            None,
        );
        let hint = error_hint(&err).expect("expected a 301607 hint");
        assert!(
            hint.contains("fewer") || hint.contains("reduce"),
            "got: {hint}"
        );
    }

    #[test]
    fn minimal_valid_instance_fills_required_fields_with_typed_zeros() {
        use serde_json::json;
        let schema: rmcp::model::JsonObject = serde_json::from_value(json!({
            "type": "object",
            "required": ["name", "count", "flag", "items", "nested"],
            "properties": {
                "name": {"type": "string"},
                "count": {"type": "integer"},
                "flag": {"type": "boolean"},
                "items": {"type": "array"},
                "nested": {
                    "type": "object",
                    "required": ["inner"],
                    "properties": {"inner": {"type": "number"}}
                },
                "optional_ignored": {"type": "string"}
            }
        }))
        .unwrap();
        let out = super::minimal_valid_instance(&schema);
        assert_eq!(
            out,
            json!({
                "name": "", "count": 0, "flag": false, "items": [],
                "nested": {"inner": 0}
            })
        );
    }

    #[test]
    fn minimal_valid_instance_prefers_first_enum_value() {
        use serde_json::json;
        let schema: rmcp::model::JsonObject = serde_json::from_value(json!({
            "type": "object",
            "required": ["status"],
            "properties": {"status": {"type": "string", "enum": ["open", "closed"]}}
        }))
        .unwrap();
        assert_eq!(
            super::minimal_valid_instance(&schema),
            json!({"status": "open"})
        );
    }

    #[test]
    fn output_schema_map_covers_the_schema_backed_terminal_tools() {
        // Only the object-rooted terminal tools that actually declare an
        // `output_schema` need schema-valid structured content; those are the
        // members of `TERMINAL_OBJECT_ROOTED`. (Object-rooted-but-schemaless
        // tools like static_info/intraday/capital_flow/calc_indexes carry no
        // schema contract and leave structuredContent unset.)
        let map = super::output_schema_map();
        for name in super::TERMINAL_OBJECT_ROOTED {
            assert!(map.contains_key(*name), "schema map missing {name}");
        }
    }
}
