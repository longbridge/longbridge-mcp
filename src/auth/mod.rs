pub mod metadata;
pub mod middleware;

use std::sync::Arc;

use axum::Router;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};

use crate::tools::{self, Longbridge};

async fn tools_json() -> axum::Json<&'static serde_json::Value> {
    static TOOLS_JSON: std::sync::LazyLock<serde_json::Value> =
        std::sync::LazyLock::new(|| serde_json::json!({ "tools": tools::list_tools() }));
    axum::Json(&*TOOLS_JSON)
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

    let health_route = Router::new().route("/health", axum::routing::get(health));

    let metrics_route = Router::new().route(
        "/metrics",
        axum::routing::get(crate::metrics::metrics_handler),
    );

    let tools_route: Router =
        Router::new().route("/mcp/tools.json", axum::routing::get(tools_json));

    let mcp_service = StreamableHttpService::new(
        move || Ok(Longbridge),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default().disable_allowed_hosts(),
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
        .merge(health_route)
        .merge(metrics_route)
        .merge(tools_route)
        .nest_service("/mcp", mcp_with_auth)
}
