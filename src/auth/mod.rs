pub mod metadata;
pub mod middleware;

use std::sync::Arc;

use axum::Router;
use axum::response::Response;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::tools::{self, Longbridge};

/// Per-locale embedded translation files. Each entry is one
/// `(language code, content of locales/<code>/<filename>)` pair.
const TOOL_LOCALES: &[(&str, &str)] = &[
    ("zh-CN", include_str!("../../locales/zh-CN/tools.json")),
    ("zh-HK", include_str!("../../locales/zh-HK/tools.json")),
];

const SCOPE_LOCALES: &[(&str, &str)] = &[
    ("zh-CN", include_str!("../../locales/zh-CN/scopes.json")),
    ("zh-HK", include_str!("../../locales/zh-HK/scopes.json")),
];

/// Build the `locales` node merging zero or more per-locale source files
/// (e.g. `tools.json` + `scopes.json`) per language. Top-level keys from
/// each source are kept side-by-side under that language's entry.
fn build_locales_node(sources: &[&[(&str, &str)]]) -> serde_json::Value {
    let mut locales = serde_json::Map::new();
    for src in sources {
        for (code, raw) in src.iter() {
            let parsed: serde_json::Value =
                serde_json::from_str(raw).expect("locale file must be valid JSON");
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

/// Restricted tool manifest for the public `/v1` endpoint: only the curated
/// read-only analysis allowlist, with scopes pruned to match.
async fn v1_tools_json() -> axum::Json<&'static serde_json::Value> {
    static V1_TOOLS_JSON: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        let allow: std::collections::HashSet<&'static str> =
            tools::v1_tool_names().into_iter().collect();
        build_tools_json(Some(&allow))
    });
    axum::Json(&*V1_TOOLS_JSON)
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

/// Serve [`LANDING_PAGE_HTML`] only for a browser navigation to `/`
/// (a `GET` whose `Accept` header asks for `text/html`). Every other request —
/// JSON-RPC `POST`, the SSE `GET` with `Accept: text/event-stream`, `Accept:
/// */*`, any non-root path — falls through untouched to the wrapped MCP service,
/// so client connection/installation is never affected. Layered ahead of the
/// auth middleware so the page renders without credentials.
async fn landing_or_mcp(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    use axum::response::IntoResponse;

    let is_browser_root = req.method() == axum::http::Method::GET
        && req.uri().path() == "/"
        && req
            .headers()
            .get(axum::http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"));

    if is_browser_root {
        return (
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            LANDING_PAGE_HTML,
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
        // RFC 9728 resource-specific metadata for the restricted `/v1` endpoint:
        // advertises read-only scopes so the OAuth consent screen hides trading.
        .route(
            "/.well-known/oauth-protected-resource/v1",
            axum::routing::get(metadata::protected_resource_metadata_v1),
        )
        // RFC 9728 resource-specific metadata for the restricted `/v2` endpoint.
        .route(
            "/.well-known/oauth-protected-resource/v2",
            axum::routing::get(metadata::protected_resource_metadata_v2),
        )
        .with_state(state.clone());

    // Serve the static server card at both the host-root path Smithery's docs
    // specify and the `/mcp/`-prefixed path (some upstream gateways only forward
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
        // Restricted public manifests for the `/v1` and `/v2` endpoints
        // (allowlist only).
        .route("/v1/tools.json", axum::routing::get(v1_tools_json))
        .route("/v2/tools.json", axum::routing::get(v2_tools_json));

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
        .layer(axum::middleware::from_fn(landing_or_mcp))
        .service(mcp_with_auth_root);
    // Optional-auth endpoint — same MCP server, but token-less requests are let
    // through so an OAuth-incapable client can call the `authenticate` tool.
    let mcp_agent =
        make_mcp_with_auth(state.base_url.clone(), middleware::AuthMode::Optional, None);
    // Restricted public endpoints — Bearer required, but only the version's
    // curated allowlist is listed and callable. `/v1` is the read-only
    // analysis surface submitted to third-party app directories
    // (OpenAI/Claude/Grok); `/v2` is the broader read surface (adds read-only
    // account/portfolio + order history) with the same no-execution guarantee.
    let mcp_v1 = make_mcp_with_auth(
        state.base_url.clone(),
        middleware::AuthMode::Required,
        Some(middleware::RestrictedVersion::V1),
    );
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
        .nest_service("/agent", mcp_agent)
        .nest_service("/v1", mcp_v1)
        .nest_service("/v2", mcp_v2)
        .nest_service("/mcp", mcp_with_auth)
        // Also serve at root so deployments that omit the /mcp path prefix work.
        .fallback_service(root_service)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

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
}
