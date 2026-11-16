pub mod jwt;
pub mod metadata;
pub mod middleware;
mod oauth_proxy;

use std::sync::Arc;

use axum::Router;
use axum::response::Response;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::tools::{self, Longbridge};

/// Largest MCP request body accepted. rmcp reads the whole body into memory
/// before parsing it, and nothing else bounds it (axum's default limit only
/// applies to axum's own extractors), so without this a few concurrent
/// multi-hundred-MB POSTs — `/agent` needs no token — push the pod past its
/// memory limit. Real requests are a JSON-RPC tool call: ~2.4 KB on average in
/// production (Istio, 7 days); the largest legitimate arguments are a topic
/// body (1 MiB ≈ 350k CJK characters) and a 4096-char `_jq`.
const MAX_MCP_REQUEST_BYTES: usize = 1024 * 1024;

/// Overall deadline for reading one MCP request body. A real request is
/// a few KB sent at once; this bounds how long a slow sender (a client
/// dripping one byte at a time) can hold a body-read slot.
const BODY_READ_TIMEOUT: std::time::Duration = if cfg!(test) {
    std::time::Duration::from_secs(1)
} else {
    std::time::Duration::from_secs(10)
};
/// Request bodies being read at once. With [`MAX_MCP_REQUEST_BYTES`] this
/// bounds the memory held by in-flight bodies at 256 MiB.
const MAX_CONCURRENT_BODY_READS: usize = 256;

/// Enforce [`MAX_MCP_REQUEST_BYTES`] on an MCP request.
///
/// - A declared `Content-Length` over the limit gets 413 before anything is
///   read.
/// - Any other body is read here, never past the limit (413 if it would
///   exceed it), and passed on as one buffer. Each frame is copied out as it
///   arrives: keeping hyper's frames would pin a whole read buffer (8 KiB or
///   more) per frame, so a client sending 1-byte chunks could make ~12 KB of
///   body cost ~90 MB of memory, whatever the byte limit.
/// - The read must finish within [`BODY_READ_TIMEOUT`] (else 408), and at
///   most [`MAX_CONCURRENT_BODY_READS`] run at once (else 503 once the same
///   deadline passes).
///
/// Reading here rather than letting rmcp do it also keeps statuses right:
/// rmcp reports a failed body read as 500, a server fault.
async fn limit_request_body(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;
    static BODY_READS: tokio::sync::Semaphore =
        tokio::sync::Semaphore::const_new(MAX_CONCURRENT_BODY_READS);
    let too_large = || {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("request body is larger than {MAX_MCP_REQUEST_BYTES} bytes"),
        )
            .into_response()
    };
    let declared = request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if declared.is_some_and(|length| length > MAX_MCP_REQUEST_BYTES as u64) {
        return too_large();
    }
    let deadline = tokio::time::Instant::now() + BODY_READ_TIMEOUT;
    let Ok(Ok(_permit)) = tokio::time::timeout_at(deadline, BODY_READS.acquire()).await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "too many requests are being received; retry shortly",
        )
            .into_response();
    };
    let (parts, mut body) = request.into_parts();
    // Sized from the declared length when present (already checked against
    // the limit), so a normal request is read without reallocating.
    let mut buffer = Vec::with_capacity(declared.map_or(0, |length| length as usize));
    let read = async {
        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|error| error.to_string())?;
            if let Some(data) = frame.data_ref() {
                if buffer.len() + data.len() > MAX_MCP_REQUEST_BYTES {
                    return Ok(false);
                }
                buffer.extend_from_slice(data);
            }
        }
        Ok::<bool, String>(true)
    };
    match tokio::time::timeout_at(deadline, read).await {
        Ok(Ok(true)) => {}
        Ok(Ok(false)) => return too_large(),
        Ok(Err(error)) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("failed to read request body: {error}"),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::REQUEST_TIMEOUT,
                format!(
                    "request body was not received within {}s",
                    BODY_READ_TIMEOUT.as_secs()
                ),
            )
                .into_response();
        }
    }
    next.run(axum::extract::Request::from_parts(
        parts,
        axum::body::Body::from(buffer),
    ))
    .await
}

