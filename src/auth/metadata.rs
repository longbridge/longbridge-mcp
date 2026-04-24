use std::sync::{Arc, LazyLock};

use axum::extract::State;
use axum::response::Json;
use serde::Serialize;

use crate::auth::AppState;

fn longbridge_oauth_url() -> String {
    std::env::var("LONGBRIDGE_HTTP_URL")
        .unwrap_or_else(|_| "https://openapi.longbridge.com".to_string())
}

#[derive(Serialize)]
pub(crate) struct ProtectedResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<String>,
}

pub async fn protected_resource_metadata(
    State(state): State<Arc<AppState>>,
) -> Json<ProtectedResourceMetadata> {
    Json(ProtectedResourceMetadata {
        resource: state.base_url.clone(),
        authorization_servers: vec![longbridge_oauth_url()],
        scopes_supported: vec!["openapi".to_string()],
    })
}

#[derive(Serialize)]
struct ServerInfoCard {
    name: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct AuthCard {
    required: bool,
    schemes: Vec<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct ServerCard {
    #[serde(rename = "serverInfo")]
    server_info: ServerInfoCard,
    authentication: AuthCard,
    tools: Vec<rmcp::model::Tool>,
}

static SERVER_CARD: LazyLock<ServerCard> = LazyLock::new(|| ServerCard {
    server_info: ServerInfoCard {
        name: "Longbridge MCP",
        version: env!("CARGO_PKG_VERSION"),
    },
    authentication: AuthCard {
        required: true,
        schemes: vec!["oauth2"],
    },
    tools: crate::tools::list_tools(),
});

/// Static MCP server card served at `/.well-known/mcp/server-card.json`.
///
/// Lets directory scanners (e.g. Smithery) discover server metadata and the
/// full tool list without performing the authenticated `tools/list` probe.
/// Declaring `authentication.schemes = ["oauth2"]` signals that the client
/// should follow the RFC 9728 protected-resource-metadata flow rather than
/// attempting Dynamic Client Registration directly.
pub async fn server_card() -> Json<&'static ServerCard> {
    Json(&*SERVER_CARD)
}
