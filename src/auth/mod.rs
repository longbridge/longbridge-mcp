pub mod metadata;
pub mod middleware;

use std::sync::Arc;

use axum::Router;
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

async fn tools_json() -> axum::Json<&'static serde_json::Value> {
    static TOOLS_JSON: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        // Order: tools → scopes → locales. Relies on serde_json's
        // `preserve_order` feature.
        let mut out = serde_json::Map::new();
        out.insert(
            "tools".to_string(),
            serde_json::to_value(tools::list_tools()).expect("tool list must be JSON-serialisable"),
        );
        let scopes: serde_json::Value = serde_json::from_str(include_str!("../../scopes.json"))
            .expect("scopes.json must be valid JSON");
        if let serde_json::Value::Object(scopes_map) = scopes {
            for (k, v) in scopes_map {
                // Live tool list always wins over any `tools` in scopes.json.
                out.entry(k).or_insert(v);
            }
        }
        out.insert(
            "locales".to_string(),
            build_locales_node(&[TOOL_LOCALES, SCOPE_LOCALES]),
        );
        serde_json::Value::Object(out)
    });
    axum::Json(&*TOOLS_JSON)
}

async fn scopes_json() -> axum::Json<&'static serde_json::Value> {
    static SCOPES_JSON: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        let mut out: serde_json::Map<String, serde_json::Value> =
            match serde_json::from_str(include_str!("../../scopes.json")) {
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

pub struct AppState {
    pub base_url: String,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let metadata_routes = Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            axum::routing::get(metadata::protected_resource_metadata),
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

    let metrics_route = Router::new().route(
        "/metrics",
        axum::routing::get(crate::metrics::metrics_handler),
    );

    let tools_route: Router = Router::new()
        .route("/mcp/tools.json", axum::routing::get(tools_json))
        .route("/mcp/scopes.json", axum::routing::get(scopes_json));

    let mcp_service = StreamableHttpService::new(
        move || Ok(Longbridge),
        Arc::new(NeverSessionManager::default()),
        StreamableHttpServerConfig::default()
            .with_stateful_mode(false)
            .disable_allowed_hosts(),
    );

    // Auth middleware layer: extracts Bearer token into extensions
    let base_url = state.base_url.clone();
    let mcp_with_auth = tower::ServiceBuilder::new()
        .layer(axum::middleware::from_fn(
            move |req: axum::extract::Request, next: axum::middleware::Next| {
                let base_url = base_url.clone();
                async move { middleware::mcp_auth_layer(req, next, &base_url).await }
            },
        ))
        .service(mcp_service);

    Router::new()
        .merge(metadata_routes)
        .merge(server_card_route)
        .merge(health_route)
        .merge(metrics_route)
        .merge(tools_route)
        .nest_service("/mcp", mcp_with_auth)
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
            serde_json::from_str(include_str!("../../scopes.json")).expect("scopes.json valid");
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