/// Per-locale embedded translation files. Each entry is one
/// `(language code, content of locales/<code>/<filename>)` pair.
const TOOL_LOCALES: &[(&str, &str)] = &[
    ("zh-CN", include_str!("../../locales/zh-CN/tools.json")),
    ("zh-HK", include_str!("../../locales/zh-HK/tools.json")),
    ("ja", include_str!("../../locales/ja/tools.json")),
];

const SCOPE_LOCALES: &[(&str, &str)] = &[
    ("zh-CN", include_str!("../../locales/zh-CN/scopes.json")),
    ("zh-HK", include_str!("../../locales/zh-HK/scopes.json")),
    ("ja", include_str!("../../locales/ja/scopes.json")),
];

/// Build the `locales` node merging zero or more per-locale source files
/// (e.g. `tools.json` + `scopes.json`) per language. Top-level keys from
/// each source are kept side-by-side under that language's entry.
fn build_locales_node(sources: &[&[(&str, &str)]]) -> serde_json::Value {
    let mut locales = serde_json::Map::new();
    for src in sources {
        for (code, raw) in src.iter() {
            // Translations name the production connect page too; retarget it the
            // same way the live descriptions are (see
            // `crate::tools::all_tools_full_cached`). A no-op in production.
            let raw = raw.replace(
                crate::endpoints::STATIC_CONNECT_PAGE,
                crate::endpoints::connect_page_url(),
            );
            let parsed: serde_json::Value =
                serde_json::from_str(&raw).expect("locale file must be valid JSON");
            let entry = locales
                .entry((*code).to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let (serde_json::Value::Object(target), serde_json::Value::Object(parsed_map)) =
                (entry, parsed)
            {
                for (k, v) in parsed_map {
                    target.insert(k, v);
                }
            }
        }
    }
    serde_json::Value::Object(locales)
}

/// Rewrite each scope catalogue entry's `id` to the current environment's id.
///
/// `data/scopes.json` records production ids, but the authorization server
/// numbers the same concepts differently on canary (see
/// [`crate::endpoints::Scope`]). The `key` field is stable across environments,
/// so it drives the rewrite. A no-op in production, and entries with no
/// matching [`crate::endpoints::Scope`] (e.g. the `General` bucket) are left
/// untouched.
fn retarget_scope_ids(scopes: &mut serde_json::Value) {
    let Some(entries) = scopes.as_array_mut() else {
        return;
    };
    let environment = crate::endpoints::current();
    for entry in entries {
        let Some(key) = entry.get("key").and_then(|k| k.as_str()) else {
            continue;
        };
        let Some(scope) = crate::endpoints::SCOPES.iter().find(|s| s.key() == key) else {
            continue;
        };
        let id = scope.id(environment).to_string();
        if let Some(slot) = entry.get_mut("id") {
            *slot = serde_json::Value::String(id);
        }
    }
}

/// Build the `/mcp/tools.json` (and `/v1/tools.json`, `/v2/tools.json`) body.
///
/// Order: tools → scopes → locales (relies on serde_json's `preserve_order`).
/// When `allow` is `Some`, the tool list and every scope's `tools` array are
/// pruned to that allowlist and scopes left with no tools are dropped — so a
/// restricted document never advertises a capability the endpoint blocks.
/// Locales (a translation dictionary keyed by tool name) are carried verbatim.
fn build_tools_json(allow: Option<&std::collections::HashSet<&'static str>>) -> serde_json::Value {
    let mut out = serde_json::Map::new();

    let tool_list = match allow {
        Some(allow) => tools::list_tools()
            .into_iter()
            .filter(|t| allow.contains(t.name.as_ref()))
            .collect(),
        None => tools::list_tools(),
    };
    let tools: serde_json::Value =
        serde_json::to_value(tool_list).expect("tool list must be JSON-serialisable");
    out.insert("tools".to_string(), tools);

    let scopes: serde_json::Value = serde_json::from_str(include_str!("../../data/scopes.json"))
        .expect("scopes.json must be valid JSON");
    if let serde_json::Value::Object(scopes_map) = scopes {
        for (k, mut v) in scopes_map {
            if let (Some(allow), Some(arr)) = (allow, v.as_array_mut()) {
                arr.retain_mut(|scope| {
                    let Some(tools_arr) = scope.get_mut("tools").and_then(|t| t.as_array_mut())
                    else {
                        return false;
                    };
                    tools_arr.retain(|t| t.as_str().is_some_and(|s| allow.contains(s)));
                    !tools_arr.is_empty()
                });
            }
            retarget_scope_ids(&mut v);
            // Live tool list always wins over any `tools` in scopes.json.
            out.entry(k).or_insert(v);
        }
    }
    out.insert(
        "locales".to_string(),
        build_locales_node(&[TOOL_LOCALES, SCOPE_LOCALES]),
    );
    serde_json::Value::Object(out)
}

async fn tools_json() -> axum::Json<&'static serde_json::Value> {
    static TOOLS_JSON: std::sync::LazyLock<serde_json::Value> =
        std::sync::LazyLock::new(|| build_tools_json(None));
    axum::Json(&*TOOLS_JSON)
}

