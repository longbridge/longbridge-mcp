pub mod admin;
pub mod longbridge;
pub mod metadata;
pub mod middleware;
pub mod server;
pub mod token;

use std::sync::Arc;

use axum::Router;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

use crate::registry::UserRegistry;
use crate::tools::Longbridge;

pub struct AppState {
    pub registry: Arc<UserRegistry>,
    pub jwt_secret: Vec<u8>,
    pub base_url: String,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let oauth_routes = server::routes(state.clone());
    let api_routes = Router::new()
        .route("/api/users", axum::routing::get(admin::list_users))
        .route(
            "/api/users/{user_id}",
            axum::routing::delete(admin::delete_user),
        )
        .with_state(state.clone());

    let metrics_route = Router::new().route(
        "/metrics",
        axum::routing::get(crate::metrics::metrics_handler),
    );

    let registry = state.registry.clone();
    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(Longbridge {
                registry: registry.clone(),
            })
        },
        Arc::new(LocalSessionManager::default()),
        Default::default(),
    );

    // Auth middleware layer: validates Bearer token and injects UserIdentity
    let jwt_secret = state.jwt_secret.clone();
    let auth_registry = state.registry.clone();
    let base_url = state.base_url.clone();
    let mcp_with_auth =
        tower::ServiceBuilder::new()
            .layer(axum::middleware::from_fn(
                move |req: axum::extract::Request, next: axum::middleware::Next| {
                    let secret = jwt_secret.clone();
                    let registry = auth_registry.clone();
                    let base_url = base_url.clone();
                    async move {
                        middleware::mcp_auth_layer(req, next, &secret, &registry, &base_url).await
                    }
                },
            ))
            .service(mcp_service);

    Router::new()
        .merge(oauth_routes)
        .merge(api_routes)
        .merge(metrics_route)
        .nest_service("/mcp", mcp_with_auth)
}