/// Restricted tool manifest for the public `/v2` endpoint: the broader
/// allowlist (read-only account/portfolio + order history, but no trade
/// execution, DCA, IPO orders, or money movement), with scopes pruned to match.
async fn v2_tools_json() -> axum::Json<&'static serde_json::Value> {
    static V2_TOOLS_JSON: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        let allow: std::collections::HashSet<&'static str> =
            tools::v2_tool_names().into_iter().collect();
        build_tools_json(Some(&allow))
    });
    axum::Json(&*V2_TOOLS_JSON)
}

async fn scopes_json() -> axum::Json<&'static serde_json::Value> {
    static SCOPES_JSON: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        let mut out: serde_json::Map<String, serde_json::Value> =
            match serde_json::from_str(include_str!("../../data/scopes.json")) {
                Ok(serde_json::Value::Object(m)) => m,
                _ => panic!("scopes.json must be a JSON object"),
            };
        if let Some(scopes) = out.get_mut("scopes") {
            retarget_scope_ids(scopes);
        }
        // Only the scope-specific locale files — no tool translations here.
        out.insert("locales".to_string(), build_locales_node(&[SCOPE_LOCALES]));
        serde_json::Value::Object(out)
    });
    axum::Json(&*SCOPES_JSON)
}

async fn health() -> axum::http::StatusCode {
    axum::http::StatusCode::OK
}

/// Human-facing landing page served at `/` for browser visits. Static
/// onboarding text only — see the file for content.
const LANDING_PAGE_HTML: &str = include_str!("landing.html");

/// Placeholder substituted with the request's resolved public URL (see
/// [`metadata::public_url_from_headers`]) before the landing page is served.
/// Must match the literal URL baked into `landing.html`'s onboarding snippets.
const LANDING_PAGE_URL_PLACEHOLDER: &str = "https://mcp.longbridge.com";

/// Replaced with the crate version embedded by Cargo at compile time.
const LANDING_PAGE_VERSION_PLACEHOLDER: &str = "{{VERSION}}";

/// Serve [`LANDING_PAGE_HTML`] only for a browser navigation to `/`
/// (a `GET` whose `Accept` header asks for `text/html`). Every other request —
/// JSON-RPC `POST`, the SSE `GET` with `Accept: text/event-stream`, `Accept:
/// */*`, any non-root path — falls through untouched to the wrapped MCP service,
/// so client connection/installation is never affected. Layered ahead of the
/// auth middleware so the page renders without credentials.
///
/// The page's onboarding snippets hardcode [`LANDING_PAGE_URL_PLACEHOLDER`];
/// it's replaced here with the same resolved public URL the OAuth metadata
/// endpoints advertise (see [`metadata::public_url_from_headers`]), so a
/// visitor arriving via an allowlisted global `X-Host` entry gets copy-paste
/// snippets pointing at the domain they're actually using instead of the
/// origin's own hostname.
async fn landing_or_mcp(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    use axum::response::IntoResponse;

    let is_browser_root = req.method() == axum::http::Method::GET
        && req.uri().path() == "/"
        && req
            .headers()
            .get(axum::http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"));

    if is_browser_root {
        let public = metadata::public_url_from_headers(req.headers(), &state.base_url);
        let html = LANDING_PAGE_HTML
            .replace(LANDING_PAGE_URL_PLACEHOLDER, &public.url)
            .replace(LANDING_PAGE_VERSION_PLACEHOLDER, env!("CARGO_PKG_VERSION"));
        return (
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response();
    }

    next.run(req).await
}

/// Domain-verification token for the OpenAI Apps directory, served as plain text
/// at `/.well-known/openai-apps-challenge`. OpenAI fetches this path and expects
/// the exact token back to confirm ownership of the deployment.
const OPENAI_APPS_CHALLENGE: &str = "h5mgJvzRNVwtVXfT4QYbhlGhhNoni0WdHUFwIrZpbpE";

async fn openai_apps_challenge() -> &'static str {
    OPENAI_APPS_CHALLENGE
}

pub struct AppState {
    pub base_url: String,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let metadata_routes = Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            axum::routing::get(metadata::protected_resource_metadata),
        )
        // RFC 9728 resource-specific metadata for the restricted `/v2` endpoint.
        // The `/v2` resource URL identifies the endpoint; advertised scopes
        // mirror the authorization server ids while excluding trade execution.
        .route(
            "/.well-known/oauth-protected-resource/v2",
            axum::routing::get(metadata::protected_resource_metadata_v2),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            axum::routing::get(metadata::authorization_server_metadata),
        )
        .with_state(state.clone());

    // Serve the static server card at both the host-root well-known path and
    // the `/mcp/`-prefixed path (some upstream gateways only forward
    // `/mcp/*` to this service, so the prefixed copy is a fallback).
    let server_card_route = Router::new()
        .route(
            "/.well-known/mcp/server-card.json",
            axum::routing::get(metadata::server_card),
        )
        .route(
            "/mcp/.well-known/mcp/server-card.json",
            axum::routing::get(metadata::server_card),
        );

    let health_route = Router::new().route("/health", axum::routing::get(health));

    let openai_apps_challenge_route = Router::new().route(
        "/.well-known/openai-apps-challenge",
        axum::routing::get(openai_apps_challenge),
    );

    let metrics_route = Router::new().route(
        "/metrics",
        axum::routing::get(crate::metrics::metrics_handler),
    );

    let tools_route: Router = Router::new()
        .route("/mcp/tools.json", axum::routing::get(tools_json))
        .route("/mcp/scopes.json", axum::routing::get(scopes_json))
        // Restricted public manifest for the `/v2` endpoint (allowlist only).
        .route("/v2/tools.json", axum::routing::get(v2_tools_json));

    let oauth_proxy_routes: Router = Router::new()
        .route(
            "/oauth2/authorize",
            axum::routing::get(oauth_proxy::authorize),
        )
        .route("/oauth2/token", axum::routing::post(oauth_proxy::token))
        .route("/oauth2/revoke", axum::routing::post(oauth_proxy::revoke))
        .with_state(state.clone());

    // Build an auth-wrapped MCP service for one mounting point. The same
    // `Longbridge` MCP server is mounted several times; the only thing that
    // differs is the auth `mode`:
    //   - `AuthMode::Required` for the main endpoints (`/mcp` and root): a
    //     missing token yields 401, exactly as before this feature.
    //   - `AuthMode::Optional` for the `/agent` endpoint: token-less requests
    //     are allowed through into the `authenticate` reverse-auth flow.
    let make_mcp_with_auth =
        |base_url: String,
         mode: middleware::AuthMode,
         restricted: Option<middleware::RestrictedVersion>| {
            let svc = StreamableHttpService::new(
                move || Ok(Longbridge),
                Arc::new(NeverSessionManager::default()),
                StreamableHttpServerConfig::default()
                    .with_stateful_mode(false)
                    .disable_allowed_hosts(),
            );
            tower::ServiceBuilder::new()
                // Outermost, so oversized bodies are cut off before auth or
                // rmcp read anything.
                .layer(axum::middleware::from_fn(limit_request_body))
                .layer(axum::middleware::from_fn(
                    move |req: axum::extract::Request, next: axum::middleware::Next| {
                        let base_url = base_url.clone();
                        async move {
                            middleware::mcp_auth_layer(req, next, &base_url, mode, restricted).await
                        }
                    },
                ))
                .service(svc)
        };

    // Main endpoints — Bearer required (token-less -> 401). Mounted at both
    // `/mcp` and root so deployments that strip or omit the `/mcp` prefix work.
    let mcp_with_auth =
        make_mcp_with_auth(state.base_url.clone(), middleware::AuthMode::Required, None);
    let mcp_with_auth_root =
        make_mcp_with_auth(state.base_url.clone(), middleware::AuthMode::Required, None);
    // Root mount, plus a thin front layer that serves the human landing page for
    // browser GETs to `/`. All programmatic MCP traffic passes straight through.
    let root_service = tower::ServiceBuilder::new()
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            landing_or_mcp,
        ))
        .service(mcp_with_auth_root);
    // Optional-auth endpoint — same MCP server, but token-less requests are let
    // through so an OAuth-incapable client can call the `authenticate` tool.
    let mcp_agent =
        make_mcp_with_auth(state.base_url.clone(), middleware::AuthMode::Optional, None);
    // Restricted public endpoint — Bearer required, but only the curated
    // allowlist is listed and callable. `/v2` is the read surface submitted to
    // third-party app directories (OpenAI/Claude/Grok): read-only market
    // analysis plus read-only account/portfolio + order history, never trade
    // execution, DCA, IPO orders, or money movement.
    let mcp_v2 = make_mcp_with_auth(
        state.base_url.clone(),
        middleware::AuthMode::Required,
        Some(middleware::RestrictedVersion::V2),
    );

    Router::new()
        .merge(metadata_routes)
        .merge(server_card_route)
        .merge(health_route)
        .merge(openai_apps_challenge_route)
        .merge(metrics_route)
        .merge(tools_route)
        .merge(oauth_proxy_routes)
        .nest_service("/agent", mcp_agent)
        .nest_service("/v2", mcp_v2)
        .nest_service("/mcp", mcp_with_auth)
        // Also serve at root so deployments that omit the /mcp path prefix work.
        .fallback_service(root_service)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;

    use super::{AppState, MAX_MCP_REQUEST_BYTES, create_router};

    fn mcp_post(body: Body) -> Request<Body> {
        Request::post("/agent")
            .header(header::HOST, "127.0.0.1:8000")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .body(body)
            .expect("request should build")
    }

    fn router() -> axum::Router {
        create_router(Arc::new(AppState {
            base_url: "http://127.0.0.1:8000".to_string(),
        }))
    }

    /// A declared length over the limit is refused before anything is read.
    #[tokio::test]
    async fn oversized_mcp_request_with_content_length_gets_413() {
        let body = vec![b' '; MAX_MCP_REQUEST_BYTES + 1];
        let mut request = mcp_post(Body::from(body));
        request
            .headers_mut()
            .insert(header::CONTENT_LENGTH, (MAX_MCP_REQUEST_BYTES + 1).into());
        let response = router()
            .oneshot(request)
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    /// A chunked body with no length is read only up to the limit, not
    /// buffered whole: `/agent` needs no token, so before this a few
    /// concurrent multi-hundred-MB bodies could exhaust the pod's memory.
    #[tokio::test]
    async fn oversized_chunked_mcp_request_is_cut_off_at_the_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        const CHUNK: usize = 64 * 1024;
        const TOTAL: usize = 64 * 1024 * 1024;
        let produced = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&produced);
        let stream = futures_util::stream::iter((0..TOTAL / CHUNK).map(move |_| {
            counter.fetch_add(CHUNK, Ordering::Relaxed);
            Ok::<_, std::io::Error>(axum::body::Bytes::from(vec![b' '; CHUNK]))
        }));
        let response = router()
            .oneshot(mcp_post(Body::from_stream(stream)))
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        // Read up to the limit (so it was the limit that stopped it, not an
        // earlier rejection), and not much further.
        let read = produced.load(Ordering::Relaxed);
        assert!(
            (MAX_MCP_REQUEST_BYTES..=MAX_MCP_REQUEST_BYTES + 2 * CHUNK).contains(&read),
            "read {read} bytes of a {TOTAL}-byte body"
        );
    }

    /// A body that never finishes is given up on at the read deadline instead
    /// of holding a body-read slot (and its buffer) indefinitely.
    #[tokio::test]
    async fn a_body_that_never_finishes_times_out() {
        use futures_util::StreamExt;
        let stream = futures_util::stream::once(async {
            Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"{"))
        })
        .chain(futures_util::stream::pending());
        let started = std::time::Instant::now();
        let response = router()
            .oneshot(mcp_post(Body::from_stream(stream)))
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    /// Many tiny frames are reassembled into the right bytes.
    #[tokio::test]
    async fn a_body_in_one_byte_frames_is_passed_on_intact() {
        let initialize = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-03-26", "capabilities": {},
                       "clientInfo": {"name": "frames-test", "version": "1"}}
        })
        .to_string();
        let frames: Vec<Result<axum::body::Bytes, std::io::Error>> = initialize
            .into_bytes()
            .into_iter()
            .map(|byte| Ok(axum::body::Bytes::from(vec![byte])))
            .collect();
        let response = router()
            .oneshot(mcp_post(Body::from_stream(futures_util::stream::iter(
                frames,
            ))))
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Ordinary MCP traffic is unaffected.
    #[tokio::test]
    async fn small_mcp_request_passes_the_body_limit() {
        let initialize = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-03-26", "capabilities": {},
                       "clientInfo": {"name": "limit-test", "version": "1"}}
        });
        let response = router()
            .oneshot(mcp_post(Body::from(initialize.to_string())))
            .await
            .expect("router should respond");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    }

    /// `/oauth2/authorize` must be routed to the redirect handler, not swallowed
    /// by the Bearer-protected root MCP fallback, and the redirect must not be
    /// cacheable at the edge.
    #[tokio::test]
    async fn authorize_route_redirects_without_bearer_and_is_not_cacheable() {
        let router = create_router(Arc::new(AppState {
            base_url: "http://127.0.0.1:8000".to_string(),
        }));
        let query = "response_type=code&client_id=abc&state=s%2B1&code_challenge=x";
        let response = router
            .oneshot(
                Request::get(format!("/oauth2/authorize?{query}"))
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .expect("redirect should carry a Location header");
        assert!(
            location.ends_with(&format!("/oauth2/authorize?{query}")),
            "query must be forwarded verbatim, got {location}"
        );
        assert!(
            !location.starts_with("http://127.0.0.1:8000"),
            "authorize must redirect upstream, not to itself: {location}"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&header::HeaderValue::from_static("no-store"))
        );
    }

    /// Guard: in static tool metadata — descriptions, input schemas, and the
    /// translations — the connect page may only appear as
    /// `endpoints::STATIC_CONNECT_PAGE`, optionally with a path suffix such as
    /// `/done`. That exact prefix is what both retarget passes
    /// (`tools::all_tools_full_cached` and `build_locales_node`) replace, so any
    /// other spelling would survive into a canary process and send users to the
    /// production page — whose code then fails the exchange with an opaque
    /// `invalid_grant`.
    #[test]
    fn static_connect_page_urls_all_start_with_the_placeholder() {
        let placeholder = crate::endpoints::STATIC_CONNECT_PAGE;
        let mut sources: Vec<(String, String)> = super::TOOL_LOCALES
            .iter()
            .map(|(code, raw)| (format!("locales/{code}/tools.json"), (*raw).to_string()))
            .collect();
        for tool in crate::tools::list_tools() {
            sources.push((
                format!("tool `{}`", tool.name),
                format!(
                    "{} {}",
                    tool.description.as_deref().unwrap_or_default(),
                    serde_json::to_string(&tool.input_schema).expect("schema must serialize")
                ),
            ));
        }

        let mut offenders = Vec::new();
        for (origin, text) in &sources {
            for (at, _) in text.match_indices("https://open.longbridge") {
                if !text[at..].starts_with(placeholder) {
                    let end = (at + placeholder.len() + 8).min(text.len());
                    offenders.push(format!("{origin}: {}…", &text[at..end]));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "static tool metadata must spell the connect page as `{placeholder}` \
             so it can be retargeted for canary. Offending occurrences:\n{}",
            offenders.join("\n")
        );
    }

    /// Every locale file must cover every registered tool, and every locale
    /// param key must exist in the live JSON Schema. Catches drift when a
    /// new tool is added without its translation, or a tool is renamed and
    /// the locale entry becomes orphaned.
    #[test]
    fn locales_match_live_tool_schema() {
        let live = crate::tools::list_tools();
        let live_names: HashSet<String> = live.iter().map(|t| t.name.to_string()).collect();
        let live_params: std::collections::HashMap<String, HashSet<String>> = live
            .iter()
            .map(|t| {
                let props = t
                    .input_schema
                    .as_ref()
                    .get("properties")
                    .and_then(|v| v.as_object())
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                (t.name.to_string(), props)
            })
            .collect();

        for (code, raw) in [
            ("zh-CN", include_str!("../../locales/zh-CN/tools.json")),
            ("zh-HK", include_str!("../../locales/zh-HK/tools.json")),
            ("ja", include_str!("../../locales/ja/tools.json")),
        ] {
            let parsed: serde_json::Value =
                serde_json::from_str(raw).expect("locale file must be valid JSON");
            let tools = parsed
                .get("tools")
                .and_then(|v| v.as_object())
                .expect("locale file must have a `tools` object");

            let loc_names: HashSet<String> = tools.keys().cloned().collect();
            let missing: Vec<_> = live_names.difference(&loc_names).collect();
            let extra: Vec<_> = loc_names.difference(&live_names).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "[{code}] tool drift — missing translation: {missing:?}, orphan locale entry: {extra:?}"
            );

            for (tool_name, entry) in tools {
                let Some(params) = entry.get("properties").and_then(|v| v.as_object()) else {
                    continue;
                };
                let real_params = live_params.get(tool_name).expect("name already validated");
                for pname in params.keys() {
                    assert!(
                        real_params.contains(pname),
                        "[{code}] tool '{tool_name}' has locale param '{pname}' not in live schema"
                    );
                }
            }
        }
    }

    /// snake_case identifiers (`last_done`, `published_at`) in `text`: the
    /// field and parameter names translations keep verbatim in English.
    fn identifiers(text: &str) -> HashSet<String> {
        text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|word| {
                word.contains('_')
                    && word.starts_with(|c: char| c.is_ascii_lowercase())
                    && word.split('_').all(|part| {
                        !part.is_empty()
                            && part
                                .chars()
                                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                    })
            })
            .map(str::to_string)
            .collect()
    }

    /// A translated description must not name a field the English one does
    /// not: when an English description is corrected (a field renamed or
    /// dropped) and its translation is not, this fails. Every field the
    /// translation names must appear in the English description, among the
    /// tool's parameters, or be another tool's name.
    #[test]
    fn locale_descriptions_only_name_fields_the_english_one_names() {
        let live = crate::tools::list_tools();
        let tool_names: HashSet<String> = live.iter().map(|t| t.name.to_string()).collect();
        let mut stale = Vec::new();
        for (code, raw) in [
            ("zh-CN", include_str!("../../locales/zh-CN/tools.json")),
            ("zh-HK", include_str!("../../locales/zh-HK/tools.json")),
            ("ja", include_str!("../../locales/ja/tools.json")),
        ] {
            let parsed: serde_json::Value =
                serde_json::from_str(raw).expect("locale file must be valid JSON");
            let tools = parsed["tools"]
                .as_object()
                .expect("locale file must have a `tools` object");
            for tool in &live {
                let Some(translated) = tools
                    .get(tool.name.as_ref())
                    .and_then(|t| t["description"].as_str())
                else {
                    continue;
                };
                let english = tool.description.as_deref().unwrap_or_default();
                let mut known = identifiers(english);
                if let Some(props) = tool
                    .input_schema
                    .get("properties")
                    .and_then(|p| p.as_object())
                {
                    known.extend(props.keys().cloned());
                }
                let mut unknown: Vec<String> = identifiers(translated)
                    .into_iter()
                    .filter(|id| !known.contains(id) && !tool_names.contains(id))
                    .collect();
                if !unknown.is_empty() {
                    unknown.sort();
                    stale.push(format!("[{code}] {}: {}", tool.name, unknown.join(", ")));
                }
            }
        }
        assert!(
            stale.is_empty(),
            "translated descriptions name fields their English description does not (update the \
             translation):\n{}",
            stale.join("\n")
        );
    }

    /// Every scope locale file must cover the same English scope `name`s as
    /// `scopes.json`. Locale entries are keyed by the English `name` (mirrors
    /// the tools-locale shape, which keys by tool name). Catches drift when a
    /// scope is added/removed/renamed without a corresponding update to the
    /// per-language scope translation.
    #[test]
    fn scope_locales_match_scopes_json() {
        let scopes: serde_json::Value =
            serde_json::from_str(include_str!("../../data/scopes.json"))
                .expect("scopes.json valid");
        let real_names: HashSet<String> = scopes
            .get("scopes")
            .and_then(|v| v.as_array())
            .expect("scopes.json must have `scopes` array")
            .iter()
            .filter_map(|s| s.get("name").and_then(|v| v.as_str()).map(String::from))
            .collect();

        for (code, raw) in [
            ("zh-CN", include_str!("../../locales/zh-CN/scopes.json")),
            ("zh-HK", include_str!("../../locales/zh-HK/scopes.json")),
            ("ja", include_str!("../../locales/ja/scopes.json")),
        ] {
            let parsed: serde_json::Value =
                serde_json::from_str(raw).expect("scope locale file must be valid JSON");
            let loc_scopes = parsed
                .get("scopes")
                .and_then(|v| v.as_object())
                .expect("scope locale file must have `scopes` object");
            let loc_names: HashSet<String> = loc_scopes.keys().cloned().collect();
            let missing: Vec<_> = real_names.difference(&loc_names).collect();
            let extra: Vec<_> = loc_names.difference(&real_names).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "[{code}] scope name drift — missing translation: {missing:?}, orphan locale entry: {extra:?}"
            );
        }
    }

    /// Catches drift if a future edit to `landing.html` changes or removes
    /// the hardcoded URL without updating `LANDING_PAGE_URL_PLACEHOLDER` —
    /// otherwise `landing_or_mcp`'s substitution would silently become a
    /// no-op and the page would go back to always showing the origin domain.
    #[test]
    fn landing_page_contains_the_url_placeholder() {
        let count = super::LANDING_PAGE_HTML
            .matches(super::LANDING_PAGE_URL_PLACEHOLDER)
            .count();
        assert!(
            count > 0,
            "landing.html no longer contains {:?} — update LANDING_PAGE_URL_PLACEHOLDER \
             (or drop the substitution if the page no longer needs it)",
            super::LANDING_PAGE_URL_PLACEHOLDER
        );
    }

    /// Every occurrence of the placeholder must be replaced — a visitor
    /// arriving via an allowlisted global `X-Host` should see that domain in
    /// every onboarding snippet, not a mix of the new and origin domains.
    #[test]
    fn landing_page_substitution_replaces_every_occurrence() {
        let dynamic_url = "https://mcp-global.longbridge.xyz";
        let html =
            super::LANDING_PAGE_HTML.replace(super::LANDING_PAGE_URL_PLACEHOLDER, dynamic_url);
        assert!(!html.contains(super::LANDING_PAGE_URL_PLACEHOLDER));
        let expected_count = super::LANDING_PAGE_HTML
            .matches(super::LANDING_PAGE_URL_PLACEHOLDER)
            .count();
        assert_eq!(html.matches(dynamic_url).count(), expected_count);
    }

    #[test]
    fn landing_page_version_matches_the_crate_version() {
        assert!(
            super::LANDING_PAGE_HTML.contains(super::LANDING_PAGE_VERSION_PLACEHOLDER),
            "landing.html must contain the version placeholder"
        );
        let html = super::LANDING_PAGE_HTML.replace(
            super::LANDING_PAGE_VERSION_PLACEHOLDER,
            env!("CARGO_PKG_VERSION"),
        );
        assert!(!html.contains(super::LANDING_PAGE_VERSION_PLACEHOLDER));
        assert!(html.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    }
}
